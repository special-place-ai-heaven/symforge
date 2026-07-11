use rmcp::model::{CallToolResult, ContentBlock, JsonObject, Meta};
use serde::{Deserialize, Serialize};
use std::future::Future;

pub const RESULT_STATUS_META_KEY: &str = "symforge/result_status";
pub const RESULT_STATUS_CONTRACT_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeClass {
    Found,
    NotFound,
    Ambiguous,
    InvalidRequest,
    EmptyResult,
    InternalFailure,
}

impl OutcomeClass {
    pub const ALL: [Self; 6] = [
        Self::Found,
        Self::NotFound,
        Self::Ambiguous,
        Self::InvalidRequest,
        Self::EmptyResult,
        Self::InternalFailure,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Found => "found",
            Self::NotFound => "not_found",
            Self::Ambiguous => "ambiguous",
            Self::InvalidRequest => "invalid_request",
            Self::EmptyResult => "empty_result",
            Self::InternalFailure => "internal_failure",
        }
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::InvalidRequest | Self::InternalFailure)
    }
}

/// `_meta` key carrying the selected-project trust evidence (Task 7).
pub const PROJECT_EVIDENCE_META_KEY: &str = "symforge/project_evidence";

/// HTTP response header carrying the daemon's selected-project evidence for a
/// proxied tool call (JSON-serialized [`ProjectEvidence`]). Out-of-band so the
/// human-readable body stays byte-identical for existing consumers.
pub const PROJECT_EVIDENCE_HEADER: &str = "x-symforge-project-evidence";

/// Machine-readable trust evidence identifying WHICH project (and which index
/// generation / load source) actually served a tool response (Task 7). Built by
/// the daemon from the resolved per-call runtime, or locally from the bound
/// index; attached to statused tool results under
/// [`PROJECT_EVIDENCE_META_KEY`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEvidence {
    pub project_id: String,
    pub project_name: String,
    pub canonical_root: Option<String>,
    pub generation: u64,
    pub index_state: String,
    pub load_source: String,
    pub index_files: usize,
    pub index_symbols: usize,
}

tokio::task_local! {
    /// Selected-project evidence for the tool call currently being rendered.
    /// Scoped once per `tools/call` dispatch by [`with_project_evidence_scope`],
    /// seeded with the LOCAL bound-project evidence, and overwritten by the
    /// daemon proxy layer when the daemon answered (the daemon's receipt names
    /// the project that actually served — which may be an explicitly routed
    /// sibling, not the local home). Same bound-to-the-future pattern as the
    /// D23 connection-surface task-local.
    static PROJECT_EVIDENCE: std::cell::RefCell<Option<ProjectEvidence>>;
}

/// Run one `tools/call` dispatch with the evidence slot bound. `seed` is the
/// local bound-project evidence (or `None` when nothing is bound yet).
pub async fn with_project_evidence_scope<F, T>(seed: Option<ProjectEvidence>, future: F) -> T
where
    F: Future<Output = T>,
{
    PROJECT_EVIDENCE
        .scope(std::cell::RefCell::new(seed), future)
        .await
}

/// Overwrite the in-scope evidence with the daemon's receipt for this call.
/// No-op outside a dispatch scope (direct unit-test calls, hook paths).
pub fn record_project_evidence(evidence: ProjectEvidence) {
    let _ = PROJECT_EVIDENCE.try_with(|cell| *cell.borrow_mut() = Some(evidence));
}

/// Evidence for the response currently being built, if a dispatch scope is
/// active and populated.
pub fn current_project_evidence() -> Option<ProjectEvidence> {
    PROJECT_EVIDENCE
        .try_with(|cell| cell.borrow().clone())
        .ok()
        .flatten()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultStatus {
    pub contract_version: u8,
    pub outcome_class: OutcomeClass,
}

impl ResultStatus {
    pub const fn new(outcome_class: OutcomeClass) -> Self {
        Self {
            contract_version: RESULT_STATUS_CONTRACT_VERSION,
            outcome_class,
        }
    }

    pub fn into_call_tool_result(self, human_text: impl Into<String>) -> CallToolResult {
        let mut meta = JsonObject::new();
        meta.insert(
            RESULT_STATUS_META_KEY.to_string(),
            serde_json::to_value(self).expect("ResultStatus must serialize to JSON"),
        );
        // Task 7: attach the selected-project trust evidence for this call —
        // the daemon receipt when a proxy answered, else the local bound
        // project. Absent outside a dispatch scope (direct unit calls).
        if let Some(evidence) = current_project_evidence()
            && let Ok(value) = serde_json::to_value(&evidence)
        {
            meta.insert(PROJECT_EVIDENCE_META_KEY.to_string(), value);
        }

        let content = vec![ContentBlock::text(human_text.into())];
        let result = if self.outcome_class.is_error() {
            CallToolResult::error(content)
        } else {
            CallToolResult::success(content)
        };
        result.with_meta(Some(Meta(meta)))
    }
}
