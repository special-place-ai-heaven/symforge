// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Acceptance coverage for STEL find fusion (feature 007, US4).
//!
//! Contract: `specs/007-intelligence-pattern-ports/contracts/stel-find-fusion.md`
//! · Research R4.
//!
//! A multi-word, fuzzy find query (e.g. "stel planner find") that matches no
//! explicit routing phrase is fused across BOTH the path/file matcher
//! (`search_files`, with the gated co-change boost) AND the symbol-name matcher
//! (`search_symbols`, tier-ordered). The STEL planner stays plan-only: it emits
//! an ordered two-step plan and the serve executor runs each step on the real
//! search surfaces and merges their bodies into one ranked envelope.
//!
//! Invariants pinned here:
//!  - both surfaces appear in one merged ranked envelope;
//!  - no NEW public tool name is introduced (only search_files + search_symbols);
//!  - co-change neighbours are boosted when a Ready coupling store exists;
//!  - the route degrades to pure path/name ranking when co-change evidence is
//!    unavailable (no error, no dropped results).
//!
//! Frecency neutrality of this route is pinned separately in
//! `tests/frecency_ranking.rs::symforge_find_intent_does_not_bump`.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;
use symforge::live_index::LiveIndex;
use symforge::live_index::coupling::{AnchorKey, CouplingRow, CouplingStore};
use symforge::paths::SYMFORGE_COUPLING_DB_PATH;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

mod git_test_helpers {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/git/test_helpers.rs"
    ));
}

// ─── Fixture ─────────────────────────────────────────────────────────────────

struct Fixture {
    _dir: TempDir,
    server: SymForgeServer,
}

impl Fixture {
    /// Plain tempdir: NOT a git repo, so the coupling store can never reach
    /// Ready. Exercises the graceful co-change-unavailable degradation path.
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        write_files(&root, files);
        let server = build_server(root);
        Self { _dir: dir, server }
    }

    /// Git repo + a seeded Ready coupling store so the co-change boost engages
    /// for the named anchor.
    fn with_ready_coupling(files: &[(&str, &str)], rows: &[CouplingRow]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        write_files(&root, files);
        let head = init_git_repo(&root);
        seed_ready_coupling(&root, &head, rows);
        let server = build_server(root);
        Self { _dir: dir, server }
    }
}

fn build_server(root: std::path::PathBuf) -> SymForgeServer {
    let shared = LiveIndex::load(&root).expect("LiveIndex::load");
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    SymForgeServer::new(
        shared,
        "stel_find_fusion_test".to_string(),
        watcher_info,
        Some(root),
        None,
    )
}

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write fixture file");
    }
}

fn init_git_repo(root: &Path) -> String {
    let repo = git2::Repository::init(root).expect("git init");
    let sig =
        git2::Signature::now("SymForge Tests", "symforge-tests@example.com").expect("git sig");
    let tree_id = {
        let mut index = repo.index().expect("git index");
        index
            .add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("git add");
        index.write_tree().expect("git write tree")
    };
    let tree = repo.find_tree(tree_id).expect("git tree");
    let commit = git_test_helpers::commit_head_with_retry(&repo, &sig, &sig, "root", &tree, &[]);
    let head = commit.to_string();
    drop(tree);
    drop(repo);
    head
}

fn seed_ready_coupling(root: &Path, head: &str, rows: &[CouplingRow]) {
    let store =
        CouplingStore::open(&root.join(SYMFORGE_COUPLING_DB_PATH)).expect("open coupling store");
    store.set_last_head(head).expect("set last head");
    store
        .set_cold_built_at(1_700_000_000)
        .expect("set cold built timestamp");
    store.bulk_upsert(rows).expect("seed coupling rows");
}

fn row(anchor: &str, partner: &str, shared: u32, weighted: f64) -> CouplingRow {
    CouplingRow {
        anchor: AnchorKey::file(anchor),
        partner: AnchorKey::file(partner),
        shared_commits: shared,
        weighted_score: weighted,
        last_commit_ts: 1_700_000_000,
    }
}

/// Run a find query through the compact `symforge` STEL surface end-to-end.
async fn run_find(server: &SymForgeServer, query: &str) -> String {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    let request = symforge::stel::StelRequest {
        query: query.to_string(),
        intent: None,
        path: None,
        symbol: None,
        max_tokens: None,
        preview: None,
    };
    let params: Value = serde_json::to_value(symforge::stel::SymforgeCallInput {
        request,
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("serialize symforge params");
    let result = server
        .dispatch_tool_result_for_tests("symforge", params)
        .await
        .expect("symforge dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize CallToolResult");
    serialized["content"][0]["text"]
        .as_str()
        .expect("symforge result text")
        .to_string()
}

/// A fixture spanning both surfaces: the multi-word query "stel planner find"
/// matches the FILE path `src/stel/planner.rs` (path tokens stel + planner) and
/// the SYMBOL name `route_find` (content/name "find"). The co-change partner
/// `src/stel/planner_helpers.rs` shares the anchor's basename stem `planner`,
/// so it stays a path candidate the co-change boost can promote.
fn stel_planner_corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "src/stel/planner.rs",
            "pub fn route_find() {}\npub fn build_plan() {}\n",
        ),
        (
            "src/stel/planner_helpers.rs",
            "pub fn planner_helper() {}\n",
        ),
        ("src/stel/executor.rs", "pub fn serve_find_step() {}\n"),
        ("src/live_index/search.rs", "pub fn find_symbol() {}\n"),
        ("src/unrelated/leaf.rs", "pub fn leaf() {}\n"),
    ]
}

// ─── (1) Fusion spans both symbol and path surfaces, in one envelope ─────────

#[tokio::test]
async fn multi_word_find_fuses_path_and_symbol_surfaces() {
    let fx = Fixture::new(&stel_planner_corpus());
    let output = run_find(&fx.server, "stel planner find").await;

    // One merged ranked envelope: STEL trust header + serve decision.
    assert!(
        output.starts_with("── stel ──"),
        "fused find must produce the STEL trust envelope; got:\n{output}"
    );
    assert!(
        output.contains("decision: serve"),
        "fused find must serve (execute) both steps; got:\n{output}"
    );

    // Path surface: a search_files step ran (path/file ranking + co-change).
    assert!(
        output.contains("Chosen tool: search_files"),
        "fused find must route a path/file matcher step; got:\n{output}"
    );

    // Name/content surface: a search_text step ran with OR terms over the
    // tokenized query (the multi-term matcher that spans symbol names + paths).
    assert!(
        output.contains("Chosen tool: search_text"),
        "fused find must route a multi-term name/content matcher step; got:\n{output}"
    );

    // The merged envelope surfaces BOTH the matching file path AND the matching
    // symbol name across the two steps.
    assert!(
        output.contains("src/stel/planner.rs"),
        "fused result must surface the matching file path; got:\n{output}"
    );
    assert!(
        output.contains("route_find"),
        "fused result must surface the matching symbol name; got:\n{output}"
    );
}

// ─── (2) No NEW public tool name is introduced ───────────────────────────────

#[tokio::test]
async fn fused_find_introduces_no_new_tool_name() {
    let fx = Fixture::new(&stel_planner_corpus());
    let output = run_find(&fx.server, "stel planner find").await;

    // Every executed step must route through an existing search_* surface only.
    let chosen_tools: Vec<&str> = output
        .lines()
        .filter_map(|line| line.strip_prefix("Chosen tool: "))
        .map(str::trim)
        .collect();
    assert!(
        !chosen_tools.is_empty(),
        "fused find must execute at least one step; got:\n{output}"
    );
    for tool in &chosen_tools {
        assert!(
            matches!(*tool, "search_files" | "search_symbols" | "search_text"),
            "fused find must only use existing search_* surfaces, never a new tool; \
             saw `{tool}` in:\n{output}"
        );
    }
    // No invented fusion-specific tool name leaks into the surface.
    assert!(
        !output.contains("find_fusion") && !output.contains("fused_find"),
        "no new fusion tool name may appear in the STEL surface; got:\n{output}"
    );
}

// ─── (3) Co-change neighbours are boosted when a Ready store exists ──────────

#[tokio::test]
async fn fused_find_applies_cochange_boost_when_available() {
    // The executor resolves `src/stel/planner.rs` as the anchor and retargets
    // the path step to its basename stem `planner`, which clears the
    // basename-tier anchor-confidence floor (SF-006 stem promotion). The seeded
    // 3/3-shared-commit partner `src/stel/planner_helpers.rs` shares that stem,
    // so it is a path candidate the co-change boost can promote.
    let fx = Fixture::with_ready_coupling(
        &stel_planner_corpus(),
        &[row(
            "src/stel/planner.rs",
            "src/stel/planner_helpers.rs",
            3,
            11.0,
        )],
    );
    let output = run_find(&fx.server, "stel planner find").await;

    assert!(
        output.contains("decision: serve"),
        "fused find must serve; got:\n{output}"
    );
    // The co-change boost engaged for the resolved anchor.
    assert!(
        output.contains("co-change ranking applied")
            || output.contains("co-change signal: applied"),
        "fused find must apply the co-change boost when a Ready store exists; got:\n{output}"
    );
    // The boosted neighbour surfaces in the path side of the merged envelope.
    assert!(
        output.contains("src/stel/planner_helpers.rs"),
        "boosted co-change neighbour must appear in the merged result; got:\n{output}"
    );
}

// ─── (4) Graceful degradation when co-change evidence is unavailable ─────────

#[tokio::test]
async fn fused_find_degrades_to_path_name_ranking_without_cochange() {
    // Plain tempdir (no git repo) → coupling store never Ready. The route must
    // still return sensible path/name-ranked results with no error.
    let fx = Fixture::new(&stel_planner_corpus());
    let output = run_find(&fx.server, "stel planner find").await;

    assert!(
        output.contains("decision: serve"),
        "fused find must still serve without co-change evidence; got:\n{output}"
    );
    assert!(
        !output.contains("Index not loaded.") && !output.contains("Error:"),
        "co-change-unavailable degradation must not error; got:\n{output}"
    );
    // Pure path/name ranking still surfaces both surfaces.
    assert!(
        output.contains("src/stel/planner.rs"),
        "path ranking must still surface the file; got:\n{output}"
    );
    assert!(
        output.contains("route_find"),
        "name ranking must still surface the symbol; got:\n{output}"
    );
}

// ─── (5) Deterministic: identical state + query ⇒ identical ordering ─────────

#[tokio::test]
async fn fused_find_is_deterministic() {
    let fx = Fixture::new(&stel_planner_corpus());
    let first = run_find(&fx.server, "stel planner find").await;
    let second = run_find(&fx.server, "stel planner find").await;
    // Compare only the served result ORDERING (the contract's determinism
    // claim), excluding the trust envelope whose economics (session token
    // accumulation, plan_id timestamp) legitimately drift call-to-call. The
    // step bodies after the envelope carry the ranked path/symbol entries.
    let result_lines = |s: &str| -> Vec<String> {
        s.lines()
            .skip_while(|line| !line.starts_with("Step 1:"))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let first_body = result_lines(&first);
    let second_body = result_lines(&second);
    assert!(
        !first_body.is_empty(),
        "fused find must produce step bodies; got:\n{first}"
    );
    assert_eq!(
        first_body, second_body,
        "identical repo state + query must yield identical result ordering"
    );
}

/// Run a find query and return the machine-readable `outcome_class` string from
/// the result-status `_meta` (P3-7 asserts the fusion outcome metadata, not just
/// the body text that `run_find` returns).
async fn run_find_outcome(server: &SymForgeServer, query: &str) -> String {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");
    let request = symforge::stel::StelRequest {
        query: query.to_string(),
        intent: None,
        path: None,
        symbol: None,
        max_tokens: None,
        preview: None,
    };
    let params: Value = serde_json::to_value(symforge::stel::SymforgeCallInput {
        request,
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("serialize symforge params");
    let result = server
        .dispatch_tool_result_for_tests("symforge", params)
        .await
        .expect("symforge dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize CallToolResult");
    serialized["_meta"]["symforge/result_status"]["outcome_class"]
        .as_str()
        .expect("outcome_class in result-status meta")
        .to_string()
}

#[tokio::test]
async fn fused_find_with_both_surfaces_empty_reports_empty_result() {
    // P3-7: a multi-word find that fuses the path + name surfaces but matches
    // NOTHING on either must report the machine-readable `empty_result` outcome,
    // not a misleading `found`. Agents key on this status to know the union was
    // genuinely empty (instead of wasting tokens parsing a "successful" empty
    // envelope). Consistent with how plain search_text/search_files classify
    // "No matches".
    let fx = Fixture::new(&stel_planner_corpus());
    let outcome = run_find_outcome(&fx.server, "zzqwx_nomatch_aaa bbqzx_nomatch_ccc").await;
    assert_eq!(
        outcome, "empty_result",
        "both-empty fusion union must report empty_result, got: {outcome}"
    );
}
