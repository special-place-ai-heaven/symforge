//! Shared MCP and embedded knowledge-review requests.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Source-scope vocabulary accepted by `search_knowledge` and `review_knowledge`.
///
/// Both `search_knowledge` and `review_knowledge` compose across all four scopes
/// (Gate L): `current`, `worktrees`, `local_refs`, and `all`.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceScope {
    Current,
    Worktrees,
    LocalRefs,
    All,
}

/// Source scopes advertised by `search_knowledge` and `review_knowledge`
/// (Gate L): both compose across the captured source set, so every implemented
/// P1 scope is advertised.
#[allow(dead_code)]
#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AdvertisedSearchKnowledgeSourceScope {
    Current,
    Worktrees,
    LocalRefs,
    All,
}

/// Frozen read-only review modes for `review_knowledge`.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewKnowledgeMode {
    Summary,
    Document,
    Remediation,
}

/// Exact Gate J input for `review_knowledge`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewKnowledgeInput {
    /// Review projection: aggregate summary, exact document, or ranked remediation.
    pub mode: ReviewKnowledgeMode,
    /// Exact normalized repository-relative path. Required by `document` mode.
    #[serde(default)]
    pub path: Option<String>,
    /// Optional normalized repository-relative prefix for summary/remediation scope.
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// Captured repository source scope: `current` (default), `worktrees`,
    /// `local_refs`, or `all`. Composed from one captured source set.
    #[serde(default)]
    #[schemars(with = "AdvertisedSearchKnowledgeSourceScope")]
    pub source_scope: Option<KnowledgeSourceScope>,
    /// Optional open-project ID/alias; exclusive with `projects`. Omit both for
    /// the active project.
    #[serde(default)]
    pub project: Option<String>,
    /// Explicit open-project ids/aliases or `["*"]`; mutually exclusive with `project`.
    #[serde(default)]
    #[schemars(with = "Vec<String>")]
    pub projects: Option<Vec<String>>,
    /// Maximum number of complete dossiers. Defaults to ten and is server-bounded.
    #[serde(default)]
    #[cfg_attr(
        feature = "server",
        serde(deserialize_with = "crate::protocol::read_tools::lenient_u32")
    )]
    pub limit: Option<u32>,
    /// Response budget; compute complete-plan hashes before applying it.
    #[serde(default)]
    #[cfg_attr(
        feature = "server",
        serde(deserialize_with = "crate::protocol::read_tools::lenient_u64")
    )]
    pub max_tokens: Option<u64>,
}
