use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutableIdentity {
    pub device: Option<u64>,
    pub inode: Option<u64>,
    pub size: Option<u64>,
    pub modified_unix_nanos: Option<i128>,
}

impl ExecutableIdentity {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.device.is_some() && self.inode.is_some()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub started_at_unix_micros: u64,
    pub executable_device: Option<u64>,
    pub executable_inode: Option<u64>,
}

impl ProcessIdentity {
    #[must_use]
    pub fn exact_match(&self, other: &Self) -> bool {
        self == other && self.executable_device.is_some() && self.executable_inode.is_some()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Running,
    Sleeping,
    Stopped,
    Zombie,
    Other(u32),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessRecord {
    pub identity: ProcessIdentity,
    pub parent_pid: u32,
    pub process_group_id: u32,
    pub uid: u32,
    pub tty_device: Option<u32>,
    pub name: String,
    pub executable_path: Option<String>,
    pub executable: ExecutableIdentity,
    pub arguments: Option<Vec<String>>,
    pub resident_memory_bytes: u64,
    pub status: ProcessStatus,
}

impl ProcessRecord {
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.identity.pid
    }

    #[must_use]
    pub fn executable_basename(&self) -> String {
        self.executable_path
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.name)
            .to_owned()
    }

    #[must_use]
    pub fn has_complete_classification_facts(&self) -> bool {
        self.executable_path.is_some()
            && self.arguments.is_some()
            && self.executable.is_complete()
            && self.identity.executable_device.is_some()
            && self.identity.executable_inode.is_some()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotCoverage {
    pub listed_processes: usize,
    pub inspected_processes: usize,
    pub unreadable_processes: usize,
    pub arguments_unavailable: usize,
    pub executable_identity_unavailable: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Snapshot {
    pub observed_at_unix_millis: u64,
    pub current_uid: u32,
    pub processes: Vec<ProcessRecord>,
    pub coverage: SnapshotCoverage,
}
