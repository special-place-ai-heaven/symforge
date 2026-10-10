//! Shared MCP and embedded knowledge-search input contract.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::index_lifecycle::guidance::serde_input::{lenient_u32, lenient_u64};
use crate::knowledge::review_contract::{
    AdvertisedSearchKnowledgeSourceScope, KnowledgeSourceScope,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeAuthorityScope {
    Default,
    Current,
    Intent,
    History,
    All,
}

/// Exact Gate I selectors and bounds; the host resolves project admission.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchKnowledgeInput {
    pub query: String,
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    #[schemars(with = "AdvertisedSearchKnowledgeSourceScope")]
    pub source_scope: Option<KnowledgeSourceScope>,
    #[serde(default)]
    pub authority_scope: Option<KnowledgeAuthorityScope>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    #[schemars(with = "Vec<String>")]
    pub projects: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}
