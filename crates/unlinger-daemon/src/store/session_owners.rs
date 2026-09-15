//! Durable host-declared ownership of ordinary Playwright CLI sessions.
//!
//! This is an additive operator lane beside the task-owned lane. It shares the
//! same trust rules (authenticated local same-user peer, registrar's exact
//! child as the owner, immutable release, exact controller binding) but the
//! host declares and releases it itself instead of running `unlinger task run`.
//!
//! The lease is keyed on an ordinary *selector fingerprint* derived from the
//! controller-owned Playwright registry record (session name plus its path-free
//! 16-hex registry namespace). The Playwright session name is
//! never rewritten, the `unlinger-<task_id>` task lane owns its own selectors,
//! and a lease supplies evidence only: never a signal target, never cleanup
//! authority, never a widened protection rule.
use super::*;
use unlinger_core::{
    ProcessIdentity, SessionOwnerBinding, Snapshot, TaskOwnerIdentity,
    fingerprint_process_identity, task_id_from_session, valid_ordinary_session_name,
};

pub(crate) const SESSION_OWNER_SCHEMA_SQL: &str = "
CREATE TABLE session_owner_leases (
    lease_id TEXT PRIMARY KEY CHECK (length(lease_id) = 32),
    capability TEXT NOT NULL CHECK (length(capability) = 32),
    selector_fingerprint TEXT NOT NULL CHECK (length(selector_fingerprint) = 16),
    session_name TEXT NOT NULL CHECK (length(session_name) BETWEEN 1 AND 64),
    controller_version TEXT NOT NULL CHECK (length(controller_version) BETWEEN 1 AND 64),
    generation INTEGER NOT NULL CHECK (generation >= 1),
    registrar_json TEXT NOT NULL,
    owner_json TEXT,
    created_at_ms INTEGER NOT NULL,
    activated_at_us INTEGER,
    released_at_us INTEGER,
    release_reason TEXT,
    UNIQUE (selector_fingerprint, generation)
);
CREATE INDEX session_owner_leases_selector ON session_owner_leases(selector_fingerprint);
CREATE TABLE session_owner_controllers (
    identity_key TEXT PRIMARY KEY,
    lease_id TEXT NOT NULL REFERENCES session_owner_leases(lease_id) ON DELETE CASCADE,
    identity_json TEXT NOT NULL,
    incident_id TEXT NOT NULL
);
CREATE INDEX session_owner_controllers_lease ON session_owner_controllers(lease_id);
";

const MAX_SESSION_OWNER_LEASES: i64 = 4096;
const SESSION_OWNER_RETENTION_MILLIS: u64 = 14 * 24 * 60 * 60 * 1_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionOwnerLease {
    pub lease_id: String,
    pub session_name: String,
    pub selector_fingerprint: String,
    pub generation: u64,
    pub capability: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionOwnerStatus {
    pub lease_id: String,
    pub session_name: String,
    pub selector_fingerprint: String,
    pub generation: u64,
    pub controller_version: String,
    pub phase: TaskPhase,
    pub release_reason: Option<String>,
    pub controller_bound: bool,
    pub incident_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionOwnerScope {
    pub lease_id: String,
    pub selector_fingerprint: String,
    pub session_name: String,
    pub controller_version: String,
    pub generation: u64,
    pub registrar: TaskOwnerIdentity,
    pub owner: Option<TaskOwnerIdentity>,
    pub activated_at_us: Option<u64>,
    pub released_at_us: Option<u64>,
}

/// Row shape of one durable session-owner lease for status projection.
type SessionOwnerStatusRow = (
    String,
    String,
    String,
    i64,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

impl HistoryStore {
    /// Declare or idempotently re-join the current generation of one ordinary
    /// session selector. A released generation is terminal: a later host turn
    /// must declare again and receives a fresh generation.
    pub(crate) fn declare_session_owner(
        &self,
        selector_fingerprint: &str,
        session_name: &str,
        controller_version: &str,
        registrar: &TaskOwnerIdentity,
        now: u64,
    ) -> Result<SessionOwnerLease, StoreError> {
        if selector_fingerprint.len() != 16
            || !selector_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StoreError::Invalid(
                "invalid session-owner selector fingerprint".to_owned(),
            ));
        }
        if !valid_ordinary_session_name(session_name) {
            return Err(StoreError::Invalid(
                "invalid ordinary session name".to_owned(),
            ));
        }
        if controller_version.is_empty() || controller_version.len() > 64 {
            return Err(StoreError::Invalid(
                "session-owner controller version must be 1..=64 bytes".to_owned(),
            ));
        }
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active: Option<(String, String, String, i64, String, String)> = tx
            .query_row(
                "SELECT lease_id, capability, registrar_json, generation,
                        session_name, controller_version
                 FROM session_owner_leases
                 WHERE selector_fingerprint = ?1 AND released_at_us IS NULL
                 ORDER BY generation DESC LIMIT 1",
                [selector_fingerprint],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?;
        if let Some((
            lease_id,
            capability,
            prior,
            generation,
            prior_session_name,
            prior_controller_version,
        )) = active
        {
            let prior: TaskOwnerIdentity = serde_json::from_str(&prior)?;
            if &prior != registrar {
                return Err(StoreError::Invalid(
                    "session owner belongs to another registrar".to_owned(),
                ));
            }
            if prior_session_name != session_name || prior_controller_version != controller_version
            {
                return Err(StoreError::Invalid(
                    "active session-owner selector facts cannot be replaced".to_owned(),
                ));
            }
            let stored: (String, String) = tx.query_row(
                "SELECT session_name, controller_version FROM session_owner_leases
                 WHERE lease_id = ?1",
                [&lease_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if stored.0 != session_name || stored.1 != controller_version {
                return Err(StoreError::Invalid(
                    "session owner is already declared with different selector facts".to_owned(),
                ));
            }
            tx.commit()?;
            return Ok(SessionOwnerLease {
                lease_id,
                session_name: session_name.to_owned(),
                selector_fingerprint: selector_fingerprint.to_owned(),
                generation: u64::try_from(generation).unwrap_or(1),
                capability,
            });
        }
        let count: i64 = tx.query_row("SELECT count(*) FROM session_owner_leases", [], |row| {
            row.get(0)
        })?;
        if count >= MAX_SESSION_OWNER_LEASES {
            return Err(StoreError::Invalid(
                "session-owner registry capacity reached".to_owned(),
            ));
        }
        let generation: i64 = tx.query_row(
            "SELECT coalesce(max(generation), 0) + 1 FROM session_owner_leases
             WHERE selector_fingerprint = ?1",
            [selector_fingerprint],
            |row| row.get(0),
        )?;
        let lease_id: String =
            tx.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
        let capability: String =
            tx.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
        tx.execute(
            "INSERT INTO session_owner_leases(
                 lease_id, capability, selector_fingerprint, session_name,
                 controller_version, generation, registrar_json, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                lease_id,
                capability,
                selector_fingerprint,
                session_name,
                controller_version,
                generation,
                serde_json::to_string(registrar)?,
                sqlite_millis(now, "session-owner declaration")?
            ],
        )?;
        tx.commit()?;
        Ok(SessionOwnerLease {
            lease_id,
            session_name: session_name.to_owned(),
            selector_fingerprint: selector_fingerprint.to_owned(),
            generation: u64::try_from(generation).unwrap_or(1),
            capability,
        })
    }

    pub(crate) fn authorize_session_owner(
        &self,
        lease_id: &str,
        capability: &str,
    ) -> Result<SessionOwnerScope, StoreError> {
        let connection = self.connection()?;
        let matches: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM session_owner_leases
             WHERE lease_id = ?1 AND capability = ?2)",
            params![lease_id, capability],
            |row| row.get(0),
        )?;
        if !matches {
            return Err(StoreError::Invalid(
                "session-owner capability does not match".to_owned(),
            ));
        }
        self.session_owner_scopes()?
            .into_iter()
            .find(|scope| scope.lease_id == lease_id)
            .ok_or_else(|| StoreError::Invalid("session owner is unavailable".to_owned()))
    }

    pub(crate) fn activate_session_owner(
        &self,
        lease_id: &str,
        owner: &TaskOwnerIdentity,
        now: u64,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (prior, released): (Option<String>, Option<i64>) = tx.query_row(
            "SELECT owner_json, released_at_us FROM session_owner_leases WHERE lease_id = ?1",
            [lease_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if released.is_some() {
            return Err(StoreError::Invalid(
                "released session owners cannot be activated".to_owned(),
            ));
        }
        if let Some(prior) = prior {
            if serde_json::from_str::<TaskOwnerIdentity>(&prior)? != *owner {
                return Err(StoreError::Invalid(
                    "session owner cannot be replaced".to_owned(),
                ));
            }
        } else {
            tx.execute(
                "UPDATE session_owner_leases SET owner_json = ?2, activated_at_us = ?3
                 WHERE lease_id = ?1",
                params![
                    lease_id,
                    serde_json::to_string(owner)?,
                    sqlite_u64(now.saturating_mul(1000), "session-owner activation")?
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Release is terminal for that generation. A later host turn over the same
    /// ordinary session receives a fresh generation instead.
    pub(crate) fn release_session_owner(
        &self,
        lease_id: &str,
        now: u64,
        reason: &str,
        owner: Option<&TaskOwnerIdentity>,
    ) -> Result<(), StoreError> {
        self.connection()?.execute(
            "UPDATE session_owner_leases
             SET released_at_us = ?2, release_reason = ?3
             WHERE lease_id = ?1 AND released_at_us IS NULL AND owner_json IS ?4",
            params![
                lease_id,
                sqlite_u64(
                    now.saturating_add(1).saturating_mul(1000),
                    "session-owner release"
                )?,
                reason,
                owner.map(serde_json::to_string).transpose()?
            ],
        )?;
        Ok(())
    }

    pub(crate) fn session_owner_scopes(&self) -> Result<Vec<SessionOwnerScope>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT lease_id, selector_fingerprint, session_name, controller_version,
                    generation, registrar_json, owner_json, activated_at_us, released_at_us
             FROM session_owner_leases",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
            ))
        })?;
        rows.map(|row| {
            let (
                lease_id,
                selector_fingerprint,
                session_name,
                controller_version,
                generation,
                registrar,
                owner,
                activated_at_us,
                released_at_us,
            ) = row?;
            Ok(SessionOwnerScope {
                lease_id,
                selector_fingerprint,
                session_name,
                controller_version,
                generation: u64::try_from(generation).unwrap_or(1),
                registrar: serde_json::from_str(&registrar)?,
                owner: owner
                    .map(|value| serde_json::from_str(&value))
                    .transpose()?,
                activated_at_us: activated_at_us
                    .map(|value| parse_nonnegative_millis(value, "session-owner activation"))
                    .transpose()?,
                released_at_us: released_at_us
                    .map(|value| parse_nonnegative_millis(value, "session-owner release"))
                    .transpose()?,
            })
        })
        .collect()
    }

    /// The highest generation per selector is the only one that may supply
    /// evidence, so a released older generation cannot keep authorizing
    /// abandonment once a newer generation owns the same ordinary session.
    fn current_session_owner_scopes(&self) -> Result<Vec<SessionOwnerScope>, StoreError> {
        let scopes = self.session_owner_scopes()?;
        let mut current: BTreeMap<String, SessionOwnerScope> = BTreeMap::new();
        for scope in scopes {
            current
                .entry(scope.selector_fingerprint.clone())
                .and_modify(|existing| {
                    if scope.generation > existing.generation {
                        *existing = scope.clone();
                    }
                })
                .or_insert(scope);
        }
        Ok(current.into_values().collect())
    }

    /// Bind exactly the observed ordinary Playwright CLI controllers admitted
    /// by the current generation of a declared lease: same selector
    /// fingerprint, same controller version, same registrar uid, born inside
    /// the activation/release window and carrying a complete executable
    /// identity. Task-owned selectors are never adopted here.
    pub(crate) fn bind_session_owner_controllers(
        &self,
        snapshot: &Snapshot,
    ) -> Result<(), StoreError> {
        let scopes = self.current_session_owner_scopes()?;
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for process in &snapshot.processes {
            let Some(cli) = &process.runtime.playwright_cli else {
                continue;
            };
            if task_id_from_session(&cli.session_name).is_some() {
                continue;
            }
            let Some(selector_fingerprint) = cli.selector_fingerprint.as_deref() else {
                continue;
            };
            let Some(scope) = scopes.iter().find(|scope| {
                scope.selector_fingerprint == selector_fingerprint
                    && scope.session_name == cli.session_name
                    && scope.controller_version == cli.version
            }) else {
                continue;
            };
            let Some(activated) = scope.activated_at_us else {
                continue;
            };
            let birth = process.identity.started_at_unix_micros;
            if process.uid != scope.registrar.uid
                || birth < activated
                || scope.released_at_us.is_some_and(|cutoff| birth > cutoff)
                || process.identity.executable_device.is_none()
                || process.identity.executable_inode.is_none()
            {
                continue;
            }
            let fingerprint = fingerprint_process_identity(&process.identity);
            let incident_key = format!("playwright:{}:{fingerprint}", process.pid());
            let incident_id = format!(
                "inc-{}",
                unlinger_core::fingerprint_parts([incident_key.as_bytes()])
            );
            tx.execute(
                "INSERT INTO session_owner_controllers(
                     identity_key, lease_id, identity_json, incident_id
                 ) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(identity_key) DO NOTHING",
                params![
                    fingerprint,
                    scope.lease_id,
                    serde_json::to_string(&process.identity)?,
                    incident_id
                ],
            )?;
        }
        tx.commit()?;
        self.prune_session_owner_leases(snapshot)?;
        Ok(())
    }

    pub(crate) fn session_owner_bindings(&self) -> Result<Vec<SessionOwnerBinding>, StoreError> {
        let current = self
            .current_session_owner_scopes()?
            .into_iter()
            .map(|scope| (scope.lease_id.clone(), scope))
            .collect::<BTreeMap<_, _>>();
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT lease_id, identity_json FROM session_owner_controllers ORDER BY lease_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut bindings = Vec::new();
        for row in rows {
            let (lease_id, identity) = row?;
            let Some(scope) = current.get(&lease_id) else {
                continue;
            };
            bindings.push(SessionOwnerBinding {
                lease_id: lease_id.clone(),
                selector_fingerprint: scope.selector_fingerprint.clone(),
                session_name: scope.session_name.clone(),
                controller_version: scope.controller_version.clone(),
                controller: serde_json::from_str::<ProcessIdentity>(&identity)?,
                released: scope.released_at_us.is_some(),
            });
        }
        Ok(bindings)
    }

    pub fn session_owner_status(
        &self,
        lease_id: &str,
    ) -> Result<Option<SessionOwnerStatus>, StoreError> {
        let connection = self.connection()?;
        let row: Option<SessionOwnerStatusRow> = connection
            .query_row(
                "SELECT session_name, selector_fingerprint, controller_version, generation,
                            activated_at_us, released_at_us, release_reason
                     FROM session_owner_leases WHERE lease_id = ?1",
                [lease_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            session_name,
            selector_fingerprint,
            controller_version,
            generation,
            activated,
            released,
            release_reason,
        )) = row
        else {
            return Ok(None);
        };
        let mut statement = connection.prepare(
            "SELECT incident_id FROM session_owner_controllers
             WHERE lease_id = ?1 ORDER BY incident_id",
        )?;
        let incident_ids = statement
            .query_map([lease_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(SessionOwnerStatus {
            lease_id: lease_id.to_owned(),
            session_name,
            selector_fingerprint,
            generation: u64::try_from(generation).unwrap_or(1),
            controller_version,
            phase: if released.is_some() {
                TaskPhase::Released
            } else if activated.is_some() {
                TaskPhase::Active
            } else {
                TaskPhase::Reserved
            },
            release_reason,
            controller_bound: !incident_ids.is_empty(),
            incident_ids,
        }))
    }

    /// Released leases expire only after the minimum retention window and a
    /// complete snapshot that no longer observes their selector or controller.
    fn prune_session_owner_leases(&self, snapshot: &Snapshot) -> Result<(), StoreError> {
        if !snapshot.proves_complete_exact_identity_coverage() {
            return Ok(());
        }
        let cutoff = snapshot
            .observed_at_unix_millis
            .saturating_sub(SESSION_OWNER_RETENTION_MILLIS)
            .saturating_mul(1000);
        let scopes = self.session_owner_scopes()?;
        let connection = self.connection()?;
        for scope in &scopes {
            if !scope
                .released_at_us
                .is_some_and(|released| released < cutoff)
            {
                continue;
            }
            let still_observed = snapshot.processes.iter().any(|process| {
                process
                    .runtime
                    .playwright_cli
                    .as_ref()
                    .and_then(|cli| cli.selector_fingerprint.as_deref())
                    == Some(scope.selector_fingerprint.as_str())
            });
            if still_observed {
                continue;
            }
            connection.execute(
                "DELETE FROM session_owner_leases WHERE lease_id = ?1",
                [&scope.lease_id],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use unlinger_core::{
        ExecutableIdentity, PlaywrightCliRuntime, ProcessRecord, ProcessRuntimeFacts,
        ProcessStatus, SnapshotCoverage, ordinary_selector_fingerprint,
    };

    struct TempStore {
        store: HistoryStore,
        directory: PathBuf,
    }
    impl TempStore {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let directory = std::env::temp_dir().join(format!(
                "ul-session-owner-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
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

    const VERSION: &str = "1.62.1";
    const NAMESPACE_A: &str = "0521184cff085302";
    const NAMESPACE_B: &str = "625daa9ea0d6cbcf";

    fn owner(pid: u32) -> TaskOwnerIdentity {
        TaskOwnerIdentity {
            pid,
            started_at_unix_micros: 500,
            uid: 501,
        }
    }

    fn selector(registry_namespace: &str) -> String {
        ordinary_selector_fingerprint(registry_namespace, "default")
    }

    fn cli_controller(
        pid: u32,
        birth: u64,
        version: &str,
        uid: u32,
        selector_fingerprint: Option<&str>,
    ) -> ProcessRecord {
        ProcessRecord {
            identity: ProcessIdentity {
                pid,
                started_at_unix_micros: birth,
                executable_device: Some(1),
                executable_inode: Some(2),
            },
            parent_pid: 1,
            process_group_id: pid,
            uid,
            tty_device: None,
            name: "node".to_owned(),
            executable_path: Some("/synthetic/bin/node".to_owned()),
            executable: ExecutableIdentity::default(),
            arguments: None,
            resident_memory_bytes: 0,
            status: ProcessStatus::Sleeping,
            runtime: ProcessRuntimeFacts {
                playwright_cli: Some(PlaywrightCliRuntime {
                    session_name: "default".to_owned(),
                    version: version.to_owned(),
                    persistent: false,
                    attached: false,
                    selector_fingerprint: selector_fingerprint.map(str::to_owned),
                }),
                task_session_facts_complete: true,
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

    fn declared(
        store: &HistoryStore,
        registry_namespace: &str,
        registrar: &TaskOwnerIdentity,
    ) -> SessionOwnerLease {
        store
            .declare_session_owner(
                &selector(registry_namespace),
                "default",
                VERSION,
                registrar,
                1_000,
            )
            .expect("declare session owner")
    }

    #[test]
    fn lease_is_durable_private_and_never_uses_the_playwright_session_name() {
        let temp = TempStore::new();
        let registrar = owner(500);
        let lease = declared(&temp.store, NAMESPACE_A, &registrar);
        assert_eq!(lease.session_name, "default");
        assert_eq!(lease.selector_fingerprint, selector(NAMESPACE_A));
        assert_eq!(lease.generation, 1);
        assert_eq!(lease.capability.len(), 32);
        assert_ne!(lease.lease_id, lease.session_name);

        // Re-declaring the same active selector is idempotent for the same
        // registrar and refuses another registrar.
        let again = declared(&temp.store, NAMESPACE_A, &registrar);
        assert_eq!(again.lease_id, lease.lease_id);
        assert_eq!(again.capability, lease.capability);
        assert!(
            temp.store
                .declare_session_owner(&selector(NAMESPACE_A), "other", VERSION, &registrar, 2_000)
                .is_err(),
            "an active selector cannot be re-described with another session name"
        );
        assert!(
            temp.store
                .declare_session_owner(
                    &selector(NAMESPACE_A),
                    "default",
                    "1.62.2",
                    &registrar,
                    2_000
                )
                .is_err(),
            "an active selector cannot change controller version"
        );
        assert!(
            temp.store
                .declare_session_owner(
                    &selector(NAMESPACE_A),
                    "default",
                    VERSION,
                    &owner(501),
                    2_000
                )
                .is_err()
        );
        assert!(
            temp.store
                .declare_session_owner("not-hex", "default", VERSION, &registrar, 2_000)
                .is_err()
        );
        assert!(
            temp.store
                .declare_session_owner(
                    &selector(NAMESPACE_A),
                    "unlinger-0123456789abcdef0123456789abcdef",
                    VERSION,
                    &registrar,
                    2_000
                )
                .is_err()
        );
        assert!(
            temp.store
                .declare_session_owner(&selector(NAMESPACE_A), "default", "", &registrar, 2_000)
                .is_err()
        );
        assert!(
            temp.store
                .authorize_session_owner(&lease.lease_id, "00000000000000000000000000000000")
                .is_err()
        );

        assert_eq!(
            temp.store
                .session_owner_status(&lease.lease_id)
                .unwrap()
                .unwrap()
                .phase,
            TaskPhase::Reserved
        );
        temp.store
            .activate_session_owner(&lease.lease_id, &owner(600), 3_000)
            .unwrap();
        temp.store
            .activate_session_owner(&lease.lease_id, &owner(600), 4_000)
            .unwrap();
        assert!(
            temp.store
                .activate_session_owner(&lease.lease_id, &owner(601), 4_000)
                .is_err()
        );
        assert_eq!(
            temp.store
                .session_owner_status(&lease.lease_id)
                .unwrap()
                .unwrap()
                .phase,
            TaskPhase::Active
        );

        let reopened = HistoryStore::open(temp.store.path()).unwrap();
        assert_eq!(
            reopened
                .session_owner_status(&lease.lease_id)
                .unwrap()
                .unwrap()
                .phase,
            TaskPhase::Active
        );
    }

    #[test]
    fn released_generation_is_terminal_and_a_later_turn_gets_a_fresh_generation() {
        let temp = TempStore::new();
        let registrar = owner(500);
        let first = declared(&temp.store, NAMESPACE_A, &registrar);
        temp.store
            .activate_session_owner(&first.lease_id, &owner(600), 10)
            .unwrap();
        let birth = 10 * 1_000_000 + 500;
        temp.store
            .bind_session_owner_controllers(&snapshot(
                vec![cli_controller(
                    700,
                    birth,
                    VERSION,
                    501,
                    Some(&selector(NAMESPACE_A)),
                )],
                20_000,
            ))
            .unwrap();
        assert_eq!(temp.store.session_owner_bindings().unwrap().len(), 1);

        temp.store
            .release_session_owner(&first.lease_id, 20_000, "reported_exit", Some(&owner(600)))
            .unwrap();
        let released = temp.store.session_owner_bindings().unwrap();
        assert_eq!(released.len(), 1);
        assert!(released[0].released);
        assert_eq!(released[0].lease_id, first.lease_id);
        // The released generation is terminal for this lease id.
        assert!(
            temp.store
                .activate_session_owner(&first.lease_id, &owner(600), 21_000)
                .is_err()
        );
        assert_eq!(
            temp.store
                .session_owner_status(&first.lease_id)
                .unwrap()
                .unwrap()
                .phase,
            TaskPhase::Released
        );

        // A later host turn over the same ordinary session declares again and
        // receives a fresh generation; the older released generation stops
        // supplying evidence immediately.
        let second = declared(&temp.store, NAMESPACE_A, &registrar);
        assert_ne!(second.lease_id, first.lease_id);
        assert_eq!(second.generation, 2);
        assert_ne!(second.capability, first.capability);
        assert!(temp.store.session_owner_bindings().unwrap().is_empty());
        temp.store
            .activate_session_owner(&second.lease_id, &owner(700), 30_000)
            .unwrap();
        let fresh_birth = 30_000_000_000 + 500;
        temp.store
            .bind_session_owner_controllers(&snapshot(
                vec![cli_controller(
                    701,
                    fresh_birth,
                    VERSION,
                    501,
                    Some(&selector(NAMESPACE_A)),
                )],
                40_000,
            ))
            .unwrap();
        let relive = temp.store.session_owner_bindings().unwrap();
        assert_eq!(relive.len(), 1);
        assert_eq!(relive[0].lease_id, second.lease_id);
        assert!(!relive[0].released);
    }

    #[test]
    fn only_the_exact_selector_version_identity_and_window_bind() {
        let temp = TempStore::new();
        let registrar = owner(500);
        let lease = declared(&temp.store, NAMESPACE_A, &registrar);
        let activated_at_ms = 1_000_000;
        temp.store
            .activate_session_owner(&lease.lease_id, &owner(600), activated_at_ms)
            .unwrap();
        let birth = activated_at_ms * 1_000 + 500;
        let observed_at = activated_at_ms + 60_000;

        let matching = cli_controller(700, birth, VERSION, 501, Some(&selector(NAMESPACE_A)));
        temp.store
            .bind_session_owner_controllers(&snapshot(vec![matching.clone()], observed_at))
            .unwrap();
        let bindings = temp.store.session_owner_bindings().unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].selector_fingerprint, selector(NAMESPACE_A));
        assert_eq!(bindings[0].controller_version, VERSION);
        assert!(!bindings[0].released);
        assert!(bindings[0].binds(&matching));
        let identity_fingerprint = fingerprint_process_identity(&matching.identity);
        let incident_key = format!("playwright:{}:{identity_fingerprint}", matching.pid());
        let expected_incident_id = format!(
            "inc-{}",
            unlinger_core::fingerprint_parts([incident_key.as_bytes()])
        );
        assert_eq!(
            temp.store
                .session_owner_status(&lease.lease_id)
                .unwrap()
                .unwrap()
                .incident_ids,
            [expected_incident_id]
        );

        let mut wrong_session =
            cli_controller(706, birth, VERSION, 501, Some(&selector(NAMESPACE_A)));
        wrong_session
            .runtime
            .playwright_cli
            .as_mut()
            .unwrap()
            .session_name = "other".to_owned();

        for (label, process) in [
            (
                "same-name-other-workspace",
                cli_controller(701, birth, VERSION, 501, Some(&selector(NAMESPACE_B))),
            ),
            (
                "version",
                cli_controller(
                    702,
                    birth,
                    "1.63.0-alpha-2026-08-31",
                    501,
                    Some(&selector(NAMESPACE_A)),
                ),
            ),
            (
                "uid",
                cli_controller(703, birth, VERSION, 502, Some(&selector(NAMESPACE_A))),
            ),
            (
                "too-early",
                cli_controller(
                    704,
                    birth - 1_000,
                    VERSION,
                    501,
                    Some(&selector(NAMESPACE_A)),
                ),
            ),
            (
                "no-selector-facts",
                cli_controller(705, birth, VERSION, 501, None),
            ),
            ("session-name", wrong_session),
        ] {
            let fresh = TempStore::new();
            let lease = declared(&fresh.store, NAMESPACE_A, &registrar);
            fresh
                .store
                .activate_session_owner(&lease.lease_id, &owner(600), activated_at_ms)
                .unwrap();
            fresh
                .store
                .bind_session_owner_controllers(&snapshot(vec![process], observed_at))
                .unwrap();
            assert!(
                fresh.store.session_owner_bindings().unwrap().is_empty(),
                "{label}"
            );
        }
    }

    #[test]
    fn released_leases_prune_only_after_retention_and_proved_absence() {
        let temp = TempStore::new();
        let registrar = owner(500);
        let lease = declared(&temp.store, NAMESPACE_A, &registrar);
        let activated_at_ms = 1_000_000;
        temp.store
            .activate_session_owner(&lease.lease_id, &owner(600), activated_at_ms)
            .unwrap();
        temp.store
            .release_session_owner(
                &lease.lease_id,
                activated_at_ms,
                "reported_exit",
                Some(&owner(600)),
            )
            .unwrap();

        let birth = activated_at_ms * 1_000 + 500;
        let observed = activated_at_ms + 15 * 24 * 60 * 60 * 1_000;
        temp.store
            .bind_session_owner_controllers(&snapshot(
                vec![cli_controller(
                    700,
                    birth,
                    VERSION,
                    501,
                    Some(&selector(NAMESPACE_A)),
                )],
                observed,
            ))
            .unwrap();
        assert!(
            temp.store
                .session_owner_status(&lease.lease_id)
                .unwrap()
                .is_some()
        );

        temp.store
            .bind_session_owner_controllers(&snapshot(Vec::new(), observed + 1))
            .unwrap();
        assert!(
            temp.store
                .session_owner_status(&lease.lease_id)
                .unwrap()
                .is_none()
        );
    }
}
