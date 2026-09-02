//! Compact-surface `status` tool — operational STEL report.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::path::PathBuf;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::stel::types::{AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent};
use symforge::stel::{self, GoldenRouteRow};

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
        .expect("status result must contain text content")
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

async fn dispatch_status(server: &SymForgeServer, detail: Option<&str>) -> String {
    let mut params = serde_json::Map::new();
    if let Some(level) = detail {
        params.insert("detail".to_string(), serde_json::json!(level));
    }
    let result = server
        .dispatch_tool_result_for_tests("status", serde_json::Value::Object(params))
        .await
        .expect("status dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize CallToolResult");
    tool_result_text(&serialized).to_string()
}

async fn replay_symforge_row(server: &SymForgeServer, row: &GoldenRouteRow) {
    let request = row.to_request();
    let params = serde_json::to_value(stel::SymforgeCallInput {
        request,
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("symforge params serialize");
    server
        .dispatch_tool_result_for_tests("symforge", params)
        .await
        .expect("symforge dispatch");
}

fn row_by_id<'a>(rows: &'a [GoldenRouteRow], id: &str) -> &'a GoldenRouteRow {
    rows.iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("missing golden row {id}"))
}

#[tokio::test]
async fn status_runs_on_full_surface_and_reports_it() {
    // Wave 1 Fix 4: `status` is a read-only health/trust readout the docs tell
    // every client to call at session start, so it must NOT refuse on the full
    // surface (the pre-fix behavior). It self-describes the ACTIVE surface
    // instead of erroring or lying about being compact.
    if !corpora_available() {
        eprintln!("skip status_runs_on_full_surface_and_reports_it: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("full");

    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "status-full-surface");
    let output = dispatch_status(&server, None).await;

    assert!(
        !output.contains("requires SYMFORGE_SURFACE=compact"),
        "status must not refuse on the full surface:\n{output}"
    );
    assert!(
        output.contains("── stel status ──"),
        "status must render its report on the full surface:\n{output}"
    );
    assert!(
        output.contains("surface: full"),
        "status must self-describe the active (full) surface:\n{output}"
    );
}

#[tokio::test]
async fn compact_status_reports_operational_state() {
    if !corpora_available() {
        eprintln!("skip compact_status_reports_operational_state: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "status-compact");
    let output = dispatch_status(&server, None).await;

    for needle in [
        "── stel status ──",
        "surface: compact",
        &format!("phase0_go: {}", stel::PHASE0_GO_COMMIT),
        &format!("phase0_evidence: {}", stel::PHASE0_EVIDENCE_COMMIT),
        "l1_planner: wired",
        "l4_ledger: in_memory",
        "handler_status: wired",
        "handler_symforge_edit: preview-and-apply",
        "ledger_events: 0",
        "index_ready: true",
        &format!("deferred: {}", stel::DEFERRED_ITEMS),
    ] {
        assert!(output.contains(needle), "missing `{needle}` in:\n{output}");
    }
}

#[tokio::test]
async fn full_status_includes_project_and_ledger_summary() {
    if !corpora_available() {
        eprintln!("skip full_status_includes_project_and_ledger_summary: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let rows = stel::load_golden_rows(&golden_fixture_path()).expect("golden fixture");
    let row = row_by_id(&rows, "cfg-if/t4_refs");
    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "status-full");
    replay_symforge_row(&server, row).await;

    let output = dispatch_status(&server, Some("full")).await;
    assert!(output.contains("project: status-full"));
    assert!(output.contains("ledger_events: 1"));
    assert!(output.contains("last_ledger_decision: serve"));
    assert!(output.contains("last_ledger_route: find_references"));
    assert!(output.contains("── calibration (observational) ──"));
    assert!(output.contains("serve: 1"));
    assert!(output.contains("legacy_executed: 1"));
    assert!(output.contains("tuning:"));
}

/// A server over an empty index — the status body needs no corpus, only the
/// session ledger this test seeds directly (032 US2 T012).
fn empty_index_server(project: &str) -> SymForgeServer {
    SymForgeServer::new(
        LiveIndex::empty(),
        project.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        None,
        None,
    )
}

/// A literal serve event. `ts_ms`/`plan_id` are the clocks the collapse
/// identity ignores; `equivalence` is the one identity field the control flips
/// (calibration never reads it, so every aggregate line stays byte-stable).
fn serve_event(ts_ms: u64, equivalence: Option<serde_json::Value>) -> StelLedgerEvent {
    StelLedgerEvent {
        ts_ms,
        plan_id: format!("plan-{ts_ms}"),
        surface: "symforge".to_string(),
        intent: IntentBucket::Trace,
        decision: AdmissionDecision::Serve,
        tools_called: vec!["find_references".to_string()],
        predicted_response_tokens: 400,
        actual_response_tokens: 380,
        manual_baseline_tokens: 800,
        net_vs_manual: 420,
        equivalence,
        route_confidence: RouteConfidence::Exact,
        pff_bypass: None,
        cache_hit: None,
        degrade_flags: vec![],
    }
}

fn exact_line<'a>(output: &'a str, prefix: &str) -> &'a str {
    output
        .lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("missing `{prefix}` line in:\n{output}"))
}

#[tokio::test]
async fn status_full_annotates_trailing_run() {
    // 032 US2 (spec FR-007 / SC-003, status lane): a trailing run of N≥2
    // ledger-identical events renders ` ×N (first=…, last=…)` on the
    // `last_ledger_decision:` line ONLY; a trailing run of 1 renders today's
    // bare line byte-for-byte, and no other line moves.
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    // Control 1: a single event — today's format, no annotation anywhere.
    let single = empty_index_server("status-runs");
    single.stel_ledger().lock().push(serve_event(1_000, None));
    let single_body = dispatch_status(&single, Some("full")).await;
    assert_eq!(
        exact_line(&single_body, "last_ledger_decision:"),
        "last_ledger_decision: serve",
        "single event renders the bare line:\n{single_body}"
    );
    assert!(
        !single_body.contains('×'),
        "single event must carry no run annotation:\n{single_body}"
    );

    // Positive: a trailing run of three events identical in every identity
    // field (they differ only in ts_ms/plan_id).
    let tripled = empty_index_server("status-runs");
    {
        let ledger = tripled.stel_ledger().lock();
        for ts_ms in 1_000..1_003 {
            ledger.push(serve_event(ts_ms, None));
        }
    }
    let tripled_body = dispatch_status(&tripled, Some("full")).await;
    assert_eq!(
        exact_line(&tripled_body, "last_ledger_decision:"),
        "last_ledger_decision: serve ×3 (first=1000, last=1002)",
        "trailing run of 3 must be annotated on the decision line:\n{tripled_body}"
    );
    assert_eq!(
        exact_line(&tripled_body, "last_ledger_route:"),
        "last_ledger_route: find_references",
        "the route line is never annotated:\n{tripled_body}"
    );
    assert_eq!(
        tripled_body.matches('×').count(),
        1,
        "exactly one annotated line:\n{tripled_body}"
    );

    // Control 2: the SAME three-event totals, but the middle event breaks the
    // run (trailing run of 1). Its decision line is today's bare format, and
    // every OTHER line equals the tripled render — the aggregates count the
    // uncollapsed events (FR-008) and the suffix is the only difference.
    let broken = empty_index_server("status-runs");
    {
        let ledger = broken.stel_ledger().lock();
        ledger.push(serve_event(1_000, None));
        ledger.push(serve_event(
            1_001,
            Some(serde_json::json!({ "probe": true })),
        ));
        ledger.push(serve_event(1_002, None));
    }
    let broken_body = dispatch_status(&broken, Some("full")).await;
    assert_eq!(
        exact_line(&broken_body, "last_ledger_decision:"),
        "last_ledger_decision: serve",
        "a trailing run of 1 renders today's bare line:\n{broken_body}"
    );
    assert!(
        broken_body.contains("ledger_events: 3"),
        "the aggregate still counts every stored event:\n{broken_body}"
    );
    assert_eq!(
        tripled_body.lines().count(),
        broken_body.lines().count(),
        "annotation must not add or remove lines:\n{tripled_body}\n---\n{broken_body}"
    );
    let differing: Vec<(&str, &str)> = tripled_body
        .lines()
        .zip(broken_body.lines())
        .filter(|(annotated, bare)| annotated != bare)
        .collect();
    assert_eq!(
        differing,
        vec![(
            "last_ledger_decision: serve ×3 (first=1000, last=1002)",
            "last_ledger_decision: serve"
        )],
        "only the decision line may differ:\n{tripled_body}\n---\n{broken_body}"
    );

    // Large-N control (spec SC-003, status lane): 10,000 identical events
    // render ×10000 — never overflowed, never truncated.
    let large = empty_index_server("status-runs");
    {
        let ledger = large.stel_ledger().lock();
        for ts_ms in 1_000..11_000 {
            ledger.push(serve_event(ts_ms, None));
        }
    }
    let large_body = dispatch_status(&large, Some("full")).await;
    assert_eq!(
        exact_line(&large_body, "last_ledger_decision:"),
        "last_ledger_decision: serve ×10000 (first=1000, last=10999)",
        "a 10,000-run must be counted in full:\n{large_body}"
    );
    assert!(
        large_body.contains("ledger_events: 10000"),
        "the aggregate is the uncollapsed count:\n{large_body}"
    );
}
