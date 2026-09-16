use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use unlinger_core::{
    ProcessRecord, Snapshot, StorageResidueKind, StorageResidueObservation,
    StorageResidueReferenceCheck, StorageResidueStatus,
};

const CLONE_ROOT_NAME: &str = "com.google.Chrome.code_sign_clone";
const CLONE_PREFIX: &str = "code_sign_clone.";
const CHROME_BUNDLE_ID: &str = "com.google.Chrome";
const CHROME_HELPER_BUNDLE_ID_PREFIX: &str = "com.google.Chrome.helper";
const CLONE_CLEANUP_TYPE_ARGUMENT: &str = "--type=code-sign-clone-cleanup";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChromeCloneCleanupMode {
    ReportOnly,
    Enforce,
    LifecycleBlocked,
}

/// Keeps only the previous private candidate identities in memory. Candidate
/// names and descriptors never enter SQLite, IPC, diagnostics, or logs.
#[derive(Debug, Default)]
pub struct ChromeCloneCleanup {
    previous_candidates: Option<Vec<CloneCandidateIdentity>>,
}

impl ChromeCloneCleanup {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn reconcile<F>(
        &mut self,
        observed_at_unix_millis: u64,
        snapshot: Option<&Snapshot>,
        mode: ChromeCloneCleanupMode,
        mut run_mutation: F,
    ) -> StorageResidueObservation
    where
        F: FnMut(&mut dyn FnMut() -> io::Result<()>) -> Option<io::Result<()>>,
    {
        let temporary = std::env::temp_dir();
        let Some(parent) = temporary.parent() else {
            self.previous_candidates = None;
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.temp_root_unavailable",
            );
        };
        self.reconcile_root_guarded(
            &parent.join("X").join(CLONE_ROOT_NAME),
            observed_at_unix_millis,
            snapshot,
            mode,
            &DescriptorRelativeRemover,
            &mut run_mutation,
        )
    }

    /// Isolated fixture seam. Production exposes only [`Self::reconcile`], so
    /// its deletion path is hard-wired to the exact current-user X clone root.
    #[cfg(test)]
    #[must_use]
    fn reconcile_root(
        &mut self,
        root: &Path,
        observed_at_unix_millis: u64,
        snapshot: Option<&Snapshot>,
        mode: ChromeCloneCleanupMode,
    ) -> StorageResidueObservation {
        self.reconcile_root_guarded(
            root,
            observed_at_unix_millis,
            snapshot,
            mode,
            &DescriptorRelativeRemover,
            &mut |action| Some(action()),
        )
    }

    #[cfg(test)]
    fn reconcile_root_with_remover<R: CandidateRemover>(
        &mut self,
        root: &Path,
        observed_at_unix_millis: u64,
        snapshot: Option<&Snapshot>,
        mode: ChromeCloneCleanupMode,
        remover: &R,
    ) -> StorageResidueObservation {
        self.reconcile_root_guarded(
            root,
            observed_at_unix_millis,
            snapshot,
            mode,
            remover,
            &mut |action| Some(action()),
        )
    }

    fn reconcile_root_guarded<
        R: CandidateRemover,
        F: FnMut(&mut dyn FnMut() -> io::Result<()>) -> Option<io::Result<()>>,
    >(
        &mut self,
        root: &Path,
        observed_at_unix_millis: u64,
        snapshot: Option<&Snapshot>,
        mode: ChromeCloneCleanupMode,
        remover: &R,
        run_mutation: &mut F,
    ) -> StorageResidueObservation {
        let scan = scan_code_sign_clone_root(root, observed_at_unix_millis);
        if scan.observation.status != StorageResidueStatus::Detected {
            self.previous_candidates = None;
            return scan.observation;
        }

        let identities = scan.identities();
        let stable = self.previous_candidates.as_ref() == Some(&identities);
        self.previous_candidates = Some(identities.clone());

        let process_gate = process_gate(snapshot);
        let mut observation = scan.observation.clone();
        apply_process_gate(&mut observation, process_gate);
        if !stable {
            observation
                .reason_ids
                .push("storage_residue.cleanup_waiting_for_stability".to_owned());
            return observation;
        }
        if process_gate != ProcessGate::Clear {
            return observation;
        }

        if mode == ChromeCloneCleanupMode::LifecycleBlocked {
            observation
                .reason_ids
                .push("storage_residue.cleanup_lifecycle_blocked".to_owned());
            return observation;
        }

        observation.automatic_cleanup_eligible = true;
        if mode == ChromeCloneCleanupMode::ReportOnly {
            observation
                .reason_ids
                .push("storage_residue.cleanup_report_only".to_owned());
            return observation;
        }
        let mut mutation = || remover.remove(&scan);
        let Some(removal_result) = run_mutation(&mut mutation) else {
            observation.automatic_cleanup_eligible = false;
            observation
                .reason_ids
                .push("storage_residue.cleanup_lifecycle_blocked".to_owned());
            return observation;
        };

        let result_scan = scan_code_sign_clone_root(root, observed_at_unix_millis);
        let remaining_candidates_unchanged = result_scan.identities() == identities;
        let mut result = result_scan.observation;
        self.previous_candidates = None;
        if removal_result.is_ok() && result.status == StorageResidueStatus::Clear {
            result
                .reason_ids
                .push("storage_residue.automatic_cleanup_completed".to_owned());
        } else {
            if result.status == StorageResidueStatus::Detected {
                apply_process_gate(&mut result, process_gate);
                result.automatic_cleanup_eligible =
                    remaining_candidates_unchanged && process_gate == ProcessGate::Clear;
            }
            result
                .reason_ids
                .push("storage_residue.automatic_cleanup_failed".to_owned());
        }
        result
    }
}

#[must_use]
pub fn observe_chrome_code_sign_clones(observed_at_unix_millis: u64) -> StorageResidueObservation {
    let temporary = std::env::temp_dir();
    let Some(parent) = temporary.parent() else {
        return unavailable(
            observed_at_unix_millis,
            "storage_residue.temp_root_unavailable",
        );
    };
    inspect_code_sign_clone_root(
        &parent.join("X").join(CLONE_ROOT_NAME),
        observed_at_unix_millis,
    )
}

#[must_use]
pub fn inspect_code_sign_clone_root(
    root: &Path,
    observed_at_unix_millis: u64,
) -> StorageResidueObservation {
    scan_code_sign_clone_root(root, observed_at_unix_millis).observation
}

struct CloneScan {
    observation: StorageResidueObservation,
    root: Option<File>,
    candidates: Vec<CloneCandidate>,
}

impl CloneScan {
    fn identities(&self) -> Vec<CloneCandidateIdentity> {
        self.candidates
            .iter()
            .map(|candidate| candidate.identity.clone())
            .collect()
    }
}

struct CloneCandidate {
    identity: CloneCandidateIdentity,
    directory: File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CloneCandidateIdentity {
    name: CString,
    device: u64,
    inode: u64,
}

fn scan_code_sign_clone_root(root: &Path, observed_at_unix_millis: u64) -> CloneScan {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return scan_from_observation(StorageResidueObservation {
                kind: StorageResidueKind::ChromeCodeSignClone,
                status: StorageResidueStatus::Clear,
                observed_at_unix_millis,
                candidate_count: 0,
                logical_bytes: 0,
                shape_complete: true,
                reference_check: StorageResidueReferenceCheck::Incomplete,
                automatic_cleanup_eligible: false,
                reason_ids: vec!["storage_residue.code_sign_clone_absent".to_owned()],
            });
        }
        Err(_) => {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            ));
        }
    };
    if !metadata.file_type().is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
        return scan_from_observation(unavailable(
            observed_at_unix_millis,
            "storage_residue.clone_root_unsafe",
        ));
    }
    let directory = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
    {
        Ok(directory) => directory,
        Err(_) => {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            ));
        }
    };
    let mut entries = match directory_names(&directory) {
        Ok(entries) => entries,
        Err(_) => {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            ));
        }
    };
    entries.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let mut candidate_count = 0_usize;
    let mut logical_bytes = 0_u64;
    let mut candidates = Vec::with_capacity(entries.len());
    for name in entries {
        let Ok(text_name) = name.to_str() else {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            ));
        };
        if !valid_clone_name(text_name) {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            ));
        }
        let Ok(clone) = open_child_directory(&directory, &name) else {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            ));
        };
        let Ok(clone_metadata) = clone.metadata() else {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            ));
        };
        if clone_metadata.uid() != unsafe { libc::geteuid() }
            || ![c"Google Chrome.app.bundle", c"Google Chrome.app"]
                .iter()
                .any(|name| open_child_directory(&clone, name).is_ok())
        {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            ));
        }
        let Ok(bytes) = logical_tree_bytes(&clone) else {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_tree_unreadable",
            ));
        };
        let Some(total) = logical_bytes.checked_add(bytes) else {
            return scan_from_observation(unavailable(
                observed_at_unix_millis,
                "storage_residue.logical_size_overflow",
            ));
        };
        logical_bytes = total;
        candidate_count += 1;
        candidates.push(CloneCandidate {
            identity: CloneCandidateIdentity {
                name,
                device: clone_metadata.dev(),
                inode: clone_metadata.ino(),
            },
            directory: clone,
        });
    }
    let status = if candidate_count == 0 {
        StorageResidueStatus::Clear
    } else {
        StorageResidueStatus::Detected
    };
    let mut reason_ids = vec![if candidate_count == 0 {
        "storage_residue.code_sign_clone_empty".to_owned()
    } else {
        "storage_residue.code_sign_clone_detected".to_owned()
    }];
    if candidate_count > 0 {
        reason_ids.push("storage_residue.logical_size_not_physical_reclaim".to_owned());
        reason_ids.push("storage_residue.reference_check_incomplete".to_owned());
    }
    CloneScan {
        observation: StorageResidueObservation {
            kind: StorageResidueKind::ChromeCodeSignClone,
            status,
            observed_at_unix_millis,
            candidate_count,
            logical_bytes,
            shape_complete: true,
            reference_check: StorageResidueReferenceCheck::Incomplete,
            automatic_cleanup_eligible: false,
            reason_ids,
        },
        root: Some(directory),
        candidates,
    }
}

fn scan_from_observation(observation: StorageResidueObservation) -> CloneScan {
    CloneScan {
        observation,
        root: None,
        candidates: Vec::new(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessGate {
    Clear,
    ChromeProcessActive,
    CleanupHelperActive,
    Incomplete,
}

impl ProcessGate {
    fn reference_check(self) -> StorageResidueReferenceCheck {
        match self {
            Self::Clear => StorageResidueReferenceCheck::CompleteNoReferences,
            Self::ChromeProcessActive | Self::CleanupHelperActive => {
                StorageResidueReferenceCheck::Referenced
            }
            Self::Incomplete => StorageResidueReferenceCheck::Incomplete,
        }
    }

    fn reason_ids(self) -> Vec<String> {
        match self {
            Self::Clear => vec!["storage_residue.no_live_chrome_references".to_owned()],
            Self::ChromeProcessActive => {
                vec!["storage_residue.chrome_process_active".to_owned()]
            }
            Self::CleanupHelperActive => {
                vec!["storage_residue.clone_cleanup_helper_active".to_owned()]
            }
            Self::Incomplete => {
                vec!["storage_residue.process_observation_incomplete".to_owned()]
            }
        }
    }
}

fn apply_process_gate(observation: &mut StorageResidueObservation, process_gate: ProcessGate) {
    observation.reference_check = process_gate.reference_check();
    observation
        .reason_ids
        .retain(|reason| reason != "storage_residue.reference_check_incomplete");
    observation.reason_ids.extend(process_gate.reason_ids());
}

fn process_gate(snapshot: Option<&Snapshot>) -> ProcessGate {
    let Some(snapshot) = snapshot else {
        return ProcessGate::Incomplete;
    };
    if snapshot.processes.iter().any(is_clone_cleanup_helper) {
        return ProcessGate::CleanupHelperActive;
    }
    if snapshot
        .processes
        .iter()
        .any(is_bundle_confirmed_chrome_process)
    {
        return ProcessGate::ChromeProcessActive;
    }
    if !snapshot.proves_complete_classification_coverage()
        || snapshot
            .processes
            .iter()
            .any(|process| may_be_chrome_process(process) && process.runtime.app_bundle.is_none())
    {
        return ProcessGate::Incomplete;
    }
    ProcessGate::Clear
}

fn is_bundle_confirmed_chrome_process(process: &ProcessRecord) -> bool {
    process.runtime.app_bundle.as_ref().is_some_and(|bundle| {
        bundle.bundle_id == CHROME_BUNDLE_ID
            || bundle.bundle_id == CHROME_HELPER_BUNDLE_ID_PREFIX
            || bundle
                .bundle_id
                .starts_with(&format!("{CHROME_HELPER_BUNDLE_ID_PREFIX}."))
    })
}

fn is_clone_cleanup_helper(process: &ProcessRecord) -> bool {
    let bundle_is_chrome = process.runtime.app_bundle.as_ref().is_some_and(|bundle| {
        bundle.bundle_id == CHROME_BUNDLE_ID
            || bundle.bundle_id == CHROME_HELPER_BUNDLE_ID_PREFIX
            || bundle
                .bundle_id
                .starts_with(&format!("{CHROME_HELPER_BUNDLE_ID_PREFIX}."))
    });
    bundle_is_chrome
        && process.arguments.as_ref().is_some_and(|arguments| {
            arguments
                .iter()
                .any(|argument| argument == CLONE_CLEANUP_TYPE_ARGUMENT)
        })
}

fn may_be_chrome_process(process: &ProcessRecord) -> bool {
    matches!(
        process.executable_basename().as_str(),
        "Google Chrome" | "Google Chrome Helper"
    ) || process.arguments.as_ref().is_some_and(|arguments| {
        arguments
            .iter()
            .any(|argument| argument == CLONE_CLEANUP_TYPE_ARGUMENT)
    })
}

fn valid_clone_name(name: &str) -> bool {
    name.strip_prefix(CLONE_PREFIX)
        .is_some_and(valid_clone_suffix)
}

fn valid_clone_suffix(suffix: &str) -> bool {
    suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

trait CandidateRemover {
    fn remove(&self, scan: &CloneScan) -> io::Result<()>;
}

struct DescriptorRelativeRemover;

impl CandidateRemover for DescriptorRelativeRemover {
    fn remove(&self, scan: &CloneScan) -> io::Result<()> {
        let root = scan
            .root
            .as_ref()
            .ok_or_else(|| io::Error::other("clone root is unavailable"))?;
        for candidate in &scan.candidates {
            let metadata = candidate.directory.metadata()?;
            if metadata.uid() != unsafe { libc::geteuid() }
                || metadata.dev() != candidate.identity.device
                || metadata.ino() != candidate.identity.inode
            {
                return Err(io::Error::other("clone candidate identity changed"));
            }
            verify_linked_candidate(root, &candidate.identity)?;
            remove_directory_contents(&candidate.directory)?;
            verify_linked_candidate(root, &candidate.identity)?;
            if unsafe {
                libc::unlinkat(
                    root.as_raw_fd(),
                    candidate.identity.name.as_ptr(),
                    libc::AT_REMOVEDIR,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

fn verify_linked_candidate(root: &File, identity: &CloneCandidateIdentity) -> io::Result<()> {
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            root.as_raw_fd(),
            identity.name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let metadata = unsafe { metadata.assume_init() };
    if metadata.st_mode & libc::S_IFMT != libc::S_IFDIR
        || metadata.st_dev as u64 != identity.device
        || metadata.st_ino != identity.inode
    {
        return Err(io::Error::other("clone candidate root entry changed"));
    }
    Ok(())
}

fn remove_directory_contents(directory: &File) -> io::Result<()> {
    for name in directory_names(directory)? {
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let metadata = unsafe { metadata.assume_init() };
        let flags = match metadata.st_mode & libc::S_IFMT {
            libc::S_IFDIR => {
                let child = open_child_directory(directory, &name)?;
                remove_directory_contents(&child)?;
                libc::AT_REMOVEDIR
            }
            libc::S_IFREG | libc::S_IFLNK => 0,
            _ => return Err(io::Error::other("clone tree contains an unsupported node")),
        };
        if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), flags) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn logical_tree_bytes(directory: &File) -> io::Result<u64> {
    let mut total = 0_u64;
    for name in directory_names(directory)? {
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let metadata = unsafe { metadata.assume_init() };
        let bytes = match metadata.st_mode & libc::S_IFMT {
            // Framework links are leaves. Count their regular targets only through
            // real directory entries; never follow links, including outside/dangling links.
            libc::S_IFLNK => 0,
            libc::S_IFREG => u64::try_from(metadata.st_size)
                .map_err(|_| io::Error::other("negative clone file size"))?,
            libc::S_IFDIR => logical_tree_bytes(&open_child_directory(directory, &name)?)?,
            _ => return Err(io::Error::other("clone tree contains an unsupported node")),
        };
        total = total
            .checked_add(bytes)
            .ok_or_else(|| io::Error::other("logical size overflow"))?;
    }
    Ok(total)
}

fn open_child_directory(parent: &File, name: &CStr) -> io::Result<File> {
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn directory_names(directory: &File) -> io::Result<Vec<CString>> {
    struct DirectoryStream(*mut libc::DIR);
    impl Drop for DirectoryStream {
        fn drop(&mut self) {
            unsafe { libc::closedir(self.0) };
        }
    }

    let descriptor = directory.try_clone()?.into_raw_fd();
    if unsafe { libc::lseek(descriptor, 0, libc::SEEK_SET) } < 0 {
        let error = io::Error::last_os_error();
        drop(unsafe { File::from_raw_fd(descriptor) });
        return Err(error);
    }
    let pointer = unsafe { libc::fdopendir(descriptor) };
    if pointer.is_null() {
        let error = io::Error::last_os_error();
        drop(unsafe { File::from_raw_fd(descriptor) });
        return Err(error);
    }
    let stream = DirectoryStream(pointer);
    let mut names = Vec::new();
    loop {
        unsafe { *libc::__error() = 0 };
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
            names.push(name.to_owned());
        }
    }
}

fn unavailable(observed_at_unix_millis: u64, reason_id: &str) -> StorageResidueObservation {
    StorageResidueObservation {
        kind: StorageResidueKind::ChromeCodeSignClone,
        status: StorageResidueStatus::Unavailable,
        observed_at_unix_millis,
        candidate_count: 0,
        logical_bytes: 0,
        shape_complete: false,
        reference_check: StorageResidueReferenceCheck::Incomplete,
        automatic_cleanup_eligible: false,
        reason_ids: vec![reason_id.to_owned()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use unlinger_core::{
        AppBundleVersion, ExecutableIdentity, ProcessIdentity, ProcessRuntimeFacts, ProcessStatus,
        SnapshotCoverage,
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "unlinger-residue-cleanup-test-{}-{nonce}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create test root");
            Self(path)
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn clone_root(temp: &TempDirectory) -> PathBuf {
        temp.0.join(CLONE_ROOT_NAME)
    }

    fn add_clone(root: &Path, suffix: &str) -> PathBuf {
        let candidate = root.join(format!("{CLONE_PREFIX}{suffix}"));
        let executable = candidate.join("Google Chrome.app/Contents/MacOS/Google Chrome");
        fs::create_dir_all(executable.parent().expect("executable parent"))
            .expect("create clone fixture");
        fs::write(executable, b"clone").expect("write clone fixture");
        candidate
    }

    fn complete_snapshot(processes: Vec<ProcessRecord>) -> Snapshot {
        Snapshot {
            observed_at_unix_millis: 1,
            current_uid: unsafe { libc::geteuid() },
            coverage: SnapshotCoverage {
                listed_processes: processes.len(),
                inspected_processes: processes.len(),
                ..SnapshotCoverage::default()
            },
            processes,
        }
    }

    fn chrome_process(arguments: Vec<&str>, bundle_id: &str, basename: &str) -> ProcessRecord {
        ProcessRecord {
            identity: ProcessIdentity {
                pid: 42,
                started_at_unix_micros: 9,
                executable_device: Some(1),
                executable_inode: Some(2),
            },
            parent_pid: 1,
            process_group_id: 42,
            uid: unsafe { libc::geteuid() },
            tty_device: None,
            name: basename.to_owned(),
            executable_path: Some(format!(
                "/Applications/Google Chrome.app/Contents/MacOS/{basename}"
            )),
            executable: ExecutableIdentity {
                device: Some(1),
                inode: Some(2),
                size: Some(3),
                modified_unix_nanos: Some(4),
            },
            arguments: Some(arguments.into_iter().map(str::to_owned).collect()),
            resident_memory_bytes: 1,
            status: ProcessStatus::Running,
            runtime: ProcessRuntimeFacts {
                app_bundle: Some(AppBundleVersion {
                    bundle_id: bundle_id.to_owned(),
                    short_version: "152.0.0.0".to_owned(),
                }),
                ..ProcessRuntimeFacts::default()
            },
        }
    }

    #[test]
    fn report_only_never_mutates_even_after_stable_observation() {
        let temp = TempDirectory::new();
        let root = clone_root(&temp);
        let candidate = add_clone(&root, "A1b2C3");
        let snapshot = complete_snapshot(Vec::new());
        let mut cleanup = ChromeCloneCleanup::new();

        let first = cleanup.reconcile_root(
            &root,
            1,
            Some(&snapshot),
            ChromeCloneCleanupMode::ReportOnly,
        );
        let second = cleanup.reconcile_root(
            &root,
            2,
            Some(&snapshot),
            ChromeCloneCleanupMode::ReportOnly,
        );

        assert!(!first.automatic_cleanup_eligible);
        assert!(second.automatic_cleanup_eligible);
        assert_eq!(
            second.reference_check,
            StorageResidueReferenceCheck::CompleteNoReferences
        );
        assert!(
            !second
                .reason_ids
                .contains(&"storage_residue.reference_check_incomplete".to_owned())
        );
        assert!(candidate.exists());
    }

    #[test]
    fn enforce_waits_for_two_stable_observations_then_deletes_and_rescans() {
        let temp = TempDirectory::new();
        let root = clone_root(&temp);
        let candidate = add_clone(&root, "A1b2C3");
        let snapshot = complete_snapshot(Vec::new());
        let mut cleanup = ChromeCloneCleanup::new();

        let first =
            cleanup.reconcile_root(&root, 1, Some(&snapshot), ChromeCloneCleanupMode::Enforce);
        assert_eq!(first.status, StorageResidueStatus::Detected);
        assert!(candidate.exists());

        let second =
            cleanup.reconcile_root(&root, 2, Some(&snapshot), ChromeCloneCleanupMode::Enforce);
        assert_eq!(second.status, StorageResidueStatus::Clear);
        assert_eq!(second.candidate_count, 0);
        assert!(!second.automatic_cleanup_eligible);
        assert!(!candidate.exists());
        assert!(
            second
                .reason_ids
                .contains(&"storage_residue.automatic_cleanup_completed".to_owned())
        );
    }

    #[test]
    fn candidate_change_restarts_stability_window() {
        let temp = TempDirectory::new();
        let root = clone_root(&temp);
        let first_candidate = add_clone(&root, "A1b2C3");
        let snapshot = complete_snapshot(Vec::new());
        let mut cleanup = ChromeCloneCleanup::new();

        let _ = cleanup.reconcile_root(&root, 1, Some(&snapshot), ChromeCloneCleanupMode::Enforce);
        let second_candidate = add_clone(&root, "D4e5F6");
        let changed =
            cleanup.reconcile_root(&root, 2, Some(&snapshot), ChromeCloneCleanupMode::Enforce);

        assert_eq!(changed.status, StorageResidueStatus::Detected);
        assert!(!changed.automatic_cleanup_eligible);
        assert!(first_candidate.exists());
        assert!(second_candidate.exists());
    }

    #[test]
    fn lifecycle_revalidation_closes_the_gate_before_mutation() {
        let temp = TempDirectory::new();
        let root = clone_root(&temp);
        let candidate = add_clone(&root, "A1b2C3");
        let snapshot = complete_snapshot(Vec::new());
        let mut cleanup = ChromeCloneCleanup::new();
        let _ = cleanup.reconcile_root(&root, 1, Some(&snapshot), ChromeCloneCleanupMode::Enforce);

        let observation = cleanup.reconcile_root_guarded(
            &root,
            2,
            Some(&snapshot),
            ChromeCloneCleanupMode::Enforce,
            &DescriptorRelativeRemover,
            &mut |_action| None,
        );

        assert!(candidate.exists());
        assert!(!observation.automatic_cleanup_eligible);
        assert!(
            observation
                .reason_ids
                .contains(&"storage_residue.cleanup_lifecycle_blocked".to_owned())
        );
    }

    #[test]
    fn lifecycle_blocked_state_is_not_projected_as_report_only_eligible() {
        let temp = TempDirectory::new();
        let root = clone_root(&temp);
        let candidate = add_clone(&root, "A1b2C3");
        let snapshot = complete_snapshot(Vec::new());
        let mut cleanup = ChromeCloneCleanup::new();
        let _ = cleanup.reconcile_root(
            &root,
            1,
            Some(&snapshot),
            ChromeCloneCleanupMode::LifecycleBlocked,
        );

        let observation = cleanup.reconcile_root(
            &root,
            2,
            Some(&snapshot),
            ChromeCloneCleanupMode::LifecycleBlocked,
        );

        assert!(candidate.exists());
        assert!(!observation.automatic_cleanup_eligible);
        assert!(
            observation
                .reason_ids
                .contains(&"storage_residue.cleanup_lifecycle_blocked".to_owned())
        );
        assert!(
            !observation
                .reason_ids
                .contains(&"storage_residue.cleanup_report_only".to_owned())
        );
    }

    #[test]
    fn enforce_never_deletes_symlinked_or_unexpected_root_shapes() {
        let temp = TempDirectory::new();
        let outside = temp.0.join("outside");
        fs::create_dir(&outside).expect("outside directory");
        fs::write(outside.join("preserve"), b"outside").expect("outside payload");
        let root = clone_root(&temp);
        symlink(&outside, &root).expect("link clone root");
        let snapshot = complete_snapshot(Vec::new());
        let mut cleanup = ChromeCloneCleanup::new();

        for observed_at in [1, 2] {
            let observation = cleanup.reconcile_root(
                &root,
                observed_at,
                Some(&snapshot),
                ChromeCloneCleanupMode::Enforce,
            );
            assert_eq!(observation.status, StorageResidueStatus::Unavailable);
        }
        assert_eq!(fs::read(outside.join("preserve")).unwrap(), b"outside");

        fs::remove_file(&root).expect("remove fixture symlink");
        fs::create_dir(&root).expect("create clone root");
        let unexpected = root.join("surprise-name");
        fs::create_dir(&unexpected).expect("unexpected entry");
        for observed_at in [3, 4] {
            let observation = cleanup.reconcile_root(
                &root,
                observed_at,
                Some(&snapshot),
                ChromeCloneCleanupMode::Enforce,
            );
            assert_eq!(observation.status, StorageResidueStatus::Unavailable);
        }
        assert!(unexpected.exists());
    }

    #[test]
    fn live_chrome_helper_and_incomplete_process_observation_block_cleanup() {
        let blockers = [
            complete_snapshot(vec![chrome_process(
                vec!["Google Chrome"],
                CHROME_BUNDLE_ID,
                "Google Chrome",
            )]),
            complete_snapshot(vec![chrome_process(
                vec!["Google Chrome Helper", "--type=renderer"],
                CHROME_HELPER_BUNDLE_ID_PREFIX,
                "Google Chrome Helper",
            )]),
            complete_snapshot(vec![chrome_process(
                vec![
                    "Google Chrome Helper",
                    CLONE_CLEANUP_TYPE_ARGUMENT,
                    "--unique-temp-dir-suffix=A1b2C3",
                ],
                CHROME_HELPER_BUNDLE_ID_PREFIX,
                "Google Chrome Helper",
            )]),
            complete_snapshot(vec![chrome_process(
                vec!["Google Chrome Helper", CLONE_CLEANUP_TYPE_ARGUMENT],
                CHROME_HELPER_BUNDLE_ID_PREFIX,
                "Google Chrome Helper",
            )]),
            Snapshot {
                observed_at_unix_millis: 1,
                current_uid: unsafe { libc::geteuid() },
                processes: Vec::new(),
                coverage: SnapshotCoverage {
                    listed_processes: 1,
                    unreadable_processes: 1,
                    ..SnapshotCoverage::default()
                },
            },
        ];

        for (index, blocker) in blockers.iter().enumerate() {
            let temp = TempDirectory::new();
            let root = clone_root(&temp);
            let candidate = add_clone(&root, "A1b2C3");
            let mut cleanup = ChromeCloneCleanup::new();
            let _ =
                cleanup.reconcile_root(&root, 1, Some(blocker), ChromeCloneCleanupMode::Enforce);
            let observation =
                cleanup.reconcile_root(&root, 2, Some(blocker), ChromeCloneCleanupMode::Enforce);
            assert!(candidate.exists(), "blocker {index} must preserve fixture");
            assert!(!observation.automatic_cleanup_eligible);
        }
    }

    struct FailingRemover;

    impl CandidateRemover for FailingRemover {
        fn remove(&self, _scan: &CloneScan) -> io::Result<()> {
            Err(io::Error::other("injected fixture failure"))
        }
    }

    #[test]
    fn failed_removal_is_rescanned_and_never_reported_clear() {
        let temp = TempDirectory::new();
        let root = clone_root(&temp);
        let candidate = add_clone(&root, "A1b2C3");
        let snapshot = complete_snapshot(Vec::new());
        let mut cleanup = ChromeCloneCleanup::new();
        cleanup.reconcile_root_with_remover(
            &root,
            1,
            Some(&snapshot),
            ChromeCloneCleanupMode::Enforce,
            &FailingRemover,
        );

        let observation = cleanup.reconcile_root_with_remover(
            &root,
            2,
            Some(&snapshot),
            ChromeCloneCleanupMode::Enforce,
            &FailingRemover,
        );

        assert!(candidate.exists());
        assert_eq!(observation.status, StorageResidueStatus::Detected);
        assert_eq!(observation.candidate_count, 1);
        assert!(observation.automatic_cleanup_eligible);
        assert!(
            observation
                .reason_ids
                .contains(&"storage_residue.automatic_cleanup_failed".to_owned())
        );
    }
}
