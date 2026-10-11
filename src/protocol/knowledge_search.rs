//! Frozen MCP path to the shared knowledge-search engine.

#[cfg(all(test, feature = "server"))]
pub(crate) use crate::knowledge::search::select_scoped_sources;
pub(crate) use crate::knowledge::search::{
    KnowledgeSearchProjection, budget_summary, search_scoped, search_scoped_projection,
    validate_input,
};
