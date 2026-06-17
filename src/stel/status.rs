//! STEL compact `status` tool — operational surface report (no calibration/edit).

use super::calibration::{
    StelCalibrationSummary, format_calibration_section, summarize_calibration,
};
use super::ledger::SessionLedger;
use super::types::{StelStatusDetail, StelStatusRequest};

/// Phase 0 §12A independent GO anchor (authorization to implement `src/stel/`).
pub const PHASE0_GO_COMMIT: &str = "07b42a8";
/// Phase 0 evidence bundle anchor (A-019 measurement artifacts).
pub const PHASE0_EVIDENCE_COMMIT: &str = "08f7d14";

/// Stable comma-separated deferred-work list (sorted for test stability).
///
/// `ledger_persistence` was removed (010 FR-004): the durable SQLite ledger
/// store DOES ship in serve mode, so listing it as deferred was false. The
/// remaining items are genuinely not-yet-implemented seams.
pub const DEFERRED_ITEMS: &str = "b_results,calibration_auto_tune,multi_step_planner";

/// Restart-survival view of the durable STEL ledger store (US3/T029).
///
/// A feature-independent POD so [`StelStatusContext`] (compiled on stdio/embed
/// too) never has to name the server-only `ledger_store` types. The server-only
/// `status` read populates it from `StelLedgerStore::summary()`; `None` means
/// no durable store is wired (stdio/embed) or the store is `Disabled`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableLedgerSummary {
    pub total_events: u64,
    pub total_net_vs_manual: i64,
    pub session_count: u64,
}

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
    /// Durable ledger summary (restart-survival, US3/T029). `None` on
    /// stdio/embed or when the durable store is `Disabled`.
    pub durable_ledger: Option<DurableLedgerSummary>,
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
            durable_ledger: None,
        }
    }

    /// Attach a durable-ledger summary (US3/T029 restart-survival view).
    ///
    /// Builder-style so the in-memory `from_server` constructor stays unchanged
    /// for stdio/embed callers; the server-only `status` read calls this with
    /// the opened store's `summary()`.
    pub fn with_durable_ledger(mut self, summary: Option<DurableLedgerSummary>) -> Self {
        self.durable_ledger = summary;
        self
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
    // Honest static labels (010 US1 / TR-10). These describe the *static*
    // truth that holds at compile time for this build — NOT a runtime liveness
    // probe (live probing is Phase B / US2). `wired` = the layer/handler code
    // path is compiled in and reachable; `l4_ledger: in_memory` = the L4 layer
    // is the always-on in-memory cache (durable restart-survival state is
    // reported separately under `detail: full`). The blanket `active` literal
    // implied more than the surface can prove without probing.
    let lines = vec![
        "── stel status ──".to_string(),
        format!("surface: {}", ctx.surface),
        format!("symforge_version: {}", ctx.version),
        format!("phase0_go: {PHASE0_GO_COMMIT}"),
        format!("phase0_evidence: {PHASE0_EVIDENCE_COMMIT}"),
        "l1_planner: wired".to_string(),
        "l2_economics: wired".to_string(),
        "l3_bypass: wired".to_string(),
        "l4_ledger: in_memory".to_string(),
        "handler_symforge: wired".to_string(),
        "handler_status: wired".to_string(),
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
    match &ctx.durable_ledger {
        Some(summary) => {
            extra.push(format!(
                "durable_ledger: events={} net_vs_manual={} sessions={}",
                summary.total_events, summary.total_net_vs_manual, summary.session_count
            ));
        }
        None => {
            extra.push("durable_ledger: unavailable".to_string());
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
            durable_ledger: None,
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
            "l1_planner: wired",
            "l2_economics: wired",
            "l3_bypass: wired",
            "l4_ledger: in_memory",
            "handler_symforge: wired",
            "handler_status: wired",
            "handler_symforge_edit: preview-and-apply",
            "ledger_events: 2",
            "index_ready: true",
            "index_files: 12",
            "deferred: b_results,calibration_auto_tune,multi_step_planner",
            "──",
        ] {
            assert!(body.contains(needle), "missing `{needle}` in:\n{body}");
        }
        // Honest contract (010 TR-10): no blanket unconditional `active` literal
        // and the stale `ledger_persistence` deferred item is gone.
        assert!(
            !body.contains(": active"),
            "no subsystem may report a blanket `active` in:\n{body}"
        );
        assert!(
            !body.contains("ledger_persistence"),
            "ledger_persistence ships in serve mode; not deferred:\n{body}"
        );
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
    fn full_status_renders_durable_ledger_summary_when_present() {
        // US3/T029: when a durable store summary is attached, the full status
        // body surfaces the restart-survival line with concrete totals.
        let ctx = sample_context().with_durable_ledger(Some(DurableLedgerSummary {
            total_events: 7,
            total_net_vs_manual: 4200,
            session_count: 3,
        }));
        let body = format_stel_status(
            &StelStatusRequest {
                detail: Some(StelStatusDetail::Full),
            },
            &ctx,
        );
        assert!(
            body.contains("durable_ledger: events=7 net_vs_manual=4200 sessions=3"),
            "durable ledger summary line missing in:\n{body}"
        );
    }

    #[test]
    fn full_status_reports_durable_ledger_unavailable_when_absent() {
        // No durable store wired (stdio/embed) -> honest "unavailable".
        let body = format_stel_status(
            &StelStatusRequest {
                detail: Some(StelStatusDetail::Full),
            },
            &sample_context(),
        );
        assert!(
            body.contains("durable_ledger: unavailable"),
            "expected durable_ledger unavailable line in:\n{body}"
        );
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
