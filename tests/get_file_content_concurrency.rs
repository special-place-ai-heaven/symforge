// SF-AAP-003 regression: parallel `get_file_content` RANGE calls at one file
// must complete (or return a typed busy/unsupported result) within a bounded
// time — never hang/deadlock.
//
// Server-only integration test.
#![cfg(feature = "server")]

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

fn build_server() -> (TempDir, Arc<SymForgeServer>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("docs")).expect("docs dir");
    // A file with enough lines to slice many distinct ranges.
    let mut body = String::from("# Big doc\n");
    for i in 0..400 {
        body.push_str(&format!("line {i} of the concurrency fixture document\n"));
    }
    fs::write(root.join("docs/big.md"), body).expect("big fixture");

    let index = LiveIndex::load(&root).expect("LiveIndex::load concurrency fixture");
    let server = SymForgeServer::new(
        index,
        "gfc_concurrency_test".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root),
        None,
    );
    (dir, Arc::new(server))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_range_reads_do_not_hang() {
    let (_dir, server) = build_server();

    // Distinct ranges per task so the session repeat-read cache does NOT
    // short-circuit — every task drives the real read/freshness path.
    let n = 32u32;
    let mut handles = Vec::with_capacity(n as usize);
    for i in 0..n {
        let server = Arc::clone(&server);
        let start = (i * 5) + 1;
        let req = json!({
            "path": "docs/big.md",
            "start_line": start,
            "end_line": start + 4,
        });
        handles.push(tokio::spawn(async move {
            server
                .dispatch_tool_for_tests("get_file_content", req)
                .await
        }));
    }

    let join = async {
        for h in handles {
            let out = h.await.expect("task panicked");
            assert!(
                out.contains("concurrency fixture document"),
                "each concurrent range read must return its slice; got:\n{out}"
            );
        }
    };

    match tokio::time::timeout(Duration::from_secs(20), join).await {
        Ok(()) => {}
        Err(_) => panic!(
            "SF-AAP-003: {n} concurrent get_file_content range reads did not complete \
             within 20s — hang/deadlock in the read path"
        ),
    }
}

/// Harder variant: the file is made stale on disk so every concurrent range
/// read drives the freshness/reconcile WRITE path (`freshen_file_if_stale` →
/// `maybe_reindex`) at the same time. This is the write-contended path where a
/// lock-ordering deadlock would surface. Must still complete within the bound.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_range_reads_under_staleness_do_not_hang() {
    let (dir, server) = build_server();

    // Rewrite + advance mtime so the indexed copy is stale for all readers.
    let path = dir.path().join("docs/big.md");
    let mut body = String::from("# Big doc v2\n");
    for i in 0..400 {
        body.push_str(&format!(
            "line {i} of the concurrency fixture document v2\n"
        ));
    }
    fs::write(&path, &body).expect("rewrite fixture");
    filetime::set_file_mtime(
        &path,
        filetime::FileTime::from_system_time(
            std::time::SystemTime::now() + std::time::Duration::from_secs(3),
        ),
    )
    .expect("advance mtime");

    let n = 32u32;
    let mut handles = Vec::with_capacity(n as usize);
    for i in 0..n {
        let server = Arc::clone(&server);
        let start = (i % 8) * 5 + 1; // deliberate overlap → concurrent reconcile of one file
        let req = json!({
            "path": "docs/big.md",
            "start_line": start,
            "end_line": start + 4,
            "force_refresh": true,
        });
        handles.push(tokio::spawn(async move {
            server
                .dispatch_tool_for_tests("get_file_content", req)
                .await
        }));
    }

    let join = async {
        for h in handles {
            let out = h.await.expect("task panicked");
            // Mirror the fresh test (assert real content, not just non-hang).
            // The substring is common to the pre-reconcile (`...document`) and
            // post-reconcile (`...document v2`) bodies, so it tolerates a read
            // racing the concurrent reindex without pinning a specific version.
            assert!(
                out.contains("concurrency fixture document"),
                "each concurrent stale range read must return real fixture content \
                 (pre- or post-reconcile), never empty/garbage; got:\n{out}"
            );
        }
    };

    match tokio::time::timeout(Duration::from_secs(20), join).await {
        Ok(()) => {}
        Err(_) => panic!(
            "SF-AAP-003: {n} concurrent stale get_file_content range reads did not complete \
             within 20s — hang/deadlock in the freshness/reconcile write path"
        ),
    }
}
