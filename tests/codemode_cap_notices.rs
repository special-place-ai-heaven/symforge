#![cfg(feature = "server")]

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use symforge::process_util::hidden_command;
use tempfile::TempDir;

fn repo_with(files: &[(&str, String)]) -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    for (path, source) in files {
        let path = repo.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    repo
}

fn git(repo: &Path, args: &[&str]) {
    let status = hidden_command("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .current_dir(repo)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn committed(files: &[(&str, String)]) -> TempDir {
    let repo = repo_with(files);
    git(repo.path(), &["init", "-q", "-b", "main"]);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "base"]);
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

fn only_notice<'a>(output: &'a Value, op: &str) -> &'a Value {
    let found: Vec<&Value> = output["notices"]
        .as_array()
        .map(|items| items.iter().filter(|item| item["op"] == op).collect())
        .unwrap_or_default();
    assert_eq!(found.len(), 1, "{output}");
    found[0]
}

fn lines(count: usize, make: impl Fn(usize) -> String) -> String {
    (0..count).map(make).collect()
}

#[test]
fn symbol_context_counts_test_callers_hidden_by_the_default_filter() {
    let repo = repo_with(&[
        ("src/lib.rs", "pub fn only_tested() {}\n".to_owned()),
        (
            "tests/caller_test.rs",
            "use crate::only_tested;\nfn check() { only_tested(); }\n".to_owned(),
        ),
    ]);
    let output = run(
        &repo,
        r#"
hidden = symbol_context(path="src/lib.rs", name="only_tested")
shown = symbol_context(path="src/lib.rs", name="only_tested", include_tests=True)
[len(hidden["callers"]), len(shown["callers"])]
"#,
    );
    let shown = output["value"][1].as_u64().unwrap();
    assert!(shown >= 1, "{output}");
    assert_eq!(output["value"][0], 0, "{output}");
    assert_eq!(
        output["notices"],
        json!([{"call": 1, "op": "symbol_context", "omitted_rows": shown,
                "withheld_files": 0, "unparsed_files": 0}]),
        "{output}"
    );
}

#[test]
fn file_context_reports_consumers_and_imports_past_their_caps() {
    let mut files = vec![
        (
            "src/target.rs".to_owned(),
            "pub fn witness() {}\n".to_owned(),
        ),
        (
            "src/many_imports.rs".to_owned(),
            lines(51, |i| format!("use crate::m{i}::Item;\n")),
        ),
    ];
    for i in 0..31 {
        files.push((
            format!("src/c{i}.rs"),
            "use crate::target::witness;\npub fn call() { witness(); }\n".to_owned(),
        ));
    }
    let borrowed: Vec<(&str, String)> = files
        .iter()
        .map(|(path, source)| (path.as_str(), source.clone()))
        .collect();
    let repo = repo_with(&borrowed);
    let output = run(
        &repo,
        r#"
consumers = file_context(path="src/target.rs", sections=["consumers"])
imports = file_context(path="src/many_imports.rs", sections=["imports"])
[consumers["rendered"].count("\n"), imports["rendered"].count("\n")]
"#,
    );
    assert_eq!(output["value"][0], 30, "{output}");
    assert_eq!(output["value"][1], 50, "{output}");
    let rows: Vec<Value> = output["notices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| json!([n["call"], n["omitted_rows"]]))
        .collect();
    assert_eq!(rows, vec![json!([1, 1]), json!([2, 1])], "{output}");
}

#[test]
fn inspect_match_reports_siblings_past_the_limit() {
    let repo = repo_with(&[("src/wide.rs", lines(13, |i| format!("fn s{i}() {{}}\n")))]);
    let output = run(
        &repo,
        r#"len(inspect_match(path="src/wide.rs", line=1)["siblings"])"#,
    );
    let shown = output["value"].as_u64().unwrap();
    assert_eq!(shown, 10, "{output}");
    assert_eq!(
        only_notice(&output, "inspect_match")["omitted_rows"],
        13 - shown,
        "{output}"
    );
}

#[test]
fn repo_map_reports_a_budget_cut_compact_map_and_a_capped_full_map() {
    let files: Vec<(String, String)> = (0..200)
        .map(|i| (format!("d{i:03}/f.rs"), format!("pub struct S{i};\n")))
        .collect();
    let borrowed: Vec<(&str, String)> = files
        .iter()
        .map(|(path, source)| (path.as_str(), source.clone()))
        .collect();
    let repo = repo_with(&borrowed);
    let output = run(
        &repo,
        r#"
compact = repo_map()
full = repo_map(detail="full", max_files=3)
rows = compact["rendered"].split("\n")
dirs = len([r for r in rows if r.startswith("  d")])
types = len([r for r in rows if r.startswith("  struct ")])
[compact["total_files"], compact["shown_files"], full["total_files"], full["shown_files"], dirs, types]
"#,
    );
    let value = &output["value"];
    assert_eq!(
        json!([value[0], value[1], value[2], value[3]]),
        json!([200, 200, 200, 3]),
        "{output}"
    );
    let shown_rows = value[4].as_u64().unwrap() + value[5].as_u64().unwrap();
    assert!(value[4].as_u64().unwrap() < 200, "{output}");
    let notices = output["notices"].as_array().unwrap();
    assert_eq!(notices.len(), 2, "{output}");
    assert_eq!(notices[0]["omitted_rows"], 400 - shown_rows, "{output}");
    assert_eq!(notices[1]["omitted_rows"], 197, "{output}");
}

#[test]
fn symbol_context_bundle_marks_dependency_cap_and_keeps_recursive_receiver_calls() {
    let types = lines(16, |i| format!("pub struct T{i};\n"));
    let params = (0..16)
        .map(|i| format!("a{i}: T{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let repo = repo_with(&[(
        "src/lib.rs",
        format!(
            "{types}pub fn wide({params}) {{}}\npub struct Walker;\nimpl Walker {{\n    pub fn walk(&self) {{\n        self.walk();\n        self.walk();\n    }}\n}}\n"
        ),
    )]);
    let output = run(
        &repo,
        r#"
wide = symbol_context(path="src/lib.rs", name="wide", bundle=True)
walk = symbol_context(path="src/lib.rs", name="walk")
[len(wide["dependencies"]), len(walk["callees"])]
"#,
    );
    assert_eq!(output["value"], json!([15, 0]), "{output}");
    let notices = output["notices"].as_array().unwrap();
    assert_eq!(notices.len(), 2, "{output}");
    assert_eq!(notices[0]["omitted_rows_inexact"], true, "{output}");
    assert_eq!(notices[1]["omitted_rows"], 1, "{output}");
}

#[test]
fn detect_impact_reports_capped_lists_refused_reads_and_clamped_depth() {
    let repo = committed(&[("src/lib.rs", "// base\n".to_owned())]);
    fs::write(
        repo.path().join("src/lib.rs"),
        lines(201, |i| format!("pub fn changed{i}() {{}}\n")),
    )
    .unwrap();
    fs::write(
        repo.path().join("src/refused.rs"),
        format!("// -----BEGIN {} KEY-----\n", "PRIVATE"),
    )
    .unwrap();
    let output = run(
        &repo,
        r#"
capped = detect_impact()
deep = detect_impact(depth=9)
[len(capped["changed_symbols"]), len(deep["changed_symbols"])]
"#,
    );
    assert_eq!(output["value"], json!([200, 200]), "{output}");
    let notices = output["notices"].as_array().unwrap();
    assert_eq!(notices.len(), 2, "{output}");
    for notice in notices {
        assert_eq!(notice["omitted_rows"], 1, "{output}");
        assert_eq!(notice["withheld_files"], 1, "{output}");
    }
    assert!(notices[0].get("omitted_rows_inexact").is_none(), "{output}");
    assert_eq!(notices[1]["omitted_rows_inexact"], true, "{output}");
}

#[test]
fn search_files_resolve_reports_ambiguous_matches_past_the_cap() {
    let files: Vec<(String, String)> = (0..12)
        .map(|i| (format!("d{i}/same.rs"), "fn f() {}\n".to_owned()))
        .collect();
    let borrowed: Vec<(&str, String)> = files
        .iter()
        .map(|(path, source)| (path.as_str(), source.clone()))
        .collect();
    let repo = repo_with(&borrowed);
    let output = run(&repo, r#"len(search_files(query="same.rs", resolve=True))"#);
    let shown = output["value"].as_u64().unwrap();
    assert_eq!(shown, 10, "{output}");
    assert_eq!(
        only_notice(&output, "search_files")["omitted_rows"],
        12 - shown,
        "{output}"
    );
}

#[test]
fn symbol_diff_marks_same_name_symbols_as_inexact() {
    let source = |body: &str| {
        format!(
            "export function pick(a) {{ return a; }}\nexport function pick(a, b) {{ return {body}; }}\n"
        )
    };
    let repo = committed(&[("src/a.js", source("a"))]);
    fs::write(repo.path().join("src/a.js"), source("b")).unwrap();
    let output = run(
        &repo,
        r#"
diff = diff_symbols()
changed = what_changed(include_symbol_diff=True)
[diff is not None, changed is not None]
"#,
    );
    let notices = output["notices"].as_array().unwrap();
    assert_eq!(notices.len(), 2, "{output}");
    for notice in notices {
        assert_eq!(notice["omitted_rows_inexact"], true, "{output}");
    }
}

#[test]
fn calls_that_drop_nothing_raise_no_notice() {
    let repo = committed(&[
        (
            "src/lib.rs",
            "pub struct T0;\npub fn small(a: T0) {}\npub fn other() {}\n".to_owned(),
        ),
        ("d0/same.rs", "fn f() {}\n".to_owned()),
        (
            "src/with_tests.rs",
            "pub fn real() {}\nmod tests {\n    fn t() {}\n}\n".to_owned(),
        ),
        ("d1/same.rs", "fn f() {}\n".to_owned()),
        ("src/mid.rs", "use crate::T0;\npub fn mid() {}\n".to_owned()),
        (
            "src/top.rs",
            "use crate::mid::mid;\npub fn top() { mid(); }\n".to_owned(),
        ),
    ]);
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub struct T0;\npub fn small(a: T0) {}\npub fn other() { small(T0); }\npub fn added() {}\n",
    )
    .unwrap();
    let output = run(
        &repo,
        r##"
ctx = file_context(path="src/lib.rs")
outline = file_context(path="src/with_tests.rs")
found = inspect_match(path="src/lib.rs", line=2)
full = repo_map(detail="full")
resolved = search_files(query="same.rs", resolve=True)
impact = detect_impact()
diff = diff_symbols()
bundle = symbol_context(path="src/lib.rs", name="small", bundle=True)
linked = file_context(path="src/mid.rs")
compact = repo_map()
plain = symbol_context(path="src/lib.rs", name="small")
[len(found["siblings"]), full["shown_files"], len(resolved), len(impact["changed_symbols"]), len(bundle["dependencies"]),
 "Imports" in linked["rendered"], "Consumers" in linked["rendered"], compact["shown_files"], len(plain["callers"])]
"##,
    );
    assert_eq!(
        output["value"],
        json!([4, 6, 2, 2, 1, true, true, 6, 1]),
        "{output}"
    );
    assert_eq!(output.get("notices"), None, "{output}");
}

#[test]
fn compact_repo_map_counts_key_type_rows_cut_inside_their_section() {
    let types = lines(40, |i| format!("pub struct K{i:02}{};\n", "x".repeat(300)));
    let repo = repo_with(&[("src/lib.rs", types)]);
    let output = run(
        &repo,
        r#"
rows = repo_map()["rendered"].split("\n")
[len([r for r in rows if r.startswith("  src")]), len([r for r in rows if r.startswith("  struct ")])]
"#,
    );
    assert_eq!(output["value"][0], 1, "{output}");
    let shown_types = output["value"][1].as_u64().unwrap();
    assert!(shown_types > 0 && shown_types < 15, "{output}");
    assert_eq!(
        only_notice(&output, "repo_map")["omitted_rows"],
        40 - shown_types,
        "{output}"
    );
}
