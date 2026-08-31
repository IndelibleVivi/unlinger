use crate::{
    ControlError, ControlPlane, CoolingClock, DaemonMode, ObservedIncidentIdentity, RecentReclaim,
    RetentionPolicy, StartupState, StoreError,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Duration;
use unlinger_core::{
    ArtifactDisposition, ArtifactFreeze, CleanupError, CleanupExecutor, CleanupPlan,
    CleanupPlanError, CleanupPolicy, CleanupReceipt, CleanupRuntime, CleanupSignal, ClockSample,
    EvidenceFamily, EvidenceItem, FrozenRuntimeArtifact, IncidentReport, IncidentState,
    ProcessGraph, ProcessIdentity, ProcessRecord, RuntimeArtifactCandidate, RuntimeFailure,
    SignalDisposition, Snapshot, WaitOutcome, fingerprint_process_identity,
};
use unlinger_rules::{Analyzer, AnalyzerContext, RuleError, RuleSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineConfig {
    pub observation_gap: Duration,
    pub abandonment_grace: Duration,
    pub cooling_continuity_gap: Duration,
    pub exact_absence_retention: Duration,
    pub cleanup_policy: CleanupPolicy,
    pub retention_policy: RetentionPolicy,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            observation_gap: Duration::from_secs(15),
            abandonment_grace: Duration::from_secs(90),
            cooling_continuity_gap: Duration::from_secs(120),
            exact_absence_retention: Duration::from_secs(14 * 24 * 60 * 60),
            cleanup_policy: CleanupPolicy::default(),
            retention_policy: RetentionPolicy::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CycleReport {
    pub schema_version: u32,
    pub observed_twice: bool,
    pub incidents: Vec<IncidentReport>,
    pub cleanup_receipts: Vec<CleanupReceipt>,
}

#[derive(Debug)]
pub enum EngineError {
    Runtime(RuntimeFailure),
    Rules(RuleError),
    Graph(String),
    Store(StoreError),
    Control(ControlError),
    Plan(CleanupPlanError),
    Cleanup(CleanupError),
    PostDeliveryFailure(String),
    Config(String),
}

impl Display for EngineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runtime(error) => write!(formatter, "snapshot runtime failed: {error}"),
            Self::Rules(error) => write!(formatter, "classification failed: {error}"),
            Self::Graph(error) => write!(formatter, "process graph failed: {error}"),
            Self::Store(error) => write!(formatter, "history store failed: {error}"),
            Self::Control(error) => write!(formatter, "daemon control failed: {error}"),
            Self::Plan(error) => write!(formatter, "cleanup plan failed: {error}"),
            Self::Cleanup(error) => write!(formatter, "cleanup execution failed: {error}"),
            Self::PostDeliveryFailure(incident_id) => write!(
                formatter,
                "cleanup {incident_id} ended with an uncertain or incomplete delivered side effect"
            ),
            Self::Config(error) => write!(formatter, "engine configuration failed: {error}"),
        }
    }
}

impl Error for EngineError {}

impl From<RuntimeFailure> for EngineError {
    fn from(value: RuntimeFailure) -> Self {
        Self::Runtime(value)
    }
}

impl From<RuleError> for EngineError {
    fn from(value: RuleError) -> Self {
        Self::Rules(value)
    }
}

impl From<StoreError> for EngineError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<ControlError> for EngineError {
    fn from(value: ControlError) -> Self {
        Self::Control(value)
    }
}

impl From<CleanupPlanError> for EngineError {
    fn from(value: CleanupPlanError) -> Self {
        Self::Plan(value)
    }
}

impl From<CleanupError> for EngineError {
    fn from(value: CleanupError) -> Self {
        Self::Cleanup(value)
    }
}

pub struct ReconciliationEngine<R> {
    runtime: R,
    rules: RuleSet,
    control: ControlPlane,
    config: EngineConfig,
    self_pid: Option<u32>,
}

struct GatedRuntime<'a, R> {
    inner: &'a mut R,
    control: ControlPlane,
    enforcement_epoch: String,
    cleanup_policy_revision: u64,
}

impl<R: CleanupRuntime> CleanupRuntime for GatedRuntime<'_, R> {
    fn snapshot(&mut self) -> Result<Snapshot, RuntimeFailure> {
        self.inner.snapshot()
    }

    fn lookup_process(&mut self, pid: u32) -> Result<Option<ProcessRecord>, RuntimeFailure> {
        self.inner.lookup_process(pid)
    }

    fn clock_sample(&self) -> Result<ClockSample, RuntimeFailure> {
        self.inner.clock_sample()
    }

    fn signal_exact(
        &mut self,
        identity: &ProcessIdentity,
        signal: CleanupSignal,
    ) -> SignalDisposition {
        let control = self.control.clone();
        let epoch = self.enforcement_epoch.clone();
        control.deliver_signal_if_armed(&epoch, self.cleanup_policy_revision, || {
            self.inner.signal_exact(identity, signal)
        })
    }

    fn wait_until(
        &mut self,
        duration: Duration,
        should_stop: &mut dyn FnMut() -> bool,
    ) -> Result<WaitOutcome, RuntimeFailure> {
        self.inner.wait_until(duration, should_stop)
    }

    fn freeze_artifact(
        &mut self,
        candidate: &RuntimeArtifactCandidate,
    ) -> Result<ArtifactFreeze, RuntimeFailure> {
        self.inner.freeze_artifact(candidate)
    }

    fn remove_artifact_exact(&mut self, artifact: &FrozenRuntimeArtifact) -> ArtifactDisposition {
        let mut artifact_disposition = ArtifactDisposition::CancelledBeforeDelivery;
        let gate = self.control.deliver_signal_if_armed(
            &self.enforcement_epoch,
            self.cleanup_policy_revision,
            || {
                artifact_disposition = self.inner.remove_artifact_exact(artifact);
                SignalDisposition::Delivered
            },
        );
        if gate == SignalDisposition::CancelledBeforeDelivery {
            ArtifactDisposition::CancelledBeforeDelivery
        } else {
            artifact_disposition
        }
    }
}

impl<R: CleanupRuntime> ReconciliationEngine<R> {
    #[must_use]
    pub fn new(
        runtime: R,
        rules: RuleSet,
        control: ControlPlane,
        config: EngineConfig,
        self_pid: Option<u32>,
    ) -> Self {
        Self {
            runtime,
            rules,
            control,
            config,
            self_pid,
        }
    }

    #[must_use]
    pub fn runtime(&self) -> &R {
        &self.runtime
    }

    #[must_use]
    pub fn config_mut(&mut self) -> &mut EngineConfig {
        &mut self.config
    }

    pub fn run_cycle_at(&mut self, now_unix_millis: u64) -> Result<CycleReport, EngineError> {
        self.run_cycle_at_until(now_unix_millis, || false)
    }

    pub fn run_cycle_at_until(
        &mut self,
        now_unix_millis: u64,
        mut should_stop: impl FnMut() -> bool,
    ) -> Result<CycleReport, EngineError> {
        self.control.update_status(|status| {
            status.scan_in_progress = true;
            if status.startup_state != StartupState::Failed {
                status.last_error = None;
            }
        })?;
        let result = self.run_cycle_inner(now_unix_millis, &mut should_stop);
        match &result {
            Ok(_) => {
                self.control.update_status(|status| {
                    status.scan_in_progress = false;
                    status.cleanup_in_progress = false;
                    status.last_scan_at_unix_millis = Some(now_unix_millis);
                    if status.startup_state != StartupState::Failed {
                        status.last_error = None;
                    }
                })?;
                self.control.complete_successful_cycle(now_unix_millis)?;
            }
            Err(error) => {
                let message = error.to_string();
                let _ = self.control.fail_closed(now_unix_millis, &message);
                let _ = self.control.update_status(|status| {
                    status.scan_in_progress = false;
                    status.cleanup_in_progress = false;
                    status.last_error = Some(message);
                });
            }
        }
        result
    }

    fn run_cycle_inner(
        &mut self,
        now_unix_millis: u64,
        should_stop: &mut impl FnMut() -> bool,
    ) -> Result<CycleReport, EngineError> {
        let lifecycle = self.control.status_at(now_unix_millis)?;
        let cleanup_policy_revision = self.control.cleanup_policy_revision();
        let enforcement_epoch = lifecycle
            .enforcement_epoch
            .clone()
            .unwrap_or_else(|| "disarmed".to_owned());
        let first_snapshot = self.runtime.snapshot()?;
        let first_clock = cooling_clock(self.runtime.clock_sample()?, &enforcement_epoch);
        let analyzer = self.analyzer_for(&first_snapshot)?;
        let first_reports = analyzer.observe(&first_snapshot)?;
        let abandonment_grace_millis = duration_millis(self.config.abandonment_grace)?;
        let continuity_gap_millis = duration_millis(self.config.cooling_continuity_gap)?;
        for report in first_reports
            .iter()
            .filter(|report| report.state == IncidentState::Cooling)
        {
            self.control.store().track_cooling(
                report,
                &first_clock,
                abandonment_grace_millis,
                continuity_gap_millis,
            )?;
        }
        let needs_second_observation = first_reports
            .iter()
            .any(|report| report.state == IncidentState::Cooling);
        let (mut incidents, observed_twice, latest_snapshot) = if needs_second_observation {
            if self
                .runtime
                .wait_until(self.config.observation_gap, should_stop)?
                == WaitOutcome::Interrupted
            {
                (first_reports, false, first_snapshot)
            } else {
                let second_snapshot = self.runtime.snapshot()?;
                let second_clock = cooling_clock(self.runtime.clock_sample()?, &enforcement_epoch);
                let second_reports = analyzer.observe(&second_snapshot)?;
                let mut active_tracking_keys = BTreeSet::new();
                let mut abandonment_confirmed = BTreeSet::new();
                for report in second_reports
                    .iter()
                    .filter(|report| report.state == IncidentState::Cooling)
                {
                    active_tracking_keys.insert(report.tracking_key.clone());
                    if self.control.store().track_cooling(
                        report,
                        &second_clock,
                        abandonment_grace_millis,
                        continuity_gap_millis,
                    )? {
                        abandonment_confirmed.insert(report.tracking_key.clone());
                    }
                }
                self.control.store().retain_cooling(&active_tracking_keys)?;
                (
                    analyzer.reconcile_with_abandonment(
                        &first_reports,
                        &second_reports,
                        &abandonment_confirmed,
                    ),
                    true,
                    second_snapshot,
                )
            }
        } else {
            self.control.store().retain_cooling(&BTreeSet::new())?;
            (first_reports, false, first_snapshot)
        };

        let observed_identities = incidents
            .iter()
            .map(ObservedIncidentIdentity::from)
            .collect::<Vec<_>>();
        let live_identity_fingerprints = exact_live_identity_fingerprints(&latest_snapshot);
        let absence_proven = latest_snapshot.proves_complete_exact_identity_coverage();
        let exact_absence_retention_millis = duration_millis(self.config.exact_absence_retention)?;
        self.control.store().reconcile_retry_blocks(
            &observed_identities,
            &live_identity_fingerprints,
            absence_proven,
            now_unix_millis,
            exact_absence_retention_millis,
        )?;
        self.control.store().reconcile_incident_protections(
            &live_identity_fingerprints,
            absence_proven,
            now_unix_millis,
            exact_absence_retention_millis,
        )?;

        for report in &mut incidents {
            if self.control.store().is_incident_protected(report)? {
                apply_owner_protection(report);
            }
        }

        for report in &incidents {
            self.control
                .store()
                .record_observation(now_unix_millis, report)?;
        }

        let status = self.control.status_at(now_unix_millis)?;
        let paused = status
            .paused_until_unix_millis
            .is_some_and(|deadline| deadline > now_unix_millis);
        let lifecycle_still_authorizes_cleanup = lifecycle.effective_mode() == DaemonMode::Enforce
            && lifecycle.enforcement_epoch.as_deref() == Some(enforcement_epoch.as_str())
            && status.effective_mode() == DaemonMode::Enforce
            && status.enforcement_epoch.as_deref() == Some(enforcement_epoch.as_str())
            && self.control.cleanup_policy_revision() == cleanup_policy_revision;
        let mut cleanup_receipts = Vec::new();
        if lifecycle_still_authorizes_cleanup && !paused && !should_stop() {
            for report in incidents
                .iter()
                .filter(|report| report.state == IncidentState::Confirmed)
            {
                if should_stop() {
                    break;
                }
                if self.control.store().cleanup_blocked(&report.incident_id)? {
                    continue;
                }
                let plan = CleanupPlan::from_confirmed(report)?;
                let Some(attempt) = self.control.begin_cleanup_attempt_if_armed(
                    now_unix_millis,
                    report,
                    &enforcement_epoch,
                    cleanup_policy_revision,
                )?
                else {
                    break;
                };
                self.control
                    .update_status(|status| status.cleanup_in_progress = true)?;
                let mut journal = self.control.store().journal_for(&attempt);
                let signal_control = self.control.clone();
                let stop_control = self.control.clone();
                let epoch_for_stop = enforcement_epoch.clone();
                let mut cleanup_should_stop = || {
                    should_stop()
                        || stop_control.action_gate_closed(&epoch_for_stop, cleanup_policy_revision)
                };
                let mut gated_runtime = GatedRuntime {
                    inner: &mut self.runtime,
                    control: signal_control,
                    enforcement_epoch: enforcement_epoch.clone(),
                    cleanup_policy_revision,
                };
                let execution = CleanupExecutor::execute(
                    &mut gated_runtime,
                    &analyzer,
                    &mut journal,
                    &mut cleanup_should_stop,
                    &plan,
                    &self.config.cleanup_policy,
                );
                drop(gated_runtime);
                let (receipt, completed_at) = match execution {
                    Ok(receipt) => {
                        let completed_at = terminal_timestamp(&self.runtime, now_unix_millis);
                        (
                            self.control.store().complete_cleanup_attempt(
                                &attempt,
                                completed_at,
                                &receipt,
                            )?,
                            completed_at,
                        )
                    }
                    Err(error) => {
                        if let Some(terminal_receipt) = error.terminal_receipt().cloned() {
                            let completed_at = terminal_timestamp(&self.runtime, now_unix_millis);
                            self.control.store().complete_cleanup_attempt(
                                &attempt,
                                completed_at,
                                &terminal_receipt,
                            )?;
                        } else {
                            self.control.fail_closed(
                                terminal_timestamp(&self.runtime, now_unix_millis),
                                error.to_string(),
                            )?;
                        }
                        self.control
                            .update_status(|status| status.cleanup_in_progress = false)?;
                        return Err(EngineError::Cleanup(error));
                    }
                };
                self.control
                    .update_status(|status| status.cleanup_in_progress = false)?;
                if receipt_requires_global_fail_close(&receipt) {
                    return Err(EngineError::PostDeliveryFailure(
                        receipt.incident_id.clone(),
                    ));
                }
                if receipt.outcome().process == unlinger_core::ProcessOutcome::Cleared {
                    self.control.update_status(|status| {
                        status.most_recent_reclaim = Some(RecentReclaim {
                            incident_id: receipt.incident_id.clone(),
                            occurred_at_unix_millis: completed_at,
                            state: receipt.state,
                            outcome: Some(receipt.outcome()),
                        });
                    })?;
                }
                cleanup_receipts.push(receipt);
            }
        }

        let attempted_ids = cleanup_receipts
            .iter()
            .map(|receipt| receipt.incident_id.as_str())
            .collect::<BTreeSet<_>>();
        let confirmed_incidents = incidents
            .iter()
            .filter(|incident| {
                incident.state == IncidentState::Confirmed
                    && !attempted_ids.contains(incident.incident_id.as_str())
            })
            .count();
        let ambiguous_incidents = incidents
            .iter()
            .filter(|incident| incident.state == IncidentState::Ambiguous)
            .count();
        self.control.update_status(|status| {
            status.confirmed_incidents = confirmed_incidents;
            status.ambiguous_incidents = ambiguous_incidents;
        })?;
        self.control
            .store()
            .prune(now_unix_millis, self.config.retention_policy)?;

        Ok(CycleReport {
            schema_version: 1,
            observed_twice,
            incidents,
            cleanup_receipts,
        })
    }

    fn analyzer_for(&self, snapshot: &unlinger_core::Snapshot) -> Result<Analyzer, EngineError> {
        let graph = ProcessGraph::from_snapshot(snapshot)
            .map_err(|error| EngineError::Graph(error.to_string()))?;
        let ancestor_pids = self
            .self_pid
            .map(|pid| graph.ancestor_pids(pid).into_iter().collect())
            .unwrap_or_default();
        Ok(Analyzer::new(
            self.rules.clone(),
            AnalyzerContext {
                self_pid: self.self_pid,
                ancestor_pids,
            },
        ))
    }
}

fn duration_millis(duration: Duration) -> Result<u64, EngineError> {
    u64::try_from(duration.as_millis())
        .map_err(|_| EngineError::Config("duration overflowed u64 milliseconds".to_owned()))
}

fn exact_live_identity_fingerprints(snapshot: &Snapshot) -> BTreeSet<String> {
    snapshot
        .processes
        .iter()
        .filter(|process| {
            process.identity.started_at_unix_micros > 0
                && process.identity.executable_device.is_some()
                && process.identity.executable_inode.is_some()
        })
        .map(|process| fingerprint_process_identity(&process.identity))
        .collect()
}

fn apply_owner_protection(report: &mut IncidentReport) {
    report.state = IncidentState::Protected;
    report.gates.no_protection_rule = false;
    if !report
        .evidence
        .iter()
        .any(|evidence| evidence.id == "protection.owner_exact_incident")
    {
        report.evidence.push(EvidenceItem {
            id: "protection.owner_exact_incident".to_owned(),
            family: EvidenceFamily::Protection,
            source_pid: Some(report.root.pid),
        });
        report.evidence.sort_by(|left, right| {
            left.family
                .cmp(&right.family)
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.source_pid.cmp(&right.source_pid))
        });
    }
}

fn receipt_requires_global_fail_close(receipt: &CleanupReceipt) -> bool {
    let uncertain = receipt
        .actions
        .iter()
        .any(|action| action.disposition == SignalDisposition::DeliveryUnknown)
        || receipt
            .artifact_actions
            .iter()
            .any(|action| action.disposition == ArtifactDisposition::DeliveryUnknown);
    let delivered_side_effect = receipt
        .actions
        .iter()
        .any(|action| action.disposition == SignalDisposition::Delivered)
        || receipt
            .artifact_actions
            .iter()
            .any(|action| action.disposition == ArtifactDisposition::Removed);
    if uncertain {
        return true;
    }
    if receipt.outcome().overall == unlinger_core::OverallOutcome::ClearedWithResidue {
        return false;
    }
    receipt.state != IncidentState::Cleared && delivered_side_effect
}

fn terminal_timestamp<R: CleanupRuntime>(runtime: &R, started_at_unix_millis: u64) -> u64 {
    runtime
        .clock_sample()
        .map(|sample| sample.wall_unix_millis)
        .unwrap_or(started_at_unix_millis)
        .max(started_at_unix_millis)
}

fn cooling_clock(sample: unlinger_core::ClockSample, enforcement_epoch: &str) -> CoolingClock {
    CoolingClock {
        wall_unix_millis: sample.wall_unix_millis,
        continuous_millis: sample.continuous_millis,
        boot_session_fingerprint: sample.boot_session_fingerprint,
        enforcement_epoch: enforcement_epoch.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::receipt_requires_global_fail_close;
    use unlinger_core::{
        ArtifactAction, ArtifactDisposition, CleanupAction, CleanupReceipt, CleanupResources,
        CleanupSignal, CleanupStage, IncidentState, RuntimeArtifactKind, SignalDisposition,
    };

    fn receipt_with_artifact(disposition: ArtifactDisposition, reason_id: &str) -> CleanupReceipt {
        CleanupReceipt {
            incident_id: "inc-artifact-outcome".to_owned(),
            state: IncidentState::Failed,
            reason_id: Some(reason_id.to_owned()),
            actions: vec![CleanupAction {
                stage: CleanupStage::PrimaryTerm,
                pid: 42,
                identity_fingerprint: "proc-redacted".to_owned(),
                signal: CleanupSignal::Term,
                disposition: SignalDisposition::Delivered,
            }],
            artifact_actions: vec![ArtifactAction {
                kind: RuntimeArtifactKind::DevToolsActivePort,
                artifact_fingerprint: "artifact-redacted".to_owned(),
                disposition,
            }],
            survivor_pids: Vec::new(),
            revival_checks_completed: 2,
            resources: CleanupResources::default(),
        }
    }

    #[test]
    fn pre_delivery_artifact_residue_does_not_fail_the_whole_daemon_closed() {
        let receipt = receipt_with_artifact(ArtifactDisposition::Unsafe, "cleanup.artifact_unsafe");

        assert!(!receipt_requires_global_fail_close(&receipt));
    }

    #[test]
    fn post_delivery_uncertainty_still_fails_the_whole_daemon_closed() {
        let receipt = receipt_with_artifact(
            ArtifactDisposition::DeliveryUnknown,
            "cleanup.artifact_delivery_unknown",
        );

        assert!(receipt_requires_global_fail_close(&receipt));
    }

    #[test]
    fn failure_after_artifact_removal_still_fails_the_whole_daemon_closed() {
        let receipt = receipt_with_artifact(
            ArtifactDisposition::Removed,
            "cleanup.target_lookup_incomplete",
        );

        assert!(receipt_requires_global_fail_close(&receipt));
    }
}
