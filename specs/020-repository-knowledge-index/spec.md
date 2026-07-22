# Feature Specification: Repository Knowledge Index

**Feature Branch**: `feat/repository-knowledge-index`
**Created**: 2026-07-16
**Status**: Frozen — scoped adversarial verification passed 2026-07-17
**Input**: User requirement for a live, in-memory, repository-wide knowledge retrieval lane that remains separate from code intelligence and cannot be stalled by pathological files.

## Problem statement

Agentic coding sessions routinely create architecture notes, specifications,
plans, schema explanations, handoffs, runbooks, postmortems, and other text
artifacts. Later sessions forget those artifacts and reconstruct the same
knowledge by broad file search, repeated direct reads, source inspection, or user
questions. Worse, successive agents leave superseded plans and implementation
descriptions beside their replacements, so an old document can misdirect a new
session while still looking authoritative. SymForge already retrieves code
surgically, but currently classifies all admitted files as code and exposes no
knowledge-specific retrieval or authority contract.

The repository walk also has correctness gaps that block a trustworthy knowledge
lane: hard-skipped giant artifacts count against the byte ceiling before
admission, the watcher reads before admission, read/circuit-breaker failures can
disappear from accounting, reconciliation scans only Tier-1 files, snapshots omit
catalog dispositions, verification bypasses shared admission, and publication can
expose components from different generations.

This feature fixes the shared indexing lifecycle first, activates a separate
knowledge target over the existing in-memory content/search machinery, and then
uses explicit links between the two lanes to reconcile repository claims with
current code evidence.

## Product boundary

- Code intelligence and repository knowledge are separate query scopes.
- Their bridge is a derived evidence layer: it may connect exact anchors and
  reconcile supported claims, but it never turns prose into code-search results.
- Cataloging a file does not imply reading or using its content.
- One discovery/scout pass routes a file to code, knowledge, both, or catalog-only.
- Source files remain authoritative; derived results carry exact provenance.
- Current code evidence has precedence only for claims about what the implementation
  does now. It cannot invalidate a declared proposal, north star, ADR, governance,
  security policy, or future intent; those remain separately voiced and may expose
  an implementation gap.
- Age and later code churn are review signals, not automatic proof of staleness.
- No embeddings, vector database, new knowledge-specific sidecar, second MCP, or
  generative ingest summary is required for v1. Existing transport/control-sidecar
  behavior remains a separate runtime concern.
- “All files” means every regular file inside the declared scope receives an
  auditable terminal disposition. It does not mean every byte is opened.
- Repository-owned hidden instruction/documentation trees such as `.github/`,
  `.claude/`, `.agents/`, and `.codex/` are in scope. VCS/runtime internals such
  as `.git/` and `.symforge/` are hard scope exclusions. Ignore-pruned subtrees
  remain explicit policy coverage, not silently complete content coverage.
- No automatic walk starts until one shared root-eligibility gate accepts the
  canonical project root. Home/profile roots, filesystem/drive roots, OS trees,
  broad user/system containers, and their aliases leave startup unbound. They may
  be indexed only by an explicit `index_folder(..., allow_protected_root=true)`
  request; launch CWD, environment, client roots, and init cannot imply consent.
- A protected-root override is direct, per-session authority for that exact
  `index_folder` request. Another session, reconnect, automatic replay, or process
  restart cannot inherit it; each session that needs membership must issue its own
  direct request with `allow_protected_root=true`.
- Source binding and runtime-state placement are separate decisions. A protected or
  otherwise unwritable source root never has to host `.symforge/`: explicit protected
  indexing uses collision-resistant user-local state, then degrades to live
  memory-only operation if persistence is unavailable. A failed automatic launch
  root leaves the MCP responsive so a later `index_folder` call can bind an ordinary
  accessible project without restarting the harness.
- Explicit `index_folder` binding and project-aware init idempotently add canonical
  `/.symforge/` only when a normal repository root already has `.gitignore`. They
  never create it. Automatic startup, scout, watcher, and reconciliation remain
  read-only; all indexing hard-excludes runtime state regardless of ignore hygiene.

## User Scenarios & Testing

### User Story 1 — A pathological artifact cannot prevent repository readiness (Priority: P0 — SAFETY, MVP)

An agent opens a repository that contains ordinary source and documentation next
to multi-gigabyte GGUF, safetensors, checkpoint, archive, database, or dataset
files. SymForge catalogs the artifacts using metadata, does not fully read/hash/map
or parse them, and still indexes the useful source and knowledge.

**Independent Test**: Create a sparse artifact larger than the existing global
byte limit beside one Rust file and one README. Instrument content reads. Indexing
must reach Ready, the artifact must receive exactly one hard-skip/catalog-only
disposition with zero full reads and zero admitted-byte charge, and both useful
files must be queryable in their respective scopes.

**Acceptance Scenarios**:

1. **Given** a file whose size/path metadata makes it terminal catalog-only,
   **When** scouting runs, **Then** no bounded probe or full read is attempted.
2. **Given** many giant artifacts, **When** their combined disk size exceeds the
   admitted-byte ceiling, **Then** only catalog entry/metadata ceilings apply; file
   payload sizes consume no catalog-metadata or admitted-byte budget.
3. **Given** a content-ingest candidate over the admitted-byte ceiling, **When**
   the scout finalizes the manifest, **Then** the next generation is refused or
   degraded explicitly while the previous valid generation remains intact.
4. **Given** unavailable metadata, **When** the scout evaluates a path, **Then**
   size never defaults to zero; the path receives an explicit unavailable issue.
5. **Given** a repository over the catalog-entry ceiling, **When** scouting
   reaches the bound, **Then** the candidate generation is refused/degraded with
   incomplete coverage and the previous valid generation remains intact; a
   truncated manifest is never labeled complete.
6. **Given** paths or diagnostics whose bounded descriptors would exceed the
   catalog-metadata ceiling, **When** scouting reaches the bound, **Then** the
   observation is refused/degraded exactly like an entry-ceiling failure; it never
   publishes partial coverage as a canonical manifest.
7. **Given** an LLM harness launched from a home/profile, filesystem root, or OS
   directory, **When** startup root resolution runs, **Then** no scout/watcher/
   snapshot/candidate-root or per-project state creation begins; SymForge stays
   safely unbound and tells the caller to select a project root. Existing process-
   global transport/control state may be used only from a safe user-local base.
8. **Given** the same protected path in an explicit `index_folder` request with
   `allow_protected_root=true`, **When** the raw and canonical paths agree with the
   requested target, **Then** bounded indexing proceeds under a clearly labeled
   explicit-protected mode without attempting `<protected-root>/.symforge`; the
   override grants that requesting session indexing only, not init/curation or
   unrelated protected-root mutation.
9. **Given** startup remained unbound because launch CWD was protected, **When** the
   caller later indexes an accessible project root, **Then** SymForge binds, indexes,
   starts the watcher, and selects state placement normally without process restart
   or retained failure state.
10. **Given** a protected source was indexed by one session, **When** another session
    connects or the process restarts, **Then** persisted receipts/state grant no
    membership by themselves; that session remains unbound from the protected source
    until it makes a fresh direct `index_folder(..., allow_protected_root=true)`
    request.

---

### User Story 2 — Every in-scope file has an explainable disposition (Priority: P0 — TRUST)

An operator can account for every path SymForge saw, including indexed content,
metadata-only files, hard skips, unreadable files, unstable files, and work
aborted by the parser circuit breaker. No walker/read/parser failure silently
removes a path from health or recovery state.

**Independent Test**: Build a fixture containing one file for each terminal state,
inject a walk/read failure, and trip the circuit breaker. Assert one and only one
    terminal disposition per path in a complete observation and the published
    health equality:

```text
indexed + metadata_only + hard_skip + unreadable + unstable + aborted
    == discovered_catalog_entries
```

**Acceptance Scenarios**:

1. Walker errors create bounded diagnostics and degraded coverage, never omission.
2. Read failures become `Unreadable`; they do not disappear through `filter_map`.
3. Parse results after a circuit-breaker trip become `AbortedCircuitBreaker`.
4. Canonical path ordering uses `(case-folded path, exact original path bytes)`;
   case-fold collisions remain distinct, receive a bounded issue, and never fail
   the unrelated repository. A path unsafe for a platform is catalog-only with
   an owned, serializable reason.
5. Scope exclusions such as `.git/`, `.symforge/`, ignore-pruned subtrees,
   special files, and symlink escapes are visible as policy/coverage information.
6. Non-UTF-8/unrepresentable path names are never lossy-collapsed or emitted;
   they remain cataloged by stable opaque ID with a metadata-only reason.

---

### User Story 3 — Repository knowledge is found in one bounded call (Priority: P0 — VALUE, MVP)

An agent asks where the repository documents an architectural rule, schema,
requirement, past decision, runbook step, or plan. `search_knowledge` returns exact
source evidence with heading context and line pointers without polluting code
symbol/reference queries or reading every candidate document into the model.

**Independent Test**: Query this repository for “shutdown is not a persistence
boundary.” The knowledge tool must return the relevant AGENTS/recovery text and
exact line pointer in one call. `search_symbols` and code-scoped `search_text`
must not return Markdown section symbols/content for that query.

**Acceptance Scenarios**:

1. Markdown/MDX, reStructuredText, AsciiDoc, Org, plain/extensionless docs, agent
   instructions, safe configs/schemas/contracts, and safe unknown-extension UTF-8
   files are searchable with exact path and 1-based line number.
2. Markdown hits include structural heading/breadcrumb and unit line range.
3. Config/schema/contract files may be searchable in both code and knowledge.
4. Prose never appears in code-scoped symbol/reference answers.
5. Results are bounded/diversified and retain provenance when output is truncated
   or moved behind CCR retrieval.
6. No-match distinguishes complete no-evidence, degraded coverage, and evidence
   withheld by security policy.

---

### User Story 4 — Knowledge remains mechanically current (Priority: P0 — CORRECTNESS)

When a knowledge file is created, changed, renamed, deleted, locked, or written in
multiple steps, SymForge never silently serves a stale unit as current. Watcher
events use the same admission policy as cold load, while manifest reconciliation
repairs missed events and unsupported/catalog-only paths.

**Independent Test**: Suppress a create event for an unknown-extension text file,
then trigger reconciliation. The file must become searchable. Delete a
catalog-only file without an event and reconcile again; its disposition must
disappear. No stale result may claim complete/current coverage between changes.

**Acceptance Scenarios**:

1. Single-file updates perform metadata admission before bounded probe/full read.
2. A stable read verifies pre/post state and content hash; a changing file is
   retried a bounded number of times then marked unstable.
3. A logical file update changes content, targets, catalog state, and derived
   indices in one generation publication.
4. Remove events clear indexed and catalog-only state.
5. Watcher overflow, `need_rescan`, restart/fresh instance, project retarget, and
   policy/topology changes trigger full manifest reconciliation.
6. Reconciliation discovers creates/deletes/renames and is idempotent for equal
   complete canonical manifests.
7. Degraded coverage schedules bounded reconciliation with backoff until it
   converges to Complete or remains explicitly degraded; equal entry digests do
   not suppress recovery while coverage is degraded. A later watcher overflow,
   access-state change, policy/topology change, or other uncertainty signal always
   re-triggers repair; explicit degradation is never a silent permanent stop.
8. A failed observation may retain last-valid bytes only behind a degraded wrapper
   with distinct publication/content generations; it cannot serve them as current.

---

### User Story 5 — Restart and recovery preserve the same truth (Priority: P0 — RECOVERY)

SymForge may load a local snapshot to reduce work, but it does not report Ready
until the current repository/source identity, scope, and admitted content have been
verified. A
snapshot round trip preserves catalog dispositions, targets, knowledge results,
and source identity. Corrupt or incompatible state is quarantined.

**Independent Test**: Index Tier-1, metadata-only, hard-skip, generic prose, and
unreadable fixture paths; checkpoint; restart; verify unchanged disk. Before and
after state must have the same canonical manifest and query results. Verification
must not fully read the metadata-terminal artifacts.

**Acceptance Scenarios**:

1. Snapshot format includes a versioned/checksummed manifest and dispositions.
2. A snapshot header binds project, repository, stable source location, source
   version, manifest, admitted-content, and available Git-history fingerprints;
   a path key or `ProjectId` alone is never sufficient identity.
3. Startup candidate snapshots are not advertised as current or allowed to
   overwrite state before strong identity and content verification.
4. Verification uses the shared scout/admission/stable-read pipeline.
5. Replacing a repository at the same path rejects/quarantines the prior snapshot
   rather than inheriting it through an unchanged placement key.
6. A verifier captured for source/generation A cannot mutate source/generation B.
7. Failed rebuild/verification leaves the last valid published generation intact.
8. Readers observe one consistent root/generation across live content, catalog,
   knowledge, health, and outline.

---

### User Story 6 — Knowledge across worktrees and local refs stays labeled (Priority: P1)

An agent can discover relevant knowledge from another active worktree or admitted
local branch without confusing it with the current checkout. Identical Git blobs
are deduplicated internally, divergent variants remain distinct, and current
working-tree evidence ranks first.

**Independent Test**: Create two linked worktrees with divergent versions of the
same architecture section and a third local branch sharing one identical blob.
Search must show two distinct variants with source labels, map the shared blob to
both sources without duplicate parsing, and rank the current worktree first.

**Acceptance Scenarios**:

1. Source identity contains repository, worktree/ref, commit/working-tree state,
   path, and content hash/object ID.
2. Checked-out worktrees use filesystem scout/watcher state independently.
3. Local refs use `git2` object size before blob loading and never fetch/smudge.
4. Giant Git blobs remain catalog-only and do not materialize.
5. Ref movement/topology changes reconcile source mappings deterministically.
6. Reflogs, stashes, deleted refs, submodule contents, and remote-only refs are
   out of v1 scope and reported as such.
7. Each source owns an immutable manifest/generation. An all-source query snapshots
   the existing project/worktree registry and captures each selected immutable
   source set at query start; it reports coverage, digest, and generation per
   source, with overall coverage equal to the worst member.
8. Local-ref ingestion is P1 and independently bounded. Failure or memory pressure
   in that lane cannot block current-worktree P0 readiness.

---

### User Story 7 — Sensitive repository content is never emitted as knowledge (Priority: P0 — SECURITY)

Credential-bearing files may exist inside a repository, but knowledge retrieval
must not emit secret values. Sensitive path policy applies before content
ingestion where possible. Stable admitted bytes pass one deterministic, versioned,
bounded detector before publication; detector-positive/indeterminate files become
metadata-only and their transient bytes are discarded. A final defense-in-depth
guard withholds the whole hit when any externally visible field is detected; it
never rewrites a value inside an “exact” excerpt.

**Independent Test**: Place representative credential-file names and placeholder
token patterns beside safe `.env.example` documentation. Actual credential files
must be catalog-only; safe templates may return exact approved content, while any
detector-positive candidate is withheld as a whole.
No test log, snapshot, diagnostic, or MCP response may contain the placeholder
secret value.

The fixture value is assembled only at runtime and assertions report booleans or
safe rule IDs, never the value. The closed-world guarantee is policy-versioned:
no byte range identified by policy V may enter published content, MCP/CCR output,
analytics, diagnostics, logs, or snapshots. Unknown secret formats remain a
detector defect rather than a claim that arbitrary secrets are recognizable.

---

### User Story 8 — First contact yields an evidence-backed repository mental model (Priority: P0 — VALUE, MVP)

Before the first edit, an agent can request a low-detail repository map or ask for
orientation and immediately see the code structure/hotspots together with organized
knowledge entry points: architecture, ownership/governance, decisions/invariants,
schemas/contracts, operations, testing/security, active/declaratively-statused
plans, and handoffs. Every item is exact source evidence, not a generated summary.

**Independent Test**: On this repository, one bounded `get_repo_map`/`ask`
orientation call returns the AGENTS architecture/mission, ownership/governance
artifacts when present, current SpecKit/plan entry points, recovery invariants, and
existing code hotspot signals with file/line/source/generation pointers. It also
states unavailable/degraded sources and facets with no declared evidence; it does
not invent owners, active status, or architecture conclusions.

**Acceptance Scenarios**:

1. A deterministic knowledge outline groups existing files/sections by declared
   role and returns exact title/heading/excerpt pointers.
2. `get_repo_map` combines that outline with existing code topology/hotspot signals;
   no new code-analysis pipeline is introduced.
3. `ask` routes orientation intent to the combined view and can then use
   `search_knowledge` for a focused follow-up.
4. Declared document status is reported only when source text provides it; absent
   status/ownership is `unknown`, never inferred.
5. Divergent worktree/ref variants remain separate cards. “Conflict” means
   multiple source variants, not a semantic claim that one is wrong.
6. Coverage, withheld/unreadable counts, freshness, and missing-role evidence are
   visible in the same bounded response.
7. Building the outline stores references to existing indexed spans/content; it
   never persists generated summaries or duplicate document bodies.
8. The bridge resolves only explicit source evidence—repository links/paths,
   unambiguous exact symbols, declared ownership rules—and records ambiguous or
   missing anchors as uncertainty instead of guessing.
9. Knowledge hits can point to code anchors, and file/symbol context can return
   bounded knowledge backlinks, while code search/symbol result sets remain
   strictly code-scoped.
10. Bridge links resolve within the same source generation by default; a document
    from one ref/worktree never silently links to code from another.

---

### User Story 9 — Stale documentation cannot masquerade as current truth (Priority: P0 — TRUST)

An agent can ask SymForge to review repository knowledge before relying on it.
SymForge separates current implementation evidence, intentional/future direction,
historical material, and unresolved testimony. For supported claims about what the
current implementation does, code evidence has precedence; no such precedence
applies to intent, ADR, governance, security policy, or north-star claims. The
review returns a remediation dossier with exact document/code evidence instead of
a context-free “stale score.”

**Independent Test**: Create (a) an old implementation guide that names a removed
symbol, (b) a declared north-star proposal whose feature is not implemented, (c) a
spec explicitly superseded by a successor, and (d) an exact duplicate of a newer
retained file. After code and watcher updates, one bounded review must classify the
removed-symbol unit as code-diverged, the proposal as intent with an implementation
gap, the superseded spec as non-current, and only the exact duplicate as a
high-confidence deletion candidate. Default current-implementation retrieval must
not let the diverged or superseded units speak as current truth, while an explicit
historical scope can still retrieve them.

**Acceptance Scenarios**:

1. Lifecycle, authority, verification, and voice are separate typed axes at the
   smallest supported knowledge unit; one bad section does not condemn a whole
   document.
2. Deterministic divergence requires proof such as a missing exact path/symbol or a
   mismatch from a supported structured extractor. Document age, filesystem mtime,
   and code commits after the document are review signals only.
3. A declared proposal, deferred plan, ADR, ideation, or north star remains intent;
   code disagreement is reported as an implementation gap, never stale-doc proof.
4. Explicitly archived/superseded and code-diverged current-implementation units
   are excluded from default current voice. Intent remains visible but labeled;
   history requires an explicit authority scope.
5. `review_knowledge` returns document/source/content generation, last-change
   evidence and history coverage, relevant code changes, exact bridge anchors,
   inbound links, confidence basis, and the smallest proposed action.
6. Remediation actions are `keep`, `update`, `relabel_intent`, `merge`,
   `mark_superseded`, `archive`, `deletion_candidate`, or `needs_review`. Age alone
   can produce only `needs_review`.
7. A deletion candidate is never automatic. The strongest tier requires an exact
   duplicate/reproducible artifact or explicit supersession plus exact successor
   coverage, no protected role, and no unresolved live backlink.
8. Repository-wide lifecycle decisions live in one versioned, reviewable policy
   file and bind to the exact document content hash. A changed document invalidates
   its old decision rather than inheriting suppression silently.
9. `curate_knowledge` is preview-first. Apply requires explicit selected actions,
   current manifest and per-file hashes, an idempotency key, and current-worktree
   routing. It is available only with durable per-project replay and atomic durable
   ledger replacement; it never mutates a local ref or another worktree implicitly.
10. Archive/supersede apply changes only the repo policy ledger. Physical move or
    deletion is proposal-only in this feature and requires a separate user-approved
    repository edit outside the hygiene tool.
11. The evidence bridge, authority view, and reverse backlinks rebuild inside the
    same atomic source publication as code and knowledge changes.
12. A `symforge-knowledge-hygiene` prompt exposes the read-review-approve-apply
    workflow to agents and users; it cannot silently cross the approval boundary.

## Edge cases

- zero-byte files and files without a final newline;
- CRLF, LF, UTF-8 multibyte/BOM text, unsupported UTF-16/legacy encodings, and
  invalid UTF-8;
- Setext/ATX headings, nested headings, duplicate headings, fenced `#` lines,
  frontmatter, tables, links, and very large sections;
- same-size/same-mtime rewrites and replacement during an open read;
- locked files, transient not-found, permission changes, and path renames;
- symlink loops/escapes, sockets/devices, non-UTF-8 names, case-fold collisions,
  very deep paths;
- huge sparse files, deceptive extensions, Git LFS pointers, and compressed data;
- gitignored/generated/vendor trees and tracked files rescued from heuristics;
- watcher event storms, overflow, coalesced renames, restart, and project retarget;
- snapshot crash points, corruption, version mismatch, and partial temp writes;
- automatic versus explicit protected roots; raw/canonical alias disagreement;
  unreadable project-local and user-local state; memory-only restart; failed retarget;
  nested global-state self-exclusion; optional ignored team-artifact export;
- identical/differing documents across worktrees and refs;
- broad queries, exact identifiers, vocabulary mismatch, and result-budget overflow;
- shallow Git history, clock-skewed commit timestamps, renames beyond the bounded
  history window, working-tree-only documents, and code changed after a doc;
- mixed-purpose documents, stale lifecycle entries after content changes, exact
  duplicates with different backlinks, and interrupted cleanup transactions.

## Functional Requirements

- **FR-001**: A single metadata-first scout MUST define repository scope for cold
  load, `index_folder`, watcher, reconciliation, verification, and ref ingestion.
- **FR-002**: The scout MUST use file metadata and deterministic policy before
  content access and MUST never substitute size zero for failed metadata.
- **FR-003**: Every in-scope regular file MUST receive exactly one terminal
  disposition; only indexed dispositions carry exactly one invariant target variant:
  `Code`, `Knowledge`, or `CodeAndKnowledge`. An empty ingest target is invalid and
  catalog-only dispositions carry no target.
- **FR-004**: Catalog-entry count, catalog-metadata bytes, in-flight bytes, and
  admitted-content bytes MUST be independent; metadata-terminal payload sizes MUST
  consume zero admitted bytes and only bounded path/descriptor metadata. Exhausting
  the catalog-entry or catalog-metadata budget MUST abort the candidate observation
  before a `RepositoryManifest` exists; no partial manifest may publish. A previously
  valid generation remains queryable only behind the degraded wrapper of FR-009,
  while cold start remains non-Ready with a typed capacity reason and zero queryable
  partial generation. The exact reasons are `CatalogEntryCapacityExceeded` and
  `CatalogMetadataCapacityExceeded`; budget-attempt issues never form a published
  partial manifest.
- **FR-005**: Bounded probe and full-read ceilings MUST be explicit and enforced
  before allocation/read; a read larger than the total in-flight budget MUST become
  terminal `HardSkip(PerFileCeiling)`. No archive/model/database deserialization is allowed.
- **FR-006**: Full reads MUST be bounded and stable-verified; only stable admitted
  bytes receive a computed hash and become queryable. Filesystem admission MUST
  compare two bounded reads/hashes (second pass may stream) plus stable metadata,
  retrying finitely before `UnstableDuringRead`.
- **FR-007**: Walker/read/parser/circuit-breaker failures MUST remain accounted for
  with bounded non-sensitive diagnostics. Canonical parse status MUST be the closed
  `Parsed`/`PartialParse`/`Failed` enum; diagnostic text is operational only and
  cannot enter the manifest digest. Circuit breakers MUST be scoped per source,
  ingestion lane, and stage; a trip degrades only that scope and schedules repair.
- **FR-008**: A complete generation MUST atomically publish live content, catalog,
  code/knowledge targets, derived search state, health, outline, source identity,
  captured source version (including closed working-tree state), and project
  generation. Every lane MUST commit under one per-`ProjectInstance` writer boundary,
  copy the current source map under that lock, replace only its own source entry, and
  swap once. A long off-lock build whose own source base changed MUST rebase/retry or
  abort; another source's registry swap MUST NOT invalidate or starve it. P1-only
  publication MUST NOT advance the current worktree's publication/content/project
  generations. No lane may check-then-swap stale map state.
- **FR-009**: A failed next content build MUST NOT modify the previous valid
  content generation. It MUST atomically publish a degraded freshness wrapper that
  references last-valid content, advances only publication generation, and cannot
  label that evidence current.
- **FR-010**: Watcher single-path updates MUST use the shared scout/admission/read
  logic and MUST update/remove all lanes atomically.
- **FR-011**: Reconciliation MUST diff complete manifests, not only Tier-1 paths,
  and MUST be authoritative after missed/ambiguous watcher state. Degraded walks
  MUST retry with bounded backoff and cannot use equal-digest as a no-op. Any
  `Unreadable`/`UnstableDuringRead` entry makes coverage Degraded and retains a
  bounded re-observation trigger until it is replaced by a stable disposition. A
  later uncertainty signal MUST restart repair even after bounded backoff has
  settled into explicit degradation.
- **FR-012**: Snapshots MUST persist/restore the canonical manifest and dispositions,
  and verification MUST use shared policy with source/generation fencing. Each
  candidate MUST carry and verify a strong `SnapshotSourceIdentity` comprising
  project, repository, stable source location, source version, manifest digest,
  indexed-content digest, and available Git-history fingerprint. Path placement or
  `ProjectId` alone MUST NOT authorize Ready, overwrite, or restore; a different
  repository at the same path is foreign state. Source version MUST represent
  working-tree state as `Clean`, `Dirty`, `NotApplicable`, or `Unknown`; no branch,
  timestamp, or state flag may substitute for the exact manifest/content digests.
  A background verifier commit MUST also match its captured base publication,
  content, and project generations; a newer source publication forces rebase/retry
  or abort.
- **FR-013**: Markdown/prose MUST be excluded from code search/symbol scopes.
- **FR-014**: Safe textual content MUST be searchable through one
  `search_knowledge` full-surface read tool with exact provenance. Search hits MUST
  return compact deterministic authority display plus stable finding/rule/link IDs
  and bounded anchor previews; full evidence arrays and bridge records remain
  available through `review_knowledge` and are not duplicated into CCR search state.
- **FR-015**: The default full surface MUST increase from 36 to exactly 39 tools via
  `search_knowledge`, `review_knowledge`, and `curate_knowledge`. The compact-3
  surface MUST remain exactly `symforge`, `symforge_edit`, and `status`; its facade/
  `ask` routing MAY reach knowledge retrieval without advertising another compact
  tool, but no-match classes MUST remain successful responses and routing ships only
  after the existing facade decode/mapping reliability gate passes.
- **FR-016**: Knowledge ranking MUST be deterministic, source-aware, diversified,
  and must not bump frecency.
- **FR-017**: Results MUST expose generation and coverage/freshness state and MUST
  never label unverifiable evidence current/full. Every per-source response envelope
  MUST carry the captured source identity and source version, including closed
  working-tree state. Per source, publication and content generations MUST be
  distinct when last-valid content is retained after a failed observation.
- **FR-018**: Sensitive-path entries MUST remain catalog-counted with a typed
  `SensitivePath` reason and no content bytes. Detector-positive files use typed
  `SensitiveContent`, lose every content target, and persist only safe rule IDs and
  counts. Safe repository-relative locations may appear in health; if a path field
  itself detects positive it is replaced by an opaque catalog ID. Every external
  field and the user query are guarded before MCP/CCR/analytics use.
- **FR-019**: Worktree/ref variants MUST remain labeled; identical immutable blobs
   MAY share raw bytes. Parse/extraction reuse MUST also match classification, route,
   and extractor version; secret-scan reuse MUST match path-policy inputs and policy
   version. Source mappings and every source-derived authority field remain distinct.
- **FR-020**: Indexing MUST perform no network fetch and MUST not trigger Git LFS
  object materialization.
- **FR-021**: Health MUST expose the manifest accounting equality, bytes by stage,
  dispositions/reasons, retry/reconciliation state, and snapshot verification.
- **FR-022**: Existing code-intelligence behavior for admitted source files MUST
  remain compatible unless the old behavior incorrectly treated prose as code.
- **FR-023**: A recognized working-tree Git LFS pointer MUST be catalog-only with
  typed `LfsPointer` reason and bounded declared OID/size metadata; pointer text
  MUST NOT enter knowledge search.
- **FR-024**: Stable bytes MUST pass the versioned secret policy before entering
  any target or snapshot. Positive/indeterminate scans fail closed to metadata-only
  and discard transient bytes/hash; the rule may not persist matched material.
- **FR-025**: Path identity MUST be lossless. A path that cannot be represented as
  safe normalized UTF-8 MUST be cataloged by opaque stable ID, never lossy text,
  and MUST remain outside content targets.
- **FR-026**: UTF-8 and UTF-8 BOM are the v1 searchable text encodings. Other or
  invalid encodings MUST remain cataloged with `UnsupportedTextEncoding`; no lossy
  decode/re-encode may create evidence.
- **FR-027**: The published repository outline MUST include a deterministic,
  source-cited knowledge map and uncertainty envelope; `get_repo_map`/`ask` MUST
  combine it with existing code topology/hotspot signals without generated claims.
- **FR-028**: A derived bidirectional bridge MUST connect safe knowledge spans to
  exact code/file anchors only from explicit resolvable evidence. Ambiguous/missing
  anchors MUST remain typed uncertainty; bridge discovery MUST NOT contaminate code
  search scopes, cross source generations, or frecency signals.
- **FR-029**: Each supported knowledge unit MUST carry separate lifecycle,
  authority-domain, code-evidence, and retrieval-voice states. Whole-document
  aggregation MUST retain mixed-unit evidence and MUST NOT suppress unaffected
  units because one unit diverged.
- **FR-030**: Code reconciliation MUST be rule-bound and exact. Missing path/symbol
  anchors and supported structured-value mismatches MAY prove implementation drift;
  age, mtime, birth/creation time, later commits, and unresolved semantics MUST NOT.
- **FR-031**: Temporal evidence MUST distinguish filesystem birth/creation and
  modification hints, Git first-seen/last-touch commits, working-tree changes, and
  relevant code changes since the document. Every timestamp/history answer MUST
  expose provenance and coverage (including shallow/window-limited/unavailable).
  Every async job and coalesced pending-latest marker MUST capture the live content
  generation and exact source-version commit/tip at scheduling. A completion is
  accepted only when its analyzed target equals that marker and the current live
  target. Stale results MUST be rejected and coalesced into one bounded latest-state
  recomputation. Accepted derived-only publication MUST carry that exact commit/tip
  consistently in the bundle, manifest, temporal snapshot, and response envelope
  while content generation and manifest/content digests remain unchanged.
- **FR-032**: Code disagreement with a declared normative/intent unit MUST be an
  implementation gap. Only current-implementation claims may become code-diverged;
  explicit archived/superseded units and proven divergent units have no default
  current voice, while intent remains separately labeled and retrievable.
- **FR-033**: `search_knowledge` MUST accept an optional authority scope. Its
  `default` MUST include current, declared intent, review-required, and unknown
  evidence with labels, while excluding suppressed/history-only units; explicit
  history/all scopes MAY retrieve non-current evidence without promoting it.
- **FR-034**: One read-only full-surface `review_knowledge` tool MUST return bounded,
  deterministic remediation dossiers with exact knowledge/code anchors, temporal
  evidence, backlinks, rule IDs, uncertainty, and proposed actions. It MUST NOT
  generate semantic conclusions unsupported by a deterministic rule or caller
  review.
- **FR-035**: Remediation confidence and action eligibility MUST be rule-derived.
  Age alone can only request review. A deletion candidate MUST expose the retained
  successor/duplicate, unique-content and backlink checks, protected-role checks,
  and every unmet precondition.
- **FR-036**: Repository lifecycle decisions MUST use one versioned repo-owned
  `.symforge-knowledge.toml` policy ledger whose entries bind to exact safe path and
  content hash. Hash mismatch makes the entry stale/review-required; it MUST NOT
  keep suppressing changed content. Inline declared status MAY be read as evidence
  but MUST NOT create a second mutable authority.
- **FR-037**: One mutating full-surface `curate_knowledge` tool MUST be preview-first
  and idempotent. Apply MUST require an explicit action list, current manifest
  digest, per-document hash guards, idempotency key, and current-worktree target.
  Before ledger mutation it MUST durably record and sync a canonical pending intent
  containing request hash plus exact pre-image and intended post-image digests. The
  pending intent and replay record MUST also bind verified `RepositoryId`/`SourceId`
  plus a continuity proof; `ProjectId` or placement path alone is insufficient. Git
  continuity requires the recorded object format and anchor tip to remain resolvable
  as a commit in the live object database. Non-Git continuity requires unchanged
  platform root-object identity and an unbroken durable catalog lineage. Recovery and
  stored-success replay MUST verify that identity predicate before ledger inspection
  or result replay; current ref/tip/history movement alone is drift, not foreign
  identity. Guarded manifest/policy digests remain first-execution freshness guards
  and MUST NOT participate in source-sameness comparison or block same-key/same-hash
  terminal replay. A failed continuity proof MUST return a typed foreign-source
  conflict, quarantine attributable intent, write nothing, and never report the
  foreign result as applied. The ledger writer MUST use guarded temp `write_all`, file
  `sync_all`, atomic replace, and durable parent-directory commit under a tested
  platform contract, then durably mark completion. Recovery MUST finalize an observed
  exact post-image, retry only an
  unchanged exact pre-image, and refuse any third state as a typed conflict. Apply
  capability MUST be unavailable unless durable per-project replay and that complete
  tested file-plus-parent atomic-durability contract are available; no best-effort
  weakening is allowed. Preview/review remain usable. A
  completed update MUST be watcher/reconciliation visible.
  The platform contract is normative: Unix requires same-directory replacement plus
  parent-directory sync; Windows requires temp `FlushFileBuffers` plus write-through
  same-directory replacement. Only after normal-current-worktree, writable-source,
  and durable replay/intent requirements are `Available` may first apply use probe
  each directory receiving durable curation records: the ledger parent and the
  `ProjectStateDir` replay/intent-journal parent. Either failed probe makes apply
  unavailable before reservation. Explicit-protected, read-only, ref, implicit-
  worktree, and memory-only bindings MUST return their typed reason with zero probe
  file operations anywhere under the source root. The platform crash suite MUST pass.
- **FR-038**: `curate_knowledge` MUST NOT move or delete repository files. Physical
  cleanup remains a proposal with explicit protected-role/backlink/unique-content/
  dirty-state preconditions for a later user-approved repository edit; no review or
  archive action may imply deletion.
- **FR-039**: Knowledge roles, bridge links, temporal/authority evidence, and reverse
  backlinks MUST be derived from one captured source generation, rebuilt/published
  atomically, and independently bounded with explicit truncated coverage. Hash-valid
  suppression/proven-divergence evidence has reserved priority; if a limit still
  cannot represent it, affected units fail closed to voice `Suppressed`, remain out
  of default/current, stay retrievable through history/all, and expose canonical
  skipped-suppression IDs plus truncated coverage.
- **FR-040**: Existing orientation/context surfaces MUST expose bounded authority
  evidence: `get_repo_map` and `ask` show current/intent/hygiene summaries,
  `get_file_context`/`get_symbol_context` show exact knowledge backlinks, and the
  `symforge-knowledge-hygiene` prompt orchestrates review then explicit approval.
- **FR-041**: One canonical source-root decision MUST guard auto-start,
  workspace-env/client-root binding, daemon/session open, `index_folder`, init,
  watcher/reconciliation, and snapshot verification before traversal. It MUST
  classify both raw and canonical paths and apply the stricter result. Automatic and
  init candidates that resolve to filesystem/drive roots, home/profile roots,
  OS/sensitive trees, broad containers, or symlink/extended-path aliases MUST remain
  unbound even when they are Git repositories. Only an explicit `index_folder`
  request with `allow_protected_root=true` may bind that exact protected canonical
  root. Missing/non-directory targets, device/special namespaces, and targets whose
  canonical identity cannot be established remain non-indexable even with override.
  The override creates membership only for the requesting session; every other
  session and every post-restart session requires its own fresh direct request.
- **FR-042**: When no automatically eligible root exists, SymForge MUST serve an
  explicit unbound health/readiness state with zero indexed content and actionable
  project-selection guidance. It MUST NOT silently choose an unrelated source,
  start a project watcher, create candidate-root/per-project state, or advertise
  complete no-evidence. The same live process MUST accept a later valid
  `index_folder` request and transition from unbound to Ready without retaining the
  earlier root/state error.
- **FR-043**: One shared ignore-hygiene operation MUST run after a successful explicit
  normal `index_folder` binding and during project-aware `symforge init`, after a
  repository-mutation capability check. When root `.gitignore` exists
  and does not effectively ignore root `.symforge/`, the operation MUST append canonical
  `/.symforge/` idempotently with
  a guarded atomic write while preserving existing bytes/line-ending style. When
  `.gitignore` is absent, the operation MUST do nothing and MUST NOT create it. A
  permission/race failure MUST be reported but MUST NOT roll back or disable the
  already valid live index.
- **FR-044**: Automatic startup, scout, watcher, reconciliation, verification, and
  ref ingestion MUST NOT mutate `.gitignore`. Every source path MUST hard-exclude
  `.symforge/` independently of Git ignore state. Health/tool receipts MUST report
  `effective`, `missing_rule`, `no_root_gitignore`, `unverifiable`, or
  `not_applicable_explicit_protected`, with init remediation for the missing-rule
  case only when repository-init capability is available.
- **FR-045**: Runtime-state placement MUST be selected only after source binding and
  MUST NOT be a prerequisite for live indexing. An ordinary writable project uses
  `<project>/.symforge`; explicit-protected mode MUST skip that path entirely, and
  any project-local access/creation failure MUST fall back to a user-local
  per-project directory keyed by a versioned digest of the canonical root identity.
  If user-local persistence is also unavailable, SymForge MUST continue with an
  in-memory generation and explicitly disable persistence-dependent capabilities.
- **FR-046**: A state-placement fallback MUST move only derived SymForge state, never
  retarget the source being indexed. Global state directories MUST be private to the
  current user, collision-resistant across repositories/worktrees, and preserve the
  same snapshot/quarantine/integrity rules. Snapshot headers MUST verify the strong
  source identity defined by FR-012. When either the resolved `ProjectStateDir` or
  `ControlStateDir` lies inside the indexed source (for example user-local state
  while explicitly indexing a home tree), each absolute canonical subtree MUST be
  excluded from scout, watcher, reconciliation, and verification to prevent self-
  indexing/feedback loops. Health MUST expose source
  mode, state placement class, persistence availability, and disabled capabilities
  without requiring callers to infer them from an I/O failure.
- **FR-047**: Explicit-protected mode grants bounded read/index authority for the
  exact requested root only. It MUST NOT enable init, repository-policy curation,
  source mutation, or protected-root `.symforge`/`.gitignore` writes. Normal
  read-only project roots remain indexable through global or memory-only state, but
  repository mutations MUST refuse with an explicit capability reason.
- **FR-048**: A rejected/failed retarget MUST leave any existing project binding,
  watcher, and published generation untouched. An unbound process remains unbound;
  a bound process keeps serving its prior source. Explicit-protected authority is a
  non-transferable result created at the direct `index_folder` boundary and MUST NOT
  be inherited by another session, reconnect, client-root, environment, init,
  automatic replay, session-open input, or process restart. A session may join an
  already-live protected project only after its own matching direct request succeeds.
- **FR-049**: Placement is stable for a project-instance lifetime. A later snapshot,
  quarantine, checkpoint, or state-write failure MUST degrade persistence health
  without changing source identity or live query readiness. `Ready` MUST report
  query readiness independently from durability and watcher freshness. Durable
  cross-restart `index_folder` idempotency uses global control state; when unavailable
  replay remains process-local and is labeled non-durable. A stored completion
  receipt is historical evidence, not a live postcondition: same-key/same-hash replay
  MUST re-establish or verify the requested binding and requesting-session membership
  before returning `applied=true`. If the source or required project instance cannot
  be reconstructed, the call MUST return a successful typed
  `live_postcondition_unavailable` result with `applied=false`; replay with the same
  key and a different hash MUST fail deterministically.
- **FR-050**: Existing process-global transport/control state is separate from
  per-project `StatePlacement`, and state ownership MUST be closed and exhaustive.
  The canonical source owns source/Git reads, watcher roots, relative paths, and
  guarded repository writes. `ProjectStateDir` owns snapshot/temp/quarantine/reset/
  checkpoint, per-project replay and mutation intent, edit-safety TEE, frecency/
  coupling/STEL, analytics, API-key state, and derived cleanup. `ControlStateDir`
  owns the edit-safety trust store, sidecar port/PID/session descriptors and status
  readers, daemon discovery/control and runtime-startup coordination, hook adoption/
  hint state, operator profile, onboarding state, version registry/updater, and cross-
  project `index_folder` replay/locks. Each reader and writer MUST receive the same typed
  owner; no consumer
  may reconstruct state from source root, launch CWD, a rejected candidate, or a
  relative `.symforge`. If no safe private user-local control base exists, that
  control feature MUST be process-local or explicitly unavailable. Root rejection
  itself creates no per-project state, and both state-directory subtrees are excluded
  under FR-046 when nested in the source.
- **FR-051**: The existing opt-in team artifact under
  `<project>/.symforge/index.bin.zst` is not runtime-state fallback and MUST NOT be
  redirected to user-local storage. Because `.symforge/` is ignored by default, its
  export receipt MUST disclose exactly one Git visibility state:
  `already_tracked`, `untracked_visible`, `ignored_force_add_required`, or
  `git_visibility_unavailable`; it MUST NOT infer shareability when Git visibility
  cannot be established. Export is allowed only for a normal writable
  current project with repository-mutation capability; explicit-protected,
  read-only, user-local-only, and memory-only bindings MUST refuse before artifact
  or `.gitattributes` mutation. Feature 020 does not silently relocate or retire the
  compatibility artifact.
- **FR-052**: Persistence-only tools MUST distinguish unavailability from transport
  failure. In user-local failure or memory-only mode, `checkpoint_now` MUST return a
  successful typed tool result with `applied=false` and a reason-bearing unavailable
  status; it MUST NOT return a stale success receipt or force callers to infer
  capability from an MCP error.

## Non-Functional Requirements

- **NFR-001 Determinism**: Equal logical repository state produces equal canonical
  manifest ordering/digest and equal search ordering.
- **NFR-002 Memory safety**: An in-flight permit covers read, verification, parse,
  and hand-off, then releases when ownership transfers into the staged index.
  Staged/indexed residency is governed separately by the admitted-content ceiling;
  an in-flight budget smaller than that ceiling is legal and MUST NOT deadlock.
- **NFR-003 Availability**: One bad file cannot poison unrelated code/knowledge;
  incomplete coverage is explicit.
- **NFR-004 Recovery**: Crash/corruption leaves either the previous valid generation
  or an explicit source rebuild path.
- **NFR-005 Latency**: Cold-start scout/verification latency is acceptable for
  correctness; optimization may not weaken readiness/freshness guarantees.
- **NFR-006 Security**: For secret-policy version V, no byte range detected by V
  may be published, persisted, logged, analyzed, cached, or emitted. Detection is
  pure/local/deterministic/bounded and failure is closed.
- **NFR-007 Local-first**: Serving remains in-process and memory-resident; local
  snapshots are derived/rebuildable state.
- **NFR-008 Hygiene safety**: No age threshold, score, model judgment, or broad
  “cleanup all” request may directly mutate or recycle repository files. Every
  applied decision is reviewable, hash-guarded, recoverable, and source-scoped.

## Success Criteria

- **SC-001**: Sparse artifacts larger than the old global byte cap do not prevent
  Ready and receive zero full reads in instrumented tests.
- **SC-002**: The terminal-disposition equality holds for every fixture and real
  repository index, including injected failures.
- **SC-003**: Watcher/reconciliation tests recover missed create/delete/rename and
  catalog-only changes with zero silent stale results.
- **SC-004**: Snapshot/source builds return the same logical manifest and knowledge
  answers for unchanged sources; replacing a repository at the same path fails
  strong source-identity verification and cannot publish or overwrite from the old
  snapshot.
- **SC-005**: Concurrent publication stress observes zero mixed-generation bundles.
- **SC-006**: Real-repository knowledge corpus queries find the correct source
   pointer in one call; the corpus median returned-token count is at least 50% lower
   than the recorded broad-discovery-plus-direct-read baseline.
- **SC-007**: Code search/symbol tests contain zero prose-only hits while config
  overlap tests remain available in both intended scopes.
- **SC-008**: Worktree/ref tests preserve variant labels and deduplicate identical
  blobs without network/LFS materialization.
- **SC-009**: Runtime-assembled secret-safety canaries emit no value in output,
  logs, analytics, diagnostics, CCR, or serialized snapshots; policy mismatch
  forces re-scout before Ready.
- **SC-010**: Formatting, Clippy, focused suites, serial all-target tests, and the
  exact CI embed gate pass before completion.
- **SC-011**: One bounded first-contact call returns source-cited entry points for
  every declared knowledge role present plus explicit unknown/degraded roles, with
  no unsupported ownership/status/architecture assertion.
- **SC-012**: Bridge fixtures resolve exact path/unique-symbol links in both
  directions, expose ambiguous/missing links without guessing, and remove/repair
  backlinks atomically after source changes.
- **SC-013**: Mixed-purpose stale-doc fixtures suppress only proven divergent units,
  preserve declared intent, and report timestamp/history provenance without any
  age-only archive or deletion proposal.
- **SC-014**: A real-repository deep review returns code-backed remediation dossiers
  in one bounded call, including exact anchors and actionable uncertainty, while
  default orientation/search contains zero explicitly superseded or proven-diverged
  claims voiced as current implementation truth.
- **SC-015**: Lifecycle apply rejects stale manifest/file hashes and idempotency
  conflicts; a changed archived document re-enters review instead of remaining
  silently suppressed. Crash injection after intent sync, temp write, file sync,
  atomic replace, and completion recording recovers only the exact pre/post states;
  any third state conflicts, and absent durable replay/atomic durability disables
  apply while preview remains available.
- **SC-016**: Attempted move/delete input to `curate_knowledge` is rejected; deletion
  candidates remain evidence-only and include every protected-role, backlink,
  uniqueness, dirty-state, and source-ownership blocker.
- **SC-017**: Windows/Unix/WSL/UNC/extended-prefix/symlink fixtures prove every
  automatic/init entry point leaves home, drive/filesystem root, OS, and broad
  container candidates unbound before any source walk/candidate-root/per-project
  state write, while ordinary nested project roots remain accepted. Unsafe harness
  startup stays responsive and a later accessible `index_folder` request reaches
  Ready in the same process.
- **SC-018**: Explicit normal `index_folder` and project-aware init fixtures prove
  a missing root rule in an existing `.gitignore` is added once, equivalent rules are
  no-ops, CRLF/LF/BOM/no-final-newline bytes are preserved except for the bounded
  append, concurrent change refuses without disabling the live index, and absent
  `.gitignore` remains absent; automatic paths never mutate and indexing never
  admits `.symforge/`.
- **SC-019**: An explicit System32/protected-root fixture with
  `allow_protected_root=true` reaches a queryable live index and records user-local
  or memory-only placement while an instrumented filesystem proves zero state or
  durability-probe create/inspect/write/delete operations anywhere beneath the
  protected source root, including `<protected-root>/.symforge`. A second session and
  a restarted process receive no inherited membership; each succeeds only after its
  own direct override request.
- **SC-020**: Injected project-local and user-local persistence failures leave live
  code and knowledge queries usable with explicit memory-only health;
  `checkpoint_now` returns a successful typed `applied=false` unavailable result,
  and subsequent reindexing of a writable project restores normal placement without
  restart.
- **SC-021**: Failed retarget and alias tests preserve the prior binding/watcher/
  generation, while nested `ProjectStateDir` and `ControlStateDir` trees are absent
  from manifest, knowledge, code, watcher events, reconciliation, and verification
  under an explicitly indexed protected parent. Instrumented state consumers prove
  every reader/writer uses its typed owner and never launch CWD or a source-derived
  fallback.
- **SC-022**: Team-artifact tests prove standard init ignores `.symforge/`, explicit
  normal-project export reports each of `already_tracked`, `untracked_visible`,
  `ignored_force_add_required`, and `git_visibility_unavailable` exactly, and
  protected/read-only/non-project-local placements write neither artifact nor
  `.gitattributes`.
- **SC-023**: Durable and process-local `index_folder` replay tests prove a stored
  receipt never substitutes for a live binding/session membership: replay with the
  same key and hash re-establishes the postcondition or returns successful typed
  `applied=false`/`live_postcondition_unavailable`, while a changed request conflicts.

## Assumptions

- The current `LiveIndex`, trigram index, line rendering, Markdown section spans,
  worktree support, `git2`, snapshots, and CCR are reusable seams.
- Repository knowledge is primarily textual; binary office/PDF/OCR ingestion is
  not required for v1.
- Exact lexical/structural retrieval delivers the first measurable value. Semantic
  embeddings are justified only by later relevance evidence.

## Out of scope

- embeddings/vector databases and generative ingest summaries;
- remote fetch, remote-only refs, reflog/stash archaeology;
- OCR, PDF/office conversion, archive expansion, model/dataset parsing;
- submodule-content traversal;
- separate MCP/server or new knowledge-specific sidecar/control plane;
- automatic or hygiene-tool file movement/deletion; feature 020 applies lifecycle
  policy only and leaves physical cleanup to an explicit later repository edit;
- unrestricted semantic truth judgment. SymForge may prove only versioned,
  rule-supported code divergence; unsupported prose remains unresolved for an
  evidence-backed agent/user review.
