//! STEL L1 edit planner — map [`StelEditRequest`] to a single-step dry-run edit plan.

use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{IntentBucket, RouteConfidence, StelEditRequest, StelPlan, StelPlanStep};

/// Validation failure before an edit plan can be built.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditValidationError {
    pub message: String,
}

impl EditValidationError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Validate compact-surface edit inputs before planning.
pub fn validate_edit_request(request: &StelEditRequest) -> Result<(), EditValidationError> {
    let path = request.path.trim();
    if path.is_empty() {
        return Err(EditValidationError::new("path is required"));
    }
    if path.contains("..") {
        return Err(EditValidationError::new(
            "path must not contain parent traversal (`..`)",
        ));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(EditValidationError::new(
            "path must be repository-relative, not absolute",
        ));
    }
    if path.contains(':') {
        return Err(EditValidationError::new(
            "path must be repository-relative (no drive or scheme prefixes)",
        ));
    }
    let symbol = request
        .symbol
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let body = request
        .body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if symbol.is_none() {
        return Err(EditValidationError::new(
            "symbol is required for structural edit preview",
        ));
    }
    if body.is_none() {
        return Err(EditValidationError::new(
            "body is required for structural edit preview",
        ));
    }
    Ok(())
}

/// Build a single-step `replace_symbol_body` plan for compact `symforge_edit`.
pub fn build_edit_plan(request: &StelEditRequest) -> Result<StelPlan, EditValidationError> {
    validate_edit_request(request)?;
    let symbol = request.symbol.as_deref().unwrap_or("").trim();
    let body = request.body.as_deref().unwrap_or("").trim();
    let path = request.path.trim();
    let dry_run = !super::edit_apply::apply_requested(request);
    let mut args = serde_json::json!({
        "path": path,
        "name": symbol,
        "new_body": body,
        "dry_run": dry_run,
    });
    if let Some(key) = &request.idempotency_key {
        args["idempotency_key"] = serde_json::json!(key);
    }
    Ok(StelPlan {
        plan_id: edit_plan_id(request),
        intent: IntentBucket::Edit,
        confidence: RouteConfidence::Exact,
        confidence_rationale: "explicit path, symbol, and body".to_string(),
        steps: vec![StelPlanStep {
            order: 1,
            tool: "replace_symbol_body".to_string(),
            args,
            est_response_tokens: 520,
            est_manual_tokens: 900,
            index_refs: vec![],
        }],
        suggested_followup: None,
    })
}

/// Envelope plan line for edit preview (`edit → replace_symbol_body (exact)`).
pub fn edit_plan_summary_line(plan: &StelPlan) -> String {
    let tool = plan
        .steps
        .first()
        .map(|step| step.tool.as_str())
        .unwrap_or("?");
    format!(
        "{} → {} ({})",
        plan.intent.as_str(),
        tool,
        super::planner::confidence_label(plan.confidence)
    )
}

fn edit_plan_id(request: &StelEditRequest) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path_token = request.path.trim().replace(['/', '\\'], "-");
    format!("stel-edit-{path_token}-{ts}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_empty_path() {
        let err = validate_edit_request(&StelEditRequest::default()).unwrap_err();
        assert!(err.message.contains("path"));
    }

    #[test]
    fn validate_rejects_parent_traversal() {
        let err = validate_edit_request(&StelEditRequest {
            path: "../secret.rs".to_string(),
            symbol: Some("foo".to_string()),
            body: Some("fn foo() {}".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.message.contains(".."));
    }

    #[test]
    fn validate_requires_symbol_and_body() {
        let err = validate_edit_request(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.message.contains("symbol"));

        let err = validate_edit_request(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("foo".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.message.contains("body"));
    }

    #[test]
    fn build_edit_plan_emits_dry_run_replace_symbol_body() {
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("helper".to_string()),
            body: Some("fn helper() {}".to_string()),
            ..Default::default()
        })
        .expect("valid edit request");
        assert_eq!(plan.intent, IntentBucket::Edit);
        assert_eq!(plan.steps[0].tool, "replace_symbol_body");
        assert_eq!(plan.steps[0].args["dry_run"], true);
        assert_eq!(plan.steps[0].args["name"], "helper");
    }

    #[test]
    fn build_edit_plan_apply_sets_dry_run_false() {
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("helper".to_string()),
            body: Some("fn helper() {}".to_string()),
            apply: Some(true),
            ..Default::default()
        })
        .expect("valid edit request");
        assert_eq!(plan.steps[0].args["dry_run"], false);
    }
}
