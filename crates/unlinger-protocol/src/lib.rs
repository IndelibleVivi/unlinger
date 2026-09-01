#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestEnvelope {
    pub schema_version: u32,
    pub request_id: u64,
    pub command: Command,
}

impl RequestEnvelope {
    #[must_use]
    pub fn new(request_id: u64, command: Command) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            request_id,
            command,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    Status,
    History {
        limit: usize,
    },
    Explain {
        incident_id: String,
    },
    Incidents,
    MutationStatus {
        context: MutationContext,
    },
    Pause {
        context: MutationContext,
        duration_millis: u64,
    },
    Resume {
        context: MutationContext,
    },
    RetryFailedCleanup {
        context: MutationContext,
        incident_id: String,
    },
    ProtectIncident {
        context: MutationContext,
        incident_id: String,
    },
    UnprotectIncident {
        context: MutationContext,
        incident_id: String,
    },
    ExportDiagnostics {
        incident_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResponseEnvelope {
    pub schema_version: u32,
    pub request_id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl ResponseEnvelope {
    #[must_use]
    pub fn success(request_id: u64, payload: Payload) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            request_id,
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(request_id: u64, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            request_id,
            ok: false,
            payload: None,
            error: Some(ErrorBody {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    InvalidJson,
    UnsupportedSchema,
    InvalidArgument,
    Conflict,
    NotFound,
    AuthorityLost,
    StoreError,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Payload {
    Status(PublicStatus),
    History(Vec<HistoryEvent>),
    Incident(IncidentDetail),
    Incidents(ObservationRoster),
    MutationStatus(MutationStatus),
    MutationCommitted(MutationReceipt),
    Diagnostics(DiagnosticsBundle),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MutationStatus {
    NotFound { context: MutationContext },
    AuthorityLost { context: MutationContext },
    Committed { receipt: MutationReceipt },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MutationContext {
    pub namespace_token: String,
    pub mutation_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Pause,
    Resume,
    RetryFailedCleanup,
    ProtectIncident,
    UnprotectIncident,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MutationReceipt {
    pub namespace_token: String,
    pub mutation_id: String,
    pub kind: MutationKind,
    pub committed_at_unix_millis: u64,
    pub retain_until_unix_millis: u64,
    pub policy_revision_after: u64,
    pub outcome: MutationOutcome,
}

/// Mutation IDs become durable database keys, so the wire representation has
/// one deliberately narrow UUID-style shape shared by every client and server.
#[must_use]
pub fn is_valid_mutation_id(value: &str) -> bool {
    if value.len() != 36 || !value.is_ascii() {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

/// Receipt-authority namespaces are public-safe opaque identifiers. Their
/// exact generation is deliberately private to the daemon, while the bounded
/// lowercase-hex shape keeps journal and IPC validation deterministic.
#[must_use]
pub fn is_valid_namespace_token(value: &str) -> bool {
    value.len() == 32
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MutationOutcome {
    Applied { result: MutationResult },
    NoChange { reason_id: String },
    Rejected { reason_id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum MutationResult {
    Paused { until_unix_millis: u64 },
    Resumed,
    RetryScheduled { incident_id: String },
    IncidentProtected { protection: ProtectionSummary },
    IncidentUnprotected { incident_id: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    ReportOnly,
    Enforce,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    Starting,
    Ready,
    Draining,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicStatus {
    pub daemon_version: String,
    pub healthy: bool,
    pub readiness: Readiness,
    pub effective_mode: Mode,
    pub scan_in_progress: bool,
    pub cleanup_in_progress: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_until_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_started_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_observation_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan_at_unix_millis: Option<u64>,
    pub confirmed_incident_count: usize,
    pub ambiguous_incident_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most_recent_reclaim: Option<RecentReclaim>,
    pub event_source: EventSourceHealth,
    pub storage: StorageHealth,
    pub attention: AttentionProjection,
    pub protection: ProtectionProjection,
    pub capabilities: GlobalCapabilities,
    pub mutation_authority: MutationAuthority,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MutationAuthority {
    pub namespace_token: String,
    pub minimum_reconciliation_window_millis: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentReclaim {
    pub event_token: String,
    pub incident_id: String,
    pub occurred_at_unix_millis: u64,
    pub process_outcome: ProcessOutcome,
    pub artifact_outcome: ArtifactOutcome,
    pub overall_outcome: OverallOutcome,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventSourceHealth {
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageHealth {
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_recovery: Option<StorageRecovery>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageRecovery {
    pub recovery_token: String,
    pub occurred_at_unix_millis: u64,
    pub reason_id: String,
    pub quarantined_sidecar_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttentionProjection {
    pub total_count: usize,
    pub items: Vec<AttentionItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    CleanupFailed,
    CleanupRevived,
    CleanupResidue,
    DaemonUnhealthy,
    EventSourceDegraded,
    StorageRecovered,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttentionItem {
    pub kind: AttentionKind,
    pub reason_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overall_outcome: Option<OverallOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_token: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtectionProjection {
    pub total_count: usize,
    pub items: Vec<ProtectionSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtectionSummary {
    pub incident_id: String,
    pub protected_at_unix_millis: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exact_observed_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact_absence_since_unix_millis: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GlobalCapabilities {
    pub pause: Capability,
    pub resume: Capability,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Capability {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason_id: Option<String>,
}

impl Capability {
    #[must_use]
    pub fn available() -> Self {
        Self {
            available: true,
            unavailable_reason_id: None,
        }
    }

    #[must_use]
    pub fn unavailable(reason_id: impl Into<String>) -> Self {
        Self {
            available: false,
            unavailable_reason_id: Some(reason_id.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IncidentState {
    Protected,
    Active,
    Cooling,
    Confirmed,
    Ambiguous,
    Reclaiming,
    Cleared,
    Revived,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOutcome {
    Cleared,
    Revived,
    Failed,
    DeliveryUnknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactOutcome {
    NotApplicable,
    Reconciled,
    Residue,
    DeliveryUnknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallOutcome {
    Cleared,
    ClearedWithResidue,
    Revived,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryEvent {
    pub event_token: String,
    pub incident_id: String,
    pub occurred_at_unix_millis: u64,
    pub state: IncidentState,
    pub payload: EventPayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum EventPayload {
    Observation { observation: Observation },
    Cleanup { cleanup: Cleanup },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Observation {
    pub family: String,
    pub family_version: String,
    pub state: IncidentState,
    pub executable_basename: String,
    pub member_count: usize,
    pub resident_memory_bytes: u64,
    pub roles: Vec<RoleCount>,
    pub evidence: Vec<Evidence>,
    pub gates: GateLedger,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoleCount {
    pub role: ProcessRole,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    Controller,
    BrowserRoot,
    Renderer,
    Gpu,
    Utility,
    BrowserHelper,
    CrashHandler,
    Recorder,
    IncidentMember,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Evidence {
    pub id: String,
    pub family: EvidenceFamily,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFamily {
    AutomationProvenance,
    Abandonment,
    Isolation,
    Protection,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GateLedger {
    pub same_user: bool,
    pub strong_automation_provenance: bool,
    pub confirmed_abandonment: bool,
    pub isolated_session: bool,
    pub stable_across_two_observations: bool,
    pub process_identity_unchanged: bool,
    pub no_protection_rule: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Cleanup {
    pub state: IncidentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_id: Option<String>,
    pub process_outcome: ProcessOutcome,
    pub artifact_outcome: ArtifactOutcome,
    pub overall_outcome: OverallOutcome,
    pub attention_required: bool,
    pub process_actions: Vec<ProcessAction>,
    pub artifact_actions: Vec<ArtifactAction>,
    pub survivor_count: usize,
    pub revival_checks_completed: usize,
    pub resources: CleanupResources,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessAction {
    pub action_token: String,
    pub stage: CleanupStage,
    pub signal: CleanupSignal,
    pub disposition: SignalDisposition,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStage {
    PrimaryTerm,
    MemberTerm,
    ExactKill,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupSignal {
    Term,
    Kill,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalDisposition {
    Delivered,
    AlreadyExited,
    IdentityMismatch,
    Rejected,
    CancelledBeforeDelivery,
    DeliveryUnknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactAction {
    pub action_token: String,
    pub kind: RuntimeArtifactKind,
    pub disposition: ArtifactDisposition,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeArtifactKind {
    DevToolsActivePort,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactDisposition {
    Removed,
    AlreadyAbsent,
    IdentityMismatch,
    Referenced,
    Unsafe,
    Rejected,
    CancelledBeforeDelivery,
    DeliveryUnknown,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupResources {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<ResourceSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<ResourceSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_reclaimed_memory_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceSnapshot {
    pub process_count: usize,
    pub resident_memory_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IncidentCapabilities {
    pub retry_failed_cleanup: Capability,
    pub protect_incident: Capability,
    pub unprotect_incident: Capability,
    pub export_diagnostics: Capability,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IncidentDetail {
    pub incident_id: String,
    pub events: Vec<HistoryEvent>,
    pub capabilities: IncidentCapabilities,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationFreshness {
    Current,
    ScanInProgress,
    StaleAfterFailure,
    NeverObserved,
}

/// The latest completed, owner-protection-adjusted observation snapshot. It is
/// read-only history-of-observation, not a cleanup work queue or a claim that
/// every item is still live when this response is read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservationRoster {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at_unix_millis: Option<u64>,
    pub freshness: ObservationFreshness,
    pub items: Vec<CurrentIncident>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CurrentIncident {
    pub incident_id: String,
    pub observation: Observation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticsBundle {
    pub document_schema_version: u32,
    pub generated_at_unix_millis: u64,
    pub status: PublicStatus,
    pub incident: IncidentDetail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppStateFixture {
    pub fixture_schema_version: u32,
    #[serde(flatten)]
    pub state: AppTransportState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AppTransportState {
    DaemonUnavailable {
        reason_id: String,
        suggested_action: String,
    },
    MutationDeliveryUncertain {
        command: String,
        reason_id: String,
        readback_command: String,
        automatic_retry: bool,
    },
    DaemonIncompatible {
        reason_id: String,
        suggested_action: String,
    },
    MutationUnresolved {
        command: String,
        reason_id: String,
        automatic_retry: bool,
        global_mutation_lock: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_surface_cannot_encode_service_lifecycle_authority() {
        let encoded = serde_json::to_string(&RequestEnvelope::new(7, Command::Status))
            .expect("encode public request");

        assert_eq!(
            encoded,
            r#"{"schema_version":3,"request_id":7,"command":{"command":"status"}}"#
        );
        for forbidden in [
            "arm",
            "disarm",
            "begin_drain",
            "instance_id",
            "generation",
            "epoch",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "public request leaked {forbidden}"
            );
        }
    }

    #[test]
    fn canonical_v3_frontend_fixtures_roundtrip_without_internal_fields() {
        macro_rules! fixture {
            ($name:literal) => {
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../apps/UnlingerApp/Contract/v3/",
                    $name,
                    ".json"
                ))
            };
        }
        let wire_fixtures = [
            fixture!("status-all-clear"),
            fixture!("status-report-only"),
            fixture!("status-enforce"),
            fixture!("status-scanning"),
            fixture!("status-starting"),
            fixture!("status-draining"),
            fixture!("status-failed"),
            fixture!("status-paused"),
            fixture!("status-recently-reclaimed"),
            fixture!("status-needs-attention"),
            fixture!("status-storage-recovered"),
            fixture!("history-cleared"),
            fixture!("history-cleared-with-residue"),
            fixture!("incident-protected"),
            fixture!("incident-revived"),
            fixture!("incident-failed"),
            fixture!("incidents-current"),
            fixture!("roster-current"),
            fixture!("roster-scanning"),
            fixture!("roster-stale"),
            fixture!("roster-never-observed"),
            fixture!("diagnostics"),
            fixture!("mutation-committed"),
            fixture!("mutation-not-found"),
            fixture!("mutation-authority-lost"),
        ];
        for source in wire_fixtures {
            let decoded: ResponseEnvelope =
                serde_json::from_str(source).expect("decode canonical wire fixture");
            assert_eq!(decoded.schema_version, SCHEMA_VERSION);
            let encoded = serde_json::to_string(&decoded).expect("encode canonical wire fixture");
            let reparsed: ResponseEnvelope =
                serde_json::from_str(&encoded).expect("reparse canonical wire fixture");
            assert_eq!(reparsed, decoded);
            for forbidden in [
                "\"pid\"",
                "\"instance_id\"",
                "\"activation_generation\"",
                "\"armed_generation\"",
                "\"enforcement_epoch\"",
                "\"requested_mode\"",
                "\"database_schema_version\"",
                "\"identity_fingerprint\"",
                "\"member_fingerprint\"",
                "\"artifact_fingerprint\"",
                "\"survivor_pids\"",
                "\"event_id\"",
                "\"attempt_id\"",
            ] {
                assert!(!encoded.contains(forbidden), "fixture leaked {forbidden}");
            }
        }

        for source in [
            fixture!("app-daemon-unavailable"),
            fixture!("app-mutation-delivery-uncertain"),
            fixture!("daemon-incompatible"),
            fixture!("app-mutation-unresolved"),
        ] {
            let decoded: AppStateFixture =
                serde_json::from_str(source).expect("decode canonical app-state fixture");
            let encoded = serde_json::to_string(&decoded).expect("encode app-state fixture");
            let reparsed: AppStateFixture =
                serde_json::from_str(&encoded).expect("reparse app-state fixture");
            assert_eq!(reparsed, decoded);
        }
    }
}
