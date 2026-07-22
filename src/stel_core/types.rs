//! STEL wire types from `docs/stel-schema.md` (S2 compile proof).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Closed set of legacy MCP tool names referenced by plan steps (32 today).
pub type CoreToolName = String;

/// L0 optional hint → L1 intent bucket.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntentBucket {
    Orient,
    Find,
    Read,
    Trace,
    Impact,
    Edit,
    Meta,
    Auto,
}

impl IntentBucket {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Orient => "orient",
            Self::Find => "find",
            Self::Read => "read",
            Self::Trace => "trace",
            Self::Impact => "impact",
            Self::Edit => "edit",
            Self::Meta => "meta",
            Self::Auto => "auto",
        }
    }
}

/// Route confidence for plan steps (maps to `protocol::smart_query::RouteConfidence`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RouteConfidence {
    Exact,
    Inferred,
    Fallback,
}

/// L2 economics gate outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionDecision {
    Serve,
    Degrade,
    Bypass,
    CacheHit,
    Reject,
}

impl AdmissionDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Serve => "serve",
            Self::Degrade => "degrade",
            Self::Bypass => "bypass",
            Self::CacheHit => "cache_hit",
            Self::Reject => "reject",
        }
    }
}

/// MCP input for the `symforge` compact-surface tool (L0 → L1).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelRequest {
    /// The natural-language phrase to route. `#[serde(default)]` so an OMITTED
    /// `query` deserializes to an empty string instead of failing rmcp
    /// `Parameters` deserialization with an opaque error; `symforge_facade_tool`
    /// then validates emptiness up front and returns a clean
    /// `OutcomeClass::InvalidRequest` "query is required" (research D6,
    /// contracts/engine-and-surface.md §3c).
    #[serde(default)]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<IntentBucket>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 64))]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<bool>,
    /// Feature 012 (Phase 3): target a SINGLE open project by id/alias for the
    /// underlying cross-project reads. Mutually exclusive with `projects`; must be
    /// a project id/alias, never a path. Omitting both keeps today's active-project
    /// behavior. Surface parity only (Principle VII): the compact facade carries
    /// the param but does not yet route it into its planned read steps — direct
    /// `search_symbols`/`search_text`/`find_references` are the Phase 3 vehicle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Feature 012 (Phase 3): target an EXPLICIT subset of open projects by
    /// id/alias, or `["*"]` for all. Mutually exclusive with `project`; an empty
    /// list is rejected. Daemon-only; surface parity only (see `project`).
    // `#[schemars(with = "Vec<String>")]` keeps this as a plain `type: "array"`
    // schema, NOT a `type: ["array", "null"]` union that strict MCP clients
    // reject (enforced by `tests/strict_client_schema_compat.rs`); serde keeps the
    // field optional. Kept as a plain comment (not a doc line) so it does not
    // bloat the budget-constrained compact `symforge` schema description (A-025/H1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>")]
    pub projects: Option<Vec<String>>,
}

/// MCP call input for `symforge` — production [`StelRequest`] plus optional Phase 0 harness fields.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SymforgeCallInput {
    #[serde(flatten)]
    pub request: StelRequest,
    #[serde(default, rename = "_probe_legacy_tool")]
    #[schemars(skip)]
    pub probe_legacy_tool: Option<String>,
    #[serde(default, rename = "_probe_legacy_args")]
    #[schemars(skip)]
    pub probe_legacy_args: Option<Value>,
}

impl SymforgeCallInput {
    pub fn is_probe_relay(&self) -> bool {
        self.probe_legacy_tool.is_some()
    }
}

/// MCP input for the `symforge_edit` compact-surface tool.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelEditRequest {
    pub path: String,
    /// Optional daemon-session project selector. Mirrors the structural edit
    /// tools so the compact facade cannot silently fall back to the home repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// New source. For replace: the FULL item (signature + body), flush-left
    /// (re-columned to the symbol's indent, not doubled). For insert: the new
    /// symbol's source. Omit/false `apply` previews instead of writing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<StelEditIntent>,
    /// insert/edit_within use symbol as anchor/scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<StelEditOp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_all: Option<bool>,
    /// When true, commit a validated single-symbol edit. Default / omitted = preview dry_run only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<bool>,
    /// When set on apply, must match the current indexed symbol body bytes exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_match: Option<String>,
    /// Replay guard for committed apply (forwarded to legacy `replace_symbol_body`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    // Caller's working directory (absolute): routes the write into the matching
    // git worktree instead of the indexed root. Hidden from the compact
    // symforge_edit schema (A-025 byte budget) but still deserialized — the
    // worktree-awareness hook injects it at call time, so routing works without
    // advertising an advanced field on the token-sensitive compact surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub working_directory: Option<String>,
}

/// Edit intent for `symforge_edit` (L0 routes structural mutations here).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StelEditIntent {
    Edit,
}

// Structural operation selector for `symforge_edit`.
//
// Routes the compact facade to one of the existing internal edit tools:
// `Replace` -> `replace_symbol_body` (default; preserves replace-only callers),
// `InsertBefore` / `InsertAfter` -> `insert_symbol`, `EditWithin` ->
// `edit_within_symbol`. The default lets an omitted `op` stay byte-identical to
// the original replace-only schema and behavior. NOTE: kept as a plain comment,
// not a doc comment, so schemars does not emit a `$defs` description that would
// inflate the A-025 schema budget (`symforge_edit` schema must stay <= 1500 B).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StelEditOp {
    #[default]
    Replace,
    InsertBefore,
    InsertAfter,
    EditWithin,
}

impl StelEditOp {
    /// The legacy internal tool this op routes to.
    pub const fn legacy_tool(self) -> &'static str {
        match self {
            Self::Replace => "replace_symbol_body",
            Self::InsertBefore | Self::InsertAfter => "insert_symbol",
            Self::EditWithin => "edit_within_symbol",
        }
    }
}

/// Detail level for the `status` compact-surface tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StelStatusDetail {
    Compact,
    Full,
    /// Session project inventory (Task 7): one row per open project with
    /// identity, root, counts, index state, and home marker. On the daemon
    /// route this lists every project open in the session; a local/embedded
    /// server lists its single bound project.
    Projects,
}

/// MCP input for the `status` compact-surface tool.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelStatusRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<StelStatusDetail>,
    /// Feature 013 FR-011 operator reset: when `true`, clear accumulated
    /// calibration (the active tuned constants AND the current-estimator sample
    /// rows) before rendering, so the calibration surface returns to `Deferred`.
    /// MCP-native — a mode/param on the existing `status` tool, never injected
    /// context. Does NOT rebuild the index; only the calibration tables are
    /// cleared. No-op on a build/surface with no durable store wired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_calibration: Option<bool>,
    /// Surface actually served on the connection this request arrived on, as a
    /// canonical label (`full`/`compact`/`meta`; see
    /// [`crate::protocol::surface_probe::surface_profile_label`]). The stdio
    /// ADAPTER sets this to its OWN `surface_profile_from_env()` before proxying
    /// `status` to the warm daemon, so the daemon reports the surface truly
    /// served here — not the daemon process's own env, which may differ (a
    /// full-env adapter serves legacy tools through a compact-env daemon). Scope
    /// of the anti-spoof guarantee: on the stdio-ADAPTER path the adapter
    /// overwrites this unconditionally, so a client value never survives; on a
    /// DIRECT daemon `/mcp` attach the daemon cannot distinguish proxy from
    /// direct, so the value is client-supplied and only label-validated (see
    /// [`crate::protocol::surface_probe::surface_label_from_str`]) —
    /// cosmetic-only: it can skew this readout's surface LABEL but no gate reads
    /// it. `None` = direct/
    /// daemon-less serving OR an older adapter predating this field; either way
    /// the daemon falls back to its own env (unchanged behavior). Internal proxy
    /// field: hidden from the advertised tool schema (`#[schemars(skip)]`, so it
    /// stays out of the compact H1 byte budget) and wired under
    /// `_connection_surface`, mirroring the `_probe_*` fields above.
    #[serde(
        default,
        rename = "_connection_surface",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(skip)]
    pub connection_surface: Option<String>,
}

/// Index file reference driving manual-baseline estimation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexRef {
    pub path: String,
    pub raw_chars: u64,
}

/// Single step in a draft execution plan (L1 → L2).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelPlanStep {
    pub order: u32,
    pub tool: CoreToolName,
    pub args: Value,
    pub est_response_tokens: u32,
    pub est_manual_tokens: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub index_refs: Vec<IndexRef>,
}

/// Draft execution plan before the economics gate (L1 → L2).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelPlan {
    pub plan_id: String,
    pub intent: IntentBucket,
    pub confidence: RouteConfidence,
    pub confidence_rationale: String,
    pub steps: Vec<StelPlanStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_followup: Option<String>,
}

/// L2 preview payload when `StelRequest.preview == true`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelEstimate {
    pub plan_id: String,
    pub decision: AdmissionDecision,
    pub predicted_response_tokens: u32,
    pub predicted_manual_tokens: u32,
    pub predicted_schema_tokens: u32,
    pub predicted_invoke_tokens: u32,
    pub predicted_net_vs_manual: i32,
    pub recommended: bool,
}

/// Host-read bypass body when economics favor skipping L3.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelBypassBody {
    pub action: String,
    pub path: String,
    pub start_line: u32,
    /// `None` means whole-file host read (P-FF whole-file bypass).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    pub predicted_manual_tokens: u32,
    pub predicted_symforge_tokens: u32,
    pub reason: String,
}

/// Session cache hit body.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelCacheBody {
    pub kind: String,
    pub path: String,
    pub name: String,
    pub prior_tokens: u32,
    pub session_age_secs: u64,
}

/// L2 gate output consumed by the executor (L2 → L3).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelDecision {
    pub plan_id: String,
    pub decision: AdmissionDecision,
    pub decision_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degrade_flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<StelPlanStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bypass: Option<StelBypassBody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<StelCacheBody>,
}

/// Per-step L3 execution record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelStepExecution {
    pub tool: CoreToolName,
    pub success: bool,
    pub response_bytes: u64,
    pub response_tokens: u32,
    pub duration_ms: u64,
}

/// Aggregate economics for one `symforge` invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelExecutionTotals {
    pub response_tokens: u32,
    pub manual_baseline_tokens: u32,
    pub net_vs_manual: i32,
    pub schema_tokens: u32,
    pub invoke_tokens: u32,
}

/// Executor output merged for the LLM (L3 → L4).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelExecution {
    pub plan_id: String,
    pub decision: AdmissionDecision,
    pub steps_executed: Vec<StelStepExecution>,
    pub body: String,
    pub totals: StelExecutionTotals,
}

/// Append-only ledger row (L4).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelLedgerEvent {
    pub ts_ms: u64,
    pub plan_id: String,
    pub surface: String,
    pub intent: IntentBucket,
    pub decision: AdmissionDecision,
    pub tools_called: Vec<CoreToolName>,
    pub predicted_response_tokens: u32,
    pub actual_response_tokens: u32,
    pub manual_baseline_tokens: u32,
    pub net_vs_manual: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equivalence: Option<Value>,
    pub route_confidence: RouteConfidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pff_bypass: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degrade_flags: Vec<String>,
}

/// Per `(tool, intent_bucket)` calibration feedback (L4 → L2).
///
/// DEFERRED SEAM (010 N-1, intentionally inert). `ema_predict_error` and
/// `fudge_multiplier` are neither updated from real ledger samples nor read
/// back into the L2 controller anywhere today — auto-tuning is permanently
/// deferred, not transient. The seam is preserved on purpose (ledger Do-Not #7)
/// for the future adaptive-calibration work; the *observational* calibration
/// summary that the `status` surface actually renders lives in
/// `crate::stel::calibration::StelCalibrationSummary` (read-only, honest).
/// Surfaces label this `deferred` (never `pending`, which would imply
/// in-progress work). Do not delete or revive without grounding it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationState {
    pub tool: CoreToolName,
    pub intent: IntentBucket,
    pub ema_predict_error: f64,
    pub sample_count: u64,
    pub fudge_multiplier: f64,
}

/// One row from `docs/fixtures/routes.golden.jsonl` (path replay corpus).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GoldenRouteRow {
    pub id: String,
    pub query: String,
    #[serde(default)]
    pub intent: Option<IntentBucket>,
    #[serde(default)]
    pub must_call: Vec<CoreToolName>,
    #[serde(default)]
    pub must_not_call: Vec<CoreToolName>,
    pub expected_decision: AdmissionDecision,
    // TR-13 (010 FR-015): `expected_equiv` was write-only dead data — golden
    // replay grades route SHAPE and L2 decision only, never equivalence, so the
    // field implied a measurement that never ran (A-028 demoted to OPEN). There
    // is no runtime equivalence oracle, so the honest fix is removal, not a
    // tautological self-assertion. Equivalence remains an OPEN, offline-only
    // signal (the A-029 bench fixtures), not something this corpus grades.
    #[serde(default)]
    pub chain: Option<String>,
    #[serde(default)]
    pub eligible_h6: Option<bool>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl GoldenRouteRow {
    pub fn to_request(&self) -> StelRequest {
        StelRequest {
            query: self.query.clone(),
            intent: self.intent.or(Some(IntentBucket::Auto)),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn symforge_call_input_detects_probe_relay() {
        let probe = SymforgeCallInput {
            request: StelRequest {
                query: "x".to_string(),
                ..Default::default()
            },
            probe_legacy_tool: Some("search_text".to_string()),
            probe_legacy_args: Some(serde_json::json!({})),
        };
        assert!(probe.is_probe_relay());

        let production = SymforgeCallInput {
            request: StelRequest {
                query: "who calls foo".to_string(),
                ..Default::default()
            },
            probe_legacy_tool: None,
            probe_legacy_args: None,
        };
        assert!(!production.is_probe_relay());
    }

    #[test]
    fn stel_request_roundtrip_matches_schema_shape() {
        let req = StelRequest {
            query: "who calls hard_link".to_string(),
            intent: Some(IntentBucket::Auto),
            path: None,
            symbol: None,
            max_tokens: Some(1000),
            preview: Some(false),
            project: None,
            projects: None,
        };
        let value = serde_json::to_value(&req).expect("serialize");
        assert_eq!(value["query"], "who calls hard_link");
        assert_eq!(value["intent"], "auto");
        assert_eq!(value["max_tokens"], 1000);

        let back: StelRequest = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, req);
    }

    #[test]
    fn stel_plan_roundtrip_from_schema_example() {
        let plan = StelPlan {
            plan_id: "00000000-0000-4000-8000-000000000001".to_string(),
            intent: IntentBucket::Trace,
            confidence: RouteConfidence::Exact,
            confidence_rationale: "matched explicit caller phrasing".to_string(),
            steps: vec![StelPlanStep {
                order: 1,
                tool: "find_references".to_string(),
                args: json!({ "name": "hard_link", "limit": 20, "compact": true }),
                est_response_tokens: 420,
                est_manual_tokens: 800,
                index_refs: vec![IndexRef {
                    path: "tokio/src/fs/hard_link.rs".to_string(),
                    raw_chars: 3200,
                }],
            }],
            suggested_followup: None,
        };
        let back: StelPlan =
            serde_json::from_value(serde_json::to_value(&plan).expect("ser")).expect("de");
        assert_eq!(back, plan);
    }

    #[test]
    fn golden_route_row_deserializes_fixture_shape() {
        let line = r#"{"id":"cfg-if/t4_refs","query":"who references cfg_if","must_call":["find_references"],"must_not_call":[],"expected_decision":"serve","chain":"single","eligible_h6":true,"notes":"T2 reference trace; reviewed"}"#;
        let row: GoldenRouteRow = serde_json::from_str(line).expect("parse golden row");
        assert_eq!(row.id, "cfg-if/t4_refs");
        assert_eq!(row.must_call, vec!["find_references".to_string()]);
        assert_eq!(row.expected_decision, AdmissionDecision::Serve);
        assert_eq!(row.to_request().query, "who references cfg_if");
    }
}
