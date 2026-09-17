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

/// Public-safe facts describing one bounded automatic storage cleanup attempt.
///
/// These facts deliberately exclude candidate directory names, absolute paths,
/// argv, user names and raw candidate identities, so they may be persisted and
/// projected to frontends. Logical bytes are never physical APFS reclaim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCleanupAttemptFacts {
    /// Candidates the scanner planned to remove this attempt.
    pub planned_candidate_count: usize,
    /// Candidates observed immediately before the attempt.
    pub before_candidate_count: usize,
    /// Sum of logical file sizes observed immediately before the attempt.
    pub before_logical_bytes: u64,
}

/// Terminal disposition of one bounded storage cleanup attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageCleanupDisposition {
    /// Every planned candidate was proved absent and no residue remains.
    Complete,
    /// Every planned candidate was proved absent but unplanned residue remains.
    Partial,
    /// Removal was attempted but its result could not be proved complete.
    Failed,
    /// A durable PREPARED attempt was never finished by the process that
    /// created it, so delivery can never be inferred from later observations.
    DeliveryUnknown,
}

impl StorageCleanupDisposition {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::DeliveryUnknown => "delivery_unknown",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "complete" => Some(Self::Complete),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            "delivery_unknown" => Some(Self::DeliveryUnknown),
            _ => None,
        }
    }
}

/// Terminal result of one bounded storage cleanup attempt.
///
/// The after-observation fields are absent when the immediate rescan could not
/// prove the clone shape, because an unproved rescan never authorizes a claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCleanupResultFacts {
    pub disposition: StorageCleanupDisposition,
    pub planned_candidate_count: usize,
    pub before_candidate_count: usize,
    pub before_logical_bytes: u64,
    /// Planned candidates the immediate rescan proved absent, when provable.
    pub removed_candidate_count: Option<usize>,
    /// Candidates present after the attempt, when the rescan proved shape.
    pub after_candidate_count: Option<usize>,
    /// Logical bytes after the attempt, when the rescan proved shape.
    pub after_logical_bytes: Option<u64>,
    /// Candidates still present that were never part of the planned set.
    pub retained_not_planned_count: Option<usize>,
}
