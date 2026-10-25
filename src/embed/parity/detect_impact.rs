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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
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
            paths: None,
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
    #[serde(default)]
    pub evidence_status: ImpactEvidenceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Vec<ImpactPathStep>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactEvidenceStatus {
    Available,
    Truncated,
    #[default]
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactPathStep {
    pub caller: ImpactChangedSymbol,
    pub callee: ImpactChangedSymbol,
    pub call_site: ImpactCallSite,
    pub resolution: ImpactResolution,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImpactCallSite {
    pub path: String,
    /// Half-open raw byte range `[start, end)` into the source file.
    pub byte_range: [usize; 2],
    /// Half-open, zero-based line range `[start, end)`.
    pub line_range: [usize; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactResolution {
    UniqueName,
    QualifiedModule,
    QualifiedOwner,
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

#[cfg(test)]
mod tests {
    use super::{ImpactBlastEntry, ImpactEvidenceStatus};

    #[test]
    fn blast_entry_deserializes_legacy_payload_without_evidence() {
        let entry: ImpactBlastEntry = serde_json::from_value(serde_json::json!({
            "symbol": "src/lib.rs::caller",
            "hop": 1,
            "risk": "medium"
        }))
        .expect("legacy impact row remains readable");

        assert_eq!(entry.evidence_status, ImpactEvidenceStatus::Unavailable);
        assert!(entry.evidence.is_none());
    }
}
