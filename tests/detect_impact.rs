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
    let output = symforge::process_util::hidden_command("git")
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

    assert_eq!(payload["pagination"]["blast_radius"]["total"], json!(1));
    assert_eq!(payload["pagination"]["blast_radius"]["returned"], json!(1));
    assert_eq!(
        payload["pagination"]["blast_radius"]["truncated"],
        json!(false)
    );
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
    // Fix 6: with only a local `main` (no origin/main), the default resolves to
    // local `main` and discloses it; no staleness note is emitted.
    assert!(
        body.contains("base: main"),
        "default resolution must disclose the resolved base ref:\n{body}"
    );
    assert!(
        !body.contains("differs from origin/main"),
        "no origin/main → no staleness note:\n{body}"
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

/// Fix 6: when local `main` lags `origin/main`, the DEFAULT resolution (no
/// base_branch/since) must diff against `origin/main` — the true delta — not the
/// stale local ref, and disclose the resolved ref + staleness.
#[tokio::test]
async fn detect_impact_default_prefers_origin_main_over_stale_local() {
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
    git(&["commit", "-m", "C0 initial"], root);
    git(&["branch", "-M", "main"], root);

    // C1 changes src/b.rs — this is the "origin/main" state.
    fs::write(
        root.join("src/b.rs"),
        "pub fn call_b() -> u32 {\n    2\n}\n// C1: on origin/main only.\n",
    )
    .expect("rewrite src/b.rs");
    git(&["commit", "-am", "C1 change b"], root);
    // Record C1 as origin/main, THEN rewind local main to C0 (stale by 1 commit).
    git(&["update-ref", "refs/remotes/origin/main", "main"], root);
    git(&["checkout", "-b", "feature"], root);
    git(&["branch", "-f", "main", "HEAD~1"], root);

    // C2 changes src/a.rs on feature — the true delta vs origin/main (C1).
    fs::write(
        root.join("src/a.rs"),
        "use cbm_impact_fixture::core;\n\n\
         // C2: on feature only.\n\
         pub fn call_a() -> u32 {\n    core()\n}\n",
    )
    .expect("rewrite src/a.rs");
    git(&["commit", "-am", "C2 change a on feature"], root);

    let server = server_over(root);
    let body = server
        .dispatch_tool_for_tests("detect_impact", json!({}))
        .await;
    let payload = impact_payload(&body);

    // origin/main(C1)...HEAD(C2) = src/a.rs only. If it had used the stale local
    // main(C0) the diff would also include src/b.rs (C1) — the confidently-wrong
    // result this fix prevents.
    assert_eq!(
        payload["changed_files"],
        json!(["src/a.rs"]),
        "default must diff against origin/main (true delta), not stale local main:\n{body}"
    );
    assert!(
        body.contains("base: origin/main"),
        "must disclose the resolved base ref:\n{body}"
    );
    assert!(
        body.contains("local main is behind origin/main"),
        "must disclose direction-aware staleness (local behind) vs origin/main:\n{body}"
    );
}

/// Fix 1: a changed-set exceeding the per-list caps must bound every list at 200,
/// disclose the full totals + truncation in `pagination`, and keep `risk_summary`
/// counting the FULL blast set (not the truncated 200).
#[tokio::test]
async fn detect_impact_caps_large_changed_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"caps_fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");

    // changed.rs: core() + 210 standalone pads => 211 changed symbols (> cap).
    let mut changed = String::from("pub fn core() -> u32 {\n    0\n}\n");
    for i in 0..210 {
        changed.push_str(&format!("pub fn pad_{i}() -> u32 {{\n    {i}\n}}\n"));
    }
    fs::write(root.join("src/changed.rs"), &changed).expect("write changed.rs");

    // callers.rs: 210 functions each calling core() => 210 blast nodes (> cap),
    // all hop 1 / risk High. Lives in an UNCHANGED file so the callers are not
    // themselves hop-0 changed symbols (which compute_impact excludes).
    let mut callers = String::new();
    for i in 0..210 {
        callers.push_str(&format!("pub fn caller_{i}() -> u32 {{\n    core()\n}}\n"));
    }
    fs::write(root.join("src/callers.rs"), &callers).expect("write callers.rs");

    git(&["init"], root);
    git(&["config", "user.email", "test@test.com"], root);
    git(&["config", "user.name", "Test"], root);
    git(&["add", "."], root);
    git(&["commit", "-m", "initial"], root);

    // Second commit touches ONLY changed.rs, so the changed-file set is one file
    // but its 211 symbols all enter changed_symbols.
    changed.push_str("// widened comment, same symbols.\n");
    fs::write(root.join("src/changed.rs"), &changed).expect("rewrite changed.rs");
    git(&["commit", "-am", "touch changed"], root);

    let server = server_over(root);
    let body = server
        .dispatch_tool_for_tests("detect_impact", json!({ "since": "HEAD~1" }))
        .await;
    let payload = impact_payload(&body);

    // changed_files: exactly one, not truncated.
    assert_eq!(payload["changed_files"], json!(["src/changed.rs"]));
    assert_eq!(payload["pagination"]["changed_files"]["total"], json!(1));
    assert_eq!(
        payload["pagination"]["changed_files"]["truncated"],
        json!(false)
    );

    // changed_symbols: capped at 200, full total disclosed.
    assert_eq!(
        payload["changed_symbols"].as_array().expect("array").len(),
        200,
        "changed_symbols array must be capped at 200:\n{body}"
    );
    assert_eq!(
        payload["pagination"]["changed_symbols"]["total"],
        json!(211)
    );
    assert_eq!(
        payload["pagination"]["changed_symbols"]["returned"],
        json!(200)
    );
    assert_eq!(
        payload["pagination"]["changed_symbols"]["truncated"],
        json!(true)
    );

    // blast_radius: capped at 200, full total disclosed.
    assert_eq!(
        payload["blast_radius"].as_array().expect("array").len(),
        200,
        "blast_radius array must be capped at 200:\n{body}"
    );
    assert_eq!(payload["pagination"]["blast_radius"]["total"], json!(210));
    assert_eq!(
        payload["pagination"]["blast_radius"]["returned"],
        json!(200)
    );
    assert_eq!(
        payload["pagination"]["blast_radius"]["truncated"],
        json!(true)
    );

    // risk_summary counts the FULL blast set (210 high), NOT the truncated 200.
    assert_eq!(
        payload["risk_summary"]["high"],
        json!(210),
        "risk_summary must reflect the full blast set, not the capped list:\n{body}"
    );

    // Human summary discloses truncation with the house marker.
    assert!(
        body.contains("[truncated]"),
        "summary must disclose truncation:\n{body}"
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

/// US1 (018) FR-001/FR-002 · SC-002: a real committed source edit plus a dirty
/// (untracked) non-source data file. Default `detect_impact` must source-focus
/// the changed-set — the data file and its JSON-key symbols must not seed the
/// impact walk — while `include_data=true` restores the prior inclusive result.
///
/// Fails on pre-fix code: the untracked `data/config.json` (merged via
/// `include_untracked=true`) lands in `changed_files` and its keys become
/// changed_symbols.
#[tokio::test]
async fn detect_impact_default_source_focuses_changed_set() {
    // bootstrap_fixture commits the Rust fixture, then edits src/a.rs in a
    // second commit (the HEAD~1..HEAD delta).
    let dir = bootstrap_fixture();
    let root = dir.path();
    // Dirty, untracked data file whose JSON keys the config extractor turns
    // into first-class symbols — the reported noise source.
    fs::create_dir_all(root.join("data")).expect("mkdir data");
    fs::write(
        root.join("data/config.json"),
        "{\n  \"alpha_key\": 1,\n  \"beta_key\": 2\n}\n",
    )
    .expect("write data file");

    let server = server_over(root);

    // Default: source-focused seed → data file excluded.
    let body = server
        .dispatch_tool_for_tests("detect_impact", json!({ "since": "HEAD~1" }))
        .await;
    let payload = impact_payload(&body);
    assert_eq!(
        payload["changed_files"],
        json!(["src/a.rs"]),
        "default detect_impact must source-focus the seed and drop untracked data files:\n{body}"
    );
    let changed_names: Vec<&str> = payload["changed_symbols"]
        .as_array()
        .expect("changed_symbols array")
        .iter()
        .map(|s| s["name"].as_str().expect("symbol name"))
        .collect();
    assert!(
        !changed_names
            .iter()
            .any(|n| n.contains("alpha_key") || n.contains("beta_key")),
        "no data-file-derived (JSON key) symbols may seed the impact walk: {changed_names:?}\n{body}"
    );

    // Opt-in: include_data=true restores full inclusion (FR-003).
    let body_incl = server
        .dispatch_tool_for_tests(
            "detect_impact",
            json!({ "since": "HEAD~1", "include_data": true }),
        )
        .await;
    let payload_incl = impact_payload(&body_incl);
    let incl_files: Vec<&str> = payload_incl["changed_files"]
        .as_array()
        .expect("changed_files array")
        .iter()
        .map(|f| f.as_str().expect("file path"))
        .collect();
    assert!(
        incl_files.contains(&"data/config.json"),
        "include_data=true must restore the untracked data file in the changed-set: {incl_files:?}\n{body_incl}"
    );
}
