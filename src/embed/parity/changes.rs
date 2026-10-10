//! Full timestamp, working-tree and committed change queries.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhatChangedRequest {
    pub project: Option<String>,
    pub since: Option<i64>,
    pub git_ref: Option<String>,
    pub uncommitted: Option<bool>,
    pub path_prefix: Option<String>,
    pub language: Option<String>,
    pub code_only: Option<bool>,
    pub include_symbol_diff: Option<bool>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffSymbolsRequest {
    pub project: Option<String>,
    pub base: Option<String>,
    pub target: Option<String>,
    pub path_prefix: Option<String>,
    pub language: Option<String>,
    pub code_only: Option<bool>,
    pub compact: Option<bool>,
    pub summary_only: Option<bool>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChangeMode {
    Timestamp { since: i64 },
    GitRef { reference: String },
    Uncommitted,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WhatChangedResult {
    pub mode: ChangeMode,
    pub paths: Vec<String>,
    pub total_paths: u64,
    pub filtered_paths: u64,
    pub symbol_diff: Option<DiffSymbolsResult>,
    pub rendered: String,
    pub estimated_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymbolFileDelta {
    pub path: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
    pub withheld: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffSymbolsResult {
    pub base: String,
    pub target: String,
    pub base_commit: Option<String>,
    pub target_commit: Option<String>,
    pub total_changed_files: u64,
    pub filtered_changed_files: u64,
    pub files: Vec<SymbolFileDelta>,
    pub added_count: u64,
    pub removed_count: u64,
    pub modified_count: u64,
    pub withheld_count: u64,
    pub rendered: String,
    pub estimated_tokens: Option<u64>,
}
