//! Multi-hop STEL serve chain failure regression coverage.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::path::PathBuf;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::stel::{self, StelRequest};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tool_result_text(result: &serde_json::Value) -> &str {
    result["content"][0]["text"]
        .as_str()
        .expect("symforge result must contain text content")
}

fn server_for_corpus(relative: &str, project: &str) -> SymForgeServer {
    let root = repo_root().join(relative);
    let shared = LiveIndex::load(&root).unwrap_or_else(|error| {
        panic!("index {}: {error}", root.display());
    });
    SymForgeServer::new(
        shared,
        project.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root),
        None,
    )
}

#[tokio::test]
async fn multi_hop_chain_rejects_when_inner_step_fails() {
    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let corpus = repo_root().join("tests/fixtures/stel_multi_hop/cfg-if-rust");
    assert!(
        corpus.join("src/lib.rs").is_file(),
        "missing checked-in multi-hop fixture: {}",
        corpus.join("src/lib.rs").display()
    );
    let server = server_for_corpus("tests/fixtures/stel_multi_hop/cfg-if-rust", "cfg-if-rust");
    let params = serde_json::to_value(stel::SymforgeCallInput {
        request: StelRequest {
            query: "search then fetch cfg_if body".to_string(),
            ..Default::default()
        },
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("serialize symforge params");

    let result = server
        .dispatch_tool_result_for_tests("symforge", params)
        .await
        .expect("symforge dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize result");
    let output = tool_result_text(&serialized);

    assert!(
        output.contains("decision: serve"),
        "successful multi-hop chain must serve on checked-in fixture"
    );
    assert!(!output.contains("decision: reject"));

    // Run a multi-hop plan against the wrong corpus so step 1 (`get_file_context records.py`) fails.
    let bad_server = server_for_corpus("tests/fixtures/stel_multi_hop/cfg-if-rust", "cfg-if-rust");
    let bad_params = serde_json::to_value(stel::SymforgeCallInput {
        request: StelRequest {
            query: "outline then find Connection refs".to_string(),
            ..Default::default()
        },
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("serialize bad params");
    let bad_result = bad_server
        .dispatch_tool_result_for_tests("symforge", bad_params)
        .await
        .expect("symforge dispatch");
    let bad_serialized = serde_json::to_value(&bad_result).expect("serialize");
    let bad_output = tool_result_text(&bad_serialized);

    assert!(
        bad_output.contains("decision: reject"),
        "failed inner step must not finalize as serve; got: {bad_output}"
    );
    assert!(
        bad_output.contains("Multi-hop chain failed"),
        "failed chain must include failure footer"
    );
    assert!(
        bad_output.contains("File not found:"),
        "failed chain body must include tool failure text"
    );
}
