mod service;
mod task;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};
use unlinger_core::{IncidentReport, IncidentState, ProcessGraph, SnapshotCoverage};
use unlinger_daemon::{
    DaemonStatus, EventPayload, HistoryEvent, IpcClient, IpcCommand, IpcPayload, LocalPaths,
};
use unlinger_macos::MacosSnapshotter;
use unlinger_protocol::{
    BrowserCompatibilityDecision, BrowserOverviewPhase, BrowserOverviewSnapshot, BrowserProduct,
    Mode, ObservationFreshness,
};
use unlinger_rules::{Analyzer, AnalyzerContext, RuleSet};

const MAX_PAUSE_MILLIS: u64 = 30 * 24 * 60 * 60 * 1_000;
const MAX_PRINT_ATTENTION_ITEMS: usize = 16;

#[derive(Debug, Parser)]
#[command(
    name = "unlinger",
    version,
    about = "Inspect and reclaim verified abandoned browser automation"
)]
struct Cli {
    /// Override the daemon Unix-domain socket for IPC commands.
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run a command with an exclusive, lifetime-tracked Playwright CLI session.
    Task(task::TaskArgs),
    #[command(name = "__task-exec", hide = true)]
    TaskExec(task::GatedExecArgs),
    /// Show daemon lifecycle, health, activity, recovery, and bounded attention state.
    Status(OutputArgs),
    /// Show the atomic browser-leftover product projection.
    Browser(BrowserArgs),
    /// List redacted local incident and cleanup events.
    History(HistoryArgs),
    /// Explain one redacted incident timeline.
    Explain(ExplainArgs),
    /// Verify source/platform checks and, by default, installed daemon readiness.
    Doctor(DoctorArgs),
    /// Pause automatic cleanup while keeping observation and history live.
    Pause(PauseArgs),
    /// Resume automatic cleanup.
    Resume(OutputArgs),
    /// Clear one durable failed/revived cleanup block and restart full cooling.
    Retry(RetryArgs),
    /// Protect one exact incident from automatic cleanup until explicitly unprotected.
    Protect(IncidentProtectionArgs),
    /// Remove the protection override from one exact incident.
    Unprotect(IncidentProtectionArgs),
    /// Inspect current-user automation incidents without changing the machine.
    Scan(ScanArgs),
    /// Export one redacted incident timeline and daemon status.
    ExportDiagnostics(ExportDiagnosticsArgs),
    /// Install, inspect, change, or remove the per-user LaunchAgent.
    Service(ServiceArgs),
}

#[derive(Clone, Debug, Args)]
struct BrowserArgs {
    #[command(subcommand)]
    command: BrowserCommand,
}

#[derive(Clone, Debug, Subcommand)]
enum BrowserCommand {
    /// Show current browser sessions, compatibility, coverage, and recent settlement.
    Status(OutputArgs),
}

#[derive(Clone, Debug, Args)]
struct ServiceArgs {
    #[command(subcommand)]
    command: ServiceCommand,
}

#[derive(Clone, Debug, Subcommand)]
enum ServiceCommand {
    /// Transactionally install both binaries and load the per-user LaunchAgent.
    Install(ServiceInstallArgs),
    /// Inspect installed files, launchd ownership, IPC identity, mode, and permissions.
    Status(OutputArgs),
    /// Durably accept a verified report-only candidate and retire its rollback lease.
    AcceptCandidate(OutputArgs),
    /// Restore the prior report-only generation and its SQLite snapshot.
    RollbackCandidate(OutputArgs),
    /// Restart the exact active generation at the report-only floor.
    RestartReportOnly(OutputArgs),
    /// Change the managed daemon between report-only and enforce mode.
    SetMode(ServiceSetModeArgs),
    /// Unload the LaunchAgent and remove service binaries while preserving history and logs.
    Uninstall(OutputArgs),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ServiceModeArgument {
    ReportOnly,
    Enforce,
}

impl From<ServiceModeArgument> for unlinger_daemon::DaemonMode {
    fn from(value: ServiceModeArgument) -> Self {
        match value {
            ServiceModeArgument::ReportOnly => Self::ReportOnly,
            ServiceModeArgument::Enforce => Self::Enforce,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ServiceInstallArgs {
    /// Initial daemon mode. Report-only is the safe default.
    #[arg(long, value_enum, default_value_t = ServiceModeArgument::ReportOnly)]
    mode: ServiceModeArgument,
    /// Candidate unlingerd binary. Defaults to the sibling of this unlinger executable.
    #[arg(long)]
    daemon: Option<PathBuf>,
    /// Emit machine-readable JSON after verified activation.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct ServiceSetModeArgs {
    #[arg(value_enum)]
    mode: ServiceModeArgument,
    /// Emit machine-readable JSON after the verified lifecycle transition.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct OutputArgs {
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct DoctorArgs {
    /// Check source/platform capability without requiring a running daemon.
    #[arg(long)]
    source_only: bool,
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
struct RetryArgs {
    incident_id: String,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct IncidentProtectionArgs {
    incident_id: String,
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
    /// Backward-compatible alias for `overall_healthy`.
    healthy: bool,
    source_healthy: bool,
    daemon_required: bool,
    daemon_reachable: bool,
    daemon_healthy: bool,
    daemon_ready: bool,
    overall_healthy: bool,
    platform: &'static str,
    product_version: Option<String>,
    supported_platform: bool,
    current_uid: u32,
    non_root_user: bool,
    native_snapshot_ok: bool,
    snapshot_elapsed_millis: u128,
    coverage: Option<SnapshotCoverage>,
    signature_packs: Vec<PackSummary>,
    daemon_status: Option<DoctorDaemonStatus>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DoctorDaemonStatus {
    managed: bool,
    activation_generation: Option<u64>,
    armed_generation: Option<u64>,
    healthy: bool,
    ready: bool,
    startup_state: unlinger_daemon::StartupState,
    requested_mode: unlinger_daemon::DaemonMode,
    effective_mode: unlinger_daemon::DaemonMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_source_healthy: Option<bool>,
    storage_recovery: Option<DoctorStorageRecovery>,
    blocked_cleanup_count: usize,
    protected_incident_count: usize,
    attention_count: usize,
}

#[derive(Debug, Serialize)]
struct DoctorStorageRecovery {
    occurred_at_unix_millis: u64,
    reason: unlinger_daemon::StorageRecoveryReason,
    quarantined_sidecar_count: usize,
}

impl From<&DaemonStatus> for DoctorDaemonStatus {
    fn from(status: &DaemonStatus) -> Self {
        Self {
            managed: status.managed,
            activation_generation: status.activation_generation,
            armed_generation: status.armed_generation,
            healthy: status.healthy,
            ready: daemon_status_ready(status),
            startup_state: status.startup_state,
            requested_mode: if status.lifecycle_schema_version == 0 {
                status.mode
            } else {
                status.requested_mode
            },
            effective_mode: status.effective_mode(),
            event_source_healthy: (status.lifecycle_schema_version != 0
                || status.ipc_schema_version != 0)
                .then_some(status.event_source_healthy),
            storage_recovery: status.storage_recovery.as_ref().map(|recovery| {
                DoctorStorageRecovery {
                    occurred_at_unix_millis: recovery.occurred_at_unix_millis,
                    reason: recovery.reason,
                    quarantined_sidecar_count: recovery.quarantined_sidecar_count,
                }
            }),
            blocked_cleanup_count: status.attention.blocked_cleanup_count,
            protected_incident_count: status.protected_incident_count,
            attention_count: status.attention.items.len().min(MAX_PRINT_ATTENTION_ITEMS),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DoctorHealth {
    daemon_reachable: bool,
    daemon_healthy: bool,
    daemon_ready: bool,
    overall_healthy: bool,
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

#[derive(Debug, Serialize)]
struct IncidentProtectionReport {
    schema_version: u32,
    incident_id: String,
    protected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    protected_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_exact_observed_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exact_absence_since_unix_millis: Option<u64>,
}

impl IncidentProtectionReport {
    fn protected(protection: &unlinger_daemon::ProtectedIncidentSummary) -> Self {
        Self {
            schema_version: 1,
            incident_id: protection.incident_id.clone(),
            protected: true,
            protected_at_unix_millis: Some(protection.protected_at_unix_millis),
            last_exact_observed_at_unix_millis: protection.last_exact_observed_at_unix_millis,
            exact_absence_since_unix_millis: protection.exact_absence_since_unix_millis,
        }
    }

    fn unprotected(incident_id: String) -> Self {
        Self {
            schema_version: 1,
            incident_id,
            protected: false,
            protected_at_unix_millis: None,
            last_exact_observed_at_unix_millis: None,
            exact_absence_since_unix_millis: None,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(exit) = error.downcast_ref::<task::CommandExit>() {
                return ExitCode::from(exit.0);
            }
            eprintln!("unlinger: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    let paths = LocalPaths::discover()?;
    if matches!(&cli.command, Commands::Service(_)) && cli.socket.is_some() {
        return Err("--socket does not apply to service lifecycle commands".into());
    }
    let socket = cli.socket.unwrap_or_else(|| paths.socket.clone());
    match cli.command {
        Commands::Task(arguments) => task::run(&socket, arguments),
        Commands::TaskExec(arguments) => task::exec(arguments),
        Commands::Status(output) => status(&socket, output.json),
        Commands::Browser(arguments) => browser_command(&socket, arguments),
        Commands::History(arguments) => history(&socket, arguments),
        Commands::Explain(arguments) => explain(&socket, arguments),
        Commands::Doctor(arguments) => doctor(&socket, arguments),
        Commands::Pause(arguments) => pause(&socket, arguments),
        Commands::Resume(output) => resume(&socket, output.json),
        Commands::Retry(arguments) => retry(&socket, arguments),
        Commands::Protect(arguments) => protect(&socket, arguments),
        Commands::Unprotect(arguments) => unprotect(&socket, arguments),
        Commands::Scan(arguments) => scan(arguments),
        Commands::ExportDiagnostics(arguments) => export_diagnostics(&socket, arguments),
        Commands::Service(arguments) => service_command(&paths, arguments),
    }
}

fn browser_command(socket: &std::path::Path, arguments: BrowserArgs) -> Result<(), Box<dyn Error>> {
    match arguments.command {
        BrowserCommand::Status(output) => browser_status(socket, output.json),
    }
}

fn browser_status(socket: &std::path::Path, json: bool) -> Result<(), Box<dyn Error>> {
    let overview = IpcClient::new(socket).request_browser_overview()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&overview)?);
        return Ok(());
    }
    for line in browser_status_lines(&overview) {
        println!("{line}");
    }
    Ok(())
}

fn browser_status_lines(overview: &BrowserOverviewSnapshot) -> Vec<String> {
    let mut lines = vec![
        format!("Browser overview: {}", browser_phase_name(overview.phase)),
        format!("Mode: {}", browser_mode_name(overview.effective_mode)),
        format!(
            "Observation freshness: {}",
            browser_freshness_name(overview.freshness)
        ),
    ];
    if let Some(observed_at) = overview.observed_at_unix_millis {
        lines.push(format!("Observed at Unix ms: {observed_at}"));
    }
    if let Some(paused_until) = overview.paused_until_unix_millis {
        lines.push(format!(
            "Automatic cleanup paused until Unix ms: {paused_until}"
        ));
    }
    if overview.sessions.is_empty() {
        lines.push("No supported browser leftovers found in the trusted snapshot.".to_owned());
    } else {
        lines.push(format!("Browser sessions: {}", overview.sessions.len()));
        for session in &overview.sessions {
            let version = session
                .compatibility
                .observed_version
                .as_deref()
                .unwrap_or("version unavailable");
            lines.push(format!(
                "- {} {}: {:?}, {} processes, {} MiB; {} {} ({})",
                session.family,
                session.incident_id,
                session.state,
                session.member_count,
                session.resident_memory_bytes / (1024 * 1024),
                browser_product_name(session.compatibility.product),
                version,
                browser_decision_name(session.compatibility.decision),
            ));
            if let Some(reason_id) = &session.compatibility.reason_id {
                lines.push(format!("  coverage: {reason_id}"));
            }
        }
    }
    if let Some(settlement) = &overview.recent_settlement {
        let process_count = settlement.process_count.map_or_else(
            || "process count unavailable".to_owned(),
            |count| format!("{count} processes"),
        );
        let memory = settlement.estimated_reclaimed_memory_bytes.map_or_else(
            || "memory estimate unavailable".to_owned(),
            |bytes| format!("{} MiB estimated reclaimed", bytes / (1024 * 1024)),
        );
        lines.push(format!(
            "Recent settlement: {} at Unix ms {}; {process_count}, {memory}, {} revival checks; {:?}",
            settlement.family,
            settlement.occurred_at_unix_millis,
            settlement.revival_checks_completed,
            settlement.overall_outcome,
        ));
    }
    lines.push(format!(
        "Support catalog: {} ({} families)",
        overview.support_catalog.support_revision,
        overview.support_catalog.families.len()
    ));
    lines
}

fn browser_phase_name(phase: BrowserOverviewPhase) -> &'static str {
    match phase {
        BrowserOverviewPhase::Unknown => "unknown",
        BrowserOverviewPhase::Clear => "clear",
        BrowserOverviewPhase::Active => "active",
        BrowserOverviewPhase::Verifying => "verifying",
        BrowserOverviewPhase::Confirmed => "confirmed",
        BrowserOverviewPhase::Reclaiming => "reclaiming",
        BrowserOverviewPhase::Protected => "protected",
        BrowserOverviewPhase::Attention => "attention",
    }
}

fn browser_mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::ReportOnly => "report-only",
        Mode::Enforce => "enforce",
    }
}

fn browser_freshness_name(freshness: ObservationFreshness) -> &'static str {
    match freshness {
        ObservationFreshness::Current => "current",
        ObservationFreshness::ScanInProgress => "scan-in-progress",
        ObservationFreshness::StaleAfterFailure => "stale-after-failure",
        ObservationFreshness::NeverObserved => "never-observed",
    }
}

fn browser_product_name(product: BrowserProduct) -> &'static str {
    match product {
        BrowserProduct::ChromeForTesting => "Chrome for Testing",
        BrowserProduct::Chromium => "Chromium",
        BrowserProduct::GoogleChrome => "Google Chrome",
        BrowserProduct::Other => "other browser",
        BrowserProduct::Unknown => "unknown browser",
    }
}

fn browser_decision_name(decision: BrowserCompatibilityDecision) -> &'static str {
    match decision {
        BrowserCompatibilityDecision::Automatic => "automatic",
        BrowserCompatibilityDecision::ObserveOnly => "observe-only",
        BrowserCompatibilityDecision::Protected => "protected",
        BrowserCompatibilityDecision::Unknown => "unknown",
    }
}

fn service_command(paths: &LocalPaths, arguments: ServiceArgs) -> Result<(), Box<dyn Error>> {
    match arguments.command {
        ServiceCommand::Install(arguments) => {
            let source_cli = std::env::current_exe()?;
            let source_daemon = arguments.daemon.unwrap_or_else(|| {
                source_cli
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join("unlingerd")
            });
            let report =
                service::install(paths, &source_cli, &source_daemon, arguments.mode.into())?;
            print_service_status(&report, arguments.json)?;
        }
        ServiceCommand::Status(output) => {
            let report = service::status(paths)?;
            print_service_status(&report, output.json)?;
        }
        ServiceCommand::AcceptCandidate(output) => {
            let report = service::accept_candidate(paths)?;
            print_service_status(&report, output.json)?;
        }
        ServiceCommand::RollbackCandidate(output) => {
            let report = service::rollback_candidate(paths)?;
            print_service_status(&report, output.json)?;
        }
        ServiceCommand::RestartReportOnly(output) => {
            let report = service::restart_report_only(paths)?;
            print_service_status(&report, output.json)?;
        }
        ServiceCommand::SetMode(arguments) => {
            let report = service::set_mode(paths, arguments.mode.into())?;
            print_service_status(&report, arguments.json)?;
        }
        ServiceCommand::Uninstall(output) => {
            let report = service::uninstall(paths)?;
            if output.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&service::public_status_report(&report))?
                );
            } else {
                println!("Unlinger LaunchAgent and service binaries removed.");
                println!("Local history and logs were preserved.");
                println!("launchd loaded: {}", report.loaded);
            }
        }
    }
    Ok(())
}

fn print_service_status(
    report: &service::ServiceStatusReport,
    json: bool,
) -> Result<(), Box<dyn Error>> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&service::public_status_report(report))?
        );
        return Ok(());
    }
    for line in service_status_lines(report) {
        println!("{line}");
    }
    Ok(())
}

fn service_status_lines(report: &service::ServiceStatusReport) -> Vec<String> {
    let mut lines = vec![
        "Unlinger LaunchAgent".to_owned(),
        format!("installed: {}", report.installed),
        format!("loaded: {}", report.loaded),
        format!("healthy: {}", report.healthy),
        format!("desired mode: {:?}", report.expected_mode),
        match report.active_generation {
            Some(generation) => format!("active generation: {generation}"),
            None => "active generation: legacy layout".to_owned(),
        },
        format!(
            "launchd/IPC identity: {}",
            if report.pid_matches {
                "matched"
            } else {
                "not matched"
            }
        ),
        format!(
            "generation identity: {}",
            if report.generation_matches {
                "matched"
            } else {
                "not matched"
            }
        ),
        format!(
            "binary identity: {}",
            if report.binary_matches {
                "matched"
            } else {
                "not matched"
            }
        ),
        format!(
            "permissions: {}",
            if report.permissions_ok {
                "private"
            } else {
                "unsafe"
            }
        ),
        "history and logs: private local data preserved on uninstall".to_owned(),
    ];
    if let Some(status) = &report.daemon_status {
        lines.push("daemon runtime:".to_owned());
        lines.extend(
            daemon_status_lines(status)
                .into_iter()
                .map(|line| format!("  {line}")),
        );
    }
    if let Some(acceptance) = &report.acceptance {
        lines.push("candidate acceptance:".to_owned());
        lines.push(format!("  phase: {}", acceptance.phase));
        lines.push(format!(
            "  rollback available: {}",
            acceptance.rollback_available
        ));
        if let Some(prior) = acceptance.prior_generation {
            lines.push(format!("  rollback generation: {prior}"));
        }
    }
    if !report.errors.is_empty() {
        lines.push(format!(
            "reported problems: {} (use --json for structured details)",
            report.errors.len()
        ));
    }
    lines
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

fn retry(socket: &std::path::Path, arguments: RetryArgs) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::RetryFailedCleanup {
        incident_id: arguments.incident_id.clone(),
    })?;
    let IpcPayload::RetryScheduled { incident_id } = payload else {
        return Err("daemon returned the wrong payload for retry".into());
    };
    if arguments.json {
        println!(
            "{}",
            serde_json::json!({
                "incident_id": incident_id,
                "retry_scheduled": true,
                "cooling_reset": true
            })
        );
    } else {
        println!("Cleanup retry scheduled for {incident_id}.");
        println!("The incident must pass the complete cooling and safety gates again.");
    }
    Ok(())
}

fn protect(
    socket: &std::path::Path,
    arguments: IncidentProtectionArgs,
) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::ProtectIncident {
        incident_id: arguments.incident_id,
    })?;
    let IpcPayload::IncidentProtected { protection } = payload else {
        return Err("daemon returned the wrong payload for protect".into());
    };
    let report = IncidentProtectionReport::protected(&protection);
    if arguments.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Exact incident protection is active.");
        println!(
            "protected at: Unix ms {}",
            protection.protected_at_unix_millis
        );
        println!("Only an explicit unprotect command removes this override.");
    }
    Ok(())
}

fn unprotect(
    socket: &std::path::Path,
    arguments: IncidentProtectionArgs,
) -> Result<(), Box<dyn Error>> {
    let payload = IpcClient::new(socket).request(IpcCommand::UnprotectIncident {
        incident_id: arguments.incident_id,
    })?;
    let IpcPayload::IncidentUnprotected { incident_id } = payload else {
        return Err("daemon returned the wrong payload for unprotect".into());
    };
    let report = IncidentProtectionReport::unprotected(incident_id);
    if arguments.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Exact incident protection was removed.");
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
        write_private_diagnostics(&output, &document, arguments.force)?;
        println!("Wrote redacted diagnostics to {}", output.display());
    } else {
        std::io::stdout().write_all(&document)?;
        std::io::stdout().write_all(b"\n")?;
    }
    Ok(())
}

fn write_private_diagnostics(
    output: &std::path::Path,
    document: &[u8],
    force: bool,
) -> Result<(), Box<dyn Error>> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).mode(0o600);
    if force {
        options.create(true);
    } else {
        options.create_new(true);
    }
    #[cfg(target_os = "macos")]
    options.custom_flags(libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC);
    #[cfg(all(unix, not(target_os = "macos")))]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);

    let mut file = options.open(output)?;
    let metadata = file.metadata()?;
    #[cfg(unix)]
    {
        if !metadata.file_type().is_file() || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(format!(
                "diagnostic output must be a regular file owned by the current user: {}",
                output.display()
            )
            .into());
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    if force {
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
    }
    file.write_all(document)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn daemon_status_ready(status: &DaemonStatus) -> bool {
    if status.managed {
        status.ready
            && !status.draining
            && matches!(
                status.startup_state,
                unlinger_daemon::StartupState::ReadyReportOnly
                    | unlinger_daemon::StartupState::ReadyEnforce
            )
    } else {
        status.healthy && status.last_scan_at_unix_millis.is_some()
    }
}

fn doctor_health(
    source_healthy: bool,
    daemon_required: bool,
    daemon_status: Option<&DaemonStatus>,
) -> DoctorHealth {
    let daemon_reachable = daemon_status.is_some();
    let daemon_healthy = daemon_status.is_some_and(|status| status.healthy);
    let daemon_ready = daemon_status.is_some_and(daemon_status_ready);
    DoctorHealth {
        daemon_reachable,
        daemon_healthy,
        daemon_ready,
        overall_healthy: source_healthy
            && (!daemon_required || (daemon_reachable && daemon_healthy && daemon_ready)),
    }
}

fn doctor(socket: &std::path::Path, arguments: DoctorArgs) -> Result<(), Box<dyn Error>> {
    let mut errors = Vec::new();
    let signature_packs = match RuleSet::embedded() {
        Ok(rules) => rules
            .packs()
            .iter()
            .map(|pack| PackSummary {
                id: pack.id.clone(),
                version: pack.version.clone(),
                supported_versions: pack.supported_versions.clone(),
            })
            .collect(),
        Err(_) => {
            errors.push("embedded signature packs could not be loaded".to_owned());
            Vec::new()
        }
    };
    let product_version = macos_product_version();
    let supported_platform = product_version
        .as_deref()
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 14);
    let started = Instant::now();
    let snapshot = MacosSnapshotter::new().capture();
    let elapsed = started.elapsed().as_millis();
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
    let source_healthy = errors.is_empty();
    let daemon_required = !arguments.source_only;
    let daemon_status = daemon_required
        .then(|| IpcClient::new(socket).request(IpcCommand::Status))
        .and_then(Result::ok)
        .and_then(|payload| match payload {
            IpcPayload::Status(status) => Some(status),
            _ => None,
        });
    let health = doctor_health(source_healthy, daemon_required, daemon_status.as_ref());
    if daemon_required {
        if !health.daemon_reachable {
            errors.push("daemon is not reachable".to_owned());
        } else {
            if !health.daemon_healthy {
                errors.push("daemon reports an unhealthy reconciliation engine".to_owned());
            }
            if !health.daemon_ready {
                errors.push("daemon has not reached a ready startup state".to_owned());
            }
        }
    }
    let report = DoctorReport {
        schema_version: 2,
        healthy: health.overall_healthy,
        source_healthy,
        daemon_required,
        daemon_reachable: health.daemon_reachable,
        daemon_healthy: health.daemon_healthy,
        daemon_ready: health.daemon_ready,
        overall_healthy: health.overall_healthy,
        platform: "macOS",
        product_version,
        supported_platform,
        current_uid,
        non_root_user,
        native_snapshot_ok,
        snapshot_elapsed_millis: elapsed,
        coverage,
        signature_packs,
        daemon_status: daemon_status.as_ref().map(DoctorDaemonStatus::from),
        errors,
    };

    if arguments.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Unlinger doctor");
        println!("source healthy: {}", report.source_healthy);
        println!("daemon required: {}", report.daemon_required);
        println!("daemon reachable: {}", report.daemon_reachable);
        println!("daemon healthy: {}", report.daemon_healthy);
        println!("daemon ready: {}", report.daemon_ready);
        println!("overall healthy: {}", report.overall_healthy);
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
        for error in &report.errors {
            println!("error: {error}");
        }
    }
    doctor_result(&report)
}

fn doctor_result(report: &DoctorReport) -> Result<(), Box<dyn Error>> {
    if report.overall_healthy {
        Ok(())
    } else if report.daemon_required {
        Err("doctor found one or more blocking source/platform/daemon readiness failures".into())
    } else {
        Err("source-only doctor found one or more blocking source/platform failures".into())
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
            task_controllers: Vec::new(),
        },
    ))
}

fn print_status(status: &DaemonStatus) {
    println!("Unlinger daemon");
    for line in daemon_status_lines(status) {
        println!("{line}");
    }
}

fn daemon_status_lines(status: &DaemonStatus) -> Vec<String> {
    let mut lines = vec![format!("healthy: {}", status.healthy)];
    if status.managed {
        lines.push(match status.activation_generation {
            Some(generation) => format!("managed generation: {generation}"),
            None => "managed generation: missing".to_owned(),
        });
        lines.push(format!("ready: {}", status.ready));
        lines.push(format!("startup: {:?}", status.startup_state));
        lines.push(format!("requested mode: {:?}", status.requested_mode));
        lines.push(format!("effective mode: {:?}", status.effective_mode()));
        lines.push(match status.armed_generation {
            Some(generation) => format!("armed generation: {generation}"),
            None => "armed generation: none".to_owned(),
        });
    } else {
        lines.push("runtime: legacy or explicitly unmanaged".to_owned());
        lines.push(format!("effective mode: {:?}", status.effective_mode()));
    }
    lines.push(format!(
        "activity: {}",
        if status.cleanup_in_progress {
            "cleanup"
        } else if status.scan_in_progress {
            "scan"
        } else {
            "idle"
        }
    ));
    if let Some(deadline) = status.paused_until_unix_millis {
        lines.push(format!(
            "automatic cleanup: paused until Unix ms {deadline}"
        ));
    } else {
        lines.push("automatic cleanup: not paused".to_owned());
    }
    lines.push(
        if status.lifecycle_schema_version == 0 && status.ipc_schema_version == 0 {
            "event source: not reported by legacy daemon".to_owned()
        } else if status.event_source_healthy {
            "event source: healthy".to_owned()
        } else {
            "event source: degraded; periodic reconciliation remains active".to_owned()
        },
    );
    if let Some(recovery) = &status.storage_recovery {
        lines.push(format!(
            "storage recovery: {:?} at Unix ms {}; {} sidecar(s) quarantined",
            recovery.reason, recovery.occurred_at_unix_millis, recovery.quarantined_sidecar_count
        ));
    } else {
        lines.push("storage recovery: none reported".to_owned());
    }
    lines.push(format!(
        "current incidents: {} confirmed, {} ambiguous",
        status.confirmed_incidents, status.ambiguous_incidents
    ));
    lines.push(format!(
        "blocked cleanups: {}",
        status.attention.blocked_cleanup_count
    ));
    lines.push(format!(
        "protected incidents: {}",
        status.protected_incident_count
    ));
    for item in status
        .attention
        .items
        .iter()
        .take(MAX_PRINT_ATTENTION_ITEMS)
    {
        let mut line = format!("attention: {:?} reason={}", item.kind, item.reason_id);
        if let Some(state) = item.state {
            line.push_str(&format!(" state={state:?}"));
        }
        if let Some(occurred_at) = item.occurred_at_unix_millis {
            line.push_str(&format!(" at Unix ms {occurred_at}"));
        }
        lines.push(line);
    }
    if status.attention.items.len() > MAX_PRINT_ATTENTION_ITEMS {
        lines.push(format!(
            "attention items omitted: {}",
            status.attention.items.len() - MAX_PRINT_ATTENTION_ITEMS
        ));
    }
    if let Some(reclaim) = &status.most_recent_reclaim {
        lines.push(format!(
            "latest reclaim: {:?} at Unix ms {}",
            reclaim.state, reclaim.occurred_at_unix_millis
        ));
    } else {
        lines.push("latest reclaim: none recorded in retained history".to_owned());
    }
    if status.last_error.is_some() {
        lines.push("last error: recorded; details withheld from human status output".to_owned());
    }
    lines
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
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "unlinger-diagnostics-{}-{nonce}-{}",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create temp directory");
            Self(fs::canonicalize(path).expect("canonical temp directory"))
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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

    #[test]
    fn socket_override_remains_accepted_after_an_ipc_subcommand() {
        let cli = Cli::try_parse_from(["unlinger", "status", "--socket", "/tmp/unlinger.sock"])
            .expect("parse legacy global socket position");

        assert_eq!(cli.socket, Some(PathBuf::from("/tmp/unlinger.sock")));
        assert!(matches!(cli.command, Commands::Status(_)));
    }

    #[test]
    fn browser_status_parser_keeps_human_and_json_modes_explicit() {
        let human =
            Cli::try_parse_from(["unlinger", "browser", "status"]).expect("browser status command");
        let Commands::Browser(human) = human.command else {
            panic!("browser command");
        };
        assert!(matches!(
            human.command,
            BrowserCommand::Status(OutputArgs { json: false })
        ));

        let json = Cli::try_parse_from(["unlinger", "browser", "status", "--json"])
            .expect("browser status JSON command");
        let Commands::Browser(json) = json.command else {
            panic!("browser command");
        };
        assert!(matches!(
            json.command,
            BrowserCommand::Status(OutputArgs { json: true })
        ));
    }

    #[test]
    fn browser_status_human_projection_uses_the_atomic_snapshot_without_fake_totals() {
        let response: unlinger_protocol::ResponseEnvelope = serde_json::from_str(include_str!(
            "../../../apps/UnlingerApp/Contract/v4/browser-overview-confirmed.json"
        ))
        .expect("decode canonical browser overview");
        let Some(unlinger_protocol::Payload::BrowserOverview(overview)) = response.payload else {
            panic!("expected browser overview payload");
        };
        let lines = browser_status_lines(&overview);
        assert!(
            lines
                .iter()
                .any(|line| line == "Browser overview: confirmed")
        );
        assert!(lines.iter().any(|line| line == "Mode: report-only"));
        assert!(
            lines
                .iter()
                .any(|line| line == "Observation freshness: current")
        );
        assert!(lines.iter().any(|line| line == "Browser sessions: 1"));
        assert!(lines.iter().any(|line| line.contains("Chrome for Testing")));
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("Support catalog: rules:"))
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.starts_with("Total processes:"))
        );
        assert!(!lines.iter().any(|line| line.starts_with("Total memory:")));
    }

    #[test]
    fn doctor_parser_distinguishes_default_readiness_from_source_only() {
        let default = Cli::try_parse_from(["unlinger", "doctor"]).expect("default doctor");
        let Commands::Doctor(default) = default.command else {
            panic!("doctor command");
        };
        assert!(!default.source_only);

        let source_only = Cli::try_parse_from(["unlinger", "doctor", "--source-only", "--json"])
            .expect("source-only doctor");
        let Commands::Doctor(source_only) = source_only.command else {
            panic!("doctor command");
        };
        assert!(source_only.source_only);
        assert!(source_only.json);
    }

    #[test]
    fn service_candidate_commands_are_explicit_and_report_only() {
        let accept = Cli::try_parse_from(["unlinger", "service", "accept-candidate", "--json"])
            .expect("accept candidate command");
        let Commands::Service(accept) = accept.command else {
            panic!("service command");
        };
        assert!(matches!(
            accept.command,
            ServiceCommand::AcceptCandidate(OutputArgs { json: true })
        ));

        let rollback = Cli::try_parse_from(["unlinger", "service", "rollback-candidate", "--json"])
            .expect("rollback candidate command");
        let Commands::Service(rollback) = rollback.command else {
            panic!("service command");
        };
        assert!(matches!(
            rollback.command,
            ServiceCommand::RollbackCandidate(OutputArgs { json: true })
        ));

        let restart = Cli::try_parse_from(["unlinger", "service", "restart-report-only", "--json"])
            .expect("restart report-only command");
        let Commands::Service(restart) = restart.command else {
            panic!("service command");
        };
        assert!(matches!(
            restart.command,
            ServiceCommand::RestartReportOnly(OutputArgs { json: true })
        ));
    }

    #[test]
    fn protect_and_unprotect_parsers_require_one_exact_incident() {
        let protect = Cli::try_parse_from(["unlinger", "protect", "incident-exact-1", "--json"])
            .expect("protect command");
        let Commands::Protect(protect) = protect.command else {
            panic!("protect command");
        };
        assert_eq!(protect.incident_id, "incident-exact-1");
        assert!(protect.json);

        let unprotect = Cli::try_parse_from(["unlinger", "unprotect", "incident-exact-1"])
            .expect("unprotect command");
        let Commands::Unprotect(unprotect) = unprotect.command else {
            panic!("unprotect command");
        };
        assert_eq!(unprotect.incident_id, "incident-exact-1");
        assert!(!unprotect.json);

        assert!(Cli::try_parse_from(["unlinger", "protect"]).is_err());
        assert!(Cli::try_parse_from(["unlinger", "unprotect"]).is_err());
    }

    #[test]
    fn protection_json_reports_exact_state_without_widening_scope() {
        let protected = unlinger_daemon::ProtectedIncidentSummary {
            incident_id: "incident-exact-1".to_owned(),
            protected_at_unix_millis: 100,
            last_exact_observed_at_unix_millis: Some(90),
            exact_absence_since_unix_millis: None,
        };

        let protected_json = serde_json::to_value(IncidentProtectionReport::protected(&protected))
            .expect("protected JSON");
        assert_eq!(protected_json["schema_version"], 1);
        assert_eq!(protected_json["incident_id"], "incident-exact-1");
        assert_eq!(protected_json["protected"], true);
        assert_eq!(protected_json["protected_at_unix_millis"], 100);
        assert_eq!(protected_json["last_exact_observed_at_unix_millis"], 90);
        assert!(
            protected_json
                .get("exact_absence_since_unix_millis")
                .is_none()
        );

        let unprotected_json = serde_json::to_value(IncidentProtectionReport::unprotected(
            "incident-exact-1".to_owned(),
        ))
        .expect("unprotected JSON");
        assert_eq!(unprotected_json["incident_id"], "incident-exact-1");
        assert_eq!(unprotected_json["protected"], false);
        assert!(unprotected_json.get("protected_at_unix_millis").is_none());
    }

    #[test]
    fn doctor_help_explains_default_readiness_and_source_only_mode() {
        use clap::CommandFactory;

        let mut command = Cli::command();
        let doctor = command
            .find_subcommand_mut("doctor")
            .expect("doctor subcommand");
        let help = doctor.render_long_help().to_string();

        assert!(help.contains("by default, installed daemon readiness"));
        assert!(help.contains("--source-only"));
        assert!(help.contains("without requiring a running daemon"));
    }

    #[test]
    fn default_doctor_marks_an_unreachable_daemon_unhealthy() {
        let health = doctor_health(true, true, None);

        assert!(!health.daemon_reachable);
        assert!(!health.daemon_healthy);
        assert!(!health.daemon_ready);
        assert!(!health.overall_healthy);
    }

    #[test]
    fn managed_doctor_requires_a_ready_startup_state() {
        let mut status = DaemonStatus::new(unlinger_daemon::DaemonMode::ReportOnly, 42);
        status.managed = true;
        status.healthy = true;
        status.startup_state = unlinger_daemon::StartupState::Recovering;

        let recovering = doctor_health(true, true, Some(&status));
        assert!(recovering.daemon_reachable);
        assert!(recovering.daemon_healthy);
        assert!(!recovering.daemon_ready);
        assert!(!recovering.overall_healthy);

        status.ready = true;
        status.startup_state = unlinger_daemon::StartupState::ReadyReportOnly;
        let ready = doctor_health(true, true, Some(&status));
        assert!(ready.daemon_ready);
        assert!(ready.overall_healthy);
    }

    #[test]
    fn source_only_doctor_does_not_claim_daemon_readiness() {
        let health = doctor_health(true, false, None);

        assert!(!health.daemon_reachable);
        assert!(!health.daemon_healthy);
        assert!(!health.daemon_ready);
        assert!(health.overall_healthy);
    }

    #[test]
    fn doctor_json_keeps_source_and_daemon_health_distinct() {
        let report = DoctorReport {
            schema_version: 2,
            healthy: false,
            source_healthy: true,
            daemon_required: true,
            daemon_reachable: false,
            daemon_healthy: false,
            daemon_ready: false,
            overall_healthy: false,
            platform: "macOS",
            product_version: Some("15.0".to_owned()),
            supported_platform: true,
            current_uid: 501,
            non_root_user: true,
            native_snapshot_ok: true,
            snapshot_elapsed_millis: 5,
            coverage: None,
            signature_packs: Vec::new(),
            daemon_status: None,
            errors: vec!["daemon is not reachable".to_owned()],
        };

        let json = serde_json::to_value(&report).expect("doctor JSON");
        assert_eq!(json["healthy"], false);
        assert_eq!(json["source_healthy"], true);
        assert_eq!(json["daemon_required"], true);
        assert_eq!(json["daemon_reachable"], false);
        assert_eq!(json["daemon_healthy"], false);
        assert_eq!(json["daemon_ready"], false);
        assert_eq!(json["overall_healthy"], false);
        assert!(doctor_result(&report).is_err());
    }

    #[test]
    fn human_status_is_bounded_and_omits_private_runtime_identifiers() {
        let mut status = DaemonStatus::new(unlinger_daemon::DaemonMode::ReportOnly, 4242);
        status.managed = true;
        status.instance_id = "private-instance-id".to_owned();
        status.activation_generation = Some(9);
        status.ready = true;
        status.startup_state = unlinger_daemon::StartupState::ReadyReportOnly;
        status.healthy = true;
        status.event_source_healthy = false;
        status.last_event_source_error = Some("/Users/private/event-source".to_owned());
        status.storage_recovery = Some(unlinger_daemon::StorageRecoveryStatus {
            recovery_id: "private-recovery-id".to_owned(),
            public_token: "public-recovery-token".to_owned(),
            occurred_at_unix_millis: 123,
            reason: unlinger_daemon::StorageRecoveryReason::IntegrityCheckFailed,
            quarantined_sidecar_count: 2,
        });
        status.attention = unlinger_daemon::AttentionProjection {
            blocked_cleanup_count: 23,
            items: (0..20)
                .map(|index| unlinger_daemon::AttentionItem {
                    event_token: None,
                    outcome: None,
                    kind: unlinger_daemon::AttentionKind::CleanupFailed,
                    reason_id: "cleanup.delivery_unknown".to_owned(),
                    incident_id: Some(format!("private-incident-{index}")),
                    state: Some(IncidentState::Failed),
                    occurred_at_unix_millis: Some(200 + index),
                })
                .collect(),
        };
        status.protected_incident_count = 17;
        status.protected_incidents = vec![unlinger_daemon::ProtectedIncidentSummary {
            incident_id: "private-protected-incident".to_owned(),
            protected_at_unix_millis: 300,
            last_exact_observed_at_unix_millis: Some(290),
            exact_absence_since_unix_millis: None,
        }];

        let lines = daemon_status_lines(&status);
        let rendered = lines.join("\n");
        assert!(rendered.contains("managed generation: 9"));
        assert!(rendered.contains("startup: ReadyReportOnly"));
        assert!(rendered.contains("effective mode: ReportOnly"));
        assert!(rendered.contains("event source: degraded"));
        assert!(rendered.contains("storage recovery: IntegrityCheckFailed"));
        assert!(rendered.contains("blocked cleanups: 23"));
        assert!(rendered.contains("protected incidents: 17"));
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.starts_with("attention: "))
                .count(),
            16
        );
        assert!(!rendered.contains("private-instance-id"));
        assert!(!rendered.contains("private-recovery-id"));
        assert!(!rendered.contains("private-incident"));
        assert!(!rendered.contains("private-protected-incident"));
        assert!(!rendered.contains("/Users/private"));
    }

    #[test]
    fn human_service_status_shows_generation_lifecycle_without_private_paths() {
        let mut daemon = DaemonStatus::new(unlinger_daemon::DaemonMode::ReportOnly, 4242);
        daemon.managed = true;
        daemon.instance_id = "private-instance-id".to_owned();
        daemon.activation_generation = Some(9);
        daemon.ready = true;
        daemon.startup_state = unlinger_daemon::StartupState::ReadyReportOnly;
        daemon.healthy = true;
        let private_root = PathBuf::from("/Users/private/Library/Application Support/Unlinger");
        let report = service::ServiceStatusReport {
            schema_version: 3,
            label: "app.unlinger.daemon",
            installed: true,
            loaded: true,
            healthy: true,
            unmanaged_daemon: false,
            expected_mode: Some(unlinger_daemon::DaemonMode::ReportOnly),
            active_generation: Some(9),
            launchd_pid: Some(4242),
            daemon_status: Some(daemon),
            pid_matches: true,
            generation_matches: true,
            binary_matches: true,
            permissions_ok: true,
            launch_agent_path: PathBuf::from("/Users/private/Library/LaunchAgents/private.plist"),
            daemon_path: private_root.join("generations/9/unlingerd"),
            cli_path: private_root.join("generations/9/unlinger"),
            database_path: private_root.join("history.sqlite3"),
            socket_path: private_root.join("unlingerd.sock"),
            data_preserved: true,
            acceptance: Some(service::ServiceAcceptanceReport {
                phase: "candidate_ready_report_only",
                candidate_generation: 10,
                prior_generation: Some(9),
                rollback_available: true,
                database_backup_present: true,
            }),
            errors: vec!["unsafe path /Users/private".to_owned()],
        };

        let rendered = service_status_lines(&report).join("\n");

        assert!(rendered.contains("active generation: 9"));
        assert!(rendered.contains("startup: ReadyReportOnly"));
        assert!(rendered.contains("effective mode: ReportOnly"));
        assert!(rendered.contains("phase: candidate_ready_report_only"));
        assert!(rendered.contains("rollback generation: 9"));
        assert!(rendered.contains("reported problems: 1"));
        assert!(!rendered.contains("4242"));
        assert!(!rendered.contains("private-instance-id"));
        assert!(!rendered.contains("/Users/private"));
    }

    #[test]
    fn private_diagnostics_refuses_to_replace_without_force() {
        let temp = TempDirectory::new();
        let output = temp.0.join("diagnostics.json");
        fs::write(&output, b"original").expect("seed output");

        assert!(write_private_diagnostics(&output, b"replacement", false).is_err());
        assert_eq!(fs::read(&output).expect("read output"), b"original");
    }

    #[test]
    fn forced_private_diagnostics_replaces_and_tightens_the_exact_file() {
        let temp = TempDirectory::new();
        let output = temp.0.join("diagnostics.json");
        fs::write(&output, b"longer original contents").expect("seed output");
        fs::set_permissions(&output, fs::Permissions::from_mode(0o644))
            .expect("loosen output mode");

        write_private_diagnostics(&output, b"{}", true).expect("replace output");

        assert_eq!(fs::read(&output).expect("read output"), b"{}\n");
        assert_eq!(
            fs::metadata(&output)
                .expect("output metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn forced_private_diagnostics_refuses_a_symlink_and_preserves_its_target() {
        let temp = TempDirectory::new();
        let target = temp.0.join("private-target");
        let output = temp.0.join("diagnostics.json");
        fs::write(&target, b"must remain untouched").expect("seed target");
        symlink(&target, &output).expect("create diagnostic symlink");

        assert!(write_private_diagnostics(&output, b"replacement", true).is_err());
        assert_eq!(
            fs::read(&target).expect("read target"),
            b"must remain untouched"
        );
    }
}
