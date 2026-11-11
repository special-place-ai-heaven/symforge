//! `secret_remediate` dismiss apply needs operator approval outside MCP.
//! Own binary: this process never sets the approval variable.
#![cfg(feature = "server")]

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::withheld::WITHHELD_META_KEY;
use symforge::watcher::WatcherInfo;

const WITHHELD_PREFIX: &str = "Content withheld by admission policy:";

async fn dispatch(server: &SymForgeServer, tool: &str, params: serde_json::Value) -> String {
    let result = server
        .dispatch_tool_result_for_tests(tool, params)
        .await
        .expect("dispatch");
    let value = serde_json::to_value(&result).expect("serialize");
    value["content"][0]["text"]
        .as_str()
        .expect("text")
        .to_string()
}

fn server_for(root: &Path) -> SymForgeServer {
    SymForgeServer::new(
        LiveIndex::load(root).expect("index"),
        "dismiss-gate".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root.to_path_buf()),
        None,
    )
}

#[tokio::test]
async fn dismiss_apply_over_mcp_alone_leaves_refusal_in_force() {
    assert!(std::env::var_os("SYMFORGE_SECRET_DISMISS_APPROVED").is_none());
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join("config")).unwrap();
    std::fs::write(
        dir.path().join("config/app.json"),
        "{\n  \"password\": \"S3cretValue9xAb\"\n}\n",
    )
    .unwrap();
    let server = server_for(dir.path());
    let read = json!({ "path": "config/app.json" });
    let result = server
        .dispatch_tool_result_for_tests("get_file_content", read.clone())
        .await
        .expect("dispatch");
    let refusal = serde_json::to_value(&result).unwrap();
    let id = refusal["_meta"][WITHHELD_META_KEY]["findings"][0]["id"]
        .as_str()
        .expect("finding id from the refusal")
        .to_string();

    for key in [None, Some("dismiss-gate-key")] {
        let mut request = json!({
            "scope": "config/app.json",
            "finding_ids": [id],
            "action": "dismiss",
            "preview": false
        });
        if let Some(key) = key {
            request["idempotency_key"] = json!(key);
        }
        let text = dispatch(&server, "secret_remediate", request).await;
        assert!(
            text.starts_with("Error: dismiss apply refused")
                && text.contains("SYMFORGE_SECRET_DISMISS_APPROVED")
                && text.contains("secret_remediate preview (dismiss)")
                && !text.contains("S3cretValue9xAb"),
            "{text}"
        );
        assert!(
            !dir.path().join(".symforge/secret-dismissals.json").exists(),
            "an unapproved dismiss must write nothing"
        );
        assert!(
            dispatch(&server, "get_file_content", read.clone())
                .await
                .starts_with(WITHHELD_PREFIX),
            "the refusal must remain in force"
        );
    }
    let fresh = server_for(dir.path());
    assert!(
        dispatch(&fresh, "get_file_content", read)
            .await
            .starts_with(WITHHELD_PREFIX)
    );
}
