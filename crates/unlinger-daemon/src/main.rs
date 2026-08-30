use clap::Parser;
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unlinger_daemon::{
    ControlPlane, DaemonMode, DaemonStatus, EngineConfig, HistoryStore, IpcServer, LocalPaths,
    ReconciliationEngine,
};
use unlinger_macos::MacosRuntime;
use unlinger_rules::RuleSet;

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Parser)]
#[command(name = "unlingerd", version, about = "Unlinger reconciliation daemon")]
struct Arguments {
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
    let database = arguments.database.unwrap_or(defaults.database);
    let socket = arguments.socket.unwrap_or(defaults.socket);
    let mode = if arguments.enforce {
        DaemonMode::Enforce
    } else {
        DaemonMode::ReportOnly
    };
    let store = HistoryStore::open(database)?;
    let control = ControlPlane::new(store, DaemonStatus::new(mode, std::process::id()));
    let _server = if arguments.once {
        None
    } else {
        Some(IpcServer::start(socket, control.clone())?)
    };
    let mut engine = ReconciliationEngine::new(
        MacosRuntime::new(),
        RuleSet::embedded()?,
        control,
        EngineConfig {
            observation_gap: Duration::from_secs(arguments.observe_seconds),
            ..EngineConfig::default()
        },
        Some(std::process::id()),
    );
    let mut last_cycle_error = None;

    while !shutdown_requested() {
        let now = now_unix_millis()?;
        match engine.run_cycle_at_until(now, shutdown_requested) {
            Ok(report) if arguments.once => {
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }
            Ok(_) => last_cycle_error = None,
            Err(error) if arguments.once => return Err(Box::new(error)),
            Err(error) => {
                let message = error.to_string();
                if should_log_cycle_error(&mut last_cycle_error, &message) {
                    eprintln!("unlingerd: reconciliation cycle failed: {message}");
                }
            }
        }
        if wait_for_shutdown(Duration::from_secs(arguments.interval_seconds)) {
            break;
        }
    }
    Ok(())
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

fn wait_for_shutdown(duration: Duration) -> bool {
    let started = Instant::now();
    while !shutdown_requested() {
        let Some(remaining) = duration.checked_sub(started.elapsed()) else {
            break;
        };
        thread::sleep(remaining.min(Duration::from_millis(100)));
    }
    shutdown_requested()
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
}
