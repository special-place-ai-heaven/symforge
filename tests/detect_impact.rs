//! detect_impact acceptance — Program 015 S1a (C-S1A-003).
//!
//! `tests/fixtures/cbm_impact` ships without a `.git/` directory (see its
//! README): rather than committing a permanent nested git repo into this
//! repo, each test copies the fixture source into a fresh tempdir and
//! bootstraps a real 2-commit history there, mirroring `git.rs`'s own
//! `make_test_repo` helper (git CLI for repo setup only, never production
//! code).

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;

const FIXTURE_FILES: &[&str] = &[
    "Cargo.toml",
    "src/lib.rs",
    "src/a.rs",
    "src/b.rs",
    "src/c.rs",
    "src/main.rs",
];

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

/// Copy `tests/fixtures/cbm_impact` into a fresh tempdir, commit it, then
/// make a real content edit to `src/a.rs` and commit again — the
/// `commit_2_changes_src_a_rs` scenario from `expected_impact.json`. The edit
/// only widens a comment; `call_a`'s call-graph edges (it still calls
/// `core()`) are untouched, so the blast radius is driven purely by who
/// calls `call_a`, not by what this specific edit changed.
fn bootstrap_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let fixture_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cbm_impact");

    for rel in FIXTURE_FILES {
        let from = fixture_src.join(rel);
        let to = root.join(rel);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir for {rel}: {e}"));
        }
        fs::copy(&from, &to).unwrap_or_else(|e| panic!("copy {rel}: {e}"));
    }

    git(&["init"], root);
    git(&["config", "user.email", "test@test.com"], root);
    git(&["config", "user.name", "Test"], root);
    git(&["add", "."], root);
    git(&["commit", "-m", "initial"], root);

    fs::write(
        root.join("src/a.rs"),
        "use cbm_impact_fixture::core;\n\n\
         // S1a fixture bootstrap: widened comment, same call graph.\n\
         pub fn call_a() -> u32 {\n    core()\n}\n",
    )
    .expect("rewrite src/a.rs");
    git(&["commit", "-am", "change a"], root);

    dir
}

fn server_over(root: &Path) -> SymForgeServer {
    let shared = LiveIndex::load(root).expect("load index");
    SymForgeServer::new(
        shared,
        "detect_impact_test".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    )
}

/// Extract the embedded `--- impact payload ---` JSON block from a
/// `detect_impact` response body (house convention: tool responses are text,
/// never raw JSON — see `format::detect_impact_result`).
fn impact_payload(body: &str) -> Value {
    const MARKER: &str = "--- impact payload ---\n";
    let start = body
        .find(MARKER)
        .unwrap_or_else(|| panic!("no impact payload marker in:\n{body}"));
    let json_text = &body[start + MARKER.len()..];
    // A depth-cap warning footer (if any) follows a blank line after the JSON.
    let json_text = json_text.split("\n\nWarning:").next().unwrap_or(json_text);
    serde_json::from_str(json_text)
        .unwrap_or_else(|e| panic!("parse impact payload: {e}\n---\n{json_text}"))
}

#[tokio::test]
async fn detect_impact_fixture_blast_matches_expected() {
    let dir = bootstrap_fixture();
    let server = server_over(dir.path());

    let body = server
        .dispatch_tool_for_tests("detect_impact", json!({ "since": "HEAD~1" }))
        .await;
    let payload = impact_payload(&body);

    assert_eq!(payload["changed_files"], json!(["src/a.rs"]));

    let changed_names: Vec<&str> = payload["changed_symbols"]
        .as_array()
        .expect("changed_symbols array")
        .iter()
        .map(|s| s["name"].as_str().expect("symbol name"))
        .collect();
    assert_eq!(
        changed_names,
        vec!["call_a"],
        "src/a.rs defines exactly one symbol: call_a"
    );

    // main() is the only caller of call_a (via the qualified `a::call_a()`)
    // and is itself the entry point, so it lands at hop 1 / risk critical.
    let blast = payload["blast_radius"]
        .as_array()
        .expect("blast_radius array");
    assert_eq!(blast.len(), 1, "unexpected blast radius: {blast:?}");
    assert_eq!(blast[0]["symbol"], json!("main"));
    assert_eq!(blast[0]["hop"], json!(1));
    assert_eq!(blast[0]["risk"], json!("critical"));

    assert_eq!(payload["risk_summary"]["critical"], json!(1));
    assert_eq!(payload["risk_summary"]["high"], json!(0));
    assert_eq!(payload["risk_summary"]["medium"], json!(0));
    assert_eq!(payload["risk_summary"]["low"], json!(0));

    assert_eq!(payload["pagination"]["total"], json!(1));
    assert_eq!(payload["pagination"]["returned"], json!(1));
    assert_eq!(payload["pagination"]["has_more"], json!(false));
}

/// Contract default (contracts/detect-impact.md § Input): omitting both
/// `base_branch` and `since` must diff against `main`, not silently degrade to
/// an uncommitted-only (empty, on a clean tree) result. Regression guard for
/// the STEL-upgraded path (`route_impact` plans only `{"scope":"files"}`).
#[tokio::test]
async fn detect_impact_defaults_base_branch_to_main_when_unset() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let fixture_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cbm_impact");
    for rel in FIXTURE_FILES {
        let to = root.join(rel);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir for {rel}: {e}"));
        }
        fs::copy(fixture_src.join(rel), &to).unwrap_or_else(|e| panic!("copy {rel}: {e}"));
    }

    git(&["init"], root);
    git(&["config", "user.email", "test@test.com"], root);
    git(&["config", "user.name", "Test"], root);
    git(&["add", "."], root);
    git(&["commit", "-m", "initial"], root);
    // Force the default branch name so the test does not depend on the host's
    // `init.defaultBranch`.
    git(&["branch", "-M", "main"], root);

    // A committed-but-unmerged change on a feature branch, clean working tree.
    // Before the fix, no base/since meant "uncommitted-only" → empty result
    // here (nothing uncommitted). With the contract default, the omitted
    // base_branch diffs against `main` and surfaces the committed change.
    git(&["checkout", "-b", "feature"], root);
    fs::write(
        root.join("src/a.rs"),
        "use cbm_impact_fixture::core;\n\n\
         // committed on feature, not merged into main.\n\
         pub fn call_a() -> u32 {\n    core()\n}\n",
    )
    .expect("rewrite src/a.rs");
    git(&["commit", "-am", "change a on feature"], root);

    let server = server_over(root);

    let body = server
        .dispatch_tool_for_tests("detect_impact", json!({}))
        .await;
    let payload = impact_payload(&body);

    assert_eq!(
        payload["changed_files"],
        json!(["src/a.rs"]),
        "omitting base_branch/since must diff against the default `main` branch \
         (a real blast radius), not return an empty uncommitted-only result:\n{body}"
    );
}

#[tokio::test]
async fn detect_impact_non_git_repo_clear_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("lib.rs"), "pub fn core() {}\n").expect("write fixture file");
    let server = server_over(dir.path());

    let body = server
        .dispatch_tool_for_tests("detect_impact", json!({}))
        .await;

    assert!(
        body.contains("Git unavailable"),
        "expected a clear git-unavailable error, got:\n{body}"
    );
}

#[tokio::test]
async fn detect_impact_depth_cap_at_five() {
    let dir = bootstrap_fixture();
    let server = server_over(dir.path());

    let body = server
        .dispatch_tool_for_tests("detect_impact", json!({ "since": "HEAD~1", "depth": 99 }))
        .await;

    assert!(
        body.contains("depth clamped to 5 (requested 99)"),
        "expected a depth-cap warning, got:\n{body}"
    );
    let payload = impact_payload(&body);
    // The fixture's blast radius terminates at hop 1 regardless of depth, so
    // the clamp is exercised via the warning text above, not the payload shape.
    assert_eq!(payload["changed_files"], json!(["src/a.rs"]));
}
