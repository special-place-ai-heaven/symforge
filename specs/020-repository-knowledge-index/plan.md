# Implementation Plan: Repository Knowledge Index

**Branch**: `design/project-activation-prevention`
**Spec**: [spec.md](./spec.md)
**Status**: V11 refreeze in progress — implementation is not authorized until the
manifest, attestation, external approval, and public-API contract gates pass

## Summary

Refreeze the Feature 020 knowledge work onto the cleared V11 source-lifecycle
architecture, then build it in four product layers:

1. Make repository discovery and publication total, bounded, deterministic, live,
   and recovery-safe through one metadata-first manifest.
2. Activate the already-present in-memory text lane as repository knowledge and
   expose one bounded `search_knowledge` tool.
3. Derive an exact, source-local bridge and first-contact mental model from existing
   knowledge spans and code anchors.
4. Reconcile supported current-implementation claims against code evidence, expose
   bounded remediation review, and persist approved lifecycle voice in one
   hash-bound repo policy ledger.

Do not start production lifecycle or knowledge implementation until every Feature
020 artifact is classified and the externally anchored refreeze approval binds the
exact commit/tree. Bulk, watcher, reconciliation, snapshot verification, embed, and
publication must share one lifecycle authority before knowledge-tool/bridge/authority
activation. Physical file movement/deletion is not part of this feature.

## Technical context

**Language**: Rust
**Primary storage**: in-process `LiveIndex`; derived snapshots use project-local or
user-local `StatePlacement` when writable and are optional in memory-only mode
**Search**: existing trigram candidate index, Aho-Corasick/regex line matching,
exact line renderer, Markdown section spans
**Concurrency**: Rayon for off-lock build, bounded in-flight bytes, and one
`ArcSwap<ProjectRuntimePublication>` as the sole immutable publication root; numeric
epochs are diagnostic and never authorize a read, mutation, or commit
**Git**: existing `git2`, bounded temporal evidence, worktree/project routing
**V11 lifecycle topology**: `src/index_lifecycle/` owns whole-authority tokens,
physical-root leases, mutation permits, registry/process ownership, capacity,
embedded handles, closed runtime state, candidate/supervisor/observer/verification,
query acquisition, activation, and the planned public API adapter
**Tests and proof**: colocated Rust unit tests plus focused integration fixtures,
production-kernel Loom interleavings, four pure proptest command models, four TLA+
models under `formal/v11/`, and `ObservedRefreshGateV1` correctness/performance
fixtures under `benches/` and `tests/fixtures/`

## Project-principles check

`.specify/memory/constitution.md` is still an unratified template and therefore
supplies no project-specific normative clauses. This table checks the repository
principles in `AGENTS.md`; no template placeholder is treated as constitutional
authority. Ratifying a Spec Kit constitution is separate governance work.

| Principle | Plan response |
|---|---|
| Coding-first | Code queries stay code-scoped; knowledge is a separate target/tool. |
| Evidence authority | Code evidence has precedence only for checked current-implementation claims; intent/governance remain separately voiced. |
| Determinism | Canonical sorted manifest, stable ties, one captured source set with immutable source generations. |
| Local-first | No remote service, DB, new knowledge-specific sidecar, or fetch; source roots need not be writable. |
| Byte exactness | Stable reads preserve raw bytes/hash/line endings. |
| Recovery | Versioned manifest snapshot, quarantine, source rebuild. |
| Idempotency | Equal complete, exactly fenced candidates produce no mutation; failed attempts never mint a manifest. |
| Failure isolation | Failed candidates are discarded; a reload-entered refresh still serves its retained generation; remnant and mutation-entered retentions stay unqueryable. |
| Bounded work | Independent entry/catalog-metadata/read/probe/in-flight/derived/source/output limits. |
| Simplicity | Reuse current index/search; no embeddings/FTS/vector store. |

## Existing seams to reuse

- `src/discovery/mod.rs::discover_all_files`
- `src/discovery/mod.rs::classify_admission`
- `src/discovery/mod.rs::{find_project_root, resolve_workspace_root, is_forbidden_root}`
- `src/paths.rs::{is_sensitive_path, select_runtime_data_base, ensure_runtime_symforge_dir}`
- `src/cli/init.rs::{run_init, run_init_with_paths}`
- `src/daemon.rs::{open_project_session, index_folder_for_session}` and protocol
  `index_folder`/startup bind paths
- `src/live_index/store.rs::admit_and_parse_entries`
- `src/live_index/store.rs::{build_reload_data, apply_reload_data}`
- `src/live_index/search.rs::{FileClass::Text, SearchScope::Text}`
- `src/live_index/trigram.rs`
- `src/live_index/graph.rs::SymbolId`
- `src/live_index/git_temporal.rs::{GitTemporalIndex, GitFileHistory}`
- `src/live_index/query.rs::RepoOutlineView`
- `src/parsing/config_extractors/markdown.rs`
- `src/watcher/mod.rs::{read_and_index, reconcile_stale_files_with_stop}`
- `src/live_index/persist.rs::{IndexSnapshot, background_verify}`
- `src/worktree/` and daemon `ProjectInstance`/`ProjectSlot`
- `src/protocol/ccr.rs`
- existing `get_repo_map`, `ask`, `get_file_context`, and `get_symbol_context`
  format/dispatch paths (currently recapture state independently and must be fixed)

## Architecture

```text
workspace/env/client/index/init candidate
       |
       v
source-root decision (raw + canonical + per-session request authority)
       |
       +-- automatic protected ----------> Unbound (responsive; accepts rebind)
       |
       +-- normal / direct explicit protected --> bound source (never retargeted)
                                             |
                                             +-- project-state placement
                                             |     project-local
                                             |     user-local per root-id
                                             |     memory-only degraded persistence
                                              v
repository/worktree/ref source
       |
       v
scout_repository (metadata only first)
       |
       +-- terminal catalog-only ----------+
       |
       +-- bounded probe -> terminal -------+--> RepositoryManifest
       |                                    |
       +-- stable admitted read             |
              |                             |
               +-- code parse ------------------------+
               +-- knowledge extraction --------------+
                                                        |
                    exact source-local resolution       |
                    +-- role cards                      |
                    +-- knowledge <-> code bridge       |
                    +-- temporal/authority evidence     |
                    +-- repo policy voice               |
                                                        v
                              isolated candidate + manifest + strict certificates
                                                        |
                              validate accounting/generation/coverage
                                                        |
                                                        v
                    registry-owned ArcSwap<ProjectRuntimePublication> (one root store)
                                  ProjectRuntimeState::Live
                                  SourceRuntimeState::Current
                                  -> Arc<VerifiedGeneration>
```

Process-global `ControlStateDir` selection is a separate private user-local lane;
it is neither source binding nor `ProjectStateDir`, and it has no launch-CWD/source-
derived fallback. A protected membership exists only for the session whose direct
override request succeeded and is dormant across other sessions and restarts.

### V11 publication rule

All I/O contributing to `Generation` authority or candidate artifacts, including
authoritative source observation, hashing, parsing, discovery, proof refresh,
reconciliation, outline/search, bridge, temporal, and authority derivation, happens
in a capacity-reserved isolated candidate. Typed pure `DiskObservation`,
`WorktreeScopeObservation`, and `GitObservation` adapters run under their own
observation leases/`ClaimContext` outside candidates and cannot mint generation
authority. Excluded `ProjectStateDir` persistence, exact post-image receipt
finalization, control-plane health capture, and post-lease response rendering are
not candidate artifacts and cannot mint source authority. The candidate is unreachable from queries, caches,
checkpoints, and legacy embed paths. Promotion requires one sealed completeness
certificate covering the manifest and every advertised strict artifact.

The registry alone owns the immutable `Arc<ProjectRuntimePublication>` root; its
closed payload is `ProjectRuntimeState`. Source lifecycle code supplies a sealed
source transition intent; the registry patches that source into the latest whole
project root under the publication writer and performs one non-fallible store.
Untouched membership and source siblings are preserved. Binding,
physical-root, observer, mutation, candidate, and exact source-publication identities
are validated as one authority; check-then-swap or separately sampled generations
are forbidden.

Every generation-backed public consumer obtains a strict
`ProjectQueryLease`/`QueryLease` from that single runtime root. A successful lease
exists only for `Current(VerifiedGeneration)` and pins one immutable generation plus
its exact scope/provenance evidence. Health instead uses the lifecycle-owned composite
`capture_source_view` seam and remains available while a source is non-Current. Pure
`DiskObservation`, complete `WorktreeScopeObservation`, and immutable
`GitObservation` operations use their own authority leases; a mixed derivation uses
one identity-compatible `ClaimContext` and requires a Current generation lease only
when generation structure is an input. A failed generation- or candidate-critical
observation publishes `Loading`, `Refreshing`, or `Blocked`; a failed pure
observation returns typed `SourceRefusal` and preserves lifecycle state unless it
independently proves lifecycle invalidation through the observer seam. Any retained
generation remains byte-identical and internal. Snapshot seeds remain untrusted
candidates. There is no public degraded wrapper or derived-only mutation of Current.

### V11 read and capacity rule

Metadata may terminate admission. Only undecided, under-ceiling candidates receive
a bounded probe. Only admitted candidates receive a full read. Full-read bytes
remain charged through verification, parsing, and hand-off. The permit releases
when ownership transfers into the staged `LiveIndex`; the independent admitted-
content ceiling then governs staged/resident bytes. This release point permits an
in-flight budget smaller than the admitted ceiling without deadlock.

All allocation-producing interfaces reserve from one process capacity domain before
allocation and retain exact charges through final physical drop. Active, candidate,
retired/query-pinned generations, observer/journal state, snapshot stages, parser and
blocking work, runtime publication nodes, output materialization, and child partitions
are all included. Capacity refusal leaves the runtime non-current and cannot construct
a placeholder project/source.

Catalog entry and canonical descriptor bytes are budgeted before a complete
manifest can exist. Entry/metadata exhaustion aborts the candidate observation; it
does not publish a truncated `RepositoryManifest`. Oversized path spellings fall
back to an opaque catalog ID and metadata-only disposition where one entry still
fits; issue strings/counts are independently bounded.

## V11 prerequisite and implementation sequence

The checked-in `REFREEZE-MANIFEST-v11.md` classifies every Feature 020 artifact plus
bound `CONTEXT.md`, maps `F020-V11-A01` through `A19` to exact replaced/replacement
clauses and regressions, and excludes only its own recursive digest. The detached
attestation pins the manifest, cleared lifecycle design, context, canonical public
API, and amendment-set digest. A signed append-only `RefreezeApprovalRecordV11`
stored outside this repository must bind the exact target commit/tree and trusted
release identity. Internal validation cannot substitute for that trust anchor.

The canonical `contracts/public-api-v11.json` is a closed allowlist over every
supported target/cfg/feature cell and the complete externally reachable Rust public
item/impl graph. It also defines the v10 keep/replace/remove mapping. No slice may
invent or widen an external Interface after approval.

1. **Slice 0 — causal RED oracles.** Check in current positive controls for the
   generation-before-root race, duplicate first load, placeholder/capacity admission,
   same-stamp staleness, raw-disk generation attribution, and raw embed bypass. Map
   every later acceptance oracle to its production seam, slice, exact command,
   fairness bound, and CI artifact; do not claim unimplementable future oracles ran.
2. **Slice 1 — atomic mutation authority.** Add whole binding/root/observer/candidate/
   mutation authority, pinned beneath-confined root I/O, non-cloneable permits, and
   Freeze -> Drain -> Install. Remove `effective_fence_generation` inference.
3. **Slice 2 — registry tombstone and capacity foundation.** Add pending singleflight
   admission, project/source revocation packages, stable process runtime/capacity
   domain, exact grant-to-charge ownership, blocking/query/snapshot envelopes,
   hierarchical tombstones, fair admission, and cleanup-before-resize.
4. **Slice 3 — behavior-neutral seams, provenance, and dark runtime.** Consolidate
   current reads without relabeling them Current; type generation, disk,
   worktree-scope, and Git evidence; preserve operation/selection/evaluation receipts;
   build closed runtime/query/embed Interfaces behind unreachable constructors; and
   generate the attested external API harness. Production authority remains V10.
5. **Slice 4 — indivisible candidate/delta activation.** Install lifecycle-owned
   loading, isolated full/delta candidates, observer accumulator/handoff, pre-write
   invalidation, proof obligations/deadlines, snapshot-as-seed, strict leases, claim
   composition, and one process-wide `PreventiveV1` cut across every entry path. The
   same cut retires all legacy writers/exports and must pass `ObservedRefreshGateV1`,
   capacity, equivalence, activation, and external approval gates. No full-rebuild-
   per-edit refusal intermediate ships.
6. **Slice 5 — mechanical cleanup only.** Delete already-unreachable placeholder,
   circuit-breaker lifecycle, legacy mode, secondary publication, and raw embed code.
   This slice cannot retire authority or change behavior.

## Historical V10 Phase 0 — superseded execution record

The V10 phases below remain implementation evidence. Where they conflict with the
V11 publication/readiness rules or sequence above, they are non-authoritative.

Write and run the red tests before production edits:

- giant sparse artifact is not read or admitted-byte charged;
- home/profile/filesystem-root/OS/symlink aliases remain unbound before any source
  walk or candidate-root/per-project state write across automatic/startup/init entry
  points;
- explicit protected-root indexing never probes `<root>/.symforge`, uses user-local
  or memory-only state, grants no init/curation capability, and authorizes only the
  requesting session; a second session/reconnect/restart must make its own direct
  override request;
- project-local and user-local state failures degrade to live memory-only indexing;
- memory-only `checkpoint_now` returns successful typed `applied=false` unavailable;
- unbound startup can rebind an accessible project and reach Ready without restart;
- failed retarget preserves any prior binding/generation/watcher, while device,
  special, missing, and uncanonicalizable roots remain refused under every flag;
- nested `ProjectStateDir` and `ControlStateDir` are dynamically excluded from their
  protected parent source;
- snapshot verification rejects a different repository occupying the same path and
  never treats a placement key/`ProjectId` as source identity;
- same-key `index_folder` replay re-establishes live binding/session membership or
  returns typed `live_postcondition_unavailable`; a receipt alone is never success;
- every snapshot/quarantine/checkpoint/TEE/frecency/analytics/API-key and sidecar/
  status/runtime-startup/hook/operator/onboarding/version-updater/replay consumer
  uses its typed owner with no CWD/source-derived/relative fallback;
- existing `.gitignore` receives canonical `/.symforge/` during explicit normal
  `index_folder` and project-aware init; absent stays absent, automatic paths never
  write it, and hygiene failure does not disable a valid live index;
- team-artifact export reports the four Git visibility states exactly and refuses
  every protected/read-only/non-project-local placement before either repository
  write;
- catalog descriptor exhaustion never publishes a partial manifest as Complete;
- metadata failure never becomes size zero;
- total deterministic manifest/disposition equality;
- case-fold path pairs remain isolated and deterministically ordered;
- watcher admission before read;
- read mutation refusal;
- in-flight hand-off cannot deadlock a corpus larger than the in-flight budget;
- catalog-only delete/missed create reconciliation;
- incomplete observation remains non-current and retries to convergence;
- reconciliation racing a watcher event cannot lose either update;
- snapshot manifest parity;
- stale verifier retarget fence;
- mixed-generation concurrent publication;
- code/knowledge scope separation.
- one-call state capture across map/context/bridge/authority formatters;
- exact/ambiguous/missing bridge resolution and reverse-link atomicity;
- lifecycle/authority/voice separation, age-only review, and intent preservation;
- stale remediation/curation hashes refuse before policy mutation;
- curation intent/pre/post journaling recovers each crash boundary deterministically,
  and apply is unavailable without durable replay plus atomic durable replacement.

Stop if any proposed “red” test already passes for the intended reason; revise the
oracle rather than adding redundant code.

## Historical V10 Phase 1 — Metadata-first manifest

### Changes

- Consolidate source-root resolution into one typed raw+canonical result used by
  startup env/client/CWD, daemon/session open, `index_folder`, init, watcher,
  reconciliation, and snapshot verification. Automatic protected roots stay
  unbound; only `index_folder(..., allow_protected_root=true)` binds the exact
  protected target. A rejected request never falls through to unrelated CWD.
- Represent project membership separately from the live project slot. A protected
  slot may be reused only after each session's own matching direct override succeeds;
  reconnect/session metadata, persisted state, and restart never manufacture that
  authority.
- Keep the server responsive over an empty unbound index with corrective health;
  do not start traversal/project watcher/snapshot/per-project state creation for a
  refused root, and prove a later valid `index_folder` clears the failed bootstrap
  state.
- Keep process-global transport/control placement independent. Remove every fallback
  from its user-local base to launch CWD, a rejected candidate, or relative
  `.symforge`; if unavailable, coordination/idempotency is explicitly process-local.
  Route the edit-safety trust store, sidecar port/PID/session and status readers,
  daemon discovery/control and runtime-startup coordination, hook adoption/hints,
  operator profile, onboarding, version registry/updater, and cross-project replay/
  locks through this same typed `ControlStateDir` selector.
- Select runtime-state placement after source binding. Reuse the existing runtime
  data-base seam, but namespace user-local state by a versioned digest of the
  canonical root identity. Consolidate existing project-key/project-ID helpers into
  that one lossless constructor. Normal roots try project-local state first; explicit
  protected mode skips that path. Catch local/user-local access failures and keep a
  queryable memory-only index with persistence capabilities disabled explicitly.
- Route snapshot/temp/quarantine/reset/checkpoint, per-project replay/mutation
  intent, edit-safety TEE, frecency/coupling/STEL, analytics, API-key state, and
  derived cleanup through typed `ProjectStateDir`. Route only the process-global
  consumers named above through typed `ControlStateDir`. Remove every root-derived,
  launch-CWD, and relative compatibility wrapper; each writer and reader receives
  the same typed owner.
- Apply the stricter raw/canonical root classification, keep explicit-protected
  authority non-transferable outside the direct tool request and requesting session,
  and preserve an existing binding on every failed retarget.
- Dynamically hard-exclude both selected state directories when either is nested
  under the source. Bind snapshot headers to project/repository/stable-source/source-
  version plus manifest/admitted-content/history fingerprints; refuse foreign state,
  including a different repository occupying the same path.
- Make `index_folder` replay verify/re-establish the live source and requesting-
  session membership before returning `applied=true`. If the exact postcondition
  cannot be reconstructed, return successful typed `applied=false` with
  `live_postcondition_unavailable`; never return a historical receipt as live success.
- After successful explicit normal `index_folder` binding and during project-aware
  init, inspect only an existing repository-root `.gitignore`; append canonical
  `/.symforge/` atomically/idempotently, preserve byte/line-ending style, and do not
  create a missing file. A failure is visible but does not poison the live index.
  Automatic startup/scout/watcher/reconciliation/ref ingestion only report hygiene,
  and every path always hard-excludes `.symforge/` independently.
- Preserve the legacy team artifact only for a normal writable project-local
  placement. Report `already_tracked`, `untracked_visible`,
  `ignored_force_add_required`, or `git_visibility_unavailable`; never redirect it
  into user-local state or infer shareability when Git visibility is unavailable.
- Return reason-bearing capability states. In particular, memory-only
  `checkpoint_now` is a successful typed `applied=false` response, not an MCP error
  or stale success.
- Add minimal manifest/target/disposition/coverage types to `src/domain/index.rs`.
- Replace `discover_all_files` as the authoritative load path with
  `scout_repository`; retain a compatibility wrapper only if a real caller needs it.
- Fail explicitly on double metadata failure.
- Apply `classify_admission(path, size, None)` before admitted-byte accounting.
- Keep catalog entry count and canonical catalog-metadata bytes independent from
  admitted, in-flight, and derived-state bytes. Refuse an incomplete candidate
  observation before a `RepositoryManifest` exists when either catalog bound is
  exhausted. Publish no partial manifest: discard the candidate, keep any retained
  verified generation internal/non-current, and return a typed capacity refusal with
  zero queryable partial generation.
- Sort safe paths by normalized case-insensitive key plus exact UTF-8 bytes and
  opaque paths by public ID; retain case-fold collisions as distinct entries and
  isolate only platform-unsafe paths.
- Represent safe UTF-8 paths exactly; catalog non-UTF-8/unsafe path text by stable
  opaque ID without lossy conversion and keep it outside content targets.
- Bound every persisted path/issue descriptor; use an opaque ID plus typed
  `PathMetadataTooLarge` disposition rather than retaining an oversized spelling.
- Convert walker errors into bounded attempt issues that prevent promotion.
- Include repository-owned hidden instruction/documentation trees while hard-
  excluding `.git/` and `.symforge/`; expose ignore-pruned coverage as policy.
- Return an immutable manifest plan; do not store content in it.

### Exit gate

Root/session authorization, state-owner routing, strong snapshot identity,
idempotent live-postcondition replay, ignore/artifact hygiene, capability receipts,
and scout/admission focused tests pass; the existing normal over-byte-cap test still
refuses admitted content.

## Historical V10 Phase 2 — Stable bounded content execution

### Changes

- Introduce one narrow stable-read helper at the existing shared ingestion seam.
- Open handle, validate scout stamp, enforce `u64`/`usize`/policy limits, reserve
  fallibly, read through a hard bound, hash bytes, and verify handle/path state.
- Re-open and stream a second bounded hash pass; accept only matching length/hash
  and stable metadata. This narrows same-size/coarse-mtime torn-read races.
- Run the versioned content detector before staged-index hand-off; positive or
  indeterminate scans discard transient bytes/hash and become metadata-only.
- Retry a small fixed number of times; produce `UnstableDuringRead` afterward.
- Hold in-flight permits through read/verify/parse/hand-off, then release after
  bytes transfer into staged-index accounting.
- Replace outcome `filter_map` loss with terminal dispositions.
- Mark all post-break paths `AbortedCircuitBreaker` in attempt accounting, cancel the
  remaining work, and discard the candidate without minting a manifest.
- Map an individual read request larger than the global in-flight budget to terminal
  `HardSkip(PerFileCeiling)` before allocation.

### Exit gate

Stable-read and terminal-accounting tests pass under normal, read-error,
mutation, and circuit-breaker fixtures.

## Historical V10 Phase 3 — Watcher and reconciliation convergence

### Changes

- Remove supported-language filtering before single-path scout/removal.
- Apply hidden/generated/sensitive/size admission before any full read.
- Add one batch file-update operation that changes ingested content, targets,
  catalog disposition, and derived indices in one publication.
- Keep `RepositoryManifest` as the sole disposition authority; delete stored
  `skipped_files`, project any compatibility response ephemerally, and retire
  direct legacy skip mutations.
- Make removal clear every state for a path.
- Replace Tier-1-only reconciliation with fresh manifest diff.
- Treat rename as delete+add unless identity evidence is conclusive.
- Trigger authoritative reconciliation on overflow/`need_rescan`, watcher fresh
  instance, generation/policy/topology changes, and any incomplete observation.
- Publish non-current work state and retry with lifecycle-owned bounded backoff;
  equal entry digest is a no-op only for a complete, exactly fenced proof.
- Treat every `Unreadable`/`UnstableDuringRead` observation as promotion-blocking
  attempt evidence and keep an independent re-observation trigger; persistent failure
  remains an explicit refusal rather than becoming an equal-digest no-op.

### Exit gate

Missed create/delete/rename, catalog-only shrink/delete, event storm, overflow,
transient non-current recovery, and stale-generation tests pass; equal complete
manifest reconciliation is a no-op.

## Historical V10 Phase 4 — Snapshot and atomic generation

### Changes

- Version snapshot schema to persist canonical manifest/dispositions/targets.
- Capture `SourceVersion` once per observation, including the closed working-tree
  state, and carry it in the manifest, snapshot identity, core published bundle, and
  every per-source response envelope. Exact manifest/content digests remain the
  dirty-byte identity.
- Include the secret-policy version; mismatch forces re-scout before Ready, and
  sensitive content persists only safe reason IDs/counts, never source bytes.
- Restore the snapshot manifest before rebuilding Gate-E core state: live index,
  health, outline, resident search structures, and code temporal signals.
- Treat snapshot as a candidate until full scope/admitted-content verification.
- Route verification through shared scout/admission/stable-read logic.
- Store and verify strong source identity: project/repository/stable source location,
  source version, manifest digest, admitted-content digest, and available Git-history
  fingerprint. Placement path/`ProjectId` is not proof; same-path repository
  replacement rejects/quarantines the candidate before Ready or overwrite.
- Fence every verifier result by strong source identity plus its captured base
  publication/content/project generations. A watcher or reconciliation publication
  that advances that source makes the verifier stale; commit must rebase/retry or
  abort instead of replacing newer content.
- Replace externally independent live/state/outline generation observations with
  one `PublishedSourceSet` atomic boundary containing immutable per-source
  core `PublishedGeneration`s. The final data-model shape is post-H: Gate G extends
  the bundle with bridge state and Gate H extends it with authority state only after
  those types exist.
- Route every map/context/search/review formatter through one captured immutable
  bundle rather than independent ArcSwap loads.
- Fold Git temporal/hotspot state into that core bundle. Each initial or pending-
  latest marker captures the live source/content generation and exact source-version
  commit/tip when scheduled. Accept completion only when its analyzed target equals
  both that marker and the current live target, then atomically republish affected
  core/map state with the accepted tip carried by bundle, manifest, temporal snapshot,
  and envelope while content generation and manifest/content digests stay unchanged.
  Gates G/H attach their derived rebuilds to the same publication path later. Rejected
  stale work re-captures one bounded pending-latest recomputation target.
- Represent source truth and work in one closed runtime state. Failed generation- or
  candidate-critical source observations atomically publish non-current work/refusal
  state while any retained verified generation remains immutable and unreachable
  through strict acquisition. A failed pure observation returns typed refusal without
  changing lifecycle state unless it independently proves observer-seam invalidation.
- Compute the canonical manifest digest once per debounced published batch and
  cache it; never recompute it per query or per raw watcher event.
- Construct reload replacement directly from `ReloadData`; avoid cloning the old
  growing index only to overwrite it.

### Exit gate

Snapshot/source logical parity, corrupt quarantine, crash-before-publish, stale
verifier, and concurrent mixed-generation tests pass.

## Historical V10 Phase 5 — Activate knowledge targeting

### Changes

- Add invariant-bearing `IndexTargets::{Code, Knowledge, CodeAndKnowledge}`; keep
  `FileClass` descriptive and make an empty ingest target unrepresentable.
- Add `LanguageId::Text`/generic safe text extraction.
- Bounded-probe unknown under-ceiling files; valid textual content targets knowledge.
- Markdown/prose target knowledge only; configs/schemas may target both.
- Recognize bounded Git LFS pointers as catalog-only with declared metadata; never
  index pointer text or materialize its object.
- Add a separate deterministic sensitive-repository-path policy before reads.
- Add one small versioned detector using the existing bounded byte-regex engine;
  compile once, use context-anchored high-precision rules, and never use entropy
  without a captured context rule. No external scanner dependency.
- Scan stable bytes before publication and discard positive/indeterminate files
  from both code and knowledge targets. Scan query/path/heading/excerpt/diagnostic/
  source-label fields again before output and withhold the whole hit.
- Project current Markdown section spans as structural units; do not persist a
  duplicate `KnowledgeUnit` representation.
- Keep code doc-comments out of the knowledge lane in v1.
- Add CommonMark/GFM parser dependency only if the hardened existing extractor
  cannot pass ATX/Setext/fence/frontmatter/table/link position fixtures.
- Make internal code/text search scopes consult targets.

### Exit gate

README/unknown prose/config/Rust fixtures show exact target partitioning; prose
never leaks into code symbol/text results.

## Historical V10 Phase 6 — Evidence bridge core

### Changes

- Add compact `KnowledgeAnchor`, `KnowledgeRole`, `CodeAnchorId`, link resolution,
  stable `KnowledgeCodeLinkId`, reverse-link, and derived-coverage types; store
  indices/anchors, never copied document bodies or generated summaries.
- Assign roles only from declared spans, exact heading rules, and versioned path
  conventions. Ownership/status claims require declared evidence or remain unknown.
- Extract bridge candidates only from internal links, exact repository path tokens,
  code-spanned exact symbol names, supported structured values, and declared
  ownership selectors.
- Resolve path/symbol anchors inside one captured source/content generation.
  Preserve exact, declared-set, ambiguous, and missing outcomes; never fuzzy-link.
- Reuse existing code topology/churn/hotspot signals and publish them inside the
  same immutable bundle as role cards/bridge state.
- Persist only bridge rule/policy versions, rebuild the compact bridge from verified
  live state, and add the bridge field to the Gate-E bundle in this phase.

### Exit gate

Exact path/unique-symbol/ownership fixtures resolve bidirectionally; ambiguous,
missing, cross-source, secret-positive, removal, rename, derived-budget, and
concurrent-publication tests report exact uncertainty with no code-scope/frecency
contamination.

## Historical V10 Phase 7 — Authority and policy foundations

### Changes

- Define lifecycle, authority-domain, code-evidence, and retrieval-voice types before
  any public search schema depends on `authority_scope`.
- Project typed units from admitted knowledge spans and preserve mixed-unit state;
  unresolved evidence remains labeled unknown rather than inheriting document-wide
  voice.
- Parse the versioned repo-owned `.symforge-knowledge.toml` ledger read-only. Bind
  every whole-file/unit decision to exact safe path and content hash; malformed,
  unsupported, or stale policy cannot suppress content.
- Add versioned deterministic rules for broken exact anchors and supported
  structured mismatches. Treat age, filesystem timestamps, later code commits,
  lexical similarity, and model opinion as review signals only.
- Capture filesystem birth/modified hints plus bounded Git first-seen/last-touch and
  code-since-document evidence with explicit shallow/window/rename/dirty coverage.
- Establish the precedence rule: code evidence has precedence only for checked claims
  about current implementation behavior. Intent, ADR, governance, security policy,
  and north-star evidence remain independent and may expose an implementation gap.
- Derive deterministic voice from declared lifecycle, bridge evidence, temporal
  coverage, and hash-valid policy; code never silently assigns `Implemented`.
- Persist only authority-rule and policy-ledger versions/digest, rebuild authority
  from verified live/bridge state, and add the authority field to the immutable
  bundle in this phase.

### Exit gate

Axis/type, mixed-unit, declared-intent, exact divergence, temporal provenance,
policy hash/version/conflict, budget, and default-voice tests pass before the search
schema is advertised.

## Historical V10 Phase 8 — `search_knowledge` and routing

### Changes

- Add one narrow eight-field input schema (including authority scope) and one
  full-surface read tool.
- Reuse existing trigram candidate lookup, exact line matches, enclosing sections,
  context bounding, CCR, and cross-project result attribution.
- Start with deterministic exact-phrase, heading, distinct-term coverage, current-
  source precedence, and canonical path/line ties; add diversity only when a failing
  real-corpus query demonstrates same-file flooding.
- Include source identity/version, content hash/object ID, generation, coverage,
  line range, compact deterministic authority display, stable finding/rule/link IDs,
  bounded anchor previews, secret-policy version, and withheld/overflow state. Keep
  full evidence arrays and bridge records behind `review_knowledge`.
- Enforce `extract -> detect -> SafeHit -> format -> budget -> CCR`; raw candidates
  never enter CCR summary/full storage, analytics, or formatting buffers.
- Route `ask` immediately. Route the compact `symforge` facade only after red tests
  prove knowledge no-match remains a successful response and existing intent
  decoding cannot misroute code questions.
- Keep compact surface count at three and frecency unchanged by discovery.
- Default authority scope to current, explicitly labeled intent, needs-review, and
  unknown evidence; history-only and suppressed units require explicit scope and
  never regain current voice.

### Exit gate

Tool schema (including existing `project`/`projects` selectors), formatter,
routing, no-match, security, output budget, cross-project, and deterministic
ranking tests pass.

## Historical V10 Phase 9 — Repository mental model and read-only review

### Changes

- Extend compact `get_repo_map` with a fixed-budget knowledge/intent/uncertainty
  section; tree/full modes expand by existing budgets rather than dumping prose.
- Extend `ask` orientation plus bounded knowledge backlinks in file/symbol context.
  These additions do not alter code search result sets or document frecency.
- Replace independent map/context live/outline/temporal recaptures with one captured
  `PublishedSourceSet` per selected `ProjectInstance`, then its selected immutable
  source generations, per call.
- Add read-only `review_knowledge` for bounded source-cited remediation dossiers.
  It reuses the Phase 7 evidence/voice model and cannot reserve idempotency or mutate
  policy.
- Give code precedence only for checked current-implementation claims. A mismatch
  against intent, ADR, governance, security, or north-star evidence remains an
  implementation gap, not stale-document proof.

### Exit gate

Map/ask/context capture, role coverage, old-correct/new-wrong, mixed-section,
future-intent, ADR divergence, explicit supersession, exact duplicate, stale policy,
metadata conflict, age-only, shallow-history, review-hash, and no-mutation fixtures
pass with deterministic voice and exact evidence.

## Historical V10 Phase 10 — Guarded logical curation

### Changes

- Add preview-first `curate_knowledge` for atomic ledger-only changes against the
  Phase 7 policy model. Apply requires manifest/policy/document guards plus
  idempotency and can target only the selected current worktree.
- Before mutation, durably sync a canonical pending intent with request hash and
  exact pre/post ledger digests. Under a tested platform contract, use guarded temp
  `write_all`, file `sync_all`, atomic replace, durable parent-directory commit, then
  durable completion. Recovery accepts only exact pre-image or post-image; a third
  state conflicts. Expose apply only when durable per-project replay and the complete
  file-plus-parent atomic-durability contract are available; no best-effort fallback.
  Unix uses same-directory rename plus parent-directory sync. Windows uses temp
  `FlushFileBuffers` plus write-through same-directory replacement. A first-use
  same-directory capability probe and crash suite gate availability; failure disables
  apply before reservation. Preview and review remain usable.
- Reject move/delete mutations. Deletion remains a conservative proposal with
  duplicate/successor/unique-content/protected-role/backlink/dirty-state evidence
  for a separate user-approved repository edit.
- Add `symforge-knowledge-hygiene` prompt: audit -> focused code evidence -> proposal
  -> explicit approval -> guarded policy apply. It never applies on review alone.
- Rebuild/publish authority, map, bridge, reverse links, health, and search voice
  atomically after code, document, policy, or temporal-signal changes.

### Exit gate

Preview/no-write, stale-guard, concurrent-curator, idempotent replay, crash-after-
intent/temp-sync/file-sync/replace/completion, unsupported-durability, wrong-source,
and no-file-delete fixtures pass with exact pre/post recovery and zero partial policy.

## Historical V10 Phase 11 — Worktrees and local refs

### Changes

- Treat each active worktree as a source with its own manifest/generation under
  the existing daemon `ProjectInstance`; do not duplicate its index into a sibling.
- Enumerate admitted local branch refs through existing `git2` boundaries.
- Inspect tree/blob size before content; deduplicate immutable raw bytes by object ID.
  Reuse parse/extraction only for matching classification/route/extractor versions,
  and secret-scan results only for matching path-policy inputs/policy version.
- Never fetch or invoke LFS smudge/materialization.
- Map one parsed blob to multiple labeled sources.
- Resolve bridge/authority evidence only within each source. A ref's policy ledger
  comes from that ref tree; current-worktree curation never writes another source.
- Reconcile ref/worktree topology and movement.
- Publish each source through the registry-owned closed runtime root. A multi-source
  read acquires a sealed selection receipt and exact Current generation for every
  selected source; any unavailable member returns the per-source refusal set rather
  than a mixed-freshness success envelope.
- Keep local refs as an independently bounded P1 lane. Its failure or memory cap
  cannot block current-worktree readiness, and advertised `source_scope` values
  expand only with the gate that implements them.

### Exit gate

Current-worktree precedence, divergent variants, identical-blob dedupe, strict
multi-source refusal evidence, ref movement, giant blob, bounded mapping memory,
and no-network/LFS tests pass.

## Historical V10 Phase 12 — Health, corpus, and release gates

- Add manifest/resource/derived-coverage/disposition/retry/reconciliation/authority-
  hygiene fields to health.
- Add source-binding mode, state-placement class, persistence capabilities, and
  `.gitignore` hygiene to health, including per-session membership authority,
  control-state durability, and replay postcondition status. An unsafe launch remains
  responsive/unbound; explicit protected indexing and memory-only operation are
  labeled, not errors.
- Run the real-repository knowledge corpus from `quickstart.md`.
- Compare one-call results/tokens to broad discovery + direct reads.
- Run all focused tests, format, Clippy, serial all-target suite, and exact embed gate.
- Run adversarial implementation review (Rust, recovery/concurrency, security,
  minimalist/tool UX) and resolve accepted blockers.
- Update `tasks/todo.md` with receipts and measured limitations.

## File impact map

| Area | Primary files |
|---|---|
| Domain/manifest | `src/domain/index.rs` |
| Root binding/state placement/init ignore | `src/paths.rs`, `src/discovery/mod.rs`, `src/cli/init.rs`, `src/daemon.rs`, protocol/startup bind paths, snapshot and existing control-sidecar path consumers |
| Scout/admission | `src/discovery/mod.rs` |
| Bulk/stable read/publication | `src/live_index/store.rs` |
| Search scopes/ranking | `src/live_index/search.rs`, `src/live_index/trigram.rs` |
| Knowledge extraction | `src/parsing/mod.rs`, `src/parsing/config_extractors/markdown.rs` |
| Mental model/bridge/authority | `src/live_index/query.rs`, `src/live_index/graph.rs`, `src/live_index/git_temporal.rs`, one narrow knowledge-derived module |
| Lifecycle policy | one versioned `.symforge-knowledge.toml` parser/writer plus existing idempotency/edit-safety seams |
| Preventive lifecycle | `src/index_lifecycle/{authority,physical_root,mutation,registry,process_runtime,capacity,embedded,runtime,supervisor,candidate,observer,verification,query,activation,public_api}.rs` |
| Watch/reconcile | `src/watcher/mod.rs` |
| Snapshot/verification | `src/live_index/persist.rs` |
| Worktree/ref sources | `src/worktree/`, `src/git.rs`, daemon project ownership |
| MCP/tool UX | `src/protocol/search_tools.rs`, `src/protocol/tools.rs`, `src/protocol/mod.rs`, `src/protocol/smart_query.rs`, `src/daemon.rs` |
| Proof/performance | `src/index_lifecycle/loom_tests.rs`, `tests/model/`, `formal/v11/`, `benches/observed_refresh_gate_v1.rs`, `tests/fixtures/observed-refresh-v1/` |
| Surface/catalog docs | `src/cli/init.rs`, README/AGENTS/tool catalog |

## Complexity budget

Allowed new concepts:

1. source binding separated from derived-state placement;
2. metadata-only repository manifest;
3. overlapping index targets;
4. terminal file disposition;
5. one registry-owned immutable project-runtime root with closed per-source states;
6. one source-lifecycle module owning whole-authority tokens, isolated candidates,
   observer/verification state, physical-root leases, and mutation permits;
7. one process runtime and process-wide capacity domain;
8. one sealed claim/provenance/refusal algebra at every public read seam;
9. one knowledge-search tool;
10. one compact derived bridge/authority view;
11. one repo-owned lifecycle policy ledger;
12. one read-only review tool and one ledger-only curation tool so MCP mutation
   annotations and user approval remain unambiguous.

Anything else requires a demonstrated missing behavior. In particular, no new
database, background service, plugin framework, parser abstraction hierarchy,
ranking configuration matrix, persisted secondary index, internal LLM judge, or
physical document move/delete workflow is permitted in v1.

## Review gates

1. SpecKit completeness checklist.
2. Opposite-model adversarial specification review: Skeptic, Architect, Minimalist.
3. Red-oracle review before production code.
4. Review after each publication/recovery slice.
5. Final adversarial code review before completion.
