use crate::{
    ArtifactAction, ArtifactActionIntent, ArtifactDisposition, ArtifactFreeze,
    FrozenRuntimeArtifact, IncidentReport, IncidentState, ProcessIdentity, ProcessRecord,
    ProcessRole, ProcessTarget, RuntimeArtifactCandidate, Snapshot, fingerprint_process_identity,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Duration;

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
    /// Recovery projection for an action left PREPARED across a process crash.
    /// A live signal adapter must never manufacture this disposition.
    DeliveryUnknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStage {
    PrimaryTerm,
    MemberTerm,
    ExactKill,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupActionIntent {
    pub stage: CleanupStage,
    pub pid: u32,
    pub identity_fingerprint: String,
    pub signal: CleanupSignal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupAction {
    pub stage: CleanupStage,
    pub pid: u32,
    pub identity_fingerprint: String,
    pub signal: CleanupSignal,
    pub disposition: SignalDisposition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupReceipt {
    pub incident_id: String,
    pub state: IncidentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_id: Option<String>,
    pub actions: Vec<CleanupAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_actions: Vec<ArtifactAction>,
    pub survivor_pids: Vec<u32>,
    pub revival_checks_completed: usize,
    #[serde(default)]
    pub resources: CleanupResources,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupOutcome {
    pub process: ProcessOutcome,
    pub artifact: ArtifactOutcome,
    pub overall: OverallOutcome,
    pub attention_required: bool,
}

impl CleanupReceipt {
    /// Absence is a result, not proof that Unlinger caused it. Attribution
    /// requires a delivered process action as well as the terminal proof.
    #[must_use]
    pub fn proves_process_reclaim(&self) -> bool {
        self.outcome().process == ProcessOutcome::Cleared
            && self
                .actions
                .iter()
                .any(|action| action.disposition == SignalDisposition::Delivered)
    }

    #[must_use]
    pub fn ended_without_intervention(&self) -> bool {
        self.state == IncidentState::Cleared
            && self.outcome().overall == OverallOutcome::Cleared
            && !self.proves_process_reclaim()
            && self.artifact_actions.is_empty()
    }

    /// Projects the independent process and runtime-artifact facts from both
    /// current and retained pre-projection receipts. The persisted incident
    /// state remains the whole frozen-plan execution state, while this view
    /// prevents an artifact residue from erasing a proved process-tree result.
    #[must_use]
    pub fn outcome(&self) -> CleanupOutcome {
        let reason = self.reason_id.as_deref();
        let signal_delivery_unknown = self
            .actions
            .iter()
            .any(|action| action.disposition == SignalDisposition::DeliveryUnknown);
        let artifact_delivery_unknown = self
            .artifact_actions
            .iter()
            .any(|action| action.disposition == ArtifactDisposition::DeliveryUnknown)
            || reason == Some("cleanup.artifact_delivery_unknown");
        let artifact_residue_reason = matches!(
            reason,
            Some(
                "cleanup.artifact_identity_changed"
                    | "cleanup.artifact_live_reference"
                    | "cleanup.artifact_unsafe"
                    | "cleanup.artifact_rejected"
            )
        );

        let process = if signal_delivery_unknown {
            ProcessOutcome::DeliveryUnknown
        } else if self.state == IncidentState::Revived {
            ProcessOutcome::Revived
        } else if self.state == IncidentState::Cleared
            || artifact_residue_reason
            || artifact_delivery_unknown
        {
            ProcessOutcome::Cleared
        } else {
            ProcessOutcome::Failed
        };

        let artifact = if artifact_delivery_unknown {
            ArtifactOutcome::DeliveryUnknown
        } else if artifact_residue_reason
            || self
                .artifact_actions
                .iter()
                .any(|action| !action.disposition.completed_cleanup())
        {
            ArtifactOutcome::Residue
        } else if self.artifact_actions.is_empty() {
            ArtifactOutcome::NotApplicable
        } else {
            ArtifactOutcome::Reconciled
        };

        let overall = match (process, artifact) {
            (ProcessOutcome::DeliveryUnknown, _)
            | (_, ArtifactOutcome::DeliveryUnknown)
            | (ProcessOutcome::Failed, _) => OverallOutcome::Failed,
            (
                ProcessOutcome::Cleared,
                ArtifactOutcome::NotApplicable | ArtifactOutcome::Reconciled,
            ) => OverallOutcome::Cleared,
            (ProcessOutcome::Cleared, ArtifactOutcome::Residue) => {
                OverallOutcome::ClearedWithResidue
            }
            (ProcessOutcome::Revived, _) => OverallOutcome::Revived,
        };

        CleanupOutcome {
            process,
            artifact,
            overall,
            attention_required: overall != OverallOutcome::Cleared,
        }
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupPlan {
    pub incident_id: String,
    pub tracking_key: String,
    pub session_fingerprint: String,
    pub signature_pack: String,
    pub signature_version: String,
    pub root_identity: ProcessIdentity,
    pub member_fingerprint: String,
    pub targets: Vec<ProcessTarget>,
    pub artifacts: Vec<RuntimeArtifactCandidate>,
}

impl CleanupPlan {
    pub fn from_confirmed(report: &IncidentReport) -> Result<Self, CleanupPlanError> {
        if report.state != IncidentState::Confirmed || !report.gates.cleanup_eligible() {
            return Err(CleanupPlanError::NotConfirmed);
        }
        if report.targets.is_empty() {
            return Err(CleanupPlanError::NoTargets);
        }
        if report.session_fingerprint.trim().is_empty() {
            return Err(CleanupPlanError::MissingSessionFingerprint);
        }

        let mut pids = BTreeSet::new();
        for target in &report.targets {
            if !pids.insert(target.identity.pid) {
                return Err(CleanupPlanError::DuplicateTarget(target.identity.pid));
            }
            if target.identity.executable_device.is_none()
                || target.identity.executable_inode.is_none()
            {
                return Err(CleanupPlanError::IncompleteIdentity(target.identity.pid));
            }
        }
        if report.runtime_artifacts.len() > 1 {
            return Err(CleanupPlanError::TooManyArtifacts);
        }
        let mut artifact_fingerprints = BTreeSet::new();
        for artifact in &report.runtime_artifacts {
            if artifact.session_fingerprint() != report.session_fingerprint {
                return Err(CleanupPlanError::ArtifactSessionMismatch);
            }
            if !artifact_fingerprints.insert(artifact.artifact_fingerprint()) {
                return Err(CleanupPlanError::DuplicateArtifact);
            }
        }
        let root_identity = report
            .targets
            .iter()
            .find(|target| target.identity.pid == report.root.pid)
            .map(|target| target.identity.clone())
            .ok_or(CleanupPlanError::RootMissing)?;
        if !report.targets.iter().any(|target| {
            matches!(
                target.role,
                ProcessRole::Controller | ProcessRole::BrowserRoot
            )
        }) {
            return Err(CleanupPlanError::NoPrimaryTarget);
        }

        Ok(Self {
            incident_id: report.incident_id.clone(),
            tracking_key: report.tracking_key.clone(),
            session_fingerprint: report.session_fingerprint.clone(),
            signature_pack: report.signature_pack.clone(),
            signature_version: report.signature_version.clone(),
            root_identity,
            member_fingerprint: report.member_fingerprint.clone(),
            targets: report.targets.clone(),
            artifacts: report.runtime_artifacts.clone(),
        })
    }

    #[must_use]
    pub fn primary_targets(&self) -> Vec<&ProcessTarget> {
        let controllers = self
            .targets
            .iter()
            .filter(|target| target.role == ProcessRole::Controller)
            .collect::<Vec<_>>();
        if controllers.is_empty() {
            self.targets
                .iter()
                .filter(|target| target.role == ProcessRole::BrowserRoot)
                .collect()
        } else {
            controllers
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupPlanError {
    NotConfirmed,
    NoTargets,
    MissingSessionFingerprint,
    DuplicateTarget(u32),
    IncompleteIdentity(u32),
    RootMissing,
    NoPrimaryTarget,
    TooManyArtifacts,
    DuplicateArtifact,
    ArtifactSessionMismatch,
}

impl Display for CleanupPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfirmed => write!(formatter, "incident is not cleanup-eligible CONFIRMED"),
            Self::NoTargets => write!(formatter, "incident has no frozen targets"),
            Self::MissingSessionFingerprint => {
                write!(formatter, "incident has no revival/session fingerprint")
            }
            Self::DuplicateTarget(pid) => write!(formatter, "duplicate target PID {pid}"),
            Self::IncompleteIdentity(pid) => {
                write!(formatter, "target PID {pid} has incomplete identity")
            }
            Self::RootMissing => write!(formatter, "incident root is missing from frozen targets"),
            Self::NoPrimaryTarget => {
                write!(
                    formatter,
                    "incident has no controller or browser root target"
                )
            }
            Self::TooManyArtifacts => {
                write!(formatter, "incident has more than one 0.1 runtime artifact")
            }
            Self::DuplicateArtifact => write!(formatter, "incident has a duplicate artifact"),
            Self::ArtifactSessionMismatch => {
                write!(
                    formatter,
                    "runtime artifact does not belong to the incident session"
                )
            }
        }
    }
}

impl Error for CleanupPlanError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupPolicy {
    pub primary_term_grace: Duration,
    pub member_term_grace: Duration,
    pub kill_grace: Duration,
    pub revival_windows: Vec<Duration>,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            primary_term_grace: Duration::from_secs(3),
            member_term_grace: Duration::from_secs(3),
            kill_grace: Duration::from_secs(1),
            revival_windows: vec![Duration::from_secs(15), Duration::from_secs(60)],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeFailure {
    message: String,
}

impl RuntimeFailure {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for RuntimeFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RuntimeFailure {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClockSample {
    pub wall_unix_millis: u64,
    pub continuous_millis: u64,
    pub boot_session_fingerprint: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitOutcome {
    DeadlineReached,
    Interrupted,
}

pub trait CleanupRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure>;
    /// Resolve one PID at the action boundary. `None` proves that PID is no
    /// longer live; a live process whose native identity cannot be read must
    /// return an error rather than being projected as absent.
    fn lookup_process(&mut self, pid: u32) -> Result<Option<ProcessRecord>, RuntimeFailure>;
    fn clock_sample(&self) -> Result<ClockSample, RuntimeFailure>;
    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition;
    fn wait_until(
        &mut self,
        duration: Duration,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<WaitOutcome, RuntimeFailure>;

    fn freeze_artifact(
        &mut self,
        _candidate: &RuntimeArtifactCandidate,
    ) -> Result<ArtifactFreeze, RuntimeFailure> {
        Err(RuntimeFailure::new(
            "runtime artifact adapter is unavailable",
        ))
    }

    fn remove_artifact_exact(&mut self, _artifact: &FrozenRuntimeArtifact) -> ArtifactDisposition {
        ArtifactDisposition::Rejected
    }
}

pub trait CleanupActionJournal {
    /// Persist PREPARED and return its durable action identity before this call returns.
    fn prepare_action(
        &mut self,
        intent: &CleanupActionIntent,
        prepared_at_unix_millis: u64,
    ) -> Result<String, RuntimeFailure>;

    /// Persist the delivery disposition before this call returns.
    fn complete_action(
        &mut self,
        action_id: &str,
        disposition: SignalDisposition,
        completed_at_unix_millis: u64,
    ) -> Result<(), RuntimeFailure>;

    /// Persist an artifact PREPARED row before any unlink attempt. Implementations
    /// without an artifact journal fail closed when an artifact is actually present.
    fn prepare_artifact_action(
        &mut self,
        _intent: &ArtifactActionIntent,
        _prepared_at_unix_millis: u64,
    ) -> Result<String, RuntimeFailure> {
        Err(RuntimeFailure::new(
            "runtime artifact action journal is unavailable",
        ))
    }

    /// Persist the terminal unlink disposition before returning success.
    fn complete_artifact_action(
        &mut self,
        _action_id: &str,
        _disposition: ArtifactDisposition,
        _completed_at_unix_millis: u64,
    ) -> Result<(), RuntimeFailure> {
        Err(RuntimeFailure::new(
            "runtime artifact action journal is unavailable",
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevalidationPhase {
    BeforeSignal,
    RevivalCheck,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevalidationStatus {
    Eligible,
    Gone,
    Blocked,
    Revived,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Revalidation {
    pub status: RevalidationStatus,
    pub reason_id: String,
}

pub trait IncidentRevalidator {
    fn revalidate(
        &self,
        snapshot: &Snapshot,
        plan: &CleanupPlan,
        phase: RevalidationPhase,
    ) -> Revalidation;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CleanupError {
    Runtime {
        error: RuntimeFailure,
        terminal_receipt: Box<CleanupReceipt>,
    },
    AttemptOpen {
        error: RuntimeFailure,
        partial_receipt: Box<CleanupReceipt>,
        prepared_action_id: String,
    },
}

impl CleanupError {
    #[must_use]
    pub fn terminal_receipt(&self) -> Option<&CleanupReceipt> {
        match self {
            Self::Runtime {
                terminal_receipt, ..
            } => Some(terminal_receipt),
            Self::AttemptOpen { .. } => None,
        }
    }

    #[must_use]
    pub fn partial_receipt(&self) -> &CleanupReceipt {
        match self {
            Self::Runtime {
                terminal_receipt, ..
            } => terminal_receipt,
            Self::AttemptOpen {
                partial_receipt, ..
            } => partial_receipt,
        }
    }

    #[must_use]
    pub fn prepared_action_id(&self) -> Option<&str> {
        match self {
            Self::Runtime { .. } => None,
            Self::AttemptOpen {
                prepared_action_id, ..
            } => Some(prepared_action_id),
        }
    }
}

impl Display for CleanupError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runtime { error, .. } => write!(formatter, "cleanup runtime failed: {error}"),
            Self::AttemptOpen { error, .. } => {
                write!(formatter, "cleanup action remains open: {error}")
            }
        }
    }
}

impl Error for CleanupError {}

pub struct CleanupExecutor;

impl CleanupExecutor {
    pub fn execute<R: CleanupRuntime, V: IncidentRevalidator, J: CleanupActionJournal>(
        runtime: &mut R,
        revalidator: &V,
        journal: &mut J,
        should_stop: &mut dyn FnMut() -> bool,
        plan: &CleanupPlan,
        policy: &CleanupPolicy,
    ) -> Result<CleanupReceipt, CleanupError> {
        let mut receipt = CleanupReceipt {
            incident_id: plan.incident_id.clone(),
            state: IncidentState::Reclaiming,
            reason_id: None,
            actions: Vec::new(),
            artifact_actions: Vec::new(),
            survivor_pids: Vec::new(),
            revival_checks_completed: 0,
            resources: CleanupResources::default(),
        };
        let mut term_attempted = BTreeSet::new();

        if should_stop() {
            return Ok(fail(
                receipt,
                "cleanup.drain_requested".to_owned(),
                Vec::new(),
            ));
        }
        let initial = snapshot_or_receipt_error(runtime, &receipt)?;
        receipt.resources.before = Some(resource_snapshot(&initial, plan));
        let mut survivors = match validate_stage(runtime, &initial, revalidator, plan) {
            Ok(survivors) => survivors,
            Err(reason) => return Ok(fail(receipt, reason, Vec::new())),
        };
        let mut frozen_artifacts = Vec::with_capacity(plan.artifacts.len());
        for candidate in &plan.artifacts {
            let frozen = runtime
                .freeze_artifact(candidate)
                .map_err(|error| terminal_runtime_error(error, &receipt))?;
            frozen_artifacts.push((candidate.clone(), frozen));
        }

        if !survivors.is_empty() {
            let primary_pids = plan
                .primary_targets()
                .into_iter()
                .map(|target| target.identity.pid)
                .collect::<BTreeSet<_>>();
            for target in &plan.targets {
                if survivors.contains_key(&target.identity.pid)
                    && primary_pids.contains(&target.identity.pid)
                {
                    if let Some(reason) = signal(
                        SignalContext {
                            runtime,
                            revalidator,
                            journal,
                            should_stop,
                            receipt: &mut receipt,
                            plan,
                        },
                        target,
                        CleanupStage::PrimaryTerm,
                        CleanupSignal::Term,
                    )? {
                        return Ok(fail(receipt, reason, survivors.into_keys().collect()));
                    }
                    term_attempted.insert(target.identity.pid);
                }
            }
            if wait_interrupted(runtime, should_stop, policy.primary_term_grace, &receipt)? {
                return Ok(fail(
                    receipt,
                    "cleanup.drain_requested".to_owned(),
                    survivors.into_keys().collect(),
                ));
            }
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            survivors = match validate_stage(runtime, &snapshot, revalidator, plan) {
                Ok(survivors) => survivors,
                Err(reason) => return Ok(fail(receipt, reason, Vec::new())),
            };
        }

        if !survivors.is_empty() {
            for target in &plan.targets {
                if survivors.contains_key(&target.identity.pid)
                    && !term_attempted.contains(&target.identity.pid)
                {
                    if let Some(reason) = signal(
                        SignalContext {
                            runtime,
                            revalidator,
                            journal,
                            should_stop,
                            receipt: &mut receipt,
                            plan,
                        },
                        target,
                        CleanupStage::MemberTerm,
                        CleanupSignal::Term,
                    )? {
                        return Ok(fail(receipt, reason, survivors.into_keys().collect()));
                    }
                    term_attempted.insert(target.identity.pid);
                }
            }
            if wait_interrupted(runtime, should_stop, policy.member_term_grace, &receipt)? {
                return Ok(fail(
                    receipt,
                    "cleanup.drain_requested".to_owned(),
                    survivors.into_keys().collect(),
                ));
            }
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            survivors = match validate_stage(runtime, &snapshot, revalidator, plan) {
                Ok(survivors) => survivors,
                Err(reason) => return Ok(fail(receipt, reason, Vec::new())),
            };
        }

        if !survivors.is_empty() {
            for target in &plan.targets {
                if survivors.contains_key(&target.identity.pid) {
                    if !term_attempted.contains(&target.identity.pid) {
                        return Ok(fail(
                            receipt,
                            "cleanup.kill_without_term".to_owned(),
                            survivors.into_keys().collect(),
                        ));
                    }
                    if let Some(reason) = signal(
                        SignalContext {
                            runtime,
                            revalidator,
                            journal,
                            should_stop,
                            receipt: &mut receipt,
                            plan,
                        },
                        target,
                        CleanupStage::ExactKill,
                        CleanupSignal::Kill,
                    )? {
                        return Ok(fail(receipt, reason, survivors.into_keys().collect()));
                    }
                }
            }
            if wait_interrupted(runtime, should_stop, policy.kill_grace, &receipt)? {
                return Ok(fail(
                    receipt,
                    "cleanup.drain_requested".to_owned(),
                    survivors.into_keys().collect(),
                ));
            }
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            survivors = match observe_exact_target_liveness(runtime, &snapshot, plan) {
                Ok(observation) => observation.survivors,
                Err(reason) => return Ok(fail(receipt, reason, Vec::new())),
            };
            if !survivors.is_empty() {
                return Ok(fail(
                    receipt,
                    "cleanup.survivors_after_kill".to_owned(),
                    survivors.into_keys().collect(),
                ));
            }
        }

        let mut terminal_snapshot = None;
        for window in &policy.revival_windows {
            if wait_interrupted(runtime, should_stop, *window, &receipt)? {
                return Ok(fail(
                    receipt,
                    "cleanup.drain_requested".to_owned(),
                    Vec::new(),
                ));
            }
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            receipt.revival_checks_completed += 1;
            let validation =
                revalidator.revalidate(&snapshot, plan, RevalidationPhase::RevivalCheck);
            match validation.status {
                RevalidationStatus::Gone => terminal_snapshot = Some(snapshot),
                RevalidationStatus::Revived | RevalidationStatus::Eligible => {
                    set_resource_after(&mut receipt, resource_snapshot(&snapshot, plan));
                    receipt.state = IncidentState::Revived;
                    receipt.reason_id = Some(validation.reason_id);
                    return Ok(receipt);
                }
                RevalidationStatus::Blocked => {
                    set_resource_after(&mut receipt, resource_snapshot(&snapshot, plan));
                    return Ok(fail(receipt, validation.reason_id, Vec::new()));
                }
            }
        }

        let terminal_snapshot = match terminal_snapshot {
            Some(snapshot) => snapshot,
            None => snapshot_or_receipt_error(runtime, &receipt)?,
        };
        let terminal_liveness =
            match observe_exact_target_liveness(runtime, &terminal_snapshot, plan) {
                Ok(observation) => observation,
                Err(reason) => return Ok(fail(receipt, reason, Vec::new())),
            };
        set_resource_after(&mut receipt, resource_snapshot(&terminal_snapshot, plan));
        match terminal_revalidation(revalidator, &terminal_snapshot, plan) {
            TerminalRevalidation::Gone => {}
            TerminalRevalidation::Revived(reason) => {
                receipt.state = IncidentState::Revived;
                receipt.reason_id = Some(reason);
                return Ok(receipt);
            }
            TerminalRevalidation::Blocked(reason) => {
                return Ok(fail(receipt, reason, Vec::new()));
            }
        }
        if !terminal_liveness.survivors.is_empty() {
            return Ok(fail(
                receipt,
                "cleanup.terminal_tree_not_gone".to_owned(),
                terminal_liveness.survivors.into_keys().collect(),
            ));
        }

        for (candidate, frozen) in frozen_artifacts {
            let disposition = execute_artifact_action(
                runtime,
                journal,
                should_stop,
                &mut receipt,
                &candidate,
                frozen,
            )?;
            if !disposition.completed_cleanup() {
                return Ok(fail(
                    receipt,
                    artifact_failure_reason(disposition).to_owned(),
                    Vec::new(),
                ));
            }
        }

        if !plan.artifacts.is_empty() {
            let post_artifact = snapshot_or_receipt_error(runtime, &receipt)?;
            let post_artifact_liveness =
                match observe_exact_target_liveness(runtime, &post_artifact, plan) {
                    Ok(observation) => observation,
                    Err(reason) => return Ok(fail(receipt, reason, Vec::new())),
                };
            set_resource_after(&mut receipt, resource_snapshot(&post_artifact, plan));
            match terminal_revalidation(revalidator, &post_artifact, plan) {
                TerminalRevalidation::Gone => {}
                TerminalRevalidation::Revived(reason) => {
                    receipt.state = IncidentState::Revived;
                    receipt.reason_id = Some(reason);
                    return Ok(receipt);
                }
                TerminalRevalidation::Blocked(reason) => {
                    return Ok(fail(receipt, reason, Vec::new()));
                }
            }
            if !post_artifact_liveness.survivors.is_empty() {
                return Ok(fail(
                    receipt,
                    "cleanup.post_artifact_tree_not_gone".to_owned(),
                    post_artifact_liveness.survivors.into_keys().collect(),
                ));
            }
        }

        receipt.state = IncidentState::Cleared;
        receipt.reason_id = Some(if receipt.ended_without_intervention() {
            receipt.resources.estimated_reclaimed_memory_bytes = None;
            "cleanup.tree_gone_without_signal".to_owned()
        } else if receipt.artifact_actions.is_empty() {
            "cleanup.tree_gone_no_revival".to_owned()
        } else {
            "cleanup.tree_gone_artifacts_reconciled".to_owned()
        });
        Ok(receipt)
    }
}

enum TerminalRevalidation {
    Gone,
    Revived(String),
    Blocked(String),
}

fn terminal_revalidation<V: IncidentRevalidator>(
    revalidator: &V,
    snapshot: &Snapshot,
    plan: &CleanupPlan,
) -> TerminalRevalidation {
    let validation = revalidator.revalidate(snapshot, plan, RevalidationPhase::RevivalCheck);
    match validation.status {
        RevalidationStatus::Gone => TerminalRevalidation::Gone,
        RevalidationStatus::Revived | RevalidationStatus::Eligible => {
            TerminalRevalidation::Revived(validation.reason_id)
        }
        RevalidationStatus::Blocked => TerminalRevalidation::Blocked(validation.reason_id),
    }
}

fn execute_artifact_action<R: CleanupRuntime, J: CleanupActionJournal>(
    runtime: &mut R,
    journal: &mut J,
    should_stop: &mut dyn FnMut() -> bool,
    receipt: &mut CleanupReceipt,
    candidate: &RuntimeArtifactCandidate,
    frozen: ArtifactFreeze,
) -> Result<ArtifactDisposition, CleanupError> {
    let intent = candidate.intent();
    if should_stop() {
        return Ok(ArtifactDisposition::CancelledBeforeDelivery);
    }
    // An artifact that was absent when the cleanup plan was frozen is not an
    // authorization to remove a file which appears later. Re-check before the
    // durable action boundary and classify any late candidate without touching
    // it.
    let absent_revalidation = match frozen {
        ArtifactFreeze::Absent => Some(
            runtime
                .freeze_artifact(candidate)
                .map_err(|error| terminal_runtime_error(error, receipt))?,
        ),
        ArtifactFreeze::Frozen(_) | ArtifactFreeze::Unsafe => None,
    };
    let prepared_at = runtime
        .clock_sample()
        .map_err(|error| terminal_runtime_error(error, receipt))?
        .wall_unix_millis;
    let action_id = journal
        .prepare_artifact_action(&intent, prepared_at)
        .map_err(|error| terminal_runtime_error(error, receipt))?;
    let disposition = if should_stop() {
        ArtifactDisposition::CancelledBeforeDelivery
    } else {
        match frozen {
            ArtifactFreeze::Frozen(frozen) => runtime.remove_artifact_exact(&frozen),
            ArtifactFreeze::Absent => match absent_revalidation {
                Some(ArtifactFreeze::Absent) => ArtifactDisposition::AlreadyAbsent,
                Some(ArtifactFreeze::Frozen(_)) => ArtifactDisposition::IdentityMismatch,
                Some(ArtifactFreeze::Unsafe) => ArtifactDisposition::Unsafe,
                None => unreachable!("absent artifacts are revalidated before journalling"),
            },
            ArtifactFreeze::Unsafe => ArtifactDisposition::Unsafe,
        }
    };
    complete_artifact_prepared(runtime, journal, receipt, &intent, &action_id, disposition)?;
    if !disposition.completed_cleanup() {
        return Ok(disposition);
    }

    // Removal/absence is not complete until the canonical path is proved
    // absent after the action. A replacement is deliberately left untouched:
    // it was never part of the frozen cleanup plan.
    let final_freeze = runtime
        .freeze_artifact(candidate)
        .map_err(|error| terminal_runtime_error(error, receipt))?;
    Ok(match final_freeze {
        ArtifactFreeze::Absent => disposition,
        ArtifactFreeze::Frozen(_) => ArtifactDisposition::IdentityMismatch,
        ArtifactFreeze::Unsafe => ArtifactDisposition::Unsafe,
    })
}

fn complete_artifact_prepared<R: CleanupRuntime, J: CleanupActionJournal>(
    runtime: &R,
    journal: &mut J,
    receipt: &mut CleanupReceipt,
    intent: &ArtifactActionIntent,
    action_id: &str,
    disposition: ArtifactDisposition,
) -> Result<(), CleanupError> {
    let completed_at = runtime
        .clock_sample()
        .map_err(|error| CleanupError::AttemptOpen {
            error,
            partial_receipt: Box::new(receipt.clone()),
            prepared_action_id: action_id.to_owned(),
        })?
        .wall_unix_millis;
    journal
        .complete_artifact_action(action_id, disposition, completed_at)
        .map_err(|error| CleanupError::AttemptOpen {
            error,
            partial_receipt: Box::new(receipt.clone()),
            prepared_action_id: action_id.to_owned(),
        })?;
    receipt.artifact_actions.push(ArtifactAction {
        kind: intent.kind,
        artifact_fingerprint: intent.artifact_fingerprint.clone(),
        disposition,
    });
    Ok(())
}

fn artifact_failure_reason(disposition: ArtifactDisposition) -> &'static str {
    match disposition {
        ArtifactDisposition::Removed | ArtifactDisposition::AlreadyAbsent => {
            "cleanup.artifact_reconciled"
        }
        ArtifactDisposition::IdentityMismatch => "cleanup.artifact_identity_changed",
        ArtifactDisposition::Referenced => "cleanup.artifact_live_reference",
        ArtifactDisposition::Unsafe => "cleanup.artifact_unsafe",
        ArtifactDisposition::Rejected => "cleanup.artifact_rejected",
        ArtifactDisposition::CancelledBeforeDelivery => "cleanup.drain_requested",
        ArtifactDisposition::DeliveryUnknown => "cleanup.artifact_delivery_unknown",
    }
}

fn resource_snapshot(snapshot: &Snapshot, plan: &CleanupPlan) -> ResourceSnapshot {
    let identities = plan
        .targets
        .iter()
        .map(|target| (target.identity.pid, &target.identity))
        .collect::<BTreeMap<_, _>>();
    let matching = snapshot.processes.iter().filter(|process| {
        identities
            .get(&process.pid())
            .is_some_and(|identity| identity.exact_match(&process.identity))
    });
    let mut process_count = 0_usize;
    let mut resident_memory_bytes = 0_u64;
    for process in matching {
        process_count += 1;
        resident_memory_bytes = resident_memory_bytes.saturating_add(process.resident_memory_bytes);
    }
    ResourceSnapshot {
        process_count,
        resident_memory_bytes,
    }
}

fn set_resource_after(receipt: &mut CleanupReceipt, after: ResourceSnapshot) {
    receipt.resources.estimated_reclaimed_memory_bytes =
        receipt.resources.before.as_ref().and_then(|before| {
            before
                .resident_memory_bytes
                .checked_sub(after.resident_memory_bytes)
        });
    receipt.resources.after = Some(after);
}

fn snapshot_or_receipt_error<R: CleanupRuntime>(
    runtime: &mut R,
    receipt: &CleanupReceipt,
) -> Result<Snapshot, CleanupError> {
    runtime.snapshot().map_err(|error| {
        let mut failed = receipt.clone();
        failed.state = IncidentState::Failed;
        failed.reason_id = Some("cleanup.runtime_failure".to_owned());
        CleanupError::Runtime {
            error,
            terminal_receipt: Box::new(failed),
        }
    })
}

fn wait_interrupted<R: CleanupRuntime>(
    runtime: &mut R,
    should_stop: &mut dyn FnMut() -> bool,
    duration: Duration,
    receipt: &CleanupReceipt,
) -> Result<bool, CleanupError> {
    if should_stop() {
        return Ok(true);
    }
    match runtime.wait_until(duration, should_stop) {
        Ok(WaitOutcome::DeadlineReached) => Ok(should_stop()),
        Ok(WaitOutcome::Interrupted) => Ok(true),
        Err(error) => Err(terminal_runtime_error(error, receipt)),
    }
}

fn terminal_runtime_error(error: RuntimeFailure, receipt: &CleanupReceipt) -> CleanupError {
    let mut failed = receipt.clone();
    failed.state = IncidentState::Failed;
    failed.reason_id = Some("cleanup.runtime_failure".to_owned());
    CleanupError::Runtime {
        error,
        terminal_receipt: Box::new(failed),
    }
}

fn validate_stage<R: CleanupRuntime, V: IncidentRevalidator>(
    runtime: &mut R,
    snapshot: &Snapshot,
    revalidator: &V,
    plan: &CleanupPlan,
) -> Result<BTreeMap<u32, ProcessIdentity>, String> {
    let observation = observe_exact_target_liveness(runtime, snapshot, plan)?;
    if !observation.missing_live_targets.is_empty() {
        return Err("cleanup.snapshot_target_inconsistent".to_owned());
    }
    if observation.survivors.is_empty() {
        return Ok(observation.survivors);
    }
    let validation = revalidator.revalidate(snapshot, plan, RevalidationPhase::BeforeSignal);
    match validation.status {
        RevalidationStatus::Eligible => Ok(observation.survivors),
        RevalidationStatus::Gone => Err("cleanup.revalidator_gone_with_survivors".to_owned()),
        RevalidationStatus::Blocked | RevalidationStatus::Revived => Err(validation.reason_id),
    }
}

struct ExactTargetLiveness {
    survivors: BTreeMap<u32, ProcessIdentity>,
    missing_live_targets: Vec<u32>,
}

/// Resolve every frozen PID that the full process-table snapshot omitted.
/// Targeted lookup may reject an absence proof, but an omitted live target
/// makes the table observation too racy to authorize a signal: the same listing
/// could also have missed a newly attached member or protection fact.
fn observe_exact_target_liveness<R: CleanupRuntime>(
    runtime: &mut R,
    snapshot: &Snapshot,
    plan: &CleanupPlan,
) -> Result<ExactTargetLiveness, String> {
    let by_pid = snapshot
        .processes
        .iter()
        .map(|process| (process.pid(), process))
        .collect::<BTreeMap<_, _>>();
    let mut survivors = BTreeMap::new();
    let mut missing_live_targets = Vec::new();

    for target in &plan.targets {
        let (process, missing_from_snapshot) =
            if let Some(process) = by_pid.get(&target.identity.pid) {
                (Some((*process).clone()), false)
            } else {
                match runtime.lookup_process(target.identity.pid) {
                    Ok(process) => (process, true),
                    Err(_) => return Err("cleanup.target_lookup_incomplete".to_owned()),
                }
            };
        let Some(process) = process else {
            continue;
        };
        if !target.identity.exact_match(&process.identity) {
            return Err("cleanup.identity_changed".to_owned());
        }
        survivors.insert(target.identity.pid, process.identity.clone());
        if missing_from_snapshot {
            missing_live_targets.push(target.identity.pid);
        }
    }

    Ok(ExactTargetLiveness {
        survivors,
        missing_live_targets,
    })
}

struct SignalContext<'a, R, V, J> {
    runtime: &'a mut R,
    revalidator: &'a V,
    journal: &'a mut J,
    should_stop: &'a mut dyn FnMut() -> bool,
    receipt: &'a mut CleanupReceipt,
    plan: &'a CleanupPlan,
}

fn signal<R: CleanupRuntime, V: IncidentRevalidator, J: CleanupActionJournal>(
    context: SignalContext<'_, R, V, J>,
    target: &ProcessTarget,
    stage: CleanupStage,
    cleanup_signal: CleanupSignal,
) -> Result<Option<String>, CleanupError> {
    let SignalContext {
        runtime,
        revalidator,
        journal,
        should_stop,
        receipt,
        plan,
    } = context;
    let intent = CleanupActionIntent {
        stage,
        pid: target.identity.pid,
        identity_fingerprint: fingerprint_process_identity(&target.identity),
        signal: cleanup_signal,
    };
    if should_stop() {
        return Ok(Some("cleanup.drain_requested".to_owned()));
    }
    let prepared_at = runtime
        .clock_sample()
        .map_err(|error| terminal_runtime_error(error, receipt))?
        .wall_unix_millis;
    let action_id = journal
        .prepare_action(&intent, prepared_at)
        .map_err(|error| terminal_runtime_error(error, receipt))?;

    if should_stop() {
        complete_prepared(
            runtime,
            journal,
            receipt,
            &intent,
            &action_id,
            SignalDisposition::CancelledBeforeDelivery,
        )?;
        return Ok(Some("cleanup.drain_requested".to_owned()));
    }

    let snapshot = match runtime.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            complete_prepared(
                runtime,
                journal,
                receipt,
                &intent,
                &action_id,
                SignalDisposition::CancelledBeforeDelivery,
            )?;
            return Err(terminal_runtime_error(error, receipt));
        }
    };
    let exact = match validate_stage(runtime, &snapshot, revalidator, plan) {
        Ok(exact) => exact,
        Err(reason) => {
            let disposition = if reason == "cleanup.identity_changed" {
                SignalDisposition::IdentityMismatch
            } else {
                SignalDisposition::Rejected
            };
            complete_prepared(
                runtime,
                &mut *journal,
                receipt,
                &intent,
                &action_id,
                disposition,
            )?;
            return Ok(Some(reason));
        }
    };
    if !exact.contains_key(&target.identity.pid) {
        complete_prepared(
            runtime,
            journal,
            receipt,
            &intent,
            &action_id,
            SignalDisposition::AlreadyExited,
        )?;
        return Ok(None);
    }

    if should_stop() {
        complete_prepared(
            runtime,
            journal,
            receipt,
            &intent,
            &action_id,
            SignalDisposition::CancelledBeforeDelivery,
        )?;
        return Ok(Some("cleanup.drain_requested".to_owned()));
    }

    let disposition = runtime.signal_exact(&target.identity, cleanup_signal);
    complete_prepared(runtime, journal, receipt, &intent, &action_id, disposition)?;
    Ok(match disposition {
        SignalDisposition::Delivered | SignalDisposition::AlreadyExited => None,
        SignalDisposition::IdentityMismatch => Some("cleanup.signal_identity_mismatch".to_owned()),
        SignalDisposition::Rejected => Some("cleanup.signal_rejected".to_owned()),
        SignalDisposition::CancelledBeforeDelivery => Some("cleanup.drain_requested".to_owned()),
        SignalDisposition::DeliveryUnknown => Some("cleanup.delivery_unknown".to_owned()),
    })
}

fn complete_prepared<R: CleanupRuntime, J: CleanupActionJournal>(
    runtime: &R,
    journal: &mut J,
    receipt: &mut CleanupReceipt,
    intent: &CleanupActionIntent,
    action_id: &str,
    disposition: SignalDisposition,
) -> Result<(), CleanupError> {
    let completed_at = runtime
        .clock_sample()
        .map_err(|error| CleanupError::AttemptOpen {
            error,
            partial_receipt: Box::new(receipt.clone()),
            prepared_action_id: action_id.to_owned(),
        })?
        .wall_unix_millis;
    journal
        .complete_action(action_id, disposition, completed_at)
        .map_err(|error| CleanupError::AttemptOpen {
            error,
            partial_receipt: Box::new(receipt.clone()),
            prepared_action_id: action_id.to_owned(),
        })?;
    receipt.actions.push(CleanupAction {
        stage: intent.stage,
        pid: intent.pid,
        identity_fingerprint: intent.identity_fingerprint.clone(),
        signal: intent.signal,
        disposition,
    });
    Ok(())
}

fn fail(mut receipt: CleanupReceipt, reason: String, mut survivors: Vec<u32>) -> CleanupReceipt {
    survivors.sort_unstable();
    survivors.dedup();
    receipt.state = IncidentState::Failed;
    receipt.reason_id = Some(reason);
    receipt.survivor_pids = survivors;
    receipt
}
