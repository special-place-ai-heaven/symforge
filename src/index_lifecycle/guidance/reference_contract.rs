//! Shared reference query contract.
use super::serde_input::{lenient_bool, lenient_u32, lenient_u64};
use serde::{Deserialize, Serialize};

/// Input for `find_references`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FindReferencesInput {
    /// Symbol name to find references for (or trait/type name when mode='implementations').
    pub name: String,
    /// Filter by reference kind: "call", "import", "type_usage", or "all" (default: "all"). Ignored when mode='implementations'.
    pub kind: Option<String>,
    /// Optional exact-selector path from `search_symbols`, for example `src/db.rs`. Ignored when mode='implementations'.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`. Ignored when mode='implementations'.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`. Ignored when mode='implementations'.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Maximum number of files/entries to show (default 20 for references, 200 for implementations; capped at 100/500).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Maximum number of reference hits per file (default 10, capped at 50). Ignored when mode='implementations'.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// When true, show compact output: file:line \[kind\] in symbol — no source text (60-75% smaller). Ignored when mode='implementations'.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub compact: Option<bool>,
    /// Mode: "references" (default — call sites, imports, type usages) or "implementations" (trait/interface implementors and implemented traits). When mode='implementations', only name, direction, and limit are used.
    #[serde(default)]
    pub mode: Option<String>,
    /// Search direction for implementations mode: "trait" (find implementors), "type" (find traits a type implements), or "auto" (default: search both).
    #[serde(default)]
    pub direction: Option<String>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Optional open-project ID/alias; exclusive with `projects`. Omit both for
    /// the active project. IDs/aliases only, never paths.
    #[serde(default)]
    pub project: Option<String>,
    /// Feature 012 (Phase 3): target an EXPLICIT subset of open projects by
    /// id/alias, or `["*"]` for every open project. Mutually exclusive with
    /// `project`; an empty list is rejected. Daemon-only.
    // `#[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]` keeps this a plain `type: "array"`
    // schema, NOT the `type: ["array", "null"]` union that strict MCP clients
    // reject (mirrors `SearchTextInput::terms`; enforced by
    // `tests/strict_client_schema_compat.rs`); serde keeps the field optional.
    #[serde(default)]
    #[cfg_attr(feature = "server", schemars(with = "Vec<String>"))]
    pub projects: Option<Vec<String>>,
}
