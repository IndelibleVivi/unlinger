use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};
use unlinger_core::{IncidentReport, IncidentState, ProcessGraph, SnapshotCoverage};
use unlinger_daemon::{
    DaemonStatus, EventPayload, HistoryEvent, IpcClient, IpcCommand, IpcPayload, LocalPaths,
};
use unlinger_macos::MacosSnapshotter;
use unlinger_rules::{Analyzer, AnalyzerContext, RuleSet};

const MAX_PAUSE_MILLIS: u64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Parser)]
#[command(
    name = "unlinger",
    version,
    about = "Zero-touch runtime hygiene for abandoned local automation"
)]
struct Cli {
    /// Override the daemon Unix-domain socket.
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Show daemon health, activity, pause state, and the latest reclaim.
    Status(OutputArgs),
    /// List redacted local incident and cleanup events.
    History(HistoryArgs),
    /// Explain one redacted incident timeline.
    Explain(ExplainArgs),
    /// Verify platform, process-inspection, rule, and daemon readiness.
    Doctor(OutputArgs),
    /// Pause automatic cleanup while keeping observation and history live.
    Pause(PauseArgs),
    /// Resume automatic cleanup.
    Resume(OutputArgs),
    /// Inspect current-user automation incidents without changing the machine.
    Scan(ScanArgs),
    /// Export one redacted incident timeline and daemon status.
    ExportDiagnostics(ExportDiagnosticsArgs),
}

#[derive(Clone, Debug, Args)]
struct OutputArgs {
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct HistoryArgs {
    /// Maximum recent events to return.
    #[arg(long, default_value_t = 50, value_parser = parse_history_limit)]
    limit: usize,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct ExplainArgs {
    incident_id: String,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct PauseArgs {
    /// Duration such as 30s, 15m, 2h, or 1d (maximum 30d).
    #[arg(value_parser = parse_duration_millis)]
    duration_millis: u64,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct ScanArgs {
    /// Mandatory acknowledgement that the local scan cannot send signals.
    #[arg(long)]
    dry_run: bool,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
    /// Cooling interval before the second observation.
    #[arg(long, default_value_t = 15)]
    observe_seconds: u64,
}

#[derive(Clone, Debug, Args)]
struct ExportDiagnosticsArgs {
    incident_id: String,
    /// Write to a 0600 local file instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Permit replacing the exact --output file.
    #[arg(long, requires = "output")]
    force: bool,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    schema_version: u32,
    healthy: bool,
    platform: &'static str,
    product_version: Option<String>,
    supported_platform: bool,
    current_uid: u32,
    non_root_user: bool,
    native_snapshot_ok: bool,
    snapshot_elapsed_millis: u128,
    coverage: Option<SnapshotCoverage>,
    signature_packs: Vec<PackSummary>,
    daemon_reachable: bool,
    daemon_status: Option<DaemonStatus>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PackSummary {
    id: String,
    version: String,
    supported_versions: String,
}

#[derive(Debug, Serialize)]
struct ScanReport {
    schema_version: u32,
    mode: &'static str,
    observed_twice: bool,
    interval_seconds: u64,
    first_coverage: SnapshotCoverage,
    second_coverage: Option<SnapshotCoverage>,
    incidents: Vec<IncidentReport>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("unlinger: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    let socket = cli.socket.unwrap_or(LocalPaths::discover()?.socket);
    match cli.command {
        Commands::Status(output) => status(&socket, output.json),
        Commands::History(arguments) => history(&socket, arguments),
        Commands::Explain(arguments) => explain(&socket, arguments),
        Commands::Doctor(output) => doctor(&socket, output.json),
        Commands::Pause(arguments) => pause(&socket, arguments),
        Commands::Resume(output) => resume(&socket, output.json),
        Commands::Scan(arguments) => scan(arguments),
        Commands::ExportDiagnostics(arguments) => export_diagnostics(&socket, arguments),
    }
}

fn status(socket: &std::path::Path, json: bool) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::Status)?;
    let IpcPayload::Status(status) = payload else {
        return Err("daemon returned the wrong payload for status".into());
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        print_status(&status);
    }
    Ok(())
}

fn history(socket: &std::path::Path, arguments: HistoryArgs) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::History {
        limit: arguments.limit,
    })?;
    let IpcPayload::History(events) = payload else {
        return Err("daemon returned the wrong payload for history".into());
    };
    if arguments.json {
        println!("{}", serde_json::to_string_pretty(&events)?);
    } else if events.is_empty() {
        println!("No recorded Unlinger incidents.");
    } else {
        for event in &events {
            print_history_line(event);
        }
    }
    Ok(())
}

fn explain(socket: &std::path::Path, arguments: ExplainArgs) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::Explain {
        incident_id: arguments.incident_id,
    })?;
    let IpcPayload::Incident(detail) = payload else {
        return Err("daemon returned the wrong payload for explain".into());
    };
    if arguments.json {
        println!("{}", serde_json::to_string_pretty(&detail)?);
    } else {
        println!("Incident {}", detail.incident_id);
        for event in &detail.events {
            print_history_line(event);
            match &event.payload {
                EventPayload::Observation { report } => {
                    println!(
                        "  {}@{} root={} members={} rss={} MiB",
                        report.signature_pack,
                        report.signature_version,
                        report.root.executable_basename,
                        report.member_count,
                        report.resident_memory_bytes / (1024 * 1024)
                    );
                    for evidence in &report.evidence {
                        println!("  evidence: {}", evidence.id);
                    }
                }
                EventPayload::Cleanup { receipt } => {
                    if let Some(reason) = &receipt.reason_id {
                        println!("  outcome: {reason}");
                    }
                    for action in &receipt.actions {
                        println!(
                            "  action: {:?} pid={} {:?} -> {:?}",
                            action.stage, action.pid, action.signal, action.disposition
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn pause(socket: &std::path::Path, arguments: PauseArgs) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::Pause {
        duration_millis: arguments.duration_millis,
    })?;
    let IpcPayload::Pause { until_unix_millis } = payload else {
        return Err("daemon returned the wrong payload for pause".into());
    };
    if arguments.json {
        println!(
            "{}",
            serde_json::json!({ "paused_until_unix_millis": until_unix_millis })
        );
    } else {
        println!("Automatic cleanup paused until Unix ms {until_unix_millis}.");
        println!("Observation and local history remain active.");
    }
    Ok(())
}

fn resume(socket: &std::path::Path, json: bool) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::Resume)?;
    if !matches!(payload, IpcPayload::Resumed) {
        return Err("daemon returned the wrong payload for resume".into());
    }
    if json {
        println!("{}", serde_json::json!({ "paused": false }));
    } else {
        println!("Automatic cleanup resumed.");
    }
    Ok(())
}

fn export_diagnostics(
    socket: &std::path::Path,
    arguments: ExportDiagnosticsArgs,
) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::ExportDiagnostics {
        incident_id: arguments.incident_id,
    })?;
    let IpcPayload::Diagnostics(bundle) = payload else {
        return Err("daemon returned the wrong payload for diagnostic export".into());
    };
    let document = serde_json::to_vec_pretty(&bundle)?;
    if let Some(output) = arguments.output {
        let mut options = OpenOptions::new();
        options.write(true);
        if arguments.force {
            options.create(true).truncate(true);
        } else {
            options.create_new(true);
        }
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&output)?;
        file.write_all(&document)?;
        file.write_all(b"\n")?;
        file.flush()?;
        println!("Wrote redacted diagnostics to {}", output.display());
    } else {
        std::io::stdout().write_all(&document)?;
        std::io::stdout().write_all(b"\n")?;
    }
    Ok(())
}

fn doctor(socket: &std::path::Path, json: bool) -> Result<(), Box<dyn Error>> {
    let rules = RuleSet::embedded()?;
    let product_version = macos_product_version();
    let supported_platform = product_version
        .as_deref()
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 14);
    let started = Instant::now();
    let snapshot = MacosSnapshotter::new().capture();
    let elapsed = started.elapsed().as_millis();
    let mut errors = Vec::new();
    if !supported_platform {
        errors.push("macOS 14 or later was not confirmed".to_owned());
    }
    let (native_snapshot_ok, current_uid, coverage) = match snapshot {
        Ok(snapshot) => (true, snapshot.current_uid, Some(snapshot.coverage)),
        Err(error) => {
            errors.push(error.to_string());
            (false, 0, None)
        }
    };
    let non_root_user = current_uid != 0;
    if !non_root_user {
        errors.push("Unlinger must run as a non-root per-user process".to_owned());
    }
    let daemon_status = IpcClient::new(socket)
        .request(IpcCommand::Status)
        .ok()
        .and_then(|payload| match payload {
            IpcPayload::Status(status) => Some(status),
            _ => None,
        });
    let daemon_reachable = daemon_status.is_some();
    let report = DoctorReport {
        schema_version: 1,
        healthy: errors.is_empty(),
        platform: "macOS",
        product_version,
        supported_platform,
        current_uid,
        non_root_user,
        native_snapshot_ok,
        snapshot_elapsed_millis: elapsed,
        coverage,
        signature_packs: rules
            .packs()
            .iter()
            .map(|pack| PackSummary {
                id: pack.id.clone(),
                version: pack.version.clone(),
                supported_versions: pack.supported_versions.clone(),
            })
            .collect(),
        daemon_reachable,
        daemon_status,
        errors,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Unlinger doctor");
        println!("healthy: {}", report.healthy);
        println!(
            "platform: macOS {} ({})",
            report.product_version.as_deref().unwrap_or("unknown"),
            if report.supported_platform {
                "supported"
            } else {
                "unsupported"
            }
        );
        println!(
            "current user: uid {} ({})",
            report.current_uid,
            if report.non_root_user {
                "non-root"
            } else {
                "root"
            }
        );
        println!(
            "native snapshot: {} in {} ms",
            if report.native_snapshot_ok {
                "ok"
            } else {
                "failed"
            },
            report.snapshot_elapsed_millis
        );
        if let Some(coverage) = &report.coverage {
            println!(
                "coverage: {}/{} inspected; {} unreadable; {} argv unavailable",
                coverage.inspected_processes,
                coverage.listed_processes,
                coverage.unreadable_processes,
                coverage.arguments_unavailable
            );
        }
        println!("signature packs: {}", report.signature_packs.len());
        println!(
            "daemon: {}",
            if report.daemon_reachable {
                "reachable"
            } else {
                "not reachable (source checks still ran)"
            }
        );
        for error in &report.errors {
            println!("error: {error}");
        }
    }
    if report.healthy {
        Ok(())
    } else {
        Err("doctor found one or more blocking source/platform failures".into())
    }
}

fn scan(arguments: ScanArgs) -> Result<(), Box<dyn Error>> {
    if !arguments.dry_run {
        return Err("scan requires --dry-run and never sends signals".into());
    }
    let snapshotter = MacosSnapshotter::new();
    let first_snapshot = snapshotter.capture()?;
    let analyzer = analyzer_for_snapshot(&first_snapshot)?;
    let first_reports = analyzer.observe(&first_snapshot)?;
    let should_observe_twice = arguments.observe_seconds > 0
        && first_reports
            .iter()
            .any(|report| report.state == IncidentState::Cooling);

    let (incidents, second_coverage) = if should_observe_twice {
        thread::sleep(Duration::from_secs(arguments.observe_seconds));
        let second_snapshot = snapshotter.capture()?;
        let second_reports = analyzer.observe(&second_snapshot)?;
        (
            analyzer.reconcile(&first_reports, &second_reports),
            Some(second_snapshot.coverage),
        )
    } else {
        (first_reports, None)
    };
    let report = ScanReport {
        schema_version: 1,
        mode: "dry-run",
        observed_twice: should_observe_twice,
        interval_seconds: if should_observe_twice {
            arguments.observe_seconds
        } else {
            0
        },
        first_coverage: first_snapshot.coverage,
        second_coverage,
        incidents,
    };

    if arguments.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_scan_report(&report);
    }
    Ok(())
}

fn analyzer_for_snapshot(snapshot: &unlinger_core::Snapshot) -> Result<Analyzer, Box<dyn Error>> {
    let graph = ProcessGraph::from_snapshot(snapshot)?;
    let self_pid = std::process::id();
    let ancestor_pids = graph
        .ancestor_pids(self_pid)
        .into_iter()
        .collect::<BTreeSet<_>>();
    Ok(Analyzer::new(
        RuleSet::embedded()?,
        AnalyzerContext {
            self_pid: Some(self_pid),
            ancestor_pids,
        },
    ))
}

fn print_status(status: &DaemonStatus) {
    println!("Unlinger daemon");
    println!("healthy: {}", status.healthy);
    println!("mode: {:?}", status.mode);
    println!(
        "activity: {}",
        if status.cleanup_in_progress {
            "cleanup"
        } else if status.scan_in_progress {
            "scan"
        } else {
            "idle"
        }
    );
    if let Some(deadline) = status.paused_until_unix_millis {
        println!("automatic cleanup: paused until Unix ms {deadline}");
    } else {
        println!("automatic cleanup: not paused");
    }
    println!(
        "current incidents: {} confirmed, {} ambiguous",
        status.confirmed_incidents, status.ambiguous_incidents
    );
    if let Some(reclaim) = &status.most_recent_reclaim {
        println!(
            "latest reclaim: {} {:?} at Unix ms {}",
            reclaim.incident_id, reclaim.state, reclaim.occurred_at_unix_millis
        );
    } else {
        println!("latest reclaim: none recorded in retained history");
    }
    if let Some(error) = &status.last_error {
        println!("last error: {error}");
    }
}

fn print_history_line(event: &HistoryEvent) {
    println!(
        "{}  {}  {:?}  {:?}  {}",
        event.occurred_at_unix_millis, event.incident_id, event.kind, event.state, event.event_id
    );
}

fn print_scan_report(report: &ScanReport) {
    println!("Unlinger dry-run scan");
    println!(
        "observations: {}",
        if report.observed_twice { "two" } else { "one" }
    );
    println!("incidents: {}", report.incidents.len());
    for incident in &report.incidents {
        println!(
            "{} {:?} {} root={} members={} rss={} MiB",
            incident.incident_id,
            incident.state,
            incident.signature_pack,
            incident.root.executable_basename,
            incident.member_count,
            incident.resident_memory_bytes / (1024 * 1024)
        );
        for evidence in &incident.evidence {
            println!("  - {}", evidence.id);
        }
    }
}

fn parse_duration_millis(value: &str) -> Result<u64, String> {
    let (number, multiplier) = if let Some(number) = value.strip_suffix("ms") {
        (number, 1)
    } else if let Some(number) = value.strip_suffix('s') {
        (number, 1_000)
    } else if let Some(number) = value.strip_suffix('m') {
        (number, 60 * 1_000)
    } else if let Some(number) = value.strip_suffix('h') {
        (number, 60 * 60 * 1_000)
    } else if let Some(number) = value.strip_suffix('d') {
        (number, 24 * 60 * 60 * 1_000)
    } else {
        return Err("duration must end in ms, s, m, h, or d".to_owned());
    };
    let number = number
        .parse::<u64>()
        .map_err(|_| "duration must start with a whole number".to_owned())?;
    let duration = number
        .checked_mul(multiplier)
        .ok_or_else(|| "duration overflowed u64".to_owned())?;
    if duration == 0 || duration > MAX_PAUSE_MILLIS {
        return Err("duration must be between 1ms and 30d".to_owned());
    }
    Ok(duration)
}

fn parse_history_limit(value: &str) -> Result<usize, String> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| "history limit must be a whole number".to_owned())?;
    if (1..=1_000).contains(&limit) {
        Ok(limit)
    } else {
        Err("history limit must be between 1 and 1000".to_owned())
    }
}

fn macos_product_version() -> Option<String> {
    let output = Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|version| !version.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_human_pause_durations() {
        assert_eq!(parse_duration_millis("250ms"), Ok(250));
        assert_eq!(parse_duration_millis("15m"), Ok(900_000));
        assert_eq!(parse_duration_millis("2h"), Ok(7_200_000));
        assert!(parse_duration_millis("2").is_err());
        assert!(parse_duration_millis("0s").is_err());
        assert!(parse_duration_millis("31d").is_err());
    }

    #[test]
    fn bounds_history_limit() {
        assert_eq!(parse_history_limit("50"), Ok(50));
        assert!(parse_history_limit("0").is_err());
        assert!(parse_history_limit("1001").is_err());
    }
}
