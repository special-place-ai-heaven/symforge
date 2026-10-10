//! Full native search requests. Source identity comes from the admitted handle.
use super::{QuerySymbol, QueryTextMatch};

mod files;
pub use files::*;

/// All search filters are applied by the shared engine before result caps.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolSearchRequest {
    pub query: Option<String>,
    pub kind: Option<String>,
    pub path_prefix: Option<String>,
    pub language: Option<String>,
    pub limit: Option<u32>,
    pub include_generated: Option<bool>,
    pub include_tests: Option<bool>,
    pub include_vendor: Option<bool>,
    pub include_personal_tooling: Option<bool>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSearchRequest {
    pub query: Option<String>,
    pub terms: Option<Vec<String>>,
    pub regex: Option<bool>,
    pub structural: Option<bool>,
    pub path_prefix: Option<String>,
    pub language: Option<String>,
    pub limit: Option<u32>,
    pub max_per_file: Option<u32>,
    pub include_generated: Option<bool>,
    pub include_tests: Option<bool>,
    pub include_vendor: Option<bool>,
    pub include_personal_tooling: Option<bool>,
    pub glob: Option<String>,
    pub exclude_glob: Option<String>,
    pub context: Option<u32>,
    pub case_sensitive: Option<bool>,
    pub whole_word: Option<bool>,
    pub group_by: Option<TextGrouping>,
    pub follow_refs: Option<bool>,
    pub follow_refs_limit: Option<u32>,
    pub ranked: Option<bool>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextGrouping {
    #[default]
    File,
    Symbol,
    #[serde(alias = "purpose")]
    Usage,
    Names,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchEstimate {
    pub approximate_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymbolSearchResult {
    pub symbols: Vec<SymbolSearchHit>,
    pub total_matches: u64,
    pub overflow_count: u64,
    pub file_count: u64,
    pub suppressed_by_noise: u64,
    pub text_fallback: Vec<TextFallback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolMatchTier {
    Exact,
    Prefix,
    Substring,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymbolSearchHit {
    pub symbol: QuerySymbol,
    pub tier: SymbolMatchTier,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextFallback {
    pub path: String,
    pub line: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextCaller {
    pub path: String,
    pub symbol: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ContextLine {
    Separator,
    Line {
        line: u64,
        text: String,
        is_match: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextFile {
    pub path: String,
    pub matches: Vec<QueryTextMatch>,
    pub context: Option<Vec<ContextLine>>,
    /// None means caller enrichment was not requested for this file.
    pub callers: Option<Vec<TextCaller>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextSymbolGroup {
    pub symbol: QuerySymbol,
    pub match_count: u64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TextRows {
    Files(Vec<TextFile>),
    Symbols {
        groups: Vec<TextSymbolGroup>,
        top_level: Vec<QueryTextMatch>,
    },
    Names(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextSearchResult {
    pub rows: TextRows,
    pub total_matches: u64,
    pub overflow_count: u64,
    pub suppressed_by_noise: u64,
    pub excluded_by_usage: u64,
    pub excluded_knowledge_files: u64,
    pub withheld_policy_files: u64,
    pub withheld_size_files: u64,
    pub regex: bool,
    pub structural: bool,
    pub ranked: bool,
    pub auto_detected_regex: bool,
    pub auto_corrected_regex: bool,
}
