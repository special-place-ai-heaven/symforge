#![cfg(feature = "server")]

use std::fs;

use serde_json::{Value, json};
use symforge::process_util::hidden_command;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    // Header only: deliberately no key or other credential bytes.
    let envelope = format!("-----BEGIN {} KEY-----\n", "PRIVATE");
    let rust_marker = format!("// {envelope}");
    let python_marker = format!("# {envelope}");
    let files = [
        ("src/target.rs", "pub fn witness_target() {}\n"),
        (
            "src/a.rs",
            "use crate::target;\npub fn witness_a() {\n    target::witness_target(); // witness_marker a1\n    target::witness_target(); // witness_marker a2\n}\n",
        ),
        (
            "src/b.rs",
            "use crate::target;\npub fn witness_b() {\n    target::witness_target(); // witness_marker b1\n}\n",
        ),
        ("src/withheld_fixture.rs", rust_marker.as_str()),
        ("other/withheld_outside.rs", rust_marker.as_str()),
        ("src/withheld_language.py", python_marker.as_str()),
        ("src/vendor/withheld_vendor.rs", rust_marker.as_str()),
        ("src/tests/withheld_test.rs", rust_marker.as_str()),
    ];
    for (path, source) in files {
        let path = repo.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    repo
}

fn run(repo: &TempDir, script: &str) -> Value {
    let home = tempfile::tempdir().unwrap();
    let output = hidden_command(env!("CARGO_BIN_EXE_symforge"))
        .args(["code", "run", "-c", script, "--root"])
        .arg(repo.path())
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn notice(call: usize, op: &str, omitted_rows: usize, withheld_files: usize) -> Value {
    json!({"call": call, "op": op, "omitted_rows": omitted_rows,
           "withheld_files": withheld_files, "unparsed_files": 0})
}

#[test]
fn transformed_script_keeps_call_order_and_exact_symbol_reference_and_dependent_counts() {
    let repo = fixture();
    let output = run(
        &repo,
        r#"
syms = search_symbols(query="witness_", path_prefix="src", limit=1)
refs = find_references(name="witness_target", kind="call", limit=1, max_per_file=1)
deps = find_dependents(path="src/target.rs", name="witness_target", limit=1, max_per_file=1)
{"symbol_names": [s["name"] for s in syms], "lens": [len(refs), len(deps)]}
"#,
    );
    assert_eq!(output["value"]["lens"], json!([1, 1]));
    assert_eq!(output["value"]["symbol_names"].as_array().unwrap().len(), 1);
    assert_eq!(
        output["notices"],
        json!([
            notice(1, "search_symbols", 2, 2),
            notice(2, "find_references", 2, 5),
            notice(3, "find_dependents", 2, 4),
        ])
    );
}

#[test]
fn empty_and_count_searches_report_scoped_catalog_withholding_without_paths() {
    let repo = fixture();
    let output = run(
        &repo,
        r#"
miss = search_text(query="no_fixture_match", path_prefix="src", language="rust")
counts = search_text(query="witness_marker", group_by="count", path_prefix="src", language="rust", limit=1, max_per_file=1)
{"empty": miss, "counts": counts}
"#,
    );
    assert_eq!(
        output["value"],
        json!({"empty": [], "counts": [{"path": "src/a.rs", "count": 2}]})
    );
    assert_eq!(
        output["notices"],
        json!([
            notice(1, "search_text", 0, 1),
            notice(2, "search_text", 1, 1),
        ])
    );
    assert!(!output.to_string().contains("withheld_fixture.rs"));
}

#[test]
fn line_and_structural_caps_include_rows_in_files_after_the_limit() {
    let repo = fixture();
    let output = run(
        &repo,
        r#"
lines = search_text(query="witness_marker", path_prefix="src", language="rust", limit=1, max_per_file=1)
structural = search_text(query="target::witness_target()", structural=True, path_prefix="src", language="rust", limit=1, max_per_file=1)
[len(lines), len(structural)]
"#,
    );
    assert_eq!(output["value"], json!([1, 1]));
    assert_eq!(
        output["notices"],
        json!([
            notice(1, "search_text", 2, 1),
            notice(2, "search_text", 2, 1),
        ])
    );
}

#[test]
fn structural_scan_cap_keeps_a_lower_bound_and_never_emits_partial_file_counts() {
    let repo = fixture();
    fs::write(
        repo.path().join("src/dense.rs"),
        format!(
            "fn dense() {{\n{}}}\n",
            "    target::witness_target();\n".repeat(1025)
        ),
    )
    .unwrap();
    let output = run(
        &repo,
        r#"
lines = search_text(query="target::witness_target()", structural=True, path_prefix="src", language="rust", limit=1, max_per_file=1)
counts = search_text(query="target::witness_target()", structural=True, group_by="count", path_prefix="src", language="rust", limit=1)
[len(lines), counts]
"#,
    );
    assert_eq!(
        output["value"],
        json!([1, [{"path": "src/a.rs", "count": 2}]])
    );
    let mut line_notice = notice(1, "search_text", 2, 1);
    let mut count_notice = notice(2, "search_text", 1, 1);
    for item in [&mut line_notice, &mut count_notice] {
        item["unparsed_files"] = json!(1);
        item["omitted_rows_inexact"] = json!(true);
    }
    assert_eq!(output["notices"], json!([line_notice, count_notice]));
    assert!(!output.to_string().contains("dense.rs"));
}

#[test]
fn scalar_and_bare_empty_list_both_keep_notices() {
    let repo = fixture();
    let empty = run(
        &repo,
        r#"search_text(query="no_fixture_match", path_prefix="src", language="rust")"#,
    );
    assert_eq!(empty["value"], json!([]));
    assert_eq!(empty["notices"], json!([notice(1, "search_text", 0, 1)]));
    let scalar = run(
        &repo,
        r#"len(search_text(query="witness_marker", path_prefix="src", language="rust", limit=1))"#,
    );
    assert_eq!(scalar["value"], 1);
    assert_eq!(scalar["notices"], json!([notice(1, "search_text", 2, 1)]));
}

#[test]
fn no_visible_relationships_do_not_claim_the_corpus_was_searched() {
    let repo = fixture();
    let output = run(
        &repo,
        r#"
refs = find_references(name="no_fixture_match", kind="call")
impls = find_references(name="NoFixtureTrait", mode="implementations")
deps = find_dependents(path="src/target.rs", name="no_fixture_match")
[len(refs), len(impls), len(deps)]
"#,
    );
    assert_eq!(output["value"], json!([0, 0, 0]));
    assert_eq!(
        output["notices"],
        json!([
            notice(1, "find_references", 0, 5),
            notice(2, "find_references", 0, 5),
            notice(3, "find_dependents", 0, 4),
        ])
    );
    assert!(!output.to_string().contains("withheld_fixture.rs"));
}

#[test]
fn q_keeps_rows_and_reports_completeness_on_json_stdout() {
    let repo = fixture();
    let home = tempfile::tempdir().unwrap();
    let output = hidden_command(env!("CARGO_BIN_EXE_symforge"))
        .args([
            "q",
            "search_text",
            r#"{"query":"no_fixture_match","path_prefix":"src","language":"rust"}"#,
            "--root",
        ])
        .arg(repo.path())
        .env("SYMFORGE_NO_DAEMON", "1")
        .env("SYMFORGE_HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let output: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output["value"], json!([]));
    assert_eq!(output["notices"], json!([notice(1, "search_text", 0, 1)]));
}
