//! L2 admission hardening — serve, degrade, bypass, cache_hit states.
#![cfg(feature = "server")]

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::path::PathBuf;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::session::SessionContext;
use symforge::stel::{
    self, AdmissionDecision, IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest,
    apply_degrade_to_plan, evaluate_plan, evaluate_plan_with_session,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn corpus_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

fn corpus_available(relative: &str, marker: &str) -> bool {
    corpus_path(relative).join(marker).is_file()
}

fn corpora_available() -> bool {
    corpus_available(stel::S4_REPLAY_CORPUS, "src/lib.rs")
}

fn server_for_corpus(relative: &str, project: &str) -> SymForgeServer {
    let root = corpus_path(relative);
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

fn tool_result_text(result: &serde_json::Value) -> &str {
    result["content"][0]["text"]
        .as_str()
        .expect("symforge result must contain text content")
}

async fn dispatch_symforge(server: &SymForgeServer, request: StelRequest) -> String {
    let params = serde_json::to_value(stel::SymforgeCallInput {
        request,
        probe_legacy_tool: None,
        probe_legacy_args: None,
    })
    .expect("symforge params serialize");
    let result = server
        .dispatch_tool_result_for_tests("symforge", params)
        .await
        .expect("symforge dispatch");
    let serialized = serde_json::to_value(&result).expect("serialize CallToolResult");
    tool_result_text(&serialized).to_string()
}

fn low_net_plan() -> StelPlan {
    StelPlan {
        plan_id: "low-net".to_string(),
        intent: IntentBucket::Read,
        confidence: RouteConfidence::Inferred,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "get_file_context".to_string(),
            args: serde_json::json!({ "path": "src/lib.rs" }),
            est_response_tokens: 900,
            est_manual_tokens: 100,
            index_refs: vec![],
        }],
        suggested_followup: None,
    }
}

fn marginal_degrade_plan() -> StelPlan {
    StelPlan {
        plan_id: "degrade".to_string(),
        intent: IntentBucket::Read,
        confidence: RouteConfidence::Inferred,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "get_file_context".to_string(),
            args: serde_json::json!({ "path": "src/lib.rs" }),
            est_response_tokens: 400,
            est_manual_tokens: 530,
            index_refs: vec![],
        }],
        suggested_followup: None,
    }
}

#[test]
fn l2_controller_covers_all_four_admission_states() {
    let serve_request = StelRequest {
        query: "who references cfg_if".to_string(),
        ..Default::default()
    };
    let serve_plan = stel::build_plan(&serve_request);
    let serve = evaluate_plan(&serve_request, &serve_plan);
    assert_eq!(serve.decision, AdmissionDecision::Serve);

    let bypass = evaluate_plan(&StelRequest::default(), &low_net_plan());
    assert_eq!(bypass.decision, AdmissionDecision::Bypass);
    assert!(bypass.bypass.is_some());

    let degrade = evaluate_plan(&StelRequest::default(), &marginal_degrade_plan());
    assert_eq!(degrade.decision, AdmissionDecision::Degrade);
    assert!(!degrade.degrade_flags.is_empty());

    let session = SessionContext::new();
    session.record_symbol("src/lib.rs", "cfg_if", 128);
    let cache_plan = StelPlan {
        plan_id: "cache".to_string(),
        intent: IntentBucket::Read,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "get_symbol".to_string(),
            args: serde_json::json!({ "path": "src/lib.rs", "name": "cfg_if" }),
            est_response_tokens: 400,
            est_manual_tokens: 800,
            index_refs: vec![],
        }],
        suggested_followup: None,
    };
    let cache_hit =
        evaluate_plan_with_session(&StelRequest::default(), &cache_plan, Some(&session));
    assert_eq!(cache_hit.decision, AdmissionDecision::CacheHit);
    assert!(cache_hit.cache.is_some());
}

#[test]
fn degrade_decision_is_distinct_from_bypass_and_serve_metadata() {
    let bypass = evaluate_plan(&StelRequest::default(), &low_net_plan());
    let degrade = evaluate_plan(&StelRequest::default(), &marginal_degrade_plan());
    assert_ne!(bypass.decision, degrade.decision);
    assert!(bypass.bypass.is_some());
    assert!(degrade.bypass.is_none());
    assert!(!degrade.degrade_flags.is_empty());
    assert!(degrade.steps.is_some());
}

#[test]
fn apply_degrade_injects_outline_only_sections() {
    let plan = marginal_degrade_plan();
    let decision = evaluate_plan(&StelRequest::default(), &plan);
    let degraded = apply_degrade_to_plan(&plan, &decision);
    let args = degraded.steps[0].args.as_object().expect("object args");
    assert_eq!(args["sections"], serde_json::json!(["outline"]));
}

#[tokio::test]
async fn cache_hit_dispatch_skips_legacy_tools_after_session_prefetch() {
    if !corpora_available() {
        eprintln!("skip cache_hit_dispatch: missing corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let server = server_for_corpus(stel::S4_REPLAY_CORPUS, "l2-cache-hit");
    let request = StelRequest {
        query: "body of cfg_if in src/lib.rs".to_string(),
        ..Default::default()
    };

    let first = dispatch_symforge(&server, request.clone()).await;
    assert!(
        first.contains("decision: serve"),
        "prefetch should serve first:\n{first}"
    );
    assert!(first.contains("Chosen tool: get_symbol"));

    let second = dispatch_symforge(&server, request).await;
    assert!(
        second.contains("decision: cache_hit"),
        "repeat should cache_hit:\n{second}"
    );
    assert!(second.contains("did not re-execute a legacy tool"));
    assert!(!second.contains("Chosen tool: get_symbol"));

    let event = server.stel_ledger().lock().last().expect("ledger event");
    assert_eq!(event.decision, AdmissionDecision::CacheHit);
    assert!(event.tools_called.is_empty());
    assert_eq!(event.cache_hit, Some(true));
}

/// T035 (SC-005, US5 AC-1) end-to-end: the SAME read operation over two real
/// files of materially different size yields DIFFERENT live predicted figures.
/// Proven through the production serve path (`build_plan` → index-aware
/// grounding → L2 gate → envelope), not a hand-built plan.
#[tokio::test]
async fn grounded_predictions_differ_by_real_file_size_end_to_end() {
    let small_corpus = "tests/fixtures/stel_multi_hop/is-plain-obj-ts"; // test.js ~134 B
    let large_corpus = "tests/fixtures/compression_ratio/rust"; // service.rs ~2.2 KB
    if !corpus_available(small_corpus, "test.js") || !corpus_available(large_corpus, "service.rs") {
        eprintln!("skip grounded_predictions_differ: missing checked-in corpora");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let small_server = server_for_corpus(small_corpus, "ground-small");
    let large_server = server_for_corpus(large_corpus, "ground-large");

    let small = dispatch_symforge(
        &small_server,
        StelRequest {
            query: "outline test.js".to_string(),
            preview: Some(true),
            ..Default::default()
        },
    )
    .await;
    let large = dispatch_symforge(
        &large_server,
        StelRequest {
            query: "outline service.rs".to_string(),
            preview: Some(true),
            ..Default::default()
        },
    )
    .await;

    // The preview body is `envelope + "\n\n" + pretty-printed StelEstimate JSON`;
    // the JSON object is the trailing block starting at the first `{`.
    fn parse_preview_estimate(output: &str) -> serde_json::Value {
        let json_start = output
            .find('{')
            .unwrap_or_else(|| panic!("preview output missing JSON estimate:\n{output}"));
        serde_json::from_str(output[json_start..].trim())
            .unwrap_or_else(|e| panic!("preview must be StelEstimate JSON: {e}\n{output}"))
    }
    let small_est = parse_preview_estimate(&small);
    let large_est = parse_preview_estimate(&large);

    let small_manual = small_est["predicted_manual_tokens"].as_i64().unwrap();
    let large_manual = large_est["predicted_manual_tokens"].as_i64().unwrap();
    assert_ne!(
        small_manual, large_manual,
        "grounded manual baseline must differ by real file size (small={small_manual} large={large_manual})"
    );
    assert!(
        large_manual > small_manual,
        "bigger file ⇒ bigger manual baseline (small={small_manual} large={large_manual})"
    );
}

/// T036 (US5 AC-2, TR-04b, N-2) end-to-end: a read over a trivially small real
/// file reaches the economics BYPASS branch on the LIVE serve path — the
/// adaptive economics is no longer parked permanently in `serve` by the
/// 400/800 constant. The tiny file's competent-manual baseline is below
/// SymForge's fixed schema+invoke overhead, so the gate correctly tells the
/// agent to host-read it directly.
#[tokio::test]
async fn grounded_small_file_reaches_bypass_end_to_end() {
    let small_corpus = "tests/fixtures/stel_multi_hop/is-plain-obj-ts"; // test.js ~134 B
    if !corpus_available(small_corpus, "test.js") {
        eprintln!("skip grounded_small_file_reaches_bypass: missing checked-in corpus");
        return;
    }

    let _guard = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let server = server_for_corpus(small_corpus, "ground-bypass");
    let output = dispatch_symforge(
        &server,
        StelRequest {
            query: "outline test.js".to_string(),
            ..Default::default()
        },
    )
    .await;

    assert!(
        output.contains("decision: bypass"),
        "tiny real file must reach economics bypass on the live path:\n{output}"
    );
    assert!(
        output.contains("did not execute a legacy tool"),
        "economics bypass must instruct a host-read, not execute a tool:\n{output}"
    );
    let event = server.stel_ledger().lock().last().expect("ledger event");
    assert_eq!(event.decision, AdmissionDecision::Bypass);
}

#[test]
fn calibration_summary_counts_degrade_and_cache_hit() {
    use symforge::stel::ledger::{LedgerCaptureInput, capture_ledger};
    use symforge::stel::{estimate_economics, summarize_calibration};

    let degrade_plan = marginal_degrade_plan();
    let degrade_decision = evaluate_plan(&StelRequest::default(), &degrade_plan);
    let degrade_economics = estimate_economics(&degrade_plan);
    let (degrade_event, _) = capture_ledger(&LedgerCaptureInput {
        plan: &degrade_plan,
        decision: &degrade_decision,
        economics: &degrade_economics,
        selected_tool: "get_file_context",
        tools_called: None,
        legacy_executed: true,
        output_body: "Economics: degrade",
        surface: "symforge",
    });

    let session = SessionContext::new();
    session.record_symbol("src/lib.rs", "cfg_if", 128);
    let cache_plan = StelPlan {
        plan_id: "cache".to_string(),
        intent: IntentBucket::Read,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "test".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "get_symbol".to_string(),
            args: serde_json::json!({ "path": "src/lib.rs", "name": "cfg_if" }),
            est_response_tokens: 400,
            est_manual_tokens: 800,
            index_refs: vec![],
        }],
        suggested_followup: None,
    };
    let cache_decision =
        evaluate_plan_with_session(&StelRequest::default(), &cache_plan, Some(&session));
    let cache_economics = estimate_economics(&cache_plan);
    let (cache_event, _) = capture_ledger(&LedgerCaptureInput {
        plan: &cache_plan,
        decision: &cache_decision,
        economics: &cache_economics,
        selected_tool: "get_symbol",
        tools_called: None,
        legacy_executed: false,
        output_body: "Decision: cache_hit",
        surface: "symforge",
    });

    let summary = summarize_calibration(&[degrade_event, cache_event]);
    assert_eq!(summary.degrade_count, 1);
    assert_eq!(summary.cache_hit_count, 1);
    assert_eq!(summary.serve_count, 0);
    assert_eq!(summary.bypass_count, 0);
}
