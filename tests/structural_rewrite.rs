//! `batch_edit` structural mode (`rewrite`): ast-grep pattern + template over
//! the indexed files in scope, through the ordinary batch edit pipeline.
#![cfg(feature = "server")]

use std::path::Path;

use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;

const FILES: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        "pub fn run() {\n    old_call(1);\n    old_call(2);\n}\n",
    ),
    (
        "web/app.ts",
        "export function go() {\n  return old_call(3);\n}\n",
    ),
    ("py/tool.py", "def go():\n    return old_call(4)\n"),
    ("docs/notes.md", "old_call(5) stays in prose\n"),
];

fn fixture_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git2::Repository::init(dir.path()).expect("git init");
    std::fs::write(dir.path().join(".gitignore"), ".symforge/\n").expect("gitignore");
    for (path, content) in FILES {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).expect("parent");
        std::fs::write(file, content).expect("file");
    }
    dir
}

fn server_for(root: &Path) -> SymForgeServer {
    SymForgeServer::new(
        LiveIndex::load(root).expect("index"),
        "structural-rewrite".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    )
}

async fn call(server: &SymForgeServer, input: Value) -> (String, bool) {
    let result = server
        .dispatch_tool_result_for_tests("batch_edit", input)
        .await
        .expect("batch_edit dispatch");
    let result = serde_json::to_value(&result).unwrap();
    (
        result["content"][0]["text"]
            .as_str()
            .expect("text answer")
            .to_string(),
        result["isError"] == json!(true),
    )
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).expect("fixture file")
}

fn unchanged(root: &Path) {
    for (path, content) in FILES {
        assert_eq!(&read(root, path), content, "{path} must be untouched");
    }
}

fn rewrite(pattern: &str, template: &str) -> Value {
    json!({ "pattern": pattern, "template": template })
}

/// Per-file summary lines of a batch answer, without the dry-run prefix.
fn summary_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.contains("rewrote"))
        .map(|line| line.trim_start_matches("[DRY RUN] Would ").to_string())
        .collect()
}

#[tokio::test]
async fn rewrite_spans_rust_typescript_and_python_and_dry_run_reports_the_real_run() {
    let repo = fixture_repo();
    let root = repo.path();
    let server = server_for(root);
    let request = json!({ "rewrite": rewrite("old_call($A)", "new_call($A, 0)"), "dry_run": true });

    let (dry, dry_error) = call(&server, request.clone()).await;
    assert!(!dry_error, "{dry}");
    unchanged(root);
    assert!(dry.contains("[DRY RUN] Would "), "{dry}");
    assert!(
        dry.contains("4 structural match(es) across 3 file(s)"),
        "{dry}"
    );

    let mut apply = request;
    apply["dry_run"] = json!(false);
    let (real, real_error) = call(&server, apply).await;
    assert!(!real_error, "{real}");
    assert_eq!(
        summary_lines(&dry),
        summary_lines(&real),
        "{dry}\n---\n{real}"
    );
    assert_eq!(summary_lines(&real).len(), 3, "{real}");

    assert_eq!(
        read(root, "src/lib.rs"),
        "pub fn run() {\n    new_call(1, 0);\n    new_call(2, 0);\n}\n"
    );
    assert_eq!(
        read(root, "web/app.ts"),
        "export function go() {\n  return new_call(3, 0);\n}\n"
    );
    assert_eq!(
        read(root, "py/tool.py"),
        "def go():\n    return new_call(4, 0)\n"
    );
    assert_eq!(
        read(root, "docs/notes.md"),
        FILES[3].1,
        "prose is not rewritten"
    );

    // The index was refreshed: a second identical rewrite finds nothing.
    let (again, again_error) = call(
        &server,
        json!({ "rewrite": rewrite("old_call($A)", "new_call($A, 0)") }),
    )
    .await;
    assert!(again_error && again.contains("matched nothing"), "{again}");
}

#[tokio::test]
async fn scope_language_and_limit_narrow_the_rewrite() {
    let repo = fixture_repo();
    let root = repo.path();
    let server = server_for(root);

    let mut scoped = rewrite("old_call($A)", "new_call($A)");
    scoped["language"] = json!("Python");
    let (text, error) = call(&server, json!({ "rewrite": scoped })).await;
    assert!(!error, "{text}");
    assert_eq!(
        read(root, "py/tool.py"),
        "def go():\n    return new_call(4)\n"
    );
    assert_eq!(read(root, "src/lib.rs"), FILES[0].1);

    let mut limited = rewrite("old_call($A)", "new_call($A)");
    limited["path_prefix"] = json!("src");
    limited["limit"] = json!(1);
    let (text, error) = call(&server, json!({ "rewrite": limited.clone() })).await;
    assert!(error && text.contains("more than 1 match(es)"), "{text}");
    assert_eq!(read(root, "src/lib.rs"), FILES[0].1);

    limited["limit"] = json!(2);
    let (text, error) = call(&server, json!({ "rewrite": limited })).await;
    assert!(!error, "{text}");
    assert_eq!(read(root, "web/app.ts"), FILES[1].1, "outside the prefix");

    let mut globbed = rewrite("old_call($A)", "new_call($A)");
    globbed["glob"] = json!("web/*.ts");
    let (text, error) = call(&server, json!({ "rewrite": globbed })).await;
    assert!(!error, "{text}");
    assert_eq!(
        read(root, "web/app.ts"),
        "export function go() {\n  return new_call(3);\n}\n"
    );
}

#[tokio::test]
async fn unrecoverable_requests_are_refused_and_write_nothing() {
    let repo = fixture_repo();
    let root = repo.path();
    let server = server_for(root);
    let outside = root.parent().unwrap().display().to_string();

    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "syntax-breaking template",
            json!({ "rewrite": rewrite("old_call($A)", "new_call($A") }),
            "would break the syntax of",
        ),
        (
            "unknown metavariable",
            json!({ "rewrite": rewrite("old_call($A)", "new_call($B)") }),
            "which the pattern does not capture",
        ),
        (
            "zero matches",
            json!({ "rewrite": rewrite("never_called($A)", "x($A)") }),
            "matched nothing",
        ),
        (
            "pattern that does not compile",
            json!({ "rewrite": rewrite("first(); second();", "x") }),
            "invalid structural pattern",
        ),
        (
            "parent-relative scope",
            json!({ "rewrite": { "pattern": "old_call($A)", "template": "x", "path_prefix": "../elsewhere" } }),
            "outside the repository",
        ),
        (
            "absolute scope",
            json!({ "rewrite": { "pattern": "old_call($A)", "template": "x", "path_prefix": outside } }),
            "outside the repository",
        ),
        (
            "edits and rewrite together",
            json!({
                "rewrite": rewrite("old_call($A)", "x"),
                "edits": [{ "path": "src/lib.rs", "name": "run", "operation": { "type": "delete" } }]
            }),
            "not both",
        ),
        (
            "neither edits nor rewrite",
            json!({ "dry_run": true }),
            "needs `edits`",
        ),
    ];
    for (label, input, needle) in cases {
        let (text, _) = call(&server, input).await;
        assert!(text.contains(needle), "{label}: {text}");
        unchanged(root);
    }
}

#[tokio::test]
async fn nested_matches_are_refused_as_overlapping() {
    let repo = fixture_repo();
    let root = repo.path();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn run() {\n    old_call(old_call(1));\n}\n",
    )
    .unwrap();
    let server = server_for(root);
    let (text, error) = call(
        &server,
        json!({ "rewrite": rewrite("old_call($A)", "new_call($A)") }),
    )
    .await;
    assert!(error && text.contains("overlap"), "{text}");
    assert_eq!(
        read(root, "src/lib.rs"),
        "pub fn run() {\n    old_call(old_call(1));\n}\n"
    );
}
