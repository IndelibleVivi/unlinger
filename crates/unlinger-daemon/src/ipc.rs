use crate::{EventPayload, HistoryEvent, HistoryStore, IncidentDetail, StoreError};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

const IPC_SCHEMA_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_HISTORY_LIMIT: usize = 1_000;
const MAX_PAUSE_MILLIS: u64 = 30 * 24 * 60 * 60 * 1_000;
const MAX_ERROR_CHARS: usize = 512;

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonMode {
    ReportOnly,
    Enforce,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentReclaim {
    pub incident_id: String,
    pub occurred_at_unix_millis: u64,
    pub state: unlinger_core::IncidentState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DaemonStatus {
    pub healthy: bool,
    pub mode: DaemonMode,
    pub pid: u32,
    pub scan_in_progress: bool,
    pub cleanup_in_progress: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_until_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan_at_unix_millis: Option<u64>,
    pub confirmed_incidents: usize,
    pub ambiguous_incidents: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most_recent_reclaim: Option<RecentReclaim>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl DaemonStatus {
    #[must_use]
    pub fn new(mode: DaemonMode, pid: u32) -> Self {
        Self {
            healthy: true,
            mode,
            pid,
            scan_in_progress: false,
            cleanup_in_progress: false,
            paused_until_unix_millis: None,
            last_scan_at_unix_millis: None,
            confirmed_incidents: 0,
            ambiguous_incidents: 0,
            most_recent_reclaim: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum IpcCommand {
    Status,
    History { limit: usize },
    Explain { incident_id: String },
    Pause { duration_millis: u64 },
    Resume,
    ExportDiagnostics { incident_id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticsBundle {
    pub schema_version: u32,
    pub generated_at_unix_millis: u64,
    pub status: DaemonStatus,
    pub incident: IncidentDetail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum IpcPayload {
    Status(DaemonStatus),
    History(Vec<HistoryEvent>),
    Incident(IncidentDetail),
    Pause { until_unix_millis: u64 },
    Resumed,
    Diagnostics(DiagnosticsBundle),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlError {
    InvalidArgument(String),
    NotFound(String),
    Store(String),
    Unavailable(String),
}

impl ControlError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "invalid_argument",
            Self::NotFound(_) => "not_found",
            Self::Store(_) => "store_error",
            Self::Unavailable(_) => "unavailable",
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::InvalidArgument(message)
            | Self::NotFound(message)
            | Self::Store(message)
            | Self::Unavailable(message) => message,
        }
    }
}

impl Display for ControlError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl Error for ControlError {}

impl From<StoreError> for ControlError {
    fn from(value: StoreError) -> Self {
        Self::Store(bounded_message(&value.to_string()))
    }
}

#[derive(Clone)]
pub struct ControlPlane {
    store: HistoryStore,
    status: Arc<Mutex<DaemonStatus>>,
}

impl ControlPlane {
    #[must_use]
    pub fn new(store: HistoryStore, mut status: DaemonStatus) -> Self {
        match (store.pause_until(), store.history(1_000)) {
            (Ok(deadline), Ok(history)) => {
                status.paused_until_unix_millis = deadline;
                status.most_recent_reclaim = history.into_iter().find_map(|event| {
                    let EventPayload::Cleanup { receipt } = event.payload else {
                        return None;
                    };
                    (receipt.state == unlinger_core::IncidentState::Cleared).then_some(
                        RecentReclaim {
                            incident_id: receipt.incident_id,
                            occurred_at_unix_millis: event.occurred_at_unix_millis,
                            state: receipt.state,
                        },
                    )
                });
            }
            (Err(error), _) | (_, Err(error)) => {
                status.healthy = false;
                status.last_error = Some(bounded_message(&format!(
                    "could not restore daemon state: {error}"
                )));
            }
        }
        Self {
            store,
            status: Arc::new(Mutex::new(status)),
        }
    }

    #[must_use]
    pub fn store(&self) -> &HistoryStore {
        &self.store
    }

    pub fn status(&self) -> Result<DaemonStatus, ControlError> {
        self.status
            .lock()
            .map(|status| status.clone())
            .map_err(|_| ControlError::Unavailable("daemon status lock is poisoned".to_owned()))
    }

    pub fn status_at(&self, now_unix_millis: u64) -> Result<DaemonStatus, ControlError> {
        self.expire_pause(now_unix_millis)?;
        self.status()
    }

    pub fn update_status(
        &self,
        update: impl FnOnce(&mut DaemonStatus),
    ) -> Result<(), ControlError> {
        let mut status = self
            .status
            .lock()
            .map_err(|_| ControlError::Unavailable("daemon status lock is poisoned".to_owned()))?;
        update(&mut status);
        if let Some(error) = &mut status.last_error {
            *error = bounded_message(error);
        }
        Ok(())
    }

    pub fn handle_at(
        &self,
        command: IpcCommand,
        now_unix_millis: u64,
    ) -> Result<IpcPayload, ControlError> {
        match command {
            IpcCommand::Status => Ok(IpcPayload::Status(self.status_at(now_unix_millis)?)),
            IpcCommand::History { limit } => {
                if limit > MAX_HISTORY_LIMIT {
                    return Err(ControlError::InvalidArgument(format!(
                        "history limit must not exceed {MAX_HISTORY_LIMIT}"
                    )));
                }
                Ok(IpcPayload::History(self.store.history(limit)?))
            }
            IpcCommand::Explain { incident_id } => {
                validate_incident_id(&incident_id)?;
                let detail = self.store.explain(&incident_id)?.ok_or_else(|| {
                    ControlError::NotFound(format!("incident {incident_id:?} was not found"))
                })?;
                Ok(IpcPayload::Incident(detail))
            }
            IpcCommand::Pause { duration_millis } => {
                if duration_millis == 0 || duration_millis > MAX_PAUSE_MILLIS {
                    return Err(ControlError::InvalidArgument(format!(
                        "pause duration must be between 1 millisecond and {MAX_PAUSE_MILLIS} milliseconds"
                    )));
                }
                let deadline = now_unix_millis
                    .checked_add(duration_millis)
                    .ok_or_else(|| {
                        ControlError::InvalidArgument("pause deadline overflowed u64".to_owned())
                    })?;
                self.store.set_pause_until(Some(deadline))?;
                self.update_status(|status| {
                    status.paused_until_unix_millis = Some(deadline);
                })?;
                Ok(IpcPayload::Pause {
                    until_unix_millis: deadline,
                })
            }
            IpcCommand::Resume => {
                self.store.set_pause_until(None)?;
                self.update_status(|status| status.paused_until_unix_millis = None)?;
                Ok(IpcPayload::Resumed)
            }
            IpcCommand::ExportDiagnostics { incident_id } => {
                validate_incident_id(&incident_id)?;
                let incident = self.store.explain(&incident_id)?.ok_or_else(|| {
                    ControlError::NotFound(format!("incident {incident_id:?} was not found"))
                })?;
                Ok(IpcPayload::Diagnostics(DiagnosticsBundle {
                    schema_version: 1,
                    generated_at_unix_millis: now_unix_millis,
                    status: self.status()?,
                    incident,
                }))
            }
        }
    }

    fn expire_pause(&self, now_unix_millis: u64) -> Result<(), ControlError> {
        let expired = self
            .status()?
            .paused_until_unix_millis
            .is_some_and(|deadline| deadline <= now_unix_millis);
        if expired {
            self.store.set_pause_until(None)?;
            self.update_status(|status| status.paused_until_unix_millis = None)?;
        }
        Ok(())
    }
}

fn validate_incident_id(incident_id: &str) -> Result<(), ControlError> {
    if incident_id.is_empty() || incident_id.len() > 128 {
        Err(ControlError::InvalidArgument(
            "incident ID must contain 1 to 128 bytes".to_owned(),
        ))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RequestEnvelope {
    schema_version: u32,
    request_id: u64,
    command: IpcCommand,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResponseEnvelope {
    schema_version: u32,
    request_id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<IpcPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<IpcErrorBody>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IpcErrorBody {
    code: String,
    message: String,
}

#[derive(Debug)]
pub enum IpcError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Protocol(String),
    Remote { code: String, message: String },
}

impl Display for IpcError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local IPC I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "local IPC JSON failed: {error}"),
            Self::Protocol(message) => write!(formatter, "local IPC protocol failed: {message}"),
            Self::Remote { code, message } => {
                write!(formatter, "daemon rejected {code}: {message}")
            }
        }
    }
}

impl Error for IpcError {}

impl From<std::io::Error> for IpcError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Clone, Debug)]
pub struct IpcClient {
    socket_path: PathBuf,
}

impl IpcClient {
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: path.as_ref().to_path_buf(),
        }
    }

    #[cfg(unix)]
    pub fn request(&self, command: IpcCommand) -> Result<IpcPayload, IpcError> {
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = RequestEnvelope {
            schema_version: IPC_SCHEMA_VERSION,
            request_id,
            command,
        };
        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        serde_json::to_writer(&mut stream, &request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        let response_bytes = read_bounded_line(&mut stream, MAX_RESPONSE_BYTES)?;
        let response = serde_json::from_slice::<ResponseEnvelope>(&response_bytes)?;
        if response.schema_version != IPC_SCHEMA_VERSION || response.request_id != request_id {
            return Err(IpcError::Protocol(
                "response schema or request ID did not match".to_owned(),
            ));
        }
        match (response.ok, response.payload, response.error) {
            (true, Some(payload), None) => Ok(payload),
            (false, None, Some(error)) => Err(IpcError::Remote {
                code: error.code,
                message: error.message,
            }),
            _ => Err(IpcError::Protocol(
                "response success/error fields were contradictory".to_owned(),
            )),
        }
    }
}

pub struct IpcServer {
    socket_path: PathBuf,
    socket_device: u64,
    socket_inode: u64,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl IpcServer {
    #[cfg(unix)]
    pub fn start(path: impl AsRef<Path>, control: ControlPlane) -> Result<Self, IpcError> {
        let path = path.as_ref().to_path_buf();
        prepare_socket_parent(&path)?;
        let listener = bind_local_socket(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let metadata = fs::metadata(&path)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("unlinger-ipc".to_owned())
            .spawn(move || {
                while !thread_shutdown.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if thread_shutdown.load(Ordering::Acquire) {
                                break;
                            }
                            if stream
                                .set_read_timeout(Some(Duration::from_secs(3)))
                                .is_err()
                                || stream
                                    .set_write_timeout(Some(Duration::from_secs(3)))
                                    .is_err()
                            {
                                continue;
                            }
                            let _ = serve_connection(&mut stream, &control);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
            })?;

        Ok(Self {
            socket_path: path,
            socket_device: metadata.dev(),
            socket_inode: metadata.ino(),
            shutdown,
            thread: Some(thread),
        })
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        #[cfg(unix)]
        {
            let _ = UnixStream::connect(&self.socket_path);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(&self.socket_path)
            && metadata.file_type().is_socket()
            && metadata.dev() == self.socket_device
            && metadata.ino() == self.socket_inode
        {
            let _ = fs::remove_file(&self.socket_path);
        }
    }
}

#[cfg(unix)]
fn prepare_socket_parent(path: &Path) -> Result<(), IpcError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| IpcError::Protocol("socket path has no parent directory".to_owned()))?;
    let existed = parent.exists();
    fs::create_dir_all(parent)?;
    if !existed {
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(unix)]
fn bind_local_socket(path: &Path) -> Result<UnixListener, IpcError> {
    match UnixListener::bind(path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            let metadata = fs::metadata(path)?;
            if !metadata.file_type().is_socket() {
                return Err(IpcError::Protocol(format!(
                    "refusing to replace non-socket path {}",
                    path.display()
                )));
            }
            if UnixStream::connect(path).is_ok() {
                return Err(IpcError::Protocol(format!(
                    "another daemon is already listening at {}",
                    path.display()
                )));
            }
            fs::remove_file(path)?;
            UnixListener::bind(path).map_err(IpcError::Io)
        }
        Err(error) => Err(IpcError::Io(error)),
    }
}

#[cfg(unix)]
fn serve_connection(stream: &mut UnixStream, control: &ControlPlane) -> Result<(), IpcError> {
    let request_bytes = match read_bounded_line(stream, MAX_REQUEST_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            write_response(
                stream,
                &ResponseEnvelope {
                    schema_version: IPC_SCHEMA_VERSION,
                    request_id: 0,
                    ok: false,
                    payload: None,
                    error: Some(IpcErrorBody {
                        code: "invalid_request".to_owned(),
                        message: bounded_message(&error.to_string()),
                    }),
                },
            )?;
            return Ok(());
        }
    };
    let request = match serde_json::from_slice::<RequestEnvelope>(&request_bytes) {
        Ok(request) => request,
        Err(error) => {
            write_response(
                stream,
                &ResponseEnvelope {
                    schema_version: IPC_SCHEMA_VERSION,
                    request_id: 0,
                    ok: false,
                    payload: None,
                    error: Some(IpcErrorBody {
                        code: "invalid_json".to_owned(),
                        message: bounded_message(&error.to_string()),
                    }),
                },
            )?;
            return Ok(());
        }
    };
    let response = if request.schema_version != IPC_SCHEMA_VERSION {
        ResponseEnvelope {
            schema_version: IPC_SCHEMA_VERSION,
            request_id: request.request_id,
            ok: false,
            payload: None,
            error: Some(IpcErrorBody {
                code: "unsupported_schema".to_owned(),
                message: format!("supported IPC schema is {IPC_SCHEMA_VERSION}"),
            }),
        }
    } else {
        match control.handle_at(request.command, now_unix_millis()?) {
            Ok(payload) => ResponseEnvelope {
                schema_version: IPC_SCHEMA_VERSION,
                request_id: request.request_id,
                ok: true,
                payload: Some(payload),
                error: None,
            },
            Err(error) => ResponseEnvelope {
                schema_version: IPC_SCHEMA_VERSION,
                request_id: request.request_id,
                ok: false,
                payload: None,
                error: Some(IpcErrorBody {
                    code: error.code().to_owned(),
                    message: bounded_message(error.message()),
                }),
            },
        }
    };
    write_response(stream, &response)
}

fn write_response(stream: &mut impl Write, response: &ResponseEnvelope) -> Result<(), IpcError> {
    serde_json::to_writer(&mut *stream, response)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn read_bounded_line(reader: &mut impl Read, max_bytes: u64) -> Result<Vec<u8>, IpcError> {
    let mut buffered = BufReader::new(reader.take(max_bytes + 1));
    let mut bytes = Vec::new();
    buffered.read_until(b'\n', &mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(IpcError::Protocol(format!(
            "message exceeds {max_bytes} byte limit"
        )));
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.is_empty() {
        return Err(IpcError::Protocol("empty IPC message".to_owned()));
    }
    Ok(bytes)
}

fn now_unix_millis() -> Result<u64, IpcError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| IpcError::Protocol(format!("wall clock failed: {error}")))?
        .as_millis();
    u64::try_from(millis).map_err(|_| IpcError::Protocol("wall clock overflowed u64".to_owned()))
}

fn bounded_message(message: &str) -> String {
    message.chars().take(MAX_ERROR_CHARS).collect()
}
