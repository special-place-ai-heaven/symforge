//! Typed full file discovery and optional ranking evidence.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSearchRequest {
    #[serde(default)]
    pub query: String,
    pub limit: Option<u32>,
    pub current_file: Option<String>,
    pub changed_with: Option<String>,
    pub resolve: Option<bool>,
    pub estimate: Option<bool>,
    pub max_tokens: Option<u64>,
    pub debug_ranking: Option<bool>,
    pub rank_by: Option<FileRanking>,
    pub anchor_path: Option<String>,
    pub include_vendor: Option<bool>,
    pub include_personal_tooling: Option<bool>,
    pub path_prefix: Option<String>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileRanking {
    #[default]
    #[serde(rename = "path")]
    Path,
    #[serde(rename = "frecency")]
    Frecency,
    #[serde(rename = "path+cochange")]
    PathCochange,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMatchTier {
    CoChange,
    StrongPath,
    Basename,
    LoosePath,
    MetadataOnly,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileSearchHit {
    pub path: String,
    pub tier: FileMatchTier,
    pub coupling_score: Option<f32>,
    pub shared_commits: Option<u32>,
    pub metadata_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileResolution {
    Resolved {
        path: String,
        metadata_reason: Option<String>,
    },
    Ambiguous {
        candidates: Vec<String>,
        overflow_count: u64,
    },
    NotFound,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingCapability {
    FrecencyRanking,
    CoChangeRanking,
    RankingDiagnostics,
    WorktreeRouting,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingStatus {
    Applied,
    Ready,
    Preparing,
    Unavailable,
    DisabledByPolicy,
    FallbackUsed,
    Stale,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingFreshness {
    Current,
    Stale,
    Empty,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingCost {
    Free,
    Low,
    Bounded,
    Expensive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingSafety {
    ReadOnly,
    WriteRequiresConsent,
    OperatorDiagnostics,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RankingEvidence {
    pub capability: RankingCapability,
    pub status: RankingStatus,
    pub freshness: RankingFreshness,
    pub cost: RankingCost,
    pub safety: RankingSafety,
    pub detail: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CoChangeState {
    Loading,
    Unavailable { reason: String },
    NoHistory,
    NoCoupling,
    Strong,
    Weak,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoChangeSummary {
    pub state: CoChangeState,
    pub analyzed_commits: u64,
    pub low_history_confidence: bool,
    pub deprecated_changed_with: bool,
}
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileSearchResult {
    pub hits: Vec<FileSearchHit>,
    pub total_matches: u64,
    pub overflow_count: u64,
    pub suppressed_by_noise: u64,
    pub resolution: Option<FileResolution>,
    pub cochange: Option<CoChangeSummary>,
    pub ranking: Vec<RankingEvidence>,
    pub ranking_explanation: Option<String>,
}
