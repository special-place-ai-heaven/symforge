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
/// store DOES ship in serve mode, so listing it as deferred was false.
/// `calibration_auto_tune` was removed (013 US2, T038): the auto-tune now
/// DERIVES, held-out-VALIDATES, and APPLIES corrected token-estimate constants
/// (the `tuned` state is reachable with a before/after error artifact), so
/// listing it as deferred would be false. The remaining items are genuinely
/// not-yet-implemented seams.
pub const DEFERRED_ITEMS: &str = "b_results,multi_step_planner";

/// Restart-survival view of the durable STEL ledger store (US3/T029).
///
/// A feature-independent POD so [`StelStatusContext`] (compiled on stdio/embed
/// too) never has to name the server-only `ledger_store` types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableLedgerSummary {
    pub total_events: u64,
    pub total_net_vs_manual: i64,
    pub session_count: u64,
}

/// Reported state of the durable-ledger subsystem on the `status` surface
/// (data-model E4, N-3 / TR-17 / FR-008).
///
/// A feature-independent POD mirror of
/// [`crate::stel::ledger_store::LedgerSubsystemState`] plus the `Unavailable`
/// case (no store wired in this build/surface). Distinguishing `Disabled` from
/// `Unavailable` is the whole point: a wired-but-failing store must never read
/// identically to a never-configured one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurableLedgerState {
    /// Durable store open and a summary query succeeded (serve mode).
    Durable(DurableLedgerSummary),
    /// Store configured/attempted but not serving — carries the reason so the
    /// operator can tell "broken" from "off" (N-3).
    Disabled { reason: String },
    /// No durable store wired into this build/surface (stdio/embed).
    Unavailable,
}

/// Inputs collected from the live server when formatting a status response.
///
/// `PartialEq` (not `Eq`): the embedded calibration summary carries `f64`
/// held-out error figures in its verdict (feature 013 US2).
#[derive(Clone, Debug, PartialEq)]
pub struct StelStatusContext<'a> {
    pub surface: &'static str,
    pub version: &'static str,
    pub project_name: &'a str,
    /// Bound workspace root that ANSWERED this request (012 D6-a bound-root
    /// visibility). Forward-slash normalized for cross-platform stability.
    /// `None` when no workspace is bound (cold start / never retargeted) — a
    /// LOUD signal so a stale or wrong binding can never read as a working one.
    pub project_root: Option<String>,
    pub index_ready: bool,
    pub index_files: usize,
    pub index_symbols: usize,
    pub ledger_events: usize,
    pub session_tokens: u64,
    pub last_ledger_decision: Option<String>,
    pub last_ledger_route: Option<String>,
    pub calibration: StelCalibrationSummary,
    /// Durable-ledger subsystem state (restart-survival, US3/T029; N-3 FR-008).
    /// `Unavailable` on stdio/embed (no store wired); `Disabled { reason }` when
    /// a wired store failed to open or its query failed; `Durable` otherwise.
    pub durable_ledger: DurableLedgerState,
}

impl<'a> StelStatusContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn from_server(
        surface: &'static str,
        project_name: &'a str,
        project_root: Option<String>,
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
            project_root,
            index_ready,
            index_files,
            index_symbols,
            ledger_events: ledger.len(),
            session_tokens,
            last_ledger_decision,
            last_ledger_route,
            calibration,
            durable_ledger: DurableLedgerState::Unavailable,
        }
    }

    /// Attach the durable-ledger subsystem state (US3/T029 restart-survival view;
    /// N-3 FR-008).
    ///
    /// Builder-style so the in-memory `from_server` constructor stays unchanged
    /// for stdio/embed callers (which default to `Unavailable`); the server-only
    /// `status` read calls this with the opened store's `subsystem_state()`.
    pub fn with_durable_ledger(mut self, state: DurableLedgerState) -> Self {
        self.durable_ledger = state;
        self
    }

    /// Override the calibration verdict with the DURABLE one (feature 013, T033 /
    /// FR-009), so `status detail:full` reflects the persisted calibration state
    /// (cross-session samples + active tuning), not only this session's in-memory
    /// events. Also re-renders the `tuning_note` from the new verdict so the
    /// section's two calibration lines stay consistent.
    ///
    /// Honesty invariant (data-model): the durable verdict is fail-closed. `None`
    /// is reserved for "no durable store wired" — the caller keeps the in-memory
    /// verdict (`Deferred`/`Accumulating`, never a false `Tuned`). A wired-but-
    /// broken store whose sample read fails passes `Some(Deferred)` so status
    /// never keeps an in-memory `Tuned` without a readable durable artifact. Only
    /// a readable `Durable` store with a real artifact yields `Tuned` here.
    pub fn with_calibration_verdict(
        mut self,
        verdict: crate::stel::calibration::CalibrationVerdict,
    ) -> Self {
        self.calibration.tuning_note =
            crate::stel::calibration::render_calibration_verdict(&verdict);
        self.calibration.verdict = verdict;
        self
    }
}

/// Render the single `durable_ledger:` line for a [`DurableLedgerState`].
///
/// Single source of truth for the line format so the full-status formatter and
/// the daemon-proxy status overlay ([`crate::protocol`]) never drift. Returns
/// exactly one of:
/// - `durable_ledger: events={} net_vs_manual={} sessions={}` (Durable)
/// - `durable_ledger: disabled ({reason})` (wired-but-failing, N-3/FR-008)
/// - `durable_ledger: unavailable` (no store wired)
pub fn format_durable_ledger_line(state: &DurableLedgerState) -> String {
    match state {
        DurableLedgerState::Durable(summary) => format!(
            "durable_ledger: events={} net_vs_manual={} sessions={}",
            summary.total_events, summary.total_net_vs_manual, summary.session_count
        ),
        // N-3 / FR-008: a wired-but-failing store is reported distinctly from a
        // never-configured one, carrying the reason.
        DurableLedgerState::Disabled { reason } => {
            format!("durable_ledger: disabled ({reason})")
        }
        DurableLedgerState::Unavailable => "durable_ledger: unavailable".to_string(),
    }
}

/// Render the two `last_ledger_decision:` / `last_ledger_route:` lines for a
/// status context.
///
/// Single source of truth for the format so the full-status formatter and the
/// daemon-proxy status overlay ([`crate::protocol`]) never drift. Returns a
/// 2-element array `[decision_line, route_line]`:
/// - `last_ledger_decision: {decision}` / `last_ledger_route: {route}` when the
///   session ledger has a last event (route falls back to `none` when the event
///   recorded no tool),
/// - `last_ledger_decision: none` / `last_ledger_route: none` when the ledger is
///   empty.
pub fn format_last_ledger_lines(ctx: &StelStatusContext<'_>) -> [String; 2] {
    match (&ctx.last_ledger_decision, &ctx.last_ledger_route) {
        (Some(decision), route) => {
            let route = route.as_deref().unwrap_or("none");
            [
                format!("last_ledger_decision: {decision}"),
                format!("last_ledger_route: {route}"),
            ]
        }
        (None, _) => [
            "last_ledger_decision: none".to_string(),
            "last_ledger_route: none".to_string(),
        ],
    }
}

/// The set of `status` body lines/blocks DERIVED from proxy-owned state (the
/// session ledger + durable store), as the proxy itself would render them.
///
/// On the daemon-backed stdio default, `status` is proxied to the daemon WORKER
/// (which owns the populated INDEX but has an EMPTY ledger + no durable store),
/// while the PROXY owns `stel_ledger` + `stel_ledger_store`. So every line below
/// reads the worker's blind zero unless the proxy overlays its OWN rendering.
/// This struct is that rendering — built from a proxy-side [`StelStatusContext`]
/// via [`render_proxy_owned_lines`] so the overlay reuses the EXACT formatters
/// the worker uses (no divergent formatting), and consumed by
/// `crate::protocol`'s `overlay_proxy_status_lines`.
///
/// It deliberately does NOT carry the INDEX lines (`index_ready`/`index_files`/
/// `index_symbols`/`project`): the worker owns the warm index (TR-01), so those
/// stay the worker's and must never be overlaid.
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyOwnedStatusLines {
    /// `ledger_events: {n}` — the proxy session ledger length.
    pub ledger_events: String,
    /// `last_ledger_decision: {..}` — full-detail only.
    pub last_ledger_decision: String,
    /// `last_ledger_route: {..}` — full-detail only.
    pub last_ledger_route: String,
    /// `durable_ledger: {..}` — full-detail only.
    pub durable_ledger: String,
    /// The whole `── calibration (observational) ──` … `──` block, rendered from
    /// the proxy's calibration summary/verdict — full-detail only.
    pub calibration_section: String,
}

/// Render the proxy-owned `status` line-set from a proxy-side context.
///
/// The SINGLE place that maps a [`StelStatusContext`] onto the exact line/block
/// strings the proxy must overlay. Reuses the same `format_*` helpers the
/// worker-side formatter calls, so the overlaid lines are byte-identical to what
/// a worker WOULD render if it had the proxy's ledger/store. Honesty: an empty
/// proxy ledger yields `ledger_events: 0` / `last_ledger_*: none`, an
/// `Unavailable`/`Disabled` store yields the truthful durable line, and a
/// `Deferred`/`Accumulating` verdict yields that calibration section — the
/// overlay never invents state.
pub fn render_proxy_owned_lines(ctx: &StelStatusContext<'_>) -> ProxyOwnedStatusLines {
    let [last_ledger_decision, last_ledger_route] = format_last_ledger_lines(ctx);
    ProxyOwnedStatusLines {
        ledger_events: format!("ledger_events: {}", ctx.ledger_events),
        last_ledger_decision,
        last_ledger_route,
        durable_ledger: format_durable_ledger_line(&ctx.durable_ledger),
        calibration_section: format_calibration_section(&ctx.calibration),
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
        // 012 D6-a bound-root visibility: surface WHICH project answered so a
        // stale/wrong binding is loud, not silent. `(unbound)` when no workspace
        // is bound (cold start / never retargeted).
        format!(
            "project_root: {}",
            ctx.project_root.as_deref().unwrap_or("(unbound)")
        ),
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
    // Single source of truth for the two last-ledger lines (shared with the
    // daemon-proxy overlay via `format_last_ledger_lines`), so the worker-side
    // formatter and the proxy overlay can never drift in format.
    extra.extend(format_last_ledger_lines(ctx));
    extra.push(format_durable_ledger_line(&ctx.durable_ledger));
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
            project_root: Some("E:/project/symforge-test".to_string()),
            index_ready: true,
            index_files: 12,
            index_symbols: 48,
            ledger_events: 2,
            session_tokens: 128,
            last_ledger_decision: Some("serve".to_string()),
            last_ledger_route: Some("search_text".to_string()),
            calibration: summarize_calibration(&[]),
            durable_ledger: DurableLedgerState::Unavailable,
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
            // 012 D6-a: bound-root visibility line is present on the compact
            // surface so the answering project is never silent.
            "project_root: E:/project/symforge-test",
            "index_ready: true",
            "index_files: 12",
            "deferred: b_results,multi_step_planner",
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
        // 013 T038: the auto-tune ships (tuned state reachable); not deferred.
        assert!(
            !body.contains("calibration_auto_tune"),
            "calibration_auto_tune ships in 013 US2; not deferred:\n{body}"
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
                reset_calibration: None,
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
        let ctx = sample_context().with_durable_ledger(DurableLedgerState::Durable(
            DurableLedgerSummary {
                total_events: 7,
                total_net_vs_manual: 4200,
                session_count: 3,
            },
        ));
        let body = format_stel_status(
            &StelStatusRequest {
                detail: Some(StelStatusDetail::Full),
                reset_calibration: None,
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
                reset_calibration: None,
            },
            &sample_context(),
        );
        assert!(
            body.contains("durable_ledger: unavailable"),
            "expected durable_ledger unavailable line in:\n{body}"
        );
    }

    #[test]
    fn full_status_reports_disabled_distinct_from_unavailable() {
        // N-3 / FR-008 (surface side): a wired-but-failing store reads as
        // `disabled (reason)`, which is textually distinct from `unavailable`.
        let ctx = sample_context().with_durable_ledger(DurableLedgerState::Disabled {
            reason: "summary query failed: no such table".to_string(),
        });
        let body = format_stel_status(
            &StelStatusRequest {
                detail: Some(StelStatusDetail::Full),
                reset_calibration: None,
            },
            &ctx,
        );
        assert!(
            body.contains("durable_ledger: disabled (summary query failed: no such table)"),
            "expected durable_ledger disabled(reason) line in:\n{body}"
        );
        // The disabled line must NOT collapse to the unavailable wording.
        assert!(
            !body.contains("durable_ledger: unavailable"),
            "a disabled (broken) store must not read as unavailable (off):\n{body}"
        );
    }

    #[test]
    fn from_server_reflects_empty_ledger() {
        let ledger = SessionLedger::new();
        let ctx = StelStatusContext::from_server("compact", "proj", None, false, 0, 0, &ledger, 0);
        assert_eq!(ctx.ledger_events, 0);
        assert_eq!(ctx.last_ledger_decision, None);
        assert_eq!(ctx.project_root, None);
        let body = format_stel_status(&StelStatusRequest::default(), &ctx);
        assert!(body.contains("ledger_events: 0"));
        assert!(body.contains("index_ready: false"));
    }

    #[test]
    fn unbound_project_root_reads_loudly_not_silently() {
        // 012 D6-a: a cold-start / never-retargeted session has no bound root.
        // The status MUST say so explicitly, never omit the line (silence would
        // let a wrong/empty binding masquerade as healthy).
        let ledger = SessionLedger::new();
        let ctx = StelStatusContext::from_server("compact", "proj", None, false, 0, 0, &ledger, 0);
        let body = format_stel_status(&StelStatusRequest::default(), &ctx);
        assert!(
            body.contains("project_root: (unbound)"),
            "an unbound session must surface `project_root: (unbound)`:\n{body}"
        );
    }

    #[test]
    fn bound_project_root_is_surfaced() {
        // 012 D6-a: when a workspace is bound, the answering root is visible so a
        // consumer can confirm which project produced the result.
        let ledger = SessionLedger::new();
        let ctx = StelStatusContext::from_server(
            "compact",
            "proj",
            Some("/home/u/repo".to_string()),
            true,
            10,
            40,
            &ledger,
            0,
        );
        let body = format_stel_status(&StelStatusRequest::default(), &ctx);
        assert!(
            body.contains("project_root: /home/u/repo"),
            "bound root must be surfaced verbatim:\n{body}"
        );
    }
}
