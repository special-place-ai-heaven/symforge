//! STEL L1 planner — map [`StelRequest`] to a single-step [`StelPlan`] (L2 scores separately).

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::protocol::smart_query;

use super::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest};

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
    if bucket != IntentBucket::Auto
        && let Some(step) = route_with_bucket(request, bucket)
    {
        return step;
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

    if let Some(name) = parse_bare_references_target(query) {
        return Some(planned(
            "find_references",
            json!({ "name": name, "compact": true }),
            IntentBucket::Trace,
            RouteConfidence::Inferred,
            "references {symbol} phrasing",
        ));
    }

    if lower.contains("repo map") {
        return Some(planned(
            "get_repo_map",
            json!({}),
            IntentBucket::Orient,
            RouteConfidence::Inferred,
            "repo map phrasing",
        ));
    }

    if lower == "index health" {
        return Some(planned(
            "health_compact",
            json!({}),
            IntentBucket::Meta,
            RouteConfidence::Exact,
            "index health probe",
        ));
    }

    if let Some((path, max_lines)) = parse_bounded_content_read(query, &lower) {
        let mut args = json!({ "path": path });
        if let Some(max_lines) = max_lines {
            args["max_lines"] = json!(max_lines);
        }
        return Some(planned(
            "get_file_content",
            args,
            IntentBucket::Read,
            RouteConfidence::Inferred,
            "bounded file content read",
        ));
    }

    if let Some((name, path)) = parse_symbol_in_path(query, &lower) {
        return Some(planned(
            "get_symbol",
            json!({ "path": path, "name": name }),
            IntentBucket::Read,
            RouteConfidence::Inferred,
            "symbol-in-path phrasing",
        ));
    }

    if let Some(name) = parse_symbol_body_phrase(query, &lower) {
        return Some(planned(
            "get_symbol",
            json!({ "name": name }),
            IntentBucket::Read,
            RouteConfidence::Inferred,
            "symbol body phrasing",
        ));
    }

    if let Some(name) = parse_trailing_symbol_phrase(query, &lower) {
        return Some(planned(
            "search_symbols",
            json!({ "query": name }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "trailing symbol phrasing",
        ));
    }

    if let Some(hint) = parse_files_named_hint(query, &lower) {
        return Some(planned(
            "search_files",
            json!({ "query": hint }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "files named phrasing",
        ));
    }

    if let Some(hint) = parse_files_for_hint(query, &lower) {
        return Some(planned(
            "search_files",
            json!({ "query": hint }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "files for phrasing",
        ));
    }

    if let Some(term) = parse_find_entity_search(query, &lower) {
        return Some(planned(
            "search_text",
            json!({ "query": term }),
            IntentBucket::Find,
            RouteConfidence::Inferred,
            "find class/function phrasing",
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
        "get_file_context" => {
            json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) })
        }
        "get_file_content" => {
            json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) })
        }
        "get_symbol" => json!({
            "path": request.path.clone().unwrap_or_default(),
            "name": request.symbol.clone().unwrap_or_else(|| request.query.trim().to_string()),
        }),
        "find_references" => json!({
            "name": request.symbol.clone().unwrap_or_else(|| request.query.trim().to_string()),
            "compact": true,
        }),
        "find_dependents" => {
            json!({ "path": request.path.clone().unwrap_or_else(|| request.query.trim().to_string()) })
        }
        "explore" => json!({ "query": request.query.trim(), "depth": 2 }),
        "what_changed" => json!({}),
        "get_repo_map" => json!({}),
        "health_compact" => json!({}),
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
        "context_inventory" | "investigation_suggest" | "ask" | "health_compact" => {
            IntentBucket::Meta
        }
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
                return slice_after_prefix(query, prefix);
            }
        }
    }
    None
}

fn parse_bare_references_target(query: &str) -> Option<String> {
    const PREFIX: &str = "references ";
    let lower = query.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix(PREFIX)?.trim();
    if rest.is_empty() || rest.starts_with("to ") {
        return None;
    }
    slice_after_prefix(query, PREFIX)
}

fn parse_bounded_content_read(query: &str, lower: &str) -> Option<(String, Option<u32>)> {
    if let Some(rest) = lower.strip_prefix("first ")
        && let Some((lines, path)) = rest.split_once(" lines ")
    {
        let lines = lines.trim().parse().ok()?;
        let path = path.trim();
        if !path.is_empty() {
            return Some((normalize_path(path), Some(lines)));
        }
    }
    if let Some(rest) = lower.strip_prefix("read ") {
        if let Some((path, limit)) = rest.split_once(" limit ") {
            let lines = limit.trim().parse().ok()?;
            let path = path.trim();
            if !path.is_empty() {
                return Some((path.to_string(), Some(lines)));
            }
        }
        if rest.ends_with(" header") {
            let path = rest.strip_suffix(" header")?.trim();
            if !path.is_empty() {
                return Some((path.to_string(), Some(40)));
            }
        }
    }
    let _ = query;
    None
}

fn parse_symbol_in_path(query: &str, lower: &str) -> Option<(String, String)> {
    let marker = " symbol in ";
    let idx = lower.rfind(marker)?;
    let name = query[..idx].trim();
    let path = query[idx + marker.len()..].trim();
    if name.is_empty() || path.is_empty() {
        return None;
    }
    Some((name.to_string(), path.to_string()))
}

fn parse_symbol_body_phrase(query: &str, lower: &str) -> Option<String> {
    if lower.ends_with(" symbol body") {
        let name = query.trim().strip_suffix(" symbol body")?.trim();
        if name.is_empty() {
            return None;
        }
        return Some(name.to_string());
    }
    if lower.ends_with(" body") && !lower.starts_with("body of ") {
        let name = query.trim().strip_suffix(" body")?.trim();
        if name.is_empty() || name.contains(' ') {
            return None;
        }
        return Some(name.to_string());
    }
    None
}

fn parse_trailing_symbol_phrase(query: &str, lower: &str) -> Option<String> {
    if !lower.ends_with(" symbol") || lower.starts_with("locate ") {
        return None;
    }
    let name = query.trim().strip_suffix(" symbol")?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_files_named_hint(query: &str, lower: &str) -> Option<String> {
    let rest = lower.strip_prefix("files named ")?.trim();
    if rest.is_empty() {
        return None;
    }
    slice_after_prefix(query, "files named ")
}

fn parse_files_for_hint(query: &str, lower: &str) -> Option<String> {
    let marker = " files for ";
    let idx = lower.find(marker)?;
    let hint = query[idx + marker.len()..].trim();
    if hint.is_empty() {
        None
    } else {
        Some(hint.to_string())
    }
}

fn parse_find_entity_search(query: &str, lower: &str) -> Option<String> {
    if !lower.starts_with("find ") {
        return None;
    }
    let term = if lower.ends_with(" function") {
        query
            .trim()
            .strip_prefix("find ")
            .and_then(|s| s.strip_suffix(" function"))
            .map(str::trim)
    } else if lower.contains(" class") {
        query
            .trim()
            .strip_prefix("find ")
            .and_then(|s| s.strip_suffix(" class"))
            .map(str::trim)
    } else if lower.ends_with(" check") {
        query
            .trim()
            .strip_prefix("find ")
            .and_then(|s| s.strip_suffix(" check"))
            .map(str::trim)
    } else {
        None
    }?;
    if term.is_empty() {
        None
    } else {
        Some(term.to_string())
    }
}

fn slice_after_prefix(query: &str, prefix: &str) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let pos = lower.find(&prefix.to_ascii_lowercase())?;
    let value = query[pos + prefix.len()..].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
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
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(super::super::golden_replay::GOLDEN_ROUTES_FIXTURE);
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
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(super::super::golden_replay::GOLDEN_ROUTES_FIXTURE);
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

    #[test]
    fn repo_map_phrasing_plans_get_repo_map() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "repo map cfg-if".to_string(),
                ..Default::default()
            }),
            "get_repo_map"
        );
    }

    #[test]
    fn bounded_content_reads_plan_get_file_content() {
        let req = StelRequest {
            query: "first 80 lines lib.rs".to_string(),
            ..Default::default()
        };
        let plan = build_plan(&req);
        assert_eq!(plan.steps[0].tool, "get_file_content");
        assert_eq!(plan.steps[0].args["path"], "src/lib.rs");
        assert_eq!(plan.steps[0].args["max_lines"], 80);

        assert_eq!(
            plan_tool(StelRequest {
                query: "read index.js limit 80".to_string(),
                ..Default::default()
            }),
            "get_file_content"
        );
    }

    #[test]
    fn find_entity_phrasing_plans_search_text() {
        for query in [
            "find reconcile function",
            "find Database class",
            "find plainObject check",
        ] {
            assert_eq!(
                plan_tool(StelRequest {
                    query: query.to_string(),
                    ..Default::default()
                }),
                "search_text",
                "query: {query}"
            );
        }
    }

    #[test]
    fn bare_references_phrasing_plans_find_references() {
        let plan = build_plan(&StelRequest {
            query: "references isPlainObject".to_string(),
            ..Default::default()
        });
        assert_eq!(plan.steps[0].tool, "find_references");
        assert_eq!(plan.steps[0].args["name"], "isPlainObject");
    }

    #[test]
    fn symbol_body_and_in_path_phrasing_plan_get_symbol() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "Database symbol in records.py".to_string(),
                ..Default::default()
            }),
            "get_symbol"
        );
        assert_eq!(
            plan_tool(StelRequest {
                query: "reconcile symbol body".to_string(),
                ..Default::default()
            }),
            "get_symbol"
        );
        assert_eq!(
            plan_tool(StelRequest {
                query: "isPlainObject body".to_string(),
                ..Default::default()
            }),
            "get_symbol"
        );
    }

    #[test]
    fn trailing_symbol_phrasing_plans_search_symbols() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "isPlainObject symbol".to_string(),
                ..Default::default()
            }),
            "search_symbols"
        );
    }

    #[test]
    fn files_search_phrasing_plans_search_files() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "files named records".to_string(),
                ..Default::default()
            }),
            "search_files"
        );
        assert_eq!(
            plan_tool(StelRequest {
                query: "test files for plain object".to_string(),
                ..Default::default()
            }),
            "search_files"
        );
    }

    #[test]
    fn index_health_plans_health_compact() {
        assert_eq!(
            plan_tool(StelRequest {
                query: "index health".to_string(),
                ..Default::default()
            }),
            "health_compact"
        );
    }
}
