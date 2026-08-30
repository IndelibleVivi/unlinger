#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TempState {
    directory: PathBuf,
}

impl TempState {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ul-s-{}-{nonce:x}", std::process::id()));
        fs::create_dir(&directory).expect("create temp directory");
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

#[test]
fn sigterm_finishes_the_cycle_and_removes_the_owned_socket() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
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
    assert!(socket.exists(), "daemon did not create its IPC socket");

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
