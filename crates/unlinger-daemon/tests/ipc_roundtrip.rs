#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unlinger_core::{
    BrowserCompatibility, BrowserCompatibilityDecision, BrowserProduct, CleanupReceipt,
    CleanupResources, GateLedger, IncidentReport, IncidentState, ProcessIdentity, ProcessRole,
    ProcessRoleCount, ProcessTarget, ResourceSnapshot, RootSummary, StorageResidueKind,
    StorageResidueObservation, StorageResidueReferenceCheck, StorageResidueStatus,
};
use unlinger_daemon::{
    AttentionKind, ControlPlane, DaemonMode, DaemonStatus, HistoryStore, IpcClient, IpcCommand,
    IpcPayload, IpcServer, StartupState,
};

struct TempState {
    directory: PathBuf,
}

static NEXT_TEMP_STATE_ID: AtomicU64 = AtomicU64::new(1);

impl TempState {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "ul-{:x}-{:x}-{:x}",
            std::process::id(),
            nonce & 0xffff_ffff,
            NEXT_TEMP_STATE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("create temp directory");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("protect temp directory");
        Self { directory }
    }
}

impl Drop for TempState {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn raw_request(socket: &std::path::Path, request: &str) -> serde_json::Value {
    let mut stream = UnixStream::connect(socket).expect("connect raw IPC client");
    stream
        .write_all(request.as_bytes())
        .expect("write raw IPC request");
    stream.write_all(b"\n").expect("terminate IPC request");
    stream.flush().expect("flush raw IPC request");
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .expect("read raw IPC response");
    serde_json::from_str(&response).expect("parse raw IPC response")
}

fn confirmed_report(id: &str, tracking_key: &str) -> IncidentReport {
    let identity = ProcessIdentity {
        pid: 4242,
        started_at_unix_micros: 1_700_000_000_000_000,
        executable_device: Some(1),
        executable_inode: Some(2),
    };
    IncidentReport {
        incident_id: id.to_owned(),
        tracking_key: tracking_key.to_owned(),
        session_fingerprint: "session-redacted".to_owned(),
        signature_pack: "agent-browser".to_owned(),
        signature_version: "0.1.0".to_owned(),
        state: IncidentState::Confirmed,
        root: RootSummary {
            pid: identity.pid,
            started_at_unix_micros: identity.started_at_unix_micros,
            executable_basename: "node".to_owned(),
            identity_fingerprint: format!("identity-{id}"),
        },
        member_count: 1,
        resident_memory_bytes: 4096,
        member_fingerprint: format!("members-{id}"),
        roles: vec![ProcessRoleCount {
            role: ProcessRole::Controller,
            count: 1,
        }],
        evidence: Vec::new(),
        gates: GateLedger {
            same_user: true,
            strong_automation_provenance: true,
            confirmed_abandonment: true,
            isolated_session: true,
            stable_across_two_observations: true,
            process_identity_unchanged: true,
            no_protection_rule: true,
        },
        browser_compatibility: Default::default(),
        targets: vec![ProcessTarget {
            identity,
            process_group_id: 4242,
            role: ProcessRole::Controller,
        }],
        runtime_artifacts: Vec::new(),
    }
}

fn fail_cleanup(store: &HistoryStore, report: &IncidentReport, now: u64, reason_id: &str) {
    let attempt = store
        .begin_cleanup_attempt(now, report, "epoch-a")
        .expect("begin failed attempt");
    store
        .complete_cleanup_attempt(
            &attempt,
            now + 1,
            &CleanupReceipt {
                incident_id: report.incident_id.clone(),
                state: IncidentState::Failed,
                reason_id: Some(reason_id.to_owned()),
                actions: Vec::new(),
                artifact_actions: Vec::new(),
                survivor_pids: vec![4242],
                revival_checks_completed: 0,
                resources: CleanupResources::default(),
            },
        )
        .expect("terminalize failed attempt");
}

#[test]
fn local_socket_serves_status_and_persists_pause_resume() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let control = ControlPlane::new(
        store.clone(),
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let server = IpcServer::start(&socket, control.clone()).expect("start IPC server");
    let client = IpcClient::new(&socket);

    let response = client.request(IpcCommand::Status).expect("status request");
    let IpcPayload::Status(status) = response else {
        panic!("expected status response");
    };
    assert_eq!(status.mode, DaemonMode::ReportOnly);
    assert_eq!(status.pid, std::process::id());

    let response = client
        .request(IpcCommand::Pause {
            duration_millis: 60_000,
        })
        .expect("pause request");
    let IpcPayload::Pause { until_unix_millis } = response else {
        panic!("expected pause response");
    };
    assert!(until_unix_millis > 60_000);
    assert_eq!(
        store.pause_until().expect("persisted pause"),
        Some(until_unix_millis)
    );

    assert!(matches!(
        client.request(IpcCommand::Resume).expect("resume request"),
        IpcPayload::Resumed
    ));
    assert_eq!(store.pause_until().expect("cleared pause"), None);

    let metadata = fs::metadata(&socket).expect("socket metadata");
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    drop(server);
    assert!(!socket.exists());
}

#[test]
fn slow_partial_client_does_not_block_an_independent_status_request() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let control = ControlPlane::new(
        store,
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let server = IpcServer::start(&socket, control).expect("start IPC server");

    let mut slow_client = std::os::unix::net::UnixStream::connect(&socket)
        .expect("connect deliberately partial client");
    slow_client
        .write_all(b"{")
        .expect("write partial request without newline");
    slow_client.flush().expect("flush partial request");
    thread::sleep(Duration::from_millis(50));

    let payload = IpcClient::with_io_timeout(&socket, Duration::from_secs(1))
        .request(IpcCommand::Status)
        .expect("status must bypass the stalled connection");
    assert!(matches!(payload, IpcPayload::Status(_)));

    drop(slow_client);
    drop(server);
}

#[test]
fn history_limit_is_bounded_before_querying() {
    let temp = TempState::new();
    let store = HistoryStore::open(temp.directory.join("history.sqlite3")).expect("open store");
    let control = ControlPlane::new(
        store,
        DaemonStatus::new(DaemonMode::Enforce, std::process::id()),
    )
    .expect("restore control state");
    let result = control.handle_at(IpcCommand::History { limit: 100_000 }, 1_000);
    assert!(result.is_err());
}

#[test]
fn managed_control_requires_exact_ready_generation_and_instance() {
    let temp = TempState::new();
    let store = HistoryStore::open(temp.directory.join("history.sqlite3")).expect("open store");
    let control = ControlPlane::begin_managed(store, 41, std::process::id(), 1_000)
        .expect("begin managed control");
    control
        .finish_startup_recovery(0, 1_010)
        .expect("finish recovery");
    let before_ready = control.status().expect("status");
    assert!(before_ready.managed);
    assert!(!before_ready.ready);
    assert_eq!(
        before_ready.startup_state,
        StartupState::FirstScanReportOnly
    );
    assert_eq!(before_ready.effective_mode(), DaemonMode::ReportOnly);

    let arm = |generation, instance_id: String| IpcCommand::Arm {
        activation_generation: generation,
        instance_id,
    };
    assert!(
        control
            .handle_at(arm(41, before_ready.instance_id.clone()), 1_020)
            .expect_err("arm must wait for a first scan")
            .to_string()
            .contains("not ready")
    );

    control
        .complete_successful_cycle(1_030)
        .expect("first scan becomes ready");
    let ready = control.status().expect("ready status");
    assert!(ready.ready);
    assert_eq!(ready.startup_state, StartupState::ReadyReportOnly);
    assert!(
        control
            .handle_at(arm(40, ready.instance_id.clone()), 1_040)
            .expect_err("stale generation")
            .to_string()
            .contains("generation")
    );
    assert!(
        control
            .handle_at(arm(41, "replacement-instance".to_owned()), 1_040)
            .expect_err("stale instance")
            .to_string()
            .contains("instance")
    );

    let IpcPayload::Lifecycle(armed) = control
        .handle_at(arm(41, ready.instance_id.clone()), 1_050)
        .expect("arm exact ready daemon")
    else {
        panic!("expected lifecycle response")
    };
    assert_eq!(armed.effective_mode(), DaemonMode::Enforce);
    assert_eq!(armed.armed_generation, Some(41));
    let epoch = armed.enforcement_epoch.clone().expect("fresh epoch");
    let IpcPayload::Lifecycle(repeated) = control
        .handle_at(arm(41, ready.instance_id.clone()), 1_060)
        .expect("arm retry is idempotent")
    else {
        panic!("expected lifecycle response")
    };
    assert_eq!(repeated.enforcement_epoch.as_deref(), Some(epoch.as_str()));

    for now in [1_070, 1_080] {
        let IpcPayload::Lifecycle(disarmed) = control
            .handle_at(
                IpcCommand::Disarm {
                    activation_generation: 41,
                    instance_id: ready.instance_id.clone(),
                },
                now,
            )
            .expect("disarm is idempotent")
        else {
            panic!("expected lifecycle response")
        };
        assert_eq!(disarmed.effective_mode(), DaemonMode::ReportOnly);
        assert_eq!(disarmed.enforcement_epoch, None);
    }

    control
        .handle_at(arm(41, ready.instance_id.clone()), 1_090)
        .expect("rearm");
    for now in [1_100, 1_110] {
        let IpcPayload::Lifecycle(draining) = control
            .handle_at(
                IpcCommand::BeginDrain {
                    activation_generation: 41,
                    instance_id: ready.instance_id.clone(),
                },
                now,
            )
            .expect("drain is idempotent")
        else {
            panic!("expected lifecycle response")
        };
        assert!(draining.draining);
        assert!(!draining.ready);
        assert_eq!(draining.startup_state, StartupState::Draining);
        assert_eq!(draining.effective_mode(), DaemonMode::ReportOnly);
    }
}

#[test]
fn failed_managed_lifecycle_requires_restart_instead_of_reusing_first_scan_completion() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let control = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("open store"),
        42,
        std::process::id(),
        1_000,
    )
    .expect("begin managed control");
    control
        .finish_startup_recovery(0, 1_010)
        .expect("finish recovery");
    control
        .complete_successful_cycle(1_020)
        .expect("complete first report-only scan");
    let ready = control.status().expect("ready status");
    control
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 42,
                instance_id: ready.instance_id.clone(),
            },
            1_030,
        )
        .expect("arm exact managed instance");
    control
        .update_status(|status| {
            status.scan_in_progress = true;
            status.cleanup_in_progress = true;
        })
        .expect("mark in-flight cleanup");

    let IpcPayload::Lifecycle(disarmed_during_cleanup) = control
        .handle_at(
            IpcCommand::Disarm {
                activation_generation: 42,
                instance_id: ready.instance_id.clone(),
            },
            1_040,
        )
        .expect("disarm closes the in-flight signal gate")
    else {
        panic!("expected lifecycle response")
    };
    assert_eq!(
        disarmed_during_cleanup.startup_state,
        StartupState::ReadyReportOnly
    );
    assert!(disarmed_during_cleanup.scan_in_progress);
    assert!(disarmed_during_cleanup.cleanup_in_progress);

    control
        .fail_closed(1_050, "primary post-delivery cleanup failure")
        .expect("persist failed lifecycle");
    control
        .update_status(|status| {
            status.scan_in_progress = false;
            status.cleanup_in_progress = false;
        })
        .expect("finish failed cycle projection");

    let IpcPayload::Lifecycle(disarmed_after_failure) = control
        .handle_at(
            IpcCommand::Disarm {
                activation_generation: 42,
                instance_id: ready.instance_id,
            },
            1_060,
        )
        .expect("failed lifecycle remains safely disarmed")
    else {
        panic!("expected lifecycle response")
    };
    assert_eq!(disarmed_after_failure.startup_state, StartupState::Failed);
    assert!(!disarmed_after_failure.ready);
    assert_eq!(
        disarmed_after_failure.last_error.as_deref(),
        Some("primary post-delivery cleanup failure")
    );

    let completion_error = control
        .complete_successful_cycle(1_070)
        .expect_err("a later report-only scan cannot rehabilitate Failed in place");
    assert!(completion_error.to_string().contains("restart"));
    assert!(
        !completion_error
            .to_string()
            .contains("first report-only scan")
    );
    let failed = control.status().expect("failed status remains observable");
    assert_eq!(failed.startup_state, StartupState::Failed);
    assert!(!failed.healthy);
    assert!(!failed.ready);
    assert_eq!(failed.requested_mode, DaemonMode::ReportOnly);
    assert_eq!(failed.effective_mode(), DaemonMode::ReportOnly);
    assert!(failed.armed_generation.is_none());
    assert!(failed.enforcement_epoch.is_none());
    assert_eq!(
        failed.last_error.as_deref(),
        Some("primary post-delivery cleanup failure")
    );
    let durable = control
        .store()
        .managed_lifecycle()
        .expect("read durable lifecycle")
        .expect("managed lifecycle exists");
    assert_eq!(
        durable.startup_phase,
        unlinger_daemon::ManagedStartupPhase::Failed
    );
}

#[test]
fn managed_same_generation_restart_rearms_only_after_its_report_only_first_scan() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let first = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("open store"),
        42,
        4242,
        1_000,
    )
    .expect("begin first instance");
    first
        .finish_startup_recovery(0, 1_010)
        .expect("finish first recovery");
    first
        .complete_successful_cycle(1_020)
        .expect("finish first scan");
    let first_ready = first.status().expect("first ready status");
    let IpcPayload::Lifecycle(armed) = first
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 42,
                instance_id: first_ready.instance_id.clone(),
            },
            1_030,
        )
        .expect("arm first instance")
    else {
        panic!("expected lifecycle payload")
    };
    let old_epoch = armed.enforcement_epoch.expect("first epoch");
    drop(first);

    let replacement = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("reopen store"),
        42,
        4343,
        2_000,
    )
    .expect("begin replacement instance");
    let recovering = replacement.status().expect("replacement recovering");
    assert_eq!(recovering.requested_mode, DaemonMode::Enforce);
    assert_eq!(recovering.effective_mode(), DaemonMode::ReportOnly);
    assert!(!recovering.ready);
    assert_ne!(recovering.instance_id, first_ready.instance_id);
    assert_eq!(recovering.enforcement_epoch, None);
    assert!(
        replacement
            .handle_at(
                IpcCommand::Disarm {
                    activation_generation: 42,
                    instance_id: first_ready.instance_id.clone(),
                },
                2_005,
            )
            .expect_err("stale first instance cannot cancel replacement intent")
            .to_string()
            .contains("instance")
    );
    assert_eq!(
        replacement.status().expect("intent remains").requested_mode,
        DaemonMode::Enforce
    );

    replacement
        .finish_startup_recovery(0, 2_010)
        .expect("finish replacement recovery");
    let before_scan = replacement.status().expect("before first scan");
    assert_eq!(before_scan.requested_mode, DaemonMode::Enforce);
    assert_eq!(before_scan.effective_mode(), DaemonMode::ReportOnly);
    assert_eq!(before_scan.startup_state, StartupState::FirstScanReportOnly);

    replacement
        .complete_successful_cycle(2_020)
        .expect("report-only first scan commits re-arm");
    let rearmed = replacement.status().expect("replacement rearmed");
    assert_eq!(rearmed.requested_mode, DaemonMode::Enforce);
    assert_eq!(rearmed.effective_mode(), DaemonMode::Enforce);
    assert_eq!(rearmed.startup_state, StartupState::ReadyEnforce);
    assert_eq!(rearmed.armed_generation, Some(42));
    assert_ne!(
        rearmed.enforcement_epoch.as_deref(),
        Some(old_epoch.as_str())
    );
}

#[test]
fn disarm_and_drain_before_replacement_first_scan_cancel_carried_rearm() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let first = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("open store"),
        43,
        4242,
        1_000,
    )
    .expect("begin first instance");
    first
        .finish_startup_recovery(0, 1_010)
        .expect("finish first recovery");
    first
        .complete_successful_cycle(1_020)
        .expect("finish first scan");
    let first_status = first.status().expect("first status");
    first
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 43,
                instance_id: first_status.instance_id,
            },
            1_030,
        )
        .expect("arm first instance");
    drop(first);

    let replacement = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("reopen store"),
        43,
        4343,
        2_000,
    )
    .expect("begin replacement");
    replacement
        .finish_startup_recovery(0, 2_010)
        .expect("finish replacement recovery");
    let replacement_status = replacement.status().expect("replacement status");
    replacement
        .handle_at(
            IpcCommand::Disarm {
                activation_generation: 43,
                instance_id: replacement_status.instance_id.clone(),
            },
            2_020,
        )
        .expect("disarm before first scan");
    replacement
        .complete_successful_cycle(2_030)
        .expect("complete first scan report-only");
    let disarmed = replacement.status().expect("disarmed ready status");
    assert_eq!(disarmed.requested_mode, DaemonMode::ReportOnly);
    assert_eq!(disarmed.effective_mode(), DaemonMode::ReportOnly);
    assert_eq!(disarmed.startup_state, StartupState::ReadyReportOnly);

    replacement
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 43,
                instance_id: replacement_status.instance_id,
            },
            2_040,
        )
        .expect("rearm replacement");
    drop(replacement);
    let draining = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("reopen for drain"),
        43,
        4444,
        3_000,
    )
    .expect("begin draining replacement");
    draining
        .finish_startup_recovery(0, 3_010)
        .expect("finish draining replacement recovery");
    let draining_status = draining.status().expect("draining replacement status");
    draining
        .handle_at(
            IpcCommand::BeginDrain {
                activation_generation: 43,
                instance_id: draining_status.instance_id,
            },
            3_020,
        )
        .expect("drain before first scan");
    draining
        .complete_successful_cycle(3_030)
        .expect("draining completion is inert");
    let drained = draining.status().expect("drained status");
    assert!(drained.draining);
    assert_eq!(drained.requested_mode, DaemonMode::ReportOnly);
    assert_eq!(drained.effective_mode(), DaemonMode::ReportOnly);
}

#[test]
fn fatal_first_scan_completion_fails_managed_and_clears_intent() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let first = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("open store"),
        44,
        4242,
        1_000,
    )
    .expect("begin managed control");
    first
        .finish_startup_recovery(0, 1_010)
        .expect("finish first recovery");
    first
        .complete_successful_cycle(1_020)
        .expect("complete first scan");
    let first_status = first.status().expect("first ready status");
    first
        .handle_at(
            IpcCommand::Arm {
                activation_generation: 44,
                instance_id: first_status.instance_id,
            },
            1_030,
        )
        .expect("arm first instance");
    drop(first);

    let control = ControlPlane::begin_managed(
        HistoryStore::open(&database).expect("reopen store"),
        44,
        4343,
        2_000,
    )
    .expect("begin replacement control");
    control
        .finish_startup_recovery(0, 2_010)
        .expect("finish replacement recovery");
    rusqlite::Connection::open(&database)
        .expect("open failure injector")
        .execute_batch(
            "CREATE TRIGGER reject_ready_enforce
             BEFORE UPDATE ON managed_lifecycle
             WHEN NEW.startup_phase = 'ready_enforce'
             BEGIN
                 SELECT RAISE(ABORT, 'injected first scan completion failure');
             END;",
        )
        .expect("install exact completion failure trigger");

    let error = control
        .complete_successful_cycle(2_020)
        .expect_err("durable completion failure is fatal");
    assert!(error.to_string().contains("SQLite"));
    let failed = control.status().expect("failed status");
    assert!(!failed.healthy);
    assert!(!failed.ready);
    assert_eq!(failed.requested_mode, DaemonMode::ReportOnly);
    assert_eq!(failed.effective_mode(), DaemonMode::ReportOnly);
    assert_eq!(failed.startup_state, StartupState::Failed);
    let durable = HistoryStore::open(&database)
        .expect("reopen failed lifecycle")
        .managed_lifecycle()
        .expect("read lifecycle")
        .expect("lifecycle exists");
    assert!(!durable.requested_enforce);
    assert_eq!(
        durable.startup_phase,
        unlinger_daemon::ManagedStartupPhase::Failed
    );
}

#[test]
fn ordinary_schema_v1_status_and_not_found_envelopes_have_stable_json() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    status.instance_id.clear();
    status.healthy = true;
    status.ready = true;
    status.startup_state = StartupState::ReadyReportOnly;
    status.last_scan_at_unix_millis = Some(1_234);
    let control = ControlPlane::new(HistoryStore::open(&database).expect("open store"), status)
        .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let status_response = raw_request(
        &socket,
        r#"{"schema_version":1,"request_id":7,"command":{"command":"status"}}"#,
    );
    assert_eq!(
        status_response,
        serde_json::json!({
            "schema_version": 1,
            "request_id": 7,
            "ok": true,
            "payload": {
                "type": "status",
                "data": {
                    "lifecycle_schema_version": 1,
                    "ipc_schema_version": 1,
                    "database_schema_version": HistoryStore::schema_version(),
                    "daemon_version": env!("CARGO_PKG_VERSION"),
                    "managed": false,
                    "instance_id": "",
                    "healthy": true,
                    "ready": true,
                    "startup_state": "ready_report_only",
                    "mode": "report_only",
                    "requested_mode": "report_only",
                    "effective_mode": "report_only",
                    "draining": false,
                    "recovered_cleanup_attempts": 0,
                    "event_source_healthy": true,
                    "attention": {"blocked_cleanup_count": 0, "items": []},
                    "protected_incident_count": 0,
                    "protected_incidents": [],
                    "pid": 42,
                    "scan_in_progress": false,
                    "cleanup_in_progress": false,
                    "last_scan_at_unix_millis": 1234,
                    "confirmed_incidents": 0,
                    "ambiguous_incidents": 0
                }
            }
        })
    );

    let not_found = raw_request(
        &socket,
        r#"{"schema_version":1,"request_id":8,"command":{"command":"retry_failed_cleanup","incident_id":"missing-golden"}}"#,
    );
    assert_eq!(
        not_found,
        serde_json::json!({
            "schema_version": 1,
            "request_id": 8,
            "ok": false,
            "error": {
                "code": "not_found",
                "message": "incident \"missing-golden\" has no blocked cleanup to retry"
            }
        })
    );
}

#[test]
fn frontend_schema_v3_status_is_public_and_has_explicit_capabilities() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    status.healthy = true;
    status.ready = true;
    status.startup_state = StartupState::ReadyReportOnly;
    status.managed = true;
    status.activation_generation = Some(9);
    status.armed_generation = Some(9);
    status.enforcement_epoch = Some("private-epoch".to_owned());
    status.last_scan_at_unix_millis = Some(1_234);
    let control = ControlPlane::new(HistoryStore::open(&database).expect("open store"), status)
        .expect("restore control state");
    let namespace_token = control
        .store()
        .mutation_namespace_token()
        .expect("mutation namespace");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":3,"request_id":17,"command":{"command":"status"}}"#,
    );
    assert_eq!(
        response,
        serde_json::json!({
            "schema_version": 3,
            "request_id": 17,
            "ok": true,
            "payload": {
                "type": "status",
                "data": {
                    "daemon_version": env!("CARGO_PKG_VERSION"),
                    "healthy": true,
                    "readiness": "ready",
                    "effective_mode": "report_only",
                    "scan_in_progress": false,
                    "cleanup_in_progress": false,
                    "last_scan_at_unix_millis": 1234,
                    "confirmed_incident_count": 0,
                    "ambiguous_incident_count": 0,
                    "event_source": {"healthy": true},
                    "storage": {"healthy": true},
                    "attention": {"total_count": 0, "items": []},
                    "protection": {"total_count": 0, "items": []},
                    "capabilities": {
                        "pause": {"available": true},
                        "resume": {
                            "available": false,
                            "unavailable_reason_id": "action.not_paused"
                        }
                    },
                    "mutation_authority": {
                        "namespace_token": namespace_token,
                        "minimum_reconciliation_window_millis": 1209600000_u64
                    }
                }
            }
        })
    );
    let encoded = response.to_string();
    for forbidden in [
        "pid",
        "instance_id",
        "activation_generation",
        "armed_generation",
        "enforcement_epoch",
        "requested_mode",
        "database_schema_version",
        "last_error",
    ] {
        assert!(!encoded.contains(forbidden), "v3 status leaked {forbidden}");
    }
}

#[test]
fn frontend_schema_v4_browser_overview_is_atomic_typed_and_public_safe() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    status.healthy = true;
    status.ready = true;
    status.startup_state = StartupState::ReadyReportOnly;
    let control = ControlPlane::new(HistoryStore::open(&database).expect("open store"), status)
        .expect("restore control state");
    let cycle = control.begin_observation_cycle(1_000).expect("begin cycle");
    let mut report = confirmed_report("inc-v4", "tracking-private");
    report.signature_pack = "playwright".to_owned();
    report.browser_compatibility = BrowserCompatibility {
        product: BrowserProduct::ChromeForTesting,
        observed_version: Some("151.0.7922.34".to_owned()),
        decision: BrowserCompatibilityDecision::Automatic,
        reason_id: None,
    };
    control
        .publish_roster(&cycle, 1_050, vec![report], true)
        .expect("publish roster");
    control.finish_observation_cycle(&cycle, true);
    control
        .update_status(|status| {
            status.scan_in_progress = false;
            status.latest_observation_at_unix_millis = Some(1_050);
        })
        .expect("finish status");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":4,"request_id":71,"command":{"command":"browser_overview"}}"#,
    );
    assert_eq!(response["schema_version"], 4);
    assert_eq!(response["ok"], true);
    let overview = &response["payload"]["data"];
    assert_eq!(response["payload"]["type"], "browser_overview");
    assert_eq!(overview["freshness"], "current");
    assert_eq!(overview["phase"], "confirmed");
    assert_eq!(overview["effective_mode"], "report_only");
    assert!(overview.get("impact").is_none());
    assert!(overview.get("storage_residue").is_none());
    assert_eq!(overview["sessions"][0]["family"], "playwright");
    assert_eq!(
        overview["sessions"][0]["compatibility"]["decision"],
        "automatic"
    );
    assert_eq!(
        overview["support_catalog"]["families"]
            .as_array()
            .expect("support families")
            .len(),
        3
    );
    for forbidden in [
        "tracking-private",
        "pid",
        "executable_basename",
        "identity_fingerprint",
        "member_fingerprint",
        "targets",
        "runtime_artifacts",
    ] {
        assert!(
            !response.to_string().contains(forbidden),
            "leaked {forbidden}"
        );
    }
}

#[test]
fn frontend_schema_v5_projects_impact_residue_and_observation_spans_without_changing_v4() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let report = confirmed_report("inc-v5-impact", "tracking-private");
    store
        .record_observation(1_000, &report)
        .expect("record first observation");
    store
        .record_observation(2_000, &report)
        .expect("extend observation span");
    let attempt = store
        .begin_cleanup_attempt(2_100, &report, "epoch-v5-impact")
        .expect("begin cleanup attempt");
    journal_delivered_term(&store, &attempt, 2110);
    store
        .complete_cleanup_attempt(
            &attempt,
            2_200,
            &CleanupReceipt {
                incident_id: report.incident_id.clone(),
                state: IncidentState::Cleared,
                reason_id: Some("cleanup.tree_gone_no_revival".to_owned()),
                actions: Vec::new(),
                artifact_actions: Vec::new(),
                survivor_pids: Vec::new(),
                revival_checks_completed: 2,
                resources: CleanupResources {
                    before: Some(ResourceSnapshot {
                        process_count: 3,
                        resident_memory_bytes: 24 * 1024 * 1024,
                    }),
                    after: Some(ResourceSnapshot {
                        process_count: 0,
                        resident_memory_bytes: 0,
                    }),
                    estimated_reclaimed_memory_bytes: Some(24 * 1024 * 1024),
                },
            },
        )
        .expect("complete cleanup attempt");
    store
        .record_storage_residue_observation(&StorageResidueObservation {
            kind: StorageResidueKind::ChromeCodeSignClone,
            status: StorageResidueStatus::Detected,
            observed_at_unix_millis: 2_300,
            candidate_count: 4,
            logical_bytes: 8 * 1024 * 1024,
            shape_complete: true,
            reference_check: StorageResidueReferenceCheck::Incomplete,
            automatic_cleanup_eligible: false,
            reason_ids: vec!["storage_residue.observe_only".to_owned()],
        })
        .expect("record storage residue observation");

    let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    status.healthy = true;
    status.ready = true;
    status.startup_state = StartupState::ReadyReportOnly;
    let control = ControlPlane::new(store, status).expect("restore v5 projection");
    let cycle = control.begin_observation_cycle(3_000).expect("begin cycle");
    control
        .publish_roster(&cycle, 3_050, Vec::new(), true)
        .expect("publish empty current roster");
    control.finish_observation_cycle(&cycle, true);
    control
        .update_status(|status| {
            status.scan_in_progress = false;
            status.latest_observation_at_unix_millis = Some(3_050);
        })
        .expect("finish status projection");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let v5 = raw_request(
        &socket,
        r#"{"schema_version":5,"request_id":73,"command":{"command":"browser_overview"}}"#,
    );
    assert_eq!(v5["schema_version"], 5);
    assert_eq!(v5["payload"]["data"]["impact"]["proved_reclaim_count"], 1);
    assert_eq!(
        v5["payload"]["data"]["impact"]["estimated_reclaimed_memory_bytes"],
        24 * 1024 * 1024
    );
    assert_eq!(
        v5["payload"]["data"]["storage_residue"]["status"],
        "detected"
    );
    assert_eq!(
        v5["payload"]["data"]["storage_residue"]["automatic_cleanup_eligible"],
        false
    );

    let v4 = raw_request(
        &socket,
        r#"{"schema_version":4,"request_id":74,"command":{"command":"browser_overview"}}"#,
    );
    assert!(v4["payload"]["data"].get("impact").is_none());
    assert!(v4["payload"]["data"].get("storage_residue").is_none());

    let history_v5 = raw_request(
        &socket,
        r#"{"schema_version":5,"request_id":75,"command":{"command":"history","limit":20}}"#,
    );
    let observation_v5 = history_v5["payload"]["data"]
        .as_array()
        .expect("v5 history")
        .iter()
        .find(|event| event["payload"]["record_type"] == "observation")
        .expect("v5 observation");
    assert_eq!(observation_v5["observation_span"]["observation_count"], 2);
    assert_eq!(
        observation_v5["observation_span"]["first_observed_at_unix_millis"],
        1_000
    );

    let history_v4 = raw_request(
        &socket,
        r#"{"schema_version":4,"request_id":76,"command":{"command":"history","limit":20}}"#,
    );
    assert!(
        history_v4["payload"]["data"]
            .as_array()
            .expect("v4 history")
            .iter()
            .all(|event| event.get("observation_span").is_none())
    );
}

#[test]
fn v3_rejects_the_browser_overview_without_downgrading() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let control = ControlPlane::new(
        HistoryStore::open(&database).expect("open store"),
        DaemonStatus::new(DaemonMode::ReportOnly, 42),
    )
    .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");
    let response = raw_request(
        &socket,
        r#"{"schema_version":3,"request_id":72,"command":{"command":"browser_overview"}}"#,
    );
    assert_eq!(response["schema_version"], 3);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "invalid_request");
}

#[test]
fn schema_v5_browser_overview_joins_the_exact_recent_settlement_by_event_identity() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let report = confirmed_report("inc-v5-settlement", "tracking-private");

    // Use the same millisecond for all three events. The product join must use
    // the exact cleanup event token and event ordering, never timestamp alone.
    store
        .record_observation(2_000, &report)
        .expect("record settlement source observation");
    let attempt = store
        .begin_cleanup_attempt(2_000, &report, "epoch-v5-settlement")
        .expect("begin settlement attempt");
    journal_delivered_term(&store, &attempt, 2000);
    store
        .complete_cleanup_attempt(
            &attempt,
            2_000,
            &CleanupReceipt {
                incident_id: report.incident_id.clone(),
                state: IncidentState::Cleared,
                reason_id: Some("cleanup.tree_gone_no_revival".to_owned()),
                actions: Vec::new(),
                artifact_actions: Vec::new(),
                survivor_pids: Vec::new(),
                revival_checks_completed: 2,
                resources: CleanupResources {
                    before: Some(ResourceSnapshot {
                        process_count: 3,
                        resident_memory_bytes: 12 * 1024 * 1024,
                    }),
                    after: Some(ResourceSnapshot {
                        process_count: 0,
                        resident_memory_bytes: 0,
                    }),
                    estimated_reclaimed_memory_bytes: Some(12 * 1024 * 1024),
                },
            },
        )
        .expect("complete settlement attempt");
    let failed_report = confirmed_report("inc-v5-later-failure", "tracking-later-failure");
    fail_cleanup(&store, &failed_report, 2_500, "cleanup.signal_rejected");

    let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    status.healthy = true;
    status.ready = true;
    status.startup_state = StartupState::ReadyReportOnly;
    let control = ControlPlane::new(store, status).expect("restore settlement projection");
    let cycle = control.begin_observation_cycle(3_000).expect("begin cycle");
    control
        .publish_roster(&cycle, 3_050, Vec::new(), true)
        .expect("publish empty current roster");
    control.finish_observation_cycle(&cycle, true);
    control
        .update_status(|status| {
            status.scan_in_progress = false;
            status.latest_observation_at_unix_millis = Some(3_050);
        })
        .expect("finish status projection");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let overview = IpcClient::new(&socket)
        .request_browser_overview()
        .expect("typed browser overview");
    let settlement = overview.recent_settlement.expect("recent settlement");
    assert_eq!(settlement.incident_id, "inc-v5-settlement");
    assert_eq!(settlement.family, "agent-browser");
    assert_eq!(settlement.occurred_at_unix_millis, 2_000);
    assert_eq!(settlement.process_count, Some(3));
    assert_eq!(
        settlement.estimated_reclaimed_memory_bytes,
        Some(12 * 1024 * 1024)
    );
    assert_eq!(settlement.revival_checks_completed, 2);
    assert_eq!(
        settlement.artifact_outcome,
        unlinger_protocol::ArtifactOutcome::NotApplicable
    );
    assert_eq!(
        settlement.overall_outcome,
        unlinger_protocol::OverallOutcome::Cleared
    );
}

#[test]
fn frontend_schema_v3_lifecycle_capabilities_match_authoritative_policy() {
    for (startup_state, readiness, reason_id) in [
        (StartupState::Draining, "draining", "action.daemon_draining"),
        (StartupState::Failed, "failed", "action.daemon_failed"),
    ] {
        let temp = TempState::new();
        let database = temp.directory.join("history.sqlite3");
        let socket = temp.directory.join("unlingerd.sock");
        let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
        status.healthy = startup_state != StartupState::Failed;
        status.ready = false;
        status.startup_state = startup_state;
        let control = ControlPlane::new(HistoryStore::open(&database).expect("open store"), status)
            .expect("restore control state");
        let _server = IpcServer::start(&socket, control).expect("start IPC server");

        let response = raw_request(
            &socket,
            r#"{"schema_version":3,"request_id":18,"command":{"command":"status"}}"#,
        );
        assert_eq!(
            response.pointer("/payload/data/readiness"),
            Some(&serde_json::json!(readiness))
        );
        for action in ["pause", "resume"] {
            assert_eq!(
                response.pointer(&format!("/payload/data/capabilities/{action}/available")),
                Some(&serde_json::json!(false))
            );
            assert_eq!(
                response.pointer(&format!(
                    "/payload/data/capabilities/{action}/unavailable_reason_id"
                )),
                Some(&serde_json::json!(reason_id))
            );
        }
    }
}

#[test]
fn frontend_schema_v3_history_strips_process_and_storage_identities() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let report = confirmed_report("inc-public-history", "tracking-private");
    store
        .record_observation(1_000, &report)
        .expect("record observation");
    fail_cleanup(&store, &report, 2_000, "cleanup.signal_rejected");
    let control = ControlPlane::new(
        store,
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":3,"request_id":18,"command":{"command":"history","limit":20}}"#,
    );
    assert_eq!(response["schema_version"], 3);
    assert_eq!(response["ok"], true);
    assert_eq!(response["payload"]["type"], "history");
    let encoded = response.to_string();
    assert!(encoded.contains("inc-public-history"));
    assert!(encoded.contains("agent-browser"));
    for forbidden in [
        "tracking-private",
        "session-redacted",
        "identity-inc-public-history",
        "members-inc-public-history",
        "\"pid\"",
        "identity_fingerprint",
        "member_fingerprint",
        "attempt_id",
        "event_id",
        "survivor_pids",
        "artifact_fingerprint",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "v3 history leaked {forbidden}"
        );
    }
}

#[test]
fn frontend_schema_v3_observation_roster_is_public_redacted_and_fresh() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    let control = ControlPlane::new(HistoryStore::open(&database).expect("open store"), status)
        .expect("restore control state");
    let cycle_token = control
        .begin_observation_cycle(100)
        .expect("begin observation cycle");
    control
        .publish_roster(
            &cycle_token,
            101,
            vec![confirmed_report(
                "inc-public-roster",
                "tracking-private-roster",
            )],
            true,
        )
        .expect("publish observation roster");
    control.finish_observation_cycle(&cycle_token, true);
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":3,"request_id":31,"command":{"command":"incidents"}}"#,
    );
    assert_eq!(
        response,
        serde_json::json!({
            "schema_version": 3,
            "request_id": 31,
            "ok": true,
            "payload": {
                "type": "incidents",
                "data": {
                    "cycle_token": cycle_token,
                    "observed_at_unix_millis": 101,
                    "freshness": "current",
                    "items": [{
                        "incident_id": "inc-public-roster",
                        "observation": {
                        "family": "agent-browser",
                        "family_version": "0.1.0",
                        "state": "CONFIRMED",
                        "executable_basename": "node",
                        "member_count": 1,
                        "resident_memory_bytes": 4096,
                        "roles": [{"role": "controller", "count": 1}],
                        "evidence": [],
                        "gates": {
                            "same_user": true,
                            "strong_automation_provenance": true,
                            "confirmed_abandonment": true,
                            "isolated_session": true,
                            "stable_across_two_observations": true,
                            "process_identity_unchanged": true,
                            "no_protection_rule": true
                        }
                        }
                    }]
                }
            }
        })
    );
    let encoded = response.to_string();
    for forbidden in [
        "tracking-private-roster",
        "tracking_key",
        "session_fingerprint",
        "member_fingerprint",
        "identity_fingerprint",
        "targets",
        "pid",
    ] {
        assert!(!encoded.contains(forbidden), "v3 roster leaked {forbidden}");
    }
}

#[test]
fn frontend_schema_v3_observation_roster_is_bounded() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    let control = ControlPlane::new(HistoryStore::open(&database).expect("open store"), status)
        .expect("restore control state");
    let reports = (0..40)
        .map(|index| confirmed_report(&format!("inc-roster-{index}"), &format!("tracking-{index}")))
        .collect::<Vec<_>>();
    let cycle_token = control
        .begin_observation_cycle(100)
        .expect("begin observation cycle");
    control
        .publish_roster(&cycle_token, 101, reports, true)
        .expect("publish observation roster");
    control.finish_observation_cycle(&cycle_token, true);
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":3,"request_id":32,"command":{"command":"incidents"}}"#,
    );
    let data = response["payload"]["data"]["items"]
        .as_array()
        .expect("roster array");
    assert_eq!(data.len(), 32, "roster must stay bounded");
}

#[test]
fn frontend_schema_v3_roster_never_observed_has_no_fake_token_or_timestamp() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let control = ControlPlane::new(
        HistoryStore::open(&database).expect("open store"),
        DaemonStatus::new(DaemonMode::ReportOnly, 42),
    )
    .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":3,"request_id":33,"command":{"command":"incidents"}}"#,
    );
    let data = &response["payload"]["data"];
    assert_eq!(data["freshness"], "never_observed");
    assert_eq!(data["items"], serde_json::json!([]));
    assert!(data.get("cycle_token").is_none());
    assert!(data.get("observed_at_unix_millis").is_none());
}

#[test]
fn frontend_schema_v3_cannot_invoke_service_lifecycle_commands() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let control = ControlPlane::new(
        HistoryStore::open(&database).expect("open store"),
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":3,"request_id":19,"command":{"command":"arm","activation_generation":9,"instance_id":"private"}}"#,
    );
    assert_eq!(response["schema_version"], 3);
    assert_eq!(response["request_id"], 19);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "invalid_json");
}

#[test]
fn superseded_frontend_schema_v2_is_rejected_before_dispatch() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let control = ControlPlane::new(
        HistoryStore::open(&database).expect("open store"),
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");

    let response = raw_request(
        &socket,
        r#"{"schema_version":2,"request_id":34,"command":{"command":"pause","duration_millis":3600000}}"#,
    );
    assert_eq!(response["schema_version"], 1);
    assert_eq!(response["request_id"], 34);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "unsupported_schema");
    assert!(response.get("payload").is_none());
}

#[test]
fn frontend_schema_v3_errors_do_not_echo_private_request_values() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let control = ControlPlane::new(
        HistoryStore::open(&database).expect("open store"),
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let namespace_token = control
        .store()
        .mutation_namespace_token()
        .expect("mutation namespace");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");
    let private_value = "missing-private-correlation";
    let request = serde_json::json!({
        "schema_version": 3,
        "request_id": 20,
        "command": {
            "command": "retry_failed_cleanup",
            "context": {
                "namespace_token": namespace_token,
                "mutation_id": "11111111-1111-4111-8111-111111111111"
            },
            "incident_id": private_value
        }
    })
    .to_string();

    let response = raw_request(&socket, &request);
    assert_eq!(response["schema_version"], 3);
    assert_eq!(response["request_id"], 20);
    assert_eq!(response["ok"], true);
    assert_eq!(response["payload"]["type"], "mutation_committed");
    assert_eq!(
        response["payload"]["data"]["outcome"]["outcome"],
        "rejected"
    );
    assert_eq!(
        response["payload"]["data"]["outcome"]["reason_id"],
        "action.no_blocked_cleanup"
    );
    assert!(!response.to_string().contains(private_value));
}

#[test]
fn frontend_schema_v3_receipt_replay_precedes_failed_lifecycle_policy() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let mut status = DaemonStatus::new(DaemonMode::ReportOnly, std::process::id());
    status.healthy = true;
    status.ready = true;
    status.startup_state = StartupState::ReadyReportOnly;
    let control = ControlPlane::new(HistoryStore::open(&database).expect("open store"), status)
        .expect("restore control state");
    let namespace_token = control
        .store()
        .mutation_namespace_token()
        .expect("mutation namespace");
    let _server = IpcServer::start(&socket, control.clone()).expect("start IPC server");
    let context = serde_json::json!({
        "namespace_token": namespace_token,
        "mutation_id": "99999999-9999-4999-8999-999999999999"
    });
    let first = raw_request(
        &socket,
        &serde_json::json!({
            "schema_version": 3,
            "request_id": 40,
            "command": {
                "command": "pause",
                "context": context,
                "duration_millis": 500
            }
        })
        .to_string(),
    );
    assert_eq!(first["ok"], true);
    assert_eq!(first["payload"]["data"]["outcome"]["outcome"], "applied");
    let stored_receipt = first["payload"]["data"].clone();

    control
        .fail_closed(2_000, "test lifecycle failure")
        .expect("fail lifecycle");
    let replay = raw_request(
        &socket,
        &serde_json::json!({
            "schema_version": 3,
            "request_id": 41,
            "command": {
                "command": "pause",
                "context": context,
                "duration_millis": 500
            }
        })
        .to_string(),
    );
    assert_eq!(replay["ok"], true);
    assert_eq!(replay["payload"]["data"], stored_receipt);

    let rejected_context = serde_json::json!({
        "namespace_token": namespace_token,
        "mutation_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
    });
    let rejected = raw_request(
        &socket,
        &serde_json::json!({
            "schema_version": 3,
            "request_id": 42,
            "command": {
                "command": "resume",
                "context": rejected_context
            }
        })
        .to_string(),
    );
    assert_eq!(rejected["ok"], true);
    assert_eq!(
        rejected["payload"]["data"]["outcome"],
        serde_json::json!({
            "outcome": "rejected",
            "reason_id": "action.daemon_failed"
        })
    );
    assert_eq!(
        rejected["payload"]["data"]["policy_revision_after"],
        stored_receipt["policy_revision_after"]
    );

    let status_response = raw_request(
        &socket,
        &serde_json::json!({
            "schema_version": 3,
            "request_id": 43,
            "command": {
                "command": "mutation_status",
                "context": context
            }
        })
        .to_string(),
    );
    assert_eq!(status_response["ok"], true);
    assert_eq!(status_response["payload"]["data"]["status"], "committed");
    assert_eq!(
        status_response["payload"]["data"]["receipt"],
        stored_receipt
    );
}

#[test]
fn retry_failed_cleanup_roundtrip_is_named_and_not_found_is_typed() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let first = confirmed_report("inc-first", "tracking-first");
    let second = confirmed_report("inc-second", "tracking-second");
    fail_cleanup(&store, &first, 1_000, "cleanup.signal_rejected");
    fail_cleanup(&store, &second, 2_000, "cleanup.supervisor_revival");
    let control = ControlPlane::new(
        store.clone(),
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");
    let client = IpcClient::new(&socket);

    let response = client
        .request(IpcCommand::RetryFailedCleanup {
            incident_id: "inc-first".to_owned(),
        })
        .expect("schedule named retry");
    assert_eq!(
        response,
        IpcPayload::RetryScheduled {
            incident_id: "inc-first".to_owned()
        }
    );
    assert!(!store.cleanup_blocked("inc-first").expect("first cleared"));
    assert!(
        store
            .cleanup_blocked("inc-second")
            .expect("second isolated")
    );

    let missing = client
        .request(IpcCommand::RetryFailedCleanup {
            incident_id: "inc-first".to_owned(),
        })
        .expect_err("already-cleared block is not found");
    assert!(missing.to_string().contains("not_found"));
    assert!(store.cleanup_blocked("inc-second").expect("still isolated"));
}

#[test]
fn protect_unprotect_roundtrip_is_exact_idempotent_named_and_typed() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let first = confirmed_report("inc-protected", "tracking-protected");
    let second = confirmed_report("inc-other", "tracking-other");
    store
        .record_observation(1_000, &first)
        .expect("record first observation");
    store
        .record_observation(1_010, &second)
        .expect("record second observation");
    let control = ControlPlane::new(
        store.clone(),
        DaemonStatus::new(DaemonMode::ReportOnly, std::process::id()),
    )
    .expect("restore control state");
    let _server = IpcServer::start(&socket, control).expect("start IPC server");
    let client = IpcClient::new(&socket);

    let protected = client
        .request(IpcCommand::ProtectIncident {
            incident_id: first.incident_id.clone(),
        })
        .expect("protect observed incident");
    let IpcPayload::IncidentProtected { protection } = protected else {
        panic!("expected incident protection payload")
    };
    assert_eq!(protection.incident_id, first.incident_id);
    assert!(
        store
            .is_incident_protected(&first)
            .expect("exact incident protected")
    );

    let repeated = client
        .request(IpcCommand::ProtectIncident {
            incident_id: first.incident_id.clone(),
        })
        .expect("repeated protect is idempotent");
    assert_eq!(
        repeated,
        IpcPayload::IncidentProtected {
            protection: protection.clone()
        }
    );

    client
        .request(IpcCommand::ProtectIncident {
            incident_id: second.incident_id.clone(),
        })
        .expect("protect second incident");
    let IpcPayload::Status(status) = client
        .request(IpcCommand::Status)
        .expect("project protections into status")
    else {
        panic!("expected status payload")
    };
    assert_eq!(status.protected_incident_count, 2);
    assert_eq!(status.protected_incidents.len(), 2);
    let serialized = serde_json::to_string(&status).expect("serialize status");
    assert!(!serialized.contains(&first.root.identity_fingerprint));
    assert!(!serialized.contains(&first.member_fingerprint));
    assert!(!serialized.contains("session-redacted"));

    let missing = client
        .request(IpcCommand::ProtectIncident {
            incident_id: "inc-missing".to_owned(),
        })
        .expect_err("unobserved incident is typed not found");
    assert!(missing.to_string().contains("not_found"));

    assert_eq!(
        client
            .request(IpcCommand::UnprotectIncident {
                incident_id: first.incident_id.clone(),
            })
            .expect("unprotect exact incident"),
        IpcPayload::IncidentUnprotected {
            incident_id: first.incident_id.clone()
        }
    );
    assert!(
        !store
            .is_incident_protected(&first)
            .expect("first no longer protected")
    );
    assert!(
        store
            .is_incident_protected(&second)
            .expect("second protection remains isolated")
    );
    let already_unprotected = client
        .request(IpcCommand::UnprotectIncident {
            incident_id: first.incident_id,
        })
        .expect_err("named protection already absent");
    assert!(already_unprotected.to_string().contains("not_found"));
}

#[test]
fn status_projects_bounded_cleanup_and_event_source_attention() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let socket = temp.directory.join("unlingerd.sock");
    let store = HistoryStore::open(&database).expect("open store");
    let report = confirmed_report("inc-attention", "tracking-attention");
    fail_cleanup(&store, &report, 1_000, "cleanup.signal_rejected");
    let mut initial = DaemonStatus::new(DaemonMode::ReportOnly, std::process::id());
    initial.healthy = true;
    let control = ControlPlane::new(store, initial).expect("restore control state");
    control
        .note_event_source_failure()
        .expect("record native event source degradation");
    let _server = IpcServer::start(&socket, control.clone()).expect("start IPC server");
    let client = IpcClient::new(&socket);

    let IpcPayload::Status(status) = client
        .request(IpcCommand::Status)
        .expect("read projected status")
    else {
        panic!("expected status payload")
    };
    assert!(!status.event_source_healthy);
    assert_eq!(
        status.last_event_source_error.as_deref(),
        Some("native event source unavailable; periodic reconciliation remains active")
    );
    assert_eq!(status.attention.blocked_cleanup_count, 1);
    assert!(status.attention.items.len() <= 16);
    assert!(
        status
            .attention
            .items
            .iter()
            .any(|item| item.kind == AttentionKind::EventSourceDegraded)
    );
    let cleanup = status
        .attention
        .items
        .iter()
        .find(|item| item.kind == AttentionKind::CleanupFailed)
        .expect("failed cleanup attention");
    assert_eq!(cleanup.incident_id.as_deref(), Some("inc-attention"));
    assert_eq!(cleanup.reason_id, "cleanup.signal_rejected");

    control
        .note_event_source_healthy()
        .expect("event source recovered");
    let status = control.status().expect("refreshed status");
    assert!(status.event_source_healthy);
    assert_eq!(status.last_event_source_error, None);
    assert!(
        status
            .attention
            .items
            .iter()
            .all(|item| item.kind != AttentionKind::EventSourceDegraded)
    );
}

#[test]
fn malformed_history_projection_cannot_erase_independent_impact_or_durable_pause() {
    let temp = TempState::new();
    let database = temp.directory.join("history.sqlite3");
    let store = HistoryStore::open(&database).expect("open store");
    store
        .set_pause_until(Some(88_000))
        .expect("persist owner pause");
    let report = confirmed_report("inc-malformed", "tracking-malformed");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-malformed")
        .expect("begin cleared fixture attempt");
    journal_delivered_term(&store, &attempt, 1_000);
    store
        .complete_cleanup_attempt(
            &attempt,
            1_001,
            &CleanupReceipt {
                incident_id: report.incident_id.clone(),
                state: IncidentState::Cleared,
                reason_id: Some("cleanup.tree_gone_no_revival".to_owned()),
                actions: Vec::new(),
                artifact_actions: Vec::new(),
                survivor_pids: Vec::new(),
                revival_checks_completed: 2,
                resources: CleanupResources::default(),
            },
        )
        .expect("terminalize cleared fixture attempt");
    drop(store);

    let connection = rusqlite::Connection::open(&database).expect("open projection fixture");
    connection
        .execute(
            "UPDATE events SET payload_json = '{' WHERE kind = 'cleanup'",
            [],
        )
        .expect("corrupt only the derived cleanup projection");
    drop(connection);

    let reopened = HistoryStore::open(&database).expect("schema remains structurally valid");
    let control = ControlPlane::new(
        reopened.clone(),
        DaemonStatus::new(DaemonMode::Enforce, std::process::id()),
    )
    .expect("independent impact authority restores without parsing bounded history");
    assert!(
        control
            .status()
            .expect("restored status")
            .most_recent_reclaim
            .is_some()
    );
    assert!(
        reopened.history(20).is_err(),
        "the malformed optional history projection must remain visibly unreadable"
    );
    assert_eq!(
        reopened.pause_until().expect("read preserved owner pause"),
        Some(88_000)
    );
}

fn journal_delivered_term(
    store: &HistoryStore,
    attempt: &unlinger_daemon::CleanupAttemptHandle,
    now: u64,
) {
    let intent = unlinger_core::CleanupActionIntent {
        stage: unlinger_core::CleanupStage::PrimaryTerm,
        pid: 4242,
        identity_fingerprint: "identity-redacted".to_owned(),
        signal: unlinger_core::CleanupSignal::Term,
    };
    let prepared = store
        .prepare_cleanup_action(attempt, 0, now, &intent)
        .unwrap();
    store
        .complete_cleanup_action(&prepared, now, unlinger_core::SignalDisposition::Delivered)
        .unwrap();
}

#[test]
fn empty_roster_requires_complete_observation_and_known_sessions_remain_visible() {
    let temp = TempState::new();
    let socket = temp.directory.join("overview.sock");
    let mut status = DaemonStatus::new(DaemonMode::ReportOnly, 42);
    status.healthy = true;
    status.ready = true;
    status.startup_state = StartupState::ReadyReportOnly;
    let control = ControlPlane::new(
        HistoryStore::open(temp.directory.join("history.sqlite3")).unwrap(),
        status,
    )
    .unwrap();
    let _server = IpcServer::start(&socket, control.clone()).unwrap();
    for (complete, reports, expected) in [
        (false, Vec::new(), "unknown"),
        (true, Vec::new(), "clear"),
        (
            false,
            vec![confirmed_report("known", "tracking")],
            "confirmed",
        ),
    ] {
        let cycle = control.begin_observation_cycle(1_000).unwrap();
        control
            .publish_roster(&cycle, 1_050, reports, complete)
            .unwrap();
        control.finish_observation_cycle(&cycle, true);
        control
            .update_status(|status| status.scan_in_progress = false)
            .unwrap();
        let response = raw_request(
            &socket,
            r#"{"schema_version":5,"request_id":90,"command":{"command":"browser_overview"}}"#,
        );
        assert_eq!(response["payload"]["data"]["phase"], expected);
    }
}

#[test]
fn no_intervention_receipt_has_no_reclaim_settlement_or_memory_claim_over_ipc() {
    let temp = TempState::new();
    let socket = temp.directory.join("attribution.sock");
    let store = HistoryStore::open(temp.directory.join("history.sqlite3")).unwrap();
    let report = confirmed_report("no-intervention", "synthetic-tracking");
    let attempt = store
        .begin_cleanup_attempt(1_000, &report, "epoch-test")
        .unwrap();
    store
        .complete_cleanup_attempt(
            &attempt,
            1_010,
            &CleanupReceipt {
                incident_id: report.incident_id,
                state: IncidentState::Cleared,
                reason_id: Some("cleanup.tree_gone_no_revival".to_owned()),
                actions: Vec::new(),
                artifact_actions: Vec::new(),
                survivor_pids: Vec::new(),
                revival_checks_completed: 2,
                resources: CleanupResources {
                    estimated_reclaimed_memory_bytes: Some(8_192),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let control = ControlPlane::new(store, DaemonStatus::new(DaemonMode::ReportOnly, 42)).unwrap();
    let _server = IpcServer::start(&socket, control).unwrap();
    let history = raw_request(
        &socket,
        r#"{"schema_version":5,"request_id":91,"command":{"command":"history","limit":10}}"#,
    );
    let receipt = &history["payload"]["data"][0]["payload"]["cleanup"];
    assert_eq!(receipt["process_outcome"], "cleared");
    assert_eq!(receipt["reason_id"], "cleanup.tree_gone_without_signal");
    assert!(receipt["resources"]["estimated_reclaimed_memory_bytes"].is_null());
    let overview = raw_request(
        &socket,
        r#"{"schema_version":5,"request_id":92,"command":{"command":"browser_overview"}}"#,
    );
    assert!(overview["payload"]["data"]["recent_settlement"].is_null());
    assert_eq!(
        overview["payload"]["data"]["impact"]["proved_reclaim_count"],
        0
    );
}
