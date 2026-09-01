use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unlinger_core::ProcessIdentity;
use unlinger_daemon::{
    DaemonInstanceLock, DaemonLockError, DaemonMode, DaemonStatus, HistoryStore, IpcClient,
    IpcCommand, IpcPayload, LAUNCH_AGENT_LABEL, LocalPaths, StartupState,
};
use unlinger_macos::MacosSnapshotter;

const SERVICE_SCHEMA_VERSION: u32 = 3;
const SERVICE_MANIFEST_SCHEMA_VERSION: u32 = 1;
const SERVICE_TRANSACTION_SCHEMA_VERSION: u32 = 2;
const SERVICE_START_TIMEOUT: Duration = Duration::from_secs(120);
const SERVICE_STOP_TIMEOUT: Duration = Duration::from_secs(125);
const SERVICE_POLL_INTERVAL: Duration = Duration::from_millis(200);
const SERVICE_IPC_IO_TIMEOUT: Duration = Duration::from_secs(15);
const EXIT_TIMEOUT_SECONDS: u64 = 120;
const THROTTLE_INTERVAL_SECONDS: u64 = 10;

static NEXT_STAGE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize)]
pub struct ServiceStatusReport {
    pub schema_version: u32,
    pub label: &'static str,
    pub installed: bool,
    pub loaded: bool,
    pub healthy: bool,
    pub unmanaged_daemon: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_mode: Option<DaemonMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launchd_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_status: Option<DaemonStatus>,
    pub pid_matches: bool,
    pub generation_matches: bool,
    pub binary_matches: bool,
    pub permissions_ok: bool,
    pub launch_agent_path: PathBuf,
    pub daemon_path: PathBuf,
    pub cli_path: PathBuf,
    pub database_path: PathBuf,
    pub socket_path: PathBuf,
    pub data_preserved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<ServiceAcceptanceReport>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServiceAcceptanceReport {
    pub phase: &'static str,
    pub candidate_generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_generation: Option<u64>,
    pub rollback_available: bool,
    pub database_backup_present: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ActiveServiceManifest {
    schema_version: u32,
    active_generation: u64,
    desired_mode: DaemonMode,
}

impl ActiveServiceManifest {
    fn new(active_generation: u64, desired_mode: DaemonMode) -> Self {
        Self {
            schema_version: SERVICE_MANIFEST_SCHEMA_VERSION,
            active_generation,
            desired_mode,
        }
    }

    fn rollback_floor(&self) -> Self {
        Self::new(self.active_generation, DaemonMode::ReportOnly)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct GenerationManifest {
    schema_version: u32,
    activation_generation: u64,
    daemon_file: String,
    cli_file: String,
}

impl GenerationManifest {
    fn new(activation_generation: u64) -> Self {
        Self {
            schema_version: SERVICE_MANIFEST_SCHEMA_VERSION,
            activation_generation,
            daemon_file: "unlingerd".to_owned(),
            cli_file: "unlinger".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TransactionPhase {
    Prepared,
    CandidateDurable,
    PriorDrained,
    DatabaseBackedUp,
    CandidateSelected,
    CandidateReadyReportOnly,
    AcceptanceInProgress,
    Accepted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransactionRecoveryDisposition {
    RollbackPrior,
    HoldForExplicitDecision,
    FinalizeAccepted,
}

fn transaction_recovery_disposition(phase: TransactionPhase) -> TransactionRecoveryDisposition {
    match phase {
        TransactionPhase::Prepared
        | TransactionPhase::CandidateDurable
        | TransactionPhase::PriorDrained
        | TransactionPhase::DatabaseBackedUp
        | TransactionPhase::CandidateSelected
        | TransactionPhase::AcceptanceInProgress => TransactionRecoveryDisposition::RollbackPrior,
        TransactionPhase::CandidateReadyReportOnly => {
            TransactionRecoveryDisposition::HoldForExplicitDecision
        }
        TransactionPhase::Accepted => TransactionRecoveryDisposition::FinalizeAccepted,
    }
}

fn transaction_phase_name(phase: TransactionPhase) -> &'static str {
    match phase {
        TransactionPhase::Prepared => "prepared",
        TransactionPhase::CandidateDurable => "candidate_durable",
        TransactionPhase::PriorDrained => "prior_drained",
        TransactionPhase::DatabaseBackedUp => "database_backed_up",
        TransactionPhase::CandidateSelected => "candidate_selected",
        TransactionPhase::CandidateReadyReportOnly => "candidate_ready_report_only",
        TransactionPhase::AcceptanceInProgress => "acceptance_in_progress",
        TransactionPhase::Accepted => "accepted",
    }
}

fn candidate_database_may_have_changed(phase: TransactionPhase) -> bool {
    matches!(
        phase,
        TransactionPhase::CandidateSelected
            | TransactionPhase::CandidateReadyReportOnly
            | TransactionPhase::AcceptanceInProgress
    )
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct InstallTransaction {
    schema_version: u32,
    phase: TransactionPhase,
    candidate_generation: u64,
    prior_manifest: Option<ActiveServiceManifest>,
    prior_plist: Option<String>,
    prior_was_loaded: bool,
    #[serde(default)]
    database_backed_up: bool,
}

impl InstallTransaction {
    fn new(
        candidate_generation: u64,
        prior_manifest: Option<ActiveServiceManifest>,
        prior_plist: Option<String>,
        prior_was_loaded: bool,
    ) -> Self {
        Self {
            schema_version: SERVICE_TRANSACTION_SCHEMA_VERSION,
            phase: TransactionPhase::Prepared,
            candidate_generation,
            prior_manifest,
            prior_plist,
            prior_was_loaded,
            database_backed_up: false,
        }
    }
}

#[derive(Clone, Debug)]
struct ServiceLayout {
    generations: PathBuf,
    active_manifest: PathBuf,
    transaction: PathBuf,
    database_backup: PathBuf,
    database_backup_pending: PathBuf,
    failed_generations: PathBuf,
}

impl ServiceLayout {
    fn new(paths: &LocalPaths) -> Self {
        Self {
            generations: paths.application_support.join("generations"),
            active_manifest: paths.application_support.join("service.json"),
            transaction: paths.application_support.join("service-transaction.json"),
            database_backup: paths.application_support.join("history.rollback.sqlite3"),
            database_backup_pending: paths
                .application_support
                .join("history.rollback.sqlite3.pending"),
            failed_generations: paths.application_support.join("failed-generations"),
        }
    }

    fn generation(&self, activation_generation: u64) -> GenerationPaths {
        let directory = self.generations.join(activation_generation.to_string());
        GenerationPaths {
            daemon: directory.join("unlingerd"),
            cli: directory.join("unlinger"),
            manifest: directory.join("manifest.json"),
            directory,
            activation_generation,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GenerationPaths {
    directory: PathBuf,
    pub daemon: PathBuf,
    pub cli: PathBuf,
    manifest: PathBuf,
    activation_generation: u64,
}

#[derive(Debug)]
pub struct ServiceError {
    message: String,
}

impl ServiceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn context(context: &str, error: impl Display) -> Self {
        Self::new(format!("{context}: {error}"))
    }
}

impl Display for ServiceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ServiceError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LaunchdState {
    loaded: bool,
    pid: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct LaunchAgentDocument {
    #[serde(rename = "Label")]
    label: String,
    #[serde(rename = "Program")]
    program: String,
    #[serde(rename = "ProgramArguments")]
    program_arguments: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchAgentExpectation {
    Managed(u64),
    Legacy(DaemonMode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReportOnlyRecoveryRoute {
    Running(u32),
    Offline,
}

fn report_only_recovery_route(state: LaunchdState) -> ReportOnlyRecoveryRoute {
    match (state.loaded, state.pid) {
        (true, Some(pid)) => ReportOnlyRecoveryRoute::Running(pid),
        _ => ReportOnlyRecoveryRoute::Offline,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetModeRoute {
    ReportOnlyContainment,
    EnforceOnline(u32),
    RejectEnforce,
}

fn set_mode_route(mode: DaemonMode, state: LaunchdState) -> SetModeRoute {
    match (mode, state.loaded, state.pid) {
        (DaemonMode::ReportOnly, _, _) => SetModeRoute::ReportOnlyContainment,
        (DaemonMode::Enforce, true, Some(pid)) => SetModeRoute::EnforceOnline(pid),
        (DaemonMode::Enforce, _, _) => SetModeRoute::RejectEnforce,
    }
}

pub fn install(
    paths: &LocalPaths,
    source_cli: &Path,
    source_daemon: &Path,
    mode: DaemonMode,
) -> Result<ServiceStatusReport, ServiceError> {
    validate_acceptance_install_mode(mode)?;
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let layout = ServiceLayout::new(paths);
    recover_incomplete_install(paths, &layout, uid)?;

    let old_launchd = launchd_state(uid)?;
    let old_status = match (old_launchd.loaded, old_launchd.pid) {
        (true, Some(pid)) => ipc_status_for_launchd(paths, Some(pid))?,
        _ => None,
    };
    let mut daemon_lock = match (old_launchd.loaded, old_launchd.pid) {
        (true, None) => {
            bootout_and_wait(uid, None)?;
            Some(prove_daemon_offline(paths, SERVICE_STOP_TIMEOUT)?)
        }
        (false, _) => Some(prove_daemon_offline(paths, SERVICE_STOP_TIMEOUT)?),
        (true, Some(_)) => None,
    };

    let prior_manifest = read_optional_manifest(&layout.active_manifest)?;
    let prior_plist = read_optional_text(&paths.launch_agent)?;
    let generation_number = next_generation(&layout)?;
    let generation = layout.generation(generation_number);
    let mut transaction = InstallTransaction::new(
        generation_number,
        prior_manifest,
        prior_plist,
        old_launchd.loaded,
    );
    if transaction.prior_plist.is_none()
        && (transaction.prior_manifest.is_some() || transaction.prior_was_loaded)
    {
        return Err(ServiceError::new(
            "prior service has no durable LaunchAgent rollback material; installation was not started",
        ));
    }
    if let Some(prior_plist) = transaction.prior_plist.as_deref() {
        validate_report_only_rollback_plist(paths, &layout, &transaction, prior_plist)?;
    }
    persist_transaction(&layout, &transaction)?;

    let install_result = (|| {
        create_generation(&layout, &generation, source_cli, source_daemon)?;
        transaction.phase = TransactionPhase::CandidateDurable;
        persist_transaction(&layout, &transaction)?;

        let plist = launch_agent_plist(paths, &generation);
        validate_plist_bytes(
            plist.as_bytes(),
            &generation.daemon,
            LaunchAgentExpectation::Managed(generation_number),
        )?;

        if old_launchd.loaded && old_launchd.pid.is_some() {
            quiesce_loaded_service(paths, uid, old_launchd, old_status.as_ref())?;
            daemon_lock = Some(prove_daemon_offline(paths, SERVICE_STOP_TIMEOUT)?);
        }
        let offline = daemon_lock.as_ref().ok_or_else(|| {
            ServiceError::new("daemon offline proof was lost before installation mutation")
        })?;
        let _ = offline;
        transaction.phase = TransactionPhase::PriorDrained;
        persist_transaction(&layout, &transaction)?;

        transaction.database_backed_up = backup_database(&paths.database, &layout)?;
        transaction.phase = TransactionPhase::DatabaseBackedUp;
        persist_transaction(&layout, &transaction)?;

        write_bytes_atomic(&paths.launch_agent, plist.as_bytes(), 0o600)?;
        let active = ActiveServiceManifest::new(generation_number, DaemonMode::ReportOnly);
        write_json_atomic(&layout.active_manifest, &active, 0o600)?;
        transaction.phase = TransactionPhase::CandidateSelected;
        persist_transaction(&layout, &transaction)?;

        drop(daemon_lock.take());
        bootstrap(uid, &paths.launch_agent)?;
        wait_for_generation_ready(paths, uid, generation_number, SERVICE_START_TIMEOUT)?;
        transaction.phase = TransactionPhase::CandidateReadyReportOnly;
        persist_transaction(&layout, &transaction)?;
        Ok::<(), ServiceError>(())
    })();
    drop(daemon_lock.take());

    if let Err(error) = install_result {
        let recovery = recover_incomplete_install(paths, &layout, uid);
        return match recovery {
            Ok(()) => Err(ServiceError::new(format!(
                "candidate installation failed: {error}; prior service restored report-only"
            ))),
            Err(recovery_error) => Err(ServiceError::new(format!(
                "candidate installation failed: {error}; report-only recovery failed: {recovery_error}"
            ))),
        };
    }

    wait_for_mode(paths, uid, DaemonMode::ReportOnly, SERVICE_START_TIMEOUT)
}

fn validate_acceptance_install_mode(mode: DaemonMode) -> Result<(), ServiceError> {
    if mode == DaemonMode::Enforce {
        return Err(ServiceError::new(
            "candidate installation is report-only; install first, verify the candidate, then explicitly accept or roll it back",
        ));
    }
    Ok(())
}

pub fn accept_candidate(paths: &LocalPaths) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let layout = ServiceLayout::new(paths);
    let Some(mut transaction) = read_optional_transaction(&layout.transaction)? else {
        return Err(ServiceError::new("there is no candidate acceptance lease"));
    };
    match transaction.phase {
        TransactionPhase::CandidateReadyReportOnly => {}
        TransactionPhase::Accepted => {
            finalize_accepted_lease(paths, &layout, &transaction)?;
            return status(paths);
        }
        _ => {
            recover_incomplete_install(paths, &layout, uid)?;
            return Err(ServiceError::new(
                "the incomplete candidate transaction was rolled back instead of accepted",
            ));
        }
    }
    validate_candidate_acceptance_state(paths, &layout, &transaction)?;
    transaction.phase = TransactionPhase::AcceptanceInProgress;
    persist_transaction(&layout, &transaction)?;

    let acceptance_commit = (|| {
        validate_candidate_acceptance_state(paths, &layout, &transaction)?;
        transaction.phase = TransactionPhase::Accepted;
        persist_transaction(&layout, &transaction)
    })();
    if let Err(error) = acceptance_commit {
        let recovery = recover_incomplete_install(paths, &layout, uid);
        return match recovery {
            Ok(()) => {
                let report = status(paths)?;
                if generation_runtime_is_ready_report_only(
                    &report,
                    transaction.candidate_generation,
                ) {
                    Ok(report)
                } else {
                    Err(ServiceError::new(format!(
                        "candidate acceptance failed: {error}; prior service restored report-only"
                    )))
                }
            }
            Err(recovery_error) => Err(ServiceError::new(format!(
                "candidate acceptance failed: {error}; report-only recovery failed: {recovery_error}"
            ))),
        };
    }
    finalize_accepted_lease(paths, &layout, &transaction).map_err(|error| {
        ServiceError::new(format!(
            "candidate acceptance is durable, but rollback-material cleanup is incomplete: {error}"
        ))
    })?;
    status(paths)
}

pub fn rollback_candidate(paths: &LocalPaths) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let layout = ServiceLayout::new(paths);
    let Some(transaction) = read_optional_transaction(&layout.transaction)? else {
        return Err(ServiceError::new("there is no candidate acceptance lease"));
    };
    if transaction.phase == TransactionPhase::Accepted {
        finalize_accepted_lease(paths, &layout, &transaction)?;
        return Err(ServiceError::new(
            "candidate acceptance is already durable; rollback material has been retired",
        ));
    }
    rollback_install_transaction(paths, &layout, uid, transaction)?;
    status(paths)
}

pub fn restart_report_only(paths: &LocalPaths) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let layout = ServiceLayout::new(paths);
    if let Some(transaction) = read_optional_transaction(&layout.transaction)? {
        match transaction.phase {
            TransactionPhase::CandidateReadyReportOnly => {
                validate_candidate_acceptance_state(paths, &layout, &transaction)?;
            }
            TransactionPhase::Accepted => {
                finalize_accepted_lease(paths, &layout, &transaction)?;
            }
            _ => {
                recover_incomplete_install(paths, &layout, uid)?;
                return Err(ServiceError::new(
                    "the incomplete candidate transaction was rolled back before restart",
                ));
            }
        }
    }
    let manifest = read_optional_manifest(&layout.active_manifest)?
        .ok_or_else(|| ServiceError::new("managed service manifest is missing"))?;
    if manifest.desired_mode != DaemonMode::ReportOnly {
        return Err(ServiceError::new(
            "restart-report-only refuses a service whose desired mode is enforce",
        ));
    }
    emergency_restart_generation_report_only(
        paths,
        &layout,
        uid,
        manifest.active_generation,
        "explicit report-only acceptance restart",
    )?;
    wait_for_generation_ready(
        paths,
        uid,
        manifest.active_generation,
        SERVICE_START_TIMEOUT,
    )
}

fn validate_candidate_acceptance_state(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    transaction: &InstallTransaction,
) -> Result<ServiceStatusReport, ServiceError> {
    let acceptance = inspect_acceptance_lease(paths, layout)?
        .ok_or_else(|| ServiceError::new("candidate acceptance lease disappeared"))?;
    if acceptance.candidate_generation != transaction.candidate_generation
        || acceptance.phase != transaction_phase_name(transaction.phase)
        || !acceptance.rollback_available
    {
        return Err(ServiceError::new(
            "candidate rollback material is incomplete or inconsistent",
        ));
    }
    let report = status(paths)?;
    if !report.healthy
        || report.expected_mode != Some(DaemonMode::ReportOnly)
        || !generation_runtime_is_ready_report_only(&report, transaction.candidate_generation)
    {
        return Err(ServiceError::new(format!(
            "generation {} is not a stable, quiescent report-only candidate",
            transaction.candidate_generation
        )));
    }
    Ok(report)
}

pub fn set_mode(paths: &LocalPaths, mode: DaemonMode) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let layout = ServiceLayout::new(paths);
    recover_incomplete_install(paths, &layout, uid)?;
    set_mode_locked(paths, &layout, mode)
}

pub fn uninstall(paths: &LocalPaths) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let layout = ServiceLayout::new(paths);
    recover_incomplete_install(paths, &layout, uid)?;
    let launchd = launchd_state(uid)?;
    match (launchd.loaded, launchd.pid) {
        (true, Some(pid)) => {
            let daemon_status = ipc_status_for_launchd(paths, Some(pid))?;
            quiesce_loaded_service(paths, uid, launchd, daemon_status.as_ref())?;
        }
        (true, None) => bootout_and_wait(uid, None)?,
        (false, _) => {}
    }
    let daemon_lock = prove_daemon_offline(paths, SERVICE_STOP_TIMEOUT)?;
    for path in [
        &paths.launch_agent,
        &layout.active_manifest,
        &layout.transaction,
        &paths.daemon_binary,
        &paths.cli_binary,
    ] {
        remove_managed_file(path)?;
    }
    remove_generation_tree(&layout)?;
    drop(daemon_lock);
    status(paths)
}

pub fn status(paths: &LocalPaths) -> Result<ServiceStatusReport, ServiceError> {
    let uid = current_uid();
    let launchd = launchd_state(uid)?;
    let mut errors = Vec::new();
    let layout = ServiceLayout::new(paths);
    let active_manifest = match read_optional_manifest(&layout.active_manifest) {
        Ok(manifest) => manifest,
        Err(error) => {
            errors.push(error.to_string());
            None
        }
    };
    let acceptance = match inspect_acceptance_lease(paths, &layout) {
        Ok(acceptance) => acceptance,
        Err(error) => {
            errors.push(error.to_string());
            None
        }
    };
    let active_generation = active_manifest
        .as_ref()
        .map(|manifest| manifest.active_generation);
    let expected_mode = active_manifest
        .as_ref()
        .map(|manifest| manifest.desired_mode)
        .or_else(|| installed_legacy_mode(paths));
    let generation = active_generation.map(|number| layout.generation(number));
    let daemon_path = generation
        .as_ref()
        .map_or_else(|| paths.daemon_binary.clone(), |paths| paths.daemon.clone());
    let cli_path = generation
        .as_ref()
        .map_or_else(|| paths.cli_binary.clone(), |paths| paths.cli.clone());
    let daemon_status = match ipc_status_for_launchd(paths, launchd.pid) {
        Ok(status) => status,
        Err(error) => {
            errors.push(error.to_string());
            None
        }
    };
    let socket_path = if daemon_status.as_ref().is_some_and(|status| !status.managed) {
        paths.cache_directory.join("unlingerd.sock")
    } else {
        paths.socket.clone()
    };
    let plist_generation = match installed_generation_result(paths) {
        Ok(generation) => generation,
        Err(error) => {
            errors.push(error.to_string());
            None
        }
    };
    let installed = paths.launch_agent.is_file()
        && match generation.as_ref() {
            Some(generation) => validate_generation(generation).is_ok(),
            None => paths.daemon_binary.is_file() && paths.cli_binary.is_file(),
        };
    let unmanaged_daemon = !launchd.loaded && daemon_status.is_some();
    let pid_matches = matches!(
        (launchd.pid, daemon_status.as_ref().map(|status| status.pid)),
        (Some(launchd_pid), Some(daemon_pid)) if launchd_pid == daemon_pid
    );
    let generation_matches = match (active_generation, daemon_status.as_ref()) {
        (Some(expected), Some(actual)) => {
            actual.managed
                && actual.activation_generation == Some(expected)
                && plist_generation == Some(expected)
        }
        (None, Some(actual)) => !actual.managed && plist_generation.is_none(),
        (None, None) => true,
        _ => false,
    };
    let binary_matches = match (launchd.pid, generation.as_ref()) {
        (Some(pid), Some(generation)) => process_runs_binary(pid, &generation.daemon),
        (Some(pid), None) => process_runs_binary(pid, &paths.daemon_binary),
        (None, _) => !launchd.loaded,
    };
    let permissions_ok = verify_permissions(paths, generation.as_ref(), &layout, uid, &mut errors);
    if !installed {
        errors.push("service files are not fully installed".to_owned());
    }
    if installed && !launchd.loaded {
        errors.push("LaunchAgent is installed but not loaded".to_owned());
    }
    if launchd.loaded && daemon_status.is_none() {
        errors.push("LaunchAgent is loaded but daemon IPC is unavailable".to_owned());
    }
    if unmanaged_daemon {
        errors.push("daemon IPC is reachable outside the LaunchAgent".to_owned());
    }
    if launchd.loaded && !pid_matches {
        errors.push("launchd PID and daemon IPC PID do not match".to_owned());
    }
    if launchd.loaded && !generation_matches {
        errors.push("LaunchAgent, manifest, and daemon generation do not match".to_owned());
    }
    if launchd.loaded && !binary_matches {
        errors.push("launchd process does not execute the active generation binary".to_owned());
    }
    if let (Some(expected), Some(actual)) = (expected_mode, daemon_status.as_ref())
        && expected != actual.effective_mode()
    {
        errors.push("desired service mode and daemon effective mode do not match".to_owned());
    }
    if daemon_status.as_ref().is_some_and(|status| !status.healthy) {
        errors.push("daemon reports an unhealthy reconciliation engine".to_owned());
    }
    if launchd.loaded
        && daemon_status
            .as_ref()
            .is_some_and(|status| status.last_scan_at_unix_millis.is_none())
    {
        errors.push("daemon has not completed its first reconciliation cycle".to_owned());
    }
    let healthy = installed
        && launchd.loaded
        && pid_matches
        && generation_matches
        && binary_matches
        && permissions_ok
        && expected_mode.is_some()
        && daemon_status.as_ref().is_some_and(|status| {
            status.healthy
                && status.last_scan_at_unix_millis.is_some()
                && (!status.managed || status.ready)
        })
        && daemon_status
            .as_ref()
            .is_some_and(|status| Some(status.effective_mode()) == expected_mode)
        && errors.is_empty();

    Ok(ServiceStatusReport {
        schema_version: SERVICE_SCHEMA_VERSION,
        label: LAUNCH_AGENT_LABEL,
        installed,
        loaded: launchd.loaded,
        healthy,
        unmanaged_daemon,
        expected_mode,
        active_generation,
        launchd_pid: launchd.pid,
        daemon_status,
        pid_matches,
        generation_matches,
        binary_matches,
        permissions_ok,
        launch_agent_path: paths.launch_agent.clone(),
        daemon_path,
        cli_path,
        database_path: paths.database.clone(),
        socket_path,
        data_preserved: true,
        acceptance,
        errors,
    })
}

fn inspect_acceptance_lease(
    paths: &LocalPaths,
    layout: &ServiceLayout,
) -> Result<Option<ServiceAcceptanceReport>, ServiceError> {
    let Some(transaction) = read_optional_transaction(&layout.transaction)? else {
        return Ok(None);
    };
    let database_backup_present = path_entry_exists(&layout.database_backup)?;
    let rollback_database_available = if transaction.database_backed_up {
        database_backup_present && validate_sqlite_database(&layout.database_backup).is_ok()
    } else {
        !database_backup_present
    };
    let rollback_plist_available = match transaction.prior_plist.as_deref() {
        Some(plist) => {
            validate_report_only_rollback_plist(paths, layout, &transaction, plist).is_ok()
        }
        None => transaction.prior_manifest.is_none() && !transaction.prior_was_loaded,
    };
    let rollback_generation_available = transaction.prior_manifest.as_ref().map_or_else(
        || transaction.prior_plist.is_none() || paths.daemon_binary.is_file(),
        |manifest| validate_generation(&layout.generation(manifest.active_generation)).is_ok(),
    );
    Ok(Some(ServiceAcceptanceReport {
        phase: transaction_phase_name(transaction.phase),
        candidate_generation: transaction.candidate_generation,
        prior_generation: transaction
            .prior_manifest
            .as_ref()
            .map(|manifest| manifest.active_generation),
        rollback_available: transaction.phase != TransactionPhase::Accepted
            && rollback_database_available
            && rollback_plist_available
            && rollback_generation_available,
        database_backup_present,
    }))
}

pub fn launch_agent_plist(paths: &LocalPaths, generation: &GenerationPaths) -> String {
    let program = xml_escape(&generation.daemon.to_string_lossy());
    let activation_generation = generation.activation_generation;
    let error_log = xml_escape(&paths.error_log.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCH_AGENT_LABEL}</string>
  <key>Program</key>
  <string>{program}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{program}</string>
    <string>--managed</string>
    <string>--activation-generation</string>
    <string>{activation_generation}</string>
  </array>
  <key>KeepAlive</key>
  <true/>
  <key>ProcessType</key>
  <string>Background</string>
  <key>LowPriorityIO</key>
  <true/>
  <key>ThrottleInterval</key>
  <integer>{THROTTLE_INTERVAL_SECONDS}</integer>
  <key>ExitTimeOut</key>
  <integer>{EXIT_TIMEOUT_SECONDS}</integer>
  <key>Umask</key>
  <string>077</string>
  <key>StandardErrorPath</key>
  <string>{error_log}</string>
</dict>
</plist>
"#
    )
}

fn mutation_uid() -> Result<u32, ServiceError> {
    let uid = current_uid();
    if uid == 0 {
        Err(ServiceError::new(
            "refusing to install a per-user LaunchAgent as root",
        ))
    } else {
        Ok(uid)
    }
}

fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

fn prepare_install_directories(paths: &LocalPaths) -> Result<(), ServiceError> {
    let layout = ServiceLayout::new(paths);
    for directory in [
        &paths.application_support,
        &paths.binary_directory,
        &layout.generations,
        &paths.cache_directory,
        &paths.runtime_directory,
        &paths.logs_directory,
    ] {
        fs::create_dir_all(directory)
            .map_err(|error| ServiceError::context("could not create private directory", error))?;
        let metadata = fs::symlink_metadata(directory)
            .map_err(|error| ServiceError::context("could not inspect private directory", error))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(ServiceError::new(format!(
                "refusing non-directory private path {}",
                directory.display()
            )));
        }
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|error| {
            ServiceError::context("could not protect private directory permissions", error)
        })?;
    }
    let launch_agents = paths
        .launch_agent
        .parent()
        .ok_or_else(|| ServiceError::new("LaunchAgent path has no parent directory"))?;
    fs::create_dir_all(launch_agents)
        .map_err(|error| ServiceError::context("could not create LaunchAgents directory", error))?;
    Ok(())
}

fn installed_legacy_mode(paths: &LocalPaths) -> Option<DaemonMode> {
    let document = read_optional_launch_agent(&paths.launch_agent).ok()??;
    if document
        .program_arguments
        .iter()
        .any(|argument| argument == "--managed")
    {
        return None;
    }
    legacy_mode_from_document(&document, Some(&paths.daemon_binary)).ok()
}

fn installed_generation_result(paths: &LocalPaths) -> Result<Option<u64>, ServiceError> {
    let Some(document) = read_optional_launch_agent(&paths.launch_agent)? else {
        return Ok(None);
    };
    let Some(generation) = managed_generation_from_document(&document)? else {
        return Ok(None);
    };
    let expected = ServiceLayout::new(paths).generation(generation);
    validate_launch_agent_document(
        &document,
        &expected.daemon,
        LaunchAgentExpectation::Managed(generation),
    )?;
    Ok(Some(generation))
}

fn read_optional_launch_agent(path: &Path) -> Result<Option<LaunchAgentDocument>, ServiceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect installed LaunchAgent",
                error,
            ));
        }
    };
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != current_uid()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(ServiceError::new(format!(
            "installed LaunchAgent must be a current-user 0600 regular file: {}",
            path.display()
        )));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|error| ServiceError::context("could not open installed LaunchAgent", error))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| ServiceError::context("could not read installed LaunchAgent", error))?;
    parse_launch_agent_bytes(&bytes).map(Some)
}

fn parse_launch_agent_bytes(bytes: &[u8]) -> Result<LaunchAgentDocument, ServiceError> {
    let mut child = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ServiceError::context("could not execute plutil", error))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| ServiceError::new("plutil stdin was unavailable"))?;
    stdin
        .write_all(bytes)
        .map_err(|error| ServiceError::context("could not send LaunchAgent to plutil", error))?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|error| ServiceError::context("could not wait for plutil", error))?;
    if !output.status.success() {
        return Err(ServiceError::new(format!(
            "LaunchAgent failed structured plist parsing: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| ServiceError::context("could not decode structured LaunchAgent", error))
}

fn validate_launch_agent_base(document: &LaunchAgentDocument) -> Result<(), ServiceError> {
    if document.label != LAUNCH_AGENT_LABEL {
        return Err(ServiceError::new(format!(
            "LaunchAgent Label must be exactly {LAUNCH_AGENT_LABEL}"
        )));
    }
    if document.program.is_empty() || document.program_arguments.first() != Some(&document.program)
    {
        return Err(ServiceError::new(
            "LaunchAgent Program must be non-empty and equal ProgramArguments[0]",
        ));
    }
    Ok(())
}

fn managed_generation_from_document(
    document: &LaunchAgentDocument,
) -> Result<Option<u64>, ServiceError> {
    validate_launch_agent_base(document)?;
    let has_managed_contract = document
        .program_arguments
        .iter()
        .any(|argument| argument == "--managed" || argument == "--activation-generation");
    if !has_managed_contract {
        return Ok(None);
    }
    if document.program_arguments.len() != 4
        || document.program_arguments[1] != "--managed"
        || document.program_arguments[2] != "--activation-generation"
    {
        return Err(ServiceError::new(
            "managed LaunchAgent ProgramArguments must be exactly Program, --managed, --activation-generation, generation",
        ));
    }
    document.program_arguments[3]
        .parse::<u64>()
        .map(Some)
        .map_err(|error| {
            ServiceError::context("managed LaunchAgent generation is not numeric", error)
        })
}

fn legacy_mode_from_document(
    document: &LaunchAgentDocument,
    expected_program: Option<&Path>,
) -> Result<DaemonMode, ServiceError> {
    validate_launch_agent_base(document)?;
    if document.program_arguments.len() != 2 {
        return Err(ServiceError::new(
            "legacy LaunchAgent ProgramArguments must be exactly Program and one mode argument",
        ));
    }
    if let Some(expected_program) = expected_program {
        let expected_program = expected_program.to_str().ok_or_else(|| {
            ServiceError::new("expected LaunchAgent Program path is not valid UTF-8")
        })?;
        if document.program != expected_program {
            return Err(ServiceError::new(
                "legacy LaunchAgent Program does not match the managed daemon path",
            ));
        }
    }
    match document.program_arguments[1].as_str() {
        "--report-only" => Ok(DaemonMode::ReportOnly),
        "--enforce" => Ok(DaemonMode::Enforce),
        _ => Err(ServiceError::new(
            "legacy LaunchAgent must declare exactly one of --report-only or --enforce",
        )),
    }
}

fn validate_launch_agent_document(
    document: &LaunchAgentDocument,
    expected_program: &Path,
    expectation: LaunchAgentExpectation,
) -> Result<(), ServiceError> {
    validate_launch_agent_base(document)?;
    let expected_program = expected_program
        .to_str()
        .ok_or_else(|| ServiceError::new("expected LaunchAgent Program path is not valid UTF-8"))?;
    if document.program != expected_program {
        return Err(ServiceError::new(
            "LaunchAgent Program does not match the exact selected daemon binary",
        ));
    }
    match expectation {
        LaunchAgentExpectation::Managed(expected_generation) => {
            let actual_generation =
                managed_generation_from_document(document)?.ok_or_else(|| {
                    ServiceError::new("LaunchAgent does not declare the managed lifecycle contract")
                })?;
            if actual_generation != expected_generation {
                return Err(ServiceError::new(
                    "LaunchAgent activation generation does not match the selected generation",
                ));
            }
            Ok(())
        }
        LaunchAgentExpectation::Legacy(expected_mode) => {
            let actual_mode =
                legacy_mode_from_document(document, Some(Path::new(expected_program)))?;
            if actual_mode == expected_mode {
                Ok(())
            } else {
                Err(ServiceError::new(
                    "legacy LaunchAgent mode does not match the required recovery floor",
                ))
            }
        }
    }
}

fn ipc_status_result(path: &Path) -> Result<Option<DaemonStatus>, ServiceError> {
    if !path.exists() {
        return Ok(None);
    }
    match IpcClient::with_io_timeout(path, SERVICE_IPC_IO_TIMEOUT).request(IpcCommand::Status) {
        Ok(IpcPayload::Status(status)) => Ok(Some(status)),
        Ok(_) => Err(ServiceError::new(
            "daemon returned the wrong IPC payload for service status",
        )),
        Err(error) => Err(ServiceError::context("daemon IPC status failed", error)),
    }
}

fn ipc_status_for_launchd(
    paths: &LocalPaths,
    expected_pid: Option<u32>,
) -> Result<Option<DaemonStatus>, ServiceError> {
    let legacy_socket = paths.cache_directory.join("unlingerd.sock");
    let mut first_status = None;
    let mut errors = Vec::new();
    for socket in [&paths.socket, &legacy_socket] {
        match ipc_status_result(socket) {
            Ok(Some(status)) if Some(status.pid) == expected_pid => return Ok(Some(status)),
            Ok(Some(status)) => {
                if first_status.is_none() {
                    first_status = Some(status);
                }
            }
            Ok(None) => {}
            Err(error) => errors.push(error.to_string()),
        }
    }
    if expected_pid.is_none() && first_status.is_some() {
        return Ok(first_status);
    }
    if let Some(status) = first_status {
        return Err(ServiceError::new(format!(
            "daemon IPC PID {} does not match launchd PID {:?}",
            status.pid, expected_pid
        )));
    }
    if errors.is_empty() {
        Ok(None)
    } else {
        Err(ServiceError::new(errors.join("; ")))
    }
}

fn launchd_state(uid: u32) -> Result<LaunchdState, ServiceError> {
    let target = service_target(uid);
    let output = Command::new("/bin/launchctl")
        .arg("print")
        .arg(&target)
        .output()
        .map_err(|error| ServiceError::context("could not execute launchctl print", error))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        return Ok(LaunchdState {
            loaded: true,
            pid: parse_launchd_pid(&stdout),
        });
    }
    let combined = format!("{stdout}\n{stderr}");
    if combined.contains("Could not find service") {
        Ok(LaunchdState {
            loaded: false,
            pid: None,
        })
    } else {
        Err(ServiceError::new(format!(
            "launchctl print failed for {target}: {}",
            combined.trim()
        )))
    }
}

fn parse_launchd_pid(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("pid = ")
            .and_then(|value| value.parse::<u32>().ok())
    })
}

fn capture_loaded_identity(state: LaunchdState) -> Result<Option<ProcessIdentity>, ServiceError> {
    let Some(pid) = state.pid else {
        return Ok(None);
    };
    MacosSnapshotter::new()
        .lookup(pid)
        .map(|process| process.map(|process| process.identity))
        .map_err(|error| ServiceError::context("could not capture loaded daemon identity", error))
}

fn bootstrap(uid: u32, plist: &Path) -> Result<(), ServiceError> {
    let target = service_target(uid);
    run_launchctl(&["enable", &target])?;
    let domain = service_domain(uid);
    let plist = plist.to_string_lossy().into_owned();
    run_launchctl(&["bootstrap", &domain, &plist])
}

fn bootout_and_wait(uid: u32, identity: Option<&ProcessIdentity>) -> Result<(), ServiceError> {
    let target = service_target(uid);
    run_launchctl(&["bootout", &target])?;
    let started = Instant::now();
    loop {
        let unloaded = !launchd_state(uid)?.loaded;
        let identity_gone = match identity {
            Some(identity) => match MacosSnapshotter::new().lookup(identity.pid) {
                Ok(Some(process)) => !identity.exact_match(&process.identity),
                Ok(None) => true,
                Err(_) => false,
            },
            None => true,
        };
        if unloaded && identity_gone {
            return Ok(());
        }
        if started.elapsed() >= SERVICE_STOP_TIMEOUT {
            return Err(ServiceError::new(format!(
                "LaunchAgent did not stop within {} seconds",
                SERVICE_STOP_TIMEOUT.as_secs()
            )));
        }
        thread::sleep(SERVICE_POLL_INTERVAL);
    }
}

fn set_mode_locked(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    mode: DaemonMode,
) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    let mut manifest = read_optional_manifest(&layout.active_manifest)?.ok_or_else(|| {
        ServiceError::new("managed service metadata is unavailable; reinstall before changing mode")
    })?;
    let launchd = launchd_state(uid)?;
    let expected_pid = match set_mode_route(mode, launchd) {
        SetModeRoute::ReportOnlyContainment => {
            recover_then_publish_report_only(
                &mut manifest,
                |generation| restore_generation_report_only(paths, layout, uid, generation),
                |ready_manifest| write_json_atomic(&layout.active_manifest, ready_manifest, 0o600),
            )?;
            return wait_for_mode(paths, uid, DaemonMode::ReportOnly, SERVICE_START_TIMEOUT);
        }
        SetModeRoute::EnforceOnline(pid) => pid,
        SetModeRoute::RejectEnforce => {
            return Err(ServiceError::new(
                "managed LaunchAgent has no running daemon; enforcement was not requested",
            ));
        }
    };
    let mut current = ipc_status_for_launchd(paths, Some(expected_pid))?.ok_or_else(|| {
        ServiceError::new("managed daemon IPC is unavailable; mode was not changed")
    })?;
    validate_managed_identity(&current, manifest.active_generation)?;

    if manifest.desired_mode == DaemonMode::ReportOnly
        && current.effective_mode() == DaemonMode::Enforce
    {
        let instance_id = current.instance_id.clone();
        if let Err(disarm_error) = request_disarm(paths, &current).and_then(|report_only| {
            validate_report_only_response(&report_only, manifest.active_generation, &instance_id)
        }) {
            restore_generation_report_only(
                paths,
                layout,
                uid,
                manifest.active_generation,
            )
            .map_err(|restore_error| {
                ServiceError::new(format!(
                    "unexpected enforcement could not be disarmed: {disarm_error}; fail-closed report-only recovery failed: {restore_error}"
                ))
            })?;
        }
    }

    if current.effective_mode() == mode
        && manifest.desired_mode == mode
        && current.ready
        && !current.draining
    {
        let report = status(paths)?;
        validate_ready_enforce_report(&report, manifest.active_generation)?;
        return Ok(report);
    }

    current = arm_preflight(paths, layout, manifest.active_generation)?;

    // The durable desired-mode write is the arming linearization point. A
    // crash before the IPC request leaves the daemon report-only. A crash
    // after a successful request leaves a committed enforce intent.
    manifest.desired_mode = DaemonMode::Enforce;
    write_json_atomic(&layout.active_manifest, &manifest, 0o600)?;
    match request_arm(paths, &current).and_then(|response| {
        validate_enforce_response(&response, manifest.active_generation, &current.instance_id)?;
        Ok(response)
    }) {
        Ok(_) => {}
        Err(error) => {
            return match restore_report_only_after_arm_failure(paths, layout, uid, &mut manifest) {
                Ok(()) => Err(ServiceError::new(format!(
                    "arming failed and the service was restored report-only: {error}"
                ))),
                Err(restore_error) => Err(ServiceError::new(format!(
                    "arming failed: {error}; fail-closed report-only recovery failed: {restore_error}"
                ))),
            };
        }
    }

    wait_for_mode(paths, uid, DaemonMode::Enforce, SERVICE_START_TIMEOUT)
}

fn recover_then_publish_report_only(
    manifest: &mut ActiveServiceManifest,
    recover: impl FnOnce(u64) -> Result<(), ServiceError>,
    publish: impl FnOnce(&ActiveServiceManifest) -> Result<(), ServiceError>,
) -> Result<(), ServiceError> {
    recover(manifest.active_generation)?;
    manifest.desired_mode = DaemonMode::ReportOnly;
    publish(manifest)
}

fn restore_report_only_after_arm_failure(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    uid: u32,
    manifest: &mut ActiveServiceManifest,
) -> Result<(), ServiceError> {
    // Publish report-only intent only after the runtime has proved it. If this
    // process dies earlier, the prior durable enforce intent remains truthful and
    // the next service mutation can retry the exact-instance fail-close.
    recover_then_publish_report_only(
        manifest,
        |generation| restore_generation_report_only(paths, layout, uid, generation),
        |ready_manifest| write_json_atomic(&layout.active_manifest, ready_manifest, 0o600),
    )
}

fn restore_generation_report_only(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    uid: u32,
    generation: u64,
) -> Result<(), ServiceError> {
    let launchd = launchd_state(uid)?;
    if !launchd.loaded {
        return emergency_restart_generation_report_only(
            paths,
            layout,
            uid,
            generation,
            "managed LaunchAgent disappeared during fail-closed recovery",
        );
    }
    if report_only_recovery_route(launchd) == ReportOnlyRecoveryRoute::Offline {
        return emergency_restart_generation_report_only(
            paths,
            layout,
            uid,
            generation,
            "managed LaunchAgent is loaded without a running daemon",
        );
    }
    match ipc_status_for_launchd(paths, launchd.pid) {
        Ok(Some(status)) => {
            if let Err(identity_error) = validate_managed_identity(&status, generation) {
                return emergency_restart_generation_report_only(
                    paths,
                    layout,
                    uid,
                    generation,
                    &format!("daemon IPC lifecycle identity is ambiguous: {identity_error}"),
                );
            }
            let instance_id = status.instance_id.clone();
            let disarm = request_disarm(paths, &status).and_then(|report_only| {
                validate_disarmed_response(&report_only, generation, &instance_id)
            });
            match disarm {
                Ok(()) => wait_for_generation_runtime_report_only(
                    paths,
                    uid,
                    generation,
                    SERVICE_START_TIMEOUT,
                )
                .map(|_| ())
                .or_else(|wait_error| {
                    confirm_or_restart_generation_report_only(
                        paths,
                        layout,
                        uid,
                        generation,
                        &format!("exact Disarm did not reach ready report-only: {wait_error}"),
                    )
                }),
                Err(disarm_error) => confirm_or_restart_generation_report_only(
                    paths,
                    layout,
                    uid,
                    generation,
                    &format!("exact Disarm confirmation failed: {disarm_error}"),
                ),
            }
        }
        Ok(None) => emergency_restart_generation_report_only(
            paths,
            layout,
            uid,
            generation,
            "daemon IPC status is unavailable",
        ),
        Err(ipc_error) => emergency_restart_generation_report_only(
            paths,
            layout,
            uid,
            generation,
            &format!("daemon IPC status is ambiguous: {ipc_error}"),
        ),
    }
}

fn confirm_or_restart_generation_report_only(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    uid: u32,
    generation: u64,
    reason: &str,
) -> Result<(), ServiceError> {
    match status(paths) {
        Ok(report) if generation_runtime_is_ready_report_only(&report, generation) => Ok(()),
        Ok(_) | Err(_) => {
            emergency_restart_generation_report_only(paths, layout, uid, generation, reason)
        }
    }
}

fn arm_preflight(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    generation: u64,
) -> Result<DaemonStatus, ServiceError> {
    let report = status(paths)?;
    let generation_paths = layout.generation(generation);
    validate_generation(&generation_paths)?;
    if !report.installed
        || !report.loaded
        || report.active_generation != Some(generation)
        || report.launchd_pid.is_none()
        || !report.pid_matches
        || !report.generation_matches
        || !report.binary_matches
        || !report.permissions_ok
    {
        return Err(ServiceError::new(format!(
            "managed generation failed exact pre-arm identity checks: {}",
            report.errors.join("; ")
        )));
    }
    let daemon = report.daemon_status.ok_or_else(|| {
        ServiceError::new("managed generation has no daemon status for pre-arm validation")
    })?;
    validate_report_only_response(&daemon, generation, &daemon.instance_id)?;
    if !daemon.healthy || report.launchd_pid != Some(daemon.pid) {
        return Err(ServiceError::new(
            "managed daemon is not healthy under its exact launchd identity",
        ));
    }
    Ok(daemon)
}

fn validate_ready_enforce_report(
    report: &ServiceStatusReport,
    generation: u64,
) -> Result<(), ServiceError> {
    if !report.healthy
        || !report.installed
        || !report.loaded
        || report.expected_mode != Some(DaemonMode::Enforce)
        || report.active_generation != Some(generation)
        || report.launchd_pid.is_none()
        || !report.pid_matches
        || !report.generation_matches
        || !report.binary_matches
        || !report.permissions_ok
    {
        return Err(ServiceError::new(format!(
            "managed generation failed exact enforce identity checks: {}",
            report.errors.join("; ")
        )));
    }
    let daemon = report.daemon_status.as_ref().ok_or_else(|| {
        ServiceError::new("managed generation has no daemon status for enforce validation")
    })?;
    validate_enforce_response(daemon, generation, &daemon.instance_id)?;
    if !daemon.healthy
        || daemon.last_scan_at_unix_millis.is_none()
        || report.launchd_pid != Some(daemon.pid)
    {
        return Err(ServiceError::new(
            "managed daemon is not healthy under its exact enforced launchd identity",
        ));
    }
    Ok(())
}

fn emergency_restart_generation_report_only(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    uid: u32,
    generation: u64,
    reason: &str,
) -> Result<(), ServiceError> {
    validate_exact_generation_selection(paths, layout, generation)
        .map_err(|error| ServiceError::new(format!("{reason}; {error}")))?;
    let launchd = launchd_state(uid)?;
    match report_only_recovery_route(launchd) {
        ReportOnlyRecoveryRoute::Running(_) => {
            let (_launchd, identity) =
                capture_exact_generation_process(paths, layout, uid, generation)
                    .map_err(|error| ServiceError::new(format!("{reason}; {error}")))?;
            bootout_and_wait(uid, Some(&identity))?;
        }
        ReportOnlyRecoveryRoute::Offline if launchd.loaded => {
            // launchd may be between KeepAlive attempts and expose no PID. Unload
            // the exact selected job, then use the daemon's lifetime lock below
            // to prove that any concurrently spawned instance has fully exited.
            bootout_and_wait(uid, None)?;
        }
        ReportOnlyRecoveryRoute::Offline => {}
    }

    let daemon_lock = prove_daemon_offline(paths, SERVICE_STOP_TIMEOUT)
        .map_err(|error| ServiceError::new(format!("{reason}; {error}")))?;
    validate_exact_generation_selection(paths, layout, generation)?;
    clear_generation_enforce_request_offline(paths, layout, generation, &daemon_lock)?;
    drop(daemon_lock);
    bootstrap(uid, &paths.launch_agent)?;
    wait_for_generation_runtime_report_only(paths, uid, generation, SERVICE_START_TIMEOUT)?;
    Ok(())
}

fn prove_daemon_offline(
    paths: &LocalPaths,
    timeout: Duration,
) -> Result<DaemonInstanceLock, ServiceError> {
    let daemon_lock = wait_for_offline_daemon_lock(&paths.daemon_lock, timeout)?;
    for socket in [
        paths.socket.as_path(),
        paths.cache_directory.join("unlingerd.sock").as_path(),
    ] {
        prove_listener_absent_or_remove_stale(socket)?;
    }
    Ok(daemon_lock)
}

fn prove_listener_absent_or_remove_stale(path: &Path) -> Result<(), ServiceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect offline IPC socket",
                error,
            ));
        }
    };
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::new("offline IPC socket has no parent directory"))?;
    validate_private_socket_parent(parent)?;
    validate_private_socket(path, &metadata)?;
    match UnixStream::connect(path) {
        Ok(_) => {
            return Err(ServiceError::new(format!(
                "daemon IPC listener remains reachable at {}",
                path.display()
            )));
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) => {}
        Err(error) => {
            return Err(ServiceError::context(
                "could not prove offline IPC socket is stale",
                error,
            ));
        }
    }

    let current = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ServiceError::context(
                "could not revalidate stale IPC socket",
                error,
            ));
        }
    };
    validate_private_socket(path, &current)?;
    if current.dev() != metadata.dev() || current.ino() != metadata.ino() {
        return Err(ServiceError::new(
            "offline IPC socket identity changed during stale-socket validation",
        ));
    }
    fs::remove_file(path)
        .map_err(|error| ServiceError::context("could not remove stale IPC socket", error))?;
    sync_directory(parent)
}

fn validate_private_socket_parent(path: &Path) -> Result<(), ServiceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ServiceError::context("could not inspect IPC socket parent", error))?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != current_uid()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(ServiceError::new(format!(
            "IPC socket parent must be a current-user 0700 directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_private_socket(path: &Path, metadata: &fs::Metadata) -> Result<(), ServiceError> {
    if !metadata.file_type().is_socket()
        || metadata.file_type().is_symlink()
        || metadata.uid() != current_uid()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(ServiceError::new(format!(
            "offline IPC path must be a current-user 0600 Unix socket: {}",
            path.display()
        )));
    }
    Ok(())
}

fn wait_for_offline_daemon_lock(
    path: &Path,
    timeout: Duration,
) -> Result<DaemonInstanceLock, ServiceError> {
    let started = Instant::now();
    loop {
        match DaemonInstanceLock::acquire(path) {
            Ok(lock) => return Ok(lock),
            Err(DaemonLockError::AlreadyHeld { .. }) if started.elapsed() < timeout => {
                thread::sleep(SERVICE_POLL_INTERVAL);
            }
            Err(error) => {
                return Err(ServiceError::context(
                    "could not prove the managed daemon is offline",
                    error,
                ));
            }
        }
    }
}

fn clear_generation_enforce_request_offline(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    generation: u64,
    _daemon_lock: &DaemonInstanceLock,
) -> Result<(), ServiceError> {
    validate_managed_database_path(&paths.database, layout)?;
    let now_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ServiceError::context("wall clock failed", error))?
        .as_millis();
    let now_unix_millis = u64::try_from(now_unix_millis)
        .map_err(|_| ServiceError::new("wall clock overflowed u64"))?;
    let store = HistoryStore::open(&paths.database)
        .map_err(|error| ServiceError::context("could not open managed history offline", error))?;
    store
        .clear_managed_enforce_request_offline(generation, now_unix_millis)
        .map_err(|error| {
            ServiceError::context(
                "could not clear exact-generation enforcement intent offline",
                error,
            )
        })?;
    Ok(())
}

fn capture_exact_generation_process(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    uid: u32,
    generation: u64,
) -> Result<(LaunchdState, ProcessIdentity), ServiceError> {
    validate_exact_generation_selection(paths, layout, generation)?;
    let launchd = launchd_state(uid)?;
    let pid = launchd.pid.ok_or_else(|| {
        ServiceError::new("loaded LaunchAgent has no exact process identity to validate")
    })?;
    let identity = capture_loaded_identity(launchd)?
        .ok_or_else(|| ServiceError::new("could not capture the loaded daemon identity"))?;
    let generation_paths = layout.generation(generation);
    let binary = fs::metadata(&generation_paths.daemon).map_err(|error| {
        ServiceError::context("could not inspect active generation daemon", error)
    })?;
    if identity.pid != pid
        || identity.executable_device != Some(binary.dev())
        || identity.executable_inode != Some(binary.ino())
    {
        return Err(ServiceError::new(
            "refusing to mutate a process that is not the exact active generation binary",
        ));
    }
    Ok((launchd, identity))
}

fn validate_exact_generation_selection(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    generation: u64,
) -> Result<(), ServiceError> {
    let active = read_optional_manifest(&layout.active_manifest)?.ok_or_else(|| {
        ServiceError::new("active manifest disappeared during exact-generation validation")
    })?;
    if active.active_generation != generation {
        return Err(ServiceError::new(
            "active manifest no longer selects the expected generation",
        ));
    }
    if installed_generation_result(paths)? != Some(generation) {
        return Err(ServiceError::new(
            "LaunchAgent no longer selects the expected generation",
        ));
    }
    let generation_paths = layout.generation(generation);
    validate_generation(&generation_paths)?;
    Ok(())
}

fn request_arm(paths: &LocalPaths, status: &DaemonStatus) -> Result<DaemonStatus, ServiceError> {
    request_lifecycle(
        paths,
        IpcCommand::Arm {
            activation_generation: required_generation(status)?,
            instance_id: status.instance_id.clone(),
        },
    )
}

fn request_disarm(paths: &LocalPaths, status: &DaemonStatus) -> Result<DaemonStatus, ServiceError> {
    request_lifecycle(
        paths,
        IpcCommand::Disarm {
            activation_generation: required_generation(status)?,
            instance_id: status.instance_id.clone(),
        },
    )
}

fn request_drain(paths: &LocalPaths, status: &DaemonStatus) -> Result<DaemonStatus, ServiceError> {
    request_lifecycle(
        paths,
        IpcCommand::BeginDrain {
            activation_generation: required_generation(status)?,
            instance_id: status.instance_id.clone(),
        },
    )
}

fn request_lifecycle(
    paths: &LocalPaths,
    command: IpcCommand,
) -> Result<DaemonStatus, ServiceError> {
    // Lifecycle commands are never automatically resent: a timed-out Arm or
    // Disarm has an uncertain delivery outcome and must enter the existing
    // fail-closed recovery path. The longer single-request bound covers a
    // legitimate status/store projection queued ahead of this connection.
    match IpcClient::with_io_timeout(&paths.socket, SERVICE_IPC_IO_TIMEOUT).request(command) {
        Ok(IpcPayload::Lifecycle(status)) => Ok(status),
        Ok(_) => Err(ServiceError::new(
            "daemon returned the wrong lifecycle IPC payload",
        )),
        Err(error) => Err(ServiceError::context("daemon lifecycle IPC failed", error)),
    }
}

fn required_generation(status: &DaemonStatus) -> Result<u64, ServiceError> {
    status
        .activation_generation
        .ok_or_else(|| ServiceError::new("managed daemon status omits its activation generation"))
}

fn validate_managed_identity(status: &DaemonStatus, generation: u64) -> Result<(), ServiceError> {
    if status.managed
        && status.activation_generation == Some(generation)
        && !status.instance_id.is_empty()
    {
        Ok(())
    } else {
        Err(ServiceError::new(
            "daemon lifecycle identity does not match the active generation",
        ))
    }
}

fn validate_report_only_response(
    status: &DaemonStatus,
    generation: u64,
    instance_id: &str,
) -> Result<(), ServiceError> {
    validate_disarmed_response(status, generation, instance_id)?;
    if status.ready && !status.draining && status.startup_state == StartupState::ReadyReportOnly {
        Ok(())
    } else {
        Err(ServiceError::new(
            "daemon did not confirm a ready report-only lifecycle state",
        ))
    }
}

fn validate_disarmed_response(
    status: &DaemonStatus,
    generation: u64,
    instance_id: &str,
) -> Result<(), ServiceError> {
    validate_managed_identity(status, generation)?;
    if status.instance_id == instance_id
        && status.requested_mode == DaemonMode::ReportOnly
        && status.effective_mode() == DaemonMode::ReportOnly
        && status.armed_generation.is_none()
        && status.enforcement_epoch.is_none()
    {
        Ok(())
    } else {
        Err(ServiceError::new(
            "daemon did not confirm an exact disarmed lifecycle state",
        ))
    }
}

fn validate_quiesce_disarm_response(
    status: &DaemonStatus,
    generation: u64,
    instance_id: &str,
) -> Result<(), ServiceError> {
    if status.startup_state != StartupState::Failed {
        return validate_report_only_response(status, generation, instance_id);
    }
    validate_disarmed_response(status, generation, instance_id)?;
    if !status.healthy && !status.ready && !status.draining {
        Ok(())
    } else {
        Err(ServiceError::new(
            "failed daemon did not confirm a terminal disarmed lifecycle state",
        ))
    }
}

fn validate_enforce_response(
    status: &DaemonStatus,
    generation: u64,
    instance_id: &str,
) -> Result<(), ServiceError> {
    validate_managed_identity(status, generation)?;
    if status.instance_id == instance_id
        && status.ready
        && status.requested_mode == DaemonMode::Enforce
        && status.effective_mode() == DaemonMode::Enforce
        && status.armed_generation == Some(generation)
        && status
            .enforcement_epoch
            .as_ref()
            .is_some_and(|epoch| !epoch.is_empty())
        && !status.draining
        && status.startup_state == StartupState::ReadyEnforce
    {
        Ok(())
    } else {
        Err(ServiceError::new(
            "daemon did not confirm generation-bound enforcement",
        ))
    }
}

fn quiesce_loaded_service(
    paths: &LocalPaths,
    uid: u32,
    launchd: LaunchdState,
    status: Option<&DaemonStatus>,
) -> Result<(), ServiceError> {
    let identity = capture_loaded_identity(launchd)?;
    let status = status.ok_or_else(|| {
        ServiceError::new("loaded daemon IPC is unavailable; refusing an uncoordinated bootout")
    })?;
    if status.managed {
        let instance_id = status.instance_id.clone();
        let report_only = request_disarm(paths, status)?;
        validate_quiesce_disarm_response(&report_only, required_generation(status)?, &instance_id)?;
        let instance_id = report_only.instance_id.clone();
        let draining = request_drain(paths, &report_only)?;
        validate_managed_identity(&draining, required_generation(status)?)?;
        if draining.instance_id != instance_id
            || !draining.draining
            || draining.startup_state != StartupState::Draining
            || draining.effective_mode() != DaemonMode::ReportOnly
        {
            return Err(ServiceError::new(
                "daemon did not acknowledge report-only drain",
            ));
        }
    } else {
        wait_for_legacy_quiescent(paths, status.pid, SERVICE_STOP_TIMEOUT)?;
    }
    bootout_and_wait(uid, identity.as_ref())
}

fn wait_for_legacy_quiescent(
    paths: &LocalPaths,
    pid: u32,
    timeout: Duration,
) -> Result<(), ServiceError> {
    let started = Instant::now();
    loop {
        let status = ipc_status_for_launchd(paths, Some(pid))?.ok_or_else(|| {
            ServiceError::new("legacy daemon IPC disappeared before coordinated bootout")
        })?;
        if !status.managed
            && status.effective_mode() == DaemonMode::ReportOnly
            && !status.cleanup_in_progress
            && !status.scan_in_progress
        {
            return Ok(());
        }
        if status.managed || status.effective_mode() != DaemonMode::ReportOnly {
            return Err(ServiceError::new(
                "legacy daemon is not report-only; refusing bootout",
            ));
        }
        if started.elapsed() >= timeout {
            return Err(ServiceError::new(
                "legacy report-only daemon did not become quiescent before bootout",
            ));
        }
        thread::sleep(SERVICE_POLL_INTERVAL);
    }
}

fn run_launchctl(arguments: &[&str]) -> Result<(), ServiceError> {
    let output = Command::new("/bin/launchctl")
        .args(arguments)
        .output()
        .map_err(|error| ServiceError::context("could not execute launchctl", error))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(ServiceError::new(format!(
        "launchctl {} failed: {} {}",
        arguments.join(" "),
        stdout.trim(),
        stderr.trim()
    )))
}

fn wait_for_mode(
    paths: &LocalPaths,
    uid: u32,
    mode: DaemonMode,
    timeout: Duration,
) -> Result<ServiceStatusReport, ServiceError> {
    let started = Instant::now();
    loop {
        let report = status(paths)?;
        if report.healthy
            && report.expected_mode == Some(mode)
            && report.launchd_pid == report.daemon_status.as_ref().map(|status| status.pid)
        {
            return Ok(report);
        }
        if started.elapsed() >= timeout {
            let errors = Some(report.errors.join("; "))
                .filter(|errors| !errors.is_empty())
                .unwrap_or_else(|| "no healthy service report was produced".to_owned());
            return Err(ServiceError::new(format!(
                "LaunchAgent did not become healthy in {}: {errors}",
                service_domain(uid)
            )));
        }
        thread::sleep(SERVICE_POLL_INTERVAL);
    }
}

fn wait_for_generation_ready(
    paths: &LocalPaths,
    uid: u32,
    generation: u64,
    timeout: Duration,
) -> Result<ServiceStatusReport, ServiceError> {
    let started = Instant::now();
    loop {
        let report = status(paths)?;
        let ready = report.healthy
            && report.expected_mode == Some(DaemonMode::ReportOnly)
            && generation_runtime_is_ready_report_only(&report, generation);
        if ready {
            return Ok(report);
        }
        if started.elapsed() >= timeout {
            let errors = Some(report.errors.join("; "))
                .filter(|errors| !errors.is_empty())
                .unwrap_or_else(|| "candidate never reached ready report-only".to_owned());
            return Err(ServiceError::new(format!(
                "generation {generation} did not become ready in {}: {errors}",
                service_domain(uid)
            )));
        }
        thread::sleep(SERVICE_POLL_INTERVAL);
    }
}

fn wait_for_generation_runtime_report_only(
    paths: &LocalPaths,
    uid: u32,
    generation: u64,
    timeout: Duration,
) -> Result<ServiceStatusReport, ServiceError> {
    let started = Instant::now();
    loop {
        let report = status(paths)?;
        if generation_runtime_is_ready_report_only(&report, generation) {
            return Ok(report);
        }
        if generation_runtime_is_failed(&report, generation) {
            return Err(ServiceError::new(format!(
                "generation {generation} entered a failed managed lifecycle and requires exact report-only restart recovery"
            )));
        }
        if started.elapsed() >= timeout {
            let errors = Some(report.errors.join("; "))
                .filter(|errors| !errors.is_empty())
                .unwrap_or_else(|| "generation never reached exact runtime report-only".to_owned());
            return Err(ServiceError::new(format!(
                "generation {generation} did not recover report-only in {}: {errors}",
                service_domain(uid)
            )));
        }
        thread::sleep(SERVICE_POLL_INTERVAL);
    }
}

fn generation_runtime_is_ready_report_only(report: &ServiceStatusReport, generation: u64) -> bool {
    report.installed
        && report.loaded
        && report.active_generation == Some(generation)
        && report.pid_matches
        && report.generation_matches
        && report.binary_matches
        && report.permissions_ok
        && report.daemon_status.as_ref().is_some_and(|status| {
            status.managed
                && status.activation_generation == Some(generation)
                && report.launchd_pid == Some(status.pid)
                && status.healthy
                && status.ready
                && status.startup_state == StartupState::ReadyReportOnly
                && status.requested_mode == DaemonMode::ReportOnly
                && status.effective_mode() == DaemonMode::ReportOnly
                && status.armed_generation.is_none()
                && status.enforcement_epoch.is_none()
                && !status.draining
                && !status.scan_in_progress
                && !status.cleanup_in_progress
                && !status.instance_id.is_empty()
        })
}

fn generation_runtime_is_failed(report: &ServiceStatusReport, generation: u64) -> bool {
    report.installed
        && report.loaded
        && report.active_generation == Some(generation)
        && report.pid_matches
        && report.generation_matches
        && report.binary_matches
        && report.permissions_ok
        && report.daemon_status.as_ref().is_some_and(|status| {
            status.managed
                && status.activation_generation == Some(generation)
                && report.launchd_pid == Some(status.pid)
                && status.startup_state == StartupState::Failed
                && !status.instance_id.is_empty()
        })
}

fn recover_incomplete_install(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    uid: u32,
) -> Result<(), ServiceError> {
    let Some(transaction) = read_optional_transaction(&layout.transaction)? else {
        remove_file_durable(&layout.database_backup)?;
        remove_file_durable(&layout.database_backup_pending)?;
        return Ok(());
    };
    match transaction_recovery_disposition(transaction.phase) {
        TransactionRecoveryDisposition::HoldForExplicitDecision => Err(ServiceError::new(format!(
            "generation {} is ready report-only with rollback retained; use service accept-candidate or service rollback-candidate",
            transaction.candidate_generation
        ))),
        TransactionRecoveryDisposition::FinalizeAccepted => {
            finalize_accepted_lease(paths, layout, &transaction)
        }
        TransactionRecoveryDisposition::RollbackPrior => {
            rollback_install_transaction(paths, layout, uid, transaction)
        }
    }
}

fn rollback_install_transaction(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    uid: u32,
    transaction: InstallTransaction,
) -> Result<(), ServiceError> {
    let launchd = launchd_state(uid)?;
    if launchd.loaded {
        match (launchd.pid, transaction.phase) {
            (
                Some(pid),
                TransactionPhase::CandidateSelected
                | TransactionPhase::CandidateReadyReportOnly
                | TransactionPhase::AcceptanceInProgress,
            ) => {
                let daemon_status = ipc_status_for_launchd(paths, Some(pid));
                if let Ok(Some(status)) = daemon_status {
                    validate_managed_identity(&status, transaction.candidate_generation)?;
                    let (exact_launchd, identity) = capture_exact_generation_process(
                        paths,
                        layout,
                        uid,
                        transaction.candidate_generation,
                    )?;
                    if status.pid != identity.pid {
                        return Err(ServiceError::new(
                            "candidate IPC and exact launchd process identities do not match",
                        ));
                    }
                    quiesce_loaded_service(paths, uid, exact_launchd, Some(&status))?;
                } else {
                    // A selected candidate can only have been launched by a managed
                    // plist, whose boot floor is report-only. Re-prove the manifest,
                    // plist, immutable binary, and exact process identity before the
                    // only recovery bootout that can proceed without lifecycle IPC.
                    let (_launchd, identity) = capture_exact_generation_process(
                        paths,
                        layout,
                        uid,
                        transaction.candidate_generation,
                    )?;
                    bootout_and_wait(uid, Some(&identity))?;
                }
            }
            (Some(pid), _) => {
                let status = ipc_status_for_launchd(paths, Some(pid))?.ok_or_else(|| {
                    ServiceError::new(
                        "cannot recover service transaction while prior daemon IPC is unavailable",
                    )
                })?;
                quiesce_loaded_service(paths, uid, launchd, Some(&status))?;
            }
            (
                None,
                TransactionPhase::CandidateSelected
                | TransactionPhase::CandidateReadyReportOnly
                | TransactionPhase::AcceptanceInProgress,
            ) => {
                validate_exact_generation_selection(
                    paths,
                    layout,
                    transaction.candidate_generation,
                )?;
                bootout_and_wait(uid, None)?;
            }
            (None, _) => bootout_and_wait(uid, None)?,
        }
    } else if candidate_database_may_have_changed(transaction.phase) {
        validate_exact_generation_selection(paths, layout, transaction.candidate_generation)?;
    }
    let mut daemon_lock = Some(prove_daemon_offline(paths, SERVICE_STOP_TIMEOUT)?);

    // Resolve and structurally validate the exact rollback target before any
    // database or LaunchAgent mutation. The daemon lifetime lock remains held
    // across this validation and the subsequent offline restore transaction.
    let report_only_prior_plist = transaction
        .prior_plist
        .as_deref()
        .map(|prior_plist| {
            validate_report_only_rollback_plist(paths, layout, &transaction, prior_plist)
        })
        .transpose()?;

    remove_file_durable(&layout.database_backup_pending)?;

    if candidate_database_may_have_changed(transaction.phase) {
        if transaction.database_backed_up {
            restore_database_backup_files(paths, layout, transaction.candidate_generation)?;
        } else {
            if path_entry_exists(&layout.database_backup)? {
                return Err(ServiceError::new(
                    "candidate may have touched history but its transaction did not commit the existing database backup",
                ));
            }
            preserve_failed_database(paths, layout, transaction.candidate_generation)?;
        }
    } else {
        remove_file_durable(&layout.database_backup)?;
    }

    match report_only_prior_plist {
        Some(report_only_plist) => {
            write_bytes_atomic(&paths.launch_agent, report_only_plist.as_bytes(), 0o600)?;
            match transaction.prior_manifest.as_ref() {
                Some(prior) => {
                    let rollback = prior.rollback_floor();
                    write_json_atomic(&layout.active_manifest, &rollback, 0o600)?;
                }
                None => remove_file_durable(&layout.active_manifest)?,
            }
            if transaction.prior_was_loaded {
                drop(daemon_lock.take());
                bootstrap(uid, &paths.launch_agent)?;
                wait_for_restored_report_only(
                    paths,
                    uid,
                    transaction.prior_manifest.as_ref(),
                    SERVICE_START_TIMEOUT,
                )?;
            }
        }
        None => {
            remove_file_durable(&paths.launch_agent)?;
            remove_file_durable(&layout.active_manifest)?;
        }
    }
    remove_file_durable(&layout.transaction)?;
    remove_file_durable(&layout.database_backup)?;
    remove_file_durable(&layout.database_backup_pending)?;
    drop(daemon_lock.take());
    Ok(())
}

fn wait_for_restored_report_only(
    paths: &LocalPaths,
    uid: u32,
    manifest: Option<&ActiveServiceManifest>,
    timeout: Duration,
) -> Result<ServiceStatusReport, ServiceError> {
    if let Some(manifest) = manifest {
        return wait_for_generation_ready(paths, uid, manifest.active_generation, timeout);
    }
    let started = Instant::now();
    loop {
        let launchd = launchd_state(uid)?;
        let daemon = ipc_status_for_launchd(paths, launchd.pid)?;
        if launchd.loaded
            && daemon.as_ref().is_some_and(|status| {
                !status.managed
                    && status.effective_mode() == DaemonMode::ReportOnly
                    && status.healthy
                    && status.last_scan_at_unix_millis.is_some()
                    && !status.cleanup_in_progress
            })
        {
            return status(paths);
        }
        if started.elapsed() >= timeout {
            return Err(ServiceError::new(format!(
                "prior report-only service did not recover in {}",
                service_domain(uid)
            )));
        }
        thread::sleep(SERVICE_POLL_INTERVAL);
    }
}

fn report_only_rollback_plist(contents: &str) -> Result<String, ServiceError> {
    let document = parse_launch_agent_bytes(contents.as_bytes())?;
    if managed_generation_from_document(&document)?.is_some() {
        return Ok(contents.to_owned());
    }
    match legacy_mode_from_document(&document, None)? {
        DaemonMode::ReportOnly => Ok(contents.to_owned()),
        DaemonMode::Enforce => Ok(contents.replace(
            "<string>--enforce</string>",
            "<string>--report-only</string>",
        )),
    }
}

fn validate_report_only_rollback_plist(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    transaction: &InstallTransaction,
    prior_plist: &str,
) -> Result<String, ServiceError> {
    let report_only_plist = report_only_rollback_plist(prior_plist)?;
    let (expected_program, expectation) = match transaction.prior_manifest.as_ref() {
        Some(prior) => (
            layout.generation(prior.active_generation).daemon,
            LaunchAgentExpectation::Managed(prior.active_generation),
        ),
        None => (
            paths.daemon_binary.clone(),
            LaunchAgentExpectation::Legacy(DaemonMode::ReportOnly),
        ),
    };
    validate_plist_bytes(report_only_plist.as_bytes(), &expected_program, expectation)?;
    Ok(report_only_plist)
}

fn persist_transaction(
    layout: &ServiceLayout,
    transaction: &InstallTransaction,
) -> Result<(), ServiceError> {
    write_json_atomic(&layout.transaction, transaction, 0o600)
}

fn finalize_accepted_lease(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    transaction: &InstallTransaction,
) -> Result<(), ServiceError> {
    if transaction.phase != TransactionPhase::Accepted {
        return Err(ServiceError::new(
            "candidate is not durably accepted; rollback material must be retained",
        ));
    }
    remove_legacy_binaries(paths)?;
    remove_file_durable(&layout.database_backup)?;
    remove_file_durable(&layout.database_backup_pending)?;
    remove_file_durable(&layout.transaction)
}

fn create_generation(
    layout: &ServiceLayout,
    generation: &GenerationPaths,
    source_cli: &Path,
    source_daemon: &Path,
) -> Result<(), ServiceError> {
    if generation.directory.exists() {
        return Err(ServiceError::new(format!(
            "activation generation already exists: {}",
            generation.activation_generation
        )));
    }
    let staging = unique_generation_stage(layout, generation.activation_generation)?;
    fs::create_dir(&staging)
        .map_err(|error| ServiceError::context("could not create generation staging", error))?;
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).map_err(|error| {
        ServiceError::context("could not protect generation staging directory", error)
    })?;
    let mut guard = GenerationStageGuard {
        path: staging.clone(),
        promoted: false,
    };
    let staged = GenerationPaths {
        daemon: staging.join("unlingerd"),
        cli: staging.join("unlinger"),
        manifest: staging.join("manifest.json"),
        directory: staging.clone(),
        activation_generation: generation.activation_generation,
    };
    copy_file_synced(source_daemon, &staged.daemon, 0o700)?;
    copy_file_synced(source_cli, &staged.cli, 0o700)?;
    validate_binary(&staged.daemon, "unlingerd")?;
    validate_binary(&staged.cli, "unlinger")?;
    write_json_atomic(
        &staged.manifest,
        &GenerationManifest::new(generation.activation_generation),
        0o600,
    )?;
    validate_generation(&staged)?;

    fs::set_permissions(&staged.daemon, fs::Permissions::from_mode(0o500))
        .map_err(|error| ServiceError::context("could not seal daemon binary", error))?;
    fs::set_permissions(&staged.cli, fs::Permissions::from_mode(0o500))
        .map_err(|error| ServiceError::context("could not seal CLI binary", error))?;
    fs::set_permissions(&staged.manifest, fs::Permissions::from_mode(0o400))
        .map_err(|error| ServiceError::context("could not seal generation manifest", error))?;
    for path in [&staged.daemon, &staged.cli, &staged.manifest] {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| ServiceError::context("could not sync sealed generation", error))?;
    }
    sync_directory(&staging)?;

    // macOS 14/15 rejects renaming a directory after its owner-write bit has
    // been removed, even when the rename stays within one writable parent.
    // The sealed child artifacts are already durable; publish the still-
    // inactive owner-private directory atomically, then seal the final path
    // before any active manifest can select it.
    fs::rename(&staging, &generation.directory)
        .map_err(|error| ServiceError::context("could not publish generation", error))?;
    guard.path = generation.directory.clone();
    fs::set_permissions(&generation.directory, fs::Permissions::from_mode(0o500))
        .map_err(|error| ServiceError::context("could not seal generation directory", error))?;
    sync_directory(&generation.directory)?;
    sync_directory(&layout.generations)?;
    validate_generation(generation)?;
    guard.promoted = true;
    Ok(())
}

fn validate_generation(generation: &GenerationPaths) -> Result<(), ServiceError> {
    let directory = fs::symlink_metadata(&generation.directory)
        .map_err(|error| ServiceError::context("could not inspect generation directory", error))?;
    if !directory.file_type().is_dir() || directory.file_type().is_symlink() {
        return Err(ServiceError::new("generation path is not a real directory"));
    }
    for (path, label) in [
        (&generation.daemon, "daemon"),
        (&generation.cli, "CLI"),
        (&generation.manifest, "manifest"),
    ] {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            ServiceError::context(&format!("could not inspect generation {label}"), error)
        })?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(ServiceError::new(format!(
                "generation {label} must be a regular file"
            )));
        }
    }
    let manifest: GenerationManifest = read_json(&generation.manifest, "generation manifest")?;
    if manifest != GenerationManifest::new(generation.activation_generation) {
        return Err(ServiceError::new(
            "generation manifest does not match its immutable directory",
        ));
    }
    Ok(())
}

fn next_generation(layout: &ServiceLayout) -> Result<u64, ServiceError> {
    let mut maximum = 0_u64;
    for entry in fs::read_dir(&layout.generations)
        .map_err(|error| ServiceError::context("could not list service generations", error))?
    {
        let entry = entry.map_err(|error| {
            ServiceError::context("could not inspect service generation", error)
        })?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(number) = name.parse::<u64>() else {
            continue;
        };
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            ServiceError::context("could not inspect numbered generation", error)
        })?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(ServiceError::new(format!(
                "numbered generation is not a directory: {name}"
            )));
        }
        maximum = maximum.max(number);
    }
    maximum
        .checked_add(1)
        .ok_or_else(|| ServiceError::new("activation generation space is exhausted"))
}

fn unique_generation_stage(
    layout: &ServiceLayout,
    generation: u64,
) -> Result<PathBuf, ServiceError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ServiceError::context("wall clock failed", error))?
        .as_nanos();
    let sequence = NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed);
    Ok(layout.generations.join(format!(
        ".{generation}.stage.{}-{now:x}-{sequence}",
        std::process::id()
    )))
}

struct GenerationStageGuard {
    path: PathBuf,
    promoted: bool,
}

impl Drop for GenerationStageGuard {
    fn drop(&mut self) {
        if !self.promoted {
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn copy_file_synced(source: &Path, destination: &Path, mode: u32) -> Result<(), ServiceError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| ServiceError::context("could not inspect source binary", error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ServiceError::new(format!(
            "source binary must be a regular file: {}",
            source.display()
        )));
    }
    let mut input = File::open(source)
        .map_err(|error| ServiceError::context("could not open source binary", error))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(destination)
        .map_err(|error| ServiceError::context("could not create generation binary", error))?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| ServiceError::context("could not copy generation binary", error))?;
    output
        .sync_all()
        .map_err(|error| ServiceError::context("could not sync generation binary", error))?;
    fs::set_permissions(destination, fs::Permissions::from_mode(mode))
        .map_err(|error| ServiceError::context("could not set generation binary mode", error))?;
    output
        .sync_all()
        .map_err(|error| ServiceError::context("could not sync generation binary mode", error))
}

fn backup_database(database: &Path, layout: &ServiceLayout) -> Result<bool, ServiceError> {
    validate_managed_database_path(database, layout)?;
    let metadata = match fs::symlink_metadata(database) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            remove_file_durable(&layout.database_backup)?;
            remove_file_durable(&layout.database_backup_pending)?;
            return Ok(false);
        }
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect history database",
                error,
            ));
        }
    };
    validate_database_component(database, &metadata)?;
    validate_database_sidecars(database)?;
    remove_file_durable(&layout.database_backup)?;
    remove_file_durable(&layout.database_backup_pending)?;
    let staged = &layout.database_backup_pending;
    let result = (|| {
        let connection = rusqlite::Connection::open_with_flags(
            database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| ServiceError::context("could not open history for backup", error))?;
        connection
            .execute("VACUUM INTO ?1", [staged.to_string_lossy().as_ref()])
            .map_err(|error| ServiceError::context("could not snapshot history database", error))?;
        fs::set_permissions(staged, fs::Permissions::from_mode(0o600))
            .map_err(|error| ServiceError::context("could not protect history backup", error))?;
        File::open(staged)
            .and_then(|file| file.sync_all())
            .map_err(|error| ServiceError::context("could not sync history backup", error))?;
        validate_sqlite_database(staged)?;
        validate_replaceable_file(&layout.database_backup)?;
        fs::rename(staged, &layout.database_backup)
            .map_err(|error| ServiceError::context("could not publish history backup", error))?;
        sync_directory(
            layout
                .database_backup
                .parent()
                .ok_or_else(|| ServiceError::new("history backup has no parent"))?,
        )?;
        Ok::<(), ServiceError>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(staged);
    }
    result.map(|()| true)
}

fn validate_database_sidecars(database: &Path) -> Result<(), ServiceError> {
    for sidecar in [
        database_sidecar(database, "-wal"),
        database_sidecar(database, "-shm"),
        database_sidecar(database, "-journal"),
    ] {
        match fs::symlink_metadata(&sidecar) {
            Ok(metadata) => validate_database_component(&sidecar, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ServiceError::context(
                    "could not inspect history database sidecar",
                    error,
                ));
            }
        }
    }
    Ok(())
}

fn restore_database_backup_files(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    failed_generation: u64,
) -> Result<Option<PathBuf>, ServiceError> {
    validate_managed_database_path(&paths.database, layout)?;
    let backup_metadata = fs::symlink_metadata(&layout.database_backup).map_err(|error| {
        ServiceError::context("committed history rollback backup is unavailable", error)
    })?;
    validate_database_component(&layout.database_backup, &backup_metadata)?;
    validate_sqlite_database(&layout.database_backup)?;

    let evidence = preserve_failed_database(paths, layout, failed_generation)?;
    for path in [
        paths.database.clone(),
        database_sidecar(&paths.database, "-wal"),
        database_sidecar(&paths.database, "-shm"),
        database_sidecar(&paths.database, "-journal"),
    ] {
        if path_entry_exists(&path)? {
            return Err(ServiceError::new(format!(
                "database component remained after evidence preservation: {}",
                path.display()
            )));
        }
    }

    copy_file_synced(&layout.database_backup, &paths.database, 0o600)?;
    sync_directory(&paths.application_support)?;
    validate_sqlite_database(&paths.database)?;
    Ok(evidence)
}

fn preserve_failed_database(
    paths: &LocalPaths,
    layout: &ServiceLayout,
    failed_generation: u64,
) -> Result<Option<PathBuf>, ServiceError> {
    validate_managed_database_path(&paths.database, layout)?;
    let candidates = [
        paths.database.clone(),
        database_sidecar(&paths.database, "-wal"),
        database_sidecar(&paths.database, "-shm"),
        database_sidecar(&paths.database, "-journal"),
    ];
    let mut present = Vec::new();
    for path in &candidates {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                validate_database_component(path, &metadata)?;
                present.push(path.clone());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ServiceError::context(
                    "could not inspect failed database component",
                    error,
                ));
            }
        }
    }
    if present.is_empty() {
        return Ok(None);
    }

    create_private_directory(&layout.failed_generations)?;
    let evidence = unique_failed_evidence_path(layout, failed_generation)?;
    fs::create_dir(&evidence)
        .map_err(|error| ServiceError::context("could not create database evidence", error))?;
    fs::set_permissions(&evidence, fs::Permissions::from_mode(0o700))
        .map_err(|error| ServiceError::context("could not protect database evidence", error))?;
    for source in present {
        let filename = source
            .file_name()
            .ok_or_else(|| ServiceError::new("database component has no filename"))?;
        fs::rename(&source, evidence.join(filename)).map_err(|error| {
            ServiceError::context("could not preserve failed database component", error)
        })?;
    }
    sync_directory(&evidence)?;
    sync_directory(&layout.failed_generations)?;
    sync_directory(&paths.application_support)?;
    Ok(Some(evidence))
}

fn validate_database_component(path: &Path, metadata: &fs::Metadata) -> Result<(), ServiceError> {
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != current_uid()
    {
        return Err(ServiceError::new(format!(
            "database component must be a current-user regular file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_managed_database_path(
    database: &Path,
    layout: &ServiceLayout,
) -> Result<(), ServiceError> {
    let expected = layout
        .active_manifest
        .parent()
        .ok_or_else(|| ServiceError::new("active manifest has no parent"))?
        .join("history.sqlite3");
    if database == expected {
        Ok(())
    } else {
        Err(ServiceError::new(format!(
            "refusing database transaction outside the managed history path: {}",
            database.display()
        )))
    }
}

fn validate_sqlite_database(path: &Path) -> Result<(), ServiceError> {
    let connection = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| ServiceError::context("could not open SQLite validation target", error))?;
    let result = connection
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| ServiceError::context("could not validate SQLite backup", error))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(ServiceError::new(format!(
            "SQLite backup quick_check failed: {result}"
        )))
    }
}

fn database_sidecar(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

fn create_private_directory(path: &Path) -> Result<(), ServiceError> {
    fs::create_dir_all(path)
        .map_err(|error| ServiceError::context("could not create private directory", error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ServiceError::context("could not inspect private directory", error))?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != current_uid()
    {
        return Err(ServiceError::new(format!(
            "private path is not a current-user directory: {}",
            path.display()
        )));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| ServiceError::context("could not protect private directory", error))
}

fn unique_failed_evidence_path(
    layout: &ServiceLayout,
    generation: u64,
) -> Result<PathBuf, ServiceError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ServiceError::context("wall clock failed", error))?
        .as_nanos();
    let sequence = NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed);
    Ok(layout.failed_generations.join(format!(
        "generation-{generation}-{}-{now:x}-{sequence}",
        std::process::id()
    )))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T, mode: u32) -> Result<(), ServiceError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| ServiceError::context("could not encode service metadata", error))?;
    bytes.push(b'\n');
    write_bytes_atomic(path, &bytes, mode)
}

fn write_bytes_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<(), ServiceError> {
    validate_replaceable_file(path)?;
    let staged = unique_peer_path(path, "stage")?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(no_follow_flag())
        .open(&staged)
        .map_err(|error| ServiceError::context("could not create staged metadata", error))?;
    let result = (|| {
        output
            .write_all(bytes)
            .map_err(|error| ServiceError::context("could not write staged metadata", error))?;
        output
            .sync_all()
            .map_err(|error| ServiceError::context("could not sync staged metadata", error))?;
        fs::set_permissions(&staged, fs::Permissions::from_mode(mode))
            .map_err(|error| ServiceError::context("could not protect staged metadata", error))?;
        output
            .sync_all()
            .map_err(|error| ServiceError::context("could not sync staged metadata mode", error))?;
        fs::rename(&staged, path)
            .map_err(|error| ServiceError::context("could not publish service metadata", error))?;
        let parent = path
            .parent()
            .ok_or_else(|| ServiceError::new("service metadata path has no parent"))?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, ServiceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ServiceError::context(&format!("could not inspect {label}"), error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ServiceError::new(format!("{label} must be a regular file")));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|error| ServiceError::context(&format!("could not open {label}"), error))?;
    serde_json::from_reader(file)
        .map_err(|error| ServiceError::context(&format!("could not decode {label}"), error))
}

fn read_optional_manifest(path: &Path) -> Result<Option<ActiveServiceManifest>, ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect active service manifest",
                error,
            ));
        }
    }
    let manifest: ActiveServiceManifest = read_json(path, "active service manifest")?;
    if manifest.schema_version != SERVICE_MANIFEST_SCHEMA_VERSION {
        return Err(ServiceError::new(format!(
            "unsupported active service manifest schema {}",
            manifest.schema_version
        )));
    }
    Ok(Some(manifest))
}

fn read_optional_transaction(path: &Path) -> Result<Option<InstallTransaction>, ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect service transaction",
                error,
            ));
        }
    }
    let transaction: InstallTransaction = read_json(path, "service transaction")?;
    if transaction.schema_version != SERVICE_TRANSACTION_SCHEMA_VERSION {
        return Err(ServiceError::new(format!(
            "unsupported service transaction schema {}",
            transaction.schema_version
        )));
    }
    Ok(Some(transaction))
}

fn read_optional_text(path: &Path) -> Result<Option<String>, ServiceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect managed text file",
                error,
            ));
        }
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ServiceError::new(format!(
            "managed text path must be a regular file: {}",
            path.display()
        )));
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|error| ServiceError::context("could not read managed text file", error))
}

fn path_entry_exists(path: &Path) -> Result<bool, ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ServiceError::context(
            "could not inspect managed path",
            error,
        )),
    }
}

fn validate_replaceable_file(path: &Path) -> Result<(), ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => Err(ServiceError::new(format!(
            "managed destination must be absent or a regular file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ServiceError::context(
            "could not inspect managed destination",
            error,
        )),
    }
}

fn validate_plist_bytes(
    bytes: &[u8],
    expected_program: &Path,
    expectation: LaunchAgentExpectation,
) -> Result<(), ServiceError> {
    let document = parse_launch_agent_bytes(bytes)?;
    validate_launch_agent_document(&document, expected_program, expectation)
}

fn sync_directory(path: &Path) -> Result<(), ServiceError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| ServiceError::context("could not sync containing directory", error))
}

fn remove_file_durable(path: &Path) -> Result<(), ServiceError> {
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::new("managed file path has no parent"))?;
    remove_managed_file(path)?;
    sync_directory(parent)
}

fn process_runs_binary(pid: u32, expected: &Path) -> bool {
    let Ok(metadata) = fs::metadata(expected) else {
        return false;
    };
    MacosSnapshotter::new()
        .lookup(pid)
        .ok()
        .flatten()
        .is_some_and(|process| {
            process.identity.executable_device == Some(metadata.dev())
                && process.identity.executable_inode == Some(metadata.ino())
        })
}

fn remove_legacy_binaries(paths: &LocalPaths) -> Result<(), ServiceError> {
    for path in [&paths.daemon_binary, &paths.cli_binary] {
        remove_file_durable(path)?;
    }
    Ok(())
}

fn remove_generation_tree(layout: &ServiceLayout) -> Result<(), ServiceError> {
    let metadata = match fs::symlink_metadata(&layout.generations) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect generation root",
                error,
            ));
        }
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(ServiceError::new(
            "refusing to remove a non-directory generation root",
        ));
    }
    for entry in fs::read_dir(&layout.generations)
        .map_err(|error| ServiceError::context("could not list generation root", error))?
    {
        let entry =
            entry.map_err(|error| ServiceError::context("could not inspect generation", error))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.parse::<u64>().is_err() && !name.contains(".stage.") {
            return Err(ServiceError::new(format!(
                "refusing to remove unexpected generation entry {name}"
            )));
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| ServiceError::context("could not inspect generation entry", error))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(ServiceError::new(format!(
                "refusing to remove non-directory generation entry {name}"
            )));
        }
        fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o700)).map_err(|error| {
            ServiceError::context("could not unseal generation for uninstall", error)
        })?;
        fs::remove_dir_all(entry.path())
            .map_err(|error| ServiceError::context("could not remove generation", error))?;
    }
    fs::remove_dir(&layout.generations)
        .map_err(|error| ServiceError::context("could not remove generation root", error))?;
    sync_directory(
        layout
            .generations
            .parent()
            .ok_or_else(|| ServiceError::new("generation root has no parent"))?,
    )
}

#[cfg(target_vendor = "apple")]
const fn no_follow_flag() -> i32 {
    libc::O_NOFOLLOW_ANY
}

#[cfg(not(target_vendor = "apple"))]
const fn no_follow_flag() -> i32 {
    libc::O_NOFOLLOW
}

fn service_domain(uid: u32) -> String {
    format!("gui/{uid}")
}

fn service_target(uid: u32) -> String {
    format!("{}/{}", service_domain(uid), LAUNCH_AGENT_LABEL)
}

fn validate_binary(path: &Path, expected_name: &str) -> Result<(), ServiceError> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|error| ServiceError::context("could not execute staged binary", error))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.trim_start().starts_with(expected_name) {
        Ok(())
    } else {
        Err(ServiceError::new(format!(
            "staged binary {} failed identity check for {expected_name}",
            path.display()
        )))
    }
}

fn verify_permissions(
    paths: &LocalPaths,
    generation: Option<&GenerationPaths>,
    layout: &ServiceLayout,
    uid: u32,
    errors: &mut Vec<String>,
) -> bool {
    let mut ok = true;
    let mut managed_paths = vec![
        (&paths.application_support, 0o700, "directory"),
        (&paths.binary_directory, 0o700, "directory"),
        (&layout.generations, 0o700, "directory"),
        (&paths.cache_directory, 0o700, "directory"),
        (&paths.runtime_directory, 0o700, "directory"),
        (&paths.logs_directory, 0o700, "directory"),
        (&paths.launch_agent, 0o600, "file"),
        (&paths.daemon_binary, 0o700, "file"),
        (&paths.cli_binary, 0o700, "file"),
        (&layout.active_manifest, 0o600, "file"),
        (&layout.transaction, 0o600, "file"),
        (&layout.database_backup, 0o600, "file"),
        (&layout.database_backup_pending, 0o600, "file"),
        (&layout.failed_generations, 0o700, "directory"),
        (&paths.service_lock, 0o600, "file"),
        (&paths.daemon_lock, 0o600, "file"),
        (&paths.database, 0o600, "file"),
        (&paths.error_log, 0o600, "file"),
    ];
    if let Some(generation) = generation {
        managed_paths.extend([
            (&generation.directory, 0o500, "directory"),
            (&generation.daemon, 0o500, "file"),
            (&generation.cli, 0o500, "file"),
            (&generation.manifest, 0o400, "file"),
        ]);
    }
    for (path, expected_mode, kind) in managed_paths {
        if !path.exists() {
            continue;
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                let type_matches = match kind {
                    "directory" => metadata.file_type().is_dir(),
                    _ => metadata.file_type().is_file(),
                } && !metadata.file_type().is_symlink();
                let mode_matches = metadata.permissions().mode() & 0o777 == expected_mode;
                let owner_matches = metadata.uid() == uid;
                if !type_matches || !mode_matches || !owner_matches {
                    ok = false;
                    errors.push(format!(
                        "unsafe ownership, type, or permissions at {}",
                        path.display()
                    ));
                }
            }
            Err(error) => {
                ok = false;
                errors.push(format!("could not inspect {}: {error}", path.display()));
            }
        }
    }
    if paths.socket.exists() {
        match fs::symlink_metadata(&paths.socket) {
            Ok(metadata) => {
                if !metadata.file_type().is_socket()
                    || metadata.file_type().is_symlink()
                    || metadata.permissions().mode() & 0o777 != 0o600
                    || metadata.uid() != uid
                {
                    ok = false;
                    errors.push(format!(
                        "unsafe ownership, type, or permissions at {}",
                        paths.socket.display()
                    ));
                }
            }
            Err(error) => {
                ok = false;
                errors.push(format!(
                    "could not inspect {}: {error}",
                    paths.socket.display()
                ));
            }
        }
    }
    ok
}

fn remove_managed_file(path: &Path) -> Result<(), ServiceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ServiceError::context(
                "could not inspect managed file",
                error,
            ));
        }
    };
    if metadata.file_type().is_dir() {
        return Err(ServiceError::new(format!(
            "refusing to remove directory at managed file path {}",
            path.display()
        )));
    }
    fs::remove_file(path)
        .map_err(|error| ServiceError::context("could not remove managed file", error))
}

struct ServiceLock {
    _file: File,
}

impl ServiceLock {
    fn acquire(path: &Path) -> Result<Self, ServiceError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(no_follow_flag())
            .open(path)
            .map_err(|error| {
                ServiceError::context("could not open service transaction lock", error)
            })?;
        let metadata = file.metadata().map_err(|error| {
            ServiceError::context("could not inspect service transaction lock", error)
        })?;
        if !metadata.file_type().is_file() || metadata.uid() != current_uid() {
            return Err(ServiceError::new(
                "service transaction lock must be a regular file owned by the current user",
            ));
        }
        if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
            return Err(ServiceError::context(
                "could not protect service transaction lock",
                std::io::Error::last_os_error(),
            ));
        }
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            return Err(ServiceError::new(format!(
                "another Unlinger service transaction is active: {error}"
            )));
        }
        Ok(Self { _file: file })
    }
}

impl Drop for ServiceLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self._file.as_raw_fd(), libc::LOCK_UN) };
    }
}

fn unique_peer_path(destination: &Path, role: &str) -> Result<PathBuf, ServiceError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ServiceError::new("managed file path has no parent"))?;
    let filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ServiceError::new("managed filename is not valid UTF-8"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ServiceError::context("wall clock failed", error))?
        .as_nanos();
    let sequence = NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed);
    Ok(parent.join(format!(
        ".{filename}.{role}.{}-{now:x}-{sequence}",
        std::process::id()
    )))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::os::unix::net::UnixListener;

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ul-service-{}-{}",
                std::process::id(),
                NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed)
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

    fn managed_lifecycle_status(
        mode: DaemonMode,
        generation: u64,
        instance_id: &str,
    ) -> DaemonStatus {
        let mut status = DaemonStatus::new(mode, 42);
        status.managed = true;
        status.instance_id = instance_id.to_owned();
        status.activation_generation = Some(generation);
        status.ready = true;
        status.healthy = true;
        status.last_scan_at_unix_millis = Some(1);
        match mode {
            DaemonMode::ReportOnly => {
                status.requested_mode = DaemonMode::ReportOnly;
                status.effective_mode = DaemonMode::ReportOnly;
                status.armed_generation = None;
                status.enforcement_epoch = None;
                status.startup_state = StartupState::ReadyReportOnly;
            }
            DaemonMode::Enforce => {
                status.requested_mode = DaemonMode::Enforce;
                status.effective_mode = DaemonMode::Enforce;
                status.armed_generation = Some(generation);
                status.enforcement_epoch = Some("epoch-1".to_owned());
                status.startup_state = StartupState::ReadyEnforce;
            }
        }
        status
    }

    fn seed_carried_enforce(paths: &LocalPaths, generation: u64) {
        let store = HistoryStore::open(&paths.database).expect("open managed store");
        store
            .begin_managed_boot(generation, "instance-a", 1_000)
            .expect("begin first managed boot");
        store
            .finish_managed_recovery(generation, "instance-a", 1_010)
            .expect("finish first recovery");
        store
            .complete_managed_first_scan(generation, "instance-a", "unused-first-epoch", 1_020)
            .expect("complete first scan");
        store
            .arm_managed(generation, "instance-a", "epoch-a", 1_030)
            .expect("persist enforce intent");
        let restarted = store
            .begin_managed_boot(generation, "instance-b", 2_000)
            .expect("begin same-generation replacement");
        assert!(restarted.requested_enforce);
        assert!(!restarted.effective_enforce);
    }

    fn shorten_test_ipc_paths(paths: &mut LocalPaths, root: &Path) {
        paths.runtime_directory = root.join("run");
        paths.socket = paths.runtime_directory.join("daemon.sock");
        paths.daemon_lock = paths.runtime_directory.join("daemon.lock");
        paths.cache_directory = root.join("cache");
    }

    fn legacy_launch_agent_plist(program: &Path, mode: DaemonMode) -> String {
        let mode = match mode {
            DaemonMode::ReportOnly => "--report-only",
            DaemonMode::Enforce => "--enforce",
        };
        let program = xml_escape(&program.to_string_lossy());
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCH_AGENT_LABEL}</string>
  <key>Program</key>
  <string>{program}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{program}</string>
    <string>{mode}</string>
  </array>
</dict>
</plist>
"#
        )
    }

    #[test]
    fn plist_is_generation_bound_report_only_and_escapes_local_paths() {
        let paths = LocalPaths::from_home("/Users/A&B Person").expect("paths");
        let layout = ServiceLayout::new(&paths);
        let generation = layout.generation(41);
        let plist = launch_agent_plist(&paths, &generation);
        assert!(plist.contains("<string>app.unlinger.daemon</string>"));
        assert!(plist.contains("A&amp;B Person/Library/Application Support"));
        assert!(plist.contains("generations/41/unlingerd"));
        assert!(plist.contains("<string>--managed</string>"));
        assert!(plist.contains("<string>--activation-generation</string>"));
        assert!(plist.contains("<string>41</string>"));
        assert!(!plist.contains("--enforce"));
        assert!(!plist.contains("--report-only"));
        assert!(plist.contains("<key>KeepAlive</key>\n  <true/>"));
        assert!(plist.contains("<key>ProcessType</key>\n  <string>Background</string>"));
        assert!(plist.contains("<key>ExitTimeOut</key>\n  <integer>120</integer>"));
        assert!(plist.contains("<key>Umask</key>\n  <string>077</string>"));
        assert!(!plist.contains("RunAtLoad"));
        let document = parse_launch_agent_bytes(plist.as_bytes()).expect("parse plist");
        assert_eq!(
            managed_generation_from_document(&document).expect("generation"),
            Some(41)
        );
    }

    #[test]
    fn generated_plist_passes_the_host_plutil_parser() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home("/Users/example").expect("paths");
        let plist_path = temp.0.join("app.unlinger.daemon.plist");
        fs::write(
            &plist_path,
            launch_agent_plist(&paths, &ServiceLayout::new(&paths).generation(7)),
        )
        .expect("write plist");

        let output = Command::new("/usr/bin/plutil")
            .arg("-lint")
            .arg(&plist_path)
            .output()
            .expect("run plutil");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn managed_plist_contract_rejects_label_program_and_argv_drift() {
        let paths = LocalPaths::from_home("/Users/example").expect("paths");
        let generation = ServiceLayout::new(&paths).generation(7);
        let plist = launch_agent_plist(&paths, &generation);
        validate_plist_bytes(
            plist.as_bytes(),
            &generation.daemon,
            LaunchAgentExpectation::Managed(7),
        )
        .expect("exact generated contract");

        let wrong_label = plist.replacen(
            &format!("<string>{LAUNCH_AGENT_LABEL}</string>"),
            "<string>app.unlinger.drifted</string>",
            1,
        );
        assert!(
            validate_plist_bytes(
                wrong_label.as_bytes(),
                &generation.daemon,
                LaunchAgentExpectation::Managed(7),
            )
            .is_err()
        );

        let escaped_program = xml_escape(&generation.daemon.to_string_lossy());
        let wrong_program = plist.replacen(
            &format!("<string>{escaped_program}</string>"),
            "<string>/tmp/not-the-selected-daemon</string>",
            1,
        );
        assert!(
            validate_plist_bytes(
                wrong_program.as_bytes(),
                &generation.daemon,
                LaunchAgentExpectation::Managed(7),
            )
            .is_err()
        );

        let extra_argument =
            plist.replacen("  </array>", "    <string>--once</string>\n  </array>", 1);
        assert!(
            validate_plist_bytes(
                extra_argument.as_bytes(),
                &generation.daemon,
                LaunchAgentExpectation::Managed(7),
            )
            .is_err()
        );
    }

    #[test]
    fn structured_plist_classification_ignores_mode_tokens_outside_program_arguments() {
        let program = Path::new("/Users/example/unlingerd");
        let plist = legacy_launch_agent_plist(program, DaemonMode::ReportOnly).replace(
            "</dict>",
            "  <key>StandardErrorPath</key>\n  <string>/tmp/--managed--activation-generation</string>\n</dict>",
        );
        let document = parse_launch_agent_bytes(plist.as_bytes()).expect("parse legacy plist");

        assert_eq!(
            managed_generation_from_document(&document).expect("classify"),
            None
        );
        assert_eq!(
            legacy_mode_from_document(&document, Some(program)).expect("legacy mode"),
            DaemonMode::ReportOnly
        );
    }

    #[test]
    fn rollback_manifest_can_only_request_report_only() {
        let active = ActiveServiceManifest::new(27, DaemonMode::Enforce);

        let rollback = active.rollback_floor();

        assert_eq!(rollback.active_generation, 27);
        assert_eq!(rollback.desired_mode, DaemonMode::ReportOnly);
    }

    #[test]
    fn lifecycle_response_validation_requires_exact_generation_instance_and_readiness() {
        let report_only = managed_lifecycle_status(DaemonMode::ReportOnly, 7, "instance-a");
        validate_report_only_response(&report_only, 7, "instance-a")
            .expect("exact report-only response");
        validate_quiesce_disarm_response(&report_only, 7, "instance-a")
            .expect("ready report-only remains quiescent");
        assert!(validate_report_only_response(&report_only, 7, "instance-b").is_err());
        assert!(validate_report_only_response(&report_only, 8, "instance-a").is_err());

        let mut not_ready = report_only.clone();
        not_ready.ready = false;
        not_ready.startup_state = StartupState::Recovering;
        validate_disarmed_response(&not_ready, 7, "instance-a")
            .expect("fail-closed disarm does not require readiness");
        assert!(validate_report_only_response(&not_ready, 7, "instance-a").is_err());
        assert!(validate_quiesce_disarm_response(&not_ready, 7, "instance-a").is_err());

        let mut failed = report_only.clone();
        failed.healthy = false;
        failed.ready = false;
        failed.startup_state = StartupState::Failed;
        failed.last_error = Some("primary managed failure".to_owned());
        validate_quiesce_disarm_response(&failed, 7, "instance-a")
            .expect("exact terminal Failed can proceed to coordinated drain");
        assert!(validate_report_only_response(&failed, 7, "instance-a").is_err());
        assert!(validate_quiesce_disarm_response(&failed, 7, "instance-b").is_err());
        assert!(validate_quiesce_disarm_response(&failed, 8, "instance-a").is_err());

        let mut failed_not_disarmed = failed.clone();
        failed_not_disarmed.requested_mode = DaemonMode::Enforce;
        failed_not_disarmed.effective_mode = DaemonMode::Enforce;
        failed_not_disarmed.armed_generation = Some(7);
        failed_not_disarmed.enforcement_epoch = Some("stale-epoch".to_owned());
        assert!(validate_quiesce_disarm_response(&failed_not_disarmed, 7, "instance-a").is_err());

        let mut failed_but_ready = failed;
        failed_but_ready.healthy = true;
        failed_but_ready.ready = true;
        assert!(validate_quiesce_disarm_response(&failed_but_ready, 7, "instance-a").is_err());

        let enforce = managed_lifecycle_status(DaemonMode::Enforce, 7, "instance-a");
        validate_enforce_response(&enforce, 7, "instance-a").expect("exact enforce response");
        assert!(validate_enforce_response(&enforce, 7, "instance-b").is_err());
    }

    #[test]
    fn already_enforced_mode_requires_exact_runtime_and_service_identity() {
        let paths = LocalPaths::from_home("/Users/example").expect("paths");
        let daemon = managed_lifecycle_status(DaemonMode::Enforce, 7, "instance-a");
        let report = ServiceStatusReport {
            schema_version: SERVICE_SCHEMA_VERSION,
            label: LAUNCH_AGENT_LABEL,
            installed: true,
            loaded: true,
            healthy: true,
            unmanaged_daemon: false,
            expected_mode: Some(DaemonMode::Enforce),
            active_generation: Some(7),
            launchd_pid: Some(42),
            daemon_status: Some(daemon),
            pid_matches: true,
            generation_matches: true,
            binary_matches: true,
            permissions_ok: true,
            launch_agent_path: paths.launch_agent.clone(),
            daemon_path: paths.binary_directory.join("unlingerd"),
            cli_path: paths.binary_directory.join("unlinger"),
            database_path: paths.database.clone(),
            socket_path: paths.socket.clone(),
            data_preserved: true,
            acceptance: None,
            errors: Vec::new(),
        };

        validate_ready_enforce_report(&report, 7).expect("exact enforced service");

        let mut missing_epoch = report.clone();
        missing_epoch
            .daemon_status
            .as_mut()
            .expect("daemon")
            .enforcement_epoch = None;
        assert!(validate_ready_enforce_report(&missing_epoch, 7).is_err());

        let mut wrong_binary = report.clone();
        wrong_binary.binary_matches = false;
        assert!(validate_ready_enforce_report(&wrong_binary, 7).is_err());

        let mut stale_scan = report;
        stale_scan
            .daemon_status
            .as_mut()
            .expect("daemon")
            .last_scan_at_unix_millis = None;
        assert!(validate_ready_enforce_report(&stale_scan, 7).is_err());
    }

    #[test]
    fn offline_emergency_recovery_clears_only_the_exact_generation_before_rebootstrap() {
        let temp = TempDirectory::new();
        let mut paths = LocalPaths::from_home(&temp.0).expect("paths");
        shorten_test_ipc_paths(&mut paths, &temp.0);
        prepare_install_directories(&paths).expect("prepare managed directories");
        let layout = ServiceLayout::new(&paths);
        let store = HistoryStore::open(&paths.database).expect("open managed store");
        store
            .begin_managed_boot(7, "instance-a", 1_000)
            .expect("begin managed boot");
        store
            .finish_managed_recovery(7, "instance-a", 1_010)
            .expect("finish recovery");
        store
            .complete_managed_first_scan(7, "instance-a", "unused-first-epoch", 1_020)
            .expect("complete first scan");
        store
            .arm_managed(7, "instance-a", "epoch-a", 1_030)
            .expect("persist enforce intent");
        drop(store);
        let daemon_lock =
            prove_daemon_offline(&paths, Duration::ZERO).expect("prove daemon offline");

        assert!(
            clear_generation_enforce_request_offline(&paths, &layout, 8, &daemon_lock).is_err()
        );
        let still_armed = HistoryStore::open(&paths.database)
            .expect("reopen after rejected clear")
            .managed_lifecycle()
            .expect("read lifecycle")
            .expect("lifecycle exists");
        assert!(still_armed.requested_enforce);

        clear_generation_enforce_request_offline(&paths, &layout, 7, &daemon_lock)
            .expect("clear exact generation offline");
        drop(daemon_lock);
        let offline = HistoryStore::open(&paths.database).expect("reopen cleared store");
        let cleared = offline
            .managed_lifecycle()
            .expect("read cleared lifecycle")
            .expect("lifecycle exists");
        assert!(!cleared.requested_enforce);
        assert!(!cleared.effective_enforce);
        assert!(!cleared.ready);
        assert_eq!(cleared.armed_generation, None);
        assert_eq!(cleared.enforcement_epoch, None);

        let reboot = offline
            .begin_managed_boot(7, "instance-b", 2_000)
            .expect("rebootstrap same generation");
        assert!(!reboot.requested_enforce);
        offline
            .finish_managed_recovery(7, "instance-b", 2_010)
            .expect("finish replacement recovery");
        let ready = offline
            .complete_managed_first_scan(7, "instance-b", "must-not-arm", 2_020)
            .expect("replacement remains report-only");
        assert!(ready.ready);
        assert!(!ready.requested_enforce);
        assert!(!ready.effective_enforce);
    }

    #[test]
    fn emergency_runtime_readiness_ignores_stale_enforce_manifest_but_requires_exact_identity() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        let mut daemon = managed_lifecycle_status(DaemonMode::ReportOnly, 7, "instance-a");
        daemon.pid = 42;
        let report = ServiceStatusReport {
            schema_version: SERVICE_SCHEMA_VERSION,
            label: LAUNCH_AGENT_LABEL,
            installed: true,
            loaded: true,
            healthy: false,
            unmanaged_daemon: false,
            expected_mode: Some(DaemonMode::Enforce),
            active_generation: Some(7),
            launchd_pid: Some(42),
            daemon_status: Some(daemon),
            pid_matches: true,
            generation_matches: true,
            binary_matches: true,
            permissions_ok: true,
            launch_agent_path: paths.launch_agent.clone(),
            daemon_path: paths.binary_directory.join("unlingerd"),
            cli_path: paths.binary_directory.join("unlinger"),
            database_path: paths.database.clone(),
            socket_path: paths.socket.clone(),
            data_preserved: true,
            acceptance: None,
            errors: vec!["desired mode has not been published yet".to_owned()],
        };

        assert!(generation_runtime_is_ready_report_only(&report, 7));
        assert!(!generation_runtime_is_ready_report_only(&report, 8));
        let mut scanning = report.clone();
        scanning
            .daemon_status
            .as_mut()
            .expect("daemon status")
            .scan_in_progress = true;
        assert!(!generation_runtime_is_ready_report_only(&scanning, 7));
        let mut cleaning = report.clone();
        cleaning
            .daemon_status
            .as_mut()
            .expect("daemon status")
            .cleanup_in_progress = true;
        assert!(!generation_runtime_is_ready_report_only(&cleaning, 7));
        let mut failed = report.clone();
        let failed_daemon = failed.daemon_status.as_mut().expect("daemon status");
        failed_daemon.healthy = false;
        failed_daemon.ready = false;
        failed_daemon.startup_state = StartupState::Failed;
        assert!(!generation_runtime_is_ready_report_only(&failed, 7));
        assert!(generation_runtime_is_failed(&failed, 7));
        assert!(!generation_runtime_is_failed(&failed, 8));
        let mut failed_with_stale_enforce_projection = failed.clone();
        let failed_daemon = failed_with_stale_enforce_projection
            .daemon_status
            .as_mut()
            .expect("daemon status");
        failed_daemon.requested_mode = DaemonMode::Enforce;
        failed_daemon.effective_mode = DaemonMode::Enforce;
        failed_daemon.armed_generation = Some(7);
        failed_daemon.enforcement_epoch = Some("stale-epoch".to_owned());
        assert!(generation_runtime_is_failed(
            &failed_with_stale_enforce_projection,
            7
        ));
        let mut wrong_pid = report.clone();
        wrong_pid.launchd_pid = Some(43);
        assert!(!generation_runtime_is_ready_report_only(&wrong_pid, 7));
        let mut wrong_binary = report.clone();
        wrong_binary.binary_matches = false;
        assert!(!generation_runtime_is_ready_report_only(&wrong_binary, 7));
        let mut wrong_selection = report.clone();
        wrong_selection.generation_matches = false;
        assert!(!generation_runtime_is_ready_report_only(
            &wrong_selection,
            7
        ));
        let mut unsafe_permissions = report;
        unsafe_permissions.permissions_ok = false;
        assert!(!generation_runtime_is_ready_report_only(
            &unsafe_permissions,
            7
        ));
    }

    #[test]
    fn next_generation_ignores_incomplete_and_non_numeric_directories() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        let layout = ServiceLayout::new(&paths);
        fs::create_dir_all(layout.generations.join("2")).expect("generation two");
        fs::create_dir_all(layout.generations.join("9")).expect("generation nine");
        fs::create_dir_all(layout.generations.join(".10.stage-dead")).expect("staging");
        fs::create_dir_all(layout.generations.join("notes")).expect("unrelated directory");

        assert_eq!(next_generation(&layout).expect("next generation"), 10);
    }

    #[test]
    fn active_manifest_is_atomically_replaced_and_private() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        let layout = ServiceLayout::new(&paths);
        fs::create_dir_all(&paths.application_support).expect("application support");
        let first = ActiveServiceManifest::new(3, DaemonMode::ReportOnly);
        let second = ActiveServiceManifest::new(4, DaemonMode::Enforce);

        write_json_atomic(&layout.active_manifest, &first, 0o600).expect("write first manifest");
        write_json_atomic(&layout.active_manifest, &second, 0o600).expect("replace manifest");

        let restored: ActiveServiceManifest =
            read_json(&layout.active_manifest, "active service manifest").expect("read manifest");
        assert_eq!(restored, second);
        let metadata = fs::symlink_metadata(&layout.active_manifest).expect("manifest metadata");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            fs::read_dir(&paths.application_support)
                .expect("list application support")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains("stage"))
                .count(),
            0
        );
    }

    #[test]
    fn launchd_pid_parser_selects_the_service_pid() {
        let output = r#"
app.unlinger.daemon = {
    active count = 1
    pid = 4242
    last exit code = 0
}
"#;
        assert_eq!(parse_launchd_pid(output), Some(4242));
    }

    #[test]
    fn report_only_recovery_treats_unloaded_and_pidless_jobs_as_offline_not_safe() {
        assert_eq!(
            report_only_recovery_route(LaunchdState {
                loaded: true,
                pid: Some(42),
            }),
            ReportOnlyRecoveryRoute::Running(42)
        );
        assert_eq!(
            report_only_recovery_route(LaunchdState {
                loaded: true,
                pid: None,
            }),
            ReportOnlyRecoveryRoute::Offline
        );
        assert_eq!(
            report_only_recovery_route(LaunchdState {
                loaded: false,
                pid: None,
            }),
            ReportOnlyRecoveryRoute::Offline
        );
    }

    #[test]
    fn report_only_mode_change_always_selects_containment_while_enforce_requires_a_running_pid() {
        for state in [
            LaunchdState {
                loaded: true,
                pid: Some(42),
            },
            LaunchdState {
                loaded: true,
                pid: None,
            },
            LaunchdState {
                loaded: false,
                pid: None,
            },
        ] {
            assert_eq!(
                set_mode_route(DaemonMode::ReportOnly, state),
                SetModeRoute::ReportOnlyContainment
            );
        }
        assert_eq!(
            set_mode_route(
                DaemonMode::Enforce,
                LaunchdState {
                    loaded: true,
                    pid: Some(42),
                }
            ),
            SetModeRoute::EnforceOnline(42)
        );
        for state in [
            LaunchdState {
                loaded: true,
                pid: None,
            },
            LaunchdState {
                loaded: false,
                pid: None,
            },
        ] {
            assert_eq!(
                set_mode_route(DaemonMode::Enforce, state),
                SetModeRoute::RejectEnforce
            );
        }
    }

    #[test]
    fn report_only_manifest_is_published_only_after_containment_succeeds() {
        let contained = Cell::new(false);
        let published = Cell::new(false);
        let mut manifest = ActiveServiceManifest::new(7, DaemonMode::Enforce);

        recover_then_publish_report_only(
            &mut manifest,
            |generation| {
                assert_eq!(generation, 7);
                contained.set(true);
                Ok(())
            },
            |ready_manifest| {
                assert!(contained.get());
                assert_eq!(ready_manifest.desired_mode, DaemonMode::ReportOnly);
                published.set(true);
                Ok(())
            },
        )
        .expect("contain then publish report-only");
        assert!(published.get());
        assert_eq!(manifest.desired_mode, DaemonMode::ReportOnly);

        let attempted_publish = Cell::new(false);
        let mut failed_manifest = ActiveServiceManifest::new(8, DaemonMode::Enforce);
        let error = recover_then_publish_report_only(
            &mut failed_manifest,
            |_| Err(ServiceError::new("synthetic containment failure")),
            |_| {
                attempted_publish.set(true);
                Ok(())
            },
        )
        .expect_err("failed containment must block manifest publication");
        assert!(error.to_string().contains("synthetic containment failure"));
        assert!(!attempted_publish.get());
        assert_eq!(failed_manifest.desired_mode, DaemonMode::Enforce);
    }

    #[test]
    fn offline_recovery_requires_exclusive_daemon_lifetime_lock() {
        let temp = TempDirectory::new();
        let mut paths = LocalPaths::from_home(&temp.0).expect("paths");
        shorten_test_ipc_paths(&mut paths, &temp.0);
        prepare_install_directories(&paths).expect("prepare managed directories");
        let held = DaemonInstanceLock::acquire(&paths.daemon_lock).expect("hold daemon lock");

        assert!(
            wait_for_offline_daemon_lock(&paths.daemon_lock, Duration::ZERO).is_err(),
            "an active daemon lifetime lock must block offline database mutation"
        );
        drop(held);
        wait_for_offline_daemon_lock(&paths.daemon_lock, Duration::ZERO)
            .expect("offline recovery can own the released lifetime lock");
    }

    #[test]
    fn offline_proof_blocks_intent_mutation_then_removes_an_exact_safe_stale_socket() {
        let temp = TempDirectory::new();
        let mut paths = LocalPaths::from_home(&temp.0).expect("paths");
        shorten_test_ipc_paths(&mut paths, &temp.0);
        prepare_install_directories(&paths).expect("prepare managed directories");
        let layout = ServiceLayout::new(&paths);
        seed_carried_enforce(&paths, 7);

        let stale = UnixListener::bind(&paths.socket).expect("bind future stale socket");
        fs::set_permissions(&paths.socket, fs::Permissions::from_mode(0o600))
            .expect("protect stale socket");
        drop(stale);

        let held = DaemonInstanceLock::acquire(&paths.daemon_lock).expect("hold daemon lock");
        assert!(prove_daemon_offline(&paths, Duration::ZERO).is_err());
        assert!(
            paths.socket.exists(),
            "failed proof must not remove the socket"
        );
        let still_requested = HistoryStore::open(&paths.database)
            .expect("read blocked lifecycle")
            .managed_lifecycle()
            .expect("read lifecycle")
            .expect("lifecycle exists");
        assert!(still_requested.requested_enforce);

        drop(held);
        let offline = prove_daemon_offline(&paths, Duration::ZERO)
            .expect("released lock and stale socket prove daemon offline");
        assert!(!paths.socket.exists(), "stale socket must be removed");
        clear_generation_enforce_request_offline(&paths, &layout, 7, &offline)
            .expect("clear carried intent only after offline proof");
        drop(offline);

        let cleared = HistoryStore::open(&paths.database)
            .expect("read cleared lifecycle")
            .managed_lifecycle()
            .expect("read lifecycle")
            .expect("lifecycle exists");
        assert!(!cleared.requested_enforce);
        assert!(!cleared.effective_enforce);
    }

    #[test]
    fn offline_proof_rejects_a_live_listener() {
        let temp = TempDirectory::new();
        let mut paths = LocalPaths::from_home(&temp.0).expect("paths");
        shorten_test_ipc_paths(&mut paths, &temp.0);
        prepare_install_directories(&paths).expect("prepare managed directories");
        seed_carried_enforce(&paths, 7);
        let listener = UnixListener::bind(&paths.socket).expect("bind live listener");
        fs::set_permissions(&paths.socket, fs::Permissions::from_mode(0o600))
            .expect("protect live socket");

        let error = prove_daemon_offline(&paths, Duration::ZERO)
            .expect_err("a reachable listener must prevent offline mutation");

        assert!(error.to_string().contains("listener remains reachable"));
        assert!(paths.socket.exists());
        let lifecycle = HistoryStore::open(&paths.database)
            .expect("read lifecycle after refusal")
            .managed_lifecycle()
            .expect("read lifecycle")
            .expect("lifecycle exists");
        assert!(
            lifecycle.requested_enforce,
            "reachable listener must refuse before offline intent mutation"
        );
        drop(listener);
    }

    #[test]
    fn offline_proof_rejects_non_socket_and_symlink_entries() {
        use std::os::unix::fs::symlink;

        let temp = TempDirectory::new();
        let mut paths = LocalPaths::from_home(&temp.0).expect("paths");
        shorten_test_ipc_paths(&mut paths, &temp.0);
        prepare_install_directories(&paths).expect("prepare managed directories");
        seed_carried_enforce(&paths, 7);
        fs::write(&paths.socket, b"not a socket").expect("write unsafe IPC entry");
        fs::set_permissions(&paths.socket, fs::Permissions::from_mode(0o600))
            .expect("set regular entry mode");

        assert!(prove_daemon_offline(&paths, Duration::ZERO).is_err());
        assert_eq!(
            fs::read(&paths.socket).expect("regular entry remains"),
            b"not a socket"
        );
        assert!(
            HistoryStore::open(&paths.database)
                .expect("read lifecycle after regular-file refusal")
                .managed_lifecycle()
                .expect("read lifecycle")
                .expect("lifecycle exists")
                .requested_enforce
        );

        fs::remove_file(&paths.socket).expect("remove temp regular entry");
        let target = temp.0.join("private-target");
        fs::write(&target, b"preserve-me").expect("write symlink target");
        symlink(&target, &paths.socket).expect("create unsafe IPC symlink");

        assert!(prove_daemon_offline(&paths, Duration::ZERO).is_err());
        assert_eq!(fs::read(&target).expect("target remains"), b"preserve-me");
        assert!(
            HistoryStore::open(&paths.database)
                .expect("read lifecycle after symlink refusal")
                .managed_lifecycle()
                .expect("read lifecycle")
                .expect("lifecycle exists")
                .requested_enforce
        );
        assert!(
            fs::symlink_metadata(&paths.socket)
                .expect("symlink remains")
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn transaction_record_preserves_the_prior_report_only_recovery_material() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let prior = ActiveServiceManifest::new(8, DaemonMode::Enforce);
        let mut transaction = InstallTransaction::new(
            9,
            Some(prior.clone()),
            Some("<string>--enforce</string>".to_owned()),
            true,
        );
        transaction.phase = TransactionPhase::PriorDrained;

        persist_transaction(&layout, &transaction).expect("persist transaction");
        let restored = read_optional_transaction(&layout.transaction)
            .expect("read transaction")
            .expect("transaction");

        assert_eq!(restored, transaction);
        assert_eq!(
            restored.prior_manifest.expect("prior").rollback_floor(),
            ActiveServiceManifest::new(8, DaemonMode::ReportOnly)
        );
    }

    #[test]
    fn generation_publish_seals_versioned_binaries_and_manifest() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let source_daemon = temp.0.join("source-daemon");
        let source_cli = temp.0.join("source-cli");
        fs::write(&source_daemon, b"#!/bin/sh\necho 'unlingerd 0.1.0'\n").expect("daemon source");
        fs::write(&source_cli, b"#!/bin/sh\necho 'unlinger 0.1.0'\n").expect("cli source");
        fs::set_permissions(&source_daemon, fs::Permissions::from_mode(0o700))
            .expect("daemon executable");
        fs::set_permissions(&source_cli, fs::Permissions::from_mode(0o700))
            .expect("cli executable");
        let generation = layout.generation(1);

        create_generation(&layout, &generation, &source_cli, &source_daemon)
            .expect("publish generation");

        validate_generation(&generation).expect("validate generation");
        assert_eq!(
            fs::metadata(&generation.directory)
                .expect("directory")
                .permissions()
                .mode()
                & 0o777,
            0o500
        );
        assert_eq!(
            fs::metadata(&generation.daemon)
                .expect("daemon")
                .permissions()
                .mode()
                & 0o777,
            0o500
        );
        assert_eq!(
            fs::metadata(&generation.manifest)
                .expect("manifest")
                .permissions()
                .mode()
                & 0o777,
            0o400
        );
    }

    #[test]
    fn service_transaction_lock_rejects_concurrent_mutation_and_releases_on_drop() {
        let temp = TempDirectory::new();
        let path = temp.0.join("service.lock");
        let first = ServiceLock::acquire(&path).expect("first lock");
        assert!(ServiceLock::acquire(&path).is_err());
        drop(first);
        ServiceLock::acquire(&path).expect("lock after release");
    }

    #[test]
    fn atomic_replace_refuses_a_directory_at_a_managed_file_path() {
        let temp = TempDirectory::new();
        let destination = temp.0.join("daemon");
        fs::create_dir(&destination).expect("create unexpected directory");
        let error =
            write_bytes_atomic(&destination, b"candidate", 0o700).expect_err("refuse directory");

        assert!(
            error
                .to_string()
                .contains("managed destination must be absent or a regular file")
        );
        assert!(destination.is_dir());
    }

    #[test]
    fn atomic_replace_refuses_a_symlink_at_a_managed_file_path() {
        use std::os::unix::fs::symlink;

        let temp = TempDirectory::new();
        let target = temp.0.join("private-target");
        let destination = temp.0.join("service.json");
        fs::write(&target, b"preserve-me").expect("target");
        symlink(&target, &destination).expect("symlink");

        let error =
            write_bytes_atomic(&destination, b"replacement", 0o600).expect_err("refuse symlink");

        assert!(error.to_string().contains("absent or a regular file"));
        assert_eq!(fs::read(&target).expect("preserved target"), b"preserve-me");
        assert!(
            fs::symlink_metadata(&destination)
                .expect("symlink remains")
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn next_generation_rejects_a_numeric_symlink() {
        use std::os::unix::fs::symlink;

        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let outside = temp.0.join("outside");
        fs::create_dir(&outside).expect("outside");
        symlink(&outside, layout.generations.join("1")).expect("numeric symlink");

        let error = next_generation(&layout).expect_err("refuse numeric symlink");

        assert!(error.to_string().contains("not a directory"));
    }

    #[test]
    fn permission_report_rejects_a_world_readable_service_log() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("prepare directories");
        fs::write(&paths.error_log, b"diagnostic").expect("write log");
        fs::set_permissions(&paths.error_log, fs::Permissions::from_mode(0o644))
            .expect("set unsafe mode");
        let mut errors = Vec::new();

        let layout = ServiceLayout::new(&paths);
        assert!(!verify_permissions(
            &paths,
            None,
            &layout,
            current_uid(),
            &mut errors
        ));
        assert!(
            errors
                .iter()
                .any(|error| error.contains(&paths.error_log.display().to_string()))
        );
    }

    #[test]
    fn rollback_rewrites_legacy_enforce_and_never_adds_mode_to_managed_plist() {
        let paths = LocalPaths::from_home("/Users/example").expect("paths");
        let legacy = legacy_launch_agent_plist(&paths.daemon_binary, DaemonMode::Enforce);
        let legacy_rollback = report_only_rollback_plist(&legacy).expect("legacy rollback");
        assert!(!legacy_rollback.contains("--enforce"));
        assert!(legacy_rollback.contains("--report-only"));
        validate_plist_bytes(
            legacy_rollback.as_bytes(),
            &paths.daemon_binary,
            LaunchAgentExpectation::Legacy(DaemonMode::ReportOnly),
        )
        .expect("exact legacy rollback contract");

        let managed = launch_agent_plist(&paths, &ServiceLayout::new(&paths).generation(4));
        let managed_rollback = report_only_rollback_plist(&managed).expect("managed rollback");
        assert_eq!(managed_rollback, managed);
        assert!(!managed_rollback.contains("--enforce"));
        assert!(!managed_rollback.contains("--report-only"));
    }

    #[test]
    fn sqlite_backup_restores_prior_schema_and_preserves_failed_candidate_evidence() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let prior = rusqlite::Connection::open(&paths.database).expect("prior database");
        prior
            .execute_batch(
                "PRAGMA user_version = 2;
                 CREATE TABLE prior_marker(value TEXT NOT NULL);
                 INSERT INTO prior_marker VALUES ('prior');",
            )
            .expect("prior schema");
        drop(prior);

        assert!(backup_database(paths.database.as_path(), &layout).expect("backup"));

        let candidate = rusqlite::Connection::open(&paths.database).expect("candidate database");
        candidate
            .execute_batch(
                "PRAGMA user_version = 3;
                 CREATE TABLE candidate_marker(value TEXT NOT NULL);
                 INSERT INTO candidate_marker VALUES ('candidate');",
            )
            .expect("candidate schema");
        drop(candidate);
        let candidate_journal = database_sidecar(&paths.database, "-journal");
        fs::write(&candidate_journal, b"failed-candidate-journal")
            .expect("candidate rollback journal evidence");

        let evidence = restore_database_backup_files(&paths, &layout, 12)
            .expect("restore backup")
            .expect("failed candidate evidence");
        assert!(layout.database_backup.is_file());
        assert!(!candidate_journal.exists());
        assert_eq!(
            fs::read(evidence.join("history.sqlite3-journal")).expect("preserved journal"),
            b"failed-candidate-journal"
        );
        restore_database_backup_files(&paths, &layout, 12)
            .expect("repeatable restore after a crash boundary");

        let restored = rusqlite::Connection::open(&paths.database).expect("restored database");
        assert_eq!(
            restored
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("restored version"),
            2
        );
        assert_eq!(
            restored
                .query_row("SELECT value FROM prior_marker", [], |row| {
                    row.get::<_, String>(0)
                })
                .expect("prior marker"),
            "prior"
        );
        drop(restored);

        let failed = rusqlite::Connection::open(evidence.join("history.sqlite3"))
            .expect("failed evidence database");
        assert_eq!(
            failed
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("failed version"),
            3
        );
        assert_eq!(
            failed
                .query_row("SELECT value FROM candidate_marker", [], |row| {
                    row.get::<_, String>(0)
                })
                .expect("candidate marker"),
            "candidate"
        );
    }

    #[test]
    fn database_evidence_refuses_a_symlink_sidecar_before_moving_any_file() {
        use std::os::unix::fs::symlink;

        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        fs::write(&paths.database, b"candidate-db").expect("database");
        let target = temp.0.join("private-target");
        fs::write(&target, b"preserve-me").expect("target");
        let wal = database_sidecar(&paths.database, "-wal");
        symlink(&target, &wal).expect("wal symlink");

        let error = preserve_failed_database(&paths, &layout, 4).expect_err("refuse sidecar");

        assert!(error.to_string().contains("database component"));
        assert_eq!(
            fs::read(&paths.database).expect("database remains"),
            b"candidate-db"
        );
        assert_eq!(fs::read(&target).expect("target remains"), b"preserve-me");
    }

    #[test]
    fn database_backup_refuses_a_symlink_sidecar_before_opening_sqlite() {
        use std::os::unix::fs::symlink;

        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let database = rusqlite::Connection::open(&paths.database).expect("database");
        database
            .execute_batch("CREATE TABLE marker(value TEXT NOT NULL);")
            .expect("schema");
        drop(database);
        let target = temp.0.join("private-target");
        fs::write(&target, b"preserve-me").expect("target");
        symlink(&target, database_sidecar(&paths.database, "-journal"))
            .expect("rollback journal symlink");

        let error = backup_database(&paths.database, &layout).expect_err("refuse sidecar");

        assert!(error.to_string().contains("database component"));
        assert!(!layout.database_backup.exists());
        assert_eq!(fs::read(&target).expect("target remains"), b"preserve-me");
    }

    #[test]
    fn candidate_created_database_is_preserved_when_no_prior_database_existed() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let candidate = rusqlite::Connection::open(&paths.database).expect("candidate database");
        candidate
            .execute_batch(
                "PRAGMA user_version = 3;
                 CREATE TABLE candidate_only(value TEXT NOT NULL);",
            )
            .expect("candidate schema");
        drop(candidate);

        let evidence = preserve_failed_database(&paths, &layout, 22)
            .expect("preserve candidate")
            .expect("evidence path");

        assert!(!paths.database.exists());
        assert!(evidence.join("history.sqlite3").is_file());
    }

    #[test]
    fn database_backup_refuses_a_path_outside_the_managed_history_location() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let outside = temp.0.join("outside.sqlite3");
        drop(rusqlite::Connection::open(&outside).expect("outside database"));

        let error = backup_database(&outside, &layout).expect_err("refuse outside path");

        assert!(
            error
                .to_string()
                .contains("outside the managed history path")
        );
        assert!(outside.is_file());
    }

    #[test]
    fn acceptance_phases_hold_or_restore_until_acceptance_is_durable() {
        assert_eq!(
            transaction_recovery_disposition(TransactionPhase::Prepared),
            TransactionRecoveryDisposition::RollbackPrior
        );
        assert_eq!(
            transaction_recovery_disposition(TransactionPhase::CandidateSelected),
            TransactionRecoveryDisposition::RollbackPrior
        );
        assert_eq!(
            transaction_recovery_disposition(TransactionPhase::CandidateReadyReportOnly),
            TransactionRecoveryDisposition::HoldForExplicitDecision
        );
        assert_eq!(
            transaction_recovery_disposition(TransactionPhase::AcceptanceInProgress),
            TransactionRecoveryDisposition::RollbackPrior
        );
        assert_eq!(
            transaction_recovery_disposition(TransactionPhase::Accepted),
            TransactionRecoveryDisposition::FinalizeAccepted
        );
        assert!(!candidate_database_may_have_changed(
            TransactionPhase::DatabaseBackedUp
        ));
        assert!(candidate_database_may_have_changed(
            TransactionPhase::CandidateSelected
        ));
        assert!(candidate_database_may_have_changed(
            TransactionPhase::CandidateReadyReportOnly
        ));
        assert!(candidate_database_may_have_changed(
            TransactionPhase::AcceptanceInProgress
        ));
        assert!(!candidate_database_may_have_changed(
            TransactionPhase::Accepted
        ));
    }

    #[test]
    fn rollback_material_survives_ready_candidate_and_only_accepted_can_delete_it() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let prior = rusqlite::Connection::open(&paths.database).expect("prior database");
        prior
            .execute_batch("PRAGMA user_version = 5; CREATE TABLE prior_marker(value TEXT);")
            .expect("prior schema");
        drop(prior);
        assert!(backup_database(&paths.database, &layout).expect("backup"));

        let mut transaction = InstallTransaction::new(
            10,
            Some(ActiveServiceManifest::new(9, DaemonMode::ReportOnly)),
            Some("prior plist".to_owned()),
            true,
        );
        transaction.database_backed_up = true;
        transaction.phase = TransactionPhase::CandidateReadyReportOnly;
        persist_transaction(&layout, &transaction).expect("persist ready lease");

        let error = finalize_accepted_lease(&paths, &layout, &transaction)
            .expect_err("ready candidate cannot delete rollback material");
        assert!(error.to_string().contains("not durably accepted"));
        assert!(layout.transaction.is_file());
        assert!(layout.database_backup.is_file());

        transaction.phase = TransactionPhase::Accepted;
        persist_transaction(&layout, &transaction).expect("persist accepted");
        finalize_accepted_lease(&paths, &layout, &transaction).expect("finalize accepted lease");
        assert!(!layout.transaction.exists());
        assert!(!layout.database_backup.exists());
    }

    #[test]
    fn acceptance_status_requires_a_valid_database_and_prior_generation() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home(&temp.0).expect("paths");
        prepare_install_directories(&paths).expect("directories");
        let layout = ServiceLayout::new(&paths);
        let prior_generation = layout.generation(9);
        fs::create_dir(&prior_generation.directory).expect("prior generation directory");
        fs::write(&prior_generation.daemon, b"prior daemon").expect("prior daemon");
        fs::write(&prior_generation.cli, b"prior cli").expect("prior cli");
        write_json_atomic(
            &prior_generation.manifest,
            &GenerationManifest::new(9),
            0o600,
        )
        .expect("prior generation manifest");

        let prior = rusqlite::Connection::open(&paths.database).expect("prior database");
        prior
            .execute_batch("PRAGMA user_version = 5; CREATE TABLE prior_marker(value TEXT);")
            .expect("prior schema");
        drop(prior);
        assert!(backup_database(&paths.database, &layout).expect("backup"));

        let mut transaction = InstallTransaction::new(
            10,
            Some(ActiveServiceManifest::new(9, DaemonMode::ReportOnly)),
            Some(launch_agent_plist(&paths, &prior_generation)),
            true,
        );
        transaction.database_backed_up = true;
        transaction.phase = TransactionPhase::CandidateReadyReportOnly;
        persist_transaction(&layout, &transaction).expect("persist ready lease");

        let acceptance = inspect_acceptance_lease(&paths, &layout)
            .expect("inspect lease")
            .expect("acceptance report");
        assert!(acceptance.rollback_available);
        assert!(acceptance.database_backup_present);

        transaction.phase = TransactionPhase::Accepted;
        persist_transaction(&layout, &transaction).expect("persist accepted lease");
        let acceptance = inspect_acceptance_lease(&paths, &layout)
            .expect("inspect accepted lease")
            .expect("acceptance report");
        assert!(!acceptance.rollback_available);

        transaction.phase = TransactionPhase::CandidateReadyReportOnly;
        persist_transaction(&layout, &transaction).expect("restore ready lease");

        fs::write(&layout.database_backup, b"not sqlite").expect("corrupt backup");
        let acceptance = inspect_acceptance_lease(&paths, &layout)
            .expect("inspect corrupt lease")
            .expect("acceptance report");
        assert!(!acceptance.rollback_available);
    }

    #[test]
    fn candidate_acceptance_install_rejects_enforcement_before_mutation() {
        validate_acceptance_install_mode(DaemonMode::ReportOnly).expect("report-only accepted");
        let error = validate_acceptance_install_mode(DaemonMode::Enforce)
            .expect_err("enforcement install must be refused");
        assert!(error.to_string().contains("report-only"));
    }
}
