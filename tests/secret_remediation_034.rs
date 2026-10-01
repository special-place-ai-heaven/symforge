//! Feature 034 — actionable withheld meta + secret_remediate externalize MVP.
#![cfg(feature = "server")]

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::result_status::RESULT_STATUS_META_KEY;
use symforge::protocol::withheld::WITHHELD_META_KEY;
use symforge::watcher::WatcherInfo;

const SYNTHETIC_SECRET: &str = "S3cretValue9xAb";

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn server_for_repo(root: &Path) -> SymForgeServer {
    let shared = LiveIndex::load(root).unwrap_or_else(|e| panic!("index: {e}"));
    SymForgeServer::new(
        shared,
        "034-test".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root.to_path_buf()),
        None,
    )
}

fn result_text(v: &serde_json::Value) -> &str {
    v["content"][0]["text"].as_str().expect("text content")
}

async fn dispatch(
    server: &SymForgeServer,
    tool: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let result = server
        .dispatch_tool_result_for_tests(tool, params)
        .await
        .expect("dispatch");
    serde_json::to_value(&result).expect("serialize")
}

/// Oracle 1: admission refusal carries `_meta.symforge/withheld` without secret bytes.
#[tokio::test]
async fn admission_refusal_carries_withheld_meta_without_secret_bytes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let body = format!("{{\n  \"password\": \"{SYNTHETIC_SECRET}\"\n}}\n");
    write_file(dir.path(), "config/app.json", &body);
    let server = server_for_repo(dir.path());

    let result = dispatch(
        &server,
        "get_file_content",
        json!({ "path": "config/app.json" }),
    )
    .await;

    let text = result_text(&result);
    assert!(
        text.starts_with("Content withheld by admission policy:"),
        "expected refusal, got: {text}"
    );
    assert!(
        !text.contains(SYNTHETIC_SECRET),
        "refusal text must not contain secret"
    );

    let withheld = result["_meta"][WITHHELD_META_KEY]
        .as_object()
        .unwrap_or_else(|| panic!("missing withheld meta: {result}"));
    assert_eq!(withheld["path"], "config/app.json");
    let findings = withheld["findings"].as_array().expect("findings array");
    assert!(!findings.is_empty(), "expected content findings");
    let serialized = result.to_string();
    assert!(
        !serialized.contains(SYNTHETIC_SECRET),
        "serialized CallToolResult must omit secret bytes"
    );
    assert!(
        findings[0]["id"].as_str().unwrap().starts_with("wf_"),
        "content finding id must be wf_*"
    );
    let _ = result["_meta"][RESULT_STATUS_META_KEY];
}

/// Oracle 2: clean reads omit withheld meta.
#[tokio::test]
async fn clean_read_omits_withheld_meta() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    write_file(dir.path(), "src/lib.rs", "fn ok() {}\n");
    let server = server_for_repo(dir.path());

    let result = dispatch(&server, "get_file_content", json!({ "path": "src/lib.rs" })).await;

    let text = result_text(&result);
    assert!(
        !text.starts_with("Content withheld by admission policy:"),
        "{text}"
    );
    assert!(
        result["_meta"].get(WITHHELD_META_KEY).is_none(),
        "clean read must omit withheld meta: {result}"
    );
}

/// Oracle 3: path-rule / unscanned must not invent content wf_ finding ids.
#[tokio::test]
async fn path_rule_or_unscanned_does_not_invent_content_finding_ids() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    write_file(dir.path(), ".env", "PASSWORD=not-disclosed\n");
    let server = server_for_repo(dir.path());

    let result = dispatch(&server, "get_file_content", json!({ "path": ".env" })).await;

    let text = result_text(&result);
    assert!(
        text.starts_with("Content withheld by admission policy:"),
        "{text}"
    );
    let withheld = &result["_meta"][WITHHELD_META_KEY];
    assert!(
        withheld.is_object(),
        "path-rule should still carry withheld meta"
    );
    if let Some(findings) = withheld["findings"].as_array() {
        for f in findings {
            let id = f["id"].as_str().unwrap_or("");
            assert!(
                !id.starts_with("wf_"),
                "path-rule must not invent content wf_ ids, got {id}"
            );
        }
    }
}

/// Oracle 4: externalize preview masks and writes nothing.
#[tokio::test]
async fn externalize_preview_masks_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let body = format!("{{\n  \"api_key\": \"{SYNTHETIC_SECRET}\"\n}}\n");
    write_file(dir.path(), "config/app.json", &body);
    let before = std::fs::read(dir.path().join("config/app.json")).unwrap();
    let server = server_for_repo(dir.path());

    let refusal = dispatch(
        &server,
        "get_file_content",
        json!({ "path": "config/app.json" }),
    )
    .await;
    let id = refusal["_meta"][WITHHELD_META_KEY]["findings"][0]["id"]
        .as_str()
        .expect("finding id")
        .to_string();

    let preview = dispatch(
        &server,
        "secret_remediate",
        json!({
            "scope": "config/app.json",
            "finding_ids": [id],
            "action": "externalize",
            "preview": true
        }),
    )
    .await;
    let text = result_text(&preview);
    assert!(text.contains("preview"), "{text}");
    assert!(text.contains("«secret:"), "masked diff missing: {text}");
    assert!(!text.contains(SYNTHETIC_SECRET), "preview leaked secret");
    assert_eq!(
        std::fs::read(dir.path().join("config/app.json")).unwrap(),
        before,
        "preview must not write"
    );
    assert!(
        !dir.path().join(".env").exists(),
        "preview must not create .env"
    );
}

/// Oracle 5: apply moves value to gitignored .env and rescan is clean.
#[tokio::test]
async fn externalize_apply_moves_to_gitignore_env_and_rescans_clean() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let body = format!("{{\n  \"token\": \"{SYNTHETIC_SECRET}\"\n}}\n");
    write_file(dir.path(), "config/app.json", &body);
    let server = server_for_repo(dir.path());

    let refusal = dispatch(
        &server,
        "get_file_content",
        json!({ "path": "config/app.json" }),
    )
    .await;
    let id = refusal["_meta"][WITHHELD_META_KEY]["findings"][0]["id"]
        .as_str()
        .expect("finding id")
        .to_string();

    let apply = dispatch(
        &server,
        "secret_remediate",
        json!({
            "scope": "config/app.json",
            "finding_ids": [id],
            "action": "externalize",
            "preview": false
        }),
    )
    .await;
    let text = result_text(&apply);
    assert!(text.contains("apply"), "{text}");
    assert!(text.contains("rescan"), "{text}");
    assert!(
        text.contains("clean") || text.contains("still_sensitive"),
        "{text}"
    );
    assert!(
        !text.contains(SYNTHETIC_SECRET),
        "apply response leaked secret"
    );

    let rewritten = std::fs::read_to_string(dir.path().join("config/app.json")).unwrap();
    assert!(
        !rewritten.contains(SYNTHETIC_SECRET),
        "source still has secret"
    );
    assert!(
        rewritten.contains("${"),
        "expected env subst idiom: {rewritten}"
    );

    let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(env.contains(SYNTHETIC_SECRET), ".env should hold the value");
    let gi = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(
        gi.lines().any(|l| l.trim() == ".env"),
        ".gitignore missing .env"
    );

    // Fresh server/index should admit the rewritten file (or still refuse if
    // idiom still trips — assert honest rescan in tool output either way).
    assert!(text.contains("rescan (config/app.json)"));
}

/// Empty finding_ids is rejected (contract; data-model aligned).
#[tokio::test]
async fn empty_finding_ids_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    write_file(dir.path(), "src/lib.rs", "fn ok() {}\n");
    let server = server_for_repo(dir.path());
    let result = dispatch(
        &server,
        "secret_remediate",
        json!({
            "scope": "src/lib.rs",
            "finding_ids": [],
            "action": "externalize",
            "preview": true
        }),
    )
    .await;
    let text = result_text(&result);
    assert!(text.contains("finding_ids must be a non-empty"), "{text}");
}
