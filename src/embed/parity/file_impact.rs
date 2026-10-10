//! MCP `analyze_file_impact` for an admitted embedded source: re-admit one
//! file from disk and report its symbol diff, or index a new file.

/// Every MCP `analyze_file_impact` option. `path` is repository-relative or
/// absolute beneath the source root, as MCP accepts.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileImpactRequest {
    pub path: String,
    /// Treat the path as a new file to index (MCP `new_file`).
    pub new_file: Option<bool>,
    /// Append git temporal data for the path (MCP `include_co_changes`).
    pub include_co_changes: Option<bool>,
    /// Co-change rows to show; MCP's default is 10.
    pub co_changes_limit: Option<u32>,
    /// Return only the token estimate; nothing is re-admitted.
    pub estimate: Option<bool>,
}

/// The MCP tool's text answer, rendered by the shared impact engine.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FileImpactReport {
    pub rendered: String,
}
