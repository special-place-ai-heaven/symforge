//! Frozen MCP path to the shared knowledge-search engine.

#[cfg(all(test, feature = "server"))]
pub(crate) use crate::knowledge::search::select_scoped_sources;
pub(crate) use crate::knowledge::search::{search_scoped, validate_input};
