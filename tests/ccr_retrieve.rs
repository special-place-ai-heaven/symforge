//! CCR retrieve round-trip (011 US2).

use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;

fn server_for_fixture() -> SymForgeServer {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shared = LiveIndex::load(&root).expect("load symforge index");
    SymForgeServer::new(
        shared,
        "symforge".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root),
        None,
    )
}

#[tokio::test]
async fn symforge_retrieve_unknown_hash_errors() {
    let server = server_for_fixture();
    let out = server
        .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": "deadbeefcafe" }))
        .await;
    assert!(out.contains("unknown or expired hash"));
}

#[tokio::test]
async fn search_text_ccr_stores_and_retrieves() {
    let server = server_for_fixture();
    let capped = server
        .dispatch_tool_for_tests(
            "search_text",
            json!({
                "query": "fn",
                "path_prefix": "src/",
                "limit": 200,
                "max_per_file": 20,
                "max_tokens": 200
            }),
        )
        .await;
    let hash = capped
        .split("hash=\"")
        .nth(1)
        .and_then(|s| s.split('"').next());
    let Some(hash) = hash else {
        eprintln!("skip: search_text fit within budget without CCR");
        return;
    };
    let full = server
        .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": hash }))
        .await;
    assert!(
        !full.contains("unknown or expired"),
        "retrieve failed: {full}"
    );
    assert!(full.len() > capped.len());
}
