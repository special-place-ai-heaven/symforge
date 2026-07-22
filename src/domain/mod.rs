pub mod index;

pub use index::{
    AccessErrorKind, AccessStage, CapabilityStatus, CapabilityUnavailableReason, CatalogEntry,
    CatalogPath, ControlStateDir, ControlStatePlacement, CoverageStatus, FileClass,
    FileClassification, FileDisposition, FileOutcome, FileProcessingResult, FileStamp,
    FreshnessReason, FreshnessStatus, GitignoreHygiene, HardSkipReason, HistoryCoverage,
    HistoryLimit, IndexTargets, KnowledgeUnit, KnowledgeUnitKind, LanguageId,
    ManifestResourceUsage, MetadataOnlyReason, ParseDiagnostic, ParseStatus, ProjectCapabilities,
    ProjectId, ProjectStateDir, ReferenceKind, ReferenceRecord, RepositoryFingerprint,
    RepositoryId, RepositoryManifest, RootBinding, RootCandidateSource, RootClass,
    RootRefusalReason, RootRequestMode, RootResolution, ScoutDecision, ScoutIssue, ScoutIssueKind,
    ScoutedEntry, SnapshotSourceIdentity, SourceAccessMode, SourceId, SourceIdentity,
    SourceLocation, SourceResponseEnvelope, SourceVersion, StateFailure, StateLocationKind,
    StatePlacement, SupportTier, SymbolKind, SymbolRecord, UnboundReason, UserLocalPlacementReason,
    WorkingTreeState, find_enclosing_symbol,
};
