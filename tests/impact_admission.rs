// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Admission coherence for `analyze_file_impact` (dogfood #7, 2026-07-06).
//!
//! The impact path used to force-index ANY file via `process_file` +
//! `update_file`, bypassing the admission gate that the bulk walk and the
//! watcher both apply. The result was flapping: `analyze_file_impact`
//! admitted an oversized file, the next watcher event demoted it again.
//! These tests pin: (1) a Tier-2 file gets an honest refusal that names the
//! tier and reason, never a silent admit; (2) the refusal updates the skip
//! registry so `health`/watcher agree; (3) code files between 1MB and 4MB
//! are Tier-1 end-to-end under METADATA_ONLY_CODE_BYTES (dogfood #1/#7).

use std::fs;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

fn make_server(files: &[(&str, &str)]) -> (TempDir, SymForgeServer) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    for (rel, content) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, content).expect("write fixture file");
    }
    let shared = LiveIndex::load(&root).expect("LiveIndex::load");
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let server = SymForgeServer::new(
        shared,
        "impact_admission_test".to_string(),
        watcher_info,
        Some(root),
        None,
    );
    (dir, server)
}

#[tokio::test]
async fn impact_refuses_oversized_code_file() {
    // 5MB .rs is above METADATA_ONLY_CODE_BYTES (4MB) — Tier-2 at load, and
    // analyze_file_impact must refuse honestly instead of force-admitting.
    let big = format!("fn generated() {{}}\n// {}\n", "x".repeat(5 * 1024 * 1024));
    let (_dir, server) = make_server(&[("src/lib.rs", "pub fn keep() {}\n"), ("src/big.rs", &big)]);

    let result = server
        .dispatch_tool_for_tests("analyze_file_impact", json!({ "path": "src/big.rs" }))
        .await;
    assert!(
        result.contains("Not indexed") && result.contains("Tier 2"),
        "oversized code file must get an honest Tier-2 refusal; got: {result}"
    );
    assert!(
        result.contains("over size threshold"),
        "the refusal must name the size reason; got: {result}"
    );
    // No silent admit: the file must NOT be in the Tier-1 index afterwards.
    assert!(
        server.index().read().get_file("src/big.rs").is_none(),
        "impact must not force-admit a Tier-2 file"
    );
    // The skip registry knows it, so health/watcher agree with the refusal.
    assert!(
        server
            .index()
            .read()
            .skipped_files()
            .iter()
            .any(|f| f.path == "src/big.rs"),
        "the refusal must record the demotion in the skip registry"
    );
}

#[tokio::test]
async fn impact_admits_code_file_between_1mb_and_4mb() {
    // Dogfood #1/#7: 1.2MB first-party code is load-bearing and must be
    // Tier-1 end-to-end — indexed at load AND re-indexable via impact.
    let medium = format!("pub fn loaded() {{}}\n// {}\n", "x".repeat(1_200_000));
    let (_dir, server) = make_server(&[("src/medium.rs", &medium)]);

    assert!(
        server.index().read().get_file("src/medium.rs").is_some(),
        "1.2MB code file must be Tier-1 at load under METADATA_ONLY_CODE_BYTES"
    );
    let result = server
        .dispatch_tool_for_tests("analyze_file_impact", json!({ "path": "src/medium.rs" }))
        .await;
    assert!(
        !result.contains("Not indexed"),
        "1.2MB code file must not be refused; got: {result}"
    );
    assert!(
        result.contains("Impact:") || result.contains("Symbols:"),
        "impact must actually run on a Tier-1 file (positive signal, not just \
         absence of refusal); got: {result}"
    );
}

#[tokio::test]
async fn impact_refuses_oversized_data_file_at_1mb_threshold() {
    // Data formats keep the 1MB symbol-pollution threshold.
    let big_json = format!("{{\"pad\": \"{}\"}}", "x".repeat(1_500_000));
    let (_dir, server) = make_server(&[
        ("src/lib.rs", "pub fn keep() {}\n"),
        ("data/big.json", &big_json),
    ]);

    let result = server
        .dispatch_tool_for_tests("analyze_file_impact", json!({ "path": "data/big.json" }))
        .await;
    assert!(
        result.contains("Not indexed") && result.contains("Tier 2"),
        "1.5MB data file must get an honest Tier-2 refusal; got: {result}"
    );
}
