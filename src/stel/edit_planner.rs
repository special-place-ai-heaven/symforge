//! STEL L1 edit planner — map [`StelEditRequest`] to a single-step dry-run edit plan.

use std::time::{SystemTime, UNIX_EPOCH};

use super::controller::index_ref_for_target;
use super::types::{
    IntentBucket, RouteConfidence, StelEditOp, StelEditRequest, StelPlan, StelPlanStep,
};

/// The op the request selects, defaulting to `Replace` when omitted (the
/// replace-only legacy contract). Centralizing this keeps validation, planning,
/// and the apply pre-flight agreeing on one source of truth.
pub fn effective_op(request: &StelEditRequest) -> StelEditOp {
    request.op.unwrap_or_default()
}

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
    let old_text = request
        .old_text
        .as_deref()
        .filter(|value| !value.is_empty());
    let new_text = request.new_text.as_deref();

    // The `symbol` field is the named anchor every op addresses: the target for
    // replace, the reference symbol for inserts, the scoping symbol for within.
    if symbol.is_none() {
        return Err(EditValidationError::new(match effective_op(request) {
            StelEditOp::InsertBefore | StelEditOp::InsertAfter => {
                "symbol (insertion anchor) is required for an insert"
            }
            StelEditOp::EditWithin => "symbol (edit scope) is required for edit_within",
            StelEditOp::Replace => "symbol is required for structural edit preview",
        }));
    }

    match effective_op(request) {
        StelEditOp::Replace => {
            if body.is_none() {
                return Err(EditValidationError::new(
                    "body is required for structural edit preview",
                ));
            }
        }
        StelEditOp::InsertBefore | StelEditOp::InsertAfter => {
            if body.is_none() {
                return Err(EditValidationError::new(
                    "body (the new symbol source) is required for an insert",
                ));
            }
        }
        StelEditOp::EditWithin => {
            if old_text.is_none() {
                return Err(EditValidationError::new(
                    "old_text is required for op: edit_within",
                ));
            }
            // `new_text` may be intentionally empty (a deletion within the
            // symbol), so only its PRESENCE is required, not non-emptiness.
            if new_text.is_none() {
                return Err(EditValidationError::new(
                    "new_text is required for op: edit_within",
                ));
            }
        }
    }
    Ok(())
}

/// Build a single-step legacy-tool plan for compact `symforge_edit`.
///
/// Routes by [`effective_op`] to one of the existing internal edit tools, building
/// the args each expects. `Replace` keeps the original `replace_symbol_body`
/// contract verbatim (including the `if_match` optimistic-concurrency guard);
/// inserts route to `insert_symbol`; `edit_within` routes to `edit_within_symbol`.
pub fn build_edit_plan(request: &StelEditRequest) -> Result<StelPlan, EditValidationError> {
    validate_edit_request(request)?;
    let symbol = request.symbol.as_deref().unwrap_or("").trim();
    let path = request.path.trim();
    let dry_run = !super::edit_apply::apply_requested(request);
    let op = effective_op(request);

    // `new_content_len` is the byte length of the new source the op introduces:
    // the replacement body, the inserted symbol source, or the within-symbol
    // replacement text. It grounds the economics estimate the same way the
    // original replace path grounded on `body` (009b), so inserts/within edits
    // get byte-scaled — not flat-floored — predictions. See `grounded_edit_tokens`.
    let (tool, mut args, new_content_len) = match op {
        StelEditOp::Replace => {
            let body = request.body.as_deref().unwrap_or("").trim();
            let mut args = serde_json::json!({
                "path": path,
                "name": symbol,
                "new_body": body,
                "dry_run": dry_run,
            });
            // TR-06 / FR-009: forward the optimistic-concurrency guard so it reaches
            // the write path. Dropping it here was the bug — the pre-flight checked it
            // but the actual write never saw it, leaving a TOCTOU window. The write
            // path (`replace_symbol_body` -> `guarded_atomic_write_file`) re-verifies
            // it against the bytes actually being written. Only `replace_symbol_body`
            // accepts `if_match`; the insert/within internal tools have no such
            // field, so the guard is replace-only (see the apply pre-flight).
            if let Some(if_match) = &request.if_match {
                args["if_match"] = serde_json::json!(if_match);
            }
            ("replace_symbol_body", args, body.len() as u64)
        }
        StelEditOp::InsertBefore | StelEditOp::InsertAfter => {
            let content = request.body.as_deref().unwrap_or("").trim();
            let position = if matches!(op, StelEditOp::InsertBefore) {
                "before"
            } else {
                "after"
            };
            let args = serde_json::json!({
                "path": path,
                "name": symbol,
                "content": content,
                "position": position,
                "dry_run": dry_run,
            });
            ("insert_symbol", args, content.len() as u64)
        }
        StelEditOp::EditWithin => {
            let old_text = request.old_text.as_deref().unwrap_or("");
            let new_text = request.new_text.as_deref().unwrap_or("");
            let args = serde_json::json!({
                "path": path,
                "name": symbol,
                "old_text": old_text,
                "new_text": new_text,
                "replace_all": request.replace_all.unwrap_or(false),
                "dry_run": dry_run,
            });
            ("edit_within_symbol", args, new_text.len() as u64)
        }
    };

    // The replay guard is supported uniformly by all three internal tools.
    if let Some(key) = &request.idempotency_key {
        args["idempotency_key"] = serde_json::json!(key);
    }

    // Worktree routing (beta finding F6): all three internal tools accept
    // `working_directory` and run it through the worktree-awareness edit hook.
    // Dropping it here was the contamination bug — a facade edit issued from a
    // git worktree silently landed in the shared indexed root.
    if let Some(cwd) = &request.working_directory {
        args["working_directory"] = serde_json::json!(cwd);
    }

    Ok(StelPlan {
        plan_id: edit_plan_id(request),
        intent: IntentBucket::Edit,
        confidence: RouteConfidence::Exact,
        confidence_rationale: edit_confidence_rationale(op),
        steps: vec![StelPlanStep {
            order: 1,
            tool: tool.to_string(),
            args,
            // Plan-only floor retained for any caller that ignores `index_refs`;
            // the grounded edit path (`grounded_step_tokens`) overrides both with
            // new-content-byte-scaled figures whenever the IndexRef below is present.
            est_response_tokens: 520,
            est_manual_tokens: 900,
            // Plan 009b edit-economics grounding, generalized across ops: the new
            // source the op introduces (replacement body / inserted symbol /
            // within-symbol replacement) is fully known at plan time, so its byte
            // length is the real input the edit's response/manual baselines scale
            // with. Stamping it as the step's IndexRef routes the edit through the
            // SAME byte-grounded estimator the READ tools use (no parallel
            // estimator), replacing the flat 520/900 floor with figures that move
            // with edit size.
            index_refs: vec![index_ref_for_target(path.to_string(), new_content_len)],
        }],
        suggested_followup: None,
    })
}

/// Confidence rationale string per op (all are exact-routed structural edits).
fn edit_confidence_rationale(op: StelEditOp) -> String {
    match op {
        StelEditOp::Replace => "explicit path, symbol, and body".to_string(),
        StelEditOp::InsertBefore => "explicit path, insert-before anchor, and body".to_string(),
        StelEditOp::InsertAfter => "explicit path, insert-after anchor, and body".to_string(),
        StelEditOp::EditWithin => "explicit path, symbol scope, and old/new text".to_string(),
    }
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

    #[test]
    fn build_edit_plan_forwards_if_match() {
        // TR-06 / FR-009: the optimistic-concurrency guard must reach the
        // write tool's args, not be dropped at planning (the original bug).
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("helper".to_string()),
            body: Some("fn helper() {}".to_string()),
            if_match: Some("fn helper() { old }".to_string()),
            apply: Some(true),
            ..Default::default()
        })
        .expect("valid edit request");
        assert_eq!(
            plan.steps[0].args["if_match"], "fn helper() { old }",
            "if_match must be forwarded into the replace_symbol_body plan args"
        );
    }

    #[test]
    fn build_edit_plan_forwards_working_directory() {
        // F6: dropping `working_directory` at planning was the worktree
        // contamination bug — a facade edit issued from a git worktree landed
        // in the shared indexed root instead of the worktree copy.
        for (op, key) in [
            (None, "working_directory"),
            (Some(StelEditOp::InsertAfter), "working_directory"),
        ] {
            let plan = build_edit_plan(&StelEditRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("helper".to_string()),
                body: Some("fn helper() {}".to_string()),
                op,
                working_directory: Some("/repos/wt_one".to_string()),
                ..Default::default()
            })
            .expect("valid edit request");
            assert_eq!(
                plan.steps[0].args[key], "/repos/wt_one",
                "working_directory must be forwarded into the {} plan args",
                plan.steps[0].tool
            );
        }
    }

    #[test]
    fn build_edit_plan_omits_working_directory_when_absent() {
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("helper".to_string()),
            body: Some("fn helper() {}".to_string()),
            ..Default::default()
        })
        .expect("valid edit request");
        assert!(
            plan.steps[0].args.get("working_directory").is_none(),
            "absent working_directory must not appear in plan args"
        );
    }

    #[test]
    fn build_edit_plan_default_op_routes_to_replace_symbol_body() {
        // Backward-compat: a path/symbol/body request with NO `op` must keep
        // routing to replace_symbol_body with the original arg shape.
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("helper".to_string()),
            body: Some("fn helper() {}".to_string()),
            ..Default::default()
        })
        .expect("valid replace request");
        assert_eq!(plan.steps[0].tool, "replace_symbol_body");
        assert_eq!(plan.steps[0].args["name"], "helper");
        assert_eq!(plan.steps[0].args["new_body"], "fn helper() {}");
        assert!(plan.steps[0].args.get("content").is_none());
        assert!(plan.steps[0].args.get("old_text").is_none());
    }

    #[test]
    fn build_edit_plan_insert_after_routes_to_insert_symbol() {
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("anchor".to_string()),
            body: Some("fn added() {}".to_string()),
            op: Some(StelEditOp::InsertAfter),
            ..Default::default()
        })
        .expect("valid insert request");
        assert_eq!(plan.steps[0].tool, "insert_symbol");
        assert_eq!(plan.steps[0].args["name"], "anchor");
        assert_eq!(plan.steps[0].args["content"], "fn added() {}");
        assert_eq!(plan.steps[0].args["position"], "after");
        assert_eq!(plan.steps[0].args["dry_run"], true);
        // `new_body` is replace-only and must not leak into an insert.
        assert!(plan.steps[0].args.get("new_body").is_none());
    }

    #[test]
    fn build_edit_plan_insert_before_sets_position_before() {
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("anchor".to_string()),
            body: Some("use std::fmt;".to_string()),
            op: Some(StelEditOp::InsertBefore),
            ..Default::default()
        })
        .expect("valid insert request");
        assert_eq!(plan.steps[0].tool, "insert_symbol");
        assert_eq!(plan.steps[0].args["position"], "before");
    }

    #[test]
    fn build_edit_plan_edit_within_routes_to_edit_within_symbol() {
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("module".to_string()),
            old_text: Some("use a::b;".to_string()),
            new_text: Some("use a::{b, c};".to_string()),
            op: Some(StelEditOp::EditWithin),
            ..Default::default()
        })
        .expect("valid within request");
        assert_eq!(plan.steps[0].tool, "edit_within_symbol");
        assert_eq!(plan.steps[0].args["name"], "module");
        assert_eq!(plan.steps[0].args["old_text"], "use a::b;");
        assert_eq!(plan.steps[0].args["new_text"], "use a::{b, c};");
        assert_eq!(plan.steps[0].args["replace_all"], false);
        // `body`/`new_body` are not part of a within edit.
        assert!(plan.steps[0].args.get("new_body").is_none());
    }

    #[test]
    fn build_edit_plan_edit_within_forwards_replace_all() {
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("module".to_string()),
            old_text: Some("foo".to_string()),
            new_text: Some("bar".to_string()),
            replace_all: Some(true),
            op: Some(StelEditOp::EditWithin),
            ..Default::default()
        })
        .expect("valid within request");
        assert_eq!(plan.steps[0].args["replace_all"], true);
    }

    #[test]
    fn build_edit_plan_insert_grounds_economics_on_content_len() {
        // Economics honesty: the insert step must carry the INSERTED content's
        // byte length as its IndexRef (not the body floor), so predictions scale.
        let content = "fn added() { /* some inserted body */ }";
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("anchor".to_string()),
            body: Some(content.to_string()),
            op: Some(StelEditOp::InsertAfter),
            ..Default::default()
        })
        .expect("valid insert request");
        assert_eq!(plan.steps[0].index_refs.len(), 1);
        assert_eq!(plan.steps[0].index_refs[0].raw_chars, content.len() as u64);
    }

    #[test]
    fn build_edit_plan_edit_within_grounds_economics_on_new_text_len() {
        let new_text = "use a::{b, c, d, e, f};";
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("module".to_string()),
            old_text: Some("use a::b;".to_string()),
            new_text: Some(new_text.to_string()),
            op: Some(StelEditOp::EditWithin),
            ..Default::default()
        })
        .expect("valid within request");
        assert_eq!(plan.steps[0].index_refs[0].raw_chars, new_text.len() as u64);
    }

    #[test]
    fn validate_insert_requires_anchor_symbol_and_body() {
        let err = validate_edit_request(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            op: Some(StelEditOp::InsertAfter),
            body: Some("fn x() {}".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.message.contains("anchor"), "msg: {}", err.message);

        let err = validate_edit_request(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            op: Some(StelEditOp::InsertAfter),
            symbol: Some("anchor".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.message.contains("body"), "msg: {}", err.message);
    }

    #[test]
    fn validate_edit_within_requires_old_and_new_text() {
        let err = validate_edit_request(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            op: Some(StelEditOp::EditWithin),
            symbol: Some("module".to_string()),
            new_text: Some("x".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.message.contains("old_text"), "msg: {}", err.message);

        let err = validate_edit_request(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            op: Some(StelEditOp::EditWithin),
            symbol: Some("module".to_string()),
            old_text: Some("x".to_string()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.message.contains("new_text"), "msg: {}", err.message);
    }

    #[test]
    fn validate_edit_within_allows_empty_new_text_for_deletion() {
        // Deleting text within a symbol is a legitimate within-edit: new_text is
        // present-but-empty. Only PRESENCE is required, not non-emptiness.
        validate_edit_request(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            op: Some(StelEditOp::EditWithin),
            symbol: Some("module".to_string()),
            old_text: Some(", unused".to_string()),
            new_text: Some(String::new()),
            ..Default::default()
        })
        .expect("empty new_text is a valid deletion");
    }

    #[test]
    fn build_edit_plan_omits_if_match_when_absent() {
        // Without an `if_match`, the plan args must not carry the key at all so
        // the write path stays on the unguarded (presence-off) fast path.
        let plan = build_edit_plan(&StelEditRequest {
            path: "src/lib.rs".to_string(),
            symbol: Some("helper".to_string()),
            body: Some("fn helper() {}".to_string()),
            apply: Some(true),
            ..Default::default()
        })
        .expect("valid edit request");
        assert!(
            plan.steps[0].args.get("if_match").is_none(),
            "absent if_match must not appear in plan args"
        );
    }
}
