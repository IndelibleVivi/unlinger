use clap::Parser;
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unlinger_daemon::{
    ControlPlane, DaemonMode, DaemonStatus, EngineConfig, HistoryStore, IpcServer, LocalPaths,
    ReconciliationEngine,
};
use unlinger_macos::MacosRuntime;
use unlinger_rules::RuleSet;

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

    loop {
        let now = now_unix_millis()?;
        match engine.run_cycle_at(now) {
            Ok(report) if arguments.once => {
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }
            Ok(_) => {}
            Err(error) if arguments.once => return Err(Box::new(error)),
            Err(error) => eprintln!("unlingerd: reconciliation cycle failed: {error}"),
        }
        thread::sleep(Duration::from_secs(arguments.interval_seconds));
    }
}

fn now_unix_millis() -> Result<u64, Box<dyn Error>> {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    Ok(u64::try_from(millis).map_err(|_| "wall clock overflowed u64")?)
}
