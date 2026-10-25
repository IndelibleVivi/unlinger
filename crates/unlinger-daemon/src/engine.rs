use crate::{
    ControlError, ControlPlane, CoolingClock, DaemonMode, ObservedIncidentIdentity, RecentReclaim,
    RetentionPolicy, StartupState, StoreError,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
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
    Worker(String),
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
            Self::Worker(error) => write!(formatter, "cleanup worker failed: {error}"),
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
    cleanup_runtime_factory: Option<Arc<dyn Fn() -> R + Send + Sync>>,
    background_cleanup: Option<BackgroundCleanup>,
}

struct BackgroundCleanup {
    handle: JoinHandle<CleanupCompletion>,
    cancelled: Arc<AtomicBool>,
    started_at_unix_millis: u64,
}

struct CleanupCompletion {
    completed_at_unix_millis: u64,
    result: Result<CleanupReceipt, EngineError>,
}

struct FrozenCleanup {
    analyzer: Analyzer,
    attempt: crate::CleanupAttemptHandle,
    plan: CleanupPlan,
    policy: CleanupPolicy,
    enforcement_epoch: String,
    cleanup_policy_revision: u64,
    started_at_unix_millis: u64,
}

// The worker owns activity through terminal commit. Its handle remains owned
// until join; recovery must never convert a journal while delivery is possible.
impl<R> ReconciliationEngine<R> {
    #[must_use]
    pub fn background_cleanup_finished(&self) -> bool {
        self.background_cleanup
            .as_ref()
            .is_some_and(|worker| worker.handle.is_finished())
    }

    pub fn shutdown_background_cleanup(&mut self) -> Result<(), EngineError> {
        if let Some(worker) = &self.background_cleanup {
            worker.cancelled.store(true, Ordering::Release);
        }
        self.collect_background_cleanup(true).map(|_| ())
    }

    fn collect_background_cleanup(
        &mut self,
        wait: bool,
    ) -> Result<Option<CleanupReceipt>, EngineError> {
        if !wait && !self.background_cleanup_finished() {
            return Ok(None);
        }
        let Some(worker) = self.background_cleanup.take() else {
            return Ok(None);
        };
        let completion = worker.handle.join().unwrap_or_else(|_| CleanupCompletion {
            completed_at_unix_millis: worker.started_at_unix_millis,
            result: Err(EngineError::Worker("thread panicked".to_owned())),
        });
        match completion.result {
            Ok(receipt) => Ok(Some(receipt)),
            Err(error) => {
                // The worker is gone: converting a surviving PREPARED journal
                // to delivery_unknown can no longer race its native delivery.
                let _ = self
                    .control
                    .fail_closed(completion.completed_at_unix_millis, error.to_string());
                self.control
                    .store()
                    .recover_incomplete_attempts(completion.completed_at_unix_millis)?;
                Err(error)
            }
        }
    }
}

impl<R> Drop for ReconciliationEngine<R> {
    fn drop(&mut self) {
        let _ = self.shutdown_background_cleanup();
    }
}

struct GatedRuntime<'a, R> {
    inner: &'a mut R,
    control: ControlPlane,
    enforcement_epoch: String,
    cleanup_policy_revision: u64,
    cancelled: &'a AtomicBool,
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
            if self.cancelled.load(Ordering::Acquire) {
                SignalDisposition::CancelledBeforeDelivery
            } else {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.inner.signal_exact(identity, signal)
                }))
                .unwrap_or(SignalDisposition::DeliveryUnknown)
            }
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
                if self.cancelled.load(Ordering::Acquire) {
                    SignalDisposition::CancelledBeforeDelivery
                } else {
                    artifact_disposition =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            self.inner.remove_artifact_exact(artifact)
                        }))
                        .unwrap_or(ArtifactDisposition::DeliveryUnknown);
                    SignalDisposition::Delivered
                }
            },
        );
        if gate == SignalDisposition::CancelledBeforeDelivery {
            ArtifactDisposition::CancelledBeforeDelivery
        } else {
            artifact_disposition
        }
    }
}

impl<R: CleanupRuntime + Send + 'static> ReconciliationEngine<R> {
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
            cleanup_runtime_factory: None,
            background_cleanup: None,
        }
    }

    /// Opts live service callers into one background cleanup, with a separate
    /// runtime. One-shot and field-harness callers keep synchronous semantics.
    #[must_use]
    pub fn with_background_cleanup(
        mut self,
        runtime_factory: impl Fn() -> R + Send + Sync + 'static,
    ) -> Self {
        self.cleanup_runtime_factory = Some(Arc::new(runtime_factory));
        self
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
        let cycle_token = self.control.begin_observation_cycle(now_unix_millis)?;
        let result = self.run_cycle_inner(now_unix_millis, &cycle_token, &mut should_stop);
        match &result {
            Ok(_) => {
                let completed_at_unix_millis = terminal_timestamp(&self.runtime, now_unix_millis);
                self.control.finish_observation_cycle(&cycle_token, true);
                self.control.update_status(|status| {
                    status.scan_in_progress = false;
                    status.last_scan_at_unix_millis = Some(completed_at_unix_millis);
                    if status.startup_state != StartupState::Failed {
                        status.last_error = None;
                    }
                })?;
                self.control
                    .complete_successful_cycle(completed_at_unix_millis)?;
            }
            Err(error) => {
                let message = error.to_string();
                let failed_at_unix_millis = terminal_timestamp(&self.runtime, now_unix_millis);
                self.control.finish_observation_cycle(&cycle_token, false);
                let _ = self.control.fail_closed(failed_at_unix_millis, &message);
                let _ = self.control.update_status(|status| {
                    status.scan_in_progress = false;
                    status.last_error = Some(message);
                });
            }
        }
        result
    }

    fn run_cycle_inner(
        &mut self,
        now_unix_millis: u64,
        cycle_token: &str,
        should_stop: &mut impl FnMut() -> bool,
    ) -> Result<CycleReport, EngineError> {
        let mut cleanup_receipts = self
            .collect_background_cleanup(false)?
            .into_iter()
            .collect::<Vec<_>>();
        let lifecycle = self.control.status_at(now_unix_millis)?;
        let cleanup_policy_revision = self.control.cleanup_policy_revision();
        let enforcement_epoch = lifecycle
            .enforcement_epoch
            .clone()
            .unwrap_or_else(|| "disarmed".to_owned());
        let first_snapshot = self.runtime.snapshot()?;
        self.refresh_task_ownership(&first_snapshot)?;
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
        let (mut incidents, observed_twice, latest_snapshot, observed_at_unix_millis) =
            if needs_second_observation {
                if self
                    .runtime
                    .wait_until(self.config.observation_gap, should_stop)?
                    == WaitOutcome::Interrupted
                {
                    (
                        first_reports,
                        false,
                        first_snapshot,
                        first_clock.wall_unix_millis,
                    )
                } else {
                    let second_snapshot = self.runtime.snapshot()?;
                    let second_clock =
                        cooling_clock(self.runtime.clock_sample()?, &enforcement_epoch);
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
                        second_clock.wall_unix_millis,
                    )
                }
            } else {
                self.control.store().retain_cooling(&BTreeSet::new())?;
                (
                    first_reports,
                    false,
                    first_snapshot,
                    first_clock.wall_unix_millis,
                )
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

        // Persist the complete protected snapshot first, then publish it. A
        // batch failure leaves both history and the previous roster intact.
        self.control
            .store()
            .record_observation_batch(observed_at_unix_millis, &incidents)?;
        self.control.publish_roster(
            cycle_token,
            observed_at_unix_millis,
            incidents.clone(),
            latest_snapshot.proves_complete_classification_coverage(),
        )?;

        let status = self.control.status_at(now_unix_millis)?;
        let paused = status
            .paused_until_unix_millis
            .is_some_and(|deadline| deadline > now_unix_millis);
        let lifecycle_still_authorizes_cleanup = lifecycle.effective_mode() == DaemonMode::Enforce
            && lifecycle.enforcement_epoch.as_deref() == Some(enforcement_epoch.as_str())
            && status.effective_mode() == DaemonMode::Enforce
            && status.enforcement_epoch.as_deref() == Some(enforcement_epoch.as_str())
            && self.control.cleanup_policy_revision() == cleanup_policy_revision;
        if lifecycle_still_authorizes_cleanup
            && !paused
            && !should_stop()
            && self.background_cleanup.is_none()
        {
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
                let process_activity = self.control.begin_process_cleanup_activity(cycle_token)?;
                let frozen = FrozenCleanup {
                    analyzer: analyzer.clone(),
                    attempt,
                    plan,
                    policy: self.config.cleanup_policy.clone(),
                    enforcement_epoch: enforcement_epoch.clone(),
                    cleanup_policy_revision,
                    started_at_unix_millis: now_unix_millis,
                };
                if let Some(factory) = &self.cleanup_runtime_factory {
                    let factory = Arc::clone(factory);
                    let control = self.control.clone();
                    let cancelled = Arc::new(AtomicBool::new(false));
                    let worker_cancelled = Arc::clone(&cancelled);
                    let spawned = thread::Builder::new()
                        .name("unlinger-process-cleanup".to_owned())
                        .spawn(move || {
                            // Keep exact activity ownership through terminal commit,
                            // including unwinding runtime factories and adapters.
                            let _activity = process_activity;
                            let mut completed_at_unix_millis = now_unix_millis;
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    let mut runtime = factory();
                                    let result = std::panic::catch_unwind(
                                        std::panic::AssertUnwindSafe(|| {
                                            execute_cleanup(
                                                &mut runtime,
                                                &control,
                                                &frozen,
                                                &worker_cancelled,
                                                &mut || false,
                                            )
                                        }),
                                    )
                                    .unwrap_or_else(|_| {
                                        Err(EngineError::Worker("thread panicked".to_owned()))
                                    });
                                    completed_at_unix_millis =
                                        terminal_timestamp(&runtime, now_unix_millis);
                                    result
                                }))
                                .unwrap_or_else(|_| {
                                    Err(EngineError::Worker("thread panicked".to_owned()))
                                });
                            if let Err(error) = &result {
                                let _ = control
                                    .fail_closed(completed_at_unix_millis, error.to_string());
                            }
                            CleanupCompletion {
                                completed_at_unix_millis,
                                result,
                            }
                        });
                    match spawned {
                        Ok(handle) => {
                            self.background_cleanup = Some(BackgroundCleanup {
                                handle,
                                cancelled,
                                started_at_unix_millis: now_unix_millis,
                            })
                        }
                        Err(error) => {
                            self.control
                                .fail_closed(now_unix_millis, error.to_string())?;
                            self.control
                                .store()
                                .recover_incomplete_attempts(now_unix_millis)?;
                            return Err(EngineError::Worker(error.to_string()));
                        }
                    }
                    // No queued plans: after collection the next dispatch must
                    // be derived from a new complete protected observation.
                    break;
                }
                let receipt = execute_cleanup(
                    &mut self.runtime,
                    &self.control,
                    &frozen,
                    &AtomicBool::new(false),
                    should_stop,
                );
                drop(process_activity);
                cleanup_receipts.push(receipt?);
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
                task_controllers: self.control.store().task_controller_bindings()?,
                session_owners: self.control.store().session_owner_bindings()?,
            },
        ))
    }

    fn refresh_task_ownership(&mut self, snapshot: &Snapshot) -> Result<(), EngineError> {
        self.release_absent_task_owners()?;
        self.control.store().bind_task_controllers(snapshot)?;
        self.release_absent_session_owners()?;
        self.control
            .store()
            .bind_session_owner_controllers(snapshot)?;
        Ok(())
    }

    fn release_absent_task_owners(&mut self) -> Result<(), EngineError> {
        for scope in self.control.store().task_scopes()? {
            if scope.released_at_us.is_some() {
                continue;
            }
            let owner = scope.owner.as_ref().unwrap_or(&scope.registrar);
            if self.owner_is_absent(owner)? {
                self.control.store().release_task(
                    &scope.task_id,
                    self.runtime.clock_sample()?.wall_unix_millis,
                    if scope.owner.is_some() {
                        "owner_disappeared"
                    } else {
                        "never_started"
                    },
                    scope.owner.as_ref(),
                )?;
            }
        }
        Ok(())
    }

    /// A host-declared session owner is released only on exact native absence.
    /// A read failure leaves the lease active, so a partial snapshot can never
    /// manufacture abandonment evidence for a live detached controller.
    fn release_absent_session_owners(&mut self) -> Result<(), EngineError> {
        for scope in self.control.store().session_owner_scopes()? {
            if scope.released_at_us.is_some() {
                continue;
            }
            let owner = scope.owner.as_ref().unwrap_or(&scope.registrar);
            if self.owner_is_absent(owner)? {
                self.control.store().release_session_owner(
                    &scope.lease_id,
                    self.runtime.clock_sample()?.wall_unix_millis,
                    if scope.owner.is_some() {
                        "owner_disappeared"
                    } else {
                        "never_started"
                    },
                    scope.owner.as_ref(),
                )?;
            }
        }
        Ok(())
    }

    fn owner_is_absent(
        &mut self,
        owner: &unlinger_core::TaskOwnerIdentity,
    ) -> Result<bool, EngineError> {
        Ok(match self.runtime.lookup_process(owner.pid) {
            Ok(None) => true,
            Ok(Some(process)) => !owner.matches(&process),
            Err(_) => false,
        })
    }
}

fn execute_cleanup<R: CleanupRuntime>(
    runtime: &mut R,
    control: &ControlPlane,
    frozen: &FrozenCleanup,
    cancelled: &AtomicBool,
    should_stop: &mut dyn FnMut() -> bool,
) -> Result<CleanupReceipt, EngineError> {
    let mut journal = control.store().journal_for(&frozen.attempt);
    let mut cleanup_should_stop = || {
        should_stop()
            || cancelled.load(Ordering::Acquire)
            || control.action_gate_closed(&frozen.enforcement_epoch, frozen.cleanup_policy_revision)
    };
    let mut gated_runtime = GatedRuntime {
        inner: runtime,
        control: control.clone(),
        enforcement_epoch: frozen.enforcement_epoch.clone(),
        cleanup_policy_revision: frozen.cleanup_policy_revision,
        cancelled,
    };
    let execution = CleanupExecutor::execute(
        &mut gated_runtime,
        &frozen.analyzer,
        &mut journal,
        &mut cleanup_should_stop,
        &frozen.plan,
        &frozen.policy,
    );
    drop(gated_runtime);
    let completed_at = terminal_timestamp(runtime, frozen.started_at_unix_millis);
    let receipt = match execution {
        Ok(receipt) => {
            control
                .store()
                .complete_cleanup_attempt(&frozen.attempt, completed_at, &receipt)?
        }
        Err(error) => {
            if let Some(terminal_receipt) = error.terminal_receipt() {
                control.store().complete_cleanup_attempt(
                    &frozen.attempt,
                    completed_at,
                    terminal_receipt,
                )?;
            } else {
                control.fail_closed(completed_at, error.to_string())?;
            }
            return Err(EngineError::Cleanup(error));
        }
    };
    if receipt_requires_global_fail_close(&receipt) {
        return Err(EngineError::PostDeliveryFailure(
            receipt.incident_id.clone(),
        ));
    }
    if receipt.outcome().process == unlinger_core::ProcessOutcome::Cleared {
        let latest_reclaim = control
            .store()
            .most_recent_reclaim()?
            .map(RecentReclaim::from);
        control.update_status(|status| status.most_recent_reclaim = latest_reclaim)?;
    }
    Ok(receipt)
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
