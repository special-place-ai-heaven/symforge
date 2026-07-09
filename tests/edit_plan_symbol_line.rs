// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use symforge::live_index::LiveIndex;
use symforge::live_index::git_temporal::{
    CoChangeEntry, CommitSummary, GitFileHistory, GitTemporalIndex, GitTemporalState,
    GitTemporalStats,
};
use symforge::protocol::edit_plan::plan_edit;
use tempfile::TempDir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn build_index(source: &str) -> (TempDir, symforge::live_index::SharedIndex) {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_file(dir.path(), "src/lib.rs", source);
    let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
    (dir, shared)
}

/// A temporal snapshot with no history — mirrors the harness default for a
/// freshly-loaded tempdir index that has no real git data. `plan_edit` must omit
/// the co-change line cleanly for this state (graceful-omission guard).
fn empty_temporal() -> GitTemporalIndex {
    GitTemporalIndex {
        files: HashMap::new(),
        stats: GitTemporalStats {
            total_commits_analyzed: 0,
            analysis_window_days: 90,
            hotspots: vec![],
            most_coupled: vec![],
            computed_at: SystemTime::now(),
            compute_duration: Duration::ZERO,
        },
        state: GitTemporalState::Unavailable("not a git repo".to_string()),
    }
}

/// A `Ready` temporal seeded with a single strong co-change partner for `path`.
fn ready_temporal_with_cochange(path: &str, partner: &str) -> GitTemporalIndex {
    let history = GitFileHistory {
        commit_count: 6,
        churn_score: 0.8,
        last_commit: CommitSummary {
            hash: "abc1234".to_string(),
            timestamp: "2026-06-01T12:00:00Z".to_string(),
            author: "Tester".to_string(),
            message_head: "touch lib".to_string(),
            days_ago: 2.0,
        },
        contributors: vec![],
        co_changes: vec![CoChangeEntry {
            path: partner.to_string(),
            coupling_score: 0.62,
            shared_commits: 4,
        }],
        weak_co_changes: vec![],
    };

    GitTemporalIndex {
        files: HashMap::from([(path.to_string(), history)]),
        stats: GitTemporalStats {
            total_commits_analyzed: 12,
            analysis_window_days: 90,
            hotspots: vec![],
            most_coupled: vec![],
            computed_at: SystemTime::now(),
            compute_duration: Duration::ZERO,
        },
        state: GitTemporalState::Ready,
    }
}

fn canonical_symbol_line(index: &LiveIndex, path: &str, name: &str) -> u32 {
    let detail = index
        .capture_symbol_detail_view(path)
        .expect("fixture file should be indexed");
    let symbol = detail
        .symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("{name} should be indexed"));
    symbol.line_range.0 + 1
}

fn assert_plan_line_matches_selector(
    index: &LiveIndex,
    temporal: &GitTemporalIndex,
    name: &str,
    selector_line: u32,
) {
    let plan = plan_edit(index, temporal, &format!("src/lib.rs::{name}"));
    let expected = format!("{name} in src/lib.rs (lines {selector_line}-");
    assert!(
        plan.contains(&expected),
        "edit_plan should report the one-based selector line accepted by find_references\n\
         expected fragment: {expected:?}\n\
         actual plan:\n{plan}"
    );

    index
        .capture_find_references_view_for_symbol(
            "src/lib.rs",
            name,
            Some("fn"),
            Some(selector_line),
            Some("call"),
            10,
        )
        .unwrap_or_else(|error| {
            panic!("find_references should accept edit_plan's reported line: {error}")
        });
}

#[test]
fn edit_plan_symbol_lines_match_find_references_selectors() {
    let source = "\
fn caller() {
    documented_target();
    plain_target();
    let worker = Worker;
    worker.nested_target();
}

/// First doc line.
/// Second doc line.
fn documented_target() {}

fn plain_target() {}

struct Worker;

impl Worker {
    /// Method doc.
    fn nested_target(&self) {}
}
";

    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = empty_temporal();

    for (name, expected_line) in [
        ("documented_target", 10),
        ("plain_target", 12),
        ("nested_target", 18),
    ] {
        let selector_line = canonical_symbol_line(&index, "src/lib.rs", name);
        assert_eq!(selector_line, expected_line);
        assert_plan_line_matches_selector(&index, &temporal, name, selector_line);
    }
}

/// T019(ii): with no/Unavailable temporal (the harness default), the symbol
/// branch must NOT emit a `Co-change partners:` line — clean silent omission,
/// no error, no empty/placeholder line — while existing assertions still hold.
#[test]
fn edit_plan_omits_co_change_line_when_temporal_unavailable() {
    let source = "fn target() {}\n";
    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = empty_temporal();

    let plan = plan_edit(&index, &temporal, "src/lib.rs::target");

    assert!(
        plan.contains("target in src/lib.rs"),
        "plan should still locate the target symbol:\n{plan}"
    );
    assert!(
        !plan.contains("Co-change partners:"),
        "plan must omit the co-change line when temporal is Unavailable:\n{plan}"
    );
}

/// T019(i): with a `Ready` temporal that has a strong co-change partner for the
/// target's file, the symbol branch must emit a single `Co-change partners:`
/// line listing the partner.
#[test]
fn edit_plan_emits_co_change_line_when_temporal_ready() {
    let source = "fn target() {}\n";
    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = ready_temporal_with_cochange("src/lib.rs", "src/routes.rs");

    let plan = plan_edit(&index, &temporal, "src/lib.rs::target");

    assert!(
        plan.contains("Co-change partners: src/routes.rs"),
        "plan should emit the terse co-change line listing the partner:\n{plan}"
    );
    // Terse line: a single comma-joined list, no coupling scores or block header.
    assert!(
        !plan.contains("coupling:"),
        "the plan_edit co-change line must be terse (no coupling scores):\n{plan}"
    );
}

#[test]
fn edit_plan_resolves_qualified_impl_method_selector() {
    let source = "\
struct PolicyEngine;

impl PolicyEngine {
    pub fn new() -> Self {
        PolicyEngine
    }
}
";
    let dir = TempDir::new().expect("tempdir");
    write_file(dir.path(), "crates/daemon/src/policy.rs", source);
    let shared = LiveIndex::load(dir.path()).expect("load index");
    let index = shared.read();
    let temporal = empty_temporal();

    let plan = plan_edit(
        &index,
        &temporal,
        "crates/daemon/src/policy.rs::PolicyEngine::new",
    );

    assert!(
        !plan.contains("not found"),
        "qualified impl method selector should resolve\n{plan}"
    );
    assert!(
        plan.contains("new in crates/daemon/src/policy.rs"),
        "plan should name the resolved bare method\n{plan}"
    );
}

// ---------------------------------------------------------------------------
// P1 (US1): `Type::method` selector resolution.
//
// `edit_plan("GitRepo::tracked_paths")` must resolve to the method defined on
// that type — the SAME symbol the bare name and `file::symbol` forms resolve
// to — without a file-path prefix. These tests fail on the pre-fix code (a
// bare type name matches no indexed file path, so the method is never
// searched and the tool answers "not found").
// ---------------------------------------------------------------------------

/// T002: a `Type::method` selector with a unique method resolves to the same
/// symbol as the bare-name selector.
#[test]
fn edit_plan_resolves_bare_type_method_selector() {
    let source = "\
struct GitRepo;

impl GitRepo {
    fn tracked_paths(&self) -> Vec<String> {
        Vec::new()
    }
}
";
    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = empty_temporal();

    let type_plan = plan_edit(&index, &temporal, "GitRepo::tracked_paths");
    let bare_plan = plan_edit(&index, &temporal, "tracked_paths");

    let line = canonical_symbol_line(&index, "src/lib.rs", "tracked_paths");
    let expected = format!("tracked_paths in src/lib.rs (lines {line}-");

    assert!(
        type_plan.contains(&expected),
        "Type::method must resolve to the same symbol as the bare name\n\
         expected fragment: {expected:?}\nplan:\n{type_plan}"
    );
    assert!(
        bare_plan.contains(&expected),
        "bare-name selector must resolve to the same symbol\n\
         expected fragment: {expected:?}\nplan:\n{bare_plan}"
    );
}

/// T003: when a method name is shared across types, `Type::method` resolves to
/// the method on the NAMED type only (disambiguation), never another type's.
#[test]
fn edit_plan_disambiguates_shared_method_name_by_type() {
    let source = "\
struct Alpha;

impl Alpha {
    fn new() -> Self {
        Alpha
    }
}

struct Beta;

impl Beta {
    fn new() -> Self {
        Beta
    }
}
";
    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = empty_temporal();

    // Alpha::new is at source line 4, Beta::new at source line 12.
    let alpha_plan = plan_edit(&index, &temporal, "Alpha::new");
    let beta_plan = plan_edit(&index, &temporal, "Beta::new");

    assert!(
        alpha_plan.contains("Found 1 symbol(s)")
            && alpha_plan.contains("new in src/lib.rs (lines 4-"),
        "Alpha::new must resolve to Alpha's `new` (line 4) only\n{alpha_plan}"
    );
    assert!(
        !alpha_plan.contains("(lines 12-"),
        "Alpha::new must NOT resolve to Beta's `new` (line 12)\n{alpha_plan}"
    );
    assert!(
        beta_plan.contains("Found 1 symbol(s)")
            && beta_plan.contains("new in src/lib.rs (lines 12-"),
        "Beta::new must resolve to Beta's `new` (line 12) only\n{beta_plan}"
    );
    assert!(
        !beta_plan.contains("(lines 4-"),
        "Beta::new must NOT resolve to Alpha's `new` (line 4)\n{beta_plan}"
    );
}

/// T004: pre-existing selector forms (bare name, `file::symbol`, plain file
/// path) resolve exactly as before — regression guard, passes now and after.
#[test]
fn edit_plan_preserves_existing_selector_forms() {
    let source = "\
struct Repo;

impl Repo {
    fn scan(&self) {}
}
";
    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = empty_temporal();

    let line = canonical_symbol_line(&index, "src/lib.rs", "scan");
    let expected = format!("scan in src/lib.rs (lines {line}-");

    let bare = plan_edit(&index, &temporal, "scan");
    assert!(
        bare.contains(&expected),
        "bare-name selector regressed:\n{bare}"
    );

    let file_sym = plan_edit(&index, &temporal, "src/lib.rs::scan");
    assert!(
        file_sym.contains(&expected),
        "file::symbol selector regressed:\n{file_sym}"
    );

    let file_only = plan_edit(&index, &temporal, "src/lib.rs");
    assert!(
        file_only.contains("Found file: src/lib.rs"),
        "plain file-path selector regressed:\n{file_only}"
    );
}

/// T005: a `Type::method` whose method does not exist on that type returns a
/// truthful not-found that names what was searched — never a wrong hit.
#[test]
fn edit_plan_type_method_nonexistent_is_truthful_not_found() {
    let source = "\
struct GitRepo;

impl GitRepo {
    fn tracked_paths(&self) {}
}
";
    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = empty_temporal();

    let plan = plan_edit(&index, &temporal, "GitRepo::does_not_exist");

    assert!(
        plan.contains("not found"),
        "a nonexistent Type::method must report not-found:\n{plan}"
    );
    assert!(
        plan.contains("GitRepo::does_not_exist"),
        "the not-found result must name what was searched:\n{plan}"
    );
    assert!(
        !plan.contains("tracked_paths in src/lib.rs"),
        "must not resolve to an unrelated method on the same type:\n{plan}"
    );
}

/// FR-004 guard: a free function that merely shares a file with `impl X` is NOT
/// a method of `X`. `GitRepo::head_sha` (head_sha is a free fn) must be a
/// truthful not-found, while the bare `head_sha` selector still resolves. This
/// documents why SC-001's `GitRepo::head_sha` correctly stays not-found:
/// forcing it would be a wrong hit. Passes now and after the fix.
#[test]
fn edit_plan_type_method_does_not_match_free_function() {
    let source = "\
struct GitRepo;

impl GitRepo {
    fn tracked_paths(&self) {}
}

fn head_sha() -> String {
    String::new()
}
";
    let (_dir, shared) = build_index(source);
    let index = shared.read();
    let temporal = empty_temporal();

    let qualified = plan_edit(&index, &temporal, "GitRepo::head_sha");
    assert!(
        qualified.contains("not found"),
        "a free function is not a Type method; GitRepo::head_sha must be not-found:\n{qualified}"
    );

    let bare = plan_edit(&index, &temporal, "head_sha");
    let line = canonical_symbol_line(&index, "src/lib.rs", "head_sha");
    assert!(
        bare.contains(&format!("head_sha in src/lib.rs (lines {line}-")),
        "the bare free-function selector must still resolve:\n{bare}"
    );
}
