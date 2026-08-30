use crate::{IncidentState, ProcessIdentity};
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
    pub tracking_key: String,
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
    #[serde(skip_serializing, default)]
    pub targets: Vec<ProcessTarget>,
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
}
