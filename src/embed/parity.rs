//! Versioned, bounded embedded queries over the same engine used by MCP.
//!
//! Lines are one-based and inclusive; byte spans are zero-based and exclusive
//! at the end. Every query captures one admitted publication. Returned source
//! is byte-exact. Graph pages must carry the first page's publication identity
//! when resuming; a refresh refuses that continuation instead of mixing rows.
//! The original V11 facade remains source-compatible.

pub mod ask;
pub mod changes;
pub mod detect_impact;
pub mod edit;
pub mod federation;
pub mod guidance;
pub mod host;
pub mod knowledge;
pub mod read;
pub mod read_context;
pub mod reference;
pub mod remediation;
pub mod replay;
pub mod search;
pub mod session;
pub mod source_options;
pub mod symbol;
pub mod symbol_context;

/// Contract version of the additive query namespace.
pub const API_VERSION: u32 = 1;

pub use crate::embed::lifecycle::embed_query::{QueryClaim, QueryOperationReceipt, QueryRefusal};

/// Trusted host policy. This grant is supplied in process and is never decoded
/// from a query request. Preparation additionally requires admitted state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct QueryPolicy {
    pub allow_derived_state_preparation: bool,
}

/// Aggregate row and decoded-payload bounds. `max_bytes` counts the UTF-8 bytes
/// of returned string values plus raw source buffers. It excludes field names,
/// JSON escaping/array encoding, fixed-size scalar fields, and claim provenance.
/// This is not a serialized frame limit; host adapters separately enforce their
/// exact `max_response_bytes` bound after serialization.
/// Requests above 10,000 rows or 1 MiB, or with a zero bound, are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryLimits {
    pub max_results: u32,
    pub max_bytes: u32,
}

/// Actual returned decoded payload and semantic rows, independent of the
/// conservative reservations used while building the result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueryUsage {
    pub rows: u64,
    pub decoded_bytes: u64,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_results: 100,
            max_bytes: 65_536,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuerySymbolSelector {
    pub path: String,
    pub name: String,
    pub kind: Option<String>,
    /// One-based definition line, used to disambiguate repeated names.
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum QueryOperationKind {
    File,
    FileContext,
    RepoMap,
    Symbol,
    InspectMatch,
    Context,
    References,
    Dependents,
    SearchSymbols,
    SearchText,
    SearchFiles,
    SearchKnowledge,
    Graph,
    Syntax,
    Diff,
    Impact,
    Explore,
    Conventions,
    ContextInventory,
    InvestigationSuggest,
    Retrieve,
    WhatChanged,
    DiffSymbols,
    DetectImpact,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub enum QueryRequest {
    TextSearch(search::TextSearchRequest),
    SymbolSearch(search::SymbolSearchRequest),
    FileSearch(search::FileSearchRequest),
    SearchKnowledge(knowledge::search::SearchKnowledgeRequest),
    FileContent(read::FileContentRequest),
    RepoMap(read_context::RepoMapRequest),
    FileContext(read_context::FileContextRequest),
    SourcePage(read::SourcePageRequest),
    SymbolRead(symbol::SymbolReadRequest),
    SymbolContext(symbol_context::SymbolContextRequest),
    InspectMatch(symbol::InspectMatchRequest),
    ReferenceSearch(reference::ReferenceSearchRequest),
    DependentSearch(reference::DependentSearchRequest),
    File {
        path: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
    },
    Symbol {
        selector: QuerySymbolSelector,
    },
    Context {
        selector: QuerySymbolSelector,
    },
    References {
        name: String,
        target: Option<QuerySymbolSelector>,
        kind: Option<String>,
        include_tests: bool,
    },
    Dependents {
        path: String,
    },
    SearchSymbols {
        query: Option<String>,
        path_prefix: Option<String>,
        kind: Option<String>,
        include_tests: bool,
    },
    SearchText {
        query: String,
        path_prefix: Option<String>,
        regex: bool,
        case_sensitive: bool,
        include_tests: bool,
    },
    SearchFiles {
        query: String,
        path_prefix: Option<String>,
        include_tests: bool,
    },
    Graph {
        path_prefix: Option<String>,
        offset: u64,
        expected_publication: Option<String>,
    },
    /// Reports parser diagnostics for the captured indexed bytes.
    Syntax {
        path: String,
    },
    /// Compare admitted local Git blobs, or `from_ref` to the captured current
    /// worktree publication when `to_ref` is `None`.
    Diff {
        path: String,
        from_ref: String,
        to_ref: Option<String>,
    },
    /// Symbol changes against a local Git ref plus current dependents.
    Impact {
        path: String,
        base_ref: String,
    },
    Explore(guidance::ExploreRequest),
    Conventions,
    ContextInventory,
    InvestigationSuggest {
        focus: Option<String>,
    },
    /// Read exact bytes from this session's served-output cache. The returned
    /// record retains its original publication and truncation metadata.
    Retrieve {
        handle: String,
        offset: u64,
    },
    /// Timestamp, working-tree or git-ref change report (MCP `what_changed`).
    WhatChanged(changes::WhatChangedRequest),
    /// Repository-wide symbol delta between two refs (MCP `diff_symbols`).
    DiffSymbols(changes::DiffSymbolsRequest),
    /// Git blast radius over the call graph (MCP `detect_impact`).
    DetectImpact(detect_impact::DetectImpactRequest),
    /// Natural-language routing plus native execution (MCP `ask`).
    Ask(ask::AskRequest),
}

impl QueryRequest {
    pub fn operation(&self) -> QueryOperationKind {
        match self {
            Self::TextSearch(_) => QueryOperationKind::SearchText,
            Self::SymbolSearch(_) => QueryOperationKind::SearchSymbols,
            Self::FileSearch(_) => QueryOperationKind::SearchFiles,
            Self::SearchKnowledge(_) => QueryOperationKind::SearchKnowledge,
            Self::FileContent(_) | Self::SourcePage(_) => QueryOperationKind::File,
            Self::RepoMap(_) => QueryOperationKind::RepoMap,
            Self::FileContext(_) => QueryOperationKind::FileContext,
            Self::SymbolRead(_) => QueryOperationKind::Symbol,
            Self::SymbolContext(_) => QueryOperationKind::Context,
            Self::InspectMatch(_) => QueryOperationKind::InspectMatch,
            Self::ReferenceSearch(_) => QueryOperationKind::References,
            Self::DependentSearch(_) => QueryOperationKind::Dependents,
            Self::File { .. } => QueryOperationKind::File,
            Self::Symbol { .. } => QueryOperationKind::Symbol,
            Self::Context { .. } => QueryOperationKind::Context,
            Self::References { .. } => QueryOperationKind::References,
            Self::Dependents { .. } => QueryOperationKind::Dependents,
            Self::SearchSymbols { .. } => QueryOperationKind::SearchSymbols,
            Self::SearchText { .. } => QueryOperationKind::SearchText,
            Self::SearchFiles { .. } => QueryOperationKind::SearchFiles,
            Self::Graph { .. } => QueryOperationKind::Graph,
            Self::Syntax { .. } => QueryOperationKind::Syntax,
            Self::Diff { .. } => QueryOperationKind::Diff,
            Self::Impact { .. } => QueryOperationKind::Impact,
            Self::Explore(_) => QueryOperationKind::Explore,
            Self::Conventions => QueryOperationKind::Conventions,
            Self::ContextInventory => QueryOperationKind::ContextInventory,
            Self::InvestigationSuggest { .. } => QueryOperationKind::InvestigationSuggest,
            Self::Retrieve { .. } => QueryOperationKind::Retrieve,
            Self::WhatChanged(_) => QueryOperationKind::WhatChanged,
            Self::DiffSymbols(_) => QueryOperationKind::DiffSymbols,
            Self::DetectImpact(_) => QueryOperationKind::DetectImpact,
            Self::Ask(_) => QueryOperationKind::Ask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum QueryRefusalKind {
    InvalidRequest,
    UnsupportedOption,
    SourceUnavailable,
    AdmissionUnavailable,
    NotFound,
    AmbiguousSymbol,
    StalePublication,
    InvalidSpan,
    BudgetTooSmall,
    GitUnavailable,
    Cancelled,
    DeadlineExceeded,
    SessionRequired,
    ForeignSession,
    StaleHandle,
    PolicyMismatch,
    /// A trusted output adapter rejected the complete projected envelope before
    /// read history, cache, frecency, or session revision were committed.
    OutputAdmissionRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryFile {
    pub path: String,
    pub language: String,
    /// Absent when only an admitted metadata path is available.
    pub content_hash: Option<String>,
    pub parse_status: String,
    pub is_test: bool,
    pub is_generated: bool,
    pub is_vendor: bool,
    pub symbol_count: u32,
    pub reference_count: u32,
    /// Absent when the publication does not contain a verified size.
    pub byte_len: Option<u64>,
    pub source: Option<Vec<u8>>,
    pub source_byte_start: u64,
    pub source_byte_end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QuerySymbol {
    /// Deterministic identity of path, kind, name and byte span. Stable across
    /// reopen for unchanged source; callers also retain the file content hash.
    pub identity: String,
    pub path: String,
    /// Original index in the file's symbol table, also used by references.
    pub symbol_index: u32,
    pub name: String,
    pub kind: String,
    pub depth: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub byte_start: u64,
    pub byte_end: u64,
    pub source: Option<Vec<u8>>,
    pub source_byte_start: u64,
    pub source_byte_end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryReference {
    pub path: String,
    pub reference_index: u32,
    pub name: String,
    pub qualified_name: Option<String>,
    pub kind: String,
    pub start_line: u32,
    pub end_line: u32,
    pub byte_start: u64,
    pub byte_end: u64,
    pub enclosing_symbol_index: Option<u32>,
    pub enclosing_symbol_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryTextMatch {
    pub path: String,
    pub line: u32,
    /// Exact span of the matched source line, excluding CR/LF terminators.
    pub byte_start: u64,
    pub byte_end: u64,
    pub preview: String,
    pub enclosing_symbol: Option<QuerySymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryGraph {
    pub files: Vec<QueryFile>,
    pub symbols: Vec<QuerySymbol>,
    /// Raw directed source occurrences, including unresolved targets. No
    /// name-only resolution silently discards ambiguity or import evidence.
    pub references: Vec<QueryReference>,
    pub next_offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryContext {
    pub symbol: QuerySymbol,
    pub callers: Vec<QueryReference>,
    pub callees: Vec<QueryReference>,
    pub type_usages: Vec<QueryReference>,
    pub dependencies: Vec<QuerySymbol>,
    pub unresolved_same_name_member_calls: Vec<QueryReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QuerySyntax {
    pub path: String,
    pub content_hash: String,
    pub valid: bool,
    pub parse_status: String,
    pub diagnostic: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QuerySymbolChange {
    pub name: String,
    pub kind: String,
    /// Occurrence within repeated (kind, name) definitions, in source order.
    pub occurrence: u32,
    pub change: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub before_line: Option<u32>,
    pub after_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QuerySymbolDiff {
    pub path: String,
    pub from_commit: String,
    pub to_commit: Option<String>,
    pub changes: Vec<QuerySymbolChange>,
    pub partial: bool,
}

/// Additional independently observed authority (e.g. an admitted Git blob).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct QueryObservation {
    pub kind: String,
    pub identity: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
// One reply per query, moved once to the caller; boxing every result variant would only add allocations.
#[allow(clippy::large_enum_variant)]
pub enum QueryOutput {
    TextSearch(search::TextSearchResult),
    SymbolSearch(search::SymbolSearchResult),
    FileSearch(search::FileSearchResult),
    SearchKnowledge(knowledge::search::SearchKnowledgeResult),
    FileContent(read::FileContent),
    RepoMap(read_context::RepoMapResult),
    FileContext(read_context::FileContextResult),
    SymbolRead(symbol::SymbolReadResult),
    SymbolContext(symbol_context::SymbolContextResult),
    InspectMatch(symbol::InspectMatchResult),
    ReferenceSearch(reference::ReferenceSearchResult),
    DependentSearch(reference::DependentSearchResult),
    SourcePage(read::SourcePage),
    FileReadEstimate(read::FileReadEstimate),
    SearchEstimate(search::SearchEstimate),
    ContextInventory(session::ContextInventory),
    InvestigationSuggestion(session::InvestigationSuggestion),
    RetrievedOutput(session::RetrievedOutput),
    Exploration(guidance::Exploration),
    Conventions(guidance::Conventions),
    File {
        file: QueryFile,
        symbols: Vec<QuerySymbol>,
        imports: Vec<QueryReference>,
    },
    Symbol(QuerySymbol),
    Context(QueryContext),
    References(Vec<QueryReference>),
    Dependents(Vec<QueryReference>),
    Symbols(Vec<QuerySymbol>),
    Text(Vec<QueryTextMatch>),
    Files(Vec<QueryFile>),
    Graph(QueryGraph),
    Syntax(QuerySyntax),
    Diff(QuerySymbolDiff),
    Impact {
        diff: QuerySymbolDiff,
        dependents: Vec<QueryReference>,
    },
    WhatChanged(changes::WhatChangedResult),
    DiffSymbols(changes::DiffSymbolsResult),
    DetectImpact(detect_impact::DetectImpactResult),
    Ask(ask::AskResult),
}
