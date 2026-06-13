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
    pub const ALL: [Self; COMPACT_SURFACE_TOOL_COUNT] = [
        Self::Symforge,
        Self::SymforgeEdit,
        Self::Status,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symforge => "symforge",
            Self::SymforgeEdit => "symforge_edit",
            Self::Status => "status",
        }
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
