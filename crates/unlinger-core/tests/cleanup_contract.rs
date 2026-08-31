use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;
use unlinger_core::{
    ArtifactActionIntent, ArtifactDisposition, ArtifactFreeze, ArtifactOutcome,
    CleanupActionIntent, CleanupActionJournal, CleanupExecutor, CleanupOutcome, CleanupPlan,
    CleanupPolicy, CleanupRuntime, CleanupSignal, ClockSample, EvidenceItem, FrozenRuntimeArtifact,
    GateLedger, IncidentReport, IncidentRevalidator, IncidentState, OverallOutcome,
    ProcessIdentity, ProcessOutcome, ProcessRecord, ProcessRole, ProcessRoleCount, ProcessTarget,
    Revalidation, RevalidationPhase, RevalidationStatus, RootSummary, RuntimeArtifactCandidate,
    RuntimeArtifactIdentity, RuntimeFailure, SignalDisposition, Snapshot, SnapshotCoverage,
    WaitOutcome,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExecutionEvent {
    Snapshot,
    Prepared(String, CleanupSignal),
    Signal(u32, CleanupSignal),
    Completed(String, SignalDisposition),
}

#[derive(Debug)]
struct FakeRuntime {
    snapshots: VecDeque<Snapshot>,
    last_snapshot: Option<Snapshot>,
    lookup_scripts: BTreeMap<u32, VecDeque<Result<Option<ProcessRecord>, String>>>,
    signals: Vec<(u32, CleanupSignal)>,
    waits: Vec<Duration>,
    clock: Cell<u64>,
    events: Rc<RefCell<Vec<ExecutionEvent>>>,
    interrupt_next_wait: bool,
}

impl CleanupRuntime for FakeRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        self.events.borrow_mut().push(ExecutionEvent::Snapshot);
        let snapshot = self
            .snapshots
            .pop_front()
            .ok_or_else(|| RuntimeFailure::new("test snapshot script exhausted"))?;
        self.last_snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn lookup_process(
        &mut self,
        pid: u32,
    ) -> Result<Option<unlinger_core::ProcessRecord>, RuntimeFailure> {
        if let Some(result) = self
            .lookup_scripts
            .get_mut(&pid)
            .and_then(VecDeque::pop_front)
        {
            return result.map_err(RuntimeFailure::new);
        }
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
        let now = self.clock.get();
        self.clock.set(now + 1);
        Ok(ClockSample {
            wall_unix_millis: now,
            continuous_millis: now,
            boot_session_fingerprint: "test-boot-session".to_owned(),
        })
    }

    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition {
        self.signals.push((identity.pid, signal));
        self.events
            .borrow_mut()
            .push(ExecutionEvent::Signal(identity.pid, signal));
        SignalDisposition::Delivered
    }

    fn wait_until(
        &mut self,
        duration: Duration,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<WaitOutcome, RuntimeFailure> {
        self.waits.push(duration);
        if self.interrupt_next_wait {
            self.interrupt_next_wait = false;
            return Ok(WaitOutcome::Interrupted);
        }
        if should_stop() {
            Ok(WaitOutcome::Interrupted)
        } else {
            Ok(WaitOutcome::DeadlineReached)
        }
    }
}

#[derive(Debug)]
struct FakeJournal {
    next_id: usize,
    fail_completion: bool,
    prepared_times: Vec<u64>,
    completed_times: Vec<u64>,
    events: Rc<RefCell<Vec<ExecutionEvent>>>,
}

impl FakeJournal {
    fn new(events: Rc<RefCell<Vec<ExecutionEvent>>>) -> Self {
        Self {
            next_id: 1,
            fail_completion: false,
            prepared_times: Vec::new(),
            completed_times: Vec::new(),
            events,
        }
    }
}

impl CleanupActionJournal for FakeJournal {
    fn prepare_action(
        &mut self,
        intent: &CleanupActionIntent,
        prepared_at_unix_millis: u64,
    ) -> Result<String, RuntimeFailure> {
        self.prepared_times.push(prepared_at_unix_millis);
        let id = format!("action-{}", self.next_id);
        self.next_id += 1;
        self.events
            .borrow_mut()
            .push(ExecutionEvent::Prepared(id.clone(), intent.signal));
        Ok(id)
    }

    fn complete_action(
        &mut self,
        action_id: &str,
        disposition: SignalDisposition,
        completed_at_unix_millis: u64,
    ) -> Result<(), RuntimeFailure> {
        if self.fail_completion {
            return Err(RuntimeFailure::new("synthetic journal completion failure"));
        }
        self.completed_times.push(completed_at_unix_millis);
        self.events
            .borrow_mut()
            .push(ExecutionEvent::Completed(action_id.to_owned(), disposition));
        Ok(())
    }
}

struct IdentityRevalidator;

impl IncidentRevalidator for IdentityRevalidator {
    fn revalidate(
        &self,
        snapshot: &Snapshot,
        plan: &CleanupPlan,
        phase: RevalidationPhase,
    ) -> Revalidation {
        if phase == RevalidationPhase::RevivalCheck {
            return Revalidation {
                status: RevalidationStatus::Gone,
                reason_id: "test.no_revival".to_owned(),
            };
        }
        if plan.targets.iter().any(|target| {
            snapshot.processes.iter().any(|process| {
                process.pid() == target.identity.pid && process.identity != target.identity
            })
        }) {
            Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "test.identity_changed".to_owned(),
            }
        } else {
            Revalidation {
                status: RevalidationStatus::Eligible,
                reason_id: "test.eligible".to_owned(),
            }
        }
    }
}

struct RevivalRevalidator;

impl IncidentRevalidator for RevivalRevalidator {
    fn revalidate(
        &self,
        snapshot: &Snapshot,
        _plan: &CleanupPlan,
        phase: RevalidationPhase,
    ) -> Revalidation {
        let status = if phase == RevalidationPhase::RevivalCheck {
            if snapshot.processes.is_empty() {
                RevalidationStatus::Gone
            } else {
                RevalidationStatus::Revived
            }
        } else {
            RevalidationStatus::Eligible
        };
        Revalidation {
            status,
            reason_id: "test.revival_script".to_owned(),
        }
    }
}

struct RescanFailureRuntime {
    snapshots: VecDeque<Snapshot>,
    last_snapshot: Option<Snapshot>,
    lookup_scripts: BTreeMap<u32, VecDeque<Result<Option<ProcessRecord>, String>>>,
    signals: Vec<(u32, CleanupSignal)>,
    clock: Cell<u64>,
}

impl CleanupRuntime for RescanFailureRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        let snapshot = self
            .snapshots
            .pop_front()
            .ok_or_else(|| RuntimeFailure::new("synthetic post-TERM rescan failure"))?;
        self.last_snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn lookup_process(
        &mut self,
        pid: u32,
    ) -> Result<Option<unlinger_core::ProcessRecord>, RuntimeFailure> {
        if let Some(result) = self
            .lookup_scripts
            .get_mut(&pid)
            .and_then(VecDeque::pop_front)
        {
            return result.map_err(RuntimeFailure::new);
        }
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
        let now = self.clock.get();
        self.clock.set(now + 1);
        Ok(ClockSample {
            wall_unix_millis: now,
            continuous_millis: now,
            boot_session_fingerprint: "test-rescan-boot".to_owned(),
        })
    }

    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition {
        self.signals.push((identity.pid, signal));
        SignalDisposition::Delivered
    }

    fn wait_until(
        &mut self,
        _duration: Duration,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<WaitOutcome, RuntimeFailure> {
        if should_stop() {
            Ok(WaitOutcome::Interrupted)
        } else {
            Ok(WaitOutcome::DeadlineReached)
        }
    }
}

#[test]
fn term_primary_then_remaining_members_without_kill_when_they_exit() {
    let report = confirmed_report(vec![
        target(10, ProcessRole::Controller, 100),
        target(11, ProcessRole::BrowserRoot, 110),
        target(12, ProcessRole::Renderer, 120),
    ]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(10, 100), (11, 110), (12, 120)]),
            snapshot(&[(10, 100), (11, 110), (12, 120)]),
            snapshot(&[(11, 110), (12, 120)]),
            snapshot(&[(11, 110), (12, 120)]),
            snapshot(&[(11, 110), (12, 120)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[]),
        ]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(1),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::clone(&events));
    let mut should_stop = || false;
    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("execution");

    assert_eq!(receipt.state, IncidentState::Cleared);
    assert_eq!(
        receipt
            .resources
            .before
            .as_ref()
            .map(|value| (value.process_count, value.resident_memory_bytes)),
        Some((3, 3 * 1024))
    );
    assert_eq!(
        receipt
            .resources
            .after
            .as_ref()
            .map(|value| (value.process_count, value.resident_memory_bytes)),
        Some((0, 0))
    );
    assert_eq!(
        receipt.resources.estimated_reclaimed_memory_bytes,
        Some(3 * 1024)
    );
    assert_eq!(
        runtime.signals,
        vec![
            (10, CleanupSignal::Term),
            (11, CleanupSignal::Term),
            (12, CleanupSignal::Term),
        ]
    );

    let events = events.borrow();
    for (index, event) in events.iter().enumerate() {
        let ExecutionEvent::Signal(_, signal) = event else {
            continue;
        };
        assert!(matches!(
            events.get(index.wrapping_sub(2)),
            Some(ExecutionEvent::Prepared(_, prepared_signal)) if prepared_signal == signal
        ));
        assert_eq!(events.get(index - 1), Some(&ExecutionEvent::Snapshot));
        assert!(matches!(
            events.get(index + 1),
            Some(ExecutionEvent::Completed(_, SignalDisposition::Delivered))
        ));
    }
}

#[test]
fn every_kill_is_preceded_by_term_for_the_same_exact_target() {
    let report = confirmed_report(vec![
        target(20, ProcessRole::BrowserRoot, 200),
        target(21, ProcessRole::Utility, 210),
    ]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[]),
        ]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(1),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(events);
    let mut should_stop = || false;
    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("execution");

    assert_eq!(receipt.state, IncidentState::Cleared);
    for pid in [20, 21] {
        let term_index = runtime
            .signals
            .iter()
            .position(|action| *action == (pid, CleanupSignal::Term))
            .expect("TERM action");
        let kill_index = runtime
            .signals
            .iter()
            .position(|action| *action == (pid, CleanupSignal::Kill))
            .expect("KILL action");
        assert!(term_index < kill_index);
    }
}

#[test]
fn pid_reuse_aborts_before_any_signal() {
    let report = confirmed_report(vec![target(30, ProcessRole::BrowserRoot, 300)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(30, 999)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(1),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(events);
    let mut should_stop = || false;
    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("execution");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert!(runtime.signals.is_empty());
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.identity_changed")
    );
}

#[test]
fn revival_is_attributed_once_without_an_automatic_kill_loop() {
    let report = confirmed_report(vec![target(40, ProcessRole::BrowserRoot, 400)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(40, 400)]),
            snapshot(&[(40, 400)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[(41, 410)]),
        ]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(1),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(events);
    let mut should_stop = || false;
    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &RevivalRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("execution");

    assert_eq!(receipt.state, IncidentState::Revived);
    assert_eq!(runtime.signals, vec![(40, CleanupSignal::Term)]);
}

#[test]
fn runtime_failure_after_signal_retains_a_terminal_action_receipt() {
    let report = confirmed_report(vec![target(50, ProcessRole::BrowserRoot, 500)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let mut runtime = RescanFailureRuntime {
        snapshots: VecDeque::from([snapshot(&[(50, 500)]), snapshot(&[(50, 500)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        clock: Cell::new(1),
    };
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut journal = FakeJournal::new(events);
    let mut should_stop = || false;

    let error = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect_err("post-signal rescan must fail");
    let receipt = error
        .terminal_receipt()
        .expect("post-action rescan failure has a terminal receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.runtime_failure")
    );
    assert_eq!(receipt.actions.len(), 1);
    assert_eq!(receipt.actions[0].pid, 50);
    assert_eq!(runtime.signals, vec![(50, CleanupSignal::Term)]);
}

#[test]
fn every_signal_is_freshly_revalidated_after_the_durable_prepare() {
    let report = confirmed_report(vec![target(60, ProcessRole::BrowserRoot, 600)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(60, 600)]), snapshot(&[(60, 999)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(10),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::clone(&events));
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("identity mismatch is a terminal cleanup receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.identity_changed")
    );
    assert!(runtime.signals.is_empty());
    assert_eq!(journal.prepared_times, vec![10]);
    assert_eq!(journal.completed_times, vec![11]);
    assert!(matches!(
        events.borrow().as_slice(),
        [
            ExecutionEvent::Snapshot,
            ExecutionEvent::Prepared(_, CleanupSignal::Term),
            ExecutionEvent::Snapshot,
            ExecutionEvent::Completed(_, SignalDisposition::IdentityMismatch)
        ]
    ));
}

#[test]
fn missing_frozen_target_lookup_failure_rejects_the_prepared_signal() {
    let report = confirmed_report(vec![
        target(61, ProcessRole::Controller, 610),
        target(62, ProcessRole::BrowserRoot, 620),
    ]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(61, 610), (62, 620)]), snapshot(&[(62, 620)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::from([(
            61,
            VecDeque::from([Err("synthetic targeted lookup failure".to_owned())]),
        )]),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(10),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::clone(&events));
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("lookup failure is a terminal receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.target_lookup_incomplete")
    );
    assert!(runtime.signals.is_empty());
    assert_eq!(receipt.actions.len(), 1);
    assert_eq!(receipt.actions[0].disposition, SignalDisposition::Rejected);
}

#[test]
fn targeted_live_record_missing_from_snapshot_never_authorizes_a_signal() {
    let report = confirmed_report(vec![
        target(63, ProcessRole::Controller, 630),
        target(64, ProcessRole::BrowserRoot, 640),
    ]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let controller = process_record(63, 630);
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(63, 630), (64, 640)]), snapshot(&[(64, 640)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::from([(63, VecDeque::from([Ok(Some(controller))]))]),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(10),
        events,
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::new(RefCell::new(Vec::new())));
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("incoherent action-boundary snapshot is terminal");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.snapshot_target_inconsistent")
    );
    assert!(runtime.signals.is_empty());
    assert_eq!(receipt.actions[0].disposition, SignalDisposition::Rejected);
}

#[test]
fn terminal_target_lookup_failure_prevents_a_cleared_receipt() {
    let report = confirmed_report(vec![target(65, ProcessRole::BrowserRoot, 650)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(65, 650)]),
            snapshot(&[(65, 650)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[]),
        ]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::from([(
            65,
            VecDeque::from([
                Ok(None),
                Err("synthetic terminal lookup failure".to_owned()),
            ]),
        )]),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(10),
        events,
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::new(RefCell::new(Vec::new())));
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("terminal lookup failure is a terminal receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.target_lookup_incomplete")
    );
    assert_eq!(runtime.signals, vec![(65, CleanupSignal::Term)]);
}

#[test]
fn terminal_targeted_live_process_prevents_a_cleared_receipt() {
    let report = confirmed_report(vec![target(66, ProcessRole::BrowserRoot, 660)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(66, 660)]),
            snapshot(&[(66, 660)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[]),
        ]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::from([(
            66,
            VecDeque::from([Ok(None), Ok(Some(process_record(66, 660)))]),
        )]),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(10),
        events,
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::new(RefCell::new(Vec::new())));
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("terminal exact survivor is a terminal receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.terminal_tree_not_gone")
    );
    assert_eq!(receipt.survivor_pids, vec![66]);
    assert_eq!(runtime.signals, vec![(66, CleanupSignal::Term)]);
}

#[test]
fn drain_after_prepare_is_journalled_and_never_delivers_a_signal() {
    let report = confirmed_report(vec![target(70, ProcessRole::BrowserRoot, 700)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(70, 700)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(20),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::clone(&events));
    let stop_checks = Cell::new(0_u8);
    let mut should_stop = || {
        let next = stop_checks.get() + 1;
        stop_checks.set(next);
        next >= 3
    };

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("drain is a terminal failed receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.drain_requested")
    );
    assert!(runtime.signals.is_empty());
    assert_eq!(receipt.actions.len(), 1);
    assert_eq!(
        receipt.actions[0].disposition,
        SignalDisposition::CancelledBeforeDelivery
    );
    assert!(matches!(
        events.borrow().as_slice(),
        [
            ExecutionEvent::Snapshot,
            ExecutionEvent::Prepared(_, CleanupSignal::Term),
            ExecutionEvent::Completed(_, SignalDisposition::CancelledBeforeDelivery)
        ]
    ));
}

#[test]
fn drain_during_term_grace_stops_escalation_and_revival_waits() {
    let report = confirmed_report(vec![target(80, ProcessRole::BrowserRoot, 800)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(80, 800)]), snapshot(&[(80, 800)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(30),
        events: Rc::clone(&events),
        interrupt_next_wait: true,
    };
    let mut journal = FakeJournal::new(events);
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect("interrupted grace has a terminal failed receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.drain_requested")
    );
    assert_eq!(runtime.signals, vec![(80, CleanupSignal::Term)]);
    assert_eq!(runtime.waits, vec![Duration::ZERO]);
    assert_eq!(receipt.revival_checks_completed, 0);
}

#[test]
fn post_signal_journal_failure_keeps_the_attempt_open_without_terminal_receipt() {
    let report = confirmed_report(vec![target(90, ProcessRole::BrowserRoot, 900)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(90, 900)]), snapshot(&[(90, 900)])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        signals: Vec::new(),
        waits: Vec::new(),
        clock: Cell::new(40),
        events: Rc::clone(&events),
        interrupt_next_wait: false,
    };
    let mut journal = FakeJournal::new(Rc::clone(&events));
    journal.fail_completion = true;
    let mut should_stop = || false;

    let error = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &fast_policy(),
    )
    .expect_err("undurable signal completion must remain open");

    assert!(error.terminal_receipt().is_none());
    assert_eq!(error.prepared_action_id(), Some("action-1"));
    assert_eq!(error.partial_receipt().state, IncidentState::Reclaiming);
    assert!(error.partial_receipt().actions.is_empty());
    assert_eq!(runtime.signals, vec![(90, CleanupSignal::Term)]);
    assert!(matches!(
        events.borrow().as_slice(),
        [
            ExecutionEvent::Snapshot,
            ExecutionEvent::Prepared(_, CleanupSignal::Term),
            ExecutionEvent::Snapshot,
            ExecutionEvent::Signal(90, CleanupSignal::Term)
        ]
    ));
}

#[derive(Debug)]
struct ArtifactRuntime {
    snapshots: VecDeque<Snapshot>,
    last_snapshot: Option<Snapshot>,
    lookup_scripts: BTreeMap<u32, VecDeque<Result<Option<ProcessRecord>, String>>>,
    clock: Cell<u64>,
    freezes: VecDeque<ArtifactFreeze>,
    disposition: ArtifactDisposition,
    remove_calls: usize,
    journal_prepared: Rc<Cell<bool>>,
}

impl CleanupRuntime for ArtifactRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        let snapshot = self
            .snapshots
            .pop_front()
            .ok_or_else(|| RuntimeFailure::new("artifact snapshot script exhausted"))?;
        self.last_snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn lookup_process(
        &mut self,
        pid: u32,
    ) -> Result<Option<unlinger_core::ProcessRecord>, RuntimeFailure> {
        if let Some(result) = self
            .lookup_scripts
            .get_mut(&pid)
            .and_then(VecDeque::pop_front)
        {
            return result.map_err(RuntimeFailure::new);
        }
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
        let now = self.clock.get();
        self.clock.set(now + 1);
        Ok(ClockSample {
            wall_unix_millis: now,
            continuous_millis: now,
            boot_session_fingerprint: "artifact-test-boot".to_owned(),
        })
    }

    fn signal_exact(
        &mut self,
        _identity: &ProcessIdentity,
        _signal: CleanupSignal,
    ) -> SignalDisposition {
        panic!("artifact-only cleanup must not signal")
    }

    fn wait_until(
        &mut self,
        _duration: Duration,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<WaitOutcome, RuntimeFailure> {
        if should_stop() {
            Ok(WaitOutcome::Interrupted)
        } else {
            Ok(WaitOutcome::DeadlineReached)
        }
    }

    fn freeze_artifact(
        &mut self,
        _candidate: &RuntimeArtifactCandidate,
    ) -> Result<ArtifactFreeze, RuntimeFailure> {
        self.freezes
            .pop_front()
            .ok_or_else(|| RuntimeFailure::new("artifact freeze script exhausted"))
    }

    fn remove_artifact_exact(&mut self, _artifact: &FrozenRuntimeArtifact) -> ArtifactDisposition {
        assert!(
            self.journal_prepared.get(),
            "unlink must occur only after PREPARED is durable"
        );
        self.remove_calls += 1;
        self.disposition
    }
}

#[derive(Debug)]
struct ArtifactJournal {
    prepared: Rc<Cell<bool>>,
    completed: Vec<ArtifactDisposition>,
    fail_completion: bool,
}

impl CleanupActionJournal for ArtifactJournal {
    fn prepare_action(
        &mut self,
        _intent: &CleanupActionIntent,
        _prepared_at_unix_millis: u64,
    ) -> Result<String, RuntimeFailure> {
        Err(RuntimeFailure::new("unexpected signal action"))
    }

    fn complete_action(
        &mut self,
        _action_id: &str,
        _disposition: SignalDisposition,
        _completed_at_unix_millis: u64,
    ) -> Result<(), RuntimeFailure> {
        Err(RuntimeFailure::new("unexpected signal action"))
    }

    fn prepare_artifact_action(
        &mut self,
        intent: &ArtifactActionIntent,
        _prepared_at_unix_millis: u64,
    ) -> Result<String, RuntimeFailure> {
        assert_eq!(
            intent.artifact_fingerprint,
            artifact_candidate().artifact_fingerprint()
        );
        self.prepared.set(true);
        Ok("artifact-action-1".to_owned())
    }

    fn complete_artifact_action(
        &mut self,
        action_id: &str,
        disposition: ArtifactDisposition,
        _completed_at_unix_millis: u64,
    ) -> Result<(), RuntimeFailure> {
        assert_eq!(action_id, "artifact-action-1");
        if self.fail_completion {
            return Err(RuntimeFailure::new(
                "synthetic artifact journal completion failure",
            ));
        }
        self.completed.push(disposition);
        Ok(())
    }
}

#[test]
fn artifact_unlink_is_journalled_after_tree_death_and_no_revival() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::Removed);
    let mut journal = ArtifactJournal {
        prepared,
        completed: Vec::new(),
        fail_completion: false,
    };
    let report = report_with_artifact();
    let plan = CleanupPlan::from_confirmed(&report).expect("artifact cleanup plan");
    let mut should_stop = || false;
    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect("artifact execution");

    assert_eq!(receipt.state, IncidentState::Cleared);
    assert_eq!(runtime.remove_calls, 1);
    assert_eq!(journal.completed, vec![ArtifactDisposition::Removed]);
    assert_eq!(receipt.artifact_actions.len(), 1);
    assert_eq!(
        receipt.artifact_actions[0].disposition,
        ArtifactDisposition::Removed
    );
    let json = serde_json::to_string(&receipt).expect("redacted receipt JSON");
    assert!(!json.contains("playwright_chromiumdev_profile-private"));
    assert!(!json.contains("DevToolsActivePort"));
    assert!(json.contains("artifact_fingerprint"));
}

#[test]
fn revival_returns_before_any_artifact_action() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::Removed);
    runtime.snapshots = VecDeque::from([snapshot(&[]), snapshot(&[(999, 999)])]);
    let mut journal = ArtifactJournal {
        prepared: Rc::clone(&prepared),
        completed: Vec::new(),
        fail_completion: false,
    };
    let plan = CleanupPlan::from_confirmed(&report_with_artifact()).expect("artifact plan");
    let mut should_stop = || false;
    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &RevivalRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect("revival execution");

    assert_eq!(receipt.state, IncidentState::Revived);
    assert_eq!(runtime.remove_calls, 0);
    assert!(!prepared.get());
    assert!(receipt.artifact_actions.is_empty());
}

#[test]
fn artifact_completion_crash_leaves_the_prepared_attempt_open() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::Removed);
    let mut journal = ArtifactJournal {
        prepared,
        completed: Vec::new(),
        fail_completion: true,
    };
    let plan = CleanupPlan::from_confirmed(&report_with_artifact()).expect("artifact plan");
    let mut should_stop = || false;
    let error = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect_err("completion crash must keep PREPARED open");

    assert!(error.terminal_receipt().is_none());
    assert_eq!(error.prepared_action_id(), Some("artifact-action-1"));
    assert_eq!(runtime.remove_calls, 1);
    assert!(error.partial_receipt().artifact_actions.is_empty());
}

#[test]
fn revival_during_artifact_work_prevents_a_cleared_receipt() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::Removed);
    runtime.snapshots = VecDeque::from([snapshot(&[]), snapshot(&[]), snapshot(&[(999, 999)])]);
    let mut journal = ArtifactJournal {
        prepared,
        completed: Vec::new(),
        fail_completion: false,
    };
    let plan = CleanupPlan::from_confirmed(&report_with_artifact()).expect("artifact plan");
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &RevivalRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect("post-artifact revival is terminal");

    assert_eq!(receipt.state, IncidentState::Revived);
    assert_eq!(receipt.reason_id.as_deref(), Some("test.revival_script"));
    assert_eq!(runtime.remove_calls, 1);
    assert_eq!(journal.completed, vec![ArtifactDisposition::Removed]);
}

#[test]
fn post_artifact_target_lookup_failure_cannot_prove_cleared() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::Removed);
    runtime.lookup_scripts.insert(
        100,
        VecDeque::from([
            Ok(None),
            Ok(None),
            Err("synthetic post-artifact lookup failure".to_owned()),
        ]),
    );
    let mut journal = ArtifactJournal {
        prepared,
        completed: Vec::new(),
        fail_completion: false,
    };
    let plan = CleanupPlan::from_confirmed(&report_with_artifact()).expect("artifact plan");
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect("incomplete post-artifact proof is terminal");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.target_lookup_incomplete")
    );
    assert_eq!(runtime.remove_calls, 1);
    assert_eq!(journal.completed, vec![ArtifactDisposition::Removed]);
}

#[test]
fn artifact_appearing_after_initial_absence_is_not_removed() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::Removed);
    runtime.freezes = VecDeque::from([
        ArtifactFreeze::Absent,
        ArtifactFreeze::Frozen(frozen_artifact()),
    ]);
    let mut journal = ArtifactJournal {
        prepared,
        completed: Vec::new(),
        fail_completion: false,
    };
    let plan = CleanupPlan::from_confirmed(&report_with_artifact()).expect("artifact plan");
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect("late artifact is a terminal failed receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.artifact_identity_changed")
    );
    assert_eq!(runtime.remove_calls, 0);
    assert_eq!(
        journal.completed,
        vec![ArtifactDisposition::IdentityMismatch]
    );
    assert_eq!(
        receipt.artifact_actions[0].disposition,
        ArtifactDisposition::IdentityMismatch
    );
    assert_eq!(
        receipt.outcome(),
        CleanupOutcome {
            process: ProcessOutcome::Cleared,
            artifact: ArtifactOutcome::Residue,
            overall: OverallOutcome::ClearedWithResidue,
            attention_required: true,
        }
    );
}

#[test]
fn replacement_after_exact_removal_prevents_cleared_receipt() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::Removed);
    runtime.freezes = VecDeque::from([
        ArtifactFreeze::Frozen(frozen_artifact()),
        ArtifactFreeze::Frozen(frozen_artifact()),
    ]);
    let mut journal = ArtifactJournal {
        prepared,
        completed: Vec::new(),
        fail_completion: false,
    };
    let plan = CleanupPlan::from_confirmed(&report_with_artifact()).expect("artifact plan");
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect("late replacement is a terminal failed receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.artifact_identity_changed")
    );
    assert_eq!(runtime.remove_calls, 1);
    assert_eq!(journal.completed, vec![ArtifactDisposition::Removed]);
    assert_eq!(
        receipt.artifact_actions[0].disposition,
        ArtifactDisposition::Removed
    );
    assert_eq!(
        receipt.outcome(),
        CleanupOutcome {
            process: ProcessOutcome::Cleared,
            artifact: ArtifactOutcome::Residue,
            overall: OverallOutcome::ClearedWithResidue,
            attention_required: true,
        }
    );
}

#[test]
fn artifact_delivery_unknown_preserves_confirmed_process_success_but_fails_overall() {
    let prepared = Rc::new(Cell::new(false));
    let mut runtime = artifact_runtime(Rc::clone(&prepared), ArtifactDisposition::DeliveryUnknown);
    let mut journal = ArtifactJournal {
        prepared,
        completed: Vec::new(),
        fail_completion: false,
    };
    let plan = CleanupPlan::from_confirmed(&report_with_artifact()).expect("artifact plan");
    let mut should_stop = || false;

    let receipt = CleanupExecutor::execute(
        &mut runtime,
        &IdentityRevalidator,
        &mut journal,
        &mut should_stop,
        &plan,
        &artifact_policy(),
    )
    .expect("delivery-unknown artifact receipt");

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.outcome(),
        CleanupOutcome {
            process: ProcessOutcome::Cleared,
            artifact: ArtifactOutcome::DeliveryUnknown,
            overall: OverallOutcome::Failed,
            attention_required: true,
        }
    );
}

#[test]
fn older_cleanup_receipt_json_defaults_new_resource_and_artifact_fields() {
    let receipt: unlinger_core::CleanupReceipt = serde_json::from_str(
        r#"{
            "incident_id":"inc-old",
            "state":"CLEARED",
            "actions":[],
            "survivor_pids":[],
            "revival_checks_completed":2
        }"#,
    )
    .expect("decode pre-artifact receipt");

    assert!(receipt.artifact_actions.is_empty());
    assert_eq!(
        receipt.resources,
        unlinger_core::CleanupResources::default()
    );
}

#[test]
fn retained_pre_projection_artifact_failure_normalizes_without_a_store_migration() {
    let receipt: unlinger_core::CleanupReceipt = serde_json::from_str(
        r#"{
            "incident_id":"inc-retained-residue",
            "state":"FAILED",
            "reason_id":"cleanup.artifact_unsafe",
            "actions":[{
                "stage":"primary_term",
                "pid":42,
                "identity_fingerprint":"process-redacted",
                "signal":"term",
                "disposition":"delivered"
            }],
            "artifact_actions":[{
                "kind":"dev_tools_active_port",
                "artifact_fingerprint":"artifact-redacted",
                "disposition":"unsafe"
            }],
            "survivor_pids":[],
            "revival_checks_completed":2
        }"#,
    )
    .expect("decode retained residue receipt");

    assert_eq!(
        receipt.outcome(),
        CleanupOutcome {
            process: ProcessOutcome::Cleared,
            artifact: ArtifactOutcome::Residue,
            overall: OverallOutcome::ClearedWithResidue,
            attention_required: true,
        }
    );
}

fn artifact_runtime(
    journal_prepared: Rc<Cell<bool>>,
    disposition: ArtifactDisposition,
) -> ArtifactRuntime {
    ArtifactRuntime {
        snapshots: VecDeque::from([snapshot(&[]), snapshot(&[]), snapshot(&[])]),
        last_snapshot: None,
        lookup_scripts: BTreeMap::new(),
        clock: Cell::new(1),
        freezes: VecDeque::from([
            ArtifactFreeze::Frozen(frozen_artifact()),
            ArtifactFreeze::Absent,
        ]),
        disposition,
        remove_calls: 0,
        journal_prepared,
    }
}

fn frozen_artifact() -> FrozenRuntimeArtifact {
    FrozenRuntimeArtifact::new(
        artifact_candidate(),
        RuntimeArtifactIdentity {
            device: 1,
            inode: 2,
            owner_uid: 501,
            mode: 0o100600,
            link_count: 1,
            parent_device: 1,
            parent_inode: 1,
            parent_owner_uid: 501,
            parent_mode: 0o40700,
        },
    )
}

fn artifact_candidate() -> RuntimeArtifactCandidate {
    RuntimeArtifactCandidate::devtools_active_port(
        std::path::Path::new("/private/tmp/playwright_chromiumdev_profile-private/managed-session"),
        501,
        "session-test",
    )
    .expect("test artifact candidate")
}

fn report_with_artifact() -> IncidentReport {
    let mut report = confirmed_report(vec![target(100, ProcessRole::BrowserRoot, 100)]);
    report.runtime_artifacts = vec![artifact_candidate()];
    report
}

fn artifact_policy() -> CleanupPolicy {
    CleanupPolicy {
        primary_term_grace: Duration::ZERO,
        member_term_grace: Duration::ZERO,
        kill_grace: Duration::ZERO,
        revival_windows: vec![Duration::ZERO],
    }
}

fn fast_policy() -> CleanupPolicy {
    CleanupPolicy {
        primary_term_grace: Duration::ZERO,
        member_term_grace: Duration::ZERO,
        kill_grace: Duration::ZERO,
        revival_windows: vec![Duration::ZERO, Duration::ZERO],
    }
}

fn confirmed_report(targets: Vec<ProcessTarget>) -> IncidentReport {
    IncidentReport {
        incident_id: "inc-test".to_owned(),
        tracking_key: "trk-test".to_owned(),
        session_fingerprint: "session-test".to_owned(),
        signature_pack: "agent-browser".to_owned(),
        signature_version: "0.1.0".to_owned(),
        state: IncidentState::Confirmed,
        root: RootSummary {
            pid: targets[0].identity.pid,
            started_at_unix_micros: targets[0].identity.started_at_unix_micros,
            executable_basename: "synthetic".to_owned(),
            identity_fingerprint: "identity-test".to_owned(),
        },
        member_count: targets.len(),
        resident_memory_bytes: 0,
        member_fingerprint: "members-test".to_owned(),
        roles: vec![ProcessRoleCount {
            role: ProcessRole::BrowserRoot,
            count: 1,
        }],
        evidence: vec![EvidenceItem {
            id: "test".to_owned(),
            family: unlinger_core::EvidenceFamily::AutomationProvenance,
            source_pid: None,
        }],
        gates: GateLedger {
            same_user: true,
            strong_automation_provenance: true,
            confirmed_abandonment: true,
            isolated_session: true,
            stable_across_two_observations: true,
            process_identity_unchanged: true,
            no_protection_rule: true,
        },
        targets,
        runtime_artifacts: Vec::new(),
    }
}

fn target(pid: u32, role: ProcessRole, started_at: u64) -> ProcessTarget {
    ProcessTarget {
        identity: identity(pid, started_at),
        process_group_id: pid,
        role,
    }
}

fn identity(pid: u32, started_at: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        started_at_unix_micros: started_at,
        executable_device: Some(1),
        executable_inode: Some(u64::from(pid)),
    }
}

fn process_record(pid: u32, started_at: u64) -> ProcessRecord {
    snapshot(&[(pid, started_at)])
        .processes
        .into_iter()
        .next()
        .expect("one synthetic process")
}

fn snapshot(processes: &[(u32, u64)]) -> Snapshot {
    Snapshot {
        observed_at_unix_millis: 1,
        current_uid: 501,
        processes: processes
            .iter()
            .map(|(pid, started_at)| unlinger_core::ProcessRecord {
                identity: identity(*pid, *started_at),
                parent_pid: 1,
                process_group_id: *pid,
                uid: 501,
                tty_device: None,
                name: "synthetic".to_owned(),
                executable_path: Some("/private/tmp/synthetic".to_owned()),
                executable: unlinger_core::ExecutableIdentity {
                    device: Some(1),
                    inode: Some(u64::from(*pid)),
                    size: Some(1),
                    modified_unix_nanos: Some(1),
                },
                arguments: Some(vec!["synthetic".to_owned()]),
                resident_memory_bytes: 1024,
                status: unlinger_core::ProcessStatus::Sleeping,
                runtime: Default::default(),
            })
            .collect(),
        coverage: SnapshotCoverage {
            listed_processes: processes.len(),
            inspected_processes: processes.len(),
            ..SnapshotCoverage::default()
        },
    }
}
