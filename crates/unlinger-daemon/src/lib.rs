mod engine;
mod ipc;
mod paths;
mod store;

pub use engine::{CycleReport, EngineConfig, EngineError, ReconciliationEngine};
pub use ipc::{
    AttentionItem, AttentionKind, AttentionProjection, ControlError, ControlPlane, DaemonMode,
    DaemonStatus, DiagnosticsBundle, IpcClient, IpcCommand, IpcError, IpcPayload, IpcServer,
    RecentReclaim, StartupState, StorageRecoveryStatus,
};
pub use paths::{DaemonInstanceLock, DaemonLockError, LAUNCH_AGENT_LABEL, LocalPaths, PathError};
pub use store::{
    BlockedCleanupSummary, CleanupAttemptHandle, CleanupAttemptJournal, CoolingClock, EventKind,
    EventPayload, HistoryEvent, HistoryStore, IncidentDetail, ManagedLifecycle,
    ManagedStartupPhase, MostRecentReclaim, ObservationRecord, ObservedIncidentIdentity,
    PreparedActionHandle, PreparedArtifactActionHandle, ProtectedIncidentSummary,
    ProtectionProjection, ProtectionReconciliation, PruneResult, RetentionPolicy,
    RetryBlockReconciliation, StorageRecoveryOccurrence, StorageRecoveryReason,
    StoreAttentionProjection, StoreError,
};
