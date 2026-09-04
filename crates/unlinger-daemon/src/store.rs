use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use unlinger_core::{
    ArtifactAction, ArtifactActionIntent, ArtifactDisposition, CleanupAction, CleanupActionIntent,
    CleanupActionJournal, CleanupOutcome, CleanupReceipt, CleanupResources, CleanupSignal,
    CleanupStage, EvidenceItem, GateLedger, IncidentReport, IncidentState, ProcessOutcome,
    ProcessRoleCount, RootSummary, RuntimeArtifactKind, RuntimeFailure, SignalDisposition,
    StorageResidueKind, StorageResidueObservation,
};
use unlinger_protocol::{
    MutationContext, MutationKind, MutationOutcome, MutationReceipt, MutationResult,
    ProtectionSummary as PublicProtectionSummary,
};

use crate::public_action_policy::{
    PolicyDecision, RuntimePolicyFacts, StorePolicyFacts, evaluate_action,
};

const SCHEMA_VERSION: i64 = 7;
const CLEANUP_DETAIL_RETENTION_MILLIS: u64 = 14 * 24 * 60 * 60 * 1_000;
const MAX_ATTENTION_SUMMARIES: usize = 50;
const MAX_MUTATION_RECEIPTS: usize = 10_000;
pub const MUTATION_RECONCILIATION_WINDOW_MILLIS: u64 = 14 * 24 * 60 * 60 * 1_000;
const MAX_REDACTED_IDENTIFIER_CHARS: usize = 192;
const MAX_REASON_ID_CHARS: usize = 128;
const MAX_RESOURCE_RECEIPT_BYTES: usize = 4 * 1024;
const MAX_STORAGE_RESIDUE_OBSERVATION_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Observation,
    Cleanup,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Cleanup => "cleanup",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "observation" => Ok(Self::Observation),
            "cleanup" => Ok(Self::Cleanup),
            other => Err(StoreError::Corrupt(format!(
                "unknown history event kind {other:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservationRecord {
    pub incident_id: String,
    pub signature_pack: String,
    pub signature_version: String,
    pub state: IncidentState,
    pub root: RootSummary,
    pub member_count: usize,
    pub resident_memory_bytes: u64,
    pub member_fingerprint: String,
    pub roles: Vec<ProcessRoleCount>,
    pub evidence: Vec<EvidenceItem>,
    pub gates: GateLedger,
}

impl From<&IncidentReport> for ObservationRecord {
    fn from(report: &IncidentReport) -> Self {
        Self {
            incident_id: report.incident_id.clone(),
            signature_pack: report.signature_pack.clone(),
            signature_version: report.signature_version.clone(),
            state: report.state,
            root: report.root.clone(),
            member_count: report.member_count,
            resident_memory_bytes: report.resident_memory_bytes,
            member_fingerprint: report.member_fingerprint.clone(),
            roles: report.roles.clone(),
            evidence: report.evidence.clone(),
            gates: report.gates.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum EventPayload {
    Observation { report: ObservationRecord },
    Cleanup { receipt: CleanupReceipt },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryEvent {
    pub event_id: i64,
    #[serde(default, skip_serializing)]
    pub event_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<i64>,
    pub incident_id: String,
    pub first_occurred_at_unix_millis: u64,
    pub occurred_at_unix_millis: u64,
    pub observation_count: usize,
    pub kind: EventKind,
    pub state: IncidentState,
    pub payload: EventPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoolingClock {
    pub wall_unix_millis: u64,
    pub continuous_millis: u64,
    pub boot_session_fingerprint: String,
    pub enforcement_epoch: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedStartupPhase {
    Recovering,
    FirstScanReportOnly,
    ReadyReportOnly,
    ReadyEnforce,
    Draining,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedLifecycle {
    pub activation_generation: u64,
    pub instance_id: String,
    pub requested_enforce: bool,
    pub effective_enforce: bool,
    pub armed_generation: Option<u64>,
    pub enforcement_epoch: Option<String>,
    pub ready: bool,
    pub draining: bool,
    pub startup_phase: ManagedStartupPhase,
    pub updated_at_unix_millis: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupAttemptHandle {
    pub id: i64,
    pub incident_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedActionHandle {
    pub id: i64,
    pub attempt_id: i64,
    pub sequence: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedArtifactActionHandle {
    pub id: i64,
    pub attempt_id: i64,
    pub sequence: usize,
}

pub struct CleanupAttemptJournal<'a> {
    store: &'a HistoryStore,
    attempt: CleanupAttemptHandle,
    next_sequence: usize,
    prepared: BTreeMap<String, PreparedActionHandle>,
    next_artifact_sequence: usize,
    prepared_artifacts: BTreeMap<String, PreparedArtifactActionHandle>,
}

impl CleanupActionJournal for CleanupAttemptJournal<'_> {
    fn prepare_action(
        &mut self,
        intent: &CleanupActionIntent,
        prepared_at_unix_millis: u64,
    ) -> Result<String, RuntimeFailure> {
        let next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            RuntimeFailure::new("cleanup journal action sequence overflowed usize")
        })?;
        let prepared = self
            .store
            .prepare_cleanup_action(
                &self.attempt,
                self.next_sequence,
                prepared_at_unix_millis,
                intent,
            )
            .map_err(|error| RuntimeFailure::new(error.to_string()))?;
        self.next_sequence = next_sequence;
        let action_id = format!("sqlite-action:{}", prepared.id);
        self.prepared.insert(action_id.clone(), prepared);
        Ok(action_id)
    }

    fn complete_action(
        &mut self,
        action_id: &str,
        disposition: SignalDisposition,
        completed_at_unix_millis: u64,
    ) -> Result<(), RuntimeFailure> {
        let prepared = self.prepared.get(action_id).ok_or_else(|| {
            RuntimeFailure::new(format!(
                "cleanup journal action {action_id:?} is not prepared"
            ))
        })?;
        self.store
            .complete_cleanup_action(prepared, completed_at_unix_millis, disposition)
            .map_err(|error| RuntimeFailure::new(error.to_string()))
    }

    fn prepare_artifact_action(
        &mut self,
        intent: &ArtifactActionIntent,
        prepared_at_unix_millis: u64,
    ) -> Result<String, RuntimeFailure> {
        let next_sequence = self.next_artifact_sequence.checked_add(1).ok_or_else(|| {
            RuntimeFailure::new("cleanup journal artifact sequence overflowed usize")
        })?;
        let prepared = self
            .store
            .prepare_cleanup_artifact_action(
                &self.attempt,
                self.next_artifact_sequence,
                prepared_at_unix_millis,
                intent,
            )
            .map_err(|error| RuntimeFailure::new(error.to_string()))?;
        self.next_artifact_sequence = next_sequence;
        let action_id = format!("sqlite-artifact-action:{}", prepared.id);
        self.prepared_artifacts.insert(action_id.clone(), prepared);
        Ok(action_id)
    }

    fn complete_artifact_action(
        &mut self,
        action_id: &str,
        disposition: ArtifactDisposition,
        completed_at_unix_millis: u64,
    ) -> Result<(), RuntimeFailure> {
        let prepared = self.prepared_artifacts.get(action_id).ok_or_else(|| {
            RuntimeFailure::new(format!(
                "cleanup journal artifact action {action_id:?} is not prepared"
            ))
        })?;
        self.store
            .complete_cleanup_artifact_action(prepared, completed_at_unix_millis, disposition)
            .map_err(|error| RuntimeFailure::new(error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IncidentDetail {
    pub incident_id: String,
    pub events: Vec<HistoryEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetentionPolicy {
    pub max_age_millis: u64,
    pub max_events: usize,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age_millis: 14 * 24 * 60 * 60 * 1_000,
            max_events: 10_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneResult {
    pub removed_events: usize,
    pub remaining_events: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageRecoveryReason {
    IntegrityCheckFailed,
    RequiredSchemaInvalid,
}

impl StorageRecoveryReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::IntegrityCheckFailed => "integrity_check_failed",
            Self::RequiredSchemaInvalid => "required_schema_invalid",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "integrity_check_failed" => Ok(Self::IntegrityCheckFailed),
            "required_schema_invalid" => Ok(Self::RequiredSchemaInvalid),
            other => Err(StoreError::Corrupt(format!(
                "unknown storage recovery reason {other:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageRecoveryOccurrence {
    pub recovery_id: String,
    pub public_token: String,
    pub occurred_at_unix_millis: u64,
    pub reason: StorageRecoveryReason,
    pub quarantine_directory: PathBuf,
    pub quarantined_sidecar_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MostRecentReclaim {
    pub event_token: String,
    pub incident_id: String,
    pub occurred_at_unix_millis: u64,
    pub state: IncidentState,
    pub outcome: CleanupOutcome,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactHistoryCompleteness {
    Complete,
    PartialBackfill,
}

impl ImpactHistoryCompleteness {
    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "complete" => Ok(Self::Complete),
            "partial_backfill" => Ok(Self::PartialBackfill),
            other => Err(StoreError::Corrupt(format!(
                "unknown impact history completeness {other:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupImpact {
    pub event_token: String,
    pub incident_id: String,
    pub family: String,
    pub family_version: String,
    pub occurred_at_unix_millis: u64,
    pub state: IncidentState,
    pub outcome: CleanupOutcome,
    pub process_count: Option<usize>,
    pub estimated_reclaimed_memory_bytes: Option<u64>,
    pub revival_checks_completed: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupImpactSummary {
    pub tracking_started_at_unix_millis: u64,
    pub historical_completeness: ImpactHistoryCompleteness,
    pub terminal_cleanup_count: usize,
    pub proved_reclaim_count: usize,
    pub reclaimed_process_count: Option<usize>,
    pub estimated_reclaimed_memory_bytes: Option<u64>,
    pub recent: Vec<CleanupImpact>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedIncidentIdentity {
    pub incident_id: String,
    pub tracking_key: String,
    pub root_identity_fingerprint: String,
    pub member_fingerprint: String,
}

impl From<&IncidentReport> for ObservedIncidentIdentity {
    fn from(report: &IncidentReport) -> Self {
        Self {
            incident_id: report.incident_id.clone(),
            tracking_key: report.tracking_key.clone(),
            root_identity_fingerprint: report.root.identity_fingerprint.clone(),
            member_fingerprint: report.member_fingerprint.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockedCleanupSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_token: Option<String>,
    pub incident_id: String,
    pub state: IncidentState,
    pub blocked_at_unix_millis: u64,
    pub reason_id: String,
    pub outcome: CleanupOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exact_observed_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_absence_since_unix_millis: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoreAttentionProjection {
    pub blocked_cleanup_count: usize,
    pub blocked_cleanups: Vec<BlockedCleanupSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtectedIncidentSummary {
    pub incident_id: String,
    pub protected_at_unix_millis: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exact_observed_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_absence_since_unix_millis: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrdinaryMutation {
    Pause { duration_millis: u64 },
    Resume,
    RetryFailedCleanup { incident_id: String },
    ProtectIncident { incident_id: String },
    UnprotectIncident { incident_id: String },
}

impl OrdinaryMutation {
    #[must_use]
    pub const fn kind(&self) -> MutationKind {
        match self {
            Self::Pause { .. } => MutationKind::Pause,
            Self::Resume => MutationKind::Resume,
            Self::RetryFailedCleanup { .. } => MutationKind::RetryFailedCleanup,
            Self::ProtectIncident { .. } => MutationKind::ProtectIncident,
            Self::UnprotectIncident { .. } => MutationKind::UnprotectIncident,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationCommit {
    pub receipt: MutationReceipt,
    pub newly_committed: bool,
    pub state_changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MutationLookup {
    Committed(MutationReceipt),
    NotFound,
    AuthorityLost,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtectionProjection {
    pub protected_incident_count: usize,
    pub protected_incidents: Vec<ProtectedIncidentSummary>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProtectionReconciliation {
    pub observed_count: usize,
    pub unproven_count: usize,
    pub absent_count: usize,
    pub retired_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetryBlockReconciliation {
    pub observed_count: usize,
    pub absent_count: usize,
    pub unproven_count: usize,
    pub retired_count: usize,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Corrupt(String),
    Range(String),
    Invalid(String),
    Conflict(String),
    AuthorityLost(String),
    NotFound(String),
    Capacity(String),
    UnsupportedSchema(String),
    UnsafePath(String),
}

impl Display for StoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "history I/O failed: {error}"),
            Self::Sqlite(error) => write!(formatter, "history SQLite failed: {error}"),
            Self::Json(error) => write!(formatter, "history JSON failed: {error}"),
            Self::Corrupt(message) => write!(formatter, "history data is corrupt: {message}"),
            Self::Range(message) => write!(formatter, "history value is out of range: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid history operation: {message}"),
            Self::Conflict(message) => write!(formatter, "history operation conflicts: {message}"),
            Self::AuthorityLost(message) => {
                write!(formatter, "mutation receipt authority was lost: {message}")
            }
            Self::NotFound(message) => write!(formatter, "history target was not found: {message}"),
            Self::Capacity(message) => {
                write!(formatter, "history capacity is unavailable: {message}")
            }
            Self::UnsupportedSchema(message) => {
                write!(formatter, "unsupported history schema: {message}")
            }
            Self::UnsafePath(message) => write!(formatter, "unsafe history path: {message}"),
        }
    }
}

impl Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Clone, Debug)]
pub struct HistoryStore {
    path: PathBuf,
    startup_recovery: Option<StorageRecoveryOccurrence>,
}

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            let existed = parent.exists();
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            if !existed {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
        let startup_recovery = match preflight_store_file(&path) {
            Ok(_) => {
                initialize_store_file(&path)?;
                None
            }
            Err(error) if is_recoverable_corruption(&error) => {
                let reason = recovery_reason(&error);
                let occurrence = quarantine_database(&path, reason)?;
                initialize_store_file(&path)?;
                let store = Self {
                    path: path.clone(),
                    startup_recovery: None,
                };
                store.persist_storage_recovery(&occurrence)?;
                Some(store.latest_storage_recovery()?.ok_or_else(|| {
                    StoreError::Corrupt(
                        "persisted storage recovery could not be read back".to_owned(),
                    )
                })?)
            }
            Err(error) => return Err(error),
        };
        let store = Self {
            path,
            startup_recovery,
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&store.path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(store)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn schema_version() -> u32 {
        SCHEMA_VERSION as u32
    }

    #[must_use]
    pub fn startup_recovery(&self) -> Option<&StorageRecoveryOccurrence> {
        self.startup_recovery.as_ref()
    }

    pub fn latest_storage_recovery(&self) -> Result<Option<StorageRecoveryOccurrence>, StoreError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT recovery_id, public_token, occurred_at_ms, reason_id,
                        quarantine_directory_name, quarantined_sidecar_count
                 FROM storage_recoveries
                 ORDER BY occurred_at_ms DESC, id DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(recovery_id, public_token, occurred_at, reason, quarantine_name, sidecar_count)| {
                let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
                Ok(StorageRecoveryOccurrence {
                    recovery_id,
                    public_token,
                    occurred_at_unix_millis: u64::try_from(occurred_at).map_err(|_| {
                        StoreError::Corrupt("negative storage recovery timestamp".to_owned())
                    })?,
                    reason: StorageRecoveryReason::parse(&reason)?,
                    quarantine_directory: parent.join(quarantine_name),
                    quarantined_sidecar_count: usize::try_from(sidecar_count).map_err(|_| {
                        StoreError::Corrupt(
                            "negative or oversized storage recovery sidecar count".to_owned(),
                        )
                    })?,
                })
            },
        )
        .transpose()
    }

    pub fn record_storage_residue_observation(
        &self,
        observation: &StorageResidueObservation,
    ) -> Result<(), StoreError> {
        if observation.kind != StorageResidueKind::ChromeCodeSignClone
            || observation.automatic_cleanup_eligible
        {
            return Err(StoreError::Invalid(
                "storage residue observations must remain typed and observe-only".to_owned(),
            ));
        }
        let payload_json = serde_json::to_string(observation)?;
        if payload_json.len() > MAX_STORAGE_RESIDUE_OBSERVATION_BYTES {
            return Err(StoreError::Invalid(
                "storage residue observation exceeds its storage bound".to_owned(),
            ));
        }
        let observed_at = sqlite_millis(
            observation.observed_at_unix_millis,
            "storage residue observation timestamp",
        )?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO storage_residue_latest (
                 kind, observed_at_ms, payload_json
             ) VALUES ('chrome_code_sign_clone', ?1, ?2)
             ON CONFLICT(kind) DO UPDATE SET
                 observed_at_ms = excluded.observed_at_ms,
                 payload_json = excluded.payload_json",
            params![observed_at, payload_json],
        )?;
        Ok(())
    }

    pub fn latest_storage_residue_observation(
        &self,
    ) -> Result<Option<StorageResidueObservation>, StoreError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT observed_at_ms, payload_json FROM storage_residue_latest
                 WHERE kind = 'chrome_code_sign_clone'",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(observed_at, payload_json)| {
            if payload_json.len() > MAX_STORAGE_RESIDUE_OBSERVATION_BYTES {
                return Err(StoreError::Corrupt(
                    "persisted storage residue observation exceeds its bound".to_owned(),
                ));
            }
            let observation = serde_json::from_str::<StorageResidueObservation>(&payload_json)?;
            let indexed_at =
                parse_nonnegative_millis(observed_at, "storage residue observation timestamp")?;
            if observation.kind != StorageResidueKind::ChromeCodeSignClone
                || observation.observed_at_unix_millis != indexed_at
                || observation.automatic_cleanup_eligible
            {
                return Err(StoreError::Corrupt(
                    "storage residue index disagrees with its observe-only payload".to_owned(),
                ));
            }
            Ok(observation)
        })
        .transpose()
    }

    fn persist_storage_recovery(
        &self,
        occurrence: &StorageRecoveryOccurrence,
    ) -> Result<(), StoreError> {
        let occurred_at = sqlite_millis(
            occurrence.occurred_at_unix_millis,
            "storage recovery timestamp",
        )?;
        let sidecar_count = i64::try_from(occurrence.quarantined_sidecar_count).map_err(|_| {
            StoreError::Range("storage recovery sidecar count overflowed i64".to_owned())
        })?;
        let quarantine_name = occurrence
            .quarantine_directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                StoreError::Invalid(
                    "storage recovery quarantine directory must have a UTF-8 basename".to_owned(),
                )
            })?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO storage_recoveries (
                 recovery_id, public_token, occurred_at_ms, reason_id,
                 quarantine_directory_name, quarantined_sidecar_count
             ) VALUES (?1, lower(hex(randomblob(16))), ?2, ?3, ?4, ?5)",
            params![
                occurrence.recovery_id,
                occurred_at,
                occurrence.reason.as_str(),
                quarantine_name,
                sidecar_count
            ],
        )?;
        Ok(())
    }

    pub fn begin_managed_boot(
        &self,
        activation_generation: u64,
        instance_id: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        validate_managed_identity(activation_generation, instance_id)?;
        let generation = sqlite_generation(activation_generation)?;
        let occurred_at = sqlite_millis(occurred_at_unix_millis, "managed boot timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let carry_requested_enforce =
            managed_lifecycle_transaction(&transaction)?.is_some_and(|lifecycle| {
                let ready_enforce_restart = lifecycle.startup_phase
                    == ManagedStartupPhase::ReadyEnforce
                    && lifecycle.effective_enforce;
                let graceful_pre_ready_restart = lifecycle.startup_phase
                    == ManagedStartupPhase::ReadyReportOnly
                    && !lifecycle.ready
                    && !lifecycle.effective_enforce
                    && lifecycle.armed_generation.is_none()
                    && lifecycle.enforcement_epoch.is_none();
                lifecycle.activation_generation == activation_generation
                    && lifecycle.requested_enforce
                    && (ready_enforce_restart || graceful_pre_ready_restart)
            });
        transaction.execute(
            "INSERT INTO managed_lifecycle (
                 singleton, activation_generation, instance_id,
                 requested_enforce, effective_enforce, armed_generation,
                 enforcement_epoch, ready, draining, startup_phase, updated_at_ms
             ) VALUES (1, ?1, ?2, ?3, 0, NULL, NULL, 0, 0, 'recovering', ?4)
             ON CONFLICT(singleton) DO UPDATE SET
                 activation_generation = excluded.activation_generation,
                 instance_id = excluded.instance_id,
                 requested_enforce = excluded.requested_enforce,
                 effective_enforce = 0,
                 armed_generation = NULL,
                 enforcement_epoch = NULL,
                 ready = 0,
                 draining = 0,
                 startup_phase = 'recovering',
                 updated_at_ms = excluded.updated_at_ms",
            params![
                generation,
                instance_id,
                i64::from(carry_requested_enforce),
                occurred_at
            ],
        )?;
        let lifecycle = managed_lifecycle_transaction(&transaction)?.ok_or_else(|| {
            StoreError::Corrupt("managed boot did not create lifecycle state".to_owned())
        })?;
        transaction.commit()?;
        Ok(lifecycle)
    }

    pub fn finish_managed_recovery(
        &self,
        activation_generation: u64,
        instance_id: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        self.transition_managed(
            activation_generation,
            instance_id,
            occurred_at_unix_millis,
            |transaction, lifecycle, occurred_at| {
                if lifecycle.draining {
                    return Err(StoreError::Invalid(
                        "managed daemon is already draining".to_owned(),
                    ));
                }
                transaction.execute(
                    "UPDATE managed_lifecycle
                     SET effective_enforce = 0, armed_generation = NULL,
                         enforcement_epoch = NULL,
                         ready = 0, startup_phase = 'first_scan_report_only',
                         updated_at_ms = ?1
                     WHERE singleton = 1",
                    params![occurred_at],
                )?;
                Ok(())
            },
        )
    }

    /// Preserve owner-requested enforcement across a normal raw shutdown that
    /// arrives before the signal-free first scan completes. Fatal startup exits
    /// never call this transition, so their recovering/first-scan state cannot
    /// accidentally carry intent when durable fail-close itself fails.
    pub fn preserve_managed_pre_ready_restart_intent(
        &self,
        activation_generation: u64,
        instance_id: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        self.transition_managed(
            activation_generation,
            instance_id,
            occurred_at_unix_millis,
            |transaction, lifecycle, occurred_at| {
                if lifecycle.draining {
                    return Err(StoreError::Invalid(
                        "draining managed daemon cannot preserve restart intent".to_owned(),
                    ));
                }
                if lifecycle.ready
                    || !matches!(
                        lifecycle.startup_phase,
                        ManagedStartupPhase::Recovering | ManagedStartupPhase::FirstScanReportOnly
                    )
                {
                    return Err(StoreError::Invalid(
                        "managed daemon is not in pre-ready startup".to_owned(),
                    ));
                }
                transaction.execute(
                    "UPDATE managed_lifecycle
                     SET effective_enforce = 0, armed_generation = NULL,
                         enforcement_epoch = NULL, ready = 0, draining = 0,
                         startup_phase = 'ready_report_only', updated_at_ms = ?1
                     WHERE singleton = 1",
                    params![occurred_at],
                )?;
                Ok(())
            },
        )
    }

    pub fn complete_managed_first_scan(
        &self,
        activation_generation: u64,
        instance_id: &str,
        enforcement_epoch: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        validate_enforcement_epoch(enforcement_epoch)?;
        self.transition_managed(
            activation_generation,
            instance_id,
            occurred_at_unix_millis,
            |transaction, lifecycle, occurred_at| {
                if lifecycle.draining {
                    return Err(StoreError::Invalid(
                        "managed daemon is already draining".to_owned(),
                    ));
                }
                if lifecycle.startup_phase != ManagedStartupPhase::FirstScanReportOnly {
                    return Err(StoreError::Invalid(
                        "managed daemon is not awaiting its first report-only scan".to_owned(),
                    ));
                }
                if lifecycle.requested_enforce {
                    if managed_arm_blocker(transaction)?.is_some() {
                        apply_managed_report_only_ready(transaction, occurred_at, true)?;
                    } else {
                        apply_managed_arm(transaction, enforcement_epoch, occurred_at)?;
                    }
                } else {
                    apply_managed_report_only_ready(transaction, occurred_at, false)?;
                }
                Ok(())
            },
        )
    }

    pub fn arm_managed(
        &self,
        activation_generation: u64,
        instance_id: &str,
        enforcement_epoch: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        validate_enforcement_epoch(enforcement_epoch)?;
        self.transition_managed(
            activation_generation,
            instance_id,
            occurred_at_unix_millis,
            |transaction, lifecycle, occurred_at| {
                if lifecycle.effective_enforce
                    && lifecycle.armed_generation == Some(activation_generation)
                    && lifecycle.enforcement_epoch.is_some()
                {
                    return Ok(());
                }
                if lifecycle.draining {
                    return Err(StoreError::Invalid(
                        "managed daemon is draining and cannot arm".to_owned(),
                    ));
                }
                if !lifecycle.ready {
                    return Err(StoreError::Invalid(
                        "managed daemon is not ready for enforcement".to_owned(),
                    ));
                }
                if let Some(blocker) = managed_arm_blocker(transaction)? {
                    return Err(StoreError::Invalid(blocker.to_owned()));
                }
                apply_managed_arm(transaction, enforcement_epoch, occurred_at)?;
                Ok(())
            },
        )
    }

    /// Clears durable enforcement intent while the managed daemon is proven absent.
    ///
    /// The caller owns the daemon-absence proof. Generation matching prevents a
    /// stale recovery operation from mutating a replacement generation.
    pub fn clear_managed_enforce_request_offline(
        &self,
        activation_generation: u64,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        if activation_generation == 0 {
            return Err(StoreError::Invalid(
                "managed activation generation must be greater than zero".to_owned(),
            ));
        }
        let generation = sqlite_generation(activation_generation)?;
        let occurred_at = sqlite_millis(
            occurred_at_unix_millis,
            "offline managed report-only recovery timestamp",
        )?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = managed_lifecycle_transaction(&transaction)?
            .ok_or_else(|| StoreError::Invalid("managed lifecycle has not begun".to_owned()))?;
        if current.activation_generation != activation_generation {
            return Err(StoreError::Invalid(format!(
                "managed activation generation mismatch: expected {}, got {activation_generation}",
                current.activation_generation
            )));
        }
        transaction.execute(
            "UPDATE managed_lifecycle
             SET requested_enforce = 0, effective_enforce = 0,
                 armed_generation = NULL, enforcement_epoch = NULL,
                 ready = 0, draining = 0, startup_phase = 'recovering',
                 updated_at_ms = ?1
             WHERE singleton = 1 AND activation_generation = ?2",
            params![occurred_at, generation],
        )?;
        let lifecycle = managed_lifecycle_transaction(&transaction)?.ok_or_else(|| {
            StoreError::Corrupt(
                "managed lifecycle disappeared during offline report-only recovery".to_owned(),
            )
        })?;
        transaction.commit()?;
        Ok(lifecycle)
    }

    pub fn disarm_managed(
        &self,
        activation_generation: u64,
        instance_id: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        self.transition_managed(
            activation_generation,
            instance_id,
            occurred_at_unix_millis,
            |transaction, lifecycle, occurred_at| {
                let phase = if lifecycle.draining {
                    "draining"
                } else if lifecycle.ready {
                    "ready_report_only"
                } else {
                    managed_startup_phase_name(lifecycle.startup_phase)
                };
                transaction.execute(
                    "UPDATE managed_lifecycle
                     SET requested_enforce = 0, effective_enforce = 0,
                         armed_generation = NULL, enforcement_epoch = NULL,
                         startup_phase = ?1, updated_at_ms = ?2
                     WHERE singleton = 1",
                    params![phase, occurred_at],
                )?;
                Ok(())
            },
        )
    }

    pub fn begin_managed_drain(
        &self,
        activation_generation: u64,
        instance_id: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        self.transition_managed(
            activation_generation,
            instance_id,
            occurred_at_unix_millis,
            |transaction, _lifecycle, occurred_at| {
                transaction.execute(
                    "UPDATE managed_lifecycle
                     SET requested_enforce = 0, effective_enforce = 0,
                         armed_generation = NULL, enforcement_epoch = NULL,
                         ready = 0, draining = 1, startup_phase = 'draining',
                         updated_at_ms = ?1
                     WHERE singleton = 1",
                    params![occurred_at],
                )?;
                Ok(())
            },
        )
    }

    pub fn fail_managed(
        &self,
        activation_generation: u64,
        instance_id: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<ManagedLifecycle, StoreError> {
        self.transition_managed(
            activation_generation,
            instance_id,
            occurred_at_unix_millis,
            |transaction, lifecycle, occurred_at| {
                let (phase, draining) = if lifecycle.draining {
                    ("draining", 1_i64)
                } else {
                    ("failed", 0_i64)
                };
                transaction.execute(
                    "UPDATE managed_lifecycle
                     SET requested_enforce = 0, effective_enforce = 0,
                         armed_generation = NULL, enforcement_epoch = NULL,
                         ready = 0, draining = ?1, startup_phase = ?2,
                         updated_at_ms = ?3
                     WHERE singleton = 1",
                    params![draining, phase, occurred_at],
                )?;
                Ok(())
            },
        )
    }

    pub fn managed_lifecycle(&self) -> Result<Option<ManagedLifecycle>, StoreError> {
        let connection = self.connection()?;
        managed_lifecycle_connection(&connection)
    }

    pub fn automatic_enforcement_blocked(&self) -> Result<bool, StoreError> {
        let connection = self.connection()?;
        Ok(persistent_enforcement_blocker(&connection)?.is_some())
    }

    fn transition_managed(
        &self,
        activation_generation: u64,
        instance_id: &str,
        occurred_at_unix_millis: u64,
        transition: impl FnOnce(&Transaction<'_>, &ManagedLifecycle, i64) -> Result<(), StoreError>,
    ) -> Result<ManagedLifecycle, StoreError> {
        validate_managed_identity(activation_generation, instance_id)?;
        let occurred_at = sqlite_millis(occurred_at_unix_millis, "managed lifecycle timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = managed_lifecycle_transaction(&transaction)?
            .ok_or_else(|| StoreError::Invalid("managed lifecycle has not begun".to_owned()))?;
        require_exact_managed_identity(&current, activation_generation, instance_id)?;
        transition(&transaction, &current, occurred_at)?;
        let lifecycle = managed_lifecycle_transaction(&transaction)?.ok_or_else(|| {
            StoreError::Corrupt("managed lifecycle disappeared during transition".to_owned())
        })?;
        transaction.commit()?;
        Ok(lifecycle)
    }

    #[must_use]
    pub fn journal_for<'a>(&'a self, attempt: &CleanupAttemptHandle) -> CleanupAttemptJournal<'a> {
        CleanupAttemptJournal {
            store: self,
            attempt: attempt.clone(),
            next_sequence: 0,
            prepared: BTreeMap::new(),
            next_artifact_sequence: 0,
            prepared_artifacts: BTreeMap::new(),
        }
    }

    pub fn record_observation(
        &self,
        occurred_at_unix_millis: u64,
        report: &IncidentReport,
    ) -> Result<i64, StoreError> {
        let redacted = ObservationRecord::from(report);
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let event_id =
            upsert_observation_span_transaction(&transaction, occurred_at_unix_millis, &redacted)?;
        transaction.commit()?;
        Ok(event_id)
    }

    /// Commits one complete owner-protection-adjusted observation snapshot.
    /// A failed row or token insert rolls the whole cycle back, so callers can
    /// never publish a fresh roster backed by partial durable history.
    pub fn record_observation_batch(
        &self,
        occurred_at_unix_millis: u64,
        reports: &[IncidentReport],
    ) -> Result<Vec<i64>, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut event_ids = Vec::with_capacity(reports.len());
        for report in reports {
            let redacted = ObservationRecord::from(report);
            event_ids.push(upsert_observation_span_transaction(
                &transaction,
                occurred_at_unix_millis,
                &redacted,
            )?);
        }
        transaction.commit()?;
        Ok(event_ids)
    }

    pub fn begin_cleanup_attempt(
        &self,
        occurred_at_unix_millis: u64,
        report: &IncidentReport,
        enforcement_epoch: &str,
    ) -> Result<CleanupAttemptHandle, StoreError> {
        if report.state != IncidentState::Confirmed || !report.gates.cleanup_eligible() {
            return Err(StoreError::Invalid(
                "cleanup attempts require an eligible CONFIRMED incident".to_owned(),
            ));
        }
        if enforcement_epoch.trim().is_empty() {
            return Err(StoreError::Invalid(
                "cleanup attempts require a non-empty enforcement epoch".to_owned(),
            ));
        }
        let occurred_at = sqlite_millis(occurred_at_unix_millis, "cleanup attempt timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let blocked = transaction
            .query_row(
                "SELECT 1 FROM cleanup_retry_blocks WHERE incident_id = ?1",
                params![report.incident_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if blocked {
            return Err(StoreError::Invalid(format!(
                "cleanup retry is blocked for incident {:?}",
                report.incident_id
            )));
        }
        let open_incident = transaction
            .query_row(
                "SELECT incident_id FROM cleanup_attempts WHERE completed_at_ms IS NULL LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(open_incident) = open_incident {
            return Err(StoreError::Invalid(format!(
                "cleanup attempt for incident {open_incident:?} is already open"
            )));
        }
        transaction.execute(
            "INSERT INTO cleanup_attempts (
                 incident_id, tracking_key, root_identity_fingerprint,
                 member_fingerprint, enforcement_epoch, started_at_ms,
                 signature_pack, signature_version, observed_process_count,
                 observed_resident_memory_bytes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                report.incident_id,
                report.tracking_key,
                report.root.identity_fingerprint,
                report.member_fingerprint,
                enforcement_epoch,
                occurred_at,
                report.signature_pack,
                report.signature_version,
                i64::try_from(report.member_count).map_err(|_| {
                    StoreError::Range("cleanup process count overflowed i64".to_owned())
                })?,
                sqlite_u64(
                    report.resident_memory_bytes,
                    "cleanup observed resident memory",
                )?
            ],
        )?;
        let attempt_id = transaction.last_insert_rowid();
        let started = CleanupReceipt {
            incident_id: report.incident_id.clone(),
            state: IncidentState::Reclaiming,
            reason_id: None,
            actions: Vec::new(),
            artifact_actions: Vec::new(),
            survivor_pids: Vec::new(),
            revival_checks_completed: 0,
            resources: CleanupResources::default(),
        };
        insert_event_transaction(
            &transaction,
            Some(attempt_id),
            occurred_at_unix_millis,
            &report.incident_id,
            EventKind::Cleanup,
            IncidentState::Reclaiming,
            &EventPayload::Cleanup { receipt: started },
        )?;
        transaction.commit()?;
        Ok(CleanupAttemptHandle {
            id: attempt_id,
            incident_id: report.incident_id.clone(),
        })
    }

    pub fn prepare_cleanup_action(
        &self,
        attempt: &CleanupAttemptHandle,
        sequence: usize,
        occurred_at_unix_millis: u64,
        intent: &CleanupActionIntent,
    ) -> Result<PreparedActionHandle, StoreError> {
        if intent.identity_fingerprint.trim().is_empty() {
            return Err(StoreError::Invalid(
                "cleanup action requires a redacted identity fingerprint".to_owned(),
            ));
        }
        let sequence_i64 = i64::try_from(sequence)
            .map_err(|_| StoreError::Range("cleanup action sequence overflowed i64".to_owned()))?;
        let pid = i64::from(intent.pid);
        let occurred_at = sqlite_millis(occurred_at_unix_millis, "action preparation timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = transaction
            .query_row(
                "SELECT incident_id, completed_at_ms
                 FROM cleanup_attempts WHERE id = ?1",
                params![attempt.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::Invalid("cleanup attempt does not exist".to_owned()))?;
        if stored.0 != attempt.incident_id {
            return Err(StoreError::Invalid(
                "cleanup attempt handle incident does not match storage".to_owned(),
            ));
        }
        if stored.1.is_some() {
            return Err(StoreError::Invalid(
                "cleanup attempt is already terminal".to_owned(),
            ));
        }
        transaction.execute(
            "INSERT INTO cleanup_actions (
                 attempt_id, sequence, prepared_at_ms, stage, pid,
                 identity_fingerprint, signal
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                attempt.id,
                sequence_i64,
                occurred_at,
                cleanup_stage_name(intent.stage),
                pid,
                intent.identity_fingerprint,
                cleanup_signal_name(intent.signal)
            ],
        )?;
        let action_id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(PreparedActionHandle {
            id: action_id,
            attempt_id: attempt.id,
            sequence,
        })
    }

    pub fn complete_cleanup_action(
        &self,
        prepared: &PreparedActionHandle,
        occurred_at_unix_millis: u64,
        disposition: SignalDisposition,
    ) -> Result<(), StoreError> {
        let occurred_at = sqlite_millis(occurred_at_unix_millis, "action disposition timestamp")?;
        let sequence = i64::try_from(prepared.sequence)
            .map_err(|_| StoreError::Range("cleanup action sequence overflowed i64".to_owned()))?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = transaction
            .query_row(
                "SELECT attempt_id, sequence, disposition
                 FROM cleanup_actions WHERE id = ?1",
                params![prepared.id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::Invalid("prepared cleanup action does not exist".to_owned())
            })?;
        if stored.0 != prepared.attempt_id || stored.1 != sequence {
            return Err(StoreError::Invalid(
                "prepared cleanup action handle does not match storage".to_owned(),
            ));
        }
        match stored.2 {
            Some(existing) => {
                let existing = parse_signal_disposition(&existing)?;
                if existing != disposition {
                    return Err(StoreError::Invalid(format!(
                        "cleanup action has conflicting disposition: stored {}, requested {}",
                        signal_disposition_name(existing),
                        signal_disposition_name(disposition)
                    )));
                }
            }
            None => {
                transaction.execute(
                    "UPDATE cleanup_actions
                     SET disposition = ?1, completed_at_ms = ?2
                     WHERE id = ?3 AND disposition IS NULL",
                    params![
                        signal_disposition_name(disposition),
                        occurred_at,
                        prepared.id
                    ],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn prepare_cleanup_artifact_action(
        &self,
        attempt: &CleanupAttemptHandle,
        sequence: usize,
        occurred_at_unix_millis: u64,
        intent: &ArtifactActionIntent,
    ) -> Result<PreparedArtifactActionHandle, StoreError> {
        if !valid_artifact_fingerprint(&intent.artifact_fingerprint) {
            return Err(StoreError::Invalid(
                "artifact action requires a bounded redacted artifact fingerprint".to_owned(),
            ));
        }
        let sequence_i64 = i64::try_from(sequence).map_err(|_| {
            StoreError::Range("cleanup artifact action sequence overflowed i64".to_owned())
        })?;
        let occurred_at = sqlite_millis(
            occurred_at_unix_millis,
            "artifact action preparation timestamp",
        )?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = transaction
            .query_row(
                "SELECT incident_id, completed_at_ms
                 FROM cleanup_attempts WHERE id = ?1",
                params![attempt.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::Invalid("cleanup attempt does not exist".to_owned()))?;
        if stored.0 != attempt.incident_id {
            return Err(StoreError::Invalid(
                "cleanup attempt handle incident does not match storage".to_owned(),
            ));
        }
        if stored.1.is_some() {
            return Err(StoreError::Invalid(
                "cleanup attempt is already terminal".to_owned(),
            ));
        }
        transaction.execute(
            "INSERT INTO cleanup_artifact_actions (
                 attempt_id, sequence, prepared_at_ms, kind, artifact_fingerprint
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                attempt.id,
                sequence_i64,
                occurred_at,
                runtime_artifact_kind_name(intent.kind),
                intent.artifact_fingerprint
            ],
        )?;
        let action_id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(PreparedArtifactActionHandle {
            id: action_id,
            attempt_id: attempt.id,
            sequence,
        })
    }

    pub fn complete_cleanup_artifact_action(
        &self,
        prepared: &PreparedArtifactActionHandle,
        occurred_at_unix_millis: u64,
        disposition: ArtifactDisposition,
    ) -> Result<(), StoreError> {
        let occurred_at = sqlite_millis(
            occurred_at_unix_millis,
            "artifact action disposition timestamp",
        )?;
        let sequence = i64::try_from(prepared.sequence).map_err(|_| {
            StoreError::Range("cleanup artifact action sequence overflowed i64".to_owned())
        })?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = transaction
            .query_row(
                "SELECT attempt_id, sequence, disposition
                 FROM cleanup_artifact_actions WHERE id = ?1",
                params![prepared.id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::Invalid("prepared cleanup artifact action does not exist".to_owned())
            })?;
        if stored.0 != prepared.attempt_id || stored.1 != sequence {
            return Err(StoreError::Invalid(
                "prepared cleanup artifact action handle does not match storage".to_owned(),
            ));
        }
        match stored.2 {
            Some(existing) => {
                let existing = parse_artifact_disposition(&existing)?;
                if existing != disposition {
                    return Err(StoreError::Invalid(format!(
                        "cleanup artifact action has conflicting disposition: stored {}, requested {}",
                        artifact_disposition_name(existing),
                        artifact_disposition_name(disposition)
                    )));
                }
            }
            None => {
                transaction.execute(
                    "UPDATE cleanup_artifact_actions
                     SET disposition = ?1, completed_at_ms = ?2
                     WHERE id = ?3 AND disposition IS NULL",
                    params![
                        artifact_disposition_name(disposition),
                        occurred_at,
                        prepared.id
                    ],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn complete_cleanup_attempt(
        &self,
        attempt: &CleanupAttemptHandle,
        occurred_at_unix_millis: u64,
        terminal_receipt: &CleanupReceipt,
    ) -> Result<CleanupReceipt, StoreError> {
        if !matches!(
            terminal_receipt.state,
            IncidentState::Cleared | IncidentState::Failed | IncidentState::Revived
        ) {
            return Err(StoreError::Invalid(
                "cleanup attempt terminal state must be CLEARED, FAILED, or REVIVED".to_owned(),
            ));
        }
        if terminal_receipt.incident_id != attempt.incident_id {
            return Err(StoreError::Invalid(
                "cleanup receipt incident does not match attempt handle".to_owned(),
            ));
        }
        let occurred_at = sqlite_millis(occurred_at_unix_millis, "attempt completion timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = transaction
            .query_row(
                "SELECT incident_id, tracking_key, terminal_state
                 FROM cleanup_attempts WHERE id = ?1",
                params![attempt.id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::Invalid("cleanup attempt does not exist".to_owned()))?;
        if stored.0 != attempt.incident_id {
            return Err(StoreError::Invalid(
                "cleanup attempt handle incident does not match storage".to_owned(),
            ));
        }
        if let Some(stored_state) = stored.2 {
            let stored_state = parse_state(&stored_state)?;
            if stored_state != terminal_receipt.state {
                return Err(StoreError::Invalid(format!(
                    "cleanup attempt is already terminal as {}",
                    state_name(stored_state)
                )));
            }
            let receipt = terminal_receipt_for_attempt(&transaction, attempt.id)?;
            transaction.commit()?;
            return Ok(receipt);
        }

        let prepared_count = transaction.query_row(
            "SELECT COUNT(*) FROM cleanup_actions
             WHERE attempt_id = ?1 AND disposition IS NULL",
            params![attempt.id],
            |row| row.get::<_, i64>(0),
        )?;
        let prepared_artifact_count = transaction.query_row(
            "SELECT COUNT(*) FROM cleanup_artifact_actions
             WHERE attempt_id = ?1 AND disposition IS NULL",
            params![attempt.id],
            |row| row.get::<_, i64>(0),
        )?;
        if prepared_count > 0 || prepared_artifact_count > 0 {
            return Err(StoreError::Invalid(format!(
                "cleanup attempt {} still has {prepared_count} PREPARED process action(s) and {prepared_artifact_count} PREPARED artifact action(s); startup recovery must resolve delivery uncertainty",
                attempt.id,
            )));
        }
        let actions = actions_for_attempt(&transaction, attempt.id)?;
        let artifact_actions = artifact_actions_for_attempt(&transaction, attempt.id)?;
        let resources_json = serde_json::to_string(&terminal_receipt.resources)?;
        if resources_json.len() > MAX_RESOURCE_RECEIPT_BYTES {
            return Err(StoreError::Invalid(
                "cleanup resource receipt exceeds its storage bound".to_owned(),
            ));
        }
        let resources = serde_json::from_str::<CleanupResources>(&resources_json)?;
        let projected = CleanupReceipt {
            incident_id: attempt.incident_id.clone(),
            state: terminal_receipt.state,
            reason_id: terminal_receipt.reason_id.clone(),
            actions,
            artifact_actions,
            survivor_pids: terminal_receipt.survivor_pids.clone(),
            revival_checks_completed: terminal_receipt.revival_checks_completed,
            resources,
        };
        let survivor_pids_json = serde_json::to_string(&projected.survivor_pids)?;
        transaction.execute(
            "UPDATE cleanup_attempts
             SET completed_at_ms = ?1, terminal_state = ?2, reason_id = ?3,
                 survivor_pids_json = ?4, revival_checks_completed = ?5,
                 resources_json = ?6
             WHERE id = ?7 AND completed_at_ms IS NULL",
            params![
                occurred_at,
                state_name(projected.state),
                projected.reason_id,
                survivor_pids_json,
                i64::try_from(projected.revival_checks_completed).map_err(|_| {
                    StoreError::Range("revival check count overflowed i64".to_owned())
                })?,
                resources_json,
                attempt.id
            ],
        )?;
        if matches!(
            projected.state,
            IncidentState::Failed | IncidentState::Revived
        ) {
            insert_retry_block(
                &transaction,
                &projected.incident_id,
                &stored.1,
                occurred_at,
                projected.reason_id.as_deref(),
                attempt.id,
            )?;
        }
        let terminal_event_id = insert_event_transaction(
            &transaction,
            Some(attempt.id),
            occurred_at_unix_millis,
            &projected.incident_id,
            EventKind::Cleanup,
            projected.state,
            &EventPayload::Cleanup {
                receipt: projected.clone(),
            },
        )?;
        insert_cleanup_impact_transaction(
            &transaction,
            attempt.id,
            terminal_event_id,
            occurred_at_unix_millis,
            &projected,
        )?;
        transaction.commit()?;
        Ok(projected)
    }

    pub fn recover_incomplete_attempts(
        &self,
        occurred_at_unix_millis: u64,
    ) -> Result<Vec<CleanupReceipt>, StoreError> {
        let occurred_at = sqlite_millis(occurred_at_unix_millis, "attempt recovery timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let open_attempts = {
            let mut statement = transaction.prepare(
                "SELECT id, incident_id, tracking_key
                 FROM cleanup_attempts WHERE completed_at_ms IS NULL ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut recovered = Vec::with_capacity(open_attempts.len());
        for (attempt_id, incident_id, tracking_key) in open_attempts {
            let prepared_count = transaction.query_row(
                "SELECT COUNT(*) FROM cleanup_actions
                 WHERE attempt_id = ?1 AND disposition IS NULL",
                params![attempt_id],
                |row| row.get::<_, i64>(0),
            )?;
            let prepared_artifact_count = transaction.query_row(
                "SELECT COUNT(*) FROM cleanup_artifact_actions
                 WHERE attempt_id = ?1 AND disposition IS NULL",
                params![attempt_id],
                |row| row.get::<_, i64>(0),
            )?;
            let completed_unknown_count = transaction.query_row(
                "SELECT
                     (SELECT COUNT(*) FROM cleanup_actions
                      WHERE attempt_id = ?1 AND disposition = 'delivery_unknown')
                   + (SELECT COUNT(*) FROM cleanup_artifact_actions
                      WHERE attempt_id = ?1 AND disposition = 'delivery_unknown')",
                params![attempt_id],
                |row| row.get::<_, i64>(0),
            )?;
            let completed_side_effect_count = transaction.query_row(
                "SELECT
                     (SELECT COUNT(*) FROM cleanup_actions
                      WHERE attempt_id = ?1 AND disposition = 'delivered')
                   + (SELECT COUNT(*) FROM cleanup_artifact_actions
                      WHERE attempt_id = ?1 AND disposition = 'removed')",
                params![attempt_id],
                |row| row.get::<_, i64>(0),
            )?;
            transaction.execute(
                "UPDATE cleanup_actions
                 SET disposition = 'delivery_unknown', completed_at_ms = ?1
                 WHERE attempt_id = ?2 AND disposition IS NULL",
                params![occurred_at, attempt_id],
            )?;
            transaction.execute(
                "UPDATE cleanup_artifact_actions
                 SET disposition = 'delivery_unknown', completed_at_ms = ?1
                 WHERE attempt_id = ?2 AND disposition IS NULL",
                params![occurred_at, attempt_id],
            )?;
            let receipt = CleanupReceipt {
                incident_id: incident_id.clone(),
                state: IncidentState::Failed,
                reason_id: Some(
                    if prepared_count > 0
                        || prepared_artifact_count > 0
                        || completed_unknown_count > 0
                    {
                        "cleanup.interrupted_delivery_unknown"
                    } else if completed_side_effect_count > 0 {
                        "cleanup.interrupted_after_delivery"
                    } else {
                        "cleanup.interrupted_before_signal"
                    }
                    .to_owned(),
                ),
                actions: actions_for_attempt(&transaction, attempt_id)?,
                artifact_actions: artifact_actions_for_attempt(&transaction, attempt_id)?,
                survivor_pids: Vec::new(),
                revival_checks_completed: 0,
                resources: CleanupResources::default(),
            };
            let resources_json = serde_json::to_string(&receipt.resources)?;
            transaction.execute(
                "UPDATE cleanup_attempts
                 SET completed_at_ms = ?1, terminal_state = 'FAILED', reason_id = ?2,
                     survivor_pids_json = '[]', revival_checks_completed = 0,
                     resources_json = ?3
                 WHERE id = ?4 AND completed_at_ms IS NULL",
                params![occurred_at, receipt.reason_id, resources_json, attempt_id],
            )?;
            insert_retry_block(
                &transaction,
                &incident_id,
                &tracking_key,
                occurred_at,
                receipt.reason_id.as_deref(),
                attempt_id,
            )?;
            let terminal_event_id = insert_event_transaction(
                &transaction,
                Some(attempt_id),
                occurred_at_unix_millis,
                &incident_id,
                EventKind::Cleanup,
                IncidentState::Failed,
                &EventPayload::Cleanup {
                    receipt: receipt.clone(),
                },
            )?;
            insert_cleanup_impact_transaction(
                &transaction,
                attempt_id,
                terminal_event_id,
                occurred_at_unix_millis,
                &receipt,
            )?;
            recovered.push(receipt);
        }
        transaction.commit()?;
        Ok(recovered)
    }

    pub fn cleanup_blocked(&self, incident_id: &str) -> Result<bool, StoreError> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT 1 FROM cleanup_retry_blocks WHERE incident_id = ?1",
                params![incident_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn incident_has_observation(&self, incident_id: &str) -> Result<bool, StoreError> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT 1 FROM events
                 WHERE incident_id = ?1 AND kind = 'observation' LIMIT 1",
                params![incident_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn mutation_namespace_token(&self) -> Result<String, StoreError> {
        let connection = self.connection()?;
        current_mutation_namespace(&connection)
    }

    pub fn cleanup_policy_revision(&self) -> Result<u64, StoreError> {
        let connection = self.connection()?;
        cleanup_policy_revision_connection(&connection)
    }

    pub fn mutation_lookup(&self, context: &MutationContext) -> Result<MutationLookup, StoreError> {
        let connection = self.connection()?;
        if let Some(receipt) = mutation_receipt_connection(&connection, context)? {
            return Ok(MutationLookup::Committed(receipt));
        }
        if current_mutation_namespace(&connection)? == context.namespace_token {
            Ok(MutationLookup::NotFound)
        } else {
            Ok(MutationLookup::AuthorityLost)
        }
    }

    pub(crate) fn public_action_store_facts(
        &self,
        mutation: &OrdinaryMutation,
        now_unix_millis: u64,
    ) -> Result<StorePolicyFacts, StoreError> {
        let connection = self.connection()?;
        store_policy_facts_connection(&connection, mutation, now_unix_millis)
    }

    /// Atomically applies one ordinary owner mutation and records its public,
    /// redacted result. A duplicate canonical request returns the first receipt
    /// without repeating the state change; a conflicting reuse of an ID fails.
    pub fn commit_ordinary_mutation(
        &self,
        context: &MutationContext,
        mutation: &OrdinaryMutation,
        committed_at_unix_millis: u64,
    ) -> Result<MutationCommit, StoreError> {
        self.commit_public_ordinary_mutation(
            context,
            mutation,
            RuntimePolicyFacts::default(),
            committed_at_unix_millis,
        )
    }

    pub(crate) fn commit_public_ordinary_mutation(
        &self,
        context: &MutationContext,
        mutation: &OrdinaryMutation,
        runtime_facts: RuntimePolicyFacts,
        committed_at_unix_millis: u64,
    ) -> Result<MutationCommit, StoreError> {
        let committed_at = sqlite_millis(committed_at_unix_millis, "mutation timestamp")?;
        let retain_until_unix_millis = committed_at_unix_millis
            .checked_add(MUTATION_RECONCILIATION_WINDOW_MILLIS)
            .ok_or_else(|| {
                StoreError::Range("mutation retention deadline overflowed u64".to_owned())
            })?;
        let retain_until = sqlite_millis(retain_until_unix_millis, "mutation retention deadline")?;
        let command_json = serde_json::to_string(mutation)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        if let Some((canonical_version, stored_command, receipt_json)) = transaction
            .query_row(
                "SELECT canonical_version, command_json, receipt_json
                 FROM ordinary_mutation_receipts
                 WHERE namespace_token = ?1 AND mutation_id = ?2",
                params![context.namespace_token, context.mutation_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
        {
            if canonical_version != 1 || stored_command != command_json {
                return Err(StoreError::Conflict(
                    "mutation ID was already committed for a different canonical request"
                        .to_owned(),
                ));
            }
            let receipt: MutationReceipt = serde_json::from_str(&receipt_json)?;
            validate_stored_mutation_receipt(&receipt, context, mutation.kind())?;
            transaction.commit()?;
            return Ok(MutationCommit {
                receipt,
                newly_committed: false,
                state_changed: false,
            });
        }

        if current_mutation_namespace(&transaction)? != context.namespace_token {
            return Err(StoreError::AuthorityLost(
                "request namespace is no longer current and has no retained receipt".to_owned(),
            ));
        }

        reserve_mutation_receipt_slot(&transaction)?;
        let store_facts =
            store_policy_facts_connection(&transaction, mutation, committed_at_unix_millis)?;
        let decision = evaluate_action(runtime_facts, store_facts, mutation);
        let (outcome, state_changed) = match decision {
            PolicyDecision::Allow => (
                MutationOutcome::Applied {
                    result: apply_ordinary_mutation(
                        &transaction,
                        mutation,
                        committed_at_unix_millis,
                    )?,
                },
                true,
            ),
            PolicyDecision::NoChange(reason_id) => (
                MutationOutcome::NoChange {
                    reason_id: reason_id.to_owned(),
                },
                false,
            ),
            PolicyDecision::Reject(reason_id) => (
                MutationOutcome::Rejected {
                    reason_id: reason_id.to_owned(),
                },
                false,
            ),
        };

        let current_revision = cleanup_policy_revision_connection(&transaction)?;
        let policy_revision_after = if state_changed {
            let next = current_revision.checked_add(1).ok_or_else(|| {
                StoreError::Range("cleanup policy revision overflowed u64".to_owned())
            })?;
            transaction.execute(
                "UPDATE control_metadata
                 SET cleanup_policy_revision = ?1 WHERE singleton = 1",
                params![sqlite_millis(next, "cleanup policy revision")?],
            )?;
            next
        } else {
            current_revision
        };
        let receipt = MutationReceipt {
            namespace_token: context.namespace_token.clone(),
            mutation_id: context.mutation_id.clone(),
            kind: mutation.kind(),
            committed_at_unix_millis,
            retain_until_unix_millis,
            policy_revision_after,
            outcome,
        };
        let receipt_json = serde_json::to_string(&receipt)?;
        transaction.execute(
            "INSERT INTO ordinary_mutation_receipts (
                 namespace_token, mutation_id, canonical_version, command_json,
                 receipt_json, committed_at_ms, retain_until_ms
             ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)",
            params![
                context.namespace_token,
                context.mutation_id,
                command_json,
                receipt_json,
                committed_at,
                retain_until
            ],
        )?;
        transaction.commit()?;
        Ok(MutationCommit {
            receipt,
            newly_committed: true,
            state_changed,
        })
    }

    pub fn authorize_retry(
        &self,
        incident_id: &str,
        occurred_at_unix_millis: u64,
    ) -> Result<bool, StoreError> {
        let _ = sqlite_millis(occurred_at_unix_millis, "retry authorization timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tracking_key = transaction
            .query_row(
                "SELECT tracking_key FROM cleanup_retry_blocks WHERE incident_id = ?1",
                params![incident_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(tracking_key) = tracking_key else {
            transaction.commit()?;
            return Ok(false);
        };
        transaction.execute(
            "DELETE FROM cleanup_retry_blocks WHERE incident_id = ?1",
            params![incident_id],
        )?;
        transaction.execute(
            "DELETE FROM cooling_candidates WHERE tracking_key = ?1",
            params![tracking_key],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn history(&self, limit: usize) -> Result<Vec<HistoryEvent>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit.min(10_000))
            .map_err(|_| StoreError::Range("history limit overflowed i64".to_owned()))?;
        self.query_events(
            "SELECT events.id, events.attempt_id, events.incident_id,
                    events.first_occurred_at_ms, events.occurred_at_ms,
                    events.observation_count, events.kind, events.state,
                    events.payload_json, tokens.event_token
             FROM events
             JOIN public_event_tokens AS tokens ON tokens.event_id = events.id
             ORDER BY events.occurred_at_ms DESC, events.id DESC LIMIT ?1",
            params![limit],
        )
    }

    pub fn most_recent_reclaim(&self) -> Result<Option<MostRecentReclaim>, StoreError> {
        Ok(self
            .query_cleanup_impacts("WHERE impacts.process_outcome = 'cleared'", 1)?
            .pop()
            .map(|impact| MostRecentReclaim {
                event_token: impact.event_token,
                incident_id: project_identifier(&impact.incident_id, "redacted-incident"),
                occurred_at_unix_millis: impact.occurred_at_unix_millis,
                state: impact.state,
                outcome: impact.outcome,
            }))
    }

    pub fn impact_summary(&self, recent_limit: usize) -> Result<CleanupImpactSummary, StoreError> {
        let connection = self.connection()?;
        let authority = connection.query_row(
            "SELECT tracking_started_at_ms, historical_completeness,
                    terminal_cleanup_count, proved_reclaim_count,
                    reclaimed_process_count, reclaimed_process_measurement_count,
                    estimated_reclaimed_memory_bytes,
                    reclaimed_memory_measurement_count
             FROM impact_authority WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )?;
        let tracking_started_at_unix_millis = u64::try_from(authority.0)
            .map_err(|_| StoreError::Corrupt("negative impact tracking timestamp".to_owned()))?;
        let historical_completeness = ImpactHistoryCompleteness::parse(&authority.1)?;
        let terminal_cleanup_count = usize::try_from(authority.2)
            .map_err(|_| StoreError::Corrupt("invalid terminal cleanup count".to_owned()))?;
        let proved_reclaim_count = usize::try_from(authority.3)
            .map_err(|_| StoreError::Corrupt("invalid proved reclaim count".to_owned()))?;
        let reclaimed_process_count = u64::try_from(authority.4)
            .map_err(|_| StoreError::Corrupt("invalid reclaimed process total".to_owned()))?;
        let reclaimed_process_measurement_count = usize::try_from(authority.5).map_err(|_| {
            StoreError::Corrupt("invalid reclaimed process measurement count".to_owned())
        })?;
        let estimated_reclaimed_memory_bytes = u64::try_from(authority.6)
            .map_err(|_| StoreError::Corrupt("invalid reclaimed memory total".to_owned()))?;
        let reclaimed_memory_measurement_count = usize::try_from(authority.7).map_err(|_| {
            StoreError::Corrupt("invalid reclaimed memory measurement count".to_owned())
        })?;
        drop(connection);
        Ok(CleanupImpactSummary {
            tracking_started_at_unix_millis,
            historical_completeness,
            terminal_cleanup_count,
            proved_reclaim_count,
            reclaimed_process_count: (proved_reclaim_count == reclaimed_process_measurement_count)
                .then(|| usize::try_from(reclaimed_process_count))
                .transpose()
                .map_err(|_| {
                    StoreError::Corrupt("reclaimed process total overflowed usize".to_owned())
                })?,
            estimated_reclaimed_memory_bytes: (proved_reclaim_count
                == reclaimed_memory_measurement_count)
                .then_some(estimated_reclaimed_memory_bytes),
            recent: self.query_cleanup_impacts("", recent_limit.min(1_000))?,
        })
    }

    pub fn cleanup_impact_for_event_token(
        &self,
        event_token: &str,
    ) -> Result<Option<CleanupImpact>, StoreError> {
        let connection = self.connection()?;
        let impact_id = connection
            .query_row(
                "SELECT impacts.id
                 FROM cleanup_impacts AS impacts
                 JOIN public_event_tokens AS tokens ON tokens.event_id = impacts.event_id
                 WHERE tokens.event_token = ?1",
                params![event_token],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        drop(connection);
        let Some(impact_id) = impact_id else {
            return Ok(None);
        };
        Ok(self
            .query_cleanup_impacts(&format!("WHERE impacts.id = {impact_id}"), 1)?
            .pop())
    }

    pub fn protect_incident(
        &self,
        incident_id: &str,
        now_unix_millis: u64,
    ) -> Result<Option<ProtectedIncidentSummary>, StoreError> {
        let now = sqlite_millis(now_unix_millis, "incident protection timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let payload_json = transaction
            .query_row(
                "SELECT payload_json FROM events
                 WHERE incident_id = ?1 AND kind = 'observation'
                 ORDER BY occurred_at_ms DESC, id DESC LIMIT 1",
                params![incident_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(payload_json) = payload_json else {
            let existing = protection_summary_transaction(&transaction, incident_id)?;
            transaction.commit()?;
            return Ok(existing);
        };
        let EventPayload::Observation { report } = serde_json::from_str(&payload_json)? else {
            return Err(StoreError::Corrupt(
                "observation index selected a non-observation payload".to_owned(),
            ));
        };
        if report.incident_id != incident_id {
            return Err(StoreError::Corrupt(
                "observation incident ID disagrees with its index".to_owned(),
            ));
        }
        transaction.execute(
            "INSERT INTO incident_protections (
                 incident_id, root_identity_fingerprint, member_fingerprint, protected_at_ms
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(incident_id) DO UPDATE SET
                 root_identity_fingerprint = excluded.root_identity_fingerprint,
                 member_fingerprint = excluded.member_fingerprint,
                 protected_at_ms = CASE
                     WHEN incident_protections.root_identity_fingerprint
                              = excluded.root_identity_fingerprint
                      AND incident_protections.member_fingerprint = excluded.member_fingerprint
                     THEN incident_protections.protected_at_ms
                     ELSE excluded.protected_at_ms
                 END,
                 last_exact_observed_at_ms = CASE
                     WHEN incident_protections.root_identity_fingerprint
                              = excluded.root_identity_fingerprint
                      AND incident_protections.member_fingerprint = excluded.member_fingerprint
                     THEN incident_protections.last_exact_observed_at_ms
                     ELSE NULL
                 END,
                 absence_since_ms = CASE
                     WHEN incident_protections.root_identity_fingerprint
                              = excluded.root_identity_fingerprint
                      AND incident_protections.member_fingerprint = excluded.member_fingerprint
                     THEN incident_protections.absence_since_ms
                     ELSE NULL
                 END",
            params![
                report.incident_id,
                report.root.identity_fingerprint,
                report.member_fingerprint,
                now
            ],
        )?;
        let summary =
            protection_summary_transaction(&transaction, incident_id)?.ok_or_else(|| {
                StoreError::Corrupt("incident protection disappeared during creation".to_owned())
            })?;
        transaction.commit()?;
        Ok(Some(summary))
    }

    pub fn unprotect_incident(&self, incident_id: &str) -> Result<bool, StoreError> {
        let connection = self.connection()?;
        Ok(connection.execute(
            "DELETE FROM incident_protections WHERE incident_id = ?1",
            params![incident_id],
        )? > 0)
    }

    pub fn is_incident_protected(&self, report: &IncidentReport) -> Result<bool, StoreError> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT 1 FROM incident_protections
                 WHERE incident_id = ?1
                   AND root_identity_fingerprint = ?2
                   AND member_fingerprint = ?3",
                params![
                    report.incident_id,
                    report.root.identity_fingerprint,
                    report.member_fingerprint
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn protection_for_incident(
        &self,
        incident_id: &str,
    ) -> Result<Option<ProtectedIncidentSummary>, StoreError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT incident_id, protected_at_ms,
                        last_exact_observed_at_ms, absence_since_ms
                 FROM incident_protections WHERE incident_id = ?1",
                params![incident_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()?
            .map(
                |(incident_id, protected_at, last_observed, absence_since)| {
                    projected_protection_summary(
                        incident_id,
                        protected_at,
                        last_observed,
                        absence_since,
                    )
                },
            )
            .transpose()
    }

    pub fn protection_projection(&self, limit: usize) -> Result<ProtectionProjection, StoreError> {
        let connection = self.connection()?;
        let count =
            connection.query_row("SELECT COUNT(*) FROM incident_protections", [], |row| {
                row.get::<_, i64>(0)
            })?;
        let protected_incident_count = usize::try_from(count).map_err(|_| {
            StoreError::Corrupt("negative or oversized protection count".to_owned())
        })?;
        let limit = i64::try_from(limit.min(MAX_ATTENTION_SUMMARIES))
            .map_err(|_| StoreError::Range("protection summary limit overflowed i64".to_owned()))?;
        if limit == 0 {
            return Ok(ProtectionProjection {
                protected_incident_count,
                protected_incidents: Vec::new(),
            });
        }
        let mut statement = connection.prepare(
            "SELECT incident_id, protected_at_ms,
                    last_exact_observed_at_ms, absence_since_ms
             FROM incident_protections
             ORDER BY protected_at_ms DESC, incident_id ASC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?;
        let protected_incidents = rows
            .map(|row| {
                let (incident_id, protected_at, last_observed, absence_since) = row?;
                projected_protection_summary(
                    incident_id,
                    protected_at,
                    last_observed,
                    absence_since,
                )
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(ProtectionProjection {
            protected_incident_count,
            protected_incidents,
        })
    }

    pub fn reconcile_incident_protections(
        &self,
        live_root_identity_fingerprints: &BTreeSet<String>,
        absence_proven: bool,
        now_unix_millis: u64,
        exact_absence_retention_millis: u64,
    ) -> Result<ProtectionReconciliation, StoreError> {
        if exact_absence_retention_millis == 0 {
            return Err(StoreError::Invalid(
                "protection exact-absence retention must be non-zero".to_owned(),
            ));
        }
        let now = sqlite_millis(now_unix_millis, "protection reconciliation timestamp")?;
        let retention = sqlite_millis(
            exact_absence_retention_millis,
            "protection exact-absence retention",
        )?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let protections = {
            let mut statement = transaction.prepare(
                "SELECT incident_id, root_identity_fingerprint, absence_since_ms
                 FROM incident_protections",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut result = ProtectionReconciliation::default();
        for (incident_id, root_identity, absence_since) in protections {
            if live_root_identity_fingerprints.contains(&root_identity) {
                transaction.execute(
                    "UPDATE incident_protections
                     SET last_exact_observed_at_ms = ?1, absence_since_ms = NULL
                     WHERE incident_id = ?2",
                    params![now, incident_id],
                )?;
                result.observed_count += 1;
                continue;
            }
            if !absence_proven {
                transaction.execute(
                    "UPDATE incident_protections SET absence_since_ms = NULL
                     WHERE incident_id = ?1",
                    params![incident_id],
                )?;
                result.unproven_count += 1;
                continue;
            }
            result.absent_count += 1;
            match absence_since {
                None => {
                    transaction.execute(
                        "UPDATE incident_protections SET absence_since_ms = ?1
                         WHERE incident_id = ?2",
                        params![now, incident_id],
                    )?;
                }
                Some(since) if now.saturating_sub(since) >= retention => {
                    transaction.execute(
                        "DELETE FROM incident_protections WHERE incident_id = ?1",
                        params![incident_id],
                    )?;
                    result.retired_count += 1;
                }
                Some(_) => {}
            }
        }
        transaction.commit()?;
        Ok(result)
    }

    pub fn attention_projection(
        &self,
        limit: usize,
    ) -> Result<StoreAttentionProjection, StoreError> {
        let connection = self.connection()?;
        let count =
            connection.query_row("SELECT COUNT(*) FROM cleanup_retry_blocks", [], |row| {
                row.get::<_, i64>(0)
            })?;
        let blocked_cleanup_count = usize::try_from(count).map_err(|_| {
            StoreError::Corrupt("negative or oversized retry block count".to_owned())
        })?;
        let limit = i64::try_from(limit.min(MAX_ATTENTION_SUMMARIES))
            .map_err(|_| StoreError::Range("attention summary limit overflowed i64".to_owned()))?;
        if limit == 0 {
            return Ok(StoreAttentionProjection {
                blocked_cleanup_count,
                blocked_cleanups: Vec::new(),
            });
        }
        let mut statement = connection.prepare(
            "SELECT blocks.incident_id, attempts.terminal_state, blocks.blocked_at_ms,
                    blocks.reason_id, blocks.last_exact_observed_at_ms,
                    blocks.absence_since_ms,
                    (
                        SELECT tokens.event_token
                        FROM events
                        JOIN public_event_tokens AS tokens ON tokens.event_id = events.id
                        WHERE events.attempt_id = blocks.source_attempt_id
                          AND events.kind = 'cleanup'
                          AND events.state = attempts.terminal_state
                        ORDER BY events.occurred_at_ms DESC, events.id DESC LIMIT 1
                    ),
                    (
                        SELECT events.payload_json
                        FROM events
                        WHERE events.attempt_id = blocks.source_attempt_id
                          AND events.kind = 'cleanup'
                          AND events.state = attempts.terminal_state
                        ORDER BY events.occurred_at_ms DESC, events.id DESC LIMIT 1
                    )
             FROM cleanup_retry_blocks AS blocks
             JOIN cleanup_attempts AS attempts ON attempts.id = blocks.source_attempt_id
             ORDER BY blocks.blocked_at_ms DESC, blocks.incident_id ASC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;
        let mut blocked_cleanups = Vec::new();
        for row in rows {
            let (
                incident_id,
                state,
                blocked_at,
                reason_id,
                last_observed,
                absence_since,
                event_token,
                payload_json,
            ) = row?;
            let state = parse_state(&state)?;
            if !matches!(state, IncidentState::Failed | IncidentState::Revived) {
                return Err(StoreError::Corrupt(format!(
                    "retry block references non-blocking terminal state {}",
                    state_name(state)
                )));
            }
            let payload_json = payload_json.ok_or_else(|| {
                StoreError::Corrupt("retry block has no matching terminal cleanup event".to_owned())
            })?;
            let EventPayload::Cleanup { receipt } = serde_json::from_str(&payload_json)? else {
                return Err(StoreError::Corrupt(
                    "retry block selected a non-cleanup event payload".to_owned(),
                ));
            };
            let outcome = receipt.outcome();
            blocked_cleanups.push(BlockedCleanupSummary {
                event_token,
                incident_id: project_identifier(&incident_id, "redacted-incident"),
                state,
                blocked_at_unix_millis: parse_nonnegative_millis(
                    blocked_at,
                    "retry block timestamp",
                )?,
                reason_id: project_reason_id(reason_id.as_deref()),
                outcome,
                last_exact_observed_at_unix_millis: last_observed
                    .map(|value| parse_nonnegative_millis(value, "last exact observation"))
                    .transpose()?,
                exact_absence_since_unix_millis: absence_since
                    .map(|value| parse_nonnegative_millis(value, "exact absence timestamp"))
                    .transpose()?,
            });
        }
        Ok(StoreAttentionProjection {
            blocked_cleanup_count,
            blocked_cleanups,
        })
    }

    pub fn reconcile_retry_blocks(
        &self,
        observed: &[ObservedIncidentIdentity],
        live_root_identity_fingerprints: &BTreeSet<String>,
        absence_proven: bool,
        now_unix_millis: u64,
        exact_absence_retention_millis: u64,
    ) -> Result<RetryBlockReconciliation, StoreError> {
        if exact_absence_retention_millis == 0 {
            return Err(StoreError::Invalid(
                "retry block exact-absence retention must be non-zero".to_owned(),
            ));
        }
        let now = sqlite_millis(now_unix_millis, "retry block reconciliation timestamp")?;
        let retention = sqlite_millis(
            exact_absence_retention_millis,
            "retry block exact-absence retention",
        )?;
        let observed = observed
            .iter()
            .map(|identity| {
                (
                    identity.incident_id.as_str(),
                    identity.tracking_key.as_str(),
                    identity.root_identity_fingerprint.as_str(),
                    identity.member_fingerprint.as_str(),
                )
            })
            .collect::<BTreeSet<_>>();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let blocks = {
            let mut statement = transaction.prepare(
                "SELECT blocks.incident_id, blocks.tracking_key,
                        attempts.root_identity_fingerprint, attempts.member_fingerprint,
                        blocks.absence_since_ms
                 FROM cleanup_retry_blocks AS blocks
                 JOIN cleanup_attempts AS attempts ON attempts.id = blocks.source_attempt_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut result = RetryBlockReconciliation::default();
        for (incident_id, tracking_key, root_identity, member_fingerprint, absence_since) in blocks
        {
            let exact_observed = observed.contains(&(
                incident_id.as_str(),
                tracking_key.as_str(),
                root_identity.as_str(),
                member_fingerprint.as_str(),
            )) || live_root_identity_fingerprints.contains(&root_identity);
            if exact_observed {
                transaction.execute(
                    "UPDATE cleanup_retry_blocks
                     SET last_exact_observed_at_ms = ?1, absence_since_ms = NULL
                     WHERE incident_id = ?2",
                    params![now, incident_id],
                )?;
                result.observed_count += 1;
                continue;
            }
            if !absence_proven {
                transaction.execute(
                    "UPDATE cleanup_retry_blocks SET absence_since_ms = NULL
                     WHERE incident_id = ?1",
                    params![incident_id],
                )?;
                result.unproven_count += 1;
                continue;
            }
            result.absent_count += 1;
            match absence_since {
                None => {
                    transaction.execute(
                        "UPDATE cleanup_retry_blocks SET absence_since_ms = ?1
                         WHERE incident_id = ?2",
                        params![now, incident_id],
                    )?;
                }
                Some(since) if now.saturating_sub(since) >= retention => {
                    transaction.execute(
                        "DELETE FROM cleanup_retry_blocks WHERE incident_id = ?1",
                        params![incident_id],
                    )?;
                    result.retired_count += 1;
                }
                Some(_) => {}
            }
        }
        transaction.commit()?;
        Ok(result)
    }

    pub fn explain(&self, incident_id: &str) -> Result<Option<IncidentDetail>, StoreError> {
        let events = self.query_events(
            "SELECT events.id, events.attempt_id, events.incident_id,
                    events.first_occurred_at_ms, events.occurred_at_ms,
                    events.observation_count, events.kind, events.state,
                    events.payload_json, tokens.event_token
             FROM events
             JOIN public_event_tokens AS tokens ON tokens.event_id = events.id
             WHERE events.incident_id = ?1
             ORDER BY events.occurred_at_ms ASC, events.id ASC",
            params![incident_id],
        )?;
        if events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(IncidentDetail {
                incident_id: incident_id.to_owned(),
                events,
            }))
        }
    }

    pub fn prune(
        &self,
        now_unix_millis: u64,
        policy: RetentionPolicy,
    ) -> Result<PruneResult, StoreError> {
        let before = self.event_count()?;
        let cutoff = now_unix_millis.saturating_sub(policy.max_age_millis);
        let cutoff = i64::try_from(cutoff)
            .map_err(|_| StoreError::Range("retention cutoff overflowed i64".to_owned()))?;
        let cleanup_cutoff = now_unix_millis
            .saturating_sub(policy.max_age_millis.max(CLEANUP_DETAIL_RETENTION_MILLIS));
        let cleanup_cutoff = i64::try_from(cleanup_cutoff)
            .map_err(|_| StoreError::Range("cleanup retention cutoff overflowed i64".to_owned()))?;
        let max_events = i64::try_from(policy.max_events)
            .map_err(|_| StoreError::Range("retention event limit overflowed i64".to_owned()))?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM events WHERE kind = 'observation' AND occurred_at_ms < ?1",
            params![cutoff],
        )?;
        transaction.execute(
            "DELETE FROM events
             WHERE kind = 'observation'
               AND id NOT IN (
                   SELECT id FROM events WHERE kind = 'observation'
                   ORDER BY occurred_at_ms DESC, id DESC LIMIT ?1
               )",
            params![max_events],
        )?;
        transaction.execute(
            "DELETE FROM cleanup_impacts WHERE occurred_at_ms < ?1",
            params![cleanup_cutoff],
        )?;
        transaction.execute(
            "DELETE FROM events
             WHERE kind = 'cleanup' AND occurred_at_ms < ?1
               AND NOT EXISTS (
                   SELECT 1 FROM cleanup_impacts
                   WHERE cleanup_impacts.event_id = events.id
               )",
            params![cleanup_cutoff],
        )?;
        transaction.execute(
            "DELETE FROM cooling_candidates WHERE last_wall_ms < ?1",
            params![cutoff],
        )?;
        let now = sqlite_millis(now_unix_millis, "mutation receipt prune timestamp")?;
        let deleted_receipts = transaction.execute(
            "DELETE FROM ordinary_mutation_receipts WHERE retain_until_ms <= ?1",
            params![now],
        )?;
        if deleted_receipts > 0 {
            transaction.execute(
                "UPDATE mutation_authority
                 SET namespace_token = lower(hex(randomblob(16))) WHERE singleton = 1",
                [],
            )?;
        }
        transaction.execute(
            "DELETE FROM cleanup_actions
             WHERE attempt_id IN (
                 SELECT attempts.id FROM cleanup_attempts AS attempts
                 WHERE attempts.completed_at_ms IS NOT NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM events WHERE events.attempt_id = attempts.id
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM cleanup_retry_blocks AS blocks
                       WHERE blocks.source_attempt_id = attempts.id
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM cleanup_impacts AS impacts
                       WHERE impacts.attempt_id = attempts.id
                   )
             )",
            [],
        )?;
        transaction.execute(
            "DELETE FROM cleanup_artifact_actions
             WHERE attempt_id IN (
                 SELECT attempts.id FROM cleanup_attempts AS attempts
                 WHERE attempts.completed_at_ms IS NOT NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM events WHERE events.attempt_id = attempts.id
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM cleanup_retry_blocks AS blocks
                       WHERE blocks.source_attempt_id = attempts.id
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM cleanup_impacts AS impacts
                       WHERE impacts.attempt_id = attempts.id
                   )
             )",
            [],
        )?;
        transaction.execute(
            "DELETE FROM cleanup_attempts
             WHERE completed_at_ms IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM events WHERE events.attempt_id = cleanup_attempts.id
               )
               AND NOT EXISTS (
                   SELECT 1 FROM cleanup_retry_blocks AS blocks
                   WHERE blocks.source_attempt_id = cleanup_attempts.id
               )
               AND NOT EXISTS (
                   SELECT 1 FROM cleanup_impacts AS impacts
                   WHERE impacts.attempt_id = cleanup_attempts.id
               )",
            [],
        )?;
        transaction.commit()?;
        let remaining_events = self.event_count()?;
        Ok(PruneResult {
            removed_events: before.saturating_sub(remaining_events),
            remaining_events,
        })
    }

    pub fn set_pause_until(&self, pause_until_unix_millis: Option<u64>) -> Result<(), StoreError> {
        let connection = self.connection()?;
        match pause_until_unix_millis {
            Some(deadline) => {
                let deadline = i64::try_from(deadline).map_err(|_| {
                    StoreError::Range("pause deadline overflowed SQLite integer".to_owned())
                })?;
                connection.execute(
                    "INSERT INTO settings (key, integer_value) VALUES ('pause_until_ms', ?1) \
                     ON CONFLICT(key) DO UPDATE SET integer_value = excluded.integer_value",
                    params![deadline],
                )?;
            }
            None => {
                connection.execute("DELETE FROM settings WHERE key = 'pause_until_ms'", [])?;
            }
        }
        Ok(())
    }

    pub fn track_cooling(
        &self,
        report: &IncidentReport,
        clock: &CoolingClock,
        abandonment_grace_millis: u64,
        continuity_gap_millis: u64,
    ) -> Result<bool, StoreError> {
        if report.state != IncidentState::Cooling {
            return Err(StoreError::Invalid(
                "only COOLING incidents can advance abandonment grace".to_owned(),
            ));
        }
        if clock.boot_session_fingerprint.trim().is_empty()
            || clock.enforcement_epoch.trim().is_empty()
        {
            return Err(StoreError::Invalid(
                "cooling clock requires boot and enforcement epoch fingerprints".to_owned(),
            ));
        }
        let wall = sqlite_millis(clock.wall_unix_millis, "cooling wall timestamp")?;
        let continuous = sqlite_millis(clock.continuous_millis, "cooling continuous timestamp")?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT first_continuous_ms, last_continuous_ms,
                        first_wall_ms, last_wall_ms,
                        root_identity_fingerprint, member_fingerprint,
                        boot_session_fingerprint, enforcement_epoch,
                        signature_pack, signature_version
                 FROM cooling_candidates WHERE tracking_key = ?1",
                params![report.tracking_key],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .optional()?;
        let (first_continuous, first_wall) = match existing {
            Some((
                first_continuous,
                last_continuous,
                first_wall,
                last_wall,
                root_identity,
                members,
                boot_session,
                enforcement_epoch,
                signature_pack,
                signature_version,
            )) if root_identity == report.root.identity_fingerprint
                && members == report.member_fingerprint
                && boot_session == clock.boot_session_fingerprint
                && enforcement_epoch == clock.enforcement_epoch
                && signature_pack == report.signature_pack
                && signature_version == report.signature_version
                && continuous >= last_continuous
                && wall >= last_wall
                && u64::try_from(continuous - last_continuous)
                    .is_ok_and(|gap| gap <= continuity_gap_millis)
                && wall_continuity_is_plausible(
                    wall,
                    last_wall,
                    continuous,
                    last_continuous,
                    continuity_gap_millis,
                ) =>
            {
                (first_continuous, first_wall)
            }
            _ => (continuous, wall),
        };
        transaction.execute(
            "INSERT INTO cooling_candidates (
                 tracking_key, first_continuous_ms, last_continuous_ms,
                 first_wall_ms, last_wall_ms, root_identity_fingerprint,
                 member_fingerprint, boot_session_fingerprint, enforcement_epoch,
                 signature_pack, signature_version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(tracking_key) DO UPDATE SET
                 first_continuous_ms = excluded.first_continuous_ms,
                 last_continuous_ms = excluded.last_continuous_ms,
                 first_wall_ms = excluded.first_wall_ms,
                 last_wall_ms = excluded.last_wall_ms,
                 root_identity_fingerprint = excluded.root_identity_fingerprint,
                 member_fingerprint = excluded.member_fingerprint,
                 boot_session_fingerprint = excluded.boot_session_fingerprint,
                 enforcement_epoch = excluded.enforcement_epoch,
                 signature_pack = excluded.signature_pack,
                 signature_version = excluded.signature_version",
            params![
                report.tracking_key,
                first_continuous,
                continuous,
                first_wall,
                wall,
                report.root.identity_fingerprint,
                report.member_fingerprint,
                clock.boot_session_fingerprint,
                clock.enforcement_epoch,
                report.signature_pack,
                report.signature_version,
            ],
        )?;
        transaction.commit()?;
        let elapsed =
            u64::try_from(continuous.saturating_sub(first_continuous)).unwrap_or_default();
        Ok(elapsed >= abandonment_grace_millis)
    }

    pub fn retain_cooling(
        &self,
        active_tracking_keys: &BTreeSet<String>,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing = {
            let mut statement =
                transaction.prepare("SELECT tracking_key FROM cooling_candidates")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for tracking_key in existing {
            if !active_tracking_keys.contains(&tracking_key) {
                transaction.execute(
                    "DELETE FROM cooling_candidates WHERE tracking_key = ?1",
                    params![tracking_key],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn pause_until(&self) -> Result<Option<u64>, StoreError> {
        let connection = self.connection()?;
        let value = connection
            .query_row(
                "SELECT integer_value FROM settings WHERE key = 'pause_until_ms'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        value
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    StoreError::Corrupt("negative persisted pause deadline".to_owned())
                })
            })
            .transpose()
    }

    fn event_count(&self) -> Result<usize, StoreError> {
        let connection = self.connection()?;
        let count = connection.query_row("SELECT COUNT(*) FROM events", [], |row| {
            row.get::<_, i64>(0)
        })?;
        usize::try_from(count)
            .map_err(|_| StoreError::Corrupt("negative or oversized event count".to_owned()))
    }

    fn query_cleanup_impacts(
        &self,
        predicate: &str,
        limit: usize,
    ) -> Result<Vec<CleanupImpact>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit)
            .map_err(|_| StoreError::Range("cleanup impact limit overflowed i64".to_owned()))?;
        let connection = self.connection()?;
        let sql = format!(
            "SELECT tokens.event_token, impacts.incident_id, impacts.family,
                    impacts.family_version, impacts.occurred_at_ms, impacts.state,
                    impacts.process_outcome, impacts.artifact_outcome,
                    impacts.overall_outcome, impacts.process_count,
                    impacts.estimated_reclaimed_memory_bytes,
                    impacts.revival_checks_completed
             FROM cleanup_impacts AS impacts
             JOIN public_event_tokens AS tokens ON tokens.event_id = impacts.event_id
             {predicate}
             ORDER BY impacts.occurred_at_ms DESC, impacts.id DESC LIMIT ?1"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<i64>>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })?;
        let mut impacts = Vec::new();
        for row in rows {
            let (
                event_token,
                incident_id,
                family,
                family_version,
                occurred_at,
                state,
                process_outcome,
                artifact_outcome,
                overall_outcome,
                process_count,
                estimated_reclaimed_memory_bytes,
                revival_checks_completed,
            ) = row?;
            if event_token.len() != 32 || !event_token.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(StoreError::Corrupt(
                    "cleanup impact has an invalid public event token".to_owned(),
                ));
            }
            impacts.push(CleanupImpact {
                event_token,
                incident_id,
                family,
                family_version,
                occurred_at_unix_millis: u64::try_from(occurred_at).map_err(|_| {
                    StoreError::Corrupt("negative cleanup impact timestamp".to_owned())
                })?,
                state: parse_state(&state)?,
                outcome: CleanupOutcome {
                    process: parse_process_outcome(&process_outcome)?,
                    artifact: parse_artifact_outcome(&artifact_outcome)?,
                    overall: parse_overall_outcome(&overall_outcome)?,
                    attention_required: overall_outcome != "cleared",
                },
                process_count: process_count
                    .map(|value| {
                        usize::try_from(value).map_err(|_| {
                            StoreError::Corrupt("invalid cleanup impact process count".to_owned())
                        })
                    })
                    .transpose()?,
                estimated_reclaimed_memory_bytes: estimated_reclaimed_memory_bytes
                    .map(|value| {
                        u64::try_from(value).map_err(|_| {
                            StoreError::Corrupt("invalid cleanup impact memory value".to_owned())
                        })
                    })
                    .transpose()?,
                revival_checks_completed: usize::try_from(revival_checks_completed).map_err(
                    |_| StoreError::Corrupt("invalid cleanup impact revival count".to_owned()),
                )?,
            });
        }
        Ok(impacts)
    }

    fn query_events<P: rusqlite::Params>(
        &self,
        sql: &str,
        parameters: P,
    ) -> Result<Vec<HistoryEvent>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(sql)?;
        let raw_rows = statement.query_map(parameters, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in raw_rows {
            let (
                event_id,
                attempt_id,
                incident_id,
                first_occurred_at,
                occurred_at,
                observation_count,
                kind,
                state,
                payload_json,
                event_token,
            ) = row?;
            if event_token.len() != 32 || !event_token.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(StoreError::Corrupt(format!(
                    "event {event_id} has an invalid public token"
                )));
            }
            let occurred_at_unix_millis = u64::try_from(occurred_at)
                .map_err(|_| StoreError::Corrupt("negative event timestamp".to_owned()))?;
            let first_occurred_at_unix_millis = u64::try_from(first_occurred_at)
                .map_err(|_| StoreError::Corrupt("negative event start timestamp".to_owned()))?;
            if first_occurred_at_unix_millis > occurred_at_unix_millis {
                return Err(StoreError::Corrupt(format!(
                    "event {event_id} starts after its latest observation"
                )));
            }
            let observation_count = usize::try_from(observation_count)
                .map_err(|_| StoreError::Corrupt("invalid event observation count".to_owned()))?;
            if observation_count == 0 {
                return Err(StoreError::Corrupt(format!(
                    "event {event_id} has an empty observation span"
                )));
            }
            let kind = EventKind::parse(&kind)?;
            let state = parse_state(&state)?;
            let payload = serde_json::from_str::<EventPayload>(&payload_json)?;
            if payload_kind(&payload) != kind || payload_state(&payload) != state {
                return Err(StoreError::Corrupt(format!(
                    "event {event_id} index columns disagree with payload"
                )));
            }
            events.push(HistoryEvent {
                event_id,
                event_token,
                attempt_id,
                incident_id,
                first_occurred_at_unix_millis,
                occurred_at_unix_millis,
                observation_count,
                kind,
                state,
                payload,
            });
        }
        Ok(events)
    }

    fn connection(&self) -> Result<Connection, StoreError> {
        if !validate_existing_store_components(&self.path)? {
            return Err(StoreError::UnsafePath(
                "history database disappeared after startup".to_owned(),
            ));
        }
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(connection)
    }
}

fn initialize_store_file(path: &Path) -> Result<(), StoreError> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    initialize_schema(&mut connection)?;
    run_quick_check(&connection)?;
    validate_required_schema(&connection)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    drop(connection);
    secure_store_component_permissions(path)?;
    Ok(())
}

fn preflight_store_file(path: &Path) -> Result<bool, StoreError> {
    let existed = validate_existing_store_components(path)?;
    if !existed {
        return Ok(false);
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    run_quick_check(&connection)?;
    let user_version =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    if user_version > SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(format!(
            "database schema version {user_version} is newer than supported {SCHEMA_VERSION}"
        )));
    }
    if user_version == SCHEMA_VERSION {
        validate_required_schema(&connection)?;
    }
    Ok(true)
}

fn validate_existing_store_components(path: &Path) -> Result<bool, StoreError> {
    let main = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_store_component(path, &metadata, "database")?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(StoreError::Io(error)),
    };
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = sqlite_sidecar_path(path, suffix);
        match fs::symlink_metadata(&sidecar) {
            Ok(metadata) => {
                if !main {
                    return Err(StoreError::UnsafePath(format!(
                        "SQLite {suffix} sidecar exists without its database"
                    )));
                }
                validate_store_component(&sidecar, &metadata, "SQLite sidecar")?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(StoreError::Io(error)),
        }
    }
    Ok(main)
}

fn validate_store_component(
    _path: &Path,
    metadata: &fs::Metadata,
    label: &str,
) -> Result<(), StoreError> {
    if !metadata.file_type().is_file() {
        return Err(StoreError::UnsafePath(format!(
            "{label} must be a regular file and must not be a symlink"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(StoreError::UnsafePath(format!(
                "{label} is not owned by the current user"
            )));
        }
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(StoreError::UnsafePath(format!(
                "{label} permissions expose private state"
            )));
        }
    }
    Ok(())
}

fn secure_store_component_permissions(path: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        for component in [
            path.to_path_buf(),
            sqlite_sidecar_path(path, "-wal"),
            sqlite_sidecar_path(path, "-shm"),
            sqlite_sidecar_path(path, "-journal"),
        ] {
            match fs::symlink_metadata(&component) {
                Ok(metadata) => {
                    if !metadata.file_type().is_file()
                        || metadata.uid() != unsafe { libc::geteuid() }
                    {
                        return Err(StoreError::UnsafePath(
                            "new SQLite component is not a current-user regular file".to_owned(),
                        ));
                    }
                    fs::set_permissions(&component, fs::Permissions::from_mode(0o600))?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(StoreError::Io(error)),
            }
        }
    }
    Ok(())
}

fn run_quick_check(connection: &Connection) -> Result<(), StoreError> {
    let mut statement = connection.prepare("PRAGMA quick_check")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut saw_ok = false;
    for row in rows {
        let result = row?;
        if result == "ok" {
            saw_ok = true;
        } else {
            return Err(StoreError::Corrupt(
                "SQLite quick_check reported an integrity failure".to_owned(),
            ));
        }
    }
    if !saw_ok {
        return Err(StoreError::Corrupt(
            "SQLite quick_check returned no result".to_owned(),
        ));
    }
    Ok(())
}

fn validate_required_schema(connection: &Connection) -> Result<(), StoreError> {
    let user_version =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    if user_version != SCHEMA_VERSION {
        return Err(StoreError::Corrupt(format!(
            "required schema validation expected version {SCHEMA_VERSION}, found {user_version}"
        )));
    }
    const REQUIRED: &[(&str, &[&str])] = &[
        (
            "cleanup_attempts",
            &[
                "id",
                "incident_id",
                "tracking_key",
                "root_identity_fingerprint",
                "member_fingerprint",
                "enforcement_epoch",
                "started_at_ms",
                "completed_at_ms",
                "terminal_state",
                "reason_id",
                "survivor_pids_json",
                "revival_checks_completed",
                "resources_json",
                "signature_pack",
                "signature_version",
                "observed_process_count",
                "observed_resident_memory_bytes",
            ],
        ),
        (
            "cleanup_actions",
            &[
                "id",
                "attempt_id",
                "sequence",
                "prepared_at_ms",
                "completed_at_ms",
                "stage",
                "pid",
                "identity_fingerprint",
                "signal",
                "disposition",
            ],
        ),
        (
            "cleanup_artifact_actions",
            &[
                "id",
                "attempt_id",
                "sequence",
                "prepared_at_ms",
                "completed_at_ms",
                "kind",
                "artifact_fingerprint",
                "disposition",
            ],
        ),
        (
            "cleanup_retry_blocks",
            &[
                "incident_id",
                "tracking_key",
                "blocked_at_ms",
                "reason_id",
                "source_attempt_id",
                "last_exact_observed_at_ms",
                "absence_since_ms",
            ],
        ),
        (
            "events",
            &[
                "id",
                "incident_id",
                "first_occurred_at_ms",
                "occurred_at_ms",
                "observation_count",
                "kind",
                "state",
                "payload_json",
                "attempt_id",
            ],
        ),
        ("settings", &["key", "integer_value"]),
        (
            "cooling_candidates",
            &[
                "tracking_key",
                "first_continuous_ms",
                "last_continuous_ms",
                "first_wall_ms",
                "last_wall_ms",
                "root_identity_fingerprint",
                "member_fingerprint",
                "boot_session_fingerprint",
                "enforcement_epoch",
                "signature_pack",
                "signature_version",
            ],
        ),
        (
            "managed_lifecycle",
            &[
                "singleton",
                "activation_generation",
                "instance_id",
                "requested_enforce",
                "effective_enforce",
                "armed_generation",
                "enforcement_epoch",
                "ready",
                "draining",
                "startup_phase",
                "updated_at_ms",
            ],
        ),
        (
            "storage_recoveries",
            &[
                "id",
                "recovery_id",
                "public_token",
                "occurred_at_ms",
                "reason_id",
                "quarantine_directory_name",
                "quarantined_sidecar_count",
            ],
        ),
        (
            "incident_protections",
            &[
                "incident_id",
                "root_identity_fingerprint",
                "member_fingerprint",
                "protected_at_ms",
                "last_exact_observed_at_ms",
                "absence_since_ms",
            ],
        ),
        ("public_event_tokens", &["event_id", "event_token"]),
        (
            "cleanup_impacts",
            &[
                "id",
                "attempt_id",
                "event_id",
                "incident_id",
                "family",
                "family_version",
                "occurred_at_ms",
                "state",
                "process_outcome",
                "artifact_outcome",
                "overall_outcome",
                "process_count",
                "estimated_reclaimed_memory_bytes",
                "revival_checks_completed",
            ],
        ),
        (
            "impact_authority",
            &[
                "singleton",
                "tracking_started_at_ms",
                "historical_completeness",
                "terminal_cleanup_count",
                "proved_reclaim_count",
                "reclaimed_process_count",
                "reclaimed_process_measurement_count",
                "estimated_reclaimed_memory_bytes",
                "reclaimed_memory_measurement_count",
            ],
        ),
        (
            "storage_residue_latest",
            &["kind", "observed_at_ms", "payload_json"],
        ),
        ("mutation_authority", &["singleton", "namespace_token"]),
        (
            "control_metadata",
            &["singleton", "cleanup_policy_revision"],
        ),
        (
            "ordinary_mutation_receipts",
            &[
                "namespace_token",
                "mutation_id",
                "canonical_version",
                "command_json",
                "receipt_json",
                "committed_at_ms",
                "retain_until_ms",
            ],
        ),
    ];
    for (table, required_columns) in REQUIRED {
        let exists = connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(StoreError::Corrupt(format!(
                "required schema table {table:?} is missing"
            )));
        }
        let pragma = format!("PRAGMA table_info({table})");
        let mut statement = connection.prepare(&pragma)?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<BTreeSet<_>, _>>()?;
        for column in *required_columns {
            if !columns.contains(*column) {
                return Err(StoreError::Corrupt(format!(
                    "required schema column {table}.{column} is missing"
                )));
            }
        }
    }
    Ok(())
}

fn is_recoverable_corruption(error: &StoreError) -> bool {
    match error {
        StoreError::Corrupt(_) => true,
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(failure, _)) => matches!(
            failure.code,
            rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
        ),
        _ => false,
    }
}

fn recovery_reason(error: &StoreError) -> StorageRecoveryReason {
    match error {
        StoreError::Corrupt(message)
            if message.contains("required schema") || message.contains("application tables") =>
        {
            StorageRecoveryReason::RequiredSchemaInvalid
        }
        _ => StorageRecoveryReason::IntegrityCheckFailed,
    }
}

fn quarantine_database(
    path: &Path,
    reason: StorageRecoveryReason,
) -> Result<StorageRecoveryOccurrence, StoreError> {
    if !validate_existing_store_components(path)? {
        return Err(StoreError::UnsafePath(
            "corrupt database disappeared before quarantine".to_owned(),
        ));
    }
    let occurred_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::Invalid("system clock is before Unix epoch".to_owned()))?
        .as_millis()
        .try_into()
        .map_err(|_| StoreError::Range("storage recovery timestamp overflowed u64".to_owned()))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let basename = path
        .file_name()
        .ok_or_else(|| StoreError::Invalid("history database path has no filename".to_owned()))?;
    let mut selected = None;
    for sequence in 0_u16..=u16::MAX {
        let mut name = basename.to_os_string();
        name.push(format!(".quarantine-{occurred_at_unix_millis}-{sequence}"));
        let candidate = parent.join(name);
        match fs::create_dir(&candidate) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700))?;
                }
                selected = Some((sequence, candidate));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(StoreError::Io(error)),
        }
    }
    let (sequence, quarantine_directory) = selected.ok_or_else(|| {
        StoreError::Invalid("could not allocate a unique storage quarantine directory".to_owned())
    })?;

    fs::rename(path, quarantine_directory.join(basename))?;
    let mut quarantined_sidecar_count = 0_usize;
    for suffix in ["-wal", "-shm", "-journal"] {
        let source = sqlite_sidecar_path(path, suffix);
        let destination = quarantine_directory.join(
            source
                .file_name()
                .ok_or_else(|| StoreError::Invalid("SQLite sidecar has no filename".to_owned()))?,
        );
        match fs::rename(&source, destination) {
            Ok(()) => quarantined_sidecar_count += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(StoreError::Io(error)),
        }
    }
    sync_directory(&quarantine_directory)?;
    sync_directory(parent)?;
    Ok(StorageRecoveryOccurrence {
        recovery_id: format!("storage-recovery-{occurred_at_unix_millis}-{sequence}"),
        public_token: String::new(),
        occurred_at_unix_millis,
        reason,
        quarantine_directory,
        quarantined_sidecar_count,
    })
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn sync_directory(path: &Path) -> Result<(), StoreError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn initialize_schema(connection: &mut Connection) -> Result<(), StoreError> {
    let user_version =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    if user_version > SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(format!(
            "database schema version {user_version} is newer than supported {SCHEMA_VERSION}"
        )));
    }
    if user_version == SCHEMA_VERSION {
        return Ok(());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if user_version == 0 {
        let has_application_tables = transaction
            .query_row(
                "SELECT 1 FROM sqlite_master
                 WHERE type = 'table' AND name IN ('events', 'settings', 'cooling_candidates')
                 LIMIT 1",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if has_application_tables {
            return Err(StoreError::Corrupt(
                "unversioned database contains Unlinger application tables".to_owned(),
            ));
        }
        transaction.execute_batch(SCHEMA_SQL)?;
    } else {
        if user_version < 3 {
            transaction.execute_batch(MIGRATION_V3_FOUNDATIONS_SQL)?;
            if !events_have_attempt_id(&transaction)? {
                transaction.execute_batch(
                    "ALTER TABLE events ADD COLUMN attempt_id INTEGER
                         REFERENCES cleanup_attempts(id);",
                )?;
            }
            transaction.execute_batch(
                "DROP TABLE IF EXISTS cooling_candidates;
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
                 CREATE INDEX IF NOT EXISTS events_incident_timeline
                     ON events (incident_id, occurred_at_ms, id);
                 CREATE INDEX IF NOT EXISTS events_recent
                     ON events (occurred_at_ms DESC, id DESC);
                 CREATE INDEX IF NOT EXISTS events_attempt_timeline
                     ON events (attempt_id, id);",
            )?;
        }
        if user_version < 4 {
            transaction.execute_batch(MANAGED_LIFECYCLE_SQL)?;
        }
        if user_version < 5 {
            transaction.execute_batch(MIGRATION_V5_SQL)?;
        }
        if user_version < 6 {
            let has_events = transaction
                .query_row(
                    "SELECT 1 FROM sqlite_master
                     WHERE type = 'table' AND name = 'events'",
                    [],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !has_events {
                return Err(StoreError::Corrupt(
                    "schema-v6 migration requires the events table".to_owned(),
                ));
            }
            transaction.execute_batch(MIGRATION_V6_SQL)?;
        }
        if user_version < 7 {
            add_v7_columns(&transaction)?;
            transaction.execute_batch(MIGRATION_V7_SQL)?;
            migrate_v7_history_and_impacts(&transaction)?;
        }
    }
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

fn add_v7_columns(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    if !table_has_column(transaction, "events", "first_occurred_at_ms")? {
        transaction.execute_batch(
            "ALTER TABLE events
                 ADD COLUMN first_occurred_at_ms INTEGER NOT NULL DEFAULT 0
                     CHECK (first_occurred_at_ms >= 0);",
        )?;
    }
    if !table_has_column(transaction, "events", "observation_count")? {
        transaction.execute_batch(
            "ALTER TABLE events
                 ADD COLUMN observation_count INTEGER NOT NULL DEFAULT 1
                     CHECK (observation_count >= 1);",
        )?;
    }
    transaction.execute(
        "UPDATE events SET first_occurred_at_ms = occurred_at_ms
         WHERE first_occurred_at_ms = 0",
        [],
    )?;
    for (column, definition) in [
        ("signature_pack", "TEXT"),
        ("signature_version", "TEXT"),
        (
            "observed_process_count",
            "INTEGER CHECK (observed_process_count >= 0)",
        ),
        (
            "observed_resident_memory_bytes",
            "INTEGER CHECK (observed_resident_memory_bytes >= 0)",
        ),
    ] {
        if !table_has_column(transaction, "cleanup_attempts", column)? {
            transaction.execute_batch(&format!(
                "ALTER TABLE cleanup_attempts ADD COLUMN {column} {definition};"
            ))?;
        }
    }
    Ok(())
}

fn table_has_column(
    transaction: &Transaction<'_>,
    table: &str,
    column: &str,
) -> Result<bool, StoreError> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut statement = transaction.prepare(&pragma)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for stored in rows {
        if stored? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_v7_history_and_impacts(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    collapse_legacy_observation_spans(transaction)?;
    let terminal_events = {
        let mut statement = transaction.prepare(
            "SELECT events.id, events.attempt_id, events.occurred_at_ms,
                    events.payload_json
             FROM events
             WHERE events.kind = 'cleanup'
               AND events.attempt_id IS NOT NULL
               AND events.state IN ('CLEARED', 'FAILED', 'REVIVED')
             ORDER BY events.id ASC",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (event_id, attempt_id, occurred_at, payload_json) in terminal_events {
        if let Some(report) = latest_observation_before_event(transaction, attempt_id, event_id)? {
            transaction.execute(
                "UPDATE cleanup_attempts
                 SET signature_pack = ?1, signature_version = ?2,
                     observed_process_count = ?3,
                     observed_resident_memory_bytes = ?4
                 WHERE id = ?5",
                params![
                    report.signature_pack,
                    report.signature_version,
                    i64::try_from(report.member_count).map_err(|_| {
                        StoreError::Range("legacy impact process count overflowed i64".to_owned())
                    })?,
                    sqlite_u64(
                        report.resident_memory_bytes,
                        "legacy impact resident memory",
                    )?,
                    attempt_id,
                ],
            )?;
        }
        let EventPayload::Cleanup { receipt } = serde_json::from_str(&payload_json)? else {
            return Err(StoreError::Corrupt(format!(
                "terminal event {event_id} contains a non-cleanup payload"
            )));
        };
        insert_cleanup_impact_transaction(
            transaction,
            attempt_id,
            event_id,
            u64::try_from(occurred_at)
                .map_err(|_| StoreError::Corrupt("negative legacy impact time".to_owned()))?,
            &receipt,
        )?;
    }
    transaction.execute(
        "UPDATE impact_authority
         SET tracking_started_at_ms = ?1,
             historical_completeness = 'partial_backfill'
         WHERE singleton = 1",
        params![sqlite_millis(
            current_unix_millis()?,
            "impact migration timestamp"
        )?],
    )?;
    Ok(())
}

fn collapse_legacy_observation_spans(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    let rows = {
        let mut statement = transaction.prepare(
            "SELECT id, incident_id, occurred_at_ms, kind, payload_json
             FROM events ORDER BY incident_id ASC, occurred_at_ms ASC, id ASC",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut previous: Option<(i64, String, i64, ObservationRecord, i64)> = None;
    for (event_id, incident_id, occurred_at, kind, payload_json) in rows {
        if kind != EventKind::Observation.as_str() {
            previous = None;
            continue;
        }
        let EventPayload::Observation { report } = serde_json::from_str(&payload_json)? else {
            return Err(StoreError::Corrupt(format!(
                "legacy observation event {event_id} contains a non-observation payload"
            )));
        };
        if let Some((previous_id, previous_incident, first_at, previous_report, count)) =
            previous.take()
            && previous_incident == incident_id
            && observations_semantically_equal(&previous_report, &report)
        {
            let next_count = count.checked_add(1).ok_or_else(|| {
                StoreError::Range("legacy observation span count overflowed i64".to_owned())
            })?;
            transaction.execute(
                "UPDATE events
                 SET first_occurred_at_ms = ?1, observation_count = ?2
                 WHERE id = ?3",
                params![first_at, next_count, event_id],
            )?;
            transaction.execute("DELETE FROM events WHERE id = ?1", params![previous_id])?;
            previous = Some((event_id, incident_id, first_at, report, next_count));
        } else {
            previous = Some((event_id, incident_id, occurred_at, report, 1));
        }
    }
    Ok(())
}

fn latest_observation_before_event(
    transaction: &Transaction<'_>,
    attempt_id: i64,
    event_id: i64,
) -> Result<Option<ObservationRecord>, StoreError> {
    let incident_id = transaction.query_row(
        "SELECT incident_id FROM cleanup_attempts WHERE id = ?1",
        params![attempt_id],
        |row| row.get::<_, String>(0),
    )?;
    let payload_json = transaction
        .query_row(
            "SELECT payload_json FROM events
             WHERE incident_id = ?1 AND kind = 'observation' AND id < ?2
             ORDER BY id DESC LIMIT 1",
            params![incident_id, event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    payload_json
        .map(
            |payload_json| match serde_json::from_str::<EventPayload>(&payload_json)? {
                EventPayload::Observation { report } => Ok(report),
                EventPayload::Cleanup { .. } => Err(StoreError::Corrupt(
                    "legacy observation lookup selected a cleanup payload".to_owned(),
                )),
            },
        )
        .transpose()
}

const MIGRATION_V5_SQL: &str = "DELETE FROM cooling_candidates;
     ALTER TABLE cleanup_attempts
         ADD COLUMN resources_json TEXT;
     ALTER TABLE cooling_candidates
         ADD COLUMN signature_pack TEXT NOT NULL DEFAULT '';
     ALTER TABLE cooling_candidates
         ADD COLUMN signature_version TEXT NOT NULL DEFAULT '';
     ALTER TABLE cleanup_retry_blocks
         ADD COLUMN last_exact_observed_at_ms INTEGER
             CHECK (last_exact_observed_at_ms >= blocked_at_ms);
     ALTER TABLE cleanup_retry_blocks
         ADD COLUMN absence_since_ms INTEGER
             CHECK (absence_since_ms >= blocked_at_ms);
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
     CREATE INDEX storage_recoveries_recent
         ON storage_recoveries (occurred_at_ms DESC, id DESC);
     CREATE TABLE incident_protections (
         incident_id TEXT PRIMARY KEY,
         root_identity_fingerprint TEXT NOT NULL,
         member_fingerprint TEXT NOT NULL,
         protected_at_ms INTEGER NOT NULL CHECK (protected_at_ms >= 0),
         last_exact_observed_at_ms INTEGER
             CHECK (last_exact_observed_at_ms >= protected_at_ms),
         absence_since_ms INTEGER CHECK (absence_since_ms >= protected_at_ms)
     );
     CREATE INDEX incident_protections_recent
         ON incident_protections (protected_at_ms DESC, incident_id ASC);
     CREATE TABLE cleanup_artifact_actions (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         attempt_id INTEGER NOT NULL REFERENCES cleanup_attempts(id),
         sequence INTEGER NOT NULL CHECK (sequence >= 0),
         prepared_at_ms INTEGER NOT NULL CHECK (prepared_at_ms >= 0),
         completed_at_ms INTEGER CHECK (completed_at_ms >= prepared_at_ms),
         kind TEXT NOT NULL CHECK (kind IN ('dev_tools_active_port')),
         artifact_fingerprint TEXT NOT NULL,
         disposition TEXT CHECK (
             disposition IN (
                 'removed', 'already_absent', 'identity_mismatch', 'referenced',
                 'unsafe', 'rejected', 'cancelled_before_delivery', 'delivery_unknown'
             )
         ),
         UNIQUE (attempt_id, sequence),
         CHECK (
             (completed_at_ms IS NULL AND disposition IS NULL)
             OR (completed_at_ms IS NOT NULL AND disposition IS NOT NULL)
         )
     );
     CREATE INDEX cleanup_artifact_actions_attempt_sequence
         ON cleanup_artifact_actions (attempt_id, sequence);";

const MIGRATION_V6_SQL: &str = "CREATE TABLE public_event_tokens (
         event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,
         event_token TEXT NOT NULL UNIQUE CHECK (length(event_token) = 32)
     );
     INSERT INTO public_event_tokens (event_id, event_token)
         SELECT id, lower(hex(randomblob(16))) FROM events;
     ALTER TABLE storage_recoveries ADD COLUMN public_token TEXT;
     UPDATE storage_recoveries
         SET public_token = lower(hex(randomblob(16)))
         WHERE public_token IS NULL;
     CREATE UNIQUE INDEX storage_recoveries_public_token
         ON storage_recoveries (public_token);
     CREATE TABLE mutation_authority (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         namespace_token TEXT NOT NULL UNIQUE CHECK (length(namespace_token) = 32)
     );
     INSERT INTO mutation_authority (singleton, namespace_token)
         VALUES (1, lower(hex(randomblob(16))));
     CREATE TABLE control_metadata (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         cleanup_policy_revision INTEGER NOT NULL
             CHECK (cleanup_policy_revision >= 1)
     );
     INSERT INTO control_metadata (singleton, cleanup_policy_revision)
         VALUES (1, 1);
     CREATE TABLE ordinary_mutation_receipts (
         namespace_token TEXT NOT NULL CHECK (length(namespace_token) = 32),
         mutation_id TEXT NOT NULL CHECK (length(mutation_id) = 36),
         canonical_version INTEGER NOT NULL CHECK (canonical_version = 1),
         command_json TEXT NOT NULL,
         receipt_json TEXT NOT NULL,
         committed_at_ms INTEGER NOT NULL CHECK (committed_at_ms >= 0),
         retain_until_ms INTEGER NOT NULL CHECK (retain_until_ms >= committed_at_ms),
         PRIMARY KEY (namespace_token, mutation_id)
     );
     CREATE INDEX ordinary_mutation_receipts_recent
         ON ordinary_mutation_receipts (committed_at_ms DESC, namespace_token, mutation_id);";

const MIGRATION_V7_SQL: &str = "CREATE TABLE cleanup_impacts (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         attempt_id INTEGER NOT NULL UNIQUE REFERENCES cleanup_attempts(id),
         event_id INTEGER NOT NULL UNIQUE REFERENCES events(id),
         incident_id TEXT NOT NULL,
         family TEXT NOT NULL,
         family_version TEXT NOT NULL,
         occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
         state TEXT NOT NULL CHECK (state IN ('CLEARED', 'FAILED', 'REVIVED')),
         process_outcome TEXT NOT NULL CHECK (
             process_outcome IN ('cleared', 'revived', 'failed', 'delivery_unknown')
         ),
         artifact_outcome TEXT NOT NULL CHECK (
             artifact_outcome IN ('not_applicable', 'reconciled', 'residue', 'delivery_unknown')
         ),
         overall_outcome TEXT NOT NULL CHECK (
             overall_outcome IN ('cleared', 'cleared_with_residue', 'revived', 'failed')
         ),
         process_count INTEGER CHECK (process_count >= 0),
         estimated_reclaimed_memory_bytes INTEGER
             CHECK (estimated_reclaimed_memory_bytes >= 0),
         revival_checks_completed INTEGER NOT NULL
             CHECK (revival_checks_completed >= 0)
     );
     CREATE INDEX cleanup_impacts_recent
         ON cleanup_impacts (occurred_at_ms DESC, id DESC);
     CREATE TABLE storage_residue_latest (
         kind TEXT PRIMARY KEY CHECK (kind IN ('chrome_code_sign_clone')),
         observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
         payload_json TEXT NOT NULL
     );
     CREATE TABLE impact_authority (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         tracking_started_at_ms INTEGER NOT NULL CHECK (tracking_started_at_ms >= 0),
         historical_completeness TEXT NOT NULL CHECK (
             historical_completeness IN ('complete', 'partial_backfill')
         ),
         terminal_cleanup_count INTEGER NOT NULL CHECK (terminal_cleanup_count >= 0),
         proved_reclaim_count INTEGER NOT NULL CHECK (proved_reclaim_count >= 0),
         reclaimed_process_count INTEGER NOT NULL CHECK (reclaimed_process_count >= 0),
         reclaimed_process_measurement_count INTEGER NOT NULL
             CHECK (reclaimed_process_measurement_count >= 0),
         estimated_reclaimed_memory_bytes INTEGER NOT NULL
             CHECK (estimated_reclaimed_memory_bytes >= 0),
         reclaimed_memory_measurement_count INTEGER NOT NULL
             CHECK (reclaimed_memory_measurement_count >= 0)
     );
     INSERT INTO impact_authority (
         singleton, tracking_started_at_ms, historical_completeness,
         terminal_cleanup_count, proved_reclaim_count,
         reclaimed_process_count, reclaimed_process_measurement_count,
         estimated_reclaimed_memory_bytes, reclaimed_memory_measurement_count
     ) VALUES (1, 0, 'partial_backfill', 0, 0, 0, 0, 0, 0);";

const MIGRATION_V3_FOUNDATIONS_SQL: &str = "CREATE TABLE IF NOT EXISTS cleanup_attempts (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         incident_id TEXT NOT NULL,
         tracking_key TEXT NOT NULL,
         root_identity_fingerprint TEXT NOT NULL,
         member_fingerprint TEXT NOT NULL,
         enforcement_epoch TEXT NOT NULL,
         started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
         completed_at_ms INTEGER CHECK (completed_at_ms >= started_at_ms),
         terminal_state TEXT CHECK (terminal_state IN ('CLEARED', 'FAILED', 'REVIVED')),
         reason_id TEXT,
         survivor_pids_json TEXT,
         revival_checks_completed INTEGER NOT NULL DEFAULT 0
             CHECK (revival_checks_completed >= 0),
         CHECK (
             (completed_at_ms IS NULL AND terminal_state IS NULL)
             OR (completed_at_ms IS NOT NULL AND terminal_state IS NOT NULL)
         )
     );
     CREATE UNIQUE INDEX IF NOT EXISTS cleanup_attempts_one_open
         ON cleanup_attempts ((1)) WHERE completed_at_ms IS NULL;
     CREATE INDEX IF NOT EXISTS cleanup_attempts_incident
         ON cleanup_attempts (incident_id, id DESC);
     CREATE TABLE IF NOT EXISTS cleanup_actions (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         attempt_id INTEGER NOT NULL REFERENCES cleanup_attempts(id),
         sequence INTEGER NOT NULL CHECK (sequence >= 0),
         prepared_at_ms INTEGER NOT NULL CHECK (prepared_at_ms >= 0),
         completed_at_ms INTEGER CHECK (completed_at_ms >= prepared_at_ms),
         stage TEXT NOT NULL CHECK (stage IN ('primary_term', 'member_term', 'exact_kill')),
         pid INTEGER NOT NULL CHECK (pid > 0),
         identity_fingerprint TEXT NOT NULL,
         signal TEXT NOT NULL CHECK (signal IN ('term', 'kill')),
         disposition TEXT CHECK (
             disposition IN (
                 'delivered', 'already_exited', 'identity_mismatch', 'rejected',
                 'cancelled_before_delivery', 'delivery_unknown'
             )
         ),
         UNIQUE (attempt_id, sequence),
         CHECK (
             (completed_at_ms IS NULL AND disposition IS NULL)
             OR (completed_at_ms IS NOT NULL AND disposition IS NOT NULL)
         )
     );
     CREATE INDEX IF NOT EXISTS cleanup_actions_attempt_sequence
         ON cleanup_actions (attempt_id, sequence);
     CREATE TABLE IF NOT EXISTS cleanup_retry_blocks (
         incident_id TEXT PRIMARY KEY,
         tracking_key TEXT NOT NULL,
         blocked_at_ms INTEGER NOT NULL CHECK (blocked_at_ms >= 0),
         reason_id TEXT,
         source_attempt_id INTEGER NOT NULL REFERENCES cleanup_attempts(id)
     );
     CREATE TABLE IF NOT EXISTS settings (
         key TEXT PRIMARY KEY,
         integer_value INTEGER
     );
     CREATE TABLE IF NOT EXISTS events (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         incident_id TEXT NOT NULL,
         occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
         kind TEXT NOT NULL CHECK (kind IN ('observation', 'cleanup')),
         state TEXT NOT NULL,
         payload_json TEXT NOT NULL,
         attempt_id INTEGER REFERENCES cleanup_attempts(id)
     );";

const SCHEMA_SQL: &str = "CREATE TABLE cleanup_attempts (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         incident_id TEXT NOT NULL,
         tracking_key TEXT NOT NULL,
         root_identity_fingerprint TEXT NOT NULL,
         member_fingerprint TEXT NOT NULL,
         enforcement_epoch TEXT NOT NULL,
         started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
         completed_at_ms INTEGER CHECK (completed_at_ms >= started_at_ms),
         terminal_state TEXT CHECK (terminal_state IN ('CLEARED', 'FAILED', 'REVIVED')),
         reason_id TEXT,
         survivor_pids_json TEXT,
         revival_checks_completed INTEGER NOT NULL DEFAULT 0
             CHECK (revival_checks_completed >= 0),
         resources_json TEXT,
         signature_pack TEXT,
         signature_version TEXT,
         observed_process_count INTEGER CHECK (observed_process_count >= 0),
         observed_resident_memory_bytes INTEGER
             CHECK (observed_resident_memory_bytes >= 0),
         CHECK (
             (completed_at_ms IS NULL AND terminal_state IS NULL)
             OR (completed_at_ms IS NOT NULL AND terminal_state IS NOT NULL)
         )
     );
     CREATE UNIQUE INDEX cleanup_attempts_one_open
         ON cleanup_attempts ((1)) WHERE completed_at_ms IS NULL;
     CREATE INDEX cleanup_attempts_incident
         ON cleanup_attempts (incident_id, id DESC);
     CREATE TABLE cleanup_actions (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         attempt_id INTEGER NOT NULL REFERENCES cleanup_attempts(id),
         sequence INTEGER NOT NULL CHECK (sequence >= 0),
         prepared_at_ms INTEGER NOT NULL CHECK (prepared_at_ms >= 0),
         completed_at_ms INTEGER CHECK (completed_at_ms >= prepared_at_ms),
         stage TEXT NOT NULL CHECK (stage IN ('primary_term', 'member_term', 'exact_kill')),
         pid INTEGER NOT NULL CHECK (pid > 0),
         identity_fingerprint TEXT NOT NULL,
         signal TEXT NOT NULL CHECK (signal IN ('term', 'kill')),
         disposition TEXT CHECK (
             disposition IN (
                 'delivered', 'already_exited', 'identity_mismatch', 'rejected',
                 'cancelled_before_delivery', 'delivery_unknown'
             )
         ),
         UNIQUE (attempt_id, sequence),
         CHECK (
             (completed_at_ms IS NULL AND disposition IS NULL)
             OR (completed_at_ms IS NOT NULL AND disposition IS NOT NULL)
         )
     );
     CREATE INDEX cleanup_actions_attempt_sequence
         ON cleanup_actions (attempt_id, sequence);
     CREATE TABLE cleanup_artifact_actions (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         attempt_id INTEGER NOT NULL REFERENCES cleanup_attempts(id),
         sequence INTEGER NOT NULL CHECK (sequence >= 0),
         prepared_at_ms INTEGER NOT NULL CHECK (prepared_at_ms >= 0),
         completed_at_ms INTEGER CHECK (completed_at_ms >= prepared_at_ms),
         kind TEXT NOT NULL CHECK (kind IN ('dev_tools_active_port')),
         artifact_fingerprint TEXT NOT NULL,
         disposition TEXT CHECK (
             disposition IN (
                 'removed', 'already_absent', 'identity_mismatch', 'referenced',
                 'unsafe', 'rejected', 'cancelled_before_delivery', 'delivery_unknown'
             )
         ),
         UNIQUE (attempt_id, sequence),
         CHECK (
             (completed_at_ms IS NULL AND disposition IS NULL)
             OR (completed_at_ms IS NOT NULL AND disposition IS NOT NULL)
         )
     );
     CREATE INDEX cleanup_artifact_actions_attempt_sequence
         ON cleanup_artifact_actions (attempt_id, sequence);
     CREATE TABLE cleanup_retry_blocks (
         incident_id TEXT PRIMARY KEY,
         tracking_key TEXT NOT NULL,
         blocked_at_ms INTEGER NOT NULL CHECK (blocked_at_ms >= 0),
         reason_id TEXT,
         source_attempt_id INTEGER NOT NULL REFERENCES cleanup_attempts(id),
         last_exact_observed_at_ms INTEGER
             CHECK (last_exact_observed_at_ms >= blocked_at_ms),
         absence_since_ms INTEGER CHECK (absence_since_ms >= blocked_at_ms)
     );
     CREATE TABLE events (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         incident_id TEXT NOT NULL,
         first_occurred_at_ms INTEGER NOT NULL CHECK (first_occurred_at_ms >= 0),
         occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
         observation_count INTEGER NOT NULL CHECK (observation_count >= 1),
         kind TEXT NOT NULL CHECK (kind IN ('observation', 'cleanup')),
         state TEXT NOT NULL,
         payload_json TEXT NOT NULL,
         attempt_id INTEGER REFERENCES cleanup_attempts(id),
         CHECK (first_occurred_at_ms <= occurred_at_ms),
         CHECK (
             kind = 'observation'
             OR (first_occurred_at_ms = occurred_at_ms AND observation_count = 1)
         )
     );
     CREATE INDEX events_incident_timeline
         ON events (incident_id, occurred_at_ms, id);
     CREATE INDEX events_recent
         ON events (occurred_at_ms DESC, id DESC);
     CREATE INDEX events_attempt_timeline
         ON events (attempt_id, id);
     CREATE TABLE settings (
         key TEXT PRIMARY KEY,
         integer_value INTEGER
     );
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
         enforcement_epoch TEXT NOT NULL,
         signature_pack TEXT NOT NULL,
         signature_version TEXT NOT NULL
     );
     CREATE TABLE managed_lifecycle (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         activation_generation INTEGER NOT NULL CHECK (activation_generation > 0),
         instance_id TEXT NOT NULL,
         requested_enforce INTEGER NOT NULL CHECK (requested_enforce IN (0, 1)),
         effective_enforce INTEGER NOT NULL CHECK (effective_enforce IN (0, 1)),
         armed_generation INTEGER CHECK (armed_generation > 0),
         enforcement_epoch TEXT,
         ready INTEGER NOT NULL CHECK (ready IN (0, 1)),
         draining INTEGER NOT NULL CHECK (draining IN (0, 1)),
         startup_phase TEXT NOT NULL CHECK (
             startup_phase IN (
                 'recovering', 'first_scan_report_only', 'ready_report_only',
                 'ready_enforce', 'draining', 'failed'
             )
         ),
         updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
         CHECK (
             (effective_enforce = 0 AND armed_generation IS NULL AND enforcement_epoch IS NULL)
             OR (
                 effective_enforce = 1
                 AND requested_enforce = 1
                 AND armed_generation = activation_generation
                 AND enforcement_epoch IS NOT NULL
                 AND ready = 1
                 AND draining = 0
             )
         )
     );
     CREATE TABLE storage_recoveries (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         recovery_id TEXT NOT NULL UNIQUE,
         public_token TEXT NOT NULL UNIQUE CHECK (length(public_token) = 32),
         occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
         reason_id TEXT NOT NULL CHECK (
             reason_id IN ('integrity_check_failed', 'required_schema_invalid')
         ),
         quarantine_directory_name TEXT NOT NULL,
         quarantined_sidecar_count INTEGER NOT NULL
             CHECK (quarantined_sidecar_count >= 0)
     );
     CREATE INDEX storage_recoveries_recent
         ON storage_recoveries (occurred_at_ms DESC, id DESC);
     CREATE TABLE incident_protections (
         incident_id TEXT PRIMARY KEY,
         root_identity_fingerprint TEXT NOT NULL,
         member_fingerprint TEXT NOT NULL,
         protected_at_ms INTEGER NOT NULL CHECK (protected_at_ms >= 0),
         last_exact_observed_at_ms INTEGER
             CHECK (last_exact_observed_at_ms >= protected_at_ms),
         absence_since_ms INTEGER CHECK (absence_since_ms >= protected_at_ms)
     );
     CREATE INDEX incident_protections_recent
         ON incident_protections (protected_at_ms DESC, incident_id ASC);
     CREATE TABLE public_event_tokens (
         event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,
         event_token TEXT NOT NULL UNIQUE CHECK (length(event_token) = 32)
     );
     CREATE TABLE cleanup_impacts (
         id INTEGER PRIMARY KEY AUTOINCREMENT,
         attempt_id INTEGER NOT NULL UNIQUE REFERENCES cleanup_attempts(id),
         event_id INTEGER NOT NULL UNIQUE REFERENCES events(id),
         incident_id TEXT NOT NULL,
         family TEXT NOT NULL,
         family_version TEXT NOT NULL,
         occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
         state TEXT NOT NULL CHECK (state IN ('CLEARED', 'FAILED', 'REVIVED')),
         process_outcome TEXT NOT NULL CHECK (
             process_outcome IN ('cleared', 'revived', 'failed', 'delivery_unknown')
         ),
         artifact_outcome TEXT NOT NULL CHECK (
             artifact_outcome IN ('not_applicable', 'reconciled', 'residue', 'delivery_unknown')
         ),
         overall_outcome TEXT NOT NULL CHECK (
             overall_outcome IN ('cleared', 'cleared_with_residue', 'revived', 'failed')
         ),
         process_count INTEGER CHECK (process_count >= 0),
         estimated_reclaimed_memory_bytes INTEGER
             CHECK (estimated_reclaimed_memory_bytes >= 0),
         revival_checks_completed INTEGER NOT NULL
             CHECK (revival_checks_completed >= 0)
     );
     CREATE INDEX cleanup_impacts_recent
         ON cleanup_impacts (occurred_at_ms DESC, id DESC);
     CREATE TABLE storage_residue_latest (
         kind TEXT PRIMARY KEY CHECK (kind IN ('chrome_code_sign_clone')),
         observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
         payload_json TEXT NOT NULL
     );
     CREATE TABLE impact_authority (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         tracking_started_at_ms INTEGER NOT NULL CHECK (tracking_started_at_ms >= 0),
         historical_completeness TEXT NOT NULL CHECK (
             historical_completeness IN ('complete', 'partial_backfill')
         ),
         terminal_cleanup_count INTEGER NOT NULL CHECK (terminal_cleanup_count >= 0),
         proved_reclaim_count INTEGER NOT NULL CHECK (proved_reclaim_count >= 0),
         reclaimed_process_count INTEGER NOT NULL CHECK (reclaimed_process_count >= 0),
         reclaimed_process_measurement_count INTEGER NOT NULL
             CHECK (reclaimed_process_measurement_count >= 0),
         estimated_reclaimed_memory_bytes INTEGER NOT NULL
             CHECK (estimated_reclaimed_memory_bytes >= 0),
         reclaimed_memory_measurement_count INTEGER NOT NULL
             CHECK (reclaimed_memory_measurement_count >= 0)
     );
     INSERT INTO impact_authority (
         singleton, tracking_started_at_ms, historical_completeness,
         terminal_cleanup_count, proved_reclaim_count,
         reclaimed_process_count, reclaimed_process_measurement_count,
         estimated_reclaimed_memory_bytes, reclaimed_memory_measurement_count
     ) VALUES (
         1,
         CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER),
         'complete', 0, 0, 0, 0, 0, 0
     );
     CREATE TABLE mutation_authority (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         namespace_token TEXT NOT NULL UNIQUE CHECK (length(namespace_token) = 32)
     );
     INSERT INTO mutation_authority (singleton, namespace_token)
         VALUES (1, lower(hex(randomblob(16))));
     CREATE TABLE control_metadata (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         cleanup_policy_revision INTEGER NOT NULL
             CHECK (cleanup_policy_revision >= 1)
     );
     INSERT INTO control_metadata (singleton, cleanup_policy_revision)
         VALUES (1, 1);
     CREATE TABLE ordinary_mutation_receipts (
         namespace_token TEXT NOT NULL CHECK (length(namespace_token) = 32),
         mutation_id TEXT NOT NULL CHECK (length(mutation_id) = 36),
         canonical_version INTEGER NOT NULL CHECK (canonical_version = 1),
         command_json TEXT NOT NULL,
         receipt_json TEXT NOT NULL,
         committed_at_ms INTEGER NOT NULL CHECK (committed_at_ms >= 0),
         retain_until_ms INTEGER NOT NULL CHECK (retain_until_ms >= committed_at_ms),
         PRIMARY KEY (namespace_token, mutation_id)
     );
     CREATE INDEX ordinary_mutation_receipts_recent
         ON ordinary_mutation_receipts (committed_at_ms DESC, namespace_token, mutation_id);";

const MANAGED_LIFECYCLE_SQL: &str = "CREATE TABLE IF NOT EXISTS managed_lifecycle (
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
         activation_generation INTEGER NOT NULL CHECK (activation_generation > 0),
         instance_id TEXT NOT NULL,
         requested_enforce INTEGER NOT NULL CHECK (requested_enforce IN (0, 1)),
         effective_enforce INTEGER NOT NULL CHECK (effective_enforce IN (0, 1)),
         armed_generation INTEGER CHECK (armed_generation > 0),
         enforcement_epoch TEXT,
         ready INTEGER NOT NULL CHECK (ready IN (0, 1)),
         draining INTEGER NOT NULL CHECK (draining IN (0, 1)),
         startup_phase TEXT NOT NULL CHECK (
             startup_phase IN (
                 'recovering', 'first_scan_report_only', 'ready_report_only',
                 'ready_enforce', 'draining', 'failed'
             )
         ),
         updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
         CHECK (
             (effective_enforce = 0 AND armed_generation IS NULL AND enforcement_epoch IS NULL)
             OR (
                 effective_enforce = 1
                 AND requested_enforce = 1
                 AND armed_generation = activation_generation
                 AND enforcement_epoch IS NOT NULL
                 AND ready = 1
                 AND draining = 0
             )
         )
     );";

fn events_have_attempt_id(transaction: &Transaction<'_>) -> Result<bool, StoreError> {
    let mut statement = transaction.prepare("PRAGMA table_info(events)")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for column in rows {
        if column? == "attempt_id" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn reserve_mutation_receipt_slot(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    let count = transaction.query_row(
        "SELECT COUNT(*) FROM ordinary_mutation_receipts",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let max_receipts = i64::try_from(MAX_MUTATION_RECEIPTS)
        .map_err(|_| StoreError::Range("mutation receipt limit overflowed i64".to_owned()))?;
    if count < max_receipts {
        return Ok(());
    }
    Err(StoreError::Capacity(format!(
        "the mutation receipt authority has reached its {MAX_MUTATION_RECEIPTS}-receipt admission limit"
    )))
}

fn current_mutation_namespace(connection: &Connection) -> Result<String, StoreError> {
    connection
        .query_row(
            "SELECT namespace_token FROM mutation_authority WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(StoreError::from)
        .and_then(|token| {
            if unlinger_protocol::is_valid_namespace_token(&token) {
                Ok(token)
            } else {
                Err(StoreError::Corrupt(
                    "mutation authority namespace has an invalid shape".to_owned(),
                ))
            }
        })
}

fn cleanup_policy_revision_connection(connection: &Connection) -> Result<u64, StoreError> {
    let revision = connection.query_row(
        "SELECT cleanup_policy_revision FROM control_metadata WHERE singleton = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    u64::try_from(revision)
        .map_err(|_| StoreError::Corrupt("cleanup policy revision is negative".to_owned()))
}

fn mutation_receipt_connection(
    connection: &Connection,
    context: &MutationContext,
) -> Result<Option<MutationReceipt>, StoreError> {
    let receipt_json = connection
        .query_row(
            "SELECT receipt_json FROM ordinary_mutation_receipts
             WHERE namespace_token = ?1 AND mutation_id = ?2",
            params![context.namespace_token, context.mutation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    receipt_json
        .map(|json| {
            let receipt: MutationReceipt = serde_json::from_str(&json)?;
            validate_stored_mutation_receipt(&receipt, context, receipt.kind)?;
            Ok(receipt)
        })
        .transpose()
}

fn validate_stored_mutation_receipt(
    receipt: &MutationReceipt,
    context: &MutationContext,
    expected_kind: MutationKind,
) -> Result<(), StoreError> {
    if receipt.namespace_token != context.namespace_token
        || receipt.mutation_id != context.mutation_id
        || receipt.kind != expected_kind
        || receipt.retain_until_unix_millis < receipt.committed_at_unix_millis
    {
        return Err(StoreError::Corrupt(
            "stored mutation receipt disagrees with its durable index or canonical request"
                .to_owned(),
        ));
    }
    Ok(())
}

fn store_policy_facts_connection(
    connection: &Connection,
    mutation: &OrdinaryMutation,
    now_unix_millis: u64,
) -> Result<StorePolicyFacts, StoreError> {
    let now = sqlite_millis(now_unix_millis, "policy evaluation timestamp")?;
    let paused = connection
        .query_row(
            "SELECT integer_value FROM settings WHERE key = 'pause_until_ms'",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten()
        .is_some_and(|deadline| deadline > now);
    let mut facts = StorePolicyFacts {
        paused,
        ..StorePolicyFacts::default()
    };
    let incident_id = match mutation {
        OrdinaryMutation::Pause { .. } | OrdinaryMutation::Resume => return Ok(facts),
        OrdinaryMutation::RetryFailedCleanup { incident_id }
        | OrdinaryMutation::ProtectIncident { incident_id }
        | OrdinaryMutation::UnprotectIncident { incident_id } => incident_id,
    };
    facts.cleanup_blocked = connection
        .query_row(
            "SELECT 1 FROM cleanup_retry_blocks WHERE incident_id = ?1",
            params![incident_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    facts.incident_observed = connection
        .query_row(
            "SELECT 1 FROM events
             WHERE incident_id = ?1 AND kind = 'observation' LIMIT 1",
            params![incident_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    facts.incident_protected = connection
        .query_row(
            "SELECT 1 FROM incident_protections WHERE incident_id = ?1",
            params![incident_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(facts)
}

fn apply_ordinary_mutation(
    transaction: &Transaction<'_>,
    mutation: &OrdinaryMutation,
    committed_at_unix_millis: u64,
) -> Result<MutationResult, StoreError> {
    let committed_at = sqlite_millis(committed_at_unix_millis, "mutation timestamp")?;
    match mutation {
        OrdinaryMutation::Pause { duration_millis } => {
            let deadline = committed_at_unix_millis
                .checked_add(*duration_millis)
                .ok_or_else(|| StoreError::Range("pause deadline overflowed u64".to_owned()))?;
            let deadline_sqlite = sqlite_millis(deadline, "pause deadline")?;
            transaction.execute(
                "INSERT INTO settings (key, integer_value) VALUES ('pause_until_ms', ?1)
                 ON CONFLICT(key) DO UPDATE SET integer_value = excluded.integer_value",
                params![deadline_sqlite],
            )?;
            Ok(MutationResult::Paused {
                until_unix_millis: deadline,
            })
        }
        OrdinaryMutation::Resume => {
            let changed =
                transaction.execute("DELETE FROM settings WHERE key = 'pause_until_ms'", [])?;
            if changed != 1 {
                return Err(StoreError::Corrupt(
                    "resume policy allowed without an exact durable pause".to_owned(),
                ));
            }
            Ok(MutationResult::Resumed)
        }
        OrdinaryMutation::RetryFailedCleanup { incident_id } => {
            let tracking_key = transaction
                .query_row(
                    "SELECT tracking_key FROM cleanup_retry_blocks WHERE incident_id = ?1",
                    params![incident_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    StoreError::Corrupt(
                        "retry policy allowed without an exact cleanup block".to_owned(),
                    )
                })?;
            transaction.execute(
                "DELETE FROM cleanup_retry_blocks WHERE incident_id = ?1",
                params![incident_id],
            )?;
            transaction.execute(
                "DELETE FROM cooling_candidates WHERE tracking_key = ?1",
                params![tracking_key],
            )?;
            Ok(MutationResult::RetryScheduled {
                incident_id: incident_id.clone(),
            })
        }
        OrdinaryMutation::ProtectIncident { incident_id } => {
            let payload_json = transaction
                .query_row(
                    "SELECT payload_json FROM events
                     WHERE incident_id = ?1 AND kind = 'observation'
                     ORDER BY occurred_at_ms DESC, id DESC LIMIT 1",
                    params![incident_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    StoreError::Corrupt(
                        "protection policy allowed without an exact observation".to_owned(),
                    )
                })?;
            let EventPayload::Observation { report } =
                serde_json::from_str::<EventPayload>(&payload_json)?
            else {
                return Err(StoreError::Corrupt(
                    "observation index selected a non-observation payload".to_owned(),
                ));
            };
            if report.incident_id != *incident_id {
                return Err(StoreError::Corrupt(
                    "observation incident ID disagrees with its index".to_owned(),
                ));
            }
            transaction.execute(
                "INSERT INTO incident_protections (
                     incident_id, root_identity_fingerprint,
                     member_fingerprint, protected_at_ms
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    report.incident_id,
                    report.root.identity_fingerprint,
                    report.member_fingerprint,
                    committed_at
                ],
            )?;
            let protection =
                protection_summary_transaction(transaction, incident_id)?.ok_or_else(|| {
                    StoreError::Corrupt(
                        "incident protection disappeared during creation".to_owned(),
                    )
                })?;
            Ok(MutationResult::IncidentProtected {
                protection: PublicProtectionSummary {
                    incident_id: protection.incident_id,
                    protected_at_unix_millis: protection.protected_at_unix_millis,
                    last_exact_observed_at_unix_millis: protection
                        .last_exact_observed_at_unix_millis,
                    exact_absence_since_unix_millis: protection.exact_absence_since_unix_millis,
                },
            })
        }
        OrdinaryMutation::UnprotectIncident { incident_id } => {
            let changed = transaction.execute(
                "DELETE FROM incident_protections WHERE incident_id = ?1",
                params![incident_id],
            )?;
            if changed != 1 {
                return Err(StoreError::Corrupt(
                    "unprotect policy allowed without an exact protection".to_owned(),
                ));
            }
            Ok(MutationResult::IncidentUnprotected {
                incident_id: incident_id.clone(),
            })
        }
    }
}

fn insert_event_transaction(
    transaction: &Transaction<'_>,
    attempt_id: Option<i64>,
    occurred_at_unix_millis: u64,
    incident_id: &str,
    kind: EventKind,
    state: IncidentState,
    payload: &EventPayload,
) -> Result<i64, StoreError> {
    let occurred_at = sqlite_millis(occurred_at_unix_millis, "event timestamp")?;
    let payload_json = serde_json::to_string(payload)?;
    transaction.execute(
        "INSERT INTO events (
             attempt_id, incident_id, first_occurred_at_ms, occurred_at_ms,
             observation_count, kind, state, payload_json
         ) VALUES (?1, ?2, ?3, ?3, 1, ?4, ?5, ?6)",
        params![
            attempt_id,
            incident_id,
            occurred_at,
            kind.as_str(),
            state_name(state),
            payload_json
        ],
    )?;
    let event_id = transaction.last_insert_rowid();
    insert_public_event_token(transaction, event_id)?;
    Ok(event_id)
}

fn upsert_observation_span_transaction(
    transaction: &Transaction<'_>,
    occurred_at_unix_millis: u64,
    report: &ObservationRecord,
) -> Result<i64, StoreError> {
    let occurred_at = sqlite_millis(occurred_at_unix_millis, "observation timestamp")?;
    let latest = transaction
        .query_row(
            "SELECT id, occurred_at_ms, kind, payload_json
             FROM events
             WHERE incident_id = ?1
             ORDER BY occurred_at_ms DESC, id DESC
             LIMIT 1",
            params![report.incident_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    if let Some((event_id, latest_at, kind, payload_json)) = latest
        && kind == EventKind::Observation.as_str()
        && latest_at <= occurred_at
    {
        let payload = serde_json::from_str::<EventPayload>(&payload_json)?;
        if let EventPayload::Observation {
            report: previous_report,
        } = payload
            && observations_semantically_equal(&previous_report, report)
        {
            let payload_json = serde_json::to_string(&EventPayload::Observation {
                report: report.clone(),
            })?;
            let changed = transaction.execute(
                "UPDATE events
                 SET occurred_at_ms = ?1,
                     observation_count = observation_count + 1,
                     payload_json = ?2
                 WHERE id = ?3 AND kind = 'observation'",
                params![occurred_at, payload_json, event_id],
            )?;
            if changed != 1 {
                return Err(StoreError::Corrupt(format!(
                    "observation span {event_id} disappeared during extension"
                )));
            }
            return Ok(event_id);
        }
    }
    insert_event_transaction(
        transaction,
        None,
        occurred_at_unix_millis,
        &report.incident_id,
        EventKind::Observation,
        report.state,
        &EventPayload::Observation {
            report: report.clone(),
        },
    )
}

fn observations_semantically_equal(
    previous: &ObservationRecord,
    current: &ObservationRecord,
) -> bool {
    previous.incident_id == current.incident_id
        && previous.signature_pack == current.signature_pack
        && previous.signature_version == current.signature_version
        && previous.state == current.state
        && previous.root == current.root
        && previous.member_count == current.member_count
        && previous.member_fingerprint == current.member_fingerprint
        && previous.roles == current.roles
        && previous.evidence == current.evidence
        && previous.gates == current.gates
}

fn insert_cleanup_impact_transaction(
    transaction: &Transaction<'_>,
    attempt_id: i64,
    event_id: i64,
    occurred_at_unix_millis: u64,
    receipt: &CleanupReceipt,
) -> Result<(), StoreError> {
    let (family, family_version, observed_process_count) = transaction.query_row(
        "SELECT COALESCE(NULLIF(signature_pack, ''), 'unknown'),
                COALESCE(NULLIF(signature_version, ''), 'unknown'),
                observed_process_count
         FROM cleanup_attempts WHERE id = ?1",
        params![attempt_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        },
    )?;
    let outcome = receipt.outcome();
    let process_count = if outcome.process == ProcessOutcome::Cleared {
        receipt
            .resources
            .before
            .as_ref()
            .map(|resources| i64::try_from(resources.process_count))
            .transpose()
            .map_err(|_| {
                StoreError::Range("cleanup impact process count overflowed i64".to_owned())
            })?
            .or(observed_process_count)
    } else {
        None
    };
    let estimated_memory = if outcome.process == ProcessOutcome::Cleared {
        receipt
            .resources
            .estimated_reclaimed_memory_bytes
            .map(|value| sqlite_u64(value, "cleanup impact reclaimed memory"))
            .transpose()?
    } else {
        None
    };
    let occurred_at = sqlite_millis(occurred_at_unix_millis, "cleanup impact timestamp")?;
    transaction.execute(
        "INSERT INTO cleanup_impacts (
             attempt_id, event_id, incident_id, family, family_version,
             occurred_at_ms, state, process_outcome, artifact_outcome,
             overall_outcome, process_count, estimated_reclaimed_memory_bytes,
             revival_checks_completed
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            attempt_id,
            event_id,
            receipt.incident_id,
            family,
            family_version,
            occurred_at,
            state_name(receipt.state),
            process_outcome_name(outcome.process),
            artifact_outcome_name(outcome.artifact),
            overall_outcome_name(outcome.overall),
            process_count,
            estimated_memory,
            i64::try_from(receipt.revival_checks_completed).map_err(|_| {
                StoreError::Range("cleanup impact revival count overflowed i64".to_owned())
            })?,
        ],
    )?;
    let proved_reclaim = i64::from(outcome.process == ProcessOutcome::Cleared);
    let process_measurement = i64::from(process_count.is_some());
    let memory_measurement = i64::from(estimated_memory.is_some());
    transaction.execute(
        "UPDATE impact_authority
         SET terminal_cleanup_count = terminal_cleanup_count + 1,
             proved_reclaim_count = proved_reclaim_count + ?1,
             reclaimed_process_count = reclaimed_process_count + COALESCE(?2, 0),
             reclaimed_process_measurement_count =
                 reclaimed_process_measurement_count + ?3,
             estimated_reclaimed_memory_bytes =
                 estimated_reclaimed_memory_bytes + COALESCE(?4, 0),
             reclaimed_memory_measurement_count =
                 reclaimed_memory_measurement_count + ?5
         WHERE singleton = 1",
        params![
            proved_reclaim,
            process_count,
            process_measurement,
            estimated_memory,
            memory_measurement,
        ],
    )?;
    Ok(())
}

fn insert_public_event_token(
    transaction: &Transaction<'_>,
    event_id: i64,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO public_event_tokens (event_id, event_token)
         VALUES (?1, lower(hex(randomblob(16))))",
        params![event_id],
    )?;
    Ok(())
}

fn actions_for_attempt(
    transaction: &Transaction<'_>,
    attempt_id: i64,
) -> Result<Vec<CleanupAction>, StoreError> {
    let mut statement = transaction.prepare(
        "SELECT stage, pid, identity_fingerprint, signal, disposition
         FROM cleanup_actions WHERE attempt_id = ?1 ORDER BY sequence",
    )?;
    let rows = statement.query_map(params![attempt_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut actions = Vec::new();
    for row in rows {
        let (stage, pid, identity_fingerprint, signal, disposition) = row?;
        let disposition = disposition.ok_or_else(|| {
            StoreError::Corrupt(format!(
                "cleanup attempt {attempt_id} still contains a PREPARED action"
            ))
        })?;
        actions.push(CleanupAction {
            stage: parse_cleanup_stage(&stage)?,
            pid: u32::try_from(pid)
                .map_err(|_| StoreError::Corrupt("cleanup action PID is invalid".to_owned()))?,
            identity_fingerprint,
            signal: parse_cleanup_signal(&signal)?,
            disposition: parse_signal_disposition(&disposition)?,
        });
    }
    Ok(actions)
}

fn artifact_actions_for_attempt(
    transaction: &Transaction<'_>,
    attempt_id: i64,
) -> Result<Vec<ArtifactAction>, StoreError> {
    let mut statement = transaction.prepare(
        "SELECT kind, artifact_fingerprint, disposition
         FROM cleanup_artifact_actions WHERE attempt_id = ?1 ORDER BY sequence",
    )?;
    let rows = statement.query_map(params![attempt_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut actions = Vec::new();
    for row in rows {
        let (kind, artifact_fingerprint, disposition) = row?;
        let disposition = disposition.ok_or_else(|| {
            StoreError::Corrupt(format!(
                "cleanup attempt {attempt_id} still contains a PREPARED artifact action"
            ))
        })?;
        if !valid_artifact_fingerprint(&artifact_fingerprint) {
            return Err(StoreError::Corrupt(format!(
                "cleanup attempt {attempt_id} contains an invalid artifact fingerprint"
            )));
        }
        actions.push(ArtifactAction {
            kind: parse_runtime_artifact_kind(&kind)?,
            artifact_fingerprint,
            disposition: parse_artifact_disposition(&disposition)?,
        });
    }
    Ok(actions)
}

fn terminal_receipt_for_attempt(
    transaction: &Transaction<'_>,
    attempt_id: i64,
) -> Result<CleanupReceipt, StoreError> {
    let (payload_json, resources_json) = transaction
        .query_row(
            "SELECT events.payload_json, attempts.resources_json
             FROM events
             JOIN cleanup_attempts AS attempts ON attempts.id = events.attempt_id
             WHERE events.attempt_id = ?1
               AND events.state IN ('CLEARED', 'FAILED', 'REVIVED')
             ORDER BY events.id DESC LIMIT 1",
            params![attempt_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            StoreError::Corrupt(format!(
                "terminal cleanup attempt {attempt_id} has no terminal event"
            ))
        })?;
    match serde_json::from_str::<EventPayload>(&payload_json)? {
        EventPayload::Cleanup { mut receipt } => {
            receipt.actions = actions_for_attempt(transaction, attempt_id)?;
            receipt.artifact_actions = artifact_actions_for_attempt(transaction, attempt_id)?;
            if let Some(resources_json) = resources_json {
                if resources_json.len() > MAX_RESOURCE_RECEIPT_BYTES {
                    return Err(StoreError::Corrupt(
                        "persisted cleanup resource receipt exceeds its bound".to_owned(),
                    ));
                }
                receipt.resources = serde_json::from_str(&resources_json)?;
            }
            Ok(receipt)
        }
        EventPayload::Observation { .. } => Err(StoreError::Corrupt(format!(
            "terminal cleanup attempt {attempt_id} points to an observation"
        ))),
    }
}

fn protection_summary_transaction(
    transaction: &Transaction<'_>,
    incident_id: &str,
) -> Result<Option<ProtectedIncidentSummary>, StoreError> {
    transaction
        .query_row(
            "SELECT incident_id, protected_at_ms,
                    last_exact_observed_at_ms, absence_since_ms
             FROM incident_protections WHERE incident_id = ?1",
            params![incident_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?
        .map(
            |(incident_id, protected_at, last_observed, absence_since)| {
                projected_protection_summary(
                    incident_id,
                    protected_at,
                    last_observed,
                    absence_since,
                )
            },
        )
        .transpose()
}

fn projected_protection_summary(
    incident_id: String,
    protected_at: i64,
    last_observed: Option<i64>,
    absence_since: Option<i64>,
) -> Result<ProtectedIncidentSummary, StoreError> {
    Ok(ProtectedIncidentSummary {
        incident_id: project_identifier(&incident_id, "redacted-incident"),
        protected_at_unix_millis: parse_nonnegative_millis(
            protected_at,
            "incident protection timestamp",
        )?,
        last_exact_observed_at_unix_millis: last_observed
            .map(|value| parse_nonnegative_millis(value, "last protected root observation"))
            .transpose()?,
        exact_absence_since_unix_millis: absence_since
            .map(|value| parse_nonnegative_millis(value, "protected root absence timestamp"))
            .transpose()?,
    })
}

fn insert_retry_block(
    transaction: &Transaction<'_>,
    incident_id: &str,
    tracking_key: &str,
    blocked_at_ms: i64,
    reason_id: Option<&str>,
    source_attempt_id: i64,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO cleanup_retry_blocks (
             incident_id, tracking_key, blocked_at_ms, reason_id, source_attempt_id
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(incident_id) DO UPDATE SET
             tracking_key = excluded.tracking_key,
             blocked_at_ms = excluded.blocked_at_ms,
             reason_id = excluded.reason_id,
             source_attempt_id = excluded.source_attempt_id,
             last_exact_observed_at_ms = NULL,
             absence_since_ms = NULL",
        params![
            incident_id,
            tracking_key,
            blocked_at_ms,
            reason_id,
            source_attempt_id
        ],
    )?;
    Ok(())
}

fn validate_enforcement_epoch(enforcement_epoch: &str) -> Result<(), StoreError> {
    if enforcement_epoch.trim().is_empty() || enforcement_epoch.len() > 192 {
        Err(StoreError::Invalid(
            "managed arm requires a bounded non-empty enforcement epoch".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn managed_arm_blocker(transaction: &Transaction<'_>) -> Result<Option<&'static str>, StoreError> {
    let open_attempt = transaction
        .query_row(
            "SELECT 1 FROM cleanup_attempts WHERE completed_at_ms IS NULL LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if open_attempt {
        return Ok(Some(
            "managed arm is blocked by an incomplete cleanup attempt",
        ));
    }
    persistent_enforcement_blocker(transaction)
}

fn persistent_enforcement_blocker(
    connection: &Connection,
) -> Result<Option<&'static str>, StoreError> {
    let delivery_unknown = connection
        .query_row(
            "SELECT 1 FROM cleanup_retry_blocks AS block
             WHERE EXISTS (
                 SELECT 1 FROM cleanup_actions AS action
                 WHERE action.attempt_id = block.source_attempt_id
                   AND action.disposition = 'delivery_unknown'
             ) OR EXISTS (
                 SELECT 1 FROM cleanup_artifact_actions AS action
                 WHERE action.attempt_id = block.source_attempt_id
                   AND action.disposition = 'delivery_unknown'
             )
             LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if delivery_unknown {
        return Ok(Some(
            "managed arm is blocked by unresolved delivery-unknown cleanup",
        ));
    }
    let interrupted_after_delivery = connection
        .query_row(
            "SELECT 1 FROM cleanup_retry_blocks AS block
             WHERE EXISTS (
                 SELECT 1 FROM cleanup_actions AS action
                 WHERE action.attempt_id = block.source_attempt_id
                   AND action.disposition = 'delivered'
             ) OR EXISTS (
                 SELECT 1 FROM cleanup_artifact_actions AS action
                 WHERE action.attempt_id = block.source_attempt_id
                   AND action.disposition = 'removed'
             )
             LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(interrupted_after_delivery
        .then_some("managed arm is blocked by cleanup interrupted after a delivered side effect"))
}

fn apply_managed_arm(
    transaction: &Transaction<'_>,
    enforcement_epoch: &str,
    occurred_at: i64,
) -> Result<(), StoreError> {
    transaction.execute("DELETE FROM cooling_candidates", [])?;
    transaction.execute(
        "UPDATE managed_lifecycle
         SET requested_enforce = 1, effective_enforce = 1,
             armed_generation = activation_generation,
             enforcement_epoch = ?1, ready = 1, draining = 0,
             startup_phase = 'ready_enforce', updated_at_ms = ?2
         WHERE singleton = 1",
        params![enforcement_epoch, occurred_at],
    )?;
    Ok(())
}

fn apply_managed_report_only_ready(
    transaction: &Transaction<'_>,
    occurred_at: i64,
    clear_requested_enforce: bool,
) -> Result<(), StoreError> {
    transaction.execute(
        "UPDATE managed_lifecycle
         SET requested_enforce = CASE WHEN ?1 THEN 0 ELSE requested_enforce END,
             effective_enforce = 0, armed_generation = NULL,
             enforcement_epoch = NULL, ready = 1, draining = 0,
             startup_phase = 'ready_report_only', updated_at_ms = ?2
         WHERE singleton = 1",
        params![clear_requested_enforce, occurred_at],
    )?;
    Ok(())
}

fn managed_lifecycle_transaction(
    transaction: &Transaction<'_>,
) -> Result<Option<ManagedLifecycle>, StoreError> {
    managed_lifecycle_connection(transaction)
}

fn managed_lifecycle_connection(
    connection: &Connection,
) -> Result<Option<ManagedLifecycle>, StoreError> {
    let stored = connection
        .query_row(
            "SELECT activation_generation, instance_id,
                    requested_enforce, effective_enforce, armed_generation,
                    enforcement_epoch, ready, draining, startup_phase, updated_at_ms
             FROM managed_lifecycle WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            },
        )
        .optional()?;
    stored
        .map(
            |(
                generation,
                instance_id,
                requested_enforce,
                effective_enforce,
                armed_generation,
                enforcement_epoch,
                ready,
                draining,
                startup_phase,
                updated_at,
            )| {
                let activation_generation = u64::try_from(generation).map_err(|_| {
                    StoreError::Corrupt("managed activation generation is invalid".to_owned())
                })?;
                let armed_generation = armed_generation
                    .map(|generation| {
                        u64::try_from(generation).map_err(|_| {
                            StoreError::Corrupt("managed armed generation is invalid".to_owned())
                        })
                    })
                    .transpose()?;
                let updated_at_unix_millis = u64::try_from(updated_at).map_err(|_| {
                    StoreError::Corrupt("managed lifecycle timestamp is invalid".to_owned())
                })?;
                Ok(ManagedLifecycle {
                    activation_generation,
                    instance_id,
                    requested_enforce: parse_sqlite_bool(requested_enforce, "requested_enforce")?,
                    effective_enforce: parse_sqlite_bool(effective_enforce, "effective_enforce")?,
                    armed_generation,
                    enforcement_epoch,
                    ready: parse_sqlite_bool(ready, "ready")?,
                    draining: parse_sqlite_bool(draining, "draining")?,
                    startup_phase: parse_managed_startup_phase(&startup_phase)?,
                    updated_at_unix_millis,
                })
            },
        )
        .transpose()
}

fn validate_managed_identity(
    activation_generation: u64,
    instance_id: &str,
) -> Result<(), StoreError> {
    if activation_generation == 0 {
        return Err(StoreError::Invalid(
            "managed activation generation must be greater than zero".to_owned(),
        ));
    }
    if instance_id.trim().is_empty()
        || instance_id.len() > 192
        || instance_id.chars().any(char::is_control)
    {
        return Err(StoreError::Invalid(
            "managed instance ID must contain 1 to 192 printable bytes".to_owned(),
        ));
    }
    Ok(())
}

fn require_exact_managed_identity(
    lifecycle: &ManagedLifecycle,
    activation_generation: u64,
    instance_id: &str,
) -> Result<(), StoreError> {
    if lifecycle.activation_generation != activation_generation {
        return Err(StoreError::Invalid(format!(
            "managed activation generation mismatch: active {}, requested {}",
            lifecycle.activation_generation, activation_generation
        )));
    }
    if lifecycle.instance_id != instance_id {
        return Err(StoreError::Invalid(
            "managed instance ID does not match the active daemon".to_owned(),
        ));
    }
    Ok(())
}

fn sqlite_generation(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value)
        .map_err(|_| StoreError::Range("activation generation overflowed i64".to_owned()))
}

fn parse_sqlite_bool(value: i64, field: &str) -> Result<bool, StoreError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StoreError::Corrupt(format!(
            "managed lifecycle field {field:?} is not boolean"
        ))),
    }
}

fn managed_startup_phase_name(phase: ManagedStartupPhase) -> &'static str {
    match phase {
        ManagedStartupPhase::Recovering => "recovering",
        ManagedStartupPhase::FirstScanReportOnly => "first_scan_report_only",
        ManagedStartupPhase::ReadyReportOnly => "ready_report_only",
        ManagedStartupPhase::ReadyEnforce => "ready_enforce",
        ManagedStartupPhase::Draining => "draining",
        ManagedStartupPhase::Failed => "failed",
    }
}

fn parse_managed_startup_phase(value: &str) -> Result<ManagedStartupPhase, StoreError> {
    match value {
        "recovering" => Ok(ManagedStartupPhase::Recovering),
        "first_scan_report_only" => Ok(ManagedStartupPhase::FirstScanReportOnly),
        "ready_report_only" => Ok(ManagedStartupPhase::ReadyReportOnly),
        "ready_enforce" => Ok(ManagedStartupPhase::ReadyEnforce),
        "draining" => Ok(ManagedStartupPhase::Draining),
        "failed" => Ok(ManagedStartupPhase::Failed),
        other => Err(StoreError::Corrupt(format!(
            "unknown managed startup phase {other:?}"
        ))),
    }
}

fn sqlite_millis(value: u64, label: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Range(format!("{label} overflowed i64")))
}

fn sqlite_u64(value: u64, label: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Range(format!("{label} overflowed i64")))
}

fn current_unix_millis() -> Result<u64, StoreError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            StoreError::Invalid(format!("system clock precedes Unix epoch: {error}"))
        })?;
    u64::try_from(elapsed.as_millis())
        .map_err(|_| StoreError::Range("system clock overflowed u64 milliseconds".to_owned()))
}

fn parse_nonnegative_millis(value: i64, label: &str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Corrupt(format!("{label} is negative")))
}

fn project_identifier(value: &str, fallback: &str) -> String {
    let valid = !value.is_empty()
        && value.len() <= MAX_REDACTED_IDENTIFIER_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        value.to_owned()
    } else {
        fallback.to_owned()
    }
}

fn valid_artifact_fingerprint(value: &str) -> bool {
    value.starts_with("art-")
        && value.len() <= MAX_REDACTED_IDENTIFIER_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn project_reason_id(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "cleanup.unclassified_failure".to_owned();
    };
    let valid = !value.is_empty()
        && value.len() <= MAX_REASON_ID_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        value.to_owned()
    } else {
        "cleanup.unclassified_failure".to_owned()
    }
}

fn wall_continuity_is_plausible(
    wall: i64,
    previous_wall: i64,
    continuous: i64,
    previous_continuous: i64,
    tolerance_millis: u64,
) -> bool {
    let Ok(wall_gap) = u64::try_from(wall - previous_wall) else {
        return false;
    };
    let Ok(continuous_gap) = u64::try_from(continuous - previous_continuous) else {
        return false;
    };
    wall_gap.abs_diff(continuous_gap) <= tolerance_millis
}

fn cleanup_stage_name(stage: CleanupStage) -> &'static str {
    match stage {
        CleanupStage::PrimaryTerm => "primary_term",
        CleanupStage::MemberTerm => "member_term",
        CleanupStage::ExactKill => "exact_kill",
    }
}

fn parse_cleanup_stage(value: &str) -> Result<CleanupStage, StoreError> {
    match value {
        "primary_term" => Ok(CleanupStage::PrimaryTerm),
        "member_term" => Ok(CleanupStage::MemberTerm),
        "exact_kill" => Ok(CleanupStage::ExactKill),
        other => Err(StoreError::Corrupt(format!(
            "unknown cleanup stage {other:?}"
        ))),
    }
}

fn cleanup_signal_name(signal: CleanupSignal) -> &'static str {
    match signal {
        CleanupSignal::Term => "term",
        CleanupSignal::Kill => "kill",
    }
}

fn parse_cleanup_signal(value: &str) -> Result<CleanupSignal, StoreError> {
    match value {
        "term" => Ok(CleanupSignal::Term),
        "kill" => Ok(CleanupSignal::Kill),
        other => Err(StoreError::Corrupt(format!(
            "unknown cleanup signal {other:?}"
        ))),
    }
}

fn signal_disposition_name(disposition: SignalDisposition) -> &'static str {
    match disposition {
        SignalDisposition::Delivered => "delivered",
        SignalDisposition::AlreadyExited => "already_exited",
        SignalDisposition::IdentityMismatch => "identity_mismatch",
        SignalDisposition::Rejected => "rejected",
        SignalDisposition::CancelledBeforeDelivery => "cancelled_before_delivery",
        SignalDisposition::DeliveryUnknown => "delivery_unknown",
    }
}

fn parse_signal_disposition(value: &str) -> Result<SignalDisposition, StoreError> {
    match value {
        "delivered" => Ok(SignalDisposition::Delivered),
        "already_exited" => Ok(SignalDisposition::AlreadyExited),
        "identity_mismatch" => Ok(SignalDisposition::IdentityMismatch),
        "rejected" => Ok(SignalDisposition::Rejected),
        "cancelled_before_delivery" => Ok(SignalDisposition::CancelledBeforeDelivery),
        "delivery_unknown" => Ok(SignalDisposition::DeliveryUnknown),
        other => Err(StoreError::Corrupt(format!(
            "unknown signal disposition {other:?}"
        ))),
    }
}

fn runtime_artifact_kind_name(kind: RuntimeArtifactKind) -> &'static str {
    match kind {
        RuntimeArtifactKind::DevToolsActivePort => "dev_tools_active_port",
    }
}

fn parse_runtime_artifact_kind(value: &str) -> Result<RuntimeArtifactKind, StoreError> {
    match value {
        "dev_tools_active_port" => Ok(RuntimeArtifactKind::DevToolsActivePort),
        other => Err(StoreError::Corrupt(format!(
            "unknown runtime artifact kind {other:?}"
        ))),
    }
}

fn artifact_disposition_name(disposition: ArtifactDisposition) -> &'static str {
    match disposition {
        ArtifactDisposition::Removed => "removed",
        ArtifactDisposition::AlreadyAbsent => "already_absent",
        ArtifactDisposition::IdentityMismatch => "identity_mismatch",
        ArtifactDisposition::Referenced => "referenced",
        ArtifactDisposition::Unsafe => "unsafe",
        ArtifactDisposition::Rejected => "rejected",
        ArtifactDisposition::CancelledBeforeDelivery => "cancelled_before_delivery",
        ArtifactDisposition::DeliveryUnknown => "delivery_unknown",
    }
}

fn parse_artifact_disposition(value: &str) -> Result<ArtifactDisposition, StoreError> {
    match value {
        "removed" => Ok(ArtifactDisposition::Removed),
        "already_absent" => Ok(ArtifactDisposition::AlreadyAbsent),
        "identity_mismatch" => Ok(ArtifactDisposition::IdentityMismatch),
        "referenced" => Ok(ArtifactDisposition::Referenced),
        "unsafe" => Ok(ArtifactDisposition::Unsafe),
        "rejected" => Ok(ArtifactDisposition::Rejected),
        "cancelled_before_delivery" => Ok(ArtifactDisposition::CancelledBeforeDelivery),
        "delivery_unknown" => Ok(ArtifactDisposition::DeliveryUnknown),
        other => Err(StoreError::Corrupt(format!(
            "unknown artifact disposition {other:?}"
        ))),
    }
}

fn process_outcome_name(outcome: ProcessOutcome) -> &'static str {
    match outcome {
        ProcessOutcome::Cleared => "cleared",
        ProcessOutcome::Revived => "revived",
        ProcessOutcome::Failed => "failed",
        ProcessOutcome::DeliveryUnknown => "delivery_unknown",
    }
}

fn parse_process_outcome(value: &str) -> Result<ProcessOutcome, StoreError> {
    match value {
        "cleared" => Ok(ProcessOutcome::Cleared),
        "revived" => Ok(ProcessOutcome::Revived),
        "failed" => Ok(ProcessOutcome::Failed),
        "delivery_unknown" => Ok(ProcessOutcome::DeliveryUnknown),
        other => Err(StoreError::Corrupt(format!(
            "unknown process outcome {other:?}"
        ))),
    }
}

fn artifact_outcome_name(outcome: unlinger_core::ArtifactOutcome) -> &'static str {
    match outcome {
        unlinger_core::ArtifactOutcome::NotApplicable => "not_applicable",
        unlinger_core::ArtifactOutcome::Reconciled => "reconciled",
        unlinger_core::ArtifactOutcome::Residue => "residue",
        unlinger_core::ArtifactOutcome::DeliveryUnknown => "delivery_unknown",
    }
}

fn parse_artifact_outcome(value: &str) -> Result<unlinger_core::ArtifactOutcome, StoreError> {
    match value {
        "not_applicable" => Ok(unlinger_core::ArtifactOutcome::NotApplicable),
        "reconciled" => Ok(unlinger_core::ArtifactOutcome::Reconciled),
        "residue" => Ok(unlinger_core::ArtifactOutcome::Residue),
        "delivery_unknown" => Ok(unlinger_core::ArtifactOutcome::DeliveryUnknown),
        other => Err(StoreError::Corrupt(format!(
            "unknown artifact outcome {other:?}"
        ))),
    }
}

fn overall_outcome_name(outcome: unlinger_core::OverallOutcome) -> &'static str {
    match outcome {
        unlinger_core::OverallOutcome::Cleared => "cleared",
        unlinger_core::OverallOutcome::ClearedWithResidue => "cleared_with_residue",
        unlinger_core::OverallOutcome::Revived => "revived",
        unlinger_core::OverallOutcome::Failed => "failed",
    }
}

fn parse_overall_outcome(value: &str) -> Result<unlinger_core::OverallOutcome, StoreError> {
    match value {
        "cleared" => Ok(unlinger_core::OverallOutcome::Cleared),
        "cleared_with_residue" => Ok(unlinger_core::OverallOutcome::ClearedWithResidue),
        "revived" => Ok(unlinger_core::OverallOutcome::Revived),
        "failed" => Ok(unlinger_core::OverallOutcome::Failed),
        other => Err(StoreError::Corrupt(format!(
            "unknown overall outcome {other:?}"
        ))),
    }
}

fn state_name(state: IncidentState) -> &'static str {
    match state {
        IncidentState::Protected => "PROTECTED",
        IncidentState::Active => "ACTIVE",
        IncidentState::Cooling => "COOLING",
        IncidentState::Confirmed => "CONFIRMED",
        IncidentState::Ambiguous => "AMBIGUOUS",
        IncidentState::Reclaiming => "RECLAIMING",
        IncidentState::Cleared => "CLEARED",
        IncidentState::Revived => "REVIVED",
        IncidentState::Failed => "FAILED",
    }
}

fn parse_state(value: &str) -> Result<IncidentState, StoreError> {
    match value {
        "PROTECTED" => Ok(IncidentState::Protected),
        "ACTIVE" => Ok(IncidentState::Active),
        "COOLING" => Ok(IncidentState::Cooling),
        "CONFIRMED" => Ok(IncidentState::Confirmed),
        "AMBIGUOUS" => Ok(IncidentState::Ambiguous),
        "RECLAIMING" => Ok(IncidentState::Reclaiming),
        "CLEARED" => Ok(IncidentState::Cleared),
        "REVIVED" => Ok(IncidentState::Revived),
        "FAILED" => Ok(IncidentState::Failed),
        other => Err(StoreError::Corrupt(format!(
            "unknown incident state {other:?}"
        ))),
    }
}

fn payload_kind(payload: &EventPayload) -> EventKind {
    match payload {
        EventPayload::Observation { .. } => EventKind::Observation,
        EventPayload::Cleanup { .. } => EventKind::Cleanup,
    }
}

fn payload_state(payload: &EventPayload) -> IncidentState {
    match payload {
        EventPayload::Observation { report } => report.state,
        EventPayload::Cleanup { receipt } => receipt.state,
    }
}
