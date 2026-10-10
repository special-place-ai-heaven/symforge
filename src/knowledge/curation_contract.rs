//! Shared MCP and embedded knowledge-curation request types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Lifecycle value accepted by the repository knowledge policy writer.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgePolicyLifecycleInput {
    Active,
    Proposed,
    Accepted,
    Implemented,
    Deferred,
    Rejected,
    Withdrawn,
    Deprecated,
    Superseded,
    Archived,
    Historical,
    Unknown,
}

/// Optional authority-domain assertion accepted by the policy writer.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgePolicyAuthorityDomainInput {
    CurrentImplementation,
    NormativeIntent,
    Decision,
    Operations,
    Governance,
    HistoricalRecord,
    Unknown,
}

/// Exact byte-identity target guarded by one curation action.
#[derive(Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePolicyTargetInput {
    pub path: String,
    pub content_hash: String,
    #[serde(default)]
    #[schemars(with = "[u32; 2]")]
    pub unit_byte_range: Option<[u32; 2]>,
    #[serde(default)]
    pub unit_hash: Option<String>,
}

impl std::fmt::Debug for KnowledgePolicyTargetInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KnowledgePolicyTargetInput { redacted }")
    }
}

/// Secret-safe evidence reference stored in the policy ledger.
#[derive(Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePolicyEvidenceInput {
    pub rule_id: String,
    #[serde(default)]
    pub knowledge: Option<KnowledgePolicyTargetInput>,
    #[serde(default)]
    pub code_path: Option<String>,
}

impl std::fmt::Debug for KnowledgePolicyEvidenceInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KnowledgePolicyEvidenceInput { redacted }")
    }
}

/// One complete policy entry for an upsert mutation.
#[derive(Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePolicyEntryInput {
    pub entry_id: String,
    pub target: KnowledgePolicyTargetInput,
    pub lifecycle: KnowledgePolicyLifecycleInput,
    #[serde(default)]
    pub authority_domain: Option<KnowledgePolicyAuthorityDomainInput>,
    #[serde(default)]
    pub superseded_by: Option<KnowledgePolicyTargetInput>,
    #[serde(default)]
    pub evidence: Vec<KnowledgePolicyEvidenceInput>,
    pub justification_code: String,
}

impl std::fmt::Debug for KnowledgePolicyEntryInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KnowledgePolicyEntryInput { redacted }")
    }
}

/// Ledger-only mutations. Move/delete/document-edit operations are intentionally
/// absent from the closed schema.
#[derive(Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgePolicyMutationInput {
    Upsert {
        entry: KnowledgePolicyEntryInput,
    },
    Remove {
        entry_id: String,
        expected_target: KnowledgePolicyTargetInput,
    },
}

impl std::fmt::Debug for KnowledgePolicyMutationInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KnowledgePolicyMutationInput { redacted }")
    }
}

/// An explicitly approved review action and its exact policy mutation.
#[derive(Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePolicyActionInput {
    pub action_id: String,
    pub mutation: KnowledgePolicyMutationInput,
}

impl std::fmt::Debug for KnowledgePolicyActionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KnowledgePolicyActionInput { redacted }")
    }
}

/// Preview-first Gate K input for one current-worktree source.
#[derive(Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CurateKnowledgeInput {
    pub actions: Vec<KnowledgePolicyActionInput>,
    pub if_source_review_hash: String,
    pub if_manifest_digest: String,
    pub if_policy_digest: String,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub apply: bool,
    #[schemars(required)]
    pub project: Option<String>,
}

impl std::fmt::Debug for CurateKnowledgeInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurateKnowledgeInput")
            .field("action_count", &self.actions.len())
            .field("apply", &self.apply)
            .field("has_idempotency_key", &self.idempotency_key.is_some())
            .finish()
    }
}
