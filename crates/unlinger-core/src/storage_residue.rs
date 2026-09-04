use serde::{Deserialize, Serialize};

/// A storage-residue family is deliberately separate from process-session
/// runtime artifacts. Observing one never inherits process signal authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageResidueKind {
    ChromeCodeSignClone,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageResidueStatus {
    Clear,
    Detected,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageResidueReferenceCheck {
    Incomplete,
    CompleteNoReferences,
    Referenced,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageResidueObservation {
    pub kind: StorageResidueKind,
    pub status: StorageResidueStatus,
    pub observed_at_unix_millis: u64,
    pub candidate_count: usize,
    /// Sum of logical file sizes. APFS clones may share physical blocks, so
    /// this value is never presented as reclaimable disk space.
    pub logical_bytes: u64,
    pub shape_complete: bool,
    pub reference_check: StorageResidueReferenceCheck,
    pub automatic_cleanup_eligible: bool,
    pub reason_ids: Vec<String>,
}
