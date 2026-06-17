// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Acceptance coverage for the impact intent one-envelope behavior
//! (feature 007, US5 / FR-012).
//!
//! The `symforge` impact intent plans a single `find_dependents` step. This test
//! pins that the response envelope chains BOTH the file dependents (from
//! `find_dependents`) AND the git co-change partners (from the shared
//! `git_temporal()` snapshot) into one body — reusing the existing
//! `analyze_file_impact` co-change flow, not a second index or a forked
//! formatter.
//!
//! A fresh tempdir is not a git repo, so the temporal index is seeded directly
//! onto the shared handle (mirroring `test_search_files_changed_with_surfaces_weak_candidates`).

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;
use symforge::live_index::LiveIndex;
use symforge::live_index::git_temporal::{
    CoChangeEntry, CommitSummary, GitFileHistory, GitTemporalIndex, GitTemporalState,
    GitTemporalStats,
};
use symforge::protocol::SymForgeServer;
use symforge::stel::{self, IntentBucket, StelRequest};
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

fn library_with_one_dependent() -> Vec<(&'static str, &'static str)> {
    vec![
        ("src/lib.rs", "pub mod widget;\npub mod consumer;\n"),
        ("src/widget.rs", "pub fn render() -> u32 {\n    1\n}\n"),
        (
            "src/consumer.rs",
            "use crate::widget;\n\npub fn run() -> u32 {\n    widget::render()\n}\n",
        ),
    ]
}

/// Build a server over a tempdir fixture and seed a `Ready` git temporal index
/// with a strong co-change partner for `widget.rs`.
fn server_with_seeded_cochange() -> (TempDir, SymForgeServer) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    for (rel, content) in library_with_one_dependent() {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, content).expect("write fixture file");
    }
    let shared = LiveIndex::load(&root).expect("LiveIndex::load");

    let history = GitFileHistory {
        commit_count: 7,
        churn_score: 0.9,
        last_commit: CommitSummary {
            hash: "abc1234".to_string(),
            timestamp: "2026-06-01T12:00:00Z".to_string(),
            author: "Tester".to_string(),
            message_head: "touch widget".to_string(),
            days_ago: 2.0,
        },
        contributors: vec![],
        co_changes: vec![CoChangeEntry {
            path: "src/consumer.rs".to_string(),
            coupling_score: 0.71,
            shared_commits: 5,
        }],
        weak_co_changes: vec![],
    };
    shared.update_git_temporal(GitTemporalIndex {
        files: HashMap::from([("src/widget.rs".to_string(), history)]),
        stats: GitTemporalStats {
            total_commits_analyzed: 14,
            analysis_window_days: 90,
            hotspots: vec![],
            most_coupled: vec![],
            computed_at: SystemTime::now(),
            compute_duration: Duration::ZERO,
        },
        state: GitTemporalState::Ready,
    });

    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let server = SymForgeServer::new(
        shared,
        "impact_intent_test".to_string(),
        watcher_info,
        Some(root),
        None,
    );
    (dir, server)
}

fn tool_result_text(result: &serde_json::Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .expect("symforge result must contain text content")
        .to_string()
}

async fn run_impact_intent(server: &SymForgeServer, path: &str) -> String {
    let request = StelRequest {
        query: format!("what depends on {path}"),
        intent: Some(IntentBucket::Impact),
        path: Some(path.to_string()),
        symbol: None,
        max_tokens: None,
        preview: None,
    };
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
    tool_result_text(&serialized)
}

#[tokio::test]
async fn impact_intent_returns_dependents_and_cochanges_in_one_envelope() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let (_dir, server) = server_with_seeded_cochange();

    // Sanity: widget.rs has a real dependent under the parser.
    let dependent_count = server
        .index()
        .read()
        .capture_find_dependents_view("src/widget.rs")
        .files
        .len();
    assert!(
        dependent_count >= 1,
        "fixture must yield at least one dependent of src/widget.rs (got {dependent_count})"
    );

    let body = run_impact_intent(&server, "src/widget.rs").await;

    // Dependents portion: the find_dependents step names the importing file.
    assert!(
        body.contains("src/consumer.rs"),
        "impact intent envelope must report the file dependent (src/consumer.rs):\n{body}"
    );

    // Co-change portion: chained into the SAME envelope from git_temporal.
    assert!(
        body.contains("Git temporal data for src/widget.rs"),
        "impact intent envelope must chain the git co-change section:\n{body}"
    );
    assert!(
        body.contains("Co-changing files"),
        "impact intent envelope must list co-changing files:\n{body}"
    );
}

/// When temporal is not `Ready` (the default for a non-git tempdir), the impact
/// intent still returns the dependents envelope plus a short, non-fatal note —
/// it must not error or omit the dependents body.
#[tokio::test]
async fn impact_intent_dependents_only_when_temporal_not_ready() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    for (rel, content) in library_with_one_dependent() {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, content).expect("write fixture file");
    }
    let shared = LiveIndex::load(&root).expect("LiveIndex::load");
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let server = SymForgeServer::new(
        shared,
        "impact_intent_no_temporal".to_string(),
        watcher_info,
        Some(root),
        None,
    );

    let body = run_impact_intent(&server, "src/widget.rs").await;

    assert!(
        body.contains("src/consumer.rs"),
        "impact intent must still report dependents without git temporal data:\n{body}"
    );
    assert!(
        !body.contains("Co-changing files"),
        "no co-changing files should be listed when temporal is not Ready:\n{body}"
    );
}
