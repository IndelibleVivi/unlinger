#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unlinger_core::{
    CleanupPolicy, CleanupRuntime, CleanupSignal, IncidentState, ProcessGraph, ProcessIdentity,
    ProcessRecord, RuntimeFailure, SignalDisposition, Snapshot,
};
use unlinger_daemon::{
    ControlPlane, DaemonMode, DaemonStatus, EngineConfig, HistoryStore, ReconciliationEngine,
};
use unlinger_macos::MacosRuntime;
use unlinger_rules::{Analyzer, AnalyzerContext, RuleSet};

const CFT_APP_ENV: &str = "UNLINGER_FIELDLAB_CFT_APP";
const FULL_TIMING_ENV: &str = "UNLINGER_FIELDLAB_FULL_TIMING";
const ORDINARY_CHROME_EXECUTABLE: &str =
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

#[test]
fn scoped_runtime_rejects_an_identity_not_admitted_from_the_field_profile() {
    let mut runtime = ScopedRuntime::new(PathBuf::from(
        "/private/tmp/playwright_chromiumdev_profile-unlinger-fieldlab-test",
    ));
    let identity = ProcessIdentity {
        pid: u32::MAX,
        started_at_unix_micros: 1,
        executable_device: Some(1),
        executable_inode: Some(1),
    };

    let disposition = runtime.signal_exact(&identity, CleanupSignal::Term);

    assert_eq!(disposition, SignalDisposition::Rejected);
    assert_eq!(
        runtime.rejected_signals,
        vec![(u32::MAX, CleanupSignal::Term)]
    );
    assert!(runtime.delivered_signals.is_empty());
}

#[test]
#[ignore = "explicit live fieldlab: launches and signals only the supplied Chrome for Testing app"]
fn detached_chrome_for_testing_is_reclaimed_without_touching_ordinary_chrome() {
    run_fieldlab().expect("Chrome for Testing fieldlab");
}

fn run_fieldlab() -> Result<(), Box<dyn Error>> {
    let app = fieldlab_app()?;
    let profile = create_profile()?;
    let _session_guard = FieldSession::new(profile.clone());
    launch_chrome_for_testing(&app, &profile)?;

    let (preflight, root_identity) = wait_for_detached_root(&profile)?;
    let ordinary_chrome_before = ordinary_chrome_roots(&preflight);
    let rules = RuleSet::embedded()?;
    let preflight_reports =
        Analyzer::new(rules.clone(), AnalyzerContext::default()).observe(&preflight)?;
    let target = preflight_reports
        .iter()
        .find(|report| report.root.pid == root_identity.pid)
        .ok_or_else(|| field_error("field browser did not produce an incident report"))?;
    if target.state != IncidentState::Cooling {
        return Err(field_error(format!(
            "field browser entered {:?}, expected COOLING",
            target.state
        )));
    }
    let unrelated_cooling = preflight_reports
        .iter()
        .filter(|report| {
            report.state == IncidentState::Cooling && report.root.pid != root_identity.pid
        })
        .map(|report| report.incident_id.clone())
        .collect::<Vec<_>>();
    if !unrelated_cooling.is_empty() {
        return Err(field_error(format!(
            "refusing field enforcement with unrelated COOLING incidents: {unrelated_cooling:?}"
        )));
    }
    let target_incident_id = target.incident_id.clone();

    let database = profile.join("unlinger-fieldlab.sqlite3");
    let store = HistoryStore::open(&database)?;
    let control = ControlPlane::new(
        store,
        DaemonStatus::new(DaemonMode::Enforce, std::process::id()),
    );
    let timing = TimingProfile::from_environment();
    let runtime = ScopedRuntime::new(profile.clone());
    let mut engine = ReconciliationEngine::new(
        runtime,
        rules,
        control,
        timing.config,
        Some(std::process::id()),
    );

    let mut terminal_receipt = None;
    for cycle_index in 0..timing.max_cycles {
        let now = engine.runtime().now_unix_millis()?;
        let cycle = engine.run_cycle_at(now)?;
        if cycle.cleanup_receipts.len() > 1 {
            return Err(field_error(
                "field cycle produced more than one cleanup receipt",
            ));
        }
        if let Some(receipt) = cycle.cleanup_receipts.into_iter().next() {
            terminal_receipt = Some(receipt);
            break;
        }
        if cycle_index + 1 < timing.max_cycles {
            thread::sleep(timing.cycle_pause);
        }
    }

    let receipt = terminal_receipt
        .ok_or_else(|| field_error("field incident did not mature within the bounded cycles"))?;
    if receipt.incident_id != target_incident_id {
        return Err(field_error(format!(
            "cleanup receipt {} did not belong to field incident {target_incident_id}",
            receipt.incident_id
        )));
    }
    if receipt.state != IncidentState::Cleared
        || !receipt.survivor_pids.is_empty()
        || receipt.revival_checks_completed != 2
    {
        return Err(field_error(format!(
            "field cleanup did not finish cleanly: {receipt:?}"
        )));
    }
    if !receipt.actions.iter().any(|action| {
        action.pid == root_identity.pid
            && action.signal == CleanupSignal::Term
            && action.disposition == SignalDisposition::Delivered
    }) {
        return Err(field_error(
            "field root never received an exact delivered TERM",
        ));
    }
    if !engine.runtime().rejected_signals.is_empty() {
        return Err(field_error(format!(
            "scoped runtime rejected out-of-session signal attempts: {:?}",
            engine.runtime().rejected_signals
        )));
    }

    let mut verifier = MacosRuntime::new();
    let postflight = verifier.snapshot()?;
    if find_field_root(&postflight, &profile).is_some() {
        return Err(field_error(
            "field browser root survived the terminal receipt",
        ));
    }
    assert_ordinary_chrome_unchanged(&ordinary_chrome_before, &postflight)?;

    println!(
        "fieldlab timing={} profile={} ordinary_chrome_roots={}\n{}",
        timing.name,
        profile.display(),
        ordinary_chrome_before.len(),
        serde_json::to_string_pretty(&receipt)?
    );
    Ok(())
}

fn fieldlab_app() -> Result<PathBuf, Box<dyn Error>> {
    let configured = env::var_os(CFT_APP_ENV).ok_or_else(|| {
        field_error(format!(
            "{CFT_APP_ENV} must name a Google Chrome for Testing.app bundle"
        ))
    })?;
    let app = fs::canonicalize(configured)?;
    if app.file_name() != Some(OsStr::new("Google Chrome for Testing.app")) {
        return Err(field_error(format!(
            "refusing non-CfT app bundle {}",
            app.display()
        )));
    }
    if app == Path::new("/Applications/Google Chrome.app") {
        return Err(field_error("refusing ordinary Google Chrome"));
    }
    Ok(app)
}

fn create_profile() -> Result<PathBuf, Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let profile = PathBuf::from(format!(
        "/private/tmp/playwright_chromiumdev_profile-unlinger-fieldlab-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&profile)?;
    Ok(profile)
}

fn launch_chrome_for_testing(app: &Path, profile: &Path) -> Result<(), Box<dyn Error>> {
    let status = Command::new("/usr/bin/open")
        .arg("-na")
        .arg(app)
        .arg("--args")
        .arg("--headless=new")
        .arg("--disable-background-networking")
        .arg("--disable-component-update")
        .arg("--disable-default-apps")
        .arg("--disable-sync")
        .arg("--metrics-recording-only")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .status()?;
    if !status.success() {
        return Err(field_error(format!(
            "LaunchServices returned {status} for Chrome for Testing"
        )));
    }
    Ok(())
}

fn wait_for_detached_root(profile: &Path) -> Result<(Snapshot, ProcessIdentity), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut runtime = MacosRuntime::new();
    loop {
        let snapshot = runtime.snapshot()?;
        let detached_identity = find_field_root(&snapshot, profile)
            .filter(|root| root.parent_pid == 1)
            .map(|root| root.identity.clone());
        if let Some(identity) = detached_identity {
            return Ok((snapshot, identity));
        }
        if Instant::now() >= deadline {
            return Err(field_error(
                "Chrome for Testing did not become a detached field root within 20 seconds",
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn find_field_root<'a>(snapshot: &'a Snapshot, profile: &Path) -> Option<&'a ProcessRecord> {
    snapshot.processes.iter().find(|process| {
        process
            .executable_basename()
            .eq_ignore_ascii_case("Google Chrome for Testing")
            && !has_process_type(process)
            && uses_profile(process, profile)
    })
}

fn uses_profile(process: &ProcessRecord, profile: &Path) -> bool {
    let expected = profile.to_string_lossy();
    let Some(arguments) = &process.arguments else {
        return false;
    };
    for (index, argument) in arguments.iter().enumerate() {
        if argument == "--user-data-dir"
            && arguments
                .get(index + 1)
                .is_some_and(|value| value == expected.as_ref())
        {
            return true;
        }
        if argument
            .strip_prefix("--user-data-dir=")
            .is_some_and(|value| value == expected.as_ref())
        {
            return true;
        }
    }
    false
}

fn has_process_type(process: &ProcessRecord) -> bool {
    process.arguments.as_ref().is_some_and(|arguments| {
        arguments
            .iter()
            .any(|argument| argument.to_ascii_lowercase().starts_with("--type="))
    })
}

fn ordinary_chrome_roots(snapshot: &Snapshot) -> Vec<ProcessIdentity> {
    snapshot
        .processes
        .iter()
        .filter(|process| {
            process.executable_path.as_deref() == Some(ORDINARY_CHROME_EXECUTABLE)
                && !has_process_type(process)
        })
        .map(|process| process.identity.clone())
        .collect()
}

fn assert_ordinary_chrome_unchanged(
    before: &[ProcessIdentity],
    after: &Snapshot,
) -> Result<(), Box<dyn Error>> {
    let after_by_pid = after
        .processes
        .iter()
        .map(|process| (process.pid(), &process.identity))
        .collect::<BTreeMap<_, _>>();
    for identity in before {
        if after_by_pid
            .get(&identity.pid)
            .is_none_or(|current| !identity.exact_match(current))
        {
            return Err(field_error(format!(
                "ordinary Chrome identity {} changed during field cleanup",
                identity.pid
            )));
        }
    }
    Ok(())
}

struct ScopedRuntime {
    inner: MacosRuntime,
    profile: PathBuf,
    owned_identities: BTreeMap<u32, ProcessIdentity>,
    delivered_signals: Vec<(u32, CleanupSignal, SignalDisposition)>,
    rejected_signals: Vec<(u32, CleanupSignal)>,
}

impl ScopedRuntime {
    fn new(profile: PathBuf) -> Self {
        Self {
            inner: MacosRuntime::new(),
            profile,
            owned_identities: BTreeMap::new(),
            delivered_signals: Vec::new(),
            rejected_signals: Vec::new(),
        }
    }

    fn admit_field_tree(&mut self, snapshot: &Snapshot) -> Result<(), RuntimeFailure> {
        let Some(root) = find_field_root(snapshot, &self.profile) else {
            return Ok(());
        };
        let graph = ProcessGraph::from_snapshot(snapshot)
            .map_err(|error| RuntimeFailure::new(error.to_string()))?;
        let mut pids = graph.descendant_pids(root.pid());
        pids.push(root.pid());
        pids.sort_unstable();
        pids.dedup();
        for pid in pids {
            if let Some(process) = graph.get(pid) {
                self.owned_identities
                    .entry(pid)
                    .or_insert_with(|| process.identity.clone());
            }
        }
        Ok(())
    }
}

impl CleanupRuntime for ScopedRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        let snapshot = self.inner.snapshot()?;
        self.admit_field_tree(&snapshot)?;
        Ok(snapshot)
    }

    fn now_unix_millis(&self) -> Result<u64, RuntimeFailure> {
        self.inner.now_unix_millis()
    }

    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition {
        if self
            .owned_identities
            .get(&identity.pid)
            .is_none_or(|owned| !owned.exact_match(identity))
        {
            self.rejected_signals.push((identity.pid, signal));
            return SignalDisposition::Rejected;
        }
        let disposition = self.inner.signal_exact(identity, signal);
        self.delivered_signals
            .push((identity.pid, signal, disposition));
        disposition
    }

    fn wait(&mut self, duration: Duration) {
        self.inner.wait(duration);
    }
}

struct TimingProfile {
    name: &'static str,
    config: EngineConfig,
    cycle_pause: Duration,
    max_cycles: usize,
}

impl TimingProfile {
    fn from_environment() -> Self {
        if env::var_os(FULL_TIMING_ENV).is_some() {
            Self {
                name: "full",
                config: EngineConfig::default(),
                cycle_pause: Duration::from_secs(60),
                max_cycles: 3,
            }
        } else {
            Self {
                name: "fast",
                config: EngineConfig {
                    observation_gap: Duration::from_secs(1),
                    abandonment_grace: Duration::from_secs(2),
                    cooling_continuity_gap: Duration::from_secs(30),
                    cleanup_policy: CleanupPolicy {
                        primary_term_grace: Duration::from_secs(1),
                        member_term_grace: Duration::from_secs(1),
                        kill_grace: Duration::from_secs(1),
                        revival_windows: vec![Duration::from_secs(1), Duration::from_secs(1)],
                    },
                    ..EngineConfig::default()
                },
                cycle_pause: Duration::from_secs(2),
                max_cycles: 4,
            }
        }
    }
}

struct FieldSession {
    profile: PathBuf,
}

impl FieldSession {
    fn new(profile: PathBuf) -> Self {
        Self { profile }
    }
}

impl Drop for FieldSession {
    fn drop(&mut self) {
        let mut runtime = MacosRuntime::new();
        if let Ok(snapshot) = runtime.snapshot()
            && let Some(root) = find_field_root(&snapshot, &self.profile)
        {
            let identity = root.identity.clone();
            let _ = runtime.signal_exact(&identity, CleanupSignal::Term);
            thread::sleep(Duration::from_secs(1));
            if let Ok(snapshot) = runtime.snapshot()
                && find_field_root(&snapshot, &self.profile)
                    .is_some_and(|current| identity.exact_match(&current.identity))
            {
                let _ = runtime.signal_exact(&identity, CleanupSignal::Kill);
            }
        }
        eprintln!(
            "fieldlab retained its exact temporary profile for inspection: {}",
            self.profile.display()
        );
    }
}

fn field_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::other(message.into()))
}
