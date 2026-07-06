// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! `index_folder` must refuse OS system directories and drive roots BEFORE any
//! walk or IO — never surface an access-denied error from trying (field
//! report 2026-07-06: indexing `C:\Windows\System32` errored out on access
//! instead of being refused outright).

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

fn make_server() -> (TempDir, SymForgeServer) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("lib.rs"), "pub fn keep() {}\n").expect("write fixture");
    let shared = LiveIndex::load(&root).expect("LiveIndex::load");
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let server = SymForgeServer::new(
        shared,
        "system_path_refusal_test".to_string(),
        watcher_info,
        Some(root),
        None,
    );
    (dir, server)
}

#[cfg(windows)]
#[tokio::test]
async fn index_folder_refuses_windows_system_directories() {
    let (_dir, server) = make_server();
    for target in [
        "C:\\Windows\\System32",
        "C:\\Windows",
        "C:\\Program Files",
        "C:\\ProgramData",
    ] {
        if !std::path::Path::new(target).exists() {
            continue; // unusual CI image — nothing to assert against
        }
        let result = server
            .dispatch_tool_for_tests("index_folder", json!({ "path": target }))
            .await;
        assert!(
            result.contains("Refused to index sensitive system path"),
            "{target} must be refused outright, not walked or errored; got: {result}"
        );
        assert!(
            !result.to_ascii_lowercase().contains("access is denied")
                && !result.to_ascii_lowercase().contains("permission denied"),
            "{target} refusal must come from the guard, not from an IO failure; got: {result}"
        );
    }
}

#[cfg(windows)]
#[tokio::test]
async fn index_folder_refuses_unresolvable_system_paths_without_os_error() {
    // The field failure shape: a system path that exists()/canonicalize()
    // cannot resolve (protected traversal or nonexistent under a system root)
    // used to surface a raw OS error instead of the refusal — the guard must
    // run on the RAW input first.
    let (_dir, server) = make_server();
    for target in [
        "C:\\Windows\\System32\\nonexistent-symforge-probe-dir",
        "C:/Windows/System32", // forward slashes must not dodge the guard
    ] {
        let result = server
            .dispatch_tool_for_tests("index_folder", json!({ "path": target }))
            .await;
        assert!(
            result.contains("Refused to index sensitive system path"),
            "{target} must hit the guard before exists()/canonicalize(); got: {result}"
        );
    }
}

#[cfg(windows)]
#[tokio::test]
async fn index_folder_refuses_drive_root() {
    let (_dir, server) = make_server();
    let result = server
        .dispatch_tool_for_tests("index_folder", json!({ "path": "C:\\" }))
        .await;
    assert!(
        result.contains("Refused to index sensitive system path"),
        "a bare drive root must be refused; got: {result}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn index_folder_refuses_unix_system_directories() {
    let (_dir, server) = make_server();
    for target in ["/etc", "/proc", "/usr", "/"] {
        if !std::path::Path::new(target).exists() {
            continue;
        }
        let result = server
            .dispatch_tool_for_tests("index_folder", json!({ "path": target }))
            .await;
        assert!(
            result.contains("Refused to index sensitive system path"),
            "{target} must be refused outright; got: {result}"
        );
    }
}
