use std::collections::VecDeque;
use std::time::Duration;
use unlinger_core::{
    CleanupExecutor, CleanupPlan, CleanupPolicy, CleanupRuntime, CleanupSignal, EvidenceItem,
    GateLedger, IncidentReport, IncidentRevalidator, IncidentState, ProcessIdentity, ProcessRole,
    ProcessRoleCount, ProcessTarget, Revalidation, RevalidationPhase, RevalidationStatus,
    RootSummary, RuntimeFailure, SignalDisposition, Snapshot, SnapshotCoverage,
};

#[derive(Debug)]
struct FakeRuntime {
    snapshots: VecDeque<Snapshot>,
    signals: Vec<(u32, CleanupSignal)>,
    waits: Vec<Duration>,
}

impl CleanupRuntime for FakeRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        self.snapshots
            .pop_front()
            .ok_or_else(|| RuntimeFailure::new("test snapshot script exhausted"))
    }

    fn now_unix_millis(&self) -> Result<u64, RuntimeFailure> {
        Ok(1)
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
    first: Option<Snapshot>,
    signals: Vec<(u32, CleanupSignal)>,
}

impl CleanupRuntime for RescanFailureRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        self.first
            .take()
            .ok_or_else(|| RuntimeFailure::new("synthetic post-TERM rescan failure"))
    }

    fn now_unix_millis(&self) -> Result<u64, RuntimeFailure> {
        Ok(1)
    }

    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition {
        self.signals.push((identity.pid, signal));
        SignalDisposition::Delivered
    }

    fn wait(&mut self, _duration: Duration) {}
}

#[test]
fn term_primary_then_remaining_members_without_kill_when_they_exit() {
    let report = confirmed_report(vec![
        target(10, ProcessRole::Controller, 100),
        target(11, ProcessRole::BrowserRoot, 110),
        target(12, ProcessRole::Renderer, 120),
    ]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(10, 100), (11, 110), (12, 120)]),
            snapshot(&[(11, 110), (12, 120)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[]),
        ]),
        signals: Vec::new(),
        waits: Vec::new(),
    };
    let receipt =
        CleanupExecutor::execute(&mut runtime, &IdentityRevalidator, &plan, &fast_policy())
            .expect("execution");

    assert_eq!(receipt.state, IncidentState::Cleared);
    assert_eq!(
        runtime.signals,
        vec![
            (10, CleanupSignal::Term),
            (11, CleanupSignal::Term),
            (12, CleanupSignal::Term),
        ]
    );
}

#[test]
fn every_kill_is_preceded_by_term_for_the_same_exact_target() {
    let report = confirmed_report(vec![
        target(20, ProcessRole::BrowserRoot, 200),
        target(21, ProcessRole::Utility, 210),
    ]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[(20, 200), (21, 210)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[]),
        ]),
        signals: Vec::new(),
        waits: Vec::new(),
    };
    let receipt =
        CleanupExecutor::execute(&mut runtime, &IdentityRevalidator, &plan, &fast_policy())
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
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([snapshot(&[(30, 999)])]),
        signals: Vec::new(),
        waits: Vec::new(),
    };
    let receipt =
        CleanupExecutor::execute(&mut runtime, &IdentityRevalidator, &plan, &fast_policy())
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
    let mut runtime = FakeRuntime {
        snapshots: VecDeque::from([
            snapshot(&[(40, 400)]),
            snapshot(&[]),
            snapshot(&[]),
            snapshot(&[(41, 410)]),
        ]),
        signals: Vec::new(),
        waits: Vec::new(),
    };
    let receipt =
        CleanupExecutor::execute(&mut runtime, &RevivalRevalidator, &plan, &fast_policy())
            .expect("execution");

    assert_eq!(receipt.state, IncidentState::Revived);
    assert_eq!(runtime.signals, vec![(40, CleanupSignal::Term)]);
}

#[test]
fn runtime_failure_after_signal_retains_a_terminal_action_receipt() {
    let report = confirmed_report(vec![target(50, ProcessRole::BrowserRoot, 500)]);
    let plan = CleanupPlan::from_confirmed(&report).expect("valid plan");
    let mut runtime = RescanFailureRuntime {
        first: Some(snapshot(&[(50, 500)])),
        signals: Vec::new(),
    };

    let error = CleanupExecutor::execute(&mut runtime, &IdentityRevalidator, &plan, &fast_policy())
        .expect_err("post-signal rescan must fail");
    let receipt = error.receipt();

    assert_eq!(receipt.state, IncidentState::Failed);
    assert_eq!(
        receipt.reason_id.as_deref(),
        Some("cleanup.runtime_failure")
    );
    assert_eq!(receipt.actions.len(), 1);
    assert_eq!(receipt.actions[0].pid, 50);
    assert_eq!(runtime.signals, vec![(50, CleanupSignal::Term)]);
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
                resident_memory_bytes: 0,
                status: unlinger_core::ProcessStatus::Sleeping,
            })
            .collect(),
        coverage: SnapshotCoverage {
            listed_processes: processes.len(),
            inspected_processes: processes.len(),
            ..SnapshotCoverage::default()
        },
    }
}
