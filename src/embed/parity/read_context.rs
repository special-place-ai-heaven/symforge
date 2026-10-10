//! Bounded repository orientation and file-context selections.

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoMapRequest {
    pub project: Option<String>,
    pub detail: Option<String>,
    pub path: Option<String>,
    pub depth: Option<u32>,
    pub max_files: Option<u32>,
    #[serde(default)]
    pub estimate: bool,
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepoMapResult {
    pub detail: String,
    pub rendered: String,
    pub estimated_tokens: Option<u64>,
    pub total_files: u64,
    pub shown_files: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileContextRequest {
    pub project: Option<String>,
    pub path: String,
    pub max_tokens: Option<u64>,
    pub sections: Option<Vec<String>>,
    #[serde(default)]
    pub include_tests: bool,
    #[serde(default)]
    pub estimate: bool,
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileContextResult {
    pub path: String,
    pub rendered: String,
    pub estimated_tokens: Option<u64>,
    pub raw_file_tokens: Option<u64>,
    pub selected_sections: Vec<String>,
    pub cache_hit: bool,
}
