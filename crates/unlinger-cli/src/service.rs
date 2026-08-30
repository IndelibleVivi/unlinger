use serde::Serialize;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
#[cfg(test)]
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unlinger_core::ProcessIdentity;
use unlinger_daemon::{
    DaemonMode, DaemonStatus, IpcClient, IpcCommand, IpcPayload, LAUNCH_AGENT_LABEL, LocalPaths,
};
use unlinger_macos::MacosSnapshotter;

const SERVICE_SCHEMA_VERSION: u32 = 1;
const SERVICE_START_TIMEOUT: Duration = Duration::from_secs(120);
const SERVICE_STOP_TIMEOUT: Duration = Duration::from_secs(125);
const SERVICE_POLL_INTERVAL: Duration = Duration::from_millis(200);
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
    pub launchd_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_status: Option<DaemonStatus>,
    pub pid_matches: bool,
    pub permissions_ok: bool,
    pub launch_agent_path: PathBuf,
    pub daemon_path: PathBuf,
    pub cli_path: PathBuf,
    pub database_path: PathBuf,
    pub socket_path: PathBuf,
    pub data_preserved: bool,
    pub errors: Vec<String>,
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

pub fn install(
    paths: &LocalPaths,
    source_cli: &Path,
    source_daemon: &Path,
    mode: DaemonMode,
) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;

    let old_launchd = launchd_state(uid)?;
    if !old_launchd.loaded && ipc_status(&paths.socket).is_some() {
        return Err(ServiceError::new(format!(
            "refusing to replace an unmanaged daemon listening at {}; stop that exact process first",
            paths.socket.display()
        )));
    }
    let old_mode =
        installed_mode(paths).or_else(|| ipc_status(&paths.socket).map(|status| status.mode));
    let old_identity = capture_loaded_identity(old_launchd)?;

    let mut replacements = vec![
        stage_copy(source_daemon, &paths.daemon_binary, 0o700)?,
        stage_copy(source_cli, &paths.cli_binary, 0o700)?,
    ];
    validate_binary(&replacements[0].staged, "unlingerd")?;
    validate_binary(&replacements[1].staged, "unlinger")?;
    let plist = launch_agent_plist(paths, mode);
    replacements.push(stage_bytes(plist.as_bytes(), &paths.launch_agent, 0o600)?);
    validate_plist(&replacements[2].staged)?;
    validate_managed_destinations(&replacements)?;

    if old_launchd.loaded {
        bootout_and_wait(uid, old_identity.as_ref())?;
    }

    let prior_was_loaded = old_launchd.loaded;
    activate_transaction(
        &mut replacements,
        || {
            bootstrap(uid, &paths.launch_agent)?;
            wait_for_healthy(paths, uid, mode, SERVICE_START_TIMEOUT).map(|_| ())
        },
        || stop_if_loaded(uid),
        || {
            if prior_was_loaded {
                let restored_mode = old_mode.ok_or_else(|| {
                    ServiceError::new("prior service mode was unavailable during rollback")
                })?;
                bootstrap(uid, &paths.launch_agent)?;
                wait_for_healthy(paths, uid, restored_mode, SERVICE_START_TIMEOUT).map(|_| ())?;
            }
            Ok(())
        },
    )?;

    wait_for_healthy(paths, uid, mode, SERVICE_START_TIMEOUT)
}

pub fn set_mode(paths: &LocalPaths, mode: DaemonMode) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    if !paths.launch_agent.is_file() || !paths.daemon_binary.is_file() {
        return Err(ServiceError::new(
            "Unlinger is not installed; run `unlinger service install` first",
        ));
    }
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let old_launchd = launchd_state(uid)?;
    if !old_launchd.loaded && ipc_status(&paths.socket).is_some() {
        return Err(ServiceError::new(format!(
            "refusing to change mode while an unmanaged daemon listens at {}",
            paths.socket.display()
        )));
    }
    let old_mode = installed_mode(paths).ok_or_else(|| {
        ServiceError::new("installed LaunchAgent does not declare a recognized daemon mode")
    })?;
    if old_launchd.loaded {
        let report = status(paths)?;
        if report.healthy && report.expected_mode == Some(mode) {
            return Ok(report);
        }
    }
    let old_identity = capture_loaded_identity(old_launchd)?;
    let plist = launch_agent_plist(paths, mode);
    let mut replacements = vec![stage_bytes(plist.as_bytes(), &paths.launch_agent, 0o600)?];
    validate_plist(&replacements[0].staged)?;
    validate_managed_destinations(&replacements)?;
    if old_launchd.loaded {
        bootout_and_wait(uid, old_identity.as_ref())?;
    }

    let prior_was_loaded = old_launchd.loaded;
    activate_transaction(
        &mut replacements,
        || {
            bootstrap(uid, &paths.launch_agent)?;
            wait_for_healthy(paths, uid, mode, SERVICE_START_TIMEOUT).map(|_| ())
        },
        || stop_if_loaded(uid),
        || {
            if prior_was_loaded {
                bootstrap(uid, &paths.launch_agent)?;
                wait_for_healthy(paths, uid, old_mode, SERVICE_START_TIMEOUT).map(|_| ())?;
            }
            Ok(())
        },
    )?;

    wait_for_healthy(paths, uid, mode, SERVICE_START_TIMEOUT)
}

pub fn uninstall(paths: &LocalPaths) -> Result<ServiceStatusReport, ServiceError> {
    let uid = mutation_uid()?;
    prepare_install_directories(paths)?;
    let _lock = ServiceLock::acquire(&paths.service_lock)?;
    let launchd = launchd_state(uid)?;
    if !launchd.loaded && ipc_status(&paths.socket).is_some() {
        return Err(ServiceError::new(format!(
            "refusing to uninstall while an unmanaged daemon listens at {}",
            paths.socket.display()
        )));
    }
    if launchd.loaded {
        let identity = capture_loaded_identity(launchd)?;
        bootout_and_wait(uid, identity.as_ref())?;
    }
    for path in [&paths.launch_agent, &paths.daemon_binary, &paths.cli_binary] {
        remove_managed_file(path)?;
    }
    status(paths)
}

pub fn status(paths: &LocalPaths) -> Result<ServiceStatusReport, ServiceError> {
    let uid = current_uid();
    let launchd = launchd_state(uid)?;
    let mut errors = Vec::new();
    let expected_mode = match installed_mode_result(paths) {
        Ok(mode) => mode,
        Err(error) => {
            errors.push(error.to_string());
            None
        }
    };
    let daemon_status = match ipc_status_result(&paths.socket) {
        Ok(status) => status,
        Err(error) => {
            errors.push(error.to_string());
            None
        }
    };
    let installed =
        paths.launch_agent.is_file() && paths.daemon_binary.is_file() && paths.cli_binary.is_file();
    let unmanaged_daemon = !launchd.loaded && daemon_status.is_some();
    let pid_matches = matches!(
        (launchd.pid, daemon_status.as_ref().map(|status| status.pid)),
        (Some(launchd_pid), Some(daemon_pid)) if launchd_pid == daemon_pid
    );
    let permissions_ok = verify_permissions(paths, uid, &mut errors);
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
    if let (Some(expected), Some(actual)) = (expected_mode, daemon_status.as_ref())
        && expected != actual.mode
    {
        errors.push("LaunchAgent mode and daemon runtime mode do not match".to_owned());
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
        && permissions_ok
        && expected_mode.is_some()
        && daemon_status
            .as_ref()
            .is_some_and(|status| status.healthy && status.last_scan_at_unix_millis.is_some())
        && daemon_status
            .as_ref()
            .is_some_and(|status| Some(status.mode) == expected_mode)
        && errors.is_empty();

    Ok(ServiceStatusReport {
        schema_version: SERVICE_SCHEMA_VERSION,
        label: LAUNCH_AGENT_LABEL,
        installed,
        loaded: launchd.loaded,
        healthy,
        unmanaged_daemon,
        expected_mode,
        launchd_pid: launchd.pid,
        daemon_status,
        pid_matches,
        permissions_ok,
        launch_agent_path: paths.launch_agent.clone(),
        daemon_path: paths.daemon_binary.clone(),
        cli_path: paths.cli_binary.clone(),
        database_path: paths.database.clone(),
        socket_path: paths.socket.clone(),
        data_preserved: true,
        errors,
    })
}

pub fn launch_agent_plist(paths: &LocalPaths, mode: DaemonMode) -> String {
    let mode_argument = match mode {
        DaemonMode::ReportOnly => "--report-only",
        DaemonMode::Enforce => "--enforce",
    };
    let program = xml_escape(&paths.daemon_binary.to_string_lossy());
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
    <string>{mode_argument}</string>
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
    for directory in [
        &paths.application_support,
        &paths.binary_directory,
        &paths.cache_directory,
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

fn installed_mode(paths: &LocalPaths) -> Option<DaemonMode> {
    installed_mode_result(paths).ok().flatten()
}

fn installed_mode_result(paths: &LocalPaths) -> Result<Option<DaemonMode>, ServiceError> {
    if !paths.launch_agent.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&paths.launch_agent)
        .map_err(|error| ServiceError::context("could not read installed LaunchAgent", error))?;
    mode_from_plist(&contents).map(Some)
}

fn mode_from_plist(contents: &str) -> Result<DaemonMode, ServiceError> {
    let enforce = contents.contains("<string>--enforce</string>");
    let report_only = contents.contains("<string>--report-only</string>");
    match (enforce, report_only) {
        (true, false) => Ok(DaemonMode::Enforce),
        (false, true) => Ok(DaemonMode::ReportOnly),
        _ => Err(ServiceError::new(
            "LaunchAgent must declare exactly one of --report-only or --enforce",
        )),
    }
}

fn ipc_status(path: &Path) -> Option<DaemonStatus> {
    ipc_status_result(path).ok().flatten()
}

fn ipc_status_result(path: &Path) -> Result<Option<DaemonStatus>, ServiceError> {
    if !path.exists() {
        return Ok(None);
    }
    match IpcClient::new(path).request(IpcCommand::Status) {
        Ok(IpcPayload::Status(status)) => Ok(Some(status)),
        Ok(_) => Err(ServiceError::new(
            "daemon returned the wrong IPC payload for service status",
        )),
        Err(error) => Err(ServiceError::context("daemon IPC status failed", error)),
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

fn stop_if_loaded(uid: u32) -> Result<(), ServiceError> {
    let state = launchd_state(uid)?;
    if !state.loaded {
        return Ok(());
    }
    let identity = capture_loaded_identity(state)?;
    bootout_and_wait(uid, identity.as_ref())
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

fn wait_for_healthy(
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

fn validate_plist(path: &Path) -> Result<(), ServiceError> {
    let output = Command::new("/usr/bin/plutil")
        .arg("-lint")
        .arg(path)
        .output()
        .map_err(|error| ServiceError::context("could not execute plutil", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ServiceError::new(format!(
            "staged LaunchAgent failed plutil validation: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn verify_permissions(paths: &LocalPaths, uid: u32, errors: &mut Vec<String>) -> bool {
    let mut ok = true;
    for (path, expected_mode, kind) in [
        (&paths.application_support, 0o700, "directory"),
        (&paths.binary_directory, 0o700, "directory"),
        (&paths.cache_directory, 0o700, "directory"),
        (&paths.logs_directory, 0o700, "directory"),
        (&paths.launch_agent, 0o600, "file"),
        (&paths.daemon_binary, 0o700, "file"),
        (&paths.cli_binary, 0o700, "file"),
        (&paths.service_lock, 0o600, "file"),
        (&paths.database, 0o600, "file"),
        (&paths.error_log, 0o600, "file"),
    ] {
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

fn validate_managed_destinations(replacements: &[Replacement]) -> Result<(), ServiceError> {
    for replacement in replacements {
        match fs::symlink_metadata(&replacement.destination) {
            Ok(metadata)
                if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(ServiceError::new(format!(
                    "managed destination must be absent or a regular file: {}",
                    replacement.destination.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ServiceError::context(
                    "could not inspect managed destination",
                    error,
                ));
            }
        }
    }
    Ok(())
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

struct Replacement {
    destination: PathBuf,
    staged: PathBuf,
    backup: Option<PathBuf>,
    promoted: bool,
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
            .open(path)
            .map_err(|error| {
                ServiceError::context("could not open service transaction lock", error)
            })?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
            ServiceError::context("could not protect service transaction lock", error)
        })?;
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

impl Drop for Replacement {
    fn drop(&mut self) {
        if !self.promoted {
            let _ = fs::remove_file(&self.staged);
        }
    }
}

fn stage_copy(source: &Path, destination: &Path, mode: u32) -> Result<Replacement, ServiceError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| ServiceError::context("could not inspect source binary", error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ServiceError::new(format!(
            "source binary must be a regular file: {}",
            source.display()
        )));
    }
    let staged = unique_peer_path(destination, "stage")?;
    let mut input = File::open(source)
        .map_err(|error| ServiceError::context("could not open source binary", error))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&staged)
        .map_err(|error| ServiceError::context("could not create staged binary", error))?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| ServiceError::context("could not copy staged binary", error))?;
    output
        .sync_all()
        .map_err(|error| ServiceError::context("could not sync staged binary", error))?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(mode))
        .map_err(|error| ServiceError::context("could not set staged binary mode", error))?;
    Ok(Replacement {
        destination: destination.to_path_buf(),
        staged,
        backup: None,
        promoted: false,
    })
}

fn stage_bytes(bytes: &[u8], destination: &Path, mode: u32) -> Result<Replacement, ServiceError> {
    let staged = unique_peer_path(destination, "stage")?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&staged)
        .map_err(|error| ServiceError::context("could not create staged file", error))?;
    output
        .write_all(bytes)
        .map_err(|error| ServiceError::context("could not write staged file", error))?;
    output
        .sync_all()
        .map_err(|error| ServiceError::context("could not sync staged file", error))?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(mode))
        .map_err(|error| ServiceError::context("could not set staged file mode", error))?;
    Ok(Replacement {
        destination: destination.to_path_buf(),
        staged,
        backup: None,
        promoted: false,
    })
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

fn activate_transaction(
    replacements: &mut [Replacement],
    mut activate: impl FnMut() -> Result<(), ServiceError>,
    mut deactivate: impl FnMut() -> Result<(), ServiceError>,
    mut recover: impl FnMut() -> Result<(), ServiceError>,
) -> Result<(), ServiceError> {
    if let Err(error) = promote_all(replacements) {
        let restore = restore_all(replacements);
        let recovery = recover();
        return Err(transaction_error(
            error,
            None,
            restore.err(),
            recovery.err(),
        ));
    }
    match activate() {
        Ok(()) => {
            commit_all(replacements)?;
            Ok(())
        }
        Err(error) => {
            let deactivation = deactivate();
            let restore = restore_all(replacements);
            let recovery = if deactivation.is_ok() && restore.is_ok() {
                recover()
            } else {
                Err(ServiceError::new(
                    "prior service was not restarted because candidate deactivation or file restore failed",
                ))
            };
            Err(transaction_error(
                error,
                deactivation.err(),
                restore.err(),
                recovery.err(),
            ))
        }
    }
}

fn promote_all(replacements: &mut [Replacement]) -> Result<(), ServiceError> {
    for replacement in replacements {
        if replacement.destination.exists() {
            let backup = unique_peer_path(&replacement.destination, "rollback")?;
            fs::rename(&replacement.destination, &backup)
                .map_err(|error| ServiceError::context("could not preserve prior file", error))?;
            replacement.backup = Some(backup);
        }
        if let Err(error) = fs::rename(&replacement.staged, &replacement.destination) {
            if let Some(backup) = replacement.backup.take() {
                let _ = fs::rename(backup, &replacement.destination);
            }
            return Err(ServiceError::context(
                "could not promote staged service file",
                error,
            ));
        }
        replacement.promoted = true;
    }
    Ok(())
}

fn restore_all(replacements: &mut [Replacement]) -> Result<(), ServiceError> {
    let mut errors = Vec::new();
    for replacement in replacements.iter_mut().rev() {
        if replacement.promoted {
            if let Err(error) = fs::remove_file(&replacement.destination)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                errors.push(error.to_string());
            }
            replacement.promoted = false;
        }
        if let Some(backup) = replacement.backup.take()
            && let Err(error) = fs::rename(&backup, &replacement.destination)
        {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ServiceError::new(format!(
            "file rollback failed: {}",
            errors.join("; ")
        )))
    }
}

fn commit_all(replacements: &mut [Replacement]) -> Result<(), ServiceError> {
    let mut errors = Vec::new();
    for replacement in replacements {
        if let Some(backup) = replacement.backup.take()
            && let Err(error) = fs::remove_file(backup)
        {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ServiceError::new(format!(
            "service is active but prior-file cleanup failed: {}",
            errors.join("; ")
        )))
    }
}

fn transaction_error(
    activation: ServiceError,
    deactivation: Option<ServiceError>,
    restore: Option<ServiceError>,
    recovery: Option<ServiceError>,
) -> ServiceError {
    let mut message = format!("candidate activation failed: {activation}");
    if deactivation.is_none() && restore.is_none() && recovery.is_none() {
        message.push_str("; prior files and service were restored");
    } else {
        for (label, error) in [
            ("candidate deactivation", deactivation),
            ("file restore", restore),
            ("prior service recovery", recovery),
        ] {
            if let Some(error) = error {
                message.push_str(&format!("; {label} failed: {error}"));
            }
        }
    }
    ServiceError::new(message)
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
    use std::cell::RefCell;
    use std::rc::Rc;

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ul-service-{}-{}",
                std::process::id(),
                NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create temp directory");
            Self(path)
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn plist_is_explicit_private_and_escapes_local_paths() {
        let paths = LocalPaths::from_home("/Users/A&B Person").expect("paths");
        let plist = launch_agent_plist(&paths, DaemonMode::Enforce);
        assert!(plist.contains("<string>app.unlinger.daemon</string>"));
        assert!(plist.contains("A&amp;B Person/Library/Application Support"));
        assert!(plist.contains("<string>--enforce</string>"));
        assert!(plist.contains("<key>KeepAlive</key>\n  <true/>"));
        assert!(plist.contains("<key>ProcessType</key>\n  <string>Background</string>"));
        assert!(plist.contains("<key>ExitTimeOut</key>\n  <integer>120</integer>"));
        assert!(plist.contains("<key>Umask</key>\n  <string>077</string>"));
        assert!(!plist.contains("RunAtLoad"));
        assert_eq!(mode_from_plist(&plist).expect("mode"), DaemonMode::Enforce);
    }

    #[test]
    fn generated_plist_passes_the_host_plutil_parser() {
        let temp = TempDirectory::new();
        let paths = LocalPaths::from_home("/Users/example").expect("paths");
        let plist_path = temp.0.join("app.unlinger.daemon.plist");
        fs::write(
            &plist_path,
            launch_agent_plist(&paths, DaemonMode::ReportOnly),
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
    fn activation_failure_restores_every_prior_file_before_recovery() {
        let temp = TempDirectory::new();
        let first = temp.0.join("first");
        let second = temp.0.join("second");
        fs::write(&first, b"old-first").expect("write first");
        fs::write(&second, b"old-second").expect("write second");
        let mut replacements = vec![
            stage_bytes(b"new-first", &first, 0o600).expect("stage first"),
            stage_bytes(b"new-second", &second, 0o600).expect("stage second"),
        ];
        let events = Rc::new(RefCell::new(Vec::new()));
        let activate_events = Rc::clone(&events);
        let deactivate_events = Rc::clone(&events);
        let recover_events = Rc::clone(&events);

        let result = activate_transaction(
            &mut replacements,
            move || {
                activate_events.borrow_mut().push("activate");
                Err(ServiceError::new("synthetic bootstrap failure"))
            },
            move || {
                deactivate_events.borrow_mut().push("deactivate");
                Ok(())
            },
            move || {
                recover_events.borrow_mut().push("recover");
                assert_eq!(fs::read(&first).expect("restored first"), b"old-first");
                assert_eq!(fs::read(&second).expect("restored second"), b"old-second");
                Ok(())
            },
        );

        assert!(result.is_err());
        assert_eq!(&*events.borrow(), &["activate", "deactivate", "recover"]);
    }

    #[test]
    fn successful_activation_commits_new_files_and_removes_backups() {
        let temp = TempDirectory::new();
        let destination = temp.0.join("daemon");
        fs::write(&destination, b"old").expect("write old");
        let mut replacements =
            vec![stage_bytes(b"new", &destination, 0o700).expect("stage replacement")];

        activate_transaction(
            &mut replacements,
            || Ok(()),
            || panic!("successful activation must not deactivate"),
            || panic!("successful activation must not recover"),
        )
        .expect("activate");

        let mut contents = Vec::new();
        File::open(&destination)
            .expect("open destination")
            .read_to_end(&mut contents)
            .expect("read destination");
        assert_eq!(contents, b"new");
        assert_eq!(
            fs::read_dir(&temp.0).expect("list directory").count(),
            1,
            "rollback backup should be removed after health verification"
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
    fn replacement_refuses_a_directory_at_a_managed_file_path() {
        let temp = TempDirectory::new();
        let destination = temp.0.join("daemon");
        fs::create_dir(&destination).expect("create unexpected directory");
        let replacement = stage_bytes(b"candidate", &destination, 0o700).expect("stage candidate");

        let error = validate_managed_destinations(&[replacement]).expect_err("refuse directory");

        assert!(
            error
                .to_string()
                .contains("managed destination must be absent or a regular file")
        );
        assert!(destination.is_dir());
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

        assert!(!verify_permissions(&paths, current_uid(), &mut errors));
        assert!(
            errors
                .iter()
                .any(|error| error.contains(&paths.error_log.display().to_string()))
        );
    }
}
