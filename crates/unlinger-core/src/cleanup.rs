use crate::{
    IncidentReport, IncidentState, ProcessIdentity, ProcessRole, ProcessTarget, Snapshot,
    fingerprint_parts,
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
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStage {
    PrimaryTerm,
    MemberTerm,
    ExactKill,
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
    pub survivor_pids: Vec<u32>,
    pub revival_checks_completed: usize,
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

pub trait CleanupRuntime {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure>;
    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition;
    fn wait(&mut self, duration: Duration);
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
        receipt: Box<CleanupReceipt>,
    },
}

impl CleanupError {
    #[must_use]
    pub fn receipt(&self) -> &CleanupReceipt {
        match self {
            Self::Runtime { receipt, .. } => receipt,
        }
    }
}

impl Display for CleanupError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runtime { error, .. } => write!(formatter, "cleanup runtime failed: {error}"),
        }
    }
}

impl Error for CleanupError {}

pub struct CleanupExecutor;

impl CleanupExecutor {
    pub fn execute<R: CleanupRuntime, V: IncidentRevalidator>(
        runtime: &mut R,
        revalidator: &V,
        plan: &CleanupPlan,
        policy: &CleanupPolicy,
    ) -> Result<CleanupReceipt, CleanupError> {
        let mut receipt = CleanupReceipt {
            incident_id: plan.incident_id.clone(),
            state: IncidentState::Reclaiming,
            reason_id: None,
            actions: Vec::new(),
            survivor_pids: Vec::new(),
            revival_checks_completed: 0,
        };
        let mut term_attempted = BTreeSet::new();

        let initial = snapshot_or_receipt_error(runtime, &receipt)?;
        let mut survivors = match validate_stage(&initial, revalidator, plan) {
            Ok(survivors) => survivors,
            Err(reason) => return Ok(fail(receipt, reason, Vec::new())),
        };

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
                        runtime,
                        &mut receipt,
                        target,
                        CleanupStage::PrimaryTerm,
                        CleanupSignal::Term,
                    ) {
                        return Ok(fail(receipt, reason, survivors.into_keys().collect()));
                    }
                    term_attempted.insert(target.identity.pid);
                }
            }
            runtime.wait(policy.primary_term_grace);
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            survivors = match validate_stage(&snapshot, revalidator, plan) {
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
                        runtime,
                        &mut receipt,
                        target,
                        CleanupStage::MemberTerm,
                        CleanupSignal::Term,
                    ) {
                        return Ok(fail(receipt, reason, survivors.into_keys().collect()));
                    }
                    term_attempted.insert(target.identity.pid);
                }
            }
            runtime.wait(policy.member_term_grace);
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            survivors = match validate_stage(&snapshot, revalidator, plan) {
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
                        runtime,
                        &mut receipt,
                        target,
                        CleanupStage::ExactKill,
                        CleanupSignal::Kill,
                    ) {
                        return Ok(fail(receipt, reason, survivors.into_keys().collect()));
                    }
                }
            }
            runtime.wait(policy.kill_grace);
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            survivors = match exact_survivors(&snapshot, plan) {
                Ok(survivors) => survivors,
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

        for window in &policy.revival_windows {
            runtime.wait(*window);
            let snapshot = snapshot_or_receipt_error(runtime, &receipt)?;
            receipt.revival_checks_completed += 1;
            let validation =
                revalidator.revalidate(&snapshot, plan, RevalidationPhase::RevivalCheck);
            match validation.status {
                RevalidationStatus::Gone => {}
                RevalidationStatus::Revived | RevalidationStatus::Eligible => {
                    receipt.state = IncidentState::Revived;
                    receipt.reason_id = Some(validation.reason_id);
                    return Ok(receipt);
                }
                RevalidationStatus::Blocked => {
                    return Ok(fail(receipt, validation.reason_id, Vec::new()));
                }
            }
        }

        receipt.state = IncidentState::Cleared;
        receipt.reason_id = Some("cleanup.tree_gone_no_revival".to_owned());
        Ok(receipt)
    }
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
            receipt: Box::new(failed),
        }
    })
}

fn validate_stage<V: IncidentRevalidator>(
    snapshot: &Snapshot,
    revalidator: &V,
    plan: &CleanupPlan,
) -> Result<BTreeMap<u32, ProcessIdentity>, String> {
    let survivors = exact_survivors(snapshot, plan)?;
    if survivors.is_empty() {
        return Ok(survivors);
    }
    let validation = revalidator.revalidate(snapshot, plan, RevalidationPhase::BeforeSignal);
    match validation.status {
        RevalidationStatus::Eligible => Ok(survivors),
        RevalidationStatus::Gone => Err("cleanup.revalidator_gone_with_survivors".to_owned()),
        RevalidationStatus::Blocked | RevalidationStatus::Revived => Err(validation.reason_id),
    }
}

fn exact_survivors(
    snapshot: &Snapshot,
    plan: &CleanupPlan,
) -> Result<BTreeMap<u32, ProcessIdentity>, String> {
    let by_pid = snapshot
        .processes
        .iter()
        .map(|process| (process.pid(), process))
        .collect::<BTreeMap<_, _>>();
    let mut survivors = BTreeMap::new();
    for target in &plan.targets {
        if let Some(process) = by_pid.get(&target.identity.pid) {
            if !target.identity.exact_match(&process.identity) {
                return Err("cleanup.identity_changed".to_owned());
            }
            survivors.insert(target.identity.pid, process.identity.clone());
        }
    }
    Ok(survivors)
}

fn signal<R: CleanupRuntime>(
    runtime: &mut R,
    receipt: &mut CleanupReceipt,
    target: &ProcessTarget,
    stage: CleanupStage,
    cleanup_signal: CleanupSignal,
) -> Option<String> {
    let disposition = runtime.signal_exact(&target.identity, cleanup_signal);
    let row = format!(
        "{}:{}:{}:{}",
        target.identity.pid,
        target.identity.started_at_unix_micros,
        target.identity.executable_device.unwrap_or_default(),
        target.identity.executable_inode.unwrap_or_default()
    );
    receipt.actions.push(CleanupAction {
        stage,
        pid: target.identity.pid,
        identity_fingerprint: fingerprint_parts([row.as_bytes()]),
        signal: cleanup_signal,
        disposition,
    });
    match disposition {
        SignalDisposition::Delivered | SignalDisposition::AlreadyExited => None,
        SignalDisposition::IdentityMismatch => Some("cleanup.signal_identity_mismatch".to_owned()),
        SignalDisposition::Rejected => Some("cleanup.signal_rejected".to_owned()),
    }
}

fn fail(mut receipt: CleanupReceipt, reason: String, mut survivors: Vec<u32>) -> CleanupReceipt {
    survivors.sort_unstable();
    survivors.dedup();
    receipt.state = IncidentState::Failed;
    receipt.reason_id = Some(reason);
    receipt.survivor_pids = survivors;
    receipt
}
