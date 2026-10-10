//! Embedded knowledge review over the shared MCP dossier engine.

use crate::embed::lifecycle::public_api::EmbedSourceRefusal;
pub use crate::knowledge::curation_contract::{
    CurateKnowledgeInput, KnowledgePolicyActionInput, KnowledgePolicyAuthorityDomainInput,
    KnowledgePolicyEntryInput, KnowledgePolicyEvidenceInput, KnowledgePolicyLifecycleInput,
    KnowledgePolicyMutationInput, KnowledgePolicyTargetInput,
};
pub use crate::knowledge::review_contract::{
    KnowledgeSourceScope, ReviewKnowledgeInput, ReviewKnowledgeMode,
};
use serde::{Deserialize, Serialize};

pub mod search;

#[derive(Debug)]
pub enum KnowledgeReviewError {
    Source(EmbedSourceRefusal),
    InvalidSelection,
    InvalidInput,
    Readiness,
    BoundExceeded,
}

impl From<EmbedSourceRefusal> for KnowledgeReviewError {
    fn from(value: EmbedSourceRefusal) -> Self {
        Self::Source(value)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct KnowledgeReviewResult {
    pub rendered: String,
    pub source_key: String,
    pub review_hash: String,
    pub result_hash: String,
    pub publication_identity: String,
    /// Process-scoped serving publication. Bind a later curation request to this
    /// value so equal-byte source after an OS restart cannot reuse an old review.
    pub serving_publication_identity: String,
    pub source_version: u64,
    pub publication_generation: u64,
    /// The complete result exceeded the requested byte budget and the bounded
    /// dossier summary was returned. Hashes still cover the complete result.
    pub summarized: bool,
}

#[derive(Clone)]
pub struct BoundCurateKnowledgeInput {
    pub input: CurateKnowledgeInput,
    pub if_serving_publication_identity: String,
}

impl BoundCurateKnowledgeInput {
    pub fn from_review(review: &KnowledgeReviewResult, input: CurateKnowledgeInput) -> Self {
        Self {
            input,
            if_serving_publication_identity: review.serving_publication_identity.clone(),
        }
    }
}

impl std::fmt::Debug for BoundCurateKnowledgeInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundCurateKnowledgeInput")
            .field("input", &self.input)
            .field(
                "has_serving_publication_identity",
                &!self.if_serving_publication_identity.is_empty(),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeCurationStatus {
    Preview,
    Applied,
}

#[derive(Clone, Debug, Serialize)]
pub struct KnowledgeCurationResult {
    pub status: KnowledgeCurationStatus,
    /// The shared engine's policy diff and digest receipt. The request and
    /// policy images pass the repository's content safety gate first.
    pub rendered: String,
    pub refresh_ticket_identity: Option<String>,
}

#[derive(Debug)]
pub enum KnowledgeCurationError {
    Source(EmbedSourceRefusal),
    InvalidInput,
    StalePublication,
    ReplayConflict,
    WriteAuthorityRefused,
    DurableStateUnavailable,
    EngineRejected,
    Uncertain,
}

impl From<EmbedSourceRefusal> for KnowledgeCurationError {
    fn from(value: EmbedSourceRefusal) -> Self {
        Self::Source(value)
    }
}

/// Serializable source facts only; a room host supplies its own write grant.
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WireKnowledgeRequest {
    Review {
        input: ReviewKnowledgeInput,
    },
    CuratePreview {
        input: CurateKnowledgeInput,
        if_serving_publication_identity: String,
    },
    CurateApply {
        input: CurateKnowledgeInput,
        if_serving_publication_identity: String,
    },
}

impl WireKnowledgeRequest {
    pub fn is_apply(&self) -> bool {
        match self {
            Self::CurateApply { .. } => true,
            Self::Review { .. } | Self::CuratePreview { .. } => false,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum WireKnowledgeReply {
    Review(KnowledgeReviewResult),
    Curation(KnowledgeCurationResult),
}

#[derive(Debug)]
pub enum WireKnowledgeError {
    Review(KnowledgeReviewError),
    Curation(KnowledgeCurationError),
    InvalidKind,
}
