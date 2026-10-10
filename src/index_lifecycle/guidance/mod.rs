//! Transport-independent guidance shared by MCP and embedded hosts.

pub mod conventions;
#[cfg(feature = "server")]
pub(crate) mod changes;
pub(crate) mod exploration;
pub mod explore;
pub(crate) mod filters;
pub(crate) mod file_search;
pub(crate) mod file_read;
pub(crate) mod symbol_read;
pub(crate) mod symbol_context;
pub(crate) mod reference_read;
pub(crate) mod reference_contract;
pub(crate) mod search_envelope;
#[cfg(feature = "server")]
pub(crate) mod search_render;
pub(crate) mod read_contract;
pub(crate) mod read_context;
pub(crate) mod read_gate;
pub(crate) mod read_admission_format;
pub mod withheld;
pub(crate) mod routing;
pub mod smart_query;
pub(crate) mod source;

pub mod compression;
pub mod investigation;
pub mod knowledge_model;
pub(crate) mod search;
pub(crate) mod search_contract;
pub(crate) mod serde_input;
pub mod session;
