use crate::{
    CleanupAttemptHandle, HistoryEvent, HistoryStore, IncidentDetail, ManagedLifecycle,
    ManagedStartupPhase, MostRecentReclaim, MutationCommit, MutationLookup, OrdinaryMutation,
    ProtectedIncidentSummary, ProtectionProjection, StorageRecoveryOccurrence,
    StorageRecoveryReason, StoreAttentionProjection, StoreError,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unlinger_core::{CleanupOutcome, IncidentReport, SignalDisposition};
use unlinger_protocol::{MutationContext, MutationOutcome};

use crate::public_action_policy::{RuntimePolicyFacts, evaluate_action};

#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

const IPC_SCHEMA_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_HISTORY_LIMIT: usize = 1_000;
const MAX_PAUSE_MILLIS: u64 = 30 * 24 * 60 * 60 * 1_000;
const MAX_ERROR_CHARS: usize = 512;
const MAX_STATUS_ATTENTION_ITEMS: usize = 16;
const MAX_ROSTER_ITEMS: usize = 32;
const DEFAULT_IPC_CLIENT_IO_TIMEOUT: Duration = Duration::from_secs(15);
const IPC_SERVER_IO_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_IPC_SERVER_WORKERS: usize = 8;
const EVENT_SOURCE_DEGRADED_MESSAGE: &str =
    "native event source unavailable; periodic reconciliation remains active";

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_ENFORCEMENT_EPOCH: AtomicU64 = AtomicU64::new(1);
static NEXT_CYCLE_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonMode {
    #[default]
    ReportOnly,
    Enforce,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupState {
    #[default]
    Legacy,
    Booting,
    Recovering,
    FirstScanReportOnly,
    ReadyReportOnly,
    ReadyEnforce,
    Draining,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentReclaim {
    #[serde(default, skip_serializing)]
    pub event_token: Option<String>,
    pub incident_id: String,
    pub occurred_at_unix_millis: u64,
    pub state: unlinger_core::IncidentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<CleanupOutcome>,
}

impl From<MostRecentReclaim> for RecentReclaim {
    fn from(reclaim: MostRecentReclaim) -> Self {
        Self {
            event_token: Some(reclaim.event_token),
            incident_id: reclaim.incident_id,
            occurred_at_unix_millis: reclaim.occurred_at_unix_millis,
            state: reclaim.state,
            outcome: Some(reclaim.outcome),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    CleanupFailed,
    CleanupRevived,
    DaemonUnhealthy,
    EventSourceDegraded,
    StorageRecovered,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttentionItem {
    #[serde(default, skip_serializing)]
    pub event_token: Option<String>,
    #[serde(default, skip_serializing)]
    pub outcome: Option<CleanupOutcome>,
    pub kind: AttentionKind,
    pub reason_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<unlinger_core::IncidentState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at_unix_millis: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttentionProjection {
    pub blocked_cleanup_count: usize,
    pub items: Vec<AttentionItem>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageRecoveryStatus {
    pub recovery_id: String,
    #[serde(default, skip_serializing)]
    pub public_token: String,
    pub occurred_at_unix_millis: u64,
    pub reason: StorageRecoveryReason,
    pub quarantined_sidecar_count: usize,
}

impl From<&StorageRecoveryOccurrence> for StorageRecoveryStatus {
    fn from(recovery: &StorageRecoveryOccurrence) -> Self {
        Self {
            recovery_id: recovery.recovery_id.clone(),
            public_token: recovery.public_token.clone(),
            occurred_at_unix_millis: recovery.occurred_at_unix_millis,
            reason: recovery.reason,
            quarantined_sidecar_count: recovery.quarantined_sidecar_count,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DaemonStatus {
    #[serde(default)]
    pub lifecycle_schema_version: u32,
    #[serde(default)]
    pub ipc_schema_version: u32,
    #[serde(default)]
    pub database_schema_version: u32,
    #[serde(default)]
    pub daemon_version: String,
    #[serde(default)]
    pub managed: bool,
    #[serde(default)]
    pub instance_id: String,
    pub healthy: bool,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub startup_state: StartupState,
    pub mode: DaemonMode,
    #[serde(default)]
    pub requested_mode: DaemonMode,
    #[serde(default)]
    pub effective_mode: DaemonMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armed_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforcement_epoch: Option<String>,
    #[serde(default)]
    pub draining: bool,
    #[serde(default)]
    pub recovered_cleanup_attempts: usize,
    #[serde(default = "default_true")]
    pub event_source_healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_source_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_recovery: Option<StorageRecoveryStatus>,
    #[serde(default)]
    pub attention: AttentionProjection,
    #[serde(default)]
    pub protected_incident_count: usize,
    #[serde(default)]
    pub protected_incidents: Vec<ProtectedIncidentSummary>,
    pub pid: u32,
    pub scan_in_progress: bool,
    pub cleanup_in_progress: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_until_unix_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_started_at_unix_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_observation_at_unix_millis: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan_at_unix_millis: Option<u64>,
    pub confirmed_incidents: usize,
    pub ambiguous_incidents: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most_recent_reclaim: Option<RecentReclaim>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl DaemonStatus {
    #[must_use]
    pub fn new(mode: DaemonMode, pid: u32) -> Self {
        Self {
            lifecycle_schema_version: 1,
            ipc_schema_version: IPC_SCHEMA_VERSION,
            database_schema_version: HistoryStore::schema_version(),
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            managed: false,
            instance_id: fresh_instance_id(pid, current_wall_millis()),
            healthy: false,
            ready: false,
            startup_state: StartupState::Booting,
            mode,
            requested_mode: mode,
            effective_mode: mode,
            activation_generation: None,
            armed_generation: None,
            enforcement_epoch: (mode == DaemonMode::Enforce)
                .then(|| "unmanaged-explicit-1".to_owned()),
            draining: false,
            recovered_cleanup_attempts: 0,
            event_source_healthy: true,
            last_event_source_error: None,
            storage_recovery: None,
            attention: AttentionProjection::default(),
            protected_incident_count: 0,
            protected_incidents: Vec::new(),
            pid,
            scan_in_progress: false,
            cleanup_in_progress: false,
            paused_until_unix_millis: None,
            cycle_started_at_unix_millis: None,
            latest_observation_at_unix_millis: None,
            last_scan_at_unix_millis: None,
            confirmed_incidents: 0,
            ambiguous_incidents: 0,
            most_recent_reclaim: None,
            last_error: None,
        }
    }

    #[must_use]
    pub fn effective_mode(&self) -> DaemonMode {
        if self.lifecycle_schema_version == 0 {
            self.mode
        } else {
            self.effective_mode
        }
    }

    pub fn set_effective_mode(&mut self, mode: DaemonMode) {
        self.mode = mode;
        self.effective_mode = mode;
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum IpcCommand {
    Status,
    TaskReserve {
        task_id: String,
    },
    TaskActivate {
        task_id: String,
        capability: String,
        owner_pid: u32,
    },
    TaskFinish {
        task_id: String,
        capability: String,
    },
    TaskStatus {
        task_id: String,
    },
    History {
        limit: usize,
    },
    Explain {
        incident_id: String,
    },
    Pause {
        duration_millis: u64,
    },
    Resume,
    Arm {
        activation_generation: u64,
        instance_id: String,
    },
    Disarm {
        activation_generation: u64,
        instance_id: String,
    },
    BeginDrain {
        activation_generation: u64,
        instance_id: String,
    },
    RetryFailedCleanup {
        incident_id: String,
    },
    ProtectIncident {
        incident_id: String,
    },
    UnprotectIncident {
        incident_id: String,
    },
    ExportDiagnostics {
        incident_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticsBundle {
    pub schema_version: u32,
    pub generated_at_unix_millis: u64,
    pub status: DaemonStatus,
    pub incident: IncidentDetail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum IpcPayload {
    Status(DaemonStatus),
    TaskLease(crate::TaskLease),
    TaskStatus(crate::TaskStatus),
    History(Vec<HistoryEvent>),
    Incident(IncidentDetail),
    Pause {
        until_unix_millis: u64,
    },
    Resumed,
    Lifecycle(DaemonStatus),
    RetryScheduled {
        incident_id: String,
    },
    IncidentProtected {
        protection: ProtectedIncidentSummary,
    },
    IncidentUnprotected {
        incident_id: String,
    },
    Diagnostics(DiagnosticsBundle),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlError {
    InvalidArgument(String),
    Conflict(String),
    NotFound(String),
    AuthorityLost(String),
    Store(String),
    Unavailable(String),
}

impl ControlError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "invalid_argument",
            Self::Conflict(_) => "conflict",
            Self::NotFound(_) => "not_found",
            Self::AuthorityLost(_) => "authority_lost",
            Self::Store(_) => "store_error",
            Self::Unavailable(_) => "unavailable",
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::InvalidArgument(message)
            | Self::Conflict(message)
            | Self::NotFound(message)
            | Self::AuthorityLost(message)
            | Self::Store(message)
            | Self::Unavailable(message) => message,
        }
    }
}

impl Display for ControlError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl Error for ControlError {}

impl From<StoreError> for ControlError {
    fn from(value: StoreError) -> Self {
        Self::Store(bounded_message(&value.to_string()))
    }
}

#[derive(Clone)]
pub struct ControlPlane {
    store: HistoryStore,
    status: Arc<Mutex<DaemonStatus>>,
    /// Latest observation snapshot after owner protection is applied. The
    /// active-cycle metadata keeps retained data honest while a replacement is
    /// being assembled or a cycle fails before publication.
    roster: Arc<Mutex<RosterState>>,
    cleanup_policy_revision: Arc<AtomicU64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RosterFreshness {
    Current,
    ScanInProgress,
    StaleAfterFailure,
    NeverObserved,
}

#[derive(Clone, Debug)]
pub(crate) struct RosterSnapshot {
    pub cycle_token: Option<String>,
    pub observed_at_unix_millis: Option<u64>,
    pub freshness: RosterFreshness,
    pub reports: Vec<IncidentReport>,
}

#[derive(Clone, Debug)]
pub(crate) struct BrowserSourceSnapshot {
    pub status: DaemonStatus,
    pub roster: RosterSnapshot,
}

#[derive(Debug)]
struct RosterState {
    cycle_token: Option<String>,
    observed_at_unix_millis: Option<u64>,
    freshness: RosterFreshness,
    reports: Vec<IncidentReport>,
    active_cycle_token: Option<String>,
}

impl Default for RosterState {
    fn default() -> Self {
        Self {
            cycle_token: None,
            observed_at_unix_millis: None,
            freshness: RosterFreshness::NeverObserved,
            reports: Vec::new(),
            active_cycle_token: None,
        }
    }
}

struct RestoredControlState {
    pause_until_unix_millis: Option<u64>,
    most_recent_reclaim: Option<MostRecentReclaim>,
    attention: StoreAttentionProjection,
    protection: ProtectionProjection,
    storage_recovery: Option<StorageRecoveryOccurrence>,
    cleanup_policy_revision: u64,
}

impl RestoredControlState {
    fn load(store: &HistoryStore) -> Result<Self, ControlError> {
        Ok(Self {
            pause_until_unix_millis: store
                .pause_until()
                .map_err(|error| restore_state_error("owner pause", error))?,
            most_recent_reclaim: store
                .most_recent_reclaim()
                .map_err(|error| restore_state_error("most recent reclaim", error))?,
            attention: store
                .attention_projection(MAX_STATUS_ATTENTION_ITEMS)
                .map_err(|error| restore_state_error("attention projection", error))?,
            protection: store
                .protection_projection(MAX_STATUS_ATTENTION_ITEMS)
                .map_err(|error| restore_state_error("protection projection", error))?,
            storage_recovery: store
                .latest_storage_recovery()
                .map_err(|error| restore_state_error("storage recovery", error))?,
            cleanup_policy_revision: store
                .cleanup_policy_revision()
                .map_err(|error| restore_state_error("cleanup policy revision", error))?,
        })
    }
}

impl ControlPlane {
    pub fn new(store: HistoryStore, status: DaemonStatus) -> Result<Self, ControlError> {
        let restored = RestoredControlState::load(&store)?;
        Ok(Self::from_restored(store, status, restored))
    }

    fn from_restored(
        store: HistoryStore,
        mut status: DaemonStatus,
        restored: RestoredControlState,
    ) -> Self {
        status.database_schema_version = HistoryStore::schema_version();
        status.storage_recovery = restored
            .storage_recovery
            .as_ref()
            .map(StorageRecoveryStatus::from);
        status.paused_until_unix_millis = restored.pause_until_unix_millis;
        status.most_recent_reclaim = restored.most_recent_reclaim.map(RecentReclaim::from);
        apply_attention_projection(&mut status, restored.attention);
        apply_protection_projection(&mut status, restored.protection);
        Self {
            store,
            status: Arc::new(Mutex::new(status)),
            roster: Arc::new(Mutex::new(RosterState::default())),
            cleanup_policy_revision: Arc::new(AtomicU64::new(restored.cleanup_policy_revision)),
        }
    }

    pub fn begin_observation_cycle(
        &self,
        started_at_unix_millis: u64,
    ) -> Result<String, ControlError> {
        let cycle_token = fresh_cycle_token(started_at_unix_millis);
        let mut roster = match self.roster.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        roster.active_cycle_token = Some(cycle_token.clone());
        roster.freshness = RosterFreshness::ScanInProgress;
        drop(roster);
        self.update_status(|status| {
            status.scan_in_progress = true;
            status.cycle_started_at_unix_millis = Some(started_at_unix_millis);
            if status.startup_state != StartupState::Failed {
                status.last_error = None;
            }
        })?;
        Ok(cycle_token)
    }

    /// Replaces the observation roster for the active cycle. The cycle remains
    /// `scan_in_progress` until outer reconciliation completes.
    pub fn publish_roster(
        &self,
        cycle_token: &str,
        observed_at_unix_millis: u64,
        reports: Vec<IncidentReport>,
    ) -> Result<(), ControlError> {
        let mut roster = match self.roster.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if roster.active_cycle_token.as_deref() != Some(cycle_token) {
            return Err(ControlError::Unavailable(
                "observation cycle token is no longer active".to_owned(),
            ));
        }
        roster.cycle_token = Some(cycle_token.to_owned());
        roster.observed_at_unix_millis = Some(observed_at_unix_millis);
        roster.reports.clear();
        roster
            .reports
            .extend(reports.into_iter().take(MAX_ROSTER_ITEMS));
        drop(roster);
        self.update_status(|status| {
            status.latest_observation_at_unix_millis = Some(observed_at_unix_millis);
        })
    }

    pub fn finish_observation_cycle(&self, cycle_token: &str, succeeded: bool) {
        let mut roster = match self.roster.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if roster.active_cycle_token.as_deref() != Some(cycle_token) {
            return;
        }
        let replacement_completed = roster.cycle_token.as_deref() == Some(cycle_token);
        roster.freshness = if succeeded || replacement_completed {
            RosterFreshness::Current
        } else {
            RosterFreshness::StaleAfterFailure
        };
        roster.active_cycle_token = None;
    }

    /// Returns the latest published observation plus its honest freshness.
    pub(crate) fn roster_snapshot(&self) -> RosterSnapshot {
        let roster = match self.roster.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        RosterSnapshot {
            cycle_token: roster.cycle_token.clone(),
            observed_at_unix_millis: roster.observed_at_unix_millis,
            freshness: roster.freshness,
            reports: roster.reports.clone(),
        }
    }

    /// Captures the status and latest roster under one in-memory lock
    /// boundary. Durable attention/protection are loaded first, then applied
    /// to that status clone; no SQLite work is performed while either lock is
    /// held.
    pub(crate) fn browser_source_snapshot_at(
        &self,
        now_unix_millis: u64,
    ) -> Result<BrowserSourceSnapshot, ControlError> {
        self.expire_pause(now_unix_millis)?;
        let attention = self
            .store
            .attention_projection(MAX_STATUS_ATTENTION_ITEMS)
            .map_err(map_store_error)?;
        let protection = self
            .store
            .protection_projection(MAX_STATUS_ATTENTION_ITEMS)
            .map_err(map_store_error)?;
        let status_guard = self.lock_status()?;
        let roster_guard = match self.roster.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut status = status_guard.clone();
        let roster = RosterSnapshot {
            cycle_token: roster_guard.cycle_token.clone(),
            observed_at_unix_millis: roster_guard.observed_at_unix_millis,
            freshness: roster_guard.freshness,
            reports: roster_guard.reports.clone(),
        };
        drop(roster_guard);
        drop(status_guard);
        apply_attention_projection(&mut status, attention);
        apply_protection_projection(&mut status, protection);
        Ok(BrowserSourceSnapshot { status, roster })
    }

    pub(crate) fn ordinary_mutation_unavailable_reason(
        &self,
        mutation: &OrdinaryMutation,
    ) -> Result<Option<&'static str>, ControlError> {
        let status = self.lock_status()?;
        self.ordinary_mutation_unavailable_reason_with_status(&status, mutation)
    }

    fn ordinary_mutation_unavailable_reason_with_status(
        &self,
        status: &DaemonStatus,
        mutation: &OrdinaryMutation,
    ) -> Result<Option<&'static str>, ControlError> {
        let runtime_facts = RuntimePolicyFacts {
            draining: status.draining || status.startup_state == StartupState::Draining,
            failed: status.startup_state == StartupState::Failed,
        };
        let store_facts = self
            .store
            .public_action_store_facts(mutation, current_wall_millis())?;
        Ok(evaluate_action(runtime_facts, store_facts, mutation).unavailable_reason())
    }

    pub(crate) fn mutation_lookup(
        &self,
        context: &MutationContext,
    ) -> Result<MutationLookup, ControlError> {
        // This is the same serialization boundary used by mutation admission
        // and lifecycle transitions. A status query therefore cannot overtake
        // an already-admitted mutation and manufacture a transient not_found.
        let _status = self.lock_status()?;
        self.store.mutation_lookup(context).map_err(Into::into)
    }

    pub(crate) fn commit_public_mutation(
        &self,
        context: &MutationContext,
        mutation: &OrdinaryMutation,
        now_unix_millis: u64,
    ) -> Result<MutationCommit, ControlError> {
        let mut status = self.lock_status()?;
        let runtime_facts = RuntimePolicyFacts {
            draining: status.draining || status.startup_state == StartupState::Draining,
            failed: status.startup_state == StartupState::Failed,
        };
        let commit = self
            .store
            .commit_public_ordinary_mutation(context, mutation, runtime_facts, now_unix_millis)
            .map_err(map_store_error)?;
        self.cleanup_policy_revision
            .store(commit.receipt.policy_revision_after, Ordering::Release);
        if !commit.state_changed {
            return Ok(commit);
        }

        let MutationOutcome::Applied { result } = &commit.receipt.outcome else {
            return Err(ControlError::Store(
                "a state-changing mutation receipt lacked an applied result".to_owned(),
            ));
        };
        match result {
            unlinger_protocol::MutationResult::Paused { until_unix_millis } => {
                status.paused_until_unix_millis = Some(*until_unix_millis)
            }
            unlinger_protocol::MutationResult::Resumed => {
                status.paused_until_unix_millis = None;
            }
            unlinger_protocol::MutationResult::RetryScheduled { incident_id } => {
                status.attention.items.retain(|item| {
                    item.incident_id.as_deref() != Some(incident_id.as_str())
                        || !matches!(
                            item.kind,
                            AttentionKind::CleanupFailed | AttentionKind::CleanupRevived
                        )
                });
                status.attention.blocked_cleanup_count =
                    status.attention.blocked_cleanup_count.saturating_sub(1);
            }
            unlinger_protocol::MutationResult::IncidentProtected { protection } => {
                status
                    .protected_incidents
                    .retain(|item| item.incident_id != protection.incident_id);
                status.protected_incidents.push(ProtectedIncidentSummary {
                    incident_id: protection.incident_id.clone(),
                    protected_at_unix_millis: protection.protected_at_unix_millis,
                    last_exact_observed_at_unix_millis: protection
                        .last_exact_observed_at_unix_millis,
                    exact_absence_since_unix_millis: protection.exact_absence_since_unix_millis,
                });
                status.protected_incident_count = status.protected_incidents.len();
            }
            unlinger_protocol::MutationResult::IncidentUnprotected { incident_id } => {
                status
                    .protected_incidents
                    .retain(|item| item.incident_id != *incident_id);
                status.protected_incident_count = status.protected_incidents.len();
            }
        }
        Ok(commit)
    }

    pub fn begin_managed(
        store: HistoryStore,
        activation_generation: u64,
        pid: u32,
        now_unix_millis: u64,
    ) -> Result<Self, ControlError> {
        // Restore every persisted owner/safety surface before mutating the
        // managed lifecycle. A malformed projection must not make a durable
        // Pause disappear while a carried enforce request survives.
        let restored = RestoredControlState::load(&store)?;
        let instance_id = fresh_instance_id(pid, now_unix_millis);
        let lifecycle = store
            .begin_managed_boot(activation_generation, &instance_id, now_unix_millis)
            .map_err(map_store_error)?;
        let mut status = DaemonStatus::new(DaemonMode::ReportOnly, pid);
        status.managed = true;
        status.instance_id = instance_id;
        apply_managed_lifecycle(&mut status, &lifecycle);
        status.healthy = false;
        status.ready = false;
        status.startup_state = StartupState::Recovering;
        Ok(Self::from_restored(store, status, restored))
    }

    pub fn finish_startup_recovery(
        &self,
        recovered_cleanup_attempts: usize,
        now_unix_millis: u64,
    ) -> Result<(), ControlError> {
        let mut status = self.lock_status()?;
        if !status.managed {
            status.recovered_cleanup_attempts = recovered_cleanup_attempts;
            status.startup_state = StartupState::FirstScanReportOnly;
            return Ok(());
        }
        let (generation, instance_id) = managed_identity(&status)?;
        let lifecycle = self
            .store
            .finish_managed_recovery(generation, &instance_id, now_unix_millis)
            .map_err(map_store_error)?;
        apply_managed_lifecycle(&mut status, &lifecycle);
        status.recovered_cleanup_attempts = recovered_cleanup_attempts;
        status.healthy = false;
        status.ready = false;
        status.startup_state = StartupState::FirstScanReportOnly;
        Ok(())
    }

    pub fn complete_successful_cycle(&self, now_unix_millis: u64) -> Result<(), ControlError> {
        let mut status = self.lock_status()?;
        if status.managed && status.startup_state == StartupState::Failed {
            let primary_error = status.last_error.clone();
            let (generation, instance_id) = managed_identity(&status)?;
            match self
                .store
                .fail_managed(generation, &instance_id, now_unix_millis)
            {
                Ok(lifecycle) => apply_managed_lifecycle(&mut status, &lifecycle),
                Err(error) => {
                    status.requested_mode = DaemonMode::ReportOnly;
                    status.set_effective_mode(DaemonMode::ReportOnly);
                    status.armed_generation = None;
                    status.enforcement_epoch = None;
                    status.ready = false;
                    status.startup_state = StartupState::Failed;
                    status.healthy = false;
                    status.last_error = primary_error;
                    return Err(ControlError::Store(bounded_message(&format!(
                        "managed lifecycle remains failed; durable fail-close retry failed: {error}"
                    ))));
                }
            }
            status.healthy = false;
            status.last_error = primary_error;
            return Err(ControlError::Unavailable(
                "managed lifecycle is failed; exact service restart is required".to_owned(),
            ));
        }
        if status.managed && !status.draining && !status.ready {
            if status.startup_state != StartupState::FirstScanReportOnly {
                let (generation, instance_id) = managed_identity(&status)?;
                let message = bounded_message(&format!(
                    "managed cycle completed in invalid startup state {:?}",
                    status.startup_state
                ));
                match self
                    .store
                    .fail_managed(generation, &instance_id, now_unix_millis)
                {
                    Ok(lifecycle) => apply_managed_lifecycle(&mut status, &lifecycle),
                    Err(error) => {
                        status.requested_mode = DaemonMode::ReportOnly;
                        status.set_effective_mode(DaemonMode::ReportOnly);
                        status.armed_generation = None;
                        status.enforcement_epoch = None;
                        status.ready = false;
                        status.startup_state = StartupState::Failed;
                        status.healthy = false;
                        status.last_error = Some(message.clone());
                        return Err(ControlError::Store(bounded_message(&format!(
                            "{message}; durable fail-close also failed: {error}"
                        ))));
                    }
                }
                status.healthy = false;
                status.last_error = Some(message.clone());
                return Err(ControlError::Unavailable(message));
            }
            let (generation, instance_id) = managed_identity(&status)?;
            let epoch =
                fresh_enforcement_epoch(generation, status.pid, &instance_id, now_unix_millis);
            match self.store.complete_managed_first_scan(
                generation,
                &instance_id,
                &epoch,
                now_unix_millis,
            ) {
                Ok(lifecycle) => apply_managed_lifecycle(&mut status, &lifecycle),
                Err(error) => {
                    let completion_error = map_store_error(error);
                    let completion_message = bounded_message(&format!(
                        "managed first-scan completion failed: {completion_error}"
                    ));
                    match self
                        .store
                        .fail_managed(generation, &instance_id, now_unix_millis)
                    {
                        Ok(lifecycle) => apply_managed_lifecycle(&mut status, &lifecycle),
                        Err(fail_error) => {
                            status.requested_mode = DaemonMode::ReportOnly;
                            status.set_effective_mode(DaemonMode::ReportOnly);
                            status.armed_generation = None;
                            status.enforcement_epoch = None;
                            status.ready = false;
                            status.startup_state = StartupState::Failed;
                            status.healthy = false;
                            status.last_error = Some(completion_message);
                            return Err(ControlError::Store(bounded_message(&format!(
                                "{completion_error}; durable fail-close also failed: {fail_error}"
                            ))));
                        }
                    }
                    status.healthy = false;
                    status.last_error = Some(completion_message);
                    return Err(completion_error);
                }
            }
        }
        status.healthy = !status.draining;
        status.ready = status.ready || !status.managed;
        status.startup_state = if status.draining {
            StartupState::Draining
        } else if status.effective_mode() == DaemonMode::Enforce {
            StartupState::ReadyEnforce
        } else {
            StartupState::ReadyReportOnly
        };
        Ok(())
    }

    pub fn fail_closed(
        &self,
        now_unix_millis: u64,
        message: impl Into<String>,
    ) -> Result<(), ControlError> {
        let message = bounded_message(&message.into());
        let mut status = self.lock_status()?;
        let durable_result = if status.managed {
            managed_identity(&status).and_then(|(generation, instance_id)| {
                self.store
                    .fail_managed(generation, &instance_id, now_unix_millis)
                    .map_err(map_store_error)
                    .map(Some)
            })
        } else {
            Ok(None)
        };
        if let Ok(Some(lifecycle)) = &durable_result {
            apply_managed_lifecycle(&mut status, lifecycle);
        }
        // The volatile delivery gate must close even when SQLite cannot make
        // the durable transition. Otherwise the already-running process could
        // send again under its old epoch after reporting the store error.
        status.requested_mode = DaemonMode::ReportOnly;
        status.set_effective_mode(DaemonMode::ReportOnly);
        status.armed_generation = None;
        status.enforcement_epoch = None;
        status.ready = false;
        status.startup_state = StartupState::Failed;
        status.healthy = false;
        status.last_error = Some(message);
        durable_result.map(|_| ())
    }

    pub fn preserve_managed_pre_ready_restart_intent(
        &self,
        now_unix_millis: u64,
    ) -> Result<(), ControlError> {
        let mut status = self.lock_status()?;
        let (generation, instance_id) = managed_identity(&status)?;
        let lifecycle = self
            .store
            .preserve_managed_pre_ready_restart_intent(generation, &instance_id, now_unix_millis)
            .map_err(map_store_error)?;
        apply_managed_lifecycle(&mut status, &lifecycle);
        Ok(())
    }

    #[must_use]
    pub fn is_draining(&self) -> bool {
        self.status.lock().map_or(true, |status| status.draining)
    }

    #[must_use]
    pub(crate) fn cleanup_policy_revision(&self) -> u64 {
        self.cleanup_policy_revision.load(Ordering::Acquire)
    }

    pub(crate) fn action_gate_closed(
        &self,
        enforcement_epoch: &str,
        frozen_policy_revision: u64,
    ) -> bool {
        self.status.lock().map_or(true, |status| {
            !signal_gate_matches(&status, enforcement_epoch)
                || self.cleanup_policy_revision.load(Ordering::Acquire) != frozen_policy_revision
        })
    }

    pub(crate) fn begin_cleanup_attempt_if_armed(
        &self,
        now_unix_millis: u64,
        report: &IncidentReport,
        enforcement_epoch: &str,
        frozen_policy_revision: u64,
    ) -> Result<Option<CleanupAttemptHandle>, ControlError> {
        let status = self.lock_status()?;
        if !signal_gate_matches(&status, enforcement_epoch)
            || self.cleanup_policy_revision.load(Ordering::Acquire) != frozen_policy_revision
        {
            return Ok(None);
        }
        let attempt = self
            .store
            .begin_cleanup_attempt(now_unix_millis, report, enforcement_epoch)
            .map_err(map_store_error)?;
        Ok(Some(attempt))
    }

    pub(crate) fn deliver_signal_if_armed(
        &self,
        enforcement_epoch: &str,
        frozen_policy_revision: u64,
        deliver: impl FnOnce() -> SignalDisposition,
    ) -> SignalDisposition {
        let Ok(status) = self.status.lock() else {
            return SignalDisposition::CancelledBeforeDelivery;
        };
        if !signal_gate_matches(&status, enforcement_epoch)
            || self.cleanup_policy_revision.load(Ordering::Acquire) != frozen_policy_revision
        {
            return SignalDisposition::CancelledBeforeDelivery;
        }
        deliver()
    }

    #[must_use]
    pub fn store(&self) -> &HistoryStore {
        &self.store
    }

    pub fn status(&self) -> Result<DaemonStatus, ControlError> {
        let attention = self
            .store
            .attention_projection(MAX_STATUS_ATTENTION_ITEMS)
            .map_err(map_store_error)?;
        let protection = self
            .store
            .protection_projection(MAX_STATUS_ATTENTION_ITEMS)
            .map_err(map_store_error)?;
        let mut status = self
            .status
            .lock()
            .map(|status| status.clone())
            .map_err(|_| ControlError::Unavailable("daemon status lock is poisoned".to_owned()))?;
        apply_attention_projection(&mut status, attention);
        apply_protection_projection(&mut status, protection);
        Ok(status)
    }

    pub fn status_at(&self, now_unix_millis: u64) -> Result<DaemonStatus, ControlError> {
        self.expire_pause(now_unix_millis)?;
        self.status()
    }

    pub fn update_status(
        &self,
        update: impl FnOnce(&mut DaemonStatus),
    ) -> Result<(), ControlError> {
        let mut status = self.lock_status()?;
        update(&mut status);
        if let Some(error) = &mut status.last_error {
            *error = bounded_message(error);
        }
        if status.last_event_source_error.is_some() {
            status.last_event_source_error = Some(EVENT_SOURCE_DEGRADED_MESSAGE.to_owned());
        }
        Ok(())
    }

    pub fn note_event_source_failure(&self) -> Result<(), ControlError> {
        self.update_status(|status| {
            status.event_source_healthy = false;
            status.last_event_source_error = Some(EVENT_SOURCE_DEGRADED_MESSAGE.to_owned());
        })
    }

    pub fn note_event_source_healthy(&self) -> Result<(), ControlError> {
        self.update_status(|status| {
            status.event_source_healthy = true;
            status.last_event_source_error = None;
        })
    }

    fn lock_status(&self) -> Result<std::sync::MutexGuard<'_, DaemonStatus>, ControlError> {
        self.status
            .lock()
            .map_err(|_| ControlError::Unavailable("daemon status lock is poisoned".to_owned()))
    }

    fn handle_task_peer_at(
        &self,
        command: IpcCommand,
        peer_pid: u32,
        now: u64,
    ) -> Result<IpcPayload, ControlError> {
        use unlinger_core::TaskOwnerIdentity;
        let native = unlinger_macos::MacosSnapshotter::new();
        let peer = native
            .lookup(peer_pid)
            .map_err(|_| {
                ControlError::Unavailable("task registrar identity unavailable".to_owned())
            })?
            .ok_or_else(|| ControlError::Unavailable("task registrar has exited".to_owned()))?;
        let registrar = TaskOwnerIdentity::from_process(&peer).ok_or_else(|| {
            ControlError::InvalidArgument("invalid task registrar identity".to_owned())
        })?;
        match command {
            IpcCommand::TaskReserve { task_id } => {
                let status = self.lock_status()?;
                if !status.healthy
                    || !status.ready
                    || status.draining
                    || status.startup_state == StartupState::Failed
                {
                    return Err(ControlError::Unavailable(
                        "task registration requires a healthy ready daemon".to_owned(),
                    ));
                }
                self.store
                    .reserve_task(&task_id, &registrar, now)
                    .map(IpcPayload::TaskLease)
                    .map_err(map_store_error)
            }
            IpcCommand::TaskActivate {
                task_id,
                capability,
                owner_pid,
            } => {
                let status = self.lock_status()?;
                if !status.healthy
                    || !status.ready
                    || status.draining
                    || status.startup_state == StartupState::Failed
                {
                    return Err(ControlError::Unavailable(
                        "task activation requires a healthy ready daemon".to_owned(),
                    ));
                }
                let scope = self
                    .store
                    .authorize_task(&task_id, &capability)
                    .map_err(map_store_error)?;
                if scope.registrar != registrar {
                    return Err(ControlError::Conflict(
                        "task activation belongs to its original registrar".to_owned(),
                    ));
                }
                let owner = native
                    .lookup(owner_pid)
                    .map_err(|_| {
                        ControlError::Unavailable("command owner identity unavailable".to_owned())
                    })?
                    .ok_or_else(|| ControlError::Conflict("command owner has exited".to_owned()))?;
                if owner.parent_pid != peer_pid || owner.uid != registrar.uid {
                    return Err(ControlError::Conflict(
                        "command owner must be the registrar's exact child".to_owned(),
                    ));
                }
                let identity = TaskOwnerIdentity::from_process(&owner).ok_or_else(|| {
                    ControlError::InvalidArgument("invalid command owner identity".to_owned())
                })?;
                self.store
                    .activate_task(&task_id, &identity, now)
                    .map_err(map_store_error)?;
                self.store
                    .task_status(&task_id)
                    .map_err(map_store_error)?
                    .map(IpcPayload::TaskStatus)
                    .ok_or_else(|| ControlError::NotFound("task not found".to_owned()))
            }
            IpcCommand::TaskFinish {
                task_id,
                capability,
            } => {
                let scope = self
                    .store
                    .authorize_task(&task_id, &capability)
                    .map_err(map_store_error)?;
                if scope.released_at_us.is_none() {
                    if let Some(owner) = &scope.owner {
                        match native.lookup(owner.pid) {
                            Ok(Some(process)) if owner.matches(&process) => {
                                return Err(ControlError::Conflict(
                                    "command owner is still running".to_owned(),
                                ));
                            }
                            Err(_) => {
                                return Err(ControlError::Unavailable(
                                    "command owner absence is unproved".to_owned(),
                                ));
                            }
                            _ => {}
                        }
                    }
                    self.store
                        .release_task(
                            &task_id,
                            now,
                            if scope.owner.is_some() {
                                "reported_exit"
                            } else {
                                "never_started"
                            },
                            scope.owner.as_ref(),
                        )
                        .map_err(map_store_error)?;
                }
                self.store
                    .task_status(&task_id)
                    .map_err(map_store_error)?
                    .map(IpcPayload::TaskStatus)
                    .ok_or_else(|| ControlError::NotFound("task not found".to_owned()))
            }
            _ => self.handle_at(command, now),
        }
    }

    pub fn handle_at(
        &self,
        command: IpcCommand,
        now_unix_millis: u64,
    ) -> Result<IpcPayload, ControlError> {
        match command {
            IpcCommand::Status => Ok(IpcPayload::Status(self.status_at(now_unix_millis)?)),
            IpcCommand::TaskStatus { task_id } => self
                .store
                .task_status(&task_id)
                .map_err(map_store_error)?
                .map(IpcPayload::TaskStatus)
                .ok_or_else(|| ControlError::NotFound("task not found".to_owned())),
            IpcCommand::TaskReserve { .. }
            | IpcCommand::TaskActivate { .. }
            | IpcCommand::TaskFinish { .. } => Err(ControlError::Unavailable(
                "task registration requires an authenticated local socket".to_owned(),
            )),
            IpcCommand::History { limit } => {
                if limit > MAX_HISTORY_LIMIT {
                    return Err(ControlError::InvalidArgument(format!(
                        "history limit must not exceed {MAX_HISTORY_LIMIT}"
                    )));
                }
                Ok(IpcPayload::History(self.store.history(limit)?))
            }
            IpcCommand::Explain { incident_id } => {
                validate_incident_id(&incident_id)?;
                let detail = self.store.explain(&incident_id)?.ok_or_else(|| {
                    ControlError::NotFound(format!("incident {incident_id:?} was not found"))
                })?;
                Ok(IpcPayload::Incident(detail))
            }
            IpcCommand::Pause { duration_millis } => {
                if duration_millis == 0 || duration_millis > MAX_PAUSE_MILLIS {
                    return Err(ControlError::InvalidArgument(format!(
                        "pause duration must be between 1 millisecond and {MAX_PAUSE_MILLIS} milliseconds"
                    )));
                }
                let deadline = now_unix_millis
                    .checked_add(duration_millis)
                    .ok_or_else(|| {
                        ControlError::InvalidArgument("pause deadline overflowed u64".to_owned())
                    })?;
                let mut status = self.lock_status()?;
                self.store.set_pause_until(Some(deadline))?;
                status.paused_until_unix_millis = Some(deadline);
                self.cleanup_policy_revision.fetch_add(1, Ordering::AcqRel);
                Ok(IpcPayload::Pause {
                    until_unix_millis: deadline,
                })
            }
            IpcCommand::Resume => {
                let mut status = self.lock_status()?;
                self.store.set_pause_until(None)?;
                status.paused_until_unix_millis = None;
                self.cleanup_policy_revision.fetch_add(1, Ordering::AcqRel);
                Ok(IpcPayload::Resumed)
            }
            IpcCommand::Arm {
                activation_generation,
                instance_id,
            } => {
                let mut status = self.lock_status()?;
                require_exact_status_identity(&status, activation_generation, &instance_id)?;
                let ready_report_only = status.startup_state == StartupState::ReadyReportOnly
                    && status.effective_mode() == DaemonMode::ReportOnly
                    && status.armed_generation.is_none()
                    && status.enforcement_epoch.is_none();
                let already_ready_enforce = status.startup_state == StartupState::ReadyEnforce
                    && status.effective_mode() == DaemonMode::Enforce
                    && status.armed_generation == Some(activation_generation)
                    && status.enforcement_epoch.is_some();
                if !status.healthy
                    || !status.ready
                    || status.draining
                    || !(ready_report_only || already_ready_enforce)
                {
                    return Err(ControlError::Unavailable(
                        "managed daemon lifecycle is not ready to arm".to_owned(),
                    ));
                }
                let epoch = fresh_enforcement_epoch(
                    activation_generation,
                    status.pid,
                    &status.instance_id,
                    now_unix_millis,
                );
                let lifecycle = self
                    .store
                    .arm_managed(activation_generation, &instance_id, &epoch, now_unix_millis)
                    .map_err(map_store_error)?;
                apply_managed_lifecycle(&mut status, &lifecycle);
                status.healthy = true;
                status.last_error = None;
                Ok(IpcPayload::Lifecycle(status.clone()))
            }
            IpcCommand::Disarm {
                activation_generation,
                instance_id,
            } => {
                let mut status = self.lock_status()?;
                require_exact_status_identity(&status, activation_generation, &instance_id)?;
                let failed = status.startup_state == StartupState::Failed;
                let primary_error = status.last_error.clone();
                let lifecycle = if failed {
                    self.store
                        .fail_managed(activation_generation, &instance_id, now_unix_millis)
                } else {
                    self.store
                        .disarm_managed(activation_generation, &instance_id, now_unix_millis)
                }
                .map_err(map_store_error)?;
                apply_managed_lifecycle(&mut status, &lifecycle);
                if failed {
                    status.healthy = false;
                    status.last_error = primary_error;
                } else {
                    status.last_error = None;
                }
                Ok(IpcPayload::Lifecycle(status.clone()))
            }
            IpcCommand::BeginDrain {
                activation_generation,
                instance_id,
            } => {
                let mut status = self.lock_status()?;
                require_exact_status_identity(&status, activation_generation, &instance_id)?;
                let lifecycle = self
                    .store
                    .begin_managed_drain(activation_generation, &instance_id, now_unix_millis)
                    .map_err(map_store_error)?;
                apply_managed_lifecycle(&mut status, &lifecycle);
                status.healthy = true;
                status.last_error = None;
                Ok(IpcPayload::Lifecycle(status.clone()))
            }
            IpcCommand::RetryFailedCleanup { incident_id } => {
                validate_incident_id(&incident_id)?;
                let _status = self.lock_status()?;
                if !self.store.authorize_retry(&incident_id, now_unix_millis)? {
                    return Err(ControlError::NotFound(format!(
                        "incident {incident_id:?} has no blocked cleanup to retry"
                    )));
                }
                self.cleanup_policy_revision.fetch_add(1, Ordering::AcqRel);
                Ok(IpcPayload::RetryScheduled { incident_id })
            }
            IpcCommand::ProtectIncident { incident_id } => {
                validate_incident_id(&incident_id)?;
                let _status = self.lock_status()?;
                let protection = self
                    .store
                    .protect_incident(&incident_id, now_unix_millis)?
                    .ok_or_else(|| {
                        ControlError::NotFound(format!(
                            "incident {incident_id:?} has no observation to protect"
                        ))
                    })?;
                self.cleanup_policy_revision.fetch_add(1, Ordering::AcqRel);
                Ok(IpcPayload::IncidentProtected { protection })
            }
            IpcCommand::UnprotectIncident { incident_id } => {
                validate_incident_id(&incident_id)?;
                let _status = self.lock_status()?;
                if !self.store.unprotect_incident(&incident_id)? {
                    return Err(ControlError::NotFound(format!(
                        "incident {incident_id:?} has no exact protection override"
                    )));
                }
                self.cleanup_policy_revision.fetch_add(1, Ordering::AcqRel);
                Ok(IpcPayload::IncidentUnprotected { incident_id })
            }
            IpcCommand::ExportDiagnostics { incident_id } => {
                validate_incident_id(&incident_id)?;
                let incident = self.store.explain(&incident_id)?.ok_or_else(|| {
                    ControlError::NotFound(format!("incident {incident_id:?} was not found"))
                })?;
                Ok(IpcPayload::Diagnostics(DiagnosticsBundle {
                    schema_version: 1,
                    generated_at_unix_millis: now_unix_millis,
                    status: self.status()?,
                    incident,
                }))
            }
        }
    }

    fn expire_pause(&self, now_unix_millis: u64) -> Result<(), ControlError> {
        self.expire_pause_with_hook(now_unix_millis, || {})
    }

    fn expire_pause_with_hook(
        &self,
        now_unix_millis: u64,
        before_store_clear: impl FnOnce(),
    ) -> Result<(), ControlError> {
        let mut status = self.lock_status()?;
        if status
            .paused_until_unix_millis
            .is_some_and(|deadline| deadline <= now_unix_millis)
        {
            before_store_clear();
            self.store.set_pause_until(None)?;
            status.paused_until_unix_millis = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod control_plane_tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Barrier, mpsc};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct TempState(PathBuf);

    impl TempState {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos();
            let directory = std::env::temp_dir().join(format!(
                "unlinger-expire-pause-{}-{nonce:x}",
                std::process::id()
            ));
            fs::create_dir(&directory).expect("create temp directory");
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .expect("protect temp directory");
            Self(directory)
        }
    }

    impl Drop for TempState {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn default_client_waits_through_bounded_ipc_head_of_line_work() {
        let client = IpcClient::new("/tmp/unlinger-timeout-contract.sock");
        assert_eq!(client.io_timeout, Duration::from_secs(15));
        assert_eq!(IPC_SERVER_IO_TIMEOUT, Duration::from_secs(3));
    }

    #[test]
    fn expired_pause_clear_and_new_pause_ack_are_serialized() {
        let temp = TempState::new();
        let store = HistoryStore::open(temp.0.join("history.sqlite3")).expect("open history");
        let control = ControlPlane::new(
            store,
            DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
        )
        .expect("restore control state");
        control
            .handle_at(
                IpcCommand::Pause {
                    duration_millis: 10,
                },
                0,
            )
            .expect("seed expired pause");

        let expire_entered = Arc::new(Barrier::new(2));
        let allow_expire = Arc::new(Barrier::new(2));
        let expire_control = control.clone();
        let expire_entered_worker = Arc::clone(&expire_entered);
        let allow_expire_worker = Arc::clone(&allow_expire);
        let expire = thread::spawn(move || {
            expire_control.expire_pause_with_hook(20, || {
                expire_entered_worker.wait();
                allow_expire_worker.wait();
            })
        });
        expire_entered.wait();

        let (pause_sent, pause_received) = mpsc::channel();
        let pause_control = control.clone();
        let pause = thread::spawn(move || {
            let result = pause_control.handle_at(
                IpcCommand::Pause {
                    duration_millis: 1_000,
                },
                20,
            );
            pause_sent.send(result).expect("send pause result");
        });
        assert!(
            pause_received
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "new Pause must not ACK while an expired clear holds the status/store boundary"
        );

        allow_expire.wait();
        expire
            .join()
            .expect("join expiry thread")
            .expect("expire old pause");
        let payload = pause_received
            .recv_timeout(Duration::from_secs(1))
            .expect("new Pause ACK after expiry")
            .expect("new Pause succeeds");
        pause.join().expect("join pause thread");
        assert_eq!(
            payload,
            IpcPayload::Pause {
                until_unix_millis: 1_020
            }
        );
        assert_eq!(
            control
                .status_at(20)
                .expect("read current status")
                .paused_until_unix_millis,
            Some(1_020)
        );
        assert_eq!(
            control.store().pause_until().expect("read durable pause"),
            Some(1_020)
        );
    }
}

fn apply_protection_projection(status: &mut DaemonStatus, projection: ProtectionProjection) {
    status.protected_incident_count = projection.protected_incident_count;
    status.protected_incidents = projection.protected_incidents;
}

fn apply_attention_projection(
    status: &mut DaemonStatus,
    store_projection: StoreAttentionProjection,
) {
    let mut items = Vec::with_capacity(MAX_STATUS_ATTENTION_ITEMS);
    if let Some(recovery) = &status.storage_recovery {
        let reason_id = match recovery.reason {
            StorageRecoveryReason::IntegrityCheckFailed => "storage.integrity_recovered",
            StorageRecoveryReason::RequiredSchemaInvalid => "storage.schema_recovered",
        };
        items.push(AttentionItem {
            event_token: Some(recovery.public_token.clone()),
            outcome: None,
            kind: AttentionKind::StorageRecovered,
            reason_id: reason_id.to_owned(),
            incident_id: None,
            state: None,
            occurred_at_unix_millis: Some(recovery.occurred_at_unix_millis),
        });
    }
    if !status.event_source_healthy {
        items.push(AttentionItem {
            event_token: None,
            outcome: None,
            kind: AttentionKind::EventSourceDegraded,
            reason_id: "runtime.event_source_degraded".to_owned(),
            incident_id: None,
            state: None,
            occurred_at_unix_millis: None,
        });
    }
    if !status.healthy {
        items.push(AttentionItem {
            event_token: None,
            outcome: None,
            kind: AttentionKind::DaemonUnhealthy,
            reason_id: if status.last_error.is_some() {
                "daemon.runtime_error"
            } else {
                "daemon.not_ready"
            }
            .to_owned(),
            incident_id: None,
            state: None,
            occurred_at_unix_millis: None,
        });
    }
    for blocked in store_projection.blocked_cleanups {
        if items.len() == MAX_STATUS_ATTENTION_ITEMS {
            break;
        }
        items.push(AttentionItem {
            event_token: blocked.event_token,
            outcome: Some(blocked.outcome),
            kind: if blocked.state == unlinger_core::IncidentState::Revived {
                AttentionKind::CleanupRevived
            } else {
                AttentionKind::CleanupFailed
            },
            reason_id: blocked.reason_id,
            incident_id: Some(blocked.incident_id),
            state: Some(blocked.state),
            occurred_at_unix_millis: Some(blocked.blocked_at_unix_millis),
        });
    }
    status.attention = AttentionProjection {
        blocked_cleanup_count: store_projection.blocked_cleanup_count,
        items,
    };
}

fn managed_identity(status: &DaemonStatus) -> Result<(u64, String), ControlError> {
    if !status.managed {
        return Err(ControlError::Unavailable(
            "daemon is not managed by an activation generation".to_owned(),
        ));
    }
    let generation = status.activation_generation.ok_or_else(|| {
        ControlError::Unavailable("managed daemon has no activation generation".to_owned())
    })?;
    if status.instance_id.is_empty() {
        return Err(ControlError::Unavailable(
            "managed daemon has no instance identity".to_owned(),
        ));
    }
    Ok((generation, status.instance_id.clone()))
}

fn require_exact_status_identity(
    status: &DaemonStatus,
    activation_generation: u64,
    instance_id: &str,
) -> Result<(), ControlError> {
    let (active_generation, active_instance) = managed_identity(status)?;
    if active_generation != activation_generation {
        return Err(ControlError::InvalidArgument(format!(
            "activation generation mismatch: active {active_generation}, requested {activation_generation}"
        )));
    }
    if active_instance != instance_id {
        return Err(ControlError::InvalidArgument(
            "instance ID does not match the active daemon".to_owned(),
        ));
    }
    Ok(())
}

fn apply_managed_lifecycle(status: &mut DaemonStatus, lifecycle: &ManagedLifecycle) {
    status.managed = true;
    status.activation_generation = Some(lifecycle.activation_generation);
    status.instance_id.clone_from(&lifecycle.instance_id);
    status.requested_mode = if lifecycle.requested_enforce {
        DaemonMode::Enforce
    } else {
        DaemonMode::ReportOnly
    };
    status.set_effective_mode(if lifecycle.effective_enforce {
        DaemonMode::Enforce
    } else {
        DaemonMode::ReportOnly
    });
    status.armed_generation = lifecycle.armed_generation;
    status
        .enforcement_epoch
        .clone_from(&lifecycle.enforcement_epoch);
    status.ready = lifecycle.ready;
    status.draining = lifecycle.draining;
    status.startup_state = match lifecycle.startup_phase {
        ManagedStartupPhase::Recovering => StartupState::Recovering,
        ManagedStartupPhase::FirstScanReportOnly => StartupState::FirstScanReportOnly,
        ManagedStartupPhase::ReadyReportOnly => StartupState::ReadyReportOnly,
        ManagedStartupPhase::ReadyEnforce => StartupState::ReadyEnforce,
        ManagedStartupPhase::Draining => StartupState::Draining,
        ManagedStartupPhase::Failed => StartupState::Failed,
    };
}

fn signal_gate_matches(status: &DaemonStatus, enforcement_epoch: &str) -> bool {
    if status.draining
        || status.effective_mode() != DaemonMode::Enforce
        || status.enforcement_epoch.as_deref() != Some(enforcement_epoch)
    {
        return false;
    }
    !status.managed
        || (status.ready
            && status.activation_generation.is_some()
            && status.armed_generation == status.activation_generation)
}

fn fresh_instance_id(pid: u32, now_unix_millis: u64) -> String {
    let sequence = NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);
    let wall_nanos = current_wall_nanos();
    format!("{pid:x}-{now_unix_millis:x}-{wall_nanos:x}-{sequence:x}")
}

fn fresh_enforcement_epoch(
    activation_generation: u64,
    pid: u32,
    instance_id: &str,
    now_unix_millis: u64,
) -> String {
    let sequence = NEXT_ENFORCEMENT_EPOCH.fetch_add(1, Ordering::Relaxed);
    format!("g{activation_generation:x}-p{pid:x}-i{instance_id}-{now_unix_millis:x}-{sequence:x}")
}

fn fresh_cycle_token(now_unix_millis: u64) -> String {
    let sequence = NEXT_CYCLE_TOKEN.fetch_add(1, Ordering::Relaxed);
    let entropy =
        (current_wall_nanos() as u64) ^ now_unix_millis.rotate_left(17) ^ sequence.rotate_left(41);
    format!(
        "{:016x}{:016x}",
        splitmix64(entropy),
        splitmix64(entropy ^ 0x9e37_79b9_7f4a_7c15)
    )
}

const fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn current_wall_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

const fn default_true() -> bool {
    true
}

fn current_wall_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn map_store_error(error: StoreError) -> ControlError {
    match error {
        StoreError::Invalid(message) | StoreError::Range(message) => {
            ControlError::InvalidArgument(bounded_message(&message))
        }
        StoreError::Conflict(message) => ControlError::Conflict(bounded_message(&message)),
        StoreError::AuthorityLost(message) => {
            ControlError::AuthorityLost(bounded_message(&message))
        }
        StoreError::NotFound(message) | StoreError::Capacity(message) => {
            ControlError::Unavailable(bounded_message(&message))
        }
        other => ControlError::Store(bounded_message(&other.to_string())),
    }
}

fn restore_state_error(surface: &str, error: StoreError) -> ControlError {
    ControlError::Store(bounded_message(&format!(
        "could not restore {surface}: {error}"
    )))
}

fn validate_incident_id(incident_id: &str) -> Result<(), ControlError> {
    if incident_id.is_empty() || incident_id.len() > 128 {
        Err(ControlError::InvalidArgument(
            "incident ID must contain 1 to 128 bytes".to_owned(),
        ))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RequestEnvelope {
    schema_version: u32,
    request_id: u64,
    command: IpcCommand,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct RequestHeader {
    schema_version: u32,
    request_id: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResponseEnvelope {
    schema_version: u32,
    request_id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<IpcPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<IpcErrorBody>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IpcErrorBody {
    code: String,
    message: String,
}

#[derive(Debug)]
pub enum IpcError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Protocol(String),
    Remote { code: String, message: String },
}

impl Display for IpcError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local IPC I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "local IPC JSON failed: {error}"),
            Self::Protocol(message) => write!(formatter, "local IPC protocol failed: {message}"),
            Self::Remote { code, message } => {
                write!(formatter, "daemon rejected {code}: {message}")
            }
        }
    }
}

impl Error for IpcError {}

impl From<std::io::Error> for IpcError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Clone, Debug)]
pub struct IpcClient {
    socket_path: PathBuf,
    io_timeout: Duration,
}

impl IpcClient {
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self::with_io_timeout(path, DEFAULT_IPC_CLIENT_IO_TIMEOUT)
    }

    /// Builds a one-request client with an explicit bounded I/O timeout.
    ///
    /// A response can legitimately wait behind another request's SQLite
    /// projections. This changes only how long the one request waits; it does
    /// not retry a command whose delivery outcome may be uncertain.
    #[must_use]
    pub fn with_io_timeout(path: impl AsRef<Path>, io_timeout: Duration) -> Self {
        Self {
            socket_path: path.as_ref().to_path_buf(),
            io_timeout,
        }
    }

    #[cfg(unix)]
    pub fn request(&self, command: IpcCommand) -> Result<IpcPayload, IpcError> {
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = RequestEnvelope {
            schema_version: IPC_SCHEMA_VERSION,
            request_id,
            command,
        };
        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.set_read_timeout(Some(self.io_timeout))?;
        stream.set_write_timeout(Some(self.io_timeout))?;
        serde_json::to_writer(&mut stream, &request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        let response_bytes = read_bounded_line(&mut stream, MAX_RESPONSE_BYTES)?;
        let response = serde_json::from_slice::<ResponseEnvelope>(&response_bytes)?;
        if response.schema_version != IPC_SCHEMA_VERSION || response.request_id != request_id {
            return Err(IpcError::Protocol(
                "response schema or request ID did not match".to_owned(),
            ));
        }
        match (response.ok, response.payload, response.error) {
            (true, Some(payload), None) => Ok(payload),
            (false, None, Some(error)) => Err(IpcError::Remote {
                code: error.code,
                message: error.message,
            }),
            _ => Err(IpcError::Protocol(
                "response success/error fields were contradictory".to_owned(),
            )),
        }
    }

    /// Reads the schema-v4 browser product projection with the same bounded,
    /// single-attempt transport contract as operator IPC.
    #[cfg(unix)]
    pub fn request_browser_overview(
        &self,
    ) -> Result<unlinger_protocol::BrowserOverviewSnapshot, IpcError> {
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = unlinger_protocol::RequestEnvelope::new(
            request_id,
            unlinger_protocol::Command::BrowserOverview,
        );
        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.set_read_timeout(Some(self.io_timeout))?;
        stream.set_write_timeout(Some(self.io_timeout))?;
        serde_json::to_writer(&mut stream, &request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        let response_bytes = read_bounded_line(&mut stream, MAX_RESPONSE_BYTES)?;
        let response =
            serde_json::from_slice::<unlinger_protocol::ResponseEnvelope>(&response_bytes)?;
        if response.schema_version != unlinger_protocol::SCHEMA_VERSION
            || response.request_id != request_id
        {
            return Err(IpcError::Protocol(
                "browser response schema or request ID did not match".to_owned(),
            ));
        }
        match (response.ok, response.payload, response.error) {
            (true, Some(unlinger_protocol::Payload::BrowserOverview(overview)), None) => {
                Ok(overview)
            }
            (true, Some(_), None) => Err(IpcError::Protocol(
                "daemon returned the wrong payload for browser overview".to_owned(),
            )),
            (false, None, Some(error)) => Err(IpcError::Remote {
                code: public_error_code_name(error.code).to_owned(),
                message: error.message,
            }),
            _ => Err(IpcError::Protocol(
                "browser response success/error fields were contradictory".to_owned(),
            )),
        }
    }
}

fn public_error_code_name(code: unlinger_protocol::ErrorCode) -> &'static str {
    match code {
        unlinger_protocol::ErrorCode::InvalidRequest => "invalid_request",
        unlinger_protocol::ErrorCode::InvalidJson => "invalid_json",
        unlinger_protocol::ErrorCode::UnsupportedSchema => "unsupported_schema",
        unlinger_protocol::ErrorCode::InvalidArgument => "invalid_argument",
        unlinger_protocol::ErrorCode::Conflict => "conflict",
        unlinger_protocol::ErrorCode::NotFound => "not_found",
        unlinger_protocol::ErrorCode::AuthorityLost => "authority_lost",
        unlinger_protocol::ErrorCode::StoreError => "store_error",
        unlinger_protocol::ErrorCode::Unavailable => "unavailable",
    }
}

pub struct IpcServer {
    socket_path: PathBuf,
    socket_device: u64,
    socket_inode: u64,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl IpcServer {
    #[cfg(unix)]
    pub fn start(path: impl AsRef<Path>, control: ControlPlane) -> Result<Self, IpcError> {
        let path = path.as_ref().to_path_buf();
        prepare_socket_parent(&path)?;
        let listener = bind_local_socket(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let metadata = fs::metadata(&path)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("unlinger-ipc".to_owned())
            .spawn(move || {
                let mut workers = Vec::<JoinHandle<()>>::new();
                while !thread_shutdown.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if thread_shutdown.load(Ordering::Acquire) {
                                break;
                            }
                            if !peer_is_current_user(&stream) {
                                continue;
                            }
                            if stream
                                .set_read_timeout(Some(IPC_SERVER_IO_TIMEOUT))
                                .is_err()
                                || stream
                                    .set_write_timeout(Some(IPC_SERVER_IO_TIMEOUT))
                                    .is_err()
                            {
                                continue;
                            }
                            reap_finished_workers(&mut workers);
                            if workers.len() >= MAX_IPC_SERVER_WORKERS {
                                continue;
                            }
                            let worker_control = control.clone();
                            if let Ok(worker) = thread::Builder::new()
                                .name("unlinger-ipc-client".to_owned())
                                .spawn(move || {
                                    let _ = serve_connection(&mut stream, &worker_control);
                                })
                            {
                                workers.push(worker);
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                for worker in workers {
                    let _ = worker.join();
                }
            })?;

        Ok(Self {
            socket_path: path,
            socket_device: metadata.dev(),
            socket_inode: metadata.ino(),
            shutdown,
            thread: Some(thread),
        })
    }
}

fn reap_finished_workers(workers: &mut Vec<JoinHandle<()>>) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let worker = workers.swap_remove(index);
            let _ = worker.join();
        } else {
            index += 1;
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        #[cfg(unix)]
        {
            let _ = UnixStream::connect(&self.socket_path);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        #[cfg(unix)]
        if let Ok(metadata) = fs::symlink_metadata(&self.socket_path)
            && metadata.file_type().is_socket()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.dev() == self.socket_device
            && metadata.ino() == self.socket_inode
        {
            let _ = fs::remove_file(&self.socket_path);
        }
    }
}

#[cfg(unix)]
fn prepare_socket_parent(path: &Path) -> Result<(), IpcError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| IpcError::Protocol("socket path has no parent directory".to_owned()))?;
    let existed = parent.exists();
    fs::create_dir_all(parent)?;
    if !existed {
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(IpcError::Protocol(format!(
            "socket parent must be an owner-private 0700 directory: {}",
            parent.display()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn bind_local_socket(path: &Path) -> Result<UnixListener, IpcError> {
    match UnixListener::bind(path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.file_type().is_socket()
                || metadata.file_type().is_symlink()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.permissions().mode() & 0o777 != 0o600
            {
                return Err(IpcError::Protocol(format!(
                    "refusing to replace an unsafe socket path {}",
                    path.display()
                )));
            }
            match UnixStream::connect(path) {
                Ok(_) => {
                    return Err(IpcError::Protocol(format!(
                        "another daemon is already listening at {}",
                        path.display()
                    )));
                }
                Err(connect_error)
                    if matches!(
                        connect_error.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                    ) => {}
                Err(connect_error) => {
                    return Err(IpcError::Protocol(format!(
                        "could not prove socket {} is stale: {connect_error}",
                        path.display()
                    )));
                }
            }
            fs::remove_file(path)?;
            UnixListener::bind(path).map_err(IpcError::Io)
        }
        Err(error) => Err(IpcError::Io(error)),
    }
}

#[cfg(unix)]
fn peer_is_current_user(stream: &UnixStream) -> bool {
    let mut peer_uid = 0;
    let mut peer_gid = 0;
    (unsafe { libc::getpeereid(stream.as_raw_fd(), &raw mut peer_uid, &raw mut peer_gid) }) == 0
        && peer_uid == unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn peer_pid(stream: &UnixStream) -> Result<u32, ControlError> {
    let mut pid: libc::pid_t = 0;
    let mut size = std::mem::size_of_val(&pid) as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast(),
            &raw mut size,
        )
    };
    if result != 0 || pid <= 1 || size as usize != std::mem::size_of_val(&pid) {
        return Err(ControlError::Unavailable(
            "local task peer identity unavailable".to_owned(),
        ));
    }
    Ok(pid as u32)
}

#[cfg(unix)]
fn serve_connection(stream: &mut UnixStream, control: &ControlPlane) -> Result<(), IpcError> {
    let request_bytes = match read_bounded_line(stream, MAX_REQUEST_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            write_response(
                stream,
                &ResponseEnvelope {
                    schema_version: IPC_SCHEMA_VERSION,
                    request_id: 0,
                    ok: false,
                    payload: None,
                    error: Some(IpcErrorBody {
                        code: "invalid_request".to_owned(),
                        message: bounded_message(&error.to_string()),
                    }),
                },
            )?;
            return Ok(());
        }
    };
    let header = match serde_json::from_slice::<RequestHeader>(&request_bytes) {
        Ok(header) => header,
        Err(error) => {
            write_response(
                stream,
                &ResponseEnvelope {
                    schema_version: IPC_SCHEMA_VERSION,
                    request_id: 0,
                    ok: false,
                    payload: None,
                    error: Some(IpcErrorBody {
                        code: "invalid_json".to_owned(),
                        message: bounded_message(&error.to_string()),
                    }),
                },
            )?;
            return Ok(());
        }
    };
    match header.schema_version {
        IPC_SCHEMA_VERSION => {
            let request = match serde_json::from_slice::<RequestEnvelope>(&request_bytes) {
                Ok(request) => request,
                Err(error) => {
                    write_response(
                        stream,
                        &ResponseEnvelope {
                            schema_version: IPC_SCHEMA_VERSION,
                            request_id: 0,
                            ok: false,
                            payload: None,
                            error: Some(IpcErrorBody {
                                code: "invalid_json".to_owned(),
                                message: bounded_message(&error.to_string()),
                            }),
                        },
                    )?;
                    return Ok(());
                }
            };
            let now = now_unix_millis()?;
            let result = match request.command {
                command @ (IpcCommand::TaskReserve { .. }
                | IpcCommand::TaskActivate { .. }
                | IpcCommand::TaskFinish { .. }) => {
                    peer_pid(stream).and_then(|pid| control.handle_task_peer_at(command, pid, now))
                }
                command => control.handle_at(command, now),
            };
            let response = match result {
                Ok(payload) => ResponseEnvelope {
                    schema_version: IPC_SCHEMA_VERSION,
                    request_id: request.request_id,
                    ok: true,
                    payload: Some(payload),
                    error: None,
                },
                Err(error) => ResponseEnvelope {
                    schema_version: IPC_SCHEMA_VERSION,
                    request_id: request.request_id,
                    ok: false,
                    payload: None,
                    error: Some(IpcErrorBody {
                        code: error.code().to_owned(),
                        message: bounded_message(error.message()),
                    }),
                },
            };
            write_response(stream, &response)
        }
        unlinger_protocol::LEGACY_SCHEMA_VERSION
        | unlinger_protocol::PREVIOUS_SCHEMA_VERSION
        | unlinger_protocol::SCHEMA_VERSION => {
            let frontend_schema_version = header.schema_version;
            let request = match serde_json::from_slice::<unlinger_protocol::RequestEnvelope>(
                &request_bytes,
            ) {
                Ok(request) => request,
                Err(error) => {
                    let response = unlinger_protocol::ResponseEnvelope::failure_for(
                        frontend_schema_version,
                        header.request_id,
                        unlinger_protocol::ErrorCode::InvalidJson,
                        bounded_message(&error.to_string()),
                    );
                    write_public_response(stream, &response)?;
                    return Ok(());
                }
            };
            if frontend_schema_version == unlinger_protocol::LEGACY_SCHEMA_VERSION
                && matches!(request.command, unlinger_protocol::Command::BrowserOverview)
            {
                let response = unlinger_protocol::ResponseEnvelope::failure_for(
                    frontend_schema_version,
                    request.request_id,
                    unlinger_protocol::ErrorCode::InvalidRequest,
                    "browser_overview requires frontend schema 4 or 5",
                );
                write_public_response(stream, &response)?;
                return Ok(());
            }
            let response = match crate::public_ipc::handle_at(
                control,
                request.command,
                now_unix_millis()?,
                frontend_schema_version,
            ) {
                Ok(payload) => unlinger_protocol::ResponseEnvelope::success_for(
                    frontend_schema_version,
                    request.request_id,
                    payload,
                ),
                Err(error) => unlinger_protocol::ResponseEnvelope::failure_for(
                    frontend_schema_version,
                    request.request_id,
                    public_error_code(&error),
                    public_error_message(&error),
                ),
            };
            write_public_response(stream, &response)
        }
        _ => write_response(
            stream,
            &ResponseEnvelope {
                schema_version: IPC_SCHEMA_VERSION,
                request_id: header.request_id,
                ok: false,
                payload: None,
                error: Some(IpcErrorBody {
                    code: "unsupported_schema".to_owned(),
                    message: format!(
                        "supported IPC schemas are {IPC_SCHEMA_VERSION}, {}, {}, and {}",
                        unlinger_protocol::LEGACY_SCHEMA_VERSION,
                        unlinger_protocol::PREVIOUS_SCHEMA_VERSION,
                        unlinger_protocol::SCHEMA_VERSION
                    ),
                }),
            },
        ),
    }
}

fn write_response(stream: &mut impl Write, response: &ResponseEnvelope) -> Result<(), IpcError> {
    serde_json::to_writer(&mut *stream, response)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn write_public_response(
    stream: &mut impl Write,
    response: &unlinger_protocol::ResponseEnvelope,
) -> Result<(), IpcError> {
    serde_json::to_writer(&mut *stream, response)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn read_bounded_line(reader: &mut impl Read, max_bytes: u64) -> Result<Vec<u8>, IpcError> {
    let mut buffered = BufReader::new(reader.take(max_bytes + 1));
    let mut bytes = Vec::new();
    buffered.read_until(b'\n', &mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(IpcError::Protocol(format!(
            "message exceeds {max_bytes} byte limit"
        )));
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.is_empty() {
        return Err(IpcError::Protocol("empty IPC message".to_owned()));
    }
    Ok(bytes)
}

fn now_unix_millis() -> Result<u64, IpcError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| IpcError::Protocol(format!("wall clock failed: {error}")))?
        .as_millis();
    u64::try_from(millis).map_err(|_| IpcError::Protocol("wall clock overflowed u64".to_owned()))
}

fn bounded_message(message: &str) -> String {
    message.chars().take(MAX_ERROR_CHARS).collect()
}

fn public_error_message(error: &ControlError) -> &'static str {
    match error {
        ControlError::InvalidArgument(_) => "request argument is invalid",
        ControlError::Conflict(_) => "mutation identity conflicts with a committed request",
        ControlError::NotFound(_) => "requested local record was not found",
        ControlError::AuthorityLost(_) => "mutation receipt authority is no longer available",
        ControlError::Store(_) => "local history is unavailable",
        ControlError::Unavailable(_) => "daemon operation is unavailable",
    }
}

fn public_error_code(error: &ControlError) -> unlinger_protocol::ErrorCode {
    match error {
        ControlError::InvalidArgument(_) => unlinger_protocol::ErrorCode::InvalidArgument,
        ControlError::Conflict(_) => unlinger_protocol::ErrorCode::Conflict,
        ControlError::NotFound(_) => unlinger_protocol::ErrorCode::NotFound,
        ControlError::AuthorityLost(_) => unlinger_protocol::ErrorCode::AuthorityLost,
        ControlError::Store(_) => unlinger_protocol::ErrorCode::StoreError,
        ControlError::Unavailable(_) => unlinger_protocol::ErrorCode::Unavailable,
    }
}
