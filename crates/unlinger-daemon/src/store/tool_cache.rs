//! Latest producer-cache observation and attempt. No raw paths or output.
use super::*;
use unlinger_protocol::{
    ToolCacheAttemptSummary, ToolCacheAvailability, ToolCacheKind, ToolCacheMaintenanceSummary,
    ToolCacheOutcome,
};

pub(crate) const SCHEMA_SQL: &str = "
CREATE TABLE tool_cache_latest (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    availability_json TEXT NOT NULL,
    attempt_token TEXT,
    attempt_json TEXT
);";

#[derive(Debug)]
pub struct PreparedToolCacheAttempt {
    token: String,
    prepared_at_unix_millis: u64,
}

impl HistoryStore {
    pub fn record_tool_cache_observation(
        &self,
        now: u64,
        availability: ToolCacheAvailability,
    ) -> Result<(), StoreError> {
        record_observation(&self.connection()?, now, availability)
    }

    /// Keeps the previous attempt alongside a fresh independent observation.
    pub fn latest_tool_cache_maintenance(
        &self,
    ) -> Result<Option<ToolCacheMaintenanceSummary>, StoreError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT observed_at_ms, availability_json, attempt_json
             FROM tool_cache_latest WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(observed, availability, attempt)| {
            Ok(ToolCacheMaintenanceSummary {
                kind: ToolCacheKind::NpmDownloadCache,
                observed_at_unix_millis: parse_nonnegative_millis(observed, "cache observation")?,
                availability: serde_json::from_str(&availability)?,
                // Eligibility is a current lifecycle decision, never durable authority.
                automatic_maintenance_eligible: false,
                last_attempt: attempt
                    .map(|json| serde_json::from_str(&json))
                    .transpose()?,
            })
        })
        .transpose()
    }

    pub fn begin_tool_cache_attempt(
        &self,
        now: u64,
    ) -> Result<PreparedToolCacheAttempt, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<String> = transaction
            .query_row(
                "SELECT attempt_json FROM tool_cache_latest WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if previous
            .as_deref()
            .map(serde_json::from_str::<ToolCacheAttemptSummary>)
            .transpose()?
            .is_some_and(|attempt| attempt.outcome == ToolCacheOutcome::Running)
        {
            return Err(StoreError::Invalid(
                "cache maintenance already prepared".to_owned(),
            ));
        }
        record_observation(&transaction, now, ToolCacheAvailability::Available)?;
        let attempt = ToolCacheAttemptSummary {
            outcome: ToolCacheOutcome::Running,
            prepared_at_unix_millis: now,
            completed_at_unix_millis: None,
            native_removed_entry_count: None,
            native_removed_logical_bytes: None,
        };
        transaction.execute(
            "UPDATE tool_cache_latest SET attempt_token = lower(hex(randomblob(16))), attempt_json = ?1
             WHERE singleton = 1",
            [serde_json::to_string(&attempt)?],
        )?;
        let token = transaction.query_row(
            "SELECT attempt_token FROM tool_cache_latest WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(PreparedToolCacheAttempt {
            token,
            prepared_at_unix_millis: now,
        })
    }

    /// Native result and post-run availability settle atomically. A late result
    /// cannot overwrite crash recovery or a newer invocation.
    pub fn complete_tool_cache_attempt(
        &self,
        prepared: &PreparedToolCacheAttempt,
        result: &ToolCacheAttemptSummary,
        availability: ToolCacheAvailability,
    ) -> Result<(), StoreError> {
        let completed = result
            .completed_at_unix_millis
            .ok_or_else(|| StoreError::Invalid("cache result has no completion time".to_owned()))?;
        let success = matches!(
            result.outcome,
            ToolCacheOutcome::Completed | ToolCacheOutcome::NoOp
        );
        if result.outcome == ToolCacheOutcome::Running
            || result.prepared_at_unix_millis != prepared.prepared_at_unix_millis
            || completed < prepared.prepared_at_unix_millis
            || (success
                && (result.native_removed_entry_count.is_none()
                    || result.native_removed_logical_bytes.is_none()))
            || (!success
                && (result.native_removed_entry_count.is_some()
                    || result.native_removed_logical_bytes.is_some()))
            || (result.outcome == ToolCacheOutcome::NoOp
                && (result.native_removed_entry_count != Some(0)
                    || result.native_removed_logical_bytes != Some(0)))
        {
            return Err(StoreError::Invalid("inconsistent cache result".to_owned()));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE tool_cache_latest SET attempt_json = ?1, attempt_token = NULL
             WHERE singleton = 1 AND attempt_token = ?2",
            params![serde_json::to_string(result)?, prepared.token],
        )?;
        if changed != 1 {
            return Err(StoreError::Invalid(
                "cache attempt is no longer prepared".to_owned(),
            ));
        }
        record_observation(&transaction, completed, availability)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn recover_tool_cache_attempt(&self, now: u64) -> Result<bool, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let json: Option<String> = transaction.query_row(
            "SELECT attempt_json FROM tool_cache_latest WHERE singleton = 1 AND attempt_token IS NOT NULL",
            [], |row| row.get(0),
        ).optional()?;
        let Some(json) = json else { return Ok(false) };
        let mut result: ToolCacheAttemptSummary = serde_json::from_str(&json)?;
        result.outcome = ToolCacheOutcome::DeliveryUnknown;
        result.completed_at_unix_millis = Some(now.max(result.prepared_at_unix_millis));
        result.native_removed_entry_count = None;
        result.native_removed_logical_bytes = None;
        transaction.execute(
            "UPDATE tool_cache_latest SET attempt_token = NULL, attempt_json = ?1 WHERE singleton = 1",
            [serde_json::to_string(&result)?],
        )?;
        transaction.commit()?;
        Ok(true)
    }
}

fn record_observation(
    connection: &Connection,
    now: u64,
    availability: ToolCacheAvailability,
) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO tool_cache_latest(singleton, observed_at_ms, availability_json) VALUES(1, ?1, ?2)
         ON CONFLICT(singleton) DO UPDATE SET observed_at_ms = excluded.observed_at_ms,
             availability_json = excluded.availability_json",
        params![sqlite_millis(now, "cache observation")?, serde_json::to_string(&availability)?],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStore {
        store: HistoryStore,
        directory: PathBuf,
    }
    impl TempStore {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "ul-cache-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&directory).unwrap();
            Self {
                store: HistoryStore::open(directory.join("history.db")).unwrap(),
                directory,
            }
        }
    }
    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn v12_requires_cache_authority_on_reopen() {
        let temp = TempStore::new();
        let connection = temp.store.connection().unwrap();
        connection
            .execute_batch("DROP TABLE tool_cache_latest")
            .unwrap();
        assert!(matches!(
            validate_required_schema(&connection),
            Err(StoreError::Corrupt(message)) if message.contains("tool_cache_latest")
        ));
    }

    #[test]
    fn recovery_never_turns_new_observation_into_reclaim() {
        let temp = TempStore::new();
        let store = &temp.store;
        let prepared = store.begin_tool_cache_attempt(100).unwrap();
        assert!(store.begin_tool_cache_attempt(101).is_err());
        assert!(store.recover_tool_cache_attempt(102).unwrap());
        store
            .record_tool_cache_observation(103, ToolCacheAvailability::Absent)
            .unwrap();
        let state = store.latest_tool_cache_maintenance().unwrap().unwrap();
        let attempt = state.last_attempt.unwrap();
        assert_eq!(state.availability, ToolCacheAvailability::Absent);
        assert_eq!(attempt.outcome, ToolCacheOutcome::DeliveryUnknown);
        assert_eq!(attempt.native_removed_logical_bytes, None);
        assert!(
            store
                .complete_tool_cache_attempt(&prepared, &attempt, ToolCacheAvailability::Available)
                .is_err()
        );
    }

    #[test]
    fn terminal_result_requires_native_accounting_and_masks_previous_success() {
        let temp = TempStore::new();
        let store = &temp.store;
        let prepared = store.begin_tool_cache_attempt(100).unwrap();
        let mut result = ToolCacheAttemptSummary {
            outcome: ToolCacheOutcome::Completed,
            prepared_at_unix_millis: 100,
            completed_at_unix_millis: Some(101),
            native_removed_entry_count: Some(2),
            native_removed_logical_bytes: Some(20),
        };
        store
            .complete_tool_cache_attempt(&prepared, &result, ToolCacheAvailability::Available)
            .unwrap();
        assert_eq!(
            store
                .latest_tool_cache_maintenance()
                .unwrap()
                .unwrap()
                .last_attempt,
            Some(result.clone())
        );
        let next = store.begin_tool_cache_attempt(200).unwrap();
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
        result.prepared_at_unix_millis = 200;
        result.completed_at_unix_millis = Some(201);
        result.outcome = ToolCacheOutcome::Failed;
        assert!(
            store
                .complete_tool_cache_attempt(&next, &result, ToolCacheAvailability::Unavailable)
                .is_err()
        );
        result.native_removed_entry_count = None;
        result.native_removed_logical_bytes = None;
        store
            .complete_tool_cache_attempt(&next, &result, ToolCacheAvailability::Unavailable)
            .unwrap();
    }

    #[test]
    fn v11_migration_preserves_settings_and_atomic_result_rolls_back() {
        let temp = TempStore::new();
        temp.store.set_pause_until(Some(500)).unwrap();
        temp.store
            .connection()
            .unwrap()
            .execute_batch("DROP TABLE tool_cache_latest; PRAGMA user_version = 11;")
            .unwrap();
        let migrated = HistoryStore::open(temp.store.path()).unwrap();
        assert_eq!(HistoryStore::schema_version(), 12);
        assert_eq!(migrated.pause_until().unwrap(), Some(500));
        assert!(migrated.latest_tool_cache_maintenance().unwrap().is_none());
        let prepared = migrated.begin_tool_cache_attempt(100).unwrap();
        let result = ToolCacheAttemptSummary {
            outcome: ToolCacheOutcome::NoOp,
            prepared_at_unix_millis: 100,
            completed_at_unix_millis: Some(u64::MAX),
            native_removed_entry_count: Some(0),
            native_removed_logical_bytes: Some(0),
        };
        assert!(
            migrated
                .complete_tool_cache_attempt(&prepared, &result, ToolCacheAvailability::Available)
                .is_err()
        );
        let state = migrated.latest_tool_cache_maintenance().unwrap().unwrap();
        assert_eq!(state.observed_at_unix_millis, 100);
        assert_eq!(
            state.last_attempt.unwrap().outcome,
            ToolCacheOutcome::Running
        );
    }
}
