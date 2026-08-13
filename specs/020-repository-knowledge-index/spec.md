# Feature Specification: Repository Knowledge Index

**Feature Branch**: `design/project-activation-prevention`
**Created**: 2026-07-16
**Status**: V11 refreeze candidate — lifecycle-prevention design cleared 2026-08-11; implementation remains gated by the refreeze approval record
**Input**: User requirement for a live, in-memory, repository-wide knowledge retrieval lane that remains separate from code intelligence and cannot be stalled by pathological files.

## V11 lifecycle-prevention amendment

This refreeze supersedes the V10 publication/readiness semantics wherever they
conflict with the rules below. Historical review receipts remain evidence of the
work performed, but they do not authorize a V11 implementation.

1. Only a COMPLETE verified generation is queryable (F020-V11-A20). `Current` is,
   and so is what a `Refreshing` RELOAD retains; a refresh that issued a mutation
   permit stays unqueryable until a successor `Current` installs. `Loading` holds
   none. `Blocked` and `Stopping` retentions are recovery evidence, never a lane.
2. Cold placeholders, snapshot seeds, candidates, capacity-refused attempts, and
   failed observations cannot mint query authority. They expose a closed
   `SourceRefusal` with safe diagnostics until one complete generation promotes.
3. Promotion requires a complete canonical manifest and complete certificates for
   every advertised strict scope. `Unreadable`, `UnstableDuringRead`,
   `AbortedCircuitBreaker`, `ParseStatus::Failed`, strict-scope `PartialParse`, an
   observer gap, overdue proof, incomplete discovery, or truncated required
   derivation rejects and discards the candidate; required truncation remains
   bounded attempt-only evidence, is never queryable, and never mutates the retained
   generation.
4. Circuit-breaker and resource-admission failures produce bounded
   `AttemptAccounting`, cancel/discard the candidate, and publish non-current work
   state. Attempt data cannot populate committed manifest, digest, coverage, or
   source-truth fields.
5. Every generation-backed MCP tool, resource, prompt, hook, sidecar route, CCR,
   checkpoint, snapshot read, and V11 embed operation acquires strict leases through
   the same lifecycle Interface. A complete no-match claim requires one `Current`
   lease for every source in its sealed selection receipt. Pure `DiskObservation`,
   complete `WorktreeScopeObservation`, `GitObservation`, and runtime health remain
   independently authoritative while a source is non-Current. `DiskObservation` may
   prove path-local `PathMissing`, `WorktreeScopeObservation` may prove completeness
   of its one sealed declared scope/interval, and `GitObservation` may prove
   `NotInTree` for one exact tree. None may claim generation membership, generation
   completeness, or unqualified repository-wide absence. A mixed lane requires one
   operation-specific, identity-compatible `ClaimContext` naming every authority
   input.
6. `authority_scope` is a `KnowledgeVoiceFilter` inside the selected current
   generation. It cannot select generation consistency; the wire value `current`
   means the current-implementation voice, not lifecycle `Current`.
7. Persistence health is orthogonal to source truth: a durability failure alone
   does not revoke a valid `Current` generation. Missing/gapped observation,
   incomplete baseline, unknown ordering, or an overdue verification obligation
   does revoke strict acquisition synchronously. A verification obligation becomes
   overdue at the finite monotonic deadline defined by FR-049; partial progress,
   cancellation, restart, and cursor resume cannot extend that deadline.
8. V11 activates once, across every entry path, only after the refreeze manifest,
   detached attestation, externally anchored approval, closed public-API manifest,
   causal RED oracles, capacity proof, and `ObservedRefreshGateV1` pass. No
   default-on refusal-per-edit intermediate or legacy/raw embed fallback ships.

Every SymForge-owned repository-content write, including curation and root-ignore
hygiene, obtains one non-cloneable `SourceMutationPermit`. Grant publishes
non-Current before any side effect; all destructive path resolution and I/O is
component-confined and handle-relative through the permit's pinned
`PhysicalRootLease`. Commit, failure, rollback, and a valid no-side-effect proof can
return the binding to `Current` only through a fresh complete candidate at the latest
observer cut. State-directory persistence writes remain outside source-content
mutation authority and do not revoke `Current` by themselves. A cold/pending
`index_folder`, project-aware init, or restart path is read-only and cannot mint a
permit: it must first promote one complete Current generation. Only a fresh permit
granted after that promotion may authorize pre-image retry/cleanup/probe or source-
byte I/O. If recovery observes the exact post-image, finalizing its completion record
under `ProjectStateDir` is persistence-only and requires no source permit.

### Amendment traceability clauses

- **F020-V11-A01 — runtime failure, not degraded generation**: Failure publishes a
  closed non-current runtime state; retained generation bytes and identity do not
  change. Regression: `F020-V11-R01`.
- **F020-V11-A02 — cold start is nonqueryable**: `retained=None` plus a placeholder,
  seed, refusal, or build cannot grant a lease. Regression: `F020-V11-R02`.
- **F020-V11-A03 — generation consumers are strict-current**: Every existing
  generation-backed surface uses the same strict acquisition Interface. Health and
  pure disk/worktree-scope/Git observations use their own typed authority, including
  legal path-local `PathMissing`, sealed-scope completeness, and exact-tree
  `NotInTree`; every mixed response uses a compatible `ClaimContext`. Regression:
  `F020-V11-R03`.
- **F020-V11-A04 — promotion completeness matrix**: Every observation and artifact
  required by the advertised strict scope must be complete. Regression:
  `F020-V11-R04`.
- **F020-V11-A05 — certificates never authorize partial promotion**: A capability
  certificate attests one complete generation or the capability is not advertised.
  Regression: `F020-V11-R05`.
- **F020-V11-A06 — circuit-breaker candidate discard**: A trip cancels and discards
  the candidate without a canonical manifest. Regression: `F020-V11-R06`.
- **F020-V11-A07 — trust-first availability**: One bad file may make the source
  unavailable, but cannot publish partial, stale, or mixed state. Regression:
  `F020-V11-R07`.
- **F020-V11-A08 — attempt accounting is noncanonical**: Aborted/refused attempts
  expose bounded diagnostics with no manifest/digest/completeness authority.
  Regression: `F020-V11-R08`.
- **F020-V11-A09 — partial parse is not strict code truth**: `PartialParse` and
  `Failed` remain attempt diagnostics and cannot promote strict code scope.
  Regression: `F020-V11-R09`.
- **F020-V11-A10 — derived work is promotion-bound**: Required temporal, bridge,
  authority, and mental-model artifacts complete inside the candidate. Regression:
  `F020-V11-R10`.
- **F020-V11-A11 — required truncation blocks promotion**: Required derived
  truncation discards the candidate, remains attempt-only, and is never queryable,
  cached, persisted, or CCR-addressable. Truncation is permitted only when rendering
  a response after a complete lease. Regression: `F020-V11-R11`.
- **F020-V11-A12 — protected-root readiness survives**: An authorized protected root
  may reach Current through user-local/memory-only placement with zero state/probe I/O
  beneath the source root. Regression: `F020-V11-R12`.
- **F020-V11-A13 — no-match needs an all-Current selection**: Global absence requires
  a sealed selection/generation bijection; otherwise return per-source refusal.
  Regression: `F020-V11-R13`.
- **F020-V11-A14 — first contact needs Current evidence**: A map/orientation cannot
  omit an unavailable selected source or convert it into missing-role evidence.
  Regression: `F020-V11-R14`.
- **F020-V11-A15 — manifest equality is committed-only**: SC-002 applies only to a
  promoted Current manifest; attempt accounting has its own invariant. Regression:
  `F020-V11-R15`.
- **F020-V11-A16 — health separates truth from attempts**: Committed-generation and
  attempt-diagnostic fields cannot populate one another. Regression:
  `F020-V11-R16`.
- **F020-V11-A17 — durability is orthogonal, observation is not**: Persistence-only
  failure preserves Current; gap/unknown/overdue observation revokes it. Regression:
  `F020-V11-R17`.
- **F020-V11-A18 — observed refresh activation gate**: Delta equivalence, latency,
  convergence, and charged peak residency pass in the indivisible activation cut.
  Regressions: `F020-V11-R18A`, `F020-V11-R18B`, `F020-V11-R18C`.
- **F020-V11-A19 — strict refusal plus voice-only authority scope**: The four public
  contracts use `SourceRefusal`; `authority_scope` is only `KnowledgeVoiceFilter`.
  Regressions: `F020-V11-R19A`, `F020-V11-R19B`.

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
  never create it. Cold admission, observer installation, discovery, and first
  candidate construction are read-only and must promote a complete Current generation
  before ignore hygiene may request a fresh `SourceMutationPermit`. The write-capable
  phase then publishes non-Current before side effects and returns through a fresh
  candidate even when final revalidation proves no write occurred. Automatic startup,
  scout, watcher, and reconciliation remain read-only; all indexing hard-excludes
  runtime state regardless of ignore hygiene.

## User Scenarios & Testing

### User Story 1 — A pathological artifact cannot prevent repository readiness (Priority: P0 — SAFETY, MVP)

An agent opens a repository that contains ordinary source and documentation next
to multi-gigabyte GGUF, safetensors, checkpoint, archive, database, or dataset
files. SymForge catalogs the artifacts using metadata, does not fully read/hash/map
or parse them, and still indexes the useful source and knowledge.

**Independent Test**: Create a sparse artifact larger than the existing global
byte limit beside one Rust file and one README. Instrument content reads. Indexing
must reach lifecycle `Current`, the artifact must receive exactly one hard-skip/catalog-only
disposition with zero full reads and zero admitted-byte charge, and both useful
files must be queryable in their respective scopes.

**Acceptance Scenarios**:

1. **Given** a file whose size/path metadata makes it terminal catalog-only,
   **When** scouting runs, **Then** no bounded probe or full read is attempted.
2. **Given** many giant artifacts, **When** their combined disk size exceeds the
   admitted-byte ceiling, **Then** only catalog entry/metadata ceilings apply; file
   payload sizes consume no catalog-metadata or admitted-byte budget.
3. **Given** a content-ingest candidate over the admitted-byte ceiling, **When**
   admission accounts the candidate, **Then** no canonical manifest is minted,
   the candidate is discarded, strict acquisition is refused with a typed capacity
   cause, and any retained verified generation remains byte-identical and non-current.
4. **Given** unavailable metadata, **When** the scout evaluates a path, **Then**
   size never defaults to zero; the path receives an explicit unavailable issue.
5. **Given** a repository over the catalog-entry ceiling, **When** scouting
   reaches the bound, **Then** attempt accounting records the refusal, no candidate
   generation or truncated manifest can promote, and any retained verified
   generation remains internal and non-current.
6. **Given** paths or diagnostics whose bounded descriptors would exceed the
   catalog-metadata ceiling, **When** scouting reaches the bound, **Then** the
   observation is refused exactly like an entry-ceiling failure; it never publishes
   partial coverage as a canonical manifest or a degraded generation.
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

An operator can account for every path SymForge saw. A promoted manifest contains
only indexed content and complete metadata-only/hard-skip terminal dispositions;
unreadable, unstable, partial/failed parse, and circuit-breaker-aborted paths appear
only in bounded attempt health. No walker/read/parser failure silently disappears or
enters committed-generation equality.

**Independent Test**: Build a fixture containing one file for each promotable
terminal disposition and assert one and only one disposition per path in a complete
promoted observation:

```text
indexed + metadata_only + hard_skip
    == discovered_catalog_entries
```

Separately inject walk/read instability, partial/failed parse, and a circuit-breaker
trip; assert the candidate is discarded and each path remains in bounded
`AttemptAccounting` with no committed manifest, digest, equality, or query authority.

**Acceptance Scenarios**:

1. Walker errors create bounded attempt diagnostics and prevent candidate promotion;
   they never become omissions or committed degraded coverage.
2. Read failures become attempt disposition `Unreadable`, prevent strict-scope
   promotion, and do not disappear through `filter_map`.
3. Parse results after a circuit-breaker trip become bounded attempt disposition
   `AbortedCircuitBreaker`; the candidate and its would-be manifest are discarded.
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
6. A no-match result is `no_evidence_complete` only when a sealed selection receipt
   has one `Current` lease for every selected source. Any non-current selected source
   returns `SourceRefusal::SelectionUnavailable`; security withholding remains a
   distinct, explicitly attributed outcome and cannot establish absence.

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
7. Incomplete observation publishes a non-current work state and schedules bounded
   supervised reconciliation with backoff; equal entry digests do not suppress the
   required proof. A later watcher overflow, access-state change, policy/topology
   change, or uncertainty signal always re-triggers authoritative observation.
8. A failed generation- or candidate-critical observation leaves any retained verified
   generation byte-identical and internal. Every generation-backed public consumer
   receives `SourceRefusal` until a complete successor generation promotes. Pure
   root-bound disk, complete worktree-scope, Git, and runtime-health observations
   remain available only under their own non-generation authority; their own failure
   returns typed refusal without changing lifecycle state unless it independently
   proves observer-seam invalidation.

---

### User Story 5 — Restart and recovery preserve the same truth (Priority: P0 — RECOVERY)

SymForge may load a local snapshot to reduce work, but it does not publish `Current`
until the current repository/source identity, scope, and admitted content have been
verified. A
snapshot round trip preserves catalog dispositions, targets, knowledge results,
and source identity. Corrupt or incompatible state is quarantined.

**Independent Test**: Index Tier-1, metadata-only, hard-skip, and generic prose
fixtures whose required reads all succeed; checkpoint, restart, and verify unchanged
disk. Before and after state must have the same canonical manifest and query results.
Verification must not fully read metadata-terminal artifacts. Separately make one
required path unreadable during source build or snapshot verification: the candidate
must be discarded, any retained generation stays internal/non-current, and strict
acquisition refuses. Restore access and prove bounded re-observation promotes a fresh
complete candidate with source-build/snapshot parity.

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
6. A verifier captured with `SourcePublicationToken` and `GenerationAuthority` for A
   cannot mutate B even when their numeric diagnostic epochs are equal. Numeric
   binding/publication/content/project epochs are health evidence only and never a
   verifier fence.
7. Failed rebuild/verification leaves the retained verified generation intact and
   non-current; strict acquisition refuses until a complete successor promotes.
8. Readers capture one `ProjectRuntimePublication` root and the exact selected
   `GenerationAuthority` values across live content, catalog, knowledge, health, and
   outline; no second ArcSwap or diagnostic counter participates.

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
7. Each source owns an immutable manifest/generation. A generation-backed all-source
   query acquires one sealed selection with an exact bijection to `Current`
   generation leases; any non-Current member returns `SourceRefusal` and no partial
   aggregate. A pure Git-object observation may still succeed independently with
   `GitObservation`, but it cannot establish generation membership/completeness or
   repository-wide absence; exact-tree `NotInTree` remains legal.
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
existing code hotspot signals with file/line/source/generation pointers. It refuses
with per-source evidence when any selected source is non-current; from a complete
Current selection it may state facets with no declared evidence but does not invent
owners, active status, or architecture conclusions.

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
6. Coverage and policy-withheld counts are visible in the same bounded response only
   from a complete Current selection. Attempt/unreadable diagnostics appear in a
   `SourceRefusal`, never as missing-role evidence in a successful orientation.
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
5. `review_knowledge` returns the document/source `GenerationAuthority` and content
   identity, last-change evidence and history coverage, relevant code changes, exact bridge anchors,
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
   The write-capable phase must hold one `SourceMutationPermit`, publish non-Current
   before I/O, and perform confined handle-relative replacement through that
   permit's pinned root.
10. Archive/supersede apply changes only the repo policy ledger. Physical move or
    deletion is proposal-only in this feature and requires a separate user-approved
    repository edit outside the hygiene tool.
11. The evidence bridge, authority view, and reverse backlinks rebuild inside the
    same complete successor candidate as code and knowledge changes. Commit, failure,
    rollback, and no-side-effect terminal paths cannot restore `Current` directly.
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
  before a `RepositoryManifest` exists; no partial manifest may publish. A retained
  verified generation remains byte-identical and non-current, while cold start stays
  non-queryable. Strict acquisition returns a typed capacity refusal. The exact
  reasons are `CatalogEntryCapacityExceeded` and
  `CatalogMetadataCapacityExceeded`; budget-attempt issues remain bounded
  `AttemptAccounting` and never form a published manifest.
- **FR-005**: Bounded probe and full-read ceilings MUST be explicit and enforced
  before allocation/read; a read larger than the total in-flight budget MUST become
  terminal `HardSkip(PerFileCeiling)`. No archive/model/database deserialization is allowed.
- **FR-006**: Full reads MUST be bounded and stable-verified; only stable admitted
  bytes receive a computed hash and become queryable. Filesystem admission MUST
  compare two bounded reads/hashes (second pass may stream) plus stable metadata,
  retrying finitely before `UnstableDuringRead`.
- **FR-007**: Walker/read/parser/circuit-breaker failures MUST remain in bounded,
  non-sensitive attempt accounting. Canonical parse status MUST be the closed
  `Parsed`/`PartialParse`/`Failed` enum; diagnostic text is operational only and
  cannot enter the manifest digest. `Unreadable`, `UnstableDuringRead`,
  `AbortedCircuitBreaker`, `Failed`, or strict-code-scope `PartialParse` rejects and
  discards the candidate. Circuit breakers remain scoped for cancellation and
  diagnosis, but no aborted scope may publish a degraded generation.
- **FR-008**: `ArcSwap<ProjectRuntimePublication>` MUST be the sole query/publication
  root. One immutable whole-project root contains membership plus every source's
  closed runtime state; a complete source generation contains live content, catalog,
  code/knowledge targets, derived search state, health evidence, outline, source
  identity, and captured source version (including closed working-tree state). Every
  lane prepares a sealed source delta off-lock, then under the one per-
  `ProjectInstance` writer exact-matches its expected source publication against the
  latest whole root, patches only that source entry, preserves every latest sibling,
  mints a new never-reused `ProjectRuntimePublication` identity, and performs one
  ArcSwap store. A long build whose own source base changed MUST rebase/retry or abort;
  an unrelated sibling update MUST NOT invalidate it. Updating a P1 sibling therefore
  advances the whole-project publication identity while leaving the unchanged current
  worktree's exact `GenerationAuthority` untouched. Numeric publication/content/
  project generations and epochs are diagnostic only and MUST NOT fence, authorize,
  or identify publication. No lane may read or swap a second public root, compose a
  hybrid view, or check-then-swap stale map state.
- **FR-009**: A failed next build MUST NOT modify the retained verified generation.
  The lifecycle owner MUST publish a closed non-current runtime state with bounded
  attempt diagnostics and `SourceRefusal`; retained content remains internal and no
  public consumer may acquire or label it as current.
- **FR-010**: Watcher single-path updates MUST use the shared scout/admission/read
  logic and MUST update/remove all lanes atomically.
- **FR-011**: Reconciliation MUST diff complete manifests, not only Tier-1 paths,
  and MUST be authoritative after missed/ambiguous watcher state. Incomplete walks
  publish non-current work state, retry under lifecycle-owned bounded supervision,
  and cannot use equal-digest as a no-op. An `Unreadable` or
  `UnstableDuringRead` observation prevents promotion and retains an independent
  re-observation trigger. A later uncertainty signal MUST restart repair; no bounded
  retry exhaustion may silently convert incomplete evidence into Current.
- **FR-012**: Snapshots MUST persist/restore the canonical manifest and dispositions,
  and verification MUST use shared policy with source/generation fencing. Each
  candidate MUST carry and verify a strong `SnapshotSourceIdentity` comprising
  project, repository, stable source location, source version, manifest digest,
  indexed-content digest, and available Git-history fingerprint. Path placement or
  `ProjectId` alone MUST NOT authorize lifecycle `Current`, overwrite, or restore; a different
  repository at the same path is foreign state. Source version MUST represent
  working-tree state as `Clean`, `Dirty`, `NotApplicable`, or `Unknown`; no branch,
  timestamp, or state flag may substitute for the exact manifest/content digests.
  A background verifier commit MUST exact-match its opaque captured
  `SourcePublicationToken` and `GenerationAuthority`; a newer or different source
  publication forces rebase/retry or abort. Numeric binding, observer, publication,
  content, project, or runtime epochs are diagnostic health evidence only and MUST
  NOT authorize verifier adoption, overwrite, or publication.
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
- **FR-017**: Every successful result carrying `GenerationAuthority`, or deriving any
  fact from generation structure, MUST be built from a sealed Current lease and carry
  its complete generation, binding, source, scope, operation, and provenance
  identities. A source with no COMPLETE generation MUST return the closed
  `SourceRefusal` envelope instead of a stale/degraded success; per F020-V11-A20 a
  `Refreshing` RELOAD still holds one and leases it, while a refresh that issued a
  mutation permit does not until a successor installs. Pure root-bound
  `DiskObservation`, complete `WorktreeScopeObservation`, `GitObservation`, and
  runtime health do not require Current and remain independently authoritative within
  their closed authority: disk may prove path-local `PathMissing` from its retained
  final-parent handle, a sealed worktree-scope receipt may prove completeness for its
  declared scope/interval, and Git may prove `NotInTree` for one exact tree. These
  authorities MUST NOT claim generation membership, generation completeness, or
  unqualified repository-wide absence. Any response combining them with generation
  structure MUST be constructed from one identity-compatible operation
  `ClaimContext` that names every authority input and refuses a cross-root/rebind
  mixture.
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
- **FR-021**: Health MUST expose committed-generation evidence separately from
  bounded attempt accounting: bytes by stage, safe causes, retry/reconciliation
  state, snapshot verification, and runtime work state. Attempt fields MUST NOT
  populate committed digest, equality, coverage, or source-truth fields.
- **FR-022**: Existing code-intelligence behavior for admitted source files MUST
  remain compatible unless it treated prose as code or served `PartialParse`/
  `Failed` evidence as a complete strict code scope. Those parse outcomes remain
  candidate diagnostics until a separately reviewed capability contract exists.
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
  Temporal artifacts required by an advertised strict scope MUST be built and
  verified inside the isolated candidate and committed only with that generation's
  completeness certificate. A stale async completion is discarded and coalesced
  into bounded successor work; it cannot mutate a Current generation in place or
  publish an independently current derived root.
- **FR-032**: Code disagreement with a declared normative/intent unit MUST be an
  implementation gap. Only current-implementation claims may become code-diverged;
  explicit archived/superseded units and proven divergent units have no default
  current voice, while intent remains separately labeled and retrievable.
- **FR-033**: `search_knowledge` MUST accept an optional wire `authority_scope`,
  parsed internally as `KnowledgeVoiceFilter`. Its `default` MUST include the
  current-implementation voice, declared intent, review-required, and unknown
  evidence with labels while excluding suppressed/history-only units; explicit
  history/all values select voices inside the same Current generation. This filter
  MUST NOT select lifecycle consistency or authorize a retained generation.
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
  foreign result as applied. Before any source-content side effect, apply MUST obtain
  one non-cloneable `SourceMutationPermit`; permit grant invalidates prior candidates
  and publishes non-Current. Every path component MUST resolve no-follow/reparse-safe
  beneath the permit's pinned `PhysicalRootLease`, and temp creation, `write_all`,
  `sync_all`, and atomic replacement MUST operate through the validated final-parent
  handle rather than a recaptured/raw path. The ledger writer MUST durably commit the
  parent directory under the tested platform contract, then mark completion. On cold
  restart, recovery may inspect durable intent read-only but MUST first promote a
  complete Current generation from the observed source. An exact pre-image retry, any
  source cleanup/probe, or source-byte write then requires a fresh
  `SourceMutationPermit`; it cannot borrow pending-startup authority. An observed exact
  post-image requires only completion finalization inside `ProjectStateDir`, which is
  persistence-only and MUST NOT acquire a source permit or make the source
  non-Current. Recovery MUST refuse any third source state as a typed conflict. Apply
  capability MUST be unavailable unless durable per-project replay and that complete
  tested file-plus-parent atomic-durability contract are available; no best-effort
  weakening is allowed. Preview/review remain usable. A
  completed update MUST schedule a fresh complete candidate. Failure, rollback, drop,
  or any side effect remains non-Current until verified promotion; even a valid
  no-side-effect proof returns through a fresh verification/no-op candidate at the
  latest observer cut and cannot restore Current directly.
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
  backlinks required by an advertised scope MUST be derived inside one isolated
  candidate from one source cut, included in its sealed artifact set, and promoted
  atomically with the generation. Truncation is permitted only for non-required
  output rendering. If a required bridge, authority, backlink, or suppression proof
  cannot be represented completely, the candidate is discarded and the truncation
  remains bounded attempt-only evidence with no query/cache/CCR/snapshot identity;
  the retained generation is unchanged and strict acquisition refuses while recovery
  proceeds.
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
  `index_folder` request and transition from unbound to lifecycle `Current` without retaining the
  earlier root/state error.
- **FR-043**: One shared ignore-hygiene operation MUST run only after an explicit
  normal `index_folder` or project-aware `symforge init` has completed read-only cold
  admission/observation and promoted one complete Current generation. Pending startup
  cannot mutate or grant a permit. After a repository-mutation capability check, the
  write-capable phase MUST obtain a fresh
  non-cloneable `SourceMutationPermit`, publish non-Current before side effects, and
  use the permit's pinned `PhysicalRootLease` plus component-by-component no-follow/
  reparse-safe, final-parent-handle-relative I/O. When root `.gitignore` exists
  and does not effectively ignore root `.symforge/`, the operation MUST append canonical
  `/.symforge/` idempotently with
  a guarded atomic write while preserving existing bytes/line-ending style. When
  `.gitignore` is absent, the operation MUST do nothing and MUST NOT create it. A
  permission/race failure MUST be reported and MUST NOT corrupt or roll back the
  retained generation. Once a permit has been granted, success, refusal, concurrent
  guard failure, or a proven no-side-effect no-op can return to `Current` only through
  a fresh complete candidate at the latest observer cut; no terminal path directly
  restores the prior publication.
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
  without changing source identity or, by itself, revoking a valid Current
  generation. Query readiness remains independent from durability but depends on
  complete live observation. A whole-declared-scope verification pass begins from a
  sealed `VerificationScopeReceipt` bound to the exact project-slot instance,
  source-slot instance, generation digest, stable observer cut, policy version, and
  declared scope. That receipt enumerates every canonical catalog entry and terminal
  disposition, admitted source-byte range, scope-discovery obligation, required
  derived artifact, and required certificate. Completion requires authoritative
  stable-cut rescans at both boundaries, an exact path/disposition bijection, every
  catalog obligation checked, every admitted byte range rehashed, every required
  derived artifact and certificate recomputed, and zero missing, extra, skipped, or
  unresolved obligations; any identity, cut, path, policy, or content drift aborts
  the pass and cannot emit a complete `VerificationRecord`.

  Under the default finite profile, `verification_bytes` is admitted-source plus
  required-artifact bytes and is at most 17,179,869,184; `verification_entries` is
  catalog, disposition, discovery, and artifact obligations and is at most 200,000.
  Current promotion requires a `VerificationFeasibilityReceipt` reserving at least
  33,554,432 verification bytes/second and 1,000 verification entries/second, with
  `ceil(verification_bytes / 33554432) + ceil(verification_entries / 1000) <= 720`
  seconds. The successor pass MUST start within 180 seconds of the prior completion,
  so even the default maximum (512 + 200 = 712 seconds) completes before the fixed
  900-second deadline. A source outside either bound, without both reservations, or
  with a computed pass bound above 720 seconds remains non-current with
  `SourceRefusal`; runtime configuration cannot extend these default ceilings or the
  deadline.

  Promotion and each later complete whole-declared-scope verification pass set
  `verification_deadline = completion_monotonic + 15 minutes` for that exact sealed
  identity. A partial slice, cancellation, retry, cursor resume, persistence failure,
  or process restart MUST NOT advance or reconstruct that deadline; restart begins
  non-current until a fresh complete proof.
  At or after the monotonic deadline, if no newer complete `VerificationRecord` is
  bound to that exact identity tuple, the source supervisor MUST atomically latch
  `VerificationOverdueLatched` before any strict lease can linearize. An
  absent/gapped observer, incomplete baseline, unknown ordering, scope-dirty marker,
  or that overdue latch MUST synchronously make strict acquisition non-current and
  return `SourceRefusal`. Only a fresh complete exact-bound verification and
  publication may clear the latch. Loss of the reserved service floor makes the
  source non-current rather than extending the deadline. Durable
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
  or `.gitattributes` mutation. Any required `.gitattributes` repository-content
  write MUST use `SourceMutationPermit` and the same pre-write non-Current,
  handle-relative confinement, and fresh-candidate terminal rules as FR-037/FR-043;
  writing the excluded team artifact itself remains a persistence operation. Feature
  020 does not silently relocate or retire the compatibility artifact.
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
- **NFR-003 Availability and trust**: One bad file cannot publish partial, stale, or
  mixed state. An observation-critical failure blocks candidate promotion and
  strict-current acquisition; a retained verified generation remains immutable, and
  internal except where F020-V11-A20 leases a reload's, with refusal evidence.
- **NFR-004 Recovery**: Crash/corruption leaves either the previous valid generation
  or an explicit source rebuild path.
- **NFR-005 Latency**: V11 defines no independent cold-start SLO: a cold source stays
  responsive for protocol/health but generation-nonqueryable until complete
  promotion, however long correctness requires. After an observed edit, visibility
  and convergence MUST meet `ObservedRefreshGateV1`/SC-024; optimization may not
  weaken authority, completeness, or freshness guarantees.
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
  lifecycle `Current` and receive zero full reads in instrumented tests.
- **SC-002**: The terminal-disposition equality holds for every promoted Current
  generation. Injected aborted/refused attempts satisfy their separate bounded
  attempt-accounting invariant and cannot claim canonical manifest equality.
- **SC-003**: Watcher/reconciliation tests recover missed create/delete/rename and
  catalog-only changes with zero silent stale results; from first invalidation until
  complete promotion, every generation-backed strict consumer refuses rather than
  serving retained evidence as current. Pure observation and health claims retain
  only their own authority.
- **SC-004**: Successful snapshot/source builds over fully readable required inputs
  return the same logical manifest and knowledge answers for unchanged sources.
  Injected unreadable/unstable/failed/partial inputs discard the candidate, remain
  attempt-only, and recover only through a fresh complete candidate after access is
  restored. Replacing a repository at the same path fails opaque-token and strong
  source-identity verification and cannot publish or overwrite from the old snapshot.
- **SC-005**: Concurrent publication stress observes zero mixed-generation bundles
  from the sole `ArcSwap<ProjectRuntimePublication>` root. Every sibling update mints
  a new project-runtime publication identity while each unchanged source retains its
  exact `GenerationAuthority`; numeric diagnostic generations cannot authorize a
  store or reproduce a hybrid view.
- **SC-006**: Real-repository knowledge corpus queries find the correct source
   pointer in one call; the corpus median returned-token count is at least 50% lower
   than the recorded broad-discovery-plus-direct-read baseline.
- **SC-007**: Code search/symbol tests contain zero prose-only hits while config
  overlap tests remain available in both intended scopes.
- **SC-008**: Worktree/ref tests preserve variant labels and deduplicate identical
  blobs without network/LFS materialization.
- **SC-009**: Runtime-assembled secret-safety canaries emit no value in output,
  logs, analytics, diagnostics, CCR, or serialized snapshots; policy mismatch
  forces re-scout before lifecycle `Current`.
- **SC-010**: Formatting, Clippy, focused suites, serial all-target tests, and the
  exact CI embed gate pass before completion.
- **SC-011**: From an all-Current sealed selection, one bounded first-contact call
  returns source-cited entry points for every declared knowledge role plus explicit
  unknown roles. If any selected source has no complete leasable generation per
  F020-V11-A20, the call returns a typed per-source refusal and no absence claim.
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
  apply while preview remains available. Every repository-content write is confined
  through a `SourceMutationPermit`; permit grant makes the source non-Current before
  I/O, and every terminal path—including a no-side-effect proof—requires a fresh
  complete candidate before Current returns. Cold recovery first promotes Current
  read-only; exact-pre-image retry/cleanup/probe/source bytes require a fresh permit,
  while exact-post-image completion finalization in `ProjectStateDir` is persistence-
  only and does not revoke Current.
- **SC-016**: Attempted move/delete input to `curate_knowledge` is rejected; deletion
  candidates remain evidence-only and include every protected-role, backlink,
  uniqueness, dirty-state, and source-ownership blocker.
- **SC-017**: Windows/Unix/WSL/UNC/extended-prefix/symlink fixtures prove every
  automatic/init entry point leaves home, drive/filesystem root, OS, and broad
  container candidates unbound before any source walk/candidate-root/per-project
  state write, while ordinary nested project roots remain accepted. Unsafe harness
  startup stays responsive and a later accessible `index_folder` request reaches
  lifecycle `Current` in the same process.
- **SC-018**: Explicit normal `index_folder` and project-aware init fixtures prove
  a missing root rule in an existing `.gitignore` is added once, equivalent rules are
  no-ops, CRLF/LF/BOM/no-final-newline bytes are preserved except for the bounded
  append, concurrent change refuses without corrupting the retained generation, and
  absent `.gitignore` remains absent; automatic paths never mutate and indexing never
  admits `.symforge/`. Cold/pending startup remains read-only until a complete Current
  generation promotes; only then may a fresh `SourceMutationPermit` publish
  non-Current and perform confined handle-relative I/O. Current returns only through
  a fresh candidate for successful and no-side-effect outcomes.
- **SC-019**: An explicit System32/protected-root fixture reaches a queryable live
  index only after its direct `allow_protected_root=true` request and records
  user-local or memory-only placement. An instrumented filesystem proves zero
  create, inspect, write, or delete operations for state or durability probes
  anywhere beneath that protected source, including
  `<protected-root>/.symforge`. Neither a second session nor a restarted process
  inherits membership; each must independently complete the same direct override
  request before it can join the live project.
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
- **SC-024**: `ObservedRefreshGateV1` passes on the pinned SymForge and maximum-
  admitted calibration corpora: completed write burst to first strict lease carrying
  that byte identity is at most 2 seconds p95, 5 seconds maximum, and 1.25 times the
  recorded baseline p95; delta output is equivalent to a clean full rebuild, a
  single-path hint does not request a full candidate unless `Gapped`/`ScopeDirty`,
  and peak retained-plus-candidate residency stays within its pre-granted vector plus
  declared scratch/headroom.
- **SC-025**: Process-capacity campaigns prove all runtimes, active/candidate/retired
  generations, query/output leases, snapshot stages, observers, journals, parser
  workers, and blocking tasks remain in one conserved capacity domain; cancellation,
  resize, panic, sharing, shutdown, and reincarnation never refund live residency.
- **SC-026**: One attested V11 activation makes every daemon, stdio, serve, embed,
  snapshot, watcher, edit, CCR/cache, resource, prompt, hook, and sidecar entry path
  use the closed lifecycle/query/claim Interfaces. The generated public-API graph has
  no legacy constructor, mutator, raw `LiveIndex`, or authority-forging export, and
  no user-selectable fallback can reactivate V10 publication semantics.

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
