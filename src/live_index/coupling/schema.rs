//! Schema DDL and migration constants for the coupling store.

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub const META_SCHEMA_VERSION: &str = "schema_version";
pub const META_LAST_HEAD: &str = "last_indexed_head_oid";
pub const META_COLD_BUILT_AT: &str = "cold_build_completed_at";
pub const META_LAST_REFERENCE_TS: &str = "last_reference_ts";
/// Number of `commit_delta` applications since the last VACUUM. Persisted so
/// the compaction cadence survives process restarts. Cold builds reset it to 0
/// because they VACUUM unconditionally.
pub const META_DELTA_SINCE_VACUUM: &str = "delta_since_vacuum";

pub const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS coupling_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS coupling (
    anchor_key     TEXT NOT NULL,
    partner_key    TEXT NOT NULL,
    shared_commits INTEGER NOT NULL,
    weighted_score REAL NOT NULL,
    last_commit_ts INTEGER NOT NULL,
    PRIMARY KEY (anchor_key, partner_key)
);

CREATE INDEX IF NOT EXISTS idx_coupling_anchor_score
    ON coupling (anchor_key, weighted_score DESC);

-- Commits currently inside the bounded window. Source of truth for
-- "what's in the aggregate". Deltas diff this set against the new
-- bounded set to find incoming / outgoing.
CREATE TABLE IF NOT EXISTS coupling_active_commits (
    commit_oid TEXT PRIMARY KEY,
    commit_ts  INTEGER NOT NULL
);

-- Per-commit per-pair contribution ledger. Lets delta subtract the
-- exact contribution of any outgoing commit by recomputing it at the
-- current reference_ts from stored base_weight.
CREATE TABLE IF NOT EXISTS coupling_commit_edges (
    commit_oid  TEXT NOT NULL,
    anchor_key  TEXT NOT NULL,
    partner_key TEXT NOT NULL,
    shared_inc  INTEGER NOT NULL,
    base_weight REAL NOT NULL,
    commit_ts   INTEGER NOT NULL,
    PRIMARY KEY (commit_oid, anchor_key, partner_key)
);

CREATE INDEX IF NOT EXISTS idx_ledger_pair
    ON coupling_commit_edges (anchor_key, partner_key);
"#;
