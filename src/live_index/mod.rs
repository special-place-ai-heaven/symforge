mod context_bundle;
pub mod coupling;
mod disambiguation;
pub mod frecency;
pub mod git_temporal;
// ponytail: Program 015 SP-0A spike module — name-based call graph + inbound BFS.
#[cfg(feature = "cbm-spike")]
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
