//! Owner-approved Node compile-cache exception, pinned to Node v26.7.0.
//! Source authority: nodejs/node v26.7.0 src/compile_cache.{cc,h}.
//! Only the current user's Darwin default temp root and exact producer tag are
//! admitted. Node atomically publishes final files; temporary writers, unknown
//! contents and custom roots are protected. No recursive deletion occurs.

use crate::tool_cache::{ProducerBinding, read_producer_output};
use crate::{ControlPlane, PreparedToolCacheAttempt};
use serde::Deserialize;
use std::ffi::{CStr, CString};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use unlinger_protocol::{ToolCacheAvailability, ToolCacheKind, ToolCacheOutcome};

pub const SUPPORTED_NODE_VERSION: &str = "v26.7.0";
const ROOT_NAME: &CStr = c"node-compile-cache";
const MAGIC: u32 = 0x8adf_dbb2;
const MAX_ENTRIES: usize = 65_536;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SCAN_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const BUDGET: Duration = Duration::from_secs(120);
const NODE_INFO: &str = "console.log(JSON.stringify({version:process.version,arch:process.arch,tag:require('node:v8').cachedDataVersionTag().toString(16).padStart(8,'0'),uid:process.getuid()}))";

#[derive(Debug, Deserialize)]
struct ProducerInfo {
    version: String,
    arch: String,
    tag: String,
    uid: u32,
}

impl ProducerInfo {
    fn bucket(&self) -> Option<CString> {
        (self.version == SUPPORTED_NODE_VERSION
            && self.arch == std::env::consts::ARCH.replace("aarch64", "arm64")
            && hex_name(self.tag.as_bytes())
            && self.uid == unsafe { libc::geteuid() })
        .then(|| {
            CString::new(format!(
                "{}-{}-{}-{}",
                self.version, self.arch, self.tag, self.uid
            ))
            .ok()
        })
        .flatten()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Identity {
    device: u64,
    inode: u64,
    length: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Identity {
    fn of(meta: &Metadata) -> Self {
        Self {
            device: meta.dev(),
            inode: meta.ino(),
            length: meta.len(),
            modified: (meta.mtime(), meta.mtime_nsec()),
            changed: (meta.ctime(), meta.ctime_nsec()),
        }
    }
    fn same_object(&self, meta: &Metadata) -> bool {
        (self.device, self.inode) == (meta.dev(), meta.ino())
    }
}

struct Entry {
    name: CString,
    identity: Identity,
}

pub struct NodeCompileCacheMaintenance {
    producer: PathBuf,
    binding: ProducerBinding,
    temp: File,
    temp_path: PathBuf,
    temp_identity: Identity,
    root: File,
    bucket: File,
    bucket_name: CString,
    root_identity: Identity,
    bucket_identity: Identity,
    entries: Vec<Entry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeCompileCacheResult {
    pub outcome: ToolCacheOutcome,
    pub removed_entry_count: Option<u64>,
    pub removed_logical_bytes: Option<u64>,
}

impl NodeCompileCacheResult {
    fn unsuccessful(outcome: ToolCacheOutcome) -> Self {
        Self {
            outcome,
            removed_entry_count: None,
            removed_logical_bytes: None,
        }
    }
}

impl NodeCompileCacheMaintenance {
    pub fn discover(cancelled: impl Fn() -> bool) -> Result<Self, ToolCacheAvailability> {
        let (producer, binding, info) = discover_producer(&cancelled)?;
        let bucket = info.bucket().ok_or(ToolCacheAvailability::Unsupported)?;
        let temp = default_temp_root().map_err(|_| ToolCacheAvailability::Unavailable)?;
        Self::open_at(&temp, producer, binding, bucket, &cancelled)
    }

    fn open_at(
        temp_path: &Path,
        producer: PathBuf,
        binding: ProducerBinding,
        bucket_name: CString,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, ToolCacheAvailability> {
        let deadline = Instant::now() + BUDGET;
        let temp = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | no_follow_any() | libc::O_CLOEXEC)
            .open(temp_path)
            .map_err(|_| ToolCacheAvailability::Unavailable)?;
        let temp_meta = temp
            .metadata()
            .map_err(|_| ToolCacheAvailability::Unavailable)?;
        if !safe_directory(&temp_meta) {
            return Err(ToolCacheAvailability::Unavailable);
        }
        let root = open_at(&temp, ROOT_NAME, true).map_err(availability_for_open)?;
        let root_meta = root
            .metadata()
            .map_err(|_| ToolCacheAvailability::Unavailable)?;
        if !safe_directory(&root_meta) {
            return Err(ToolCacheAvailability::Unavailable);
        }
        let bucket = open_at(&root, &bucket_name, true).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ToolCacheAvailability::Unsupported
            } else {
                ToolCacheAvailability::Unavailable
            }
        })?;
        let bucket_meta = bucket
            .metadata()
            .map_err(|_| ToolCacheAvailability::Unavailable)?;
        if !safe_directory(&bucket_meta) {
            return Err(ToolCacheAvailability::Unavailable);
        }
        let entries = scan_entries(&bucket, deadline, cancelled)
            .map_err(|_| ToolCacheAvailability::Unavailable)?;
        let value = Self {
            producer,
            binding,
            temp,
            root,
            bucket,
            bucket_name,
            temp_path: temp_path.to_owned(),
            temp_identity: Identity::of(&temp_meta),
            root_identity: Identity::of(&root_meta),
            bucket_identity: Identity::of(&bucket_meta),
            entries,
        };
        if !value.bindings_current() {
            return Err(ToolCacheAvailability::Unavailable);
        }
        Ok(value)
    }

    fn bindings_current(&self) -> bool {
        ProducerBinding::capture(&self.producer).ok().as_ref() == Some(&self.binding)
            && OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | no_follow_any() | libc::O_CLOEXEC)
                .open(&self.temp_path)
                .and_then(|file| file.metadata())
                .is_ok_and(|meta| safe_directory(&meta) && self.temp_identity.same_object(&meta))
            && entry_metadata(&self.temp, ROOT_NAME)
                .is_ok_and(|meta| safe_directory(&meta) && self.root_identity.same_object(&meta))
            && entry_metadata(&self.root, &self.bucket_name)
                .is_ok_and(|meta| safe_directory(&meta) && self.bucket_identity.same_object(&meta))
    }

    /// The opaque PREPARED handle and current exact lifecycle lease are required
    /// even on this direct in-process path. Work runs outside the lifecycle lock.
    pub fn execute(
        &self,
        control: &ControlPlane,
        prepared: &PreparedToolCacheAttempt,
        epoch: &str,
        stopping: impl Fn() -> bool,
    ) -> NodeCompileCacheResult {
        let cancelled = || {
            stopping()
                || prepared.kind() != ToolCacheKind::NodeCompileCache
                || !control.tool_cache_action_may_continue(prepared, epoch)
        };
        self.remove_files(&cancelled)
    }

    fn remove_files(&self, cancelled: &impl Fn() -> bool) -> NodeCompileCacheResult {
        let deadline = Instant::now() + BUDGET;
        if cancelled() || !self.bindings_current() {
            return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Failed);
        }
        // A second full shape check rejects an already-active writer or any
        // foreign content before the first effect. Only this frozen set is used.
        let current = match scan_entries(&self.bucket, deadline, cancelled) {
            Ok(entries) => entries,
            Err(ScanError::Busy) => {
                return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Busy);
            }
            Err(_) => return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Failed),
        };
        if current.len() != self.entries.len()
            || current
                .iter()
                .zip(&self.entries)
                .any(|(a, b)| a.name != b.name || a.identity != b.identity)
        {
            return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Busy);
        }
        let mut count = 0;
        let mut bytes = 0;
        for entry in &self.entries {
            if cancelled() || Instant::now() >= deadline || !self.bindings_current() {
                return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Failed);
            }
            let file = match open_at(&self.bucket, &entry.name, false) {
                Ok(file) => file,
                Err(_) => return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Failed),
            };
            let Ok(meta) = file.metadata() else {
                return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Failed);
            };
            if !safe_file(&meta)
                || Identity::of(&meta) != entry.identity
                || !entry_metadata(&self.bucket, &entry.name)
                    .is_ok_and(|meta| safe_file(&meta) && Identity::of(&meta) == entry.identity)
                || cancelled()
            {
                return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::Failed);
            }
            // Node writes a different temporary inode and atomically renames.
            // Changed identities fail the plan. This no-follow unlink admits
            // normal producer concurrency, not hostile same-UID path swapping.
            if unsafe { libc::unlinkat(self.bucket.as_raw_fd(), entry.name.as_ptr(), 0) } != 0 {
                return NodeCompileCacheResult::unsuccessful(ToolCacheOutcome::DeliveryUnknown);
            }
            count += 1;
            bytes += entry.identity.length;
        }
        NodeCompileCacheResult {
            outcome: ToolCacheOutcome::Completed,
            removed_entry_count: Some(count),
            removed_logical_bytes: Some(bytes),
        }
    }

    pub fn observe(&self, cancelled: impl Fn() -> bool) -> ToolCacheAvailability {
        if !self.bindings_current() {
            return ToolCacheAvailability::Unavailable;
        }
        if scan_entries(&self.bucket, Instant::now() + BUDGET, &cancelled).is_ok() {
            ToolCacheAvailability::Available
        } else {
            ToolCacheAvailability::Unavailable
        }
    }
}

#[derive(Debug)]
enum ScanError {
    Busy,
    Unsafe,
    Io,
}
impl From<io::Error> for ScanError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}

fn scan_entries(
    bucket: &File,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<Vec<Entry>, ScanError> {
    let mut names = directory_names(bucket)?;
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let mut result = Vec::with_capacity(names.len());
    let mut total = 0u64;
    for name in names {
        if cancelled() || Instant::now() >= deadline {
            return Err(ScanError::Unsafe);
        }
        if !hex_name(name.as_bytes()) {
            let value = name.as_bytes();
            if value.len() == 15
                && hex_name(&value[..8])
                && value[8] == b'.'
                && value[9..].iter().all(u8::is_ascii_alphanumeric)
            {
                return Err(ScanError::Busy);
            }
            return Err(ScanError::Unsafe);
        }
        let mut file = open_at(bucket, &name, false)?;
        let meta = file.metadata()?;
        if !safe_file(&meta) || meta.len() > MAX_FILE_BYTES || meta.len() < 20 {
            return Err(ScanError::Unsafe);
        }
        total = total.checked_add(meta.len()).ok_or(ScanError::Unsafe)?;
        if total > MAX_SCAN_BYTES {
            return Err(ScanError::Unsafe);
        }
        validate_cache_file(&mut file, meta.len(), deadline, cancelled)?;
        if Identity::of(&meta) != Identity::of(&file.metadata()?)
            || !entry_metadata(bucket, &name).is_ok_and(|current| {
                safe_file(&current) && Identity::of(&current) == Identity::of(&meta)
            })
        {
            return Err(ScanError::Busy);
        }
        result.push(Entry {
            name,
            identity: Identity::of(&meta),
        });
    }
    Ok(result)
}

fn validate_cache_file(
    file: &mut File,
    length: u64,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<(), ScanError> {
    let mut header = [0u8; 20];
    file.read_exact(&mut header)?;
    let word = |index: usize| {
        u32::from_le_bytes(
            header[index * 4..index * 4 + 4]
                .try_into()
                .expect("header word"),
        )
    };
    // Use the actual indexed headers in .h/.cc, not the stale layout comment.
    if word(0) != MAGIC || u64::from(word(2)) + 20 != length {
        return Err(ScanError::Unsafe);
    }
    let mut crc = !0u32;
    let mut buffer = [0u8; 65_536];
    loop {
        if cancelled() || Instant::now() >= deadline {
            return Err(ScanError::Unsafe);
        }
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        crc = update_crc(crc, &buffer[..size]);
    }
    if !crc != word(4) {
        return Err(ScanError::Unsafe);
    }
    Ok(())
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}
const CRC: [u32; 256] = crc_table();
fn update_crc(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc = CRC[((crc ^ u32::from(*byte)) & 255) as usize] ^ (crc >> 8);
    }
    crc
}
fn hex_name(value: &[u8]) -> bool {
    value.len() == 8
        && value
            .iter()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(value))
}
fn safe_directory(meta: &Metadata) -> bool {
    meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o022 == 0
}
fn safe_file(meta: &Metadata) -> bool {
    meta.is_file()
        && meta.uid() == unsafe { libc::geteuid() }
        && meta.nlink() == 1
        && meta.mode() & 0o022 == 0
}
fn availability_for_open(error: io::Error) -> ToolCacheAvailability {
    if error.kind() == io::ErrorKind::NotFound {
        ToolCacheAvailability::Absent
    } else {
        ToolCacheAvailability::Unavailable
    }
}

fn open_at(parent: &File, name: &CStr, directory: bool) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_CLOEXEC
                | if directory { libc::O_DIRECTORY } else { 0 },
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn entry_metadata(parent: &File, name: &CStr) -> io::Result<Metadata> {
    // Open with no-follow/nonblocking: FIFO/device/symlink entries cannot block
    // or acquire authority. Type is validated before reading or deleting.
    open_at(parent, name, false)?.metadata()
}

fn directory_names(directory: &File) -> io::Result<Vec<CString>> {
    struct Stream(*mut libc::DIR);
    impl Drop for Stream {
        fn drop(&mut self) {
            unsafe { libc::closedir(self.0) };
        }
    }
    let fd = directory.try_clone()?.into_raw_fd();
    if unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } < 0 {
        let error = io::Error::last_os_error();
        drop(unsafe { File::from_raw_fd(fd) });
        return Err(error);
    }
    let pointer = unsafe { libc::fdopendir(fd) };
    if pointer.is_null() {
        let error = io::Error::last_os_error();
        drop(unsafe { File::from_raw_fd(fd) });
        return Err(error);
    }
    let stream = Stream(pointer);
    let mut names = Vec::new();
    loop {
        #[cfg(target_os = "macos")]
        unsafe {
            *libc::__error() = 0
        };
        #[cfg(target_os = "linux")]
        unsafe {
            *libc::__errno_location() = 0
        };
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(0) {
                return Ok(names);
            }
            return Err(error);
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name != c"." && name != c".." {
            if names.len() >= MAX_ENTRIES {
                return Err(io::Error::other("node cache entry budget exceeded"));
            }
            names.push(name.to_owned());
        }
    }
}

#[cfg(target_os = "macos")]
fn no_follow_any() -> i32 {
    libc::O_NOFOLLOW_ANY
}
#[cfg(not(target_os = "macos"))]
fn no_follow_any() -> i32 {
    libc::O_NOFOLLOW
}

#[cfg(target_os = "macos")]
fn default_temp_root() -> io::Result<PathBuf> {
    let needed = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null_mut(), 0) };
    if needed == 0 || needed > 4096 {
        return Err(io::Error::other("Darwin temp root unavailable"));
    }
    let mut value = vec![0u8; needed];
    if unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            value.as_mut_ptr().cast(),
            value.len(),
        )
    } != needed
    {
        return Err(io::Error::other("Darwin temp root changed"));
    }
    let path = CStr::from_bytes_with_nul(&value)
        .map_err(|_| io::Error::other("invalid Darwin temp root"))?;
    use std::os::unix::ffi::OsStrExt;
    std::fs::canonicalize(Path::new(std::ffi::OsStr::from_bytes(path.to_bytes())))
}
#[cfg(not(target_os = "macos"))]
fn default_temp_root() -> io::Result<PathBuf> {
    Err(io::Error::other("Node maintenance is macOS-only"))
}

fn discover_producer(
    cancelled: &impl Fn() -> bool,
) -> Result<(PathBuf, ProducerBinding, ProducerInfo), ToolCacheAvailability> {
    let mut paths = vec![
        PathBuf::from("/opt/homebrew/bin/node"),
        PathBuf::from("/usr/local/bin/node"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".local/bin/node"));
    }
    let mut present = false;
    for path in paths {
        if cancelled() {
            return Err(ToolCacheAvailability::Unavailable);
        }
        let Ok(path) = std::fs::canonicalize(path) else {
            continue;
        };
        present = true;
        let Ok(binding) = ProducerBinding::capture(&path) else {
            continue;
        };
        let Some(output) = read_producer_output(&path, &["--eval", NODE_INFO], cancelled) else {
            continue;
        };
        let Ok(info) = serde_json::from_str::<ProducerInfo>(&output) else {
            continue;
        };
        if info.bucket().is_some()
            && ProducerBinding::capture(&path).ok().as_ref() == Some(&binding)
        {
            return Ok((path, binding, info));
        }
    }
    Err(if present {
        ToolCacheAvailability::Unsupported
    } else {
        ToolCacheAvailability::Absent
    })
}

#[cfg(test)]
#[path = "node_compile_cache_tests.rs"]
mod tests;
