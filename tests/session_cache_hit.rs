//! Session cache-hit for full read tools (011 US1 / #574).
#![cfg(feature = "server")]

use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::session::hash_symbol_params;

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

fn ccr_hash(body: &str) -> Option<&str> {
    body.split("hash=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
}

#[tokio::test]
async fn get_symbol_repeat_returns_cache_hit() {
    let server = server_for_fixture();
    let params = json!({ "path": "src/cli/entry.rs", "name": "run_main" });
    let first = server
        .dispatch_tool_for_tests("get_symbol", params.clone())
        .await;
    assert!(
        !first.contains("Decision: cache_hit"),
        "first fetch should be full body"
    );
    assert!(first.len() > 100, "expected substantive body");

    let second = server.dispatch_tool_for_tests("get_symbol", params).await;
    assert!(
        second.contains("Decision: cache_hit"),
        "repeat should cache_hit:\n{second}"
    );
    assert!(
        second.len() < first.len(),
        "cache_hit ({}) should be smaller than full body ({})",
        second.len(),
        first.len()
    );

    let forced = server
        .dispatch_tool_for_tests(
            "get_symbol",
            json!({ "path": "src/cli/entry.rs", "name": "run_main", "force_refresh": true }),
        )
        .await;
    assert!(
        !forced.contains("Decision: cache_hit"),
        "force_refresh should bypass cache_hit"
    );
    assert!(forced.len() > 100);
}

#[tokio::test]
async fn get_symbol_cache_hit_is_redeemable_via_retrieve() {
    let server = server_for_fixture();
    let params = json!({ "path": "src/cli/entry.rs", "name": "run_main" });
    let first = server
        .dispatch_tool_for_tests("get_symbol", params.clone())
        .await;
    assert!(!first.contains("Decision: cache_hit"));

    let second = server
        .dispatch_tool_for_tests("get_symbol", params.clone())
        .await;
    assert!(second.contains("Decision: cache_hit"), "{second}");
    assert!(
        second.contains("hash=\""),
        "hit body must use the search_text hash= spelling:\n{second}"
    );
    assert!(
        !second.contains("already loaded in this session"),
        "hit copy must not claim the caller already has the bytes:\n{second}"
    );
    assert!(
        second.contains("this MCP connection"),
        "hit copy must name the MCP connection, not the caller:\n{second}"
    );
    assert!(
        second.contains("not the recovery path for missing bytes"),
        "force_refresh must not be offered as byte recovery:\n{second}"
    );

    let hash = ccr_hash(&second).expect("retrieve hash in cache_hit body");
    let retrieved = server
        .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": hash }))
        .await;
    assert_eq!(
        retrieved, first,
        "retrieve must return the first formatted serve, not a re-query"
    );
}

#[tokio::test]
async fn shared_session_cache_hit_is_redeemable_issue_574() {
    let server = server_for_fixture();
    let params = json!({ "path": "src/cli/entry.rs", "name": "run_main" });
    let first = server
        .dispatch_tool_for_tests("get_symbol", params.clone())
        .await;

    // Same SymForgeServer / SessionContext — Cursor subagents share the parent's
    // MCP connection. The second caller never saw `first`.
    let second = server.dispatch_tool_for_tests("get_symbol", params).await;
    assert!(
        second.contains("Decision: cache_hit"),
        "shared session still cache_hits:\n{second}"
    );
    let hash = ccr_hash(&second).expect("retrieve hash");
    let retrieved = server
        .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": hash }))
        .await;
    assert_eq!(retrieved, first);
}

#[tokio::test]
async fn evicted_ccr_blob_turns_cache_hit_into_miss() {
    let server = server_for_fixture();
    let params = json!({ "path": "src/cli/entry.rs", "name": "run_main" });
    let first = server
        .dispatch_tool_for_tests("get_symbol", params.clone())
        .await;
    let hit = server
        .dispatch_tool_for_tests("get_symbol", params.clone())
        .await;
    let hash = ccr_hash(&hit).expect("retrieve hash");
    assert!(
        server.drop_ccr_blob_for_tests(hash),
        "blob for {hash} must exist before eviction"
    );

    let third = server.dispatch_tool_for_tests("get_symbol", params).await;
    assert!(
        !third.contains("Decision: cache_hit"),
        "missing CCR blob must be a miss, not a lying hit:\n{third}"
    );
    assert!(third.len() > 100, "miss must re-serve a full body");
    assert_eq!(
        third, first,
        "re-serve after eviction should match the original formatted body"
    );
}

#[test]
fn session_detailed_fetch_drives_stel_cache_hit() {
    use symforge::protocol::session::SessionContext;
    use symforge::stel::{
        AdmissionDecision, IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest,
        evaluate_plan_with_session,
    };

    let session = SessionContext::new();
    session.record_symbol_fetch(
        "src/lib.rs",
        "foo",
        hash_symbol_params(None, None, None, 0, "unavailable", 0, 0),
        200,
        "deadbeefcafe",
    );
    let plan = StelPlan {
        plan_id: "t".to_string(),
        intent: IntentBucket::Read,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "get_symbol".to_string(),
            args: json!({ "path": "src/lib.rs", "name": "foo" }),
            est_response_tokens: 100,
            est_manual_tokens: 200,
            index_refs: vec![],
        }],
        suggested_followup: None,
    };
    let decision = evaluate_plan_with_session(&StelRequest::default(), &plan, Some(&session));
    assert_ne!(
        decision.decision,
        AdmissionDecision::CacheHit,
        "generation-blind STEL admission must not cache_hit; the primitive owns the key"
    );
}

/// The repeat-cache PUBLICATION-IDENTITY FENCE (Feature 020, carried Slice 3
/// residual; verified after C9's benchmark flushed a stale
/// `Decision: cache_hit` out of baseline `1521abb0`): a bare repeat read —
/// no `force_refresh` — after the file changed ON DISK must serve the fresh
/// bytes, never a cache hit of the old publication. The freshen-on-read
/// lane runs BEFORE the cache key is computed, so a real change bumps the
/// content generation and the stale record structurally cannot match. The
/// accepting control in the same test: an UNCHANGED repeat is exactly the
/// case the cache exists for, and it hits.
///
/// (C9's incident root cause, recorded honestly: the benchmark's original
/// now-relative mtime backdate collided across revisions, so the freshen
/// legitimately saw an unchanged mtime and the cache consistently served
/// the unchanged publication. The fence itself holds — this oracle is what
/// proves that, instead of the report arguing it.)
#[tokio::test]
async fn stale_publication_never_satisfies_the_repeat_read_cache() {
    use symforge::live_index::LiveIndex;
    use symforge::protocol::SymForgeServer;

    let dir = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    let file = dir.path().join("src/lib.rs");
    std::fs::write(&file, "pub fn fenced() -> u64 {\n    111\n}\n").expect("seed");
    let backdate = |secs: u64| {
        let handle = std::fs::File::options()
            .write(true)
            .open(&file)
            .expect("open");
        handle
            .set_times(std::fs::FileTimes::new().set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
            ))
            .expect("set mtime");
    };
    backdate(1_000_000_001);
    let shared = LiveIndex::load(dir.path()).expect("load");
    let server = SymForgeServer::new(
        shared,
        "fence".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(dir.path().to_path_buf()),
        None,
    );
    let params = json!({ "path": "src/lib.rs" });

    let first = server
        .dispatch_tool_for_tests("get_file_content", params.clone())
        .await;
    assert!(first.contains("111"), "first read serves the body: {first}");

    // Accepting control: the UNCHANGED repeat is the cache's whole job.
    let unchanged = server
        .dispatch_tool_for_tests("get_file_content", params.clone())
        .await;
    assert!(
        unchanged.contains("Decision: cache_hit"),
        "an unchanged repeat read hits: {unchanged}"
    );

    // The fence: the file changes ON DISK (distinct deterministic mtime);
    // a bare repeat read must serve the fresh identity, never the old hit.
    std::fs::write(&file, "pub fn fenced() -> u64 {\n    222\n}\n").expect("rewrite");
    backdate(1_000_000_002);
    let refreshed = server
        .dispatch_tool_for_tests("get_file_content", params.clone())
        .await;
    assert!(
        !refreshed.contains("Decision: cache_hit"),
        "a stale publication satisfied the repeat-read cache: {refreshed}"
    );
    assert!(
        refreshed.contains("222"),
        "the repeat read must observe the fresh identity: {refreshed}"
    );

    // And the fence re-arms: the NEW publication's own repeat hits again.
    let rearmed = server
        .dispatch_tool_for_tests("get_file_content", params)
        .await;
    assert!(
        rearmed.contains("Decision: cache_hit"),
        "the fresh publication's own repeat hits: {rearmed}"
    );

    // get_file_context rides the same freshen-before-key order: after a
    // further on-disk change, its repeat serves the fresh symbol set.
    let context_params = json!({ "path": "src/lib.rs" });
    let context_first = server
        .dispatch_tool_for_tests("get_file_context", context_params.clone())
        .await;
    assert!(context_first.contains("fenced"), "{context_first}");
    std::fs::write(&file, "pub fn refenced() -> u64 {\n    333\n}\n").expect("rewrite 2");
    backdate(1_000_000_003);
    let context_fresh = server
        .dispatch_tool_for_tests("get_file_context", context_params)
        .await;
    assert!(
        !context_fresh.contains("Decision: cache_hit"),
        "a stale publication satisfied get_file_context's repeat cache: {context_fresh}"
    );
    assert!(
        context_fresh.contains("refenced"),
        "get_file_context must observe the fresh identity: {context_fresh}"
    );
}
