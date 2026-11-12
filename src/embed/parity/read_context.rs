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
    /// Append the repository knowledge digests (default off).
    #[serde(default)]
    pub include_knowledge: bool,
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
    #[serde(default, deserialize_with = "lowercase_sections")]
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

/// `sections` lowercased as it is decoded: the value table accepts section names in any
/// letter case and the handlers compare them exactly, as on the MCP lane.
pub(crate) fn lowercase_sections<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    let sections: Option<Vec<String>> = serde::Deserialize::deserialize(deserializer)?;
    Ok(sections.map(|items| {
        items
            .into_iter()
            .map(|item| item.to_ascii_lowercase())
            .collect()
    }))
}
