#![cfg(target_os = "macos")]

use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use unlinger_core::{
    ArtifactAction, ArtifactDisposition, ArtifactFreeze, CleanupAction, CleanupReceipt,
    CleanupRuntime, CleanupSignal, CleanupStage, IncidentState, ProcessGraph, ProcessIdentity,
    ProcessRecord, RuntimeArtifactCandidate, RuntimeArtifactIdentity, RuntimeArtifactKind,
    SignalDisposition, Snapshot, fingerprint_process_identity,
};
use unlinger_daemon::{
    EventKind, EventPayload, HistoryEvent, IncidentDetail, IpcClient, IpcCommand, IpcError,
    IpcPayload, LAUNCH_AGENT_LABEL,
};
use unlinger_macos::MacosRuntime;
use unlinger_rules::{Analyzer, AnalyzerContext, RuleSet};

const CFT_APP_ENV: &str = "UNLINGER_FIELDLAB_CFT_APP";
const MANAGED_CLI_ENV: &str = "UNLINGER_FIELDLAB_MANAGED_CLI";
const FULL_TIMING_ENV: &str = "UNLINGER_FIELDLAB_FULL_TIMING";
const MANAGED_ACK_ENV: &str = "UNLINGER_FIELDLAB_MANAGED_ACK";
const MANAGED_ACK: &str = "I_ACCEPT_INSTALLED_CFT_SIGNALING";
const ORDINARY_CHROME_EXECUTABLE: &str =
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const RECEIPT_TIMEOUT: Duration = Duration::from_secs(8 * 60);
const SERVICE_TIMEOUT: Duration = Duration::from_secs(150);
const POST_RESTART_OBSERVATION: Duration = Duration::from_secs(65);
const CANDIDATE_TIMEOUT: Duration = Duration::from_secs(75);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const RECEIPT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const RECEIPT_POLL_IO_TIMEOUT: Duration = Duration::from_secs(3);

#[test]
fn managed_acknowledgement_is_exact_and_full_timing_is_mandatory() {
    assert!(validate_ack(None).is_err());
    assert!(validate_ack(Some(OsStr::new("yes"))).is_err());
    assert!(validate_ack(Some(OsStr::new(MANAGED_ACK))).is_ok());
    assert!(validate_full_timing(None).is_err());
    assert!(validate_full_timing(Some(OsStr::new("true"))).is_err());
    assert!(validate_full_timing(Some(OsStr::new("1"))).is_ok());
}

#[test]
fn managed_status_requires_generation_bound_readiness() {
    let report = serde_json::json!({
        "installed": true,
        "loaded": true,
        "healthy": true,
        "expected_mode": "enforce",
        "launchd_pid": 4321,
        "pid_matches": true,
        "permissions_ok": true,
        "active_generation": 7,
        "generation_matches": true,
        "binary_matches": true,
        "daemon_path": "/Users/example/Library/Application Support/Unlinger/generations/7/unlingerd",
        "cli_path": "/Users/example/Library/Application Support/Unlinger/generations/7/unlinger",
        "database_path": "/Users/example/Library/Application Support/Unlinger/history.sqlite3",
        "socket_path": "/Users/example/Library/Application Support/Unlinger/run/unlingerd.sock",
        "daemon_status": {
            "managed": true,
            "instance_id": "instance-7-b",
            "healthy": true,
            "ready": true,
            "startup_state": "ready_enforce",
            "requested_mode": "enforce",
            "effective_mode": "enforce",
            "activation_generation": 7,
            "armed_generation": 7,
            "enforcement_epoch": "epoch-7-b",
            "draining": false,
            "pid": 4321,
            "last_scan_at_unix_millis": 42
        }
    });

    let parsed = ManagedServiceSnapshot::parse(&report).expect("managed service report");
    parsed.require_ready_enforce().expect("armed generation");

    let mut stale = report;
    stale["daemon_status"]["armed_generation"] = Value::from(6_u64);
    assert!(
        ManagedServiceSnapshot::parse(&stale)
            .and_then(|status| status.require_ready_enforce())
            .is_err()
    );
}

#[test]
fn journal_contract_requires_ordered_term_kill_and_full_revival() {
    let baseline_artifact = BaselineArtifact {
        kind: RuntimeArtifactKind::DevToolsActivePort,
        artifact_fingerprint: "art-field-baseline".to_owned(),
        identity: RuntimeArtifactIdentity {
            device: 1,
            inode: 2,
            owner_uid: 501,
            mode: 0o100600,
            link_count: 1,
            parent_device: 1,
            parent_inode: 3,
            parent_owner_uid: 501,
            parent_mode: 0o40700,
        },
    };
    let receipt_artifacts = vec![ArtifactAction {
        kind: RuntimeArtifactKind::DevToolsActivePort,
        artifact_fingerprint: baseline_artifact.artifact_fingerprint.clone(),
        disposition: ArtifactDisposition::Removed,
    }];
    let exact_root = ProcessIdentity {
        pid: 900,
        started_at_unix_micros: 1_000,
        executable_device: Some(1),
        executable_inode: Some(2),
    };
    let exact_root_fingerprint = fingerprint_process_identity(&exact_root);
    let receipt_actions = vec![
        CleanupAction {
            stage: CleanupStage::PrimaryTerm,
            pid: exact_root.pid,
            identity_fingerprint: exact_root_fingerprint.clone(),
            signal: CleanupSignal::Term,
            disposition: SignalDisposition::Delivered,
        },
        CleanupAction {
            stage: CleanupStage::ExactKill,
            pid: exact_root.pid,
            identity_fingerprint: exact_root_fingerprint.clone(),
            signal: CleanupSignal::Kill,
            disposition: SignalDisposition::Delivered,
        },
    ];
    let journal = JournalSnapshot {
        attempts: vec![JournalAttempt {
            id: 4,
            enforcement_epoch: "epoch-4".to_owned(),
            started_at_ms: 100_000,
            completed_at_ms: Some(191_000),
            terminal_state: Some("CLEARED".to_owned()),
            reason_id: None,
            revival_checks_completed: Some(2),
        }],
        actions: vec![
            JournalAction {
                id: 10,
                attempt_id: 4,
                sequence: 0,
                prepared_at_ms: 115_000,
                completed_at_ms: Some(115_001),
                stage: "primary_term".to_owned(),
                pid: 900,
                identity_fingerprint: exact_root_fingerprint.clone(),
                signal: "term".to_owned(),
                disposition: Some("delivered".to_owned()),
            },
            JournalAction {
                id: 11,
                attempt_id: 4,
                sequence: 1,
                prepared_at_ms: 116_000,
                completed_at_ms: Some(116_001),
                stage: "exact_kill".to_owned(),
                pid: 900,
                identity_fingerprint: exact_root_fingerprint.clone(),
                signal: "kill".to_owned(),
                disposition: Some("delivered".to_owned()),
            },
        ],
        artifact_actions: vec![JournalArtifactAction {
            id: 12,
            attempt_id: 4,
            sequence: 0,
            prepared_at_ms: 190_250,
            completed_at_ms: Some(190_251),
            kind: "dev_tools_active_port".to_owned(),
            artifact_fingerprint: baseline_artifact.artifact_fingerprint.clone(),
            disposition: Some("removed".to_owned()),
        }],
        terminal_events: 1,
    };

    journal
        .verify_terminal_contract(TerminalContract {
            root_pid: 900,
            root_fingerprint: &exact_root_fingerprint,
            epoch: "epoch-4",
            arm_command_started_at: 10_000,
            exact_tree_absent_at_ms: 116_000,
            baseline_artifact: &baseline_artifact,
            exact_tree: std::slice::from_ref(&exact_root),
            receipt_actions: &receipt_actions,
            receipt_artifact_actions: &receipt_artifacts,
        })
        .expect("full terminal contract");

    let mut duplicate = journal.clone();
    duplicate.actions.push(duplicate.actions[0].clone());
    assert!(
        duplicate
            .verify_terminal_contract(TerminalContract {
                root_pid: 900,
                root_fingerprint: &exact_root_fingerprint,
                epoch: "epoch-4",
                arm_command_started_at: 10_000,
                exact_tree_absent_at_ms: 116_000,
                baseline_artifact: &baseline_artifact,
                exact_tree: std::slice::from_ref(&exact_root),
                receipt_actions: &receipt_actions,
                receipt_artifact_actions: &receipt_artifacts,
            })
            .is_err()
    );

    let mismatched_receipt = vec![ArtifactAction {
        kind: RuntimeArtifactKind::DevToolsActivePort,
        artifact_fingerprint: baseline_artifact.artifact_fingerprint.clone(),
        disposition: ArtifactDisposition::AlreadyAbsent,
    }];
    assert!(
        journal
            .verify_terminal_contract(TerminalContract {
                root_pid: 900,
                root_fingerprint: &exact_root_fingerprint,
                epoch: "epoch-4",
                arm_command_started_at: 10_000,
                exact_tree_absent_at_ms: 116_000,
                baseline_artifact: &baseline_artifact,
                exact_tree: std::slice::from_ref(&exact_root),
                receipt_actions: &receipt_actions,
                receipt_artifact_actions: &mismatched_receipt,
            })
            .is_err()
    );
}

#[test]
#[ignore = "explicit owner-only fieldlab: controls the installed managed LaunchAgent and signals only its unique Chrome-for-Testing profile"]
fn installed_generation_reclaims_stopped_cft_once_and_restarts_without_resend() {
    run_managed_fieldlab().expect("managed Chrome for Testing fieldlab");
}

#[test]
fn ordinary_chrome_noninterference_allows_new_roots_but_rejects_baseline_changes() {
    let baseline_root = ProcessIdentity {
        pid: 100,
        started_at_unix_micros: 1_000,
        executable_device: Some(1),
        executable_inode: Some(2),
    };
    let new_root = ProcessIdentity {
        pid: 200,
        started_at_unix_micros: 2_000,
        executable_device: Some(1),
        executable_inode: Some(2),
    };
    let baseline = OrdinaryChromeTree {
        roots: BTreeMap::from([(baseline_root.pid, baseline_root.clone())]),
        identities: BTreeMap::from([(baseline_root.pid, baseline_root.clone())]),
    };
    let with_new_root = OrdinaryChromeTree {
        roots: BTreeMap::from([
            (baseline_root.pid, baseline_root.clone()),
            (new_root.pid, new_root.clone()),
        ]),
        identities: BTreeMap::from([
            (baseline_root.pid, baseline_root.clone()),
            (new_root.pid, new_root),
        ]),
    };
    assert!(
        baseline
            .missing_or_changed_preexisting_roots(&with_new_root)
            .is_empty(),
        "an unrelated browser opened during the field run is not interference"
    );

    let changed_identity = ProcessIdentity {
        started_at_unix_micros: baseline_root.started_at_unix_micros + 1,
        ..baseline_root.clone()
    };
    let changed_baseline = OrdinaryChromeTree {
        roots: BTreeMap::from([(baseline_root.pid, changed_identity.clone())]),
        identities: BTreeMap::from([(baseline_root.pid, changed_identity)]),
    };
    assert_eq!(
        baseline.missing_or_changed_preexisting_roots(&changed_baseline),
        vec![baseline_root.pid]
    );
    assert_eq!(
        baseline.missing_or_changed_preexisting_roots(&OrdinaryChromeTree {
            roots: BTreeMap::new(),
            identities: BTreeMap::new(),
        }),
        vec![baseline_root.pid]
    );
}

#[test]
fn terminal_receipt_targets_only_frozen_field_identities() {
    let root = ProcessIdentity {
        pid: 100,
        started_at_unix_micros: 1_000,
        executable_device: Some(1),
        executable_inode: Some(2),
    };
    let root_fingerprint = fingerprint_process_identity(&root);
    let mut receipt = CleanupReceipt {
        incident_id: "inc-field".to_owned(),
        state: IncidentState::Cleared,
        reason_id: None,
        actions: vec![
            CleanupAction {
                stage: CleanupStage::PrimaryTerm,
                pid: root.pid,
                identity_fingerprint: root_fingerprint.clone(),
                signal: CleanupSignal::Term,
                disposition: SignalDisposition::Delivered,
            },
            CleanupAction {
                stage: CleanupStage::ExactKill,
                pid: root.pid,
                identity_fingerprint: root_fingerprint.clone(),
                signal: CleanupSignal::Kill,
                disposition: SignalDisposition::Delivered,
            },
        ],
        artifact_actions: Vec::new(),
        survivor_pids: Vec::new(),
        revival_checks_completed: 2,
        resources: Default::default(),
    };
    verify_terminal_receipt(
        &receipt,
        &root,
        &root_fingerprint,
        std::slice::from_ref(&root),
    )
    .expect("exact frozen field actions");

    receipt.actions.push(CleanupAction {
        stage: CleanupStage::MemberTerm,
        pid: 200,
        identity_fingerprint: "outside-field-tree".to_owned(),
        signal: CleanupSignal::Term,
        disposition: SignalDisposition::Delivered,
    });
    assert!(
        verify_terminal_receipt(
            &receipt,
            &root,
            &root_fingerprint,
            std::slice::from_ref(&root),
        )
        .is_err()
    );
}

#[test]
fn exact_incident_poll_ignores_unrelated_history() {
    let unrelated = terminal_history_event("inc-unrelated", 1);
    let target = terminal_history_event("inc-target", 2);
    let response = IpcPayload::Incident(IncidentDetail {
        incident_id: "inc-target".to_owned(),
        events: vec![unrelated, target],
    });

    let ReceiptPollOutcome::Terminal(receipt) =
        classify_explain_response(Ok(response), "inc-target")
    else {
        panic!("exact Explain did not return the target terminal receipt");
    };
    assert_eq!(receipt.incident_id, "inc-target");
}

#[test]
fn explain_poll_retries_only_read_only_transients() {
    assert!(matches!(
        classify_explain_response(
            Err(IpcError::Remote {
                code: "not_found".to_owned(),
                message: "not recorded yet".to_owned(),
            }),
            "inc-target",
        ),
        ReceiptPollOutcome::Pending
    ));
    assert!(matches!(
        classify_explain_response(
            Err(IpcError::Remote {
                code: "unavailable".to_owned(),
                message: "busy".to_owned(),
            }),
            "inc-target",
        ),
        ReceiptPollOutcome::Retryable(_)
    ));
    assert!(matches!(
        classify_explain_response(
            Err(IpcError::Io(io::Error::from(io::ErrorKind::WouldBlock))),
            "inc-target",
        ),
        ReceiptPollOutcome::Retryable(_)
    ));

    let malformed_json = serde_json::from_str::<Value>("{").expect_err("malformed JSON");
    for fatal in [
        classify_explain_response(
            Err(IpcError::Io(io::Error::from(
                io::ErrorKind::PermissionDenied,
            ))),
            "inc-target",
        ),
        classify_explain_response(
            Err(IpcError::Protocol("wrong request ID".to_owned())),
            "inc-target",
        ),
        classify_explain_response(Err(IpcError::Json(malformed_json)), "inc-target"),
        classify_explain_response(Ok(IpcPayload::Resumed), "inc-target"),
    ] {
        assert!(matches!(fatal, ReceiptPollOutcome::Fatal(_)));
    }
}

#[test]
fn slow_receipt_worker_does_not_block_absence_tracking() {
    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        started_sender.send(()).expect("announce slow poll");
        release_receiver.recv().expect("release slow poll");
    });
    started_receiver.recv().expect("slow poll started");

    let mut absence = ExactTreeAbsenceTracker::default();
    absence.observe(false, 1_000).expect("first absence");
    absence.observe(false, 1_250).expect("second absence");
    assert_eq!(absence.exact_tree_absent_at_ms, Some(1_250));

    release_sender.send(()).expect("release worker");
    worker.join().expect("join worker");
}

fn terminal_history_event(incident_id: &str, event_id: i64) -> HistoryEvent {
    HistoryEvent {
        event_id,
        attempt_id: Some(event_id),
        incident_id: incident_id.to_owned(),
        occurred_at_unix_millis: u64::try_from(event_id).expect("positive event ID"),
        kind: EventKind::Cleanup,
        state: IncidentState::Cleared,
        payload: EventPayload::Cleanup {
            receipt: CleanupReceipt {
                incident_id: incident_id.to_owned(),
                state: IncidentState::Cleared,
                reason_id: None,
                actions: Vec::new(),
                artifact_actions: Vec::new(),
                survivor_pids: Vec::new(),
                revival_checks_completed: 2,
                resources: Default::default(),
            },
        },
    }
}

fn run_managed_fieldlab() -> Result<(), Box<dyn Error>> {
    let config = ManagedFieldConfig::from_environment()?;
    let profile = create_profile()?;
    let _field_session = FieldSession::new(profile.clone());

    let report_only = set_mode(&config.cli, "report-only")?;
    report_only.require_ready_report_only()?;
    if fs::canonicalize(&report_only.cli_path)? != config.cli {
        return Err(field_error(format!(
            "{MANAGED_CLI_ENV} does not name the active generation CLI: configured={}, active={}",
            config.cli.display(),
            report_only.cli_path.display()
        )));
    }
    let mut service_guard = ManagedServiceGuard::new(config.cli.clone());
    write_artifact(&profile, "01-report-only.json", &report_only)?;

    let mut snapshotter = MacosRuntime::new();
    let ordinary_before = OrdinaryChromeTree::capture(&snapshotter.snapshot()?)?;
    assert_no_cleanup_candidates(&snapshotter.snapshot()?)?;

    launch_chrome_for_testing(&config.app, &profile)?;
    let (_, root_identity) = wait_for_detached_root(&profile)?;
    let target = wait_for_unique_field_candidate(&profile, &root_identity)?;
    let artifact_candidate = unique_field_artifact(&target, &profile)?;
    let baseline_artifact = wait_for_baseline_artifact(&mut snapshotter, &artifact_candidate)?;
    let field_tree = target
        .targets
        .iter()
        .map(|target| target.identity.clone())
        .collect::<Vec<_>>();
    let incident_id = target.incident_id.clone();
    let root_fingerprint = target.root.identity_fingerprint.clone();
    let baseline_journal = JournalSnapshot::capture(&report_only.database_path, &incident_id)?;
    if !baseline_journal.attempts.is_empty()
        || !baseline_journal.actions.is_empty()
        || !baseline_journal.artifact_actions.is_empty()
    {
        return Err(field_error(
            "unique field incident unexpectedly had pre-existing cleanup journal rows",
        ));
    }

    stop_exact_field_root(&profile, &root_identity)?;
    require_exact_baseline_artifact(&mut snapshotter, &artifact_candidate, &baseline_artifact)?;
    unique_field_candidate(&snapshotter.snapshot()?, &root_identity)?;
    let arm_command_started_at = now_unix_millis()?;
    service_guard.armed = true;
    let armed = set_mode(&config.cli, "enforce")?;
    armed.require_ready_enforce()?;
    if armed.active_generation != report_only.active_generation {
        return Err(field_error("arming changed the installed generation"));
    }
    if armed.daemon.instance_id != report_only.daemon.instance_id {
        return Err(field_error(
            "arming restarted the daemon instead of arming the ready instance",
        ));
    }
    let enforcement_epoch = armed
        .daemon
        .enforcement_epoch
        .clone()
        .ok_or_else(|| field_error("armed daemon omitted its enforcement epoch"))?;
    write_artifact(&profile, "02-armed.json", &armed)?;

    let terminal = wait_for_terminal_receipt(&armed.socket_path, &incident_id, &field_tree)?;
    let receipt = terminal.receipt;
    verify_terminal_receipt(&receipt, &root_identity, &root_fingerprint, &field_tree)?;
    let terminal_journal = JournalSnapshot::capture(&armed.database_path, &incident_id)?;
    terminal_journal.verify_terminal_contract(TerminalContract {
        root_pid: root_identity.pid,
        root_fingerprint: &root_fingerprint,
        epoch: &enforcement_epoch,
        arm_command_started_at,
        exact_tree_absent_at_ms: terminal.exact_tree_absent_at_ms,
        baseline_artifact: &baseline_artifact,
        exact_tree: &field_tree,
        receipt_actions: &receipt.actions,
        receipt_artifact_actions: &receipt.artifact_actions,
    })?;
    require_canonical_artifact_absent(&artifact_candidate)?;
    write_artifact(&profile, "03-terminal-receipt.json", &receipt)?;
    write_artifact(&profile, "04-terminal-journal.json", &terminal_journal)?;

    let before_restart_identity = daemon_identity(&armed)?;
    kickstart_managed_launch_agent()?;
    let (restarted, restart_trace) = wait_for_rearmed_restart(&config.cli, &armed)?;
    if !restart_trace.transition_observed {
        return Err(field_error(
            "restart never exposed an unavailable/recovery/report-only transition before re-arm",
        ));
    }
    restarted.require_ready_enforce()?;
    if restarted.active_generation != armed.active_generation {
        return Err(field_error("restart changed the active generation"));
    }
    if restarted.daemon.instance_id == armed.daemon.instance_id {
        return Err(field_error("restart reused the prior daemon instance ID"));
    }
    if restarted.daemon.enforcement_epoch == armed.daemon.enforcement_epoch {
        return Err(field_error("restart reused the prior enforcement epoch"));
    }
    let after_restart_identity = daemon_identity(&restarted)?;
    if before_restart_identity.exact_match(&after_restart_identity) {
        return Err(field_error(
            "restart did not replace the exact daemon identity",
        ));
    }
    write_artifact(&profile, "05-restart-trace.json", &restart_trace)?;
    write_artifact(&profile, "06-restarted.json", &restarted)?;

    thread::sleep(POST_RESTART_OBSERVATION);
    let after_restart_journal = JournalSnapshot::capture(&restarted.database_path, &incident_id)?;
    if after_restart_journal != terminal_journal {
        return Err(field_error(
            "same-generation restart duplicated or mutated terminal attempt/action rows",
        ));
    }
    let receipt_after_restart =
        match poll_exact_terminal_receipt(&restarted.socket_path, &incident_id) {
            ReceiptPollOutcome::Terminal(receipt) => receipt,
            ReceiptPollOutcome::Pending => {
                return Err(field_error("terminal receipt disappeared after restart"));
            }
            ReceiptPollOutcome::Retryable(error) => return Err(field_error(error)),
            ReceiptPollOutcome::Fatal(error) => return Err(field_error(error)),
        };
    if receipt_after_restart != receipt {
        return Err(field_error(
            "terminal receipt changed after same-generation restart",
        ));
    }

    let disarmed = set_mode(&config.cli, "report-only")?;
    disarmed.require_ready_report_only()?;
    if disarmed.active_generation != restarted.active_generation
        || disarmed.daemon.instance_id != restarted.daemon.instance_id
    {
        return Err(field_error(
            "teardown disarm changed generation or restarted the managed instance",
        ));
    }
    service_guard.armed = false;
    write_artifact(&profile, "07-disarmed.json", &disarmed)?;

    let postflight = snapshotter.snapshot()?;
    if find_field_root(&postflight, &profile).is_some() {
        return Err(field_error(
            "field Chrome for Testing root survived its terminal receipt",
        ));
    }
    require_canonical_artifact_absent(&artifact_candidate)?;
    ordinary_before.assert_preexisting_preserved(&postflight)?;
    write_artifact(
        &profile,
        "08-ordinary-chrome-proof.json",
        &ordinary_before.proof(&postflight)?,
    )?;

    println!(
        "managed fieldlab passed: generation={} initial_instance={} restarted_instance={} incident={}",
        armed.active_generation,
        armed.daemon.instance_id,
        restarted.daemon.instance_id,
        incident_id
    );
    Ok(())
}

struct ManagedFieldConfig {
    app: PathBuf,
    cli: PathBuf,
}

impl ManagedFieldConfig {
    fn from_environment() -> Result<Self, Box<dyn Error>> {
        validate_ack(env::var_os(MANAGED_ACK_ENV).as_deref())?;
        validate_full_timing(env::var_os(FULL_TIMING_ENV).as_deref())?;
        Ok(Self {
            app: fieldlab_app()?,
            cli: canonical_executable(MANAGED_CLI_ENV)?,
        })
    }
}

fn validate_ack(value: Option<&OsStr>) -> Result<(), Box<dyn Error>> {
    if value != Some(OsStr::new(MANAGED_ACK)) {
        return Err(field_error(format!(
            "{MANAGED_ACK_ENV} must equal {MANAGED_ACK:?}; this test controls the installed service and sends exact signals to its unique CfT tree"
        )));
    }
    Ok(())
}

fn validate_full_timing(value: Option<&OsStr>) -> Result<(), Box<dyn Error>> {
    if value != Some(OsStr::new("1")) {
        return Err(field_error(format!(
            "{FULL_TIMING_ENV}=1 is mandatory; managed acceptance never substitutes fast timings"
        )));
    }
    Ok(())
}

fn canonical_executable(variable: &str) -> Result<PathBuf, Box<dyn Error>> {
    let configured = env::var_os(variable)
        .ok_or_else(|| field_error(format!("{variable} must name the installed unlinger CLI")))?;
    let path = fs::canonicalize(configured)?;
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(field_error(format!(
            "{variable} is not an executable regular file: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn fieldlab_app() -> Result<PathBuf, Box<dyn Error>> {
    let configured = env::var_os(CFT_APP_ENV).ok_or_else(|| {
        field_error(format!(
            "{CFT_APP_ENV} must name a Google Chrome for Testing.app bundle"
        ))
    })?;
    let app = fs::canonicalize(configured)?;
    if app.file_name() != Some(OsStr::new("Google Chrome for Testing.app")) {
        return Err(field_error(format!(
            "refusing non-CfT app bundle {}",
            app.display()
        )));
    }
    let executable = fs::canonicalize(app.join("Contents/MacOS/Google Chrome for Testing"))?;
    let ordinary = fs::canonicalize(ORDINARY_CHROME_EXECUTABLE)
        .unwrap_or_else(|_| PathBuf::from(ORDINARY_CHROME_EXECUTABLE));
    if executable == ordinary || app == Path::new("/Applications/Google Chrome.app") {
        return Err(field_error("refusing ordinary Google Chrome"));
    }
    Ok(app)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ManagedServiceSnapshot {
    active_generation: u64,
    generation_matches: bool,
    binary_matches: bool,
    daemon_path: PathBuf,
    cli_path: PathBuf,
    database_path: PathBuf,
    socket_path: PathBuf,
    launchd_pid: u32,
    expected_mode: String,
    daemon: ManagedDaemonSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ManagedDaemonSnapshot {
    managed: bool,
    instance_id: String,
    healthy: bool,
    ready: bool,
    startup_state: String,
    requested_mode: String,
    effective_mode: String,
    activation_generation: u64,
    armed_generation: Option<u64>,
    enforcement_epoch: Option<String>,
    draining: bool,
    pid: u32,
    last_scan_at_unix_millis: Option<u64>,
}

impl ManagedServiceSnapshot {
    fn parse(value: &Value) -> Result<Self, Box<dyn Error>> {
        for field in [
            "installed",
            "loaded",
            "healthy",
            "pid_matches",
            "permissions_ok",
            "generation_matches",
            "binary_matches",
        ] {
            if !required_bool(value, field)? {
                return Err(field_error(format!(
                    "managed service report requires {field}=true"
                )));
            }
        }
        let daemon = required_object(value, "daemon_status")?;
        Ok(Self {
            active_generation: required_u64(value, "active_generation")?,
            generation_matches: required_bool(value, "generation_matches")?,
            binary_matches: required_bool(value, "binary_matches")?,
            daemon_path: PathBuf::from(required_string(value, "daemon_path")?),
            cli_path: PathBuf::from(required_string(value, "cli_path")?),
            database_path: PathBuf::from(required_string(value, "database_path")?),
            socket_path: PathBuf::from(required_string(value, "socket_path")?),
            launchd_pid: required_u32(value, "launchd_pid")?,
            expected_mode: required_string(value, "expected_mode")?.to_owned(),
            daemon: ManagedDaemonSnapshot {
                managed: required_bool(daemon, "managed")?,
                instance_id: required_string(daemon, "instance_id")?.to_owned(),
                healthy: required_bool(daemon, "healthy")?,
                ready: required_bool(daemon, "ready")?,
                startup_state: required_string(daemon, "startup_state")?.to_owned(),
                requested_mode: required_string(daemon, "requested_mode")?.to_owned(),
                effective_mode: required_string(daemon, "effective_mode")?.to_owned(),
                activation_generation: required_u64(daemon, "activation_generation")?,
                armed_generation: optional_u64(daemon, "armed_generation")?,
                enforcement_epoch: optional_string(daemon, "enforcement_epoch")?,
                draining: required_bool(daemon, "draining")?,
                pid: required_u32(daemon, "pid")?,
                last_scan_at_unix_millis: optional_u64(daemon, "last_scan_at_unix_millis")?,
            },
        })
    }

    fn require_common_ready(&self) -> Result<(), Box<dyn Error>> {
        if !self.generation_matches
            || !self.binary_matches
            || !self.daemon.managed
            || !self.daemon.healthy
            || !self.daemon.ready
            || self.daemon.draining
            || self.daemon.last_scan_at_unix_millis.is_none()
            || self.launchd_pid != self.daemon.pid
            || self.active_generation != self.daemon.activation_generation
        {
            return Err(field_error(format!(
                "service is not a ready exact managed generation: {self:?}"
            )));
        }
        let expected_suffix = Path::new("generations")
            .join(self.active_generation.to_string())
            .join("unlingerd");
        if !self.daemon_path.ends_with(&expected_suffix) {
            return Err(field_error(format!(
                "active daemon path does not end in {}: {}",
                expected_suffix.display(),
                self.daemon_path.display()
            )));
        }
        Ok(())
    }

    fn require_ready_report_only(&self) -> Result<(), Box<dyn Error>> {
        self.require_common_ready()?;
        if self.expected_mode != "report_only"
            || self.daemon.requested_mode != "report_only"
            || self.daemon.effective_mode != "report_only"
            || self.daemon.startup_state != "ready_report_only"
            || self.daemon.armed_generation.is_some()
            || self.daemon.enforcement_epoch.is_some()
        {
            return Err(field_error(format!(
                "service is not durably disarmed report-only: {self:?}"
            )));
        }
        Ok(())
    }

    fn require_ready_enforce(&self) -> Result<(), Box<dyn Error>> {
        self.require_common_ready()?;
        if self.expected_mode != "enforce"
            || self.daemon.requested_mode != "enforce"
            || self.daemon.effective_mode != "enforce"
            || self.daemon.startup_state != "ready_enforce"
            || self.daemon.armed_generation != Some(self.active_generation)
            || self
                .daemon
                .enforcement_epoch
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(field_error(format!(
                "service is not armed to its exact ready generation: {self:?}"
            )));
        }
        Ok(())
    }
}

fn required_object<'a>(value: &'a Value, field: &str) -> Result<&'a Value, Box<dyn Error>> {
    value
        .get(field)
        .filter(|value| value.is_object())
        .ok_or_else(|| field_error(format!("service JSON omitted object field {field:?}")))
}

fn required_bool(value: &Value, field: &str) -> Result<bool, Box<dyn Error>> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| field_error(format!("service JSON omitted boolean field {field:?}")))
}

fn required_u64(value: &Value, field: &str) -> Result<u64, Box<dyn Error>> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| field_error(format!("service JSON omitted integer field {field:?}")))
}

fn required_u32(value: &Value, field: &str) -> Result<u32, Box<dyn Error>> {
    u32::try_from(required_u64(value, field)?)
        .map_err(|_| field_error(format!("service JSON field {field:?} overflowed u32")))
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, Box<dyn Error>> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| field_error(format!("service JSON omitted string field {field:?}")))
}

fn optional_u64(value: &Value, field: &str) -> Result<Option<u64>, Box<dyn Error>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| field_error(format!("service JSON field {field:?} is not an integer"))),
    }
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>, Box<dyn Error>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| field_error(format!("service JSON field {field:?} is not a string"))),
    }
}

fn set_mode(cli: &Path, mode: &str) -> Result<ManagedServiceSnapshot, Box<dyn Error>> {
    ManagedServiceSnapshot::parse(&run_cli_json(cli, ["service", "set-mode", mode, "--json"])?)
}

fn run_cli_json<const N: usize>(cli: &Path, arguments: [&str; N]) -> Result<Value, Box<dyn Error>> {
    let output = Command::new(cli).args(arguments).output()?;
    if !output.status.success() {
        return Err(field_error(format!(
            "{} failed with {}: {}",
            cli.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

struct TerminalObservation {
    receipt: CleanupReceipt,
    exact_tree_absent_at_ms: u64,
}

#[derive(Default)]
struct ExactTreeAbsenceTracker {
    consecutive_absent_snapshots: u8,
    exact_tree_absent_at_ms: Option<u64>,
}

impl ExactTreeAbsenceTracker {
    fn observe(
        &mut self,
        exact_member_present: bool,
        observed_at_unix_millis: u64,
    ) -> Result<(), Box<dyn Error>> {
        if exact_member_present {
            if self.exact_tree_absent_at_ms.is_some() {
                return Err(field_error(
                    "an exact frozen field-tree member reappeared after confirmed tree absence",
                ));
            }
            self.consecutive_absent_snapshots = 0;
        } else {
            self.consecutive_absent_snapshots = self.consecutive_absent_snapshots.saturating_add(1);
            if self.consecutive_absent_snapshots >= 2 && self.exact_tree_absent_at_ms.is_none() {
                self.exact_tree_absent_at_ms = Some(observed_at_unix_millis);
            }
        }
        Ok(())
    }
}

enum ReceiptPollOutcome {
    Pending,
    Terminal(CleanupReceipt),
    Retryable(String),
    Fatal(String),
}

enum ReceiptPollMessage {
    Terminal(CleanupReceipt),
    Retryable(String),
    Fatal(String),
}

struct ReceiptPollWorker {
    messages: Receiver<ReceiptPollMessage>,
    stop: Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ReceiptPollWorker {
    fn start(socket: PathBuf, incident_id: String) -> Result<Self, Box<dyn Error>> {
        let (message_sender, messages) = mpsc::channel();
        let (stop, stop_receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("unlinger-field-receipt".to_owned())
            .spawn(move || {
                loop {
                    match poll_exact_terminal_receipt(&socket, &incident_id) {
                        ReceiptPollOutcome::Pending => {}
                        ReceiptPollOutcome::Terminal(receipt) => {
                            let _ = message_sender.send(ReceiptPollMessage::Terminal(receipt));
                            return;
                        }
                        ReceiptPollOutcome::Retryable(error) => {
                            if message_sender
                                .send(ReceiptPollMessage::Retryable(error))
                                .is_err()
                            {
                                return;
                            }
                        }
                        ReceiptPollOutcome::Fatal(error) => {
                            let _ = message_sender.send(ReceiptPollMessage::Fatal(error));
                            return;
                        }
                    }
                    match stop_receiver.recv_timeout(RECEIPT_POLL_INTERVAL) {
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
            })?;
        Ok(Self {
            messages,
            stop,
            thread: Some(thread),
        })
    }

    fn try_recv(&self) -> Result<ReceiptPollMessage, TryRecvError> {
        self.messages.try_recv()
    }
}

impl Drop for ReceiptPollWorker {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn poll_exact_terminal_receipt(socket: &Path, incident_id: &str) -> ReceiptPollOutcome {
    classify_explain_response(
        IpcClient::with_io_timeout(socket, RECEIPT_POLL_IO_TIMEOUT).request(IpcCommand::Explain {
            incident_id: incident_id.to_owned(),
        }),
        incident_id,
    )
}

fn classify_explain_response(
    response: Result<IpcPayload, IpcError>,
    incident_id: &str,
) -> ReceiptPollOutcome {
    match response {
        Ok(IpcPayload::Incident(detail)) if detail.incident_id == incident_id => {
            terminal_receipt(&detail.events, incident_id)
                .map_or(ReceiptPollOutcome::Pending, ReceiptPollOutcome::Terminal)
        }
        Ok(IpcPayload::Incident(detail)) => ReceiptPollOutcome::Fatal(format!(
            "exact incident response returned {:?} instead of {incident_id:?}",
            detail.incident_id
        )),
        Ok(_) => ReceiptPollOutcome::Fatal(
            "daemon returned the wrong payload for exact incident Explain".to_owned(),
        ),
        Err(IpcError::Remote { code, .. }) if code == "not_found" => ReceiptPollOutcome::Pending,
        Err(IpcError::Remote { code, message }) if code == "unavailable" => {
            ReceiptPollOutcome::Retryable(format!(
                "daemon Explain is temporarily unavailable: {message}"
            ))
        }
        Err(IpcError::Io(error)) if transient_ipc_error(error.kind()) => {
            ReceiptPollOutcome::Retryable(format!("exact incident Explain I/O failed: {error}"))
        }
        Err(error) => ReceiptPollOutcome::Fatal(format!(
            "exact incident Explain failed without a retryable read-only outcome: {error}"
        )),
    }
}

fn transient_ipc_error(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::WouldBlock
            | io::ErrorKind::TimedOut
            | io::ErrorKind::Interrupted
            | io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
    )
}

fn wait_for_terminal_receipt(
    socket: &Path,
    incident_id: &str,
    exact_tree: &[ProcessIdentity],
) -> Result<TerminalObservation, Box<dyn Error>> {
    let deadline = Instant::now() + RECEIPT_TIMEOUT;
    let mut snapshotter = MacosRuntime::new();
    let mut absence = ExactTreeAbsenceTracker::default();
    let mut receipt = None;
    let mut last_retryable_error = None;
    let receipt_worker = ReceiptPollWorker::start(socket.to_path_buf(), incident_id.to_owned())?;
    loop {
        let snapshot = snapshotter.snapshot()?;
        let exact_member_present = exact_tree.iter().any(|identity| {
            snapshot.processes.iter().any(|process| {
                process.pid() == identity.pid && identity.exact_match(&process.identity)
            })
        });
        absence.observe(exact_member_present, snapshot.observed_at_unix_millis)?;

        loop {
            match receipt_worker.try_recv() {
                Ok(ReceiptPollMessage::Terminal(terminal)) => receipt = Some(terminal),
                Ok(ReceiptPollMessage::Retryable(error)) => last_retryable_error = Some(error),
                Ok(ReceiptPollMessage::Fatal(error)) => return Err(field_error(error)),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) if receipt.is_some() => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(field_error(
                        "exact incident receipt worker exited before a terminal result",
                    ));
                }
            }
        }
        if let Some(exact_tree_absent_at_ms) = absence.exact_tree_absent_at_ms
            && let Some(receipt) = receipt.take()
        {
            return Ok(TerminalObservation {
                receipt,
                exact_tree_absent_at_ms,
            });
        }
        let now = Instant::now();
        if now >= deadline {
            let retryable = last_retryable_error
                .as_deref()
                .map_or(String::new(), |error| {
                    format!("; last read-only poll error: {error}")
                });
            return Err(field_error(format!(
                "incident {incident_id} did not reach a terminal journal receipt within {} seconds{retryable}",
                RECEIPT_TIMEOUT.as_secs(),
            )));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn terminal_receipt(history: &[HistoryEvent], incident_id: &str) -> Option<CleanupReceipt> {
    history.iter().rev().find_map(|event| {
        if event.incident_id != incident_id {
            return None;
        }
        let EventPayload::Cleanup { receipt } = &event.payload else {
            return None;
        };
        matches!(
            receipt.state,
            IncidentState::Cleared | IncidentState::Failed | IncidentState::Revived
        )
        .then(|| receipt.clone())
    })
}

fn verify_terminal_receipt(
    receipt: &CleanupReceipt,
    root: &ProcessIdentity,
    root_fingerprint: &str,
    exact_tree: &[ProcessIdentity],
) -> Result<(), Box<dyn Error>> {
    if receipt.state != IncidentState::Cleared
        || !receipt.survivor_pids.is_empty()
        || receipt.revival_checks_completed != 2
    {
        return Err(field_error(format!(
            "managed field cleanup did not finish CLEARED after both revival windows: {receipt:?}"
        )));
    }
    let root_term = receipt.actions.iter().position(|action| {
        action.pid == root.pid
            && action.identity_fingerprint == root_fingerprint
            && action.signal == CleanupSignal::Term
            && action.disposition == SignalDisposition::Delivered
    });
    let root_kill = receipt.actions.iter().position(|action| {
        action.pid == root.pid
            && action.identity_fingerprint == root_fingerprint
            && action.signal == CleanupSignal::Kill
            && action.disposition == SignalDisposition::Delivered
    });
    if !matches!((root_term, root_kill), (Some(term), Some(kill)) if term < kill) {
        return Err(field_error(format!(
            "stopped field root did not produce ordered delivered TERM then KILL: {:?}",
            receipt.actions
        )));
    }
    let admitted_targets = exact_tree
        .iter()
        .map(|identity| (identity.pid, fingerprint_process_identity(identity)))
        .collect::<BTreeSet<_>>();
    let unexpected_targets = receipt
        .actions
        .iter()
        .filter(|action| {
            !admitted_targets.contains(&(action.pid, action.identity_fingerprint.clone()))
        })
        .map(|action| (action.pid, action.identity_fingerprint.clone()))
        .collect::<Vec<_>>();
    if !unexpected_targets.is_empty() {
        return Err(field_error(format!(
            "terminal receipt targeted identities outside the frozen field tree: {unexpected_targets:?}"
        )));
    }
    Ok(())
}

fn cleanup_stage_name(stage: CleanupStage) -> &'static str {
    match stage {
        CleanupStage::PrimaryTerm => "primary_term",
        CleanupStage::MemberTerm => "member_term",
        CleanupStage::ExactKill => "exact_kill",
    }
}

fn cleanup_signal_name(signal: CleanupSignal) -> &'static str {
    match signal {
        CleanupSignal::Term => "term",
        CleanupSignal::Kill => "kill",
    }
}

fn signal_disposition_name(disposition: SignalDisposition) -> &'static str {
    match disposition {
        SignalDisposition::Delivered => "delivered",
        SignalDisposition::AlreadyExited => "already_exited",
        SignalDisposition::IdentityMismatch => "identity_mismatch",
        SignalDisposition::Rejected => "rejected",
        SignalDisposition::CancelledBeforeDelivery => "cancelled_before_delivery",
        SignalDisposition::DeliveryUnknown => "delivery_unknown",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct JournalSnapshot {
    attempts: Vec<JournalAttempt>,
    actions: Vec<JournalAction>,
    artifact_actions: Vec<JournalArtifactAction>,
    terminal_events: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct JournalAttempt {
    id: i64,
    enforcement_epoch: String,
    started_at_ms: i64,
    completed_at_ms: Option<i64>,
    terminal_state: Option<String>,
    reason_id: Option<String>,
    revival_checks_completed: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct JournalAction {
    id: i64,
    attempt_id: i64,
    sequence: i64,
    prepared_at_ms: i64,
    completed_at_ms: Option<i64>,
    stage: String,
    pid: i64,
    identity_fingerprint: String,
    signal: String,
    disposition: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct JournalArtifactAction {
    id: i64,
    attempt_id: i64,
    sequence: i64,
    prepared_at_ms: i64,
    completed_at_ms: Option<i64>,
    kind: String,
    artifact_fingerprint: String,
    disposition: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineArtifact {
    kind: RuntimeArtifactKind,
    artifact_fingerprint: String,
    identity: RuntimeArtifactIdentity,
}

struct TerminalContract<'a> {
    root_pid: u32,
    root_fingerprint: &'a str,
    epoch: &'a str,
    arm_command_started_at: u64,
    exact_tree_absent_at_ms: u64,
    baseline_artifact: &'a BaselineArtifact,
    exact_tree: &'a [ProcessIdentity],
    receipt_actions: &'a [CleanupAction],
    receipt_artifact_actions: &'a [ArtifactAction],
}

impl JournalSnapshot {
    fn capture(database: &Path, incident_id: &str) -> Result<Self, Box<dyn Error>> {
        let connection = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_secs(2))?;

        let attempts = {
            let mut statement = connection.prepare(
                "SELECT id, enforcement_epoch, started_at_ms, completed_at_ms,
                        terminal_state, reason_id, revival_checks_completed
                 FROM cleanup_attempts WHERE incident_id = ?1 ORDER BY id",
            )?;
            let rows = statement.query_map(params![incident_id], |row| {
                Ok(JournalAttempt {
                    id: row.get(0)?,
                    enforcement_epoch: row.get(1)?,
                    started_at_ms: row.get(2)?,
                    completed_at_ms: row.get(3)?,
                    terminal_state: row.get(4)?,
                    reason_id: row.get(5)?,
                    revival_checks_completed: row.get(6)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let actions = {
            let mut statement = connection.prepare(
                "SELECT actions.id, actions.attempt_id, actions.sequence,
                        actions.prepared_at_ms, actions.completed_at_ms, actions.stage,
                        actions.pid, actions.identity_fingerprint, actions.signal,
                        actions.disposition
                 FROM cleanup_actions AS actions
                 JOIN cleanup_attempts AS attempts ON attempts.id = actions.attempt_id
                 WHERE attempts.incident_id = ?1
                 ORDER BY actions.attempt_id, actions.sequence",
            )?;
            let rows = statement.query_map(params![incident_id], |row| {
                Ok(JournalAction {
                    id: row.get(0)?,
                    attempt_id: row.get(1)?,
                    sequence: row.get(2)?,
                    prepared_at_ms: row.get(3)?,
                    completed_at_ms: row.get(4)?,
                    stage: row.get(5)?,
                    pid: row.get(6)?,
                    identity_fingerprint: row.get(7)?,
                    signal: row.get(8)?,
                    disposition: row.get(9)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let artifact_actions = {
            let mut statement = connection.prepare(
                "SELECT actions.id, actions.attempt_id, actions.sequence,
                        actions.prepared_at_ms, actions.completed_at_ms, actions.kind,
                        actions.artifact_fingerprint, actions.disposition
                 FROM cleanup_artifact_actions AS actions
                 JOIN cleanup_attempts AS attempts ON attempts.id = actions.attempt_id
                 WHERE attempts.incident_id = ?1
                 ORDER BY actions.attempt_id, actions.sequence",
            )?;
            let rows = statement.query_map(params![incident_id], |row| {
                Ok(JournalArtifactAction {
                    id: row.get(0)?,
                    attempt_id: row.get(1)?,
                    sequence: row.get(2)?,
                    prepared_at_ms: row.get(3)?,
                    completed_at_ms: row.get(4)?,
                    kind: row.get(5)?,
                    artifact_fingerprint: row.get(6)?,
                    disposition: row.get(7)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let terminal_events = connection.query_row(
            "SELECT COUNT(*) FROM events
             WHERE incident_id = ?1 AND kind = 'cleanup'
               AND state IN ('CLEARED', 'FAILED', 'REVIVED')",
            params![incident_id],
            |row| row.get(0),
        )?;
        Ok(Self {
            attempts,
            actions,
            artifact_actions,
            terminal_events,
        })
    }

    fn verify_terminal_contract(
        &self,
        expected: TerminalContract<'_>,
    ) -> Result<(), Box<dyn Error>> {
        let TerminalContract {
            root_pid,
            root_fingerprint,
            epoch,
            arm_command_started_at,
            exact_tree_absent_at_ms,
            baseline_artifact,
            exact_tree,
            receipt_actions,
            receipt_artifact_actions,
        } = expected;
        if self.attempts.len() != 1 || self.terminal_events != 1 {
            return Err(field_error(format!(
                "expected one attempt and one terminal event, found {self:?}"
            )));
        }
        let attempt = &self.attempts[0];
        if attempt.enforcement_epoch != epoch
            || attempt.terminal_state.as_deref() != Some("CLEARED")
            || attempt.completed_at_ms.is_none()
            || attempt.revival_checks_completed != Some(2)
        {
            return Err(field_error(format!(
                "terminal attempt did not retain exact epoch/CLEARED/revival state: {attempt:?}"
            )));
        }
        let arm_started = i64::try_from(arm_command_started_at)?;
        if attempt.started_at_ms - arm_started < 89_000 {
            return Err(field_error(format!(
                "cleanup matured before the production 90-second abandonment grace: {attempt:?}"
            )));
        }
        let first_prepared = self
            .actions
            .first()
            .ok_or_else(|| field_error("terminal attempt had no journaled actions"))?;
        if first_prepared.prepared_at_ms - attempt.started_at_ms < 14_000 {
            return Err(field_error(format!(
                "first signal was prepared before the production 15-second re-observation gap: {first_prepared:?}"
            )));
        }
        let last_action_completed = self
            .actions
            .iter()
            .filter_map(|action| action.completed_at_ms)
            .max()
            .ok_or_else(|| field_error("terminal attempt retained a PREPARED action"))?;
        if attempt
            .completed_at_ms
            .ok_or_else(|| field_error("terminal attempt omitted completion time"))?
            - last_action_completed
            < 74_000
        {
            return Err(field_error(
                "terminal receipt arrived before the 15/60-second revival windows elapsed",
            ));
        }

        let mut ids = BTreeSet::new();
        let mut sequences = BTreeSet::new();
        for action in &self.actions {
            if !ids.insert(action.id)
                || !sequences.insert((action.attempt_id, action.sequence))
                || action.disposition.is_none()
            {
                return Err(field_error(format!(
                    "cleanup journal contains a duplicate or PREPARED action: {action:?}"
                )));
            }
        }
        if self.actions.len() != receipt_actions.len() {
            return Err(field_error(format!(
                "durable process-action count did not match terminal receipt: journal={}, receipt={}",
                self.actions.len(),
                receipt_actions.len()
            )));
        }
        let admitted_targets = exact_tree
            .iter()
            .map(|identity| (identity.pid, fingerprint_process_identity(identity)))
            .collect::<BTreeSet<_>>();
        for (index, (journal, receipt)) in self.actions.iter().zip(receipt_actions).enumerate() {
            let journal_pid = u32::try_from(journal.pid).map_err(|_| {
                field_error(format!(
                    "durable process action stored an invalid pid: {journal:?}"
                ))
            })?;
            let completed_at_ms = journal.completed_at_ms.ok_or_else(|| {
                field_error(format!(
                    "durable process action remained PREPARED: {journal:?}"
                ))
            })?;
            let expected_sequence = i64::try_from(index)?;
            if journal.attempt_id != attempt.id
                || journal.sequence != expected_sequence
                || journal.prepared_at_ms <= 0
                || completed_at_ms < journal.prepared_at_ms
                || journal_pid != receipt.pid
                || journal.identity_fingerprint != receipt.identity_fingerprint
                || journal.stage != cleanup_stage_name(receipt.stage)
                || journal.signal != cleanup_signal_name(receipt.signal)
                || journal.disposition.as_deref()
                    != Some(signal_disposition_name(receipt.disposition))
            {
                return Err(field_error(format!(
                    "durable process action did not exactly project terminal receipt order: journal={journal:?}, receipt={receipt:?}"
                )));
            }
            if !admitted_targets.contains(&(journal_pid, journal.identity_fingerprint.clone())) {
                return Err(field_error(format!(
                    "durable process action targeted an identity outside the frozen field tree: {journal:?}"
                )));
            }
        }
        let root_pid = i64::from(root_pid);
        let term = self.actions.iter().find(|action| {
            action.pid == root_pid
                && action.identity_fingerprint == root_fingerprint
                && action.signal == "term"
                && action.disposition.as_deref() == Some("delivered")
        });
        let kill = self.actions.iter().find(|action| {
            action.pid == root_pid
                && action.identity_fingerprint == root_fingerprint
                && action.signal == "kill"
                && action.disposition.as_deref() == Some("delivered")
        });
        if !matches!((term, kill), (Some(term), Some(kill)) if term.sequence < kill.sequence) {
            return Err(field_error(
                "journal did not retain ordered delivered TERM then KILL for the exact stopped root",
            ));
        }

        let [artifact] = self.artifact_actions.as_slice() else {
            return Err(field_error(format!(
                "expected exactly one durable runtime-artifact row, found {:?}",
                self.artifact_actions
            )));
        };
        let completed_at_ms = artifact.completed_at_ms.ok_or_else(|| {
            field_error(format!(
                "runtime-artifact row remained PREPARED without a disposition: {artifact:?}"
            ))
        })?;
        if artifact.attempt_id != attempt.id
            || artifact.sequence != 0
            || artifact.prepared_at_ms <= 0
            || completed_at_ms < artifact.prepared_at_ms
            || artifact.kind != "dev_tools_active_port"
            || artifact.artifact_fingerprint != baseline_artifact.artifact_fingerprint
            || artifact.disposition.as_deref() != Some("removed")
        {
            return Err(field_error(format!(
                "runtime-artifact journal did not complete the exact baseline DevToolsActivePort removal: {artifact:?}"
            )));
        }
        let exact_tree_absent_at_ms = i64::try_from(exact_tree_absent_at_ms)?;
        if artifact.prepared_at_ms - exact_tree_absent_at_ms < 74_000 {
            return Err(field_error(format!(
                "runtime-artifact PREPARED boundary preceded full 15/60-second revival and final tree revalidation: artifact={artifact:?}, exact_tree_absent_at_ms={exact_tree_absent_at_ms}"
            )));
        }
        if artifact.prepared_at_ms <= last_action_completed
            || attempt
                .completed_at_ms
                .is_none_or(|attempt_completed| attempt_completed < completed_at_ms)
        {
            return Err(field_error(format!(
                "runtime-artifact action was not durably bounded between process delivery and terminal receipt: artifact={artifact:?}, attempt={attempt:?}"
            )));
        }
        let expected_receipt_action = ArtifactAction {
            kind: baseline_artifact.kind,
            artifact_fingerprint: baseline_artifact.artifact_fingerprint.clone(),
            disposition: ArtifactDisposition::Removed,
        };
        if receipt_artifact_actions != [expected_receipt_action] {
            return Err(field_error(format!(
                "terminal receipt artifact actions did not exactly match the durable journal projection: receipt={receipt_artifact_actions:?}, journal={artifact:?}"
            )));
        }
        Ok(())
    }
}

fn kickstart_managed_launch_agent() -> Result<(), Box<dyn Error>> {
    let domain = format!("gui/{}/{}", unsafe { libc::geteuid() }, LAUNCH_AGENT_LABEL);
    let output = Command::new("/bin/launchctl")
        .args(["kickstart", "-k", &domain])
        .output()?;
    if !output.status.success() {
        return Err(field_error(format!(
            "launchctl kickstart failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct RestartTrace {
    transition_observed: bool,
    unavailable_samples: usize,
    status_samples: Vec<Value>,
}

fn wait_for_rearmed_restart(
    cli: &Path,
    prior: &ManagedServiceSnapshot,
) -> Result<(ManagedServiceSnapshot, RestartTrace), Box<dyn Error>> {
    let deadline = Instant::now() + SERVICE_TIMEOUT;
    let mut trace = RestartTrace {
        transition_observed: false,
        unavailable_samples: 0,
        status_samples: Vec::new(),
    };
    loop {
        match run_cli_json(cli, ["service", "status", "--json"]) {
            Ok(value) => {
                trace.status_samples.push(value.clone());
                match ManagedServiceSnapshot::parse(&value) {
                    Ok(status) if status.daemon.instance_id != prior.daemon.instance_id => {
                        if status.daemon.startup_state != "ready_enforce"
                            || !status.daemon.ready
                            || status.daemon.effective_mode != "enforce"
                        {
                            trace.transition_observed = true;
                        }
                        if status.require_ready_enforce().is_ok()
                            && status.active_generation == prior.active_generation
                            && status.daemon.enforcement_epoch != prior.daemon.enforcement_epoch
                        {
                            return Ok((status, trace));
                        }
                    }
                    Err(_) => trace.transition_observed = true,
                    Ok(_) => {}
                }
            }
            Err(_) => {
                trace.transition_observed = true;
                trace.unavailable_samples += 1;
            }
        }
        if Instant::now() >= deadline {
            return Err(field_error(
                "managed service did not recover, complete its first scan, and re-arm with a fresh epoch after restart",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn daemon_identity(status: &ManagedServiceSnapshot) -> Result<ProcessIdentity, Box<dyn Error>> {
    let mut runtime = MacosRuntime::new();
    let snapshot = runtime.snapshot()?;
    snapshot
        .processes
        .iter()
        .find(|process| process.pid() == status.daemon.pid)
        .map(|process| process.identity.clone())
        .ok_or_else(|| field_error("managed daemon PID was absent from the native snapshot"))
}

fn create_profile() -> Result<PathBuf, Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let profile = PathBuf::from(format!(
        "/private/tmp/playwright_chromiumdev_profile-unlinger-managed-fieldlab-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&profile)?;
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o700))?;
    Ok(profile)
}

fn launch_chrome_for_testing(app: &Path, profile: &Path) -> Result<(), Box<dyn Error>> {
    let status = Command::new("/usr/bin/open")
        .arg("-na")
        .arg(app)
        .arg("--args")
        .arg("--headless=new")
        .arg("--disable-background-networking")
        .arg("--disable-component-update")
        .arg("--disable-default-apps")
        .arg("--disable-sync")
        .arg("--metrics-recording-only")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .status()?;
    if !status.success() {
        return Err(field_error(format!(
            "LaunchServices returned {status} for Chrome for Testing"
        )));
    }
    Ok(())
}

fn wait_for_detached_root(profile: &Path) -> Result<(Snapshot, ProcessIdentity), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut runtime = MacosRuntime::new();
    loop {
        let snapshot = runtime.snapshot()?;
        let detached_identity = find_field_root(&snapshot, profile)
            .filter(|root| root.parent_pid == 1)
            .map(|root| root.identity.clone());
        if let Some(identity) = detached_identity {
            return Ok((snapshot, identity));
        }
        if Instant::now() >= deadline {
            return Err(field_error(
                "Chrome for Testing did not become a detached field root within 20 seconds",
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn analyzer_reports(
    snapshot: &Snapshot,
) -> Result<Vec<unlinger_core::IncidentReport>, Box<dyn Error>> {
    Ok(Analyzer::new(RuleSet::embedded()?, AnalyzerContext::default()).observe(snapshot)?)
}

fn assert_no_cleanup_candidates(snapshot: &Snapshot) -> Result<(), Box<dyn Error>> {
    let candidates = analyzer_reports(snapshot)?
        .into_iter()
        .filter(|report| {
            matches!(
                report.state,
                IncidentState::Cooling | IncidentState::Confirmed
            )
        })
        .map(|report| report.incident_id)
        .collect::<Vec<_>>();
    if !candidates.is_empty() {
        return Err(field_error(format!(
            "refusing managed enforcement with pre-existing cleanup candidates: {candidates:?}"
        )));
    }
    Ok(())
}

fn unique_field_candidate(
    snapshot: &Snapshot,
    expected: &ProcessIdentity,
) -> Result<unlinger_core::IncidentReport, Box<dyn Error>> {
    let candidates = analyzer_reports(snapshot)?
        .into_iter()
        .filter(|report| {
            matches!(
                report.state,
                IncidentState::Cooling | IncidentState::Confirmed
            )
        })
        .collect::<Vec<_>>();
    let target_index = candidates
        .iter()
        .position(|report| report.root.pid == expected.pid)
        .ok_or_else(|| field_error("unique CfT root did not produce a cleanup candidate"))?;
    if candidates.len() != 1 {
        return Err(field_error(format!(
            "refusing managed enforcement with unrelated cleanup candidates: {:?}",
            candidates
                .iter()
                .filter(|report| report.root.pid != expected.pid)
                .map(|report| &report.incident_id)
                .collect::<Vec<_>>()
        )));
    }
    Ok(candidates[target_index].clone())
}

fn wait_for_unique_field_candidate(
    profile: &Path,
    expected: &ProcessIdentity,
) -> Result<unlinger_core::IncidentReport, Box<dyn Error>> {
    let deadline = Instant::now() + CANDIDATE_TIMEOUT;
    let mut runtime = MacosRuntime::new();
    loop {
        let snapshot = runtime.snapshot()?;
        let Some(root) = find_field_root(&snapshot, profile) else {
            return Err(field_error(
                "field Chrome for Testing root exited before candidate admission",
            ));
        };
        if !expected.exact_match(&root.identity) {
            return Err(field_error(
                "field Chrome for Testing root identity changed before candidate admission",
            ));
        }
        let reports = analyzer_reports(&snapshot)?;
        let cleanup_candidates = reports
            .iter()
            .filter(|report| {
                matches!(
                    report.state,
                    IncidentState::Cooling | IncidentState::Confirmed
                )
            })
            .collect::<Vec<_>>();
        let unrelated = cleanup_candidates
            .iter()
            .filter(|report| report.root.pid != expected.pid)
            .map(|report| &report.incident_id)
            .collect::<Vec<_>>();
        if !unrelated.is_empty() {
            return Err(field_error(format!(
                "refusing managed enforcement with unrelated cleanup candidates: {unrelated:?}"
            )));
        }
        if cleanup_candidates
            .iter()
            .any(|report| report.root.pid == expected.pid)
        {
            return unique_field_candidate(&snapshot, expected);
        }
        if Instant::now() >= deadline {
            let target = reports
                .iter()
                .find(|report| report.root.pid == expected.pid);
            let state = target.map(|report| report.state);
            let evidence = target
                .into_iter()
                .flat_map(|report| report.evidence.iter().map(|item| item.id.as_str()))
                .collect::<Vec<_>>();
            return Err(field_error(format!(
                "unique CfT root did not become a cleanup candidate within {} seconds: state={state:?}, evidence={evidence:?}",
                CANDIDATE_TIMEOUT.as_secs()
            )));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn unique_field_artifact(
    report: &unlinger_core::IncidentReport,
    profile: &Path,
) -> Result<RuntimeArtifactCandidate, Box<dyn Error>> {
    let [candidate] = report.runtime_artifacts.as_slice() else {
        return Err(field_error(format!(
            "managed field candidate must expose exactly one redacted runtime-artifact plan, found {}",
            report.runtime_artifacts.len()
        )));
    };
    let expected_path = profile.join("DevToolsActivePort");
    if candidate.kind() != RuntimeArtifactKind::DevToolsActivePort
        || candidate.profile_path() != profile
        || candidate.artifact_path() != expected_path
        || candidate.artifact_fingerprint().contains('/')
        || !candidate.artifact_fingerprint().starts_with("art-")
    {
        return Err(field_error(
            "managed field candidate did not expose the exact private-path-free DevToolsActivePort plan",
        ));
    }
    Ok(candidate.clone())
}

fn wait_for_baseline_artifact(
    runtime: &mut MacosRuntime,
    candidate: &RuntimeArtifactCandidate,
) -> Result<BaselineArtifact, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match runtime.freeze_artifact(candidate)? {
            ArtifactFreeze::Frozen(frozen) => {
                return Ok(BaselineArtifact {
                    kind: candidate.kind(),
                    artifact_fingerprint: candidate.artifact_fingerprint().to_owned(),
                    identity: frozen.identity().clone(),
                });
            }
            ArtifactFreeze::Absent if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100));
            }
            ArtifactFreeze::Absent => {
                return Err(field_error(
                    "Chrome for Testing did not create its exact DevToolsActivePort artifact within 20 seconds",
                ));
            }
            ArtifactFreeze::Unsafe => {
                return Err(field_error(
                    "Chrome for Testing created an unsafe DevToolsActivePort artifact",
                ));
            }
        }
    }
}

fn require_exact_baseline_artifact(
    runtime: &mut MacosRuntime,
    candidate: &RuntimeArtifactCandidate,
    baseline: &BaselineArtifact,
) -> Result<(), Box<dyn Error>> {
    match runtime.freeze_artifact(candidate)? {
        ArtifactFreeze::Frozen(current)
            if current.identity() == &baseline.identity
                && current.candidate().kind() == baseline.kind
                && current.candidate().artifact_fingerprint() == baseline.artifact_fingerprint =>
        {
            Ok(())
        }
        ArtifactFreeze::Frozen(_) => Err(field_error(
            "DevToolsActivePort identity changed between baseline capture and managed arming",
        )),
        ArtifactFreeze::Absent => Err(field_error(
            "DevToolsActivePort disappeared between baseline capture and managed arming",
        )),
        ArtifactFreeze::Unsafe => Err(field_error(
            "DevToolsActivePort became unsafe between baseline capture and managed arming",
        )),
    }
}

fn require_canonical_artifact_absent(
    candidate: &RuntimeArtifactCandidate,
) -> Result<(), Box<dyn Error>> {
    match fs::symlink_metadata(candidate.artifact_path()) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(field_error(format!(
            "could not prove canonical DevToolsActivePort absence: {error}"
        ))),
        Ok(_) => Err(field_error(
            "canonical DevToolsActivePort still existed after terminal cleanup",
        )),
    }
}

fn stop_exact_field_root(profile: &Path, expected: &ProcessIdentity) -> Result<(), Box<dyn Error>> {
    let mut runtime = MacosRuntime::new();
    let snapshot = runtime.snapshot()?;
    let current = find_field_root(&snapshot, profile)
        .ok_or_else(|| field_error("field root exited before SIGSTOP"))?;
    if !expected.exact_match(&current.identity) {
        return Err(field_error("field root identity changed before SIGSTOP"));
    }
    let pid = i32::try_from(expected.pid)?;
    if unsafe { libc::kill(pid, libc::SIGSTOP) } != 0 {
        return Err(Box::new(io::Error::last_os_error()));
    }
    thread::sleep(Duration::from_millis(100));
    let snapshot = runtime.snapshot()?;
    if find_field_root(&snapshot, profile)
        .is_none_or(|current| !expected.exact_match(&current.identity))
    {
        return Err(field_error(
            "field root identity changed after exact SIGSTOP",
        ));
    }
    Ok(())
}

fn find_field_root<'a>(snapshot: &'a Snapshot, profile: &Path) -> Option<&'a ProcessRecord> {
    snapshot.processes.iter().find(|process| {
        process
            .executable_basename()
            .eq_ignore_ascii_case("Google Chrome for Testing")
            && !has_process_type(process)
            && uses_profile(process, profile)
    })
}

fn uses_profile(process: &ProcessRecord, profile: &Path) -> bool {
    let expected = profile.to_string_lossy();
    let Some(arguments) = &process.arguments else {
        return false;
    };
    for (index, argument) in arguments.iter().enumerate() {
        if argument == "--user-data-dir"
            && arguments
                .get(index + 1)
                .is_some_and(|value| value == expected.as_ref())
        {
            return true;
        }
        if argument
            .strip_prefix("--user-data-dir=")
            .is_some_and(|value| value == expected.as_ref())
        {
            return true;
        }
    }
    false
}

fn has_process_type(process: &ProcessRecord) -> bool {
    process.arguments.as_ref().is_some_and(|arguments| {
        arguments
            .iter()
            .any(|argument| argument.to_ascii_lowercase().starts_with("--type="))
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct OrdinaryChromeTree {
    roots: BTreeMap<u32, ProcessIdentity>,
    identities: BTreeMap<u32, ProcessIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct OrdinaryChromeProof {
    before_roots: usize,
    before_tree_members: usize,
    after_roots: usize,
    after_tree_members: usize,
    exact_preexisting_root_identities_preserved: bool,
}

impl OrdinaryChromeTree {
    fn capture(snapshot: &Snapshot) -> Result<Self, Box<dyn Error>> {
        let graph = ProcessGraph::from_snapshot(snapshot)?;
        let roots = snapshot
            .processes
            .iter()
            .filter(|process| {
                process.executable_path.as_deref() == Some(ORDINARY_CHROME_EXECUTABLE)
                    && !has_process_type(process)
            })
            .map(|process| (process.pid(), process.identity.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut identities = BTreeMap::new();
        for (pid, identity) in &roots {
            identities.insert(*pid, identity.clone());
            for descendant in graph.descendant_pids(*pid) {
                if let Some(process) = graph.get(descendant) {
                    identities.insert(descendant, process.identity.clone());
                }
            }
        }
        Ok(Self { roots, identities })
    }

    fn proof(&self, after: &Snapshot) -> Result<OrdinaryChromeProof, Box<dyn Error>> {
        let after_tree = Self::capture(after)?;
        let exact_preserved = self
            .missing_or_changed_preexisting_roots(&after_tree)
            .is_empty();
        Ok(OrdinaryChromeProof {
            before_roots: self.roots.len(),
            before_tree_members: self.identities.len(),
            after_roots: after_tree.roots.len(),
            after_tree_members: after_tree.identities.len(),
            exact_preexisting_root_identities_preserved: exact_preserved,
        })
    }

    fn missing_or_changed_preexisting_roots(&self, after: &Self) -> Vec<u32> {
        self.roots
            .iter()
            .filter_map(|(pid, identity)| {
                after
                    .roots
                    .get(pid)
                    .is_none_or(|current| !identity.exact_match(current))
                    .then_some(*pid)
            })
            .collect()
    }

    fn assert_preexisting_preserved(&self, after: &Snapshot) -> Result<(), Box<dyn Error>> {
        let after_tree = Self::capture(after)?;
        let missing_or_changed = self.missing_or_changed_preexisting_roots(&after_tree);
        if !missing_or_changed.is_empty() {
            return Err(field_error(format!(
                "pre-existing ordinary Chrome root identities changed: {missing_or_changed:?}"
            )));
        }
        Ok(())
    }
}

fn write_artifact(
    profile: &Path,
    name: &str,
    value: &impl Serialize,
) -> Result<(), Box<dyn Error>> {
    let path = profile.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

struct ManagedServiceGuard {
    cli: PathBuf,
    armed: bool,
}

impl ManagedServiceGuard {
    fn new(cli: PathBuf) -> Self {
        Self { cli, armed: false }
    }
}

impl Drop for ManagedServiceGuard {
    fn drop(&mut self) {
        if self.armed {
            let output = Command::new(&self.cli)
                .args(["service", "set-mode", "report-only", "--json"])
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    eprintln!("managed fieldlab fail-safe disarmed the installed service")
                }
                Ok(output) => eprintln!(
                    "WARNING: managed fieldlab fail-safe disarm failed with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
                Err(error) => eprintln!(
                    "WARNING: managed fieldlab could not invoke fail-safe disarm: {error}"
                ),
            }
        }
    }
}

struct FieldSession {
    profile: PathBuf,
}

impl FieldSession {
    fn new(profile: PathBuf) -> Self {
        Self { profile }
    }
}

impl Drop for FieldSession {
    fn drop(&mut self) {
        let mut runtime = MacosRuntime::new();
        if let Ok(snapshot) = runtime.snapshot()
            && let Some(root) = find_field_root(&snapshot, &self.profile)
        {
            let identity = root.identity.clone();
            let _ = runtime.signal_exact(&identity, CleanupSignal::Term);
            thread::sleep(Duration::from_secs(1));
            if let Ok(snapshot) = runtime.snapshot()
                && find_field_root(&snapshot, &self.profile)
                    .is_some_and(|current| identity.exact_match(&current.identity))
            {
                let _ = runtime.signal_exact(&identity, CleanupSignal::Kill);
            }
        }
        eprintln!(
            "managed fieldlab retained its isolated 0700 profile and receipts for local inspection"
        );
    }
}

fn now_unix_millis() -> Result<u64, Box<dyn Error>> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

fn field_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::other(message.into()))
}
