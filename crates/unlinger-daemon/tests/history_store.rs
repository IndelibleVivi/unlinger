use rusqlite::{Connection, params};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use unlinger_core::{
    ArtifactAction, ArtifactActionIntent, ArtifactDisposition, CleanupAction, CleanupActionIntent,
    CleanupActionJournal, CleanupReceipt, CleanupResources, CleanupSignal, CleanupStage,
    GateLedger, IncidentReport, IncidentState, ProcessIdentity, ProcessRole, ProcessRoleCount,
    ProcessTarget, ResourceSnapshot, RootSummary, RuntimeArtifactKind, SignalDisposition,
    StorageResidueKind, StorageResidueObservation, StorageResidueReferenceCheck,
    StorageResidueStatus,
};
use unlinger_daemon::{
    CoolingClock, EventKind, EventPayload, HistoryStore, ImpactHistoryCompleteness,
    ManagedStartupPhase, MutationLookup, ObservationRecord, ObservedIncidentIdentity,
    OrdinaryMutation, RetentionPolicy, StorageRecoveryReason, StoreError,
};
use unlinger_protocol::{MutationContext, MutationKind, MutationOutcome, MutationResult};

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

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "unlinger-{label}-{}-{nonce}-{}",
            std::process::id(),
            NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create temporary directory");
        Self(path)
    }

    fn database(&self) -> PathBuf {
        self.0.join("history.sqlite3")
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn make_private(path: &PathBuf) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .expect("set private fixture permissions");
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
        browser_compatibility: Default::default(),
        targets: vec![ProcessTarget {
            identity,
            process_group_id: 4242,
            role: ProcessRole::Controller,
        }],
        runtime_artifacts: Vec::new(),
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
        artifact_actions: Vec::new(),
        survivor_pids: Vec::new(),
        revival_checks_completed: 2,
        resources: CleanupResources::default(),
    }
}

fn cleared_with_residue_receipt(id: &str) -> CleanupReceipt {
    CleanupReceipt {
        incident_id: id.to_owned(),
        state: IncidentState::Failed,
        reason_id: Some("cleanup.artifact_unsafe".to_owned()),
        actions: vec![CleanupAction {
            stage: CleanupStage::PrimaryTerm,
            pid: 4242,
            identity_fingerprint: "identity-redacted".to_owned(),
            signal: CleanupSignal::Term,
            disposition: SignalDisposition::Delivered,
        }],
        artifact_actions: vec![ArtifactAction {
            kind: RuntimeArtifactKind::DevToolsActivePort,
            artifact_fingerprint: "artifact-redacted".to_owned(),
            disposition: ArtifactDisposition::Unsafe,
        }],
        survivor_pids: Vec::new(),
        revival_checks_completed: 2,
        resources: CleanupResources::default(),
    }
}

fn primary_term_intent() -> CleanupActionIntent {
    CleanupActionIntent {
        stage: CleanupStage::PrimaryTerm,
        pid: 4242,
        identity_fingerprint: "identity-redacted".to_owned(),
        signal: CleanupSignal::Term,
    }
}

fn artifact_intent() -> ArtifactActionIntent {
    ArtifactActionIntent {
        kind: RuntimeArtifactKind::DevToolsActivePort,
        artifact_fingerprint: "art-redacted-fixture".to_owned(),
    }
}

fn cooling_clock(
    wall_unix_millis: u64,
    continuous_millis: u64,
    boot_session_fingerprint: &str,
    enforcement_epoch: &str,
) -> CoolingClock {
    CoolingClock {
        wall_unix_millis,
        continuous_millis,
        boot_session_fingerprint: boot_session_fingerprint.to_owned(),
        enforcement_epoch: enforcement_epoch.to_owned(),
    }
}

fn cooling_report(id: &str) -> IncidentReport {
    let mut report = confirmed_report(id);
    report.state = IncidentState::Cooling;
    report.gates.confirmed_abandonment = false;
    report
}

fn failed_receipt(id: &str, state: IncidentState, reason_id: &str) -> CleanupReceipt {
    CleanupReceipt {
        incident_id: id.to_owned(),
        state,
        reason_id: Some(reason_id.to_owned()),
        actions: Vec::new(),
        artifact_actions: Vec::new(),
        survivor_pids: vec![4242],
        revival_checks_completed: usize::from(state == IncidentState::Revived),
        resources: CleanupResources::default(),
    }
}

fn create_v2_database(path: &PathBuf) {
    let connection = Connection::open(path).expect("open legacy database");
    connection
        .execute_batch(
            "CREATE TABLE events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 incident_id TEXT NOT NULL,
                 occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
                 kind TEXT NOT NULL CHECK (kind IN ('observation', 'cleanup')),
                 state TEXT NOT NULL,
                 payload_json TEXT NOT NULL
             );
             CREATE INDEX events_incident_timeline
                 ON events (incident_id, occurred_at_ms, id);
             CREATE INDEX events_recent
                 ON events (occurred_at_ms DESC, id DESC);
             CREATE TABLE settings (
                 key TEXT PRIMARY KEY,
                 integer_value INTEGER
             );
             CREATE TABLE cooling_candidates (
                 tracking_key TEXT PRIMARY KEY,
                 first_seen_ms INTEGER NOT NULL CHECK (first_seen_ms >= 0),
                 last_seen_ms INTEGER NOT NULL CHECK (last_seen_ms >= first_seen_ms),
                 root_identity_fingerprint TEXT NOT NULL,
                 member_fingerprint TEXT NOT NULL
             );
             PRAGMA user_version = 2;",
        )
        .expect("create legacy schema");
    let report = confirmed_report("legacy-incident");
    let payload = serde_json::to_string(&EventPayload::Observation {
        report: ObservationRecord::from(&report),
    })
    .expect("serialize legacy event");
    connection
        .execute(
            "INSERT INTO events (
                 incident_id, occurred_at_ms, kind, state, payload_json
             ) VALUES (?1, ?2, 'observation', 'CONFIRMED', ?3)",
            params![report.incident_id, 1_000_i64, payload],
        )
        .expect("insert legacy event");
    connection
        .execute(
            "INSERT INTO settings (key, integer_value)
             VALUES ('pause_until_ms', 88000)",
            [],
        )
        .expect("insert legacy pause");
    connection
        .execute(
            "INSERT INTO cooling_candidates (
                 tracking_key, first_seen_ms, last_seen_ms,
                 root_identity_fingerprint, member_fingerprint
             ) VALUES (?1, 1000, 91000, ?2, ?3)",
            params![
                report.tracking_key,
                report.root.identity_fingerprint,
                report.member_fingerprint
            ],
        )
        .expect("insert legacy cooling");
    drop(connection);
    make_private(path);
}

fn downgrade_current_database_to_v4(path: &PathBuf) {
    let connection = Connection::open(path).expect("open current database for v4 fixture");
    connection
        .execute_batch(
            "DROP TABLE storage_residue_latest;
             DROP TABLE cleanup_impacts;
             DROP TABLE impact_authority;
             DROP TABLE ordinary_mutation_receipts;
             DROP TABLE control_metadata;
             DROP TABLE mutation_authority;
             DROP TABLE public_event_tokens;
             DROP TABLE cleanup_artifact_actions;
             DROP TABLE incident_protections;
             DROP TABLE storage_recoveries;
             ALTER TABLE cleanup_attempts DROP COLUMN resources_json;
             ALTER TABLE cooling_candidates RENAME TO cooling_candidates_v5;
             CREATE TABLE cooling_candidates (
                 tracking_key TEXT PRIMARY KEY,
                 first_continuous_ms INTEGER NOT NULL CHECK (first_continuous_ms >= 0),
                 last_continuous_ms INTEGER NOT NULL
                     CHECK (last_continuous_ms >= first_continuous_ms),
                 first_wall_ms INTEGER NOT NULL CHECK (first_wall_ms >= 0),
                 last_wall_ms INTEGER NOT NULL CHECK (last_wall_ms >= 0),
                 root_identity_fingerprint TEXT NOT NULL,
                 member_fingerprint TEXT NOT NULL,
                 boot_session_fingerprint TEXT NOT NULL,
                 enforcement_epoch TEXT NOT NULL
             );
             INSERT INTO cooling_candidates (
                 tracking_key, first_continuous_ms, last_continuous_ms,
                 first_wall_ms, last_wall_ms, root_identity_fingerprint,
                 member_fingerprint, boot_session_fingerprint, enforcement_epoch
             ) SELECT
                 tracking_key, first_continuous_ms, last_continuous_ms,
                 first_wall_ms, last_wall_ms, root_identity_fingerprint,
                 member_fingerprint, boot_session_fingerprint, enforcement_epoch
             FROM cooling_candidates_v5;
             DROP TABLE cooling_candidates_v5;
             ALTER TABLE cleanup_retry_blocks RENAME TO cleanup_retry_blocks_v5;
             CREATE TABLE cleanup_retry_blocks (
                 incident_id TEXT PRIMARY KEY,
                 tracking_key TEXT NOT NULL,
                 blocked_at_ms INTEGER NOT NULL CHECK (blocked_at_ms >= 0),
                 reason_id TEXT,
                 source_attempt_id INTEGER NOT NULL REFERENCES cleanup_attempts(id)
             );
             INSERT INTO cleanup_retry_blocks (
                 incident_id, tracking_key, blocked_at_ms, reason_id, source_attempt_id
             ) SELECT
                 incident_id, tracking_key, blocked_at_ms, reason_id, source_attempt_id
             FROM cleanup_retry_blocks_v5;
             DROP TABLE cleanup_retry_blocks_v5;
             PRAGMA user_version = 4;",
        )
        .expect("build schema-v4 fixture");
}

fn downgrade_current_database_to_v5(path: &PathBuf) {
    let connection = Connection::open(path).expect("open current database for v5 fixture");
    connection
        .execute_batch(
            "DROP TABLE storage_residue_latest;
             DROP TABLE cleanup_impacts;
             DROP TABLE impact_authority;
             DROP TABLE ordinary_mutation_receipts;
             DROP TABLE control_metadata;
             DROP TABLE mutation_authority;
             DROP TABLE public_event_tokens;
             DROP INDEX storage_recoveries_recent;
             ALTER TABLE storage_recoveries RENAME TO storage_recoveries_v6;
             CREATE TABLE storage_recoveries (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 recovery_id TEXT NOT NULL UNIQUE,
                 occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
                 reason_id TEXT NOT NULL CHECK (
                     reason_id IN ('integrity_check_failed', 'required_schema_invalid')
                 ),
                 quarantine_directory_name TEXT NOT NULL,
                 quarantined_sidecar_count INTEGER NOT NULL
                     CHECK (quarantined_sidecar_count >= 0)
             );
             INSERT INTO storage_recoveries (
                 id, recovery_id, occurred_at_ms, reason_id,
                 quarantine_directory_name, quarantined_sidecar_count
             ) SELECT
                 id, recovery_id, occurred_at_ms, reason_id,
                 quarantine_directory_name, quarantined_sidecar_count
             FROM storage_recoveries_v6;
             DROP TABLE storage_recoveries_v6;
             CREATE INDEX storage_recoveries_recent
                 ON storage_recoveries (occurred_at_ms DESC, id DESC);
             PRAGMA user_version = 5;",
        )
        .expect("build schema-v5 fixture");
}

#[test]
fn records_redacted_observation_and_cleanup_timeline() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-1");
    store
        .record_observation(1_000, &report)
        .expect("record observation");
    let attempt = store
        .begin_cleanup_attempt(1_500, &report, "epoch-a")
        .expect("begin cleanup");
    let prepared = store
        .prepare_cleanup_action(&attempt, 0, 1_600, &primary_term_intent())
        .expect("prepare action");
    store
        .complete_cleanup_action(&prepared, 1_700, SignalDisposition::Delivered)
        .expect("complete action");
    store
        .complete_cleanup_attempt(&attempt, 2_000, &cleared_receipt("inc-1"))
        .expect("complete cleanup");

    let history = store.history(10).expect("history");
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].kind, EventKind::Cleanup);
    assert_eq!(history[0].state, IncidentState::Cleared);

    let detail = store
        .explain("inc-1")
        .expect("explain query")
        .expect("incident exists");
    assert_eq!(detail.events.len(), 3);
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
fn repeated_semantically_identical_observations_extend_one_durable_span() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let mut report = confirmed_report("inc-span");

    store
        .record_observation_batch(1_000, &[report.clone()])
        .expect("record first observation");
    report.resident_memory_bytes = 8_192;
    store
        .record_observation_batch(2_000, &[report.clone()])
        .expect("extend observation span");
    report.resident_memory_bytes = 12_288;
    store
        .record_observation_batch(3_000, &[report.clone()])
        .expect("extend observation span again");

    let history = store.history(10).expect("read compact history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].first_occurred_at_unix_millis, 1_000);
    assert_eq!(history[0].occurred_at_unix_millis, 3_000);
    assert_eq!(history[0].observation_count, 3);
    let EventPayload::Observation {
        report: stored_report,
    } = &history[0].payload
    else {
        panic!("span must retain an observation payload");
    };
    assert_eq!(stored_report.resident_memory_bytes, 12_288);

    let mut changed = report.clone();
    changed.state = IncidentState::Protected;
    changed.gates.no_protection_rule = false;
    store
        .record_observation_batch(4_000, &[changed])
        .expect("record semantic state change");
    let history = store.history(10).expect("read changed history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].observation_count, 1);
    assert_eq!(history[1].observation_count, 3);
}

#[test]
fn observation_retention_cannot_evict_cleanup_impact_or_terminal_receipt() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-impact");
    store
        .record_observation(900, &report)
        .expect("record cleanup family authority");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin cleanup");
    let prepared = store
        .prepare_cleanup_action(&attempt, 0, 1_010, &primary_term_intent())
        .expect("prepare action");
    store
        .complete_cleanup_action(&prepared, 1_020, SignalDisposition::Delivered)
        .expect("complete action");
    let mut receipt = cleared_receipt("inc-impact");
    receipt.resources = CleanupResources {
        before: Some(ResourceSnapshot {
            process_count: 2,
            resident_memory_bytes: 8_192,
        }),
        after: Some(ResourceSnapshot {
            process_count: 0,
            resident_memory_bytes: 0,
        }),
        estimated_reclaimed_memory_bytes: Some(8_192),
    };
    store
        .complete_cleanup_attempt(&attempt, 1_100, &receipt)
        .expect("complete cleanup");

    for index in 0..25_u64 {
        store
            .record_observation(2_000 + index, &confirmed_report(&format!("noise-{index}")))
            .expect("record unrelated observation");
    }
    store
        .prune(
            3_000,
            RetentionPolicy {
                max_age_millis: 10_000,
                max_events: 1,
            },
        )
        .expect("prune observation presentation");

    let history = store.history(100).expect("read retained cleanup history");
    assert!(history.iter().any(|event| {
        event.incident_id == "inc-impact"
            && event.kind == EventKind::Cleanup
            && event.state == IncidentState::Cleared
    }));
    let recent = store
        .most_recent_reclaim()
        .expect("read recent reclaim")
        .expect("terminal impact remains authoritative");
    assert_eq!(recent.incident_id, "inc-impact");

    let impact = store.impact_summary(10).expect("read impact summary");
    assert_eq!(
        impact.historical_completeness,
        ImpactHistoryCompleteness::Complete
    );
    assert_eq!(impact.terminal_cleanup_count, 1);
    assert_eq!(impact.proved_reclaim_count, 1);
    assert_eq!(impact.reclaimed_process_count, Some(2));
    assert_eq!(impact.estimated_reclaimed_memory_bytes, Some(8_192));
    assert_eq!(impact.recent.len(), 1);
    assert_eq!(impact.recent[0].family, "agent-browser");
}

#[test]
fn aggregate_impact_survives_expired_cleanup_detail_without_claiming_full_history() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-expired-impact");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin cleanup");
    store
        .complete_cleanup_attempt(&attempt, 1_100, &cleared_receipt("inc-expired-impact"))
        .expect("complete cleanup");
    store
        .prune(
            15 * 24 * 60 * 60 * 1_000,
            RetentionPolicy {
                max_age_millis: 1,
                max_events: 0,
            },
        )
        .expect("expire presentation detail");

    let impact = store.impact_summary(10).expect("read durable aggregate");
    assert_eq!(impact.terminal_cleanup_count, 1);
    assert_eq!(impact.proved_reclaim_count, 1);
    assert!(impact.recent.is_empty());
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
fn storage_residue_observation_is_durable_redacted_and_cannot_encode_cleanup_authority() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let observation = StorageResidueObservation {
        kind: StorageResidueKind::ChromeCodeSignClone,
        status: StorageResidueStatus::Detected,
        observed_at_unix_millis: 4_200,
        candidate_count: 49,
        logical_bytes: 67_000,
        shape_complete: true,
        reference_check: StorageResidueReferenceCheck::Incomplete,
        automatic_cleanup_eligible: false,
        reason_ids: vec!["storage_residue.code_sign_clone_detected".to_owned()],
    };
    store
        .record_storage_residue_observation(&observation)
        .expect("record residue observation");
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    assert_eq!(
        reopened
            .latest_storage_residue_observation()
            .expect("read residue observation"),
        Some(observation.clone())
    );
    let mut forbidden = observation;
    forbidden.automatic_cleanup_eligible = true;
    assert!(matches!(
        reopened.record_storage_residue_observation(&forbidden),
        Err(StoreError::Invalid(_))
    ));
}

#[test]
fn cooling_grace_is_durable_and_resets_on_identity_or_observation_gap() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = cooling_report("inc-cooling");

    assert!(
        !store
            .track_cooling(
                &report,
                &cooling_clock(1_000, 10_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("first cooling observation")
    );
    drop(store);
    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    assert!(
        reopened
            .track_cooling(
                &report,
                &cooling_clock(91_000, 100_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("durable cooling observation")
    );

    let mut changed = report.clone();
    changed.member_fingerprint = "different-members".to_owned();
    assert!(
        !reopened
            .track_cooling(
                &changed,
                &cooling_clock(92_000, 101_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("changed membership resets grace")
    );
    assert!(
        !reopened
            .track_cooling(
                &changed,
                &cooling_clock(300_000, 309_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("long observation gap resets grace")
    );
}

#[test]
fn migrates_v2_history_and_pause_but_resets_legacy_wall_clock_cooling() {
    let database = TempDatabase::new();
    create_v2_database(&database.0);

    let store = HistoryStore::open(&database.0).expect("migrate v2 store");
    assert_eq!(store.history(10).expect("preserved history").len(), 1);
    assert_eq!(store.pause_until().expect("preserved pause"), Some(88_000));

    let connection = Connection::open(&database.0).expect("inspect migrated database");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read schema version");
    let journal_mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("read journal mode");
    let synchronous: i64 = connection
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .expect("read synchronous mode");
    assert_eq!(version, 7);
    assert_eq!(journal_mode, "wal");
    assert_eq!(synchronous, 2);
    drop(connection);

    let report = cooling_report("legacy-cooling");
    assert!(
        !store
            .track_cooling(
                &report,
                &cooling_clock(200_000, 500_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("legacy cooling must restart")
    );
}

#[test]
fn v3_to_v7_adds_lifecycle_public_identity_and_resets_signature_unknown_cooling() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("create current store");
    let report = cooling_report("inc-v3");
    assert!(
        !store
            .track_cooling(
                &report,
                &cooling_clock(1_000, 1_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("start continuous cooling")
    );
    store
        .record_observation(1_001, &confirmed_report("history-survives-v3"))
        .expect("record history before migration");
    drop(store);
    downgrade_current_database_to_v4(&database.0);
    let connection = Connection::open(&database.0).expect("downgrade fixture metadata");
    connection
        .execute_batch("DROP TABLE managed_lifecycle; PRAGMA user_version = 3;")
        .expect("simulate a schema-v3 candidate");
    drop(connection);

    let migrated = HistoryStore::open(&database.0).expect("migrate v3 to v7");
    assert!(
        !migrated
            .track_cooling(
                &report,
                &cooling_clock(91_000, 91_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("v3 cooling lacks signature identity and must restart")
    );
    assert_eq!(migrated.history(10).expect("preserved history").len(), 1);
    assert!(
        migrated
            .managed_lifecycle()
            .expect("lifecycle query")
            .is_none()
    );
}

#[test]
fn v4_to_v7_preserves_history_and_retry_block_but_resets_incompatible_cooling() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("create current store");
    let cooling = cooling_report("inc-v4-cooling");
    store
        .track_cooling(
            &cooling,
            &cooling_clock(1_000, 1_000, "boot-a", "epoch-a"),
            90_000,
            120_000,
        )
        .expect("start cooling");
    store
        .record_observation(1_010, &confirmed_report("history-v4"))
        .expect("record history");
    let blocked = confirmed_report("blocked-v4");
    let attempt = store
        .begin_cleanup_attempt(1_020, &blocked, "epoch-a")
        .expect("begin failure");
    store
        .complete_cleanup_attempt(
            &attempt,
            1_030,
            &failed_receipt(
                "blocked-v4",
                IncidentState::Failed,
                "cleanup.signal_rejected",
            ),
        )
        .expect("block retry");
    drop(store);
    downgrade_current_database_to_v4(&database.0);

    let migrated = HistoryStore::open(&database.0).expect("migrate v4 to v7");
    assert_eq!(HistoryStore::schema_version(), 7);
    let connection = Connection::open(&database.0).expect("inspect migrated artifact journal");
    let artifact_columns = connection
        .prepare("PRAGMA table_info(cleanup_artifact_actions)")
        .expect("prepare migrated artifact schema query")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query migrated artifact schema")
        .collect::<Result<std::collections::BTreeSet<_>, _>>()
        .expect("collect migrated artifact columns");
    assert_eq!(
        artifact_columns,
        std::collections::BTreeSet::from([
            "artifact_fingerprint".to_owned(),
            "attempt_id".to_owned(),
            "completed_at_ms".to_owned(),
            "disposition".to_owned(),
            "id".to_owned(),
            "kind".to_owned(),
            "prepared_at_ms".to_owned(),
            "sequence".to_owned(),
        ])
    );
    drop(connection);
    assert_eq!(migrated.history(10).expect("preserved history").len(), 3);
    assert!(
        migrated
            .cleanup_blocked("blocked-v4")
            .expect("block preserved")
    );
    assert!(
        !migrated
            .track_cooling(
                &cooling,
                &cooling_clock(91_000, 91_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("signature-unknown v4 cooling resets")
    );
}

#[test]
fn v5_to_v7_backfills_event_tokens_and_impact_authority() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("create current store");
    store
        .record_observation(1_010, &confirmed_report("history-v5"))
        .expect("record history before v5 fixture");
    let cleanup_report = confirmed_report("cleanup-v5");
    let attempt = store
        .begin_cleanup_attempt(1_020, &cleanup_report, "epoch-v5")
        .expect("begin cleanup before v5 fixture");
    store
        .complete_cleanup_attempt(&attempt, 1_030, &cleared_receipt("cleanup-v5"))
        .expect("complete cleanup before v5 fixture");
    drop(store);
    downgrade_current_database_to_v5(&database.0);

    let migrated = HistoryStore::open(&database.0).expect("migrate v5 to v7");
    let history = migrated.history(10).expect("preserved history");
    assert_eq!(history.len(), 3);
    assert!(history.iter().all(|event| event.event_token.len() == 32));
    let impact = migrated.impact_summary(10).expect("backfilled impact");
    assert_eq!(
        impact.historical_completeness,
        ImpactHistoryCompleteness::PartialBackfill
    );
    assert_eq!(impact.terminal_cleanup_count, 1);
    assert_eq!(impact.proved_reclaim_count, 1);
    assert_eq!(impact.recent.len(), 1);
    assert_eq!(impact.recent[0].incident_id, "cleanup-v5");
    assert_eq!(
        migrated
            .mutation_namespace_token()
            .expect("namespace")
            .len(),
        32
    );
    assert_eq!(migrated.cleanup_policy_revision().expect("revision"), 1);
    drop(migrated);

    let connection = Connection::open(&database.0).expect("inspect migrated schema");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read migrated version");
    assert_eq!(version, 7);
}

#[test]
fn v5_to_v7_failure_rolls_back_the_entire_migration_transaction() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("create current store");
    drop(store);
    downgrade_current_database_to_v5(&database.0);
    let connection = Connection::open(&database.0).expect("damage v5 fixture");
    connection
        .execute_batch("DROP TABLE events;")
        .expect("remove required v5 events table");
    drop(connection);

    let error = HistoryStore::open(&database.0).expect_err("migration must fail atomically");
    assert!(matches!(error, StoreError::Corrupt(_)));
    let preserved = Connection::open(&database.0).expect("inspect rolled-back v5 fixture");
    let version: i64 = preserved
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read preserved version");
    assert_eq!(version, 5);
    for table in [
        "public_event_tokens",
        "mutation_authority",
        "control_metadata",
        "ordinary_mutation_receipts",
    ] {
        let count: i64 = preserved
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query rolled-back table");
        assert_eq!(count, 0, "{table} must not survive failed migration");
    }
}

#[test]
fn managed_same_generation_restart_carries_only_enforce_intent_and_rearms_fresh() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");

    let boot = store
        .begin_managed_boot(7, "instance-a", 1_000)
        .expect("begin managed boot");
    assert_eq!(boot.activation_generation, 7);
    assert_eq!(boot.instance_id, "instance-a");
    assert!(!boot.ready);
    assert!(!boot.effective_enforce);
    assert_eq!(boot.startup_phase, ManagedStartupPhase::Recovering);

    assert!(
        store
            .arm_managed(7, "instance-a", "epoch-too-early", 1_010)
            .expect_err("arm must wait for recovery and a first scan")
            .to_string()
            .contains("not ready")
    );
    store
        .finish_managed_recovery(7, "instance-a", 1_020)
        .expect("finish recovery");
    store
        .complete_managed_first_scan(7, "instance-a", "unused-first-epoch", 1_030)
        .expect("mark first scan ready");
    let armed = store
        .arm_managed(7, "instance-a", "epoch-a", 1_040)
        .expect("arm exact boot");
    assert!(armed.requested_enforce);
    assert!(armed.effective_enforce);
    assert_eq!(armed.armed_generation, Some(7));
    assert_eq!(armed.enforcement_epoch.as_deref(), Some("epoch-a"));

    let idempotent = store
        .arm_managed(7, "instance-a", "must-not-replace-epoch", 1_050)
        .expect("repeat arm is idempotent");
    assert_eq!(idempotent.enforcement_epoch.as_deref(), Some("epoch-a"));

    drop(store);
    let reopened = HistoryStore::open(&database.0).expect("reopen after simulated crash");
    let reboot = reopened
        .begin_managed_boot(7, "instance-b", 2_000)
        .expect("same-generation restart");
    assert!(!reboot.ready);
    assert!(reboot.requested_enforce);
    assert!(!reboot.effective_enforce);
    assert_eq!(reboot.armed_generation, None);
    assert_eq!(reboot.enforcement_epoch, None);
    assert!(
        reopened
            .arm_managed(7, "instance-a", "stale", 2_010)
            .expect_err("stale instance cannot mutate replacement")
            .to_string()
            .contains("instance")
    );
    let recovering = reopened
        .finish_managed_recovery(7, "instance-b", 2_020)
        .expect("finish replacement recovery");
    assert!(recovering.requested_enforce);
    assert!(!recovering.effective_enforce);
    let rearmed = reopened
        .complete_managed_first_scan(7, "instance-b", "epoch-b", 2_030)
        .expect("first report-only scan commits fresh re-arm");
    assert!(rearmed.ready);
    assert!(rearmed.requested_enforce);
    assert!(rearmed.effective_enforce);
    assert_eq!(rearmed.armed_generation, Some(7));
    assert_eq!(rearmed.enforcement_epoch.as_deref(), Some("epoch-b"));
    assert_ne!(rearmed.enforcement_epoch, armed.enforcement_epoch);

    let replacement = reopened
        .begin_managed_boot(8, "instance-c", 3_000)
        .expect("new generation boot");
    assert_eq!(replacement.activation_generation, 8);
    assert!(!replacement.requested_enforce);
    assert!(!replacement.effective_enforce);
    assert_eq!(replacement.armed_generation, None);
}

#[test]
fn pre_ready_intent_carries_only_after_an_explicit_graceful_restart_transition() {
    let fatal_database = TempDatabase::new();
    let fatal = HistoryStore::open(&fatal_database.0).expect("open fatal fixture");
    fatal
        .begin_managed_boot(35, "fatal-ready", 1_000)
        .expect("begin ready seed");
    fatal
        .finish_managed_recovery(35, "fatal-ready", 1_010)
        .expect("finish ready seed recovery");
    fatal
        .complete_managed_first_scan(35, "fatal-ready", "unused-fatal-epoch", 1_020)
        .expect("complete ready seed scan");
    fatal
        .arm_managed(35, "fatal-ready", "fatal-seed-epoch", 1_030)
        .expect("arm ready seed");
    let fatal_pre_ready = fatal
        .begin_managed_boot(35, "fatal-pre-ready", 2_000)
        .expect("begin replacement that will fail during startup");
    assert!(fatal_pre_ready.requested_enforce);
    assert_eq!(
        fatal_pre_ready.startup_phase,
        ManagedStartupPhase::Recovering
    );
    let after_failed_start = fatal
        .begin_managed_boot(35, "after-fatal", 3_000)
        .expect("restart after unpersisted fatal fail-close");
    assert!(
        !after_failed_start.requested_enforce,
        "recovering state is not a durable authorization to carry intent"
    );

    let graceful_database = TempDatabase::new();
    let graceful = HistoryStore::open(&graceful_database.0).expect("open graceful fixture");
    graceful
        .begin_managed_boot(36, "graceful-ready", 1_000)
        .expect("begin graceful seed");
    graceful
        .finish_managed_recovery(36, "graceful-ready", 1_010)
        .expect("finish graceful seed recovery");
    graceful
        .complete_managed_first_scan(36, "graceful-ready", "unused-graceful-epoch", 1_020)
        .expect("complete graceful seed scan");
    graceful
        .arm_managed(36, "graceful-ready", "graceful-seed-epoch", 1_030)
        .expect("arm graceful seed");
    graceful
        .begin_managed_boot(36, "graceful-pre-ready", 2_000)
        .expect("begin graceful replacement");
    graceful
        .finish_managed_recovery(36, "graceful-pre-ready", 2_010)
        .expect("finish graceful replacement recovery");
    let pending = graceful
        .preserve_managed_pre_ready_restart_intent(36, "graceful-pre-ready", 2_020)
        .expect("persist explicit graceful restart transition");
    assert!(pending.requested_enforce);
    assert!(!pending.ready);
    assert_eq!(pending.startup_phase, ManagedStartupPhase::ReadyReportOnly);
    let after_graceful = graceful
        .begin_managed_boot(36, "after-graceful", 3_000)
        .expect("restart after explicit graceful transition");
    assert!(after_graceful.requested_enforce);
    assert_eq!(
        after_graceful.startup_phase,
        ManagedStartupPhase::Recovering
    );
}

#[test]
fn managed_disarm_and_drain_are_exact_durable_and_idempotent() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(11, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(11, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(11, "instance-a", "unused-first-epoch", 1_020)
        .expect("ready");
    store
        .arm_managed(11, "instance-a", "epoch-a", 1_030)
        .expect("arm");

    for now in [1_040, 1_050] {
        let disarmed = store
            .disarm_managed(11, "instance-a", now)
            .expect("idempotent disarm");
        assert!(!disarmed.effective_enforce);
        assert_eq!(disarmed.armed_generation, None);
        assert_eq!(disarmed.enforcement_epoch, None);
        assert!(!disarmed.draining);
    }

    store
        .arm_managed(11, "instance-a", "epoch-b", 1_060)
        .expect("rearm after disarm");
    for now in [1_070, 1_080] {
        let draining = store
            .begin_managed_drain(11, "instance-a", now)
            .expect("idempotent drain");
        assert!(draining.draining);
        assert!(!draining.ready);
        assert!(!draining.effective_enforce);
        assert_eq!(draining.startup_phase, ManagedStartupPhase::Draining);
    }
    assert!(
        store
            .arm_managed(11, "instance-a", "epoch-c", 1_090)
            .expect_err("draining daemon cannot arm")
            .to_string()
            .contains("draining")
    );

    drop(store);
    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    let persisted = reopened
        .managed_lifecycle()
        .expect("read lifecycle")
        .expect("lifecycle exists");
    assert!(persisted.draining);
    assert!(!persisted.effective_enforce);
}

#[test]
fn managed_disarm_drain_and_fail_clear_carried_intent_before_first_scan() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(19, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(19, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(19, "instance-a", "unused-first-epoch", 1_020)
        .expect("ready");
    store
        .arm_managed(19, "instance-a", "epoch-a", 1_030)
        .expect("arm");

    let carried_for_disarm = store
        .begin_managed_boot(19, "instance-b", 2_000)
        .expect("same generation restart");
    assert!(carried_for_disarm.requested_enforce);
    store
        .finish_managed_recovery(19, "instance-b", 2_010)
        .expect("finish recovery");
    let disarmed = store
        .disarm_managed(19, "instance-b", 2_020)
        .expect("disarm before first scan");
    assert!(!disarmed.requested_enforce);
    let ready_report_only = store
        .complete_managed_first_scan(19, "instance-b", "unused-second-epoch", 2_030)
        .expect("complete report-only first scan");
    assert!(ready_report_only.ready);
    assert!(!ready_report_only.effective_enforce);

    store
        .arm_managed(19, "instance-b", "epoch-b", 2_040)
        .expect("arm again");
    store
        .begin_managed_boot(19, "instance-c", 3_000)
        .expect("carry for drain");
    store
        .finish_managed_recovery(19, "instance-c", 3_010)
        .expect("finish recovery");
    let draining = store
        .begin_managed_drain(19, "instance-c", 3_020)
        .expect("drain before first scan");
    assert!(!draining.requested_enforce);
    assert!(draining.draining);

    store
        .begin_managed_boot(19, "instance-d", 4_000)
        .expect("drain-cleared restart");
    store
        .finish_managed_recovery(19, "instance-d", 4_010)
        .expect("finish recovery for fail carry");
    store
        .complete_managed_first_scan(19, "instance-d", "unused-fourth-epoch", 4_020)
        .expect("ready for fail carry");
    store
        .arm_managed(19, "instance-d", "epoch-d", 4_030)
        .expect("arm for fail carry");
    let carried_for_fail = store
        .begin_managed_boot(19, "instance-e", 5_000)
        .expect("carry intent for fail");
    assert!(carried_for_fail.requested_enforce);
    let failed = store
        .fail_managed(19, "instance-e", 5_010)
        .expect("fail clears any request");
    assert!(!failed.requested_enforce);
    assert_eq!(failed.startup_phase, ManagedStartupPhase::Failed);
}

#[test]
fn delivery_unknown_blocks_managed_arm_until_the_incident_is_explicitly_retried() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-unknown");
    let attempt = store
        .begin_cleanup_attempt(100, &report, "old-epoch")
        .expect("begin interrupted attempt");
    store
        .prepare_cleanup_action(&attempt, 0, 110, &primary_term_intent())
        .expect("prepare uncertain delivery");
    store
        .recover_incomplete_attempts(120)
        .expect("recover uncertain delivery");

    store
        .begin_managed_boot(12, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(12, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(12, "instance-a", "unused-first-epoch", 1_020)
        .expect("ready");
    assert!(
        store
            .arm_managed(12, "instance-a", "epoch-a", 1_030)
            .expect_err("delivery uncertainty must block global enforcement")
            .to_string()
            .contains("delivery-unknown")
    );
    assert!(
        store
            .authorize_retry("inc-unknown", 1_040)
            .expect("authorize retry")
    );
    assert!(
        store
            .arm_managed(12, "instance-a", "epoch-a", 1_050)
            .expect("arm after uncertainty acknowledgment")
            .effective_enforce
    );
}

#[test]
fn carried_intent_delivery_unknown_falls_back_durably_to_ready_report_only() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(23, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(23, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(23, "instance-a", "unused-first-epoch", 1_020)
        .expect("ready");
    store
        .arm_managed(23, "instance-a", "epoch-a", 1_030)
        .expect("arm");
    let report = confirmed_report("inc-carried-unknown");
    let attempt = store
        .begin_cleanup_attempt(1_040, &report, "epoch-a")
        .expect("begin interrupted attempt");
    store
        .prepare_cleanup_action(&attempt, 0, 1_050, &primary_term_intent())
        .expect("prepare uncertain delivery");
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen after crash");
    let reboot = reopened
        .begin_managed_boot(23, "instance-b", 2_000)
        .expect("same-generation restart");
    assert!(reboot.requested_enforce);
    reopened
        .recover_incomplete_attempts(2_010)
        .expect("recover delivery unknown");
    reopened
        .finish_managed_recovery(23, "instance-b", 2_020)
        .expect("finish recovery");
    let ready = reopened
        .complete_managed_first_scan(23, "instance-b", "must-not-arm", 2_030)
        .expect("blocker degrades to durable report-only");
    assert!(ready.ready);
    assert_eq!(ready.startup_phase, ManagedStartupPhase::ReadyReportOnly);
    assert!(!ready.requested_enforce);
    assert!(!ready.effective_enforce);
    assert_eq!(ready.armed_generation, None);
    assert_eq!(ready.enforcement_epoch, None);

    drop(reopened);
    let persisted = HistoryStore::open(&database.0)
        .expect("reopen durable fallback")
        .managed_lifecycle()
        .expect("read lifecycle")
        .expect("lifecycle exists");
    assert_eq!(persisted, ready);
}

#[test]
fn carried_intent_open_attempt_falls_back_durably_to_ready_report_only() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(26, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(26, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(26, "instance-a", "unused-first-epoch", 1_020)
        .expect("ready");
    store
        .arm_managed(26, "instance-a", "epoch-a", 1_030)
        .expect("arm");
    store
        .begin_managed_boot(26, "instance-b", 2_000)
        .expect("carry intent");
    store
        .finish_managed_recovery(26, "instance-b", 2_010)
        .expect("finish recovery");
    let report = confirmed_report("inc-open-during-first-scan");
    store
        .begin_cleanup_attempt(2_020, &report, "old-epoch")
        .expect("leave attempt open at completion boundary");

    let ready = store
        .complete_managed_first_scan(26, "instance-b", "must-not-arm", 2_030)
        .expect("open attempt degrades to durable report-only");
    assert!(ready.ready);
    assert_eq!(ready.startup_phase, ManagedStartupPhase::ReadyReportOnly);
    assert!(!ready.requested_enforce);
    assert!(!ready.effective_enforce);
    assert_eq!(ready.enforcement_epoch, None);
}

#[test]
fn interrupted_before_signal_does_not_block_carried_intent_rearm() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(24, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(24, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(24, "instance-a", "unused-first-epoch", 1_020)
        .expect("ready");
    store
        .arm_managed(24, "instance-a", "epoch-a", 1_030)
        .expect("arm");
    let report = confirmed_report("inc-before-signal");
    store
        .begin_cleanup_attempt(1_040, &report, "epoch-a")
        .expect("open attempt without PREPARED action");
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen after crash");
    reopened
        .begin_managed_boot(24, "instance-b", 2_000)
        .expect("same-generation restart");
    let recovered = reopened
        .recover_incomplete_attempts(2_010)
        .expect("recover before-signal attempt");
    assert_eq!(
        recovered[0].reason_id.as_deref(),
        Some("cleanup.interrupted_before_signal")
    );
    reopened
        .finish_managed_recovery(24, "instance-b", 2_020)
        .expect("finish recovery");
    let rearmed = reopened
        .complete_managed_first_scan(24, "instance-b", "epoch-b", 2_030)
        .expect("before-signal recovery permits fresh re-arm");
    assert!(rearmed.requested_enforce);
    assert!(rearmed.effective_enforce);
    assert_eq!(rearmed.enforcement_epoch.as_deref(), Some("epoch-b"));
}

#[test]
fn offline_report_only_recovery_requires_the_exact_generation_and_clears_intent() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(25, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(25, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(25, "instance-a", "unused-first-epoch", 1_020)
        .expect("ready");
    store
        .arm_managed(25, "instance-a", "epoch-a", 1_030)
        .expect("arm");
    drop(store);

    let offline = HistoryStore::open(&database.0).expect("open offline store");
    assert!(
        offline
            .clear_managed_enforce_request_offline(24, 2_000)
            .expect_err("wrong generation cannot mutate lifecycle")
            .to_string()
            .contains("generation")
    );
    let cleared = offline
        .clear_managed_enforce_request_offline(25, 2_010)
        .expect("clear exact generation offline");
    assert!(!cleared.requested_enforce);
    assert!(!cleared.effective_enforce);
    assert!(!cleared.ready);
    assert!(!cleared.draining);
    assert_eq!(cleared.armed_generation, None);
    assert_eq!(cleared.enforcement_epoch, None);
    assert_eq!(cleared.startup_phase, ManagedStartupPhase::Recovering);
}

#[test]
fn cooling_elapsed_uses_continuous_time_and_resets_on_clock_boundary_changes() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = cooling_report("inc-clock");

    assert!(
        !store
            .track_cooling(
                &report,
                &cooling_clock(10_000, 1_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("start cooling")
    );
    assert!(
        !store
            .track_cooling(
                &report,
                &cooling_clock(100_000, 80_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("wall time cannot mature cooling")
    );
    assert!(
        store
            .track_cooling(
                &report,
                &cooling_clock(110_000, 91_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("continuous time matures cooling")
    );

    for (wall, continuous, boot, epoch, label) in [
        (111_000, 92_000, "boot-b", "epoch-a", "boot change"),
        (112_000, 93_000, "boot-b", "epoch-b", "epoch change"),
        (
            113_000,
            92_000,
            "boot-b",
            "epoch-b",
            "continuous regression",
        ),
        (112_000, 93_000, "boot-b", "epoch-b", "wall regression"),
        (500_000, 94_000, "boot-b", "epoch-b", "wall anomaly"),
        (501_000, 500_000, "boot-b", "epoch-b", "continuity gap"),
    ] {
        assert!(
            !store
                .track_cooling(
                    &report,
                    &cooling_clock(wall, continuous, boot, epoch),
                    90_000,
                    120_000,
                )
                .unwrap_or_else(|error| panic!("{label}: {error}")),
            "{label} must reset cooling"
        );
    }
}

#[test]
fn cleanup_journal_is_reopenable_and_disposition_completion_is_idempotent() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-journal");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let prepared = store
        .prepare_cleanup_action(&attempt, 0, 1_100, &primary_term_intent())
        .expect("persist PREPARED action");
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    reopened
        .complete_cleanup_action(&prepared, 1_200, SignalDisposition::Delivered)
        .expect("complete disposition");
    reopened
        .complete_cleanup_action(&prepared, 1_300, SignalDisposition::Delivered)
        .expect("same disposition is idempotent");
    let conflict = reopened
        .complete_cleanup_action(&prepared, 1_400, SignalDisposition::Rejected)
        .expect_err("conflicting disposition must be rejected");
    assert!(conflict.to_string().contains("conflicting disposition"));

    let mut claimed_receipt = cleared_receipt("inc-journal");
    claimed_receipt.actions[0].disposition = SignalDisposition::Rejected;
    let projected = reopened
        .complete_cleanup_attempt(&attempt, 2_000, &claimed_receipt)
        .expect("complete attempt");
    assert_eq!(projected.actions.len(), 1);
    assert_eq!(
        projected.actions[0].disposition,
        SignalDisposition::Delivered
    );
    let repeated = reopened
        .complete_cleanup_attempt(&attempt, 2_100, &claimed_receipt)
        .expect("terminal completion is idempotent");
    assert_eq!(repeated, projected);
    let history = reopened.history(10).expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].attempt_id, Some(attempt.id));
    assert_eq!(history[0].state, IncidentState::Cleared);
    assert_eq!(history[1].attempt_id, Some(attempt.id));
    assert_eq!(history[1].state, IncidentState::Reclaiming);
}

#[test]
fn resource_receipt_survives_reopen_and_idempotent_completion() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-resources");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let mut receipt = cleared_receipt("inc-resources");
    receipt.resources = CleanupResources {
        before: Some(ResourceSnapshot {
            process_count: 3,
            resident_memory_bytes: 32_768,
        }),
        after: Some(ResourceSnapshot {
            process_count: 0,
            resident_memory_bytes: 0,
        }),
        estimated_reclaimed_memory_bytes: Some(32_768),
    };
    let canonical = store
        .complete_cleanup_attempt(&attempt, 2_000, &receipt)
        .expect("complete with resources");
    assert_eq!(canonical.resources, receipt.resources);
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    let mut conflicting_retry = cleared_receipt("inc-resources");
    conflicting_retry.resources = CleanupResources::default();
    let repeated = reopened
        .complete_cleanup_attempt(&attempt, 3_000, &conflicting_retry)
        .expect("idempotent completion reads canonical resources");
    assert_eq!(repeated.resources, receipt.resources);
}

#[test]
fn store_owned_journal_adapter_round_trips_core_action_contract() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-adapter");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let intent = primary_term_intent();
    let mut journal = store.journal_for(&attempt);
    let action_id = journal
        .prepare_action(&intent, 1_100)
        .expect("journal prepares action");
    assert!(action_id.starts_with("sqlite-action:"));
    journal
        .complete_action(&action_id, SignalDisposition::Delivered, 1_200)
        .expect("journal completes action");
    drop(journal);

    let projected = store
        .complete_cleanup_attempt(&attempt, 2_000, &cleared_receipt("inc-adapter"))
        .expect("complete attempt");
    assert_eq!(projected.actions.len(), 1);
    assert_eq!(
        projected.actions[0].disposition,
        SignalDisposition::Delivered
    );
}

#[test]
fn artifact_journal_replays_canonical_rows_and_ignores_caller_spoof() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-artifact-canonical");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let intent = artifact_intent();
    let mut journal = store.journal_for(&attempt);
    let action_id = journal
        .prepare_artifact_action(&intent, 1_100)
        .expect("journal commits artifact PREPARED");
    assert!(action_id.starts_with("sqlite-artifact-action:"));
    journal
        .complete_artifact_action(&action_id, ArtifactDisposition::Removed, 1_200)
        .expect("journal commits artifact disposition");
    drop(journal);

    let private_path = "/private/tmp/playwright_chromiumdev_profile-secret/DevToolsActivePort";
    let mut caller_receipt = cleared_receipt(&report.incident_id);
    caller_receipt.artifact_actions = vec![ArtifactAction {
        kind: RuntimeArtifactKind::DevToolsActivePort,
        artifact_fingerprint: private_path.to_owned(),
        disposition: ArtifactDisposition::Rejected,
    }];
    caller_receipt.resources = CleanupResources {
        before: Some(ResourceSnapshot {
            process_count: 1,
            resident_memory_bytes: 8_192,
        }),
        after: Some(ResourceSnapshot {
            process_count: 0,
            resident_memory_bytes: 0,
        }),
        estimated_reclaimed_memory_bytes: Some(8_192),
    };
    let canonical = store
        .complete_cleanup_attempt(&attempt, 1_300, &caller_receipt)
        .expect("terminalize from durable journals");
    assert_eq!(
        canonical.artifact_actions,
        vec![ArtifactAction {
            kind: intent.kind,
            artifact_fingerprint: intent.artifact_fingerprint.clone(),
            disposition: ArtifactDisposition::Removed,
        }]
    );
    assert_eq!(canonical.resources, caller_receipt.resources);
    let serialized = serde_json::to_string(&store.history(10).expect("history"))
        .expect("serialize canonical history");
    assert!(!serialized.contains(private_path));
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    let mut spoofed_retry = cleared_receipt(&report.incident_id);
    spoofed_retry.artifact_actions = Vec::new();
    spoofed_retry.resources = CleanupResources::default();
    let replayed = reopened
        .complete_cleanup_attempt(&attempt, 9_999, &spoofed_retry)
        .expect("same-state completion replays canonical rows");
    assert_eq!(replayed.artifact_actions, canonical.artifact_actions);
    assert_eq!(replayed.resources, canonical.resources);
}

#[test]
fn artifact_disposition_completion_is_exact_idempotent_and_rejects_conflicts() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-artifact-idempotent");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let prepared = store
        .prepare_cleanup_artifact_action(&attempt, 0, 1_100, &artifact_intent())
        .expect("persist artifact PREPARED");

    for completed_at in [1_200, 1_300] {
        store
            .complete_cleanup_artifact_action(
                &prepared,
                completed_at,
                ArtifactDisposition::AlreadyAbsent,
            )
            .expect("same disposition completion is idempotent");
    }
    let conflict = store
        .complete_cleanup_artifact_action(&prepared, 1_400, ArtifactDisposition::Removed)
        .expect_err("conflicting terminal disposition must fail");
    assert!(conflict.to_string().contains("conflicting disposition"));

    let canonical = store
        .complete_cleanup_attempt(&attempt, 1_500, &cleared_receipt(&report.incident_id))
        .expect("complete attempt from canonical artifact row");
    assert_eq!(canonical.artifact_actions.len(), 1);
    assert_eq!(
        canonical.artifact_actions[0].disposition,
        ArtifactDisposition::AlreadyAbsent
    );
}

#[test]
fn artifact_journal_rejects_non_fingerprint_input_without_persisting_raw_path() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-artifact-private-input");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let private_path = "/private/tmp/secret-profile/DevToolsActivePort";
    let invalid = ArtifactActionIntent {
        kind: RuntimeArtifactKind::DevToolsActivePort,
        artifact_fingerprint: private_path.to_owned(),
    };
    let error = store
        .prepare_cleanup_artifact_action(&attempt, 0, 1_100, &invalid)
        .expect_err("raw path cannot masquerade as a fingerprint");
    assert!(matches!(error, StoreError::Invalid(_)));

    let connection = Connection::open(&database.0).expect("inspect artifact journal");
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM cleanup_artifact_actions", [], |row| {
            row.get(0)
        })
        .expect("count journal rows");
    assert_eq!(count, 0);
    drop(connection);
    let bytes = fs::read(&database.0).expect("read test database");
    assert!(
        !bytes
            .windows(private_path.len())
            .any(|window| window == private_path.as_bytes())
    );
}

#[test]
fn startup_recovery_marks_prepared_artifact_delivery_unknown_and_blocks_arming() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-artifact-interrupted");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    store
        .prepare_cleanup_artifact_action(&attempt, 0, 1_100, &artifact_intent())
        .expect("persist artifact PREPARED before unlink");
    assert!(
        store
            .complete_cleanup_attempt(&attempt, 1_200, &cleared_receipt(&report.incident_id))
            .expect_err("PREPARED artifact cannot become fabricated certainty")
            .to_string()
            .contains("PREPARED")
    );
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen after simulated crash");
    reopened
        .begin_managed_boot(7, "artifact-recovery", 2_000)
        .expect("begin managed recovery");
    let recovered = reopened
        .recover_incomplete_attempts(2_100)
        .expect("recover PREPARED artifact without resending");
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered[0].reason_id.as_deref(),
        Some("cleanup.interrupted_delivery_unknown")
    );
    assert!(recovered[0].actions.is_empty());
    assert_eq!(recovered[0].artifact_actions.len(), 1);
    assert_eq!(
        recovered[0].artifact_actions[0].disposition,
        ArtifactDisposition::DeliveryUnknown
    );
    assert!(
        reopened
            .cleanup_blocked(&report.incident_id)
            .expect("delivery-unknown blocks retry")
    );
    assert!(
        reopened
            .recover_incomplete_attempts(2_200)
            .expect("recovery never resends terminal unknown artifact")
            .is_empty()
    );
    reopened
        .finish_managed_recovery(7, "artifact-recovery", 2_300)
        .expect("finish managed recovery");
    reopened
        .complete_managed_first_scan(7, "artifact-recovery", "unused-first-epoch", 2_400)
        .expect("mark report-only ready");
    assert!(
        reopened
            .arm_managed(7, "artifact-recovery", "epoch-b", 2_500)
            .expect_err("unresolved artifact delivery unknown blocks arming")
            .to_string()
            .contains("delivery-unknown")
    );
}

#[test]
fn terminalization_rejects_prepared_action_and_leaves_recovery_authoritative() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-open-action");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    store
        .prepare_cleanup_action(&attempt, 0, 1_100, &primary_term_intent())
        .expect("persist PREPARED action");

    assert!(
        store
            .complete_cleanup_attempt(&attempt, 1_200, &cleared_receipt("inc-open-action"))
            .expect_err("PREPARED cannot be fabricated into terminal certainty")
            .to_string()
            .contains("still has 1 PREPARED")
    );
    let recovered = store
        .recover_incomplete_attempts(2_000)
        .expect("startup-style recovery remains authoritative");
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered[0].actions[0].disposition,
        SignalDisposition::DeliveryUnknown
    );
}

#[test]
fn startup_recovery_marks_delivery_unknown_fails_and_blocks_without_resuming() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-interrupted");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    store
        .prepare_cleanup_action(&attempt, 0, 1_100, &primary_term_intent())
        .expect("persist PREPARED action");
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    let recovered = reopened
        .recover_incomplete_attempts(2_000)
        .expect("recover interrupted attempt");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].state, IncidentState::Failed);
    assert_eq!(recovered[0].actions.len(), 1);
    assert_eq!(
        recovered[0].actions[0].disposition,
        SignalDisposition::DeliveryUnknown
    );
    assert!(reopened.cleanup_blocked("inc-interrupted").expect("block"));
    assert!(
        reopened
            .begin_cleanup_attempt(2_100, &report, "epoch-a")
            .expect_err("blocked incident cannot retry automatically")
            .to_string()
            .contains("retry is blocked")
    );
    assert!(
        reopened
            .recover_incomplete_attempts(2_200)
            .expect("recovery is idempotent")
            .is_empty()
    );
}

#[test]
fn terminal_signal_delivery_unknown_blocks_carried_rearm_until_explicit_retry() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(31, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(31, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(31, "instance-a", "unused-first", 1_020)
        .expect("ready report-only");
    store
        .arm_managed(31, "instance-a", "epoch-a", 1_030)
        .expect("arm");

    let report = confirmed_report("inc-terminal-signal-unknown");
    let attempt = store
        .begin_cleanup_attempt(1_100, &report, "epoch-a")
        .expect("begin attempt");
    let prepared = store
        .prepare_cleanup_action(&attempt, 0, 1_110, &primary_term_intent())
        .expect("prepare signal");
    store
        .complete_cleanup_action(&prepared, 1_120, SignalDisposition::DeliveryUnknown)
        .expect("complete unknown signal disposition");
    store
        .complete_cleanup_attempt(
            &attempt,
            1_130,
            &failed_receipt(
                &report.incident_id,
                IncidentState::Failed,
                "cleanup.delivery_unknown",
            ),
        )
        .expect("terminalize unknown delivery");
    assert!(
        store
            .automatic_enforcement_blocked()
            .expect("global delivery blocker")
    );

    let reboot = store
        .begin_managed_boot(31, "instance-b", 2_000)
        .expect("same-generation restart");
    assert!(reboot.requested_enforce);
    store
        .finish_managed_recovery(31, "instance-b", 2_010)
        .expect("finish replacement recovery");
    let ready = store
        .complete_managed_first_scan(31, "instance-b", "must-not-arm", 2_020)
        .expect("unknown delivery falls back report-only");
    assert!(!ready.requested_enforce);
    assert!(!ready.effective_enforce);
    assert_eq!(ready.startup_phase, ManagedStartupPhase::ReadyReportOnly);

    assert!(
        store
            .authorize_retry(&report.incident_id, 2_030)
            .expect("explicitly acknowledge retry")
    );
    assert!(
        !store
            .automatic_enforcement_blocked()
            .expect("blocker clears only after explicit retry")
    );
}

#[test]
fn terminal_artifact_delivery_unknown_blocks_carried_rearm() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .begin_managed_boot(32, "instance-a", 1_000)
        .expect("boot");
    store
        .finish_managed_recovery(32, "instance-a", 1_010)
        .expect("recovery");
    store
        .complete_managed_first_scan(32, "instance-a", "unused-first", 1_020)
        .expect("ready report-only");
    store
        .arm_managed(32, "instance-a", "epoch-a", 1_030)
        .expect("arm");

    let report = confirmed_report("inc-terminal-artifact-unknown");
    let attempt = store
        .begin_cleanup_attempt(1_100, &report, "epoch-a")
        .expect("begin attempt");
    let prepared = store
        .prepare_cleanup_artifact_action(&attempt, 0, 1_110, &artifact_intent())
        .expect("prepare artifact");
    store
        .complete_cleanup_artifact_action(&prepared, 1_120, ArtifactDisposition::DeliveryUnknown)
        .expect("complete unknown artifact disposition");
    store
        .complete_cleanup_attempt(
            &attempt,
            1_130,
            &failed_receipt(
                &report.incident_id,
                IncidentState::Failed,
                "cleanup.artifact_delivery_unknown",
            ),
        )
        .expect("terminalize unknown artifact delivery");

    store
        .begin_managed_boot(32, "instance-b", 2_000)
        .expect("same-generation restart");
    store
        .finish_managed_recovery(32, "instance-b", 2_010)
        .expect("finish replacement recovery");
    let ready = store
        .complete_managed_first_scan(32, "instance-b", "must-not-arm", 2_020)
        .expect("artifact uncertainty falls back report-only");
    assert!(!ready.requested_enforce);
    assert!(!ready.effective_enforce);
    assert_eq!(ready.startup_phase, ManagedStartupPhase::ReadyReportOnly);
}

#[test]
fn crash_after_durable_delivery_is_not_recovered_as_before_signal() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-delivered-before-crash");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let prepared = store
        .prepare_cleanup_action(&attempt, 0, 1_010, &primary_term_intent())
        .expect("prepare signal");
    store
        .complete_cleanup_action(&prepared, 1_020, SignalDisposition::Delivered)
        .expect("persist known delivery");
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen after crash");
    let recovered = reopened
        .recover_incomplete_attempts(2_000)
        .expect("recover post-delivery interruption");
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered[0].reason_id.as_deref(),
        Some("cleanup.interrupted_after_delivery")
    );
    assert_eq!(
        recovered[0].actions[0].disposition,
        SignalDisposition::Delivered
    );
    assert!(
        reopened
            .automatic_enforcement_blocked()
            .expect("known delivery keeps global enforcement closed")
    );
}

#[test]
fn only_one_attempt_can_be_open_and_failed_or_revived_attempts_create_blocks() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let first = confirmed_report("inc-first");
    let second = confirmed_report("inc-second");
    let attempt = store
        .begin_cleanup_attempt(1_000, &first, "epoch-a")
        .expect("begin first attempt");
    assert!(
        store
            .begin_cleanup_attempt(1_001, &second, "epoch-a")
            .expect_err("second concurrent attempt must fail")
            .to_string()
            .contains("already open")
    );

    let failed = CleanupReceipt {
        incident_id: first.incident_id.clone(),
        state: IncidentState::Failed,
        reason_id: Some("cleanup.signal_rejected".to_owned()),
        actions: Vec::new(),
        artifact_actions: Vec::new(),
        survivor_pids: vec![4242],
        revival_checks_completed: 0,
        resources: CleanupResources::default(),
    };
    store
        .complete_cleanup_attempt(&attempt, 2_000, &failed)
        .expect("terminalize failure");
    assert!(store.cleanup_blocked("inc-first").expect("failed block"));

    let second_attempt = store
        .begin_cleanup_attempt(2_100, &second, "epoch-a")
        .expect("begin after previous terminal");
    let revived = CleanupReceipt {
        incident_id: second.incident_id.clone(),
        state: IncidentState::Revived,
        reason_id: Some("cleanup.supervisor_revival".to_owned()),
        actions: Vec::new(),
        artifact_actions: Vec::new(),
        survivor_pids: vec![4242],
        revival_checks_completed: 1,
        resources: CleanupResources::default(),
    };
    store
        .complete_cleanup_attempt(&second_attempt, 3_000, &revived)
        .expect("terminalize revival");
    assert!(store.cleanup_blocked("inc-second").expect("revival block"));
}

#[test]
fn authorize_retry_clears_only_the_named_block_and_restarts_its_cooling() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let first = confirmed_report("inc-first");
    let mut second = confirmed_report("inc-second");
    second.tracking_key = "tracking-second".to_owned();
    let failed = |id: &str| CleanupReceipt {
        incident_id: id.to_owned(),
        state: IncidentState::Failed,
        reason_id: Some("cleanup.interrupted".to_owned()),
        actions: Vec::new(),
        artifact_actions: Vec::new(),
        survivor_pids: Vec::new(),
        revival_checks_completed: 0,
        resources: CleanupResources::default(),
    };

    let first_attempt = store
        .begin_cleanup_attempt(1_000, &first, "epoch-a")
        .expect("first attempt");
    store
        .complete_cleanup_attempt(&first_attempt, 1_100, &failed("inc-first"))
        .expect("fail first");
    let second_attempt = store
        .begin_cleanup_attempt(1_200, &second, "epoch-a")
        .expect("second attempt");
    store
        .complete_cleanup_attempt(&second_attempt, 1_300, &failed("inc-second"))
        .expect("fail second");

    let cooling = cooling_report("inc-first");
    assert!(
        !store
            .track_cooling(
                &cooling,
                &cooling_clock(2_000, 2_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("start cooling")
    );
    assert!(
        store
            .track_cooling(
                &cooling,
                &cooling_clock(92_000, 92_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("mature cooling")
    );

    assert!(
        store
            .authorize_retry("inc-first", 100_000)
            .expect("authorize named retry")
    );
    assert!(!store.cleanup_blocked("inc-first").expect("first cleared"));
    assert!(
        store
            .cleanup_blocked("inc-second")
            .expect("second retained")
    );
    assert!(
        !store
            .track_cooling(
                &cooling,
                &cooling_clock(101_000, 101_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("cooling restarts after authorization")
    );
}

#[test]
fn retention_never_discards_active_retry_blocks() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-blocked");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin attempt");
    let failed = CleanupReceipt {
        incident_id: report.incident_id.clone(),
        state: IncidentState::Failed,
        reason_id: Some("cleanup.interrupted".to_owned()),
        actions: Vec::new(),
        artifact_actions: Vec::new(),
        survivor_pids: Vec::new(),
        revival_checks_completed: 0,
        resources: CleanupResources::default(),
    };
    store
        .complete_cleanup_attempt(&attempt, 1_100, &failed)
        .expect("fail attempt");
    store
        .prune(
            1_000_000,
            RetentionPolicy {
                max_age_millis: 1,
                max_events: 0,
            },
        )
        .expect("prune presentation history");
    let retained = store.history(10).expect("cleanup history retained");
    assert!(
        retained
            .iter()
            .all(|event| event.kind == EventKind::Cleanup)
    );
    assert!(
        store
            .cleanup_blocked("inc-blocked")
            .expect("block survives retention")
    );
}

#[test]
fn startup_quarantines_corrupt_database_and_sidecars_then_records_recovery() {
    let directory = TempDirectory::new("corrupt-recovery");
    let database = directory.database();
    fs::write(&database, b"not a sqlite database").expect("write corrupt database");
    fs::write(
        PathBuf::from(format!("{}-wal", database.display())),
        b"wal evidence",
    )
    .expect("write WAL evidence");
    fs::write(
        PathBuf::from(format!("{}-shm", database.display())),
        b"shm evidence",
    )
    .expect("write SHM evidence");
    make_private(&database);
    make_private(&PathBuf::from(format!("{}-wal", database.display())));
    make_private(&PathBuf::from(format!("{}-shm", database.display())));

    let store = HistoryStore::open(&database).expect("recover corrupt store");
    let recovery = store
        .startup_recovery()
        .expect("startup must expose recovery occurrence");
    assert_eq!(recovery.reason, StorageRecoveryReason::IntegrityCheckFailed);
    assert!(recovery.quarantine_directory.is_dir());
    assert!(
        recovery
            .quarantine_directory
            .join("history.sqlite3")
            .is_file()
    );
    assert!(
        recovery
            .quarantine_directory
            .join("history.sqlite3-wal")
            .is_file()
    );
    assert!(
        recovery
            .quarantine_directory
            .join("history.sqlite3-shm")
            .is_file()
    );
    assert_eq!(recovery.quarantined_sidecar_count, 2);
    assert_eq!(
        store.latest_storage_recovery().expect("persisted recovery"),
        Some(recovery.clone())
    );

    let lifecycle = store
        .begin_managed_boot(77, "recovered-instance", 10_000)
        .expect("new managed lifecycle");
    assert!(!lifecycle.requested_enforce);
    assert!(!lifecycle.effective_enforce);
    assert_eq!(lifecycle.armed_generation, None);
    assert_eq!(lifecycle.enforcement_epoch, None);
}

#[test]
fn current_schema_missing_required_table_is_quarantined_not_reused() {
    let directory = TempDirectory::new("shape-recovery");
    let database = directory.database();
    let store = HistoryStore::open(&database).expect("create store");
    drop(store);
    let connection = Connection::open(&database).expect("open fixture database");
    connection
        .execute_batch("DROP TABLE events;")
        .expect("damage current schema shape");
    drop(connection);

    let recovered = HistoryStore::open(&database).expect("recover malformed current schema");
    let occurrence = recovered
        .startup_recovery()
        .expect("shape corruption recovery");
    assert_eq!(
        occurrence.reason,
        StorageRecoveryReason::RequiredSchemaInvalid
    );
    assert!(
        occurrence
            .quarantine_directory
            .join("history.sqlite3")
            .is_file()
    );
    assert!(recovered.history(1).expect("fresh history").is_empty());
}

#[test]
fn current_schema_missing_artifact_journal_is_quarantined_and_recreated() {
    let directory = TempDirectory::new("artifact-shape-recovery");
    let database = directory.database();
    let store = HistoryStore::open(&database).expect("create store");
    drop(store);
    let connection = Connection::open(&database).expect("open fixture database");
    connection
        .execute_batch("DROP TABLE cleanup_artifact_actions;")
        .expect("damage required artifact journal shape");
    drop(connection);

    let recovered = HistoryStore::open(&database).expect("recover malformed artifact schema");
    assert_eq!(
        recovered
            .startup_recovery()
            .expect("artifact shape recovery")
            .reason,
        StorageRecoveryReason::RequiredSchemaInvalid
    );
    let connection = Connection::open(&database).expect("inspect fresh schema");
    let columns = connection
        .prepare("PRAGMA table_info(cleanup_artifact_actions)")
        .expect("prepare artifact schema query")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query artifact schema")
        .collect::<Result<std::collections::BTreeSet<_>, _>>()
        .expect("collect artifact columns");
    assert_eq!(
        columns,
        std::collections::BTreeSet::from([
            "artifact_fingerprint".to_owned(),
            "attempt_id".to_owned(),
            "completed_at_ms".to_owned(),
            "disposition".to_owned(),
            "id".to_owned(),
            "kind".to_owned(),
            "prepared_at_ms".to_owned(),
            "sequence".to_owned(),
        ])
    );
}

#[test]
fn unsupported_newer_schema_is_preserved_and_never_auto_recovered() {
    let directory = TempDirectory::new("future-schema");
    let database = directory.database();
    let connection = Connection::open(&database).expect("create future fixture");
    connection
        .execute_batch("CREATE TABLE future_marker (value TEXT); PRAGMA user_version = 999;")
        .expect("create future schema");
    drop(connection);
    make_private(&database);

    let error = HistoryStore::open(&database).expect_err("future schema must fail closed");
    assert!(matches!(error, StoreError::UnsupportedSchema(_)));
    let preserved = Connection::open(&database).expect("future database remains in place");
    let marker_count: i64 = preserved
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'future_marker'",
            [],
            |row| row.get(0),
        )
        .expect("query preserved marker");
    assert_eq!(marker_count, 1);
    assert!(
        fs::read_dir(&directory.0)
            .expect("read fixture directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .contains(".quarantine-"))
    );
}

#[test]
fn post_migration_shape_failure_is_returned_without_fresh_start() {
    let directory = TempDirectory::new("broken-migration");
    let database = directory.database();
    let store = HistoryStore::open(&database).expect("create current store");
    drop(store);
    downgrade_current_database_to_v4(&database);
    let connection = Connection::open(&database).expect("damage v4 fixture");
    connection
        .execute_batch("DROP TABLE events;")
        .expect("remove table not touched by v5 migration");
    drop(connection);

    let error = HistoryStore::open(&database).expect_err("post-migration shape must fail");
    assert!(
        matches!(error, StoreError::Corrupt(_)),
        "unexpected migration failure: {error:?}"
    );
    let preserved = Connection::open(&database).expect("inspect failed candidate");
    let version: i64 = preserved
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read attempted migration version");
    assert_eq!(version, 4);
    let events: i64 = preserved
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'events'",
            [],
            |row| row.get(0),
        )
        .expect("query missing table");
    assert_eq!(events, 0);
    assert!(
        fs::read_dir(&directory.0)
            .expect("read fixture directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .contains(".quarantine-"))
    );
}

#[test]
fn database_symlink_fails_closed_without_touching_target() {
    let directory = TempDirectory::new("database-symlink");
    let database = directory.database();
    let target = directory.0.join("private-target");
    fs::write(&target, b"private target state").expect("write symlink target");
    make_private(&target);
    symlink(&target, &database).expect("create database symlink");

    let error = HistoryStore::open(&database).expect_err("database symlink must fail closed");
    assert!(matches!(error, StoreError::UnsafePath(_)));
    assert_eq!(
        fs::read(&target).expect("target survives"),
        b"private target state"
    );
    assert!(
        fs::symlink_metadata(&database)
            .expect("link survives")
            .file_type()
            .is_symlink()
    );
}

#[test]
fn wal_symlink_fails_closed_without_quarantine_or_target_mutation() {
    let directory = TempDirectory::new("wal-symlink");
    let database = directory.database();
    let wal = PathBuf::from(format!("{}-wal", database.display()));
    let target = directory.0.join("private-wal-target");
    fs::write(&database, b"corrupt but owned database").expect("write database");
    fs::write(&target, b"private WAL target state").expect("write WAL target");
    make_private(&database);
    make_private(&target);
    symlink(&target, &wal).expect("create WAL symlink");

    let error = HistoryStore::open(&database).expect_err("WAL symlink must fail closed");
    assert!(matches!(error, StoreError::UnsafePath(_)));
    assert_eq!(
        fs::read(&target).expect("target survives"),
        b"private WAL target state"
    );
    assert!(database.is_file());
    assert!(
        fs::symlink_metadata(&wal)
            .expect("WAL link survives")
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::read_dir(&directory.0)
            .expect("read fixture directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .contains(".quarantine-"))
    );
}

#[test]
fn most_recent_reclaim_uses_a_dedicated_query_not_history_presentation_limit() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-cleared");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin cleanup");
    store
        .complete_cleanup_attempt(&attempt, 1_100, &cleared_receipt("inc-cleared"))
        .expect("complete cleanup");
    let newer = confirmed_report("inc-new");
    let payload = serde_json::to_string(&EventPayload::Observation {
        report: ObservationRecord::from(&newer),
    })
    .expect("serialize newer observation");
    let mut connection = Connection::open(&database.0).expect("open bulk fixture connection");
    let transaction = connection.transaction().expect("begin bulk fixture");
    for index in 0..1_005_u64 {
        transaction
            .execute(
                "INSERT INTO events (
                     incident_id, first_occurred_at_ms, occurred_at_ms,
                     observation_count, kind, state, payload_json, attempt_id
                 ) VALUES ('inc-new', ?1, ?1, 1, 'observation', 'CONFIRMED', ?2, NULL)",
                params![
                    i64::try_from(2_000 + index).expect("fixture timestamp"),
                    payload
                ],
            )
            .expect("insert newer observation");
        let event_id = transaction.last_insert_rowid();
        transaction
            .execute(
                "INSERT INTO public_event_tokens (event_id, event_token)
                 VALUES (?1, lower(hex(randomblob(16))))",
                params![event_id],
            )
            .expect("insert public event token");
    }
    transaction.commit().expect("commit bulk fixture");
    drop(connection);
    assert!(
        store
            .history(1_000)
            .expect("bounded presentation history")
            .iter()
            .all(|event| event.incident_id != "inc-cleared")
    );
    let reclaim = store
        .most_recent_reclaim()
        .expect("dedicated reclaim query")
        .expect("cleared receipt remains addressable");
    assert_eq!(reclaim.incident_id, "inc-cleared");
    assert_eq!(reclaim.state, IncidentState::Cleared);
}

#[test]
fn most_recent_reclaim_includes_proved_process_clearance_with_artifact_residue() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-residue");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin cleanup");
    store
        .complete_cleanup_attempt(
            &attempt,
            1_100,
            &cleared_with_residue_receipt("inc-residue"),
        )
        .expect("complete cleanup with residue");

    let reclaim = store
        .most_recent_reclaim()
        .expect("query recent reclaim")
        .expect("process clearance remains visible");
    assert_eq!(reclaim.incident_id, "inc-residue");
    assert_eq!(reclaim.state, IncidentState::Failed);
    assert_eq!(
        reclaim.outcome.overall,
        unlinger_core::OverallOutcome::ClearedWithResidue
    );
}

#[test]
fn cooling_resets_when_signature_pack_or_version_changes() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = cooling_report("inc-signature");
    assert!(
        !store
            .track_cooling(
                &report,
                &cooling_clock(1_000, 1_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("start cooling")
    );
    assert!(
        store
            .track_cooling(
                &report,
                &cooling_clock(91_000, 91_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("mature matching signature")
    );

    let mut new_version = report.clone();
    new_version.signature_version = "0.2.0".to_owned();
    assert!(
        !store
            .track_cooling(
                &new_version,
                &cooling_clock(92_000, 92_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("version drift resets cooling")
    );

    let mut new_pack = new_version.clone();
    new_pack.signature_pack = "playwright".to_owned();
    assert!(
        !store
            .track_cooling(
                &new_pack,
                &cooling_clock(182_000, 182_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("pack drift also resets cooling")
    );
}

#[test]
fn exact_incident_protection_is_observation_backed_durable_idempotent_and_named() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    assert_eq!(
        store
            .protect_incident("inc-missing", 900)
            .expect("missing protection lookup"),
        None
    );

    let first = confirmed_report("inc-protected");
    let mut second = confirmed_report("inc-other");
    second.root.identity_fingerprint = "identity-other".to_owned();
    second.member_fingerprint = "members-other".to_owned();
    store
        .record_observation(1_000, &first)
        .expect("record first observation");
    store
        .record_observation(1_010, &second)
        .expect("record second observation");

    let first_protection = store
        .protect_incident("inc-protected", 1_100)
        .expect("protect observed incident")
        .expect("observed protection");
    assert_eq!(first_protection.incident_id, "inc-protected");
    assert_eq!(first_protection.protected_at_unix_millis, 1_100);
    assert!(store.is_incident_protected(&first).expect("exact match"));

    let repeated = store
        .protect_incident("inc-protected", 1_200)
        .expect("repeat exact protection")
        .expect("repeated protection");
    assert_eq!(repeated, first_protection);

    let mut changed_root = first.clone();
    changed_root.root.identity_fingerprint = "replacement-root".to_owned();
    assert!(
        !store
            .is_incident_protected(&changed_root)
            .expect("changed root does not match")
    );
    let mut changed_members = first.clone();
    changed_members.member_fingerprint = "replacement-members".to_owned();
    assert!(
        !store
            .is_incident_protected(&changed_members)
            .expect("changed membership does not match")
    );

    store
        .protect_incident("inc-other", 1_300)
        .expect("protect second incident")
        .expect("second protection");
    let projection = store.protection_projection(1).expect("bounded projection");
    assert_eq!(projection.protected_incident_count, 2);
    assert_eq!(projection.protected_incidents.len(), 1);
    let serialized = serde_json::to_string(&projection).expect("serialize projection");
    assert!(!serialized.contains(&first.root.identity_fingerprint));
    assert!(!serialized.contains(&first.member_fingerprint));
    assert!(!serialized.contains("session-redacted"));

    assert!(
        store
            .unprotect_incident("inc-protected")
            .expect("unprotect named incident")
    );
    assert!(
        !store
            .unprotect_incident("inc-protected")
            .expect("repeated unprotect is idempotent")
    );
    assert!(
        store
            .is_incident_protected(&second)
            .expect("other protection remains isolated")
    );
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    assert!(
        reopened
            .is_incident_protected(&second)
            .expect("protection survives reopen")
    );
    reopened
        .prune(
            10_000,
            RetentionPolicy {
                max_age_millis: 1,
                max_events: 0,
            },
        )
        .expect("prune source observations");
    assert!(reopened.history(1).expect("pruned history").is_empty());
    assert_eq!(
        reopened
            .protect_incident("inc-other", 11_000)
            .expect("retry existing durable protection without source event")
            .expect("durable protection remains idempotent")
            .protected_at_unix_millis,
        1_300
    );
}

#[test]
fn protection_retires_only_after_complete_exact_root_absence_window() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-protected-retirement");
    store
        .record_observation(1_000, &report)
        .expect("record observation");
    store
        .protect_incident(&report.incident_id, 1_100)
        .expect("protect incident")
        .expect("protection exists");

    let live_roots = std::collections::BTreeSet::from([report.root.identity_fingerprint.clone()]);
    let observed = store
        .reconcile_incident_protections(&live_roots, true, 10_000, 1_000)
        .expect("live root remains protected despite classification absence");
    assert_eq!(observed.observed_count, 1);
    assert_eq!(observed.retired_count, 0);

    let no_live_roots = std::collections::BTreeSet::new();
    for now in [20_000, 30_000] {
        let unreadable = store
            .reconcile_incident_protections(&no_live_roots, false, now, 1_000)
            .expect("incomplete coverage cannot prove absence");
        assert_eq!(unreadable.unproven_count, 1);
        assert_eq!(unreadable.retired_count, 0);
    }
    let unreadable_projection = store
        .protection_projection(1)
        .expect("projection after incomplete coverage");
    assert_eq!(unreadable_projection.protected_incident_count, 1);
    assert_eq!(
        unreadable_projection.protected_incidents[0].exact_absence_since_unix_millis,
        None
    );

    store
        .reconcile_incident_protections(&no_live_roots, true, 40_000, 1_000)
        .expect("start complete exact-absence window");
    let early = store
        .reconcile_incident_protections(&no_live_roots, true, 40_999, 1_000)
        .expect("retention has not elapsed");
    assert_eq!(early.retired_count, 0);
    assert!(
        store
            .is_incident_protected(&report)
            .expect("not retired early")
    );

    let reappeared = store
        .reconcile_incident_protections(&live_roots, true, 41_000, 1_000)
        .expect("exact root reappearance resets absence");
    assert_eq!(reappeared.observed_count, 1);
    store
        .reconcile_incident_protections(&no_live_roots, true, 50_000, 1_000)
        .expect("restart exact-absence window");
    let retired = store
        .reconcile_incident_protections(&no_live_roots, true, 51_000, 1_000)
        .expect("retire after explicit retention");
    assert_eq!(retired.retired_count, 1);
    assert!(
        !store
            .is_incident_protected(&report)
            .expect("retired exact override")
    );
}

#[test]
fn retry_blocks_project_bounded_redacted_attention_and_retire_only_after_exact_absence() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let first = confirmed_report("inc-failed");
    let mut second = confirmed_report("inc-revived");
    second.tracking_key = "tracking-second".to_owned();
    second.root.identity_fingerprint = "identity-second".to_owned();
    second.member_fingerprint = "members-second".to_owned();

    let first_attempt = store
        .begin_cleanup_attempt(1_000, &first, "epoch-a")
        .expect("first attempt");
    store
        .complete_cleanup_attempt(
            &first_attempt,
            1_100,
            &failed_receipt(
                "inc-failed",
                IncidentState::Failed,
                &"private/raw/error/".repeat(100),
            ),
        )
        .expect("fail first");
    let second_attempt = store
        .begin_cleanup_attempt(1_200, &second, "epoch-a")
        .expect("second attempt");
    store
        .complete_cleanup_attempt(
            &second_attempt,
            1_300,
            &failed_receipt(
                "inc-revived",
                IncidentState::Revived,
                "cleanup.supervisor_revival",
            ),
        )
        .expect("revive second");

    let attention = store.attention_projection(1).expect("attention projection");
    assert_eq!(attention.blocked_cleanup_count, 2);
    assert_eq!(attention.blocked_cleanups.len(), 1);
    assert_eq!(attention.blocked_cleanups[0].state, IncidentState::Revived);
    assert_eq!(
        attention.blocked_cleanups[0].reason_id,
        "cleanup.supervisor_revival"
    );
    let all_attention = store.attention_projection(10).expect("all attention");
    let failed = all_attention
        .blocked_cleanups
        .iter()
        .find(|summary| summary.incident_id == "inc-failed")
        .expect("failed projection");
    assert_eq!(failed.reason_id, "cleanup.unclassified_failure");
    assert!(
        serde_json::to_string(&all_attention)
            .expect("serialize")
            .len()
            < 2_048
    );

    let exact = ObservedIncidentIdentity::from(&first);
    let second_exact = ObservedIncidentIdentity::from(&second);
    let no_live_roots = std::collections::BTreeSet::new();
    let kept = store
        .reconcile_retry_blocks(
            &[exact.clone(), second_exact.clone()],
            &no_live_roots,
            true,
            10_000,
            1_000,
        )
        .expect("observe exact live block");
    assert_eq!(kept.retired_count, 0);
    assert!(store.cleanup_blocked("inc-failed").expect("still blocked"));

    let live_first_root =
        std::collections::BTreeSet::from([first.root.identity_fingerprint.clone()]);
    for now in [20_000, 30_000] {
        let live = store
            .reconcile_retry_blocks(
                std::slice::from_ref(&second_exact),
                &live_first_root,
                true,
                now,
                1_000,
            )
            .expect("live root remains blocked even when no longer classified");
        assert_eq!(live.retired_count, 0);
    }
    for now in [40_000, 60_000] {
        let unreadable = store
            .reconcile_retry_blocks(
                std::slice::from_ref(&second_exact),
                &no_live_roots,
                false,
                now,
                1_000,
            )
            .expect("incomplete coverage cannot prove absence");
        assert_eq!(unreadable.retired_count, 0);
        assert_eq!(unreadable.unproven_count, 1);
    }
    assert!(
        store
            .cleanup_blocked("inc-failed")
            .expect("coverage gap keeps block")
    );

    store
        .reconcile_retry_blocks(
            std::slice::from_ref(&second_exact),
            &no_live_roots,
            true,
            70_000,
            1_000,
        )
        .expect("begin exact-absence window");
    store
        .reconcile_retry_blocks(
            std::slice::from_ref(&second_exact),
            &no_live_roots,
            true,
            70_999,
            1_000,
        )
        .expect("window not yet elapsed");
    assert!(
        store
            .cleanup_blocked("inc-failed")
            .expect("not retired early")
    );

    store
        .reconcile_retry_blocks(
            &[exact, second_exact.clone()],
            &no_live_roots,
            true,
            71_000,
            1_000,
        )
        .expect("reappearance resets absence");
    store
        .reconcile_retry_blocks(
            std::slice::from_ref(&second_exact),
            &no_live_roots,
            true,
            80_000,
            1_000,
        )
        .expect("restart absence window");
    let retired = store
        .reconcile_retry_blocks(
            std::slice::from_ref(&second_exact),
            &no_live_roots,
            true,
            81_000,
            1_000,
        )
        .expect("retire after explicit exact absence");
    assert_eq!(retired.retired_count, 1);
    assert!(!store.cleanup_blocked("inc-failed").expect("retired block"));
    assert!(
        store
            .cleanup_blocked("inc-revived")
            .expect("named isolation")
    );
}

#[test]
fn ordinary_pause_receipt_is_atomic_durable_idempotent_and_conflict_checked() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let context = MutationContext {
        namespace_token: store.mutation_namespace_token().expect("namespace"),
        mutation_id: "11111111-1111-4111-8111-111111111111".to_owned(),
    };
    let mutation = OrdinaryMutation::Pause {
        duration_millis: 500,
    };

    let first = store
        .commit_ordinary_mutation(&context, &mutation, 1_000)
        .expect("commit pause and receipt");
    assert!(first.newly_committed);
    assert!(first.state_changed);
    assert_eq!(first.receipt.kind, MutationKind::Pause);
    assert!(matches!(
        first.receipt.outcome,
        MutationOutcome::Applied {
            result: MutationResult::Paused {
                until_unix_millis: 1_500
            }
        }
    ));
    assert_eq!(first.receipt.policy_revision_after, 2);
    assert_eq!(
        first.receipt.retain_until_unix_millis,
        1_000 + unlinger_daemon::MUTATION_RECONCILIATION_WINDOW_MILLIS
    );
    assert_eq!(
        store.pause_until().expect("read committed pause"),
        Some(1_500)
    );

    let duplicate = store
        .commit_ordinary_mutation(&context, &mutation, 9_000)
        .expect("return original receipt");
    assert!(!duplicate.newly_committed);
    assert!(!duplicate.state_changed);
    assert_eq!(duplicate.receipt, first.receipt);
    assert_eq!(
        store.pause_until().expect("pause is not extended"),
        Some(1_500)
    );

    let conflict = store
        .commit_ordinary_mutation(
            &context,
            &OrdinaryMutation::Pause {
                duration_millis: 700,
            },
            10_000,
        )
        .expect_err("same ID with different arguments must conflict");
    assert!(matches!(conflict, StoreError::Conflict(_)));
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    assert_eq!(
        reopened
            .mutation_lookup(&context)
            .expect("read durable receipt"),
        MutationLookup::Committed(first.receipt)
    );
    assert_eq!(
        reopened.pause_until().expect("read durable state"),
        Some(1_500)
    );
}

#[test]
fn duplicate_retry_receipt_does_not_clear_a_new_cooling_candidate() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-retry-receipt");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-a")
        .expect("begin failed cleanup");
    store
        .complete_cleanup_attempt(
            &attempt,
            1_100,
            &failed_receipt(
                "inc-retry-receipt",
                IncidentState::Failed,
                "cleanup.signal_rejected",
            ),
        )
        .expect("create retry block");
    let context = MutationContext {
        namespace_token: store.mutation_namespace_token().expect("namespace"),
        mutation_id: "22222222-2222-4222-8222-222222222222".to_owned(),
    };
    let mutation = OrdinaryMutation::RetryFailedCleanup {
        incident_id: "inc-retry-receipt".to_owned(),
    };
    assert!(
        store
            .commit_ordinary_mutation(&context, &mutation, 2_000)
            .expect("authorize retry")
            .state_changed
    );

    let mut cooling = report;
    cooling.state = IncidentState::Cooling;
    cooling.gates.confirmed_abandonment = false;
    assert!(
        !store
            .track_cooling(
                &cooling,
                &cooling_clock(3_000, 3_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("start fresh cooling after authorization")
    );
    assert!(
        !store
            .commit_ordinary_mutation(&context, &mutation, 4_000)
            .expect("duplicate returns receipt")
            .state_changed
    );
    assert!(
        store
            .track_cooling(
                &cooling,
                &cooling_clock(93_000, 93_000, "boot-a", "epoch-a"),
                90_000,
                120_000,
            )
            .expect("duplicate must not clear cooling again")
    );
}

#[test]
fn protect_and_unprotect_receipt_replays_cannot_mutate_later_state() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let report = confirmed_report("inc-protection-receipt");
    store
        .record_observation(100, &report)
        .expect("record exact observation");
    let namespace_token = store.mutation_namespace_token().expect("namespace");
    let protect_context = MutationContext {
        namespace_token: namespace_token.clone(),
        mutation_id: "33333333-3333-4333-8333-333333333333".to_owned(),
    };
    let protect = OrdinaryMutation::ProtectIncident {
        incident_id: report.incident_id.clone(),
    };
    let first_protect = store
        .commit_ordinary_mutation(&protect_context, &protect, 200)
        .expect("protect exact incident");
    let duplicate_protect = store
        .commit_ordinary_mutation(&protect_context, &protect, 300)
        .expect("replay protection receipt");
    assert!(!duplicate_protect.newly_committed);
    assert_eq!(duplicate_protect.receipt, first_protect.receipt);

    let unprotect_context = MutationContext {
        namespace_token: namespace_token.clone(),
        mutation_id: "44444444-4444-4444-8444-444444444444".to_owned(),
    };
    let unprotect = OrdinaryMutation::UnprotectIncident {
        incident_id: report.incident_id.clone(),
    };
    assert!(
        store
            .commit_ordinary_mutation(&unprotect_context, &unprotect, 400)
            .expect("remove exact protection")
            .state_changed
    );
    store
        .commit_ordinary_mutation(
            &MutationContext {
                namespace_token,
                mutation_id: "55555555-5555-4555-8555-555555555555".to_owned(),
            },
            &protect,
            500,
        )
        .expect("create a later protection");
    assert!(
        !store
            .commit_ordinary_mutation(&unprotect_context, &unprotect, 600)
            .expect("old unprotect receipt replays")
            .state_changed
    );
    assert_eq!(
        store
            .protection_for_incident(&report.incident_id)
            .expect("query later protection")
            .expect("later protection survives")
            .protected_at_unix_millis,
        500
    );
}

#[test]
fn receipt_prune_rotates_namespace_atomically_and_never_fabricates_not_found() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let original_namespace = store.mutation_namespace_token().expect("namespace");
    let context = MutationContext {
        namespace_token: original_namespace.clone(),
        mutation_id: "66666666-6666-4666-8666-666666666666".to_owned(),
    };
    store
        .commit_ordinary_mutation(
            &context,
            &OrdinaryMutation::Pause {
                duration_millis: 1_000,
            },
            100_000,
        )
        .expect("commit receipt");
    store
        .prune(
            100_001,
            RetentionPolicy {
                max_age_millis: 1,
                max_events: 0,
            },
        )
        .expect("prune inside reconciliation window");
    assert!(matches!(
        store.mutation_lookup(&context).expect("read young receipt"),
        MutationLookup::Committed(_)
    ));
    assert_eq!(
        store.mutation_namespace_token().expect("same namespace"),
        original_namespace
    );

    store
        .prune(
            100_000 + unlinger_daemon::MUTATION_RECONCILIATION_WINDOW_MILLIS,
            RetentionPolicy {
                max_age_millis: 1,
                max_events: 0,
            },
        )
        .expect("prune beyond reconciliation window");
    assert_ne!(
        store.mutation_namespace_token().expect("rotated namespace"),
        original_namespace
    );
    assert_eq!(
        store
            .mutation_lookup(&context)
            .expect("old authority lookup"),
        MutationLookup::AuthorityLost
    );
}

#[test]
fn old_namespace_missing_receipt_request_is_rejected_without_state_change() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let original_namespace = store.mutation_namespace_token().expect("namespace");
    let expiring = MutationContext {
        namespace_token: original_namespace.clone(),
        mutation_id: "77777777-7777-4777-8777-777777777777".to_owned(),
    };
    store
        .commit_ordinary_mutation(
            &expiring,
            &OrdinaryMutation::Pause {
                duration_millis: 10,
            },
            1_000,
        )
        .expect("commit expiring receipt");
    store
        .prune(
            1_000 + unlinger_daemon::MUTATION_RECONCILIATION_WINDOW_MILLIS,
            RetentionPolicy::default(),
        )
        .expect("expire receipt and rotate namespace");
    let old_context = MutationContext {
        namespace_token: original_namespace,
        mutation_id: "88888888-8888-4888-8888-888888888888".to_owned(),
    };
    let error = store
        .commit_ordinary_mutation(&old_context, &OrdinaryMutation::Resume, 2_000)
        .expect_err("old namespace request cannot apply");
    assert!(matches!(error, StoreError::AuthorityLost(_)));
}

#[test]
fn public_event_tokens_are_unique_and_stable_across_reopen() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    store
        .record_observation(1_000, &confirmed_report("inc-token-a"))
        .expect("record first event");
    store
        .record_observation(1_001, &confirmed_report("inc-token-b"))
        .expect("record second event");
    let first = store.history(10).expect("read event tokens");
    assert_eq!(first.len(), 2);
    assert_ne!(first[0].event_token, first[1].event_token);
    assert!(first.iter().all(|event| event.event_token.len() == 32));
    let tokens = first
        .iter()
        .map(|event| event.event_token.clone())
        .collect::<Vec<_>>();
    drop(store);

    let reopened = HistoryStore::open(&database.0).expect("reopen store");
    assert_eq!(
        reopened
            .history(10)
            .expect("read stable tokens")
            .into_iter()
            .map(|event| event.event_token)
            .collect::<Vec<_>>(),
        tokens
    );
}

#[test]
fn observation_batch_rolls_back_every_event_when_one_insert_fails() {
    let database = TempDatabase::new();
    let store = HistoryStore::open(&database.0).expect("open store");
    let connection = Connection::open(&database.0).expect("open trigger fixture");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_second_observation
             BEFORE INSERT ON events
             WHEN NEW.incident_id = 'inc-batch-b'
             BEGIN
                 SELECT RAISE(ABORT, 'injected batch failure');
             END;",
        )
        .expect("install failure trigger");
    drop(connection);

    let error = store
        .record_observation_batch(
            1_000,
            &[
                confirmed_report("inc-batch-a"),
                confirmed_report("inc-batch-b"),
            ],
        )
        .expect_err("second insert aborts the batch");
    assert!(error.to_string().contains("injected batch failure"));
    assert!(
        store
            .history(10)
            .expect("read rolled-back history")
            .is_empty(),
        "the first observation must not survive a failed batch"
    );
}
