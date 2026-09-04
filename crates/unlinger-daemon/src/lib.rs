mod engine;
mod ipc;
mod paths;
mod public_action_policy;
mod public_ipc;
mod store;

pub use engine::{CycleReport, EngineConfig, EngineError, ReconciliationEngine};
pub use ipc::{
    AttentionItem, AttentionKind, AttentionProjection, ControlError, ControlPlane, DaemonMode,
    DaemonStatus, DiagnosticsBundle, IpcClient, IpcCommand, IpcError, IpcPayload, IpcServer,
    RecentReclaim, StartupState, StorageRecoveryStatus,
};
pub use paths::{DaemonInstanceLock, DaemonLockError, LAUNCH_AGENT_LABEL, LocalPaths, PathError};
pub use store::{
    BlockedCleanupSummary, CleanupAttemptHandle, CleanupAttemptJournal, CleanupImpact,
    CleanupImpactSummary, CoolingClock, EventKind, EventPayload, HistoryEvent, HistoryStore,
    ImpactHistoryCompleteness, IncidentDetail, MUTATION_RECONCILIATION_WINDOW_MILLIS,
    ManagedLifecycle, ManagedStartupPhase, MostRecentReclaim, MutationCommit, MutationLookup,
    ObservationRecord, ObservedIncidentIdentity, OrdinaryMutation, PreparedActionHandle,
    PreparedArtifactActionHandle, ProtectedIncidentSummary, ProtectionProjection,
    ProtectionReconciliation, PruneResult, RetentionPolicy, RetryBlockReconciliation,
    StorageRecoveryOccurrence, StorageRecoveryReason, StoreAttentionProjection, StoreError,
};
