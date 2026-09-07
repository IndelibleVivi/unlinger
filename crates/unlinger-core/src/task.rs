use crate::{ProcessIdentity, ProcessRecord};
use serde::{Deserialize, Serialize};

/// Task ownership survives the launcher's exec. Executable identity is still
/// mandatory for signal targets, but must not define the lifetime of a command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskOwnerIdentity {
    pub pid: u32,
    pub started_at_unix_micros: u64,
    pub uid: u32,
}

impl TaskOwnerIdentity {
    #[must_use]
    pub fn from_process(process: &ProcessRecord) -> Option<Self> {
        (process.pid() > 1 && process.identity.started_at_unix_micros > 0 && process.uid != 0)
            .then_some(Self {
                pid: process.pid(),
                started_at_unix_micros: process.identity.started_at_unix_micros,
                uid: process.uid,
            })
    }

    #[must_use]
    pub fn matches(&self, process: &ProcessRecord) -> bool {
        self.pid == process.pid()
            && self.started_at_unix_micros == process.identity.started_at_unix_micros
            && self.uid == process.uid
    }
}

/// A durable task binding supplies ownership evidence, never a signal plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskControllerBinding {
    pub task_id: String,
    pub controller: ProcessIdentity,
    pub released: bool,
}

#[must_use]
pub fn valid_task_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[must_use]
pub fn task_id_from_session(value: &str) -> Option<&str> {
    value
        .strip_prefix("unlinger-")
        .filter(|id| valid_task_id(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_session_names_are_exact_and_path_free() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(task_id_from_session(&format!("unlinger-{id}")), Some(id));
        for value in [
            "default",
            "unlinger-../profile",
            "unlinger-0123",
            "UNLINGER-0123456789abcdef0123456789abcdef",
        ] {
            assert_eq!(task_id_from_session(value), None);
        }
    }
}
