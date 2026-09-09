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
    /// Native runtime safety facts are transient inputs. They are never
    /// serialized into fixtures, history, IPC, or diagnostics.
    #[serde(skip)]
    pub runtime: ProcessRuntimeFacts,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppBundleVersion {
    pub bundle_id: String,
    pub short_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaywrightCliRuntime {
    pub session_name: String,
    pub version: String,
    pub persistent: bool,
    pub attached: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessRuntimeFacts {
    pub cpu_total_nanos: u64,
    pub open_file_descriptors: usize,
    pub descriptor_facts_complete: bool,
    pub debug_transport_facts_complete: bool,
    pub tcp_listening_ports: Vec<u16>,
    pub tcp_established_local_ports: Vec<u16>,
    pub connected_unix_sockets: usize,
    pub attached_debug_transport: bool,
    pub app_bundle: Option<AppBundleVersion>,
    pub crashpad_bundle: Option<AppBundleVersion>,
    pub task_session_name: Option<String>,
    pub task_session_facts_complete: bool,
    pub unix_socket_fingerprints: Vec<String>,
    pub connected_named_unix_socket_fingerprints: Vec<String>,
    pub playwright_cli: Option<PlaywrightCliRuntime>,
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
    pub descriptor_facts_unavailable: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Snapshot {
    pub observed_at_unix_millis: u64,
    pub current_uid: u32,
    pub processes: Vec<ProcessRecord>,
    pub coverage: SnapshotCoverage,
}

impl Snapshot {
    /// A negative classifier result needs readable classification inputs, not
    /// merely a successful process-list call. Socket visibility is a separate
    /// per-candidate action gate and does not hide already identified sessions.
    #[must_use]
    pub fn proves_complete_classification_coverage(&self) -> bool {
        self.proves_complete_exact_identity_coverage()
            && self.coverage.arguments_unavailable == 0
            && self
                .processes
                .iter()
                .all(ProcessRecord::has_complete_classification_facts)
    }

    /// True only when the snapshot can prove exact process absence rather than
    /// merely omitting an unreadable or identity-incomplete process.
    #[must_use]
    pub fn proves_complete_exact_identity_coverage(&self) -> bool {
        self.coverage.listed_processes == self.coverage.inspected_processes
            && self.coverage.unreadable_processes == 0
            && self.coverage.executable_identity_unavailable == 0
            && self.processes.len() == self.coverage.inspected_processes
            && self.processes.iter().all(|process| {
                process.identity.started_at_unix_micros > 0
                    && process.identity.executable_device.is_some()
                    && process.identity.executable_inode.is_some()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bundle_version_facts_are_transient() {
        let process = ProcessRecord {
            identity: ProcessIdentity {
                pid: 10,
                started_at_unix_micros: 20,
                executable_device: Some(1),
                executable_inode: Some(2),
            },
            parent_pid: 1,
            process_group_id: 10,
            uid: 501,
            tty_device: None,
            name: "Google Chrome for Testing".to_owned(),
            executable_path: Some("/synthetic/Google Chrome for Testing".to_owned()),
            executable: ExecutableIdentity {
                device: Some(1),
                inode: Some(2),
                size: Some(3),
                modified_unix_nanos: Some(4),
            },
            arguments: Some(vec!["Google Chrome for Testing".to_owned()]),
            resident_memory_bytes: 1,
            status: ProcessStatus::Sleeping,
            runtime: ProcessRuntimeFacts {
                app_bundle: Some(AppBundleVersion {
                    bundle_id: "com.google.chrome.for.testing".to_owned(),
                    short_version: "151.0.7922.34".to_owned(),
                }),
                ..ProcessRuntimeFacts::default()
            },
        };

        let serialized = serde_json::to_string(&process).expect("serialize process");
        assert!(!serialized.contains("app_bundle"));
        assert!(!serialized.contains("com.google.chrome.for.testing"));
        assert!(!serialized.contains("151.0.7922.34"));
    }

    #[test]
    fn exact_absence_requires_complete_process_identity_coverage() {
        let complete = Snapshot {
            observed_at_unix_millis: 1,
            current_uid: 501,
            processes: Vec::new(),
            coverage: SnapshotCoverage::default(),
        };
        assert!(complete.proves_complete_exact_identity_coverage());

        let mut incomplete = complete;
        incomplete.coverage.listed_processes = 1;
        incomplete.coverage.unreadable_processes = 1;
        assert!(!incomplete.proves_complete_exact_identity_coverage());
    }
}
