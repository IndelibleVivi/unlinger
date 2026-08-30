use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use unlinger_core::{
    CleanupReceipt, EvidenceItem, GateLedger, IncidentReport, IncidentState, ProcessRoleCount,
    RootSummary,
};

const SCHEMA_VERSION: i64 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Observation,
    Cleanup,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Cleanup => "cleanup",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "observation" => Ok(Self::Observation),
            "cleanup" => Ok(Self::Cleanup),
            other => Err(StoreError::Corrupt(format!(
                "unknown history event kind {other:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservationRecord {
    pub incident_id: String,
    pub signature_pack: String,
    pub signature_version: String,
    pub state: IncidentState,
    pub root: RootSummary,
    pub member_count: usize,
    pub resident_memory_bytes: u64,
    pub member_fingerprint: String,
    pub roles: Vec<ProcessRoleCount>,
    pub evidence: Vec<EvidenceItem>,
    pub gates: GateLedger,
}

impl From<&IncidentReport> for ObservationRecord {
    fn from(report: &IncidentReport) -> Self {
        Self {
            incident_id: report.incident_id.clone(),
            signature_pack: report.signature_pack.clone(),
            signature_version: report.signature_version.clone(),
            state: report.state,
            root: report.root.clone(),
            member_count: report.member_count,
            resident_memory_bytes: report.resident_memory_bytes,
            member_fingerprint: report.member_fingerprint.clone(),
            roles: report.roles.clone(),
            evidence: report.evidence.clone(),
            gates: report.gates.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum EventPayload {
    Observation { report: ObservationRecord },
    Cleanup { receipt: CleanupReceipt },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryEvent {
    pub event_id: i64,
    pub incident_id: String,
    pub occurred_at_unix_millis: u64,
    pub kind: EventKind,
    pub state: IncidentState,
    pub payload: EventPayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IncidentDetail {
    pub incident_id: String,
    pub events: Vec<HistoryEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetentionPolicy {
    pub max_age_millis: u64,
    pub max_events: usize,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age_millis: 14 * 24 * 60 * 60 * 1_000,
            max_events: 10_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneResult {
    pub removed_events: usize,
    pub remaining_events: usize,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Corrupt(String),
    Range(String),
    Invalid(String),
}

impl Display for StoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "history I/O failed: {error}"),
            Self::Sqlite(error) => write!(formatter, "history SQLite failed: {error}"),
            Self::Json(error) => write!(formatter, "history JSON failed: {error}"),
            Self::Corrupt(message) => write!(formatter, "history data is corrupt: {message}"),
            Self::Range(message) => write!(formatter, "history value is out of range: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid history operation: {message}"),
        }
    }
}

impl Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Clone, Debug)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            let existed = parent.exists();
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            if !existed {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
        let store = Self { path };
        let connection = store.connection()?;
        initialize_schema(&connection)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&store.path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(store)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_observation(
        &self,
        occurred_at_unix_millis: u64,
        report: &IncidentReport,
    ) -> Result<i64, StoreError> {
        let redacted = ObservationRecord::from(report);
        self.insert_event(
            occurred_at_unix_millis,
            &report.incident_id,
            EventKind::Observation,
            report.state,
            &EventPayload::Observation { report: redacted },
        )
    }

    pub fn record_cleanup(
        &self,
        occurred_at_unix_millis: u64,
        receipt: &CleanupReceipt,
    ) -> Result<i64, StoreError> {
        self.insert_event(
            occurred_at_unix_millis,
            &receipt.incident_id,
            EventKind::Cleanup,
            receipt.state,
            &EventPayload::Cleanup {
                receipt: receipt.clone(),
            },
        )
    }

    fn insert_event(
        &self,
        occurred_at_unix_millis: u64,
        incident_id: &str,
        kind: EventKind,
        state: IncidentState,
        payload: &EventPayload,
    ) -> Result<i64, StoreError> {
        let occurred_at = i64::try_from(occurred_at_unix_millis).map_err(|_| {
            StoreError::Range("event timestamp cannot be represented by SQLite".to_owned())
        })?;
        let payload_json = serde_json::to_string(payload)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO events (incident_id, occurred_at_ms, kind, state, payload_json) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                incident_id,
                occurred_at,
                kind.as_str(),
                state_name(state),
                payload_json
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn history(&self, limit: usize) -> Result<Vec<HistoryEvent>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit.min(10_000))
            .map_err(|_| StoreError::Range("history limit overflowed i64".to_owned()))?;
        self.query_events(
            "SELECT id, incident_id, occurred_at_ms, kind, state, payload_json \
             FROM events ORDER BY occurred_at_ms DESC, id DESC LIMIT ?1",
            params![limit],
        )
    }

    pub fn explain(&self, incident_id: &str) -> Result<Option<IncidentDetail>, StoreError> {
        let events = self.query_events(
            "SELECT id, incident_id, occurred_at_ms, kind, state, payload_json \
             FROM events WHERE incident_id = ?1 ORDER BY occurred_at_ms ASC, id ASC",
            params![incident_id],
        )?;
        if events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(IncidentDetail {
                incident_id: incident_id.to_owned(),
                events,
            }))
        }
    }

    pub fn prune(
        &self,
        now_unix_millis: u64,
        policy: RetentionPolicy,
    ) -> Result<PruneResult, StoreError> {
        let before = self.event_count()?;
        let cutoff = now_unix_millis.saturating_sub(policy.max_age_millis);
        let cutoff = i64::try_from(cutoff)
            .map_err(|_| StoreError::Range("retention cutoff overflowed i64".to_owned()))?;
        let max_events = i64::try_from(policy.max_events)
            .map_err(|_| StoreError::Range("retention event limit overflowed i64".to_owned()))?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM events WHERE occurred_at_ms < ?1",
            params![cutoff],
        )?;
        transaction.execute(
            "DELETE FROM events WHERE id NOT IN (\
                 SELECT id FROM events ORDER BY occurred_at_ms DESC, id DESC LIMIT ?1\
             )",
            params![max_events],
        )?;
        transaction.execute(
            "DELETE FROM cooling_candidates WHERE last_seen_ms < ?1",
            params![cutoff],
        )?;
        transaction.commit()?;
        let remaining_events = self.event_count()?;
        Ok(PruneResult {
            removed_events: before.saturating_sub(remaining_events),
            remaining_events,
        })
    }

    pub fn set_pause_until(&self, pause_until_unix_millis: Option<u64>) -> Result<(), StoreError> {
        let connection = self.connection()?;
        match pause_until_unix_millis {
            Some(deadline) => {
                let deadline = i64::try_from(deadline).map_err(|_| {
                    StoreError::Range("pause deadline overflowed SQLite integer".to_owned())
                })?;
                connection.execute(
                    "INSERT INTO settings (key, integer_value) VALUES ('pause_until_ms', ?1) \
                     ON CONFLICT(key) DO UPDATE SET integer_value = excluded.integer_value",
                    params![deadline],
                )?;
            }
            None => {
                connection.execute("DELETE FROM settings WHERE key = 'pause_until_ms'", [])?;
            }
        }
        Ok(())
    }

    pub fn track_cooling(
        &self,
        report: &IncidentReport,
        observed_at_unix_millis: u64,
        abandonment_grace_millis: u64,
        continuity_gap_millis: u64,
    ) -> Result<bool, StoreError> {
        if report.state != IncidentState::Cooling {
            return Err(StoreError::Invalid(
                "only COOLING incidents can advance abandonment grace".to_owned(),
            ));
        }
        let observed_at = i64::try_from(observed_at_unix_millis).map_err(|_| {
            StoreError::Range("cooling observation timestamp overflowed i64".to_owned())
        })?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT first_seen_ms, last_seen_ms, root_identity_fingerprint, member_fingerprint \
                 FROM cooling_candidates WHERE tracking_key = ?1",
                params![report.tracking_key],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let first_seen = match existing {
            Some((first_seen, last_seen, root_identity, members))
                if root_identity == report.root.identity_fingerprint
                    && members == report.member_fingerprint
                    && observed_at >= last_seen
                    && u64::try_from(observed_at - last_seen)
                        .is_ok_and(|gap| gap <= continuity_gap_millis) =>
            {
                first_seen
            }
            _ => observed_at,
        };
        transaction.execute(
            "INSERT INTO cooling_candidates (
                 tracking_key, first_seen_ms, last_seen_ms,
                 root_identity_fingerprint, member_fingerprint
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(tracking_key) DO UPDATE SET
                 first_seen_ms = excluded.first_seen_ms,
                 last_seen_ms = excluded.last_seen_ms,
                 root_identity_fingerprint = excluded.root_identity_fingerprint,
                 member_fingerprint = excluded.member_fingerprint",
            params![
                report.tracking_key,
                first_seen,
                observed_at,
                report.root.identity_fingerprint,
                report.member_fingerprint
            ],
        )?;
        transaction.commit()?;
        let elapsed = u64::try_from(observed_at.saturating_sub(first_seen)).unwrap_or_default();
        Ok(elapsed >= abandonment_grace_millis)
    }

    pub fn retain_cooling(
        &self,
        active_tracking_keys: &BTreeSet<String>,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing = {
            let mut statement =
                transaction.prepare("SELECT tracking_key FROM cooling_candidates")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for tracking_key in existing {
            if !active_tracking_keys.contains(&tracking_key) {
                transaction.execute(
                    "DELETE FROM cooling_candidates WHERE tracking_key = ?1",
                    params![tracking_key],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn pause_until(&self) -> Result<Option<u64>, StoreError> {
        let connection = self.connection()?;
        let value = connection
            .query_row(
                "SELECT integer_value FROM settings WHERE key = 'pause_until_ms'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        value
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    StoreError::Corrupt("negative persisted pause deadline".to_owned())
                })
            })
            .transpose()
    }

    fn event_count(&self) -> Result<usize, StoreError> {
        let connection = self.connection()?;
        let count = connection.query_row("SELECT COUNT(*) FROM events", [], |row| {
            row.get::<_, i64>(0)
        })?;
        usize::try_from(count)
            .map_err(|_| StoreError::Corrupt("negative or oversized event count".to_owned()))
    }

    fn query_events<P: rusqlite::Params>(
        &self,
        sql: &str,
        parameters: P,
    ) -> Result<Vec<HistoryEvent>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(sql)?;
        let raw_rows = statement.query_map(parameters, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in raw_rows {
            let (event_id, incident_id, occurred_at, kind, state, payload_json) = row?;
            let occurred_at_unix_millis = u64::try_from(occurred_at)
                .map_err(|_| StoreError::Corrupt("negative event timestamp".to_owned()))?;
            let kind = EventKind::parse(&kind)?;
            let state = parse_state(&state)?;
            let payload = serde_json::from_str::<EventPayload>(&payload_json)?;
            if payload_kind(&payload) != kind || payload_state(&payload) != state {
                return Err(StoreError::Corrupt(format!(
                    "event {event_id} index columns disagree with payload"
                )));
            }
            events.push(HistoryEvent {
                event_id,
                incident_id,
                occurred_at_unix_millis,
                kind,
                state,
                payload,
            });
        }
        Ok(events)
    }

    fn connection(&self) -> Result<Connection, StoreError> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }
}

fn initialize_schema(connection: &Connection) -> Result<(), StoreError> {
    let user_version =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    if user_version > SCHEMA_VERSION {
        return Err(StoreError::Corrupt(format!(
            "database schema version {user_version} is newer than supported {SCHEMA_VERSION}"
        )));
    }
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS events (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             incident_id TEXT NOT NULL,
             occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
             kind TEXT NOT NULL CHECK (kind IN ('observation', 'cleanup')),
             state TEXT NOT NULL,
             payload_json TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS events_incident_timeline
             ON events (incident_id, occurred_at_ms, id);
         CREATE INDEX IF NOT EXISTS events_recent
             ON events (occurred_at_ms DESC, id DESC);
         CREATE TABLE IF NOT EXISTS settings (
             key TEXT PRIMARY KEY,
             integer_value INTEGER
         );
         CREATE TABLE IF NOT EXISTS cooling_candidates (
             tracking_key TEXT PRIMARY KEY,
             first_seen_ms INTEGER NOT NULL CHECK (first_seen_ms >= 0),
             last_seen_ms INTEGER NOT NULL CHECK (last_seen_ms >= first_seen_ms),
             root_identity_fingerprint TEXT NOT NULL,
             member_fingerprint TEXT NOT NULL
         );
         PRAGMA user_version = 2;",
    )?;
    Ok(())
}

fn state_name(state: IncidentState) -> &'static str {
    match state {
        IncidentState::Protected => "PROTECTED",
        IncidentState::Active => "ACTIVE",
        IncidentState::Cooling => "COOLING",
        IncidentState::Confirmed => "CONFIRMED",
        IncidentState::Ambiguous => "AMBIGUOUS",
        IncidentState::Reclaiming => "RECLAIMING",
        IncidentState::Cleared => "CLEARED",
        IncidentState::Revived => "REVIVED",
        IncidentState::Failed => "FAILED",
    }
}

fn parse_state(value: &str) -> Result<IncidentState, StoreError> {
    match value {
        "PROTECTED" => Ok(IncidentState::Protected),
        "ACTIVE" => Ok(IncidentState::Active),
        "COOLING" => Ok(IncidentState::Cooling),
        "CONFIRMED" => Ok(IncidentState::Confirmed),
        "AMBIGUOUS" => Ok(IncidentState::Ambiguous),
        "RECLAIMING" => Ok(IncidentState::Reclaiming),
        "CLEARED" => Ok(IncidentState::Cleared),
        "REVIVED" => Ok(IncidentState::Revived),
        "FAILED" => Ok(IncidentState::Failed),
        other => Err(StoreError::Corrupt(format!(
            "unknown incident state {other:?}"
        ))),
    }
}

fn payload_kind(payload: &EventPayload) -> EventKind {
    match payload {
        EventPayload::Observation { .. } => EventKind::Observation,
        EventPayload::Cleanup { .. } => EventKind::Cleanup,
    }
}

fn payload_state(payload: &EventPayload) -> IncidentState {
    match payload {
        EventPayload::Observation { report } => report.state,
        EventPayload::Cleanup { receipt } => receipt.state,
    }
}
