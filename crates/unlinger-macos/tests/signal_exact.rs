#![cfg(target_os = "macos")]

use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};
use unlinger_core::{CleanupRuntime, CleanupSignal, SignalDisposition};
use unlinger_macos::{MacosRuntime, MacosSnapshotter};

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[test]
fn refuses_changed_identity_then_terms_the_exact_owned_child() {
    let child = Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .expect("spawn isolated test child");
    let mut child = ChildGuard(child);
    let snapshotter = MacosSnapshotter::new();
    let process = snapshotter
        .lookup(child.0.id())
        .expect("lookup succeeds")
        .expect("child is live");
    let mut changed = process.identity.clone();
    changed.started_at_unix_micros += 1;
    let mut runtime = MacosRuntime::new();

    assert_eq!(
        runtime.signal_exact(&changed, CleanupSignal::Term),
        SignalDisposition::IdentityMismatch
    );
    assert!(child.0.try_wait().expect("child status").is_none());
    assert_eq!(
        runtime.signal_exact(&process.identity, CleanupSignal::Term),
        SignalDisposition::Delivered
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child.0.try_wait().expect("child status").is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("isolated child did not exit after exact SIGTERM");
}
