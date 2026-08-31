use clap::Parser;
mod scheduler;

use scheduler::{ReconcileTrigger, ReconciliationScheduler};
use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unlinger_core::{CleanupRuntime, ProcessRole};
use unlinger_daemon::{
    ControlPlane, DaemonInstanceLock, DaemonMode, DaemonStatus, EngineConfig, HistoryStore,
    IpcServer, LocalPaths, ReconciliationEngine,
};
use unlinger_macos::MacosRuntime;
use unlinger_rules::RuleSet;

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Parser)]
#[command(name = "unlingerd", version, about = "Unlinger reconciliation daemon")]
struct Arguments {
    /// Run under a generation-bound service lifecycle. Managed boots always start report-only.
    #[arg(
        long,
        requires = "activation_generation",
        conflicts_with_all = ["enforce", "report_only", "once"]
    )]
    managed: bool,
    /// Exact immutable activation generation supplied by the service manager.
    #[arg(long, requires = "managed")]
    activation_generation: Option<u64>,
    /// Explicitly activate automatic cleanup for CONFIRMED incidents.
    #[arg(long, conflicts_with = "report_only")]
    enforce: bool,
    /// Run the full detector and history path without sending signals (the default).
    #[arg(long, conflicts_with = "enforce")]
    report_only: bool,
    /// Run one reconciliation cycle, print its redacted JSON receipt, and exit.
    #[arg(long)]
    once: bool,
    /// Seconds between full reconciliation cycles.
    #[arg(long, default_value_t = 60)]
    interval_seconds: u64,
    /// Seconds between first and second observations of COOLING incidents.
    #[arg(long, default_value_t = 15)]
    observe_seconds: u64,
    /// Override the local SQLite history path.
    #[arg(long)]
    database: Option<PathBuf>,
    /// Override the local Unix-domain socket path.
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Override the lifetime singleton lock path (for isolated tests only).
    #[arg(long)]
    instance_lock: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run(Arguments::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("unlingerd: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Arguments) -> Result<(), Box<dyn Error>> {
    if !arguments.once && arguments.interval_seconds == 0 {
        return Err("--interval-seconds must be greater than zero".into());
    }
    install_signal_handlers()?;
    let defaults = LocalPaths::discover()?;
    let isolated_paths = arguments.database.is_some() || arguments.socket.is_some();
    let database = arguments.database.unwrap_or(defaults.database);
    let socket = arguments.socket.unwrap_or(defaults.socket);
    let instance_lock_path = arguments.instance_lock.unwrap_or_else(|| {
        if isolated_paths {
            socket.with_extension("instance.lock")
        } else {
            defaults.daemon_lock
        }
    });
    let _instance_lock = DaemonInstanceLock::acquire(&instance_lock_path)?;
    let mode = if arguments.enforce && !arguments.managed {
        DaemonMode::Enforce
    } else {
        DaemonMode::ReportOnly
    };
    let store = HistoryStore::open(database)?;
    let runtime = MacosRuntime::new();
    let recovery_now = runtime.clock_sample()?.wall_unix_millis;
    let (control, mut managed_startup) = if arguments.managed {
        let generation = arguments
            .activation_generation
            .ok_or("--managed requires --activation-generation")?;
        let control =
            ControlPlane::begin_managed(store, generation, std::process::id(), recovery_now)?;
        let startup = ManagedStartupFailClosed::new(control.clone(), recovery_now);
        let recovered = control.store().recover_incomplete_attempts(recovery_now)?;
        control.finish_startup_recovery(recovered.len(), recovery_now)?;
        (control, Some(startup))
    } else {
        let recovered = store.recover_incomplete_attempts(recovery_now)?;
        let enforcement_blocked = store.automatic_enforcement_blocked()?;
        let mut daemon_status = DaemonStatus::new(mode, std::process::id());
        daemon_status.recovered_cleanup_attempts = recovered.len();
        if !recovered.is_empty() || enforcement_blocked {
            daemon_status.set_effective_mode(DaemonMode::ReportOnly);
            daemon_status.requested_mode = DaemonMode::ReportOnly;
            daemon_status.armed_generation = None;
            daemon_status.enforcement_epoch = None;
        }
        daemon_status.startup_state = unlinger_daemon::StartupState::FirstScanReportOnly;
        (ControlPlane::new(store, daemon_status)?, None)
    };
    let _server = if arguments.once {
        None
    } else {
        Some(IpcServer::start(socket, control.clone())?)
    };
    let mut engine = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded()?,
        control.clone(),
        EngineConfig {
            observation_gap: Duration::from_secs(arguments.observe_seconds),
            ..EngineConfig::default()
        },
        Some(std::process::id()),
    );
    let mut last_cycle_error = None;
    let mut scheduler = (!arguments.once)
        .then(|| ReconciliationScheduler::start(Duration::from_secs(arguments.interval_seconds)));
    if let Some(scheduler) = scheduler.as_mut()
        && !report_event_source_error(scheduler, &control)
    {
        control.note_event_source_healthy()?;
    }

    'daemon: while !shutdown_requested() && !control.is_draining() {
        let now = now_unix_millis()?;
        let stop_control = control.clone();
        match engine.run_cycle_at_until(now, || shutdown_requested() || stop_control.is_draining())
        {
            Ok(report) => {
                if let Some(startup) = managed_startup.as_mut() {
                    startup.mark_first_cycle_complete();
                }
                if arguments.once {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    return Ok(());
                }
                last_cycle_error = None;
                if let Some(scheduler) = scheduler.as_mut() {
                    scheduler.complete_cycle(&candidate_watch_pids(&report));
                    report_event_source_error(scheduler, &control);
                }
            }
            Err(error) if arguments.once => return Err(Box::new(error)),
            Err(error) => {
                let message = error.to_string();
                if should_log_cycle_error(&mut last_cycle_error, &message) {
                    eprintln!("unlingerd: reconciliation cycle failed: {message}");
                }
                if let Some(scheduler) = scheduler.as_mut() {
                    scheduler.cycle_failed();
                }
            }
        }
        if let Some(scheduler) = scheduler.as_mut() {
            loop {
                let trigger =
                    scheduler.wait_for_trigger(|| shutdown_requested() || control.is_draining());
                report_event_source_error(scheduler, &control);
                match trigger {
                    ReconcileTrigger::SourceFailed => continue,
                    ReconcileTrigger::StopRequested => break 'daemon,
                    ReconcileTrigger::Periodic | ReconcileTrigger::Runtime(_) => break,
                }
            }
        }
    }
    if let Some(startup) = managed_startup.as_mut() {
        if shutdown_requested() && !control.is_draining() && !startup.first_cycle_complete {
            control.preserve_managed_pre_ready_restart_intent(now_unix_millis()?)?;
        }
        startup.mark_normal_signal_or_drain_exit();
    }
    Ok(())
}

/// Clears carried enforce intent if a managed boot exits before its first
/// successful report-only cycle. This guard is installed immediately after
/// `begin_managed`, so every later startup `?` path fails closed durably.
struct ManagedStartupFailClosed {
    control: ControlPlane,
    fallback_now_unix_millis: u64,
    first_cycle_complete: bool,
}

impl ManagedStartupFailClosed {
    fn new(control: ControlPlane, fallback_now_unix_millis: u64) -> Self {
        Self {
            control,
            fallback_now_unix_millis,
            first_cycle_complete: false,
        }
    }

    fn mark_first_cycle_complete(&mut self) {
        self.first_cycle_complete = true;
    }

    fn mark_normal_signal_or_drain_exit(&mut self) {
        self.first_cycle_complete = true;
    }
}

impl Drop for ManagedStartupFailClosed {
    fn drop(&mut self) {
        if self.first_cycle_complete {
            return;
        }
        let now = now_unix_millis().unwrap_or(self.fallback_now_unix_millis);
        if let Err(error) = self.control.fail_closed(
            now,
            "managed startup exited before the first successful report-only cycle",
        ) {
            eprintln!("unlingerd: managed startup durable fail-close failed: {error}");
        }
    }
}

extern "C" fn request_shutdown(_signal: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::Release);
}

fn install_signal_handlers() -> Result<(), std::io::Error> {
    let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = request_shutdown as *const () as usize;
    action.sa_flags = libc::SA_RESTART;
    if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    for signal in [libc::SIGTERM, libc::SIGINT] {
        if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Acquire)
}

fn candidate_watch_pids(report: &unlinger_daemon::CycleReport) -> BTreeSet<u32> {
    report
        .incidents
        .iter()
        .flat_map(|incident| incident.targets.iter())
        .filter(|target| {
            matches!(
                target.role,
                ProcessRole::Controller | ProcessRole::BrowserRoot
            )
        })
        .map(|target| target.identity.pid)
        .collect()
}

fn report_event_source_error(
    scheduler: &mut ReconciliationScheduler,
    control: &ControlPlane,
) -> bool {
    if let Some(error) = scheduler.take_source_error() {
        let _ = control.note_event_source_failure();
        eprintln!(
            "unlingerd: native event source unavailable; periodic reconciliation remains active: {error}"
        );
        true
    } else {
        false
    }
}

fn now_unix_millis() -> Result<u64, Box<dyn Error>> {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    Ok(u64::try_from(millis).map_err(|_| "wall clock overflowed u64")?)
}

fn should_log_cycle_error(last_error: &mut Option<String>, message: &str) -> bool {
    let should_log = last_error.as_deref() != Some(message);
    *last_error = Some(message.to_owned());
    should_log
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Arguments;

    #[test]
    fn repeated_cycle_errors_are_suppressed_until_a_success() {
        let mut last_error = None;

        assert!(super::should_log_cycle_error(
            &mut last_error,
            "snapshot failed"
        ));
        assert!(!super::should_log_cycle_error(
            &mut last_error,
            "snapshot failed"
        ));
        last_error = None;
        assert!(super::should_log_cycle_error(
            &mut last_error,
            "snapshot failed"
        ));
    }

    #[test]
    fn managed_mode_requires_a_generation_and_rejects_direct_mode_flags() {
        assert!(Arguments::try_parse_from(["unlingerd", "--managed"]).is_err());
        assert!(
            Arguments::try_parse_from([
                "unlingerd",
                "--managed",
                "--activation-generation",
                "7",
                "--enforce",
            ])
            .is_err()
        );
        assert!(
            Arguments::try_parse_from([
                "unlingerd",
                "--managed",
                "--activation-generation",
                "7",
                "--once",
            ])
            .is_err()
        );
        let parsed =
            Arguments::try_parse_from(["unlingerd", "--managed", "--activation-generation", "7"])
                .expect("managed generation");
        assert!(parsed.managed);
        assert_eq!(parsed.activation_generation, Some(7));
        assert!(!parsed.enforce);
    }
}
