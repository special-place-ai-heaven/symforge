//! Compression economics counters (011 US5).

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
async fn cache_hit_and_ccr_counters_surface_in_context_inventory() {
    let server = server_for_fixture();
    let params = json!({ "path": "src/main.rs", "name": "main" });
    server
        .dispatch_tool_for_tests("get_symbol", params.clone())
        .await;
    server.dispatch_tool_for_tests("get_symbol", params).await;

    let capped = server
        .dispatch_tool_for_tests(
            "search_text",
            json!({
                "query": "fn",
                "path_prefix": "src/",
                "limit": 200,
                "max_tokens": 200
            }),
        )
        .await;

    let hash = capped
        .split("hash=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(str::to_string);
    if let Some(ref handle) = hash {
        server
            .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": handle }))
            .await;
    }

    let inventory = server
        .dispatch_tool_for_tests("context_inventory", json!({}))
        .await;
    assert!(
        inventory.contains("cache_hits: 1"),
        "expected cache hit counter:\n{inventory}"
    );
    if hash.is_some() {
        assert!(
            inventory.contains("ccr_offloads:"),
            "expected CCR counters:\n{inventory}"
        );
        assert!(inventory.contains("ccr_bytes_retrieved:"));
    }

    let h = server.session_compression_heuristic();
    assert_eq!(h.cache_hits, 1);
    if hash.is_some() {
        assert!(h.ccr_offloads >= 1);
        assert!(h.ccr_bytes_retrieved > 0);
    }
}
