//! Focused native-adapter tests for uv cache maintenance.
//!
//! Every test creates its own synthetic cache root under a process temp
//! directory and removes only that exact root on drop. The real user cache
//! (`~/.cache/uv`) is never touched. Fixture-only tests pin the human-summary
//! parser, the Busy classification, containment, and the supervisor's reaping
//! behavior with `/bin/sh` stand-ins. The producer-backed tests are `#[ignore]`d
//! because they require the exact staged uv 0.11.20 binary; they operate only on
//! test-owned roots and perform no network access.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    HumanSummary, UvCacheMaintenance, execute_supervisor, fixed_cache_shape_safe,
    parse_human_bytes, parse_human_summary, parse_supervisor_result, preflight_cache_root,
};
use unlinger_protocol::{ToolCacheAvailability, ToolCacheOutcome};

/// Exact staged producer path for the ignored native lanes. Overridable via the
/// environment so the acceptance harness can point at the coordinator's staging
/// root without hard-coding a machine path in the source.
fn native_uv() -> PathBuf {
    let binary = required_path("UNLINGER_UV_BINARY");
    assert_eq!(
        super::producer_version(&binary, &|| false).as_deref(),
        Some("0.11.20")
    );
    binary
}
fn installed_uv() -> PathBuf {
    required_path("UNLINGER_UV_INSTALLED_BINARY")
}
fn supervisor_binary() -> PathBuf {
    required_path("UNLINGER_UV_SUPERVISOR_BIN")
}
fn local_python() -> PathBuf {
    required_path("UNLINGER_UV_TEST_PYTHON")
}
fn required_path(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("set {name} for this explicit native test")),
    );
    assert!(
        path.is_absolute() && path.is_file(),
        "{name} must name an existing absolute executable"
    );
    path
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A test-owned root. Drop removes only this exact directory.
struct OwnedRoot {
    root: PathBuf,
}

impl OwnedRoot {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "unlinger-uvcache-{label}-{}-{nonce}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create owned root");
        Self { root }
    }

    fn cache(&self) -> PathBuf {
        self.root.join("uv-cache")
    }
}

impl Drop for OwnedRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn seed_bucket(cache: &Path, bucket: &str) {
    fs::create_dir_all(cache.join(bucket)).expect("seed bucket");
}

/// Render a path as a single-quoted string literal usable in generated Python.
fn rust_str(path: &str) -> String {
    serde_json::to_string(path).unwrap()
}

/// Build a minimal but genuine uv cache fixture: one referenced archive (kept
/// via a `wheels-v6` symlink) and one unreferenced archive (removed).
fn seed_archive_pair(cache: &Path) {
    let archive = cache.join("archive-v0");
    let referenced = archive.join("referenced");
    let unreferenced = archive.join("unreferenced");
    fs::create_dir_all(&referenced).unwrap();
    fs::create_dir_all(&unreferenced).unwrap();
    fs::write(referenced.join("payload"), b"REFERENCED").unwrap();
    fs::write(unreferenced.join("payload"), b"UNREFERENCED").unwrap();
    let wheels = cache.join("wheels-v6").join("pypi").join("demo");
    fs::create_dir_all(&wheels).unwrap();
    std::os::unix::fs::symlink("../../../archive-v0/referenced", wheels.join("link")).unwrap();
}

/// A live parent for the supervisor: its stdin is a real pipe whose write end is
/// held by this struct. Dropping (or `close`) the struct closes the write end,
/// which is the EOF the supervisor observes.
struct LiveParent {
    write: Option<std::fs::File>,
}

impl LiveParent {
    fn new() -> (Self, std::fs::File) {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut fds = [0_i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: we own both fds returned by pipe(2).
        let read = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let write = unsafe { std::fs::File::from_raw_fd(fds[1]) };
        for fd in fds {
            assert_eq!(
                unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        // The supervisor's stdin loop polls; make the read end nonblocking so
        // it never blocks while the write end is still open.
        let flags = unsafe { libc::fcntl(read.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(read.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        (Self { write: Some(write) }, read)
    }

    fn close(&mut self) {
        self.write.take();
    }
}

impl Drop for LiveParent {
    fn drop(&mut self) {
        self.write.take();
    }
}

// ---------------------------------------------------------------------------
// Fixture-only parser tests
// ---------------------------------------------------------------------------

#[test]
fn human_summary_parses_removed_file_and_plural_forms() {
    assert_eq!(
        parse_human_summary("Pruning cache at: /x\nRemoved 1 file (6B)\n"),
        HumanSummary::Removed { count: 1, bytes: 6 }
    );
    assert_eq!(
        parse_human_summary("Pruning cache at: /x\nRemoved 3 files (2.0KiB)\n"),
        HumanSummary::Removed {
            count: 3,
            bytes: 2048
        }
    );
    assert_eq!(
        parse_human_summary("Pruning cache at: /x\nRemoved 2 directories (6.0MiB)\n"),
        HumanSummary::Removed {
            count: 2,
            bytes: 6 * 1024 * 1024
        }
    );
}

#[test]
fn human_summary_absent_and_no_unused_are_no_work() {
    assert_eq!(
        parse_human_summary("No unused entries found\n"),
        HumanSummary::NoWork
    );
    assert_eq!(
        parse_human_summary("No cache found at: /x\n"),
        HumanSummary::NoWork
    );
}

#[test]
fn human_summary_unrecognized_noun_or_byte_is_never_zero() {
    // Bad noun.
    assert_eq!(
        parse_human_summary("Removed 3 widgets (2.0KiB)\n"),
        HumanSummary::Unrecognized
    );
    // Unknown unit.
    assert_eq!(
        parse_human_summary("Removed 3 files (2.0Z)\n"),
        HumanSummary::Unrecognized
    );
    // Missing byte figure.
    assert_eq!(
        parse_human_summary("Removed 3 files\n"),
        HumanSummary::Unrecognized
    );
    // Empty / unrelated output.
    assert_eq!(parse_human_summary(""), HumanSummary::Unrecognized);
}

#[test]
fn human_bytes_parses_producer_units_and_rejects_garbage() {
    assert_eq!(parse_human_bytes("11B"), Some(11));
    assert_eq!(parse_human_bytes("2.0KiB"), Some(2048));
    assert_eq!(parse_human_bytes("6.0MiB"), Some(6 * 1024 * 1024));
    assert_eq!(parse_human_bytes("nonsense"), None);
    assert_eq!(parse_human_bytes("-1B"), None);
}

#[test]
fn supervisor_result_enforces_payload_consistency() {
    let completed = parse_supervisor_result(
        br#"{"outcome":"completed","removedEntryCount":0,"removedLogicalBytes":0}"#,
    )
    .expect("completed parses");
    assert_eq!(completed.outcome, ToolCacheOutcome::Completed);
    assert_eq!(completed.removed_entry_count, Some(0));

    // Completed without accounting is allowed (successful but unrecognized).
    let completed_bare = parse_supervisor_result(br#"{"outcome":"completed"}"#).unwrap();
    assert_eq!(completed_bare.outcome, ToolCacheOutcome::Completed);
    assert_eq!(completed_bare.removed_entry_count, None);

    // Busy/failed must never carry counts.
    assert!(parse_supervisor_result(br#"{"outcome":"busy"}"#).is_some());
    assert!(parse_supervisor_result(br#"{"outcome":"busy","removedEntryCount":1}"#).is_none());
    assert!(parse_supervisor_result(br#"{"outcome":"failed","removedLogicalBytes":1}"#).is_none());
    // A completed with only one counter is inconsistent.
    assert!(parse_supervisor_result(br#"{"outcome":"completed","removedEntryCount":1}"#).is_none());
    assert!(parse_supervisor_result(br#"{"outcome":"unknown"}"#).is_none());
    assert!(parse_supervisor_result(b"not json").is_none());
}

#[test]
fn execute_supervisor_classifies_busy_only_with_both_markers_and_no_mutation_start() {
    // Busy: in-use warning + lock timeout, and no mutation-start marker.
    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!(
            "printf '%s\\n' '{}' >&2; printf 'error: Timeout (15s) when waiting for lock on \"/x\" at \"/x/.lock\"\\n' >&2; exit 2",
            super::BUSY_PHASE_MARKER
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (_parent, read) = LiveParent::new();
    let result = execute_supervisor(child, read, Duration::from_secs(5));
    assert_eq!(result.outcome, "busy");

    // Failed: non-zero exit without the in-use phase marker.
    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg("printf 'boom\\n' >&2; exit 3")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (_parent, read) = LiveParent::new();
    let result = execute_supervisor(child, read, Duration::from_secs(5));
    assert_eq!(result.outcome, "deliveryUnknown");

    // A busy-looking child that DID begin mutating is not Busy.
    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!(
            "printf '%s\\n' '{}' >&2; printf '{} /x\\n' >&2; exit 2",
            super::BUSY_PHASE_MARKER,
            super::MUTATION_START_MARKER
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (_parent, read) = LiveParent::new();
    let result = execute_supervisor(child, read, Duration::from_secs(5));
    assert_eq!(result.outcome, "deliveryUnknown");
}

#[test]
fn execute_supervisor_reaps_child_on_parent_stdin_eof() {
    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg("exec /bin/sleep 300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (mut parent, read) = LiveParent::new();
    parent.close(); // immediate EOF: the write end is gone.
    let started = std::time::Instant::now();
    let result = execute_supervisor(child, read, Duration::from_secs(60));
    assert_eq!(result.outcome, "deliveryUnknown");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "supervisor must reap on EOF, not run to the deadline"
    );
}

// ---------------------------------------------------------------------------
// Fixture-only containment tests
// ---------------------------------------------------------------------------

#[test]
fn containment_rejects_symlinked_bucket_root() {
    let root = OwnedRoot::new("symlink-bucket");
    let cache = root.cache();
    fs::create_dir_all(&cache).unwrap();
    let victim = root.root.join("victim");
    fs::create_dir_all(&victim).unwrap();
    std::os::unix::fs::symlink(&victim, cache.join("archive-v0")).unwrap();
    assert!(!fixed_cache_shape_safe(&cache));
    assert!(preflight_cache_root(&cache, &|| false).is_err());
}

#[test]
fn containment_rejects_symlinked_ancestor() {
    let root = OwnedRoot::new("symlink-ancestor");
    let real = root.root.join("real-cache");
    fs::create_dir_all(&real).unwrap();
    let link = root.root.join("link-cache");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    // `link-cache` is itself a symlink: its ancestors include `root`; the root
    // path being a symlink must be refused.
    assert!(!fixed_cache_shape_safe(&link));
}

#[test]
fn containment_rejects_symlinked_lock_and_hardlinked_marker() {
    let root = OwnedRoot::new("symlink-lock");
    let cache = root.cache();
    fs::create_dir_all(&cache).unwrap();
    let victim = root.root.join("victim-lock");
    fs::write(&victim, b"x").unwrap();
    std::os::unix::fs::symlink(&victim, cache.join(".lock")).unwrap();
    assert!(!fixed_cache_shape_safe(&cache));

    // A hard-linked `.lock` (nlink != 1) is likewise refused.
    let root2 = OwnedRoot::new("hardlink-lock");
    let cache2 = root2.cache();
    fs::create_dir_all(&cache2).unwrap();
    fs::write(cache2.join(".lock"), b"x").unwrap();
    fs::hard_link(cache2.join(".lock"), root2.root.join("elsewhere")).unwrap();
    assert!(!fixed_cache_shape_safe(&cache2));
}

// ---------------------------------------------------------------------------
// Producer-backed native tests (ignored by default)
// ---------------------------------------------------------------------------

/// Build a real, blackhole local wheel (no download) from a tiny source tree and
/// return its path plus the module name.
fn build_local_wheel(root: &Path, python: &Path) -> (PathBuf, String) {
    let src = root.join("wheel-src");
    fs::create_dir_all(src.join("demo_pkg")).unwrap();
    fs::write(
        src.join("pyproject.toml"),
        "[build-system]\nrequires = []\nbuild-backend = \"demo_backend\"\nbackend-path = [\".\"]\n[project]\nname = \"demo-pkg\"\nversion = \"0.0.1\"\n",
    )
    .unwrap();
    fs::write(src.join("demo_pkg/__init__.py"), "VALUE = 7\n").unwrap();
    // A trivial PEP 517 backend so no network build requirement is fetched.
    fs::write(
        src.join("demo_backend.py"),
        r#"
import base64, hashlib, os, sys, zipfile
def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    name = "demo_pkg-0.0.1-py3-none-any.whl"
    path = os.path.join(wheel_directory, name)
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("demo_pkg/__init__.py", "VALUE = 7\n")
        z.writestr("demo_pkg-0.0.1.dist-info/METADATA",
                   "Metadata-Version: 2.1\nName: demo-pkg\nVersion: 0.0.1\n")
        z.writestr("demo_pkg-0.0.1.dist-info/WHEEL",
                   "Wheel-Version: 1.0\nGenerator: demo\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
        z.writestr("demo_pkg-0.0.1.dist-info/RECORD", "")
    return name
"#,
    )
    .unwrap();
    // Build the wheel with a tiny inline builder (no network, no build isolation).
    let build_script = root.join("build_wheel.py");
    fs::write(
        &build_script,
        r#"
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "wheel-src"))
import demo_backend
print(demo_backend.build_wheel(os.environ["WHEEL_OUT"]))
"#,
    )
    .unwrap();
    let out = root.join("wheels");
    fs::create_dir_all(&out).unwrap();
    let status = Command::new(python)
        .arg(&build_script)
        .env("WHEEL_OUT", &out)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run wheel builder");
    assert!(status.success(), "wheel build must succeed");
    (
        out.join("demo_pkg-0.0.1-py3-none-any.whl"),
        "demo_pkg".to_owned(),
    )
}

fn uv_run_script(uv: &Path, cache: &Path, python: &Path, script: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(uv);
    cmd.arg("run")
        .arg("--no-config")
        .arg("--no-project")
        .arg("--no-progress")
        .arg("--cache-dir")
        .arg(cache)
        .arg("--python")
        .arg(python)
        .arg(script)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("UV_OFFLINE", "1")
        .env("UV_NO_PROGRESS", "1")
        .env("UV_PYTHON_DOWNLOADS", "never");
    cmd
}

/// Drive the real supervisor routine against the exact binary with immediate
/// stdin EOF. The supervisor notices the EOF, reaps the native child, and
/// reports the parent-gone (delivery-unknown) result.
fn run_native_eof(uv: &Path, cache: &Path) -> super::SupervisorResult {
    super::supervise_sync_for_tests(std::io::empty(), uv, cache)
}

/// Drive the real supervisor routine with a live parent whose write end closes
/// only when the supervisor has settled, letting the native child run to its own
/// bounded completion (so a pre-mutation Busy can be observed).
fn run_native_live(uv: &Path, cache: &Path) -> super::SupervisorResult {
    let (mut parent, read) = LiveParent::new();
    let result = super::supervise_sync_for_tests(read, uv, cache);
    parent.close();
    result
}

#[test]
#[ignore = "requires the exact staged uv 0.11.20 binary and an isolated cache root"]
fn availability_reports_available_for_isolated_root() {
    let uv = native_uv();
    let root = OwnedRoot::new("available");
    seed_bucket(&root.cache(), "archive-v0");
    let maintenance =
        UvCacheMaintenance::from_paths(uv, root.cache(), Duration::from_secs(30), &|| false);
    assert_eq!(
        maintenance.observe(|| false),
        ToolCacheAvailability::Available
    );
}

#[test]
#[ignore = "requires the exact staged uv 0.11.20 binary and an isolated cache root"]
fn availability_reports_absent_when_root_missing() {
    let uv = native_uv();
    let root = OwnedRoot::new("absent");
    let cache = root.cache();
    let maintenance = UvCacheMaintenance::from_paths(uv, cache, Duration::from_secs(30), &|| false);
    assert_eq!(maintenance.observe(|| false), ToolCacheAvailability::Absent);
}

#[test]
#[ignore = "requires the exact staged uv 0.11.20 binary and an isolated cache root"]
fn native_prune_removes_unreferenced_and_keeps_referenced() {
    let uv = native_uv();
    let root = OwnedRoot::new("prune");
    let cache = root.cache();
    seed_archive_pair(&cache);
    let result = run_native_live(&uv, &cache);
    assert_eq!(result.outcome, "completed");
    assert!(result.removed_entry_count.unwrap_or(0) >= 1);
    assert!(cache.join("archive-v0/referenced/payload").is_file());
    assert!(!cache.join("archive-v0/unreferenced").exists());

    // A second successful prune with no work is Completed with zero counters,
    // never NoOp and never a claim of no effect.
    let again = run_native_live(&uv, &cache);
    assert_eq!(again.outcome, "completed");
    assert_eq!(again.removed_entry_count, Some(0));
    assert_eq!(again.removed_logical_bytes, Some(0));
}

#[test]
#[ignore = "requires the exact staged uv 0.11.20 binary and an isolated cache root"]
fn native_prune_removes_every_grouped_escape_and_keeps_external_targets() {
    let uv = native_uv();
    let root = OwnedRoot::new("escape");
    let cache = root.cache();
    fs::create_dir_all(&cache).unwrap();

    // External targets that must never be touched, referenced by symlinks placed
    // at each mutating family the producer sweep reaches.
    let mut canaries = Vec::new();
    for name in ["stale-bucket", "env-entry", "archive-entry", "intermediate"] {
        let victim = root.root.join(format!("victim-{name}"));
        fs::create_dir_all(&victim).unwrap();
        fs::write(victim.join("canary"), b"CANARY").unwrap();
        canaries.push(victim.clone());
    }
    // Stale bucket root symlink.
    std::os::unix::fs::symlink(&canaries[0], cache.join("stale-bucket-vX")).unwrap();
    // Environment entry symlink.
    let envs = cache.join("environments-v2");
    fs::create_dir_all(&envs).unwrap();
    std::os::unix::fs::symlink(&canaries[1], envs.join("env-entry")).unwrap();
    // Archive entry symlink.
    let archives = cache.join("archive-v0");
    fs::create_dir_all(&archives).unwrap();
    std::os::unix::fs::symlink(&canaries[2], archives.join("archive-entry")).unwrap();
    // Intermediate directory symlink inside a bucket whose entries are walked.
    let sdists = cache.join("sdists-v9/pypi/demo");
    fs::create_dir_all(&sdists).unwrap();
    std::os::unix::fs::symlink(&canaries[3], sdists.join("intermediate")).unwrap();

    // A real external project `.venv` must be excluded and left intact.
    let project_venv = root.root.join("project/.venv");
    fs::create_dir_all(&project_venv).unwrap();
    fs::write(project_venv.join("sentinel"), b"VENV").unwrap();

    let result = run_native_live(&uv, &cache);
    assert_eq!(result.outcome, "completed");
    for victim in &canaries {
        assert!(
            victim.join("canary").is_file(),
            "canary {victim:?} was deleted"
        );
    }
    assert!(project_venv.join("sentinel").is_file());
    // The escape symlinks themselves are removed, never their targets.
    assert!(!cache.join("stale-bucket-vX").exists());
    assert!(!envs.join("env-entry").exists());
    assert!(!archives.join("archive-entry").exists());
}

/// A bounded script exits normally when released, even if the Rust assertion
/// unwinds. Its own deadline is a final bound if the test process disappears.
struct Holder {
    child: super::OwnedChild,
    release: PathBuf,
}
impl Holder {
    fn spawn(mut command: Command, release: PathBuf, ready: &Path) -> Self {
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn owned holder");
        let mut holder = Self {
            child: super::OwnedChild(child),
            release,
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(
                holder.child.0.try_wait().unwrap().is_none(),
                "holder exited before readiness"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "holder readiness deadline"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        holder
    }
    fn release(&mut self) {
        fs::write(&self.release, b"release").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.0.try_wait().unwrap() {
                assert!(status.success(), "holder must exit normally");
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "holder failed to release"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Holder {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, b"release");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while matches!(self.child.0.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

fn holder_script(root: &Path, preamble: &str) -> (PathBuf, PathBuf, PathBuf) {
    let script = root.join("holder.py");
    let release = root.join("release");
    let ready = root.join("ready");
    fs::write(&script, format!(
        "import os, time\n{preamble}\nopen({ready}, 'w').close()\ndeadline = time.monotonic() + 90\nwhile not os.path.exists({release}) and time.monotonic() < deadline:\n    time.sleep(0.025)\n{after}\n",
        ready = rust_str(ready.to_str().unwrap()), release = rust_str(release.to_str().unwrap()),
        after = if preamble.contains("demo_pkg") { "assert demo_pkg.VALUE == 7" } else { "" }
    )).unwrap();
    (script, release, ready)
}

fn start_uv_holder(uv: &Path, cache: &Path, python: &Path, root: &Path) -> Holder {
    let (script, release, ready) = holder_script(root, "");
    Holder::spawn(
        uv_run_script(uv, cache, python, &script, root),
        release,
        &ready,
    )
}

#[test]
#[ignore = "requires exact staged uv and local Python; real cached package, offline"]
fn native_prune_defers_live_package_then_rebuilds_its_cache_environment() {
    let (uv, python) = (native_uv(), local_python());
    let root = OwnedRoot::new("busy-package");
    let cache = root.cache();
    seed_archive_pair(&cache);
    let (wheel, _) = build_local_wheel(&root.root, &python);
    let (script, release, ready) =
        holder_script(&root.root, "import demo_pkg\nassert demo_pkg.VALUE == 7");
    let command = uv_run_with_wheel(&uv, &cache, &python, &script, &root.root, &wheel);
    let mut holder = Holder::spawn(command, release, &ready);
    let envs = cache.join("environments-v2");
    assert!(
        fs::read_dir(&envs).unwrap().next().is_some(),
        "must exercise a real cached environment"
    );
    let busy = run_native_live(&uv, &cache);
    assert_eq!(busy.outcome, "busy");
    assert!(cache.join("archive-v0/unreferenced/payload").is_file());
    holder.release(); // script also imports/uses its package after the refusal
    let after = run_native_live(&uv, &cache);
    assert_eq!(after.outcome, "completed");
    assert!(fs::read_dir(&envs).unwrap().next().is_none());
    assert!(cache.join("archive-v0/referenced/payload").is_file());
    assert!(!cache.join("archive-v0/unreferenced").exists());
    let check = root.root.join("check.py");
    fs::write(&check, "import demo_pkg; assert demo_pkg.VALUE == 7\n").unwrap();
    assert!(
        uv_run_with_wheel(&uv, &cache, &python, &check, &root.root, &wheel)
            .status()
            .unwrap()
            .success(),
        "ordinary uv operation rebuilds the pruned environment offline"
    );
}

fn uv_run_with_wheel(
    uv: &Path,
    cache: &Path,
    python: &Path,
    script: &Path,
    home: &Path,
    wheel: &Path,
) -> Command {
    let mut command = uv_run_script(uv, cache, python, Path::new("--with"), home);
    command.arg(wheel).arg(script);
    command
}

#[test]
#[ignore = "requires exact staged uv, installed uv and local Python, test-owned cache only"]
fn installed_uv_shared_holder_blocks_staged_prune() {
    let (uv, installed, python) = (native_uv(), installed_uv(), local_python());
    let root = OwnedRoot::new("interop");
    let cache = root.cache();
    seed_archive_pair(&cache);
    let mut holder = start_uv_holder(&installed, &cache, &python, &root.root);
    assert_eq!(run_native_live(&uv, &cache).outcome, "busy");
    assert!(cache.join("archive-v0/unreferenced/payload").is_file());
    holder.release();
    assert_eq!(run_native_live(&uv, &cache).outcome, "completed");
}

#[test]
#[ignore = "requires staged uv, installed uv and Python; verifies reverse native lock direction"]
fn native_exclusive_lock_blocks_ordinary_uv_until_release() {
    let (uv, installed, python) = (native_uv(), installed_uv(), local_python());
    for binary in [uv, installed] {
        let root = OwnedRoot::new("exclusive");
        let cache = root.cache();
        fs::create_dir_all(&cache).unwrap();
        // uv-fs LockedFile uses the OS flock shared/exclusive protocol on .lock.
        let (script, release, ready) = holder_script(
            &root.root,
            &format!(
                "import fcntl\nlock = open({}, 'a+b')\nfcntl.flock(lock, fcntl.LOCK_EX)",
                rust_str(cache.join(".lock").to_str().unwrap())
            ),
        );
        let mut locker = Command::new(&python);
        locker.arg(&script);
        let mut holder = Holder::spawn(locker, release, &ready);
        let ran = root.root.join("ran");
        let operation = root.root.join("operation.py");
        fs::write(
            &operation,
            format!("open({}, 'w').write('ok')", rust_str(ran.to_str().unwrap())),
        )
        .unwrap();
        let mut child = super::OwnedChild(
            uv_run_script(&binary, &cache, &python, &operation, &root.root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        std::thread::sleep(Duration::from_millis(400));
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(
            !ran.exists(),
            "ordinary operation must wait for the exclusive cache owner"
        );
        holder.release();
        assert!(child.0.wait().unwrap().success());
        assert_eq!(fs::read_to_string(ran).unwrap(), "ok");
    }
}

#[test]
#[ignore = "requires exact staged uv and local Python, test-owned cache only"]
fn native_prune_reaps_owned_child_on_parent_stdin_eof() {
    let (uv, python) = (native_uv(), local_python());
    let root = OwnedRoot::new("eof");
    let cache = root.cache();
    seed_archive_pair(&cache);
    let mut holder = start_uv_holder(&uv, &cache, &python, &root.root);
    let started = std::time::Instant::now();
    assert_eq!(run_native_eof(&uv, &cache).outcome, "deliveryUnknown");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(cache.join("archive-v0/unreferenced/payload").is_file());
    holder.release();
}

// ---------------------------------------------------------------------------
// Hidden-entry binary integration (requires the built daemon + staged uv)
// ---------------------------------------------------------------------------

/// Spawn the built daemon with the hidden supervisor entry and a live parent
/// pipe. Returns the child plus the write end that must be kept open.
fn spawn_supervisor_binary(
    bin: &Path,
    uv: &Path,
    cache: &Path,
) -> (std::process::Child, std::process::ChildStdin) {
    let mut child = Command::new(bin)
        .arg("--native-cache-child")
        .arg(uv)
        .arg(cache)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn supervisor binary");
    let write = child.stdin.take().expect("piped parent stdin");
    (child, write)
}

fn read_child_stdout(child: &mut std::process::Child) -> String {
    use std::io::Read as _;
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    out
}

#[test]
#[ignore = "requires the built daemon binary (UNLINGER_UV_SUPERVISOR_BIN) and staged uv"]
fn binary_hidden_entry_completes_with_a_live_parent() {
    let (bin, uv) = (supervisor_binary(), native_uv());
    let root = OwnedRoot::new("bin-complete");
    let cache = root.cache();
    seed_archive_pair(&cache);
    let (mut child, write) = spawn_supervisor_binary(&bin, &uv, &cache);
    // Keep the write end open so the supervisor waits for native settlement.
    let out = read_child_stdout(&mut child);
    let status = child.wait().expect("supervisor exits");
    drop(write);
    assert!(status.success());
    let parsed: super::SupervisorResult = serde_json::from_str(&out).expect("typed JSON");
    assert_eq!(parsed.outcome, "completed");
    assert!(parsed.removed_entry_count.unwrap_or(0) >= 1);
    assert!(cache.join("archive-v0/referenced/payload").is_file());
    assert!(!cache.join("archive-v0/unreferenced").exists());
}

#[test]
#[ignore = "requires the built daemon binary (UNLINGER_UV_SUPERVISOR_BIN) and staged uv"]
fn binary_hidden_entry_reaps_uv_when_parent_stdin_closes() {
    let (bin, uv) = (supervisor_binary(), native_uv());
    let python = local_python();
    let root = OwnedRoot::new("bin-eof");
    let cache = root.cache();
    seed_archive_pair(&cache);
    let mut holder = start_uv_holder(&uv, &cache, &python, &root.root);

    let (mut child, write) = spawn_supervisor_binary(&bin, &uv, &cache);
    // Parent gone immediately: close the write end before uv can hit its own
    // 15s lock timeout, so the reaping path (not the busy path) is exercised.
    drop(write);
    let started = std::time::Instant::now();
    let out = read_child_stdout(&mut child);
    let status = child.wait().expect("supervisor exits");
    assert!(status.success());
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "supervisor took {:?}; out={out}",
        started.elapsed()
    );
    let parsed: super::SupervisorResult = serde_json::from_str(&out).expect("typed JSON");
    assert_eq!(parsed.outcome, "deliveryUnknown");
    assert!(cache.join("archive-v0/unreferenced/payload").is_file());
    holder.release();
}

#[test]
fn supervisor_bounds_output_and_deadline_without_inventing_accounting() {
    for (script, budget, outcome) in [
        (
            "while :; do printf 'oversized-output\\n' >&2; done",
            Duration::from_secs(5),
            "deliveryUnknown",
        ),
        (
            "exec /bin/sleep 30",
            Duration::from_millis(100),
            "deliveryUnknown",
        ),
        (
            "printf 'unrecognized successful summary\\n' >&2",
            Duration::from_secs(5),
            "completed",
        ),
    ] {
        let child = Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (_parent, read) = LiveParent::new();
        let started = std::time::Instant::now();
        let result = execute_supervisor(child, read, budget);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(result.outcome, outcome);
        assert!(result.removed_entry_count.is_none());
        assert!(result.removed_logical_bytes.is_none());
    }
}

#[test]
#[ignore = "requires exact staged uv and Python; valid source-revision pointers, offline"]
fn source_revision_prune_removes_stale_sibling_without_traversing_external_link() {
    let (uv, python) = (native_uv(), local_python());
    let root = OwnedRoot::new("source-revision");
    let cache = root.cache();
    build_local_wheel(&root.root, &python);
    let script = root.root.join("check.py");
    fs::write(&script, "import demo_pkg; assert demo_pkg.VALUE == 7\n").unwrap();
    assert!(
        uv_run_with_wheel(
            &uv,
            &cache,
            &python,
            &script,
            &root.root,
            &root.root.join("wheel-src")
        )
        .status()
        .unwrap()
        .success()
    );
    let sdists = cache.join("sdists-v9");
    let mut queue = vec![sdists.clone()];
    let mut revision = None;
    while let Some(directory) = queue.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == "revision.rev" {
                revision = Some(entry.path());
            }
            if entry.file_type().unwrap().is_dir() {
                queue.push(entry.path());
            }
        }
    }
    let revision = revision.expect("real uv source build writes a native revision pointer");
    let stale = revision.parent().unwrap().join("stale-revision");
    fs::create_dir(&stale).unwrap();
    fs::write(stale.join("payload"), b"STALE").unwrap();
    let outside = root.root.join("outside");
    fs::create_dir_all(outside.join("stale-revision")).unwrap();
    fs::copy(&revision, outside.join("revision.rev")).unwrap();
    fs::write(outside.join("stale-revision/canary"), b"KEEP").unwrap();
    std::os::unix::fs::symlink(&outside, sdists.join("external-revisions")).unwrap();
    assert_eq!(run_native_live(&uv, &cache).outcome, "completed");
    assert!(
        !stale.exists(),
        "the valid pointer must exercise native revision removal"
    );
    assert_eq!(
        fs::read(outside.join("stale-revision/canary")).unwrap(),
        b"KEEP"
    );
    assert!(revision.is_file());
}
