use std::ffi::{CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use unlinger_core::{
    StorageResidueKind, StorageResidueObservation, StorageResidueReferenceCheck,
    StorageResidueStatus,
};

const CLONE_ROOT_NAME: &str = "com.google.Chrome.code_sign_clone";
const CLONE_PREFIX: &str = "code_sign_clone.";

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
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return StorageResidueObservation {
                kind: StorageResidueKind::ChromeCodeSignClone,
                status: StorageResidueStatus::Clear,
                observed_at_unix_millis,
                candidate_count: 0,
                logical_bytes: 0,
                shape_complete: true,
                reference_check: StorageResidueReferenceCheck::Incomplete,
                automatic_cleanup_eligible: false,
                reason_ids: vec!["storage_residue.code_sign_clone_absent".to_owned()],
            };
        }
        Err(_) => {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            );
        }
    };
    if !metadata.file_type().is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
        return unavailable(observed_at_unix_millis, "storage_residue.clone_root_unsafe");
    }
    let directory = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
    {
        Ok(directory) => directory,
        Err(_) => {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            );
        }
    };
    let entries = match directory_names(&directory) {
        Ok(entries) => entries,
        Err(_) => {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            );
        }
    };
    let mut candidate_count = 0_usize;
    let mut logical_bytes = 0_u64;
    for name in entries {
        let Ok(text_name) = name.to_str() else {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            );
        };
        if !valid_clone_name(text_name) {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            );
        }
        let Ok(clone) = open_child_directory(&directory, &name) else {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            );
        };
        if clone.metadata().map(|metadata| metadata.uid()).ok() != Some(unsafe { libc::geteuid() })
            || ![c"Google Chrome.app.bundle", c"Google Chrome.app"]
                .iter()
                .any(|name| open_child_directory(&clone, name).is_ok())
        {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            );
        }
        let Ok(bytes) = logical_tree_bytes(&clone) else {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_tree_unreadable",
            );
        };
        let Some(total) = logical_bytes.checked_add(bytes) else {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.logical_size_overflow",
            );
        };
        logical_bytes = total;
        candidate_count += 1;
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
    StorageResidueObservation {
        kind: StorageResidueKind::ChromeCodeSignClone,
        status,
        observed_at_unix_millis,
        candidate_count,
        logical_bytes,
        shape_complete: true,
        reference_check: StorageResidueReferenceCheck::Incomplete,
        automatic_cleanup_eligible: false,
        reason_ids,
    }
}

fn valid_clone_name(name: &str) -> bool {
    name.strip_prefix(CLONE_PREFIX).is_some_and(|suffix| {
        suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
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
