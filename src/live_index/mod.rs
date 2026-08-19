mod context_bundle;
pub mod coupling;
pub(crate) mod disambiguation;
pub(crate) use disambiguation::enclosing_impl_owner;
pub mod frecency;
pub mod git_temporal;
// Program 015 SP-0A spike -> C-S1A-002: name-based call graph + inbound BFS,
// now a real `detect_impact` production dependency (no longer cbm-spike-gated).
pub mod graph;
// Public within the crate tree for the C5 SEAM-HEALTH anchors (reachable
// externally only through the repo-internal test door).
pub mod health_view;
// Feature 020 V11 (C5 — the pre-plotted mount flip executed): the lifecycle
// directory is now mounted at the crate root through the private `internals`
// wrapper, and this alias keeps every `crate::live_index::index_lifecycle::`
// path — production and the integration suite's — resolving unchanged. No
// file moved. Under the repo-internal `__test-internals` door the alias is
// `pub` so the suite's `symforge::live_index::index_lifecycle::` paths keep
// working; in every supported cell it is crate-internal and the V11 surface
// is reachable ONLY through `embed`/`server_api`.
#[cfg(not(feature = "__test-internals"))]
pub(crate) use crate::index_lifecycle;
#[cfg(feature = "__test-internals")]
pub use crate::index_lifecycle;
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
