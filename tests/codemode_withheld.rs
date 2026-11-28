//! Direct Code Mode refusals retain actionable metadata without source bytes.
#![cfg(feature = "server")]

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::codemode::{CodeModeRunner, LocalCodeBackend};
use symforge::live_index::LiveIndex;
use symforge::process_util::hidden_command;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

const PATH: &str = "fixture.py";
const SYMBOL: &str = "get_symbol(path=\"fixture.py\", name=\"callback\")";
const FILE: &str = "file_content(path=\"fixture.py\")";

fn fixture() -> (TempDir, String) {
    let root = TempDir::new().expect("fixture");
    let canary = "synthetic-canary-".repeat(5);
    std::fs::write(
        root.path().join(PATH),
        format!("password = \"{canary}\"\n\ndef callback():\n    return 42\n"),
    )
    .expect("write fixture");
    (root, canary)
}

fn assert_metadata(envelope: &Value, canary: &str) {
    let serialized = envelope.to_string();
    assert!(!serialized.contains(canary), "source canary was disclosed");
    assert!(!serialized.contains("return 42"), "source was disclosed");
    let meta = &envelope["notices"][0]["withheld"][0];
    assert_eq!(meta["path"], PATH);
    assert_eq!(meta["contract_version"], 1);
    let finding = &meta["findings"][0];
    assert_eq!(finding["line_start"], 1);
    assert_eq!(finding["line_end"], 1);
    assert!(finding["rule_id"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(finding["shape"].as_str().is_some_and(|s| !s.is_empty()));
    let keys: std::collections::BTreeSet<_> = finding
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "actions",
            "confidence",
            "id",
            "line_end",
            "line_start",
            "rule_id",
            "shape"
        ]
        .into()
    );
}

#[test]
fn local_direct_refusals_survive_ignored_values_and_caught_errors() {
    let (root, canary) = fixture();
    std::fs::write(root.path().join("plain.py"), "def plain():\n    pass\n").unwrap();
    let backend = LocalCodeBackend::open_read_only(root.path()).expect("open index");
    for code in [
        format!("{SYMBOL}\n0"),
        format!("{SYMBOL}\n{SYMBOL}\n0"),
        FILE.to_string(),
        "file_context(path=\"fixture.py\")".to_string(),
        "find_dependents(path=\"fixture.py\")".to_string(),
        "symbol_context(path=\"fixture.py\", name=\"callback\")".to_string(),
        "inspect_match(path=\"fixture.py\", line=3)".to_string(),
        format!("try:\n    {FILE}\nexcept RuntimeError:\n    pass\n0"),
        "get_symbol(targets=[{'path':'fixture.py','name':'callback'}, {'path':'plain.py'}])".to_string(),
        "try:\n    get_symbol(targets=[{'path':'fixture.py','name':'callback'}, {'path':'plain.py'}])\nexcept RuntimeError:\n    pass\n0".to_string(),
    ] {
        let outcome = CodeModeRunner::default().execute(&code, &backend);
        let json = serde_json::to_value(outcome).unwrap();
        assert_metadata(&json, &canary);
        assert_eq!(json["notices"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn cli_direct_refusals_include_sanitized_metadata() {
    let (root, canary) = fixture();
    let home = TempDir::new().expect("home");
    for code in [SYMBOL, FILE] {
        let output = hidden_command(env!("CARGO_BIN_EXE_symforge"))
            .args([
                "code",
                "run",
                "--root",
                root.path().to_str().unwrap(),
                "--code",
                code,
            ])
            .env("SYMFORGE_NO_DAEMON", "1")
            .env("SYMFORGE_HOME", home.path())
            .output()
            .expect("run CLI");
        assert!(output.stderr.is_empty(), "unexpected stderr");
        let json: Value = serde_json::from_slice(&output.stdout).expect("JSON outcome");
        assert_metadata(&json, &canary);
    }
}

#[test]
fn unindexed_disk_file_refusal_keeps_metadata() {
    let (root, canary) = fixture();
    let backend = LocalCodeBackend::open_read_only(root.path()).unwrap();
    let content = std::fs::read(root.path().join(PATH)).unwrap();
    std::fs::write(root.path().join("late.py"), content).unwrap();
    let outcome = CodeModeRunner::default().execute("file_content(path=\"late.py\")", &backend);
    let envelope = serde_json::to_value(outcome).unwrap();
    assert_eq!(envelope["notices"][0]["withheld"][0]["path"], "late.py");
    assert_eq!(
        envelope["notices"][0]["withheld"][0]["findings"][0]["line_start"],
        1
    );
    assert!(!envelope.to_string().contains(&canary));
}

#[test]
fn single_query_cli_preserves_refusal_metadata() {
    let (root, canary) = fixture();
    let home = TempDir::new().unwrap();
    for (op, args) in [
        ("get_symbol", r#"{"path":"fixture.py","name":"callback"}"#),
        ("file_content", r#"{"path":"fixture.py"}"#),
    ] {
        let output = hidden_command(env!("CARGO_BIN_EXE_symforge"))
            .args(["q", op, args, "--root", root.path().to_str().unwrap()])
            .env("SYMFORGE_NO_DAEMON", "1")
            .env("SYMFORGE_HOME", home.path())
            .output()
            .unwrap();
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(!json.to_string().contains(&canary));
        let envelope = if json["status"] == "error" {
            json!({"notices": [{"withheld": json["error"]["withheld"]}]})
        } else {
            json
        };
        assert_metadata(&envelope, &canary);
    }
}

#[test]
fn daemon_direct_refusals_preserve_local_metadata() {
    let (root, canary) = fixture();
    let home = TempDir::new().expect("daemon home");
    let state = symforge::domain::ControlStateDir::new(home.path().to_path_buf());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let outcomes = runtime.block_on(async {
        let daemon = symforge::daemon::spawn_daemon_at("127.0.0.1", &state)
            .await
            .unwrap();
        let session = symforge::daemon::connect_existing_session_at(
            root.path(),
            "withheld-test",
            Some(std::process::id()),
            &state,
        )
        .await
        .unwrap()
        .expect("session");
        let handle = tokio::runtime::Handle::current();
        let outcomes = tokio::task::spawn_blocking(move || {
            let backend =
                symforge::codemode::DaemonCodeBackend::new_ephemeral(handle, session, None);
            let outcomes =
                [SYMBOL, FILE].map(|code| CodeModeRunner::default().execute(code, &backend));
            backend.close_best_effort();
            outcomes
        })
        .await
        .unwrap();
        let _ = daemon.shutdown_tx.send(());
        outcomes
    });
    let local = LocalCodeBackend::open_read_only(root.path()).unwrap();
    for (code, outcome) in [SYMBOL, FILE].into_iter().zip(outcomes) {
        assert_eq!(
            outcome.notices,
            CodeModeRunner::default().execute(code, &local).notices
        );
        assert_metadata(&serde_json::to_value(outcome).unwrap(), &canary);
    }
}

#[tokio::test]
async fn mcp_direct_refusals_include_sanitized_metadata() {
    let (root, canary) = fixture();
    let canonical = root.path().canonicalize().unwrap();
    let server = SymForgeServer::new(
        LiveIndex::load(&canonical).expect("load index"),
        "withheld-test".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(canonical),
        None,
    );
    for code in [SYMBOL, FILE] {
        let result = server
            .dispatch_tool_result_for_tests("code_execute", json!({"code": code}))
            .await
            .expect("MCP result");
        let text = result
            .content
            .iter()
            .find_map(|block| match block {
                rmcp::model::ContentBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .expect("text result");
        let envelope: Value = match text.split_once("\n\nHost-call completeness: ") {
            Some((_, notices)) => serde_json::from_str(notices).unwrap(),
            None => serde_json::from_str(text).expect("JSON error outcome"),
        };
        assert_metadata(&envelope, &canary);
        let id = &envelope["notices"][0]["withheld"][0]["findings"][0]["id"];
        let preview = server
            .dispatch_tool_result_for_tests(
                "secret_remediate",
                json!({
                    "scope": PATH, "finding_ids": [id], "action": "externalize", "preview": true,
                }),
            )
            .await
            .expect("scoped remediation preview");
        assert_ne!(preview.is_error, Some(true), "finding ID must resolve");
        let preview = serde_json::to_string(&preview).unwrap();
        assert!(preview.contains("secret_remediate preview (externalize)"));
        assert!(preview.contains(PATH));
        assert!(!preview.contains(&canary));
        assert!(!serde_json::to_string(&result).unwrap().contains(&canary));
    }
}
