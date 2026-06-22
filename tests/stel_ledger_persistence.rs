//! Durable STEL ledger — feature 013 Foundational contract (T004/T005/T006).
//!
//! Server-gated: the `ledger_store` module is `#[cfg(feature = "server")]` for
//! this phase (the embed un-gate is US1 T018, not now), so the whole file is
//! gated to keep `--no-default-features --features embed --all-targets`
//! compiling.
#![cfg(feature = "server")]

//! Asserts the v2-migration + retention + tuned-constant + crash-durability
//! contract the Foundational storage primitives must satisfy:
//!
//! - (a) a fresh store opens at `schema_version == 2` with an `estimator_version`
//!   column present (T004);
//! - (b) a row written before the column-add reads back the `pre-013` backfill
//!   sentinel (T004);
//! - (c) inserting `LEDGER_RETENTION_MAX + N` events leaves exactly
//!   `LEDGER_RETENTION_MAX` rows, newest retained / oldest pruned (T004);
//! - (d) `store_active_tuning` then `load_active_tuning` round-trips an identical
//!   `TunedEstimateConstants` after reopen (T004/T005 byte-stable);
//! - (e) a second `store_active_tuning` for the same `estimator_version`
//!   REPLACES (not appends) the active set (T004);
//! - (f) `migrate()` is idempotent — twice leaves v2, no dup column, no
//!   re-backfill of non-sentinel rows (T005);
//! - (g) a store whose open/migrate fails degrades to `Disabled`, distinct from
//!   `Unavailable` (T005, constitution IV corruption-quarantine);
//! - (h) crash-durability: record an event, drop the handle without a clean
//!   shutdown, reopen, assert the event survived (T006, WAL append durable).

use rusqlite::Connection;

use symforge::stel::ledger_store::{
    CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX, LedgerStoreStatus, PRE_013_ESTIMATOR_SENTINEL,
    SYMFORGE_STEL_LEDGER_DB_PATH, SqliteStelLedgerStore, StelLedgerStore, TunedEstimateConstants,
};
use symforge::stel::types::{AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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
        net_vs_manual: 420,
        equivalence: None,
        route_confidence: RouteConfidence::Exact,
        pff_bypass: None,
        cache_hit: None,
        degrade_flags: vec![],
    }
}

fn sample_tuning() -> TunedEstimateConstants {
    TunedEstimateConstants {
        response_floor: 512,
        manual_floor: 1024,
        schema_tokens: 50,
        invoke_tokens: 88,
        estimator_version: CURRENT_ESTIMATOR_VERSION.to_string(),
        sample_size: 137,
        error_before: 0.4231,
        error_after: 0.1987,
        tuned_at_ms: 1_718_500_000_000,
    }
}

/// Pre-013 v1 schema (no `estimator_version` column). Mirrors the historical
/// shape so the migration's column-add + backfill is exercised against a real
/// "old" database.
const V1_SCHEMA_NO_ESTIMATOR_COL: &str = r#"
CREATE TABLE stel_ledger_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE stel_ledger_events (
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
"#;

/// Read the `estimator_version` of every row, oldest-first, via a raw read-only
/// connection (the public `StoredLedgerRecord` does not carry the column).
fn estimator_versions(db_path: &std::path::Path) -> Vec<Option<String>> {
    let conn = Connection::open(db_path).expect("open raw conn");
    let mut stmt = conn
        .prepare("SELECT estimator_version FROM stel_ledger_events ORDER BY id ASC")
        .expect("prepare estimator_version read");
    let rows = stmt
        .query_map([], |row| row.get::<_, Option<String>>(0))
        .expect("query estimator_version");
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect estimator_version")
}

// ---------------------------------------------------------------------------
// T004(a) — fresh store opens at v2 with the estimator_version column
// ---------------------------------------------------------------------------

#[test]
fn fresh_store_opens_at_schema_v2_with_estimator_version_column() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("stel-ledger.db");
    let store = SqliteStelLedgerStore::open(&db_path, "sess-v2").expect("open");

    assert_eq!(
        store.schema_version().expect("schema_version"),
        2,
        "a fresh store must open at schema_version == 2"
    );
    match store.status() {
        LedgerStoreStatus::Enabled { schema_version, .. } => {
            assert_eq!(schema_version, 2, "status must report v2");
        }
        LedgerStoreStatus::Disabled => panic!("fresh store must be Enabled"),
    }

    // The column must physically exist on the events table.
    let conn = Connection::open(&db_path).expect("raw conn");
    let mut stmt = conn
        .prepare("PRAGMA table_info(stel_ledger_events)")
        .expect("pragma");
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query cols")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect cols");
    assert!(
        cols.iter().any(|c| c == "estimator_version"),
        "estimator_version column must be present after migrate; got {cols:?}"
    );
}

#[test]
fn new_rows_are_tagged_with_current_estimator_version_not_sentinel() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("stel-ledger.db");
    let store = SqliteStelLedgerStore::open(&db_path, "sess-tag").expect("open");
    store.record(&sample_event("p-new")).expect("record");

    let versions = estimator_versions(&db_path);
    assert_eq!(versions.len(), 1);
    assert_eq!(
        versions[0].as_deref(),
        Some(CURRENT_ESTIMATOR_VERSION),
        "a new row must carry the current estimator version, never the sentinel"
    );

    // And it must be visible to the filtered sample read (the active population).
    let samples = store
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 100)
        .expect("samples");
    assert_eq!(
        samples.len(),
        1,
        "new row is in the active tuning population"
    );
    // The sentinel population is empty.
    let stale = store
        .samples_for_estimator(PRE_013_ESTIMATOR_SENTINEL, 100)
        .expect("stale samples");
    assert!(stale.is_empty(), "no sentinel rows for a fresh store");
}

// ---------------------------------------------------------------------------
// T004(b) — pre-column row backfills to the pre-013 sentinel and is excluded
// ---------------------------------------------------------------------------

#[test]
fn pre_column_row_backfills_to_pre_013_sentinel_and_is_excluded() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("stel-ledger.db");

    // Build a v1-shaped DB (no estimator_version column) and insert one row.
    {
        let conn = Connection::open(&db_path).expect("create v1 db");
        conn.execute_batch(V1_SCHEMA_NO_ESTIMATOR_COL)
            .expect("apply v1 schema");
        conn.execute(
            "INSERT INTO stel_ledger_meta (key, value) VALUES ('schema_version', '1')",
            [],
        )
        .expect("write v1 schema version");
        conn.execute(
            "INSERT INTO stel_ledger_events (
                ts_ms, session_id, plan_id, surface, intent, decision,
                tools_called_json, predicted_response_tokens, actual_response_tokens,
                manual_baseline_tokens, net_vs_manual, route_confidence, degrade_flags_json
            ) VALUES (1, 'old-sess', 'p-old', 'symforge', 'trace', 'serve', '[]', 400, 380, 800, 420, 'exact', '[]')",
            [],
        )
        .expect("insert pre-column row");
    }

    // Open via the real store: migrate() must add the column + backfill.
    let store = SqliteStelLedgerStore::open(&db_path, "sess-upgrade").expect("open upgrades");
    assert_eq!(
        store.schema_version().expect("schema_version"),
        2,
        "opening a v1 db must upgrade it to v2"
    );

    let versions = estimator_versions(&db_path);
    assert_eq!(versions.len(), 1);
    assert_eq!(
        versions[0].as_deref(),
        Some(PRE_013_ESTIMATOR_SENTINEL),
        "a pre-column row must be backfilled with the pre-013 sentinel"
    );

    // The sentinel row is EXCLUDED from the current-version active population (R3).
    let active = store
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 100)
        .expect("active samples");
    assert!(
        active.is_empty(),
        "pre-013 sentinel rows must be excluded from the active tuning population"
    );
    // It IS retained (auditable) under the sentinel filter.
    let retained = store
        .samples_for_estimator(PRE_013_ESTIMATOR_SENTINEL, 100)
        .expect("retained samples");
    assert_eq!(retained.len(), 1, "sentinel row is retained for audit");
}

// ---------------------------------------------------------------------------
// T004(c) — bounded prune-on-write keeps exactly LEDGER_RETENTION_MAX rows
// ---------------------------------------------------------------------------

#[test]
fn retention_prunes_to_cap_keeping_newest() {
    // Use a small extra count so the test is fast yet crosses the cap boundary.
    let extra = 7_usize;
    let total = LEDGER_RETENTION_MAX + extra;

    let store = SqliteStelLedgerStore::open_in_memory("sess-retention").expect("open");
    for i in 0..total {
        // plan_id encodes insertion order so we can assert which rows survived.
        store
            .record(&sample_event(&format!("p-{i:06}")))
            .expect("record");
    }

    // Exactly the cap remains.
    let summary = store.summary().expect("summary");
    assert_eq!(
        summary.total_events, LEDGER_RETENTION_MAX as u64,
        "after inserting cap + {extra}, exactly {LEDGER_RETENTION_MAX} rows remain"
    );

    // Newest retained / oldest pruned: the most-recent row is the last inserted,
    // and the oldest surviving row is index `extra` (0..extra were pruned).
    let recent = store.recent(LEDGER_RETENTION_MAX + extra).expect("recent");
    assert_eq!(recent.len(), LEDGER_RETENTION_MAX);
    assert_eq!(
        recent[0].plan_id,
        format!("p-{:06}", total - 1),
        "newest row must be retained"
    );
    assert_eq!(
        recent[recent.len() - 1].plan_id,
        format!("p-{extra:06}"),
        "oldest surviving row must be the (extra)-th insert; everything before it pruned"
    );
}

// ---------------------------------------------------------------------------
// T004(d)/(e) — tuned-constant round-trip + REPLACE semantics
// ---------------------------------------------------------------------------

#[test]
fn store_then_load_active_tuning_round_trips_after_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("stel-ledger.db");
    let tuning = sample_tuning();

    {
        let store = SqliteStelLedgerStore::open(&db_path, "sess-tune").expect("open");
        store.store_active_tuning(&tuning).expect("store tuning");
    }

    // Reopen and load — must be byte-stable across the reopen (constitution IV).
    let store2 = SqliteStelLedgerStore::open(&db_path, "sess-tune-2").expect("reopen");
    let loaded = store2
        .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
        .expect("load tuning")
        .expect("tuning present after reopen");
    assert_eq!(
        loaded, tuning,
        "tuned constants must round-trip identically"
    );
    // Explicit byte-stability check on the f64 error figures.
    assert_eq!(loaded.error_before, tuning.error_before);
    assert_eq!(loaded.error_after, tuning.error_after);
}

#[test]
fn load_active_tuning_absent_returns_none() {
    let store = SqliteStelLedgerStore::open_in_memory("sess-none").expect("open");
    let loaded = store
        .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
        .expect("load");
    assert!(loaded.is_none(), "no tuning stored yet => None");
}

#[test]
fn second_store_active_tuning_replaces_not_appends() {
    let store = SqliteStelLedgerStore::open_in_memory("sess-replace").expect("open");

    let mut first = sample_tuning();
    first.response_floor = 500;
    store.store_active_tuning(&first).expect("store first");

    let mut second = sample_tuning();
    second.response_floor = 999;
    second.sample_size = 200;
    store.store_active_tuning(&second).expect("store second");

    // Only ONE active set exists for the version, and it is the second.
    let loaded = store
        .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
        .expect("load")
        .expect("present");
    assert_eq!(
        loaded.response_floor, 999,
        "second store must win (REPLACE)"
    );
    assert_eq!(loaded.sample_size, 200);

    // Idempotent: a repeated load still yields exactly the second set (REPLACE,
    // not append — there is only one active row per estimator_version).
    let again = store
        .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
        .expect("load again")
        .expect("present");
    assert_eq!(
        again.response_floor, 999,
        "idempotent: still the second set"
    );
}

// ---------------------------------------------------------------------------
// T005(f) — migrate() is idempotent: twice -> v2, no dup column, no re-backfill
// ---------------------------------------------------------------------------

#[test]
fn migrate_is_idempotent_no_dup_column_no_rebackfill() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("stel-ledger.db");
    let store = SqliteStelLedgerStore::open(&db_path, "sess-idem").expect("open");

    // Record a NEW row (tagged current version, NOT the sentinel).
    store.record(&sample_event("p-current")).expect("record");

    // Call migrate repeatedly — open() already ran it once.
    store.migrate().expect("second migrate");
    store.migrate().expect("third migrate");

    assert_eq!(
        store.schema_version().expect("schema_version"),
        2,
        "repeated migrate stays at v2"
    );

    // The column is not duplicated: PRAGMA shows exactly one estimator_version.
    let conn = Connection::open(&db_path).expect("raw conn");
    let mut stmt = conn
        .prepare("PRAGMA table_info(stel_ledger_events)")
        .expect("pragma");
    let est_cols = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query cols")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect")
        .into_iter()
        .filter(|c| c == "estimator_version")
        .count();
    assert_eq!(est_cols, 1, "estimator_version must not be duplicated");

    // The non-sentinel row must NOT have been re-backfilled to the sentinel.
    let versions = estimator_versions(&db_path);
    assert_eq!(versions.len(), 1);
    assert_eq!(
        versions[0].as_deref(),
        Some(CURRENT_ESTIMATOR_VERSION),
        "a current-version row must never be clobbered to the sentinel by re-migrate"
    );
}

// ---------------------------------------------------------------------------
// T005(g) — open/migrate failure degrades to Disabled (not Unavailable)
// ---------------------------------------------------------------------------

#[test]
fn corrupt_db_degrades_to_disabled_not_a_panic() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    // Write a non-SQLite file at the exact db path the store will open.
    let db_path = dir.join(SYMFORGE_STEL_LEDGER_DB_PATH);
    std::fs::create_dir_all(db_path.parent().unwrap()).expect("mkdir .symforge");
    std::fs::write(
        &db_path,
        b"this is not a sqlite database, it is garbage bytes",
    )
    .expect("write garbage");

    // The dir-entry open must NOT panic and must yield Disabled (quarantined),
    // distinct from "no store wired" (Unavailable = Option::None at the boundary).
    let store = StelLedgerStore::open(dir, "sess-corrupt");
    assert!(
        matches!(store, StelLedgerStore::Disabled),
        "a corrupt/non-DB file must degrade to Disabled, not serve"
    );
    // A Disabled store serves nothing and never serves a bad tuning.
    assert!(store.schema_version().is_none());
    assert!(store.summary().is_none());
    assert!(
        store
            .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
            .expect("load on disabled is Ok(None)")
            .is_none(),
        "a Disabled store must never serve a tuning"
    );
    assert!(
        store
            .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 10)
            .expect("samples on disabled is Ok(empty)")
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// T006(h) — crash durability: WAL append survives an abrupt handle drop
// ---------------------------------------------------------------------------

#[test]
fn recorded_event_survives_abrupt_drop_without_clean_shutdown() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("stel-ledger.db");

    // Record an event, then drop the handle WITHOUT any clean checkpoint/shutdown
    // path — simulating a crash. The WAL append must already be durable.
    {
        let store = SqliteStelLedgerStore::open(&db_path, "sess-crash").expect("open");
        store.record(&sample_event("p-crash")).expect("record");
        // No explicit checkpoint, no graceful close — just drop at scope end.
        drop(store);
    }

    // Reopen the SAME db; the event must be present (constitution IV: shutdown is
    // NOT the safe boundary — the append is durable mid-write).
    let reopened = SqliteStelLedgerStore::open(&db_path, "sess-crash-2").expect("reopen");
    let summary = reopened.summary().expect("summary");
    assert_eq!(
        summary.total_events, 1,
        "the recorded event must survive an abrupt drop (WAL append durable)"
    );
    let recent = reopened.recent(10).expect("recent");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].plan_id, "p-crash");

    // And it must still be tagged with the current estimator version.
    let samples = reopened
        .samples_for_estimator(CURRENT_ESTIMATOR_VERSION, 10)
        .expect("samples");
    assert_eq!(samples.len(), 1, "survivor stays in the active population");
}
