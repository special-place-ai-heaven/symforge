//! Search-oriented MCP tool input types and pure request parsing helpers.

pub use crate::index_lifecycle::guidance::search_contract::{
    SearchFilesInput, SearchSymbolsInput, SearchTextInput,
};

pub use crate::knowledge::search_contract::{KnowledgeAuthorityScope, SearchKnowledgeInput};

pub(crate) use crate::knowledge::review_contract::KnowledgeSourceScope;
pub use crate::knowledge::review_contract::ReviewKnowledgeInput;
#[cfg(test)]
pub(crate) use crate::knowledge::review_contract::ReviewKnowledgeMode;

pub use crate::knowledge::curation_contract::CurateKnowledgeInput;

pub use crate::index_lifecycle::guidance::reference_contract::FindReferencesInput;

pub(crate) use crate::index_lifecycle::guidance::filters::parse_language_filter;

#[cfg(test)]
pub(crate) use crate::index_lifecycle::guidance::filters::normalize_search_text_glob;
