// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use parking_lot::Mutex;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;

async fn call(server: &SymForgeServer, tool: &str, params: Value) -> String {
    server.dispatch_tool_for_tests(tool, params).await
}

fn confident_site_count(result: &str) -> usize {
    let line = result
        .lines()
        .find(|line| line.contains("Confident matches") && line.contains("site(s)"))
        .expect("dry_run output should include confident match summary");
    let before_site_suffix = line
        .split("site(s)")
        .next()
        .expect("confident match summary should include site count");
    let reversed_digits: String = before_site_suffix
        .chars()
        .rev()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    let digits: String = reversed_digits.chars().rev().collect();
    digits
        .parse()
        .expect("confident match site count should be numeric")
}

#[tokio::test]
async fn batch_rename_health_dry_run_stays_under_h7_budget() {
    // See docs/notes/external-evaluations/2026-05-11/PROFILE_BATCH_RENAME.md
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shared = LiveIndex::load(&repo_root).expect("LiveIndex::load repo root");
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let server = SymForgeServer::new(
        shared,
        "batch_rename_perf_test".to_string(),
        watcher_info,
        Some(repo_root),
        None,
    );

    let started = Instant::now();
    let result = call(
        &server,
        "batch_rename",
        json!({
            "path": "src/daemon.rs",
            "name": "health",
            "new_name": "get_health",
            "dry_run": true,
        }),
    )
    .await;
    let wall_ms = started.elapsed().as_millis();
    eprintln!("primary repro batch_rename dry_run wall_ms={wall_ms}");

    assert!(!result.trim().is_empty(), "dry_run result was empty");
    assert!(
        wall_ms < 5000,
        "batch_rename dry_run exceeded H.7 budget: {wall_ms}ms\n{result}"
    );
    assert!(
        confident_site_count(&result) >= 1,
        "dry_run should find at least one confident rename site\n{result}"
    );
}
