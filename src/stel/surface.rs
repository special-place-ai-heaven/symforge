//! L0 compact surface registry (A-019: compact-3 wins over meta-1 / full-32).

/// Number of tools advertised on the compact STEL surface (H1 target).
pub const COMPACT_SURFACE_TOOL_COUNT: usize = 3;

/// Canonical compact-surface tool names (`SYMFORGE_SURFACE=compact`).
pub const COMPACT_TOOL_NAMES: [&str; COMPACT_SURFACE_TOOL_COUNT] =
    ["symforge", "symforge_edit", "status"];

/// Tools advertised on the opt-in core profile (SF-TE-05). Every other
/// registered tool stays callable there; it is only not listed.
pub const CORE_TOOL_NAMES: [&str; 13] = [
    "status",
    "get_repo_map",
    "get_file_context",
    "get_file_content",
    "search_text",
    "search_symbols",
    "get_symbol",
    "find_references",
    "replace_symbol_body",
    "edit_within_symbol",
    "batch_edit",
    "index_folder",
    // Overflow footers and cache-hit notices from core tools point here.
    "symforge_retrieve",
];

/// Environment variable carrying the operator's per-process tool allow-list
/// (issue #874). `symforge serve --tools` sets the same list for that process.
/// The embed lane has no tool surface and ignores it.
pub const TOOLS_ENV: &str = "SYMFORGE_TOOLS";

/// Named groups accepted in [`TOOLS_ENV`]. Together they are exactly the 40
/// advertised full-surface tools, each in one group (pinned by a test).
pub const TOOL_GROUPS: [(&str, &[&str]); 5] = [
    (
        "read",
        &[
            "search_symbols",
            "get_symbol",
            "find_references",
            "get_file_context",
            "search_text",
            "search_files",
            "get_file_content",
            "get_symbol_context",
            "get_repo_map",
            "find_dependents",
            "explore",
        ],
    ),
    (
        "edit",
        &[
            "batch_edit",
            "edit_within_symbol",
            "batch_insert",
            "replace_symbol_body",
            "batch_rename",
            "insert_symbol",
            "delete_symbol",
            "edit_plan",
            "symforge_edit",
        ],
    ),
    (
        "knowledge",
        &["curate_knowledge", "review_knowledge", "search_knowledge"],
    ),
    (
        "impact",
        &[
            "what_changed",
            "detect_impact",
            "diff_symbols",
            "analyze_file_impact",
        ],
    ),
    (
        "service",
        &[
            "status",
            "health",
            "health_compact",
            "index_folder",
            "checkpoint_now",
            "context_inventory",
            "conventions",
            "validate_file_syntax",
            "secret_remediate",
            "inspect_match",
            "ask",
            "investigation_suggest",
            "symforge_retrieve",
        ],
    ),
];

/// Number of tools the allow-list can name: the advertised full surface.
pub fn allow_list_tool_count() -> usize {
    TOOL_GROUPS.iter().map(|(_, tools)| tools.len()).sum()
}

/// A parsed [`TOOLS_ENV`] value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAllowList {
    /// Allowed tool names, de-duplicated, in [`TOOL_GROUPS`] order.
    pub allowed: Vec<&'static str>,
    /// Tokens that name no tool or group, as written. Never silently dropped:
    /// startup refuses or warns, and `status` and the instructions show them.
    pub unknown: Vec<String>,
}

impl ToolAllowList {
    /// Parse comma-separated, trimmed, case-insensitive tool or group names.
    /// Empty tokens are skipped. The compact-only `symforge` facade is not on
    /// the surface this list filters, so it is reported as unknown.
    pub fn parse(spec: &str) -> Self {
        let mut wanted: Vec<&'static str> = Vec::new();
        let mut unknown = Vec::new();
        for token in spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            let lower = token.to_ascii_lowercase();
            if let Some((_, members)) = TOOL_GROUPS.iter().find(|(group, _)| *group == lower) {
                wanted.extend(members.iter().copied());
            } else if let Some(tool) = TOOL_GROUPS
                .iter()
                .flat_map(|(_, members)| members.iter())
                .find(|tool| **tool == lower)
            {
                wanted.push(tool);
            } else if lower == "symforge" {
                unknown.push(format!("{token} (compact-only, not on this surface)"));
            } else {
                unknown.push(token.to_string());
            }
        }
        let allowed = TOOL_GROUPS
            .iter()
            .flat_map(|(_, members)| members.iter().copied())
            .filter(|tool| wanted.contains(tool))
            .collect();
        Self { allowed, unknown }
    }

    pub fn allows(&self, tool: &str) -> bool {
        self.allowed.contains(&tool)
    }

    /// Groups with at least one allowed member, for the short instructions form.
    pub fn touched_groups(&self) -> Vec<&'static str> {
        TOOL_GROUPS
            .iter()
            .filter(|(_, members)| members.iter().any(|tool| self.allows(tool)))
            .map(|(group, _)| *group)
            .collect()
    }
}

/// Compact L0 tool identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactSurfaceTool {
    Symforge,
    SymforgeEdit,
    Status,
}

impl CompactSurfaceTool {
    pub const ALL: [Self; COMPACT_SURFACE_TOOL_COUNT] =
        [Self::Symforge, Self::SymforgeEdit, Self::Status];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symforge => "symforge",
            Self::SymforgeEdit => "symforge_edit",
            Self::Status => "status",
        }
    }
}

/// The MCP tool-list profile a connection is served (`SYMFORGE_SURFACE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceProfile {
    /// Opt-in (`SYMFORGE_SURFACE=core`): lists [`CORE_TOOL_NAMES`]; unlisted tools stay callable.
    Core,
    Full,
    Compact,
    Meta,
}

/// Canonical lowercase label (`full`/`compact`/`meta`) for a surface profile.
///
/// Single source of the wording used by the `status` readout AND threaded across
/// the adapter→daemon proxy boundary, so both processes agree on the exact
/// string (see `StelStatusRequest::connection_surface`).
pub fn surface_profile_label(profile: SurfaceProfile) -> &'static str {
    match profile {
        SurfaceProfile::Core => "core",
        SurfaceProfile::Full => "full",
        SurfaceProfile::Meta => "meta",
        SurfaceProfile::Compact => "compact",
    }
}

/// Map a proxy-threaded connection-surface string back to a canonical static
/// label. Returns `None` for anything the adapter would never send, so an
/// unrecognized value falls back to the daemon's own env rather than echoing
/// arbitrary text into the trust readout.
///
/// Delegates to [`surface_profile_from_label`] — the single canonical str→enum
/// parser — so a new [`SurfaceProfile`] variant is added in exactly one place;
/// this label round-trip inherits it instead of drifting behind a private table.
pub fn surface_label_from_str(value: &str) -> Option<&'static str> {
    surface_profile_from_label(value).map(surface_profile_label)
}

/// Map a canonical connection-surface label back to a [`SurfaceProfile`].
///
/// Mirrors [`surface_label_from_str`] but yields the profile the guard renderer
/// needs. Returns `None` for anything an adapter would never send, so an
/// unrecognized header falls back to the daemon's own env rather than trusting
/// arbitrary text.
pub fn surface_profile_from_label(value: &str) -> Option<SurfaceProfile> {
    match value {
        "core" => Some(SurfaceProfile::Core),
        "full" => Some(SurfaceProfile::Full),
        "meta" => Some(SurfaceProfile::Meta),
        "compact" => Some(SurfaceProfile::Compact),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_tool_names_match_a019_registry() {
        let from_enum: Vec<&str> = CompactSurfaceTool::ALL.iter().map(|t| t.as_str()).collect();
        assert_eq!(from_enum.as_slice(), COMPACT_TOOL_NAMES);
    }

    #[test]
    fn allow_list_groups_expand_and_mixed_lists_deduplicate() {
        let list = ToolAllowList::parse(" Knowledge , search_knowledge,STATUS,, status ");
        assert_eq!(
            list.allowed,
            vec![
                "curate_knowledge",
                "review_knowledge",
                "search_knowledge",
                "status"
            ]
        );
        assert!(list.unknown.is_empty());
        assert_eq!(list.touched_groups(), vec!["knowledge", "service"]);
    }

    #[test]
    fn allow_list_reports_unknown_names_and_the_compact_facade() {
        let list = ToolAllowList::parse("get_symbol,bogus,symforge");
        assert_eq!(list.allowed, vec!["get_symbol"]);
        assert_eq!(
            list.unknown,
            vec![
                "bogus".to_string(),
                "symforge (compact-only, not on this surface)".to_string()
            ]
        );
    }

    #[test]
    fn allow_list_groups_are_disjoint_and_never_named_like_a_tool() {
        let tools: Vec<&str> = TOOL_GROUPS
            .iter()
            .flat_map(|(_, members)| members.iter().copied())
            .collect();
        let unique: std::collections::BTreeSet<&str> = tools.iter().copied().collect();
        assert_eq!(unique.len(), tools.len(), "a tool sits in two groups");
        assert_eq!(allow_list_tool_count(), 40);
        for (group, _) in TOOL_GROUPS {
            assert!(!unique.contains(group), "group `{group}` shadows a tool");
        }
    }
}
