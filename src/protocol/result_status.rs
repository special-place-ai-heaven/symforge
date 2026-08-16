use rmcp::model::{CallToolResult, ContentBlock, JsonObject, MetaObject};
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

/// Match a daemon receipt to a project selector without turning an ID/name
/// collision into authority for the wrong project. Omission accepts the
/// daemon session's active project; canonical IDs keep the daemon's ID-first
/// resolution rule.
pub fn project_evidence_matches_selector(
    evidence: &ProjectEvidence,
    selector: Option<&str>,
) -> bool {
    let Some(root) = evidence.canonical_root.as_deref() else {
        return false;
    };
    let computed = crate::daemon::project_key(std::path::Path::new(root));
    if computed != evidence.project_id {
        return false;
    }
    let Some(selector) = selector.filter(|value| !value.trim().is_empty()) else {
        return true;
    };
    if selector.starts_with("project-v1-") {
        evidence.project_id == selector
    } else {
        evidence.project_id == selector || evidence.project_name == selector
    }
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

/// Clear any previously seeded or recorded evidence in the current dispatch.
///
/// A routed call uses this before crossing into another project so a missing,
/// malformed, or failed daemon receipt cannot fall back to the adapter's home
/// evidence and mislabel the response.
pub fn clear_project_evidence() {
    let _ = PROJECT_EVIDENCE.try_with(|cell| *cell.borrow_mut() = None);
}

/// Evidence for the response currently being built, if a dispatch scope is
/// active and populated.
pub fn current_project_evidence() -> Option<ProjectEvidence> {
    PROJECT_EVIDENCE
        .try_with(|cell| cell.borrow().clone())
        .ok()
        .flatten()
}

/// FR-319 (spec 025): central evidence attachment at the trait-boundary seam.
///
/// Called on every `tools/call` and `resources/read` result AFTER the router /
/// resource renderer returns. Single-writer rule: if the statused path
/// (`into_call_tool_result`) already wrote [`PROJECT_EVIDENCE_META_KEY`], the
/// result stays byte-identical. Otherwise the in-scope evidence is attached.
/// When no trustworthy evidence is available, attach the explicit marker
/// `{"bound": false, "reason": "project_evidence_unavailable"}`. This is
/// distinct from the full `project_id: "unbound"` evidence emitted when the
/// local adapter is observed but has no repository root.
pub fn attach_project_evidence_meta(meta: &mut Option<MetaObject>) {
    let meta = meta.get_or_insert_with(|| MetaObject(JsonObject::new()));
    if meta.0.contains_key(PROJECT_EVIDENCE_META_KEY) {
        return;
    }
    let value = match current_project_evidence().map(|e| serde_json::to_value(&e)) {
        Some(Ok(value)) => value,
        // Missing/cleared (or unserializable — cannot happen for
        // ProjectEvidence): disclose loudly rather than omitting the key or
        // mislabeling the state as an observed local-unbound workspace.
        _ => serde_json::json!({
            "bound": false,
            "reason": "project_evidence_unavailable",
        }),
    };
    meta.0.insert(PROJECT_EVIDENCE_META_KEY.to_string(), value);
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
        result.with_meta(Some(MetaObject(meta)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_evidence_selector_matching_preserves_nonblank_whitespace() {
        let root = std::path::Path::new("/work/repo");
        let evidence = ProjectEvidence {
            project_id: crate::daemon::project_key(root),
            project_name: "repo".to_string(),
            canonical_root: Some(root.display().to_string()),
            generation: 1,
            index_state: "ready".to_string(),
            load_source: "memory".to_string(),
            index_files: 1,
            index_symbols: 1,
        };

        assert!(project_evidence_matches_selector(&evidence, Some("repo")));
        assert!(
            !project_evidence_matches_selector(&evidence, Some("repo ")),
            "a legal trailing space must not be normalized onto another project name"
        );
        assert!(project_evidence_matches_selector(&evidence, Some("   ")));
    }
}
