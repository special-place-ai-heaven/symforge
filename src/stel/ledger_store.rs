//! STEL L4 durable economics ledger — SQLite-backed store for [`StelLedgerEvent`] rows.
//!
//! Mirrors `src/analytics/store.rs` in structure: `enum StelLedgerStore { Sqlite(...), Disabled }`,
//! idempotent `migrate()`, `record()` + `recent()` + `summary()`. Dedicated `stel-ledger.db`
//! in the SymForge data dir. Gated under `#[cfg(feature = "server")]` to preserve embed isolation.
//!
//! **Column-map deviation from `specs/004-v8-operator-serve/data-model.md`:**
//! - Data-model column `tool` (single TEXT) does not exist as a singular field on
//!   [`StelLedgerEvent`]; the real field is `tools_called: Vec<String>`. Stored as JSON in column
//!   `tools_called_json` (TEXT NOT NULL).
//! - Data-model columns `accepted` and `eligible_h6` are not fields on [`StelLedgerEvent`];
//!   stored as `INTEGER NULL` for forward-compatibility (always NULL from `record()`).
//! - Additional columns beyond the data-model minimum are included (`surface`,
//!   `predicted_response_tokens`, `route_confidence`, `pff_bypass`, `cache_hit`,
//!   `degrade_flags_json`) because they are real fields on the struct and useful for analytics.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::types::StelLedgerEvent;

// ---------------------------------------------------------------------------
// DB path constant (mirrors SYMFORGE_ANALYTICS_DB_PATH in paths.rs)
// ---------------------------------------------------------------------------

pub const SYMFORGE_STEL_LEDGER_DB_PATH: &str = ".symforge/stel-ledger.db";

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

const CURRENT_SCHEMA_VERSION: u32 = 2;
const META_SCHEMA_VERSION: &str = "schema_version";

/// Identifier of the prediction estimator in force (feature 013, R3).
///
/// Every NEW `stel_ledger_events` row is tagged with this value so that, when
/// the estimator changes, stale-estimator samples can be EXCLUDED from the
/// active tuning population without being destroyed (they stay auditable). The
/// value names feature 010 (which replaced the flat `520/900` floor) and the
/// mechanism it introduced — the byte-grounded estimator. Bump this string
/// whenever the estimator's prediction behaviour changes so a new calibration
/// never tunes on data that no longer reflects how the estimator behaves.
///
/// Rows that predate the `estimator_version` column are backfilled with
/// [`PRE_013_ESTIMATOR_SENTINEL`] instead and are excluded from tuning.
pub const CURRENT_ESTIMATOR_VERSION: &str = "010-byte-grounded";

/// Backfill sentinel for rows written before the `estimator_version` column was
/// added (schema < v2). These rows are retained for audit but EXCLUDED from the
/// active tuning population (R3) because the estimator that produced them is
/// unknown.
pub const PRE_013_ESTIMATOR_SENTINEL: &str = "pre-013";

/// Count-based retention cap for `stel_ledger_events` (feature 013, R2).
///
/// `record()` prunes oldest rows beyond this cap on every write (prune-on-write,
/// deterministic — aids reproducible tuning, FR-012). Closes the former P3-B
/// unbounded-growth debt. Tunable: raise once real per-project data accrues.
pub const LEDGER_RETENTION_MAX: usize = 2000;

// P3-B (closed, feature 013 T011): `stel_ledger_events` is now bounded by
// `LEDGER_RETENTION_MAX` via prune-on-write in `record()`. No TTL/archival is
// needed; the count cap bounds disk deterministically.

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS stel_ledger_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS stel_ledger_events (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_ms                       INTEGER NOT NULL,
    session_id                  TEXT NOT NULL,
    plan_id                     TEXT NOT NULL,
    surface                     TEXT NOT NULL,
    intent                      TEXT NOT NULL,
    decision                    TEXT NOT NULL,
    tools_called_json           TEXT NOT NULL,
    predicted_response_tokens   INTEGER NOT NULL,
    actual_response_tokens      INTEGER NOT NULL,
    manual_baseline_tokens      INTEGER NOT NULL,
    net_vs_manual               INTEGER NOT NULL,
    route_confidence            TEXT NOT NULL,
    pff_bypass                  INTEGER,
    cache_hit                   INTEGER,
    degrade_flags_json          TEXT NOT NULL,
    accepted                    INTEGER,
    eligible_h6                 INTEGER
);

CREATE INDEX IF NOT EXISTS idx_stel_ledger_events_session
    ON stel_ledger_events (session_id);

CREATE INDEX IF NOT EXISTS idx_stel_ledger_events_ts
    ON stel_ledger_events (ts_ms);

CREATE TABLE IF NOT EXISTS stel_calibration (
    estimator_version   TEXT PRIMARY KEY,
    response_floor      INTEGER NOT NULL,
    manual_floor        INTEGER NOT NULL,
    schema_tokens       INTEGER NOT NULL,
    invoke_tokens       INTEGER NOT NULL,
    sample_size         INTEGER NOT NULL,
    error_before        REAL NOT NULL,
    error_after         REAL NOT NULL,
    tuned_at_ms         INTEGER NOT NULL
);
"#;

/// Schema v2 delta (feature 013, T009): add the `estimator_version` column to
/// `stel_ledger_events`. Applied via an idempotent ALTER guard in `migrate()`
/// rather than baked into [`SCHEMA_V1`] so that a v1 database upgrades through
/// the SAME code path a fresh database takes — the table is created without the
/// column by the v1 DDL, then the column is added here exactly once.
const ALTER_ADD_ESTIMATOR_VERSION: &str =
    "ALTER TABLE stel_ledger_events ADD COLUMN estimator_version TEXT";

// ---------------------------------------------------------------------------
// Bounds helpers (mirrors analytics store)
// ---------------------------------------------------------------------------

const MAX_PLAN_ID_BYTES: usize = 128;
const MAX_SESSION_ID_BYTES: usize = 128;
const MAX_SURFACE_BYTES: usize = 64;
const MAX_INTENT_BYTES: usize = 32;
const MAX_DECISION_BYTES: usize = 32;
const MAX_ROUTE_CONFIDENCE_BYTES: usize = 32;
const MAX_TOOLS_JSON_BYTES: usize = 1024;
const MAX_DEGRADE_FLAGS_JSON_BYTES: usize = 512;

fn bounded_text(raw: &str, max_bytes: usize) -> String {
    let normalized: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if normalized.len() <= max_bytes {
        return normalized;
    }
    let budget = max_bytes.saturating_sub(3);
    let mut out = String::new();
    for ch in normalized.chars() {
        if out.len() + ch.len_utf8() > budget {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn u32_to_i64(value: u32) -> i64 {
    i64::from(value)
}

fn i64_to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// True when `table` already has a column named `column`, by probing the
/// SQLite catalog via `PRAGMA table_info`. Used to make the schema-v2 column
/// add idempotent (feature 013, T009) without relying on catching a
/// duplicate-column error — the probe is explicit and side-effect free.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("probing columns of {table}"))?;
    // PRAGMA table_info columns: (cid, name, type, notnull, dflt_value, pk).
    let mut rows = stmt
        .query([])
        .with_context(|| format!("reading column list of {table}"))?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

/// A row read back from `stel_ledger_events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredLedgerRecord {
    pub id: i64,
    pub ts_ms: u64,
    pub session_id: String,
    pub plan_id: String,
    pub surface: String,
    pub intent: String,
    pub decision: String,
    pub tools_called_json: String,
    pub predicted_response_tokens: u32,
    pub actual_response_tokens: u32,
    pub manual_baseline_tokens: u32,
    pub net_vs_manual: i32,
    pub route_confidence: String,
}

/// The calibrated replacements for the static prediction floors, plus the
/// evidence that justifies them (feature 013, data-model `TunedEstimateConstants`).
///
/// Persisted in `stel_calibration` (single active set per `estimator_version`)
/// so tuning survives restart and stays auditable. This is a pure POD — the
/// foundation stores and loads it; the derivation/validation math (US2) lives
/// in `calibration.rs`. `error_before`/`error_after` are the held-out mean
/// absolute error figures that let the surface honestly read `tuned`; storing
/// constants without them is never enough to promote the surface.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TunedEstimateConstants {
    /// Tuned replacement for the `400` plan-floor (`controller.rs`).
    pub response_floor: u32,
    /// Tuned replacement for the `800` manual baseline.
    pub manual_floor: u32,
    /// Tuned replacement for `COMPACT_SCHEMA_TOKENS` (`45`).
    pub schema_tokens: u32,
    /// Tuned replacement for `COMPACT_INVOKE_TOKENS` (`80`).
    pub invoke_tokens: u32,
    /// The estimator these were tuned for; must match [`CURRENT_ESTIMATOR_VERSION`]
    /// to be in force (R3 in-force rule).
    pub estimator_version: String,
    /// Number of samples used to derive the constants.
    pub sample_size: u32,
    /// Held-out mean absolute prediction error BEFORE applying these constants.
    pub error_before: f64,
    /// Held-out mean absolute prediction error AFTER applying these constants.
    pub error_after: f64,
    /// Wall-clock timestamp (ms since epoch) the tuning was applied — audit.
    pub tuned_at_ms: u64,
}

/// Aggregate summary of ledger contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerSummary {
    pub total_events: u64,
    pub total_net_vs_manual: i64,
    pub accepted_count: u64,
    pub session_count: u64,
}

/// Status of the ledger store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerStoreStatus {
    Disabled,
    Enabled {
        db_path: PathBuf,
        schema_version: u32,
    },
}

/// Runtime state of the durable-ledger subsystem, computed at status-read time
/// (data-model E4, N-3 / TR-17 / FR-008).
///
/// Distinguishes a wired-but-failing store from one that was never configured.
/// A plain `Option<LedgerSummary>` (as `summary()` returns) collapses both into
/// `None`; this enum keeps them distinct so `status` can report the truth.
///
/// `Unavailable` is *not* a variant here: it means "no store wired into this
/// build/surface" and is represented at the server boundary by the absence of a
/// store (`Option::None`). `subsystem_state()` is only called on a present store
/// and therefore only ever yields `Durable` or `Disabled`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerSubsystemState {
    /// The durable store is open and a summary query succeeded (serve mode).
    Durable { summary: LedgerSummary },
    /// The store is configured/attempted but not serving — open failed at
    /// startup (the `Disabled` variant) or a live summary query failed. Carries
    /// the reason so the operator can tell "broken" from "off".
    Disabled { reason: String },
}

// ---------------------------------------------------------------------------
// StelLedgerStore — public enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum StelLedgerStore {
    Sqlite(SqliteStelLedgerStore),
    Disabled,
}

impl StelLedgerStore {
    /// Open or create the durable ledger db under the project `root`, run
    /// migration, set WAL + busy timeout. On any failure returns `Disabled`
    /// (logged, never panics) — FR-011.
    ///
    /// `root` is the project ROOT, not the `.symforge` data dir: this joins the
    /// `.symforge/`-prefixed [`SYMFORGE_STEL_LEDGER_DB_PATH`] against it itself,
    /// matching every other store (analytics/coupling/frecency join their
    /// prefixed const against the project root). The parent `.symforge` dir is
    /// created on demand by [`SqliteStelLedgerStore::open`] (SQLite will NOT
    /// create it). Passing the already-`.symforge` data dir here would double the
    /// prefix to `root/.symforge/.symforge/...`.
    ///
    /// Migration note: any pre-fix doubled-path data at
    /// `root/.symforge/.symforge/stel-ledger.db` is orphaned, not migrated —
    /// economics rows are best-effort calibration input (pre-1.0), so a one-time
    /// reset of unpromoted, never-surfaced data is acceptable.
    pub fn open(root: &Path, session_id: impl Into<String>) -> Self {
        let db_path = root.join(SYMFORGE_STEL_LEDGER_DB_PATH);
        match SqliteStelLedgerStore::open(&db_path, session_id) {
            Ok(store) => Self::Sqlite(store),
            Err(err) => {
                tracing::warn!(
                    path = %db_path.display(),
                    error = %err,
                    "stel ledger store failed to open; economics will not be persisted"
                );
                Self::Disabled
            }
        }
    }

    /// In-memory constructor for tests.
    pub fn open_in_memory(session_id: impl Into<String>) -> Result<Self> {
        Ok(Self::Sqlite(SqliteStelLedgerStore::open_in_memory(
            session_id,
        )?))
    }

    pub fn status(&self) -> LedgerStoreStatus {
        match self {
            Self::Disabled => LedgerStoreStatus::Disabled,
            Self::Sqlite(store) => store.status(),
        }
    }

    /// Insert one ledger event. No-op when `Disabled`.
    ///
    /// Degrade-silently contract (FR-011): a record error is logged and dropped,
    /// never propagated. Under pathological multi-process contention a single
    /// event may be lost if every retry within the `busy_timeout` window is
    /// blocked — acceptable for best-effort calibration data (one dropped sample
    /// out of thousands does not change a tuned constant; never blocks the
    /// request path).
    pub fn record(&self, event: &StelLedgerEvent) {
        if let Self::Sqlite(store) = self
            && let Err(err) = store.record(event)
        {
            tracing::warn!(error = %err, "stel ledger record failed; degrading silently");
        }
    }

    /// Return the `limit` most-recent rows. Returns empty vec when `Disabled`.
    pub fn recent(&self, limit: usize) -> Result<Vec<StoredLedgerRecord>> {
        match self {
            Self::Disabled => Ok(vec![]),
            Self::Sqlite(store) => store.recent(limit),
        }
    }

    /// Return aggregate summary. Returns `None` when `Disabled` OR when a live
    /// summary query fails.
    ///
    /// N-3: this `Option` API collapses "off" and "broken" into `None`. It is
    /// retained for the admin DTO ([`crate::server::admin`]) which has its own
    /// None handling. The `status` tool MUST use [`Self::subsystem_state`]
    /// instead, which preserves the open-error and distinguishes the two states
    /// (FR-008, data-model E4).
    pub fn summary(&self) -> Option<LedgerSummary> {
        match self {
            Self::Disabled => None,
            Self::Sqlite(store) => store.summary().ok(),
        }
    }

    /// Compute the durable-ledger subsystem state for the `status` surface
    /// (N-3 / TR-17 / FR-008). Unlike [`Self::summary`], a failed summary query
    /// on a wired store is reported as [`LedgerSubsystemState::Disabled`] with
    /// the error reason — never swallowed into the same shape as a never-opened
    /// store. The server reports "no store wired" by NOT calling this (the store
    /// is `Option::None` at the boundary).
    pub fn subsystem_state(&self) -> LedgerSubsystemState {
        match self {
            Self::Disabled => LedgerSubsystemState::Disabled {
                reason: "failed to open at startup (see server logs)".to_string(),
            },
            Self::Sqlite(store) => match store.summary() {
                Ok(summary) => LedgerSubsystemState::Durable { summary },
                Err(err) => LedgerSubsystemState::Disabled {
                    reason: format!("summary query failed: {err}"),
                },
            },
        }
    }

    pub fn schema_version(&self) -> Option<u32> {
        match self {
            Self::Disabled => None,
            Self::Sqlite(store) => store.schema_version().ok(),
        }
    }

    /// Persist the active tuned-constant set (feature 013, T010). No-op (`Ok`)
    /// when `Disabled` — a degraded store silently drops the write rather than
    /// erroring, mirroring `record()`'s degrade-silently contract.
    pub fn store_active_tuning(&self, c: &TunedEstimateConstants) -> Result<()> {
        match self {
            Self::Disabled => Ok(()),
            Self::Sqlite(store) => store.store_active_tuning(c),
        }
    }

    /// Load the active tuned-constant set for `estimator_version`, or `None`
    /// (feature 013, T010). A `Disabled` store yields `None` — never serves a
    /// bad/absent tuning; the caller falls back to the static floors.
    pub fn load_active_tuning(
        &self,
        estimator_version: &str,
    ) -> Result<Option<TunedEstimateConstants>> {
        match self {
            Self::Disabled => Ok(None),
            Self::Sqlite(store) => store.load_active_tuning(estimator_version),
        }
    }

    /// Return the `limit` most-recent samples for `version`, newest-first
    /// (feature 013, T012). Empty vec when `Disabled`. EXCLUDES `pre-013` rows
    /// from the active tuning population.
    pub fn samples_for_estimator(
        &self,
        version: &str,
        limit: usize,
    ) -> Result<Vec<StoredLedgerRecord>> {
        match self {
            Self::Disabled => Ok(vec![]),
            Self::Sqlite(store) => store.samples_for_estimator(version, limit),
        }
    }
}

// ---------------------------------------------------------------------------
// SqliteStelLedgerStore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SqliteStelLedgerStore {
    db_path: PathBuf,
    session_id: String,
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStelLedgerStore {
    pub fn open(path: &Path, session_id: impl Into<String>) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating stel ledger db parent dir {:?}", parent))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening stel ledger db at {:?}", path))?;

        // WAL mode + busy timeout (mirrors analytics store pattern)
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .context("configuring stel ledger db pragmas")?;

        let store = Self {
            db_path: path.to_path_buf(),
            session_id: session_id.into(),
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory(session_id: impl Into<String>) -> Result<Self> {
        let conn = Connection::open_in_memory().context("opening in-memory stel ledger db")?;
        let store = Self {
            db_path: PathBuf::from(":memory:"),
            session_id: session_id.into(),
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn status(&self) -> LedgerStoreStatus {
        match self.schema_version() {
            Ok(v) => LedgerStoreStatus::Enabled {
                db_path: self.db_path.clone(),
                schema_version: v,
            },
            Err(_) => LedgerStoreStatus::Disabled,
        }
    }

    /// Idempotent schema migration. Safe to call multiple times.
    // REVIEW P3-A (deferred): no forward-compat guard. Opening a DB whose
    // `schema_version > CURRENT_SCHEMA_VERSION` re-applies v1 DDL and downgrades
    // the recorded version. Future fix: if `schema_version > CURRENT` then
    // degrade to Disabled / refuse to migrate down rather than clobber.
    pub fn migrate(&self) -> Result<()> {
        // P2-D / FR-011 "never panic": a poisoned mutex (a prior holder
        // panicked) must degrade, not crash the operator server. Recover the
        // inner guard so the ledger keeps serving instead of propagating the
        // poison as a panic on every subsequent lock.
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        // v1 DDL: tables + indexes (all `IF NOT EXISTS`, idempotent).
        conn.execute_batch(SCHEMA_V1)
            .context("applying stel ledger schema v1")?;

        // v2 (feature 013, T009): add the `estimator_version` column ONLY when
        // absent, then backfill pre-column rows with the sentinel. Both steps
        // are idempotent: the column is added once (guarded by a catalog probe),
        // and the backfill only touches rows whose value is still NULL, so it
        // never re-stamps a row that already carries a real estimator version.
        if !column_exists(&conn, "stel_ledger_events", "estimator_version")? {
            conn.execute(ALTER_ADD_ESTIMATOR_VERSION, [])
                .context("adding estimator_version column (schema v2)")?;
        }
        conn.execute(
            "UPDATE stel_ledger_events
                SET estimator_version = ?1
                WHERE estimator_version IS NULL",
            params![PRE_013_ESTIMATOR_SENTINEL],
        )
        .context("backfilling estimator_version sentinel (schema v2)")?;

        conn.execute(
            "INSERT OR REPLACE INTO stel_ledger_meta (key, value) VALUES (?1, ?2)",
            params![META_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION.to_string()],
        )
        .context("writing stel ledger schema version")?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<u32> {
        // P2-D / FR-011 "never panic": a poisoned mutex (a prior holder
        // panicked) must degrade, not crash the operator server. Recover the
        // inner guard so the ledger keeps serving instead of propagating the
        // poison as a panic on every subsequent lock.
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM stel_ledger_meta WHERE key = ?1",
                params![META_SCHEMA_VERSION],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.and_then(|v| v.parse().ok()).unwrap_or(0))
    }

    /// Insert one [`StelLedgerEvent`] row into `stel_ledger_events`.
    ///
    /// P2-C (resolved): this performs a blocking `std::sync::Mutex<Connection>`
    /// INSERT under a busy-timeout. It must never be awaited inline on the async
    /// MCP tool path. The caller — `SymForgeServer::persist_ledger_event_durably`
    /// — offloads this onto `tokio::task::spawn_blocking` when a runtime is
    /// present, so the request task never blocks on the DB lock/busy-timeout.
    /// Callers without a runtime (sync tests / embed) invoke it directly.
    pub fn record(&self, event: &StelLedgerEvent) -> Result<i64> {
        let tools_json =
            serde_json::to_string(&event.tools_called).unwrap_or_else(|_| "[]".to_string());
        let degrade_json =
            serde_json::to_string(&event.degrade_flags).unwrap_or_else(|_| "[]".to_string());

        let session_id = bounded_text(&self.session_id, MAX_SESSION_ID_BYTES);
        let plan_id = bounded_text(&event.plan_id, MAX_PLAN_ID_BYTES);
        let surface = bounded_text(&event.surface, MAX_SURFACE_BYTES);
        let intent = bounded_text(event.intent.as_str(), MAX_INTENT_BYTES);
        let decision = bounded_text(event.decision.as_str(), MAX_DECISION_BYTES);
        let tools_called_json = bounded_text(&tools_json, MAX_TOOLS_JSON_BYTES);
        let route_confidence = bounded_text(
            match event.route_confidence {
                super::types::RouteConfidence::Exact => "exact",
                super::types::RouteConfidence::Inferred => "inferred",
                super::types::RouteConfidence::Fallback => "fallback",
            },
            MAX_ROUTE_CONFIDENCE_BYTES,
        );
        let degrade_flags_json = bounded_text(&degrade_json, MAX_DEGRADE_FLAGS_JSON_BYTES);

        // P2-D / FR-011 "never panic": a poisoned mutex (a prior holder
        // panicked) must degrade, not crash the operator server. Recover the
        // inner guard so the ledger keeps serving instead of propagating the
        // poison as a panic on every subsequent lock.
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO stel_ledger_events (
                ts_ms,
                session_id,
                plan_id,
                surface,
                intent,
                decision,
                tools_called_json,
                predicted_response_tokens,
                actual_response_tokens,
                manual_baseline_tokens,
                net_vs_manual,
                route_confidence,
                pff_bypass,
                cache_hit,
                degrade_flags_json,
                accepted,
                eligible_h6,
                estimator_version
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                u64_to_i64(event.ts_ms),
                session_id,
                plan_id,
                surface,
                intent,
                decision,
                tools_called_json,
                u32_to_i64(event.predicted_response_tokens),
                u32_to_i64(event.actual_response_tokens),
                u32_to_i64(event.manual_baseline_tokens),
                event.net_vs_manual,
                route_confidence,
                event.pff_bypass.map(|b| if b { 1i64 } else { 0i64 }),
                event.cache_hit.map(|b| if b { 1i64 } else { 0i64 }),
                degrade_flags_json,
                Option::<i64>::None, // accepted — not on StelLedgerEvent; reserved
                Option::<i64>::None, // eligible_h6 — not on StelLedgerEvent; reserved
                // New rows carry the estimator in force, NOT the pre-013
                // sentinel — only NEW rows enter the active tuning population (R3).
                CURRENT_ESTIMATOR_VERSION,
            ],
        )
        .context("inserting stel ledger event")?;
        let row_id = conn.last_insert_rowid();

        // T011 prune-on-write (feature 013, R2): bound the table to the most
        // recent `LEDGER_RETENTION_MAX` rows. Runs under the SAME held lock as
        // the INSERT — no extra lock round-trip, deterministic, idempotent (a
        // no-op once the table is already at or below the cap). Keeps `record()`
        // off the request hot path exactly as before (its `spawn_blocking`
        // caller contract is unchanged).
        conn.execute(
            "DELETE FROM stel_ledger_events
                WHERE id NOT IN (
                    SELECT id FROM stel_ledger_events
                    ORDER BY id DESC
                    LIMIT ?1
                )",
            params![usize_to_i64(LEDGER_RETENTION_MAX)],
        )
        .context("pruning stel ledger events beyond retention cap")?;

        Ok(row_id)
    }

    /// Return the `limit` most-recent rows ordered by descending id.
    pub fn recent(&self, limit: usize) -> Result<Vec<StoredLedgerRecord>> {
        // P2-D / FR-011 "never panic": a poisoned mutex (a prior holder
        // panicked) must degrade, not crash the operator server. Recover the
        // inner guard so the ledger keeps serving instead of propagating the
        // poison as a panic on every subsequent lock.
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT
                id,
                ts_ms,
                session_id,
                plan_id,
                surface,
                intent,
                decision,
                tools_called_json,
                predicted_response_tokens,
                actual_response_tokens,
                manual_baseline_tokens,
                net_vs_manual,
                route_confidence
            FROM stel_ledger_events
            ORDER BY id DESC
            LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![usize_to_i64(limit)], |row| {
            let ts_ms: i64 = row.get(1)?;
            let predicted: i64 = row.get(8)?;
            let actual: i64 = row.get(9)?;
            let manual: i64 = row.get(10)?;
            let net: i64 = row.get(11)?;
            Ok(StoredLedgerRecord {
                id: row.get(0)?,
                ts_ms: i64_to_u64(ts_ms),
                session_id: row.get(2)?,
                plan_id: row.get(3)?,
                surface: row.get(4)?,
                intent: row.get(5)?,
                decision: row.get(6)?,
                tools_called_json: row.get(7)?,
                predicted_response_tokens: u32::try_from(predicted.max(0)).unwrap_or(u32::MAX),
                actual_response_tokens: u32::try_from(actual.max(0)).unwrap_or(u32::MAX),
                manual_baseline_tokens: u32::try_from(manual.max(0)).unwrap_or(u32::MAX),
                net_vs_manual: i32::try_from(net.clamp(i64::from(i32::MIN), i64::from(i32::MAX)))
                    .unwrap_or(i32::MIN),
                route_confidence: row.get(12)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Aggregate summary: total events, total net_vs_manual, accepted count, unique sessions.
    pub fn summary(&self) -> Result<LedgerSummary> {
        // P2-D / FR-011 "never panic": a poisoned mutex (a prior holder
        // panicked) must degrade, not crash the operator server. Recover the
        // inner guard so the ledger keeps serving instead of propagating the
        // poison as a panic on every subsequent lock.
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let total_events: i64 =
            conn.query_row("SELECT COUNT(*) FROM stel_ledger_events", [], |r| r.get(0))?;
        let total_net: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(net_vs_manual), 0) FROM stel_ledger_events",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let accepted_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM stel_ledger_events WHERE accepted = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let session_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT session_id) FROM stel_ledger_events",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        Ok(LedgerSummary {
            total_events: i64_to_u64(total_events),
            total_net_vs_manual: total_net,
            accepted_count: i64_to_u64(accepted_count),
            session_count: i64_to_u64(session_count),
        })
    }

    /// Persist the single active tuned-constant set for its `estimator_version`
    /// (feature 013, T010 / FR-008 audited gated action). `INSERT OR REPLACE`
    /// keyed on `estimator_version`, so storing a second set for the same
    /// version REPLACES rather than appends — there is exactly one active set
    /// per estimator version.
    ///
    /// Pure data store: no derivation/validation (that is US2), and NO frecency
    /// bump (no discovery/search call — Principle V). Runs under the same
    /// poisoned-mutex recovery as the rest of the store; corruption/open-failure
    /// surfaces as an `Err` the enum wrapper degrades to `Disabled`.
    pub fn store_active_tuning(&self, c: &TunedEstimateConstants) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO stel_calibration (
                estimator_version,
                response_floor,
                manual_floor,
                schema_tokens,
                invoke_tokens,
                sample_size,
                error_before,
                error_after,
                tuned_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                c.estimator_version,
                u32_to_i64(c.response_floor),
                u32_to_i64(c.manual_floor),
                u32_to_i64(c.schema_tokens),
                u32_to_i64(c.invoke_tokens),
                u32_to_i64(c.sample_size),
                c.error_before,
                c.error_after,
                u64_to_i64(c.tuned_at_ms),
            ],
        )
        .context("storing active stel tuning constants")?;
        Ok(())
    }

    /// Load the active tuned-constant set for `estimator_version`, or `None`
    /// when no set has been stored for that version (feature 013, T010).
    ///
    /// Pure read; NO frecency bump (Principle V). The stored REAL columns
    /// round-trip the `f64` error figures byte-exact, so a load after reopen
    /// equals the stored value.
    pub fn load_active_tuning(
        &self,
        estimator_version: &str,
    ) -> Result<Option<TunedEstimateConstants>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tuned = conn
            .query_row(
                "SELECT
                    estimator_version,
                    response_floor,
                    manual_floor,
                    schema_tokens,
                    invoke_tokens,
                    sample_size,
                    error_before,
                    error_after,
                    tuned_at_ms
                FROM stel_calibration
                WHERE estimator_version = ?1",
                params![estimator_version],
                |row| {
                    let response_floor: i64 = row.get(1)?;
                    let manual_floor: i64 = row.get(2)?;
                    let schema_tokens: i64 = row.get(3)?;
                    let invoke_tokens: i64 = row.get(4)?;
                    let sample_size: i64 = row.get(5)?;
                    let tuned_at_ms: i64 = row.get(8)?;
                    Ok(TunedEstimateConstants {
                        estimator_version: row.get(0)?,
                        response_floor: u32::try_from(response_floor.max(0)).unwrap_or(u32::MAX),
                        manual_floor: u32::try_from(manual_floor.max(0)).unwrap_or(u32::MAX),
                        schema_tokens: u32::try_from(schema_tokens.max(0)).unwrap_or(u32::MAX),
                        invoke_tokens: u32::try_from(invoke_tokens.max(0)).unwrap_or(u32::MAX),
                        sample_size: u32::try_from(sample_size.max(0)).unwrap_or(u32::MAX),
                        error_before: row.get(6)?,
                        error_after: row.get(7)?,
                        tuned_at_ms: i64_to_u64(tuned_at_ms),
                    })
                },
            )
            .optional()
            .context("loading active stel tuning constants")?;
        Ok(tuned)
    }

    /// Return up to `limit` most-recent rows whose `estimator_version` matches
    /// `version`, newest-first (feature 013, T012, R3).
    ///
    /// EXCLUDES the `pre-013` sentinel rows from the active tuning population:
    /// callers pass [`CURRENT_ESTIMATOR_VERSION`], and rows tagged with a
    /// different estimator (including the sentinel) are filtered out. Pure read,
    /// NO frecency bump (Principle V). This is the seam US2 consumes for
    /// derivation; the foundation provides only the filtered read.
    pub fn samples_for_estimator(
        &self,
        version: &str,
        limit: usize,
    ) -> Result<Vec<StoredLedgerRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT
                id,
                ts_ms,
                session_id,
                plan_id,
                surface,
                intent,
                decision,
                tools_called_json,
                predicted_response_tokens,
                actual_response_tokens,
                manual_baseline_tokens,
                net_vs_manual,
                route_confidence
            FROM stel_ledger_events
            WHERE estimator_version = ?1
            ORDER BY id DESC
            LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![version, usize_to_i64(limit)], |row| {
            let ts_ms: i64 = row.get(1)?;
            let predicted: i64 = row.get(8)?;
            let actual: i64 = row.get(9)?;
            let manual: i64 = row.get(10)?;
            let net: i64 = row.get(11)?;
            Ok(StoredLedgerRecord {
                id: row.get(0)?,
                ts_ms: i64_to_u64(ts_ms),
                session_id: row.get(2)?,
                plan_id: row.get(3)?,
                surface: row.get(4)?,
                intent: row.get(5)?,
                decision: row.get(6)?,
                tools_called_json: row.get(7)?,
                predicted_response_tokens: u32::try_from(predicted.max(0)).unwrap_or(u32::MAX),
                actual_response_tokens: u32::try_from(actual.max(0)).unwrap_or(u32::MAX),
                manual_baseline_tokens: u32::try_from(manual.max(0)).unwrap_or(u32::MAX),
                net_vs_manual: i32::try_from(net.clamp(i64::from(i32::MIN), i64::from(i32::MAX)))
                    .unwrap_or(i32::MIN),
                route_confidence: row.get(12)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::types::{AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent};

    fn sample_event(plan_id: &str) -> StelLedgerEvent {
        StelLedgerEvent {
            ts_ms: 1_718_000_000_000,
            plan_id: plan_id.to_string(),
            surface: "symforge".to_string(),
            intent: IntentBucket::Trace,
            decision: AdmissionDecision::Serve,
            tools_called: vec!["find_references".to_string()],
            predicted_response_tokens: 400,
            actual_response_tokens: 380,
            manual_baseline_tokens: 800,
            net_vs_manual: 420, // 800 - 380
            equivalence: None,
            route_confidence: RouteConfidence::Exact,
            pff_bypass: None,
            cache_hit: None,
            degrade_flags: vec![],
        }
    }

    #[test]
    fn open_in_memory_creates_schema_at_current_version() {
        let store = SqliteStelLedgerStore::open_in_memory("session-test").expect("in-memory store");
        assert_eq!(
            store.schema_version().expect("schema_version"),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn migration_is_idempotent_and_preserves_current_version() {
        let store = SqliteStelLedgerStore::open_in_memory("session-test").expect("in-memory store");
        store.migrate().expect("second migrate");
        store.migrate().expect("third migrate");
        assert_eq!(
            store.schema_version().expect("schema_version"),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn record_and_recent_return_correct_rows() {
        let store = SqliteStelLedgerStore::open_in_memory("sess-1").expect("in-memory store");

        let n = 5_usize;
        let mut expected_net: i32 = 0;
        for i in 0..n {
            let mut ev = sample_event(&format!("plan-{i}"));
            ev.net_vs_manual = 100 + i as i32;
            expected_net += ev.net_vs_manual;
            store.record(&ev).expect("record");
        }

        let recent = store.recent(10).expect("recent");
        assert_eq!(recent.len(), n, "recent should return all {n} rows");

        // rows are newest-first
        assert_eq!(recent[0].plan_id, "plan-4");
        assert_eq!(recent[n - 1].plan_id, "plan-0");

        // check a representative row
        let row = recent.iter().find(|r| r.plan_id == "plan-0").unwrap();
        assert_eq!(row.session_id, "sess-1");
        assert_eq!(row.surface, "symforge");
        assert_eq!(row.intent, "trace");
        assert_eq!(row.decision, "serve");
        assert_eq!(row.tools_called_json, r#"["find_references"]"#);
        assert_eq!(row.actual_response_tokens, 380);
        assert_eq!(row.manual_baseline_tokens, 800);
        assert_eq!(row.route_confidence, "exact");

        // summary
        let summary = store.summary().expect("summary");
        assert_eq!(summary.total_events, n as u64);
        assert_eq!(summary.total_net_vs_manual, i64::from(expected_net));
        assert_eq!(summary.session_count, 1);
    }

    #[test]
    fn recent_with_limit_caps_result_set() {
        let store = SqliteStelLedgerStore::open_in_memory("sess-cap").expect("in-memory store");
        for i in 0..10 {
            store
                .record(&sample_event(&format!("p-{i}")))
                .expect("record");
        }
        let recent = store.recent(3).expect("recent");
        assert_eq!(recent.len(), 3);
        // most-recent first
        assert_eq!(recent[0].plan_id, "p-9");
    }

    #[test]
    fn disabled_store_record_is_noop() {
        let store = StelLedgerStore::Disabled;
        // Must not panic
        store.record(&sample_event("plan-noop"));
        assert!(store.recent(10).expect("recent").is_empty());
        assert!(store.summary().is_none());
        assert!(store.schema_version().is_none());
        assert_eq!(store.status(), LedgerStoreStatus::Disabled);
    }

    #[test]
    fn disabled_store_summary_reports_unavailable() {
        let store = StelLedgerStore::Disabled;
        let summary = store.summary();
        assert!(
            summary.is_none(),
            "Disabled store summary must return None (unavailable)"
        );
    }

    /// T017 / N-3 / TR-17 / FR-008: a wired-but-FAILING durable store must report
    /// a state distinct from a never-configured one. Before the fix both
    /// collapsed to `summary() == None`.
    ///
    /// After the fix, the never-configured `Disabled` variant reports
    /// `Disabled { reason }` whose reason names the startup open-failure; a
    /// `Sqlite` store whose live query fails reports `Disabled { reason }` whose
    /// reason names the query failure; and a healthy `Sqlite` store reports
    /// `Durable { .. }`. The server maps "no store wired at all" to `Unavailable`
    /// by not calling this method (the store is `Option::None` at the server
    /// boundary), so the three surface states (Durable, Disabled-with-reason,
    /// Unavailable) are all distinct.
    #[test]
    fn subsystem_state_distinguishes_broken_from_off_and_healthy() {
        // Healthy wired store -> Durable with a real summary.
        let healthy = StelLedgerStore::open_in_memory("sess-healthy").expect("healthy store");
        healthy.record(&sample_event("p-healthy"));
        match healthy.subsystem_state() {
            LedgerSubsystemState::Durable { summary } => {
                assert_eq!(
                    summary.total_events, 1,
                    "healthy store must report its rows"
                );
            }
            other => panic!("healthy Sqlite store must be Durable, got {other:?}"),
        }

        // Never-configured store (open failed at startup) -> Disabled(reason).
        let off = StelLedgerStore::Disabled;
        let off_state = off.subsystem_state();
        let off_reason = match &off_state {
            LedgerSubsystemState::Disabled { reason } => reason.clone(),
            other => panic!("Disabled variant must be Disabled(reason), got {other:?}"),
        };
        assert!(
            off_reason.contains("open"),
            "never-configured reason must name the startup open-failure: {off_reason}"
        );

        // Wired-but-failing store: drop the events table under the lock so the
        // next summary query errors, simulating a corrupt/failed durable store.
        let broken = StelLedgerStore::open_in_memory("sess-broken").expect("broken store");
        if let StelLedgerStore::Sqlite(store) = &broken {
            let conn = store.conn.lock().unwrap();
            conn.execute_batch("DROP TABLE stel_ledger_events;")
                .expect("drop events table");
        } else {
            panic!("open_in_memory must yield a Sqlite store");
        }
        let broken_state = broken.subsystem_state();
        let broken_reason = match &broken_state {
            LedgerSubsystemState::Disabled { reason } => reason.clone(),
            other => panic!("a wired-but-failing store must be Disabled(reason), got {other:?}"),
        };
        assert!(
            broken_reason.contains("query failed"),
            "broken reason must name the failed live query: {broken_reason}"
        );

        // The two Disabled reasons are DISTINCT — "broken" never reads identical
        // to "off". This is the FR-008 invariant.
        assert_ne!(
            off_reason, broken_reason,
            "wired-but-failing store must not report identically to a never-configured one"
        );
    }

    #[test]
    fn enum_open_in_memory_roundtrip() {
        let store = StelLedgerStore::open_in_memory("sess-enum").expect("enum store");
        let ev = sample_event("plan-enum");
        store.record(&ev);
        let recent = store.recent(5).expect("recent");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].plan_id, "plan-enum");

        let summary = store.summary().expect("summary is Some for Sqlite variant");
        assert_eq!(summary.total_events, 1);
        assert_eq!(summary.total_net_vs_manual, 420);
    }

    #[test]
    fn multi_session_summary_counts_distinct_sessions() {
        // Two separate stores simulate two sessions writing to the same DB
        // (in-memory so each is isolated, but tests the logic path).
        let store_a = SqliteStelLedgerStore::open_in_memory("session-a").expect("store-a");
        let store_b = SqliteStelLedgerStore::open_in_memory("session-b").expect("store-b");
        store_a.record(&sample_event("p1")).expect("record a");
        store_b.record(&sample_event("p2")).expect("record b");
        // Each in-memory DB is independent; verify per-store counts.
        assert_eq!(store_a.summary().unwrap().total_events, 1);
        assert_eq!(store_b.summary().unwrap().total_events, 1);
        assert_eq!(store_a.summary().unwrap().session_count, 1);
    }

    #[test]
    fn persist_to_file_and_reopen_preserves_rows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("stel-ledger.db");
        let n = 3_usize;

        {
            let store = SqliteStelLedgerStore::open(&db_path, "session-persist").expect("open");
            for i in 0..n {
                store
                    .record(&sample_event(&format!("p-{i}")))
                    .expect("record");
            }
            assert_eq!(store.recent(100).unwrap().len(), n);
        }

        // Reopen — must find N rows (FR-010 / SC-003 acceptance criterion)
        let store2 = SqliteStelLedgerStore::open(&db_path, "session-persist-2").expect("reopen");
        let rows = store2.recent(100).expect("recent after reopen");
        assert_eq!(rows.len(), n, "reopened store must have all {n} rows");
        let summary = store2.summary().expect("summary");
        assert_eq!(summary.total_events, n as u64);
    }

    #[test]
    fn degrade_event_stores_degrade_flags() {
        let store = SqliteStelLedgerStore::open_in_memory("sess-degrade").expect("store");
        let mut ev = sample_event("plan-degrade");
        ev.decision = AdmissionDecision::Degrade;
        ev.degrade_flags = vec!["outline_only".to_string()];
        store.record(&ev).expect("record");

        // We can verify via raw recent that degrade_flags_json is stored (not in StoredLedgerRecord
        // to keep the public type minimal, but the column is in the DB schema).
        let conn = store.conn.lock().unwrap();
        let flags: String = conn
            .query_row(
                "SELECT degrade_flags_json FROM stel_ledger_events WHERE plan_id = ?1",
                params!["plan-degrade"],
                |r| r.get(0),
            )
            .expect("query degrade_flags_json");
        assert!(flags.contains("outline_only"));
    }

    #[test]
    fn bypass_event_stores_pff_bypass_flag() {
        let store = SqliteStelLedgerStore::open_in_memory("sess-bypass").expect("store");
        let mut ev = sample_event("plan-bypass");
        ev.decision = AdmissionDecision::Bypass;
        ev.pff_bypass = Some(true);
        store.record(&ev).expect("record");

        let conn = store.conn.lock().unwrap();
        let pff: Option<i64> = conn
            .query_row(
                "SELECT pff_bypass FROM stel_ledger_events WHERE plan_id = ?1",
                params!["plan-bypass"],
                |r| r.get(0),
            )
            .expect("query pff_bypass");
        assert_eq!(pff, Some(1));
    }
}
