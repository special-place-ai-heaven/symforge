//! STEL L4 session ledger — append-only in-memory decision/execution records.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use super::controller::EconomicsBreakdown;
use super::executor::is_pff_bypass_body;
use super::handler::estimate_tokens;
use super::ledger_store::StoredLedgerRecord;
use super::planner::confidence_label;
use super::types::{
    AdmissionDecision, CoreToolName, IntentBucket, RouteConfidence, StelDecision, StelLedgerEvent,
    StelPlan,
};

/// In-memory append-only ledger for one MCP server session (no persistence in this slice).
#[derive(Debug, Default)]
pub struct SessionLedger {
    events: Mutex<Vec<StelLedgerEvent>>,
}

impl SessionLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, event: StelLedgerEvent) {
        self.events.lock().expect("session ledger lock").push(event);
    }

    pub fn len(&self) -> usize {
        self.events.lock().expect("session ledger lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.lock().expect("session ledger lock").is_empty()
    }

    pub fn last(&self) -> Option<StelLedgerEvent> {
        self.events
            .lock()
            .expect("session ledger lock")
            .last()
            .cloned()
    }

    pub fn events(&self) -> Vec<StelLedgerEvent> {
        self.events.lock().expect("session ledger lock").clone()
    }
}

/// Inputs captured after L3 serve or enforced bypass.
#[derive(Clone, Debug)]
pub struct LedgerCaptureInput<'a> {
    pub plan: &'a StelPlan,
    pub decision: &'a StelDecision,
    pub economics: &'a EconomicsBreakdown,
    pub selected_tool: &'a str,
    pub tools_called: Option<&'a [String]>,
    pub legacy_executed: bool,
    pub output_body: &'a str,
    pub surface: &'static str,
}

/// Compact machine-readable metadata embedded in the trust envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
pub struct LedgerEnvelopeMeta {
    pub plan_id: String,
    pub route_tool: String,
    pub decision: String,
    pub bypass: bool,
    pub pff_bypass: bool,
    pub cache_hit: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degrade_flags: Vec<String>,
    pub legacy_executed: bool,
    pub schema_tokens: u32,
    pub invoke_tokens: u32,
    pub predicted_net: i32,
    pub output_bytes: u64,
    pub output_tokens: u32,
    pub route_confidence: String,
}

/// Build a schema-aligned [`StelLedgerEvent`] from a completed `symforge` invocation.
pub fn build_ledger_event(input: &LedgerCaptureInput<'_>) -> StelLedgerEvent {
    let output_tokens = estimate_tokens(input.output_body);
    let symforge_cost = output_tokens
        .saturating_add(input.economics.predicted_schema_tokens)
        .saturating_add(input.economics.predicted_invoke_tokens);
    let net_vs_manual = input.economics.predicted_manual_tokens as i32 - symforge_cost as i32;

    StelLedgerEvent {
        ts_ms: ledger_timestamp_ms(),
        plan_id: input.plan.plan_id.clone(),
        surface: input.surface.to_string(),
        intent: input.plan.intent,
        decision: input.decision.decision,
        tools_called: if input.legacy_executed {
            input
                .tools_called
                .map(|tools| tools.to_vec())
                .unwrap_or_else(|| vec![input.selected_tool.to_string()])
        } else {
            vec![]
        },
        // D15: record the RAW (pre-correction-factor) prediction, NOT the
        // corrected one. The calibration learns `median(actual / recorded)`; if the
        // recorded figure were already `raw * f0` (the corrected value), tune #2+
        // would learn a DELTA (`f_true / f0`) the live path then under-applies to
        // RAW, and the held-out baseline would double-apply `f0` → false re-accept
        // (D15, same class as D8). Recording RAW makes `derive` learn the ABSOLUTE
        // `f_true` so `apply_factor(raw, f_true)` is exact and
        // `held_out_mae(_, in_force_factor)` reconstructs the true live residual
        // under ANY active tuning. When no tuning is in force `raw == corrected`,
        // so the static path (and golden-replay) is byte-identical.
        predicted_response_tokens: input.economics.raw_predicted_response_tokens,
        actual_response_tokens: output_tokens,
        manual_baseline_tokens: input.economics.predicted_manual_tokens,
        net_vs_manual,
        equivalence: None,
        route_confidence: input.plan.confidence,
        pff_bypass: (input.decision.decision == AdmissionDecision::Bypass).then(|| {
            input
                .decision
                .bypass
                .as_ref()
                .is_some_and(is_pff_bypass_body)
        }),
        cache_hit: (input.decision.decision == AdmissionDecision::CacheHit).then_some(true),
        degrade_flags: if input.decision.decision == AdmissionDecision::Degrade {
            input.decision.degrade_flags.clone()
        } else {
            vec![]
        },
    }
}

/// Format compact ledger metadata for the trust envelope `ledger:` line.
pub fn format_ledger_envelope_line(event: &StelLedgerEvent, meta: &LedgerEnvelopeMeta) -> String {
    let json = serde_json::to_string(meta).expect("ledger meta serializes");
    let _ = event;
    format!("ledger: {json}")
}

/// Build envelope metadata and ledger event together.
pub fn capture_ledger(input: &LedgerCaptureInput<'_>) -> (StelLedgerEvent, LedgerEnvelopeMeta) {
    let output_tokens = estimate_tokens(input.output_body);
    let output_bytes = input.output_body.len() as u64;
    let event = build_ledger_event(input);
    let meta = LedgerEnvelopeMeta {
        plan_id: event.plan_id.clone(),
        route_tool: input.selected_tool.to_string(),
        decision: input.decision.decision.as_str().to_string(),
        bypass: input.decision.decision == AdmissionDecision::Bypass,
        pff_bypass: input.decision.decision == AdmissionDecision::Bypass
            && input
                .decision
                .bypass
                .as_ref()
                .is_some_and(is_pff_bypass_body),
        cache_hit: input.decision.decision == AdmissionDecision::CacheHit,
        degrade_flags: if input.decision.decision == AdmissionDecision::Degrade {
            input.decision.degrade_flags.clone()
        } else {
            vec![]
        },
        legacy_executed: input.legacy_executed,
        schema_tokens: input.economics.predicted_schema_tokens,
        invoke_tokens: input.economics.predicted_invoke_tokens,
        predicted_net: input.economics.predicted_net_vs_manual,
        output_bytes,
        output_tokens,
        route_confidence: confidence_label(input.plan.confidence).to_string(),
    };
    (event, meta)
}

fn ledger_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Run-length collapse (032 US2) — presentation-only; one algorithm, two lanes
// ---------------------------------------------------------------------------

/// One maximal run of consecutive rows that share a collapse identity.
///
/// Rendering-only (spec FR-009): a run is computed from the rows a view is
/// about to render and is never written back — the stored events, in-memory
/// and durable, stay individually intact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Run<T> {
    /// The positional first row of the run (the first in input order).
    pub(crate) canonical: T,
    /// Run length, always ≥ 1.
    pub(crate) count: u64,
    /// The smallest `ts_ms` over the run's rows.
    pub(crate) first_ts_ms: u64,
    /// The largest `ts_ms` over the run's rows.
    pub(crate) last_ts_ms: u64,
}

/// Collapse `items` (in chronological order) into maximal runs of STRICTLY
/// consecutive rows whose `identity` keys are equal: `A,A,B,A` → `A×2, B, A`.
/// Non-adjacent repeats never merge (spec FR-007 / US2 scenario 2).
///
/// Invariants, pinned by the property tests below: the counts sum to
/// `items.len()`; flattening the runs by identity reproduces the input
/// identity sequence; an all-distinct input yields one run per row.
/// `first_ts_ms`/`last_ts_ms` are the min/max `ts_ms` over the run's rows
/// (robust to insert-order skew, where a later row carries an earlier
/// clock); `canonical` stays the positional first row. Callers pass rows in
/// chronological (insert) order so that adjacency means "consecutive".
pub(crate) fn collapse_runs<'a, T: Clone + 'a, K: PartialEq>(
    items: &'a [T],
    identity: impl Fn(&'a T) -> K,
    ts_ms: impl Fn(&T) -> u64,
) -> Vec<Run<T>> {
    let mut runs: Vec<Run<T>> = Vec::new();
    let mut open_key: Option<K> = None;
    for item in items {
        let key = identity(item);
        let ts = ts_ms(item);
        let extends_open_run = match (&open_key, runs.last_mut()) {
            (Some(open), Some(run)) if *open == key => {
                run.count += 1;
                // min/max, not positional: under insert-order skew a later row
                // can carry an earlier clock, and the span must still cover it.
                run.first_ts_ms = run.first_ts_ms.min(ts);
                run.last_ts_ms = run.last_ts_ms.max(ts);
                true
            }
            _ => false,
        };
        if !extends_open_run {
            runs.push(Run {
                canonical: item.clone(),
                count: 1,
                first_ts_ms: ts,
                last_ts_ms: ts,
            });
            open_key = Some(key);
        }
    }
    runs
}

/// Lane A collapse identity — the in-memory [`StelLedgerEvent`] (status view).
///
/// Built by [`ledger_event_identity`]'s EXHAUSTIVE destructure: the two clocks
/// (`ts_ms`, and `plan_id`, which embeds wall-clock millis) and the four
/// per-call measurements are bound and ignored; every other field is identity
/// (data-model.md Lane A). A field added to the event fails compilation there
/// and forces an identity decision instead of silently joining or skipping.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EventIdentity<'a> {
    surface: &'a str,
    intent: IntentBucket,
    decision: AdmissionDecision,
    tools_called: &'a [CoreToolName],
    equivalence: Option<&'a Value>,
    route_confidence: RouteConfidence,
    pff_bypass: Option<bool>,
    cache_hit: Option<bool>,
    degrade_flags: &'a [String],
}

/// Lane A identity extractor — see [`EventIdentity`].
pub(crate) fn ledger_event_identity(event: &StelLedgerEvent) -> EventIdentity<'_> {
    let StelLedgerEvent {
        ts_ms: _,
        plan_id: _,
        surface,
        intent,
        decision,
        tools_called,
        predicted_response_tokens: _,
        actual_response_tokens: _,
        manual_baseline_tokens: _,
        net_vs_manual: _,
        equivalence,
        route_confidence,
        pff_bypass,
        cache_hit,
        degrade_flags,
    } = event;
    EventIdentity {
        surface,
        intent: *intent,
        decision: *decision,
        tools_called,
        equivalence: equivalence.as_ref(),
        route_confidence: *route_confidence,
        pff_bypass: *pff_bypass,
        cache_hit: *cache_hit,
        degrade_flags,
    }
}

/// Lane B collapse identity — the durable [`StoredLedgerRecord`] (admin view).
///
/// Built by [`stored_record_identity`]'s EXHAUSTIVE destructure: `id`,
/// `ts_ms`, `plan_id` and the four token measurements are bound and ignored;
/// `session_id` is identity (a run never spans sessions); the stored string
/// forms are compared verbatim; `pff_bypass`/`cache_hit`/`degrade_flags_json`
/// are the columns 032 widened the read-back with, so rows differing in them
/// never merge (data-model.md Lane B). `accepted`/`eligible_h6` are excluded
/// by construction — the row type does not read them back. `equivalence` has
/// no column at all: a documented durable-lane coarseness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredRecordIdentity<'a> {
    session_id: &'a str,
    surface: &'a str,
    intent: &'a str,
    decision: &'a str,
    tools_called_json: &'a str,
    route_confidence: &'a str,
    pff_bypass: Option<bool>,
    cache_hit: Option<bool>,
    degrade_flags_json: &'a str,
}

/// Lane B identity extractor — see [`StoredRecordIdentity`].
pub(crate) fn stored_record_identity(record: &StoredLedgerRecord) -> StoredRecordIdentity<'_> {
    let StoredLedgerRecord {
        id: _,
        ts_ms: _,
        session_id,
        plan_id: _,
        surface,
        intent,
        decision,
        tools_called_json,
        predicted_response_tokens: _,
        actual_response_tokens: _,
        manual_baseline_tokens: _,
        net_vs_manual: _,
        route_confidence,
        pff_bypass,
        cache_hit,
        degrade_flags_json,
    } = record;
    StoredRecordIdentity {
        session_id,
        surface,
        intent,
        decision,
        tools_called_json,
        route_confidence,
        pff_bypass: *pff_bypass,
        cache_hit: *cache_hit,
        degrade_flags_json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::controller::{estimate_economics, evaluate_plan};
    use crate::stel::planner::build_plan;
    use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep, StelRequest};

    fn serve_plan() -> StelPlan {
        StelPlan {
            plan_id: "plan-serve".to_string(),
            intent: IntentBucket::Trace,
            confidence: RouteConfidence::Exact,
            confidence_rationale: "test".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "find_references".to_string(),
                args: serde_json::json!({ "name": "cfg_if" }),
                est_response_tokens: 400,
                est_manual_tokens: 800,
                index_refs: vec![],
            }],
            suggested_followup: None,
        }
    }

    #[test]
    fn serve_ledger_records_tool_execution() {
        let plan = serve_plan();
        let request = StelRequest {
            query: "who references cfg_if".to_string(),
            ..Default::default()
        };
        let decision = evaluate_plan(&request, &plan);
        let economics = estimate_economics(&plan);
        let body = "Chosen tool: find_references\n\nrefs";
        let (event, meta) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: "find_references",
            tools_called: None,
            legacy_executed: true,
            output_body: body,
            surface: "symforge",
        });
        assert_eq!(event.decision, AdmissionDecision::Serve);
        assert_eq!(event.tools_called, vec!["find_references".to_string()]);
        assert!(meta.legacy_executed);
        assert!(!meta.bypass);
        assert!(!meta.pff_bypass);
        assert!(!meta.cache_hit);
        assert!(meta.degrade_flags.is_empty());
        assert_eq!(meta.route_tool, "find_references");
        assert!(meta.output_bytes > 0);
    }

    #[test]
    fn pff_bypass_ledger_skips_legacy_execution() {
        let request = StelRequest {
            query: "review entire lib.rs for security".to_string(),
            ..Default::default()
        };
        let plan = build_plan(&request);
        let decision = evaluate_plan(&request, &plan);
        let economics = super::super::controller::economics_for_bypass(
            decision.bypass.as_ref().expect("pff bypass"),
        );
        let body = "Decision: bypass\nSymForge did not execute a legacy tool";
        let (event, meta) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: plan.steps[0].tool.as_str(),
            tools_called: None,
            legacy_executed: false,
            output_body: body,
            surface: "symforge",
        });
        assert_eq!(event.decision, AdmissionDecision::Bypass);
        assert!(event.tools_called.is_empty());
        assert!(meta.bypass);
        assert!(meta.pff_bypass);
        assert!(!meta.cache_hit);
        assert!(!meta.legacy_executed);
    }

    #[test]
    fn economics_bypass_ledger_records_non_pff_bypass() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
        let plan = StelPlan {
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
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        let economics = estimate_economics(&plan);
        let (event, meta) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: "get_file_context",
            tools_called: None,
            legacy_executed: false,
            output_body: "Decision: bypass",
            surface: "symforge",
        });
        assert_eq!(event.decision, AdmissionDecision::Bypass);
        assert!(meta.bypass);
        assert!(!meta.pff_bypass);
        assert!(!meta.legacy_executed);
    }

    #[test]
    fn degrade_ledger_records_flags_without_legacy_tools_when_skipped() {
        use crate::stel::types::{IntentBucket, RouteConfidence, StelPlan, StelPlanStep};
        let plan = StelPlan {
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
        };
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        let economics = estimate_economics(&plan);
        let (event, meta) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: "get_file_context",
            tools_called: None,
            legacy_executed: true,
            output_body: "Economics: degrade",
            surface: "symforge",
        });
        assert_eq!(event.decision, AdmissionDecision::Degrade);
        assert!(!meta.bypass);
        assert!(meta.degrade_flags.contains(&"outline_only".to_string()));
        assert!(meta.legacy_executed);
    }

    #[test]
    fn session_ledger_appends_events() {
        let ledger = SessionLedger::new();
        let plan = serve_plan();
        let request = StelRequest::default();
        let decision = evaluate_plan(&request, &plan);
        let economics = estimate_economics(&plan);
        let (event, _) = capture_ledger(&LedgerCaptureInput {
            plan: &plan,
            decision: &decision,
            economics: &economics,
            selected_tool: "find_references",
            tools_called: None,
            legacy_executed: true,
            output_body: "body",
            surface: "symforge",
        });
        ledger.push(event);
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger.last().unwrap().plan_id, "plan-serve");
    }
}

#[cfg(test)]
mod collapse_tests {
    //! 032 US2 property oracles for [`collapse_runs`] — one algorithm, two lane
    //! identities (data-model.md Lane A / Lane B). Every positive oracle carries
    //! its negative control in the same function.
    use super::{Run, collapse_runs, ledger_event_identity, stored_record_identity};
    use crate::stel::ledger_store::StoredLedgerRecord;
    use crate::stel::types::{AdmissionDecision, IntentBucket, RouteConfidence, StelLedgerEvent};

    /// Lane A fixture: `label` selects the identity (via `surface`); `ts_ms`
    /// and `plan_id` are the clocks the identity must ignore.
    fn event(label: &str, ts_ms: u64) -> StelLedgerEvent {
        StelLedgerEvent {
            ts_ms,
            plan_id: format!("plan-{ts_ms}"),
            surface: label.to_string(),
            intent: IntentBucket::Trace,
            decision: AdmissionDecision::Serve,
            tools_called: vec!["find_references".to_string()],
            predicted_response_tokens: 400,
            actual_response_tokens: 380,
            manual_baseline_tokens: 800,
            net_vs_manual: 420,
            equivalence: None,
            route_confidence: RouteConfidence::Exact,
            pff_bypass: None,
            cache_hit: None,
            degrade_flags: vec![],
        }
    }

    /// Lane B fixture: `label` selects the identity (via `surface`); `id`,
    /// `ts_ms` and `plan_id` are the row bookkeeping the identity must ignore.
    fn record(label: &str, id: i64, ts_ms: u64) -> StoredLedgerRecord {
        StoredLedgerRecord {
            id,
            ts_ms,
            session_id: "sess".to_string(),
            plan_id: format!("plan-{id}"),
            surface: label.to_string(),
            intent: "trace".to_string(),
            decision: "serve".to_string(),
            tools_called_json: r#"["find_references"]"#.to_string(),
            predicted_response_tokens: 400,
            actual_response_tokens: 380,
            manual_baseline_tokens: 800,
            net_vs_manual: 420,
            route_confidence: "exact".to_string(),
            pff_bypass: None,
            cache_hit: None,
            degrade_flags_json: "[]".to_string(),
        }
    }

    fn event_runs(events: &[StelLedgerEvent]) -> Vec<Run<StelLedgerEvent>> {
        collapse_runs(events, ledger_event_identity, |event| event.ts_ms)
    }

    fn record_runs(records: &[StoredLedgerRecord]) -> Vec<Run<StoredLedgerRecord>> {
        collapse_runs(records, stored_record_identity, |record| record.ts_ms)
    }

    fn events_from(labels: &[&str]) -> Vec<StelLedgerEvent> {
        labels
            .iter()
            .enumerate()
            .map(|(i, label)| event(label, 1_000 + i as u64))
            .collect()
    }

    fn records_from(labels: &[&str]) -> Vec<StoredLedgerRecord> {
        labels
            .iter()
            .enumerate()
            .map(|(i, label)| record(label, i as i64 + 1, 1_000 + i as u64))
            .collect()
    }

    #[test]
    fn collapse_runs_merges_only_strictly_consecutive_runs() {
        // spec US2 scenario 2: A,A,B,A -> A×2, B, A (the non-adjacent A never
        // merges into the first run).
        let events = events_from(&["A", "A", "B", "A"]);
        let runs = event_runs(&events);
        assert_eq!(
            runs.iter().map(|run| run.count).collect::<Vec<_>>(),
            vec![2, 1, 1],
            "event lane counts:\n{runs:?}"
        );
        assert_eq!(
            runs.iter()
                .map(|run| run.canonical.surface.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B", "A"]
        );
        assert_eq!((runs[0].first_ts_ms, runs[0].last_ts_ms), (1_000, 1_001));
        assert_eq!((runs[1].first_ts_ms, runs[1].last_ts_ms), (1_002, 1_002));
        assert_eq!((runs[2].first_ts_ms, runs[2].last_ts_ms), (1_003, 1_003));
        // The canonical row is the POSITIONAL first of its run (input
        // order), which is what `collapse_runs` clones and what
        // `collapse_runs_span_is_min_max_over_skewed_timestamps` pins under
        // insert-order skew; here the seed is in clock order too, so this
        // row happens to also be the chronologically first.
        assert_eq!(runs[0].canonical.plan_id, "plan-1000");

        let records = records_from(&["A", "A", "B", "A"]);
        let runs = record_runs(&records);
        assert_eq!(
            runs.iter().map(|run| run.count).collect::<Vec<_>>(),
            vec![2, 1, 1],
            "durable lane counts:\n{runs:?}"
        );
        assert_eq!(
            runs.iter()
                .map(|run| run.canonical.surface.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B", "A"]
        );
        assert_eq!((runs[0].first_ts_ms, runs[0].last_ts_ms), (1_000, 1_001));
        assert_eq!(runs[0].canonical.id, 1);
    }

    #[test]
    fn collapse_runs_counts_sum_to_input_length() {
        let labels = ["A", "A", "A", "B", "C", "C", "A", "D"];
        let events = events_from(&labels);
        let total: u64 = event_runs(&events).iter().map(|run| run.count).sum();
        assert_eq!(total, events.len() as u64);

        let records = records_from(&labels);
        let total: u64 = record_runs(&records).iter().map(|run| run.count).sum();
        assert_eq!(total, records.len() as u64);

        // Empty input collapses to no runs (sum 0 == len 0) — the degenerate
        // control for the invariant.
        assert!(event_runs(&[]).is_empty());
        assert!(record_runs(&[]).is_empty());
    }

    #[test]
    fn collapse_runs_flattening_reproduces_identity_sequence() {
        let labels = ["A", "A", "B", "B", "B", "A", "C"];
        let events = events_from(&labels);
        let runs = event_runs(&events);
        let flattened: Vec<_> = runs
            .iter()
            .flat_map(|run| {
                std::iter::repeat_n(ledger_event_identity(&run.canonical), run.count as usize)
            })
            .collect();
        let original: Vec<_> = events.iter().map(ledger_event_identity).collect();
        assert_eq!(flattened, original, "event lane flattening");

        let records = records_from(&labels);
        let runs = record_runs(&records);
        let flattened: Vec<_> = runs
            .iter()
            .flat_map(|run| {
                std::iter::repeat_n(stored_record_identity(&run.canonical), run.count as usize)
            })
            .collect();
        let original: Vec<_> = records.iter().map(stored_record_identity).collect();
        assert_eq!(flattened, original, "durable lane flattening");
    }

    #[test]
    fn collapse_runs_all_distinct_input_keeps_every_count_one() {
        // Negative control for the merge: nothing adjacent is identical, so the
        // output is the input, one run per row, in order, spans of one row.
        let labels = ["A", "B", "C", "D", "E"];
        let events = events_from(&labels);
        let runs = event_runs(&events);
        assert_eq!(runs.len(), events.len());
        for (run, source) in runs.iter().zip(&events) {
            assert_eq!(run.count, 1);
            assert_eq!(&run.canonical, source);
            assert_eq!(run.first_ts_ms, source.ts_ms);
            assert_eq!(run.last_ts_ms, source.ts_ms);
        }

        let records = records_from(&labels);
        let runs = record_runs(&records);
        assert_eq!(runs.len(), records.len());
        for (run, source) in runs.iter().zip(&records) {
            assert_eq!(run.count, 1);
            assert_eq!(&run.canonical, source);
            assert_eq!(run.first_ts_ms, source.ts_ms);
            assert_eq!(run.last_ts_ms, source.ts_ms);
        }
    }

    #[test]
    fn collapse_runs_ten_thousand_identical_events_collapse_to_one_run() {
        // spec SC-003 (status lane): a seeded run of 10,000 renders ×10000 —
        // the count neither overflows nor truncates.
        let events: Vec<_> = (0..10_000u64).map(|i| event("A", 1_000 + i)).collect();
        let runs = event_runs(&events);
        assert_eq!(runs.len(), 1, "one run expected, got {}", runs.len());
        assert_eq!(runs[0].count, 10_000);
        assert_eq!(runs[0].first_ts_ms, 1_000);
        assert_eq!(runs[0].last_ts_ms, 10_999);
        // Control: one distinct event at the end splits off its own run.
        let mut with_tail = events;
        with_tail.push(event("B", 20_000));
        let runs = event_runs(&with_tail);
        assert_eq!(
            runs.iter().map(|run| run.count).collect::<Vec<_>>(),
            vec![10_000, 1]
        );
    }

    #[test]
    fn event_identity_ignores_exactly_the_six_measurement_fields() {
        // data-model.md Lane A: the six non-identity fields are the two clocks
        // (`ts_ms`, `plan_id`) and the four per-call measurements. Two events
        // differing in ALL six still collapse...
        let base = event("A", 1_000);
        let mut measured = base.clone();
        measured.ts_ms = 9_999;
        measured.plan_id = "plan-other".to_string();
        measured.predicted_response_tokens = 1;
        measured.actual_response_tokens = 2;
        measured.manual_baseline_tokens = 3;
        measured.net_vs_manual = 4;
        let runs = event_runs(&[base.clone(), measured]);
        assert_eq!(runs.len(), 1, "measurement-only difference must collapse");
        assert_eq!(runs[0].count, 2);

        // ...while a difference in ANY of the nine identity fields does not.
        let variants: Vec<(&str, StelLedgerEvent)> = vec![
            ("surface", {
                let mut e = base.clone();
                e.surface = "other".to_string();
                e
            }),
            ("intent", {
                let mut e = base.clone();
                e.intent = IntentBucket::Read;
                e
            }),
            ("decision", {
                let mut e = base.clone();
                e.decision = AdmissionDecision::Bypass;
                e
            }),
            ("tools_called", {
                let mut e = base.clone();
                e.tools_called = vec!["search_text".to_string()];
                e
            }),
            ("equivalence", {
                let mut e = base.clone();
                e.equivalence = Some(serde_json::json!({ "probe": true }));
                e
            }),
            ("route_confidence", {
                let mut e = base.clone();
                e.route_confidence = RouteConfidence::Inferred;
                e
            }),
            ("pff_bypass", {
                let mut e = base.clone();
                e.pff_bypass = Some(true);
                e
            }),
            ("cache_hit", {
                let mut e = base.clone();
                e.cache_hit = Some(true);
                e
            }),
            ("degrade_flags", {
                let mut e = base.clone();
                e.degrade_flags = vec!["outline_only".to_string()];
                e
            }),
        ];
        for (field, variant) in variants {
            let runs = event_runs(&[base.clone(), variant]);
            assert_eq!(
                runs.len(),
                2,
                "a difference in identity field `{field}` must NOT collapse"
            );
        }
    }

    #[test]
    fn stored_record_identity_scopes_by_session_and_reads_widened_columns() {
        // data-model.md Lane B: `id`, `ts_ms`, `plan_id` and the four token
        // columns are excluded (`accepted`/`eligible_h6` are excluded by
        // construction — the row type does not carry them). Two rows differing
        // in ALL seven excluded fields still collapse...
        let base = record("A", 1, 1_000);
        let mut bookkeeping = base.clone();
        bookkeeping.id = 2;
        bookkeeping.ts_ms = 9_999;
        bookkeeping.plan_id = "plan-other".to_string();
        bookkeeping.predicted_response_tokens = 1;
        bookkeeping.actual_response_tokens = 2;
        bookkeeping.manual_baseline_tokens = 3;
        bookkeeping.net_vs_manual = 4;
        let runs = record_runs(&[base.clone(), bookkeeping]);
        assert_eq!(runs.len(), 1, "bookkeeping-only difference must collapse");
        assert_eq!(runs[0].count, 2);

        // ...while a difference in ANY of the nine identity columns —
        // `session_id` (runs never span sessions), the five stored string
        // forms, and the three newly read-back columns — does not.
        let variants: Vec<(&str, StoredLedgerRecord)> = vec![
            ("session_id", {
                let mut r = base.clone();
                r.session_id = "other-session".to_string();
                r
            }),
            ("surface", {
                let mut r = base.clone();
                r.surface = "other".to_string();
                r
            }),
            ("intent", {
                let mut r = base.clone();
                r.intent = "read".to_string();
                r
            }),
            ("decision", {
                let mut r = base.clone();
                r.decision = "bypass".to_string();
                r
            }),
            ("tools_called_json", {
                let mut r = base.clone();
                r.tools_called_json = r#"["search_text"]"#.to_string();
                r
            }),
            ("route_confidence", {
                let mut r = base.clone();
                r.route_confidence = "inferred".to_string();
                r
            }),
            ("pff_bypass", {
                let mut r = base.clone();
                r.pff_bypass = Some(true);
                r
            }),
            ("cache_hit", {
                let mut r = base.clone();
                r.cache_hit = Some(true);
                r
            }),
            ("degrade_flags_json", {
                let mut r = base.clone();
                r.degrade_flags_json = r#"["outline_only"]"#.to_string();
                r
            }),
        ];
        for (column, variant) in variants {
            let runs = record_runs(&[base.clone(), variant]);
            assert_eq!(
                runs.len(),
                2,
                "a difference in identity column `{column}` must NOT collapse"
            );
        }
    }

    #[test]
    fn collapse_runs_span_is_min_max_over_skewed_timestamps() {
        // Review finding collapse-honesty-2: under insert-order skew (two
        // concurrent identical calls whose durable writes land out of ts_ms
        // order — the in-memory lane can skew the same way) the span must be
        // min/max over the run, never positional, or it understates or
        // inverts. `canonical` stays the positional first row.
        let skewed = [
            record("A", 1, 1_002),
            record("A", 2, 1_000),
            record("A", 3, 1_005),
        ];
        let runs = record_runs(&skewed);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].count, 3);
        assert_eq!(
            (runs[0].first_ts_ms, runs[0].last_ts_ms),
            (1_000, 1_005),
            "span is min/max over the run:\n{runs:?}"
        );
        assert_eq!(
            runs[0].canonical.id, 1,
            "canonical stays the positional first row"
        );

        // Two-row control: an inverted pair still reports the ordered span.
        let pair = [record("A", 1, 1_002), record("A", 2, 1_000)];
        let runs = record_runs(&pair);
        assert_eq!(runs.len(), 1);
        assert_eq!(
            (runs[0].first_ts_ms, runs[0].last_ts_ms),
            (1_000, 1_002),
            "inverted pair:\n{runs:?}"
        );
        assert_eq!(runs[0].canonical.id, 1);

        // The event lane shares the algorithm: same skew, same span.
        let events = [event("A", 1_002), event("A", 1_000), event("A", 1_005)];
        let runs = event_runs(&events);
        assert_eq!(runs.len(), 1);
        assert_eq!((runs[0].first_ts_ms, runs[0].last_ts_ms), (1_000, 1_005));
        assert_eq!(runs[0].canonical.plan_id, "plan-1002");
    }
}
