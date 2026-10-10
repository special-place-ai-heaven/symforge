//! Versioned knowledge search over an admitted embedded source set.

use serde::{Deserialize, Serialize};

use crate::domain::SourceResponseEnvelope;
use crate::knowledge::review_contract::KnowledgeSourceScope;

pub use crate::knowledge::search_contract::SearchKnowledgeInput as SearchKnowledgeRequest;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchKnowledgeCounts {
    pub current: u64,
    pub intent: u64,
    pub history_only: u64,
    pub suppressed: u64,
    pub review_required: u64,
    pub unknown: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchKnowledgeDerived {
    pub authority_rule_version: u32,
    pub policy_version: u32,
    pub secret_policy_version: u32,
    pub bridge_coverage: String,
    pub authority_coverage: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchKnowledgeSource {
    pub label: String,
    /// Absent when source identity is withheld by the shared knowledge engine.
    pub envelope: Option<SourceResponseEnvelope>,
    pub withheld_count: u64,
    pub filtered: SearchKnowledgeCounts,
    pub readiness: Option<String>,
    pub degraded: bool,
    pub derived: SearchKnowledgeDerived,
    pub publication_generation: u64,
    pub content_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchKnowledgeResult {
    /// Shared MCP dossier rendering, packed only at complete hit boundaries.
    pub rendered: String,
    pub source_scope: KnowledgeSourceScope,
    pub sources: Vec<SearchKnowledgeSource>,
    pub hit_count: u64,
    pub rendered_hit_count: u64,
    pub overflow: u64,
    pub withheld_count: u64,
    pub filtered: SearchKnowledgeCounts,
    pub absence: Option<String>,
    pub degraded: bool,
    pub truncated: bool,
}
