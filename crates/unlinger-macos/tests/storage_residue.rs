use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use unlinger_core::{StorageResidueKind, StorageResidueReferenceCheck, StorageResidueStatus};
use unlinger_macos::inspect_code_sign_clone_root;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "unlinger-residue-test-{}-{nonce}-{}",
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

#[test]
fn exact_chrome_code_sign_clones_are_counted_without_mutation() {
    let temp = TempDirectory::new();
    let root = temp.0.join("com.google.Chrome.code_sign_clone");
    for (suffix, bytes) in [("A1b2C3", 11_usize), ("z9Y8x7", 17_usize)] {
        let executable = root
            .join(format!("code_sign_clone.{suffix}"))
            .join("Google Chrome.app/Contents/MacOS/Google Chrome");
        fs::create_dir_all(executable.parent().expect("executable parent"))
            .expect("create clone payload");
        fs::write(&executable, vec![b'x'; bytes]).expect("write clone payload");
    }

    let observation = inspect_code_sign_clone_root(&root, 42);

    assert_eq!(observation.kind, StorageResidueKind::ChromeCodeSignClone);
    assert_eq!(observation.status, StorageResidueStatus::Detected);
    assert_eq!(observation.observed_at_unix_millis, 42);
    assert_eq!(observation.candidate_count, 2);
    assert!(observation.logical_bytes >= 28);
    assert!(observation.shape_complete);
    assert_eq!(
        observation.reference_check,
        StorageResidueReferenceCheck::Incomplete
    );
    assert!(!observation.automatic_cleanup_eligible);
    assert_eq!(
        observation.reason_ids,
        [
            "storage_residue.code_sign_clone_detected",
            "storage_residue.logical_size_not_physical_reclaim",
            "storage_residue.reference_check_incomplete",
        ]
    );
    assert!(root.join("code_sign_clone.A1b2C3").exists());
    assert!(root.join("code_sign_clone.z9Y8x7").exists());
}

#[test]
fn missing_clone_root_is_a_clear_observation() {
    let temp = TempDirectory::new();
    let root = temp.0.join("com.google.Chrome.code_sign_clone");

    let observation = inspect_code_sign_clone_root(&root, 73);

    assert_eq!(observation.status, StorageResidueStatus::Clear);
    assert_eq!(observation.candidate_count, 0);
    assert_eq!(observation.logical_bytes, 0);
    assert!(observation.shape_complete);
    assert!(!observation.automatic_cleanup_eligible);
    assert_eq!(
        observation.reason_ids,
        ["storage_residue.code_sign_clone_absent"]
    );
}

#[test]
fn renamed_chrome_bundle_counts_regular_files_without_following_framework_links() {
    let temp = TempDirectory::new();
    let root = temp.0.join("com.google.Chrome.code_sign_clone");
    let bundle = root.join("code_sign_clone.A1b2C3/Google Chrome.app.bundle");
    let versions = bundle.join("Contents/Frameworks/Chrome.framework/Versions");
    fs::create_dir_all(versions.join("152/Resources")).expect("create framework");
    fs::write(versions.join("152/Resources/payload"), [b'x'; 17]).expect("payload");
    symlink("152", versions.join("Current")).expect("version link");
    symlink(
        "Versions/Current/Resources",
        versions.parent().unwrap().join("Resources"),
    )
    .expect("resource link");
    let outside = temp.0.join("outside");
    fs::create_dir(&outside).expect("outside directory");
    fs::write(outside.join("private-payload"), [b'y'; 101]).expect("outside payload");
    symlink(&outside, bundle.join("outside-link")).expect("outside link");
    symlink("missing", bundle.join("dangling-link")).expect("dangling link");

    let observation = inspect_code_sign_clone_root(&root, 74);

    assert_eq!(observation.status, StorageResidueStatus::Detected);
    assert_eq!(observation.candidate_count, 1);
    assert_eq!(observation.logical_bytes, 17);
    assert!(observation.shape_complete);
    assert!(!observation.automatic_cleanup_eligible);
    assert_eq!(
        observation.reference_check,
        StorageResidueReferenceCheck::Incomplete
    );
    assert_eq!(
        fs::read(outside.join("private-payload")).unwrap(),
        [b'y'; 101]
    );
    assert!(bundle.join("outside-link").is_symlink());
}

#[test]
fn clone_bundle_root_cannot_be_a_symlink() {
    let temp = TempDirectory::new();
    let root = temp.0.join("com.google.Chrome.code_sign_clone");
    let clone = root.join("code_sign_clone.A1b2C3");
    fs::create_dir_all(&clone).expect("clone directory");
    let outside = temp.0.join("outside");
    fs::create_dir(&outside).expect("outside directory");
    symlink(outside, clone.join("Google Chrome.app.bundle")).expect("bundle link");

    let observation = inspect_code_sign_clone_root(&root, 75);
    assert_eq!(observation.status, StorageResidueStatus::Unavailable);
    assert!(!observation.shape_complete);
    assert!(!observation.automatic_cleanup_eligible);
}

#[test]
fn symlinked_or_unexpected_clone_shapes_are_never_cleanup_ready() {
    let temp = TempDirectory::new();
    let target = temp.0.join("target");
    fs::create_dir(&target).expect("create symlink target");
    let linked_root = temp.0.join("com.google.Chrome.code_sign_clone");
    symlink(&target, &linked_root).expect("link clone root");

    let linked = inspect_code_sign_clone_root(&linked_root, 90);
    assert_eq!(linked.status, StorageResidueStatus::Unavailable);
    assert!(!linked.shape_complete);
    assert!(!linked.automatic_cleanup_eligible);

    fs::remove_file(&linked_root).expect("remove test link");
    fs::create_dir(&linked_root).expect("create real clone root");
    fs::create_dir(linked_root.join("surprise-name")).expect("create unexpected entry");
    let unexpected = inspect_code_sign_clone_root(&linked_root, 91);
    assert_eq!(unexpected.status, StorageResidueStatus::Unavailable);
    assert!(!unexpected.shape_complete);
    assert!(!unexpected.automatic_cleanup_eligible);
}
