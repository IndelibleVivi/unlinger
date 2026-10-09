use super::*;
use crate::{DaemonMode, DaemonStatus, HistoryStore, IpcCommand, StartupState};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::atomic::{AtomicU64, Ordering};
use unlinger_protocol::NodeCompileCacheAttemptSummary;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    temp: PathBuf,
    bucket_name: CString,
}
impl Fixture {
    fn new() -> Self {
        let temp = std::env::temp_dir().join(format!(
            "unlinger-node-cache-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&temp).unwrap();
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o700)).unwrap();
        // Production obtains the canonical Darwin directory. Keep the fixture
        // equivalent; /var is a system symlink and O_NOFOLLOW_ANY rejects it.
        let temp = fs::canonicalize(temp).unwrap();
        let bucket_name = CString::new(format!("v26.7.0-arm64-12345678-{}", unsafe {
            libc::geteuid()
        }))
        .unwrap();
        let value = Self { temp, bucket_name };
        fs::create_dir_all(value.bucket()).unwrap();
        value
    }
    fn root(&self) -> PathBuf {
        self.temp.join("node-compile-cache")
    }
    fn bucket(&self) -> PathBuf {
        self.root().join(self.bucket_name.to_str().unwrap())
    }
    fn seed(&self, name: &str) -> u64 {
        // Public format vector: IEEE CRC32("123456789") = cbf43926.
        // A producer test below separately validates real V8-generated files.
        let mut bytes = Vec::new();
        for value in [MAGIC, 9u32, 9u32, 0xcbf4_3926, 0xcbf4_3926] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(b"123456789");
        fs::write(self.bucket().join(name), &bytes).unwrap();
        fs::set_permissions(self.bucket().join(name), fs::Permissions::from_mode(0o600)).unwrap();
        bytes.len() as u64
    }
    fn adapter(&self) -> Result<NodeCompileCacheMaintenance, ToolCacheAvailability> {
        let producer = std::env::current_exe().unwrap();
        let binding = ProducerBinding::capture(&producer).unwrap();
        NodeCompileCacheMaintenance::open_at(
            &self.temp,
            producer,
            binding,
            self.bucket_name.clone(),
            &|| false,
        )
    }
    fn control(&self, enforce: bool) -> ControlPlane {
        let store = HistoryStore::open(self.temp.join("history.sqlite3")).unwrap();
        let mut status = DaemonStatus::new(
            if enforce {
                DaemonMode::Enforce
            } else {
                DaemonMode::ReportOnly
            },
            std::process::id(),
        );
        status.healthy = true;
        status.ready = true;
        status.startup_state = if enforce {
            StartupState::ReadyEnforce
        } else {
            StartupState::ReadyReportOnly
        };
        status.enforcement_epoch = enforce.then(|| "node-test-epoch".into());
        ControlPlane::new(store, status).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp);
    }
}

#[test]
fn validated_files_are_deleted_only_under_prepared_exact_enforce_lease() {
    let fixture = Fixture::new();
    let bytes = fixture.seed("12345678");
    fixture.seed("abcdef01");
    let adapter = fixture.adapter().ok().unwrap();
    let control = fixture.control(true);
    let (prepared, plan, epoch) = control
        .start_node_compile_cache_if_ready_enforce(10, |token, _| {
            assert!(!token.is_empty());
            let latest = control
                .store()
                .latest_node_compile_cache_maintenance()
                .unwrap()
                .unwrap();
            assert_eq!(
                latest.last_attempt.unwrap().outcome,
                ToolCacheOutcome::Running
            );
            assert!(fixture.bucket().join("12345678").exists());
            Ok(adapter)
        })
        .unwrap()
        .unwrap();
    let result = plan.unwrap().execute(&control, &prepared, &epoch, || false);
    assert_eq!(
        result,
        NodeCompileCacheResult {
            outcome: ToolCacheOutcome::Completed,
            removed_entry_count: Some(2),
            removed_logical_bytes: Some(bytes * 2)
        }
    );
    assert!(fixture.bucket().exists());
    assert!(fixture.root().exists());
    assert_eq!(fs::read_dir(fixture.bucket()).unwrap().count(), 0);
    control.finish_tool_cache_action(&prepared).unwrap();
}

#[test]
fn report_only_never_transfers_or_executes_plan() {
    let fixture = Fixture::new();
    fixture.seed("12345678");
    let control = fixture.control(false);
    assert!(
        control
            .start_node_compile_cache_if_ready_enforce(10, |_, _| -> io::Result<()> {
                panic!("report-only must never execute")
            })
            .unwrap()
            .is_none()
    );
    assert!(fixture.bucket().join("12345678").exists());
    assert!(
        control
            .store()
            .latest_node_compile_cache_maintenance()
            .unwrap()
            .is_none()
    );
}

#[test]
fn prepared_failure_never_transfers_a_plan() {
    let fixture = Fixture::new();
    fixture.seed("12345678");
    let control = fixture.control(true);
    let connection = rusqlite::Connection::open(control.store().path()).unwrap();
    connection
        .execute_batch("DROP TABLE node_compile_cache_latest")
        .unwrap();
    assert!(
        control
            .start_node_compile_cache_if_ready_enforce(10, |_, _| -> io::Result<()> {
                panic!("PREPARED failure must prevent execution")
            })
            .is_err()
    );
    assert!(fixture.bucket().join("12345678").exists());
}

#[test]
fn v13_migration_preserves_uv_and_retired_evidence_without_adopting_node_results() {
    let fixture = Fixture::new();
    let store = HistoryStore::open(fixture.temp.join("migration.sqlite3")).unwrap();
    store
        .record_tool_cache_observation(1, ToolCacheAvailability::Available)
        .unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    connection
        .execute_batch("DROP TABLE node_compile_cache_latest; PRAGMA user_version=13;")
        .unwrap();
    drop(connection);
    let path = store.path().to_owned();
    drop(store);
    let migrated = HistoryStore::open(path).unwrap();
    assert_eq!(HistoryStore::schema_version(), 14);
    let uv = migrated.latest_tool_cache_maintenance().unwrap().unwrap();
    assert_eq!(uv.kind, ToolCacheKind::UvCache);
    assert_eq!(uv.observed_at_unix_millis, 1);
    assert!(
        migrated
            .latest_node_compile_cache_maintenance()
            .unwrap()
            .is_none()
    );
    assert!(migrated.retired_npm_cache_maintenance().unwrap().is_none());
}

#[test]
fn format_integrity_and_foreign_content_block_whole_bucket() {
    for variant in [
        "foreign",
        "wrong-magic",
        "wrong-size",
        "wrong-crc",
        "extra-tail",
        "upper-name",
        "subdirectory",
        "fifo",
        "hardlink",
        "symlink",
        "writable",
    ] {
        let fixture = Fixture::new();
        fixture.seed("12345678");
        let other = fixture.bucket().join("abcdef01");
        match variant {
            "foreign" => fs::write(fixture.bucket().join("notes.txt"), b"user artifact").unwrap(),
            "upper-name" => {
                fixture.seed("ABCDEF01");
            }
            "subdirectory" => fs::create_dir(other).unwrap(),
            "fifo" => {
                let name = CString::new(other.to_str().unwrap()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "hardlink" => fs::hard_link(fixture.bucket().join("12345678"), other).unwrap(),
            "symlink" => symlink(fixture.bucket().join("12345678"), other).unwrap(),
            "writable" => fs::set_permissions(
                fixture.bucket().join("12345678"),
                fs::Permissions::from_mode(0o666),
            )
            .unwrap(),
            _ => {
                fixture.seed("abcdef01");
                let mut bytes = fs::read(&other).unwrap();
                match variant {
                    "wrong-magic" => bytes[0] ^= 1,
                    "wrong-size" => bytes[8] ^= 1,
                    "wrong-crc" => bytes[20] ^= 1,
                    "extra-tail" => bytes.push(0),
                    _ => unreachable!(),
                };
                fs::write(&other, bytes).unwrap();
            }
        }
        assert!(
            matches!(fixture.adapter(), Err(ToolCacheAvailability::Unavailable)),
            "{variant}"
        );
        assert!(fixture.bucket().join("12345678").exists(), "{variant}");
    }
}

#[test]
fn default_tag_only_unknown_version_and_custom_root_are_retained() {
    let fixture = Fixture::new();
    fixture.seed("12345678");
    let unknown = fixture.root().join("v99.0.0-arm64-12345678-501");
    fs::create_dir(&unknown).unwrap();
    fs::write(unknown.join("manual"), b"untouched").unwrap();
    let custom = fixture.temp.join("custom-cache");
    fs::create_dir(&custom).unwrap();
    fs::write(custom.join("file"), b"custom").unwrap();
    let result = fixture.adapter().ok().unwrap().remove_files(&|| false);
    assert_eq!(result.outcome, ToolCacheOutcome::Completed);
    assert_eq!(fs::read(unknown.join("manual")).unwrap(), b"untouched");
    assert_eq!(fs::read(custom.join("file")).unwrap(), b"custom");
    fs::rename(fixture.bucket(), fixture.root().join("unsupported-tag")).unwrap();
    assert!(matches!(
        fixture.adapter(),
        Err(ToolCacheAvailability::Unsupported)
    ));
}

#[test]
fn temporary_writer_blocks_discovery_and_defers_prepared_execution() {
    let fixture = Fixture::new();
    fixture.seed("12345678");
    let adapter = fixture.adapter().ok().unwrap();
    fs::write(
        fixture.bucket().join("abcdef01.Abc123"),
        b"in-progress writer",
    )
    .unwrap();
    assert!(matches!(
        fixture.adapter(),
        Err(ToolCacheAvailability::Unavailable)
    ));
    assert_eq!(
        adapter.remove_files(&|| false).outcome,
        ToolCacheOutcome::Busy
    );
    assert!(fixture.bucket().join("12345678").exists());
}

#[test]
fn root_bucket_and_file_replacement_cannot_redirect_deletion() {
    for variant in ["root", "bucket", "file", "temp"] {
        let fixture = Fixture::new();
        fixture.seed("12345678");
        let adapter = fixture.adapter().ok().unwrap();
        match variant {
            "root" => {
                fs::rename(fixture.root(), fixture.temp.join("old-root")).unwrap();
                fs::create_dir(fixture.root()).unwrap();
            }
            "bucket" => {
                fs::rename(fixture.bucket(), fixture.root().join("old-bucket")).unwrap();
                fs::create_dir(fixture.bucket()).unwrap();
            }
            "file" => {
                fs::rename(
                    fixture.bucket().join("12345678"),
                    fixture.temp.join("old-file"),
                )
                .unwrap();
                fixture.seed("12345678");
            }
            "temp" => {
                let moved = fixture.temp.with_extension("moved");
                fs::rename(&fixture.temp, &moved).unwrap();
                fs::create_dir(&fixture.temp).unwrap();
                assert_eq!(
                    adapter.remove_files(&|| false).outcome,
                    ToolCacheOutcome::Failed
                );
                fs::remove_dir(&fixture.temp).unwrap();
                fs::rename(moved, &fixture.temp).unwrap();
                continue;
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            adapter.remove_files(&|| false).outcome,
            ToolCacheOutcome::Failed | ToolCacheOutcome::Busy
        ));
        if variant == "file" {
            assert!(fixture.bucket().join("12345678").exists());
        }
    }
}

#[test]
fn pause_then_immediate_resume_never_resurrects_partial_lease() {
    let fixture = Fixture::new();
    fixture.seed("12345678");
    fixture.seed("abcdef01");
    let control = fixture.control(true);
    let adapter = fixture.adapter().ok().unwrap();
    let (prepared, plan, epoch) = control
        .start_node_compile_cache_if_ready_enforce(10, |_, _| Ok(adapter))
        .unwrap()
        .unwrap();
    let pause = std::cell::Cell::new(false);
    let result = plan.unwrap().execute(&control, &prepared, &epoch, || {
        if !pause.get() && !fixture.bucket().join("12345678").exists() {
            pause.set(true);
            control
                .handle_at(
                    IpcCommand::Pause {
                        duration_millis: 1000,
                    },
                    11,
                )
                .unwrap();
            control.handle_at(IpcCommand::Resume, 12).unwrap();
        }
        false
    });
    assert!(pause.get());
    assert_eq!(result.outcome, ToolCacheOutcome::Failed);
    assert!(result.removed_entry_count.is_none());
    assert!(fixture.bucket().join("abcdef01").exists());
    assert!(!control.tool_cache_action_may_continue(&prepared, &epoch));
    control.finish_tool_cache_action(&prepared).unwrap();
}

#[test]
fn node_store_is_independent_recovers_unknown_and_rejects_late_or_uv_results() {
    let fixture = Fixture::new();
    let control = fixture.control(false);
    let store = control.store();
    store
        .record_tool_cache_observation(1, ToolCacheAvailability::Available)
        .unwrap();
    let uv = store.begin_tool_cache_attempt(2).unwrap();
    let prepared = store.begin_node_compile_cache_attempt(3).unwrap();
    let result = NodeCompileCacheAttemptSummary {
        outcome: ToolCacheOutcome::Completed,
        prepared_at_unix_millis: 3,
        completed_at_unix_millis: Some(4),
        removed_entry_count: Some(2),
        removed_logical_bytes: Some(58),
    };
    assert!(
        store
            .complete_node_compile_cache_attempt(&uv, &result, ToolCacheAvailability::Available)
            .is_err()
    );
    assert!(store.begin_node_compile_cache_attempt(4).is_err());
    assert!(store.recover_node_compile_cache_attempt(5).unwrap());
    assert!(!store.recover_node_compile_cache_attempt(6).unwrap());
    assert!(
        store
            .complete_node_compile_cache_attempt(
                &prepared,
                &result,
                ToolCacheAvailability::Available
            )
            .is_err()
    );
    store
        .record_node_compile_cache_observation(7, ToolCacheAvailability::Available)
        .unwrap();
    let latest = store
        .latest_node_compile_cache_maintenance()
        .unwrap()
        .unwrap()
        .last_attempt
        .unwrap();
    assert_eq!(latest.outcome, ToolCacheOutcome::DeliveryUnknown);
    assert!(latest.removed_entry_count.is_none());
    assert_eq!(
        store
            .latest_tool_cache_maintenance()
            .unwrap()
            .unwrap()
            .last_attempt
            .unwrap()
            .outcome,
        ToolCacheOutcome::Running
    );
}

#[test]
fn successful_zero_result_is_durable_and_has_no_native_accounting_fields() {
    let fixture = Fixture::new();
    let store = fixture.control(false).store().clone();
    let prepared = store.begin_node_compile_cache_attempt(1).unwrap();
    let mut result = NodeCompileCacheAttemptSummary {
        outcome: ToolCacheOutcome::Completed,
        prepared_at_unix_millis: 1,
        completed_at_unix_millis: Some(2),
        removed_entry_count: Some(0),
        removed_logical_bytes: Some(0),
    };
    store
        .complete_node_compile_cache_attempt(&prepared, &result, ToolCacheAvailability::Available)
        .unwrap();
    let json = serde_json::to_string(
        &store
            .latest_node_compile_cache_maintenance()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(!json.contains("native_removed"));
    assert!(!json.contains("unlinger-node-cache"));
    let prepared = store.begin_node_compile_cache_attempt(3).unwrap();
    result.prepared_at_unix_millis = 3;
    result.completed_at_unix_millis = Some(4);
    result.outcome = ToolCacheOutcome::Failed;
    assert!(
        store
            .complete_node_compile_cache_attempt(
                &prepared,
                &result,
                ToolCacheAvailability::Unavailable
            )
            .is_err()
    );
}

#[test]
#[ignore = "requires exact supported Node, uses only isolated test-owned cache"]
fn real_node_cache_survives_concurrent_import_and_regenerates_offline() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    let node = std::env::var_os("UNLINGER_NODE_TEST_BIN").expect("explicit exact Node binary");
    let node = PathBuf::from(node);
    let binding = ProducerBinding::capture(&node).unwrap();
    let info: ProducerInfo = serde_json::from_str(
        &read_producer_output(&node, &["--eval", NODE_INFO], &|| false).unwrap(),
    )
    .unwrap();
    let mut fixture = Fixture::new();
    fs::remove_dir(fixture.bucket()).unwrap();
    fixture.bucket_name = info
        .bucket()
        .expect("exact supported version/arch/current-user");
    fs::write(fixture.temp.join("a.cjs"), "module.exports=42").unwrap();
    fs::write(fixture.temp.join("b.cjs"), "module.exports=43").unwrap();
    let script = "const m=require('node:module');m.enableCompileCache(process.argv[1]);const first=require(process.argv[2]);m.flushCompileCache();console.log(first);const rl=require('node:readline').createInterface({input:process.stdin});rl.once('line',()=>{console.log(require(process.argv[3]));m.flushCompileCache();rl.close()});";
    let child = Command::new(&node)
        .args(["--eval", script])
        .arg(fixture.root())
        .arg(fixture.temp.join("a.cjs"))
        .arg(fixture.temp.join("b.cjs"))
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    struct Guard(std::process::Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let mut guard = Guard(child);
    let stdout = guard.0.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout);
    let mut line = String::new();
    lines.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "42");
    // The print follows flush; observe a real published file before maintenance.
    let deadline = Instant::now() + Duration::from_secs(10);
    while fs::read_dir(fixture.bucket()).unwrap().count() == 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let adapter = NodeCompileCacheMaintenance::open_at(
        &fixture.temp,
        node.clone(),
        binding,
        fixture.bucket_name.clone(),
        &|| false,
    )
    .ok()
    .unwrap();
    let control = fixture.control(true);
    let (prepared, plan, epoch) = control
        .start_node_compile_cache_if_ready_enforce(10, |_, _| Ok(adapter))
        .unwrap()
        .unwrap();
    let result = plan.unwrap().execute(&control, &prepared, &epoch, || false);
    assert_eq!(result.outcome, ToolCacheOutcome::Completed);
    assert!(result.removed_entry_count.unwrap() > 0);
    control.finish_tool_cache_action(&prepared).unwrap();
    guard
        .0
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    drop(guard.0.stdin.take());
    line.clear();
    lines.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "43");
    assert!(guard.0.wait().unwrap().success());
    let second=Command::new(&node).args(["--eval","require('node:module').enableCompileCache(process.argv[1]);console.log(require(process.argv[2]))"]).arg(fixture.root()).arg(fixture.temp.join("a.cjs")).env_clear().output().unwrap();
    assert!(second.status.success());
    assert_eq!(String::from_utf8(second.stdout).unwrap().trim(), "42");
    assert!(
        !NodeCompileCacheMaintenance::open_at(
            &fixture.temp,
            node.clone(),
            ProducerBinding::capture(&node).unwrap(),
            fixture.bucket_name.clone(),
            &|| false
        )
        .ok()
        .unwrap()
        .entries
        .is_empty()
    );
}
