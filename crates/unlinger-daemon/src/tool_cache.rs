//! Bounded, native npm download-cache (`_cacache`) maintenance.
//!
//! Unlinger does not implement cache garbage collection. This module discovers
//! the *producer's own* bundled `cacache` library and invokes its public
//! `verify(...)` API in-process through a tiny JavaScript driver
//! (`tool_cache_driver.js`). The only first family is the default npm download
//! content cache at `$HOME/.npm/_cacache` (npm 11.19.0 / cacache 20.0.4).
//!
//! The `_cacache` tree is a *rebuildable* content-addressed cache: a missing
//! tarball is a cache miss that the producer re-fetches, so an unattended sweep
//! needs no per-task registration. It is explicitly *not* the npx runtime tree
//! (`_npx`), the log directory (`_logs`), or the whole `~/.npm` directory.
//!
//! Safety posture:
//! - Diagnostics never carry raw paths outside transient memory; nothing here
//!   persists a path, an argument, or native output.
//! - The child is `env_clear`ed and given only fixed absolute paths, so
//!   `NODE_OPTIONS`, `NODE_PATH`, `NODE_COMPILE_CACHE`, and user npm config can
//!   never influence it.
//! - The parent holds the child's stdin write end for the whole child lifetime;
//!   the driver aborts on EOF, so a daemon crash cannot leave an indefinite GC.
//! - Only the exact child we spawned is ever signalled and reaped.

use serde::Deserialize;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use unlinger_protocol::{ToolCacheAvailability, ToolCacheOutcome};

/// Exact producer versions this adapter has been verified against. A mismatch is
/// `Unsupported`, never a silent best-effort run: the native counter semantics
/// and the containment assumptions are version-specific.
const EXPECTED_NPM_VERSION: &str = "11.19.0";
const EXPECTED_CACACHE_VERSION: &str = "20.0.4";

/// Fixed installation prefixes. No PATH is consulted; only these exact absolute
/// roots are probed for a real `node` and the bundled npm package.
const FIXED_PREFIXES: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// Fixed home-relative fallback prefix, used only when the sibling `bin` layout
/// is observed to exist.
const HOME_PREFIX_BIN: &str = ".local/bin";

/// The child's only output is one small JSON object. Anything larger is a
/// protocol violation and is refused rather than buffered.
const STDOUT_CAP_BYTES: usize = 16 * 1024;

/// Total wall-clock budget for one native maintenance run. `cacache.verify`
/// walks the content tree and checksums live content; a few seconds is typical
/// and larger caches may exceed the bound. Timeout is delivery-unknown.
const DEFAULT_RUNTIME_BUDGET: Duration = Duration::from_secs(120);

/// Poll cadence while waiting for the child.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The fixed `_cacache` bucket entries that the native sweep touches. The
/// preflight rejects a symlinked root or a symlinked entry in any of these
/// fixed buckets; the producer never legitimately creates symlinks here, so a
/// symlink is evidence of an unexpected cache shape and fails closed.
const SCANNED_BUCKETS: [&str; 3] = ["content-v2", "index-v5", "tmp"];

/// A native maintenance result. `removed_entry_count`/`removed_logical_bytes`
/// are the producer's own counters and are only ever present on a completed run;
/// they are never a measured physical-space reclaim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolCacheNativeResult {
    pub outcome: ToolCacheOutcome,
    pub removed_entry_count: Option<u64>,
    pub removed_logical_bytes: Option<u64>,
}

impl ToolCacheNativeResult {
    fn delivery_unknown() -> Self {
        Self {
            outcome: ToolCacheOutcome::DeliveryUnknown,
            removed_entry_count: None,
            removed_logical_bytes: None,
        }
    }
}

/// A discovered, allowlisted npm producer bound to one exact `_cacache` root.
///
/// Construction is the only place discovery runs. `start` revalidates the bound
/// identity immediately before spawning.
#[derive(Clone, Debug)]
pub struct NpmCacheMaintenance {
    node_binary: PathBuf,
    binding: Option<ProducerBinding>,
    root_identity: Option<(u64, u64)>,
    initial_availability: ToolCacheAvailability,
    cacache_package: PathBuf,
    cache_root: PathBuf,
    runtime_budget: Duration,
}

impl NpmCacheMaintenance {
    /// Discover the current user's supported npm producer and its default
    /// download cache. Never consults PATH, a shell, or user npm config.
    pub fn discover(should_cancel: impl Fn() -> bool) -> Result<Self, ToolCacheAvailability> {
        let home = PathBuf::from(
            crate::paths::effective_user_home(unsafe { libc::geteuid() })
                .map_err(|_| ToolCacheAvailability::Unavailable)?,
        );
        let cache_root = home.join(".npm").join("_cacache");

        let (node_binary, cacache_package) = discover_producer(&home)?;

        let maintenance = Self::from_paths(
            node_binary,
            cacache_package,
            cache_root,
            DEFAULT_RUNTIME_BUDGET,
            &should_cancel,
        );
        match maintenance.initial_availability {
            ToolCacheAvailability::Available => Ok(maintenance),
            other => Err(other),
        }
    }

    fn from_paths(
        node_binary: PathBuf,
        cacache_package: PathBuf,
        cache_root: PathBuf,
        runtime_budget: Duration,
        should_cancel: &impl Fn() -> bool,
    ) -> Self {
        let mut maintenance = Self {
            binding: ProducerBinding::capture(&node_binary, &cacache_package).ok(),
            root_identity: directory_identity(&cache_root),
            initial_availability: ToolCacheAvailability::Unavailable,
            node_binary,
            cacache_package,
            cache_root,
            runtime_budget,
        };
        maintenance.initial_availability = maintenance.availability(should_cancel);
        maintenance
    }

    /// Recompute the current availability without mutating anything. Bounded
    /// to five seconds of local traversal; no native mutator is invoked.
    #[must_use]
    pub fn observe(&self, should_cancel: impl Fn() -> bool) -> ToolCacheAvailability {
        self.availability(&should_cancel)
    }

    fn availability(&self, should_cancel: &impl Fn() -> bool) -> ToolCacheAvailability {
        if should_cancel() {
            return ToolCacheAvailability::Unavailable;
        }
        if !self.producer_is_allowlisted() {
            return ToolCacheAvailability::Unsupported;
        }
        if self.binding.is_none() {
            return ToolCacheAvailability::Unavailable;
        }
        match preflight_cache_root(&self.cache_root, should_cancel) {
            Ok(CacheRootState::Present) => ToolCacheAvailability::Available,
            Ok(CacheRootState::Absent) => ToolCacheAvailability::Absent,
            Err(()) => ToolCacheAvailability::Unavailable,
        }
    }

    fn producer_is_allowlisted(&self) -> bool {
        if !self.node_binary.is_absolute()
            || !self.cacache_package.is_absolute()
            || !self.node_binary.is_file()
            || !self.cacache_package.is_dir()
        {
            return false;
        }
        if read_package_version(&self.cacache_package, "cacache").as_deref()
            != Some(EXPECTED_CACACHE_VERSION)
        {
            return false;
        }
        // The cacache package must live inside a real bundled npm package at
        // the allowlisted version. `cacache_package` is `<npm-root>/node_modules/cacache`,
        // so the npm package root is two levels up.
        let Some(npm_root) = self.cacache_package.parent().and_then(Path::parent) else {
            return false;
        };
        read_package_version(npm_root, "npm").as_deref() == Some(EXPECTED_NPM_VERSION)
    }

    /// Revalidate identity and immediately spawn the native child. The caller is
    /// responsible for the PREPARED record and for observing the returned
    /// running handle; `start` never returns a partial success.
    pub fn start(&self) -> io::Result<RunningNpmMaintenance> {
        // Discovery has already checked the tree outside the IPC gate. Normal
        // cacache writers do not introduce symlinks. Rebind the exact producer,
        // root and fixed buckets here without a recursive walk under the gate.
        if self.initial_availability != ToolCacheAvailability::Available
            || !self.producer_is_allowlisted()
            || self.binding.as_ref()
                != ProducerBinding::capture(&self.node_binary, &self.cacache_package)
                    .ok()
                    .as_ref()
            || self.binding.is_none()
            || self.root_identity.is_none()
            || self.root_identity != directory_identity(&self.cache_root)
            || !fixed_cache_shape_safe(&self.cache_root)
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "npm cache maintenance is no longer available",
            ));
        }

        let mut command = Command::new(&self.node_binary);
        command
            .arg("-e")
            .arg(include_str!("tool_cache_driver.js"))
            .arg("--")
            .arg(&self.cacache_package)
            .arg(&self.cache_root)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        Ok(RunningNpmMaintenance::from_child(
            command.spawn()?,
            self.runtime_budget,
        ))
    }
}

/// An owned, running native maintenance child. Only this exact child is ever
/// signalled and reaped.
#[derive(Debug)]
pub struct RunningNpmMaintenance {
    child: Child,
    stdout: std::process::ChildStdout,
    output: Vec<u8>,
    readable: bool,
    /// Retained to keep the driver's stdin write end open. Dropped when `wait`
    /// consumes the handle (including on the cancel/timeout paths).
    _stdin: std::process::ChildStdin,
    started_at: Instant,
    runtime_budget: Duration,
}

impl RunningNpmMaintenance {
    fn from_child(mut child: Child, runtime_budget: Duration) -> Self {
        let stdin = child.stdin.take().expect("requested piped stdin");
        let stdout = child.stdout.take().expect("requested piped stdout");
        let flags = unsafe { libc::fcntl(stdout.as_raw_fd(), libc::F_GETFL) };
        let readable = flags >= 0
            && unsafe { libc::fcntl(stdout.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                == 0;
        Self {
            child,
            stdout,
            output: Vec::new(),
            readable,
            _stdin: stdin,
            started_at: Instant::now(),
            runtime_budget,
        }
    }

    /// Wait for the child, polling `should_cancel` between reads. The callback is
    /// how the caller aborts an exact running child on a lifecycle change.
    ///
    /// Result semantics:
    /// - exit 0 with a well-formed JSON object → `Completed` (all-zero counters
    ///   are reported as `NoOp`) with the native counters;
    /// - spawn/exec failure or a refused preflight → `Failed` with no counters;
    /// - non-zero exit → `Failed` (partial effects possible);
    /// - timeout, cancel, or unparsable output → `DeliveryUnknown`
    ///   with no counters, because the native run may have deleted content.
    pub fn wait(mut self, mut should_cancel: impl FnMut() -> bool) -> ToolCacheNativeResult {
        let deadline = self.started_at + self.runtime_budget;
        loop {
            if !self.readable || !self.drain_output() {
                self.kill_and_reap();
                return ToolCacheNativeResult::delivery_unknown();
            }
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    if !self.drain_output() {
                        return ToolCacheNativeResult::delivery_unknown();
                    }
                    return if status.success() {
                        parse_native_result(&self.output)
                            .unwrap_or_else(ToolCacheNativeResult::delivery_unknown)
                    } else {
                        ToolCacheNativeResult {
                            outcome: ToolCacheOutcome::Failed,
                            removed_entry_count: None,
                            removed_logical_bytes: None,
                        }
                    };
                }
                Ok(None) => {}
                Err(_) => {
                    self.kill_and_reap();
                    return ToolCacheNativeResult::delivery_unknown();
                }
            }
            if should_cancel() || Instant::now() >= deadline {
                self.kill_and_reap();
                return ToolCacheNativeResult::delivery_unknown();
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn drain_output(&mut self) -> bool {
        let mut chunk = [0_u8; 4096];
        loop {
            match self.stdout.read(&mut chunk) {
                Ok(0) => return true,
                Ok(count) => {
                    if self.output.len() + count > STDOUT_CAP_BYTES {
                        return false;
                    }
                    self.output.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return true,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
    }

    fn kill_and_reap(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for RunningNpmMaintenance {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            self.kill_and_reap();
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeCounters {
    reclaimed_count: u64,
    reclaimed_size: u64,
    bad_content_count: u64,
}

fn parse_native_result(bytes: &[u8]) -> Option<ToolCacheNativeResult> {
    let stats: NativeCounters = serde_json::from_slice(bytes).ok()?;
    if stats.bad_content_count > stats.reclaimed_count {
        return None;
    }
    Some(ToolCacheNativeResult {
        outcome: if stats.reclaimed_count == 0 && stats.reclaimed_size == 0 {
            ToolCacheOutcome::NoOp
        } else {
            ToolCacheOutcome::Completed
        },
        removed_entry_count: Some(stats.reclaimed_count),
        removed_logical_bytes: Some(stats.reclaimed_size),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProducerBinding(Vec<(u64, u64, u64, i64, i64)>);
impl ProducerBinding {
    fn capture(node: &Path, package: &Path) -> io::Result<Self> {
        let npm = package
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| io::Error::other("invalid producer layout"))?;
        let paths = [
            node.to_path_buf(),
            npm.join("package.json"),
            package.join("package.json"),
            package.join("lib/index.js"),
            package.join("lib/verify.js"),
        ];
        let uid = unsafe { libc::geteuid() };
        let mut facts = Vec::new();
        for path in paths {
            let meta = std::fs::metadata(path)?;
            if !meta.is_file() || ![0, uid].contains(&meta.uid()) || meta.mode() & 0o022 != 0 {
                return Err(io::Error::other("unsafe producer identity"));
            }
            facts.push((
                meta.dev(),
                meta.ino(),
                meta.len(),
                meta.mtime(),
                meta.mtime_nsec(),
            ));
        }
        Ok(Self(facts))
    }
}

fn directory_identity(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    (meta.is_dir() && !meta.file_type().is_symlink()).then_some((meta.dev(), meta.ino()))
}

fn fixed_cache_shape_safe(root: &Path) -> bool {
    let uid = unsafe { libc::geteuid() };
    let Some(parent) = root.parent() else {
        return false;
    };
    for path in [parent.to_path_buf(), root.to_path_buf()] {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return false;
        };
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != uid
            || meta.mode() & 0o022 != 0
        {
            return false;
        }
    }
    // verify writes this marker and truncates index buckets. Unlike content
    // unlinking, those writes could affect a linked object outside the cache.
    match std::fs::symlink_metadata(root.join("_lastverified")) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(meta)
            if meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.uid() == uid
                && meta.nlink() == 1
                && meta.mode() & 0o022 == 0 => {}
        _ => return false,
    }
    SCANNED_BUCKETS
        .iter()
        .all(|name| match std::fs::symlink_metadata(root.join(name)) {
            Err(error) => error.kind() == io::ErrorKind::NotFound,
            Ok(meta) => {
                meta.is_dir()
                    && !meta.file_type().is_symlink()
                    && meta.uid() == uid
                    && meta.mode() & 0o022 == 0
            }
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CacheRootState {
    Present,
    Absent,
}

/// Reject a cache root that is a symlink, not owned by the current user, or that
/// contains a symlink in any fixed bucket the native sweep touches. The producer
/// never legitimately creates these symlinks, so their presence fails closed.
fn preflight_cache_root(
    root: &Path,
    should_cancel: &impl Fn() -> bool,
) -> Result<CacheRootState, ()> {
    if should_cancel() {
        return Err(());
    }
    match std::fs::symlink_metadata(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(CacheRootState::Absent),
        Err(_) => return Err(()),
        Ok(_) => {}
    }
    if !fixed_cache_shape_safe(root) {
        return Err(());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut visited = 0_usize;
    for bucket in SCANNED_BUCKETS {
        let path = root.join(bucket);
        if path.exists() {
            check_tree(
                &path,
                deadline,
                &mut visited,
                0,
                bucket == "index-v5",
                should_cancel,
            )?;
        }
    }
    Ok(CacheRootState::Present)
}

fn check_tree(
    path: &Path,
    deadline: Instant,
    visited: &mut usize,
    depth: usize,
    writable_index: bool,
    should_cancel: &impl Fn() -> bool,
) -> Result<(), ()> {
    if depth > 64 || should_cancel() {
        return Err(());
    }
    let uid = unsafe { libc::geteuid() };
    for entry in std::fs::read_dir(path).map_err(|_| ())? {
        *visited += 1;
        if *visited > 1_000_000 || Instant::now() > deadline || should_cancel() {
            return Err(());
        }
        let entry = entry.map_err(|_| ())?;
        let meta = match std::fs::symlink_metadata(entry.path()) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue, // normal concurrent cache writer/GC
            Err(_) => return Err(()),
        };
        if meta.file_type().is_symlink()
            || meta.uid() != uid
            || (!meta.is_file() && !meta.is_dir())
            || (writable_index && meta.is_file() && meta.nlink() != 1)
        {
            return Err(());
        }
        if meta.is_dir() {
            check_tree(
                &entry.path(),
                deadline,
                visited,
                depth + 1,
                writable_index,
                should_cancel,
            )?;
        }
    }
    Ok(())
}

/// Locate a real node binary and the bundled cacache package under the fixed
/// prefixes, using symlink resolution against known package shapes only.
fn discover_producer(home: &Path) -> Result<(PathBuf, PathBuf), ToolCacheAvailability> {
    let mut prefixes: Vec<PathBuf> = FIXED_PREFIXES.iter().map(PathBuf::from).collect();
    let home_prefix = home.join(HOME_PREFIX_BIN);
    if home_prefix.is_dir() {
        prefixes.push(home_prefix);
    }

    let mut saw_any_producer = false;
    for prefix in prefixes {
        let node = prefix.join("node");
        let npm = prefix.join("npm");

        let Some(node_real) = resolve_regular_file(&node) else {
            continue;
        };
        let Some(npm_cli) = resolve_regular_file(&npm) else {
            continue;
        };
        saw_any_producer = true;

        // npm's package root contains `bin/npm-cli.js`, so it is the parent of
        // `bin`.
        let Some(npm_root) = npm_cli.parent().and_then(Path::parent) else {
            continue;
        };
        let cacache = npm_root.join("node_modules").join("cacache");
        if read_package_version(npm_root, "npm").as_deref() != Some(EXPECTED_NPM_VERSION) {
            continue;
        }
        if read_package_version(&cacache, "cacache").as_deref() != Some(EXPECTED_CACACHE_VERSION) {
            continue;
        }
        return Ok((node_real, cacache));
    }

    Err(if saw_any_producer {
        ToolCacheAvailability::Unsupported
    } else {
        ToolCacheAvailability::Absent
    })
}

/// Resolve a path through symlinks and require the final target to be a real
/// regular file. Returns the canonical target.
fn resolve_regular_file(path: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(path).ok()?;
    if resolved.is_file() {
        Some(resolved)
    } else {
        None
    }
}

#[derive(Deserialize)]
struct PackageVersion {
    name: String,
    version: String,
}
fn read_package_version(package_dir: &Path, expected_name: &str) -> Option<String> {
    let file = std::fs::File::open(package_dir.join("package.json")).ok()?;
    if file.metadata().ok()?.len() > 64 * 1024 {
        return None;
    }
    let value: PackageVersion = serde_json::from_reader(file.take(64 * 1024)).ok()?;
    (value.name == expected_name).then_some(value.version)
}

#[cfg(test)]
#[path = "tool_cache_tests.rs"]
mod tests;
