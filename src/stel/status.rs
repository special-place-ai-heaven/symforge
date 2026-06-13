//! STEL compact `status` tool — operational surface report (no calibration/edit).

use super::calibration::{format_calibration_section, summarize_calibration, StelCalibrationSummary};
use super::ledger::SessionLedger;
use super::types::{StelStatusDetail, StelStatusRequest};

/// Phase 0 §12A independent GO anchor (authorization to implement `src/stel/`).
pub const PHASE0_GO_COMMIT: &str = "07b42a8";
/// Phase 0 evidence bundle anchor (A-019 measurement artifacts).
pub const PHASE0_EVIDENCE_COMMIT: &str = "08f7d14";

/// Stable comma-separated deferred-work list (sorted for test stability).
pub const DEFERRED_ITEMS: &str =
    "b_results,calibration_auto_tune,ledger_persistence,multi_step_planner";

/// Inputs collected from the live server when formatting a status response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StelStatusContext<'a> {
    pub surface: &'static str,
    pub version: &'static str,
    pub project_name: &'a str,
    pub index_ready: bool,
    pub index_files: usize,
    pub index_symbols: usize,
    pub ledger_events: usize,
    pub session_tokens: u64,
    pub last_ledger_decision: Option<String>,
    pub last_ledger_route: Option<String>,
    pub calibration: StelCalibrationSummary,
}

impl<'a> StelStatusContext<'a> {
    pub fn from_server(
        surface: &'static str,
        project_name: &'a str,
        index_ready: bool,
        index_files: usize,
        index_symbols: usize,
        ledger: &SessionLedger,
        session_tokens: u64,
    ) -> Self {
        let events = ledger.events();
        let calibration = summarize_calibration(&events);
        let last = events.last();
        let last_ledger_decision = last
            .as_ref()
            .map(|event| event.decision.as_str().to_string());
        let last_ledger_route = last.and_then(|event| event.tools_called.first().cloned());
        Self {
            surface,
            version: env!("CARGO_PKG_VERSION"),
            project_name,
            index_ready,
            index_files,
            index_symbols,
            ledger_events: ledger.len(),
            session_tokens,
            last_ledger_decision,
            last_ledger_route,
            calibration,
        }
    }
}

/// Format the compact-surface `status` tool body.
pub fn format_stel_status(request: &StelStatusRequest, ctx: &StelStatusContext<'_>) -> String {
    let detail = request.detail.unwrap_or(StelStatusDetail::Compact);
    match detail {
        StelStatusDetail::Compact => format_compact_status(ctx),
        StelStatusDetail::Full => format_full_status(ctx),
    }
}

fn format_compact_status(ctx: &StelStatusContext<'_>) -> String {
    let lines = vec![
        "── stel status ──".to_string(),
        format!("surface: {}", ctx.surface),
        format!("symforge_version: {}", ctx.version),
        format!("phase0_go: {PHASE0_GO_COMMIT}"),
        format!("phase0_evidence: {PHASE0_EVIDENCE_COMMIT}"),
        "l1_planner: active".to_string(),
        "l2_economics: active".to_string(),
        "l3_bypass: active".to_string(),
        "l4_ledger: active".to_string(),
        "handler_symforge: active".to_string(),
        "handler_status: active".to_string(),
        "handler_symforge_edit: preview-and-apply".to_string(),
        format!("ledger_events: {}", ctx.ledger_events),
        format!("index_ready: {}", ctx.index_ready),
        format!("index_files: {}", ctx.index_files),
        format!("deferred: {DEFERRED_ITEMS}"),
        "──".to_string(),
    ];
    lines.join("\n")
}

fn format_full_status(ctx: &StelStatusContext<'_>) -> String {
    let mut body = format_compact_status(ctx);
    let mut extra = vec![
        format!("project: {}", ctx.project_name),
        format!("index_symbols: {}", ctx.index_symbols),
        format!("session_tokens: {}", ctx.session_tokens),
    ];
    match (&ctx.last_ledger_decision, &ctx.last_ledger_route) {
        (Some(decision), route) => {
            extra.push(format!("last_ledger_decision: {decision}"));
            let route = route.as_deref().unwrap_or("none");
            extra.push(format!("last_ledger_route: {route}"));
        }
        (None, _) => {
            extra.push("last_ledger_decision: none".to_string());
            extra.push("last_ledger_route: none".to_string());
        }
    }
    extra.push(format_calibration_section(&ctx.calibration));
    // Insert full-only lines before the closing banner.
    if let Some(pos) = body.rfind("\n──\n") {
        let (head, tail) = body.split_at(pos);
        body = format!("{head}\n{}\n{tail}", extra.join("\n"));
    } else {
        body.push('\n');
        body.push_str(&extra.join("\n"));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> StelStatusContext<'static> {
        StelStatusContext {
            surface: "compact",
            version: "0.0.0-test",
            project_name: "symforge-test",
            index_ready: true,
            index_files: 12,
            index_symbols: 48,
            ledger_events: 2,
            session_tokens: 128,
            last_ledger_decision: Some("serve".to_string()),
            last_ledger_route: Some("search_text".to_string()),
            calibration: summarize_calibration(&[]),
        }
    }

    #[test]
    fn compact_status_reports_required_operational_fields() {
        let body = format_stel_status(&StelStatusRequest::default(), &sample_context());
        for needle in [
            "── stel status ──",
            "surface: compact",
            "phase0_go: 07b42a8",
            "phase0_evidence: 08f7d14",
            "l1_planner: active",
            "l2_economics: active",
            "l3_bypass: active",
            "l4_ledger: active",
            "handler_symforge: active",
            "handler_status: active",
            "handler_symforge_edit: preview-and-apply",
            "ledger_events: 2",
            "index_ready: true",
            "index_files: 12",
            "deferred: b_results,calibration_auto_tune,ledger_persistence,multi_step_planner",
            "──",
        ] {
            assert!(body.contains(needle), "missing `{needle}` in:\n{body}");
        }
        assert!(
            !body.contains("── calibration (observational) ──"),
            "compact detail must not include calibration section"
        );
    }

    #[test]
    fn full_status_adds_session_and_ledger_summary() {
        let body = format_stel_status(
            &StelStatusRequest {
                detail: Some(StelStatusDetail::Full),
            },
            &sample_context(),
        );
        for needle in [
            "project: symforge-test",
            "index_symbols: 48",
            "session_tokens: 128",
            "last_ledger_decision: serve",
            "last_ledger_route: search_text",
            "── calibration (observational) ──",
            "tuning:",
        ] {
            assert!(body.contains(needle), "missing `{needle}` in:\n{body}");
        }
    }

    #[test]
    fn from_server_reflects_empty_ledger() {
        let ledger = SessionLedger::new();
        let ctx = StelStatusContext::from_server("compact", "proj", false, 0, 0, &ledger, 0);
        assert_eq!(ctx.ledger_events, 0);
        assert_eq!(ctx.last_ledger_decision, None);
        let body = format_stel_status(&StelStatusRequest::default(), &ctx);
        assert!(body.contains("ledger_events: 0"));
        assert!(body.contains("index_ready: false"));
    }
}
