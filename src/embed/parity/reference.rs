//! Rich reference and dependent queries over admitted source publications.
use serde::{Deserialize, Serialize};

pub use crate::embed::lifecycle::guidance::reference_contract::FindReferencesInput as ReferenceSearchRequest;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependentSearchRequest {
    pub project: Option<String>,
    pub path: String,
    pub name: Option<String>,
    pub limit: Option<u32>,
    pub max_per_file: Option<u32>,
    pub format: Option<String>,
    pub compact: Option<bool>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceSearchResult {
    pub total_references: u64,
    pub total_files: u64,
    pub files: Vec<ReferenceFile>,
    pub target_candidate_count: u64,
    pub target_candidates: Vec<ReferenceTargetCandidate>,
    pub target_parse_coverage_complete: bool,
    pub implementations: Vec<Implementation>,
    pub rendered: String,
    pub estimated_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceFile {
    pub path: String,
    pub hits: Vec<Vec<ReferenceContextLine>>,
    pub caller_declarations: Vec<CallerDeclaration>,
    pub caller_declaration_count: u64,
    pub caller_declarations_omitted: u64,
    pub caller_header_unavailable_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceContextLine {
    pub line: u32,
    pub text: String,
    pub is_reference_line: bool,
    pub enclosing_annotation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallerDeclaration {
    pub definition_byte_range: (u32, u32),
    pub definition_line_range: (u32, u32),
    pub byte_range: (u32, u32),
    pub line: u32,
    pub signature_start_line: Option<u32>,
    pub is_callable: bool,
    pub node_kind: String,
    pub scope: String,
    pub name: Option<String>,
    pub name_byte_range: Option<(u32, u32)>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferenceTargetCandidate {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
    pub header: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Implementation {
    pub trait_name: String,
    pub implementor: String,
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependentSearchResult {
    pub files: Vec<DependentFile>,
    pub references: Option<ReferenceSearchResult>,
    pub rendered: String,
    pub estimated_tokens: Option<u64>,
    /// The explicit redirected selector for a symbol-shaped dependent request.
    pub redirected_request: Option<ReferenceSearchRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependentFile {
    pub path: String,
    pub lines: Vec<DependentLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependentLine {
    pub line: u32,
    pub text: String,
    pub kind: String,
    pub name: String,
}
