use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unlinger_core::{
    CleanupRuntime, CleanupSignal, ExecutableIdentity, IncidentState, ProcessIdentity,
    ProcessRecord, ProcessStatus, RuntimeFailure, SignalDisposition, Snapshot, SnapshotCoverage,
};
use unlinger_daemon::{
    ControlPlane, DaemonMode, DaemonStatus, EngineConfig, EventPayload, HistoryStore,
    ReconciliationEngine,
};
use unlinger_rules::RuleSet;

struct TempDatabase(PathBuf);

static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

impl TempDatabase {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "unlinger-engine-test-{}-{nonce}-{}.sqlite3",
            std::process::id(),
            NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let _ = fs::remove_file(format!("{}{}", self.0.display(), suffix));
        }
    }
}

#[derive(Default)]
struct FakeRuntime {
    snapshots: VecDeque<Snapshot>,
    signals: Vec<(u32, CleanupSignal)>,
    waits: Vec<Duration>,
    now_unix_millis: u64,
}

impl FakeRuntime {
    fn with_snapshots(snapshots: Vec<Snapshot>) -> Self {
        Self {
            snapshots: snapshots.into(),
            now_unix_millis: 3_000,
            ..Self::default()
        }
    }
}

impl CleanupRuntime for FakeRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        self.snapshots
            .pop_front()
            .ok_or_else(|| RuntimeFailure::new("fake snapshot queue exhausted"))
    }

    fn now_unix_millis(&self) -> Result<u64, RuntimeFailure> {
        Ok(self.now_unix_millis)
    }

    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition {
        self.signals.push((identity.pid, signal));
        SignalDisposition::Delivered
    }

    fn wait(&mut self, duration: Duration) {
        self.waits.push(duration);
    }
}

fn abandoned_snapshot(observed_at_unix_millis: u64) -> Snapshot {
    let process = ProcessRecord {
        identity: ProcessIdentity {
            pid: 700,
            started_at_unix_micros: 7_000_000,
            executable_device: Some(1),
            executable_inode: Some(700),
        },
        parent_pid: 1,
        process_group_id: 700,
        uid: 501,
        tty_device: None,
        name: "Google Chrome for Testing".to_owned(),
        executable_path: Some(
            "/Applications/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
                .to_owned(),
        ),
        executable: ExecutableIdentity {
            device: Some(1),
            inode: Some(700),
            size: Some(1),
            modified_unix_nanos: Some(1),
        },
        arguments: Some(vec![
            "Google Chrome for Testing".to_owned(),
            "--headless=new".to_owned(),
            "--remote-debugging-pipe".to_owned(),
            "--user-data-dir=/private/tmp/agent-browser/engine-test".to_owned(),
        ]),
        resident_memory_bytes: 1024,
        status: ProcessStatus::Sleeping,
    };
    Snapshot {
        observed_at_unix_millis,
        current_uid: 501,
        processes: vec![process],
        coverage: SnapshotCoverage {
            listed_processes: 1,
            inspected_processes: 1,
            ..SnapshotCoverage::default()
        },
    }
}

fn empty_snapshot(observed_at_unix_millis: u64) -> Snapshot {
    Snapshot {
        observed_at_unix_millis,
        current_uid: 501,
        processes: Vec::new(),
        coverage: SnapshotCoverage::default(),
    }
}

fn engine(
    mode: DaemonMode,
    runtime: FakeRuntime,
) -> (
    ReconciliationEngine<FakeRuntime>,
    ControlPlane,
    TempDatabase,
) {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open history");
    let control = ControlPlane::new(store, DaemonStatus::new(mode, 999_999));
    let config = EngineConfig {
        observation_gap: Duration::ZERO,
        abandonment_grace: Duration::ZERO,
        cleanup_policy: unlinger_core::CleanupPolicy {
            primary_term_grace: Duration::ZERO,
            member_term_grace: Duration::ZERO,
            kill_grace: Duration::ZERO,
            revival_windows: vec![Duration::ZERO, Duration::ZERO],
        },
        ..EngineConfig::default()
    };
    let engine = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        control.clone(),
        config,
        None,
    );
    (engine, control, database)
}

#[test]
fn abandonment_grace_must_mature_across_durable_cycles() {
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(61_000),
        abandoned_snapshot(61_015),
        abandoned_snapshot(91_000),
        abandoned_snapshot(91_015),
    ]);
    let (mut engine, control, _database) = engine(DaemonMode::ReportOnly, runtime);
    engine.config_mut().abandonment_grace = Duration::from_millis(90_000);

    let first = engine.run_cycle_at(1_015).expect("first cycle");
    assert_eq!(first.incidents[0].state, IncidentState::Cooling);
    let second = engine.run_cycle_at(61_015).expect("second cycle");
    assert_eq!(second.incidents[0].state, IncidentState::Cooling);
    let third = engine.run_cycle_at(91_015).expect("third cycle");
    assert_eq!(third.incidents[0].state, IncidentState::Confirmed);
    assert_eq!(control.status().expect("status").confirmed_incidents, 1);
}

#[test]
fn report_only_confirms_but_never_signals() {
    let runtime =
        FakeRuntime::with_snapshots(vec![abandoned_snapshot(1_000), abandoned_snapshot(1_015)]);
    let (mut engine, control, _database) = engine(DaemonMode::ReportOnly, runtime);
    let cycle = engine.run_cycle_at(2_000).expect("run cycle");

    assert_eq!(cycle.incidents[0].state, IncidentState::Confirmed);
    assert!(cycle.cleanup_receipts.is_empty());
    assert!(engine.runtime().signals.is_empty());
    assert_eq!(control.status().expect("status").confirmed_incidents, 1);
}

#[test]
fn enforce_runs_the_frozen_cleanup_and_records_terminal_receipt() {
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(1_016),
        empty_snapshot(1_020),
        empty_snapshot(1_035),
        empty_snapshot(1_095),
    ]);
    let (mut engine, control, _database) = engine(DaemonMode::Enforce, runtime);
    let cycle = engine.run_cycle_at(2_000).expect("run cycle");

    assert_eq!(cycle.cleanup_receipts.len(), 1);
    assert_eq!(cycle.cleanup_receipts[0].state, IncidentState::Cleared);
    assert_eq!(engine.runtime().signals, vec![(700, CleanupSignal::Term)]);
    let status = control.status().expect("status");
    let incident = control
        .store()
        .explain(&cycle.cleanup_receipts[0].incident_id)
        .expect("read cleanup history")
        .expect("cleanup history");
    let cleanup_events = incident
        .events
        .iter()
        .filter(|event| matches!(event.payload, EventPayload::Cleanup { .. }))
        .collect::<Vec<_>>();
    assert_eq!(cleanup_events.len(), 2);
    assert_eq!(cleanup_events[0].occurred_at_unix_millis, 2_000);
    assert!(
        cleanup_events[1].occurred_at_unix_millis > cleanup_events[0].occurred_at_unix_millis,
        "terminal cleanup event reused the cycle-start timestamp"
    );
    assert_eq!(status.confirmed_incidents, 0);
    assert_eq!(
        status
            .most_recent_reclaim
            .as_ref()
            .expect("recent reclaim")
            .occurred_at_unix_millis,
        cleanup_events[1].occurred_at_unix_millis
    );
    assert_eq!(
        status.most_recent_reclaim.expect("recent reclaim").state,
        IncidentState::Cleared
    );
}

#[test]
fn pause_keeps_observation_live_but_suppresses_enforcement() {
    let runtime =
        FakeRuntime::with_snapshots(vec![abandoned_snapshot(1_000), abandoned_snapshot(1_015)]);
    let (mut engine, control, _database) = engine(DaemonMode::Enforce, runtime);
    control
        .handle_at(
            unlinger_daemon::IpcCommand::Pause {
                duration_millis: 60_000,
            },
            1_500,
        )
        .expect("pause");

    let cycle = engine.run_cycle_at(2_000).expect("run cycle");
    assert_eq!(cycle.incidents[0].state, IncidentState::Confirmed);
    assert!(cycle.cleanup_receipts.is_empty());
    assert!(engine.runtime().signals.is_empty());
}

#[test]
fn post_signal_runtime_failure_is_persisted_before_cycle_error() {
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(1_016),
    ]);
    let (mut engine, control, _database) = engine(DaemonMode::Enforce, runtime);

    engine
        .run_cycle_at(2_000)
        .expect_err("post-signal snapshot queue must fail");

    let history = control
        .store()
        .history(10)
        .expect("history remains readable");
    let failed_event = history
        .iter()
        .find(|event| {
            matches!(
                &event.payload,
                EventPayload::Cleanup { receipt } if receipt.state == IncidentState::Failed
            )
        })
        .expect("terminal failed receipt was persisted");
    assert_eq!(failed_event.occurred_at_unix_millis, 3_000);
    let EventPayload::Cleanup { receipt: failed } = &failed_event.payload else {
        unreachable!("failed event payload is cleanup")
    };
    assert_eq!(failed.actions.len(), 1);
    assert_eq!(failed.actions[0].pid, 700);
    assert!(!control.status().expect("status").healthy);
}
