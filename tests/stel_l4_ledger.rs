//! L4 ledger — serve and P-FF bypass invocations record decision/execution metadata.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::path::PathBuf;

use std::sync::Arc;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::stel::ledger_store::StelLedgerStore;
use symforge::stel::{self, AdmissionDecision, GoldenRouteRow};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn golden_fixture_path() -> PathBuf {
    repo_root().join(stel::GOLDEN_ROUTES_FIXTURE)
}

fn corpus_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

fn corpus_available(relative: &str, marker: &str) -> bool {
    corpus_path(relative).join(marker).is_file()
}

fn corpora_available() -> bool {
    corpus_available(stel::S4_REPLAY_CORPUS, "src/lib.rs")
}

fn tool_result_text(result: &serde_json::Value) -> &str {
    result["content"][0]["text"]
        .as_str()
        .expect("symforge result must contain text content")
}

fn server_for_corpus(relative: &str, project: &str) -> SymForgeServer {
    let root = corpus_path(relative);
    let shared = LiveIndex::load(&root).unwrap_or_else(|error| {
        panic!("index {}: {error}", root.display());
    });
    SymForgeServer::new(
        shared,
        project.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root),
        None,
    )
}

/// Build a corpus server with an in-memory durable [`StelLedgerStore`] wired in
/// (US3/T028). Returns the shared store handle so the test can read it back and
/// assert the durable write-through. The same `Arc` is held by the server, so
/// this is exactly the path `build_serve_runtime` uses on `/mcp`.
fn server_for_corpus_with_store(
    relative: &str,
    project: &str,
    session_id: &str,
) -> (SymForgeServer, Arc<StelLedgerStore>) {
    let root = corpus_path(relative);
    let shared = LiveIndex::load(&root).unwrap_or_else(|error| {
        panic!("index {}: {error}", root.display());
    });
    let store = Arc::new(StelLedgerStore::open_in_memory(session_id).expect("in-memory store"));
    let server = SymForgeServer::new(
        shared,
        project.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root),
        None,
    )
    .with_stel_ledger_store(Arc::clone(&store));
    (server, store)
}

async fn replay_row(server: &SymForgeServer, row: &GoldenRouteRow) -> String {
    let request = row.to_request();
    let params = serde_json::to_value(stel::SymforgeCallInput {
        request,
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("symforge params serialize");
    let result = server
        .dispatch_tool_result_for_tests("symforge", params)
        .await
        .expect("symforge dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize CallToolResult");
    tool_result_text(&serialized).to_string()
}

fn row_by_id<'a>(rows: &'a [GoldenRouteRow], id: &str) -> &'a GoldenRouteRow {
    rows.iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("missing golden row {id}"))
}

/// P2-C: the durable ledger write is now offloaded onto `spawn_blocking`, so it
/// lands *eventually* (asynchronously) rather than synchronously inline on the
/// request path. Poll the durable store's `total_events` until it reaches
/// `expected` or the deadline elapses. Returns the observed count.
async fn await_durable_events(store: &Arc<StelLedgerStore>, expected: u64) -> u64 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let total = store.summary().map(|s| s.total_events).unwrap_or(0);
        if total >= expected || std::time::Instant::now() >= deadline {
            return total;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn ledger_meta_line(output: &str) -> &str {
    output
        .lines()
        .find(|line| line.starts_with("ledger: "))
        .expect("ledger line in envelope")
}

#[tokio::test]
async fn serve_row_records_ledger_with_legacy_execution() {
    if !corpora_available() {
        eprintln!("skip serve_row_records_ledger: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/t4_refs");
    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "cfg-if-rust");
    let output = replay_row(&server, row).await;

    assert!(output.contains("ledger: "));
    let ledger_json = ledger_meta_line(&output).trim_start_matches("ledger: ");
    let meta: stel::LedgerEnvelopeMeta = serde_json::from_str(ledger_json).expect("ledger json");
    assert_eq!(meta.decision, "serve");
    assert!(!meta.bypass);
    assert!(meta.legacy_executed);
    assert_eq!(meta.route_tool, "find_references");
    assert!(meta.schema_tokens > 0);
    assert!(meta.invoke_tokens > 0);
    assert!(meta.output_bytes > 0);

    let event = server.stel_ledger().lock().last().expect("ledger event");
    assert_eq!(event.decision, AdmissionDecision::Serve);
    assert_eq!(event.tools_called, vec!["find_references".to_string()]);
}

#[tokio::test]
async fn pff_row_records_ledger_without_legacy_execution() {
    if !corpora_available() {
        eprintln!("skip pff_row_records_ledger: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/pff_whole_lib");
    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "cfg-if-rust");
    let output = replay_row(&server, row).await;

    assert!(output.contains("ledger: "));
    let ledger_json = ledger_meta_line(&output).trim_start_matches("ledger: ");
    let meta: stel::LedgerEnvelopeMeta = serde_json::from_str(ledger_json).expect("ledger json");
    assert_eq!(meta.decision, "bypass");
    assert!(meta.bypass);
    assert!(!meta.legacy_executed);
    assert!(meta.predicted_net != 0);

    let event = server.stel_ledger().lock().last().expect("ledger event");
    assert_eq!(event.decision, AdmissionDecision::Bypass);
    assert!(event.tools_called.is_empty());
}

#[tokio::test]
async fn serve_invocation_writes_through_to_durable_store() {
    // US3/T028+T029: a serve invocation on a server with a wired durable store
    // persists the event through to SQLite (write-through, off the in-memory
    // path) AND the store's summary() observes the row (the read path the
    // `status` tool surfaces). One ledger path: exactly one durable row per
    // invocation, matching the single in-memory event.
    if !corpora_available() {
        eprintln!("skip serve_invocation_writes_through_to_durable_store: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/t4_refs");
    let (server, store) =
        server_for_corpus_with_store(stel::S4_REPLAY_CORPUS, "cfg-if-rust", "serve-writethrough");

    // Durable store starts empty.
    assert_eq!(store.summary().expect("summary").total_events, 0);

    let _output = replay_row(&server, row).await;

    // In-memory ledger recorded exactly one event synchronously/immediately...
    assert_eq!(server.stel_ledger().lock().len(), 1);
    // ...and the durable store eventually holds exactly one matching row. P2-C:
    // the durable write is now offloaded onto `spawn_blocking`, so it lands
    // asynchronously (off the request path) rather than inline — poll for it.
    let total = await_durable_events(&store, 1).await;
    assert_eq!(
        total, 1,
        "durable store must hold exactly one row after one serve invocation (write-through landed)"
    );
    let summary = store
        .summary()
        .expect("durable summary after write-through");
    assert_eq!(
        summary.total_events, 1,
        "durable store must hold exactly one row after one serve invocation"
    );
    assert_eq!(summary.session_count, 1);

    let recent = store.recent(10).expect("recent durable rows");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].decision, "serve");
    assert_eq!(recent[0].session_id, "serve-writethrough");
    assert_eq!(recent[0].tools_called_json, r#"["find_references"]"#);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn durable_write_is_offloaded_off_the_request_path() {
    // P2-C: the durable SQLite write (sync INSERT under a 5000ms busy-timeout)
    // must not block the request task. After the request returns, the in-memory
    // `SessionLedger` push is observed *immediately* (synchronous on the hot
    // path), while the durable row lands *eventually* (offloaded onto
    // `spawn_blocking`). This test asserts both halves of that contract:
    //   1. request path completes and the in-memory event is already present;
    //   2. the durable event is not lost — it shows up after a bounded poll.
    if !corpora_available() {
        eprintln!("skip durable_write_is_offloaded_off_the_request_path: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/t4_refs");
    let (server, store) =
        server_for_corpus_with_store(stel::S4_REPLAY_CORPUS, "cfg-if-rust", "offload-nonblock");

    let request_started = std::time::Instant::now();
    let _output = replay_row(&server, row).await;
    let request_elapsed = request_started.elapsed();

    // The in-memory push is synchronous: it is already visible the instant the
    // request returns, with no dependency on the durable write completing.
    assert_eq!(
        server.stel_ledger().lock().len(),
        1,
        "in-memory ledger push must be synchronous/immediate on the request path"
    );

    // The request path itself must not have waited on the durable write. The
    // durable write is best-effort and fire-and-forget; the request returning
    // far below the 5000ms busy-timeout demonstrates it was not awaited inline.
    assert!(
        request_elapsed < std::time::Duration::from_secs(2),
        "request path returned in {request_elapsed:?}; must not block on the durable DB write"
    );

    // The durable write still lands (eventually) — events are never lost on
    // normal operation.
    let total = await_durable_events(&store, 1).await;
    assert_eq!(
        total, 1,
        "durable write must still land after being offloaded off the request path"
    );
}

#[tokio::test]
async fn serve_without_durable_store_keeps_in_memory_ledger_only() {
    // Stdio-shaped server (no durable store): in-memory ledger still records,
    // and there is simply no durable sink. Guards against the write-through
    // accidentally becoming load-bearing for the in-memory path.
    if !corpora_available() {
        eprintln!("skip serve_without_durable_store_keeps_in_memory_ledger_only: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/t4_refs");
    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "cfg-if-rust");

    let _output = replay_row(&server, row).await;

    assert_eq!(server.stel_ledger().lock().len(), 1);
}
