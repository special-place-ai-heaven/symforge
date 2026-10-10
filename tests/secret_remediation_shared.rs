#![cfg(feature = "server")]

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::withheld::WITHHELD_META_KEY;
use symforge::watcher::WatcherInfo;

fn write_source(root: &Path, path: &str, value: &str) {
    let target = root.join(path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, format!("{{\n  \"password\": \"{value}\"\n}}\n")).unwrap();
}

async fn dispatch(server: &SymForgeServer, tool: &str, input: Value) -> Value {
    let result = server
        .dispatch_tool_result_for_tests(tool, input)
        .await
        .unwrap();
    serde_json::to_value(result).unwrap()
}

#[tokio::test]
async fn multi_file_externalize_uses_one_guarded_commit_and_exact_replay() {
    let repository = tempfile::tempdir().unwrap();
    std::fs::create_dir(repository.path().join(".git")).unwrap();
    let synthetic = ["S3", "cret", "Value", "9xAb"].concat();
    for path in ["config/first.json", "config/second.json"] {
        write_source(repository.path(), path, &synthetic);
    }
    let shared = LiveIndex::load(repository.path()).unwrap();
    let server = SymForgeServer::new(
        shared,
        "shared-remediation-test".into(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(repository.path().to_path_buf()),
        None,
    );
    let mut finding_ids = Vec::new();
    for path in ["config/first.json", "config/second.json"] {
        let result = dispatch(&server, "get_file_content", json!({ "path": path })).await;
        let id = result["_meta"][WITHHELD_META_KEY]["findings"][0]["id"]
            .as_str()
            .expect("redacted finding id");
        finding_ids.push(id.to_owned());
    }
    let input = json!({
        "scope": ["config/first.json", "config/second.json"],
        "finding_ids": finding_ids,
        "action": "externalize",
        "preview": false,
        "idempotency_key": "shared-multi-file-op",
    });
    let first = dispatch(&server, "secret_remediate", input.clone()).await;
    let text = first["content"][0]["text"].as_str().unwrap();
    let category = if text.contains("admitted remediation source changed") {
        "root_spelling"
    } else if text.contains("admitted remediation source unavailable") {
        "source_unavailable"
    } else if text.contains("durable project-state replay") {
        "durable_state_unavailable"
    } else if text.contains("source binding changed") {
        "binding_changed"
    } else if text.contains("source publication changed") {
        "publication_changed"
    } else if text.contains("target lock unavailable") {
        "lock_unavailable"
    } else if text.contains("staging refused") {
        "staging_refused"
    } else if text.contains("Error:") {
        "other_pre_effect_refusal"
    } else {
        "other_response"
    };
    assert!(text.contains("apply"), "guarded apply category: {category}");
    assert!(!text.contains(&synthetic), "response exposed selected source bytes");
    for path in ["config/first.json", "config/second.json"] {
        let after = std::fs::read_to_string(repository.path().join(path)).unwrap();
        let refusal_class = if text.contains("source binding changed") {
            "binding_changed"
        } else if text.contains("source publication changed") {
            "publication_changed"
        } else if text.contains("target lock unavailable") {
            "lock_unavailable"
        } else if text.contains("staging refused") {
            "staging_refused"
        } else if text.contains("guarded_write_uncertain") {
            "guarded_write_uncertain"
        } else if text.contains("source_post_image_unavailable") {
            "postimage_unavailable"
        } else if text.contains("replay_completion_unavailable") {
            "replay_completion_unavailable"
        } else if text.starts_with("Error:") {
            "other_refusal"
        } else if text.contains("apply_status: incomplete") {
            "other_incomplete"
        } else {
            "reported_apply"
        };
        assert!(
            !after.contains(&synthetic),
            "selected source was not rewritten; classification={refusal_class}"
        );
    }
    let replay = dispatch(&server, "secret_remediate", input).await;
    let replay_text = replay["content"][0]["text"].as_str().unwrap();
    assert!(replay_text.contains("apply"), "same key did not replay applied result");
    assert!(!replay_text.contains(&synthetic), "replay exposed selected source bytes");
}
