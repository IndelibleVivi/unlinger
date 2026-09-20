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
use unlinger_daemon::tool_cache::{ToolCacheNativeResult, UvCacheMaintenance};
use unlinger_daemon::{
    ControlPlane, DaemonInstanceLock, DaemonMode, DaemonStatus, EngineConfig, HistoryStore,
    IpcServer, LocalPaths, ReconciliationEngine, StoreError,
};
use unlinger_macos::{ChromeCloneCleanup, ChromeCloneCleanupMode, MacosRuntime, MacosSnapshotter};
use unlinger_protocol::{ToolCacheAttemptSummary, ToolCacheAvailability, ToolCacheOutcome};
use unlinger_rules::RuleSet;

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
const STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS: u64 = 15 * 60 * 1_000;
const TOOL_CACHE_MAINTENANCE_INTERVAL_MILLIS: u64 = 7 * 24 * 60 * 60 * 1_000;

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
    let child_arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if child_arguments
        .first()
        .is_some_and(|arg| arg == unlinger_daemon::tool_cache::NATIVE_CACHE_CHILD_FLAG)
    {
        return ExitCode::from(
            unlinger_daemon::tool_cache::run_uv_supervisor(&child_arguments[1..]) as u8,
        );
    }
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
    // A storage cleanup attempt left PREPARED across a crash or restart can
    // never be proved by a later observation; finalize it as delivery_unknown
    // before any new cycle runs.
    control
        .store()
        .recover_storage_cleanup_attempts(recovery_now)?;
    control.store().recover_tool_cache_attempt(recovery_now)?;
    control
        .store()
        .recover_retired_npm_cache_attempt(recovery_now)?;
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
    let mut last_storage_residue_error = None;
    let mut last_storage_residue_attempt_at = None;
    let mut last_tool_cache_observation_attempt_at = None;
    let mut cache_worker = CacheWorker::default();
    let storage_snapshotter = MacosSnapshotter::new();
    let mut chrome_clone_cleanup = ChromeCloneCleanup::new();
    let mut scheduler = (!arguments.once)
        .then(|| ReconciliationScheduler::start(Duration::from_secs(arguments.interval_seconds)));
    if let Some(scheduler) = scheduler.as_mut()
        && !report_event_source_error(scheduler, &control)
    {
        control.note_event_source_healthy()?;
    }

    'daemon: while !shutdown_requested() && !control.is_draining() {
        let now = now_unix_millis()?;
        match control.store().latest_storage_residue_observation() {
            Ok(previous)
                if storage_residue_observation_due(
                    now,
                    previous
                        .as_ref()
                        .map(|observation| observation.observed_at_unix_millis),
                    last_storage_residue_attempt_at,
                ) =>
            {
                // An observation attempt consumes this cadence even if its
                // eventual SQLite write fails. A store failure must never
                // collapse the two-observation stability window.
                last_storage_residue_attempt_at = Some(now);
                let cycle = run_storage_residue_cycle(control.store(), now, || {
                    let process_snapshot = storage_snapshotter.capture().ok();
                    let cleanup_mode = match control.storage_cleanup_state_at(now) {
                        Ok((DaemonMode::Enforce, true)) => ChromeCloneCleanupMode::Enforce,
                        Ok((DaemonMode::ReportOnly, true)) => ChromeCloneCleanupMode::ReportOnly,
                        Ok(_) | Err(_) => ChromeCloneCleanupMode::LifecycleBlocked,
                    };
                    let mut prepared = None;
                    let reconciliation = chrome_clone_cleanup.reconcile(
                        now,
                        process_snapshot.as_ref(),
                        cleanup_mode,
                        |facts, action| {
                            if shutdown_requested() {
                                return None;
                            }
                            match control.run_storage_cleanup_if_ready_enforce(now, facts, action) {
                                Ok(Some((attempt, result))) => {
                                    prepared = Some(attempt);
                                    Some(result)
                                }
                                Ok(None) => None,
                                Err(error) => {
                                    // No durable PREPARED record means no deletion.
                                    let message = error.to_string();
                                    if should_log_cycle_error(
                                        &mut last_storage_residue_error,
                                        &message,
                                    ) {
                                        eprintln!(
                                            "unlingerd: storage cleanup preparation failed: {message}"
                                        );
                                    }
                                    None
                                }
                            }
                        },
                    );
                    // A terminal result is always committed together with the
                    // real latest residue observation, in one transaction.
                    match (reconciliation.cleanup, prepared) {
                        (Some(result), Some(attempt)) => {
                            control.store().complete_storage_cleanup_attempt(
                                &attempt,
                                &result,
                                &reconciliation.observation,
                            )
                        }
                        _ => control
                            .store()
                            .record_storage_residue_observation(&reconciliation.observation),
                    }
                });
                match cycle {
                    Ok(()) => last_storage_residue_error = None,
                    Err(error) => {
                        let message = error.to_string();
                        if should_log_cycle_error(&mut last_storage_residue_error, &message) {
                            eprintln!("unlingerd: storage residue cycle failed: {message}");
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                let message = error.to_string();
                if should_log_cycle_error(&mut last_storage_residue_error, &message) {
                    eprintln!("unlingerd: storage residue readback failed: {message}");
                }
            }
        }
        cache_worker.collect(&control, now);
        let stop_control = control.clone();
        let cycle_now = now_unix_millis()?;
        match engine.run_cycle_at_until(cycle_now, || {
            shutdown_requested() || stop_control.is_draining()
        }) {
            Ok(report) => {
                if let Some(startup) = managed_startup.as_mut() {
                    startup.mark_first_cycle_complete();
                }
                if arguments.once {
                    run_tool_cache_cycle(
                        &control,
                        now_unix_millis()?,
                        &AtomicBool::new(false),
                        false,
                    )?;
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
        let cache_now = now_unix_millis()?;
        if cache_worker.is_idle()
            && last_tool_cache_observation_attempt_at.is_none_or(|previous| {
                cache_now.saturating_sub(previous) >= STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS
            })
        {
            last_tool_cache_observation_attempt_at = Some(cache_now);
            cache_worker.start(control.clone(), cache_now)?;
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

#[derive(Default)]
struct CacheWorker {
    handle: Option<std::thread::JoinHandle<bool>>,
    cancelled: std::sync::Arc<AtomicBool>,
}

impl CacheWorker {
    fn is_idle(&self) -> bool {
        self.handle.is_none()
    }
    fn start(&mut self, control: ControlPlane, now: u64) -> std::io::Result<()> {
        self.launch(move |cancelled| {
            let result = run_tool_cache_cycle(&control, now, cancelled, true);
            if result.is_err() {
                let _ = control.fail_closed(
                    now_unix_millis().unwrap_or(now),
                    "tool cache maintenance state could not settle",
                );
            }
            result.is_ok()
        })
    }
    fn launch(
        &mut self,
        work: impl FnOnce(&AtomicBool) -> bool + Send + 'static,
    ) -> std::io::Result<()> {
        if self.handle.is_some() {
            return Ok(());
        }
        let cancelled = self.cancelled.clone();
        self.handle = Some(
            std::thread::Builder::new()
                .name("cache-maintenance".into())
                .spawn(move || work(&cancelled))?,
        );
        Ok(())
    }
    fn collect(&mut self, control: &ControlPlane, now: u64) {
        if self
            .handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
        {
            let result = self.handle.take().expect("finished worker").join();
            if !matches!(result, Ok(true)) {
                // The joined worker can no longer own a native child. A later
                // observation must never be used as evidence of its result.
                let _ = control.store().recover_tool_cache_attempt(now);
                let _ = control.fail_closed(now, "tool cache maintenance state could not settle");
            }
        }
    }
}

impl Drop for CacheWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn tool_cache_maintenance_due(now: u64, last_attempt: Option<&ToolCacheAttemptSummary>) -> bool {
    last_attempt.is_none_or(|attempt| {
        let interval = if attempt.outcome == ToolCacheOutcome::Busy {
            // A proved pre-mutation lock refusal gets the next observation opportunity.
            STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS
        } else {
            TOOL_CACHE_MAINTENANCE_INTERVAL_MILLIS
        };
        now.saturating_sub(attempt.prepared_at_unix_millis) >= interval
    })
}

/// The worker owns this exact activity until its native child and durable
/// settlement finish. Unwinding releases only this owner; joined-worker recovery
/// separately records unknown delivery and fails the daemon closed.
struct CacheActivityGuard<'a> {
    control: &'a ControlPlane,
    prepared: Option<&'a unlinger_daemon::PreparedToolCacheAttempt>,
}
impl CacheActivityGuard<'_> {
    fn finish(mut self) -> Result<(), unlinger_daemon::ControlError> {
        self.control
            .finish_tool_cache_action(self.prepared.take().expect("owned activity"))
    }
}
impl Drop for CacheActivityGuard<'_> {
    fn drop(&mut self) {
        if let Some(prepared) = self.prepared.take() {
            let _ = self.control.finish_tool_cache_action(prepared);
        }
    }
}

fn run_tool_cache_cycle(
    control: &ControlPlane,
    now: u64,
    cancelled: &AtomicBool,
    allow_mutation: bool,
) -> Result<(), Box<dyn Error>> {
    // Only startup (or a joined failed worker) recovers PREPARED. Ordinary
    // observations cannot reclassify a live maintenance attempt.
    let previous = control.store().latest_tool_cache_maintenance()?;
    let stopping =
        || cancelled.load(Ordering::Acquire) || shutdown_requested() || control.is_draining();
    if stopping() {
        return Ok(());
    }
    let adapter = UvCacheMaintenance::discover(stopping);
    let availability = match &adapter {
        Ok(_) => ToolCacheAvailability::Available,
        Err(availability) => *availability,
    };
    control
        .store()
        .record_tool_cache_observation(now, availability)?;
    if availability != ToolCacheAvailability::Available
        || stopping()
        || !allow_mutation
        || !tool_cache_maintenance_due(
            now,
            previous
                .as_ref()
                .and_then(|state| state.last_attempt.as_ref()),
        )
    {
        return Ok(());
    }
    let Ok(adapter) = adapter else { return Ok(()) };
    let Some((prepared, child, epoch)) =
        control.start_tool_cache_if_ready_enforce(now, || adapter.start())?
    else {
        return Ok(());
    };
    let activity = CacheActivityGuard {
        control,
        prepared: Some(&prepared),
    };
    let native = match child {
        Ok(child) => {
            child.wait(|| stopping() || !control.tool_cache_action_may_continue(&prepared, &epoch))
        }
        Err(_) => ToolCacheNativeResult {
            outcome: ToolCacheOutcome::Failed,
            removed_entry_count: None,
            removed_logical_bytes: None,
        },
    };
    let settled = (|| -> Result<(), Box<dyn Error>> {
        let after = adapter
            .observe(|| stopping() || !control.tool_cache_action_may_continue(&prepared, &epoch));
        let completed = now_unix_millis()?.max(now);
        let result = ToolCacheAttemptSummary {
            outcome: native.outcome,
            prepared_at_unix_millis: now,
            completed_at_unix_millis: Some(completed),
            native_removed_entry_count: native.removed_entry_count,
            native_removed_logical_bytes: native.removed_logical_bytes,
        };
        control
            .store()
            .complete_tool_cache_attempt(&prepared, &result, after)?;
        Ok(())
    })();
    activity.finish()?;
    settled
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

fn storage_residue_observation_due(
    now_unix_millis: u64,
    persisted_observation_at: Option<u64>,
    last_attempt_at: Option<u64>,
) -> bool {
    persisted_observation_at
        .into_iter()
        .chain(last_attempt_at)
        .max()
        .is_none_or(|last_observed_at| {
            now_unix_millis.saturating_sub(last_observed_at)
                >= STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS
        })
}

/// Runs one due storage-residue cycle behind a bounded durable-recovery retry.
///
/// A terminal write that fails after descriptor-relative deletion can leave a
/// PREPARED attempt behind in a still-running daemon. Recovery is retried here
/// before every due cycle and must become durable `delivery_unknown`; while it
/// cannot, the cycle is skipped entirely so no new deletion can start against
/// an earlier attempt whose delivery is still uncertain. Startup recovery is
/// retained separately.
fn run_storage_residue_cycle<T>(
    store: &HistoryStore,
    now_unix_millis: u64,
    cycle: impl FnOnce() -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    store.recover_storage_cleanup_attempts(now_unix_millis)?;
    cycle()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Arguments, HistoryStore};

    #[test]
    fn cache_worker_returns_before_completion_is_single_flight_and_joins_on_cancel() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        };
        use std::time::Duration;
        let (started_tx, started_rx) = mpsc::channel();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let invocations = Arc::new(AtomicUsize::new(0));
        let count = invocations.clone();
        let mut worker = super::CacheWorker::default();
        worker
            .launch(move |cancelled| {
                count.fetch_add(1, Ordering::SeqCst);
                started_tx.send(()).unwrap();
                while !cancelled.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(1));
                }
                stopped_tx.send(()).unwrap();
                true
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!worker.is_idle());
        worker
            .launch(|_| panic!("a second cache cycle must not overlap"))
            .unwrap();
        assert_eq!(invocations.load(Ordering::SeqCst), 1);
        assert!(
            stopped_rx.try_recv().is_err(),
            "caller progressed while worker remained active"
        );
        drop(worker);
        stopped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn joined_failed_cache_worker_releases_activity_and_recovers_unknown() {
        use super::*;
        let directory = std::env::temp_dir().join(format!(
            "unlinger-cache-unwind-{}-{}",
            std::process::id(),
            now_unix_millis().unwrap()
        ));
        std::fs::create_dir(&directory).unwrap();
        let store = HistoryStore::open(directory.join("history.sqlite3")).unwrap();
        let control = ControlPlane::new(
            store,
            DaemonStatus::new(DaemonMode::Enforce, std::process::id()),
        )
        .unwrap();
        control.complete_successful_cycle(1).unwrap();
        let worker_control = control.clone();
        let mut worker = CacheWorker::default();
        worker
            .launch(move |_| {
                let (prepared, _, _) = worker_control
                    .start_tool_cache_if_ready_enforce(2, || Ok(()))
                    .unwrap()
                    .unwrap();
                let _activity = CacheActivityGuard {
                    control: &worker_control,
                    prepared: Some(&prepared),
                };
                panic!("owned cache worker failure fixture");
            })
            .unwrap();
        while !worker.handle.as_ref().unwrap().is_finished() {
            std::thread::sleep(Duration::from_millis(1));
        }
        worker.collect(&control, 3);
        assert!(worker.is_idle());
        let status = control.status().unwrap();
        assert!(!status.cleanup_in_progress);
        assert!(!status.healthy);
        assert_eq!(
            control
                .store()
                .latest_tool_cache_maintenance()
                .unwrap()
                .unwrap()
                .last_attempt
                .unwrap()
                .outcome,
            ToolCacheOutcome::DeliveryUnknown
        );
        drop(control);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_cache_cadence_survives_restart_and_retries_only_proved_busy_soon() {
        use super::*;
        let mut attempt = ToolCacheAttemptSummary {
            outcome: ToolCacheOutcome::DeliveryUnknown,
            prepared_at_unix_millis: 10,
            completed_at_unix_millis: Some(11),
            native_removed_entry_count: None,
            native_removed_logical_bytes: None,
        };
        assert!(tool_cache_maintenance_due(10, None));
        assert!(!tool_cache_maintenance_due(9, Some(&attempt)));
        assert!(!tool_cache_maintenance_due(
            TOOL_CACHE_MAINTENANCE_INTERVAL_MILLIS + 9,
            Some(&attempt)
        ));
        assert!(tool_cache_maintenance_due(
            TOOL_CACHE_MAINTENANCE_INTERVAL_MILLIS + 10,
            Some(&attempt)
        ));
        attempt.outcome = ToolCacheOutcome::Busy;
        assert!(!tool_cache_maintenance_due(
            STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS + 9,
            Some(&attempt)
        ));
        assert!(tool_cache_maintenance_due(
            STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS + 10,
            Some(&attempt)
        ));
        attempt.outcome = ToolCacheOutcome::Failed;
        assert!(!tool_cache_maintenance_due(
            STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS + 10,
            Some(&attempt)
        ));
    }

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

    #[test]
    fn storage_residue_attempts_keep_the_full_observation_interval_after_store_failure() {
        let interval = super::STORAGE_RESIDUE_OBSERVATION_INTERVAL_MILLIS;

        assert!(super::storage_residue_observation_due(100, None, None));
        assert!(!super::storage_residue_observation_due(
            100 + interval - 1,
            None,
            Some(100)
        ));
        assert!(super::storage_residue_observation_due(
            100 + interval,
            None,
            Some(100)
        ));
        assert!(!super::storage_residue_observation_due(
            200 + interval - 1,
            Some(200),
            Some(100)
        ));
    }

    #[test]
    fn storage_cycles_retry_prepared_recovery_and_skip_on_failure() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "unlingerd-storage-recovery-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("create test directory");

        let store = HistoryStore::open(directory.join("history.sqlite3")).expect("open store");
        let prepared = store
            .begin_storage_cleanup_attempt(
                1_000,
                &unlinger_core::StorageCleanupAttemptFacts {
                    planned_candidate_count: 1,
                    before_candidate_count: 2,
                    before_logical_bytes: 2_048,
                },
            )
            .expect("prepare storage cleanup attempt");
        assert_eq!(prepared.attempt_token().len(), 32);

        // A healthy cycle retries recovery first, so the abandoned PREPARED
        // attempt becomes durable delivery_unknown before the cycle body runs.
        let cycle_ran = std::cell::Cell::new(false);
        super::run_storage_residue_cycle(&store, 2_000, || {
            cycle_ran.set(true);
            Ok(())
        })
        .expect("healthy storage cycle");
        assert!(cycle_ran.get());
        let record = store
            .latest_storage_cleanup_attempt()
            .expect("read recovered attempt")
            .expect("recovered row");
        assert_eq!(
            record.disposition,
            Some(unlinger_core::StorageCleanupDisposition::DeliveryUnknown)
        );

        // If durable recovery cannot be made, the cycle body never runs, so no
        // new scan or deletion can start while delivery is still uncertain.
        let blocked_directory = directory.join("blocked");
        std::fs::create_dir_all(&blocked_directory).expect("create blocked directory");
        let blocked = HistoryStore::open(blocked_directory.join("history.sqlite3"))
            .expect("open blocked store");
        let blocked_path = blocked.path().to_path_buf();
        let connection = rusqlite::Connection::open(&blocked_path).expect("open raw connection");
        connection
            .execute_batch("DROP TABLE storage_cleanup_attempts;")
            .expect("remove attempt authority");
        drop(connection);

        let blocked_cycle_ran = std::cell::Cell::new(false);
        let blocked_result = super::run_storage_residue_cycle(&blocked, 2_100, || {
            blocked_cycle_ran.set(true);
            Ok(())
        });
        assert!(blocked_result.is_err());
        assert!(
            !blocked_cycle_ran.get(),
            "a failed recovery retry must skip the whole storage cycle"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }
}
