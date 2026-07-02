// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Acceptance coverage for the impact intent's routing (feature 007, US5 /
//! FR-012 — upgraded by Program 015 C-S1A-004).
//!
//! The `symforge` impact intent with a path but no symbol used to plan a
//! single `find_dependents` step, chaining git co-change partners into the
//! same envelope via `analyze_file_impact`'s co-change flow. Per
//! specs/015-cbm-capability-ports/planning/sprint-1-quick-wins-spec.md
//! § STEL impact routing, this route now plans a single `detect_impact`
//! (scope=files) step instead: git-aware blast radius, no co-change chaining
//! (`detect_impact` has no `path` input to key the chain on).
//!
//! Unlike the old `find_dependents` route, `detect_impact` derives its
//! changed-file set from git rather than from `request.path`, so these tests
//! seed an actual uncommitted change rather than merely asking "what depends
//! on this file".

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
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

fn git(args: &[&str], root: &Path) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Build a server over a real git repo, with `src/widget.rs` left as an
/// uncommitted change so `detect_impact`'s default (no base_branch/since)
/// working-tree scan picks it up. When `seed_cochange` is set, a `Ready` git
/// temporal snapshot with a strong `widget.rs` co-change partner is seeded
/// before the server is constructed (proving `detect_impact` never chains it
/// in, unlike the old `find_dependents` route).
fn server_over_repo_with_uncommitted_widget_change(
    seed_cochange: bool,
) -> (TempDir, SymForgeServer) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    for (rel, content) in library_with_one_dependent() {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, content).expect("write fixture file");
    }
    git(&["init"], &root);
    git(&["config", "user.email", "test@test.com"], &root);
    git(&["config", "user.name", "Test"], &root);
    git(&["add", "."], &root);
    git(&["commit", "-m", "initial"], &root);
    // detect_impact's default base_branch is "main" (contracts/detect-impact.md
    // § Input). Force the branch name so this test is deterministic regardless
    // of the host's `init.defaultBranch` — HEAD == main means the three-dot diff
    // is empty and the uncommitted edit below is what the blast radius reflects.
    git(&["branch", "-M", "main"], &root);

    // Uncommitted change: detect_impact's default (base_branch=main, HEAD on
    // main) merges this via `uncommitted_paths()` — no explicit base/since.
    fs::write(
        root.join("src/widget.rs"),
        "pub fn render() -> u32 {\n    2\n}\n",
    )
    .expect("modify widget.rs");

    let shared = LiveIndex::load(&root).expect("LiveIndex::load");

    if seed_cochange {
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
    }

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
        project: None,
        projects: None,
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

/// Program 015 C-S1A-004: the impact intent's path-only route now plans
/// `detect_impact(scope=files)` instead of `find_dependents`. `detect_impact`
/// ignores `request.path` (it has no such input — it derives its changed-file
/// set from git), so the response reflects the repo's actual uncommitted
/// change (`src/widget.rs`) and its blast radius, not a lookup of the
/// queried path's importers.
#[tokio::test]
async fn impact_intent_routes_to_detect_impact_blast_radius() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let (_dir, server) = server_over_repo_with_uncommitted_widget_change(false);

    let body = run_impact_intent(&server, "src/widget.rs").await;

    assert!(
        body.contains("Chosen tool: detect_impact"),
        "impact intent (path only) must route to detect_impact:\n{body}"
    );
    assert!(
        body.contains("\"src/widget.rs\""),
        "detect_impact must report the actual uncommitted change:\n{body}"
    );
    // consumer.rs's `run()` calls `widget::render()`, so it's widget.rs's sole
    // caller; scope=files aggregates the blast radius to file granularity.
    assert!(
        body.contains("\"symbol\": \"src/consumer.rs\""),
        "blast radius must include widget.rs's caller file (src/consumer.rs):\n{body}"
    );
}

/// Fix 5 (Wave 1): the impact route drops the caller's `path` (detect_impact
/// derives its changed-file set from git), but the drop must be LOUD — disclosed
/// in the response envelope via the ParamDisposition accounting, not silently
/// discarded as it was before.
#[tokio::test]
async fn impact_intent_discloses_unconsumed_path() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let (_dir, server) = server_over_repo_with_uncommitted_widget_change(false);

    let body = run_impact_intent(&server, "src/widget.rs").await;

    assert!(
        body.contains("Chosen tool: detect_impact"),
        "impact intent (path only) must route to detect_impact:\n{body}"
    );
    assert!(
        body.contains("`path` \"src/widget.rs\" not consumed"),
        "impact intent must disclose the caller's path was not consumed:\n{body}"
    );
    assert!(
        body.contains("derives its changed-file set from git"),
        "disclosure must name WHY the path was dropped:\n{body}"
    );
}

/// The old `find_dependents` + git co-change chain
/// (`append_impact_intent_cochanges`) only fires when the executed step's
/// args carry a `path` key — `detect_impact`'s args never do (scope=files
/// only), so a `Ready` git-temporal snapshot must no longer leak co-change
/// text into the impact intent's response.
#[tokio::test]
async fn impact_intent_no_longer_chains_cochanges() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let (_dir, server) = server_over_repo_with_uncommitted_widget_change(true);

    let body = run_impact_intent(&server, "src/widget.rs").await;

    assert!(
        body.contains("Chosen tool: detect_impact"),
        "impact intent (path only) must route to detect_impact:\n{body}"
    );
    assert!(
        !body.contains("Co-changing files") && !body.contains("Git temporal data for"),
        "detect_impact does not chain co-change data, even when a Ready snapshot exists:\n{body}"
    );
}
