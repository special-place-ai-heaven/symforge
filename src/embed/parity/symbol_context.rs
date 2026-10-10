//! Full symbol-context modes with typed relationships and trace evidence.
use super::reference::{DependentFile, Implementation};
use super::symbol::InspectionSymbol;
use super::{QueryRefusalKind, QuerySymbol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolContextRequest {
    pub project: Option<String>,
    pub name: String,
    pub file: Option<String>,
    pub path: Option<String>,
    pub symbol_kind: Option<String>,
    pub symbol_line: Option<u32>,
    pub verbosity: Option<String>,
    pub bundle: Option<bool>,
    pub sections: Option<Vec<String>>,
    #[serde(default)]
    pub include_tests: bool,
    pub max_tokens: Option<u64>,
    pub estimate: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolContextMode {
    Default,
    Bundle,
    Trace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolContextResult {
    pub mode: SymbolContextMode,
    pub path: Option<String>,
    pub symbol: Option<QuerySymbol>,
    pub context: Option<SymbolContext>,
    pub trace: Option<SymbolTrace>,
    pub rendered: String,
    pub estimate: Option<SymbolContextEstimate>,
    pub refusal: Option<QueryRefusalKind>,
    pub candidates: Vec<QuerySymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolContext {
    pub path: String,
    pub body: String,
    pub kind: String,
    /// Zero-based inclusive source line range, matching the shared engine view.
    pub line_range: (u32, u32),
    pub byte_count: u64,
    pub callers: ContextReferences,
    pub callees: ContextReferences,
    pub type_usages: ContextReferences,
    pub unresolved_same_name_member_calls: Vec<ContextReference>,
    pub dependencies: Vec<ContextTypeDependency>,
    pub implementation_suggestions: Vec<ContextImplementationSuggestion>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextReferences {
    pub total_count: u64,
    pub overflow_count: u64,
    pub unique_count: u64,
    pub entries: Vec<ContextReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextReference {
    pub display_name: String,
    pub path: String,
    pub line: u32,
    pub enclosing: Option<String>,
    pub occurrence_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextTypeDependency {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line_range: (u32, u32),
    pub body: String,
    pub depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextImplementationSuggestion {
    pub display_name: String,
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolTrace {
    pub dependents: Vec<DependentFile>,
    pub siblings: Vec<InspectionSymbol>,
    pub implementations: Vec<Implementation>,
    pub git_activity: Option<ContextGitActivity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextGitActivity {
    pub churn_score: f32,
    pub churn_bar: String,
    pub churn_label: String,
    pub commit_count: u32,
    pub last_relative: String,
    pub last_hash: String,
    pub last_message: String,
    pub last_author: String,
    pub last_timestamp: String,
    pub owners: Vec<String>,
    pub co_changes: Vec<(String, f32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolContextEstimate {
    pub body_tokens: u64,
    pub caller_tokens: u64,
    pub bundle_tokens: u64,
    pub raw_file_tokens: u64,
}
