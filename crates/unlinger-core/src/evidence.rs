use crate::{IncidentState, ProcessIdentity, RuntimeArtifactCandidate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFamily {
    AutomationProvenance,
    Abandonment,
    Isolation,
    Protection,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceItem {
    pub id: String,
    pub family: EvidenceFamily,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_pid: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GateLedger {
    pub same_user: bool,
    pub strong_automation_provenance: bool,
    pub confirmed_abandonment: bool,
    pub isolated_session: bool,
    pub stable_across_two_observations: bool,
    pub process_identity_unchanged: bool,
    pub no_protection_rule: bool,
}

impl GateLedger {
    #[must_use]
    pub fn cleanup_eligible(&self) -> bool {
        self.same_user
            && self.strong_automation_provenance
            && self.confirmed_abandonment
            && self.isolated_session
            && self.stable_across_two_observations
            && self.process_identity_unchanged
            && self.no_protection_rule
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RootSummary {
    pub pid: u32,
    pub started_at_unix_micros: u64,
    pub executable_basename: String,
    pub identity_fingerprint: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserProduct {
    ChromeForTesting,
    Chromium,
    GoogleChrome,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserCompatibilityDecision {
    Automatic,
    ObserveOnly,
    Protected,
    Unknown,
}

/// Transient, typed browser coverage derived from native bundle facts and the
/// active signature-pack policy. It is deliberately excluded from persisted
/// incident/history JSON; schema-v4 public projection reads it only from the
/// current in-memory roster.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserCompatibility {
    pub product: BrowserProduct,
    pub observed_version: Option<String>,
    pub decision: BrowserCompatibilityDecision,
    pub reason_id: Option<String>,
}

impl Default for BrowserCompatibility {
    fn default() -> Self {
        Self {
            product: BrowserProduct::Unknown,
            observed_version: None,
            decision: BrowserCompatibilityDecision::Unknown,
            reason_id: Some("compatibility.browser_facts_unavailable".to_owned()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessTarget {
    pub identity: ProcessIdentity,
    pub process_group_id: u32,
    pub role: ProcessRole,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessRoleCount {
    pub role: ProcessRole,
    pub count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IncidentReport {
    pub incident_id: String,
    #[serde(skip_serializing, default)]
    pub tracking_key: String,
    #[serde(skip_serializing, default)]
    pub session_fingerprint: String,
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
    #[serde(skip, default)]
    pub browser_compatibility: BrowserCompatibility,
    #[serde(skip_serializing, default)]
    pub targets: Vec<ProcessTarget>,
    #[serde(skip, default)]
    pub runtime_artifacts: Vec<RuntimeArtifactCandidate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hard_gate_is_required() {
        let mut gates = GateLedger {
            same_user: true,
            strong_automation_provenance: true,
            confirmed_abandonment: true,
            isolated_session: true,
            stable_across_two_observations: true,
            process_identity_unchanged: true,
            no_protection_rule: true,
        };
        assert!(gates.cleanup_eligible());

        gates.process_identity_unchanged = false;
        assert!(!gates.cleanup_eligible());
    }

    #[test]
    fn incident_json_omits_internal_tracking_and_session_identifiers() {
        let report = IncidentReport {
            incident_id: "inc-redacted".to_owned(),
            tracking_key: "trk-private-internal".to_owned(),
            session_fingerprint: "ses-private-internal".to_owned(),
            signature_pack: "playwright".to_owned(),
            signature_version: "0.1.0".to_owned(),
            state: IncidentState::Ambiguous,
            root: RootSummary {
                pid: 42,
                started_at_unix_micros: 1,
                executable_basename: "browser".to_owned(),
                identity_fingerprint: "identity-redacted".to_owned(),
            },
            member_count: 1,
            resident_memory_bytes: 1,
            member_fingerprint: "members-redacted".to_owned(),
            roles: Vec::new(),
            evidence: Vec::new(),
            gates: GateLedger::default(),
            browser_compatibility: BrowserCompatibility::default(),
            targets: Vec::new(),
            runtime_artifacts: Vec::new(),
        };

        let json = serde_json::to_string(&report).expect("incident JSON");
        assert!(!json.contains("trk-private-internal"));
        assert!(!json.contains("ses-private-internal"));
        assert!(!json.contains("tracking_key"));
        assert!(!json.contains("session_fingerprint"));

        let decoded: IncidentReport = serde_json::from_str(&json).expect("backward decode");
        assert!(decoded.tracking_key.is_empty());
        assert!(decoded.session_fingerprint.is_empty());
    }
}
