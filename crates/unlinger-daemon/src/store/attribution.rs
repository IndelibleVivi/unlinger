//! Current-v14 evidence of the retired schema-v9 attribution backfill.

pub(super) const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS impact_attribution_legacy (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    captured_at_ms INTEGER NOT NULL CHECK (captured_at_ms >= 0),
    payload_json TEXT NOT NULL CHECK (length(payload_json) <= 65536)
);";
