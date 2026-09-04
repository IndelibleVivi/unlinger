use std::fs;
use std::os::unix::fs::MetadataExt;
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
    let entries = match fs::read_dir(root) {
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
    for entry in entries {
        let Ok(entry) = entry else {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            );
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            );
        };
        if !valid_clone_name(name) {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            );
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_root_unreadable",
            );
        };
        if !metadata.file_type().is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || !path.join("Google Chrome.app").is_dir()
        {
            return unavailable(
                observed_at_unix_millis,
                "storage_residue.clone_shape_unexpected",
            );
        }
        let Ok(bytes) = logical_tree_bytes(&path) else {
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

fn logical_tree_bytes(path: &Path) -> Result<u64, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(std::io::Error::other("clone tree contains a symlink"));
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Err(std::io::Error::other(
            "clone tree contains an unsupported node",
        ));
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        let bytes = logical_tree_bytes(&entry?.path())?;
        total = total
            .checked_add(bytes)
            .ok_or_else(|| std::io::Error::other("logical size overflow"))?;
    }
    Ok(total)
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
