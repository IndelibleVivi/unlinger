use crate::parse_procargs2;
use libproc::libproc::proc_pid::pidinfo;
use libproc::libproc::task_info::TaskAllInfo;
use libproc::processes::{ProcFilter, pids_by_type};
use std::ffi::{CStr, CString};
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::mem::{size_of, zeroed};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use unlinger_core::{
    ArtifactDisposition, ArtifactFreeze, FrozenRuntimeArtifact, RuntimeArtifactCandidate,
    RuntimeArtifactIdentity, RuntimeArtifactKind, RuntimeFailure,
};

const CTL_KERN: libc::c_int = 1;
const KERN_ARGMAX: libc::c_int = 8;
const KERN_PROCARGS2: libc::c_int = 49;
const PROC_ALL_PIDS: u32 = 1;
const QUARANTINE_PREFIX: &str = ".unlinger-artifact-quarantine-";
const QUARANTINE_ATTEMPTS: usize = 4;
const ACL_TYPE_EXTENDED: libc::c_int = 0x0000_0100;
const ACL_FIRST_ENTRY: libc::c_int = 0;

unsafe extern "C" {
    fn acl_get_fd_np(fd: libc::c_int, acl_type: libc::c_int) -> *mut libc::c_void;
    fn acl_get_entry(
        acl: *mut libc::c_void,
        entry_id: libc::c_int,
        entry: *mut *mut libc::c_void,
    ) -> libc::c_int;
    fn acl_free(object: *mut libc::c_void) -> libc::c_int;
}

#[link(name = "proc")]
unsafe extern "C" {
    fn proc_listpidspath(
        process_type: u32,
        process_type_info: u32,
        path: *const libc::c_char,
        path_flags: u32,
        buffer: *mut libc::c_void,
        buffer_size: libc::c_int,
    ) -> libc::c_int;
}

pub(super) fn freeze(
    candidate: &RuntimeArtifactCandidate,
) -> Result<ArtifactFreeze, RuntimeFailure> {
    if candidate.kind() != RuntimeArtifactKind::DevToolsActivePort
        || candidate.owner_uid() == 0
        || candidate.owner_uid() != current_uid()
    {
        return Ok(ArtifactFreeze::Unsafe);
    }
    let parent = match open_safe_parent(candidate.profile_path(), candidate.owner_uid()) {
        Ok(parent) => parent,
        Err(OpenFailure::Unsafe | OpenFailure::Absent) => return Ok(ArtifactFreeze::Unsafe),
        Err(OpenFailure::Runtime(error)) => return Err(error),
    };
    let parent_metadata = parent.metadata().map_err(|error| {
        RuntimeFailure::new(format!("artifact parent metadata failed: {error}"))
    })?;
    let file = match open_candidate(parent.as_raw_fd()) {
        Ok(file) => file,
        Err(OpenFailure::Absent) => return Ok(ArtifactFreeze::Absent),
        Err(OpenFailure::Unsafe) => return Ok(ArtifactFreeze::Unsafe),
        Err(OpenFailure::Runtime(error)) => return Err(error),
    };
    let metadata = file
        .metadata()
        .map_err(|error| RuntimeFailure::new(format!("artifact metadata failed: {error}")))?;
    if !safe_candidate_metadata(&metadata, candidate.owner_uid(), parent_metadata.dev()) {
        return Ok(ArtifactFreeze::Unsafe);
    }
    Ok(ArtifactFreeze::Frozen(FrozenRuntimeArtifact::new(
        candidate.clone(),
        RuntimeArtifactIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            owner_uid: metadata.uid(),
            mode: metadata.mode(),
            link_count: metadata.nlink(),
            parent_device: parent_metadata.dev(),
            parent_inode: parent_metadata.ino(),
            parent_owner_uid: parent_metadata.uid(),
            parent_mode: parent_metadata.mode(),
        },
    )))
}

pub(super) fn remove_exact(frozen: &FrozenRuntimeArtifact) -> ArtifactDisposition {
    remove_exact_with(frozen, has_live_reference)
}

fn remove_exact_with(
    frozen: &FrozenRuntimeArtifact,
    reference_scan: impl FnMut(
        &RuntimeArtifactCandidate,
        &RuntimeArtifactIdentity,
        &Path,
    ) -> Result<bool, ()>,
) -> ArtifactDisposition {
    remove_exact_with_hooks(frozen, reference_scan, || {}, |_| {})
}

fn remove_exact_with_hooks(
    frozen: &FrozenRuntimeArtifact,
    mut reference_scan: impl FnMut(
        &RuntimeArtifactCandidate,
        &RuntimeArtifactIdentity,
        &Path,
    ) -> Result<bool, ()>,
    before_quarantine: impl FnOnce(),
    after_quarantine: impl FnOnce(&CStr),
) -> ArtifactDisposition {
    let candidate = frozen.candidate();
    let expected = frozen.identity();
    if candidate.kind() != RuntimeArtifactKind::DevToolsActivePort
        || candidate.owner_uid() == 0
        || candidate.owner_uid() != current_uid()
        || expected.owner_uid != current_uid()
    {
        return ArtifactDisposition::Unsafe;
    }
    let parent = match open_safe_parent(candidate.profile_path(), candidate.owner_uid()) {
        Ok(parent) => parent,
        Err(OpenFailure::Absent | OpenFailure::Unsafe | OpenFailure::Runtime(_)) => {
            return ArtifactDisposition::Unsafe;
        }
    };
    let Ok(parent_metadata) = parent.metadata() else {
        return ArtifactDisposition::Rejected;
    };
    if parent_metadata.dev() != expected.parent_device
        || parent_metadata.ino() != expected.parent_inode
        || parent_metadata.uid() != expected.parent_owner_uid
        || parent_metadata.mode() != expected.parent_mode
    {
        return ArtifactDisposition::IdentityMismatch;
    }
    let first = match open_candidate(parent.as_raw_fd()) {
        Ok(file) => file,
        Err(OpenFailure::Absent) => return ArtifactDisposition::AlreadyAbsent,
        Err(OpenFailure::Unsafe) => return ArtifactDisposition::Unsafe,
        Err(OpenFailure::Runtime(_)) => return ArtifactDisposition::Rejected,
    };
    let Ok(first_metadata) = first.metadata() else {
        return ArtifactDisposition::Rejected;
    };
    if !same_candidate_identity(&first_metadata, expected) {
        return ArtifactDisposition::IdentityMismatch;
    }
    drop(first);

    match reference_scan(candidate, expected, candidate.artifact_path()) {
        Ok(true) => return ArtifactDisposition::Referenced,
        Ok(false) => {}
        Err(()) => return ArtifactDisposition::Unsafe,
    }

    let second = match open_candidate(parent.as_raw_fd()) {
        Ok(file) => file,
        Err(OpenFailure::Absent) => return ArtifactDisposition::AlreadyAbsent,
        Err(OpenFailure::Unsafe) => return ArtifactDisposition::Unsafe,
        Err(OpenFailure::Runtime(_)) => return ArtifactDisposition::Rejected,
    };
    let Ok(second_metadata) = second.metadata() else {
        return ArtifactDisposition::Rejected;
    };
    if !same_candidate_identity(&second_metadata, expected) {
        return ArtifactDisposition::IdentityMismatch;
    }
    if !fstatat_matches(parent.as_raw_fd(), expected) {
        return ArtifactDisposition::IdentityMismatch;
    }

    before_quarantine();
    let quarantine_name = match quarantine_candidate(parent.as_raw_fd()) {
        QuarantineResult::Moved(name) => name,
        QuarantineResult::Absent => return ArtifactDisposition::AlreadyAbsent,
        QuarantineResult::Unsafe => return ArtifactDisposition::Unsafe,
        QuarantineResult::Rejected => return ArtifactDisposition::Rejected,
    };
    after_quarantine(&quarantine_name);

    let quarantined = match open_named_candidate(parent.as_raw_fd(), &quarantine_name) {
        Ok(file) => file,
        Err(OpenFailure::Absent) => {
            drop(second);
            return fail_after_restoring_quarantined_entry(
                parent.as_raw_fd(),
                &quarantine_name,
                ArtifactDisposition::IdentityMismatch,
            );
        }
        Err(OpenFailure::Unsafe) => {
            drop(second);
            return fail_after_restoring_quarantined_entry(
                parent.as_raw_fd(),
                &quarantine_name,
                ArtifactDisposition::Unsafe,
            );
        }
        Err(OpenFailure::Runtime(_)) => {
            drop(second);
            return fail_after_restoring_quarantined_entry(
                parent.as_raw_fd(),
                &quarantine_name,
                ArtifactDisposition::Rejected,
            );
        }
    };
    let quarantined_metadata = match quarantined.metadata() {
        Ok(metadata) => metadata,
        Err(_) => {
            drop(quarantined);
            drop(second);
            return fail_after_restoring_quarantined_entry(
                parent.as_raw_fd(),
                &quarantine_name,
                ArtifactDisposition::Rejected,
            );
        }
    };
    if !same_candidate_identity(&quarantined_metadata, expected)
        || !fstatat_named_matches(parent.as_raw_fd(), &quarantine_name, expected)
    {
        drop(quarantined);
        drop(second);
        return fail_after_restoring_quarantined_entry(
            parent.as_raw_fd(),
            &quarantine_name,
            ArtifactDisposition::IdentityMismatch,
        );
    }

    // Both descriptors point at the artifact itself. Close them before the
    // second kernel path-reference query so Unlinger cannot observe its own
    // safety descriptors as live references.
    drop(quarantined);
    drop(second);

    let Some(quarantine_component) = quarantine_name.to_str().ok() else {
        return match restore_quarantined_exact(parent.as_raw_fd(), &quarantine_name, expected) {
            Ok(()) => ArtifactDisposition::Unsafe,
            Err(disposition) => disposition,
        };
    };
    let quarantine_path = candidate.profile_path().join(quarantine_component);
    match reference_scan(candidate, expected, &quarantine_path) {
        Ok(true) => {
            return match restore_quarantined_exact(parent.as_raw_fd(), &quarantine_name, expected) {
                Ok(()) => ArtifactDisposition::Referenced,
                Err(disposition) => disposition,
            };
        }
        Ok(false) => {}
        Err(()) => {
            return match restore_quarantined_exact(parent.as_raw_fd(), &quarantine_name, expected) {
                Ok(()) => ArtifactDisposition::Unsafe,
                Err(disposition) => disposition,
            };
        }
    }

    // Reopen and revalidate the exact quarantined inode immediately before
    // unlink. A newly-created canonical entry is never overwritten or removed;
    // both entries remain available for inspection and this attempt fails.
    let quarantined = match open_named_candidate(parent.as_raw_fd(), &quarantine_name) {
        Ok(file) => file,
        Err(OpenFailure::Unsafe) => return ArtifactDisposition::Unsafe,
        Err(OpenFailure::Absent) => return ArtifactDisposition::IdentityMismatch,
        Err(OpenFailure::Runtime(_)) => return ArtifactDisposition::Rejected,
    };
    let Ok(quarantined_metadata) = quarantined.metadata() else {
        return ArtifactDisposition::Rejected;
    };
    if !same_candidate_identity(&quarantined_metadata, expected)
        || !fstatat_named_matches(parent.as_raw_fd(), &quarantine_name, expected)
    {
        return ArtifactDisposition::IdentityMismatch;
    }
    match canonical_entry_absent(parent.as_raw_fd()) {
        Ok(true) => {}
        Ok(false) => return ArtifactDisposition::IdentityMismatch,
        Err(disposition) => return disposition,
    }

    let result = unsafe { libc::unlinkat(parent.as_raw_fd(), quarantine_name.as_ptr(), 0) };
    drop(quarantined);
    if result == 0 {
        ArtifactDisposition::Removed
    } else {
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) => ArtifactDisposition::Rejected,
            Some(libc::EISDIR | libc::EPERM | libc::ELOOP) => ArtifactDisposition::Unsafe,
            _ => ArtifactDisposition::Rejected,
        }
    }
}

#[derive(Debug)]
enum OpenFailure {
    Absent,
    Unsafe,
    Runtime(RuntimeFailure),
}

fn open_safe_parent(path: &Path, owner_uid: u32) -> Result<File, OpenFailure> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC)
        .open(path)
        .map_err(map_open_error)?;
    let metadata = file.metadata().map_err(|error| {
        OpenFailure::Runtime(RuntimeFailure::new(format!(
            "artifact parent metadata failed: {error}"
        )))
    })?;
    let is_directory = metadata.mode() & u32::from(libc::S_IFMT) == u32::from(libc::S_IFDIR);
    let owner_private = metadata.mode() & 0o077 == 0;
    if !is_directory || metadata.uid() != owner_uid || owner_uid != current_uid() || !owner_private
    {
        return Err(OpenFailure::Unsafe);
    }
    if !fd_has_empty_extended_acl(file.as_raw_fd())? {
        return Err(OpenFailure::Unsafe);
    }
    Ok(file)
}

fn open_candidate(parent_fd: libc::c_int) -> Result<File, OpenFailure> {
    let name = devtools_active_port_name();
    open_named_candidate(parent_fd, &name)
}

fn open_named_candidate(parent_fd: libc::c_int, name: &CStr) -> Result<File, OpenFailure> {
    let fd = unsafe {
        libc::openat(
            parent_fd,
            name.as_ptr(),
            libc::O_EVTONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(map_open_error(std::io::Error::last_os_error()));
    }
    let file = unsafe { File::from_raw_fd(fd) };
    if !fd_has_empty_extended_acl(file.as_raw_fd())? {
        return Err(OpenFailure::Unsafe);
    }
    Ok(file)
}

fn fd_has_empty_extended_acl(fd: libc::c_int) -> Result<bool, OpenFailure> {
    let acl = unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
            // Darwin reports a missing extended ACL as ENOENT rather than an
            // allocated zero-entry ACL.
            return Ok(true);
        }
        return Err(OpenFailure::Unsafe);
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &raw mut entry) };
    let free_result = unsafe { acl_free(acl) };
    if result != 0 || entry.is_null() || free_result != 0 {
        return Err(OpenFailure::Unsafe);
    }
    // Darwin returns 0 when ACL_FIRST_ENTRY resolves an entry. A missing ACL
    // was already handled as ENOENT above, so any allocated ACL is non-empty.
    Ok(false)
}

fn map_open_error(error: std::io::Error) -> OpenFailure {
    match error.raw_os_error() {
        Some(libc::ENOENT) => OpenFailure::Absent,
        Some(libc::ELOOP | libc::ENOTDIR | libc::EACCES | libc::EPERM) => OpenFailure::Unsafe,
        _ => OpenFailure::Runtime(RuntimeFailure::new(format!(
            "runtime artifact open failed: {error}"
        ))),
    }
}

fn devtools_active_port_name() -> CString {
    CString::new("DevToolsActivePort").expect("constant filename contains no NUL")
}

fn safe_candidate_metadata(
    metadata: &std::fs::Metadata,
    owner_uid: u32,
    parent_device: u64,
) -> bool {
    // Chrome for Testing creates DevToolsActivePort as 0644. The containing
    // profile is already required to be a current-owner 0700 directory with
    // no extended ACL, so read bits on the child do not make it path-visible
    // to another user. Reject every group/other write bit: the exact owner and
    // private parent must remain the only pathname mutation authority.
    let group_or_other_writable = metadata.mode() & 0o022 != 0;
    metadata.mode() & u32::from(libc::S_IFMT) == u32::from(libc::S_IFREG)
        && metadata.uid() == owner_uid
        && owner_uid == current_uid()
        && !group_or_other_writable
        && metadata.nlink() == 1
        && metadata.dev() == parent_device
}

fn same_candidate_identity(
    metadata: &std::fs::Metadata,
    expected: &RuntimeArtifactIdentity,
) -> bool {
    safe_candidate_metadata(metadata, expected.owner_uid, expected.parent_device)
        && metadata.dev() == expected.device
        && metadata.ino() == expected.inode
        && metadata.uid() == expected.owner_uid
        && metadata.mode() == expected.mode
        && metadata.nlink() == expected.link_count
}

fn fstatat_matches(parent_fd: libc::c_int, expected: &RuntimeArtifactIdentity) -> bool {
    let name = devtools_active_port_name();
    fstatat_named_matches(parent_fd, &name, expected)
}

fn fstatat_named_matches(
    parent_fd: libc::c_int,
    name: &CStr,
    expected: &RuntimeArtifactIdentity,
) -> bool {
    let mut stat = unsafe { zeroed::<libc::stat>() };
    if unsafe {
        libc::fstatat(
            parent_fd,
            name.as_ptr(),
            &raw mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return false;
    }
    u64::try_from(stat.st_dev).ok() == Some(expected.device)
        && stat.st_ino == expected.inode
        && stat.st_uid == expected.owner_uid
        && u32::from(stat.st_mode) == expected.mode
        && u64::from(stat.st_nlink) == expected.link_count
        && stat.st_mode & libc::S_IFMT == libc::S_IFREG
}

enum QuarantineResult {
    Moved(CString),
    Absent,
    Unsafe,
    Rejected,
}

fn quarantine_candidate(parent_fd: libc::c_int) -> QuarantineResult {
    let source = devtools_active_port_name();
    for _ in 0..QUARANTINE_ATTEMPTS {
        let Ok(quarantine) = random_quarantine_name() else {
            return QuarantineResult::Rejected;
        };
        if unsafe {
            libc::renameatx_np(
                parent_fd,
                source.as_ptr(),
                parent_fd,
                quarantine.as_ptr(),
                libc::RENAME_EXCL,
            )
        } == 0
        {
            return QuarantineResult::Moved(quarantine);
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST) => continue,
            Some(libc::ENOENT) => return QuarantineResult::Absent,
            Some(libc::EISDIR | libc::EPERM | libc::ELOOP | libc::ENOTDIR) => {
                return QuarantineResult::Unsafe;
            }
            _ => return QuarantineResult::Rejected,
        }
    }
    QuarantineResult::Rejected
}

fn random_quarantine_name() -> Result<CString, ()> {
    let mut random = [0_u8; 16];
    if unsafe { libc::getentropy(random.as_mut_ptr().cast(), random.len()) } != 0 {
        return Err(());
    }
    let mut name = String::with_capacity(QUARANTINE_PREFIX.len() + random.len() * 2);
    name.push_str(QUARANTINE_PREFIX);
    for byte in random {
        write!(&mut name, "{byte:02x}").map_err(|_| ())?;
    }
    CString::new(name).map_err(|_| ())
}

fn restore_quarantined_exact(
    parent_fd: libc::c_int,
    quarantine: &CStr,
    expected: &RuntimeArtifactIdentity,
) -> Result<(), ArtifactDisposition> {
    let quarantined = match open_named_candidate(parent_fd, quarantine) {
        Ok(file) => file,
        Err(OpenFailure::Absent) => return Err(ArtifactDisposition::IdentityMismatch),
        Err(OpenFailure::Unsafe) => return Err(ArtifactDisposition::Unsafe),
        Err(OpenFailure::Runtime(_)) => return Err(ArtifactDisposition::Rejected),
    };
    let metadata = quarantined
        .metadata()
        .map_err(|_| ArtifactDisposition::Rejected)?;
    if !same_candidate_identity(&metadata, expected)
        || !fstatat_named_matches(parent_fd, quarantine, expected)
    {
        return Err(ArtifactDisposition::IdentityMismatch);
    }

    restore_quarantined_entry(parent_fd, quarantine)?;

    let metadata = quarantined
        .metadata()
        .map_err(|_| ArtifactDisposition::Rejected)?;
    if !same_candidate_identity(&metadata, expected) || !fstatat_matches(parent_fd, expected) {
        return Err(ArtifactDisposition::IdentityMismatch);
    }
    Ok(())
}

fn fail_after_restoring_quarantined_entry(
    parent_fd: libc::c_int,
    quarantine: &CStr,
    failure: ArtifactDisposition,
) -> ArtifactDisposition {
    match restore_quarantined_entry(parent_fd, quarantine) {
        Ok(()) => failure,
        Err(disposition) => disposition,
    }
}

fn restore_quarantined_entry(
    parent_fd: libc::c_int,
    quarantine: &CStr,
) -> Result<(), ArtifactDisposition> {
    let source = devtools_active_port_name();
    // RENAME_EXCL is the atomic absence check: it never overwrites a
    // concurrently-created canonical entry. On conflict both entries remain.
    if unsafe {
        libc::renameatx_np(
            parent_fd,
            quarantine.as_ptr(),
            parent_fd,
            source.as_ptr(),
            libc::RENAME_EXCL,
        )
    } == 0
    {
        return Ok(());
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EEXIST | libc::ENOENT) => Err(ArtifactDisposition::IdentityMismatch),
        Some(libc::EISDIR | libc::EPERM | libc::ELOOP | libc::ENOTDIR) => {
            Err(ArtifactDisposition::Unsafe)
        }
        _ => Err(ArtifactDisposition::Rejected),
    }
}

fn canonical_entry_absent(parent_fd: libc::c_int) -> Result<bool, ArtifactDisposition> {
    let source = devtools_active_port_name();
    let mut stat = unsafe { zeroed::<libc::stat>() };
    if unsafe {
        libc::fstatat(
            parent_fd,
            source.as_ptr(),
            &raw mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Ok(false);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ENOENT) => Ok(true),
        Some(libc::EACCES | libc::EPERM | libc::ELOOP | libc::ENOTDIR) => {
            Err(ArtifactDisposition::Unsafe)
        }
        _ => Err(ArtifactDisposition::Rejected),
    }
}

fn has_live_reference(
    candidate: &RuntimeArtifactCandidate,
    expected: &RuntimeArtifactIdentity,
    reference_path: &Path,
) -> Result<bool, ()> {
    scan_live_references(candidate, expected, reference_path).map_err(|_| ())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceScanFailure {
    OpenReferenceLookup,
    ReferencePathIdentity,
    ProcessMetadata,
    Arguments,
}

fn scan_live_references(
    candidate: &RuntimeArtifactCandidate,
    expected: &RuntimeArtifactIdentity,
    reference_path: &Path,
) -> Result<bool, ReferenceScanFailure> {
    // proc_listpidspath is the Darwin kernel's targeted all-process query for
    // open references to this exact pathname. Unlike per-process FD walking,
    // it remains complete when macOS privacy controls deny proc_pidfdinfo for
    // otherwise same-user processes. Include O_EVTONLY references: any live
    // open reference is a reason to retain the artifact.
    validate_reference_path(candidate, expected, reference_path)?;
    if path_has_open_reference(reference_path)
        .map_err(|_| ReferenceScanFailure::OpenReferenceLookup)?
    {
        return Ok(true);
    }
    validate_reference_path(candidate, expected, reference_path)?;

    let uid = current_uid();
    let pids = pids_by_type(ProcFilter::ByUID { uid })
        .map_err(|_| ReferenceScanFailure::ProcessMetadata)?;
    let argmax = kernel_argmax().map_err(|_| ReferenceScanFailure::Arguments)?;
    let mut first_incomplete = None;
    for pid in pids {
        let Ok(pid_i32) = i32::try_from(pid) else {
            return Err(ReferenceScanFailure::ProcessMetadata);
        };
        let info = match pidinfo::<TaskAllInfo>(pid_i32, 0) {
            Ok(info) => info,
            Err(_) if crate::platform::is_confirmed_zombie(pid) => continue,
            Err(_) if !process_exists(pid) => continue,
            Err(_) => {
                first_incomplete.get_or_insert(ReferenceScanFailure::ProcessMetadata);
                continue;
            }
        };
        if info.pbsd.pbi_status == libc::SZOMB {
            continue;
        }
        if info.pbsd.pbi_uid != uid {
            continue;
        }
        match process_arguments(pid_i32, argmax) {
            Ok(arguments) => {
                if arguments_reference_candidate(&arguments, candidate) {
                    return Ok(true);
                }
            }
            Err(_) if crate::platform::is_confirmed_zombie(pid) || !process_exists(pid) => {
                continue;
            }
            Err(_) => {
                first_incomplete.get_or_insert(ReferenceScanFailure::Arguments);
            }
        }
    }
    match first_incomplete {
        Some(failure) => Err(failure),
        None => {
            validate_reference_path(candidate, expected, reference_path)?;
            Ok(false)
        }
    }
}

fn validate_reference_path(
    candidate: &RuntimeArtifactCandidate,
    expected: &RuntimeArtifactIdentity,
    reference_path: &Path,
) -> Result<(), ReferenceScanFailure> {
    let relative = reference_path
        .strip_prefix(candidate.profile_path())
        .map_err(|_| ReferenceScanFailure::ReferencePathIdentity)?;
    let mut components = relative.components();
    let Some(std::path::Component::Normal(name)) = components.next() else {
        return Err(ReferenceScanFailure::ReferencePathIdentity);
    };
    if components.next().is_some() {
        return Err(ReferenceScanFailure::ReferencePathIdentity);
    }
    let name =
        CString::new(name.as_bytes()).map_err(|_| ReferenceScanFailure::ReferencePathIdentity)?;
    let parent = open_safe_parent(candidate.profile_path(), candidate.owner_uid())
        .map_err(|_| ReferenceScanFailure::ReferencePathIdentity)?;
    let metadata = parent
        .metadata()
        .map_err(|_| ReferenceScanFailure::ReferencePathIdentity)?;
    if metadata.dev() != expected.parent_device
        || metadata.ino() != expected.parent_inode
        || metadata.uid() != expected.parent_owner_uid
        || metadata.mode() != expected.parent_mode
    {
        return Err(ReferenceScanFailure::ReferencePathIdentity);
    }
    let artifact = open_named_candidate(parent.as_raw_fd(), &name)
        .map_err(|_| ReferenceScanFailure::ReferencePathIdentity)?;
    let metadata = artifact
        .metadata()
        .map_err(|_| ReferenceScanFailure::ReferencePathIdentity)?;
    if !same_candidate_identity(&metadata, expected)
        || !fstatat_named_matches(parent.as_raw_fd(), &name, expected)
    {
        return Err(ReferenceScanFailure::ReferencePathIdentity);
    }
    Ok(())
}

fn path_has_open_reference(path: &Path) -> Result<bool, ()> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| ())?;
    let mut pid: libc::pid_t = 0;
    let returned_bytes = unsafe {
        proc_listpidspath(
            PROC_ALL_PIDS,
            0,
            path.as_ptr(),
            0,
            (&raw mut pid).cast(),
            libc::c_int::try_from(size_of::<libc::pid_t>()).map_err(|_| ())?,
        )
    };
    match returned_bytes {
        value if value < 0 => Err(()),
        0 => Ok(false),
        _ => Ok(true),
    }
}

#[cfg(test)]
fn process_has_argument_reference(
    pid: u32,
    candidate: &RuntimeArtifactCandidate,
) -> Result<bool, ()> {
    let pid_i32 = i32::try_from(pid).map_err(|_| ())?;
    let arguments = process_arguments(pid_i32, kernel_argmax()?)?;
    Ok(arguments_reference_candidate(&arguments, candidate))
}

fn arguments_reference_candidate(
    arguments: &[String],
    candidate: &RuntimeArtifactCandidate,
) -> bool {
    let Some(profile) = candidate.profile_path().to_str() else {
        return true;
    };
    let Some(artifact) = candidate.artifact_path().to_str() else {
        return true;
    };
    arguments
        .iter()
        .any(|argument| argument.contains(profile) || argument.contains(artifact))
}

fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

fn process_exists(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn kernel_argmax() -> Result<usize, ()> {
    let mut mib = [CTL_KERN, KERN_ARGMAX];
    let mut argmax: libc::c_int = 0;
    let mut size = size_of::<libc::c_int>();
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            (&raw mut argmax).cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || argmax <= 0
    {
        return Err(());
    }
    usize::try_from(argmax).map_err(|_| ())
}

fn process_arguments(pid: libc::c_int, argmax: usize) -> Result<Vec<String>, ()> {
    let mut mib = [CTL_KERN, KERN_PROCARGS2, pid];
    let mut buffer = vec![0_u8; argmax];
    let mut size = buffer.len();
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            buffer.as_mut_ptr().cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(());
    }
    buffer.truncate(size);
    parse_procargs2(&buffer).ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write as _;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct TempProfile {
        path: PathBuf,
    }

    impl TempProfile {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("wall clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "playwright_chromiumdev_profile-unlinger-artifact-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create owned test profile");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("make test profile owner-private");
            let path = fs::canonicalize(path).expect("canonical owned test profile");
            Self { path }
        }

        fn candidate(&self) -> RuntimeArtifactCandidate {
            let uid = fs::metadata(&self.path).expect("profile metadata").uid();
            RuntimeArtifactCandidate::devtools_active_port(&self.path, uid, "test-session")
                .expect("valid test candidate")
        }

        fn active_port(&self) -> PathBuf {
            self.path.join("DevToolsActivePort")
        }

        fn create_active_port(&self, contents: &[u8]) {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(self.active_port())
                .expect("create owner-private active port");
            file.write_all(contents).expect("write active port");
        }

        fn freeze(&self) -> FrozenRuntimeArtifact {
            match super::freeze(&self.candidate()).expect("freeze") {
                ArtifactFreeze::Frozen(frozen) => frozen,
                other => panic!("expected frozen artifact, got {other:?}"),
            }
        }
    }

    impl Drop for TempProfile {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn procargs_parser_ignores_the_environment_tail() {
        let mut bytes = 2_i32.to_ne_bytes().to_vec();
        bytes.extend_from_slice(b"/bin/tool\0\0tool\0value\0SECRET=not-an-arg\0");
        assert_eq!(
            parse_procargs2(&bytes),
            Some(vec!["tool".to_owned(), "value".to_owned()])
        );
    }

    #[test]
    fn procargs_parser_preserves_empty_arguments_before_the_environment_tail() {
        let mut bytes = 4_i32.to_ne_bytes().to_vec();
        bytes.extend_from_slice(b"/bin/tool\0\0\0tool\0\0value\0\0SECRET=not-an-arg\0");
        assert_eq!(
            parse_procargs2(&bytes),
            Some(vec![
                "tool".to_owned(),
                String::new(),
                "value".to_owned(),
                String::new()
            ])
        );
    }

    #[test]
    fn path_reference_query_treats_zero_as_success_with_stale_errno() {
        let profile = TempProfile::new("empty-path-reference");
        profile.create_active_port(b"9222\n");
        unsafe {
            *libc::__error() = libc::EINVAL;
        }
        assert_eq!(
            path_has_open_reference(&profile.active_port()),
            Ok(false),
            "a zero Darwin return is a complete empty result even when errno is stale"
        );
    }

    #[test]
    fn missing_path_reference_query_fails_closed() {
        let profile = TempProfile::new("missing-path-reference");
        assert_eq!(path_has_open_reference(&profile.active_port()), Err(()));
    }

    #[test]
    fn removes_only_the_exact_allowlisted_file() {
        let profile = TempProfile::new("success");
        profile.create_active_port(b"9222\n/devtools/browser/test\n");
        let unknown = profile.path.join("keep-me.txt");
        fs::write(&unknown, b"private state").expect("write sibling");
        let directory = profile.path.join("keep-directory");
        fs::create_dir(&directory).expect("create sibling directory");
        let frozen = profile.freeze();
        let mut scans = 0;

        assert_eq!(
            remove_exact_with(&frozen, |_, _, _| {
                scans += 1;
                Ok(false)
            }),
            ArtifactDisposition::Removed
        );
        assert_eq!(scans, 2, "safe removal requires both complete scans");
        assert!(!profile.active_port().exists());
        assert_eq!(
            fs::read(unknown).expect("retained sibling"),
            b"private state"
        );
        assert!(directory.is_dir());
    }

    #[test]
    fn symlink_and_inode_replacements_are_retained() {
        let profile = TempProfile::new("identity-race");
        profile.create_active_port(b"first");
        let frozen = profile.freeze();
        let original = profile.path.join("original-active-port");
        fs::rename(profile.active_port(), &original).expect("move target");
        symlink(&original, profile.active_port()).expect("replace with symlink");
        assert_eq!(
            remove_exact_with(&frozen, |_, _, _| Ok(false)),
            ArtifactDisposition::Unsafe
        );
        assert!(
            fs::symlink_metadata(profile.active_port())
                .expect("symlink retained")
                .file_type()
                .is_symlink()
        );

        fs::remove_file(profile.active_port()).expect("remove owned test symlink");
        profile.create_active_port(b"replacement");
        assert_eq!(
            remove_exact_with(&frozen, |_, _, _| Ok(false)),
            ArtifactDisposition::IdentityMismatch
        );
        assert_eq!(
            fs::read(profile.active_port()).expect("replacement retained"),
            b"replacement"
        );
    }

    #[test]
    fn last_hop_replacement_is_restored_without_being_unlinked() {
        let profile = TempProfile::new("last-hop-preserve");
        profile.create_active_port(b"original");
        let frozen = profile.freeze();
        let original = profile.path.join("original-active-port");

        assert_eq!(
            remove_exact_with_hooks(
                &frozen,
                |_, _, _| Ok(false),
                || {
                    fs::rename(profile.active_port(), &original).expect("move original");
                    profile.create_active_port(b"replacement");
                },
                |_| {},
            ),
            ArtifactDisposition::IdentityMismatch
        );
        assert_eq!(
            fs::read(profile.active_port()).expect("late replacement restored"),
            b"replacement"
        );
        assert_eq!(fs::read(original).expect("original retained"), b"original");
        assert!(quarantine_entries(&profile).is_empty());
    }

    #[test]
    fn last_hop_restore_conflict_preserves_canonical_and_quarantine() {
        let profile = TempProfile::new("last-hop-conflict");
        profile.create_active_port(b"original");
        let frozen = profile.freeze();
        let original = profile.path.join("original-active-port");

        assert_eq!(
            remove_exact_with_hooks(
                &frozen,
                |_, _, _| Ok(false),
                || {
                    fs::rename(profile.active_port(), &original).expect("move original");
                    profile.create_active_port(b"late replacement");
                },
                |_| profile.create_active_port(b"canonical conflict"),
            ),
            ArtifactDisposition::IdentityMismatch
        );
        assert_eq!(
            fs::read(profile.active_port()).expect("canonical conflict retained"),
            b"canonical conflict"
        );
        assert_eq!(fs::read(original).expect("original retained"), b"original");
        let quarantined = quarantine_entries(&profile);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            fs::read(&quarantined[0]).expect("late replacement retained"),
            b"late replacement"
        );
    }

    #[test]
    fn second_scan_reference_restores_the_exact_artifact() {
        let profile = TempProfile::new("second-scan-reference");
        profile.create_active_port(b"original");
        let frozen = profile.freeze();
        let mut scans = 0;

        assert_eq!(
            remove_exact_with(&frozen, |_, _, _| {
                scans += 1;
                Ok(scans == 2)
            }),
            ArtifactDisposition::Referenced
        );
        assert_eq!(scans, 2);
        assert_eq!(
            fs::read(profile.active_port()).expect("exact artifact restored"),
            b"original"
        );
        assert!(quarantine_entries(&profile).is_empty());
    }

    #[test]
    fn second_scan_error_restores_the_exact_artifact() {
        let profile = TempProfile::new("second-scan-error");
        profile.create_active_port(b"original");
        let frozen = profile.freeze();
        let mut scans = 0;

        assert_eq!(
            remove_exact_with(&frozen, |_, _, _| {
                scans += 1;
                if scans == 1 { Ok(false) } else { Err(()) }
            }),
            ArtifactDisposition::Unsafe
        );
        assert_eq!(scans, 2);
        assert_eq!(
            fs::read(profile.active_port()).expect("exact artifact restored"),
            b"original"
        );
        assert!(quarantine_entries(&profile).is_empty());
    }

    #[test]
    fn canonical_replacement_after_quarantine_preserves_both_entries() {
        let profile = TempProfile::new("post-quarantine-canonical");
        profile.create_active_port(b"original");
        let frozen = profile.freeze();

        assert_eq!(
            remove_exact_with_hooks(
                &frozen,
                |_, _, _| Ok(false),
                || {},
                |_| profile.create_active_port(b"replacement"),
            ),
            ArtifactDisposition::IdentityMismatch
        );
        assert_eq!(
            fs::read(profile.active_port()).expect("canonical replacement retained"),
            b"replacement"
        );
        let quarantined = quarantine_entries(&profile);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            fs::read(&quarantined[0]).expect("exact quarantine retained"),
            b"original"
        );
    }

    #[test]
    fn restore_conflict_retains_canonical_and_exact_quarantine() {
        let profile = TempProfile::new("restore-conflict");
        profile.create_active_port(b"original");
        let frozen = profile.freeze();
        let mut scans = 0;

        assert_eq!(
            remove_exact_with_hooks(
                &frozen,
                |_, _, _| {
                    scans += 1;
                    Ok(scans == 2)
                },
                || {},
                |_| profile.create_active_port(b"replacement"),
            ),
            ArtifactDisposition::IdentityMismatch
        );
        assert_eq!(scans, 2);
        assert_eq!(
            fs::read(profile.active_port()).expect("canonical replacement retained"),
            b"replacement"
        );
        let quarantined = quarantine_entries(&profile);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            fs::read(&quarantined[0]).expect("exact quarantine retained"),
            b"original"
        );
    }

    #[test]
    fn a_directory_named_like_the_artifact_is_never_frozen_or_removed() {
        let profile = TempProfile::new("directory-target");
        fs::create_dir(profile.active_port()).expect("create directory target");

        assert_eq!(
            super::freeze(&profile.candidate()).expect("freeze decision"),
            ArtifactFreeze::Unsafe
        );
        assert!(profile.active_port().is_dir());
    }

    #[test]
    fn fifo_named_like_the_artifact_is_promptly_unsafe() {
        let profile = TempProfile::new("fifo-target");
        let fifo_path = profile.active_port();
        let fifo_c_path =
            CString::new(fifo_path.as_os_str().as_bytes()).expect("fifo path C string");
        assert_eq!(
            unsafe { libc::mkfifo(fifo_c_path.as_ptr(), 0o600) },
            0,
            "create FIFO: {}",
            std::io::Error::last_os_error()
        );

        let candidate = profile.candidate();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            sender
                .send(super::freeze(&candidate))
                .expect("send freeze decision");
        });
        let decision = match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(decision) => decision,
            Err(error) => {
                // Release a regressed blocking FIFO reader before failing so the
                // test process does not retain a stuck worker thread.
                let fd = unsafe {
                    libc::open(
                        fifo_c_path.as_ptr(),
                        libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
                    )
                };
                if fd >= 0 {
                    unsafe { libc::close(fd) };
                }
                worker.join().expect("join released FIFO worker");
                panic!("FIFO metadata open did not return promptly: {error}");
            }
        };
        worker.join().expect("join FIFO worker");
        assert_eq!(decision.expect("freeze decision"), ArtifactFreeze::Unsafe);
    }

    #[test]
    fn owner_private_parent_and_owner_controlled_file_modes_are_required() {
        let profile = TempProfile::new("private-modes");
        profile.create_active_port(b"9222\n");

        fs::set_permissions(&profile.path, fs::Permissions::from_mode(0o755))
            .expect("make profile world-readable");
        assert_eq!(
            super::freeze(&profile.candidate()).expect("parent mode decision"),
            ArtifactFreeze::Unsafe
        );

        fs::set_permissions(&profile.path, fs::Permissions::from_mode(0o700))
            .expect("restore private profile mode");
        fs::set_permissions(profile.active_port(), fs::Permissions::from_mode(0o644))
            .expect("match Chrome for Testing artifact mode");
        assert!(matches!(
            super::freeze(&profile.candidate()).expect("real CfT file mode decision"),
            ArtifactFreeze::Frozen(_)
        ));

        fs::set_permissions(profile.active_port(), fs::Permissions::from_mode(0o664))
            .expect("make artifact group-writable");
        assert_eq!(
            super::freeze(&profile.candidate()).expect("group-writable file mode decision"),
            ArtifactFreeze::Unsafe
        );

        fs::set_permissions(profile.active_port(), fs::Permissions::from_mode(0o646))
            .expect("make artifact other-writable");
        assert_eq!(
            super::freeze(&profile.candidate()).expect("other-writable file mode decision"),
            ArtifactFreeze::Unsafe
        );
    }

    #[test]
    fn extended_acls_on_parent_or_artifact_are_rejected_via_open_descriptors() {
        let profile = TempProfile::new("extended-acl");
        profile.create_active_port(b"9222\n");

        if !add_fixture_acl(&profile.path) {
            return;
        }
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(&profile.path)
            .expect("open ACL parent fixture");
        assert!(!fd_has_empty_extended_acl(parent.as_raw_fd()).expect("inspect parent ACL"));
        assert_eq!(
            super::freeze(&profile.candidate()).expect("parent ACL decision"),
            ArtifactFreeze::Unsafe
        );
        drop(parent);
        remove_fixture_acl(&profile.path);

        assert!(add_fixture_acl(&profile.active_port()));
        let artifact = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(profile.active_port())
            .expect("open ACL artifact fixture");
        assert!(!fd_has_empty_extended_acl(artifact.as_raw_fd()).expect("inspect artifact ACL"));
        assert_eq!(
            super::freeze(&profile.candidate()).expect("artifact ACL decision"),
            ArtifactFreeze::Unsafe
        );
        drop(artifact);
        remove_fixture_acl(&profile.active_port());
    }

    #[test]
    fn internal_artifact_fds_are_closed_before_the_second_self_scan() {
        let profile = TempProfile::new("self-scan");
        profile.create_active_port(b"9222\n");
        let frozen = profile.freeze();
        let mut scans = 0;

        assert_eq!(
            remove_exact_with(&frozen, |_, _, reference_path| {
                scans += 1;
                path_has_open_reference(reference_path)
            }),
            ArtifactDisposition::Removed
        );
        assert_eq!(scans, 2, "real self-reference scan ran at both gates");
        assert!(!profile.active_port().exists());
    }

    #[test]
    fn targeted_kernel_lookup_tracks_an_open_inode_across_quarantine_rename() {
        let profile = TempProfile::new("kernel-path-references");
        profile.create_active_port(b"9222\n");
        let canonical = profile.active_port();
        let open_reference = File::open(&canonical).expect("open target reference");
        assert!(
            path_has_open_reference(&canonical).expect("query canonical path references"),
            "kernel query did not report the owned open reference"
        );

        let quarantined = profile.path.join(".kernel-path-reference-test");
        fs::rename(&canonical, &quarantined).expect("rename exact test artifact");
        assert!(
            path_has_open_reference(&quarantined).expect("query quarantined path references"),
            "kernel query lost the owned open reference after rename"
        );

        drop(open_reference);
        let mut still_referenced = true;
        for _ in 0..5 {
            still_referenced =
                path_has_open_reference(&quarantined).expect("query released path references");
            if !still_referenced {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !still_referenced,
            "kernel query retained a released owned reference"
        );
    }

    #[test]
    fn targeted_kernel_lookup_includes_event_only_references() {
        let profile = TempProfile::new("event-only-reference");
        profile.create_active_port(b"9222\n");
        let path = profile.active_port();
        let c_path = CString::new(path.as_os_str().as_bytes()).expect("artifact path C string");
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_EVTONLY | libc::O_CLOEXEC) };
        assert!(
            fd >= 0,
            "open O_EVTONLY reference: {}",
            std::io::Error::last_os_error()
        );
        let open_reference = unsafe { File::from_raw_fd(fd) };
        assert_eq!(path_has_open_reference(&path), Ok(true));
        drop(open_reference);
    }

    #[test]
    fn reference_scan_rejects_a_replaced_profile_path() {
        let profile = TempProfile::new("parent-path-replacement");
        profile.create_active_port(b"9222\n");
        let candidate = profile.candidate();
        let frozen = profile.freeze();
        let moved = profile
            .path
            .parent()
            .expect("temporary parent")
            .join(format!(
                "{}-moved",
                profile
                    .path
                    .file_name()
                    .expect("temporary profile filename")
                    .to_string_lossy()
            ));

        fs::rename(&profile.path, &moved).expect("move frozen profile path");
        fs::create_dir(&profile.path).expect("replace profile directory pathname");
        fs::set_permissions(&profile.path, fs::Permissions::from_mode(0o700))
            .expect("make replacement profile owner-private");
        profile.create_active_port(b"replacement\n");
        let result = scan_live_references(&candidate, frozen.identity(), candidate.artifact_path());

        fs::remove_dir_all(&profile.path).expect("remove owned replacement profile");
        fs::rename(&moved, &profile.path).expect("restore frozen profile path");
        assert_eq!(result, Err(ReferenceScanFailure::ReferencePathIdentity));
    }

    #[test]
    fn live_fd_and_argv_references_are_detected() {
        let profile = TempProfile::new("references");
        profile.create_active_port(b"9222\n");
        let candidate = profile.candidate();
        let frozen = profile.freeze();
        let open_reference = File::open(profile.active_port()).expect("open target reference");
        assert_eq!(
            scan_live_references(&candidate, frozen.identity(), candidate.artifact_path()),
            Ok(true),
            "targeted kernel scan missed an owned open reference"
        );
        drop(open_reference);

        let mut child = Command::new("/usr/bin/yes")
            .arg(profile.active_port())
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn argv holder");
        let child_pid = child.id();
        let mut observed = false;
        for _ in 0..50 {
            if process_has_argument_reference(child_pid, &candidate).unwrap_or(false) {
                assert_eq!(
                    scan_live_references(&candidate, frozen.identity(), candidate.artifact_path(),),
                    Ok(true),
                    "complete argv scan missed the owned child reference"
                );
                observed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.kill().expect("kill owned argv holder");
        child.wait().expect("wait owned argv holder");
        assert!(observed, "owned child argv reference was not observed");
    }

    #[test]
    fn an_incomplete_reference_scan_never_unlinks() {
        let profile = TempProfile::new("incomplete-scan");
        profile.create_active_port(b"9222\n");
        let frozen = profile.freeze();
        assert_eq!(
            remove_exact_with(&frozen, |_, _, _| Err(())),
            ArtifactDisposition::Unsafe
        );
        assert!(profile.active_port().is_file());
    }

    fn quarantine_entries(profile: &TempProfile) -> Vec<PathBuf> {
        fs::read_dir(&profile.path)
            .expect("read profile")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(QUARANTINE_PREFIX))
            })
            .collect()
    }

    fn add_fixture_acl(path: &Path) -> bool {
        let output = match Command::new("/bin/chmod")
            .arg("+a")
            .arg("everyone deny delete")
            .arg(path)
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("skipping ACL fixture: /bin/chmod is unavailable");
                return false;
            }
            Err(error) => panic!("start ACL fixture command: {error}"),
        };
        if output.status.success() {
            return true;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let normalized = stderr.to_ascii_lowercase();
        if normalized.contains("operation not supported") || normalized.contains("not supported") {
            eprintln!("skipping ACL fixture because this filesystem has no ACL support: {stderr}");
            return false;
        }
        panic!("add ACL fixture failed: {stderr}");
    }

    fn remove_fixture_acl(path: &Path) {
        let output = Command::new("/bin/chmod")
            .arg("-N")
            .arg(path)
            .output()
            .expect("start ACL fixture cleanup command");
        assert!(
            output.status.success(),
            "remove ACL fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
