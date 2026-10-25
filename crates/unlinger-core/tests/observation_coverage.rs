use unlinger_core::{
    ExecutableIdentity, ProcessIdentity, ProcessRecord, ProcessStatus, Snapshot, SnapshotCoverage,
};

fn snapshot() -> Snapshot {
    Snapshot {
        observed_at_unix_millis: 1_000,
        current_uid: 501,
        processes: vec![ProcessRecord {
            identity: ProcessIdentity {
                pid: 42,
                started_at_unix_micros: 1,
                executable_device: Some(1),
                executable_inode: Some(2),
            },
            parent_pid: 1,
            process_group_id: 42,
            uid: 501,
            tty_device: None,
            name: "synthetic".to_owned(),
            executable_path: Some("/synthetic/tool".to_owned()),
            executable: ExecutableIdentity {
                device: Some(1),
                inode: Some(2),
                ..Default::default()
            },
            arguments: Some(vec!["synthetic".to_owned()]),
            resident_memory_bytes: 0,
            status: ProcessStatus::Sleeping,
            runtime: Default::default(),
        }],
        coverage: SnapshotCoverage {
            listed_processes: 1,
            inspected_processes: 1,
            ..Default::default()
        },
    }
}

#[test]
fn negative_observations_require_readable_classification_not_just_identity() {
    let complete = snapshot();
    assert!(complete.proves_complete_classification_coverage());
    let mut missing = complete.clone();
    missing.processes[0].arguments = None;
    assert!(missing.proves_complete_exact_identity_coverage());
    assert!(!missing.proves_complete_classification_coverage());
    let mut unreadable = complete.clone();
    unreadable.coverage.listed_processes += 1;
    unreadable.coverage.unreadable_processes = 1;
    assert!(!unreadable.proves_complete_classification_coverage());
    let mut inconsistent = complete.clone();
    inconsistent.coverage.arguments_unavailable = 1;
    assert!(!inconsistent.proves_complete_classification_coverage());
    let mut unrelated_socket_unknown = complete;
    unrelated_socket_unknown
        .coverage
        .descriptor_facts_unavailable = 1;
    assert!(unrelated_socket_unknown.proves_complete_classification_coverage());
}

#[test]
fn a_complete_empty_table_and_an_unreadable_empty_table_are_distinct() {
    let mut empty = snapshot();
    empty.processes.clear();
    empty.coverage = SnapshotCoverage::default();
    assert!(empty.proves_complete_classification_coverage());
    empty.coverage.listed_processes = 1;
    empty.coverage.unreadable_processes = 1;
    assert!(!empty.proves_complete_classification_coverage());
}

#[test]
fn coverage_categories_are_additive_without_strengthening_or_weakening_proofs() {
    let complete = snapshot();
    assert!(complete.proves_complete_exact_identity_coverage());
    assert!(complete.proves_complete_classification_coverage());

    let mut missing_path = complete;
    missing_path.processes[0].executable_path = None;
    missing_path.processes[0].executable = ExecutableIdentity::default();
    missing_path.processes[0].identity.executable_device = None;
    missing_path.processes[0].identity.executable_inode = None;
    missing_path.coverage.executable_path_unavailable = 1;
    missing_path.coverage.executable_path_missing_without_errno = 1;
    missing_path.coverage.executable_identity_unavailable = 1;

    assert!(!missing_path.proves_complete_exact_identity_coverage());
    assert!(!missing_path.proves_complete_classification_coverage());

    let fixture: SnapshotCoverage = serde_json::from_str(
        r#"{"listed_processes":1,"inspected_processes":1,"unreadable_processes":0,"arguments_unavailable":0,"executable_identity_unavailable":0,"descriptor_facts_unavailable":0}"#,
    )
    .expect("missing additive counters default to zero");
    assert_eq!(fixture.executable_path_unavailable, 0);
    assert_eq!(fixture.executable_path_missing_without_errno, 0);
    assert_eq!(fixture.executable_vnode_unavailable, 0);
}
