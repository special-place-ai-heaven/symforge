//! Feature 020 V11 index lifecycle: atomic mutation authority (Slice 1).
//!
//! Slice 1 introduces the authority types that make cross-root mutation and
//! publication impossible before the larger lifecycle runtime exists. Nothing in
//! this module samples separate fields to infer permission: a mutation is
//! authorized by one whole, exact, consumed authority or it is refused.
//!
//! The Slice 1 surface is deliberately self-contained. Production integration is
//! limited to the watcher/store mutation seam (T028); the writer lanes that
//! consume permits are Slice 4 activation work.

pub mod authority;
pub mod mutation;
pub mod physical_root;
pub mod transition;
