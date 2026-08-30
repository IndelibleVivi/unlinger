use crate::{ControlError, ControlPlane, DaemonMode, RecentReclaim, RetentionPolicy, StoreError};
use serde::Serialize;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Duration;
use unlinger_core::{
    CleanupError, CleanupExecutor, CleanupPlan, CleanupPlanError, CleanupPolicy, CleanupReceipt,
    CleanupRuntime, IncidentReport, IncidentState, ProcessGraph, RuntimeFailure,
};
use unlinger_rules::{Analyzer, AnalyzerContext, RuleError, RuleSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineConfig {
    pub observation_gap: Duration,
    pub abandonment_grace: Duration,
    pub cooling_continuity_gap: Duration,
    pub cleanup_policy: CleanupPolicy,
    pub retention_policy: RetentionPolicy,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            observation_gap: Duration::from_secs(15),
            abandonment_grace: Duration::from_secs(90),
            cooling_continuity_gap: Duration::from_secs(120),
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
        self.control.update_status(|status| {
            status.scan_in_progress = true;
            status.last_error = None;
        })?;
        let result = self.run_cycle_inner(now_unix_millis);
        match &result {
            Ok(_) => {
                self.control.update_status(|status| {
                    status.healthy = true;
                    status.scan_in_progress = false;
                    status.cleanup_in_progress = false;
                    status.last_scan_at_unix_millis = Some(now_unix_millis);
                    status.last_error = None;
                })?;
            }
            Err(error) => {
                let message = error.to_string();
                let _ = self.control.update_status(|status| {
                    status.healthy = false;
                    status.scan_in_progress = false;
                    status.cleanup_in_progress = false;
                    status.last_error = Some(message);
                });
            }
        }
        result
    }

    fn run_cycle_inner(&mut self, now_unix_millis: u64) -> Result<CycleReport, EngineError> {
        let first_snapshot = self.runtime.snapshot()?;
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
                first_snapshot.observed_at_unix_millis,
                abandonment_grace_millis,
                continuity_gap_millis,
            )?;
        }
        let needs_second_observation = first_reports
            .iter()
            .any(|report| report.state == IncidentState::Cooling);
        let (incidents, observed_twice) = if needs_second_observation {
            self.runtime.wait(self.config.observation_gap);
            let second_snapshot = self.runtime.snapshot()?;
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
                    second_snapshot.observed_at_unix_millis,
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
            )
        } else {
            self.control.store().retain_cooling(&BTreeSet::new())?;
            (first_reports, false)
        };

        for report in &incidents {
            self.control
                .store()
                .record_observation(now_unix_millis, report)?;
        }

        let status = self.control.status_at(now_unix_millis)?;
        let paused = status
            .paused_until_unix_millis
            .is_some_and(|deadline| deadline > now_unix_millis);
        let mut cleanup_receipts = Vec::new();
        if status.mode == DaemonMode::Enforce && !paused {
            for report in incidents
                .iter()
                .filter(|report| report.state == IncidentState::Confirmed)
            {
                let plan = CleanupPlan::from_confirmed(report)?;
                let started = CleanupReceipt {
                    incident_id: report.incident_id.clone(),
                    state: IncidentState::Reclaiming,
                    reason_id: Some("cleanup.frozen_plan_started".to_owned()),
                    actions: Vec::new(),
                    survivor_pids: Vec::new(),
                    revival_checks_completed: 0,
                };
                self.control
                    .store()
                    .record_cleanup(now_unix_millis, &started)?;
                self.control
                    .update_status(|status| status.cleanup_in_progress = true)?;
                let execution = CleanupExecutor::execute(
                    &mut self.runtime,
                    &analyzer,
                    &plan,
                    &self.config.cleanup_policy,
                );
                let receipt = match execution {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        let completed_at = terminal_timestamp(&self.runtime, now_unix_millis);
                        self.control
                            .store()
                            .record_cleanup(completed_at, error.receipt())?;
                        self.control
                            .update_status(|status| status.cleanup_in_progress = false)?;
                        return Err(EngineError::Cleanup(error));
                    }
                };
                let completed_at = terminal_timestamp(&self.runtime, now_unix_millis);
                self.control
                    .store()
                    .record_cleanup(completed_at, &receipt)?;
                self.control
                    .update_status(|status| status.cleanup_in_progress = false)?;
                if receipt.state == IncidentState::Cleared {
                    self.control.update_status(|status| {
                        status.most_recent_reclaim = Some(RecentReclaim {
                            incident_id: receipt.incident_id.clone(),
                            occurred_at_unix_millis: completed_at,
                            state: receipt.state,
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

fn terminal_timestamp<R: CleanupRuntime>(runtime: &R, started_at_unix_millis: u64) -> u64 {
    runtime
        .now_unix_millis()
        .unwrap_or(started_at_unix_millis)
        .max(started_at_unix_millis)
}
