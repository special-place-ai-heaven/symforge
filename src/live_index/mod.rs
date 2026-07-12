mod context_bundle;
pub mod coupling;
mod disambiguation;
pub(crate) use disambiguation::enclosing_impl_owner;
pub mod frecency;
pub mod git_temporal;
// Program 015 SP-0A spike -> C-S1A-002: name-based call graph + inbound BFS,
// now a real `detect_impact` production dependency (no longer cbm-spike-gated).
pub mod graph;
mod health_view;
pub mod persist;
pub(crate) mod qualified_usages;
pub mod query;
pub mod rank_signals;
pub mod search;
pub mod store;
pub mod trigram;
pub mod view;

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
    CircuitBreakerState, IndexLoadSource, IndexState, IndexedFile, LiveIndex, ParseStatus,
    PublishedIndexState, PublishedIndexStatus, ReferenceLocation, SharedIndex, SharedIndexHandle,
    SnapshotVerifyState,
};
