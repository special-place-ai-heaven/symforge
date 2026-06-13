//! STEL L1 planner — map [`StelRequest`] to a single-step [`StelPlan`] (economics gate deferred).

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::protocol::smart_query;

use super::types::{
    IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest,
};

struct PlannedStep {
    tool: String,
    args: Value,
    intent: IntentBucket,
    confidence: RouteConfidence,
    rationale: &'static str,
}

/// Build a draft single-step plan for compact `symforge` (L1 → L2).
pub fn build_plan(request: &StelRequest) -> StelPlan {
    let step = plan_step(request);
    StelPlan {
        plan_id: new_plan_id(request),
        intent: step.intent,
        confidence: step.confidence,
        confidence_rationale: step.rationale.to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: step.tool,
            args: step.args,
            est_response_tokens: 400,
            est_manual_tokens: 800,
            index_refs: vec![],
        }],
        suggested_followup: None,
    }
}

/// Envelope plan line: `trace → find_references (exact)`.
pub fn plan_summary_line(plan: &StelPlan) -> String {
    let tool = plan
        .steps
        .first()
        .map(|step| step.tool.as_str())
        .unwrap_or("?");
    format!(
        "{} → {} ({})",
        plan.intent.as_str(),
        tool,
        confidence_label(plan.confidence)
    )
}

pub fn confidence_label(confidence: RouteConfidence) -> &'static str {
    match confidence {
        RouteConfidence::Exact => "exact",
        RouteConfidence::Inferred => "inferred",
        RouteConfidence::Fallback => "fallback",
    }
}

fn plan_step(request: &StelRequest) -> PlannedStep {
    let bucket = request.intent.unwrap_or(IntentBucket::Auto);
    if bucket != IntentBucket::Auto {
        if let Some(step) = route_with_bucket(request, bucket) {
            return step;
        }
    }
    if let Some(step) = route_with_query_patterns(request) {
        return step;
    }
    route_with_smart_query(request)
}

fn route_with_bucket(request: &StelRequest, bucket: IntentBucket) -> Option<PlannedStep> {
    match bucket {
        IntentBucket::Find => Some(route_find(request)),
        IntentBucket::Read => Some(route_read(request)),
        IntentBucket::Trace => Some(route_trace(request)),
        IntentBucket::Impact => Some(route_impact(request)),
        IntentBucket::Orient => Some(route_orient(request)),
        IntentBucket::Meta => Some(route_meta(request)),
        IntentBucket::Edit | IntentBucket::Auto => None,
    }
}

fn route_with_query_patterns(request: &StelRequest) -> Option<PlannedStep> {
    let query = request.query.trim();
    let lower = query.to_ascii_lowercase();

    if let Some(path) = parse_outline_path(query) {
        return Some(planned(
            "get_file_context",
            json!({ "path": path }),
            IntentBucket::Read,
            RouteConfidence::Exact,
            "outline path in query",
        ));
    }

    if let Some((path, name)) = parse_body_of(query) {
        return Some(planned(
            "get_symbol",
            json!({ "path": path, "name": name }),
            IntentBucket::Read,
            RouteConfidence::Exact,
            "body-of symbol request",
        ));
    }

    if let Some(name) = parse_reference_target(query) {
        let mut args = json!({ "name": name, "compact": true });
        if let Some(path) = request.path.as_deref() {
            args["path"] = json!(path);
        }
        return Some(planned(
            "find_references",
            args,
            IntentBucket::Trace,
            RouteConfidence::Exact,
            "caller/reference phrasing",
        ));
    }

    if lower.contains("macro usage") || (lower.starts_with("find ") && lower.contains(" usage")) {
        let term = extract_find_subject(query).unwrap_or_else(|| query.to_string());
        let mut args = json!({ "query": term });
        if let Some(path) = request.path.as_deref() {
            args["path_prefix"] = json!(path);
        } else {
            args["path_prefix"] = json!("src");
        }
        return Some(planned(
            "search_text",
            args,
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "text search usage phrasing",
        ));
    }

    if lower.starts_with("locate ") && lower.contains("symbol") {
        let term = query
            .trim()
            .strip_prefix("locate ")
            .unwrap_or(query)
            .trim()
            .strip_suffix(" symbol")
            .unwrap_or(query)
            .trim()
            .to_string();
        return Some(planned(
            "search_symbols",
            json!({ "query": term }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "symbol locate phrasing",
        ));
    }

    if lower.starts_with("how does ") || lower.starts_with("how do ") {
        return Some(planned(
            "explore",
            json!({ "query": query, "depth": 2 }),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "guidance/explore phrasing",
        ));
    }

    None
}

fn route_with_smart_query(request: &StelRequest) -> PlannedStep {
    let intent = smart_query::classify_intent(request.query.trim());
    let tool = smart_query::route_tool_name(&intent).to_string();
    let intent_bucket = intent_bucket_for_tool(&tool);
    let args = default_args_for_tool(&tool, request);
    PlannedStep {
        tool,
        intent: intent_bucket,
        confidence: RouteConfidence::Inferred,
        rationale: "smart_query fallback",
        args,
    }
}

fn route_find(request: &StelRequest) -> PlannedStep {
    let lower = request.query.to_ascii_lowercase();
    if lower.contains("symbol") || lower.starts_with("locate ") {
        route_with_query_patterns(request).unwrap_or_else(|| {
            planned(
                "search_symbols",
                json!({ "query": request.query.trim() }),
                IntentBucket::Find,
                RouteConfidence::Inferred,
                "find intent symbol search",
            )
        })
    } else {
        route_with_query_patterns(request).unwrap_or_else(|| {
            planned(
                "search_text",
                json!({ "query": request.query.trim() }),
                IntentBucket::Find,
                RouteConfidence::Inferred,
                "find intent text search",
            )
        })
    }
}

fn route_read(request: &StelRequest) -> PlannedStep {
    route_with_query_patterns(request).unwrap_or_else(|| {
        if let Some(path) = request.path.clone() {
            planned(
                "get_file_context",
                json!({ "path": path }),
                IntentBucket::Read,
                RouteConfidence::Exact,
                "read intent with explicit path",
            )
        } else {
            planned(
                "get_file_context",
                json!({ "path": request.query.trim() }),
                IntentBucket::Read,
                RouteConfidence::Fallback,
                "read intent path fallback",
            )
        }
    })
}

fn route_trace(request: &StelRequest) -> PlannedStep {
    route_with_query_patterns(request).unwrap_or_else(|| {
        let name = request
            .symbol
            .clone()
            .unwrap_or_else(|| request.query.trim().to_string());
        let mut args = json!({ "name": name, "compact": true });
        if let Some(path) = request.path.as_deref() {
            args["path"] = json!(path);
        }
        planned(
            "find_references",
            args,
            IntentBucket::Trace,
            RouteConfidence::Inferred,
            "trace intent reference lookup",
        )
    })
}

fn route_impact(request: &StelRequest) -> PlannedStep {
    let path = request
        .path
        .clone()
        .unwrap_or_else(|| request.query.trim().to_string());
    planned(
        "find_dependents",
        json!({ "path": path }),
        IntentBucket::Impact,
        RouteConfidence::Inferred,
        "impact intent dependents lookup",
    )
}

fn route_orient(request: &StelRequest) -> PlannedStep {
    if request.query.to_ascii_lowercase().contains("repo map") {
        planned(
            "get_repo_map",
            json!({}),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "orient intent repo map",
        )
    } else {
        planned(
            "explore",
            json!({ "query": request.query.trim(), "depth": 2 }),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "orient intent explore",
        )
    }
}

fn route_meta(_request: &StelRequest) -> PlannedStep {
    planned(
        "context_inventory",
        json!({}),
        IntentBucket::Meta,
        RouteConfidence::Fallback,
        "meta intent inventory",
    )
}

fn planned(
    tool: &str,
    args: Value,
    intent: IntentBucket,
    confidence: RouteConfidence,
    rationale: &'static str,
) -> PlannedStep {
    PlannedStep {
        tool: tool.to_string(),
        args,
        intent,
        confidence,
        rationale,
    }
}

fn default_args_for_tool(tool: &str, request: &StelRequest) -> Value {
    match tool {
        "search_text" => json!({ "query": request.query.trim() }),
        "search_symbols" => json!({ "query": request.query.trim() }),
        "search_files" => json!({ "query": request.query.trim() }),
        "get_file_context" => json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) }),
        "get_file_content" => json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) }),
        "get_symbol" => json!({
            "path": request.path.clone().unwrap_or_default(),
            "name": request.symbol.clone().unwrap_or_else(|| request.query.trim().to_string()),
        }),
        "find_references" => json!({
            "name": request.symbol.clone().unwrap_or_else(|| request.query.trim().to_string()),
            "compact": true,
        }),
        "find_dependents" => json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) }),
        "explore" => json!({ "query": request.query.trim(), "depth": 2 }),
        "what_changed" => json!({}),
        "get_repo_map" => json!({}),
        _ => json!({ "query": request.query.trim() }),
    }
}

fn intent_bucket_for_tool(tool: &str) -> IntentBucket {
    match tool {
        "search_text" | "search_symbols" | "search_files" => IntentBucket::Find,
        "get_file_context" | "get_file_content" | "get_symbol" => IntentBucket::Read,
        "find_references" | "get_symbol_context" => IntentBucket::Trace,
        "find_dependents" | "what_changed" | "analyze_file_impact" | "diff_symbols" => {
            IntentBucket::Impact
        }
        "explore" | "get_repo_map" | "conventions" => IntentBucket::Orient,
        "context_inventory" | "investigation_suggest" | "ask" => IntentBucket::Meta,
        _ => IntentBucket::Auto,
    }
}

fn new_plan_id(request: &StelRequest) -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("plan-{ms:x}-{}", request.query.len())
}

fn parse_outline_path(query: &str) -> Option<String> {
    let rest = query.trim().to_ascii_lowercase();
    let path = rest.strip_prefix("outline ")?.trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

fn parse_body_of(query: &str) -> Option<(String, String)> {
    let lower = query.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("body of ")?;
    let (name, path_part) = rest.split_once(" in ")?;
    if name.is_empty() || path_part.is_empty() {
        return None;
    }
    Some((normalize_path(path_part.trim()), name.trim().to_string()))
}

fn parse_reference_target(query: &str) -> Option<String> {
    let lower = query.trim().to_ascii_lowercase();
    for prefix in [
        "who references ",
        "who calls ",
        "references to ",
        "callers of ",
    ] {
        if let Some(target) = lower.strip_prefix(prefix) {
            let target = target.trim();
            if !target.is_empty() {
                return Some(target.to_string());
            }
        }
    }
    None
}

fn extract_find_subject(query: &str) -> Option<String> {
    let lower = query.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("find ")?.trim();
    let subject = rest
        .strip_suffix(" macro usage")
        .or_else(|| rest.strip_suffix(" usage"))
        .unwrap_or(rest)
        .trim();
    if subject.is_empty() {
        None
    } else {
        Some(subject.to_string())
    }
}

fn normalize_path(path: &str) -> String {
    if path.contains('/') {
        path.to_string()
    } else {
        format!("src/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn plan_tool(request: StelRequest) -> String {
        build_plan(&request).steps[0].tool.clone()
    }

    #[test]
    fn planner_matches_cfg_if_golden_subset() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(super::super::golden_replay::GOLDEN_ROUTES_FIXTURE);
        let rows = super::super::golden_replay::load_golden_rows(&path).expect("golden fixture");
        let subset = [
            "cfg-if/t1_search",
            "cfg-if/t2_context",
            "cfg-if/t3_symbols",
            "cfg-if/t4_refs",
            "cfg-if/t5_symbol",
        ];
        for id in subset {
            let row = rows.iter().find(|r| r.id == id).expect("row");
            let mut request = row.to_request();
            request.intent = row.intent;
            let plan = build_plan(&request);
            assert_eq!(
                plan.steps[0].tool, row.must_call[0],
                "planner tool mismatch for {id}"
            );
        }
    }

    #[test]
    fn planner_honors_s4_exit_rows() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(super::super::golden_replay::GOLDEN_ROUTES_FIXTURE);
        let rows = super::super::golden_replay::load_golden_rows(&path).expect("golden fixture");
        for row in super::super::golden_replay::s4_exit_rows(&rows) {
            let mut request = row.to_request();
            request.intent = row.intent;
            let plan = build_plan(&request);
            assert_eq!(
                plan.steps[0].tool, row.must_call[0],
                "S4 row {} planner mismatch",
                row.id
            );
        }
    }

    #[test]
    fn plan_summary_uses_planned_tool() {
        let plan = build_plan(&StelRequest {
            query: "who references cfg_if".to_string(),
            intent: None,
            path: None,
            symbol: None,
            max_tokens: None,
            preview: None,
        });
        let summary = plan_summary_line(&plan);
        assert!(summary.contains("find_references"));
        assert!(summary.contains("trace"));
    }

    #[test]
    fn outline_query_plans_get_file_context() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "outline src/lib.rs".to_string(),
                ..Default::default()
            }),
            "get_file_context"
        );
    }
}
