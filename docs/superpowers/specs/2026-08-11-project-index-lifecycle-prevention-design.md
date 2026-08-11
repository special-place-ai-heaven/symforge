# Project Index Lifecycle Prevention — Design

**Status:** FROZEN FOR EXTERNAL REVIEW · **Date:** 2026-08-11 · **Target:** `main` at `1521abb0`

**Purpose:** Remove the architectural causes that allow an incomplete, unrooted,
or unverifiable index to become active. This is a prevention design, not another
readiness predicate or a relabeling of failure.

**Evidence base:** current source at the target commit; the cold-start round-one and
round-two review packet; the HTTP sidecar readiness review and release; frozen
Feature 020 specification and plan; focused read-only lifecycle, capacity,
publication, watcher, snapshot, and concurrency tracing.

Implementation is not authorized by this document until its conflict with frozen
Feature 020 is accepted and the design passes adversarial review.

## 1. Outcome

SymForge may become temporarily unavailable when the filesystem, resources, or
source cannot converge. It must never promote or present a partial, unrooted, or
unverifiable generation as current.

The target guarantee is:

> Under arbitrary faults, a query receives one verified generation, an explicitly
> identified last-known-good generation only under a separately reviewed future
> contract, or a refusal. Candidate and partial generations are never queryable.

This does **not** promise that SymForge is always Ready. Permanent permission
failure, disk loss, an intentionally insufficient hard limit, or a source that
never stabilizes can make readiness impossible. Renaming those states would be
dishonest. The preventable failure is promotion of untrusted state.

The first safe lifecycle is deliberately strict-current. Retained last-known-good
state is internal recovery material, not a public answer source. Capability-scoped
partial availability and public as-of reads are deferred to separate designs with
their own proof rules.

## 2. Why the current architecture creates the defect

The defect is not one missing condition. Lifecycle ownership is distributed among
startup, daemon project registration, `LiveIndex`, watcher reconciliation,
freshening handlers, snapshots, and health projection. Each caller knows part of
the ordering contract, so no module owns the whole invariant.

### 2.1 Admission refusal crosses the seam as success

`bootstrap_project_index` has an interface of `Result<SharedIndex>`, but catalog
capacity refusal is converted into `Ok(LiveIndex::empty())`
(`src/daemon.rs:3452-3538`). The caller cannot distinguish a verified index from a
resource-admission refusal.

`ProjectInstance::activate` then starts the watcher and Git temporal work for every
accepted instance (`src/daemon.rs:3193-3221`). A failed admission has therefore
already crossed the project-registration seam as a valid index before activation.

**Cause:** resource admission and active-index construction share one success type.

### 2.2 Cold loading has no lifecycle owner

The local stdio path creates and publishes `LiveIndex::empty()`, starts one detached
`spawn_blocking` reload, discards its `JoinHandle`, and then starts watcher, Git,
sidecar, and MCP consumers against the same handle
(`src/main.rs:384-619`). Failure is logged but no owner guarantees retry,
cancellation, completion, or a terminal control-plane result.

**Cause:** startup responsiveness was implemented by detaching the load rather than
by separating protocol availability from index availability.

### 2.3 The watcher is a competing loader

A fresh watcher instance immediately runs full reconciliation before consuming its
event queue (`src/watcher/mod.rs:879-1143`). During cold start, that reconciliation
shares the bootstrap handle and may admit paths before the full load has published
source identity, root, and a complete manifest.

**Cause:** watcher activation means both “observe changes” and “mutate the active
index,” even when no active generation exists.

### 2.4 Project loading is not single-flight

`ensure_project_slot_for_session_with` checks the project map, performs the complete
load outside the map lock, and only then uses `entry.or_insert`
(`src/daemon.rs:1074-1115`). Concurrent first opens can build duplicate candidates;
one is discarded after paying its full I/O, parse, and memory cost.

**Cause:** the stable project slot is created after loading rather than before it.

### 2.5 Capacity controls do not reserve process capacity

Every `admit_and_parse_entries` invocation creates a fresh `InflightByteBudget`.
Its staged ceiling is the sum of that candidate's own planned bytes
(`src/live_index/store.rs:4184-4463`). It does not reserve memory across projects or
cover coexistence of an old active generation, a replacement candidate, derived
indices, duplicate first opens, and a watcher backlog.

**Cause:** per-load safety ceilings are being used as if they were aggregate resource
admission.

### 2.6 Readiness is reconstructed from unrelated facts

Callers currently infer readiness from combinations of `is_empty`, `load_source`,
`indexed_root`, `local_empty_reason`, circuit-breaker state, snapshot verification,
`FreshnessStatus`, generation counters, and separately mutable `WatcherInfo`.
These fields have different owners and publication timing. Illegal combinations are
representable, which is why each correction has required another predicate.

**Cause:** lifecycle is not an authoritative domain state. Health is an inference
over implementation details.

### 2.7 The circuit breaker acts after the expensive work

All admitted files are read and parsed in parallel before
`fold_parse_results_for_scope` examines the completed results sequentially. When the
ratio trips, the fold discards already-computed results and publishes degraded
coverage (`src/live_index/store.rs:3997-4056`). It does not protect the expensive
phase it is intended to bound.

**Cause:** cancellation is attached to publication folding instead of work
scheduling and candidate validity.

### 2.8 Root and generation authority can split

`effective_fence_generation` assumes reload publishes the new root before advancing
`project_generation` (`src/watcher/mod.rs:255-275`). The implementation performs the
opposite under the writer lock: it increments the separate atomic counter and then
publishes the new live root (`src/live_index/store.rs:2379-2418`).

One reachable interleaving is:

1. Reload increments the generation while root A is still published.
2. A stale root-A watcher reads the new generation and old root A.
3. It adopts the new generation as a same-root reload.
4. Its mutation waits behind the reload writer lock.
5. Reload publishes root B and releases the lock.
6. The root-A mutation now holds the accepted generation and can target B.

Task cancellation does not revoke already-running blocking work. Existing tests
observe states before and after reload, not this mid-commit window.

**Cause:** mutation authority combines an independently sampled atomic generation
with a separately published root instead of one immutable binding token.

### 2.9 Publication and queries have multiple authorities

`swap_and_publish_with_content_change_and_hook` stores `live`, health, outline, and
the canonical source bundle sequentially (`src/live_index/store.rs:3197-3309`).
Some consumers read `live` directly while others capture `PublishedGeneration`.
A lifecycle redesign that adds another active pointer before migrating those
consumers would temporarily increase the number of authoritative roots and permit a
hybrid response.

**Cause:** one logical generation is available through several independently
swapped interfaces, and consumers are free to mix them.

### 2.10 Failed reload removes the recovery observer

`ProjectSlot::reload_with` stops the old watcher before building. If the build
fails, `?` returns before a replacement watcher starts
(`src/daemon.rs:3320-3374`). Last-known-good content can remain in memory while its
only source-change retry trigger has disappeared.

**Cause:** observer handoff is destructive and precedes candidate success.

### 2.11 Snapshot restoration bypasses candidate isolation

Local and daemon startup deserialize a snapshot directly into the shared live index
and verify it asynchronously. Offline edits can therefore exist before verification
completes.

**Cause:** a previously verified checkpoint is treated as already active after a new
process starts, even though source and watcher epochs have changed.

## 3. Conflict with frozen Feature 020

This design deliberately reopens part of the frozen Feature 020 contract.

- User Story 1 scenarios 3, 5, and 6 allow a candidate to be “refused or degraded.”
- User Story 2 scenario 1 expects terminal accounting to publish after failed
  observation.
- User Story 3 scenario 6 and User Story 4 scenarios 7 and 8 depend on degraded
  recovery semantics.
- User Story 8 scenario 6 depends on per-source degraded availability.
- FR-004 permits a last-valid generation only behind a degraded wrapper.
- FR-007 makes circuit-breaker trips publish degraded scope.
- FR-009 requires a failed build to publish a degraded freshness wrapper.
- FR-011 permits reconciliation to settle into explicit degradation.
- FR-017 and FR-021 expose degraded publication and health as generation facts.
- FR-022 preserves code-intelligence behavior that can currently retain partial
  parse results; the strict lifecycle refuses that generation instead.
- FR-031 permits accepted temporal evidence to publish asynchronously against the
  same content generation; the strict lifecycle treats that publication as a new
  candidate and does not leave the prior generation Current while it is pending.
- FR-039 permits bounded, explicitly truncated bridge/authority/backlink coverage;
  the first strict scope cannot promote required derived coverage when it truncates.
- NFR-003, SC-002, and SC-011 assume partial/degraded coverage can remain
  available.
- SC-019 requires an explicitly authorized protected-root fixture to become
  queryable without repository-local state or probe writes; the lifecycle must
  preserve that outcome under the stricter promotion proof.
- The plan's publication rule, Phases 1, 3, 4, 6, 7, and 9, and its complexity
  budget repeat or constrain the prior publication model.

Those decisions correctly prevent stale content from being called Current, but they
also make failure mutate the published query root and spread lifecycle state through
the data generation. The revised model keeps failure and recovery in the project
slot while leaving the active generation byte-for-byte unchanged.

Required specification amendment and re-review if this design is approved:

1. Replace “publish a degraded wrapper” with “leave the active pointer unchanged and
   publish work/failure state through an immutable runtime snapshot.”
2. Cold start with no verified generation remains non-queryable.
3. Existing public consumers remain strict-current. Retained last-known-good state
   is not publicly queryable in the first safe lifecycle.
4. A candidate may promote only when its canonical manifest and every observation
   required by the advertised query scopes are complete. `Unreadable`,
   `UnstableDuringRead`, `AbortedCircuitBreaker`, `ParseStatus::Failed`, unknown
   ordering, or truncated derived coverage prevent promotion. `PartialParse` also
   prevents strict code-scope promotion until a separately reviewed capability
   contract proves what it may answer. Metadata-terminal policy exclusions remain
   complete dispositions.
5. Capability certificates in the first lifecycle attest a complete generation;
   they never authorize partial promotion. Capability-scoped partial availability is
   deferred.
6. Circuit-breaker failure cancels and discards a candidate; it never produces a
   partially published candidate.
7. NFR-003 becomes: one bad file cannot cause partial, stale, or mixed state to
   publish. An observation-critical failure prevents candidate promotion and
   current-query acquisition; the prior verified generation remains unchanged and
   explicitly non-current.
8. Aborted attempts expose bounded accounting diagnostics but never a canonical
   committed manifest.
9. FR-022 compatibility no longer includes serving partial parses as current.
10. FR-031 temporal work, Phase 6 bridge work, Phase 7 authority work, and Phase 9
    mental-model views must either complete inside the advertised strict scope before
    promotion or be absent from the advertised protocol surface for that runtime.
    They cannot publish an incomplete current generation later.
11. FR-039 truncation in a required derived scope blocks promotion. A future
    capability-scoped contract may weaken this only after defining closed proof,
    disclosure, and invalidation rules.
12. SC-019 remains load-bearing: authorized protected roots may reach Ready under
    memory-only or user-local placement without any state or durability-probe file
    operation beneath the protected source root.

These amendments must preserve FR-008 atomic whole-generation publication, FR-010
shared single-path admission, FR-012 source identity and quarantine, FR-049's
separation of query readiness from persistence health, and SC-003 deletion and
missed-event convergence.

## 4. Selected deepening opportunity

Create one deep **source index lifecycle module** behind every source slot. Keep two
different ownership domains as separate concrete modules rather than building a God
coordinator:

- **Project registry:** root eligibility, per-session membership, stable project
  slots, slot tombstones, source-slot inventory, and close/reopen single-flight.
- **Source index lifecycle:** one candidate, observer journal, retry series,
  promotion, runtime snapshots, query leases, and stop semantics for one source.
- **Capacity pool:** process-wide persistent, scratch, journal, and headroom
  reservations whose leases follow actual allocations.

The source lifecycle interface owns these operations as one ordering contract:

- request or coalesce a refresh trigger;
- own exactly one load attempt and candidate per source slot;
- accept watcher hints into a bounded journal;
- replay and validate through an observation cursor;
- promote one verified generation atomically;
- acquire one strict-current query lease;
- report one immutable runtime snapshot;
- stop scheduling and reap owned work.

Transient `WaitingForCapacity`, `Building`, `RetryWait`, and `Blocked` states remain
inside the lifecycle module. Startup and daemon callers join a stable slot; they do
not receive transient load outcomes and decide retry ownership themselves.

The implementation hides source scanning, candidate building, retry policy, journal
catch-up, snapshot staging, publication fencing, and the use of capacity leases.
The capacity pool hides global fairness and accounting. The registry hides identity
and membership rules.

Each passes the deletion test. Deleting the source lifecycle would force startup,
watcher, snapshot, query, and health callers to relearn promotion ordering. Deleting
the registry would spread membership and tombstone races through session callers.
Deleting the capacity pool would make each loader guess at aggregate residency.

No trait hierarchy is required. These modules initially have one concrete
in-process implementation. Internal seams are justified only where two real
adapters already exist, such as filesystem observation and snapshot candidate
restoration.

## 5. Authoritative state model

The project slot owns binding/membership and a map of independently progressing
sources:

```text
ProjectSlot {
    slot_instance_id,
    binding_and_membership,
    sources: Map<SourceId, SourceSlot>,
    registry_generation,
}

ProjectRuntimeSnapshot {
    sources: Map<SourceId, SourceRuntimeSnapshot>,
    runtime_epoch,
}

SourceRuntimeSnapshot {
    active: Option<Arc<VerifiedGeneration>>,
    work: WorkState,
    observer_epoch,
    visible_journal_tail,
    acknowledged_watermark,
}
```

Lifecycle transitions copy the bounded source map under the slot writer lock and
atomically publish one `Arc<ProjectRuntimeSnapshot>`. Current worktree and local-ref
sources retain independent generations and failure state while queries capture one
coherent project source set.

`WorkState` is source control-plane state:

```text
Idle
Dirty { since_cursor }
WaitingForCapacity { request, retry_trigger }
Building { candidate_id, binding_epoch, start_cursor, attempt }
Replaying { candidate_id, through_cursor }
RetryWait { cause, attempt, retry_at }
Blocked { cause, operator_action }
Stopping
```

A `VerifiedGeneration` is immutable and contains:

- slot instance ID, project ID, binding epoch, physical root identity, canonical
  root, source identity, and observer epoch;
- captured source version;
- complete canonical manifest;
- content and derived indices;
- acknowledged observation watermark;
- content and publication identity;
- complete-scope certificates for every advertised query scope;
- capacity ownership retained until the generation's final reference drops;
- one atomic query/publication root.

Mutable call-time evidence is not part of `VerifiedGeneration`. Query acquisition
also captures one immutable `QueryOperationalSnapshot { persistent_store_version,
session_version, evaluation_time, scores }`. Persistent frecency is read through one
SQLite snapshot/transaction; session frecency is copied under one version; all decay
uses the captured evaluation time. A commitment bump linearized after capture affects
later requests only. This snapshot may order hits already authorized by the selected
generation, but it cannot establish source truth, queryability, scope completeness,
or a negative/absence claim.

There is no active `Degraded` generation.

| Active | Work | Strict-current meaning |
|---|---|---|
| None | Dirty/Waiting/Building/Replaying/RetryWait | Loading or temporarily unavailable; index reads refuse. |
| None | Blocked | No queryable generation; health names the required operator action. |
| Some | Idle | Current verified generation. |
| Some | Dirty/Waiting/Building/Replaying/RetryWait | Replacement is pending; retained active is internal and public index reads refuse. |
| Some | Blocked | Retained active is internal recovery material; public index reads refuse. |

Health and content are rendered from the same captured runtime snapshot. Content,
source, and content-generation fields come from its referenced verified generation;
availability and work fields come from that same snapshot. Health does not inspect
arbitrary `LiveIndex` fields to guess lifecycle.

The public first slice exposes no as-of mode. If a later design adds one, it must
require or acknowledge exact source/content identity and disclose it in every answer.

### 5.1 Closed strict-current scope

`StrictScopeContractV1` is fixed by the binary and is not inferred from whichever
derived jobs happened to finish. For each advertised source it contains all of:

1. source/project/repository identity, canonical and physical root identity, source
   version, binding and observer epochs;
2. authoritative repository scope, complete manifest accounting, terminal path
   dispositions, secret/path/encoding policy versions, and admitted byte identities;
3. stable source bytes plus complete code and knowledge parse/extraction state;
4. file, symbol, reference, reverse-reference, text/search, graph, outline, and
   repository-map structures used by any listed tool, resource, prompt, hook, or
   sidecar route;
5. bridge links, authority/voice state, reverse backlinks, code temporal/hotspot/
   coupling evidence, and every source-derived ranking signal consumed by an
   advertised answer;
6. health/accounting evidence and checkpoint material needed to describe and persist
   exactly that generation.

Every member is either `Complete` or the candidate cannot promote. A canonical hard
scope/policy exclusion is complete because it is an accounted terminal disposition.
Optional external evidence may be terminal `Unavailable { reason, provenance }` only
when that state is part of the versioned scope contract and every dependent ranker,
formatter, and claim treats it as no evidence, never as evidence of absence.

`Unreadable`, `UnstableDuringRead`, `AbortedCircuitBreaker`, failed or partial parse,
unknown ordering, journal gap, derived truncation, stale computation, or a missing
required artifact is not complete. Per-source and per-surface capability certificates
do not weaken this rule in V1; they attest that the closed set above is complete.

An implementation may omit a future feature only by omitting its tool/resource/
prompt/schema capability from protocol advertisement before the source lifecycle
starts. It may not advertise a surface and later call its incomplete generation
queryable. Temporal, bridge, and authority recomputation after a source change is
candidate work: source work remains non-Idle until the new complete bundle promotes.

Frecency and other mutable operational ranking evidence are deliberately outside the
strict source scope. The canonical query Interface captures their separate immutable,
versioned `QueryOperationalSnapshot` and includes it in the request's `QueryLease`.
No formatter or ranker may reopen SQLite, session state, or wall-clock time after that
capture. This preserves commitment-based ranking without turning a frecency bump into
source `Dirty` or a full candidate rebuild.

## 6. Invariants

1. Strict-current queryable implies `WorkState::Idle` plus a source-bound,
   physically root-bound, verified, complete generation.
2. Candidates are never discoverable through query-addressable caches, queries,
   checkpoints, hooks, sidecar, resources, or prompts. Pure content-addressed
   memoization may be shared only when exact bytes and policy versions are keys.
3. Every mutation carries one atomic binding token containing slot instance ID,
   project ID, binding epoch, physical root identity, source identity, observer
   epoch, canonical root, and base generation.
4. A token is accepted or rejected as one value; its fields are never recaptured
   independently.
5. Exactly one live-or-stopping project slot exists per canonical identity, and at
   most one candidate exists per source slot. A tombstone remains until observers
   and every publication-capable worker are quiescent. Immutable retired generations
   may outlive the slot under independent capacity ownership.
6. Failure, panic, cancellation, or capacity refusal never changes `active`.
7. Promotion requires the complete closed `StrictScopeContractV1`, physical-root
   continuity, and a
   gap-free watcher watermark from the current observer epoch.
8. Overflow, disconnect, journal eviction, or unknown ordering invalidates the
   candidate and forces authoritative re-observation.
9. Capacity ownership follows actual allocations: active, candidate,
   retired-but-pinned generations, derived/query caches, scratch, and journal
   memory. Cancellation never releases live blocking work.
10. Checkpoints serialize committed verified generations only.
11. Deserialized snapshots are candidate seeds after restart, never active
    generations before source, root, manifest, and journal verification.
12. One response captures one `QueryLease`: an immutable project runtime snapshot,
    its active source generation, and one immutable/versioned operational-evidence
    snapshot. Content, generation, availability, and work health come from the runtime
    snapshot. Mutable frecency/time-decay ordering comes only from the operational
    snapshot and cannot affect source truth or readiness.
13. Journal append, cursor assignment, delivered-event or source-write-intent Dirty
    transition, final replay, promotion, acknowledgement, mode switch, and strict
    query acquisition share one defined linearization domain and fixed lock order.
14. Promotion performs no capacity wait and no repository-sized allocation; all
    artifacts and capacity are ready before the commit locks are taken.
15. Joining an existing slot never grants protected-root membership. Root
    eligibility and per-session authority precede registry join.
16. A permanent fault may block progress but cannot manufacture a partial success.

## 7. Preventive load and catch-up protocol

```text
validate root eligibility and this session's membership authority
        |
        v
create/join stable project slot or its stopping tombstone (single-flight)
        |
        v
obtain fixed base lease for slot + bounded scout + watcher journal
        |
        +-- unavailable --> wait without watcher or candidate allocation
        |
        v
register watcher in bounded journal-only mode; mint observer epoch
        |
        v
capture observation cursor S0
        |
        v
perform bounded scout, compute conservative full candidate charge
        |
        +-- full lease unavailable --> discard scout allocation, queue request
        |
        v
atomically reserve full charge; build isolated candidate
        |
        v
capture S1 and replay (S0, S1]
        |
        +-- gap/overflow/disconnect --> discard candidate, retry full observation
        |
        v
repeat authoritative metadata scout and require candidate manifest parity
        |
        v
replay through W, then take journal + lifecycle commit locks in fixed order:
require tail == W, no gap, current binding/physical-root token, complete scopes;
atomically publish runtime snapshot, acknowledge W, switch journal mode
        |
        v
records above W remain queued for the next coalesced delta candidate
```

The watcher never mutates bootstrap or committed state in place. It records hints.
The lifecycle module decides whether a hint can build a bounded delta candidate or
requires full authoritative observation.

Journal cursor assignment, append visibility, and the `Dirty` runtime transition are
one operation. Promotion repeatedly captures a tail `W` and replays through it. The
final commit may proceed only while holding the journal and lifecycle locks, if the
tail still equals `W`. The same commit swaps the project runtime snapshot,
acknowledges `W`, and changes journal mode. Every record is therefore either replayed
before promotion or retained after it.

The normative lock order is `SourceJournal` first, then the `ProjectSlot` publication
writer. No code may acquire them in reverse order. Journal append holds the journal
lock while publishing `Dirty`; promotion holds it while proving `tail == W` and
publishing the replacement runtime snapshot. Strict query acquisition is one atomic
runtime-snapshot load for its source linearization point and acquires neither lock.
It then captures the independently versioned operational snapshot; that Adapter may
hold its SQLite read transaction/session-version lock but never a journal or project
publication lock.

An `ObservationCursor` is `{ observer_epoch, sequence }`; cursors from different
watcher registrations are incomparable. Overflow coalesces the bounded queue to one
constant-size `Gap/Dirty` marker. It never allocates an unbounded path backlog.
Records carry the observed path operation plus available file identity/stamp; rename
is ordered delete+create unless native identity is proven. The final authoritative
metadata scout is the promotion barrier for repository-wide completeness; stable
per-file reads alone cannot prove one repository snapshot.

SymForge-owned repository writes enter this same domain before the first disk side
effect. Edit/curation code validates the complete binding token, takes the journal
then publication locks, and atomically publishes `Dirty`; only then may it write or
rename repository bytes. A successful write cannot restore `Idle` directly: it
triggers a coalesced candidate and returns to `Idle` only through complete promotion.
A failed or rolled-back write may restore `Idle` only after revalidating the binding,
physical root, file identity, and exact pre-write bytes and proving that no external
or journaled source change occurred. Otherwise it remains `Dirty`. This closes the
post-write/pre-watcher window for strict queries and checkpoints.

The observer for the current binding stays alive until its successor is registered,
journal-ready, and atomically handed off. Failed replacement leaves a path for
source changes to trigger retry. A close or rebind revokes publication authority but
keeps the stopping tombstone and task-owned capacity until blocking work actually
exits.

Immediately before promotion, the candidate must revalidate source identity and the
physical directory object behind the canonical root. Same-path directory replacement
is a rebind: watcher, cursor, candidate, and old tokens are invalid. Where a platform
cannot prove directory continuity, promotion fails conservatively and starts a new
observation. The physical-root adapter uses an open-handle file identity (for example,
volume/file ID on Windows and device/inode on Unix) rather than path text.

Filesystem notifications are not a durable ordered log. The cursor is process-local
coordination only. Every detected gap forces reconciliation; no resume claim crosses
process restart without a separately verified source observation.

“Current” is not filesystem-linearizable. It means internally consistent and caught
up through all observations delivered before the publication cut. A filesystem event
may be delivered after a query acquires its lease; every response therefore names the
captured source version and generation, and periodic authoritative reconciliation
remains mandatory.

## 8. Capacity prevention

Per-load ceilings remain useful against individual pathological inputs, but they are
not admission control.

Every server entry point uses one `ProcessIndexRuntime { registry, capacity_pool,
scheduler }`. A daemon shares it across project slots; local stdio and standalone
serve instantiate the same runtime with one or more slots. The semver-public embed
facade remains synchronous in the first slice and must accept or construct an
explicit caller-owned budget before this design claims process-wide guarantees for
embed mode.

The capacity pool has distinct immutable configuration classes:

- per-project logical catalog/content ceilings;
- process persistent residency budget;
- scratch/transient build and checkpoint budget;
- watcher/journal quota;
- reserved runtime and allocator headroom.

The versioned conservative candidate charge includes catalog descriptors, admitted
source bytes, bounded file/symbol/reference/search-index coefficients, derived-view
bounds, parser high-water allowance, and publication scratch. Checked scout counts
and sizes feed that formula. If accounted construction would cross the granted
charge, the candidate aborts before the allocation and re-enters admission with a
larger full request; it never publishes or waits while holding a partial candidate.

Admission rules:

- obtain a fixed base lease before watcher registration or source enumeration;
- after bounded scouting, compute and atomically reserve the full conservative
  candidate charge;
- never block waiting to grow while retaining a partial candidate lease; on refusal,
  discard scout allocations, release them, and enqueue the request;
- use one monotonic cancellation-aware FIFO across cold opens and freshness
  replacements; each source slot has at most one coalesced queued request, and later
  smaller requests do not bypass the head;
- coalesce watcher bursts and build structurally shared delta candidates so one file
  event does not require an unbounded repository-sized clone;
- prebuild all derived publication artifacts before commit; commit performs token
  validation and pointer transfer only.

Capacity ownership is RAII-bound to actual residency, not lifecycle labels. A
candidate task retains its lease until its buffers and parser state drop. A verified
generation retains its persistent charge while active and after retirement until the
last query, cache, or base reference drops. Promotion, map eviction, close, and
cancellation do not release memory that remains reachable. Cancellation stops future
scheduling; it cannot stop an already-running blocking closure.

Accounting includes active, candidate, retired-but-pinned generations, daemon base
interning, query-addressable caches, bridge/authority/temporal state, pre-update
snapshots, bounded query operational snapshots, watcher journals, and
checkpoint/export scratch. Every operation that materializes another representation
obtains scratch capacity first.

Snapshot and team-artifact paths participate before allocation: stat and reserve
before read, cap streaming decompression at the granted charge, account compressed,
decompressed, and deserialized coexistence, and avoid retaining all three forms.
Deserialization produces a candidate seed only.

SymForge-owned collections use checked arithmetic and fallible reservation where
possible. Opaque dependencies receive conservative multipliers and process headroom.
Allocator OOM or a hard-hung parser may still terminate the process; RAII cannot turn
an abort into recoverable in-process state. A requirement for finite hard-task cleanup
would require a killable worker process and is outside the first in-process design.

A hard project limit can still make readiness impossible. The preventive response is
`WaitingForCapacity` or `Blocked`, not a successful placeholder. Longer term,
disk-backed immutable catalog segments can remove the in-memory metadata cliff, but
that is not required for the first safe lifecycle. A project whose minimum charge
exceeds the process budget receives a deterministic `Blocked` action instead of
starving the queue.

## 9. Query policy and last-known-good state

Keeping an old generation does not automatically make it safe to call Current.

Default policy:

- Cold start with `active=None`: index-dependent reads refuse while the protocol and
  health surfaces remain responsive.
- Any delivered source-affecting event atomically makes the source runtime `Dirty`;
  every existing public index-dependent read refuses until a complete replacement
  promotes and returns work to `Idle`.
- Any SymForge-owned source mutation publishes `Dirty` before its first repository
  disk side effect. Strict reads and checkpoints therefore cannot observe changed
  bytes while the old generation is still labeled Current.
- Replacement pending with `active=Some`: retained active remains internal recovery
  material. Existing MCP, HTTP, resource, prompt, hook, and embed consumers do not
  silently use it.
- Negative/global absence claims always require complete current coverage.
- Public as-of reads and capability-scoped partial admission are deferred. A later
  design must define exact source identity, acknowledgement, disclosure, invalidation,
  and proof rules before either becomes queryable.

This preserves the strict sidecar decision. A refusal is preferable to injecting a
stale or incomplete answer into an agent prompt.

### 9.1 Compatibility projection

Lifecycle types stay private during the migration.

| Surface | First-slice behavior |
|---|---|
| Public Rust `IndexState` / `PublishedIndexStatus` | Keep exhaustive variants source-compatible. Any legacy `Degraded` spelling describes runtime availability only, never an active degraded generation. Removing it requires a versioned breaking change. |
| Health/status JSON | Preserve existing fields and add runtime work/epoch evidence additively; render from one captured runtime snapshot. |
| MCP index reads | Existing strict loading/refusal result; no last-known-good fallback. |
| HTTP sidecar and aliases | Return HTTP 503 while strict-current acquisition fails. A caller hook may fail open only by omitting enrichment; it never injects retained context. |
| Resources and prompts | Same strict-current acquisition; parameterless calls cannot opt into as-of state. |
| Daemon `/sessions` | Report slot/source work state and active identity from one runtime snapshot. |
| Hook enrichment | On sidecar refusal, a hook may fail open only by emitting no enrichment; daemon fallback remains allowed and retained stale context is never injected. |
| Public embed facade | Synchronous verified load under caller-owned capacity; no server-runtime state leaks into the public enum interface. |

## 10. Failure policy

| Cause | Preventive behavior |
|---|---|
| Aggregate capacity temporarily unavailable | Wait with a retained request; no candidate allocation. |
| Configured hard limit cannot fit project | Block with explicit configuration/action; no active placeholder. |
| Transient read/walk failure | Discard candidate and retry with bounded backoff. |
| Persistent unreadable/unstable/partial-parse file | Keep prior active unchanged; block strict-current acquisition for that source and expose the remediation class. |
| Parse-failure ratio | Cancel upstream scheduling and discard candidate; do not fold a partial generation. |
| Watcher overflow/disconnect/journal gap | Invalidate candidate and perform authoritative re-observation. |
| Snapshot mismatch/corruption | Quarantine the candidate seed; active remains unchanged or absent. |
| Loader panic/cancellation | Lifecycle owner observes completion and schedules retry or Blocked; task-owned capacity releases only after actual allocations drop. |
| Project rebind | Mint a new binding epoch and physical-root/observer identities; all old mutation tokens and candidates become invalid. |
| Same-path directory replacement | Treat as rebind; invalidate watcher, journal, candidate, and old physical-root token. |
| Close followed by immediate reopen | Join the stopping tombstone until observers and publication-capable workers are quiescent; then create a never-reused slot instance. Retired immutable generations remain independently charged and need not block reopen. |

Retries use a finite automatic-attempt budget. Exhaustion transitions to `Blocked`;
only a source change, capacity/configuration change, or explicit operator retry
starts a new attempt series. Lifecycle ownership persists, so “bounded retry” never
means an unobserved task silently stops forever.

## 11. Evolutionary implementation sequence

### Slice 0 — Freeze causal regression oracles

- Positive-control the generation/root split window.
- Prove a root-A blocking mutation cannot commit after root-B promotion.
- Prove two simultaneous first opens invoke one loader.
- Prove catalog refusal creates no queryable project instance or watcher mutation.
- Prove watcher fresh-instance reconciliation cannot mutate a candidate through the
  active interface.
- Prove a failed or panicked load leaves the active pointer identical.
- Prove same-path directory replacement invalidates watcher/candidate authority.
- Prove close/reopen cannot create a second slot while blocking work survives.
- Prove failed reload leaves the old observer capable of triggering recovery.
- Prove a restored snapshot is not queryable before new-process verification.
- Prove cancellation does not release capacity while blocking memory remains live.
- Positive-control a hybrid read across current `live` and source-bundle ArcSwaps.

### Slice 1 — Atomic mutation authority

- Replace independent generation/root sampling with one binding token captured from
  an immutable published generation.
- Validate the whole token under the publication writer lock.
- Include never-reused slot instance, binding, physical-root, source, observer, and
  base-generation identity.
- Make rebind or same-path physical replacement mint new epochs and invalidate all
  prior work.
- Remove `effective_fence_generation`'s inference from separate observations.

This closes the newly proven cross-root corruption class before the larger move.

### Slice 2 — Registry tombstone and capacity foundation

- Insert a project slot before loading and let concurrent sessions join it.
- Separate slot existence from active-generation existence.
- Check root eligibility and per-session protected membership before join; slot reuse
  never grants authority.
- Keep stopping tombstones registered until observers and publication-capable
  workers are quiescent. Retired immutable generations remain independently charged
  and do not extend slot identity lifetime.
- Introduce the shared process runtime and capacity pool for daemon, local stdio, and
  standalone serve.
- Add base/scout/journal and full-candidate reservation classes before any new
  active-plus-candidate path exists.
- Capacity refusal cannot construct an active `ProjectInstance`.

### Slice 3 — One runtime snapshot and query seam

- Make `VerifiedGeneration` subsume or directly own the existing
  `PublishedGeneration`; do not add a second active pointer.
- Atomically publish one project runtime snapshot containing per-source active/work
  and observation state.
- Route every index-dependent MCP, HTTP, resource, prompt, hook, cache, checkpoint,
  and health consumer through strict-current query acquisition.
- Preserve the compatibility matrix before candidate promotion becomes reachable.
- Remove direct consumer mixing of `live`, health, outline, and source-set ArcSwaps.

Slices 2 and 3 may use the current synchronous/active loader internally, but neither
may expose a new candidate path. Every consumer must cross the canonical query seam
before Slice 4 lands.

### Slice 4 — Owned, capacity-reserved candidate and observer

- Move loader `JoinHandle`, cancellation token, attempt counter, progress,
  classified failure, and retry trigger into the per-source lifecycle module.
- Keep protocol initialization responsive through the slot, not through a mutable
  empty index.
- Turn current outside-lock `ReloadData` construction into a candidate that cannot
  reach active caches or handlers.
- Acquire base and full conservative capacity before allocation; task and generation
  ownership retain leases until real memory drops.
- Treat deserialized snapshots as bounded candidate seeds under the same admission.
- Register a bounded watcher journal before observation, while keeping the current
  observer alive until successful handoff.
- Add observer epochs, physical-root validation, cursor capture, gap detection,
  replay, and the single journal/promotion/query linearization domain.
- Promote only complete strict-current scopes.
- Route every delivered watcher event and targeted refresh through an isolated,
  capacity-reserved full replacement candidate. Until Slice 5 optimizes this path,
  events atomically mark `Dirty` and coalesce one full rebuild; no legacy in-place
  publication lane remains once candidate promotion is reachable.
- Route every SymForge-owned source edit/curation intent through the same pre-write
  `Dirty` transition before its first disk side effect.
- Include temporal, bridge, authority, and other advertised derived work inside the
  complete candidate; none starts as a later mutation of Current state.

### Slice 5 — Optimize to structurally shared delta candidates

- Replace Slice 4's safe full-replacement refresh with coalesced delta candidates
  whose unchanged immutable structures are shared.
- Prebuild publication artifacts and reserve capacity before the commit locks.
- Preserve the same candidate tokens and promotion proof; this slice changes cost,
  not mutation authority or semantics.
- Add measured burst latency and active-plus-candidate memory gates.
- Add inactive-project eviction only after measured pressure demonstrates need.

### Slice 6 — Remove obsolete mechanisms

- Remove bootstrap lifecycle from circuit-breaker state.
- Remove watcher and targeted admission into placeholders.
- Move circuit-breaker cancellation before/inside scheduling; discard failed
  candidates rather than publishing partial folds.
- Retire redundant readiness fields and secondary publication roots only after every
  consumer and semver-public compatibility projection has migrated.

## 12. Current symptom patch disposition

The parked `fix/cold-start-terminal-lifecycle` work remains recoverable in Git stash
`wip: terminal lifecycle safety-net before prevention redesign`.

Retain as evidence or interim containment:

- regression tests and typed capacity/observation reasons;
- strict sidecar refusal;
- stable read, snapshot quarantine, disk-absence deletion checks;
- exact generation-fencing intent;
- outside-lock build followed by atomic publication.

Do not land as the final architecture:

- `BootstrapLifecycle` inside `CircuitBreakerState`;
- additional `index_state()` shape predicates;
- failure transitions that mutate active-index freshness;
- watcher or targeted admission into an empty bootstrap;
- late circuit-breaker folding into a degraded candidate.

If the preventive slices cannot land promptly, the terminal failure transition may
be used as a temporary fail-closed safety net. It must be labeled containment with a
removal issue, not completion of this design.

## 13. Verification strategy

### Pure state model

Model these actions:

```text
open, join, stop, reopen, reserve-base, reserve-full, grant, refuse,
build-success, build-failure, panic, cancel, worker-exit, query-drop,
watch-register, watch-event, delayed-event, overflow, replay, promote,
source-write-intent, source-write-commit, source-write-rollback,
retry, snapshot-valid, snapshot-invalid, root-replaced, rebind, close,
query, query-operational-capture, frecency-bump, checkpoint
```

Use `proptest` command sequences to assert:

- no query returns candidate content;
- failure leaves the active pointer unchanged;
- promotion requires the complete `StrictScopeContractV1`, physical-root
  continuity, and a gap-free current-observer watermark;
- a committed SymForge source write cannot coexist with `WorkState::Idle` and the
  pre-write active generation;
- root-A work never mutates root B;
- same-path directory replacement cannot promote against the old observer;
- at most one live/stopping slot exists per canonical identity and one candidate per
  source slot;
- accounting never exceeds process capacity;
- cancelled workers and retired query-pinned generations remain charged until drop;
- checkpoint identity always names a committed generation;
- one response ranks from one operational version/evaluation time, and a concurrent
  commitment bump affects later leases only.

### Concurrency tests

Use Loom for the small ownership modules:

- slot creation and single-flight join;
- close/reopen tombstone versus a surviving blocking loader;
- capacity grant versus cancellation and panic cleanup;
- two candidates attempting partial reservation without hold-and-wait;
- promotion versus a watcher event at the watermark;
- source-write intent versus strict query/checkpoint acquisition;
- source-write rollback proof versus an external event or physical-root change;
- query operational-snapshot capture versus concurrent session/persistent frecency
  bump and wall-clock decay;
- watcher registration replacement and delayed delivery after the publication cut;
- rebind versus a late blocking mutation;
- same-path physical-root replacement versus promotion;
- query acquisition versus promotion;
- retired-generation final drop versus new admission;
- close versus retry wake-up.

Use deterministic failpoints in integration tests at every task and filesystem seam,
including Windows sharing violations, rename/delete/recreate ABA, watcher overflow,
already-running blocking work after cancellation, bounded snapshot decompression,
failed reload observer handoff, and every direct server entry point. A load-bearing
edit failpoint pauses after atomic rename but before refresh scheduling; strict query
and checkpoint calls must already refuse because the pre-write intent published
`Dirty`.

Maintain a compact TLA+ model for `{ProjectSlot, SourceRuntimeSnapshot, Candidate,
Journal, Capacity}`. Safety checks cover stale-attempt rejection, no lost pre-cut
event, one atomic runtime publication, and allocation ownership. Liveness is
conditional on a quiescent readable source, sufficient capacity, bounded task
completion, and fair scheduling.

### Release gates

- formatting and Clippy with warnings denied;
- focused lifecycle/capacity/watcher/snapshot suites;
- serial all-target test suite;
- release build and canonical tool fixtures;
- cold-start race campaign with a working positive control;
- measured memory test with concurrent projects, retired query-pinned generations,
  active-plus-candidate overlap, snapshot scratch, and watcher journals;
- sustained watcher-burst latency/memory gate proving coalesced delta feasibility;
- adversarial architecture review before Slice 1 and code review after every slice.

## 14. Rejected alternatives

### Add more readiness predicates

Reject. It preserves distributed lifecycle ownership and makes another illegal state
distinguishable only after it has been constructed.

### Keep the placeholder but prevent only targeted freshening

Reject. Watcher reconciliation, snapshot verification, Git temporal work, future
handlers, and independently sampled identity can still touch it.

### Publish degraded last-valid wrappers

Reject as the default architecture. It truthfully labels stale state but mutates the
query root on failure and makes every consumer understand freshness composition.
Keep the active verified generation unchanged and expose work state separately.

### Capability-scoped partial promotion in the first lifecycle

Reject pending a separate proof-carrying design. Without a closed capability set,
derivation rules, and invalidation rules, this merely renames degraded coverage.
The first lifecycle promotes the closed `StrictScopeContractV1` only and keeps
retained last-known-good state private.

### Incrementally grow a partially allocated candidate lease

Reject. Two candidates can retain partial allocations and wait forever for capacity
held by the other. Use a bounded base scout lease, then atomically acquire the full
conservative candidate charge or release and queue.

### Block MCP initialization until indexing finishes

Reject as the general solution. It restores correctness by sacrificing protocol
availability and still leaves daemon single-flight, aggregate capacity, rebuild, and
watcher ownership unresolved.

### Rename Degraded to Recovering

Reject. A new label without candidate isolation, ownership, and atomic promotion is
the same defect with friendlier wording.

### Make the filesystem watcher a durable log

Reject. Filesystem notifications do not provide a portable resumable history.
Treat them as bounded hints and force authoritative observation after any gap.

## 15. Review gates and open decisions

The external reviewer must answer these before implementation:

1. Does the design prevent partial promotion, or merely move “degraded” into another
   label?
2. Do the project registry, per-source lifecycle, and capacity pool place each
   ownership rule at one deep interface without creating a God coordinator?
3. Can the journal protocol lose an event at registration, cursor capture, final
   replay, cutover, delayed OS delivery, or promotion?
4. Does the binding token eliminate both generation/root split-brain and same-path
   physical-directory replacement?
5. Is capacity accounting closed over active, candidate, retired query-pinned,
   cache/base, snapshot/checkpoint scratch, journal, and blocking-task residency?
6. Is strict refusal of every public current read while source work is non-Idle the
   correct first-slice tradeoff, or does Feature 020 require a separately specified
   capability lattice before this design can proceed?
7. Does the migration sequence preserve source identity, snapshot quarantine,
   per-source independence, deletion convergence, protected-root membership, and
   strict sidecar refusal?
8. Which frozen Feature 020 requirements must be amended, and does any amendment
   weaken a load-bearing safety property?
9. Is there a smaller module interface that preserves the same invariants and
   locality?
10. Can snapshot restore, local stdio, standalone serve, or the public embed facade
    bypass the runtime and capacity seams?
11. Does any migration slice temporarily create two publication authorities or an
    uncharged active-plus-candidate path?
12. Name a concrete interleaving that still promotes or serves untrusted state.

The design may be frozen for external review only after an internal adversarial pass
has either resolved or explicitly recorded every P0/P1/P2 finding.
