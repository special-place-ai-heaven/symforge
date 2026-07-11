// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use symforge::analytics::{
    AnalyticsObservation, AnalyticsScope, AnalyticsSurface, SqliteAnalyticsStore,
};
use symforge::protocol::SymForgeServer;
use symforge::protocol::result_status::OutcomeClass;

fn symforge_command() -> std::process::Command {
    symforge::process_util::hidden_command(env!("CARGO_BIN_EXE_symforge"))
}

fn run_json(args: &[&str]) -> Value {
    let output = symforge_command()
        .args(args)
        .output()
        .expect("symforge command should run");
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

fn db_arg(db_path: &Path) -> &str {
    db_path.to_str().expect("test path should be utf-8")
}

fn analytics_db_path(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join(".symforge").join("analytics.db")
}

fn record_observation(
    store: &SqliteAnalyticsStore,
    tool_name: &str,
    success: bool,
    outcome_class: OutcomeClass,
) {
    store
        .record(&AnalyticsObservation::new(
            tool_name,
            AnalyticsSurface::Tool,
            AnalyticsScope::Session,
            120,
            Some(30),
            Duration::from_millis(7),
            success,
            outcome_class,
        ))
        .expect("record observation");
}

#[test]
fn analytics_status_is_explicitly_disabled_and_creates_no_database() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = analytics_db_path(&tmp);

    let status = run_json(&["analytics", "status", "--db-path", db_arg(&db_path)]);

    assert_eq!(status["mode"], "disabled");
    assert_eq!(status["db_exists"], false);
    assert_eq!(status["schema_version"], Value::Null);
    assert!(!db_path.exists(), "disabled status must not create a DB");
}

#[test]
fn analytics_summary_and_export_are_bounded_and_redacted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = analytics_db_path(&tmp);
    let store = SqliteAnalyticsStore::open(&db_path).expect("analytics store");
    record_observation(&store, "get_file_content", true, OutcomeClass::Found);
    record_observation(&store, "search_text", false, OutcomeClass::InternalFailure);
    store
        .record(&AnalyticsObservation::new(
            "get_symbol",
            AnalyticsSurface::Other("Authorization: Bearer sk-export-secret".to_string()),
            AnalyticsScope::Other("password=export-secret".to_string()),
            200,
            Some(50),
            Duration::from_millis(11),
            true,
            OutcomeClass::Found,
        ))
        .expect("record redacted observation");
    drop(store);

    let summary = run_json(&["analytics", "summary", "--db-path", db_arg(&db_path)]);
    assert_eq!(summary["mode"], "enabled");
    assert_eq!(summary["summary"]["total_records"], 3);
    assert_eq!(summary["summary"]["success_count"], 2);
    assert_eq!(summary["summary"]["failure_count"], 1);

    let export = run_json(&[
        "analytics",
        "export",
        "--db-path",
        db_arg(&db_path),
        "--limit",
        "2",
    ]);
    assert_eq!(export["mode"], "enabled");
    assert_eq!(export["limit"], 2);
    assert_eq!(export["records"].as_array().expect("records").len(), 2);
    let exported = serde_json::to_string(&export).expect("export JSON string");
    assert!(!exported.contains("sk-export-secret"));
    assert!(!exported.contains("password=export-secret"));
}

#[test]
fn analytics_reset_removes_only_analytics_storage() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = analytics_db_path(&tmp);
    let symforge_dir = db_path.parent().expect("analytics parent");
    let store = SqliteAnalyticsStore::open(&db_path).expect("analytics store");
    record_observation(&store, "get_file_content", true, OutcomeClass::Found);
    drop(store);
    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    std::fs::write(&wal_path, b"wal").expect("wal sidecar");
    let index_path = symforge_dir.join("index.bin");
    std::fs::write(&index_path, b"index snapshot").expect("index snapshot");

    assert!(db_path.exists(), "precondition: analytics DB exists");
    assert!(wal_path.exists(), "precondition: analytics WAL exists");
    assert!(index_path.exists(), "precondition: index snapshot exists");

    let reset = run_json(&["analytics", "reset", "--db-path", db_arg(&db_path)]);

    assert_eq!(reset["mode"], "reset");
    assert!(!db_path.exists(), "reset must delete analytics DB");
    assert!(!wal_path.exists(), "reset must delete analytics WAL");
    assert!(
        index_path.exists(),
        "reset must not delete non-analytics snapshots"
    );
    assert!(
        reset["removed"]
            .as_array()
            .expect("removed paths")
            .iter()
            .any(|path| path.as_str().unwrap_or_default().ends_with("analytics.db"))
    );
}

#[test]
fn no_mcp_analytics_tool_is_advertised() {
    let tool_names = SymForgeServer::tool_definitions()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();

    assert!(
        !tool_names.iter().any(|name| name == "analytics"),
        "SFB26 must not add an MCP analytics tool: {tool_names:?}"
    );
}
