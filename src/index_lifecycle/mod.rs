//! Feature 020 V11 index lifecycle: atomic mutation authority (Slice 1).
//!
//! Slice 1 introduces the authority types that make cross-root mutation and
//! publication impossible before the larger lifecycle runtime exists. Nothing in
//! this module samples separate fields to infer permission: a mutation is
//! authorized by one whole, exact, consumed authority or it is refused.
//!
//! **Nothing in production calls this module.** The darkness property is about
//! CALL EDGES, not grep hits: no code outside this directory names an item in
//! it. The module's declaration in `live_index/mod.rs` — a `#[path]` attribute
//! and the `pub mod` line it decorates — is not a call edge, and since T043 the
//! doc comments of `src/lifecycle_identity.rs` mention this directory by name
//! in prose, which is not one either. An earlier version of this paragraph
//! stated the property as "grep returns no hit outside", which prose alone now
//! falsifies; T051 formalizes the call-edge form into an executing test.
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

// T066: the activation mode machine — LegacyOpen -> LegacyClosing ->
// PreventiveV1Open, monotonic, process-wide, non-configurable. The cut's
// wiring (T064/T067) is its only planned production caller.
pub mod activation;
pub mod adapters;
pub mod authority;
// T059/T060: the dark per-source supervisor and isolated candidate pipeline.
// In-directory and oracle-suite consumption only; T064/T066 activation is the
// only planned production caller.
pub mod candidate;
pub mod capacity;
pub mod embedded;
pub mod mutation;
pub mod physical_root;
pub mod process_runtime;
// T061: dark observer accumulator — bounded coalescing, monotonic cuts,
// latch-forces-baseline, drain-before-successor handoff. Oracle-suite
// consumption only until activation.
pub mod observer;
// T063: dark strict query leases — atomic exact-bijection selections,
// sealed render authority, refusal-not-no-match, and the two-ledger health
// projection seam. Oracle-suite consumption only until activation.
pub mod query;
pub mod registry;
// T065: dark snapshot migration — untrusted seeds, pre-decode capacity,
// quarantine with preserved rollback, namespace isolation, and the FR-051
// team-artifact matrix. Oracle-suite consumption only until activation.
pub mod snapshot;
pub mod supervisor;
// T062: dark rolling verification — sealed scope receipts, the fixed
// 15-minute deadline with its latch-before-acquisition ordering, work
// bounds, and feasibility. Oracle-suite consumption only until activation.
pub mod verification;
// T047: the dark V11 runtime. In-directory consumption only; the dark factory
// is the single door and Slice 4's activation cut is the only planned caller.
pub mod runtime;
// T048: the embed boundary's dark rehearsal — wrap table, contract-shaped
// wrappers, and the export delta renderer.
pub mod public_api;
pub mod transition;
