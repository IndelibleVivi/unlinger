//! Bounded, native uv cache (`~/.cache/uv`) maintenance.
//!
//! Unlinger does not implement cache garbage collection. This module discovers
//! the *producer's own* `uv` binary and invokes its public `uv cache prune`
//! command through a small supervised child. The admitted family is the default
//! uv cache at `$HOME/.cache/uv` (uv 0.11.20).
//!
//! Removal policy (exact uv 0.11.20 `Cache::prune`, see `uv-cache/src/lib.rs`):
//! - `uv_distribution::prune` first removes obsolete source revisions using
//!   revision pointers and a WalkDir traversal that does not follow symlinks;
//! - top-level *stale buckets* that are not a known `CacheBucket` are removed;
//! - every entry in `environments-v2/*` is removed wholesale (cached execution
//!   environments are never referenced by symlinks);
//! - every `archive-v0/*` entry is removed only when no `wheels-*`/`sdists-*`
//!   symlink resolves to it; a referenced archive is kept;
//! - internal `wheels-*`/`sdists-*` archive *symlinks* are legitimate and are
//!   followed only to compute references, never deleted as their targets.
//!
//! `--ci` and `--force` are never passed: ordinary retention stays intact and
//! in-use protection is never bypassed.
//!
//! Safety posture:
//! - Diagnostics never carry raw paths outside transient memory; nothing here
//!   persists a path, an argument, or native output.
//! - The native child is `env_clear`ed and given only fixed absolute paths plus
//!   the exact non-network knobs; no ambient config, proxy, or `UV_*` variable
//!   can influence it.
//! - The native mutator does not exit on stdin EOF, so a directly spawned `uv`
//!   could outlive a crashed daemon. `start` therefore spawns *this same
//!   executable* in a hidden supervisor mode that retains the exact `uv` child
//!   and kills/reaps it on parent EOF or deadline. Only exact children are ever
//!   signalled and reaped.
//! - The producer's own cache lock protects concurrent `uv run` owners: while
//!   any normal uv process holds the shared `.lock`, an exclusive `cache prune`
//!   waits and, past `UV_LOCK_TIMEOUT`, refuses *before* mutating. That refusal
//!   is the only `Busy` signal.

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use unlinger_protocol::{ToolCacheAvailability, ToolCacheOutcome};

/// Exact producer version this adapter has been verified against. A mismatch is
/// `Unsupported`, never a silent best-effort run: the native removal semantics,
/// the human summary, and the containment assumptions are version-specific.
const EXPECTED_UV_VERSION: &str = "0.11.20";

/// Fixed installation prefixes. No PATH is consulted; only these exact absolute
/// roots are probed for a real `uv` binary.
const FIXED_PREFIXES: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// Fixed home-relative fallback prefix, used only when the sibling `bin` layout
/// is observed to exist. This is where a per-user uv installation lives.
const HOME_PREFIX_BIN: &str = ".local/bin";

/// The default uv cache directory relative to the effective user home.
const HOME_CACHE_UV: &str = ".cache/uv";

/// The child's only output is one small JSON object. Anything larger is a
/// protocol violation and is refused rather than buffered.
const STDOUT_CAP_BYTES: usize = 16 * 1024;

/// Upper bound on captured native stderr while parsing the human summary. The
/// producer's summary is a handful of lines; anything larger is treated as a
/// protocol violation and the run is not parsed.
const STDERR_CAP_BYTES: usize = 64 * 1024;

/// Total wall-clock budget for one native maintenance run. `uv cache prune`
/// walks the cache buckets; larger caches may exceed the bound. Timeout is
/// delivery-unknown.
const DEFAULT_RUNTIME_BUDGET: Duration = Duration::from_secs(120);

/// Bounded wait for the producer's exclusive cache lock. A normal `uv run`
/// holder releases the shared lock only when its child exits; a bounded wait
/// turns a still-held lock into a provable pre-mutation `Busy` refusal instead
/// of an unbounded stall. Mirrored into `UV_LOCK_TIMEOUT`.
const NATIVE_LOCK_TIMEOUT_SECS: u64 = 15;

/// Extra grace the parent gives the supervisor to settle (reap its uv child)
/// after the parent's stdin closes or the budget expires.
const SUPERVISOR_SETTLE_GRACE: Duration = Duration::from_secs(NATIVE_LOCK_TIMEOUT_SECS + 10);

/// Poll cadence while waiting for the child.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Exact stderr phase marker the producer prints *before* mutating when the
/// cache is held by another uv process. Its absence rules out `Busy`.
const BUSY_PHASE_MARKER: &str = "Cache is currently in-use, waiting for other uv processes";

/// Exact stderr marker the producer prints when it refuses a held lock.
const BUSY_TIMEOUT_MARKER: &str = "when waiting for lock on";

/// Exact stderr marker the producer prints once it begins mutating. Its
/// presence means a `Busy` classification must be rejected.
const MUTATION_START_MARKER: &str = "Pruning cache at:";

/// Exact successful no-work summary printed when nothing was unused.
const SUMMARY_NO_UNUSED: &str = "No unused entries found";

/// Exact successful summary printed when the cache root does not exist.
const SUMMARY_NO_CACHE: &str = "No cache found at:";

/// The `Removed <count> <noun> (<human>)` summary the producer prints on a
/// successful non-empty prune.
const SUMMARY_REMOVED_PREFIX: &str = "Removed ";

/// The hidden argv flag the daemon intercepts before normal CLI parsing. Kept
/// here so the adapter and the entry point cannot drift apart.
pub const NATIVE_CACHE_CHILD_FLAG: &str = "--native-cache-child";

/// The hidden argv flag the daemon intercepts *before* normal CLI parsing and
/// which carries a per-run, parent-owned launch capability on a dedicated
/// inherited pipe fd. A raw `--native-cache-child` invocation is refused: the
/// daemon hidden entry is not itself admission authority.
pub const NATIVE_CACHE_CAPABILITY_FLAG: &str = "--native-cache-capability-fd";

/// One capability handshake line: the fields the parent alone can prove for the
/// exact run. The supervisor refuses to spawn the native mutator unless the
/// capability parses from an inherited pipe, the inherited root descriptor
/// still names the parent-proved object, and the producer still matches the
/// parent-proved binding. The
/// capability is a *structural accidental-bypass boundary* tying native spawn
/// to a durable matching PREPARED attempt; it is not a same-UID authentication
/// token or a hostile-process filesystem sandbox.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeCacheCapability {
    token: String,
    epoch: String,
    /// Exact cache-root device/inode the parent opened and will bind the native
    /// mutator's working directory to.
    root_device: u64,
    root_inode: u64,
    producer_binding: ProducerBinding,
}

/// A native maintenance result. `removed_entry_count`/`removed_logical_bytes`
/// are the producer's own summary numbers and are only ever present on a
/// completed run whose summary line was recognized; they are never a measured
/// physical-space reclaim. The count is what the producer reports; the byte
/// figure is the producer's human-rounded value and is therefore approximate.
///
/// A successful run whose summary was *not* recognized yields `Completed` with
/// `None` accounting — never an invented zero.
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

    fn failed() -> Self {
        Self {
            outcome: ToolCacheOutcome::Failed,
            removed_entry_count: None,
            removed_logical_bytes: None,
        }
    }

    fn busy() -> Self {
        Self {
            outcome: ToolCacheOutcome::Busy,
            removed_entry_count: None,
            removed_logical_bytes: None,
        }
    }
}

/// A discovered, allowlisted uv producer bound to one exact cache root.
///
/// Construction is the only place discovery and version proof run. `start`
/// revalidates only cheap metadata identity immediately before spawning, so it
/// never runs discovery or waits while holding the daemon's IPC/status lock.
#[derive(Clone, Debug)]
pub struct UvCacheMaintenance {
    uv_binary: PathBuf,
    binding: Option<ProducerBinding>,
    root_identity: Option<(u64, u64)>,
    initial_availability: ToolCacheAvailability,
    cache_root: PathBuf,
    runtime_budget: Duration,
}

impl UvCacheMaintenance {
    /// Discover the current user's supported uv producer and its default cache.
    /// Never consults PATH, a shell, or ambient uv config. The version proof
    /// (`uv --version`) runs here, outside any daemon lock.
    pub fn discover(should_cancel: impl Fn() -> bool) -> Result<Self, ToolCacheAvailability> {
        let home = PathBuf::from(
            crate::paths::effective_user_home(unsafe { libc::geteuid() })
                .map_err(|_| ToolCacheAvailability::Unavailable)?,
        );
        let cache_root = home.join(HOME_CACHE_UV);

        let (uv_binary, proved_binding) = discover_producer(&home, &should_cancel)?;

        let maintenance = Self::from_paths(
            uv_binary,
            cache_root,
            DEFAULT_RUNTIME_BUDGET,
            &should_cancel,
        );
        if maintenance.binding.as_ref() != Some(&proved_binding) {
            return Err(ToolCacheAvailability::Unavailable);
        }
        match maintenance.initial_availability {
            ToolCacheAvailability::Available => Ok(maintenance),
            other => Err(other),
        }
    }

    fn from_paths(
        uv_binary: PathBuf,
        cache_root: PathBuf,
        runtime_budget: Duration,
        should_cancel: &impl Fn() -> bool,
    ) -> Self {
        let mut maintenance = Self {
            binding: ProducerBinding::capture(&uv_binary).ok(),
            root_identity: directory_identity(&cache_root),
            initial_availability: ToolCacheAvailability::Unavailable,
            uv_binary,
            cache_root,
            runtime_budget,
        };
        maintenance.initial_availability = maintenance.availability(should_cancel);
        maintenance
    }

    /// Recompute the current availability without mutating anything. Only fixed paths are inspected; no native mutator is invoked. The
    /// version proof is metadata-bound in this path (see `availability`).
    #[must_use]
    pub fn observe(&self, should_cancel: impl Fn() -> bool) -> ToolCacheAvailability {
        self.availability(&should_cancel)
    }

    fn availability(&self, should_cancel: &impl Fn() -> bool) -> ToolCacheAvailability {
        if should_cancel() {
            return ToolCacheAvailability::Unavailable;
        }
        // `observe` runs on the daemon observation path; it must not fork a
        // subprocess under a lock. The initial version proof already ran during
        // discovery and the producer binding pins the exact binary inode, so a
        // later observe only revalidates that metadata identity.
        if self.binding.is_none()
            || self.binding.as_ref() != ProducerBinding::capture(&self.uv_binary).ok().as_ref()
        {
            return ToolCacheAvailability::Unavailable;
        }
        match preflight_cache_root(&self.cache_root, should_cancel) {
            Ok(CacheRootState::Present) => ToolCacheAvailability::Available,
            Ok(CacheRootState::Absent) => ToolCacheAvailability::Absent,
            Err(()) => ToolCacheAvailability::Unavailable,
        }
    }

    /// Open the exact cache-root directory the parent proved, as a bounded
    /// no-follow read-only descriptor, and require it to still be the proved
    /// device/inode. This is the object the supervisor binds the native
    /// mutator's working directory to; a later pathname replacement cannot
    /// redirect it.
    fn open_proved_root(&self) -> io::Result<OwnedFd> {
        let Some((device, inode)) = self.root_identity else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "uv cache root has no proved identity",
            ));
        };
        let fd = open_directory_no_follow(&self.cache_root)?;
        let meta = fstat(&fd)?;
        if !meta.is_dir() || meta.dev() != device || meta.ino() != inode {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "uv cache root changed identity before native open",
            ));
        }
        Ok(fd)
    }

    /// Revalidate identity, bind the proved root object, and immediately spawn
    /// the supervisor downstream of the durable PREPARED record for
    /// `attempt_token`/`epoch`.
    ///
    /// `attempt_token` and `epoch` come only from the PREPARED-gated start path;
    /// ordinary argv does not supply the capability.
    /// The caller is responsible for the PREPARED record and for observing the
    /// returned running handle; `start` never returns a partial success.
    ///
    /// The producer version was proved during discovery. The expected producer
    /// binding crosses the supervisor boundary in the private capability; the
    /// cache root crosses as an already-open descriptor. Only the owned
    /// supervisor spawn occurs under the caller's status/IPC lock; waiting
    /// happens outside it.
    pub fn start(&self, attempt_token: &str, epoch: &str) -> io::Result<RunningUvMaintenance> {
        if self.initial_availability != ToolCacheAvailability::Available
            || self.binding.as_ref() != ProducerBinding::capture(&self.uv_binary).ok().as_ref()
            || self.binding.is_none()
            || self.root_identity.is_none()
            || self.root_identity != directory_identity(&self.cache_root)
            || !fixed_cache_shape_safe(&self.cache_root)
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "uv cache maintenance is no longer available",
            ));
        }
        let Some((root_device, root_inode)) = self.root_identity else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "uv cache root has no proved identity",
            ));
        };

        let root_fd = self.open_proved_root()?;
        let producer_binding = self
            .binding
            .clone()
            .ok_or_else(|| io::Error::other("uv producer binding is unavailable"))?;

        // The capability pipe is created by the parent and the read end is
        // inherited by the supervisor at a fixed, dedicated number.
        let (read_fd, write_fd) = pipe_cloexec()?;
        let capability = NativeCacheCapability {
            token: attempt_token.to_owned(),
            epoch: epoch.to_owned(),
            root_device,
            root_inode,
            producer_binding,
        };
        let capability_line =
            serde_json::to_vec(&capability).map_err(|error| io::Error::other(error.to_string()))?;
        let mut writer = std::fs::File::from(write_fd);
        {
            use std::io::Write as _;
            writer.write_all(&capability_line)?;
            writer.flush()?;
        }
        drop(writer); // EOF is the capability boundary: the supervisor must not
        // observe a torn or absent line.

        // The native mutator does not exit on stdin EOF, so spawn *ourselves* in
        // the hidden supervisor mode. The supervisor retains the exact uv child
        // and reaps it on parent EOF or deadline.
        let supervisor = std::env::current_exe()?;
        let mut command = Command::new(supervisor);
        command
            .arg(NATIVE_CACHE_CHILD_FLAG)
            .arg(NATIVE_CACHE_CAPABILITY_FLAG)
            .arg(read_fd.as_raw_fd().to_string())
            .arg(root_fd.as_raw_fd().to_string())
            .arg(&self.uv_binary)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // `Command::pre_exec` runs only in the forked child. Clear CLOEXEC there
        // for the two exact descriptors whose numbers are already in argv; the
        // parent copies remain CLOEXEC and are dropped after a successful spawn.
        let capability_read_raw = read_fd.as_raw_fd();
        let root_raw = root_fd.as_raw_fd();
        unsafe {
            command.pre_exec(move || {
                make_fd_inheritable(capability_read_raw)?;
                make_fd_inheritable(root_raw)?;
                Ok(())
            });
        }

        let child = command.spawn()?;
        // The parent's copies of the inherited ends are no longer needed once
        // the supervisor has them; dropping closes them here.
        drop(read_fd);
        drop(root_fd);

        Ok(RunningUvMaintenance::from_child(child, self.runtime_budget))
    }
}

/// An owned, running supervised maintenance child. Only this exact child is ever
/// signalled and reaped.
#[derive(Debug)]
pub struct RunningUvMaintenance {
    child: Child,
    stdout: std::process::ChildStdout,
    output: Vec<u8>,
    readable: bool,
    /// The supervisor's stdin write end, retained for the whole child lifetime.
    /// Taking it (and dropping it) is how the parent tells the supervisor it is
    /// gone so the supervisor reaps the exact uv child.
    stdin: Option<ChildStdin>,
    started_at: Instant,
    runtime_budget: Duration,
}

impl RunningUvMaintenance {
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
            stdin: Some(stdin),
            started_at: Instant::now(),
            runtime_budget,
        }
    }

    /// Wait for the supervised child, polling `should_cancel` between reads. The
    /// callback is how the caller aborts an exact running child on a lifecycle
    /// change.
    ///
    /// Result semantics, from the supervisor's typed terminal object:
    /// - exit 0 with a well-formed object → the produced outcome
    ///   (`Completed`, including all-zero counters, or pre-mutation `Busy`);
    /// - spawn/exec failure or a refused preflight → `Failed` with no counters;
    /// - non-zero exit, timeout, cancel, or unparsable/oversized output →
    ///   `DeliveryUnknown` with no counters, because the native run may have
    ///   deleted content.
    ///
    /// On cancel or timeout the parent first closes the supervisor's stdin (so
    /// the supervisor reaps the exact uv child) and waits a bounded settle grace
    /// before only then killing the supervisor.
    pub fn wait(mut self, mut should_cancel: impl FnMut() -> bool) -> ToolCacheNativeResult {
        let deadline = self.started_at + self.runtime_budget;
        loop {
            if !self.readable || !self.drain_output() {
                return self.settle_and_reap(ToolCacheNativeResult::delivery_unknown());
            }
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    if !self.drain_output() {
                        return ToolCacheNativeResult::delivery_unknown();
                    }
                    return if status.success() {
                        parse_supervisor_result(&self.output)
                            .unwrap_or_else(ToolCacheNativeResult::delivery_unknown)
                    } else {
                        ToolCacheNativeResult::delivery_unknown()
                    };
                }
                Ok(None) => {}
                Err(_) => {
                    return self.settle_and_reap(ToolCacheNativeResult::delivery_unknown());
                }
            }
            if should_cancel() || Instant::now() >= deadline {
                // Request settlement: close stdin so the supervisor reaps its own
                // uv child, then wait a bounded grace before signalling.
                return self.settle_and_reap(ToolCacheNativeResult::delivery_unknown());
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

    /// Ask the supervisor to settle (by closing its stdin) and give it a bounded
    /// grace to reap its own uv child before, as a last resort, killing the
    /// supervisor. Returns `result` unchanged.
    fn settle_and_reap(&mut self, result: ToolCacheNativeResult) -> ToolCacheNativeResult {
        drop(self.stdin.take());
        let deadline = Instant::now() + SUPERVISOR_SETTLE_GRACE;
        loop {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return result;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                return result;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

impl Drop for RunningUvMaintenance {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            // Ordinary cancel/drop: close stdin so the supervisor reaps the exact
            // uv child, and wait a bounded grace before only then killing the
            // supervisor itself.
            let _ = self.settle_and_reap(ToolCacheNativeResult::delivery_unknown());
        }
    }
}

/// The single typed object the supervisor writes on stdout.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SupervisorResult {
    pub(crate) outcome: String,
    #[serde(default)]
    pub(crate) removed_entry_count: Option<u64>,
    #[serde(default)]
    pub(crate) removed_logical_bytes: Option<u64>,
}

/// Translate the supervisor's typed terminal object into a native result.
///
/// Consistency is enforced strictly: `busy` and `failed` may not carry counts,
/// `completed` may carry counts only if both are present, and any unknown
/// outcome is refused (delivery-unknown), never reinterpreted.
fn parse_supervisor_result(bytes: &[u8]) -> Option<ToolCacheNativeResult> {
    if bytes.is_empty() || bytes.len() > STDOUT_CAP_BYTES {
        return None;
    }
    let parsed: SupervisorResult = serde_json::from_slice(bytes).ok()?;
    match parsed.outcome.as_str() {
        "completed" => match (parsed.removed_entry_count, parsed.removed_logical_bytes) {
            (None, None) => Some(ToolCacheNativeResult {
                outcome: ToolCacheOutcome::Completed,
                removed_entry_count: None,
                removed_logical_bytes: None,
            }),
            (Some(count), Some(bytes)) => Some(ToolCacheNativeResult {
                outcome: ToolCacheOutcome::Completed,
                removed_entry_count: Some(count),
                removed_logical_bytes: Some(bytes),
            }),
            _ => None,
        },
        "busy"
            if parsed.removed_entry_count.is_none() && parsed.removed_logical_bytes.is_none() =>
        {
            Some(ToolCacheNativeResult::busy())
        }
        "failed"
            if parsed.removed_entry_count.is_none() && parsed.removed_logical_bytes.is_none() =>
        {
            Some(ToolCacheNativeResult::failed())
        }
        _ => None,
    }
}

/// The result of parsing the producer's human summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HumanSummary {
    /// A recognized non-empty removal: exact count plus the producer's rounded
    /// byte figure.
    Removed { count: u64, bytes: u64 },
    /// A recognized successful no-op (`No unused entries found` /
    /// `No cache found at:`), which is a Completed run with zero entries removed.
    NoWork,
    /// A successful run whose summary line was not recognized. The run finished
    /// but removed an unknown amount; never an invented zero.
    Unrecognized,
}

/// Parse the producer's own human summary from captured stderr.
///
/// The exact uv 0.11.20 summary lines (stderr) are:
/// - `Pruning cache at: <path>` (diagnostic; the path is never persisted)
/// - `Removed <N> file(s) (<human>)` or `Removed <N> directory(ies) (<human>)`
/// - `No unused entries found`
/// - `No cache found at: <path>` — the absence case; nothing was removed.
///
/// Only these exact shapes are recognized. A `Removed` line whose noun or byte
/// figure is not exactly understood is `Unrecognized`, never fabricated as
/// zero. The byte figure is the producer's human-rounded value, so it is
/// approximate, never a measured reclaim. No raw path or raw output is returned.
fn parse_human_summary(stderr: &str) -> HumanSummary {
    let mut has_no_unused = false;
    let mut has_no_cache = false;
    for line in stderr.lines() {
        let line = line.trim();
        if line == SUMMARY_NO_UNUSED {
            has_no_unused = true;
            continue;
        }
        if line.starts_with(SUMMARY_NO_CACHE) {
            has_no_cache = true;
            continue;
        }
        let Some(rest) = line.strip_prefix(SUMMARY_REMOVED_PREFIX) else {
            continue;
        };
        match parse_removed_line(rest) {
            Some(pair) => {
                return HumanSummary::Removed {
                    count: pair.0,
                    bytes: pair.1,
                };
            }
            // A `Removed ...` line we cannot parse exactly is not a zero.
            None => return HumanSummary::Unrecognized,
        }
    }
    if has_no_unused || has_no_cache {
        HumanSummary::NoWork
    } else {
        HumanSummary::Unrecognized
    }
}

/// Parse the remainder of a `Removed ...` line: `<count> <noun> (<human>)`.
/// The noun must be exactly `file`/`files`/`directory`/`directories`, and the
/// parenthesised byte figure must be a finite, non-negative recognized unit.
fn parse_removed_line(rest: &str) -> Option<(u64, u64)> {
    let mut parts = rest.splitn(2, ' ');
    let count_text = parts.next()?;
    let remainder = parts.next()?.trim();
    let count: u64 = count_text.parse().ok()?;
    let open = remainder.find('(')?;
    let close = remainder.find(')')?;
    if close <= open || close + 1 != remainder.len() {
        return None;
    }
    let noun = remainder[..open].trim();
    if !matches!(noun, "file" | "files" | "directory" | "directories") {
        return None;
    }
    let bytes = parse_human_bytes(&remainder[open + 1..close])?;
    Some((count, bytes))
}

/// Parse the producer's human byte figure (`11B`, `2.0KiB`, `6.0MiB`). Rejects
/// unknown units, negative values, and non-finite numbers.
fn parse_human_bytes(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let value: f64 = number.trim().parse().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let multiplier: f64 = match unit.trim() {
        "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let bytes = value * multiplier;
    if bytes > u64::MAX as f64 {
        return None;
    }
    Some(bytes as u64)
}

/// The supervisor entry point, called by the daemon binary before normal CLI
/// parsing when argv carries [`NATIVE_CACHE_CHILD_FLAG`].
///
/// Contract (matching `main`, which strips the flag before calling). A raw
/// hidden-child invocation carrying only producer/root path argv is *refused*:
/// a native mutator is opened only when the capability flag precedes the
/// remaining arguments, the inherited capability line names the matching
/// attempt/run and producer binding, and an inherited descriptor still names
/// the parent-proved root object.
///   args[0] = [`NATIVE_CACHE_CAPABILITY_FLAG`]
///   args[1] = inherited capability-pipe fd number
///   args[2] = inherited cache-root directory fd number
///   args[3] = absolute uv binary path, checked against the capability binding
///   stdout  = one small JSON object on success; nothing otherwise
///   exit 0  = supervisor settled and emitted a typed result
///   exit 7  = supervisor could not spawn or parse; no result emitted
///
/// The supervisor holds the exact uv child, polls stdin closure and the native
/// deadline, and kills/reaps the uv child before returning. It never touches
/// daemon, IPC, store, or service state.
pub fn run_uv_supervisor(args: &[std::ffi::OsString]) -> i32 {
    let Ok(result) = execute_uv_supervisor(args, libc::STDIN_FILENO) else {
        return 7;
    };
    let Ok(json) = serde_json::to_string(&result) else {
        return 7;
    };
    print!("{json}");
    0
}

fn execute_uv_supervisor(
    args: &[std::ffi::OsString],
    parent_channel_fd: i32,
) -> io::Result<SupervisorResult> {
    let [capability_flag, capability_fd, root_fd, binary] = args else {
        // A raw hidden-child invocation (producer/root path argv only) is
        // refused before any native open: argv alone is never authority.
        return Err(io::Error::other("missing native cache capability"));
    };
    if capability_flag != NATIVE_CACHE_CAPABILITY_FLAG {
        return Err(io::Error::other("invalid native cache capability flag"));
    }
    let capability_fd = parse_inherited_fd(capability_fd)
        .ok_or_else(|| io::Error::other("invalid capability descriptor"))?;
    let root_fd = parse_inherited_fd(root_fd)
        .ok_or_else(|| io::Error::other("invalid cache-root descriptor"))?;
    if capability_fd == root_fd {
        return Err(io::Error::other("capability descriptors overlap"));
    }
    let binary = PathBuf::from(binary);
    if !binary.is_absolute() {
        return Err(io::Error::other("uv producer path is not absolute"));
    }
    let capability = read_capability(capability_fd)?;
    let root_fd = take_fd(root_fd)?;
    // Revalidate the exact root object inherited from the parent and the
    // producer binding carried by the private capability before native spawn.
    // The root can no longer be redirected by replacing its pathname. The
    // producer has one final metadata check at the spawn boundary; this project
    // does not claim a hostile same-UID filesystem sandbox.
    let root_meta = fstat(&root_fd)?;
    let producer_binding = ProducerBinding::capture(&binary)?;
    if !root_meta.is_dir()
        || root_meta.dev() != capability.root_device
        || root_meta.ino() != capability.root_inode
        || producer_binding != capability.producer_binding
        || capability.token.is_empty()
        || capability.epoch.is_empty()
    {
        return Err(io::Error::other(
            "native cache capability identity mismatch",
        ));
    }
    // Fail before spawning if parent EOF cannot be observed without blocking.
    set_fd_nonblocking(parent_channel_fd)?;
    // Preflight the parent channel *before* any native open: a parent that has
    // already closed stdin (or left it unreadable) must never reach native
    // spawn. `WouldBlock` means the parent is still live; a zero-length read is
    // EOF and any other error is a dead or unreadable channel.
    preflight_parent_channel(parent_channel_fd)?;
    let stdin = NonblockingStdin {
        fd: parent_channel_fd,
    };
    supervise(stdin, &binary, root_fd, DEFAULT_RUNTIME_BUDGET)
}

fn parse_inherited_fd(value: &std::ffi::OsStr) -> Option<i32> {
    value
        .to_str()?
        .parse::<i32>()
        .ok()
        .filter(|fd| *fd > libc::STDERR_FILENO)
}

/// Rehome one inherited fd into an `OwnedFd`, refusing a closed descriptor. The
/// parent cleared CLOEXEC only in the supervisor process and consumes the
/// descriptor exactly once here.
fn take_fd(fd: i32) -> io::Result<OwnedFd> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the parent inherited this exact descriptor into the supervisor
    // and transfers ownership of that descriptor number exactly once here.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Read the whole capability line from the inherited read end. The parent
/// closes its write end immediately after writing, so a nonblocking read
/// observes EOF rather than blocking; a torn or oversized line is refused.
fn read_capability(fd: i32) -> io::Result<NativeCacheCapability> {
    let read_fd = take_fd(fd)?;
    let flags = unsafe { libc::fcntl(read_fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(read_fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut file = std::fs::File::from(read_fd);
    use std::io::Read as _;
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 512];
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match file.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                captured.extend_from_slice(&chunk[..count]);
                if captured.len() > 4096 {
                    return Err(io::Error::other("capability line oversized"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(io::Error::other("capability line deadline"));
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(error) => return Err(error),
        }
    }
    if captured.is_empty() {
        return Err(io::Error::other("missing capability line"));
    }
    serde_json::from_slice::<NativeCacheCapability>(&captured)
        .map_err(|error| io::Error::other(error.to_string()))
}

fn make_fd_inheritable(fd: i32) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_fd_nonblocking(fd: i32) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Prove the parent channel is still live before any native open. `stdin` has
/// already been set nonblocking, so one bounded read distinguishes a live
/// parent (`WouldBlock`, no bytes yet) from `EOF` (zero bytes) or a dead/
/// unreadable channel (any other error). Any outcome other than a live parent
/// is an error and the supervisor returns without spawning.
fn preflight_parent_channel(fd: i32) -> io::Result<()> {
    let mut byte = [0_u8; 1];
    loop {
        let result =
            unsafe { libc::read(fd, byte.as_mut_ptr().cast::<libc::c_void>(), byte.len()) };
        if result < 0 {
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::WouldBlock => return Ok(()),
                io::ErrorKind::Interrupted => continue,
                _ => return Err(error),
            }
        }
        if result == 0 {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "parent channel closed before native spawn",
            ));
        }
        return Err(io::Error::other("unexpected parent channel byte"));
    }
}

/// `fstat` a raw fd, rejecting a negative return.
fn fstat(fd: &OwnedFd) -> io::Result<std::fs::Metadata> {
    std::fs::File::from(fd.try_clone()?).metadata()
}

/// Open a path read-only with `O_NOFOLLOW`, so the final component must be a
/// real object and never a symlink to something else.
fn open_no_follow(path: &Path, extra_flags: libc::c_int) -> io::Result<OwnedFd> {
    use std::os::unix::ffi::OsStrExt as _;
    let mut bytes = path.as_os_str().as_bytes().to_vec();
    bytes.push(0);
    let raw = unsafe {
        libc::open(
            bytes.as_ptr().cast::<libc::c_char>(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | extra_flags,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a freshly opened fd we exclusively own.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

fn open_directory_no_follow(path: &Path) -> io::Result<OwnedFd> {
    open_no_follow(path, libc::O_DIRECTORY)
}

/// Create a `CLOEXEC` pipe. The write end is written and closed before spawn so
/// the supervisor observes one complete capability document plus EOF.
fn pipe_cloexec() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0_i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    for fd in fds {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
            let error = io::Error::last_os_error();
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            return Err(error);
        }
    }
    // SAFETY: `pipe(2)` returned two fresh fds we exclusively own. The child
    // clears CLOEXEC only for the read end selected by this invocation.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// Borrows process-owned stdin, already verified nonblocking before spawning.
struct NonblockingStdin {
    fd: i32,
}

/// Reap the exact retained child on cancellation, error, or unwinding.
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

impl Read for NonblockingStdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let result =
            unsafe { libc::read(self.fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result as usize)
    }
}

/// Retain and supervise one exact `uv cache prune` child. A spawn failure is
/// returned as `io::Error`; every successfully spawned child is reaped before a
/// typed terminal result is returned.
///
/// The producer path has already been revalidated against the binding carried
/// by the parent capability. The cache root is not looked up again: `pre_exec`
/// binds the child working directory to the inherited proved directory object,
/// and the relative `--cache-dir .` therefore starts from that object. As with
/// the rest of this local-user boundary, this does not claim resistance to a
/// deliberately racing hostile same-UID process.
fn supervise(
    stdin: impl Read,
    producer: &Path,
    root_fd: OwnedFd,
    budget: Duration,
) -> io::Result<SupervisorResult> {
    let mut command = Command::new(producer);
    command
        .arg("cache")
        .arg("prune")
        .arg("--no-config")
        .arg("--no-progress")
        .arg("--cache-dir")
        .arg(".")
        // `env_clear` drops any ambient environment (proxies, `UV_*` knobs,
        // credentials); the mutator needs only its pinned working directory
        // plus the non-network knobs below.
        .env_clear()
        .env("UV_OFFLINE", "1")
        .env("UV_NO_CONFIG", "1")
        .env("UV_NO_PROGRESS", "1")
        .env("UV_PYTHON_DOWNLOADS", "never")
        .env("UV_LOCK_TIMEOUT", NATIVE_LOCK_TIMEOUT_SECS.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let root_raw = root_fd.as_raw_fd();
    unsafe {
        command.pre_exec(move || {
            if libc::fchdir(root_raw) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = command.spawn()?;
    Ok(execute_supervisor(child, stdin, budget))
}

/// Poll native stderr and the nonblocking parent pipe in one loop. The child is
/// retained before any fallible pipe setup, and reaped before every return.
fn execute_supervisor(child: Child, mut stdin: impl Read, budget: Duration) -> SupervisorResult {
    let unknown = || SupervisorResult {
        outcome: "deliveryUnknown".to_owned(),
        removed_entry_count: None,
        removed_logical_bytes: None,
    };
    let mut child = OwnedChild(child);
    let Some(mut stderr_reader) = child.0.stderr.take().and_then(NonblockingPipe::new) else {
        return unknown();
    };
    let deadline = Instant::now() + budget;
    let mut captured = Vec::new();
    let mut parent_byte = [0_u8; 1];
    let status = loop {
        if stderr_reader.drain(&mut captured).is_err() {
            return unknown();
        }
        match child.0.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(_) => return unknown(),
        }
        match stdin.read(&mut parent_byte) {
            Ok(0) => return unknown(),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return unknown(),
        }
        if Instant::now() >= deadline {
            return unknown();
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    // The child's final write may race the preceding drain. A final over-cap
    // or unreadable result must not turn truncated output into success.
    if stderr_reader.drain(&mut captured).is_err() {
        return unknown();
    }
    let stderr_text = String::from_utf8_lossy(&captured);
    if !status.success() {
        // A pre-mutation lock refusal is the only Busy signal: the producer
        // warned it was in use, refused on the lock, and never printed the
        // mutation-start marker.
        if status.code() == Some(2)
            && stderr_text.contains(BUSY_PHASE_MARKER)
            && stderr_text.contains(BUSY_TIMEOUT_MARKER)
            && !stderr_text.contains(MUTATION_START_MARKER)
        {
            return SupervisorResult {
                outcome: "busy".to_owned(),
                removed_entry_count: None,
                removed_logical_bytes: None,
            };
        }
        // A native error may follow partial removal; only the exact lock
        // refusal above proves that mutation did not begin.
        return unknown();
    }
    match parse_human_summary(&stderr_text) {
        HumanSummary::Removed { count, bytes } => SupervisorResult {
            outcome: "completed".to_owned(),
            removed_entry_count: Some(count),
            removed_logical_bytes: Some(bytes),
        },
        // A recognized successful no-work run is Completed with zero counters.
        HumanSummary::NoWork => SupervisorResult {
            outcome: "completed".to_owned(),
            removed_entry_count: Some(0),
            removed_logical_bytes: Some(0),
        },
        // A successful run whose summary was not recognized: Completed with no
        // invented accounting.
        HumanSummary::Unrecognized => SupervisorResult {
            outcome: "completed".to_owned(),
            removed_entry_count: None,
            removed_logical_bytes: None,
        },
    }
}

/// A nonblocking reader around a pipe fd, draining into a bounded buffer.
struct NonblockingPipe {
    file: std::fs::File,
}

impl NonblockingPipe {
    fn new<T: std::os::fd::IntoRawFd>(pipe: T) -> Option<Self> {
        use std::os::fd::FromRawFd;
        let raw = pipe.into_raw_fd();
        // SAFETY: `raw` was just transferred to us by `into_raw_fd`, so we hold
        // the only owner of this fd.
        let file = unsafe { std::fs::File::from_raw_fd(raw) };
        set_fd_nonblocking(file.as_raw_fd()).ok()?;
        Some(Self { file })
    }

    /// Drain available bytes into `buffer`, capped at `STDERR_CAP_BYTES`.
    fn drain(&mut self, buffer: &mut Vec<u8>) -> io::Result<()> {
        use std::io::Read;
        let mut chunk = [0_u8; 4096];
        loop {
            if buffer.len() > STDERR_CAP_BYTES {
                return Err(io::Error::other("stderr over cap"));
            }
            match self.file.read(&mut chunk) {
                Ok(0) => return Ok(()),
                Ok(count) => buffer.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ProducerBinding(Vec<(u64, u64, u64, i64, i64)>);
impl ProducerBinding {
    /// The single identity tuple for one already-open producer object.
    fn of(meta: std::fs::Metadata) -> (u64, u64, u64, i64, i64) {
        (
            meta.dev(),
            meta.ino(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
        )
    }
    fn capture(binary: &Path) -> io::Result<Self> {
        let uid = unsafe { libc::geteuid() };
        let meta = std::fs::metadata(binary)?;
        if !meta.is_file() || ![0, uid].contains(&meta.uid()) || meta.mode() & 0o022 != 0 {
            return Err(io::Error::other("unsafe producer identity"));
        }
        Ok(Self(vec![Self::of(meta)]))
    }
}

fn directory_identity(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    (meta.is_dir() && !meta.file_type().is_symlink()).then_some((meta.dev(), meta.ino()))
}

/// Reject a cache root whose `.lock`, marker files, or fixed buckets are not
/// exactly the producer's expected plain in-cache shapes, and whose ancestors
/// include a symlink that could redirect the whole tree.
///
/// The producer holds `.lock` as an ordinary file and rewrites it in place; a
/// symlinked or hardlinked `.lock` would let the native run mutate an object
/// outside the cache. Internal `wheels-*`/`sdists-*` archive symlinks are the
/// producer's legitimate shape and are *not* rejected here; only bucket *roots*
/// must be real directories.
fn fixed_cache_shape_safe(root: &Path) -> bool {
    let uid = unsafe { libc::geteuid() };
    let Some(parent) = root.parent() else {
        return false;
    };
    // No ancestor from the root's parent up to the filesystem root may be a
    // symlink: a symlinked `~/.cache` would redirect the entire sweep.
    let mut ancestor = Some(parent);
    while let Some(path) = ancestor {
        match std::fs::symlink_metadata(path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return false;
                }
            }
            Err(_) => return false,
        }
        ancestor = path.parent();
    }
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
    // The producer's marker files and its exclusive lock must be plain,
    // single-link, owned regular files where present. uv-fs deliberately gives
    // its flock file mode 0666 (overriding umask); writable contents do not grant
    // authority to replace the inode inside a root writable only by its owner.
    // Keep the ordinary non-writable rule for the other marker files.
    for name in ["CACHEDIR.TAG", ".gitignore", ".lock"] {
        match std::fs::symlink_metadata(root.join(name)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(meta)
                if meta.is_file()
                    && !meta.file_type().is_symlink()
                    && meta.uid() == uid
                    && meta.nlink() == 1
                    && (name == ".lock" || meta.mode() & 0o022 == 0) => {}
            _ => return false,
        }
    }
    // Every fixed bucket that exists must be a real owned directory, never a
    // symlink: an escaped bucket root would let the native sweep delete outside
    // the cache.
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

/// The fixed cache bucket directories the native sweep may touch. A symlinked
/// or foreign-owned bucket root fails the preflight; a stale bucket name that is
/// *not* in this list is removed wholesale by the producer, which the containment
/// contract accounts for by rejecting symlinked root/ancestors and foreign
/// ownership of the root itself.
const SCANNED_BUCKETS: [&str; 12] = [
    "archive-v0",
    "environments-v2",
    "wheels-v6",
    "sdists-v9",
    "builds-v0",
    "git-v0",
    "interpreter-v4",
    "simple-v21",
    "flat-index-v2",
    "python-v0",
    "binaries-v0",
    "osv-v0",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CacheRootState {
    Present,
    Absent,
}

/// Reject a cache root that is a symlink, has a symlinked ancestor, is not owned
/// by the current user, or whose fixed buckets/`.lock` are unexpected shapes.
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
    Ok(CacheRootState::Present)
}

/// Locate a real uv binary under the fixed prefixes, using symlink resolution
/// against known shapes only, and require the exact reviewed version.
fn discover_producer(
    home: &Path,
    should_cancel: &impl Fn() -> bool,
) -> Result<(PathBuf, ProducerBinding), ToolCacheAvailability> {
    let mut prefixes: Vec<PathBuf> = FIXED_PREFIXES.iter().map(PathBuf::from).collect();
    let home_prefix = home.join(HOME_PREFIX_BIN);
    if home_prefix.is_dir() {
        prefixes.push(home_prefix);
    }

    let mut saw_any_producer = false;
    for prefix in prefixes {
        if should_cancel() {
            return Err(ToolCacheAvailability::Unavailable);
        }
        let candidate = prefix.join("uv");
        let Some(uv_real) = resolve_regular_file(&candidate) else {
            continue;
        };
        saw_any_producer = true;
        let Ok(binding) = ProducerBinding::capture(&uv_real) else {
            continue;
        };
        match producer_version(&uv_real, should_cancel) {
            Some(version)
                if version == EXPECTED_UV_VERSION
                    && ProducerBinding::capture(&uv_real).ok().as_ref() == Some(&binding) =>
            {
                return Ok((uv_real, binding));
            }
            Some(_) => continue,
            None => continue,
        }
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

/// Run `<uv> --version` for the discovered binary and return the exact version.
/// No network, no config, no mutation. Bounded: the child is spawned and reaped
/// with a deadline and a capped output read; on overrun or spawn failure the
/// child is killed and the version is `None`.
fn producer_version(binary: &Path, should_cancel: &impl Fn() -> bool) -> Option<String> {
    // Bind safety before executing even the read-only version command.
    ProducerBinding::capture(binary).ok()?;
    let mut child = OwnedChild(
        Command::new(binary)
            .arg("--version")
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?,
    );
    let stdout = child.0.stdout.take()?;
    let mut reader = NonblockingPipe::new(stdout)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut captured = Vec::new();
    loop {
        if reader.drain(&mut captured).is_err() {
            let _ = child.0.kill();
            let _ = child.0.wait();
            return None;
        }
        match child.0.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => {
                let _ = child.0.wait();
                return None;
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.0.kill();
                let _ = child.0.wait();
                return None;
            }
        }
        if should_cancel() || Instant::now() >= deadline || captured.len() > 4096 {
            let _ = child.0.kill();
            let _ = child.0.wait();
            return None;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    reader.drain(&mut captured).ok()?;
    if captured.len() > 4096 {
        return None;
    }
    let text = String::from_utf8(captured).ok()?;
    // `uv 0.11.20 (9252ba6b5 2026-06-10 aarch64-apple-darwin)`
    let mut words = text.split_whitespace();
    (words.next()? == "uv")
        .then(|| words.next().map(str::to_owned))
        .flatten()
}

#[cfg(test)]
#[path = "tool_cache_tests.rs"]
mod tests;

/// Test-only synchronous wrapper around [`supervise`] with an explicit budget, so
/// the fragment tests can drive the real supervisor routine against a real
/// parent pipe without needing the daemon binary to host the hidden entry.
#[cfg(test)]
pub(crate) fn supervise_sync_for_tests(
    stdin: impl Read,
    uv: &Path,
    cache: &Path,
) -> SupervisorResult {
    let root_fd = open_directory_no_follow(cache).expect("open test-owned cache root");
    supervise(stdin, uv, root_fd, DEFAULT_RUNTIME_BUDGET)
        .expect("supervisor must spawn the uv binary")
}
