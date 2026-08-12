# Data Model: Repository Knowledge Index

**V11 authority**: This file is refrozen against the cleared project-index lifecycle
prevention design. V10 structs retained in historical review artifacts are not
constructors or publication authority. The canonical external Rust surface is the
closed `contracts/public-api-v11.json`; lifecycle authority types below have private,
sealed constructors.

## Design rules

- The manifest stores metadata and disposition, never duplicate file contents.
- Source bytes remain in `IndexedFile`; search indices remain derived.
- A complete promoted `RepositoryManifest` is the sole committed disposition
  authority. Failed/cancelled/refused observations use separate `AttemptAccounting`
  and cannot mint a manifest, digest, completeness claim, or generation.
- The legacy stored
  `LiveIndex.skipped_files` state is removed; any compatibility response is
  projected ephemerally from the manifest.
- Vectors use canonical ordering before hashing/serialization.
- Transient operational fields do not affect logical manifest identity.
- Targets overlap; `FileClass` alone must not encode routing.
- Mental-model, bridge, and authority records contain compact anchors into existing
  content/symbol state, never copied document bodies or generated summaries.
- Lifecycle policy is repo-owned input; snapshots cache its derived result but are
  never the authority for an archive/supersession decision.
- `Current` is the only source state from which a generation-backed consumer may
  acquire a strict query lease. A retained `VerifiedGeneration` inside another state
  is internal recovery evidence, not a stale/degraded public read lane. Explicit
  `DiskObservation`, `WorktreeScopeObservation`, and `GitObservation` lanes carry
  their own typed authority and do not acquire or imply a generation; lifecycle
  health/progress is captured independently through `capture_source_view()`.
- Candidates, snapshot seeds, and proof-refresh work are isolated and capacity-
  charged. Only the lifecycle module may construct them; only the registry may commit
  a sealed source transition into the immutable project runtime root.
- Every successful generation-authority claim is built from one sealed strict lease
  and names exact operation, source/selection, and claim provenance. Observation
  claims instead name their exact observation authority. Non-current generation
  acquisition yields the closed `SourceRefusal` algebra.

## SourceIdentity

```rust
pub struct ProjectId(String);
pub struct RepositoryId(String);
pub struct SourceId(String);

pub enum SourceLocation {
    WorkingTree { worktree_id: String },
    GitRef { name: String },
}

pub enum WorkingTreeState {
    Clean,
    Dirty,
    NotApplicable,
    Unknown,
}

pub struct SourceVersion {
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub working_tree: WorkingTreeState,
}

pub struct SourceIdentity {
    pub repository_id: RepositoryId,
    pub source_id: SourceId,
    pub location: SourceLocation,
}
```

`ProjectId` is the path-equivalent placement/session key for one canonical worktree
root. `RepositoryId` groups sources that share one canonical Git common directory;
for a non-Git source it is derived from `ProjectId`. `SourceId` is a stable digest of
`RepositoryId` plus the worktree identity or full local-ref name. It excludes moving
branch/HEAD/commit values; those live in `SourceVersion` and advance source content
state rather than changing source identity. Constructors use domain-separated,
lossless native path/ref bytes and never basename, remote URL, or `to_string_lossy`.

`WorkingTreeState` is closed and never inferred optimistically. Checked-out Git
sources use `Clean` or `Dirty`; immutable Git refs use `NotApplicable`; non-Git or
bounded-inspection failures use `NotApplicable` or `Unknown` respectively. Dirty
content identity still comes from the canonical manifest/content digests, not a
branch label, timestamp, or the `Dirty` flag alone.

For filesystem content, `content_hash` identifies stable admitted bytes. For an
immutable Git blob, object ID is the declared identity and may also back the content
hash after size/admission succeeds. Every result combines `SourceIdentity`, its
captured `SourceVersion`, an exact safe path, and content/object identity.

## Source-root resolution, state placement, and init hygiene

```rust
pub enum RootCandidateSource {
    WorkspaceEnvironment,
    McpClientRoot,
    GitAncestor,
    LaunchCwd,
    ExplicitIndexFolder,
    InitCwd,
}

pub enum RootRequestMode {
    Automatic,
    Init,
    ExplicitIndexFolder { allow_protected_root: bool },
}

pub enum RootClass {
    Normal,
    Protected,
    NeverIndexable,
}

pub enum RootRefusalReason {
    FilesystemOrDriveRoot,
    HomeOrProfileRoot,
    OsOrSensitiveTree,
    BroadContainer,
    SymlinkAliasToForbiddenRoot,
    MissingOrNotDirectory,
    CanonicalizationFailed,
    DeviceOrSpecialNamespace,
    ProtectedRootRequiresExplicitOverride,
}

pub enum UnboundReason {
    NoCandidateDeclared,
    Refused(RootRefusalReason),
}

pub enum SourceAccessMode {
    NormalProject,
    ExplicitProtected,
}

pub struct RootBinding {
    pub source: RootCandidateSource,
    pub canonical_root: PathBuf,
    pub root_id: ProjectId,
    pub access_mode: SourceAccessMode,
}

pub enum SessionMembershipAuthority {
    NormalResolved,
    ExplicitProtectedRequest { request_hash: String },
}

pub struct ProjectMembership {
    pub project_id: ProjectId,
    pub authority: SessionMembershipAuthority,
}

pub struct ProjectStateDir(PathBuf);
pub struct ControlStateDir(PathBuf);

pub enum RootResolution {
    Bound(RootBinding),
    Unbound {
        rejected_source: Option<RootCandidateSource>,
        reason: UnboundReason,
        safe_path_id: Option<String>,
    },
}

pub enum StatePlacement {
    ProjectLocal { directory: ProjectStateDir },
    UserLocal {
        directory: ProjectStateDir,
        root_id: ProjectId,
        reason: UserLocalPlacementReason,
    },
    MemoryOnly { failures: Vec<StateFailure> },
}

pub enum UserLocalPlacementReason {
    ExplicitProtected,
    ProjectLocalUnavailable { safe_reason: AccessErrorKind },
}

pub enum StateLocationKind {
    ProjectLocal,
    UserLocal,
}

pub struct StateFailure {
    pub location: StateLocationKind,
    pub safe_reason: AccessErrorKind,
}

pub enum CapabilityUnavailableReason {
    ExplicitProtectedSource,
    SourceReadOnly,
    PersistentStateUnavailable,
    DurableMutationReplayUnavailable,
    NonProjectLocalPlacement,
    AtomicDurabilityUnavailable,
}

pub enum CapabilityStatus {
    Available,
    Unavailable { reason: CapabilityUnavailableReason },
}

pub struct ProjectCapabilities {
    pub persistent_snapshots: CapabilityStatus,
    pub checkpoint: CapabilityStatus,
    pub structural_edits: CapabilityStatus,
    pub repository_init: CapabilityStatus,
    pub repository_curation: CapabilityStatus,
    pub team_artifact_export: CapabilityStatus,
}

pub struct BoundProject {
    pub root: RootBinding,
    pub state: StatePlacement,
    pub capabilities: ProjectCapabilities,
}

pub enum ControlStatePlacement {
    UserLocal { directory: ControlStateDir },
    ProcessLocal { safe_reason: AccessErrorKind },
}

pub enum GitignoreHygiene {
    Effective,
    MissingRule,
    NoRootGitignore,
    Unverifiable { safe_reason: AccessErrorKind },
    NotApplicableExplicitProtected,
}
```

`RootResolution` decides only which source may be traversed. Raw and canonical
candidates are classified independently and the stricter `RootClass` wins.
Automatic/init requests never authorize a protected root; an explicit
`index_folder` request authorizes only the exact canonical target when its opt-in
flag is true. `NeverIndexable` remains refused with any flag. An unbound server owns
no empty `LiveIndex`, source slot, watcher, candidate, binding/root authority, or
project runtime root. Protocol/static surfaces and unbound health or admission-attempt
evidence remain available, and a later authorized request may begin a fresh admission.
Rejected/failed retargeting preserves any prior binding, watcher, and published
generation. Rejection never falls through to launch CWD or another undeclared
source, and `ExplicitProtected` cannot be reconstructed by a reconnect/session API.
`UnboundReason::NoCandidateDeclared` represents a healthy initial server with no
declared project; it is not misreported as a filesystem refusal.

Authorization to join a daemon project is session-local. A normal project may be
joined through existing validated normal-root/session routing. A protected
`ProjectInstance` requires a fresh direct `index_folder` request on that session with
the exact path and `allow_protected_root=true`; project ID/alias selection,
`projects=["*"]`, reconnect descriptors, environment/client roots, and a prior
session's membership cannot mint it. A matching explicit request may attach to an
already-running canonical project slot without duplicating its watcher. Restart does
not auto-open a protected source from persisted state; the first session must
reauthorize it explicitly.

An `index_folder` replay record is historical execution evidence, not membership or
a live binding. Same-key/same-hash replay first re-establishes the current session's
authorized binding; if that postcondition cannot be restored, the tool returns a
successful typed `applied=false`/`live_postcondition_unavailable` result rather than
stale applied success or an MCP error, without rewriting the stored receipt.

`ControlStatePlacement` is the existing process-global transport/daemon/replay
coordination lane, not project content state. It is resolved independently from a
private user-local application base and may exist while the server is unbound. It
never uses launch CWD, a candidate source, or a relative `.symforge`; unavailable
global persistence produces process-local coordination rather than a root fallback.
No `BoundProject` or per-project state entry exists until `RootResolution::Bound`.

`StatePlacement` is selected after a root is bound. A normal project first tries
its local `.symforge/`. Explicit-protected mode never probes or creates that path;
it starts with a private user-local application-state directory keyed by `root_id`.
A normal project-local permission/access failure takes the same user-local fallback.
`root_id` uses the one existing/shared `ProjectId` constructor: a versioned digest
of the lossless canonical-root identity using the
host platform's path-equivalence rules, so aliases coalesce while distinct
repositories and linked worktrees do not collide. If the user-local directory
cannot be secured/created, the project still publishes a live memory-only index.
`MemoryOnly.failures` is non-empty, canonically ordered, bounded, and contains only
safe error classes for attempted locations (explicit-protected mode has no
project-local attempt to record). Only persistence-dependent capabilities become
unavailable.

`ProjectCapabilities` is constructed once from access mode, source writability,
placement, durable replay availability, and atomic-durability support; callers never
assemble independent booleans. All non-probe curation requirements are evaluated
first. Only a first apply use whose normal current-worktree, writable-source, and
durable replay/intent requirements are already `Available` may run durability probes;
explicit-protected, read-only, ref, implicit-worktree, and `MemoryOnly` bindings return
their typed reason with zero probe file operations anywhere under the source root.
`MemoryOnly` keeps live queries, watching, and guarded project selection available but
disables snapshots/checkpoint and every mutation that requires durable replay; normal
project-aware init hygiene may remain available only when its source write is
independently guarded/idempotent. Team-artifact export additionally requires
`ProjectLocal`: excluded `.symforge/index.bin.zst` plus artifact metadata are
`ProjectStateDir` persistence-only, while an in-scope companion `.gitattributes`
change is a separately permitted source mutation. Curation apply additionally
requires the contract-tested platform
atomic-durability primitive and a successful first-use same-directory capability
probe in every directory that will receive a durable curation record: the ledger
parent and the `ProjectStateDir` replay/intent-journal parent, deduplicated when they
are the same directory. Preview/review remain available. Unix requires temp file sync,
atomic replacement, and parent-directory sync. Windows requires temp
`FlushFileBuffers` plus write-through same-directory replacement; an unsupported or
failed probe yields `AtomicDurabilityUnavailable` without reservation or mutation.

All state consumers receive a `ProjectStateDir`/`StatePlacement` or
`ControlStateDir` explicitly; none accepts a source `Path` and reconstructs a
directory from `canonical_root`. The separate newtypes make source/state/control
path swaps a compile-time error. The ownership matrix is closed:

| Owner | Consumers |
|---|---|
| canonical source root | source/Git reads, relative paths, watcher root, repo-owned inputs (including `.symforge-knowledge.toml` and retained `.symforge/` configuration), and narrowly guarded policy/ignore/`.gitattributes` writes |
| `ProjectStateDir` | snapshot/temp/quarantine/reset/checkpoint, excluded team-artifact bytes and metadata, per-project replay and curation intent, coupling/frecency/STEL, analytics, API-key store, edit-safety TEE snapshots, and derived-cache cleanup |
| `ControlStateDir` | edit-safety trust store, sidecar port/PID/session descriptors and status readers, daemon discovery/control, hook adoption/hint state, operator profile, onboarding state, runtime-startup coordination, cross-project `index_folder` replay/locks, and process-global version-registry/update state |
| process memory | `LiveIndex`, watchers, session memberships, and explicitly non-durable fallbacks |

Each `ProjectInstance` calls the project-state placement resolver once; the returned
typed placement is injected into every reader, writer, verifier, and cleanup path.
Placement is re-resolved only when a new `ProjectInstance` is constructed; reindexing
the same live instance does not silently switch state owners.
The process calls the control-state resolver once and injects the same
`ControlStatePlacement` into both descriptor/status readers and writers. The legacy
untyped runtime-data-base oracle is split: project callers receive the resolved
`ProjectStateDir`, while process-global callers receive the resolved
`ControlStateDir`. Default analytics paths accept only the former. TEE receives the
bound canonical source and `ProjectStateDir` separately. Their CWD-relative
`.symforge` fallbacks are removed, and the mere existence of
`<source>/.symforge` never proves a repository root or writable state owner.
Process-global version/update state is control state, not state of whichever
`ProjectInstance` happens to be active.
Sidecar/daemon descriptors are stored beneath a `ProjectId` namespace with a daemon/
process instance discriminator; status and discovery readers receive the same
namespace key as writers. One global control directory therefore cannot collapse two
projects or daemons into last-writer-wins descriptor state.
Operator profile and onboarding state intentionally become process-global. Legacy
per-project files remain untouched and are not merged; when no global record exists,
onboarding runs once and writes only `ControlStateDir`.

If either resolved `ProjectStateDir` or `ControlStateDir` is nested beneath the bound
source, its canonical absolute subtree is a dynamic hard exclusion for scout,
watcher, reconciliation, and snapshot verification.

```rust
pub enum RepositoryFingerprint {
    Git {
        object_format: String,
        selected_ref_or_head: String,
        tip_object_id: String,
        reachable_history_fingerprint: String,
    },
    NonGit {
        catalog_identity_digest: String,
    },
}

pub struct SnapshotSourceIdentity {
    pub project_id: ProjectId,
    pub repository_id: RepositoryId,
    pub source_id: SourceId,
    pub source_version: SourceVersion,
    pub repository_fingerprint: RepositoryFingerprint,
    pub manifest_digest: String,
    pub indexed_content_digest: String,
}
```

`ProjectId` selects a state directory; it is never sufficient snapshot proof.
Before a snapshot seed can contribute to a promotable candidate, strong stable-read
verification must match `SnapshotSourceIdentity`: exact hashes for every resident
indexed file, terminal metadata for metadata-only entries, and the captured Git
identity/version for Git-derived state. A Git fingerprint is derived from verified
object-format, selected HEAD/ref target, tip object, and the reachable history used
by temporal state—not from the Git-directory path. If any required object/fingerprint
cannot be verified, temporal state is not restored and no Current generation is minted.
The non-Git proof is the complete canonical catalog identity. Thus replacing a
repository at the same path, even with an identical checked-out tree but different
history, cannot inherit old content or temporal evidence. Ordinary source drift
rejects the candidate as stale and rebuilds; identity collision/corruption is never
loaded or overwritten and is quarantined when persistent placement exists. A later
state write failure changes persistence health, not placement or source identity and,
by itself, does not revoke Current. Observer gap/absence, incomplete baseline, unknown
ordering, or overdue proof remains source truth and synchronously revokes strict
acquisition independently of durability.

Moving derived state never changes `canonical_root`. A protected override disables
init and repository curation regardless of state placement. A normal read-only
project may be indexed using user-local or memory-only state, while init/curation
refuse through `ProjectCapabilities`. Health renders placement class and capability
status; raw unsafe paths are not required for that report.

The state-directory newtypes have private fields and are constructed only by their
placement resolvers after canonical/containment/permission checks. They are absolute;
callers cannot manufacture one from a repository-relative path.

`GitignoreHygiene` is observational during automatic startup/scout/watcher/
reconciliation/ref ingestion. A successful explicit normal `index_folder` bind and
project-aware init may change only an existing normal-project root `.gitignore`,
after mutation-capability checks, through a tracked non-cloneable
`SourceMutationPermit` granted from the exact `Current` generation. Grant advances
the mutation epoch and publishes `Refreshing` before the first possible source-disk
side effect. The append is content-hash guarded and every component of `.gitignore`
and its temporary replacement is resolved beneath the permit's pinned physical-root
lease through a no-follow/reparse-safe final-parent handle. No file means no side
effect, but the permit must still terminate through a fresh no-op verification
candidate at the latest observer cut; it never restores the predecessor directly to
`Current`. A failed or indeterminate hygiene attempt likewise remains non-Current
until a fresh complete candidate promotes. Explicit-protected mode does not inspect
or mutate a protected-root `.symforge` location and exposes no init remediation.

## CatalogPath

```rust
pub struct CatalogPath {
    pub public_id: String,
    pub normalized_utf8: Option<String>,
}
```

`public_id` is a stable bounded digest of the lossless repository-relative OS/Git
path representation. `normalized_utf8` exists only when conversion is lossless and
the path safety guard approves external/persisted use. Non-UTF-8/unrepresentable or
detector-positive names are never lossy-converted; they remain cataloged by opaque
ID, become metadata-only, and require source re-scout to reconstruct the transient
runtime path. Searchable files therefore always have an exact safe UTF-8 path.

## IndexTargets

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum IndexTargets {
    Code,
    Knowledge,
    CodeAndKnowledge,
}
```

Examples:

| File | Indexed target | Catalog |
|---|---|---:|
| `src/lib.rs` | `Code` (doc-comment bridge is post-v1) | yes |
| `README.md` | `Knowledge` | yes |
| `openapi.yaml` | `CodeAndKnowledge` | yes |
| unknown small UTF-8 prose | `Knowledge` | yes |
| giant `model.gguf` | none (catalog-only) | yes |
| actual credential file | none (typed sensitive reason) | yes |

## FileStamp

```rust
pub struct PlatformFileId(Vec<u8>);

pub struct FileStamp {
    pub size: u64,
    pub created_hint: Option<SystemTime>,
    pub modified_hint: Option<SystemTime>,
    pub platform_id: Option<PlatformFileId>,
}
```

`PlatformFileId` is a private, bounded encoding of the host's open-handle file
identity (for example device/inode or volume/file ID). It is never a logical source
identity, public path, or serialized manifest field.

The stamp is a change/race hint, not content identity. Platform birth/creation and
modification times are optional and untrusted for authority decisions; copies,
checkout, and clock skew can rewrite them. They are excluded from the logical
manifest digest. Stable reads compare scout, open-handle, and post-read path state;
admitted bytes receive a content hash.

## ScoutDecision and owned reasons

```rust
pub enum ScoutDecision {
    Ingest {
        targets: IndexTargets,
    },
    MetadataOnly {
        reason: MetadataOnlyReason,
    },
    HardSkip {
        reason: HardSkipReason,
    },
    Unavailable {
        stage: AccessStage,
        kind: AccessErrorKind,
    },
}

pub enum MetadataOnlyReason {
    Lockfile,
    Binary,
    OversizedData,
    GeneratedOrVendor,
    SensitivePath {
        rule_id: String,
    },
    SensitiveContent {
        rule_ids: Vec<String>,
        finding_count: u32,
    },
    LfsPointer {
        declared_oid: Option<String>,
        declared_size: Option<u64>,
    },
    PlatformPathCollision,
    UnsupportedPathEncoding,
    PathMetadataTooLarge,
    UnsupportedTextEncoding,
}

pub enum HardSkipReason {
    ArtifactType,
    PerFileCeiling,
}

pub enum AccessStage {
    Metadata,
    Probe,
    FullRead,
}

pub enum AccessErrorKind {
    NotFound,
    PermissionDenied,
    InvalidData,
    ResourceExhausted,
    Other,
}
```

These are owned, versionable, serializable enums; snapshot state never embeds the
non-exhaustive `std::io::ErrorKind`. Scope escapes and unsupported special files
are outside the regular-file catalog and become bounded `ScoutIssue`s. Exact path
identity remains case-sensitive; `(case-folded safe UTF-8, exact UTF-8 bytes)`
supplies total order.
A collision is an issue only unless the host cannot address one entry safely, in
which case that entry becomes `MetadataOnly(PlatformPathCollision)`.
For opaque paths, `public_id` is the deterministic sort/tie key.

`ScoutDecision` is finalized only after the optional bounded probe. The ingest
variant is the only variant that carries an `IndexTargets` value, and the enum has
no empty variant, so catalog-only plus an ingest target is unrepresentable.
An admitted read whose declared size exceeds the total in-flight budget maps
deterministically to `HardSkip(PerFileCeiling)` before allocation; it is never
dropped or represented as an access failure.

## Committed and attempt dispositions

```rust
pub enum AttemptParseStatus {
    Parsed,
    PartialParse,
    Failed,
}

pub enum FileDisposition {
    Indexed {
        targets: IndexTargets,
    },
    MetadataOnly { reason: MetadataOnlyReason },
    HardSkip { reason: HardSkipReason },
}

pub enum AttemptFileDisposition {
    CandidateIndexed {
        targets: IndexTargets,
        parse_status: AttemptParseStatus,
    },
    Unreadable {
        stage: AccessStage,
        kind: AccessErrorKind,
    },
    UnstableDuringRead,
    AbortedCircuitBreaker,
}
```

Exactly one committed `FileDisposition` exists per entry in a promoted manifest.
Only `Indexed` carries one of the three non-empty target variants, and its required
parsers/extractors have completed for every advertised strict scope. `PartialParse`,
`Failed`, `Unreadable`, `UnstableDuringRead`, and `AbortedCircuitBreaker` exist only
in bounded `AttemptFileDisposition`; any such observation rejects the candidate.
Free-text diagnostics remain bounded attempt health outside canonical manifest
identity.

## ScoutedEntry

```rust
pub struct ScoutedEntry {
    pub path: CatalogPath,
    pub absolute_path: Option<PathBuf>,
    pub stamp: FileStamp,
    pub language: Option<LanguageId>,
    pub classification: FileClassification,
    pub decision: ScoutDecision,
}
```

`absolute_path` exists only for checked-out filesystem sources and is never part
of the serialized logical manifest. Source identity belongs to the containing
single-source manifest, so it is not duplicated per entry.

## CatalogEntry

```rust
pub struct CatalogEntry {
    pub path: CatalogPath,
    pub size: u64,
    pub language: Option<LanguageId>,
    pub classification: FileClassification,
    pub disposition: FileDisposition,
    pub content_hash: Option<String>,
}
```

`content_hash` is `None` for files whose bytes were intentionally not read.
Targets are derived only from `FileDisposition::Indexed`.
Detector-positive bytes are discarded before publication, receive
`MetadataOnly(SensitiveContent)`, and also retain no content hash.

## Completeness, attempt accounting, and scout issues

```rust
pub struct CompletenessCertificate {
    pub certificate_id: CompletenessCertificateId,
    pub manifest_digest: ManifestDigest,
    pub strict_scope_contract: StrictScopeContractV1,
    pub required_artifact_set_id: RequiredArtifactSetId,
    pub artifact_digests: BTreeMap<ArtifactKind, ArtifactDigest>,
    pub observer_cut: ObservationCut,
    pub discovery_proof: ScopeDiscoveryProof,
    pub verification_obligations_digest: VerificationObligationsDigest,
    pub optional_delta_proof_digest: Option<DeltaProofDigest>,
}

/// Sealed enumeration of everything one complete whole-declared-scope verification
/// pass must check. Sealed at promotion and never widened in place: a scope change
/// produces a new receipt, so a pass can never silently verify less than it claims.
pub struct VerificationScopeReceipt {
    pub receipt_id: VerificationScopeReceiptId,
    pub project_slot: ProjectSlotInstanceId,
    pub source_slot: SourceSlotInstanceId,
    pub generation_digest: GenerationDigest,
    pub observer_cut: ObservationCut,
    pub policy_version: PolicyVersion,
    pub strict_scope_contract: StrictScopeContractV1,
    pub catalog_entries: BTreeSet<CatalogPath>,
    pub terminal_dispositions: BTreeMap<CatalogPath, FileDisposition>,
    pub admitted_byte_ranges: BTreeMap<CatalogPath, AdmittedByteRange>,
    pub discovery_obligations: BTreeSet<ScopeDiscoveryObligationId>,
    pub required_artifact_set: RequiredArtifactSetId,
    pub required_artifacts: BTreeSet<ArtifactKind>,
    pub required_certificates: BTreeSet<CompletenessCertificateId>,
}

/// Cost of the pass the receipt above describes. `bound_seconds` is
/// `ceil(verification_bytes / RESERVED_VERIFICATION_BYTES_PER_SECOND)
///  + ceil(verification_entries / RESERVED_VERIFICATION_ENTRIES_PER_SECOND)`.
/// The default ceilings cap it at 512 + 200 = 712 seconds, so
/// `DEFAULT_MAX_COMPLETE_VERIFICATION_PASS` is a ceiling the defaults never reach.
pub struct VerificationWorkBound {
    pub scope_receipt: VerificationScopeReceiptId,
    pub verification_bytes: u64,
    pub verification_entries: u64,
    pub bound_seconds: u32,
}

/// Capacity actually reserved for the pass. Promotion to `Current` requires one;
/// losing the reservation makes the source non-current rather than extending the
/// deadline, so an unaffordable pass can never masquerade as a completed one.
pub struct VerificationFeasibilityReceipt {
    pub work_bound: VerificationWorkBound,
    pub reserved_bytes_per_second: u64,
    pub reserved_entries_per_second: u64,
    pub successor_start_deadline: MonotonicInstant,
}

pub const DEFAULT_MAX_VERIFICATION_BYTES: u64 = 17_179_869_184;
pub const DEFAULT_MAX_VERIFICATION_ENTRIES: u64 = 200_000;
pub const DEFAULT_MAX_COMPLETE_VERIFICATION_PASS: Duration = Duration::from_secs(720);
pub const RESERVED_VERIFICATION_BYTES_PER_SECOND: u64 = 33_554_432;
pub const RESERVED_VERIFICATION_ENTRIES_PER_SECOND: u64 = 1_000;
pub const MAX_SUCCESSOR_VERIFICATION_START: Duration = Duration::from_secs(180);
pub const MAX_CURRENT_UNVERIFIED_AGE: Duration = Duration::from_secs(900);

pub enum DeltaCause {
    Added,
    Modified,
    Deleted,
    Renamed,
    DispositionChanged,
    PolicyChanged,
    DerivedDependencyChanged,
}

pub struct DeltaCauseSet(NonEmptySet<DeltaCause>);

pub struct DeltaProof {
    pub base_generation_authority_id: GenerationAuthorityId,
    pub base_scope_certificate_id: CompletenessCertificateId,
    pub required_artifact_set_id: RequiredArtifactSetId,
    pub artifact_dependency_contract_digest: ArtifactDependencyContractDigest,
    pub scope_and_policy_versions: ScopeAndPolicyVersions,
    pub causes: DeltaCauseSet,
    pub complete_discovery_diff_digest: DiscoveryDiffDigest,
    pub impacted_closure_digest: ImpactedClosureDigest,
    pub reused_artifact_semantic_digests:
        BTreeMap<ArtifactKind, ArtifactSemanticDigest>,
}

pub enum AttemptFailureReason {
    ObservationFailed,
    WatcherUnavailable,
    ReconciliationPending,
    SnapshotVerificationFailed,
    DerivedPublicationPending,
    CatalogEntryCapacityExceeded,
    CatalogMetadataCapacityExceeded,
}

pub struct AttemptAccounting {
    pub attempt_id: AttemptId,
    pub discovered: u64,
    pub processed: u64,
    pub dispositions: BTreeMap<AttemptDispositionKind, u64>,
    pub resource_usage: AttemptResourceUsage,
    pub reasons: Vec<AttemptFailureReason>,
    pub issues: Vec<AttemptIssue>,
    pub derived_limit_breaches: BTreeMap<ArtifactKind, Vec<LimitBreach>>,
}

pub enum HistoryLimit {
    Shallow,
    WindowLimited,
    RenameFollowLimited,
    DivergentHistory,
    WorkingTreeOnly,
    Unavailable,
}

pub struct HistoryCoverage {
    pub complete_to_root: bool,
    pub limitations: Vec<HistoryLimit>,
}

pub enum ManifestIssueKind {
    ScopeEscape,
    UnsupportedSpecialFile,
    PathIdentityCollision,
}

pub enum AttemptIssueKind {
    DirectoryEntryUnreadable { kind: AccessErrorKind },
    TraversalCircuitBreaker,
}

pub struct ManifestIssue {
    pub path_id: Option<String>,
    pub safe_path: Option<String>,
    pub kind: ManifestIssueKind,
    pub safe_message: String,
}

pub struct AttemptIssue {
    pub path_id: Option<String>,
    pub safe_path: Option<String>,
    pub kind: AttemptIssueKind,
    pub safe_message: String,
}
```

The generation-builder Adapter compiles the advertised protocol/catalog version into
one private sealed `RequiredArtifactSet`. Only its `RequiredArtifactSetId` crosses the
lifecycle seam; callers cannot choose an artifact subset or construct completeness.
The versioned artifact dependency contract is checked in with that compiler, and a
delta proof is sound only for the exact contract named by
`artifact_dependency_contract_digest`.

A full candidate seals `optional_delta_proof_digest = None`. A delta candidate seals
`Some(digest)` for the exact closed `DeltaProof` above, and promotion recomputes and
matches that digest. Path hints select work but never mint reuse authority. Any unknown
or repository-global cause, dependency-contract or scope/policy version mismatch,
incomplete discovery diff, or unclosed impacted dependency forces a full candidate;
if that complete fallback cannot promote, the delta candidate is discarded and the
source remains non-Current.

Diagnostics are bounded and may not contain file contents or secret values.
`CatalogEntryCapacityExceeded` and `CatalogMetadataCapacityExceeded` are distinct
attempt-failure reasons even when cold start has no publishable generation. Capacity
refusal has no `ManifestIssueKind`: it remains operational attempt health outside the
manifest. `AttemptAccounting` owns bounded `AttemptIssue`s and deliberately cannot
carry a manifest digest, completeness certificate, or query authority. A promoted
manifest may contain only `ManifestIssue`; a directory-entry failure or traversal-
breaker trip therefore cannot inhabit a nominally complete manifest.

## ManifestResourceUsage

```rust
pub struct ManifestResourceUsage {
    pub catalog_entries: u64,
    pub catalog_metadata_bytes: u64,
    pub admitted_content_bytes: u64,
}
```

`catalog_metadata_bytes` is the exact canonical encoded size of bounded public path
IDs/safe spellings, descriptors, dispositions, and issues—not file payload size or
allocator RSS. `admitted_content_bytes` is the sum of resident accepted source
bytes. In-flight/peak/derived-state usage is operational health state, not logical
manifest identity. If a complete candidate cannot fit its entry or metadata budget,
the partial observation is not a `RepositoryManifest`; the candidate is discarded,
the runtime stays non-current, and any retained verified generation remains internal.

## RepositoryManifest

```rust
pub struct RepositoryManifest {
    pub schema_version: u32,
    pub policy_version: u32,
    pub secret_policy_version: u32,
    pub source: SourceIdentity,
    pub source_version: SourceVersion,
    pub entries: Vec<CatalogEntry>,
    pub issues: Vec<ManifestIssue>,
    pub usage: ManifestResourceUsage,
    pub digest: String,
}
```

Canonical digest inputs:

- schema/policy version;
- secret-safety policy version;
- opaque path ID and optional exact safe normalized UTF-8 spelling;
- source identity;
- size;
- language/classification/targets;
- terminal disposition;
- optional content/object hash.

`usage` is recomputed and equality-checked on load. It is redundant canonical
accounting, so only its versioned calculation rule—not duplicated numeric fields—is
hashed. A mismatch corrupts/quarantines the candidate rather than being repaired
silently.

The digest is computed once for each prepared candidate and retained in the promoted
immutable generation; queries never recompute it. A `RepositoryManifest` is complete
by construction. Equal digest is a no-op only when the exact binding, observer cut,
scope/policy versions, and completeness proof also match. `Unreadable`,
`UnstableDuringRead`, or another attempt-only disposition prevents manifest
construction and schedules lifecycle-owned re-observation; it cannot be represented
as degraded committed coverage.

Excluded from digest:

- scan time and duration;
- mtime/platform ID;
- watcher receipt and retry counters;
- in-memory addresses/order;
- absolute checkout path.
- captured source version, which is carried and verified separately so branch labels
  or clean/dirty observations never substitute for exact manifest/content identity.

## KnowledgeUnit

```rust
pub enum KnowledgeUnitKind {
    MarkdownSection,
    TextLine,
}

pub struct KnowledgeUnit {
    pub source: SourceIdentity,
    pub path: String,
    pub content_hash: String,
    pub kind: KnowledgeUnitKind,
    pub heading_path: Vec<String>,
    pub byte_range: Range<u32>,
    pub line_range: Range<u32>,
    pub parent: Option<u32>,
}
```

V1 MUST project existing Markdown `SymbolKind::Section` records into this contract
instead of storing a second copy. Generic text uses exact line matches without
persisting paragraph units. A persisted unit store requires a failing retrieval
fixture and a later design decision.

## Repository mental-model bridge

```rust
pub enum KnowledgeRole {
    Architecture,
    OwnershipGovernance,
    DecisionInvariant,
    SchemaContract,
    Operations,
    TestingSecurity,
    PlanHandoff,
    Other,
}

pub struct KnowledgeAnchor {
    pub id: KnowledgeAnchorId,
    pub source: SourceIdentity,
    pub generation_authority: Arc<GenerationAuthority>,
    pub path: String,
    pub content_hash: String,
    pub byte_range: Range<u32>,
    pub line_range: Range<u32>,
}

pub struct KnowledgeAnchorId {
    pub path: String,
    pub content_hash: String,
    pub start_byte: u32,
}

pub enum RoleEvidence {
    DeclaredSpan(KnowledgeAnchor),
    HeadingRule { rule_id: String, anchor: KnowledgeAnchor },
    PathConvention { rule_id: String, anchor: KnowledgeAnchor },
}

pub struct KnowledgeCard {
    pub anchor: KnowledgeAnchor,
    pub roles: Vec<(KnowledgeRole, RoleEvidence)>,
}

pub struct CodeAnchor {
    pub source: SourceIdentity,
    pub generation_authority: Arc<GenerationAuthority>,
    pub id: CodeAnchorId,
    pub content_hash: String,
    pub line_range: Range<u32>,
}

pub enum CodeAnchorId {
    File { path: String },
    Symbol { symbol: SymbolId, start_line: u32 },
}

pub enum BridgeEvidenceKind {
    RepositoryLink,
    ExactPathToken,
    ExactCodeSpanSymbol,
    DeclaredOwnershipSelector,
    SupportedStructuredValue { rule_id: String },
}

pub enum BridgeResolution {
    ResolvedExact(CodeAnchor),
    ResolvedDeclaredSet {
        selector_anchor: KnowledgeAnchor,
        matched_count: u64,
    },
    Ambiguous {
        candidate_count: u32,
        bounded_samples: Vec<CodeAnchor>,
    },
    Missing,
}

pub struct KnowledgeCodeLinkId(String);

pub struct KnowledgeCodeLink {
    pub id: KnowledgeCodeLinkId,
    pub evidence: KnowledgeAnchor,
    pub evidence_kind: BridgeEvidenceKind,
    pub resolution: BridgeResolution,
}

pub enum KnowledgeLinkResolution {
    ResolvedExact(KnowledgeAnchor),
    Ambiguous {
        candidate_count: u32,
        bounded_samples: Vec<KnowledgeAnchor>,
    },
    Missing,
}

pub struct KnowledgeKnowledgeLink {
    pub evidence: KnowledgeAnchor,
    pub resolution: KnowledgeLinkResolution,
}

pub enum ArtifactCompleteness {
    Complete,
}

pub enum OutputCoverage {
    Complete,
    Truncated { breaches: Vec<LimitBreach> },
}

pub struct LimitBreach {
    pub kind: DerivedLimitKind,
    pub omitted: u64,
}

pub enum DerivedLimitKind {
    Cards,
    BridgeLinks,
    AuthorityRecords,
    Findings,
    MetadataBytes,
    Output,
}

pub struct KnowledgeBridge {
    pub cards: Vec<KnowledgeCard>,
    pub forward: Vec<KnowledgeCodeLink>,
    pub reverse_exact: BTreeMap<CodeAnchorId, Vec<u32>>,
    pub ownership_selectors: Vec<u32>,
    pub knowledge_links: Vec<KnowledgeKnowledgeLink>,
    pub reverse_knowledge: BTreeMap<KnowledgeAnchorId, Vec<u32>>,
    pub coverage: ArtifactCompleteness,
}
```

Cards and links reference safe resident bytes and existing symbol/section records;
they do not copy bodies. Candidate extraction is closed-world in v1: internal link
destinations, exact repository-relative path tokens, code-spanned exact symbol
names, supported structured values, and declared ownership selectors. Bare lexical
similarity is not bridge evidence. Exact symbols must resolve uniquely inside the
same source/content generation; otherwise the result is `Ambiguous` or `Missing`.
External/out-of-scope links are not bridge candidates.
`KnowledgeCodeLinkId` is a stable digest of the source-local evidence anchor,
evidence kind, and canonical extracted-candidate ordinal/selector identity. It
excludes the current resolution, so the same candidate keeps its ID when a target
appears, disappears, or becomes ambiguous. It never hashes copied prose or secret-
positive bytes and is shared by compact search previews and full review dossiers.

Declared ownership selectors stay compact instead of enumerating millions of
edges. A code-context query evaluates the captured selector set against its exact
anchor and returns an exact backlink. Forward/reverse arrays store indices into the
same immutable bridge. Card, edge, selector, and bounded-sample ceilings have an
independent derived-state budget. A breach in an advertised strict artifact blocks
promotion; it never constructs an incomplete artifact. `OutputCoverage::Truncated`
is produced only by bounded response rendering after strict lease acquisition and
does not change generation completeness.
`ArtifactCompleteness::Complete` is constructed only when every required derived
budget completed; otherwise all simultaneous candidate breaches are retained in
attempt accounting and the candidate is discarded in canonical kind order. History
coverage likewise retains every applicable shallow/window/rename/divergence limit.

Internal links that resolve to knowledge rather than code populate the compact
`knowledge_links`/`reverse_knowledge` arrays. They do not create semantic
relationships, but they provide exact inbound-link evidence needed to avoid unsafe
archive/deletion proposals. Ambiguous/missing outcomes and coverage are preserved.

## Knowledge authority and hygiene

```rust
pub enum KnowledgeLifecycle {
    Active,
    Proposed,
    Accepted,
    Implemented,
    Deferred,
    Rejected,
    Withdrawn,
    Deprecated,
    Superseded,
    Archived,
    Historical,
    Unknown,
}

pub enum LifecycleEvidence {
    PolicyEntry { entry_id: String },
    DeclaredSpan(KnowledgeAnchor),
    ArchivePathRule { rule_id: String },
    None,
}

pub enum AuthorityDomain {
    CurrentImplementation,
    NormativeIntent,
    Decision,
    Operations,
    Governance,
    HistoricalRecord,
    Unknown,
}

pub enum AuthorityDomainEvidence {
    PolicyEntry { entry_id: String },
    DeclaredSpan(KnowledgeAnchor),
    RoleRule { rule_id: String, anchor: KnowledgeAnchor },
    Unknown,
}

pub enum CodeEvidenceDisplay {
    DeterministicConflict,
    BrokenAnchor,
    ImplementationGap,
    SuspectedConflict,
    RelevantCodeChangedSinceDocument,
    ReviewDue,
    Partial,
    Unresolved,
    ConsistentForCheckedClaims,
    NotApplicable,
}

pub struct CodeEvidenceSummary {
    pub display: CodeEvidenceDisplay,
    pub consistent_rule_ids: Vec<String>,
    pub broken_link_indices: Vec<u32>,
    pub deterministic_conflict_ids: Vec<String>,
    pub suspected_conflict_ids: Vec<String>,
    pub implementation_gap_ids: Vec<String>,
    pub relevant_code_change_count: u32,
    pub review_signal_ids: Vec<String>,
    pub unresolved_semantics: bool,
    pub not_applicable: bool,
    pub coverage: ArtifactCompleteness,
}

pub enum KnowledgeVoice {
    Current,
    Intent,
    NeedsReview,
    Unknown,
    HistoryOnly,
    Suppressed,
}

pub enum TimeProvenance {
    FilesystemBirth,
    FilesystemModified,
    GitFirstSeen,
    GitLastTouch,
    WorkingTreeObservation,
}

pub struct TimeEvidence {
    pub unix_seconds: Option<i64>,
    pub provenance: TimeProvenance,
    pub coverage: HistoryCoverage,
}

pub struct DocumentTimeline {
    pub filesystem_created: Option<TimeEvidence>,
    pub filesystem_modified: Option<TimeEvidence>,
    pub git_first_seen: Option<TimeEvidence>,
    pub git_last_touch: Option<TimeEvidence>,
    pub working_tree_changed: bool,
    pub relevant_code_changes: Vec<CodeChangeEvidence>,
    pub coverage: HistoryCoverage,
}

pub struct CodeChangeEvidence {
    pub anchor: CodeAnchor,
    pub commit_id: Option<String>,
    pub unix_seconds: Option<i64>,
    pub topologically_after_document: Option<bool>,
    pub rule_id: String,
}

pub enum EvidenceConfidence {
    Deterministic,
    StrongCandidate,
    ReviewSignal,
    Unresolved,
}

pub enum RemediationPrecondition {
    ProtectedRole,
    UniqueContentUnknown,
    InboundLiveLinks { count: u64 },
    MissingSuccessor,
    SuccessorCoverageIncomplete,
    RequiredExternalEvidenceUnavailable,
    WorkingTreeDirtyOrUntracked,
    UnsupportedPath,
    RequiresUserJudgment,
}

pub enum RemediationAction {
    Keep,
    Update,
    RelabelIntent,
    MergeInto { target: KnowledgeAnchor },
    MarkSuperseded { successor: KnowledgeAnchor },
    Archive,
    DeletionCandidate { retained: KnowledgeAnchor },
    NeedsReview,
}

pub struct RemediationProposal {
    pub action: RemediationAction,
    pub confidence: EvidenceConfidence,
    pub evidence_ids: Vec<String>,
    pub unmet_preconditions: Vec<RemediationPrecondition>,
}

pub struct KnowledgeAuthorityRecord {
    pub unit: KnowledgeAnchor,
    pub lifecycle: KnowledgeLifecycle,
    pub lifecycle_evidence: LifecycleEvidence,
    pub authority_domain: AuthorityDomain,
    pub authority_domain_evidence: AuthorityDomainEvidence,
    pub code_evidence: CodeEvidenceSummary,
    pub voice: KnowledgeVoice,
    pub successor: Option<KnowledgeAnchor>,
    pub timeline: DocumentTimeline,
    pub proposal: RemediationProposal,
}
```

`CodeEvidenceSummary.display` is a deterministic compact precedence view; every
underlying fact set remains available and bounded. A review timestamp, mtime, commit
date, or generation number can add only a review/relevant-change signal.
The normative display precedence is `DeterministicConflict` > `BrokenAnchor` >
`ImplementationGap` > `SuspectedConflict` >
`RelevantCodeChangedSinceDocument` > `ReviewDue` > `Partial` > `Unresolved` >
`ConsistentForCheckedClaims` > `NotApplicable`.

The axes are intentionally independent. `Accepted` does not mean implemented;
an old but still-current unit does not become archived. `KnowledgeVoice` is a
deterministic projection of the other axes and policy, never free input. A
`CurrentImplementation` unit with an exact
conflict is suppressed only for current-authority answers. A `NormativeIntent`,
`Decision`, or `Governance` unit with conflicting code remains voiced in its own
domain and reports an implementation divergence.

`CodeEvidenceSummary` is an aggregate because one unit may contain checked-
consistent fields, a broken link, an unresolved sentence, and a temporal change
signal simultaneously. `display` is a compact deterministic projection with fixed
precedence; it never erases the underlying arrays, unresolved flag, or coverage.

Filesystem timestamps are hints. Git evidence records whether history is complete,
shallow, bounded-window, renamed-beyond-following, divergent, or unavailable.
Topological “code anchor changed after document commit” is stronger than timestamp
ordering but yields `RelevantCodeChangedSinceDocument`, not deterministic conflict.

## Knowledge policy ledger

```rust
pub struct KnowledgePolicy {
    pub version: u32,
    pub entries: Vec<KnowledgePolicyEntry>,
}

pub struct KnowledgePolicyTarget {
    pub path: String,
    pub content_hash: String,
    pub unit_byte_range: Option<Range<u32>>,
    pub unit_hash: Option<String>,
}

pub struct KnowledgePolicyEntry {
    pub entry_id: String,
    pub target: KnowledgePolicyTarget,
    pub lifecycle: KnowledgeLifecycle,
    pub authority_domain: Option<AuthorityDomain>,
    pub superseded_by: Option<KnowledgePolicyTarget>,
    pub evidence: Vec<PolicyEvidenceRef>,
    pub justification_code: String,
}

pub struct PolicyEvidenceRef {
    pub rule_id: String,
    pub knowledge: Option<KnowledgePolicyTarget>,
    pub code: Option<CodeAnchorId>,
}
```

The canonical repo-owned file is `.symforge-knowledge.toml`. It is normal source
input, not `.symforge/` runtime state and not a second search store. Entries use
exact safe paths/hashes and canonical ordering. A content-hash mismatch cannot
suppress the new bytes; it becomes a stale-policy finding. Native frontmatter,
MADR/RFC status, archive paths, and supersession links are read-only evidence. A
conflict with a hash-valid ledger entry is surfaced explicitly; the ledger controls
retrieval voice until the repo changes or a guarded curation updates it.

Policy targets are unit-level by default and may explicitly target the whole file
when `unit_byte_range=None`. Unit ranges are zero-based, half-open byte offsets and
are meaningful only under the bound exact whole-file content hash; `unit_hash` is an
additional integrity check, not a relocation heuristic. Lifecycle is always
accompanied by `LifecycleEvidence`; `Implemented`
is a declared lifecycle label and is never derived from code consistency. A
supersession target is the same exact target type—successor hashes are mandatory,
so stale or ambiguous successor coverage cannot authorize suppression/deletion.

```rust
pub enum KnowledgeAuthorityScope {
    Default,
    Current,
    Intent,
    History,
    All,
}

pub struct DerivedResourceUsage {
    pub knowledge_cards: u64,
    pub bridge_links: u64,
    pub authority_records: u64,
    pub derived_metadata_bytes: u64,
}

pub struct KnowledgeAuthorityView {
    pub records: Vec<KnowledgeAuthorityRecord>,
    pub finding_index: BTreeMap<String, u32>,
    pub policy_digest: String,
    pub skipped_suppression_ids: Vec<String>,
    pub coverage: ArtifactCompleteness,
    pub usage: DerivedResourceUsage,
}
```

The authority view is derived from the captured manifest/live/bridge/Git evidence
and hash-valid policy. It is immutable and source-local. `finding_index` stores safe
opaque rule/finding IDs and record indices, never evidence snippets or copied file
content. `skipped_suppression_ids` is canonically ordered and non-empty exactly when a
reserved suppression/proven-divergence record could not be represented; the affected
candidate does not construct a `KnowledgeAuthorityView`. All simultaneous breaches
enter `AttemptAccounting.derived_limit_breaches` and the whole candidate is discarded.
Work omitted from the advertised protocol is absent from `RequiredArtifactSet` and
produces no public artifact. Only a complete promoted authority view can be projected;
bounded response omission after lease acquisition is represented separately by
`OutputCoverage`. Rebuilding from source/snapshot rule-version mismatch is
deterministic.
Finding IDs are stable digests of source-local unit-anchor identity, rule ID, and
evidence kind. Provenance IDs use the same unit anchor plus provenance kind and its
safe rule/policy/evidence identity. Neither includes record index, array order,
publication identity, or resolution state; `finding_index` is only the captured
view's lookup from those stable IDs to current record indices.

## SecretSafetyPolicy

```rust
pub enum RuleStrength {
    ExactContext,
    ContextAndEntropy,
}

pub enum DetectorFailure {
    PolicyCompilation,
    ResourceLimit,
    Internal,
}

pub struct SecretRule {
    pub id: &'static str,
    pub keywords: &'static [&'static [u8]],
    pub pattern: regex::bytes::Regex,
    pub secret_capture: usize,
    pub minimum_entropy: Option<f32>,
    pub strength: RuleStrength,
}

pub enum SecretScan {
    Clean,
    Sensitive { findings: Vec<SecretFinding> },
    Indeterminate { reason: DetectorFailure },
}

pub struct SecretFinding {
    pub rule_id: &'static str,
    pub byte_range: Range<usize>,
}
```

Rules are compiled once, pure/local, deterministic for `(policy_version, path,
bytes)`, and bounded by content admission. Entropy is never a standalone rule; it
may only strengthen a context-anchored captured value. Detector failure is
fail-closed. Only safe rule IDs and finding counts survive; matched bytes, ranges,
lengths, and hashes never enter a manifest, snapshot, diagnostic, or response.

## V11 runtime, generation, and query authority

```rust
pub struct CodeSignalsSnapshot {
    pub state: GitTemporalState,
    pub temporal: Arc<GitTemporalIndex>,
    pub computed_for_generation: Arc<GenerationAuthority>,
    pub coverage: HistoryCoverage,
}

pub struct VerifiedGeneration {
    pub generation_authority: Arc<GenerationAuthority>,
    pub scope_certificate: Arc<CompletenessCertificate>,
    pub live: Arc<LiveIndex>,
    pub manifest: Arc<RepositoryManifest>,
    pub outline: Arc<RepoOutlineView>,
    pub code_signals: Arc<CodeSignalsSnapshot>,
    pub bridge: Arc<KnowledgeBridge>,
    pub knowledge_authority: Arc<KnowledgeAuthorityView>,
    pub proof_ledger: Arc<VerificationProofLedger>,
    pub charged_roots: ChargedGenerationRoots,
}
```

The registry and lifecycle implementation use these private closed publications
exactly; the names below are not public constructors:

```text
ObserverPhase =
    Absent { initial_activation_package }
  | Active { token: ObserverToken, next_handoff_publication_package }
  | Draining { token: ObserverToken, in_progress_handoff_package }
  | ObserverFree { handoff_id, retry_trigger, in_progress_handoff_package }

SourceRuntimeState =
  Loading {
      binding: BindingAuthority,
      observer_phase: ObserverPhase,
      mutation_epoch,
      source_revocation_publication_package,
      work: NonCurrentWork,
  }
  | Current {
      generation: Arc<VerifiedGeneration>,
      safety_transition_package,
      next_handoff_publication_package,
      source_revocation_publication_package,
  }
  | Refreshing {
      binding: BindingAuthority,
      observer_phase: ObserverPhase,
      mutation_epoch,
      active_permits,
      retained: Arc<VerifiedGeneration>,
      source_revocation_publication_package,
      work: NonCurrentWork,
  }
  | Blocked {
      binding: BindingAuthority,
      observer_phase: ObserverPhase,
      mutation_epoch,
      retained: Option<Arc<VerifiedGeneration>>,
      source_revocation_publication_package,
      cause,
      operator_action,
  }
  | Stopping {
      revocation,
      retained: Option<Arc<VerifiedGeneration>>,
      committed_source_revocation_residency,
  }

InvalidationAccumulatorState {
    token: ObserverToken,
    coverage: BaselinePending | Complete | Gapped,
    invalidation_seq,
    acknowledged_seq,
    scope_dirty: Option<{
        latest_seq, causes: NonEmptySet<ScopeDirtyCause>,
        required_scope_policy_versions
    }>,
}

NonCurrentWork =
    Dirty { since_seq }
  | WaitingForCapacity { request, blocker, retry_trigger }
  | Building { candidate_authority, start_cut, attempt }
  | Verifying { candidate_id, through_cut }
  | RetryWait { cause, attempt, retry_at }

ProjectSlot {
    slot_instance_id,
    sources: Map<SourceId, SourceSlot>,
    runtime: ArcSwap<ProjectRuntimePublication>,
}

ProjectRuntimePublication {
    never_reused_publication_identity,
    state: ProjectRuntimeState,
}

ProjectRuntimeState =
  Live {
      sources: PersistentMap<SourceId, Arc<SourceStatePublication>>,
      project_membership: Arc<ProjectMembershipPublication>,
      runtime_epoch,
      revocation_publication_package,
  }
  | Stopping {
      revocation,
      retained_sources: PersistentMap<SourceId, Arc<SourceStatePublication>>,
      runtime_epoch,
  }
```

`SourceMutationPermit` is a private non-cloneable deep Interface owning the exact
binding's live physical-root lease:

```text
start_side_effect() -> Result<InFlightMutation, SourceRefusal>
commit(WriteReceipt) -> RefreshTicket | Drained
rollback(NoSideEffectProof) -> RefreshTicket | Drained
```

Its closed publication-owned state is `Granted -> InFlight -> Terminal` or
`Granted -> RevokedSealPending -> RevokedDeallocationOnly`. Every terminal path that
can return the same live binding to `Current`, including a valid
`NoSideEffectProof`, does so only through fresh candidate publication at the latest
observer cut and monotonic mutation epoch. A predecessor already in `Stopping` drains
or seals the permit and never enqueues a candidate that cannot publish; any successor
binding builds its own fresh complete generation.

`ArtifactCompleteness` has only `Complete`. Any required-artifact truncation is
attempt-only, is recorded in `AttemptAccounting.derived_limit_breaches`, and discards
the entire candidate; it cannot appear inside a promoted `VerifiedGeneration` or its
certificate. Work excluded from `RequiredArtifactSet` is not built or returned as a
public artifact. All visible truncation is post-lease response rendering represented
only by `OutputCoverage::Truncated`.

`VerifiedGeneration` is immutable and complete for exactly its advertised
`StrictScopeContractV1`. Its private constructor requires the canonical manifest,
all required artifact digests, complete observer cut, scope-discovery proof, racy-
clean obligations, policy versions, and charged allocation roots. Temporal, bridge,
authority, backlink, outline, and search data required by that scope are therefore
promotion inputs, never later mutations of Current. Every internal derived snapshot,
knowledge anchor, and code anchor retains that same exact
`Arc<GenerationAuthority>`; the constructor rejects a merely equal diagnostic counter
or independently reconstructed authority.

The registry-owned `ArcSwap<ProjectRuntimePublication>` is the sole publication root.
Lifecycle code cannot name, clone, or store that root; it submits a sealed transition
intent carrying an exact retained `SourcePublicationToken`. Under the writer, the
registry validates the owning project is `Live`, exact-matches the token, rebases the
one-source delta onto the latest whole root, preserves membership and all unrelated
siblings, and performs one non-fallible store. Derived workers retain the exact
`GenerationAuthority` they read and their owning `SourcePublicationToken`; completion
can publish only after both identities still match. Numeric generation counters,
epochs, versions, timestamps, and progress values are diagnostics or ordering aids
only and can never authorize a read, reuse, mutation, or publication.

`SourceRuntimeState` makes strict queryability closed: only `Current` contains a
query-granting generation. `Loading` retains none; `Refreshing` retains exactly one;
`Blocked` and `Stopping` may retain zero or one for recovery/accounting. No public
Interface can acquire any retained generation.
`Current` has no separately published binding, mutation, coverage, sequence, or
observer-evidence side field: its immutable generation owns accepted binding authority,
observer token/cut, mutation epoch, and completeness. Its
`next_handoff_publication_package` is operational capacity, not independently sampled
source identity.

The `InvalidationAccumulatorState` is the sole live owner of observer coverage and
sequence. Runtime variants carry only the stable `ObserverToken` when a token exists;
they never copy mutable `Complete`/`Gapped` or sequence fields. Health/progress calls
the lifecycle-owned `capture_source_view()`: it loads source publication R1, acquires
R1's token accumulator when present, reloads the project root, exact-validates R1 plus
the token, and retries on drift. `Absent`, `ObserverFree`, and `Stopping` return their
immutable phase evidence without inventing a token. Callers cannot sample runtime and
accumulator independently, and the resulting bounded view exposes no cloneable runtime
or generation root.

Every generation-backed orientation, search, review, context, resource, prompt,
sidecar, checkpoint, CCR/cache, and embed call acquires through the registry. A source
query lease pins only its exact Current generation and publication identities. A
project/multi-project lease additionally retains a canonical
`SourceSelectionReceipt` and one generation per selected source. Later membership,
proof refresh, or rebind does not invalidate a fully captured lease, but cache reuse
starts from a new acquisition; no formatter performs a trailing live-current check or
reopens generation bytes from disk. Explicit disk, complete worktree-scope, and Git
observation Adapters construct their own typed receipts without acquiring a generation;
health uses only the composite runtime view. Neither lane can grant generation
queryability or establish generation completeness/absence.

A failed generation- or candidate-critical observation publishes a closed non-current
state before its ingress is acknowledged. A failed pure disk/worktree-scope/Git
observation instead returns typed refusal and preserves lifecycle state unless it
independently proves lifecycle invalidation through the observer seam. The retained
generation and its capacity charges are unchanged; snapshots remain untrusted
candidate seeds. Git temporal/hotspot work that contributes generation authority
follows the candidate/certificate rule. There is no degraded wrapper, derived-only
Current publication, or mutable health side channel that can grant query authority.

Immutable Git blob bytes may be shared through one content-addressed cache keyed by
object ID, but manifests/source mappings remain authoritative. Parse/extraction
sharing additionally keys object ID with scout classification, extraction route, and
extractor version. Secret-scan sharing additionally keys the canonical path-policy
inputs and secret-policy version. A source whose path/classification differs
re-derives those results. Roles, voice, bridge links, authority, temporal evidence,
and policy always re-derive per source. The cache never owns source identity and
cache eviction cannot change catalog truth.

## SearchKnowledgeInput

```rust
pub enum KnowledgeSourceScope {
    Current,
    Worktrees,
    LocalRefs,
    All,
}

pub struct SearchKnowledgeInput {
    pub query: String,
    pub path_prefix: Option<String>,
    pub source_scope: Option<KnowledgeSourceScope>,
    pub authority_scope: Option<KnowledgeAuthorityScope>,
    pub project: Option<String>,
    pub projects: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub max_tokens: Option<u32>,
}
```

Avoid exposing regex/structural/code-only knobs on the knowledge tool in v1. An
agent needing literal/regex mechanics can still use the existing text tool with
an explicit scope added later if evidence demands it. `authority_scope` defaults
to `Default`, which includes `Current`, `Intent`, `NeedsReview`, and `Unknown` while
excluding `HistoryOnly`/`Suppressed`; explicit history/all scopes never promote
non-current evidence. `History` membership is voice-based and contains exactly
`HistoryOnly` and `Suppressed`, regardless of lifecycle label. `project` and
`projects` are mutually exclusive and use the existing daemon project-id/alias
validation.
When reserved suppression/proven-divergence derivation still exceeds its budget, the
candidate records canonical breach evidence in attempt accounting and is discarded;
it does not construct partially represented `Suppressed` units. No default, current,
history, or all generation result can retrieve that attempt. If authority work is not
advertised it is absent from `RequiredArtifactSet` and produces no result. Only bounded
rendering of an already complete leased result may report
`OutputCoverage::Truncated`.
The raw query is scanned before tokenization; a positive/indeterminate scan is
rejected without echo and never enters analytics, CCR, diagnostics, or caches.

## SearchKnowledgeHit

```rust
pub struct KnowledgeAuthorityDisplay {
    pub lifecycle: KnowledgeLifecycle,
    pub authority_domain: AuthorityDomain,
    pub code_evidence: CodeEvidenceDisplay,
    pub voice: KnowledgeVoice,
    pub finding_ids: Vec<String>,
    pub provenance_ids: Vec<String>,
    pub coverage: OutputCoverage,
}

pub struct CodeAnchorPreview {
    pub id: CodeAnchorId,
    pub line_range: Range<u32>,
}

pub enum BridgeResolutionDisplay {
    ResolvedExact { anchor: CodeAnchorPreview },
    ResolvedDeclaredSet { matched_count: u64 },
    Ambiguous {
        candidate_count: u32,
        bounded_samples: Vec<CodeAnchorPreview>,
    },
    Missing,
}

pub struct KnowledgeBridgePreview {
    pub link_id: KnowledgeCodeLinkId,
    pub evidence_kind: BridgeEvidenceKind,
    pub resolution: BridgeResolutionDisplay,
}

pub struct SearchKnowledgeHit {
    pub source: SourceIdentity,
    pub path: String,
    pub line: u32,
    pub line_range: Option<Range<u32>>,
    pub heading_path: Vec<String>,
    pub excerpt: String,
    pub content_hash: String,
    pub authority: KnowledgeAuthorityDisplay,
    pub bridge_links: Vec<KnowledgeBridgePreview>,
}

pub enum DiskObservationReceipt {
    Bytes {
        binding: BindingAuthority,
        physical_root: PhysicalRootIdentity,
        path: CatalogPath,
        observed_at: ObservationTime,
        stable_read: StableReadReceipt,
        byte_digest: ByteDigest,
    },
    Metadata {
        binding: BindingAuthority,
        physical_root: PhysicalRootIdentity,
        path: CatalogPath,
        observed_at: ObservationTime,
        file_identity: PlatformFileId,
        stamp: FileStamp,
        size: u64,
    },
    PathMissing {
        binding: BindingAuthority,
        physical_root: PhysicalRootIdentity,
        path: CatalogPath,
        observed_at: ObservationTime,
        parent_identity: PlatformFileId,
    },
}

pub struct WorktreeObservationCut {
    pub observer_epoch: ObserverEpoch,
    pub start_seq: InvalidationSequence,
    pub end_seq: InvalidationSequence,
}

pub enum WorktreeScopeCoverage {
    Complete {
        manifest_digest: ManifestDigest,
        stable_entry_count: u64,
    },
}

pub struct WorktreeScopeObservationReceipt {
    pub binding: BindingAuthority,
    pub physical_root: PhysicalRootIdentity,
    pub scope: WorktreeObservationScope,
    pub policy_versions: ScopeAndPolicyVersions,
    pub scan_id: WorktreeScanId,
    pub started_at: ObservationTime,
    pub finished_at: ObservationTime,
    pub observation_cut: WorktreeObservationCut,
    pub coverage: WorktreeScopeCoverage,
}

pub enum GitResolvedFrom {
    Commit { oid: GitObjectId },
    Tree { oid: GitObjectId },
    Ref {
        name: String,
        resolved_oid: GitObjectId,
        observed_at: ObservationTime,
    },
    Index {
        checksum: GitIndexChecksum,
        observed_at: ObservationTime,
    },
}

pub enum GitObjectKind {
    Blob,
    Tree,
    Commit,
}

pub enum GitObservationReceipt {
    Present {
        repository_id: RepositoryId,
        resolved_from: GitResolvedFrom,
        object: GitObjectKind,
        object_id: GitObjectId,
        path: CatalogPath,
    },
    NotInTree {
        repository_id: RepositoryId,
        resolved_from: GitResolvedFrom,
        object: GitObjectKind,
        tree_id: GitObjectId,
        path: CatalogPath,
    },
}

pub enum AtomicAuthority {
    Generation(GenerationAuthority),
    DiskObservation(DiskObservationReceipt),
    WorktreeScopeObservation(WorktreeScopeObservationReceipt),
    GitObservation(GitObservationReceipt),
}

pub enum ClaimInput {
    Authority(AtomicAuthority),
    Selection(SourceSelectionReceipt),
}

pub enum ClaimProvenance {
    Single(AtomicAuthority),
    Comparison {
        operation: OperationReceipt,
        relation: ComparisonRelation,
        left: AtomicAuthority,
        right: AtomicAuthority,
    },
    Derivation {
        operation: OperationReceipt,
        nonempty_inputs: NonEmptyVec<ClaimInput>,
    },
    SelectedAggregate {
        operation: OperationReceipt,
        selections: NonEmptyVec<SourceSelectionReceipt>,
        generations: NonEmptyMap<ProjectSourceKey, GenerationAuthority>,
        additional_authorities: Vec<AtomicAuthority>,
    },
}

pub struct Claim<T> {
    pub value: T,
    pub operation: OperationReceipt,
    pub provenance: ClaimProvenance,
    pub evaluation: Option<EvaluationProvenance>,
    pub producing_publication: ProducingPublicationIdentity,
}

pub enum SourceRefusal {
    InvalidSelection {
        operation: OperationReceipt,
        canonical_selector_hash: CanonicalArgumentHash,
    },
    AdmissionUnavailable {
        operation: OperationReceipt,
        subject: AdmissionSubject,
        cause: UnavailableCause,
    },
    SourceUnavailable {
        operation: OperationReceipt,
        source: ResolvedSourceReceipt,
        cause: UnavailableCause,
    },
    SelectionUnavailable {
        operation: OperationReceipt,
        set: ResolvedSelectionSet,
    },
}
```

All receipt constructors above are sealed. A `DiskObservationReceipt` is created only
while an `ObservationLease` owns the binding's `PhysicalRootLease` and the actually
opened file or final-parent handle through stable read, metadata sampling, or missing-
path proof. `WorktreeScopeCoverage` has no partial variant: incomplete traversal,
unstable required entries, or binding/root drift refuses instead of constructing a
receipt. `GitObservationReceipt::NotInTree` proves only that exact path's
non-membership in the named immutable tree.

Mixed-lane operations use one private operation-specific acquisition:

```text
acquire_claim_context(operation, normalized_inputs)
    -> Result<ClaimContext, SourceRefusal>

ClaimContextInput {
    project_source: ProjectSourceKey,
    binding: BindingAuthority,
    physical_root_lease: Arc<PhysicalRootLease>,
    repository_id: RepositoryId,
    current: Option<CurrentQueryLease>,
}

ClaimContext {
    operation: OperationReceipt,
    inputs: NonEmptyVec<ClaimContextInput>,
    permitted_relationships: OperationRelationshipContract,
}
```

The operation-specific constructor captures session/project publication, source token,
binding, owning root lease, repository identity, and every required `Current` lease as
one coherent acquisition, then exact-identity revalidates the capture before returning.
Inputs must be identity-compatible unless the closed operation contract explicitly
permits a cross-source relation. A rebind between Generation, disk, worktree-scope, or
Git input acquisition refuses rather than composing roots or repositories. A rebind
after the complete context is returned does not trigger a trailing live-state check;
claims derived wholly from its retained authorities remain valid. Pure observation
contexts may omit `current`, but any generation-structured derivation requires it and
can never substitute a retained non-Current generation.

Absence remains scoped to its authority. `DiskObservationReceipt::PathMissing` is
path-local at `observed_at` under the retained parent; `GitObservationReceipt::NotInTree`
is tree-local for its exact immutable `tree_id`; and a complete worktree-scope receipt
may support absence only inside its declared scope and start/end interval. Generation
`NotInGeneration` is limited to one complete captured generation. Repository/global
no-match or absence is legal only through `SelectedAggregate`, whose sealed constructor
proves an exact selection-receipt-to-Current-generation bijection. None of the local
negative receipts can be widened into generation or global absence.

Public generation-backed results are `Result<Claim<T>, SourceRefusal>`. Private,
operation-specific constructors bind the normalized request in `OperationReceipt`,
enforce the allowed provenance form/input cardinality, attach an immutable ranking
`EvaluationProvenance` whenever order/scores are observable, and derive the transport
mapping from one closed `OperationContractV1` table. Callers cannot deserialize,
default, convert, or assemble authority/certificate/receipt types.

`SelectedAggregate` is the only constructor for global absence/no-match and selected-
scope totals. Its private constructor consumes one sealed project query lease and
requires an exact bijection between every selected project/source receipt and every
captured generation. Missing, extra, forged, or uncaptured inputs refuse. A later live
membership/runtime change does not invalidate the already captured request; a cache
hit starts with a new strict lease and complete envelope key.

Hits return only the deterministic authority display,
stable finding/rule/link IDs, and server-bounded anchor previews. Full aggregate
evidence arrays and full bridge records remain available through `review_knowledge`.
Direct and CCR results preserve those compact IDs and provenance without copying an
evidence corpus.
All finding/provenance/link ID vectors and anchor previews are canonically ordered
and independently bounded; omitted counts remain explicit in derived/output
coverage. `provenance_ids` includes safe rule and policy-entry IDs needed to resolve
lifecycle/domain provenance through review.
They also carry overflow and withheld-sensitive counts. A hit's generation is
identified by its `ClaimProvenance`; a path-local disk observation cannot establish
generation membership or generation/repository-wide absence, while its own
`PathMissing` receipt remains valid only for that path and observation time. Complete
root-bound worktree scan authority is limited to its declared interval/scope. `line`
and every `line_range` are one-based;
ranges are half-open. Generation bytes come from the captured `VerifiedGeneration`
or a full digest-identity substitution; generation reads never silently reopen disk.

## Review and curation inputs

```rust
pub enum KnowledgeReviewMode {
    Summary,
    Document,
    Remediation,
}

pub struct ReviewKnowledgeInput {
    pub mode: KnowledgeReviewMode,
    pub path: Option<String>,
    pub path_prefix: Option<String>,
    pub source_scope: Option<KnowledgeSourceScope>,
    pub project: Option<String>,
    pub projects: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub max_tokens: Option<u32>,
}

pub struct KnowledgeAuthoritySummary {
    pub lifecycle: KnowledgeLifecycle,
    pub lifecycle_evidence: LifecycleEvidence,
    pub authority_domain: AuthorityDomain,
    pub authority_domain_evidence: AuthorityDomainEvidence,
    pub code_evidence: CodeEvidenceSummary,
    pub voice: KnowledgeVoice,
    pub finding_ids: Vec<String>,
    pub provenance_ids: Vec<String>,
}

pub struct KnowledgeReviewFinding {
    pub finding_id: String,
    pub unit: KnowledgeAnchor,
    pub authority: KnowledgeAuthoritySummary,
    pub timeline: DocumentTimeline,
    pub knowledge_evidence: Vec<KnowledgeAnchor>,
    pub code_evidence: Vec<CodeAnchor>,
    pub structured_evidence: Vec<StructuredFindingView>,
    pub bridge_links: Vec<KnowledgeCodeLink>,
    pub inbound_knowledge_links: Vec<KnowledgeKnowledgeLink>,
    pub inbound_live_link_count: u64,
    pub link_coverage: OutputCoverage,
    pub proposal: RemediationProposal,
}

pub enum SafeStructuredScalar {
    Identifier(String),
    Boolean(bool),
    Integer(i64),
    Version(String),
    Signature(String),
    EnumLabel(String),
}

pub struct StructuredFindingView {
    pub rule_id: String,
    pub knowledge_anchor: KnowledgeAnchor,
    pub code_anchor: CodeAnchor,
    pub document_value: SafeStructuredScalar,
    pub code_value: SafeStructuredScalar,
}

pub struct KnowledgeReviewSourceResult {
    pub review_hash: String,
    pub source: SourceIdentity,
    pub source_version: SourceVersion,
    pub manifest_digest: String,
    pub policy_digest: String,
    pub generation_authority: GenerationAuthority,
    pub coverage: OutputCoverage,
    pub findings: Vec<KnowledgeReviewFinding>,
}

pub struct KnowledgeReviewResult {
    pub result_hash: String,
    pub sources: Vec<KnowledgeReviewSourceResult>,
    pub overall_coverage: OutputCoverage,
}

pub enum KnowledgePolicyMutation {
    Upsert(KnowledgePolicyEntry),
    Remove {
        entry_id: String,
        expected_target: KnowledgePolicyTarget,
    },
}

pub struct KnowledgePolicyAction {
    pub action_id: String,
    pub mutation: KnowledgePolicyMutation,
}

pub enum CurationContinuityProof {
    Git {
        object_format: String,
        anchor_tip_object_id: String,
    },
    NonGit {
        root_object_identity: String,
        catalog_identity_digest: String,
    },
}

pub struct CurationSourceBinding {
    pub repository_id: RepositoryId,
    pub source_id: SourceId,
    pub continuity: CurationContinuityProof,
}

pub struct CurateKnowledgeInput {
    pub actions: Vec<KnowledgePolicyAction>,
    pub if_source_review_hash: String,
    pub if_manifest_digest: String,
    pub if_policy_digest: String,
    pub idempotency_key: Option<String>,
    pub apply: bool,
    pub project: Option<String>,
}
```

`review_knowledge` is read-only and may target multiple captured sources.
`curate_knowledge` targets exactly one current working-tree project. Preview is the
default. Apply requires durable per-project replay, a non-empty idempotency key, and
revalidation of manifest, policy, exact path, and content hashes. Before mutation it
durably records a canonical intent containing pre-image and post-image digests.
After that intent is durable and before any source-root side effect, apply acquires a
tracked non-cloneable `SourceMutationPermit` from the exact `Current` generation.
Grant advances `mutation_epoch`, invalidates older candidates, and atomically publishes
`Refreshing` with the permit record. Fallible path preparation precedes
`start_side_effect`; that call then exact-validates the Live project/source publication,
permit, binding, and pinned-root authority under the project writer and stores
`Granted -> InFlight` before releasing the writer for I/O. Only the resulting
`InFlightMutation` may write.
Both the replay record and pending intent persist `CurationSourceBinding`;
`ProjectId` selects their state directory but never proves repository identity.
Pending-intent discovery and startup recovery are read-only. They may parse and report
the durable `ProjectStateDir` intent/replay records, but they do not open or probe the
source root or Git object database, inspect pre/post-image bytes, retry a write, remove
a temporary file, finalize an intent, or infer success. They schedule lifecycle-owned
recovery only after that source has an exact `Current` generation.

Recovery and same-key replay acquire a tracked `SourceMutationPermit` from that exact
`Current` before any pre-image retry, cleanup, source probe, or source write. Grant
publishes `Refreshing`; `start_side_effect` exact-checks the source token, binding, and
pinned physical-root lease and stores `InFlight` before any such I/O. Only then may
recovery verify source continuity rather than whole moving-state equality:
`RepositoryId`/`SourceId` must match; a Git proof additionally requires the same object
format and its recorded anchor tip to remain resolvable as a commit in the live object
database; a non-Git proof requires unchanged platform root-object identity and an
unbroken durable catalog lineage from the recorded digest to the current one.
`root_object_identity` reuses the private, bounded `PlatformFileId` encoding captured
from the permit's already pinned handle to the canonical source root; recovery never
reopens the path to reconstruct it. It is continuity evidence only, never logical
source identity or a manifest field. Accepted non-Git content publications and applied
or recovered curation commits append the prior-to-current catalog-digest transition to
the durable `ProjectStateDir` replay store before that successor can prove continuity;
links required by live replay or intent records are retained, and a missing required
link fails closed.
Current tip/ref/history movement alone is drift, never foreign identity. Manifest and
policy digests are freshness guards revalidated on first execution and intentionally
bypassed by same-key/same-hash replay; they are not source-sameness fields. A missing
continuity proof returns typed `foreign_source_conflict`, quarantines attributable
pending intent, writes nothing, and never replays an `applied` result from the foreign
repository. The
writer performs temp `write_all`, file `sync_all`, atomic replace, and required
parent-directory durability before recording success. Every source path component,
temporary file, and final replacement is resolved handle-relative beneath the permit's
pinned physical-root lease with no-follow/reparse-safe semantics and a validated final
parent; platforms without equivalent confinement refuse destructive I/O. Recovery of
a pending intent first acquires the exact `Current` generation under the mutation lock
and branches before minting a permit. If the source equals the post-image, it performs
only `ProjectStateDir` replay/result finalization: no source-root I/O, no permit, no
lifecycle transition, and no successor candidate. If the source equals the pre-image,
it must acquire a fresh `SourceMutationPermit` from that exact `Current` publication;
only then may `InFlightMutation` retry the confined source write. Otherwise it returns
an explicit indeterminate/conflict result without overwrite or cleanup. It rejects
move/delete actions at schema validation. Permit commit schedules a fresh candidate,
with watcher evidence coalesced into the same lifecycle work. A valid proof that no
source side effect began after a permit was granted schedules a cheaper no-op
verification candidate at the latest observer cut; even that path cannot restore the
predecessor directly to `Current`. Side effect, failure, drop, panic, or indeterminate
recovery after permit grant remains non-Current until a fresh complete candidate
promotes. The response is tied to the resulting or explicitly pending generation,
never a side-channel authority override.

## State transitions

```text
Candidate path:

Discovered
  -> metadata admission
       -> committed-candidate MetadataOnly | HardSkip
       -> attempt-only Unreadable
       -> bounded probe
             -> committed-candidate MetadataOnly | HardSkip
             -> attempt-only Unreadable
             -> Ingest(targets) -> StableRead
                  -> attempt-only Unreadable(FullRead)
                  -> attempt-only UnstableDuringRead
                  -> attempt-only AbortedCircuitBreaker
                  -> SecretScan -> committed-candidate MetadataOnly(SensitiveContent)
                                -> TargetParse
                                     -> committed-candidate Indexed(Parsed)
                                     -> attempt-only PartialParse | Failed
                                     -> attempt-only AbortedCircuitBreaker

Complete candidate + exact authority + complete required artifacts/proofs
  -> one registry runtime-root store
  -> SourceRuntimeState::Current(VerifiedGeneration)

Any attempt-only disposition, observer gap, proof expiry, authority mismatch,
capacity refusal, or incomplete required artifact
  -> discard candidate
  -> Loading | Refreshing | Blocked
  -> SourceRefusal for strict acquisition
```

Circuit breakers remain scoped per source, lane, and stage for bounded cancellation
and diagnostics. A trip marks the remaining affected attempt entries
`AbortedCircuitBreaker`, discards the whole candidate, and schedules lifecycle-owned
recovery. It cannot create a partially committed manifest or a degraded generation.

No transition removes an entry. Delete is represented by absence from the next
complete manifest, applied atomically as part of that generation.
