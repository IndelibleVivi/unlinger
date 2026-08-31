#![cfg(unix)]

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unlinger_daemon::{
    DaemonMode, DaemonStatus, HistoryStore, IpcClient, IpcCommand, IpcPayload, ManagedStartupPhase,
    StartupState,
};

struct TempState {
    directory: PathBuf,
}

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

impl TempState {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "ul-s-{}-{nonce:x}-{sequence:x}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create temp directory");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("protect temp directory");
        Self { directory }
    }
}

impl Drop for TempState {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn spawn_managed_child(
    database: &std::path::Path,
    socket: &std::path::Path,
    instance_lock: &std::path::Path,
    generation: u64,
) -> ChildGuard {
    ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_unlingerd"))
            .arg("--managed")
            .arg("--activation-generation")
            .arg(generation.to_string())
            .arg("--interval-seconds")
            .arg("3600")
            .arg("--observe-seconds")
            .arg("0")
            .arg("--database")
            .arg(database)
            .arg("--socket")
            .arg(socket)
            .arg("--instance-lock")
            .arg(instance_lock)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn managed daemon child"),
    )
}

fn wait_for_managed_status(
    socket: &std::path::Path,
    predicate: impl Fn(&DaemonStatus) -> bool,
) -> DaemonStatus {
    let started = Instant::now();
    loop {
        if socket.exists()
            && let Ok(IpcPayload::Status(status)) =
                IpcClient::new(socket).request(IpcCommand::Status)
            && predicate(&status)
        {
            return status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "managed daemon did not reach the expected lifecycle state"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn terminate_owned_child(child: &mut ChildGuard, socket: &std::path::Path) {
    let signal_status = Command::new("/bin/kill")
        .arg("-TERM")
        .arg(child.0.id().to_string())
        .status()
        .expect("signal owned managed daemon child");
    assert!(signal_status.success());
    let stopping = Instant::now();
    let exit = loop {
        if let Some(status) = child.0.try_wait().expect("poll managed child") {
            break status;
        }
        assert!(
            stopping.elapsed() < Duration::from_secs(5),
            "managed daemon did not stop promptly after SIGTERM"
        );
        thread::sleep(Duration::from_millis(20));
    };
    if !exit.success() {
        let mut stderr = String::new();
        if let Some(stream) = child.0.stderr.as_mut() {
            stream
                .read_to_string(&mut stderr)
                .expect("read failed managed daemon stderr");
        }
        panic!("SIGTERM should produce a clean daemon exit; status={exit}; stderr={stderr}");
    }
    assert!(!socket.exists(), "managed daemon should remove its socket");
}

#[test]
fn sigterm_finishes_the_cycle_and_removes_the_owned_socket() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let instance_lock = fs::canonicalize(&temp.directory)
        .expect("canonical temp directory")
        .join("unlingerd.instance.lock");
    let child = Command::new(env!("CARGO_BIN_EXE_unlingerd"))
        .arg("--report-only")
        .arg("--interval-seconds")
        .arg("60")
        .arg("--observe-seconds")
        .arg("0")
        .arg("--database")
        .arg(&database)
        .arg("--socket")
        .arg(&socket)
        .arg("--instance-lock")
        .arg(&instance_lock)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn owned daemon child");
    let mut child = ChildGuard(child);

    let started = Instant::now();
    while !socket.exists() && started.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(20));
    }
    if !socket.exists() {
        let status = child.0.try_wait().expect("inspect failed daemon child");
        let mut stderr = String::new();
        if let Some(stream) = child.0.stderr.as_mut() {
            stream
                .read_to_string(&mut stderr)
                .expect("read daemon stderr");
        }
        panic!("daemon did not create its IPC socket; status={status:?}; stderr={stderr}");
    }

    let signal_status = Command::new("/bin/kill")
        .arg("-TERM")
        .arg(child.0.id().to_string())
        .status()
        .expect("signal owned daemon child");
    assert!(signal_status.success());

    let stopping = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("poll daemon child") {
            break status;
        }
        assert!(
            stopping.elapsed() < Duration::from_secs(5),
            "daemon did not stop promptly after SIGTERM"
        );
        thread::sleep(Duration::from_millis(20));
    };

    assert!(
        status.success(),
        "SIGTERM should produce a clean daemon exit"
    );
    assert!(
        !socket.exists(),
        "graceful shutdown should remove the exact owned socket"
    );
}

#[test]
fn managed_begin_drain_is_durable_and_stops_the_exact_daemon() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let instance_lock = fs::canonicalize(&temp.directory)
        .expect("canonical temp directory")
        .join("unlingerd.instance.lock");
    let mut child = spawn_managed_child(&database, &socket, &instance_lock, 17);
    let client = IpcClient::new(&socket);
    let status = wait_for_managed_status(&socket, |status| {
        status.activation_generation == Some(17)
            && status.startup_state == StartupState::ReadyReportOnly
    });
    let IpcPayload::Pause { until_unix_millis } = client
        .request(IpcCommand::Pause {
            duration_millis: 3_600_000,
        })
        .expect("pause ambient cleanup before lifecycle-only Arm test")
    else {
        panic!("expected pause payload")
    };
    assert!(until_unix_millis > 3_600_000);
    let IpcPayload::Lifecycle(armed) = client
        .request(IpcCommand::Arm {
            activation_generation: 17,
            instance_id: status.instance_id.clone(),
        })
        .expect("arm exact managed daemon")
    else {
        panic!("expected lifecycle payload")
    };
    assert_eq!(armed.effective_mode(), DaemonMode::Enforce);
    assert!(armed.requested_mode == DaemonMode::Enforce);

    let IpcPayload::Lifecycle(draining) = client
        .request(IpcCommand::BeginDrain {
            activation_generation: 17,
            instance_id: status.instance_id,
        })
        .expect("begin exact managed drain")
    else {
        panic!("expected lifecycle payload")
    };
    assert!(draining.draining);
    assert_eq!(draining.startup_state, StartupState::Draining);

    let stopping = Instant::now();
    let exit = loop {
        if let Some(status) = child.0.try_wait().expect("poll managed child") {
            break status;
        }
        assert!(
            stopping.elapsed() < Duration::from_secs(5),
            "managed daemon did not stop promptly after drain ACK"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(exit.success());
    assert!(!socket.exists());

    let store = HistoryStore::open(&database).expect("reopen managed store");
    let lifecycle = store
        .managed_lifecycle()
        .expect("read lifecycle")
        .expect("managed lifecycle exists");
    assert_eq!(lifecycle.activation_generation, 17);
    assert!(lifecycle.draining);
    assert!(!lifecycle.requested_enforce);
    assert!(!lifecycle.effective_enforce);
}

#[test]
fn raw_sigterm_preserves_same_generation_intent_and_restart_rearms_with_fresh_identity() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let instance_lock = fs::canonicalize(&temp.directory)
        .expect("canonical temp directory")
        .join("unlingerd.instance.lock");
    let mut first_child = spawn_managed_child(&database, &socket, &instance_lock, 18);
    let first_ready = wait_for_managed_status(&socket, |status| {
        status.activation_generation == Some(18)
            && status.startup_state == StartupState::ReadyReportOnly
    });
    let first_client = IpcClient::new(&socket);
    let IpcPayload::Pause { until_unix_millis } = first_client
        .request(IpcCommand::Pause {
            duration_millis: 3_600_000,
        })
        .expect("pause ambient cleanup before lifecycle-only Arm test")
    else {
        panic!("expected pause payload")
    };
    assert!(until_unix_millis > 3_600_000);
    let IpcPayload::Lifecycle(first_armed) = first_client
        .request(IpcCommand::Arm {
            activation_generation: 18,
            instance_id: first_ready.instance_id.clone(),
        })
        .expect("arm first exact instance")
    else {
        panic!("expected first lifecycle payload")
    };
    let first_epoch = first_armed.enforcement_epoch.expect("first epoch");

    terminate_owned_child(&mut first_child, &socket);
    let after_signal = HistoryStore::open(&database)
        .expect("reopen after raw signal")
        .managed_lifecycle()
        .expect("read durable lifecycle")
        .expect("managed lifecycle exists");
    assert!(after_signal.requested_enforce);

    let mut replacement = spawn_managed_child(&database, &socket, &instance_lock, 18);
    let rearmed = wait_for_managed_status(&socket, |status| {
        status.activation_generation == Some(18)
            && status.startup_state == StartupState::ReadyEnforce
    });
    assert_eq!(rearmed.requested_mode, DaemonMode::Enforce);
    assert_eq!(rearmed.effective_mode(), DaemonMode::Enforce);
    assert!(
        rearmed
            .paused_until_unix_millis
            .is_some_and(|until| until > 3_600_000),
        "durable pause must remain active while lifecycle-only test is armed"
    );
    assert_ne!(rearmed.instance_id, first_ready.instance_id);
    assert_ne!(
        rearmed.enforcement_epoch.as_deref(),
        Some(first_epoch.as_str())
    );

    terminate_owned_child(&mut replacement, &socket);
    let after_replacement_signal = HistoryStore::open(&database)
        .expect("reopen after replacement raw signal")
        .managed_lifecycle()
        .expect("read replacement lifecycle")
        .expect("managed lifecycle exists");
    assert!(after_replacement_signal.requested_enforce);
}

#[test]
fn fatal_managed_startup_after_begin_clears_carried_intent_durably() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let store = HistoryStore::open(&database).expect("open managed store");
    store
        .begin_managed_boot(29, "seed-instance", 1_000)
        .expect("begin seed boot");
    store
        .finish_managed_recovery(29, "seed-instance", 1_010)
        .expect("finish seed recovery");
    store
        .complete_managed_first_scan(29, "seed-instance", "unused-seed-epoch", 1_020)
        .expect("complete seed first scan");
    store
        .arm_managed(29, "seed-instance", "seed-enforce-epoch", 1_030)
        .expect("seed carried enforce intent");
    drop(store);

    let invalid_socket_parent = temp.directory.join("not-a-directory");
    fs::write(&invalid_socket_parent, b"blocks socket parent creation")
        .expect("create invalid socket parent");
    let socket = invalid_socket_parent.join("unlingerd.sock");
    let instance_lock = fs::canonicalize(&temp.directory)
        .expect("canonical temp directory")
        .join("startup-failure.instance.lock");
    let output = Command::new(env!("CARGO_BIN_EXE_unlingerd"))
        .arg("--managed")
        .arg("--activation-generation")
        .arg("29")
        .arg("--interval-seconds")
        .arg("3600")
        .arg("--observe-seconds")
        .arg("0")
        .arg("--database")
        .arg(&database)
        .arg("--socket")
        .arg(&socket)
        .arg("--instance-lock")
        .arg(&instance_lock)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run managed daemon through fatal IPC setup");

    assert!(!output.status.success(), "invalid socket setup must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Not a directory")
            || stderr.contains("not a directory")
            || stderr.contains("File exists"),
        "unexpected startup error: {stderr}"
    );

    let lifecycle = HistoryStore::open(&database)
        .expect("reopen failed managed store")
        .managed_lifecycle()
        .expect("read lifecycle")
        .expect("managed lifecycle exists");
    assert_eq!(lifecycle.activation_generation, 29);
    assert_eq!(lifecycle.startup_phase, ManagedStartupPhase::Failed);
    assert!(!lifecycle.requested_enforce);
    assert!(!lifecycle.effective_enforce);
    assert!(lifecycle.armed_generation.is_none());
    assert!(lifecycle.enforcement_epoch.is_none());
}

#[test]
fn raw_sigterm_before_managed_ready_preserves_carried_intent() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let store = HistoryStore::open(&database).expect("open managed store");
    store
        .begin_managed_boot(30, "seed-instance", 1_000)
        .expect("begin seed boot");
    store
        .finish_managed_recovery(30, "seed-instance", 1_010)
        .expect("finish seed recovery");
    store
        .complete_managed_first_scan(30, "seed-instance", "unused-seed-epoch", 1_020)
        .expect("complete seed first scan");
    store
        .arm_managed(30, "seed-instance", "seed-enforce-epoch", 1_030)
        .expect("seed carried enforce intent");
    let pause_until = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_millis(),
    )
    .expect("wall clock fits u64")
    .saturating_add(3_600_000);
    store
        .set_pause_until(Some(pause_until))
        .expect("persist safety pause before spawning a real Mac runtime");
    drop(store);

    let socket = temp.directory.join("early-signal.sock");
    let instance_lock = fs::canonicalize(&temp.directory)
        .expect("canonical temp directory")
        .join("early-signal.instance.lock");
    let mut child = spawn_managed_child(&database, &socket, &instance_lock, 30);
    let started = Instant::now();
    let before_ready = loop {
        if socket.exists()
            && let Ok(IpcPayload::Status(status)) =
                IpcClient::new(&socket).request(IpcCommand::Status)
        {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(5) {
            let status = child.0.try_wait().expect("inspect managed daemon child");
            if status.is_none() {
                child
                    .0
                    .kill()
                    .expect("stop unresponsive managed daemon child");
                child
                    .0
                    .wait()
                    .expect("reap unresponsive managed daemon child");
            }
            let mut stderr = String::new();
            if let Some(stream) = child.0.stderr.as_mut() {
                stream
                    .read_to_string(&mut stderr)
                    .expect("read managed daemon stderr");
            }
            panic!(
                "managed daemon did not expose its startup status; status={status:?}; stderr={stderr}"
            );
        }
        thread::sleep(Duration::from_millis(1));
    };
    assert!(
        !before_ready.ready,
        "test must signal during managed startup, before Ready"
    );
    assert_eq!(
        before_ready.paused_until_unix_millis,
        Some(pause_until),
        "real-runtime lifecycle test must remain durably paused"
    );

    terminate_owned_child(&mut child, &socket);
    let lifecycle = HistoryStore::open(&database)
        .expect("reopen after early raw signal")
        .managed_lifecycle()
        .expect("read lifecycle")
        .expect("managed lifecycle exists");
    assert!(
        lifecycle.requested_enforce,
        "raw SIGTERM must preserve same-generation carried intent"
    );
    assert_ne!(lifecycle.startup_phase, ManagedStartupPhase::Failed);
}
