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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<StelEditIntent>,
    /// When true, commit a validated single-symbol edit. Default / omitted = preview dry_run only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<bool>,
    /// When set on apply, must match the current indexed symbol body bytes exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_match: Option<String>,
    /// Replay guard for committed apply (forwarded to legacy `replace_symbol_body`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// Edit intent for `symforge_edit` (L0 routes structural mutations here).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StelEditIntent {
    Edit,
}

/// Detail level for the `status` compact-surface tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StelStatusDetail {
    Compact,
    Full,
}

/// MCP input for the `status` compact-surface tool.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StelStatusRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<StelStatusDetail>,
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
    pub end_line: u32,
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
}

/// Per `(tool, intent_bucket)` calibration feedback (L4 → L2).
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
    #[serde(default)]
    pub expected_equiv: Option<bool>,
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
            path: None,
            symbol: None,
            max_tokens: None,
            preview: None,
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
        let back: StelPlan = serde_json::from_value(serde_json::to_value(&plan).expect("ser"))
            .expect("de");
        assert_eq!(back, plan);
    }

    #[test]
    fn golden_route_row_deserializes_fixture_shape() {
        let line = r#"{"id":"cfg-if/t4_refs","query":"who references cfg_if","must_call":["find_references"],"must_not_call":[],"expected_decision":"serve","expected_equiv":true,"chain":"single","eligible_h6":true,"notes":"T2 reference trace; reviewed"}"#;
        let row: GoldenRouteRow = serde_json::from_str(line).expect("parse golden row");
        assert_eq!(row.id, "cfg-if/t4_refs");
        assert_eq!(row.must_call, vec!["find_references".to_string()]);
        assert_eq!(row.expected_decision, AdmissionDecision::Serve);
        assert_eq!(row.to_request().query, "who references cfg_if");
    }
}
