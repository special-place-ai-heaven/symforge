//! Bounded symbol retrieval and focused source inspection.
use super::{QueryRefusalKind, QuerySymbol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolReadRequest {
    pub project: Option<String>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub name: String,
    pub kind: Option<String>,
    pub symbol_line: Option<u32>,
    pub targets: Option<Vec<SymbolTarget>>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
    pub force_refresh: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolTarget {
    pub path: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub symbol_line: Option<u32>,
    pub start_byte: Option<u32>,
    /// Exclusive end offset. Omitted means end of file.
    pub end_byte: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolReadResult {
    pub entries: Vec<SymbolReadEntry>,
    pub rendered: String,
    pub cache_hit: bool,
    pub estimated_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolReadEntry {
    pub path: String,
    pub symbol: Option<QuerySymbol>,
    pub start_byte: Option<u32>,
    pub end_byte: Option<u32>,
    pub content_hash: Option<String>,
    pub source: Option<Vec<u8>>,
    pub refusal: Option<QueryRefusalKind>,
    pub withheld: Option<super::read::WithheldMeta>,
    pub candidates: Vec<QuerySymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectMatchRequest {
    pub path: String,
    pub line: u32,
    pub context: Option<u32>,
    pub sibling_limit: Option<u32>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectMatchResult {
    pub path: String,
    pub line: u32,
    pub excerpt: String,
    pub enclosing: Option<InspectionSymbol>,
    pub parent_chain: Vec<InspectionSymbol>,
    pub siblings: Vec<InspectionSymbol>,
    pub siblings_overflow: u64,
    pub rendered: String,
    pub estimated_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectionSymbol {
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub end_line: u32,
}
