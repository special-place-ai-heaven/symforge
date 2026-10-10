//! Transport-independent guidance shared by MCP and embedded hosts.

pub(crate) mod changes;
pub mod conventions;
pub(crate) mod edit_plan;
pub(crate) mod edit_route;
pub(crate) mod exploration;
pub mod explore;
pub(crate) mod file_impact;
pub(crate) mod file_read;
pub(crate) mod file_search;
pub(crate) mod filters;
pub(crate) mod freshen;
pub(crate) mod health;
pub(crate) mod impact;
pub(crate) mod read_admission_format;
pub(crate) mod read_context;
pub(crate) mod read_contract;
pub(crate) mod read_gate;
pub(crate) mod reference_contract;
pub(crate) mod reference_read;
pub(crate) mod routing;
pub(crate) mod search_envelope;
pub(crate) mod search_render;
pub mod smart_query;
pub(crate) mod source;
pub(crate) mod symbol_context;
pub(crate) mod symbol_read;
pub mod withheld;

pub mod compression;
pub mod investigation;
pub mod knowledge_model;
pub(crate) mod search;
pub(crate) mod search_contract;
pub(crate) mod serde_input;
pub mod session;
