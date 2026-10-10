//! Explicit read selection and exact-byte continuation.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileContentRequest {
    pub path: String,
    pub mode: Option<ReadMode>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub chunk_index: Option<u32>,
    pub max_lines: Option<u32>,
    pub around_line: Option<u32>,
    pub around_match: Option<String>,
    pub match_occurrence: Option<u32>,
    pub around_symbol: Option<String>,
    pub symbol_line: Option<u32>,
    pub context_lines: Option<u32>,
    pub show_line_numbers: Option<bool>,
    pub header: Option<bool>,
    pub estimate: Option<bool>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
    pub max_tokens: Option<u64>,
    pub force_refresh: Option<bool>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadMode {
    Lines,
    Symbol,
    Match,
    Chunk,
}

/// A byte offset is an offset into the exact admitted source bytes, independent
/// of UTF-8 character or line boundaries. Resume only against the same serving
/// publication; the old source-capture counter is not a continuation token.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePageRequest {
    pub path: String,
    pub offset: u64,
    pub expected_publication: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourcePage {
    pub path: String,
    pub content_hash: String,
    pub byte_start: u64,
    pub total_bytes: u64,
    pub bytes: Vec<u8>,
    pub next_offset: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content_hash: String,
    pub rendered: String,
    pub approximate_tokens: u64,
    pub cache_hit: bool,
    pub authority: ReadAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReadAuthority {
    PublishedGeneration,
    DiskObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileReadEstimate {
    pub path: String,
    pub approximate_tokens: u64,
    pub approximate_lines: Option<u64>,
    pub authority: ReadAuthority,
}

pub use crate::embed::lifecycle::guidance::withheld::{
    RemediationActionAvailability, RemediationActionName, WithheldFinding, WithheldMeta,
};
