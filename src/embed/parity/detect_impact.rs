//! Typed, bounded Git blast-radius evidence for an admitted embedded source.

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImpactScope {
    Files,
    #[default]
    Symbols,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DetectImpactRequest {
    pub base_branch: Option<String>,
    pub since: Option<String>,
    pub depth: u8,
    pub scope: ImpactScope,
    pub include_untracked: bool,
    pub include_data: bool,
}

impl Default for DetectImpactRequest {
    fn default() -> Self {
        Self {
            base_branch: None,
            since: None,
            depth: 2,
            scope: ImpactScope::Symbols,
            include_untracked: true,
            include_data: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactChangedSymbol {
    pub name: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactBlastEntry {
    pub symbol: String,
    pub hop: u32,
    pub risk: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactRiskSummary {
    pub critical: u64,
    pub high: u64,
    pub medium: u64,
    pub low: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactPage {
    pub total: u64,
    pub returned: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactPagination {
    pub changed_files: ImpactPage,
    pub changed_symbols: ImpactPage,
    pub blast_radius: ImpactPage,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactSourceFilter {
    pub applied: bool,
    pub excluded_paths: u64,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DetectImpactResult {
    pub changed_files: Vec<String>,
    pub changed_symbols: Vec<ImpactChangedSymbol>,
    pub blast_radius: Vec<ImpactBlastEntry>,
    pub risk_summary: ImpactRiskSummary,
    pub pagination: ImpactPagination,
    pub source_filter: ImpactSourceFilter,
    pub requested_depth: u8,
    pub effective_depth: u8,
    pub base_branch: Option<String>,
    pub staleness_note: Option<String>,
    /// MCP `detect_impact`'s text: the summary and the impact payload.
    #[serde(default)]
    pub rendered: String,
}
