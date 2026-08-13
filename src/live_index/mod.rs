mod context_bundle;
pub mod coupling;
pub(crate) mod disambiguation;
pub(crate) use disambiguation::enclosing_impl_owner;
pub mod frecency;
pub mod git_temporal;
// Program 015 SP-0A spike -> C-S1A-002: name-based call graph + inbound BFS,
// now a real `detect_impact` production dependency (no longer cbm-spike-gated).
pub mod graph;
mod health_view;
// Feature 020 V11. The frozen seam contract places every lifecycle file at
// `src/index_lifecycle/`, so that is where the files live. The module PATH is
// a different question: a top-level `pub mod index_lifecycle;` would add
// `symforge::index_lifecycle` to the public-API census that the refreeze
// freezes for the whole preactivation period, and `introduced_v11_atoms` never
// names it — the V11 surface is re-exported through `embed`, not through a
// public lifecycle module. `#[path]` satisfies both: contract file location,
// unchanged public-API CENSUS — not unchanged public API. `live_index` is
// itself public, so these modules and their types ARE reachable by a consumer;
// the census counts only top-level `pub mod` in `lib.rs`, and that granularity
// is the contract's, not a claim that nothing was exposed. At activation
// (T060) this declaration is deleted and `lib.rs` gains a private
// `mod index_lifecycle;`, with the public types re-exported from `embed.rs`.
// No file moves then.
#[path = "../index_lifecycle/mod.rs"]
pub mod index_lifecycle;
pub mod knowledge_authority;
pub mod knowledge_bridge;
pub mod local_ref_scout;
pub mod persist;
pub(crate) mod qualified_usages;
pub mod query;
pub mod rank_signals;
pub mod search;
pub mod single_file;
pub mod store;
pub mod trigram;
pub mod view;
pub mod worktree_topology;

pub use query::{
    ContextBundleFoundView, ContextBundleReferenceView, ContextBundleSectionView,
    ContextBundleView, DependentFileView, DependentLineView, EnclosingSymbolView, FileContentView,
    FileOutlineView, FindDependentsView, FindReferencesView, GitActivityView, HealthStats,
    ImplBlockSuggestionView, ImplementationEntryView, ImplementationsView, InspectMatchFoundView,
    InspectMatchView, ReferenceContextLineView, ReferenceFileView, ReferenceHitView,
    RepoOutlineFileView, RepoOutlineView, SearchFilesCouplingEvidence,
    SearchFilesCouplingNeighbors, SearchFilesHit, SearchFilesResolveView, SearchFilesTier,
    SearchFilesView, SiblingSymbolView, SymbolDetailView, TraceSymbolView, TypeDependencyView,
    WhatChangedTimestampView,
};
pub use store::{
    AuthorityPublicationFence, CircuitBreakerState, CodeSignalsSnapshot,
    GitTemporalPublicationFence, IndexLoadSource, IndexState, IndexedFile, LiveIndex, ParseStatus,
    PreparedKnowledgeAuthority, PublicationFence, PublishedGeneration, PublishedIndexState,
    PublishedIndexStatus, PublishedSourceSet, ReferenceLocation, SharedIndex, SharedIndexHandle,
    SnapshotVerifyState,
};
