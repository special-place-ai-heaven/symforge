//! Session cache-hit for full read tools (011 US1).

use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::session::hash_symbol_params;
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
async fn get_symbol_repeat_returns_cache_hit() {
    let server = server_for_fixture();
    let params = json!({ "path": "src/main.rs", "name": "main" });
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
            json!({ "path": "src/main.rs", "name": "main", "force_refresh": true }),
        )
        .await;
    assert!(
        !forced.contains("Decision: cache_hit"),
        "force_refresh should bypass cache_hit"
    );
    assert!(forced.len() > 100);
}

#[test]
fn session_detailed_fetch_drives_stel_cache_hit() {
    use symforge::protocol::session::SessionContext;
    use symforge::stel::{
        evaluate_plan_with_session, AdmissionDecision, IntentBucket, RouteConfidence, StelPlan,
        StelPlanStep, StelRequest,
    };

    let session = SessionContext::new();
    session.record_symbol_fetch("src/lib.rs", "foo", hash_symbol_params(None, None, None), 200);
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
    assert_eq!(decision.decision, AdmissionDecision::CacheHit);
}
