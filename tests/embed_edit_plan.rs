#![cfg(feature = "embed")]
//! MCP parity for `edit_plan`: the embedded `QueryRequest::EditPlan` renders
//! the shared planner's text for symbol, `path::name`, file and missing
//! targets. The server lib test `edit_plan_matches_embed_parity_golden` in
//! `src/protocol/tools.rs` asserts the same goldens against MCP output.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

/// Shared with the MCP golden.
const SYMBOL_GOLDEN: &str = "── Edit Plan ──\nFound 1 symbol(s) matching 'target':\n  Function target in src/lib.rs (lines 1-1)\n\nReferences: 1 call sites across the project\n\nSuggested tool sequence:\n  1. get_symbol_context(name=\"target\", path=\"src/lib.rs\", bundle=true) — understand full context\n  2. Choose edit approach:\n     - Small change: edit_within_symbol(path=\"src/lib.rs\", name=\"target\", old_text=..., new_text=...)\n     - Full rewrite: replace_symbol_body(path=\"src/lib.rs\", name=\"target\", new_body=...)\n     - Rename: batch_rename(path=\"src/lib.rs\", name=\"target\", new_name=..., dry_run=true)\n     - Delete: delete_symbol(path=\"src/lib.rs\", name=\"target\", dry_run=true)\n  3. analyze_file_impact(path=\"src/lib.rs\") — verify changes";
const QUALIFIED_GOLDEN: &str = "── Edit Plan ──\nFound 1 symbol(s) matching 'src/lib.rs::target':\n  Function target in src/lib.rs (lines 1-1)\n\nReferences: 1 call sites across the project\n\nSuggested tool sequence:\n  1. get_symbol_context(name=\"target\", path=\"src/lib.rs\", bundle=true) — understand full context\n  2. Choose edit approach:\n     - Small change: edit_within_symbol(path=\"src/lib.rs\", name=\"target\", old_text=..., new_text=...)\n     - Full rewrite: replace_symbol_body(path=\"src/lib.rs\", name=\"target\", new_body=...)\n     - Rename: batch_rename(path=\"src/lib.rs\", name=\"target\", new_name=..., dry_run=true)\n     - Delete: delete_symbol(path=\"src/lib.rs\", name=\"target\", dry_run=true)\n  3. analyze_file_impact(path=\"src/lib.rs\") — verify changes";
const FILE_GOLDEN: &str = "── Edit Plan ──\nFound file: notes.md\n\nSuggested approach:\n  1. get_file_context(path=\"notes.md\", sections=[\"outline\"]) — understand structure\n  2. get_symbol(path=\"notes.md\", name=\"<target>\") — read specific symbols\n  3. Use edit_within_symbol or replace_symbol_body for changes\n  4. analyze_file_impact(path=\"notes.md\") — verify";
const MISSING_GOLDEN: &str = "── Edit Plan ──\nTarget 'does_not_exist' not found.\nTry: search_symbols(query=\"...\") to find the correct name.";

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

fn plan(handle: &EmbeddedSourceHandle, target: &str) -> String {
    let request = QueryRequest::EditPlan {
        target: target.into(),
    };
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        match handle.query(&request, QueryLimits::default()) {
            Ok(claim) => match claim.value() {
                QueryOutput::EditPlan(plan) => {
                    assert_eq!(plan.target, target);
                    return plan.rendered.clone();
                }
                other => panic!("unexpected output: {other:?}"),
            },
            Err(refusal) if refusal.kind() == QueryRefusalKind::SourceUnavailable => {
                assert!(Instant::now() < until, "source never returned to Current");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(refusal) => panic!("unexpected refusal: {refusal:?}"),
        }
    }
}

#[test]
fn edit_plan_renders_the_mcp_plan_for_symbol_file_and_missing_targets() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn target() {}\n\npub fn caller() {\n    target();\n}\n",
    )
    .unwrap();
    fs::write(root.path().join("notes.md"), "# Notes\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());

    assert_eq!(plan(&handle, "target"), SYMBOL_GOLDEN);
    assert_eq!(plan(&handle, "src/lib.rs::target"), QUALIFIED_GOLDEN);
    assert_eq!(plan(&handle, "notes.md"), FILE_GOLDEN);
    assert_eq!(plan(&handle, "does_not_exist"), MISSING_GOLDEN);
}

#[test]
fn edit_plan_refuses_a_budget_that_cannot_hold_the_plan() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn target() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let refusal = handle
        .query(
            &QueryRequest::EditPlan {
                target: "target".into(),
            },
            QueryLimits {
                max_results: 100,
                max_bytes: 8,
            },
        )
        .unwrap_err();
    assert_eq!(refusal.kind(), QueryRefusalKind::BudgetTooSmall);
}
