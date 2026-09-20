//! Focused native-adapter tests for npm `_cacache` maintenance.
//!
//! Every test creates its own synthetic cache root under the process temp
//! directory and removes only that exact root on drop. The real user cache
//! (`~/.npm/_cacache`) is never touched. No network is used: the driver only
//! walks and unlinks local content; the tests seed content directly with the
//! producer's bundled `cacache` library.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{NpmCacheMaintenance, RunningNpmMaintenance, ToolCacheNativeResult};
use unlinger_protocol::{ToolCacheAvailability, ToolCacheOutcome};

mod native {
    pub const NODE: &str = "/opt/homebrew/bin/node";
    pub const CACACHE: &str = "/opt/homebrew/lib/node_modules/npm/node_modules/cacache";
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A test-owned root. Drop removes only this exact directory.
struct OwnedCache {
    root: PathBuf,
}

impl OwnedCache {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "unlinger-toolcache-{label}-{}-{nonce}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("_cacache")).expect("create owned cache root");
        Self { root }
    }

    fn cacache(&self) -> PathBuf {
        self.root.join("_cacache")
    }

    /// Seed the isolated cache by invoking the producer's own `put`. No network.
    fn seed_put(&self, key: &str, body: &[u8]) {
        let script = "const c = require(process.argv[1]);\
            (async () => { await c.put(process.argv[2], process.argv[3], Buffer.from(process.argv[4])); })\
            ().catch((e) => { process.stderr.write(String(e.message)); process.exit(1) });";
        let status = Command::new(native::NODE)
            .env_clear()
            .arg("-e")
            .arg(script)
            .arg(native::CACACHE)
            .arg(self.cacache())
            .arg(key)
            .arg(std::str::from_utf8(body).expect("utf8 body"))
            .status()
            .expect("run seed helper");
        assert!(status.success(), "seed put must succeed");
    }

    fn maintenance(&self, budget: Duration) -> NpmCacheMaintenance {
        NpmCacheMaintenance::from_paths(
            PathBuf::from(native::NODE),
            PathBuf::from(native::CACACHE),
            self.cacache(),
            budget,
            &|| false,
        )
    }

    fn run(&self, budget: Duration) -> ToolCacheNativeResult {
        let maintenance = self.maintenance(budget);
        let running: RunningNpmMaintenance = maintenance.start().expect("start adapter");
        running.wait(|| false)
    }
}

impl Drop for OwnedCache {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn availability_reports_available_for_isolated_root() {
    let cache = OwnedCache::new("available");
    cache.seed_put("test://a", b"live-content");
    let maintenance = cache.maintenance(Duration::from_secs(30));
    assert_eq!(
        maintenance.observe(|| false),
        ToolCacheAvailability::Available
    );
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn availability_reports_absent_when_root_missing() {
    let cache = OwnedCache::new("absent");
    fs::remove_dir_all(cache.cacache()).expect("remove isolated root");
    let maintenance = cache.maintenance(Duration::from_secs(30));
    assert_eq!(maintenance.observe(|| false), ToolCacheAvailability::Absent);
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn availability_reports_unsupported_for_wrong_producer_version() {
    let cache = OwnedCache::new("unsupported");
    let fake_root = cache.root.join("fake-npm");
    let fake_cacache = fake_root.join("node_modules").join("cacache");
    fs::create_dir_all(&fake_cacache).expect("create fake cacache");
    fs::write(
        fake_root.join("package.json"),
        r#"{"name":"npm","version":"0.0.0"}"#,
    )
    .expect("write fake npm manifest");
    fs::write(
        fake_cacache.join("package.json"),
        r#"{"name":"cacache","version":"0.0.0"}"#,
    )
    .expect("write fake cacache manifest");
    let maintenance = NpmCacheMaintenance::from_paths(
        PathBuf::from(native::NODE),
        fake_cacache,
        cache.cacache(),
        Duration::from_secs(30),
        &|| false,
    );
    assert_eq!(
        maintenance.observe(|| false),
        ToolCacheAvailability::Unsupported
    );
    assert!(maintenance.start().is_err());
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn no_op_run_reports_noop_with_zero_counters() {
    let cache = OwnedCache::new("noop");
    cache.seed_put("test://keep", b"referenced-content");
    let result = cache.run(Duration::from_secs(30));
    assert_eq!(result.outcome, ToolCacheOutcome::NoOp);
    assert_eq!(result.removed_entry_count, Some(0));
    assert_eq!(result.removed_logical_bytes, Some(0));
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn referenced_and_orphan_content_reports_completed() {
    let cache = OwnedCache::new("orphan");
    cache.seed_put("test://keep", b"referenced-content");
    let orphan_dir = cache
        .cacache()
        .join("content-v2")
        .join("sha512")
        .join("aa")
        .join("bb");
    fs::create_dir_all(&orphan_dir).expect("create orphan bucket");
    fs::write(orphan_dir.join("c".repeat(120)), b"orphan-bytes").expect("write orphan");

    let result = cache.run(Duration::from_secs(30));
    assert_eq!(result.outcome, ToolCacheOutcome::Completed);
    assert!(result.removed_entry_count.unwrap_or(0) >= 1);
    assert!(result.removed_logical_bytes.unwrap_or(0) >= 1);
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn corrupt_content_is_reclaimed_by_native_verify() {
    let cache = OwnedCache::new("corrupt");
    cache.seed_put("test://keep", b"referenced-content");
    cache.seed_put("test://corrupt", b"corrupt-me");

    let content = cache.cacache().join("content-v2");
    let mut files = Vec::new();
    collect_files(&content, &mut files);
    assert!(files.len() >= 2, "expected at least two content files");
    fs::write(&files[0], b"tampered").expect("corrupt content");

    let result = cache.run(Duration::from_secs(30));
    assert_eq!(result.outcome, ToolCacheOutcome::Completed);
    assert_eq!(result.removed_entry_count, Some(1));
    assert_eq!(result.removed_logical_bytes, Some(8));
    let check = cache.run(Duration::from_secs(30));
    assert_eq!(check.outcome, ToolCacheOutcome::NoOp);
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn root_symlink_is_refused_by_preflight() {
    let cache = OwnedCache::new("rootsymlink");
    cache.seed_put("test://a", b"live");
    let victim = cache.root.join("victim");
    fs::create_dir_all(victim.join("real")).expect("create victim");
    fs::write(victim.join("real").join("keep.txt"), b"CANARY").expect("write canary");

    let cacache = cache.cacache();
    fs::remove_dir_all(&cacache).expect("remove real root");
    std::os::unix::fs::symlink(victim.join("real"), &cacache).expect("symlink root");

    let maintenance = cache.maintenance(Duration::from_secs(30));
    assert_eq!(
        maintenance.observe(|| false),
        ToolCacheAvailability::Unavailable
    );
    assert!(maintenance.start().is_err());
    assert!(victim.join("real").join("keep.txt").exists());
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn bucket_symlink_is_refused_by_preflight() {
    let cache = OwnedCache::new("bucketsymlink");
    cache.seed_put("test://a", b"live");
    let victim = cache.root.join("victim");
    fs::create_dir_all(victim.join("real")).expect("create victim");
    fs::write(victim.join("real").join("keep.txt"), b"CANARY").expect("write canary");

    let content = cache.cacache().join("content-v2");
    fs::remove_dir_all(&content).expect("remove content bucket");
    std::os::unix::fs::symlink(victim.join("real"), &content).expect("symlink bucket");

    let maintenance = cache.maintenance(Duration::from_secs(30));
    assert_eq!(
        maintenance.observe(|| false),
        ToolCacheAvailability::Unavailable
    );
    assert!(maintenance.start().is_err());
    assert!(victim.join("real").join("keep.txt").exists());
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn intermediate_symlink_in_content_bucket_is_refused() {
    let cache = OwnedCache::new("intersymlink");
    cache.seed_put("test://a", b"live");
    let victim = cache.root.join("victim");
    fs::create_dir_all(victim.join("real")).expect("create victim");
    fs::write(victim.join("real").join("keep.txt"), b"CANARY").expect("write canary");

    let link = cache.cacache().join("content-v2").join("sha512").join("zz");
    std::os::unix::fs::symlink(victim.join("real"), &link).expect("intermediate symlink");

    let maintenance = cache.maintenance(Duration::from_secs(30));
    assert_eq!(
        maintenance.observe(|| false),
        ToolCacheAvailability::Unavailable
    );
    assert!(maintenance.start().is_err());
    assert!(
        victim.join("real").join("keep.txt").exists(),
        "canary must survive"
    );
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn sibling_npx_and_logs_are_left_intact() {
    let cache = OwnedCache::new("siblings");
    cache.seed_put("test://keep", b"referenced");
    let npx = cache.root.join("_npx").join("abc");
    let logs = cache.root.join("_logs");
    fs::create_dir_all(&npx).expect("create npx sibling");
    fs::create_dir_all(&logs).expect("create logs sibling");
    fs::write(npx.join("marker"), b"npx-state").expect("write npx marker");
    fs::write(logs.join("marker"), b"log-state").expect("write log marker");

    let _ = cache.run(Duration::from_secs(30));
    assert!(
        npx.join("marker").exists(),
        "npx runtime tree must be untouched"
    );
    assert!(
        logs.join("marker").exists(),
        "log directory must be untouched"
    );
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn cancel_returns_delivery_unknown_and_reaps_child() {
    let cache = OwnedCache::new("cancel");
    cache.seed_put("test://keep", b"referenced");
    let maintenance = cache.maintenance(Duration::from_secs(30));
    let running = maintenance.start().expect("start adapter");
    let result = running.wait(|| true);
    assert_eq!(result.outcome, ToolCacheOutcome::DeliveryUnknown);
    assert_eq!(result.removed_entry_count, None);
    assert_eq!(result.removed_logical_bytes, None);
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn timeout_returns_delivery_unknown_without_counts() {
    let cache = OwnedCache::new("timeout");
    cache.seed_put("test://keep", b"referenced");
    // A zero budget forces the wait loop to hit the deadline immediately.
    let maintenance = cache.maintenance(Duration::from_millis(0));
    let running = maintenance.start().expect("start adapter");
    let result = running.wait(|| false);
    assert_eq!(result.outcome, ToolCacheOutcome::DeliveryUnknown);
    assert_eq!(result.removed_entry_count, None);
    assert_eq!(result.removed_logical_bytes, None);
}

#[test]
fn parser_rejects_malformed_or_missing_native_counts() {
    for invalid in [
        br#"{"reclaimedCount":1,"reclaimedSize":2,"badContentCount":0 garbage}"#.as_slice(),
        br#"{"reclaimedCount":1,"reclaimedSize":2}"#,
        br#"{"reclaimedCount":-1,"reclaimedSize":2,"badContentCount":0}"#,
        br#"{"reclaimedCount":1,"reclaimedCount":2,"reclaimedSize":2,"badContentCount":0}"#,
    ] {
        assert!(super::parse_native_result(invalid).is_none());
    }
    assert!(
        super::parse_native_result(
            br#"{ "reclaimedCount": 1, "reclaimedSize": 2, "badContentCount": 0 }"#
        )
        .is_some()
    );
}

fn owned_output_child(script: &str) -> RunningNpmMaintenance {
    let child = Command::new("/bin/sh")
        .args(["-c", script])
        .env_clear()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    RunningNpmMaintenance::from_child(child, Duration::from_secs(5))
}

#[test]
fn output_limit_and_nonzero_exit_never_publish_counts() {
    let result = owned_output_child("while :; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; done").wait(|| false);
    assert_eq!(result.outcome, ToolCacheOutcome::DeliveryUnknown);
    assert_eq!(result.removed_logical_bytes, None);
    let result = owned_output_child(
        "printf '%s' '{\"reclaimedCount\":1,\"reclaimedSize\":20,\"badContentCount\":0}'; exit 7",
    )
    .wait(|| false);
    assert_eq!(result.outcome, ToolCacheOutcome::Failed);
    assert_eq!(result.removed_entry_count, None);
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn root_replacement_after_discovery_is_not_adopted() {
    let cache = OwnedCache::new("root-replaced");
    let maintenance = cache.maintenance(Duration::from_secs(30));
    fs::rename(cache.cacache(), cache.root.join("previous")).unwrap();
    fs::create_dir(cache.cacache()).unwrap();
    assert!(maintenance.start().is_err());
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn missing_content_is_removed_from_index_without_invented_reclaim() {
    let cache = OwnedCache::new("missing");
    cache.seed_put("test://missing", b"missing-content");
    let mut files = Vec::new();
    collect_files(&cache.cacache().join("content-v2"), &mut files);
    assert_eq!(files.len(), 1);
    fs::remove_file(&files[0]).unwrap();
    let result = cache.run(Duration::from_secs(30));
    assert_eq!(result.outcome, ToolCacheOutcome::NoOp);
    assert_eq!(result.removed_entry_count, Some(0));
    let output = Command::new(native::NODE).env_clear().args(["-e",
        "require(process.argv[1]).ls(process.argv[2]).then(x=>console.log(Object.keys(x).length))",
        native::CACACHE]).arg(cache.cacache()).output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "0");
}

#[test]
#[ignore = "requires the exact local npm/cacache producer; operates only on test-created cache roots"]
fn native_write_targets_cannot_modify_external_canaries() {
    let cache = OwnedCache::new("write-links");
    cache.seed_put("test://keep", b"reference");
    let maintenance = cache.maintenance(Duration::from_secs(30));
    let victim = cache.root.join("outside-marker");
    fs::write(&victim, b"UNCHANGED").unwrap();
    std::os::unix::fs::symlink(&victim, cache.cacache().join("_lastverified")).unwrap();
    assert_eq!(
        maintenance.observe(|| false),
        ToolCacheAvailability::Unavailable
    );
    assert!(maintenance.start().is_err());
    assert_eq!(fs::read(&victim).unwrap(), b"UNCHANGED");
    fs::remove_file(cache.cacache().join("_lastverified")).unwrap();
    let mut indexes = Vec::new();
    collect_files(&cache.cacache().join("index-v5"), &mut indexes);
    assert_eq!(indexes.len(), 1);
    let linked = cache.root.join("outside-index");
    fs::hard_link(&indexes[0], &linked).unwrap();
    let before = fs::read(&linked).unwrap();
    let refused = cache.maintenance(Duration::from_secs(30));
    assert_eq!(
        refused.observe(|| false),
        ToolCacheAvailability::Unavailable
    );
    assert!(refused.start().is_err());
    assert_eq!(fs::read(&linked).unwrap(), before);
}

#[test]
fn changed_producer_identity_is_refused_before_spawn() {
    use std::os::unix::fs::PermissionsExt;
    let cache = OwnedCache::new("producer-replaced");
    let npm = cache.root.join("npm");
    let package = npm.join("node_modules/cacache");
    fs::create_dir_all(package.join("lib")).unwrap();
    fs::write(
        npm.join("package.json"),
        br#"{"name":"npm","version":"11.19.0"}"#,
    )
    .unwrap();
    fs::write(
        package.join("package.json"),
        br#"{"name":"cacache","version":"20.0.4"}"#,
    )
    .unwrap();
    fs::write(package.join("lib/index.js"), "").unwrap();
    fs::write(package.join("lib/verify.js"), "").unwrap();
    let node = cache.root.join("node");
    fs::write(&node, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();
    let adapter = NpmCacheMaintenance::from_paths(
        node.clone(),
        package,
        cache.cacache(),
        Duration::from_secs(1),
        &|| false,
    );
    assert_eq!(adapter.observe(|| false), ToolCacheAvailability::Available);
    fs::rename(&node, cache.root.join("previous-node")).unwrap();
    fs::write(&node, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(adapter.start().is_err());
}

#[test]
fn preflight_stops_traversing_when_shutdown_is_requested() {
    use super::preflight_cache_root;
    use std::cell::Cell;

    let cache = OwnedCache::new("cancel-preflight");
    let bucket = cache.cacache().join("index-v5");
    fs::create_dir(&bucket).unwrap();
    for index in 0..100 {
        fs::write(bucket.join(index.to_string()), b"owned index").unwrap();
    }
    assert!(preflight_cache_root(&cache.cacache(), &|| false).is_ok());
    let checks = Cell::new(0);
    assert!(
        preflight_cache_root(&cache.cacache(), &|| {
            checks.set(checks.get() + 1);
            checks.get() >= 5
        })
        .is_err()
    );
    assert_eq!(
        checks.get(),
        5,
        "stop at cancellation, not the full-tree budget"
    );
}
