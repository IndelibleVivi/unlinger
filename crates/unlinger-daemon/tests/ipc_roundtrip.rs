#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use unlinger_daemon::{
    ControlPlane, DaemonMode, DaemonStatus, HistoryStore, IpcClient, IpcCommand, IpcPayload,
    IpcServer,
};

struct TempState {
    directory: PathBuf,
}

impl TempState {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "ul-{:x}-{:x}",
            std::process::id(),
            nonce & 0xffff_ffff
        ));
        fs::create_dir(&directory).expect("create temp directory");
        Self { directory }
    }
}

impl Drop for TempState {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn local_socket_serves_status_and_persists_pause_resume() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let control = ControlPlane::new(
        store.clone(),
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    );
    let server = IpcServer::start(&socket, control.clone()).expect("start IPC server");
    let client = IpcClient::new(&socket);

    let response = client.request(IpcCommand::Status).expect("status request");
    let IpcPayload::Status(status) = response else {
        panic!("expected status response");
    };
    assert_eq!(status.mode, DaemonMode::ReportOnly);
    assert_eq!(status.pid, std::process::id());

    let response = client
        .request(IpcCommand::Pause {
            duration_millis: 60_000,
        })
        .expect("pause request");
    let IpcPayload::Pause { until_unix_millis } = response else {
        panic!("expected pause response");
    };
    assert!(until_unix_millis > 60_000);
    assert_eq!(
        store.pause_until().expect("persisted pause"),
        Some(until_unix_millis)
    );

    assert!(matches!(
        client.request(IpcCommand::Resume).expect("resume request"),
        IpcPayload::Resumed
    ));
    assert_eq!(store.pause_until().expect("cleared pause"), None);

    let metadata = fs::metadata(&socket).expect("socket metadata");
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    drop(server);
    assert!(!socket.exists());
}

#[test]
fn history_limit_is_bounded_before_querying() {
    let temp = TempState::new();
    let store = HistoryStore::open(temp.directory.join("history.sqlite3")).expect("open store");
    let control = ControlPlane::new(
        store,
        DaemonStatus::new(DaemonMode::Enforce, std::process::id()),
    );
    let result = control.handle_at(IpcCommand::History { limit: 100_000 }, 1_000);
    assert!(result.is_err());
}
