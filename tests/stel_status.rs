//! Compact-surface `status` tool — operational STEL report.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::path::PathBuf;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
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
