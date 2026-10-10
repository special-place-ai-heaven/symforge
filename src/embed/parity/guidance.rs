//! Typed guidance produced by the shared repository engine.

use super::QueryTextMatch;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExploreRequest {
    pub query: String,
    pub depth: u32,
    pub limit: u32,
    pub include_noise: bool,
    pub include_vendor: bool,
    pub include_personal_tooling: bool,
    pub language: Option<String>,
    pub path_prefix: Option<String>,
    /// Return the MCP handler's depth-keyed token estimate instead of results.
    pub estimate: Option<bool>,
    /// Response token budget; the full result stays retrievable by handle.
    pub max_tokens: Option<u64>,
}

impl ExploreRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            depth: 1,
            limit: 10,
            include_noise: false,
            include_vendor: false,
            include_personal_tooling: false,
            language: None,
            path_prefix: None,
            estimate: None,
            max_tokens: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExploreSymbol {
    pub name: String,
    pub kind: String,
    pub path: String,
    /// Shared engine score normalized to one million for wire-stable arithmetic.
    pub score_millionths: u32,
    pub signature: Option<String>,
    pub dependent_files: Vec<String>,
    pub implementations: Vec<String>,
    pub type_dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RelatedFile {
    pub path: String,
    pub matches: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Exploration {
    pub label: String,
    pub depth: u32,
    pub symbols: Vec<ExploreSymbol>,
    pub text_matches: Vec<QueryTextMatch>,
    pub related_files: Vec<RelatedFile>,
    pub derived_seed_terms: Vec<String>,
    pub derived_symbols: Vec<String>,
    pub derived_seed_files: Vec<String>,
    pub enriched_imports: Vec<String>,
    pub hidden_noise_results: u64,
}

/// The MCP `edit_plan` text: matched symbols or file, reference count,
/// co-change partners and the suggested tool sequence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EditPlanGuidance {
    pub target: String,
    pub rendered: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Conventions {
    pub language: String,
    pub error_handling: String,
    pub naming: String,
    pub test_patterns: String,
    pub common_imports: Vec<String>,
    pub file_organization: String,
    pub complexity: String,
}
