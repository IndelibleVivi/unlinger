use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use unlinger_core::{
    CleanupAction, CleanupReceipt, CleanupSignal, CleanupStage, GateLedger, IncidentReport,
    IncidentState, ProcessIdentity, ProcessRole, ProcessRoleCount, ProcessTarget, RootSummary,
    SignalDisposition,
};
use unlinger_daemon::{EventKind, HistoryStore, RetentionPolicy};

struct TempDatabase(PathBuf);

static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

impl TempDatabase {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "unlinger-history-test-{}-{nonce}-{}.sqlite3",
            std::process::id(),
            NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let path = PathBuf::from(format!("{}{}", self.0.display(), suffix));
            let _ = fs::remove_file(path);
        }
    }
}

fn confirmed_report(id: &str) -> IncidentReport {
    let identity = ProcessIdentity {
        pid: 4242,
        started_at_unix_micros: 1_700_000_000_000_000,
        executable_device: Some(1),
        executable_inode: Some(2),
    };
    IncidentReport {
        incident_id: id.to_owned(),
        tracking_key: "tracking-redacted".to_owned(),
        session_fingerprint: "session-redacted".to_owned(),
        signature_pack: "agent-browser".to_owned(),
        signature_version: "0.1.0".to_owned(),
        state: IncidentState::Confirmed,
        root: RootSummary {
            pid: identity.pid,
            started_at_unix_micros: identity.started_at_unix_micros,
            executable_basename: "node".to_owned(),
            identity_fingerprint: "identity-redacted".to_owned(),
        },
        member_count: 2,
        resident_memory_bytes: 4096,
        member_fingerprint: "members-redacted".to_owned(),
        roles: vec![ProcessRoleCount {
            role: ProcessRole::Controller,
            count: 1,
        }],
        evidence: Vec::new(),
        gates: GateLedger {
            same_user: true,
            strong_automation_provenance: true,
            confirmed_abandonment: true,
            isolated_session: true,
            stable_across_two_observations: true,
            process_identity_unchanged: true,
            no_protection_rule: true,
        },
        targets: vec![ProcessTarget {
            identity,
            process_group_id: 4242,
            role: ProcessRole::Controller,
        }],
    }
}

fn cleared_receipt(id: &str) -> CleanupReceipt {
    CleanupReceipt {
        incident_id: id.to_owned(),
        state: IncidentState::Cleared,
        reason_id: Some("cleanup.tree_gone_no_revival".to_owned()),
        actions: vec![CleanupAction {
            stage: CleanupStage::PrimaryTerm,
            pid: 4242,
            identity_fingerprint: "identity-redacted".to_owned(),
            signal: CleanupSignal::Term,
            disposition: SignalDisposition::Delivered,
        }],
        survivor_pids: Vec::new(),
        revival_checks_completed: 2,
    }
}

#[test]
fn records_redacted_observation_and_cleanup_timeline() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .record_observation(1_000, &confirmed_report("inc-1"))
        .expect("record observation");
    store
        .record_cleanup(2_000, &cleared_receipt("inc-1"))
        .expect("record cleanup");

    let history = store.history(10).expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].kind, EventKind::Cleanup);
    assert_eq!(history[0].state, IncidentState::Cleared);

    let detail = store
        .explain("inc-1")
        .expect("explain query")
        .expect("incident exists");
    assert_eq!(detail.events.len(), 2);
    let serialized = serde_json::to_string(&detail).expect("serialize detail");
    assert!(!serialized.contains("executable_device"));
    assert!(!serialized.contains("session-redacted"));
    assert!(serialized.contains("cleanup.tree_gone_no_revival"));
}

#[test]
fn retention_keeps_the_tighter_age_and_count_boundary() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    for index in 0..5 {
        store
            .record_observation(index * 1_000, &confirmed_report(&format!("inc-{index}")))
            .expect("record observation");
    }

    let result = store
        .prune(
            5_000,
            RetentionPolicy {
                max_age_millis: 3_500,
                max_events: 2,
            },
        )
        .expect("prune");
    assert_eq!(result.remaining_events, 2);
    assert_eq!(store.history(10).expect("history").len(), 2);
}

#[test]
fn pause_deadline_survives_store_reopen_and_can_be_cleared() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store.set_pause_until(Some(88_000)).expect("set pause");
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    assert_eq!(reopened.pause_until().expect("read pause"), Some(88_000));
    reopened.set_pause_until(None).expect("clear pause");
    assert_eq!(reopened.pause_until().expect("read cleared pause"), None);
}

#[test]
fn cooling_grace_is_durable_and_resets_on_identity_or_observation_gap() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let mut report = confirmed_report("inc-cooling");
    report.state = IncidentState::Cooling;
    report.gates.confirmed_abandonment = false;

    assert!(
        !store
            .track_cooling(&report, 1_000, 90_000, 120_000)
            .expect("first cooling observation")
    );
    drop(store);
    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    assert!(
        reopened
            .track_cooling(&report, 91_000, 90_000, 120_000)
            .expect("durable cooling observation")
    );

    let mut changed = report.clone();
    changed.member_fingerprint = "different-members".to_owned();
    assert!(
        !reopened
            .track_cooling(&changed, 92_000, 90_000, 120_000)
            .expect("changed membership resets grace")
    );
    assert!(
        !reopened
            .track_cooling(&changed, 300_000, 90_000, 120_000)
            .expect("long observation gap resets grace")
    );
}
