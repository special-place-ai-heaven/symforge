//! L0 compact surface registry (A-019: compact-3 wins over meta-1 / full-32).

/// Number of tools advertised on the compact STEL surface (H1 target).
pub const COMPACT_SURFACE_TOOL_COUNT: usize = 3;

/// Canonical compact-surface tool names (`SYMFORGE_SURFACE=compact`).
pub const COMPACT_TOOL_NAMES: [&str; COMPACT_SURFACE_TOOL_COUNT] =
    ["symforge", "symforge_edit", "status"];

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
}
