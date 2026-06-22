//! Durable STEL ledger — feature 013 Foundational contract (T004/T005/T006).
//!
//! Server-gated: the `ledger_store` module is `#[cfg(feature = "server")]`
//! (embed durability is DEFERRED — see the MINOR 3 note below), so the whole
//! file is gated to keep `--no-default-features --features embed --all-targets`
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

/// Env-var guard naming the db directory the crash CHILD must open. When set,
/// the test below runs in "child" mode (open + record + `abort()` mid-write);
/// when unset, it is the PARENT (spawn the child, assert it crashed, reopen the
/// SAME dir, prove the WAL append survived).
const CRASH_CHILD_DIR_ENV: &str = "TC_013_CRASH_CHILD_DIR";

/// Crash-durability (constitution IV: "shutdown is NOT a safe persistence
/// boundary"). A clean `drop(store)` triggers SQLite's WAL checkpoint, so a
/// drop-then-reopen test only proves CHECKPOINT durability — exactly the honesty
/// defect this rewrite fixes. To prove genuine CRASH durability we must record
/// an event and then terminate the process WITHOUT running any `Drop`/checkpoint,
/// leaving the `-wal` un-checkpointed, and recover it on the next open.
///
/// Mechanism (cross-platform): the parent re-execs THIS test binary as a child
/// with `--exact <this test> --nocapture` and `TC_013_CRASH_CHILD_DIR=<dir>`.
/// The child opens the store under that dir, `record()`s, and calls
/// `std::process::abort()` while the store handle is still live — so no `Drop`
/// runs, no WAL checkpoint happens, and the process dies via SIGABRT (Unix) /
/// a fatal abort (Windows). The parent asserts the child did NOT exit cleanly
/// (no `success()`), then opens the SAME db dir and asserts the recorded event
/// survived — recovered from the un-checkpointed WAL, not from a clean close.
#[test]
fn recorded_event_survives_abrupt_drop_without_clean_shutdown() {
    // ---- CHILD MODE: open, record, abort WITHOUT a clean drop/checkpoint. ----
    if let Ok(dir) = std::env::var(CRASH_CHILD_DIR_ENV) {
        let db_path = std::path::Path::new(&dir).join("stel-ledger.db");
        let store =
            SqliteStelLedgerStore::open(&db_path, "sess-crash-child").expect("child: open store");
        store
            .record(&sample_event("p-crash"))
            .expect("child: record event");
        // Force the WAL append to disk WITHOUT a checkpoint: keep the connection
        // open (no Drop, no `wal_checkpoint`) and crash hard. `abort()` does not
        // unwind, does not run destructors, and does not flush via a clean close
        // — it is the closest portable analogue of a power loss / SIGKILL. The
        // `-wal` file is left with the un-checkpointed append for the parent to
        // recover. `std::mem::forget` makes the no-Drop intent explicit even
        // though `abort()` already skips destructors.
        std::mem::forget(store);
        std::process::abort();
    }

    // ---- PARENT MODE: spawn the child, assert it crashed, recover the event. ----
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();

    let exe = std::env::current_exe().expect("current_exe for re-exec");
    let status = std::process::Command::new(exe)
        .args([
            "--exact",
            "recorded_event_survives_abrupt_drop_without_clean_shutdown",
            "--nocapture",
        ])
        .env(CRASH_CHILD_DIR_ENV, dir)
        // The harness sets this so a single `--exact` test still runs.
        .env("RUST_TEST_THREADS", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("spawn crash child");

    // The child must NOT have exited cleanly — `abort()` yields a non-success
    // status (SIGABRT on Unix, a fatal exit code on Windows). A clean exit would
    // mean the child drained through a normal shutdown path, invalidating the
    // crash-durability claim.
    assert!(
        !status.success(),
        "crash child must terminate abnormally (abort), not exit cleanly; got {status:?}"
    );

    // Reopen the SAME db dir. The event must be present — recovered from the
    // un-checkpointed WAL left by the aborted child (constitution IV: the append
    // is durable mid-write, BEFORE any clean shutdown/checkpoint).
    let db_path = dir.join("stel-ledger.db");
    assert!(
        db_path.exists(),
        "child must have created the db file before aborting"
    );
    let reopened = SqliteStelLedgerStore::open(&db_path, "sess-crash-recover").expect("reopen");
    let summary = reopened.summary().expect("summary");
    assert_eq!(
        summary.total_events, 1,
        "the recorded event must survive a genuine process abort (WAL append durable, \
         recovered without a clean checkpoint)"
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

// ===========================================================================
// User Story 1 (feature 013): durable ledger reaches stdio/embed
// ===========================================================================
//
// T014 durable-surface dependency-shape guard, T015 cross-session accumulation
// + degrade-to-Disabled, T016 transport parity, T017 frecency non-bump.
//
// IMPORTANT honesty note on T014 / embed reachability (013 US1 review fix,
// MINOR 3): embed durability is DEFERRED, so the durable `ledger_store` inner
// module and the protocol field/builder/write-through stay server-gated. An
// `any(feature="server", feature="embed")` cfg there would be DEAD under embed
// and falsely signal embed-capability: the PARENT modules are server-gated at
// the crate root (`src/lib.rs`: `#[cfg(feature="server")] pub mod stel;` and
// `pub mod protocol;`), and `stel::{controller,executor,planner,edit_apply}`
// hard-import `crate::protocol::{format,session,smart_query,result_status,
// tools}`. So genuine embed reachability of the durable store is BLOCKED at
// `lib.rs` and needs a structural split (a protocol-free ledger seam, out of
// focused-US1 scope; spec FR-001 note). `--no-default-features --features embed
// --lib` stays green because the server-gated module is simply not compiled.
// This test therefore pins the durable SURFACE SHAPE the SERVER stdio path (T020)
// relies on — feature-independent enum, no server-only type leak — not an
// embed-build reachability it cannot honestly claim today.

// ---------------------------------------------------------------------------
// T014 — durable-surface dependency-shape guard: the dir-entry `open` returns a
// feature-independent `StelLedgerStore` enum (no server-only type), and a
// `Disabled` variant the stdio degrade path holds. This is the shape the
// stdio wiring (T020) attaches and the `lib.rs` un-gate would later need.
// ---------------------------------------------------------------------------

#[test]
fn durable_store_surface_is_feature_independent_no_server_only_types() {
    // The dir-entry `open` is the seam the stdio wiring calls (T020). It returns
    // the feature-independent `StelLedgerStore` enum — NOT a server-only type —
    // so the inner un-gate to `any(server, embed)` (T018) carries no server
    // dependency. A `Disabled` value is constructible directly, which is the
    // in-memory degrade state the stdio path holds when open fails (FR-003).
    let disabled = StelLedgerStore::Disabled;
    assert!(disabled.schema_version().is_none());
    assert!(disabled.summary().is_none());

    // The subsystem-state mapping the status surface consumes is reachable from
    // the same enum, with no server-only import — this is what
    // `durable_ledger_summary_for_status` reads on the server stdio path.
    match disabled.subsystem_state() {
        symforge::stel::ledger_store::LedgerSubsystemState::Disabled { reason } => {
            assert!(!reason.is_empty(), "Disabled must carry a non-empty reason");
        }
        other => panic!("a Disabled store must map to Disabled{{reason}}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// T015 — cross-session accumulation across >= 3 restarts is cumulative /
// monotonic (non-reset); a forced open failure yields a distinguishable
// `Disabled`, never a panic or silent zero. (FR-001/FR-003/SC-003)
// ---------------------------------------------------------------------------

#[test]
fn cross_session_accumulation_is_cumulative_across_restarts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Production calling convention (013 US1 review fix): the dir-entry `open`
    // takes the project ROOT and joins the `.symforge/`-prefixed db const itself,
    // exactly as serve.rs / main.rs do. Passing the root (not `.symforge`) is
    // what genuinely exercises the real call path.
    let root = tmp.path();

    // Simulate >= 3 process restarts: each "session" opens the SAME root via the
    // dir-entry `open` (exactly what the stdio bootstrap calls), records a
    // distinct number of events, then drops the handle (process exit).
    let per_session = [2_u64, 3, 4, 5];
    let mut expected_total = 0_u64;
    let mut last_seen_total = 0_u64;

    for (session_idx, count) in per_session.iter().enumerate() {
        let store = StelLedgerStore::open(root, format!("stdio-sess-{session_idx}"));
        // On reopen, the prior sessions' events must already be counted — the
        // store is restored, NOT reset to zero.
        let on_open = store
            .summary()
            .expect("a healthy reopened store reports a summary")
            .total_events;
        assert_eq!(
            on_open, expected_total,
            "session {session_idx} must observe the cumulative prior total on open (non-reset)"
        );

        for i in 0..*count {
            store.record(&sample_event(&format!("s{session_idx}-e{i}")));
        }
        expected_total += *count;

        let after = store
            .summary()
            .expect("summary after recording")
            .total_events;
        assert_eq!(
            after, expected_total,
            "in-session total must include new events"
        );
        // Monotonic: the total never decreased across the restart boundary.
        assert!(
            after >= last_seen_total,
            "total_events must be monotonic across restarts: {after} < {last_seen_total}"
        );
        last_seen_total = after;
        // Handle dropped here = process exit between sessions.
    }

    // Final independent reopen confirms the durable cumulative count survived
    // every restart (SC-003: >= 3 restarts, cumulative, non-reset).
    let final_store = StelLedgerStore::open(root, "stdio-sess-final");
    assert_eq!(
        final_store.summary().expect("final summary").total_events,
        expected_total,
        "the durable total must survive all restarts cumulatively"
    );
}

#[test]
fn forced_open_failure_degrades_to_disabled_distinguishably_never_panics() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();

    // Make the db path unopenable: pre-create the `.symforge` dir and write a
    // DIRECTORY where the db FILE must live, so `Connection::open` fails. This is
    // the "store cannot open" path (read-only FS / unwritable path analogue).
    let db_path = dir.join(SYMFORGE_STEL_LEDGER_DB_PATH);
    std::fs::create_dir_all(&db_path).expect("create a dir at the db file path");

    // The dir-entry open must NOT panic and must yield a distinguishable
    // `Disabled` (the stdio/embed in-memory degrade, FR-003) — never a silent
    // zero-count that masquerades as durable accumulation.
    let store = StelLedgerStore::open(dir, "stdio-degrade");
    assert!(
        matches!(store, StelLedgerStore::Disabled),
        "an unopenable db must degrade to Disabled, not serve"
    );

    // Distinguishable: subsystem_state names the failure; summary is None (no
    // durable accumulation is claimed).
    match store.subsystem_state() {
        symforge::stel::ledger_store::LedgerSubsystemState::Disabled { reason } => {
            assert!(!reason.is_empty(), "degraded store must report a reason");
        }
        other => panic!("a degraded store must be Disabled{{reason}}, got {other:?}"),
    }
    assert!(
        store.summary().is_none(),
        "a Disabled store must not present a (zero) durable summary as real"
    );
    // Recording into a Disabled store is a silent no-op, never a panic.
    store.record(&sample_event("into-the-void"));
}

// ---------------------------------------------------------------------------
// T016 — transport parity: one db, session-spanning. The SAME db opened under a
// second session_id sees the cumulative cross-session count. (Principle VII)
// ---------------------------------------------------------------------------

#[test]
fn one_db_spans_sessions_transport_parity() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();

    // Session A (e.g. the serve surface) records two events, then exits.
    {
        let store_a = StelLedgerStore::open(dir, "serve-1234");
        store_a.record(&sample_event("a-0"));
        store_a.record(&sample_event("a-1"));
        assert_eq!(store_a.summary().unwrap().total_events, 2);
    }

    // Session B (e.g. the stdio surface) under a DIFFERENT session_id opens the
    // SAME dir and must see A's events — there is ONE durable db, one
    // session-spanning count. stdio and serve back onto the same store.
    let store_b = StelLedgerStore::open(dir, "stdio-5678");
    assert_eq!(
        store_b.summary().unwrap().total_events,
        2,
        "a second session_id over the same db sees the cumulative prior count"
    );
    store_b.record(&sample_event("b-0"));

    // The count is the union across both sessions, and the session_count reflects
    // both distinct writers — proving the rows are co-located in one store.
    let summary = store_b.summary().unwrap();
    assert_eq!(summary.total_events, 3, "stdio + serve rows live in one db");
    assert_eq!(
        summary.session_count, 2,
        "both session_ids contributed to the single session-spanning store"
    );
}

// ---------------------------------------------------------------------------
// T017 — recording a durable ledger event does NOT bump discovery/search
// frecency. (Principle V)
// ---------------------------------------------------------------------------

#[test]
fn recording_durable_event_does_not_bump_frecency() {
    use symforge::live_index::frecency::FrecencyStore;

    // A live frecency store (the discovery/search ranking subsystem). Snapshot
    // its bump list BEFORE any ledger activity.
    let frecency = FrecencyStore::open_in_memory().expect("open frecency store");
    let before = frecency.last_10_bumps().expect("frecency snapshot before");
    assert!(before.is_empty(), "fresh frecency store has no bumps");

    // Record several durable ledger events. The STEL ledger store has NO
    // dependency on `live_index`/frecency (Principle V), so this must not touch
    // discovery ranking at all.
    let store = StelLedgerStore::open_in_memory("sess-frecency").expect("ledger store");
    for i in 0..5 {
        store.record(&sample_event(&format!("ledger-{i}")));
    }
    assert_eq!(
        store.summary().unwrap().total_events,
        5,
        "ledger events were recorded"
    );

    // Frecency is unchanged — recording ledger events bumped nothing.
    let after = frecency.last_10_bumps().expect("frecency snapshot after");
    assert_eq!(
        before, after,
        "recording durable ledger events must NOT bump discovery/search frecency"
    );

    // Control: a real frecency bump DOES change the snapshot, proving the
    // assertion above can detect a bump (the test is not vacuously true).
    frecency
        .bump(&[std::path::PathBuf::from("src/lib.rs")], 1_718_000_000)
        .expect("control bump");
    let bumped = frecency
        .last_10_bumps()
        .expect("frecency snapshot after bump");
    assert_ne!(
        after, bumped,
        "a genuine frecency bump must change the snapshot (control)"
    );
}

// ---------------------------------------------------------------------------
// T022 — HTTP-sidecar coexistence / single-process two-opener WAL concurrency.
// The local stdio path spawns an HTTP sidecar on the SAME project root
// (main.rs:408-410) while it holds the durable `StelLedgerStore`. The sidecar
// does NOT itself open `stel-ledger.db` (it shares only the in-memory
// `LiveIndex` + `TokenStats`), so the R4 risk reduces to: can ONE process hold
// two openers of the same db without contention or lost writes? WAL +
// busy_timeout must make that safe. (R4; FR-001; Principle IV)
// ---------------------------------------------------------------------------

#[test]
fn single_process_two_openers_coexist_without_contention_or_lost_writes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();

    // Opener A is the stdio durable store; opener B is a second handle on the
    // SAME db file within the SAME process (the worst case the sidecar topology
    // could ever approximate). Both are live simultaneously.
    let store_a = StelLedgerStore::open(dir, "stdio-A");
    let store_b = StelLedgerStore::open(dir, "second-opener-B");
    assert!(
        matches!(store_a, StelLedgerStore::Sqlite(_)),
        "opener A must open cleanly (WAL)"
    );
    assert!(
        matches!(store_b, StelLedgerStore::Sqlite(_)),
        "a second concurrent opener on the same db must also open (WAL), not contend to Disabled"
    );

    // Interleave writes through BOTH handles — WAL + busy_timeout serialize the
    // writers; no write is lost and no open degrades to Disabled.
    for i in 0..10 {
        store_a.record(&sample_event(&format!("a-{i}")));
        store_b.record(&sample_event(&format!("b-{i}")));
    }

    // Both handles observe the FULL union (20 rows) — the WAL makes each writer's
    // commits visible to the other, so neither opener has a stale/partial view.
    let total_a = store_a.summary().expect("A summary").total_events;
    let total_b = store_b.summary().expect("B summary").total_events;
    assert_eq!(total_a, 20, "opener A must see all 20 interleaved writes");
    assert_eq!(total_b, 20, "opener B must see all 20 interleaved writes");

    // A fresh reopen after both handles drop confirms all 20 durably landed
    // (no lost write across the two-opener interleave).
    drop(store_a);
    drop(store_b);
    let reopened = StelLedgerStore::open(dir, "verify");
    assert_eq!(
        reopened.summary().expect("reopen summary").total_events,
        20,
        "all interleaved two-opener writes must be durable"
    );
}

// ---------------------------------------------------------------------------
// 013 US1 review fix (MAJOR 1) — the dir-entry `open` takes the project ROOT
// and joins the `.symforge/`-prefixed db const ITSELF, landing the db at exactly
// `<root>/.symforge/stel-ledger.db` (the convention every other store follows).
// The production call sites (serve.rs / main.rs stdio + daemon-proxy) pass the
// project ROOT, so this is the on-disk path they actually produce. The earlier
// bug passed `ensure_symforge_dir(root)` (= `root/.symforge`), DOUBLING the
// prefix to `root/.symforge/.symforge/stel-ledger.db`. This test pins the path
// under the real production convention — the test that would have caught it.
// ---------------------------------------------------------------------------

#[test]
fn open_under_project_root_lands_db_at_single_symforge_prefix_not_doubled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    // Open via the PRODUCTION calling convention: pass the project ROOT (exactly
    // what serve.rs / main.rs pass). The store must open cleanly and record.
    let store = StelLedgerStore::open(root, "prod-convention");
    assert!(
        matches!(store, StelLedgerStore::Sqlite(_)),
        "opening under the project root must succeed (parent .symforge created on demand)"
    );
    store.record(&sample_event("p-path"));
    assert_eq!(store.summary().expect("summary").total_events, 1);
    drop(store);

    // The db must exist at exactly `<root>/.symforge/stel-ledger.db` — the
    // single-prefix path the const encodes.
    let expected = root.join(SYMFORGE_STEL_LEDGER_DB_PATH);
    assert_eq!(
        expected,
        root.join(".symforge").join("stel-ledger.db"),
        "the db-path const must encode a single .symforge prefix"
    );
    assert!(
        expected.exists(),
        "db must land at <root>/.symforge/stel-ledger.db, not exist there: {}",
        expected.display()
    );

    // The DOUBLED path the old bug produced must NOT exist — proving the prefix
    // is applied exactly once under the production convention.
    let doubled = root
        .join(".symforge")
        .join(".symforge")
        .join("stel-ledger.db");
    assert!(
        !doubled.exists(),
        "the doubled-prefix path must NOT exist (regression guard): {}",
        doubled.display()
    );
}
