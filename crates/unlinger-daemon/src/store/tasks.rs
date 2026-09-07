use super::*;
use unlinger_core::{
    ProcessIdentity, Snapshot, TaskControllerBinding, TaskOwnerIdentity, fingerprint_parts,
    fingerprint_process_identity, task_id_from_session, valid_task_id,
};

pub(crate) const TASK_SCHEMA_SQL: &str = "
CREATE TABLE task_scopes (
    task_id TEXT PRIMARY KEY CHECK (length(task_id) = 32),
    capability TEXT NOT NULL CHECK (length(capability) = 32),
    registrar_json TEXT NOT NULL,
    owner_json TEXT,
    created_at_ms INTEGER NOT NULL,
    activated_at_us INTEGER,
    released_at_us INTEGER,
    release_reason TEXT
);
CREATE TABLE task_controllers (
    identity_key TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES task_scopes(task_id) ON DELETE CASCADE,
    identity_json TEXT NOT NULL,
    incident_id TEXT NOT NULL
);
CREATE INDEX task_controllers_scope ON task_controllers(task_id);
";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskLease {
    pub task_id: String,
    pub session_name: String,
    pub capability: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlinger_core::{
        ExecutableIdentity, PlaywrightCliRuntime, ProcessRecord, ProcessRuntimeFacts,
        ProcessStatus, SnapshotCoverage,
    };

    struct TempStore {
        store: HistoryStore,
        directory: PathBuf,
    }
    impl TempStore {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "ul-task-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&directory).unwrap();
            Self {
                store: HistoryStore::open(directory.join("history.db")).unwrap(),
                directory,
            }
        }
    }
    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
    const ID: &str = "0123456789abcdef0123456789abcdef";
    fn owner(pid: u32) -> TaskOwnerIdentity {
        TaskOwnerIdentity {
            pid,
            started_at_unix_micros: 500,
            uid: 501,
        }
    }
    fn controller(pid: u32, birth: u64) -> ProcessRecord {
        ProcessRecord {
            identity: ProcessIdentity {
                pid,
                started_at_unix_micros: birth,
                executable_device: Some(1),
                executable_inode: Some(2),
            },
            parent_pid: 1,
            process_group_id: pid,
            uid: 501,
            tty_device: None,
            name: "node".to_owned(),
            executable_path: None,
            executable: ExecutableIdentity::default(),
            arguments: None,
            resident_memory_bytes: 0,
            status: ProcessStatus::Sleeping,
            runtime: ProcessRuntimeFacts {
                playwright_cli: Some(PlaywrightCliRuntime {
                    session_name: format!("unlinger-{ID}"),
                    version: "1.63.0-alpha-2026-08-31".to_owned(),
                    persistent: false,
                    attached: false,
                }),
                ..Default::default()
            },
        }
    }
    fn snapshot(processes: Vec<ProcessRecord>, now: u64) -> Snapshot {
        Snapshot {
            observed_at_unix_millis: now,
            current_uid: 501,
            coverage: SnapshotCoverage {
                listed_processes: processes.len(),
                inspected_processes: processes.len(),
                ..Default::default()
            },
            processes,
        }
    }

    #[test]
    fn task_authority_is_durable_private_and_cannot_reassign_or_revive() {
        let temp = TempStore::new();
        let store = &temp.store;
        let lease = store.reserve_task(ID, &owner(10), 1).unwrap();
        assert_eq!(lease, store.reserve_task(ID, &owner(10), 2).unwrap());
        assert!(store.reserve_task(ID, &owner(11), 2).is_err());
        assert!(store.authorize_task(ID, ID).is_err());
        store.activate_task(ID, &owner(20), 3).unwrap();
        // A stale RESERVED scan cannot release a task activated concurrently.
        store.release_task(ID, 4, "never_started", None).unwrap();
        assert_eq!(
            store.task_status(ID).unwrap().unwrap().phase,
            TaskPhase::Active
        );
        assert!(store.activate_task(ID, &owner(21), 4).is_err());
        store
            .release_task(ID, 5, "reported_exit", Some(&owner(20)))
            .unwrap();
        store
            .release_task(ID, 8, "owner_disappeared", Some(&owner(20)))
            .unwrap();
        let reopened = HistoryStore::open(store.path()).unwrap();
        assert_eq!(
            reopened
                .authorize_task(ID, &lease.capability)
                .unwrap()
                .released_at_us,
            Some(6000)
        );
        assert!(reopened.activate_task(ID, &owner(20), 9).is_err());
        let status = reopened.task_status(ID).unwrap().unwrap();
        assert_eq!(status.release_reason.as_deref(), Some("reported_exit"));
        let public = serde_json::to_string(&status).unwrap();
        assert!(!public.contains(&lease.capability));
        assert!(!public.contains("owner") && !public.contains("registrar"));
    }

    #[test]
    fn binding_allows_multiple_workspaces_and_late_discovery_but_not_new_reuse() {
        let temp = TempStore::new();
        let store = &temp.store;
        store.reserve_task(ID, &owner(10), 1).unwrap();
        store.activate_task(ID, &owner(20), 2).unwrap();
        store
            .release_task(ID, 10, "reported_exit", Some(&owner(20)))
            .unwrap();
        let mut wrong_uid = controller(34, 5000);
        wrong_uid.uid = 502;
        store
            .bind_task_controllers(&snapshot(
                vec![
                    controller(30, 3000),
                    controller(31, 4000),
                    controller(32, 1500),
                    controller(33, 12000),
                    wrong_uid,
                ],
                30,
            ))
            .unwrap();
        let bindings = store.task_controller_bindings().unwrap();
        assert_eq!(bindings.len(), 2);
        assert!(bindings.iter().all(|binding| binding.released));
        assert_eq!(
            store.task_status(ID).unwrap().unwrap().incident_ids.len(),
            2
        );
        // Re-observation is idempotent; PID reuse does not acquire the old binding.
        store
            .bind_task_controllers(&snapshot(
                vec![controller(30, 3000), controller(31, 40000)],
                40,
            ))
            .unwrap();
        assert_eq!(store.task_controller_bindings().unwrap().len(), 2);
        let later = 15 * 24 * 60 * 60 * 1000;
        store
            .bind_task_controllers(&snapshot(vec![controller(30, 3000)], later))
            .unwrap();
        assert!(store.task_status(ID).unwrap().is_some());
        store
            .bind_task_controllers(&snapshot(vec![], later))
            .unwrap();
        assert!(store.task_status(ID).unwrap().is_none());
        assert!(store.task_controller_bindings().unwrap().is_empty());
    }

    #[test]
    fn v7_upgrade_preserves_existing_state_and_creates_an_empty_task_registry() {
        let temp = TempStore::new();
        temp.store.set_pause_until(Some(900_000)).unwrap();
        let connection = temp.store.connection().unwrap();
        connection
            .execute_batch(
                "DROP TABLE task_controllers; DROP TABLE task_scopes; PRAGMA user_version=7;",
            )
            .unwrap();
        drop(connection);
        let migrated = HistoryStore::open(temp.store.path()).unwrap();
        assert_eq!(migrated.pause_until().unwrap(), Some(900_000));
        assert!(migrated.task_scopes().unwrap().is_empty());
        assert_eq!(HistoryStore::schema_version(), 8);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    Reserved,
    Active,
    Released,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskStatus {
    pub task_id: String,
    pub session_name: String,
    pub phase: TaskPhase,
    pub release_reason: Option<String>,
    pub incident_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct TaskScope {
    pub task_id: String,
    pub registrar: TaskOwnerIdentity,
    pub owner: Option<TaskOwnerIdentity>,
    pub activated_at_us: Option<u64>,
    pub released_at_us: Option<u64>,
}

impl HistoryStore {
    pub(crate) fn reserve_task(
        &self,
        task_id: &str,
        registrar: &TaskOwnerIdentity,
        now: u64,
    ) -> Result<TaskLease, StoreError> {
        if !valid_task_id(task_id) {
            return Err(StoreError::Invalid("invalid task identifier".to_owned()));
        }
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT capability, registrar_json FROM task_scopes WHERE task_id = ?1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let capability = if let Some((capability, prior)) = existing {
            let prior: TaskOwnerIdentity = serde_json::from_str(&prior)?;
            if &prior != registrar {
                return Err(StoreError::Invalid(
                    "task belongs to another registrar".to_owned(),
                ));
            }
            capability
        } else {
            let count: i64 =
                tx.query_row("SELECT count(*) FROM task_scopes", [], |row| row.get(0))?;
            if count >= 4096 {
                return Err(StoreError::Invalid(
                    "task registry capacity reached".to_owned(),
                ));
            }
            let capability: String =
                tx.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
            tx.execute("INSERT INTO task_scopes(task_id, capability, registrar_json, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
                params![task_id, capability, serde_json::to_string(registrar)?, sqlite_millis(now, "task creation")?])?;
            capability
        };
        tx.commit()?;
        Ok(TaskLease {
            task_id: task_id.to_owned(),
            session_name: format!("unlinger-{task_id}"),
            capability,
        })
    }

    pub(crate) fn authorize_task(
        &self,
        task_id: &str,
        capability: &str,
    ) -> Result<TaskScope, StoreError> {
        let connection = self.connection()?;
        let matches: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_scopes WHERE task_id = ?1 AND capability = ?2)",
            params![task_id, capability],
            |row| row.get(0),
        )?;
        if !matches {
            return Err(StoreError::Invalid(
                "task capability does not match".to_owned(),
            ));
        }
        self.task_scopes()?
            .into_iter()
            .find(|scope| scope.task_id == task_id)
            .ok_or_else(|| StoreError::Invalid("task is unavailable".to_owned()))
    }

    pub(crate) fn activate_task(
        &self,
        task_id: &str,
        owner: &TaskOwnerIdentity,
        now: u64,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (prior, released): (Option<String>, Option<i64>) = tx.query_row(
            "SELECT owner_json, released_at_us FROM task_scopes WHERE task_id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if released.is_some() {
            return Err(StoreError::Invalid(
                "released tasks cannot be activated".to_owned(),
            ));
        }
        if let Some(prior) = prior {
            if serde_json::from_str::<TaskOwnerIdentity>(&prior)? != *owner {
                return Err(StoreError::Invalid(
                    "task command owner cannot be replaced".to_owned(),
                ));
            }
        } else {
            tx.execute(
                "UPDATE task_scopes SET owner_json = ?2, activated_at_us = ?3 WHERE task_id = ?1",
                params![
                    task_id,
                    serde_json::to_string(owner)?,
                    sqlite_u64(now.saturating_mul(1000), "task activation")?
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn release_task(
        &self,
        task_id: &str,
        now: u64,
        reason: &str,
        owner: Option<&TaskOwnerIdentity>,
    ) -> Result<(), StoreError> {
        self.connection()?.execute("UPDATE task_scopes SET released_at_us = ?2, release_reason = ?3 WHERE task_id = ?1 AND released_at_us IS NULL AND owner_json IS ?4",
            params![task_id, sqlite_u64(now.saturating_add(1).saturating_mul(1000), "task release")?, reason, owner.map(serde_json::to_string).transpose()?])?;
        Ok(())
    }

    pub(crate) fn task_scopes(&self) -> Result<Vec<TaskScope>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT task_id, registrar_json, owner_json, activated_at_us, released_at_us FROM task_scopes")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (task_id, registrar, owner, activated_at_us, released_at_us) = row?;
            Ok(TaskScope {
                task_id,
                registrar: serde_json::from_str(&registrar)?,
                owner: owner
                    .map(|value| serde_json::from_str(&value))
                    .transpose()?,
                activated_at_us: activated_at_us
                    .map(|v| parse_nonnegative_millis(v, "task activation"))
                    .transpose()?,
                released_at_us: released_at_us
                    .map(|v| parse_nonnegative_millis(v, "task release"))
                    .transpose()?,
            })
        })
        .collect()
    }

    pub(crate) fn bind_task_controllers(&self, snapshot: &Snapshot) -> Result<(), StoreError> {
        let scopes = self.task_scopes()?;
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for process in &snapshot.processes {
            let Some(cli) = &process.runtime.playwright_cli else {
                continue;
            };
            let Some(task_id) = task_id_from_session(&cli.session_name) else {
                continue;
            };
            let Some(scope) = scopes.iter().find(|scope| scope.task_id == task_id) else {
                continue;
            };
            let Some(activated) = scope.activated_at_us else {
                continue;
            };
            let birth = process.identity.started_at_unix_micros;
            if process.uid != scope.registrar.uid
                || birth < activated
                || scope.released_at_us.is_some_and(|cutoff| birth > cutoff)
                || !process.identity.exact_match(&process.identity)
            {
                continue;
            }
            let fingerprint = fingerprint_process_identity(&process.identity);
            let key = format!("playwright:{}:{fingerprint}", process.pid());
            let incident_id = format!("inc-{}", fingerprint_parts([key.as_bytes()]));
            tx.execute("INSERT INTO task_controllers(identity_key, task_id, identity_json, incident_id) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(identity_key) DO NOTHING",
                params![fingerprint, task_id, serde_json::to_string(&process.identity)?, incident_id])?;
        }
        tx.commit()?;
        if snapshot.proves_complete_exact_identity_coverage() {
            let cutoff = snapshot
                .observed_at_unix_millis
                .saturating_sub(14 * 24 * 60 * 60 * 1000)
                .saturating_mul(1000);
            let bindings = self.task_controller_bindings()?;
            for scope in &scopes {
                if scope
                    .released_at_us
                    .is_some_and(|released| released < cutoff)
                    && !snapshot.processes.iter().any(|process| {
                        process
                            .runtime
                            .task_session_name
                            .as_deref()
                            .and_then(task_id_from_session)
                            == Some(scope.task_id.as_str())
                            || bindings.iter().any(|binding| {
                                binding.task_id == scope.task_id
                                    && binding.controller.exact_match(&process.identity)
                            })
                    })
                {
                    connection.execute(
                        "DELETE FROM task_scopes WHERE task_id = ?1",
                        [&scope.task_id],
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn task_controller_bindings(
        &self,
    ) -> Result<Vec<TaskControllerBinding>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT c.task_id, c.identity_json, s.released_at_us IS NOT NULL FROM task_controllers c JOIN task_scopes s ON s.task_id = c.task_id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (task_id, identity, released) = row?;
            Ok(TaskControllerBinding {
                task_id,
                controller: serde_json::from_str::<ProcessIdentity>(&identity)?,
                released,
            })
        })
        .collect()
    }

    pub fn task_status(&self, task_id: &str) -> Result<Option<TaskStatus>, StoreError> {
        let connection = self.connection()?;
        let row: Option<(Option<i64>, Option<i64>, Option<String>)> = connection.query_row(
            "SELECT activated_at_us, released_at_us, release_reason FROM task_scopes WHERE task_id = ?1", [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
        let Some((activated, released, release_reason)) = row else {
            return Ok(None);
        };
        let mut statement = connection.prepare(
            "SELECT incident_id FROM task_controllers WHERE task_id = ?1 ORDER BY incident_id",
        )?;
        let incident_ids = statement
            .query_map([task_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(TaskStatus {
            task_id: task_id.to_owned(),
            session_name: format!("unlinger-{task_id}"),
            phase: if released.is_some() {
                TaskPhase::Released
            } else if activated.is_some() {
                TaskPhase::Active
            } else {
                TaskPhase::Reserved
            },
            release_reason,
            incident_ids,
        }))
    }
}
