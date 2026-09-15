use crate::{ProcessIdentity, ProcessRecord, fingerprint_parts};
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

/// Canonical, path-free identity of one ordinary Playwright CLI session.
///
/// The selector is derived from the controller-owned registry namespace
/// directory (Playwright's 16-hex `workspaceDirHash`) plus the ordinary session
/// name. The namespace directory name exists even when the session record omits
/// `workspaceDir`, so the same ordinary name in two registries stays two
/// selectors and no raw path or record path leaves this function.
#[must_use]
pub fn ordinary_selector_fingerprint(registry_namespace: &str, session_name: &str) -> String {
    fingerprint_parts([
        b"pw-cli".as_slice(),
        registry_namespace.as_bytes(),
        session_name.as_bytes(),
    ])
}

/// Durable host-declared ownership of one ordinary Playwright CLI session.
///
/// A lease supplies abandonment evidence only. It is never a signal target,
/// never cleanup authority, and never widens a protection rule by itself. The
/// `lease_id` authorizes the declaring peer; it is deliberately unrelated to
/// the immutable Playwright session name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionOwnerBinding {
    pub lease_id: String,
    pub selector_fingerprint: String,
    pub session_name: String,
    pub controller_version: String,
    pub controller: ProcessIdentity,
    pub released: bool,
}

impl SessionOwnerBinding {
    /// True only for the exact controller identity captured at binding time.
    #[must_use]
    pub fn binds(&self, process: &ProcessRecord) -> bool {
        self.controller.exact_match(&process.identity)
    }
}

#[must_use]
pub fn valid_task_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The opaque session selector issued for one task or session-owner identifier.
#[must_use]
pub fn session_name_from_id(id: &str) -> String {
    format!("unlinger-{id}")
}

#[must_use]
pub fn task_id_from_session(value: &str) -> Option<&str> {
    value
        .strip_prefix("unlinger-")
        .filter(|id| valid_task_id(id))
}

/// Ordinary (non-task) Playwright CLI session names are accepted only in a
/// tight shape so that an unverifiable or adversarial name stays unrecognized
/// and therefore protected.
#[must_use]
pub fn valid_ordinary_session_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with("unlinger-")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// Playwright's registry namespace directory is a lowercase 16-hex truncated
/// SHA-1. Anything else is unverifiable and must fail closed.
#[must_use]
pub fn valid_registry_namespace(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_session_names_are_exact_and_path_free() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(task_id_from_session(&format!("unlinger-{id}")), Some(id));
        assert_eq!(session_name_from_id(id), format!("unlinger-{id}"));
        for value in [
            "default",
            "unlinger-../profile",
            "unlinger-0123",
            "UNLINGER-0123456789abcdef0123456789abcdef",
        ] {
            assert_eq!(task_id_from_session(value), None);
        }
    }

    #[test]
    fn ordinary_session_names_reject_task_selectors_and_unsafe_bytes() {
        for value in [
            "default",
            "mcpb-btn",
            "workspace_a.1",
            "1",
            "x".repeat(64).as_str(),
        ] {
            assert!(valid_ordinary_session_name(value), "{value}");
        }
        let long = "x".repeat(65);
        for value in [
            "",
            long.as_str(),
            "unlinger-0123456789abcdef0123456789abcdef",
            "unlinger-default",
            "has space",
            "has/slash",
            "has\\backslash",
            "has\"quote",
            "emoji-\u{1f600}",
        ] {
            assert!(!valid_ordinary_session_name(value), "{value}");
        }
    }

    #[test]
    fn ordinary_selectors_separate_same_named_sessions_and_never_expose_paths() {
        let first = ordinary_selector_fingerprint("0521184cff085302", "default");
        let second = ordinary_selector_fingerprint("625daa9ea0d6cbcf", "default");
        assert_ne!(first, second);
        assert_eq!(
            first,
            ordinary_selector_fingerprint("0521184cff085302", "default")
        );
        assert_ne!(
            first,
            ordinary_selector_fingerprint("0521184cff085302", "other")
        );
        for selector in [&first, &second] {
            assert_eq!(selector.len(), 16);
            assert!(!selector.contains('/'));
            assert!(!selector.contains("0521184c"));
        }

        for value in ["0521184cff085302", "625daa9ea0d6cbcf", "0000000000000000"] {
            assert!(valid_registry_namespace(value), "{value}");
        }
        for value in [
            "",
            "0521184cff08530",
            "0521184cff0853022",
            "0521184CFF085302",
            "0521184cff08530z",
            "/Users/example/work",
            "unlinger-0123456789abcdef0123456789abcdef",
        ] {
            assert!(!valid_registry_namespace(value), "{value}");
        }
    }
}
