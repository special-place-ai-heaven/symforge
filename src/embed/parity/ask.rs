//! Natural-language routing and full native execution under one captured source.
use super::{QueryOutput, QueryRequest};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AskRequest {
    pub project: Option<String>,
    pub query: String,
    pub max_tokens: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AskRoute {
    FindCallers,
    FindSymbol,
    FindFile,
    FindChanges,
    Understand,
    UnderstandSymbol,
    UnderstandImplementations,
    SearchCode,
    SearchKnowledge,
    RepositoryOrientation,
    FindDependents,
    FindImplementations,
    ToolHelp,
    Explore,
}
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskResult {
    pub route: AskRoute,
    pub confidence: String,
    pub invocation: String,
    pub rationale: String,
    pub suggested_next_step: Option<String>,
    pub routed_request: Option<Box<QueryRequest>>,
    pub output: Option<Box<QueryOutput>>,
    pub rendered: String,
}
