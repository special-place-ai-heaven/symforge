//! Feature 020 V11 index lifecycle: atomic mutation authority (Slice 1).
//!
//! Slice 1 introduces the authority types that make cross-root mutation and
//! publication impossible before the larger lifecycle runtime exists. Nothing in
//! this module samples separate fields to infer permission: a mutation is
//! authorized by one whole, exact, consumed authority or it is refused.
//!
//! **Nothing in production calls this module.** `grep -rn index_lifecycle src/`
//! returns one hit outside it — the `pub mod` line in `live_index/mod.rs`.
//!
//! An earlier version of this comment said "production integration is limited to
//! the watcher/store mutation seam (T028)", which read as though T028 had wired
//! something here. It did not: T028 changed `effective_fence_generation` in
//! `src/watcher/mod.rs` to read one `Arc<PublishedGeneration>` instead of two
//! independent loads, and it names no type from this module. The fix and this
//! module share a motivation, not a call edge. Claiming an integration that does
//! not exist is the reporting defect this feature was written to prevent, so it
//! is corrected here rather than left to be discovered at activation.
//!
//! The writer lanes that consume permits are Slice 4 activation work.

pub mod authority;
pub mod capacity;
pub mod mutation;
pub mod physical_root;
pub mod process_runtime;
pub mod registry;
pub mod transition;
