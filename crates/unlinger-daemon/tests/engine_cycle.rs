use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unlinger_core::{
    AppBundleVersion, ArtifactFreeze, CleanupReceipt, CleanupResources, CleanupRuntime,
    CleanupSignal, ClockSample, ExecutableIdentity, IncidentState, ProcessIdentity, ProcessRecord,
    ProcessStatus, RuntimeArtifactCandidate, RuntimeFailure, SignalDisposition, Snapshot,
    SnapshotCoverage, WaitOutcome,
};
use unlinger_daemon::{
    ControlPlane, DaemonMode, DaemonStatus, EngineConfig, EventPayload, HistoryStore, IpcCommand,
    IpcPayload, ReconciliationEngine, StartupState,
};
use unlinger_rules::RuleSet;

struct TempDatabase(PathBuf);

static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(1);
const TEST_WALL_BASE_MILLIS: u64 = 1_700_000_000_000;

fn test_wall(relative_millis: u64) -> u64 {
    TEST_WALL_BASE_MILLIS + relative_millis
}

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
    last_snapshot: Option<Snapshot>,
    signals: Vec<(u32, CleanupSignal)>,
    waits: Vec<Duration>,
    now_unix_millis: u64,
    snapshots_taken: usize,
    drain_trigger: Option<(usize, ControlPlane, u64, String)>,
    owner_policy_trigger: Option<(usize, ControlPlane, OwnerPolicyMutation)>,
    delivery_unknown_on_signal: bool,
}

enum OwnerPolicyMutation {
    Pause,
    ProtectLatestIncident,
    Arm {
        activation_generation: u64,
        instance_id: String,
    },
}

impl FakeRuntime {
    fn with_snapshots(snapshots: Vec<Snapshot>) -> Self {
        Self {
            snapshots: snapshots.into(),
            now_unix_millis: 0,
            ..Self::default()
        }
    }

    fn drain_on_snapshot(
        mut self,
        snapshot_number: usize,
        control: ControlPlane,
        activation_generation: u64,
        instance_id: String,
    ) -> Self {
        self.drain_trigger = Some((snapshot_number, control, activation_generation, instance_id));
        self
    }

    fn owner_policy_on_snapshot(
        mut self,
        snapshot_number: usize,
        control: ControlPlane,
        mutation: OwnerPolicyMutation,
    ) -> Self {
        self.owner_policy_trigger = Some((snapshot_number, control, mutation));
        self
    }

    fn delivery_unknown_on_signal(mut self) -> Self {
        self.delivery_unknown_on_signal = true;
        self
    }
}

impl CleanupRuntime for FakeRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        let snapshot = self
            .snapshots
            .pop_front()
            .ok_or_else(|| RuntimeFailure::new("fake snapshot queue exhausted"))?;
        self.now_unix_millis = snapshot.observed_at_unix_millis;
        self.last_snapshot = Some(snapshot.clone());
        self.snapshots_taken += 1;
        if self
            .drain_trigger
            .as_ref()
            .is_some_and(|trigger| trigger.0 == self.snapshots_taken)
        {
            let (_, control, activation_generation, instance_id) =
                self.drain_trigger.take().expect("drain trigger exists");
            control
                .handle_at(
                    IpcCommand::BeginDrain {
                        activation_generation,
                        instance_id,
                    },
                    self.now_unix_millis,
                )
                .expect("acknowledge drain during fresh pre-signal snapshot");
        }
        if self
            .owner_policy_trigger
            .as_ref()
            .is_some_and(|trigger| trigger.0 == self.snapshots_taken)
        {
            let (_, control, mutation) = self
                .owner_policy_trigger
                .take()
                .expect("owner policy trigger exists");
            let command = match mutation {
                OwnerPolicyMutation::Pause => IpcCommand::Pause {
                    duration_millis: 60_000,
                },
                OwnerPolicyMutation::ProtectLatestIncident => {
                    let incident_id = control
                        .store()
                        .history(1)
                        .expect("read latest observation")
                        .into_iter()
                        .next()
                        .expect("latest observation exists")
                        .incident_id;
                    IpcCommand::ProtectIncident { incident_id }
                }
                OwnerPolicyMutation::Arm {
                    activation_generation,
                    instance_id,
                } => IpcCommand::Arm {
                    activation_generation,
                    instance_id,
                },
            };
            control
                .handle_at(command, self.now_unix_millis)
                .expect("apply owner policy during fresh pre-signal snapshot");
        }
        Ok(snapshot)
    }

    fn lookup_process(
        &mut self,
        pid: u32,
    ) -> Result<Option<unlinger_core::ProcessRecord>, RuntimeFailure> {
        Ok(self
            .last_snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .processes
                    .iter()
                    .find(|process| process.pid() == pid)
            })
            .cloned())
    }

    fn clock_sample(&self) -> Result<ClockSample, RuntimeFailure> {
        Ok(ClockSample {
            wall_unix_millis: self.now_unix_millis,
            continuous_millis: self.now_unix_millis,
            boot_session_fingerprint: "engine-test-boot".to_owned(),
        })
    }

    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition {
        self.signals.push((identity.pid, signal));
        if self.delivery_unknown_on_signal {
            SignalDisposition::DeliveryUnknown
        } else {
            SignalDisposition::Delivered
        }
    }

    fn wait_until(
        &mut self,
        duration: Duration,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<WaitOutcome, RuntimeFailure> {
        self.waits.push(duration);
        Ok(if should_stop() {
            WaitOutcome::Interrupted
        } else {
            WaitOutcome::DeadlineReached
        })
    }

    fn freeze_artifact(
        &mut self,
        _candidate: &RuntimeArtifactCandidate,
    ) -> Result<ArtifactFreeze, RuntimeFailure> {
        Ok(ArtifactFreeze::Absent)
    }
}

fn abandoned_snapshot(observed_at_unix_millis: u64) -> Snapshot {
    let process = ProcessRecord {
        identity: ProcessIdentity {
            pid: 700,
            started_at_unix_micros: (TEST_WALL_BASE_MILLIS - 120_000) * 1_000,
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
        runtime: unlinger_core::ProcessRuntimeFacts {
            descriptor_facts_complete: true,
            debug_transport_facts_complete: true,
            app_bundle: Some(AppBundleVersion {
                bundle_id: "com.google.chrome.for.testing".to_owned(),
                short_version: "151.0.7922.34".to_owned(),
            }),
            ..Default::default()
        },
    };
    Snapshot {
        observed_at_unix_millis: test_wall(observed_at_unix_millis),
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
        observed_at_unix_millis: test_wall(observed_at_unix_millis),
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
    let control =
        ControlPlane::new(store, DaemonStatus::new(mode, 999_999)).expect("restore control state");
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
fn exact_owner_protection_projects_protected_and_never_starts_cleanup() {
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(3_000),
        abandoned_snapshot(3_015),
    ]);
    let (mut engine, control, _database) = engine(DaemonMode::Enforce, runtime);
    control
        .handle_at(
            IpcCommand::Pause {
                duration_millis: 60_000,
            },
            500,
        )
        .expect("pause before owner protection");

    let first = engine.run_cycle_at(2_000).expect("observe exact incident");
    assert_eq!(first.incidents[0].state, IncidentState::Confirmed);
    let incident_id = first.incidents[0].incident_id.clone();
    control
        .store()
        .protect_incident(&incident_id, 2_100)
        .expect("protect exact incident")
        .expect("observed incident can be protected");
    control
        .handle_at(IpcCommand::Resume, 2_110)
        .expect("resume enforcement after protection");

    let protected = engine
        .run_cycle_at(4_000)
        .expect("reconcile protected incident");

    assert_eq!(protected.incidents.len(), 1);
    assert_eq!(protected.incidents[0].state, IncidentState::Protected);
    assert!(!protected.incidents[0].gates.no_protection_rule);
    assert!(
        protected.incidents[0]
            .evidence
            .iter()
            .any(|evidence| evidence.id == "protection.owner_exact_incident")
    );
    assert!(protected.cleanup_receipts.is_empty());
    assert!(engine.runtime().signals.is_empty());
    assert_eq!(control.status().expect("status").confirmed_incidents, 0);
    assert!(
        control
            .store()
            .history(100)
            .expect("history")
            .iter()
            .all(|event| !matches!(event.payload, EventPayload::Cleanup { .. }))
    );
}

#[test]
fn enforce_runs_the_frozen_cleanup_and_records_terminal_receipt() {
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(2_016),
        abandoned_snapshot(2_017),
        empty_snapshot(2_020),
        empty_snapshot(2_035),
        empty_snapshot(2_095),
        empty_snapshot(2_096),
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
fn retry_acknowledged_after_confirmation_waits_for_fresh_cooling() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open history");
    let control = ControlPlane::new(store, DaemonStatus::new(DaemonMode::Enforce, 999_999))
        .expect("restore control state");
    control
        .handle_at(
            IpcCommand::Pause {
                duration_millis: 200_000,
            },
            500,
        )
        .expect("pause while seeding durable cooling");
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(91_000),
        abandoned_snapshot(91_015),
        abandoned_snapshot(92_000),
        abandoned_snapshot(92_015),
        abandoned_snapshot(93_000),
        abandoned_snapshot(93_015),
    ]);
    let mut engine = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        control.clone(),
        EngineConfig {
            observation_gap: Duration::ZERO,
            abandonment_grace: Duration::from_secs(90),
            cleanup_policy: unlinger_core::CleanupPolicy {
                primary_term_grace: Duration::ZERO,
                member_term_grace: Duration::ZERO,
                kill_grace: Duration::ZERO,
                revival_windows: vec![Duration::ZERO, Duration::ZERO],
            },
            ..EngineConfig::default()
        },
        None,
    );

    let cooling = engine.run_cycle_at(2_000).expect("seed cooling");
    assert_eq!(cooling.incidents[0].state, IncidentState::Cooling);
    let confirmed = engine
        .run_cycle_at(91_500)
        .expect("mature durable cooling while paused");
    assert_eq!(confirmed.incidents[0].state, IncidentState::Confirmed);
    assert!(confirmed.cleanup_receipts.is_empty());

    let report = &confirmed.incidents[0];
    let attempt = control
        .store()
        .begin_cleanup_attempt(91_600, report, "seed-retry-block")
        .expect("seed failed cleanup attempt");
    control
        .store()
        .complete_cleanup_attempt(
            &attempt,
            91_610,
            &CleanupReceipt {
                incident_id: report.incident_id.clone(),
                state: IncidentState::Failed,
                reason_id: Some("cleanup.test_retry_block".to_owned()),
                actions: Vec::new(),
                artifact_actions: Vec::new(),
                survivor_pids: Vec::new(),
                revival_checks_completed: 0,
                resources: CleanupResources::default(),
            },
        )
        .expect("complete failed cleanup attempt");
    assert!(
        control
            .store()
            .cleanup_blocked(&report.incident_id)
            .expect("read retry block")
    );
    let cleanup_events_before = control
        .store()
        .history(100)
        .expect("read history before retry race")
        .into_iter()
        .filter(|event| matches!(event.payload, EventPayload::Cleanup { .. }))
        .count();
    control
        .handle_at(IpcCommand::Resume, 91_700)
        .expect("resume before retry race");

    let incident_id = report.incident_id.clone();
    let retry_control = control.clone();
    let mut stop_checks = 0;
    let raced = engine
        .run_cycle_at_until(92_500, || {
            stop_checks += 1;
            if stop_checks == 2 {
                retry_control
                    .handle_at(
                        IpcCommand::RetryFailedCleanup {
                            incident_id: incident_id.clone(),
                        },
                        92_500,
                    )
                    .expect("acknowledge retry after confirmation and before attempt start");
            }
            false
        })
        .expect("retry race remains a successful observation cycle");

    assert!(stop_checks >= 2);
    assert_eq!(raced.incidents[0].state, IncidentState::Confirmed);
    assert!(raced.cleanup_receipts.is_empty());
    assert!(engine.runtime().signals.is_empty());
    assert!(
        !control
            .store()
            .cleanup_blocked(&incident_id)
            .expect("retry block cleared")
    );
    let cleanup_events_after = control
        .store()
        .history(100)
        .expect("read history after retry race")
        .into_iter()
        .filter(|event| matches!(event.payload, EventPayload::Cleanup { .. }))
        .count();
    assert_eq!(cleanup_events_after, cleanup_events_before);

    let fresh = engine
        .run_cycle_at(93_500)
        .expect("next cycle starts a fresh cooling window");
    assert_eq!(fresh.incidents[0].state, IncidentState::Cooling);
    assert!(fresh.cleanup_receipts.is_empty());
    assert!(engine.runtime().signals.is_empty());
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
fn shutdown_requested_during_a_cycle_does_not_start_new_cleanup() {
    let runtime =
        FakeRuntime::with_snapshots(vec![abandoned_snapshot(1_000), abandoned_snapshot(1_015)]);
    let (mut engine, control, _database) = engine(DaemonMode::Enforce, runtime);

    let cycle = engine
        .run_cycle_at_until(2_000, || true)
        .expect("finish observation without starting cleanup");

    assert_eq!(cycle.incidents[0].state, IncidentState::Cooling);
    assert!(cycle.cleanup_receipts.is_empty());
    assert!(engine.runtime().signals.is_empty());
    assert_eq!(control.status().expect("status").confirmed_incidents, 0);
}

#[test]
fn post_signal_runtime_failure_is_persisted_before_cycle_error() {
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(2_016),
        abandoned_snapshot(2_017),
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
    assert_eq!(failed_event.occurred_at_unix_millis, test_wall(2_017));
    let EventPayload::Cleanup { receipt: failed } = &failed_event.payload else {
        unreachable!("failed event payload is cleanup")
    };
    assert_eq!(failed.actions.len(), 1);
    assert_eq!(failed.actions[0].pid, 700);
    assert!(!control.status().expect("status").healthy);
}

#[test]
fn delivery_unknown_fails_closed_and_persists_a_global_blocker() {
    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(2_016),
        abandoned_snapshot(2_017),
    ])
    .delivery_unknown_on_signal();
    let (mut engine, control, _database) = engine(DaemonMode::Enforce, runtime);

    let error = engine
        .run_cycle_at(2_000)
        .expect_err("unknown delivery must fail the whole enforcement cycle closed");
    assert!(error.to_string().contains("uncertain or incomplete"));

    let status = control.status().expect("failed-closed status");
    assert_eq!(status.requested_mode, DaemonMode::ReportOnly);
    assert_eq!(status.effective_mode(), DaemonMode::ReportOnly);
    assert_eq!(status.startup_state, StartupState::Failed);
    assert!(!status.healthy);
    let history = control
        .store()
        .history(10)
        .expect("history remains readable");
    let failed = history
        .iter()
        .find_map(|event| match &event.payload {
            EventPayload::Cleanup { receipt } if receipt.state == IncidentState::Failed => {
                Some(receipt)
            }
            _ => None,
        })
        .expect("terminal failed receipt persisted");
    assert_eq!(failed.actions.len(), 1);
    assert_eq!(
        failed.actions[0].disposition,
        SignalDisposition::DeliveryUnknown
    );
    assert!(
        control
            .store()
            .automatic_enforcement_blocked()
            .expect("query durable global blocker")
    );
}

#[test]
fn durable_fail_close_error_still_closes_the_live_gate_before_another_cycle() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open history");
    let control =
        ControlPlane::begin_managed(store, 13, 999_999, 500).expect("begin managed lifecycle");
    control
        .finish_startup_recovery(0, 510)
        .expect("finish recovery");
    control
        .complete_successful_cycle(520)
        .expect("complete report-only first scan");
    let instance_id = control.status().expect("ready status").instance_id;
    control
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 13,
                instance_id,
            },
            530,
        )
        .expect("arm exact managed instance");

    let connection = rusqlite::Connection::open(&database.0).expect("open failure fixture");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_fail_managed
             BEFORE UPDATE ON managed_lifecycle
             WHEN NEW.startup_phase = 'failed'
             BEGIN
               SELECT RAISE(ABORT, 'injected fail_managed failure');
             END;",
        )
        .expect("install fail_managed trigger");
    drop(connection);

    let runtime = FakeRuntime::with_snapshots(vec![
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(2_016),
        abandoned_snapshot(2_017),
        abandoned_snapshot(3_000),
        abandoned_snapshot(3_015),
    ])
    .delivery_unknown_on_signal();
    let mut managed = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        control.clone(),
        EngineConfig {
            observation_gap: Duration::ZERO,
            abandonment_grace: Duration::ZERO,
            cleanup_policy: unlinger_core::CleanupPolicy {
                primary_term_grace: Duration::ZERO,
                member_term_grace: Duration::ZERO,
                kill_grace: Duration::ZERO,
                revival_windows: Vec::new(),
            },
            ..EngineConfig::default()
        },
        None,
    );

    managed
        .run_cycle_at(2_000)
        .expect_err("delivery unknown must fail even when durable fail-close is injected");
    assert_eq!(managed.runtime().signals.len(), 1);
    let failed = control.status().expect("volatile fail-close status");
    assert_eq!(failed.requested_mode, DaemonMode::ReportOnly);
    assert_eq!(failed.effective_mode(), DaemonMode::ReportOnly);
    assert_eq!(failed.startup_state, StartupState::Failed);
    assert!(failed.enforcement_epoch.is_none());
    let failed_instance_id = failed.instance_id.clone();
    let arm_error = control
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 13,
                instance_id: failed_instance_id,
            },
            2_100,
        )
        .expect_err("volatile Failed must reject a stale durable ReadyEnforce arm");
    assert!(arm_error.to_string().contains("not ready to arm"));
    let still_failed = control
        .status()
        .expect("arm rejection preserves fail-close");
    assert_eq!(still_failed.startup_state, StartupState::Failed);
    assert_eq!(still_failed.effective_mode(), DaemonMode::ReportOnly);
    assert!(still_failed.enforcement_epoch.is_none());
    let primary_error = still_failed.last_error.clone();

    rusqlite::Connection::open(&database.0)
        .expect("open failure fixture for recovery")
        .execute_batch("DROP TRIGGER reject_fail_managed;")
        .expect("remove durable fail-close failure injection");
    let IpcPayload::Lifecycle(disarmed_failed) = control
        .handle_at(
            IpcCommand::Disarm {
                activation_generation: 13,
                instance_id: still_failed.instance_id,
            },
            2_200,
        )
        .expect("Disarm durably closes Failed without rehabilitating it")
    else {
        panic!("expected lifecycle response")
    };
    assert_eq!(disarmed_failed.startup_state, StartupState::Failed);
    assert!(!disarmed_failed.healthy);
    assert!(!disarmed_failed.ready);
    assert_eq!(disarmed_failed.effective_mode(), DaemonMode::ReportOnly);
    assert!(disarmed_failed.enforcement_epoch.is_none());
    assert_eq!(disarmed_failed.last_error, primary_error);
    assert_eq!(
        control
            .store()
            .managed_lifecycle()
            .expect("read lifecycle")
            .expect("managed lifecycle exists")
            .startup_phase,
        unlinger_daemon::ManagedStartupPhase::Failed
    );

    let retry_error = managed
        .run_cycle_at(3_500)
        .expect_err("durable lifecycle remains unhealthy under the injected trigger");
    assert!(!retry_error.to_string().contains("first report-only scan"));
    assert_eq!(
        managed.runtime().signals.len(),
        1,
        "a durable fail-close error must never reopen the old live signal gate"
    );
}

#[test]
fn managed_first_scan_is_report_only_and_only_then_becomes_armable() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open history");
    let control =
        ControlPlane::begin_managed(store, 9, 999_999, 500).expect("begin managed lifecycle");
    control
        .finish_startup_recovery(0, 510)
        .expect("finish recovery");
    let runtime =
        FakeRuntime::with_snapshots(vec![abandoned_snapshot(1_000), abandoned_snapshot(1_015)]);
    let mut managed = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        control.clone(),
        EngineConfig {
            observation_gap: Duration::ZERO,
            abandonment_grace: Duration::ZERO,
            cleanup_policy: unlinger_core::CleanupPolicy {
                primary_term_grace: Duration::ZERO,
                member_term_grace: Duration::ZERO,
                kill_grace: Duration::ZERO,
                revival_windows: Vec::new(),
            },
            ..EngineConfig::default()
        },
        None,
    );

    let first = managed.run_cycle_at(2_000).expect("first managed scan");
    assert!(first.cleanup_receipts.is_empty());
    assert!(managed.runtime().signals.is_empty());
    let status = control.status().expect("ready status");
    assert!(status.ready);
    assert_eq!(status.startup_state, StartupState::ReadyReportOnly);
    assert_eq!(status.effective_mode(), DaemonMode::ReportOnly);

    let instance_id = status.instance_id.clone();
    control
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 9,
                instance_id,
            },
            2_010,
        )
        .expect("arm after first scan");
    let armed = control.status().expect("armed status");
    assert_eq!(armed.startup_state, StartupState::ReadyEnforce);
    assert_eq!(armed.effective_mode(), DaemonMode::Enforce);
    assert!(armed.enforcement_epoch.is_some());
}

#[test]
fn carried_enforce_intent_first_scan_never_signals_then_rearms_with_a_fresh_epoch() {
    let database = TempDatabase::new();
    let first_store = HistoryStore::open(&database.0).expect("open first history");
    let first = ControlPlane::begin_managed(first_store, 11, 999_999, 500)
        .expect("begin first managed lifecycle");
    first
        .finish_startup_recovery(0, 510)
        .expect("finish first recovery");
    first
        .complete_successful_cycle(520)
        .expect("complete first instance scan");
    let first_status = first.status().expect("first ready status");
    first
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 11,
                instance_id: first_status.instance_id,
            },
            530,
        )
        .expect("arm first instance");
    let old_epoch = first
        .status()
        .expect("armed first status")
        .enforcement_epoch
        .expect("first epoch");
    drop(first);

    let replacement = ControlPlane::begin_managed(
        HistoryStore::open(&database.0).expect("reopen history"),
        11,
        999_998,
        600,
    )
    .expect("begin replacement lifecycle");
    replacement
        .finish_startup_recovery(0, 610)
        .expect("finish replacement recovery");
    let before_scan = replacement.status().expect("replacement before scan");
    assert_eq!(before_scan.requested_mode, DaemonMode::Enforce);
    assert_eq!(before_scan.effective_mode(), DaemonMode::ReportOnly);
    assert_eq!(before_scan.enforcement_epoch, None);

    let runtime =
        FakeRuntime::with_snapshots(vec![abandoned_snapshot(1_000), abandoned_snapshot(1_015)]);
    let mut managed = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        replacement.clone(),
        EngineConfig {
            observation_gap: Duration::ZERO,
            abandonment_grace: Duration::ZERO,
            cleanup_policy: unlinger_core::CleanupPolicy {
                primary_term_grace: Duration::ZERO,
                member_term_grace: Duration::ZERO,
                kill_grace: Duration::ZERO,
                revival_windows: Vec::new(),
            },
            ..EngineConfig::default()
        },
        None,
    );

    let first_scan = managed
        .run_cycle_at(2_000)
        .expect("replacement report-only first scan");
    assert!(first_scan.cleanup_receipts.is_empty());
    assert!(managed.runtime().signals.is_empty());
    let rearmed = replacement.status().expect("replacement rearmed status");
    assert_eq!(rearmed.requested_mode, DaemonMode::Enforce);
    assert_eq!(rearmed.effective_mode(), DaemonMode::Enforce);
    assert_eq!(rearmed.startup_state, StartupState::ReadyEnforce);
    assert_eq!(rearmed.armed_generation, Some(11));
    assert_ne!(
        rearmed.enforcement_epoch.as_deref(),
        Some(old_epoch.as_str())
    );
}

#[test]
fn arm_during_a_report_only_scan_waits_for_the_next_cycle() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open history");
    let control =
        ControlPlane::begin_managed(store, 14, 999_999, 500).expect("begin managed lifecycle");
    control
        .finish_startup_recovery(0, 510)
        .expect("finish recovery");
    control
        .complete_successful_cycle(520)
        .expect("complete first report-only scan");
    let instance_id = control.status().expect("ready status").instance_id;
    let runtime =
        FakeRuntime::with_snapshots(vec![abandoned_snapshot(1_000), abandoned_snapshot(1_015)])
            .owner_policy_on_snapshot(
                2,
                control.clone(),
                OwnerPolicyMutation::Arm {
                    activation_generation: 14,
                    instance_id,
                },
            );
    let mut managed = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        control.clone(),
        EngineConfig {
            observation_gap: Duration::ZERO,
            abandonment_grace: Duration::ZERO,
            cleanup_policy: unlinger_core::CleanupPolicy {
                primary_term_grace: Duration::ZERO,
                member_term_grace: Duration::ZERO,
                kill_grace: Duration::ZERO,
                revival_windows: Vec::new(),
            },
            ..EngineConfig::default()
        },
        None,
    );

    let cycle = managed
        .run_cycle_at(2_000)
        .expect("mid-scan Arm affects only the next cycle");
    assert_eq!(cycle.incidents[0].state, IncidentState::Confirmed);
    assert!(cycle.cleanup_receipts.is_empty());
    assert!(managed.runtime().signals.is_empty());
    assert_eq!(
        control.status().expect("armed status").effective_mode(),
        DaemonMode::Enforce
    );
    assert!(
        control
            .store()
            .history(100)
            .expect("history")
            .iter()
            .all(|event| !matches!(event.payload, EventPayload::Cleanup { .. }))
    );
}

#[test]
fn drain_acknowledged_after_prepare_prevents_signal_delivery() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open history");
    let control =
        ControlPlane::begin_managed(store, 10, 999_999, 500).expect("begin managed lifecycle");
    control
        .finish_startup_recovery(0, 510)
        .expect("finish recovery");
    let instance_id = control.status().expect("status").instance_id;
    let runtime = FakeRuntime::with_snapshots(vec![
        empty_snapshot(900),
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(2_016),
        abandoned_snapshot(2_017),
    ])
    .drain_on_snapshot(5, control.clone(), 10, instance_id.clone());
    let mut managed = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        control.clone(),
        EngineConfig {
            observation_gap: Duration::ZERO,
            abandonment_grace: Duration::ZERO,
            cleanup_policy: unlinger_core::CleanupPolicy {
                primary_term_grace: Duration::ZERO,
                member_term_grace: Duration::ZERO,
                kill_grace: Duration::ZERO,
                revival_windows: Vec::new(),
            },
            ..EngineConfig::default()
        },
        None,
    );

    managed.run_cycle_at(900).expect("first report-only scan");
    control
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 10,
                instance_id,
            },
            950,
        )
        .expect("arm managed daemon");
    let cycle = managed
        .run_cycle_at(2_000)
        .expect("drain produces a terminal cancelled cleanup receipt");

    assert!(managed.runtime().signals.is_empty());
    assert_eq!(cycle.cleanup_receipts.len(), 1);
    assert_eq!(cycle.cleanup_receipts[0].state, IncidentState::Failed);
    assert_eq!(
        cycle.cleanup_receipts[0].actions[0].disposition,
        SignalDisposition::CancelledBeforeDelivery
    );
    let status = control.status().expect("draining status");
    assert!(status.draining);
    assert_eq!(status.startup_state, StartupState::Draining);
    assert_eq!(status.effective_mode(), DaemonMode::ReportOnly);
}

fn assert_owner_policy_mutation_stops_inflight_cleanup(mutation: OwnerPolicyMutation) {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open history");
    let control =
        ControlPlane::begin_managed(store, 12, 999_999, 500).expect("begin managed lifecycle");
    control
        .finish_startup_recovery(0, 510)
        .expect("finish recovery");
    let instance_id = control.status().expect("status").instance_id;
    let runtime = FakeRuntime::with_snapshots(vec![
        empty_snapshot(900),
        abandoned_snapshot(1_000),
        abandoned_snapshot(1_015),
        abandoned_snapshot(2_016),
        abandoned_snapshot(2_017),
    ])
    .owner_policy_on_snapshot(5, control.clone(), mutation);
    let mut managed = ReconciliationEngine::new(
        runtime,
        RuleSet::embedded().expect("embedded rules"),
        control.clone(),
        EngineConfig {
            observation_gap: Duration::ZERO,
            abandonment_grace: Duration::ZERO,
            cleanup_policy: unlinger_core::CleanupPolicy {
                primary_term_grace: Duration::ZERO,
                member_term_grace: Duration::ZERO,
                kill_grace: Duration::ZERO,
                revival_windows: Vec::new(),
            },
            ..EngineConfig::default()
        },
        None,
    );

    managed.run_cycle_at(900).expect("first report-only scan");
    control
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 12,
                instance_id,
            },
            950,
        )
        .expect("arm managed daemon");
    let cycle = managed
        .run_cycle_at(2_000)
        .expect("owner safety mutation produces a terminal cancelled receipt");

    assert!(managed.runtime().signals.is_empty());
    assert_eq!(cycle.cleanup_receipts.len(), 1);
    assert_eq!(cycle.cleanup_receipts[0].state, IncidentState::Failed);
    assert_eq!(
        cycle.cleanup_receipts[0].actions[0].disposition,
        SignalDisposition::CancelledBeforeDelivery
    );
}

#[test]
fn pause_acknowledged_after_prepare_prevents_signal_delivery() {
    assert_owner_policy_mutation_stops_inflight_cleanup(OwnerPolicyMutation::Pause);
}

#[test]
fn protect_acknowledged_after_prepare_prevents_signal_delivery() {
    assert_owner_policy_mutation_stops_inflight_cleanup(OwnerPolicyMutation::ProtectLatestIncident);
}
