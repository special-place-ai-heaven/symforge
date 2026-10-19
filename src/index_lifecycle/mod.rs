//! Feature 020 V11 index lifecycle: atomic mutation authority (Slice 1).
//!
//! Slice 1 introduces the authority types that make cross-root mutation and
//! publication impossible before the larger lifecycle runtime exists. Nothing in
//! this module samples separate fields to infer permission: a mutation is
//! authorized by one whole, exact, consumed authority or it is refused.
//!
//! Production already calls this module. Confined-write and source-authority
//! edges resolve
//! `crate::live_index::index_lifecycle::activation::project_source_authority`:
//! `src/protocol/edit.rs` (atomic repository-source replace via
//! `acquire_write`), `src/gitignore_hygiene.rs` and `src/live_index/persist.rs`
//! (`.gitignore` / `.gitattributes` via `acquire_write_serialized`),
//! `src/sidecar/handlers.rs` and `src/protocol/edit_tools.rs` (request-path
//! freshen), `src/live_index/store.rs` (admission identity and ABA eviction),
//! `src/live_index/single_file.rs` (admission/removal observation),
//! `src/daemon.rs` and `src/server/serve.rs` (background-verify observer
//! incarnation). Writer lanes land bytes under a `SourceMutationPermit` and
//! the leased physical root. T051's wired roster is the executing call-edge
//! allow-list; a `#[path]`/`pub mod` declaration and prose mentions are not
//! edges. Production names `observer::ObserverId` from `src/watcher/mod.rs` and
//! `src/live_index/persist.rs`, and `query::HealthProjection` from
//! `src/live_index/health_view.rs`. `candidate`, `supervisor`, and
//! `verification` still have no production callers.

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
#[cfg(feature = "embed")]
mod embed_ask;
#[cfg(feature = "embed")]
mod embed_batch;
#[cfg(feature = "embed")]
mod embed_changes;
#[cfg(feature = "embed")]
mod embed_detect_impact;
#[cfg(feature = "embed")]
pub(crate) mod embed_edit_body;
#[cfg(feature = "embed")]
pub(crate) mod embed_federation;
#[cfg(feature = "embed")]
mod embed_file_impact;
#[cfg(feature = "embed")]
mod embed_file_search;
#[cfg(feature = "embed")]
mod embed_git;
#[cfg(feature = "embed")]
mod embed_git_config;
#[cfg(feature = "embed")]
mod embed_guidance;
#[cfg(feature = "embed")]
pub(crate) mod embed_host;
#[cfg(feature = "embed")]
mod embed_knowledge;
#[cfg(feature = "embed")]
mod embed_knowledge_query;
#[cfg(feature = "embed")]
mod embed_mutation;
#[cfg(feature = "embed")]
pub(crate) mod embed_query;
#[cfg(feature = "embed")]
mod embed_read;
#[cfg(feature = "embed")]
mod embed_read_context;
#[cfg(feature = "embed")]
mod embed_reference;
#[cfg(feature = "embed")]
pub(crate) mod embed_remediation;
#[cfg(feature = "embed")]
pub(super) mod embed_restore;
#[cfg(feature = "embed")]
mod embed_route;
#[cfg(feature = "embed")]
mod embed_search;
#[cfg(feature = "embed")]
pub(crate) mod embed_session;
#[cfg(feature = "embed")]
mod embed_stel;
#[cfg(feature = "embed")]
mod embed_symbol;
#[cfg(feature = "embed")]
mod embed_symbol_context;
#[cfg(feature = "embed")]
mod embed_symforge;
#[cfg(feature = "embed")]
mod embed_symforge_edit;
#[cfg(feature = "embed")]
mod embed_temporal;
#[cfg(feature = "embed")]
mod embed_usage;
#[cfg(feature = "embed")]
mod embed_wire_edit;
pub mod embedded;
pub(crate) mod guidance;
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
