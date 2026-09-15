//! Schema-v9 attribution repair. Raw receipts and the previous aggregate are
//! retained; only the derived, action-attributed impact projection is rebuilt.
use super::*;

pub(super) const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS impact_attribution_legacy (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    captured_at_ms INTEGER NOT NULL CHECK (captured_at_ms >= 0),
    payload_json TEXT NOT NULL CHECK (length(payload_json) <= 65536)
);";

pub(super) fn repair_legacy_impacts(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    let (terminal_count, prior) = transaction.query_row(
        "SELECT tracking_started_at_ms, historical_completeness,
                terminal_cleanup_count, proved_reclaim_count,
                reclaimed_process_count, reclaimed_process_measurement_count,
                estimated_reclaimed_memory_bytes, reclaimed_memory_measurement_count
         FROM impact_authority WHERE singleton = 1",
        [],
        |row| {
            let count: i64 = row.get(2)?;
            Ok((
                count,
                serde_json::json!({
                    "tracking_started_at_ms": row.get::<_, i64>(0)?,
                    "historical_completeness": row.get::<_, String>(1)?,
                    "terminal_cleanup_count": count,
                    "proved_reclaim_count": row.get::<_, i64>(3)?,
                    "reclaimed_process_count": row.get::<_, i64>(4)?,
                    "reclaimed_process_measurement_count": row.get::<_, i64>(5)?,
                    "estimated_reclaimed_memory_bytes": row.get::<_, i64>(6)?,
                    "reclaimed_memory_measurement_count": row.get::<_, i64>(7)?
                }),
            ))
        },
    )?;
    let retained_count: i64 =
        transaction.query_row("SELECT count(*) FROM cleanup_impacts", [], |row| row.get(0))?;
    if retained_count > terminal_count {
        return Err(StoreError::Corrupt(
            "retained impact count exceeds its cumulative authority".to_owned(),
        ));
    }
    transaction.execute(
        "INSERT OR IGNORE INTO impact_attribution_legacy
             (singleton, captured_at_ms, payload_json) VALUES (1, ?1, ?2)",
        params![
            sqlite_millis(current_unix_millis()?, "attribution migration")?,
            prior.to_string()
        ],
    )?;
    transaction.execute(
        "UPDATE cleanup_impacts
         SET process_count = NULL, estimated_reclaimed_memory_bytes = NULL
         WHERE process_outcome != 'cleared' OR NOT EXISTS (
             SELECT 1 FROM cleanup_actions AS action
             WHERE action.attempt_id = cleanup_impacts.attempt_id
               AND action.disposition = 'delivered'
         )",
        [],
    )?;
    let (proved, processes, process_measurements, memory, memory_measurements) = transaction
        .query_row(
            "SELECT count(*), COALESCE(sum(process_count), 0), count(process_count),
                COALESCE(sum(estimated_reclaimed_memory_bytes), 0),
                count(estimated_reclaimed_memory_bytes)
         FROM cleanup_impacts AS impact
         WHERE process_outcome = 'cleared' AND EXISTS (
             SELECT 1 FROM cleanup_actions AS action
             WHERE action.attempt_id = impact.attempt_id
               AND action.disposition = 'delivered'
         )",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )?;
    // Pruned pre-v9 details cannot be retrospectively attributed. Preserve the
    // original totals in the legacy row, mark a partial history, and publish
    // only proved retained contributions. Later pruning keeps these new totals.
    transaction.execute(
        "UPDATE impact_authority
         SET proved_reclaim_count = ?1, reclaimed_process_count = ?2,
             reclaimed_process_measurement_count = ?3,
             estimated_reclaimed_memory_bytes = ?4,
             reclaimed_memory_measurement_count = ?5,
             historical_completeness = CASE WHEN ?6 THEN 'partial_backfill'
                                           ELSE historical_completeness END
         WHERE singleton = 1",
        params![
            proved,
            processes,
            process_measurements,
            memory,
            memory_measurements,
            retained_count < terminal_count
        ],
    )?;
    Ok(())
}
