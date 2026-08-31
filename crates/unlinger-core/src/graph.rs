use crate::{ProcessRecord, Snapshot, fingerprint_process_set};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphError {
    DuplicatePid(u32),
}

impl Display for GraphError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicatePid(pid) => write!(formatter, "snapshot contains duplicate PID {pid}"),
        }
    }
}

impl Error for GraphError {}

#[derive(Clone, Debug)]
pub struct ProcessGraph {
    current_uid: u32,
    processes: BTreeMap<u32, ProcessRecord>,
    children: BTreeMap<u32, Vec<u32>>,
}

impl ProcessGraph {
    pub fn from_snapshot(snapshot: &Snapshot) -> Result<Self, GraphError> {
        let mut processes = BTreeMap::new();
        for process in &snapshot.processes {
            if processes.insert(process.pid(), process.clone()).is_some() {
                return Err(GraphError::DuplicatePid(process.pid()));
            }
        }

        let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for process in processes.values() {
            children
                .entry(process.parent_pid)
                .or_default()
                .push(process.pid());
        }
        for child_pids in children.values_mut() {
            child_pids.sort_unstable();
        }

        Ok(Self {
            current_uid: snapshot.current_uid,
            processes,
            children,
        })
    }

    #[must_use]
    pub fn current_uid(&self) -> u32 {
        self.current_uid
    }

    pub fn processes(&self) -> impl Iterator<Item = &ProcessRecord> {
        self.processes.values()
    }

    #[must_use]
    pub fn get(&self, pid: u32) -> Option<&ProcessRecord> {
        self.processes.get(&pid)
    }

    #[must_use]
    pub fn children_of(&self, pid: u32) -> &[u32] {
        self.children.get(&pid).map(Vec::as_slice).unwrap_or(&[])
    }

    #[must_use]
    pub fn descendant_pids(&self, root_pid: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut queue = VecDeque::from([root_pid]);
        let mut seen = BTreeSet::new();
        while let Some(pid) = queue.pop_front() {
            if !seen.insert(pid) {
                continue;
            }
            if self.processes.contains_key(&pid) {
                result.push(pid);
            }
            for child in self.children_of(pid) {
                queue.push_back(*child);
            }
        }
        result.sort_unstable();
        result
    }

    #[must_use]
    pub fn ancestor_pids(&self, pid: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cursor = self.get(pid).map(|process| process.parent_pid);
        while let Some(parent_pid) = cursor {
            if parent_pid == 0 || !seen.insert(parent_pid) {
                break;
            }
            result.push(parent_pid);
            cursor = self.get(parent_pid).map(|process| process.parent_pid);
        }
        result
    }

    #[must_use]
    pub fn has_ancestor(&self, pid: u32, ancestor_pid: u32) -> bool {
        self.ancestor_pids(pid).contains(&ancestor_pid)
    }

    #[must_use]
    pub fn lineage_has_cycle(&self, pid: u32) -> bool {
        let mut seen = BTreeSet::new();
        let mut cursor = Some(pid);
        while let Some(current) = cursor {
            if !seen.insert(current) {
                return true;
            }
            cursor = self.get(current).and_then(|process| {
                (process.parent_pid != 0 && process.parent_pid != 1).then_some(process.parent_pid)
            });
        }
        false
    }

    #[must_use]
    pub fn has_unresolved_parent(&self, pid: u32) -> bool {
        self.get(pid).is_some_and(|process| {
            process.parent_pid > 1 && !self.processes.contains_key(&process.parent_pid)
        })
    }

    #[must_use]
    pub fn process_set_fingerprint(&self, pids: &[u32]) -> String {
        fingerprint_process_set(
            pids.iter()
                .filter_map(|pid| self.get(*pid))
                .map(|process| &process.identity),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExecutableIdentity, ProcessIdentity, ProcessStatus, SnapshotCoverage};

    fn process(pid: u32, parent_pid: u32) -> ProcessRecord {
        ProcessRecord {
            identity: ProcessIdentity {
                pid,
                started_at_unix_micros: u64::from(pid),
                executable_device: Some(1),
                executable_inode: Some(u64::from(pid)),
            },
            parent_pid,
            process_group_id: 10,
            uid: 501,
            tty_device: None,
            name: format!("p{pid}"),
            executable_path: Some(format!("/tmp/p{pid}")),
            executable: ExecutableIdentity {
                device: Some(1),
                inode: Some(u64::from(pid)),
                size: Some(1),
                modified_unix_nanos: Some(1),
            },
            arguments: Some(vec![format!("p{pid}")]),
            resident_memory_bytes: 0,
            status: ProcessStatus::Sleeping,
            runtime: Default::default(),
        }
    }

    fn snapshot(processes: Vec<ProcessRecord>) -> Snapshot {
        Snapshot {
            observed_at_unix_millis: 1,
            current_uid: 501,
            coverage: SnapshotCoverage {
                listed_processes: processes.len(),
                inspected_processes: processes.len(),
                ..SnapshotCoverage::default()
            },
            processes,
        }
    }

    #[test]
    fn reconstructs_descendants_without_following_unrelated_peers() {
        let graph = ProcessGraph::from_snapshot(&snapshot(vec![
            process(10, 1),
            process(11, 10),
            process(12, 11),
            process(20, 1),
        ]))
        .expect("valid graph");
        assert_eq!(graph.descendant_pids(10), vec![10, 11, 12]);
        assert!(!graph.has_ancestor(20, 10));
    }

    #[test]
    fn detects_cycles_and_unresolved_non_init_parents() {
        let cyclic = ProcessGraph::from_snapshot(&snapshot(vec![process(10, 11), process(11, 10)]))
            .expect("valid keyed graph");
        assert!(cyclic.lineage_has_cycle(10));

        let missing = ProcessGraph::from_snapshot(&snapshot(vec![process(30, 29)]))
            .expect("valid keyed graph");
        assert!(missing.has_unresolved_parent(30));
    }
}
