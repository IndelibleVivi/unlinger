#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unlinger_daemon::{
    ControlPlane, DaemonMode, DaemonStatus, HistoryStore, IpcClient, IpcCommand, IpcPayload,
    IpcServer, TaskPhase,
};

struct Fixture {
    directory: PathBuf,
    store: HistoryStore,
    control: ControlPlane,
    server: Option<IpcServer>,
    retain: bool,
}
impl Fixture {
    fn new(ready: bool) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let directory = std::env::temp_dir().join(format!(
            "ul-tr-{}-{:x}-{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                & 0xffff_ffff,
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let store = HistoryStore::open(directory.join("history.db")).unwrap();
        let control = ControlPlane::new(
            store.clone(),
            DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
        )
        .unwrap();
        if ready {
            control.complete_successful_cycle(now()).unwrap();
        }
        let server = IpcServer::start(directory.join("d.sock"), control.clone()).unwrap();
        Self {
            directory,
            store,
            control,
            server: Some(server),
            retain: false,
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_unlinger"));
        command.arg("--socket").arg(self.directory.join("d.sock"));
        command
    }
    fn client(&self) -> IpcClient {
        IpcClient::new(self.directory.join("d.sock"))
    }
    fn only_task(&self) -> Option<(String, Option<String>)> {
        use rusqlite::OptionalExtension;
        rusqlite::Connection::open(self.store.path())
            .unwrap()
            .query_row("SELECT task_id, owner_json FROM task_scopes", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.server.take());
        if !self.retain {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[test]
fn task_run_preserves_command_output_exit_and_private_ownership() {
    let fixture = Fixture::new(true);
    let output = fixture
        .command()
        .args([
            "task",
            "run",
            "--",
            "/bin/sh",
            "-c",
            "printf '%s' \"$PLAYWRIGHT_CLI_SESSION\"; exit 7",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (id, owner) = fixture.only_task().unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("unlinger-{id}")
    );
    assert!(owner.is_some());
    let IpcPayload::TaskStatus(status) = fixture
        .client()
        .request(IpcCommand::TaskStatus {
            task_id: id.clone(),
        })
        .unwrap()
    else {
        panic!("task status")
    };
    assert_eq!(status.phase, TaskPhase::Released);
    assert!(status.incident_ids.is_empty());
    let output = fixture
        .command()
        .args(["task", "status", &id, "--json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["phase"], "released");
    assert!(json.get("capability").is_none() && json.get("owner_json").is_none());
}

#[test]
fn unavailable_daemon_cannot_run_the_command() {
    let fixture = Fixture::new(false);
    let output = fixture
        .command()
        .args(["task", "run", "--", "/bin/echo", "must-not-run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(fixture.only_task().is_none());
}

#[test]
fn killed_wrapper_does_not_end_a_live_exec_command_and_restart_keeps_authority() {
    let fixture = Fixture::new(true);
    let mut wrapper = fixture
        .command()
        .args(["task", "run", "--", "/bin/sleep", "4"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let (id, owner) = loop {
        if let Some((id, Some(owner))) = fixture.only_task() {
            let identity: unlinger_core::TaskOwnerIdentity = serde_json::from_str(&owner).unwrap();
            if unlinger_macos::MacosSnapshotter::new()
                .lookup(identity.pid)
                .unwrap()
                .is_some_and(|p| p.executable_basename() == "sleep")
            {
                break (id, identity);
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    };
    wrapper.kill().unwrap();
    wrapper.wait().unwrap();
    let reopened = HistoryStore::open(fixture.store.path()).unwrap();
    assert_eq!(
        reopened.task_status(&id).unwrap().unwrap().phase,
        TaskPhase::Active
    );
    let live = unlinger_macos::MacosSnapshotter::new()
        .lookup(owner.pid)
        .unwrap()
        .unwrap();
    assert!(owner.matches(&live));
    assert!(live.parent_pid != wrapper.id());
    // The exact task owner survives exec, including a changed executable inode.
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        if matches!(
            unlinger_macos::MacosSnapshotter::new().lookup(owner.pid),
            Ok(None)
        ) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut engine = unlinger_daemon::ReconciliationEngine::new(
        unlinger_macos::MacosRuntime::new(),
        unlinger_rules::RuleSet::embedded().unwrap(),
        fixture.control.clone(),
        unlinger_daemon::EngineConfig::default(),
        Some(std::process::id()),
    );
    engine.run_cycle_at(now()).unwrap();
    assert_eq!(
        reopened.task_status(&id).unwrap().unwrap().phase,
        TaskPhase::Released
    );
    assert_eq!(
        reopened
            .task_status(&id)
            .unwrap()
            .unwrap()
            .release_reason
            .as_deref(),
        Some("owner_disappeared")
    );
}

/// The ignored field lane retains its workspace and receipts. Every signal is
/// additionally restricted to identities descended from its own issued session.
struct OwnedBrowserRuntime {
    native: unlinger_macos::MacosRuntime,
    root: unlinger_core::ProcessIdentity,
    allowed: Vec<unlinger_core::ProcessIdentity>,
}

struct FieldSessions {
    node: std::ffi::OsString,
    entry: PathBuf,
    workspace: PathBuf,
    release: PathBuf,
    control_name: String,
    store: HistoryStore,
}
impl Drop for FieldSessions {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, b"end");
        let mut names = vec![self.control_name.clone()];
        if let Ok(connection) = rusqlite::Connection::open(self.store.path())
            && let Ok(mut query) = connection.prepare("SELECT task_id FROM task_scopes")
            && let Ok(rows) = query.query_map([], |row| row.get::<_, String>(0))
        {
            names.extend(rows.flatten().map(|id| format!("unlinger-{id}")));
        }
        for name in names {
            if let Ok(mut close) = Command::new(&self.node)
                .arg(&self.entry)
                .args(["-s", &name, "close"])
                .current_dir(&self.workspace)
                .env("PWTEST_CLI_GLOBAL_CONFIG", &self.workspace)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let deadline = Instant::now() + Duration::from_secs(10);
                while matches!(close.try_wait(), Ok(None)) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(50));
                }
                if matches!(close.try_wait(), Ok(None)) {
                    let _ = close.kill();
                }
                let _ = close.wait();
            }
        }
    }
}
impl unlinger_core::CleanupRuntime for OwnedBrowserRuntime {
    fn snapshot(&mut self) -> Result<unlinger_core::Snapshot, unlinger_core::RuntimeFailure> {
        let snapshot = self.native.snapshot()?;
        let graph = unlinger_core::ProcessGraph::from_snapshot(&snapshot)
            .map_err(|error| unlinger_core::RuntimeFailure::new(error.to_string()))?;
        if graph
            .get(self.root.pid)
            .is_some_and(|p| self.root.exact_match(&p.identity))
        {
            for pid in graph
                .descendant_pids(self.root.pid)
                .into_iter()
                .chain([self.root.pid])
            {
                if let Some(process) = graph.get(pid)
                    && !self.allowed.contains(&process.identity)
                {
                    self.allowed.push(process.identity.clone());
                }
            }
        }
        Ok(snapshot)
    }
    fn lookup_process(
        &mut self,
        pid: u32,
    ) -> Result<Option<unlinger_core::ProcessRecord>, unlinger_core::RuntimeFailure> {
        self.native.lookup_process(pid)
    }
    fn clock_sample(&self) -> Result<unlinger_core::ClockSample, unlinger_core::RuntimeFailure> {
        self.native.clock_sample()
    }
    fn signal_exact(
        &mut self,
        identity: &unlinger_core::ProcessIdentity,
        signal: unlinger_core::CleanupSignal,
    ) -> unlinger_core::SignalDisposition {
        if !self
            .allowed
            .iter()
            .any(|allowed| allowed.exact_match(identity))
        {
            return unlinger_core::SignalDisposition::Rejected;
        }
        self.native.signal_exact(identity, signal)
    }
    fn wait_until(
        &mut self,
        duration: Duration,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<unlinger_core::WaitOutcome, unlinger_core::RuntimeFailure> {
        self.native.wait_until(duration, should_stop)
    }
}

#[test]
#[ignore = "explicit owned Chrome for Testing task lifecycle, full production timing"]
fn task_owned_playwright_cli_reclaims_only_its_finished_session() {
    use unlinger_core::IncidentState;
    assert_eq!(
        std::env::var("UNLINGER_TASK_FIELDLAB_ACK").as_deref(),
        Ok("I_ACCEPT_OWNED_CFT_SIGNALING")
    );
    let app = PathBuf::from(std::env::var_os("UNLINGER_FIELDLAB_CFT_APP").expect("CfT app"));
    assert_eq!(app.file_name().unwrap(), "Google Chrome for Testing.app");
    let bundle = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&bundle.stdout).trim(),
        "com.google.chrome.for.testing"
    );
    let core = PathBuf::from(
        std::env::var_os("UNLINGER_TASK_FIELDLAB_CORE").expect("installed playwright-core package"),
    );
    let node = std::env::var_os("UNLINGER_TASK_FIELDLAB_NODE").unwrap_or_else(|| "node".into());
    let mut fixture = Fixture::new(true);
    fixture.retain = true;
    println!("task field workspace: {}", fixture.directory.display());
    let workspace = fixture.directory.join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(workspace.join(".playwright")).unwrap();
    let config = workspace.join("browser.json");
    fs::write(&config, serde_json::json!({"browser": {"browserName": "chromium", "isolated": true, "launchOptions": {"executablePath": app.join("Contents/MacOS/Google Chrome for Testing"), "headless": true}}}).to_string()).unwrap();
    let ready = fixture.directory.join("ready");
    let release = fixture.directory.join("release");
    let entry = core.join("lib/tools/cli-client/cli.js");
    let control_name = format!("unlinger-control-{}", std::process::id());
    let sessions = FieldSessions {
        node: node.clone(),
        entry: entry.clone(),
        workspace: workspace.clone(),
        release: release.clone(),
        control_name: control_name.clone(),
        store: fixture.store.clone(),
    };
    let control_open = Command::new(&node)
        .arg(&entry)
        .args(["-s", &control_name, "open", "--config"])
        .arg(&config)
        .current_dir(&workspace)
        .env("PWTEST_CLI_GLOBAL_CONFIG", &workspace)
        .output()
        .unwrap();
    assert!(
        control_open.status.success(),
        "{}",
        String::from_utf8_lossy(&control_open.stderr)
    );
    let output_file = fs::File::create(fixture.directory.join("command.log")).unwrap();
    let error_file = fs::File::create(fixture.directory.join("command-error.log")).unwrap();
    let mut command = fixture.command();
    command.args(["task", "run", "--", "/bin/sh", "-c", "\"$1\" \"$2\" open --config \"$3\" || exit; : > \"$4\"; while [ ! -f \"$5\" ]; do /bin/sleep 0.1; done", "field-owner"])
        .arg(&node).arg(&entry).arg(&config).arg(&ready).arg(&release).current_dir(&workspace)
        .env("PWTEST_CLI_GLOBAL_CONFIG", &workspace).stdout(output_file).stderr(error_file);
    let mut wrapper = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        assert!(
            wrapper.try_wait().unwrap().is_none(),
            "{}",
            fs::read_to_string(fixture.directory.join("command-error.log")).unwrap()
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(100));
    }
    let (task_id, _) = fixture.only_task().unwrap();
    let snapshot = unlinger_macos::MacosSnapshotter::new().capture().unwrap();
    let controller = snapshot.processes.iter().find(|p| p.runtime.playwright_cli.as_ref().is_some_and(|cli| cli.session_name == format!("unlinger-{task_id}")))
        .unwrap_or_else(|| panic!("native controller/registry/socket facts unavailable; task selector observed on {} processes", snapshot.processes.iter().filter(|p| p.runtime.task_session_name.is_some()).count()));
    let root = controller.identity.clone();
    let mut engine = unlinger_daemon::ReconciliationEngine::new(
        OwnedBrowserRuntime {
            native: unlinger_macos::MacosRuntime::new(),
            root: root.clone(),
            allowed: vec![],
        },
        unlinger_rules::RuleSet::embedded().unwrap(),
        fixture.control.clone(),
        unlinger_daemon::EngineConfig::default(),
        Some(std::process::id()),
    );
    let first = engine.run_cycle_at(now()).unwrap();
    let incident = first
        .incidents
        .iter()
        .find(|report| report.root.pid == root.pid)
        .unwrap();
    assert_eq!(incident.state, IncidentState::Protected);
    assert!(
        incident
            .evidence
            .iter()
            .any(|e| e.id == "protection.task_owner_active")
    );
    let incident_id = incident.incident_id.clone();
    assert!(
        fixture
            .store
            .task_status(&task_id)
            .unwrap()
            .unwrap()
            .incident_ids
            .contains(&incident_id)
    );
    fs::write(&release, b"end").unwrap();
    assert!(wrapper.wait().unwrap().success());
    assert_eq!(
        fixture.store.task_status(&task_id).unwrap().unwrap().phase,
        TaskPhase::Released
    );
    let released_snapshot = unlinger_macos::MacosSnapshotter::new().capture().unwrap();
    let graph = unlinger_core::ProcessGraph::from_snapshot(&released_snapshot).unwrap();
    let members = graph.descendant_pids(root.pid);
    let outside = released_snapshot
        .processes
        .iter()
        .filter(|p| {
            p.pid() != root.pid
                && !members.contains(&p.pid())
                && p.runtime.task_session_name.as_deref()
                    == Some(format!("unlinger-{task_id}").as_str())
        })
        .map(|p| (p.pid(), p.parent_pid, p.executable_basename()))
        .collect::<Vec<_>>();
    println!("task field non-browser work after release: {outside:?}");
    let cache = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join("Library/Caches/ms-playwright/daemon");
    let socket = fs::read_dir(cache)
        .unwrap()
        .filter_map(Result::ok)
        .find_map(|directory| {
            let bytes =
                fs::read(directory.path().join(format!("unlinger-{task_id}.session"))).ok()?;
            let record: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            Some(PathBuf::from(record.get("socketPath")?.as_str()?))
        })
        .expect("owned CLI registry socket");
    let active_client = std::os::unix::net::UnixStream::connect(socket).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let active = engine.run_cycle_at(now()).unwrap();
    assert!(
        active
            .incidents
            .iter()
            .find(|report| report.root.pid == root.pid)
            .unwrap()
            .evidence
            .iter()
            .any(|e| e.id == "protection.task_client_active")
    );
    drop(active_client);
    // A different ordinary session remains open throughout; only this field's
    // issued task identities can receive an OS signal from the engine.
    let enforce = ControlPlane::new(
        fixture.store.clone(),
        DaemonStatus::new(DaemonMode::Enforce, std::process::id()),
    )
    .unwrap();
    enforce.complete_successful_cycle(now()).unwrap();
    drop(fixture.server.take());
    fixture.server =
        Some(IpcServer::start(fixture.directory.join("d.sock"), enforce.clone()).unwrap());
    let mut engine = unlinger_daemon::ReconciliationEngine::new(
        OwnedBrowserRuntime {
            native: unlinger_macos::MacosRuntime::new(),
            root: root.clone(),
            allowed: vec![],
        },
        unlinger_rules::RuleSet::embedded().unwrap(),
        enforce,
        unlinger_daemon::EngineConfig::default(),
        Some(std::process::id()),
    );
    let deadline = Instant::now() + Duration::from_secs(400);
    let receipt = loop {
        assert!(
            !fixture.directory.join("stop").exists(),
            "field run cancelled before next scan"
        );
        let cycle = engine.run_cycle_at(now()).unwrap();
        for report in &cycle.incidents {
            if report.incident_id == incident_id {
                println!(
                    "task field phase {:?}: {:?}",
                    report.state,
                    report
                        .evidence
                        .iter()
                        .map(|e| e.id.as_str())
                        .filter(|id| id.starts_with("protection.") || id.starts_with("ambiguity."))
                        .collect::<Vec<_>>()
                );
            } else {
                assert_ne!(
                    report.state,
                    IncidentState::Confirmed,
                    "unrelated candidate"
                );
            }
        }
        if let Some(receipt) = cycle
            .cleanup_receipts
            .into_iter()
            .find(|receipt| receipt.incident_id == incident_id)
        {
            break receipt;
        }
        assert!(
            Instant::now() < deadline,
            "task did not settle within production timing"
        );
        std::thread::sleep(Duration::from_secs(15));
    };
    assert_eq!(receipt.state, IncidentState::Cleared);
    assert!(receipt.artifact_actions.is_empty() && receipt.survivor_pids.is_empty());
    assert_eq!(receipt.revival_checks_completed, 2);
    fs::write(
        fixture.directory.join("cleanup-receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    let overview = fixture.client().request_browser_overview().unwrap();
    let impact = overview.impact.as_ref().unwrap();
    assert_eq!(impact.proved_reclaim_count, 1);
    assert_eq!(
        impact.reclaimed_process_count,
        receipt
            .resources
            .before
            .as_ref()
            .map(|before| before.process_count)
    );
    assert_eq!(
        impact.estimated_reclaimed_memory_bytes,
        receipt.resources.estimated_reclaimed_memory_bytes
    );
    fs::write(
        fixture.directory.join("app-browser-overview.json"),
        serde_json::to_vec_pretty(&overview).unwrap(),
    )
    .unwrap();
    let control_status = Command::new(&node)
        .arg(&entry)
        .args(["-s", &control_name, "eval", "() => 42"])
        .current_dir(&workspace)
        .env("PWTEST_CLI_GLOBAL_CONFIG", &workspace)
        .output()
        .unwrap();
    assert!(
        control_status.status.success(),
        "unrelated session must remain usable"
    );
    let closed = Command::new(&node)
        .arg(&entry)
        .args(["-s", &control_name, "close"])
        .current_dir(&workspace)
        .env("PWTEST_CLI_GLOBAL_CONFIG", &workspace)
        .output()
        .unwrap();
    assert!(closed.status.success());
    drop(sessions);
    println!(
        "retained task field evidence: {}",
        fixture.directory.display()
    );
    // Preserve the private controlled-run artifacts, but stop its own IPC server.
    drop(fixture.server.take());
}
