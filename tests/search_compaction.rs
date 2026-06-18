//! Search compaction integration (011 US3).

#![cfg(feature = "server")]

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    server: SymForgeServer,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("src")).expect("src dir");
        let mut body = String::from("fn main() {}\n");
        body.push_str("fn disk_error() { log::error!(\"ERROR: disk full\"); }\n");
        body.push_str("fn retry_failed() { log::error!(\"ERROR: retry failed\"); }\n");
        for i in 0..50 {
            body.push_str(&format!("fn helper_{i}() {{ let x = {i}; }}\n"));
        }
        fs::write(root.join("src/lib.rs"), &body).expect("write lib.rs");

        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let server = SymForgeServer::new(
            shared,
            "search_compaction_test".to_string(),
            std::sync::Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Some(root),
            None,
        );
        Self { _dir: dir, server }
    }
}

#[tokio::test]
async fn search_text_compaction_preserves_error_lines_and_discloses_truncation() {
    let fixture = Fixture::new();
    let out = fixture
        .server
        .dispatch_tool_for_tests(
            "search_text",
            json!({
                "query": "fn",
                "limit": 500,
                "max_per_file": 50
            }),
        )
        .await;
    assert!(
        out.contains("ERROR: disk") || out.contains("ERROR: retry"),
        "expected error lines preserved: {out}"
    );
    assert!(
        out.contains("omitted") || out.contains("more"),
        "expected truncation disclosure: {out}"
    );
}

#[tokio::test]
async fn search_text_compaction_caps_many_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = dir.path().to_path_buf();
    for i in 0..30 {
        let path = root.join(format!("src/file_{i:02}.rs"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(&path, format!("fn match_me_{i}() {{}}\n")).expect("write");
    }
    let shared = LiveIndex::load(&root).expect("load");
    let server = SymForgeServer::new(
        shared,
        "search_compaction_caps".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
        Some(root),
        None,
    );
    let out = server
        .dispatch_tool_for_tests(
            "search_text",
            json!({ "query": "match_me", "limit": 500 }),
        )
        .await;
    let file_headers = out.matches("file_").count();
    assert!(
        file_headers <= 25,
        "expected file cap near 20, saw ~{file_headers} file mentions"
    );
}
