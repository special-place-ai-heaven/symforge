//! Schema DDL and migration constants for the local analytics store.

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub const META_SCHEMA_VERSION: &str = "schema_version";

pub const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS analytics_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS analytics_tool_calls (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name             TEXT NOT NULL CHECK (length(CAST(tool_name AS BLOB)) <= 128),
    surface               TEXT NOT NULL CHECK (length(CAST(surface AS BLOB)) <= 64),
    configured_scope      TEXT NOT NULL CHECK (length(CAST(configured_scope AS BLOB)) <= 128),
    response_bytes        INTEGER NOT NULL CHECK (response_bytes >= 0),
    estimated_tokens      INTEGER CHECK (estimated_tokens IS NULL OR estimated_tokens >= 0),
    duration_ms           INTEGER NOT NULL CHECK (duration_ms >= 0),
    success               INTEGER NOT NULL CHECK (success IN (0, 1)),
    outcome_class         TEXT NOT NULL CHECK (
        outcome_class IN (
            'found',
            'not_found',
            'ambiguous',
            'invalid_request',
            'empty_result',
            'internal_failure'
        )
    ),
    capability_state_json TEXT NOT NULL CHECK (length(CAST(capability_state_json AS BLOB)) <= 4096)
);

CREATE INDEX IF NOT EXISTS idx_analytics_tool_calls_tool_outcome
    ON analytics_tool_calls (tool_name, outcome_class);
"#;
