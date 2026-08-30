mod engine;
mod ipc;
mod paths;
mod store;

pub use engine::{CycleReport, EngineConfig, EngineError, ReconciliationEngine};
pub use ipc::{
    ControlError, ControlPlane, DaemonMode, DaemonStatus, DiagnosticsBundle, IpcClient, IpcCommand,
    IpcError, IpcPayload, IpcServer, RecentReclaim,
};
pub use paths::{LAUNCH_AGENT_LABEL, LocalPaths, PathError};
pub use store::{
    EventKind, EventPayload, HistoryEvent, HistoryStore, IncidentDetail, ObservationRecord,
    PruneResult, RetentionPolicy, StoreError,
};
