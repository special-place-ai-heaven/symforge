//! Explicit source-bound context history and bounded retrieval of served output.

pub use crate::embed::lifecycle::embed_session::QuerySession;

/// Trusted per-session cache bounds. The native facade accepts lower limits than
/// the engine defaults, including zero to disable offloading. Not deserializable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SessionCacheLimits {
    pub max_bytes: u64,
    pub max_entries: u32,
}
impl Default for SessionCacheLimits {
    fn default() -> Self {
        Self {
            max_bytes: 32 * 1024 * 1024,
            max_entries: 256,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCreationRefusal {
    InvalidLimits,
    SourceUnavailable,
}
impl std::fmt::Display for SessionCreationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session creation refused: {self:?}")
    }
}
impl std::error::Error for SessionCreationRefusal {}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionEvidence {
    pub identity: String,
    pub revision_before: u64,
    pub revision_after: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SessionResetReceipt {
    pub identity: String,
    pub revision_before: u64,
    pub revision_after: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CacheDisposition {
    NoSession,
    NotApplicable,
    Stored,
    Reused,
    SkippedCapacity,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SessionCacheStatus {
    pub limits: SessionCacheLimits,
    pub resident_bytes: u64,
    pub resident_entries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionSymbol {
    pub path: String,
    pub name: String,
    pub approximate_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionFile {
    pub path: String,
    pub approximate_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionSummary {
    pub operation: String,
    pub approximate_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextInventory {
    pub fetched_symbols: Vec<SessionSymbol>,
    pub listed_symbols: Vec<SessionSymbol>,
    pub fetched_files: Vec<SessionFile>,
    pub listed_files: Vec<SessionFile>,
    pub summary_outputs: Vec<SessionSummary>,
    pub total_tokens: u64,
    pub duration_secs: u64,
    pub cache_hits: u32,
    pub cached_outputs: u32,
    pub bytes_stored: u64,
    pub retrieves: u32,
    pub bytes_retrieved: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InvestigationSuggestion {
    /// The shared engine's guidance, bounded by QueryLimits.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RetrievedOutput {
    /// Exact UTF-8 JSON bytes of the versioned served-query record. A page may
    /// split a UTF-8 code point; concatenate bytes before decoding.
    pub bytes: Vec<u8>,
    pub byte_start: u64,
    pub next_offset: Option<u64>,
    pub total_bytes: u64,
    pub content_generation: u64,
    pub current_content_generation: u64,
    /// A cached rendering is not evidence that its source is still current.
    pub superseded: bool,
}
