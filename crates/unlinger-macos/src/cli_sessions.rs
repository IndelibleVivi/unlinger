//! Transient Playwright CLI session facts.
//!
//! Both task-owned (`unlinger-<id>`) and ordinary host-named sessions are
//! recognized from the exact `cliDaemon.js` entry, its own package version, the
//! owner-private registry record and the controller's live socket fingerprint.
//! Recognition is not eligibility: an ordinary session without an exact
//! durable owner lease stays protected in the analyzer.
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use unlinger_core::{
    PlaywrightCliRuntime, fingerprint_parts, ordinary_selector_fingerprint, task_id_from_session,
    valid_ordinary_session_name, valid_registry_namespace,
};

pub(crate) fn collect(
    arguments: &[String],
    socket_fingerprints: &[String],
) -> Option<PlaywrightCliRuntime> {
    let cache = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)?
        .join("Library/Caches/ms-playwright/daemon");
    collect_from(arguments, socket_fingerprints, &cache)
}

/// Registry root is injected so discovery is testable without touching the
/// caller's real Playwright daemon cache.
pub(crate) fn collect_from(
    arguments: &[String],
    socket_fingerprints: &[String],
    cache: &Path,
) -> Option<PlaywrightCliRuntime> {
    let entry = Path::new(arguments.get(1)?);
    if !entry.ends_with("playwright-core/lib/entry/cliDaemon.js") {
        return None;
    }
    let session = arguments.get(2)?;
    if task_id_from_session(session).is_none() && !valid_ordinary_session_name(session) {
        return None;
    }
    let package = read_json(&entry.parent()?.parent()?.parent()?.join("package.json"))?;
    if package.get("name")?.as_str()? != "playwright-core" {
        return None;
    }
    let version = package.get("version")?.as_str()?.to_owned();
    collect_registry(cache, session, &version, socket_fingerprints)
}

fn collect_registry(
    cache: &Path,
    session: &str,
    version: &str,
    sockets: &[String],
) -> Option<PlaywrightCliRuntime> {
    let mut matching = None;
    for workspace in fs::read_dir(cache).ok()? {
        let workspace = workspace.ok()?;
        if !workspace.file_type().ok()?.is_dir() {
            continue;
        }
        let Some(record) = read_json(&workspace.path().join(format!("{session}.session"))) else {
            continue;
        };
        if record.get("name")?.as_str()? != session || record.get("version")?.as_str()? != version {
            continue;
        }
        let path = record.get("socketPath")?.as_str()?;
        if !sockets.contains(&fingerprint_parts([path.as_bytes()])) {
            continue;
        }
        if matching.is_some() {
            return None;
        }
        let cli = record.get("cli")?.as_object()?;
        // The ordinary selector is the registry namespace directory name
        // (Playwright's 16-hex workspaceDirHash) plus the session name. That
        // namespace exists even when the record omits `workspaceDir`. Task-owned
        // selectors keep using the issued `unlinger-<id>` name instead.
        let namespace = workspace.file_name().to_str().map(str::to_owned);
        let namespace = namespace.as_deref();
        let selector_fingerprint = match namespace {
            Some(namespace)
                if task_id_from_session(session).is_none()
                    && valid_registry_namespace(namespace) =>
            {
                Some(ordinary_selector_fingerprint(namespace, session))
            }
            _ => None,
        };
        matching = Some(PlaywrightCliRuntime {
            session_name: session.to_owned(),
            version: version.to_owned(),
            persistent: optional_bool(cli.get("persistent"))?,
            attached: optional_bool(record.get("attached"))?,
            selector_fingerprint,
        });
    }
    matching
}

fn optional_bool(value: Option<&serde_json::Value>) -> Option<bool> {
    value.map_or(Some(false), serde_json::Value::as_bool)
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.len() > 65_536
    {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(65_537).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 65_536 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ul-cli-sessions-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn scratch_package(root: &Path) -> std::path::PathBuf {
        let package = root.join("node_modules/playwright-core");
        fs::create_dir_all(package.join("lib/entry")).unwrap();
        fs::write(
            package.join("package.json"),
            serde_json::json!({"name": "playwright-core", "version": "1.62.1"}).to_string(),
        )
        .unwrap();
        fs::write(package.join("lib/entry/cliDaemon.js"), "").unwrap();
        package.join("lib/entry/cliDaemon.js")
    }

    /// Writes one registry record. `workspace_dir` models the optional
    /// `workspaceDir` field: Playwright omits it when no `.playwright` marker is
    /// found, and the selector must not depend on it.
    fn write_record(
        cache: &Path,
        namespace: &str,
        session: &str,
        socket_path: &str,
        workspace_dir: Option<&str>,
    ) {
        let directory = cache.join(namespace);
        fs::create_dir_all(&directory).unwrap();
        let mut record = serde_json::json!({
            "name": session,
            "version": "1.62.1",
            "timestamp": 1,
            "socketPath": socket_path,
            "cli": {},
            "browser": {"browserName": "chromium"},
        });
        if let Some(workspace_dir) = workspace_dir {
            record["workspaceDir"] = serde_json::Value::String(workspace_dir.to_owned());
        }
        fs::write(
            directory.join(format!("{session}.session")),
            record.to_string(),
        )
        .unwrap();
    }

    #[test]
    fn ordinary_and_task_sessions_are_recognized_without_adopting_task_ids() {
        let root = temp_root("discovery");
        let cache = root.join("daemon");
        let entry = scratch_package(&root);
        let task_id = "0123456789abcdef0123456789abcdef";
        let task_socket = "/synthetic/task.sock";
        let ordinary_socket = "/synthetic/ordinary.sock";
        write_record(
            &cache,
            "0521184cff085302",
            &format!("unlinger-{task_id}"),
            task_socket,
            Some("/synthetic/workspace"),
        );
        write_record(&cache, "0521184cff085302", "default", ordinary_socket, None);
        let sockets = [
            fingerprint_parts([task_socket.as_bytes()]),
            fingerprint_parts([ordinary_socket.as_bytes()]),
        ];
        let arguments = |session: &str| {
            vec![
                "node".to_owned(),
                entry.to_string_lossy().into_owned(),
                session.to_owned(),
            ]
        };

        let owned = collect_from(&arguments(&format!("unlinger-{task_id}")), &sockets, &cache)
            .expect("task-owned session is recognized");
        assert_eq!(owned.session_name, format!("unlinger-{task_id}"));
        assert_eq!(owned.version, "1.62.1");
        assert_eq!(task_id_from_session(&owned.session_name), Some(task_id));
        // The task lane keeps its issued selector; it never gets an ordinary
        // workspace-derived selector.
        assert!(owned.selector_fingerprint.is_none());

        let ordinary = collect_from(&arguments("default"), &sockets, &cache)
            .expect("ordinary session is recognized");
        assert_eq!(ordinary.session_name, "default");
        assert!(task_id_from_session(&ordinary.session_name).is_none());
        assert!(!ordinary.persistent && !ordinary.attached);
        assert_eq!(
            ordinary.selector_fingerprint.as_deref(),
            Some(
                unlinger_core::ordinary_selector_fingerprint("0521184cff085302", "default")
                    .as_str()
            )
        );

        // A name outside both exact shapes, a socket the controller does not
        // own, and a mismatched registry version all stay unrecognized.
        for blocked in ["default other", "unlinger-0123", "../escape", ""] {
            assert!(collect_from(&arguments(blocked), &sockets, &cache).is_none());
        }
        assert!(collect_from(&arguments("default"), &[], &cache).is_none());
        assert!(
            collect_from(
                &[
                    "node".to_owned(),
                    entry.to_string_lossy().into_owned(),
                    "default".to_owned()
                ],
                &sockets,
                &root.join("missing-cache"),
            )
            .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registry_requires_exact_owned_socket_and_preserves_persistent_and_attached_guards() {
        let root = std::env::temp_dir().join(format!(
            "ul-registry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = root.join("workspace-a");
        let second = root.join("workspace-b");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let session = "unlinger-0123456789abcdef0123456789abcdef";
        let path = first.join(format!("{session}.session"));
        let mut record = serde_json::json!({"name": session, "version": "tested", "socketPath": "/synthetic/task.sock", "cli": {}});
        fs::write(&path, record.to_string()).unwrap();
        assert!(collect_registry(&root, session, "tested", &[]).is_none());
        let sockets = [fingerprint_parts([b"/synthetic/task.sock".as_slice()])];
        let facts = collect_registry(&root, session, "tested", &sockets).unwrap();
        assert!(!facts.persistent && !facts.attached);
        assert!(collect_registry(&root, session, "other", &sockets).is_none());
        record["cli"]["persistent"] = true.into();
        record["attached"] = true.into();
        fs::write(&path, record.to_string()).unwrap();
        let facts = collect_registry(&root, session, "tested", &sockets).unwrap();
        assert!(facts.persistent && facts.attached);
        fs::write(
            second.join(format!("{session}.session")),
            record.to_string(),
        )
        .unwrap();
        assert!(collect_registry(&root, session, "tested", &sockets).is_none());
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(second.join(format!("{session}.session")), &path).unwrap();
        assert!(read_json(&path).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_named_sessions_in_two_workspaces_resolve_to_distinct_selectors() {
        let root = temp_root("two-workspaces");
        let cache = root.join("daemon");
        let entry = scratch_package(&root);
        let socket_a = "/synthetic/tmp-a/cli/0521184cff085302-default.sock";
        let socket_b = "/synthetic/tmp-b/cli/625daa9ea0d6cbcf-default.sock";
        // The first registry has a `.playwright` workspaceDir and the second
        // omits it, exactly like the real runtime when no marker is found.
        write_record(
            &cache,
            "0521184cff085302",
            "default",
            socket_a,
            Some("/synthetic/workspace/a"),
        );
        write_record(&cache, "625daa9ea0d6cbcf", "default", socket_b, None);
        let arguments = vec![
            "node".to_owned(),
            entry.to_string_lossy().into_owned(),
            "default".to_owned(),
        ];

        let from_a = collect_from(
            &arguments,
            &[fingerprint_parts([socket_a.as_bytes()])],
            &cache,
        )
        .expect("workspace a controller");
        let from_b = collect_from(
            &arguments,
            &[fingerprint_parts([socket_b.as_bytes()])],
            &cache,
        )
        .expect("workspace b controller");
        assert_ne!(
            from_a.selector_fingerprint, from_b.selector_fingerprint,
            "same-named sessions in two workspaces are distinct selectors"
        );
        assert_eq!(from_a.session_name, from_b.session_name);

        // A controller holding both records' sockets is ambiguous and fails
        // closed rather than guessing a selector.
        assert!(
            collect_from(
                &arguments,
                &[
                    fingerprint_parts([socket_a.as_bytes()]),
                    fingerprint_parts([socket_b.as_bytes()]),
                ],
                &cache,
            )
            .is_none()
        );

        // A malformed namespace directory cannot yield a selector, even though
        // the controller is still recognized.
        let bad = temp_root("bad-namespace");
        let bad_cache = bad.join("daemon");
        let bad_entry = scratch_package(&bad);
        let bad_socket = "/synthetic/tmp-c/cli/not-a-namespace-default.sock";
        write_record(&bad_cache, "not-a-namespace", "default", bad_socket, None);
        let bad_arguments = vec![
            "node".to_owned(),
            bad_entry.to_string_lossy().into_owned(),
            "default".to_owned(),
        ];
        let bad_facts = collect_from(
            &bad_arguments,
            &[fingerprint_parts([bad_socket.as_bytes()])],
            &bad_cache,
        )
        .expect("recognized without a selector");
        assert_eq!(bad_facts.session_name, "default");
        assert!(bad_facts.selector_fingerprint.is_none());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(bad).unwrap();
    }
}
