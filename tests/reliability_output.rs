//! Feature 036 slice A1 (User Story 2): what the caller asked for is in the answer.
//!
//! Written before the change (tasks.md T005): every test here is expected to fail
//! against the 11.11.0 behaviour, for the reason its assertion message gives. The
//! fixture and its answer table are `tests/fixtures/reliability/` and `ANSWERS.md`.
#![cfg(feature = "server")]

mod reliability_support;

use reliability_support::{
    call, committed_fixture_copy, fixture_server, git, numbered_lines, repo_server, server_for,
};
use serde_json::json;

/// FR-024, scenario US2.1: an explicit 75-line range arrives whole and numbered with no
/// budget argument (the default budget derived from the input size must not cut it).
#[tokio::test]
async fn explicit_range_of_75_lines_is_returned_whole_and_numbered() {
    let server = fixture_server();
    let text = call(
        &server,
        "get_file_content",
        json!({ "path": "src/long.rs", "start_line": 1, "end_line": 75 }),
    )
    .await;
    // Lines 6..=75 carry the marker. Whole first, then numbered, so a cut and a missing
    // gutter are told apart.
    let marked = text.matches("LONG_LINE_").count();
    assert_eq!(
        marked, 70,
        "range was cut: {marked} of 70 marked lines\n{text}"
    );
    assert!(
        text.contains("LONG_LINE_0075"),
        "last requested line missing\n{text}"
    );
    let numbers = numbered_lines(&text);
    assert_eq!(
        numbers,
        (1..=75).collect::<Vec<u32>>(),
        "expected 75 numbered lines 1..=75, got {} numbered lines\n{text}",
        numbers.len()
    );
}

/// FR-027, scenario US2.5: a range past the end of the file states the file's last line.
#[tokio::test]
async fn range_past_end_of_file_states_the_last_line() {
    let server = fixture_server();
    let text = call(
        &server,
        "get_file_content",
        json!({ "path": "src/short.rs", "start_line": 1, "end_line": 20 }),
    )
    .await;
    assert_eq!(
        numbered_lines(&text),
        (1..=8).collect::<Vec<u32>>(),
        "expected exactly the 8 numbered lines of the file\n{text}"
    );
    assert!(
        text.contains("file ends at line 8"),
        "answer must state `file ends at line 8`\n{text}"
    );
}

/// FR-024: a named symbol body longer than the default budget is returned whole.
#[tokio::test]
async fn symbol_body_longer_than_the_default_budget_is_whole() {
    let server = fixture_server();
    let text = call(
        &server,
        "get_symbol",
        json!({ "path": "src/long.rs", "name": "long_body" }),
    )
    .await;
    for token in ["LONG_LINE_0006", "LONG_LINE_0150", "LONG_LINE_0298"] {
        assert!(text.contains(token), "body cut before {token}\n{text}");
    }
    assert!(
        text.trim_end().ends_with('}'),
        "body must end with its closing brace\n{text}"
    );
}

/// FR-004/FR-024: the typical file-context call carries outline, imports and consumers.
#[tokio::test]
async fn file_context_contains_outline_imports_and_consumers() {
    let server = fixture_server();
    let text = call(
        &server,
        "get_file_context",
        json!({ "path": "src/parent/leaf.rs" }),
    )
    .await;
    for name in ["target_fn", "calls_locally", "shadowed"] {
        assert!(text.contains(name), "outline lacks {name}\n{text}");
    }
    assert!(
        text.contains("std::cmp::max") || text.contains("cmp::max"),
        "imports missing\n{text}"
    );
    assert!(
        text.contains("src/user.rs"),
        "consumers missing src/user.rs\n{text}"
    );
    assert!(
        text.contains("src/parent/mod.rs"),
        "consumers missing the module declaration in src/parent/mod.rs\n{text}"
    );
}

/// Scenario US2.2: symbol context names every caller and every callee.
#[tokio::test]
async fn symbol_context_names_every_caller_and_callee() {
    let server = fixture_server();
    let text = call(
        &server,
        "get_symbol_context",
        json!({ "name": "target_fn", "path": "src/parent/leaf.rs", "symbol_kind": "fn" }),
    )
    .await;
    for callee in ["step_a", "step_b", "step_c", "step_d", "step_e"] {
        assert!(text.contains(callee), "callee {callee} not named\n{text}");
    }
    for caller in ["calls_locally", "run_parent", "via_super", "use_it"] {
        assert!(text.contains(caller), "caller {caller} not named\n{text}");
    }
}

/// Scenario US2.3 and FR-028: with defaults, the repository overview ranks the source
/// directories ahead of documentation, research and specification directories. Runs on this
/// repository, where those directories hold far more symbols than `src/` subtrees do.
#[tokio::test]
async fn repository_map_with_defaults_ranks_source_before_documentation() {
    let server = repo_server();
    let text = call(&server, "get_repo_map", json!({})).await;
    let first_row = |is_wanted: &dyn Fn(&str) -> bool| {
        text.lines().position(|line| is_wanted(line.trim_start()))
    };
    let src =
        first_row(&|row| row.starts_with("src ") || row.starts_with("src/")).unwrap_or_else(|| {
            panic!(
                "default map never reaches src
{text}"
            )
        });
    let docs = first_row(&|row| {
        ["docs", "research", "specs"]
            .iter()
            .any(|dir| row.starts_with(&format!("{dir}/")) || row.starts_with(&format!("{dir} ")))
    })
    .unwrap_or(usize::MAX);
    assert!(
        src < docs,
        "documentation row (line {docs}) ranks ahead of the first src row (line {src})
{text}"
    );
}

/// FR-028: `path` and `depth` are honoured at the default detail level.
/// Control: the tree detail level, which documents both arguments, lists the same level.
#[tokio::test]
async fn repository_map_honours_path_and_depth() {
    let server = fixture_server();
    let control = call(
        &server,
        "get_repo_map",
        json!({ "detail": "tree", "path": "src", "depth": 1 }),
    )
    .await;
    assert!(
        control.contains("parent"),
        "control (tree mode) must list src/parent
{control}"
    );
    assert!(
        !control.contains("leaf.rs"),
        "control (tree mode) must stop at depth 1
{control}"
    );

    let text = call(
        &server,
        "get_repo_map",
        json!({ "path": "src", "depth": 1 }),
    )
    .await;
    assert!(
        text.contains("parent"),
        "scoped map must list src/parent
{text}"
    );
    let outside = text
        .lines()
        .map(str::trim_start)
        .find(|row| row.starts_with("docs") || row.starts_with("(root)"));
    assert!(
        outside.is_none(),
        "path=src must exclude {outside:?}, which is outside it
{text}"
    );
}

/// FR-030, scenario US5.3: a multi-word query with no exact match returns the symbols
/// containing all the words and says which rule produced them.
#[tokio::test]
async fn multi_word_symbol_query_returns_all_words_matches_and_says_so() {
    let server = fixture_server();
    let text = call(&server, "search_symbols", json!({ "query": "long body" })).await;
    assert!(
        text.contains("long_body"),
        "no all-words match for `long body`\n{text}"
    );
    let lower = text.to_lowercase();
    assert!(
        lower.contains("all words") || lower.contains("all-words"),
        "answer must name the all-words rule\n{text}"
    );
}

/// FR-029: a term found only in documentation yields a count and the call that shows it.
/// Control: a term present in code is found.
#[tokio::test]
async fn text_search_with_only_documentation_matches_reports_them() {
    let server = fixture_server();
    let control = call(&server, "search_text", json!({ "query": "LONG_FIRST" })).await;
    assert!(
        control.contains("src/long.rs"),
        "control: code match must be found\n{control}"
    );

    let text = call(
        &server,
        "search_text",
        json!({ "query": "zebrafish_quorum" }),
    )
    .await;
    let lower = text.to_lowercase();
    assert!(
        lower.contains("1 match") && lower.contains("documentation"),
        "empty result must count documentation matches (`1 match ... documentation`)\n{text}"
    );
    assert!(
        text.contains("search_text(") && text.contains("docs"),
        "empty result must give the literal call that shows them\n{text}"
    );
}

/// FR-031: the change summary counts paths as `git status --porcelain` does (an untracked
/// directory is one entry).
#[tokio::test]
async fn what_changed_path_count_equals_git_status() {
    let repo = committed_fixture_copy();
    std::fs::write(
        repo.path().join("src/user.rs"),
        "use crate::parent::leaf::target_fn;\n\npub fn use_it() -> u32 {\n    target_fn(8)\n}\n",
    )
    .expect("modify tracked file");
    std::fs::create_dir_all(repo.path().join("src/fresh")).expect("mkdir");
    std::fs::write(repo.path().join("src/fresh/a.rs"), "pub fn a() {}\n").expect("write a");
    std::fs::write(repo.path().join("src/fresh/b.rs"), "pub fn b() {}\n").expect("write b");
    let porcelain = git(repo.path(), &["status", "--porcelain"]);
    let expected = porcelain.lines().count();
    assert_eq!(
        expected, 2,
        "setup: one modified file and one untracked directory\n{porcelain}"
    );

    let server = server_for(repo.path());
    let text = call(
        &server,
        "what_changed",
        json!({ "uncommitted": true, "code_only": false }),
    )
    .await;
    let reported = text
        .lines()
        .filter(|l| l.trim_start().starts_with("src/"))
        .count();
    assert_eq!(
        reported, expected,
        "reported {reported} paths, `git status --porcelain` shows {expected}\n{text}"
    );
}

/// FR-032: an impact answer ranks affected items so that not all share one level.
#[tokio::test]
async fn impact_levels_are_not_all_equal() {
    let repo = committed_fixture_copy();
    std::fs::write(
        repo.path().join("src/parent/leaf.rs"),
        std::fs::read_to_string(repo.path().join("src/parent/leaf.rs"))
            .expect("read leaf")
            .replace(
                "step_e(step_d(step_c(step_b(step_a(x)))))",
                "step_e(step_d(step_c(step_b(step_a(x + 1)))))",
            ),
    )
    .expect("modify target_fn");
    let server = server_for(repo.path());
    let text = call(
        &server,
        "detect_impact",
        json!({ "base_branch": "main", "depth": 3 }),
    )
    .await;
    let payload = text
        .split("--- impact payload ---")
        .nth(1)
        .unwrap_or_else(|| {
            panic!(
                "no impact payload
{text}"
            )
        });
    let payload: serde_json::Value = serde_json::from_str(payload.trim()).unwrap_or_else(|e| {
        panic!(
            "impact payload is not JSON: {e}
{text}"
        )
    });
    let nodes = payload["blast_radius"]
        .as_array()
        .expect("blast_radius array");
    assert!(
        nodes.len() >= 3,
        "expected callers at two hops, got {}
{text}",
        nodes.len()
    );
    let risks: std::collections::BTreeSet<&str> =
        nodes.iter().filter_map(|n| n["risk"].as_str()).collect();
    assert!(
        risks.len() > 1,
        "all {} affected items share the level {risks:?}; impact must rank them
{text}",
        nodes.len()
    );
}
