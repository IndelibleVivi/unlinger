use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use unlinger_core::{PlaywrightCliRuntime, fingerprint_parts, task_id_from_session};

pub(crate) fn collect(
    arguments: &[String],
    socket_fingerprints: &[String],
) -> Option<PlaywrightCliRuntime> {
    let entry = Path::new(arguments.get(1)?);
    if !entry.ends_with("playwright-core/lib/entry/cliDaemon.js") {
        return None;
    }
    let session = arguments.get(2)?;
    task_id_from_session(session)?;
    let package = read_json(&entry.parent()?.parent()?.parent()?.join("package.json"))?;
    if package.get("name")?.as_str()? != "playwright-core" {
        return None;
    }
    let version = package.get("version")?.as_str()?.to_owned();
    let cache = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)?
        .join("Library/Caches/ms-playwright/daemon");
    collect_registry(&cache, session, &version, socket_fingerprints)
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
        matching = Some(PlaywrightCliRuntime {
            session_name: session.to_owned(),
            version: version.to_owned(),
            persistent: optional_bool(cli.get("persistent"))?,
            attached: optional_bool(record.get("attached"))?,
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
}
