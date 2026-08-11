# Project Index Lifecycle Prevention — Design

**Status:** FROZEN FOR EXTERNAL REVIEW ROUND 2 · **Date:** 2026-08-11 · **Target:** `main` at `1521abb0` · **Release boundary:** v11

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

> Under arbitrary faults, every primitive **source-truth** fact names exactly one
> atomic authority: an immutable verified generation; a beneath-confined, path-local
> disk observation; a complete root-bound worktree-scope observation for its declared
> scan interval; or an immutable Git observation. Every comparison or higher-order
> derivation names its normalized operation and all source and selection inputs.
> Observable non-source ordering or scores carry separate evaluation provenance.
> Candidate and partial generations are never queryable; path-local observations
> cannot prove completeness; and a complete worktree scan proves only its declared
> scope and interval—not generation identity, repository-wide atomicity, or
> current-after-return state.

This does **not** promise that SymForge is always Ready. Permanent permission
failure, disk loss, an intentionally insufficient hard limit, or a source that
never stabilizes can make readiness impossible. Renaming those states would be
dishonest. The preventable failure is promotion of untrusted state.

The first safe generation lifecycle is deliberately strict-current. Retained
last-known-good state is internal recovery material, not an automatic answer source.
Explicit disk, worktree-scope, and Git observation tools remain available under their
own claim provenance.
Capability-scoped partial availability and public last-verified/as-of generation reads
are deferred to separate reviewed contracts with their own proof and disclosure rules.

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
with a separately published root instead of one immutable binding authority.

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

### 2.12 Raw disk reads are an unnamed atomic authority

`src/protocol/read_gate.rs:92-178` security-classifies the exact bytes it reopens from
disk, but does not bind them to the generation whose manifest/index miss led to the
read. Callers include generation fallbacks, working-tree searches, uncommitted symbol
diffs, syntax validation, and impact analysis. Some intentionally observe current disk;
the defect is attributing those observations to a generation or mixing them with
generation structure without naming the comparison.

**Cause:** response provenance models generation identity but not disk-observation
identity, so a security-admission Adapter also became an implicit content-authority
Adapter.

### 2.13 Stamp parity can preserve undetected staleness

Stable read correctly double-reads/hashes one file, but a same-size rewrite after the
read can retain a colliding timestamp. Reconciliation may then skip it when metadata
matches, and snapshot verification currently samples a fixed subset. A silently
dropped watcher notification can leave the old generation undisputed indefinitely.

**Cause:** fast file stamps are treated as durable byte identity instead of a cache
hint backed by racy-clean promotion checks and rolling content verification.

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
slot while leaving the retained verified generation byte-for-byte unchanged.

Implementation remains unauthorized until Feature 020 is refrozen as one coherent
contract. The gate creates a checked-in, hash-pinned
`specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md` that enumerates and
classifies **every** file under the Feature 020 directory plus the bound `CONTEXT.md`,
with exactly one declared exclusion: the manifest cannot recursively hash itself.
Instead, a detached
`docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md` pins the manifest SHA-256, this
design SHA-256, the bound `CONTEXT.md` SHA-256, and one amendment-set ID. The validator
rejects any other omission or any attestation/design/context inconsistency. The
checked-in file is not its own trust anchor: after review, the release system emits a
signed, append-only `RefreezeApprovalRecordV11` outside the mutable repository tree.
That record binds the exact target commit/tree, detached-attestation digest, and trusted
release identity. Activation accepts only the exact commit covered by that external
record; a coordinated manifest/design/context/API/attestation rewrite therefore needs
a new reviewed approval record and cannot validate itself by rewriting its own anchor.
The release verifier checks both internal consistency and the external signature/
identity policy before any implementation slice or PreventiveV1 activation.
The amendment-set ID is not an operator label: it is the domain-separated SHA-256 of
the canonical sorted records `(amendment_id, replaced_clause_ids_and_hashes,
replacement_clause_ids_and_hashes, regression_ids)`. The manifest validator
recomputes it before accepting the detached attestation.
The same prerequisite creates canonical
`specs/020-repository-knowledge-index/contracts/public-api-v11.json`. This is the sole
normative external Rust allowlist. It first declares a closed supported-configuration
domain: every supported target triple/OS/arch/pointer-width/atomic capability, every
legal Cargo feature vector (including negative predicates), and every other
public-item-affecting `cfg` key/value. Unknown cfg names/values and configurations
outside that matrix are explicitly rejected rather than silently falling outside the
guarantee. For every matrix cell—or a mechanically proven exhaustive cfg-cover—it
enumerates the complete externally reachable rustdoc/public-item graph: every module
and re-export; type, trait, alias, variant, and public field; function and inherent
method; direct trait implementation and every associated function/type/constant;
explicit auto-trait expectation; const/static; exported macro; generic parameter,
bound, exact signature, and `cfg` edge. It asserts the absence of every other item,
implementation edge, crate-root, flat, and deep path. In particular, authority-bearing
types cannot accidentally gain `Deserialize`, `Default`, or `From`; lifecycle handles
cannot gain `Deref`/`AsRef`/`Borrow` to raw internals; `EmbeddedSourceHandle` remains
non-`Clone`; and intentional `ProcessIndexRuntime` owner-token `Clone`/`Drop` behavior
is pinned.
It also contains the v10 `keep | replace | remove` mapping. The detached attestation
pins its canonical digest. Generated rustdoc-JSON graphs are extracted and merged for
the full matrix/cover; an all-cfg HIR/source inventory cross-checks that no public item
hidden behind an inactive target or negative predicate escaped extraction. Both are
proven against a checked completeness fixture containing target-only, negative-cfg,
trait-impl, associated-item, auto-trait, and macro exports. Dependent-crate positive/
compile-fail tests consume the same graph and fail on any unlisted item or impl edge.
Prose category names cannot add an
export. Slice 0 cannot begin until this
manifest exists; later slices implement it but do not invent the public Interface.
Its required normative set includes `GOAL.md`, `spec.md`, `plan.md`, `data-model.md`,
`tasks.md`, `quickstart.md`, every `contracts/*.md`, and
`checklists/requirements.md`; any other artifact is explicitly classified normative,
supporting evidence, historical, or superseded. The manifest maps every amendment
below to the replaced requirement/scenario, contract clause, plan/task, and regression
ID. Completed historical receipts remain byte-for-byte intact but are marked
superseded where they describe degraded publication. A manifest-aware validator must
prove that every listed hash/classification and replacement mapping is closed;
generic `speckit-analyze` is an additional consistency check, not sufficient proof by
itself. Slice 0 cannot begin while any Feature 020 artifact is unclassified or any
normative clause still permits the old degraded/last-verified behavior.

Required specification amendments:

1. Replace “publish a degraded wrapper” with “leave the retained verified generation unchanged and
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
13. US3 scenario 6 allows no-match only when every selected required source is
    `Current`; otherwise the result is a readiness refusal with per-source evidence.
14. US8 scenario 6 and SC-011 make first-contact coverage/role evidence available
    only from `Current` generations. A non-Current selected source is a readiness
    failure, not partial orientation.
15. SC-002 applies manifest-equality accounting to promoted `Current` generations.
    Aborted attempts expose separate bounded attempt accounting and never claim a
    canonical repository manifest.
16. FR-021 health separates committed-generation accounting from attempt diagnostics.
17. FR-049 continues to keep persistence/durability orthogonal to query readiness,
    but is corrected so observer telemetry alone is not source truth: `Gapped` or
    otherwise incomplete observer coverage prevents strict-current acquisition.
18. Feature 020 gains the edit-convergence latency and delta-memory success criteria
    defined by `ObservedRefreshGateV1` in this design.
19. The source-binding/state, search-knowledge, repository-mental-model, and
    knowledge-authority-hygiene contracts replace public degraded/last-verified rows
    with strict `SourceRefusal` and internal-only retention. Their `authority_scope`
    values remain `KnowledgeVoiceFilter`, never generation-consistency selection.

These amendments preserve FR-008 atomic whole-generation publication, FR-010
shared single-path admission, FR-012 source identity and quarantine, FR-049's
corrected separation of persistence health from source-observation readiness, and
SC-003 deletion and missed-event convergence.

## 4. Selected deepening opportunity

Create one deep **source index lifecycle module** behind every source slot. Keep two
different ownership domains as separate concrete modules rather than building a God
coordinator:

- **Project registry:** root eligibility, per-session membership, non-authoritative
  pending admissions, stable project slots, slot tombstones, source-slot inventory, coherent selected-source query
  acquisition, and close/reopen single-flight.
- **Source index lifecycle:** one candidate, invalidation accumulator, retry series,
  tracked destructive permits, promotion, runtime snapshots, and stop semantics for
  one source.
- **Capacity pool:** one process-wide admission/scheduling domain for persistent,
  scratch, accumulator, verifier, safety-transition, and headroom reservations plus
  accounted residency groups.

Claim composition and operational ranking remain separate Adapters above those
Modules. They can combine source-truth leases and observations, but cannot alter
readiness or publication.

The source lifecycle interface owns these operations as one ordering contract:

- request or coalesce a refresh trigger;
- own exactly one load attempt and candidate per source slot;
- accept watcher hints/gaps into one bounded invalidation accumulator;
- validate candidates against one observer epoch, coverage state, and observation cut;
- promote one verified generation atomically;
- grant and terminally resolve one tracked source mutation permit;
- report one immutable runtime snapshot;
- revoke, stop scheduling, and reap owned work.

The deep source-lifecycle Interface is initially concrete:

```text
observe(ObserverToken, ChangeHint | Gap) -> Observed | SourceRefusal
request_refresh(BindingAuthority, cause) -> RefreshTicket | SourceRefusal
begin_mutation(MutationRequest) -> SourceMutationPermit | SourceRefusal
capture_source_view() -> SourceRuntimeView
freeze_and_revoke(Revocation) -> DrainHandle
```

Candidate staging, grant consumption, proof refresh, and promotion are private
Implementation transitions, not a shallow public choreography. The CapacityPool
delivers capacity only to a registered lifecycle `WorkId`. The registry exposes one
narrow cross-Module commit seam through opaque registry-owned tokens. It mints a
non-forgeable `SourcePublicationToken` for one exact private
`Arc<SourceStatePublication>`. The lifecycle may retain/compare that token but cannot
name or inspect the Arc/runtime root. It supplies a privately constructed sealed
`SourceTransitionIntent`; the registry Adapter uses the token and intent to build an
opaque `PreparedRuntimeDelta` outside the publication writer.
`commit_source_delta(PreparedRuntimeDelta)` embeds and exact-matches that token,
validates the owning project/slot `Live` gate, binding/source membership, and the
transition-specific lifecycle authority, and patches the **latest whole** project root
without exposing representation or letting the registry choose lifecycle policy. It
never requires a `SessionBindingPublication` for observer, proof-expiry, capacity,
mutation-terminal, or promotion work. Session authorization is consumed at open/query/
mutation admission and, where needed, retained by that operation/permit; later session
retarget, add/remove, reconnect, or disappearance cannot veto a required source safety
publication. Callers cannot construct a prepared delta, supply a raw publication, or mint
completeness. Protocol topology is compiled into a `RequiredArtifactSet` by the
generation-builder Adapter, while the lifecycle validates only its sealed certificate
identity.
`SourceRuntimeView` is health/progress evidence and exposes no cloneable generation
root. `ProjectRuntimeState`, `SourceStatePublication`, and persistent-map roots are
private to the registry Module. `acquire_project_query` below is the sole Interface
that can return selected generation roots plus their selection receipt; static
visibility/consumer tests reject any lifecycle/Adapter that names the private runtime
root directly, and reject registry code that decides a lifecycle transition rather
than mechanically adapting a sealed intent.

`request_refresh` uses binding authority only to admit a new external trigger. Every
lifecycle-owned continuation—retry timer, loader completion, capacity completion, or
supervisor callback—also retains the exact owning `SourcePublicationToken` (or a
never-reused attempt-state identity plus `WorkId`) and commits only by exact CAS. If a
newer publication or promotion superseded it under the same binding, the continuation
terminally no-ops/refunds; it cannot revoke the newer `Current`. Watcher delivery is
the deliberate exception: its stable `ObserverToken` remains valid across generation
promotion so a queued post-cut event is not lost.

`SourceMutationPermit` is itself a deep Interface:

```text
start_side_effect() -> Result<InFlightMutation, SourceRefusal>
commit(WriteReceipt) -> RefreshTicket | Drained
rollback(NoSideEffectProof) -> RefreshTicket | Drained
```

The permit is non-cloneable and registered against one mutation epoch. Beginning it
invalidates every candidate based on the old epoch. Promotion requires that epoch
unchanged and no active permits. Every terminal path that can return the **same live
binding** to `Current`, including a valid `NoSideEffectProof`, returns through a
verification/no-op candidate at the current observer cut and monotonic mutation epoch
and installs a fresh safety package. This prevents rollback from erasing an observation
or reviving pre-permit candidate authority. A `Stopping` revocation may seal and
unregister an unstarted predecessor permit without building a candidate that can never
publish; any successor binding still requires its own complete fresh candidate. Drop,
panic, or any side effect stays non-Current until verified candidate promotion.
Freeze closes the permit gate. A granted-but-unstarted permit fails
`start_side_effect` with the exact authority/root/path/platform `SourceRefusal` and
enters `RevokedSealPending`; only the out-of-writer grant-seal/construction-guard
terminal path unregisters it without a side effect. An already in-flight
permit retains only its pinned old-root drain authority; under project `Stopping`, its
terminal `commit`/`rollback` returns `Drained`, unregisters, and never enqueues refresh
or resolves a canonical key. Install remains forbidden until that registration is
terminal.

The project registry owns coherent reads:

```text
acquire_project_query(selection) -> ProjectQueryLease | SourceRefusal
```

It first captures the registry's immutable, never-reused `SessionBindingPublication`
containing the active project plus authorized working-set membership, loads each named
**Live** project publication, then exact-identity revalidates the
session publication (or equivalently holds one registry read guard across the session
and project-root capture).
Retarget and additive/removal membership changes each commit exactly one CAS of that
session publication; provisional target membership and old-project cleanup never
authorize queries. Acquisition then freezes
the canonical selected source set, both session/project publication identities,
protected membership authority, never-reused session incarnation, and session
revision into a `SourceSelectionReceipt`, requires every explicitly selected source to
be `Current`, clones only those generation roots plus the receipt, and drops the
project snapshot. A single-source lease is the
specialization. The sealed lease is the request's authority: finalization validates
only its captured receipt/generation identities and never re-reads live membership or
readiness. A retarget, proof refresh, or source removal after acquisition affects the
next request, not a complete response already derived from the old lease. Pure disk/Git
observation and mixed `ClaimContext` construction live in their owning Adapters, not
behind an ambiguous lifecycle `acquire(query_class)` union.

Transient `WaitingForCapacity`, `Building`, `RetryWait`, and `Blocked` states remain
inside the lifecycle module. Startup and daemon callers join one stable registry
admission/slot handle; they do not receive transient load outcomes and decide retry
ownership themselves. That join handle grants no project authority while its registry
entry is `PendingProjectAdmission`.

The Implementation hides source scanning, candidate building, retry policy,
invalidation coalescing, snapshot staging, publication fencing, and tracked work
registration. The capacity pool hides scheduling, grant delivery, and accounting.
The registry hides identity, membership, and selected-source capture rules.

Each passes the deletion test. Deleting the source lifecycle would force startup,
watcher, snapshot, query, and health callers to relearn promotion ordering. Deleting
the registry would spread membership and tombstone races through session callers.
Deleting the capacity pool would make each loader guess at aggregate residency.

No trait hierarchy is required. These modules initially have one concrete
in-process implementation. Internal seams are justified only where two real
adapters already exist, such as filesystem observation and snapshot candidate
restoration.

## 5. Authoritative state model

The registry first owns a non-authoritative single-flight admission record, then a
project slot that owns binding/membership and a map of independently progressing
sources:

```text
ProjectRegistryEntry =
    PendingProjectAdmission(Arc<PendingAdmissionPublication>)
  | LiveProjectSlot(ProjectSlot)
  | StoppingTombstone { slot_instance_id, drain_receipt }

PendingAdmissionPublication {
    admission_identity: Arc<AdmissionAttemptIdentity>, // checked never-reused
    authorized_join_key,
    root_admission_authority: {
        canonical_root, physical_root_identity,
        owning_physical_root_lease
    },
    process_base_charged_cell,
    phase:
      Open {
        drain_registration,
        capacity_request
      }
      | Cancelling { cause, pool_tombstone, drain_receipt }
}

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

`PendingProjectAdmission(Open)` is the only entry exposed before fixed project
admission. It lets concurrent authorized opens join one attempt, but it is not a `ProjectSlot`,
cannot hold a runtime root, and grants no project, source, query, observer, work, or
mutation authority. Its bounded cell and drain registration are charged to the
process base. Immediately after membership authorization and before root I/O, the
opener mints a never-reused non-authoritative `AdmissionAttemptIdentity`. Successful
root capture creates the pending publication with that same identity and the exact
`PhysicalRootLease`; an initial capture failure can therefore produce an authorized
typed refusal without inventing pending state or binding authority. Every join captures
a fresh lease and exact-compares physical identity with
that admission; the registered worker retains the original lease and never reopens the
canonical path. A replacement directory refuses/join-cancels the old admission and
requires a distinct successor identity.

One atomic capacity transaction commits the complete fixed project base,
the always-armed project revocation package, and every initial source's fixed base and
source revocation package. Install and cancellation then contend on the exact retained
`PendingAdmissionPublication` under the process-registry writer. Install revalidates
the process `Live` gate and exact-compares a fresh confined root handle prepared
outside the writer with the held root identity (without replacing the held lease),
atomically replaces Open with
`LiveProjectSlot`, and transfers the same root lease and DrainRegistration into the
initial binding **before** fulfilling any joiner. Cancellation atomically replaces
Open with `Cancelling`, then publishes the pool tombstone and drains without a project
runtime root. The loser cannot cross the transition: an install loser drops/refunds
its charged candidate, while a cancel path that finds Live must Freeze that slot.
A late grant refunds and terminalizes against the cancelled, never-reused admission.
Consequently a `ProjectSlot` is never observable without the capacity needed to revoke
it or with a path-recaptured/different physical root.

The source count is bounded by the admitted Feature 020 source limit. Each
`SourceStatePublication` contains one closed `SourceRuntimeState` plus a checked,
never-reused state identity and remains registry-private. The lifecycle seals a
`SourceTransitionIntent` against its opaque `SourcePublicationToken`; the registry
Adapter prepares a charged opaque `PreparedRuntimeDelta` outside the slot writer: the
new source state, worst-case persistent-map path nodes, and an empty outer-root patch
shell. Under the writer the registry loads the latest whole `Live` publication, retains and
`Arc`-identity-compares only every field it intends to change (including the exact
expected source publication), fills the source-map path and outer shell without an
allocator call, preserves every untouched latest sibling—including project membership
and the revocation package—checked-increments the latest diagnostic epoch, mints a new
project publication identity, and performs one root store. A delta prepared before a
membership or unrelated-source CAS therefore rebases without restoring an old sibling.
No repository-sized copy, capacity operation, callback, or final
charged-root drop occurs under the writer. Current worktree and local-ref sources
retain independent generations while queries capture one coherent project source set.
`ProjectMembershipPublication` contains protected project/source membership and its
revision. Project membership CASes prepare and store a project-root delta under the
same writer as source changes. Query acquisition loads one
`ProjectRuntimePublication`, so project selection evidence and source roots are wholly
before or after such a CAS—never composed from separate loads.

Exact project-root comparison uses the retained non-reused publication identity, not
the diagnostic `runtime_epoch`. Epoch arithmetic is checked and the terminal value is
reserved: a Live publication may advance only through `MAX-1`; the next transition
uses the precharged project package to publish terminal `Stopping { runtime_epoch:
MAX }`, drains, and installs a new never-reused slot/full baseline. No exhausted epoch
can leave an old `Current` root queryable or wrap into ABA.
`Live` is also the project-wide query/source-add/work/permit gate. Slot base admission
precharges an unwind-safe `RevocationPublicationPackage` large enough to publish the
whole project `Stopping` root from **any** Live-map mix of
Loading/Current/Refreshing/Blocked/source-Stopping sources. The package is a root-agnostic preallocated shell: once Freeze owns the
writer it fills against the latest Live source-map root without allocation rather than
optimistically validating an older whole-project root. It is reserved for Freeze and remains armed through every work state. Freeze
uses it to publish `Stopping` in one runtime-root store before per-source drain;
`Stopping` retains source roots only for
drain/diagnostics and cannot grant a query, add a source, register work, or issue a
permit. No loop of per-source state changes approximates project revocation.

`SourceRuntimeState` is a private closed enum. Queryability cannot be inferred from
an unrelated fact combination:

```text
ObserverPhase =
    Absent { initial_activation_package }
  | Active { token: ObserverToken, next_handoff_publication_package }
  | Draining { token: ObserverToken, in_progress_handoff_package }
  | ObserverFree { handoff_id, retry_trigger, in_progress_handoff_package }

Loading {
    binding: BindingAuthority,
    observer_phase: ObserverPhase,
    mutation_epoch,
    source_revocation_publication_package,
    work: NonCurrentWork,
}
Current {
    generation: Arc<VerifiedGeneration>,
    safety_transition_package,
    next_handoff_publication_package,
    source_revocation_publication_package,
}
Refreshing {
    binding: BindingAuthority,
    observer_phase: ObserverPhase,
    mutation_epoch,
    active_permits,
    retained: Arc<VerifiedGeneration>,
    source_revocation_publication_package,
    work: NonCurrentWork,
}
Blocked {
    binding: BindingAuthority,
    observer_phase: ObserverPhase,
    mutation_epoch,
    retained: Option<Arc<VerifiedGeneration>>,
    source_revocation_publication_package,
    cause,
    operator_action,
}
Stopping {
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
```

Only `Current` admits a strict generation query. `Refreshing` and `Blocked` retain an
immutable verified generation without calling it Current. This accepts the useful
part of the three-state proposal—illegal readiness combinations become
unrepresentable—without losing retained-generation, observer, mutation, and retry
evidence. `Current` has no separately published binding or mutable observer side field: its
generation owns the accepted binding authority, observer token, and acknowledged cut.
Its next-handoff package is operational capacity, not independently sampled identity.
Private constructors enforce every other variant invariant.

The closed `ObserverPhase` in every observer-owning non-stopping state makes replacement
truthful before or after the first generation. `Absent` means no observer has yet been
admitted; `Draining` means predecessor ingress is closed but its admitted deliveries/
base still drain; `ObserverFree` holds handoff retry work after predecessor release
without retaining or pretending an observer token; `Active(T1)` means the successor
accumulator is installed and may still be `BaselinePending`. Only `Active(T1)` with
Complete coverage and an unchanged cut can contribute to promotion; Blocked must also
resolve its cause. Cold Loading and retained Refreshing use the same handoff protocol.
Every admitted `Active` observer immutably owns a non-borrowable
`NextHandoffPublicationPackage`: charged, preallocated persistent-root shells for the
triggering `Active -> Draining`, `Draining -> ObserverFree`, and successor activation
publication. Draining/ObserverFree carry the unspent remainder. Before the last shell
may publish successor `Active(T1)`, observer admission must reserve/install T1's own
fresh next-handoff package; otherwise it remains ObserverFree and no ingress opens.
Initial source base contains the first activation plus next-handoff package. Package
charges transfer through phase roots and release only with their final retained Arc;
the observer/handoff capacity vector includes the bounded current, in-progress, and
successor overlap. Thus T1 may disconnect immediately—even before promotion and while
ordinary capacity is exhausted—and still publish T1→Draining. Repeated handoffs never
depend on a singular source-creation credit.
Non-handoff Current→Refreshing safety transitions transfer the existing Active
package unchanged into `observer_phase`; proof refresh preserves it. Candidate
promotion may reuse that charged package only for the exact same observer token;
otherwise it installs a fresh one before publishing Current.

Source-slot base admission precharges a distinct root-agnostic
`SourceRevocationPublicationPackage`, armed in every non-stopping state. A source-only
Freeze fills it under the project writer from the latest Live map and exact current
source publication, then stores that source as `Stopping` before drain without
ordinary allocation. Its charge transfers into the stopping map nodes. This package
is neither the Current-only safety transition nor the whole-project revocation
package, so closing one Loading/Refreshing/Blocked/Current embedded source cannot stop
or starve unrelated sources.

The `InvalidationAccumulator` is the sole live owner of observer sequence and coverage.
Runtime variants carry only the stable `ObserverToken` when one is present; they never copy mutable
`Complete/Gapped` or sequence fields that could diverge after overflow. Health and
diagnostic consumers call one phase-aware lifecycle-owned `capture_source_view`. It
loads source publication R1; if R1 names token T, it acquires T's accumulator, reloads
the project root, exact-validates R1+T, and retries on drift. If R1 intentionally has no
observer—`Absent`, `ObserverFree`, or Stopping—it returns immutable
R1 with explicit `Absent/ObserverFree` evidence and takes no accumulator. Callers
cannot sample accumulator and runtime independently or demand a nonexistent token.

A `VerifiedGeneration` is immutable and contains:

- slot instance ID, project ID, binding epoch, physical root identity, canonical
  root, source identity, and observer epoch;
- captured source version;
- accepted mutation epoch;
- complete canonical manifest;
- one root-scope discovery obligation proving that the manifest accounts for every
  in-scope path under its scope/policy versions;
- one verification obligation and next-due evidence for every manifest entry;
- content and derived indices;
- acknowledged observation cut;
- content and publication identity;
- complete-scope certificates for every advertised query scope;
- charged accounted-residency roots retained until their final aliases drop;
- one atomic query/publication root.

Mutable call-time evidence is not part of `VerifiedGeneration` or the lifecycle
Module. After project query acquisition, the ranking Adapter constructs one immutable
`RankingSnapshot { persistent_store_version, session_version, evaluation_time,
scores }`. Persistent frecency is read through one SQLite snapshot/transaction;
session frecency is copied under one version; all decay uses the captured evaluation
time. The protocol combines them as `QueryExecutionContext { source:
ProjectQueryLease, ranking: RankingSnapshot }`. A commitment bump linearized after
capture affects later requests only. Ranking can order already-authorized hits but
cannot establish source truth, queryability, completeness, or absence.

There is no active `Degraded` generation. `Current` is the only strict-queryable
variant; every other variant refuses generation-current claims.

Health and generation content are rendered from the same captured runtime snapshot.
Health does not inspect arbitrary `LiveIndex` fields to guess lifecycle. Explicit
disk/Git observations carry their own authority receipts and are never presented as
fields of the selected generation.

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

The manifest itself and every strict manifest entry carry closed proof obligations:

```text
ScopeDiscoveryObligation = {
    binding_id, physical_root, scope, policy_versions, manifest_digest,
    last_verified, next_due
}

EntryVerificationObligation =
    ByteIdentity { digest, last_verified, next_due }          // Indexed
  | ContentDisposition {
        policy_versions, expected_disposition,
        last_verified, next_due
    }                                                         // re-read/reclassify
  | MetadataDisposition {
        policy_versions, expected_disposition,
        last_verified, next_due
    }
```

`Binary`, `SensitiveContent`, `LfsPointer`, unsupported-text encoding, and every
other content-derived terminal disposition use `ContentDisposition`: verification
stable-reads, reclassifies, and discards bytes without persisting or disclosing a
sensitive digest. Path/size/policy-only exclusions use `MetadataDisposition`.
Indexed content uses `ByteIdentity`. Snapshot promotion discharges the scope discovery
obligation and every entry obligation after complete discovery. Rolling verification
fairly revisits both the whole-scope discovery obligation and every entry obligation;
a suppressed create/rename of a previously unknown in-scope path is therefore bounded
by the discovery deadline rather than invisible outside the manifest forever.

Every member is either `Complete` or the candidate cannot promote. A canonical hard
scope/policy exclusion is complete because it is an accounted terminal disposition.
Optional external evidence may be terminal `Unavailable { reason, provenance }` only
when that state is part of the versioned scope contract and every dependent ranker,
formatter, and claim treats it as no evidence, never as evidence of absence.

`Unreadable`, `UnstableDuringRead`, `AbortedCircuitBreaker`, failed or partial parse,
unknown ordering, accumulator gap, derived truncation, stale computation, or a missing
required artifact is not complete. Per-source and per-surface capability certificates
do not weaken this rule in V1; they attest that the closed set above is complete.

The generation-builder Adapter compiles the advertised protocol/catalog version into
one sealed `RequiredArtifactSet`. A full candidate seals a
`CompletenessCertificate { required_artifact_set_id, manifest/proof identities,
artifact identities, optional_delta_proof_digest }`. A delta candidate must
additionally seal:

```text
DeltaProof {
    base_generation_authority_id,
    base_scope_certificate_id,
    required_artifact_set_id,
    artifact_dependency_contract_digest,
    scope_and_policy_versions,
    causes: DeltaCauseSet,
    complete_discovery_diff_digest,
    impacted_closure_digest,
    reused_artifact_semantic_digests,
}

DeltaCauseSet = NonEmptySet<Added | Modified | Deleted | Renamed
                          | DispositionChanged | PolicyChanged
                          | DerivedDependencyChanged>
```

The completeness certificate commits the exact `DeltaProof` digest. Path hints only
select work; they cannot attest reuse. Any unknown cause, repository-global
dependency, version mismatch, or unclosed impact marks `ScopeDirty` and requires a
full candidate. Promotion validates the certificate digest, not protocol topology.
For every edit class **and randomized mutation sequence** the acceptance oracle
compares delta output with a clean full rebuild: canonical manifest, every required
artifact semantic digest/identity, and a representative query corpus must be
identical.

An implementation may omit a future feature only by omitting its tool/resource/
prompt/schema capability from protocol advertisement before the source lifecycle
starts. It may not advertise a surface and later call its incomplete generation
queryable. Temporal, bridge, and authority recomputation after a source change is
candidate work: source state remains non-Current until the new complete bundle promotes.

Frecency and other mutable operational ranking evidence are deliberately outside the
strict source scope and lifecycle Interface. The ranking Adapter captures one
immutable, versioned `RankingSnapshot` after source acquisition and derives one
`EvaluationProvenance` from never-reused persistent-store/session instance identities,
their versions, evaluation time, and ranking policy. Reopen/reconnect mints new
identities even if counters restart. No formatter or ranker may reopen SQLite, session state, or wall-clock
time after that capture. Observable order/scores preserve that evaluation receipt in
the claim envelope. This
preserves commitment-based ranking without turning a frecency bump into source
`Dirty` or a full candidate rebuild.

### 5.2 Typed claim provenance

Primitive facts and derived claims use different closed types:

```text
GenerationAuthority = {
    binding: BindingAuthority,
    observer_epoch, source_version,
    never_reused_publication_id, content_identity,
    strict_scope_certificate_id, manifest_digest,
    path_and_byte_digest: Option<...>
}

AtomicAuthority =
    Generation(GenerationAuthority)
  | DiskObservation {
        binding, physical_root, path, observed_at,
        evidence: Bytes { stable_read, byte_digest }
                | Metadata { file_identity, stamp, size }
                | PathMissing { parent_identity }
    }
  | WorktreeScopeObservation {
        binding: BindingAuthority,
        physical_root: PhysicalRootIdentity,
        scope, policy_versions, scan_id,
        started_at, finished_at,
        observation_cut: { observer_epoch, start_seq, end_seq },
        coverage: Complete { manifest_digest, stable_entry_count }
    }
  | GitObservation {
        repository_id,
        resolved_from: Commit { oid }
                     | Tree { oid }
                     | Ref { name, resolved_oid, observed_at }
                     | Index { checksum, observed_at },
        object: Blob | Tree | Commit,
        membership: Present { object_id } | NotInTree { tree_id },
        path
    }

OperationReceipt = {
    operation_kind, operation_schema_version,
    canonical_argument_hash,
    selector_filter_consistency_ids,
    value_affecting_algorithm_and_policy_versions
}

OperationContractV1 = {
    operation_kind, operation_schema_version,
    allowed_provenance_forms_and_role_cardinalities,
    selection_requirement,
    evaluation_requirement: Forbidden | Optional | RequiredWhenObservable,
    allowed_refusal_variants_and_transport_mappings
}

ClaimProvenance =
    Single(AtomicAuthority)
  | Comparison { operation, relation, left: AtomicAuthority, right: AtomicAuthority }
  | Derivation { operation, nonempty_inputs: NonEmptyVec<ClaimInput> }
  | SelectedAggregate {
        operation,
        selections: NonEmptyVec<SourceSelectionReceipt>,
        generations: NonEmptyMap<ProjectSourceKey, GenerationAuthority>,
        additional_authorities: Vec<AtomicAuthority>
    }

ClaimInput = Authority(AtomicAuthority) | Selection(SourceSelectionReceipt)

EvaluationProvenance = {
    persistent_store_instance_id, persistent_store_version,
    session_incarnation, session_version,
    evaluation_time, ranking_policy_id
}
```

Every public claim and refusal carries one opaque `OperationReceipt` made by
the operation-specific private constructor. It binds the normalized request—not just
the source evidence—to the answer. Thus two search predicates, voice filters,
selectors, consistency modes, or value-affecting algorithm versions cannot share a
claim/cache identity merely because they read the same generation. The canonical
argument hash is the only selector material permitted before membership authorization.
`OperationContractV1` is the checked-in closed table for those constructors. One
private operation-envelope builder derives the receipt from normalized arguments and
constructs the provenance, optional evaluation receipt, or refusal together. It
rejects a provenance form/input role/cardinality not allowed for that operation,
missing or unexpected selection evidence, a receipt for another operation, and an
observable ranked value without the contract-required `EvaluationProvenance`. Callers
cannot assemble these fields independently, and transport status/body mappings come
from the same contract rather than Adapter conditionals.

Every primitive source-truth fact has one atomic authority. Generation authority serializes the
full never-reused binding authority, publication identity, content identity, and
strict-scope certificate identity, so identical manifests or recycled local counters
after same-path rebind/process restart are not replay-compatible cache/CCR identities. A
binary relationship names both sides. A higher-order claim names one closed operation
and every non-empty authority/selection input. `SelectedAggregate` is the only
constructor for global no-match/absence and selection-wide totals:
`SelectedAggregate::from_project_query_lease(...)` requires an exact bijection between
every `(project, source)` in the sealed lease's selection receipts and its captured
generation map, and rejects forged, mismatched, missing, or extra generations. Optional
extra inputs are atomic authorities only; a second uncoupled selection cannot bypass
the bijection. Construction validates the captured lease, not current registry state:
membership, proof, or binding changes after successful acquisition do not discard an
otherwise complete old-scope response. A cache hit begins from a new strict lease and
must match its complete key.
This covers `detect_impact`, whose blast-radius entries project a Git/disk delta
through a generation graph, and heterogeneous untracked-search aggregation without
pretending the aggregate has one authority. Metadata-only estimates and Git
non-membership are representable without inventing byte/blob identity.

Ranking-derived order/debug scores are not source-truth authority. If observable, the
claim envelope carries `EvaluationProvenance` from the one `RankingSnapshot`; caches
either key the complete provenance+evaluation envelope or cache pre-ranking evidence
and re-rank. Formatters, persistence, CCR, and retrieval preserve both. Ranking may
never satisfy readiness, source truth, completeness, or absence.

Generation-authority rules:

- an `Indexed` path uses the immutable bytes owned by that exact generation;
- a disk-backed transport may substitute only after its complete digest equals the
  generation's recorded admitted-byte identity;
- terminal/no-identity/absent paths return a typed generation refusal or
  `NotInGeneration`, never a silent disk fallback;
- only a complete generation may support repository completeness, no-match, absence,
  or negative/global claims.

Disk-observation rules:

- an `ObservationLease` owns the `PhysicalRootLease`, resolves every component beneath
  it with no-follow/reparse-safe semantics, and retains the actually opened file or
  final-parent handle through stable read/metadata/missing-path proof. Pre/post path
  validation is ABA-vulnerable and cannot create `DiskObservation`; unsupported
  platforms refuse, while any path-only fallback is non-authoritative diagnostics
  outside `ClaimProvenance`;
- the receipt and every derived fact come from exactly that buffer or metadata sample;
- it may state path-local observation/non-observation at that time, but never
  generation membership or repository-wide absence/completeness;
- a generation's security/path disposition may conservatively withhold the disk
  observation, but that policy dependency does not make the generation vouch for the
  observed bytes; refusal text names policy authority separately.

Worktree-scope-observation rules:

- a root-bound scanner captures one binding/root and declared scope/policy versions,
  enumerates every path, obtains the required stable entry receipts, and records a
  start/end interval, observation cut, and complete-scan digest;
- incomplete traversal, root/binding change, or unstable required entry refuses the
  aggregate rather than returning a partial receipt labeled complete;
- the receipt can support worktree changed-set totals and risk summaries for
  operations such as `detect_impact`, but it is not Generation authority and makes no
  atomic-filesystem or current-after-interval claim.

Mixed lanes use an operation-specific `ClaimContext`. It captures project/source/
binding, physical-root lease, repository identity, selected `Current` generation
leases, and permitted relationships once. Inputs must be identity-compatible unless
the closed operation explicitly allows a cross-source relation. A rebind cannot
produce Generation(root A) compared with Disk/Git(root B). Pure disk/Git observations
may run while the source is non-Current; any derivation that uses generation structure
requires a `Current` lease and never falls back to retained state. Compatibility is
decided while acquiring the closed context. A rebind between input acquisitions
refuses; a rebind after complete capture does not perform a trailing live-state check
that discards a response derived wholly from the captured old-root authorities.

The V1 lane inventory is explicit: indexed reads are Generation; syntax validation
and untracked search produce per-item DiskObservation when they use disk; immutable
diff modes are GitObservation; uncommitted diff is Comparison/Derivation; Tier-2
disclosure and `detect_impact` are Derivation. Each raw fallback must select one lane
before I/O.

`src/protocol/read_gate.rs` is the initial Adapter seam. It must split generation
resolution from root-bound observation rather than return an untyped `Vec<u8>`.
Response text, structured content, cache keys, persisted/CCR handles, and retrieval
round trips derive from and preserve the same `ClaimProvenance`; clients that ignore
structured content still see the identity in-band.

The public result algebra is one deep Interface. Its refusal variants encode legal
basis/retry combinations rather than exposing a freely combinable product:

```text
Claim<T> {
    value: T,
    operation: OperationReceipt,
    provenance: ClaimProvenance,
    evaluation: Option<EvaluationProvenance>,
    producing_runtime/publication_identity,
}

BoundObservationAttempt =
    { binding: BindingAuthority, physical_root, optional_scan_id }

AdmissionRootAttempt =
    { admission_attempt_identity, authorized_project_source_identity,
      held_physical_root: Option<PhysicalRootIdentity>,
      attempted_root_evidence: AttemptedRootEvidence }

RootAttemptBasis =
    BoundObservation(BoundObservationAttempt)
  | AdmissionRoot(AdmissionRootAttempt)

RefusalBasis =
    GenerationPolicy { generation_authority_id, policy_versions,
                       disposition_receipt }
  | RuntimePublication { publication_id }
  | RootAttempt(RootAttemptBasis)

UnavailableCause =
    NotCurrent { basis: RuntimePublication, retry: Automatic }
  | AuthorityRevoked { basis: Option<RuntimePublication>, retry: OnEvent | Never }
  | CapacityUnavailable { evidence: CapacityEvidence,
                          retry: Automatic | Operator }
  | VerificationOverdue { basis: RuntimePublication, retry: Automatic }
  | ObservationIncomplete { basis: RootAttemptBasis,
                            retry: Automatic | OnEvent | Operator }
  | RootIdentityChanged { basis: RootAttemptBasis, retry: OnEvent | Operator }
  | UnsafePathTraversal { basis: RootAttemptBasis, retry: Operator | Never }
  | RootedIoUnsupported { basis: RootAttemptBasis, retry: Operator | Never }
  | SourceAlreadyOpen { retry: OnEvent | Never }
  | SourceFault { basis: RefusalBasis, retry: Automatic | OnEvent | Operator | Never }

ResolvedSelectionSet = {
    receipts: NonEmptyVec<SourceSelectionReceipt>,
    evidence: ExactBijectionMap<ProjectSourceKey,
        Current { generation_authority_id }
      | Unavailable { cause: UnavailableCause }>
}

AdmissionSubject =
    UnresolvedRequest { canonical_request_hash }
  | AuthorizedAdmission { admission_id, authorized_project_source_identity }

SourceRefusal =
    InvalidSelection { operation, canonical_selector_hash }
  | AdmissionUnavailable { operation, subject: AdmissionSubject,
                           cause: UnavailableCause }
  | SourceUnavailable { operation, source: ResolvedSourceReceipt,
                        cause: UnavailableCause }
  | SelectionUnavailable { operation, set: ResolvedSelectionSet }
```

`ResolvedSelectionSet` is constructed only from one captured, authorized selection.
Its canonical receipts and evidence keys are an exact bijection over every selected
project/source, and at least one entry is `Unavailable`; therefore Feature 020
multi-source and cross-project refusals cannot drop the healthy or failing members.
The healthy entries identify the exact captured generation, while each unavailable
entry owns its reason, basis, and retry. It is not a success/absence claim.

`Loading` and `Refreshing` map to `NotCurrent/Automatic`; `Blocked` maps to its typed
capacity/source/observation cause; project or source `Stopping` maps to
`AuthorityRevoked`; an overdue `Current` first performs the synchronous safety
transition and returns `VerificationOverdue`; a valid `Current` returns `Claim<T>`.
Selection or membership failure is `InvalidSelection` only. Authorization and
existence are tested before any resolved identity is attached; a nonexistent selector
and an unauthorized protected selector have identical refusal shape, status, and
in-band body. Pre-exposure open failures use `AdmissionUnavailable`: an unresolved
request carries only its normalized hash, while `AuthorizedAdmission` may be minted
only after membership authorization. Before a slot/binding exists, that authorized
subject may pair root-related causes only with a sealed non-authoritative
`AdmissionRootAttempt`. Its never-reused identity is minted immediately after
authorization and before any root I/O; initial open failure has `held_physical_root=None`
plus typed attempted/open-failure evidence, while successful capture adopts the same
identity into `PendingProjectAdmission` and later replacement has `Some(held root)`.
The basis cannot enter `SourceUnavailable`, `SelectionUnavailable`, or
`ClaimProvenance` and cannot attest source truth. Once bound, root-related causes use
`BoundObservationAttempt` and cannot masquerade as admission failure. Mutation start
can return the closed root/path causes before any side effect. No Adapter invents an
empty success, generic retry, illegal basis/retry pair, or second refusal vocabulary.
An authorized duplicate embedded open returns
`AdmissionUnavailable { cause: SourceAlreadyOpen/OnEvent }` against the new call's
admission subject. It exposes no handle, close state, existing binding token, or
publication root and never joins ownership of the already-exposed source.
A generation-policy basis
attests only the withholding rule used and never vouches for disk bytes.

## 6. Invariants

1. Strict-current generation queryable means exactly
   `SourceRuntimeState::Current { generation, safety_transition_package,
   next_handoff_publication_package, source_revocation_publication_package }`, with a
   source-bound, physically root-bound, verified, complete generation. Binding and
   observer evidence cannot disagree because no independent side fields exist.
2. Candidates are never discoverable through query-addressable caches, queries,
   checkpoints, hooks, sidecar, resources, or prompts. Pure content-addressed
   memoization may be shared only when exact bytes and policy versions are keys.
3. Identity lifetimes are distinct: `BindingAuthority` has no observer/generation;
   `ObserverToken` adds only observer epoch; `CandidateAuthority` adds cut, captured
   mutation epoch, attempt, and optional base; `MutationAuthority` adds exact Current generation, mutation epoch,
   permit ID, and path scope. Each is accepted or rejected as one value and fields are
   never independently recaptured.
4. Exactly one pending-admission, live-slot, or stopping-tombstone registry entry
   exists per canonical identity, and at most one candidate exists per source slot.
   Pending admission grants no lifecycle authority and can become a live slot only
   after its complete fixed base and revocation packages are charged. Stopping closes the work-registration
   and mutation-permit gates and is non-revivable. Rebind/reopen is strictly
   `Freeze -> Drain -> Install`; no successor authority exists until observers,
   deliveries, permits, queued/claimed/running work, and publication authority are
   quiescent. Project Freeze is one `Live -> Stopping` runtime-root store that also
   closes query and source-add gates before per-source drain; partial per-source
   revocation cannot represent project stop.
   At the outer boundary, one registry `Live -> Retiring` plus inner
   `Live -> Stopping` transition closes embedded-open, server-Adapter, and partition
   factories before process drain. No successor factory incarnation exists before the
   old inner reaches `Stopped`; sequential incarnations reuse the stable process
   capacity domain.
5. Failure, panic, cancellation, or capacity refusal may change runtime work state,
   but never mutates the retained verified generation or manufactures `Current`.
6. Promotion requires the complete closed `StrictScopeContractV1`, physical-root
   continuity, `ObserverCoverage::Complete`, an unchanged observation cut, an
   unchanged mutation epoch, and zero active mutation permits.
7. Overflow, disconnect, accumulator eviction, counter exhaustion, or unknown ordering latches `Gapped`,
   invalidates the candidate, and forces new-observer authoritative re-observation.
8. Every accounted residency group has one charged shared root. A `CapacityGrant`
   atomically converts reserved units into that charge with total
   reserved-plus-charged conserved. Structural aliases reuse it; promotion moves
   candidate roots only. Cancellation never releases live blocking work or reachable
   memory.
9. Checkpoints serialize committed verified generations only.
10. Deserialized snapshots are candidate seeds after restart, never `Current` before
    source/root/manifest discovery, the scope-discovery obligation, every entry
    verification obligation, and observer proof
    complete.
11. Every primitive source-truth fact carries one `AtomicAuthority`; every
    comparison/derivation carries one `ClaimProvenance` naming the closed operation
    and all source/selection inputs. Selected aggregates enforce an exact selection↔
    generation bijection. Observable ranking order/scores carry separate preserved
    `EvaluationProvenance` that cannot establish source truth.
    Generation bytes are owned or digest-identical. Observations cannot establish
    generation membership, completeness, or repository-wide absence.
12. Observer ingress, invalidation-sequence assignment, mutation intent,
    promotion, and strict query acquisition share one defined linearization domain.
    Fallible observation/mutation work stages no live side fact. In the commit section
    Current ingress stores the non-Current runtime root before staged accumulator/
    mutation facts apply. Exact same-binding/token ingress whose source is already
    Loading/Refreshing/Blocked may apply an accumulator-only fact under that same held
    domain; a Gap/observer-phase change still uses its precharged runtime transition.
    Thus no published Current can coexist with advanced side evidence. The runtime-state store is the sole promotion commit;
    acknowledgement/pruning is idempotent and occurs afterward. Runtime and
    invalidation state are never sampled as separate authorities.
13. Promotion performs no capacity wait and no repository-sized allocation; all
    artifacts and capacity are ready before the commit locks are taken.
14. Joining an existing slot never grants protected-root membership. Root
    eligibility and per-session authority precede registry join. Every opener also
    captures a fresh `PhysicalRootLease`; under registry admission its identity must
    equal the pending `RootAdmissionAuthority` or live binding. Pending work retains
    that original lease and transfers it into the initial binding; it never recaptures
    the path. Same canonical path with a different directory object is
    `PhysicalRootReplacement`, not a join, and cannot acquire the old admission or generation.
15. A permanent fault may block progress but cannot manufacture a partial success.
16. Fast file stamps are hints, never durable byte identity. Racy promotion paths are
    stable-reverified; every manifest obligation receives fair bounded rolling
    coverage. Verification ages survive structural sharing. A mismatch or overdue
    deadline atomically makes strict acquisition fail/non-Current.
17. Current/project query leases are request-scoped, owning, read-only,
    non-retargetable, and have no publication authority. They pin only selected
    generations, never wait for capacity or transport while held, and remain charged
    until actual final drop; age never causes unsafe forced expiry.
18. Every `Current` source immutably owns a precharged/preallocated
    `SafetyTransitionPackage` for one safety publication, not a future allocation
    promise. A `SafetyPublicationGuard` stages against it without a destructive
    pre-store take: abort/unwind leaves the published package armed, while the sole
    runtime-root store transfers the same charged residency into the successor state.
    Exhausting ordinary capacity cannot prevent invalidation, mutation intent, or
    verification expiry from making the source non-Current.
19. Destructive mutation authority is tracked and root-handle-bound. Any side effect
    requires later candidate promotion; a proven `NoSideEffect` terminal path that can
    return the same live binding to `Current` uses a fenced no-op candidate and cannot
    restore Current directly. A `Stopping` predecessor may seal/unregister an unstarted
    revoked permit without an unpublishable old-binding candidate; any successor still
    requires a complete fresh candidate. A successor
    binding cannot install while any publication-capable/InFlight permit remains in
    the predecessor Drain set; revoked Granted deallocation-only handles are not
    permits to start or publish and may outlive it.
20. The capacity-pool lock never nests with lifecycle locks. Grant/release callbacks,
    final charged-root drops, and lifecycle work run only after pool and lifecycle
    locks are released; grant acceptance revalidates its exact `CapacityOwnerKey` and
    every derived capacity-domain and lifecycle revocation ancestor.
21. Every identity uses checked non-wrapping progression or retained `Arc` identity;
    modular/object-reuse ABA is forbidden. Invalidation-sequence exhaustion latches
    `Gap` and replaces the observer; observer-epoch or mutation-epoch exhaustion
    freezes/drains and mints a new binding; binding/runtime-epoch exhaustion installs
    a never-reused slot; exhaustion of the outer slot allocator fail-stops rather than
    reusing authority. Process/admission/source-admission, candidate, permit, request, grant-ticket, work, state-
    publication, and drain registrations use checked IDs or retained owning identity.
    Post-store prune exact-matches the complete observer token and is a no-op on
    mismatch. A registered trigger that reaches an exhaustion boundary closes ingress
    and transfers to a separately registered supervisor before terminalizing; it never
    drains its own registration.
22. Every Active observer owns a charged `NextHandoffPublicationPackage`. Successor
    activation cannot publish or open ingress until its own fresh package is installed;
    phase transitions carry the prior remainder. Therefore any Active observer can
    become Draining under exhausted ordinary capacity, including immediately after a
    prior handoff and before first promotion.
23. Publication/callback drain terminality is independent of allocation lifetime.
    Physical allocation begins only under a grant-local
    `AllocationConstructionGuard`, so it remains debited before charge commit. Before
    `ConvertedToDeallocationOnly`, sealing forbids new guards, waits every existing
    guard to commit or final-drop, then exact-once refunds only the remaining available
    units. Conversion strips every callback, enqueue, resize, and publication
    capability and releases the lineage drain registration, while a separate detached
    owner keeps all committed charges and the stable capacity domain alive until actual
    final drop. No detached owner can re-enter a queue or lifecycle transition. An
    executing closure remains in the process executor-join set until exit even if it
    has left a narrower source/project publication drain.
24. Candidate underestimation never waits or requeues while retaining candidate-private
    charges. Resize seal first forbids new construction guards and waits every admitted
    guard to abort/final-drop or commit into cleanup ownership. Cleanup then final-drops
    the complete partial candidate charge set while retaining exactly one sealed old-
    grant/unused-reservation token. Only after guard count and candidate-private charge
    count are both zero may one pool transaction refund that token exactly once and
    either enqueue a fresh all-at-once vector under the same lineage/age or terminalize
    on revocation. V1 has no retained-partial-candidate resize lane.
25. Every seal-pending lineage state owns one retained, helpable `GrantSealDriver`
    before it is visible. The later grant close-cell CAS installs one winning mode/
    driver; a resize/deallocation loser follows the exact winning-mode terminal path.
    Thus a pending-before-CAS window is representable but never ownerless, and initiator
    unwind cannot orphan a seal. No driver belongs to the predecessor Drain set it may
    need to finish, and no caller waits on a construction guard it still owns.
26. A canonical embedded source-registration key has at most one exposed handle owner.
    Duplicate authorized opens refuse without close authority; only that sole handle or
    process shutdown may begin its idempotent source close. A later reopen mints an
    incomparable source/binding incarnation after the prior close receipt is terminal.
27. Legacy query execution, response-cache get/put, CCR lookup/store/retrieve, and
    response finalization register under one exact activation/mode epoch. Activation
    closes registration, drains every admitted operation, invalidates legacy state,
    and only then publishes/opens PreventiveV1. No lookup, late write, or retrieval can
    straddle that cut unregistered.
28. Process factory owner tokens exact-match one never-reused incarnation. `Clone`
    under Live increments that incarnation; clone under Retiring/Stopped creates only
    an uncounted terminal token. Release of any old token cannot decrement, retire, or
    attach to a successor.

## 7. Preventive load and catch-up protocol

```text
validate root eligibility and this session's membership authority
        |
        v
mint never-reused AdmissionAttemptIdentity (non-authoritative, before root I/O)
        |
        v
capture fresh PhysicalRootLease; on failure return authorized AdmissionUnavailable
with held_root=None + typed attempted/open-failure evidence; compare any live binding
identity under registry admission
        |
        +-- same path/different object --> PhysicalRootReplacement Freeze -> Drain -> Install
        |
        v
create/join process-charged PendingProjectAdmission adopting the attempt identity and
owning that exact root lease;
fresh join lease must match or PhysicalRootReplacement refuses/cancels (single-flight)
        |
        v
atomically reserve/charge complete project fixed base + project revocation package
+ initial source base/revocation package + bounded scout/accumulator base
        |
        +-- unavailable --> wait/cancel in pending record without ProjectSlot,
        |                   watcher, source authority, or candidate allocation
        |
        v
under process-registry writer, install-vs-cancel exact-CAS; winning install revalidates
process Live and transfers root lease + DrainRegistration -> LiveProjectSlot;
late/losing grant after cancellation refunds
        |
        v
register observer into bounded accumulator; mint observer epoch
        |
        v
capture complete observation cut S0 = { observer_epoch, invalidation_seq }
        |
        v
perform bounded scout, compute conservative net-new residency + peak scratch charge
        |
        +-- full reservation unavailable --> discard scout allocation, queue request
        |
        v
atomically reserve full vector; build isolated full/delta candidate
        |
        v
stable-read/reclassify every required obligation; build every advertised artifact
        |
        +-- cut changed --> coalesce latest hints and rebuild/retry
        +-- Gapped coverage --> discard candidate, register/reobserve fully
        |
        v
authoritative final scout + racy-clean obligation verification
        |
        v
prebuild charged persistent runtime update + one armed safety-transition package
+ one Active-owned next-handoff publication package
        |
        v
take accumulator then publication writer:
require unchanged cut/mutation epoch, zero permits, Complete coverage,
current binding/physical root, complete strict scope, charged roots, and—when
scope_dirty is present—a full candidate certificate covering its sequence/versions;
perform one non-fallible runtime-root store of Current(candidate)
        |
        v
after unlock: idempotently acknowledge/prune hints through cut and drop retired roots
        |
        v
next observation uses the armed package and atomically publishes Refreshing
```

The observer never mutates bootstrap or committed state in place. Ingress first
transfers the event plus its `DrainRegistration` into a lifecycle-owned
`IngressEnvelope` outside the processing unwind boundary; a stack-local panic cannot
acknowledge or terminal-drop it. Processing under `catch_unwind` builds a
`PendingObservation` in preallocated storage owned by that envelope; it does **not** advance
the live sequence, latch coverage, or coalesce hints. Under accumulator then writer,
all fallible validation/filling completes while both published accumulator and runtime
remain unchanged. It then enters a no-allocation/no-unwind commit section with a
state-sensitive rule. If the exact source is `Current`, first use its armed package to
store `Refreshing`, making every lock-free strict query refuse, then apply staged
sequence/coverage/hints. If the exact same binding/token is already Loading,
Refreshing, or Blocked, strict reads already refuse, so apply the staged accumulator
fact without spending or requiring another runtime publication; only a Gap that
changes observer phase uses the precharged handoff transition. Stopping or token
mismatch refuses/terminalizes the stale delivery. Health/composite readers hold the
accumulator and cannot observe an internal Current/ahead interval. The
`SafetyPublicationGuard` retains the Current package until its sole runtime store,
which transfers the same charged residency.

An error, panic, or failpoint before commit changes neither published structure nor
envelope ownership. Only the outer lifecycle owner may retry that exact envelope or
consume its precharged `Gap` transition; it acknowledges/terminalizes the registration
only after one of those commits. `observe` cannot return having silently dropped it.
An unexpected post-store failure leaves the source non-Current and the still-owned
delivery forces `Gap`; it can never leave accumulator-ahead/Gapped state beside
published `Current`. Mutation-epoch advancement and permit registration use the same
staging rule: no live permit fact exists and no permit is returned before the
`Refreshing` store; the remaining infallible registration completes before unlock.
Ordinary capacity exhaustion cannot delay either revocation. Promotion takes the same
locks in the same order. A scalar `fetch_add` outside this domain is forbidden because
check-then-publish would lose a delivered invalidation. Checked sequence exhaustion
stages `Gapped`, retires the observer, and requires a new incomparable observer epoch
rather than wrapping.

Post-store acknowledgement/pruning is non-semantic bounded compaction. It exact-
matches the complete `ObserverToken`, removes only coalesced hints whose
`latest_seq <= committed_cut`, and monotonically sets
`acknowledged_seq = max(acknowledged_seq, committed_cut.seq)`. It preserves any `Gap`
and every newer hint. A full promotion may clear `scope_dirty` only when the marker's
`latest_seq <= committed_cut.seq` and the promoted completeness certificate covers its
recorded scope/policy versions; a newer or mismatched marker remains. The operation is a
no-op on token mismatch. Failure or repetition may retain bounded hints and force a
later conservative gap/full scan; it cannot change which generation was committed or
erase uncommitted observation evidence.

The accumulator is deliberately not an ordered filesystem log. For full candidates,
any changed sequence invalidates the attempt. For delta candidates, the bounded
`Path -> latest_seq` hints select work against the latest stable disk state; final
authoritative scope/byte verification plus a lifecycle-sealed `DeltaProof` over the
versioned `RequiredArtifactSet` proves the result. Reused symbol/reference/bridge/
authority/temporal or other derived shards are legal only when the dependency
contract's impacted closure proves them unchanged; unknown/global dependency latches
the sequence-tagged `scope_dirty` marker and forces a full candidate. Delta admission
is forbidden while the marker exists. Repeated markers monotonically take max sequence,
union causes, and join required scope/policy versions; coalescing cannot forget an
earlier proof obligation. Rename ambiguity is
coalesced delete/create, and any overflow, disconnect, eviction, unknown ordering, or
scope ambiguity latches one constant-size `Gap` or sequence-tagged `scope_dirty`
marker and forces a full
candidate. Observer epochs make cuts from different registrations incomparable.
`ObserverToken` contains no base generation and remains valid across promotion. A
valid event queued before G0->G1 but delivered afterward therefore advances the cut
and makes G1 non-Current rather than being rejected as stale work.

The normative single-source lock order is `InvalidationAccumulator` first, then the
`ProjectSlot` publication writer. No path acquires them in reverse. Capacity,
registry, and drain locks never nest either lifecycle lock; callbacks and
capacity-returning/final-Arc drops occur after unlock. Strict project query acquisition
loads one runtime snapshot, clones only explicitly selected `Current` generation
roots and lightweight epochs, drops the project snapshot, and acquires no lifecycle
lock. The ranking Adapter may then use a SQLite read transaction or session-version
lock but never a lifecycle lock.

Close/rebind is `Freeze -> Drain -> Install`:

1. **Freeze** acquires only the publication writer, validates that the slot is `Live`,
   fills its root-agnostic precharged `RevocationPublicationPackage` from the **latest**
   persistent source-map root without allocation, and performs the one non-fallible
   `Stopping` store that closes query/source-add/work/permit gates. It never retries on
   whole-project root identity, so continuous unrelated source publications cannot
   starve revocation. An unwind before store leaves the package armed; no ordinary
   capacity is required from any source state. It releases the writer and mints no
   successor authority.
2. **Drain** visits sources in canonical `SourceId` order, holding at most one
   accumulator and never waiting while a lifecycle lock is held. It covers queued,
   claimed, registered/running work, timers, observer deliveries, candidates, and
   destructive permits. Work follows `Queued -> Claimed -> Registered/Running ->
   Terminal`. Every async unit acquires an owning `DrainRegistration` **before** queue,
   timer, callback, or delivery exposure and carries it unchanged through dispatch,
   claim, run, terminal/refund. Claim atomically registers against the old slot or
   refunds against the closed gate. Drain waits those registrations. A survivor may
   outlive only after atomically converting to charged deallocation-only authority
   with no lifecycle callback/publication path. No callback resolves a canonical
   project key after capture.
   At the Freeze writer cut, every still-`Granted` mutation permit is atomically marked
   `RevokedSealPending`, so its later start deterministically refuses, but its
   registration remains in Drain. After releasing the writer, the lifecycle supervisor
   seals its grant: no new construction guard can begin, existing guards commit or
   final-drop, and available units refund exactly once. Only then does it transfer
   committed charges/root ownership into deallocation-only authority and release the
   registration. Sealing/refunding never occurs under the publication writer. Already-
   `InFlight` permits remain in the waited predecessor set until their destructive
   authority is terminal.
3. **Install** reserves/charges successor observer state and atomically installs one
   never-reused slot/binding only after drain. A capacity refusal leaves no half-
   installed successor.

A registered unit may trigger Freeze or observer/binding/mutation rollover, but it may
never synchronously wait on a Drain set containing its own registration. The trigger
may close ingress and publish non-Current, then hands an already-registered control
ticket to the lifecycle supervisor and terminalizes (or atomically transfers) its own
registration. That precharged lifecycle-owner/reaper control authority is explicitly
outside the predecessor Drain set; it cannot publish successor authority before the
set reaches zero, but Drain never waits on the supervisor itself. Only that supervisor begins Drain/Install. This applies to overflow at
`MAX`, physical-root replacement discovered inside a watcher callback, retry-driven
rebind, and source/project close from managed work.

If per-source drain progress must publish, it takes that accumulator then the writer
and releases both before the next source. It is forbidden to acquire an accumulator
while holding the writer or another accumulator, or to wait/join while either is held.

SymForge-owned writes use a tracked non-cloneable `SourceMutationPermit` that owns the
binding's live `Arc<PhysicalRootLease>`. Granting it advances `mutation_epoch`,
invalidates candidates, and publishes `Refreshing` before any disk side effect.
Permit state is closed and publication-owned:
`Granted -> InFlight -> Terminal` or
`Granted -> RevokedSealPending -> RevokedDeallocationOnly`.
Grant prebuilds the small charged runtime delta needed for its InFlight mark.
`start_side_effect` performs fallible root/path preparation first, then under the
project writer exact-checks the Live gate, exact source publication, permit identity/
Granted record, binding, and pinned-root authority and atomically stores the
`Granted -> InFlight` source delta. It releases the writer before I/O. Freeze before
that store atomically records `Granted -> RevokedSealPending` and makes start refuse;
after unlocking, the supervisor follows the generic grant-seal/construction-guard/
deallocation conversion and only then releases the registration. Freeze after the
InFlight store drains the recorded destructive permit. No check-then-mark side field exists. The InFlight authority then performs
handle-relative beneath-root resolution of **every** path component with no-follow/
reparse-safe semantics, ending at a validated final-parent handle. Temp creation,
write, and atomic replacement all operate through that parent; passing a
multi-component relative string to a directory handle is insufficient. A platform
without an equivalent to Linux `openat2` containment or Windows reparse-safe
component traversal refuses destructive I/O. A successful commit schedules a candidate. Drop, panic,
failure, or rollback after any side effect remains non-Current until verified
promotion. A terminal proof that **no side effect began** permits the cheaper no-op
verification candidate, but never direct restoration: it must validate the latest
observer cut and monotonic mutation epoch and publish a fresh safety package.
Close/rebind cannot install a successor while any publication-capable/InFlight permit
remains in the predecessor Drain set. A revoked Granted deallocation-only handle may
outlive the tombstone but cannot start, write, or publish.

Three operations are distinct:

- `RetargetProposal` builds/adopts a different-root slot without changing the
  session's current binding. It owns
  `RetargetAuthority { session_incarnation,
  expected_session_binding_publication_identity, expected_old_binding_revision,
  proposal_id, target_slot/source/current_identity }`; target membership is
  provisional and cannot answer the session's queries. Commit revalidates target
  Current identity and CASes one immutable, never-reused
  `SessionBindingPublication` from the exact expected Arc/revision to the new target.
  Old-project cleanup is non-authoritative bookkeeping. Concurrent A→B/A→C,
  reconnect with the same visible session ID, target invalidation, or idempotent replay
  therefore has one winner; stale/superseded proposals can only release provisional
  membership. Proposal failure leaves the old binding/observer/generation untouched.
- `CurrentSourceReset` intentionally moves the same source to `Refreshing`.
- `PhysicalRootReplacement` immediately freezes old authority, cannot retain it as
  Current, drains it, then installs a new binding through the full protocol.
  Additive `index_folder(add=true)` is source membership addition, not a retarget CAS.

V1 observer replacement deliberately avoids a two-live-observer translation protocol.
The trigger's one atomic commit uses T0's owned
`NextHandoffPublicationPackage` to close logical T0 ingress, latch handoff
`Gap`, stores `Loading { observer_phase: Draining(T0) }` when no generation is retained
or `Refreshing { observer_phase: Draining(T0), retained: G }` (or preserves
`Blocked { observer_phase: Draining(T0), ... }` while
an independent operator cause remains), and arms/transfers a separately registered
supervisor handoff ticket; the triggering
callback becomes terminal before Drain can wait. The ticket is lifecycle-owner
authority outside the predecessor Drain set. Supervisor cutover drains every already-admitted delivery/
callback. Only after the predecessor is terminal may its base charge release; the
precharged transition then stores `ObserverFree` before successor admission. No staging
successor token survives that barrier: any provisional token is retired as gapped. The
lifecycle then reserves/builds a **new incomparable post-barrier ObserverToken**,
`BaselinePending` accumulator, and T1-owned `NextHandoffPublicationPackage`; inability
to reserve the entire successor observer base leaves ObserverFree. Once complete it acquires an owning
`ObserverActivationRegistration` under the still-open source work gate before any
Active publication or external callback exposure. Under the normal fence it publishes
`Active(T1)` referencing that installed accumulator. Out of lock it opens the OS
observer with a closed logical callback gate, then exact-revalidates the Live/source/
T1 publication and atomically converts the activation registration into the persistent
observer control registration while opening the logical gate. If Freeze/revocation
won, it closes the OS observer and terminalizes; Freeze waits the activation/control
registration before Install. No callback enters lifecycle ingress through the closed
gate. It then performs a full authoritative baseline/final scout before capturing an unchanged cut.
External ingress may never become logically active before the Active store and exact
post-open revalidation. Changes before ingress opens are
covered by that baseline; changes after opening advance T1. Activation failure stays
non-Current and latches Gap where observation may be incomplete. Only this post-barrier
`Active` Complete epoch may promote. Capacity refusal leaves
the source `ObserverFree` with retained G and an armed retry trigger, never holding the predecessor's
only releasable base. No predecessor callback exists after new registration or the
successor `Current` store. Revoked deallocation-only tasks and immutable/query-pinned
generations may outlive the tombstone because they cannot publish; their charges
remain until actual drop.

The three handoff runtime publications are therefore exact: (1) semantic-state-
preserving trigger commit with `observer_phase=Draining(T0)`, (2)
supervisor post-drain commit to `ObserverFree`, and (3) successor
pre-ingress commit to `Active(T1/BaselinePending)`. No hidden phase transition exists
between them.

Immediately before promotion, the candidate must revalidate source identity and the
physical directory object behind the canonical root. Same-path directory replacement
is a rebind: observer, cut, candidate, and old tokens are invalid. Where a platform
cannot prove directory continuity, promotion fails conservatively and starts a new
observation. The physical-root adapter uses an open-handle file identity (for example,
volume/file ID on Windows and device/inode on Unix) rather than path text.

Filesystem notifications are not a durable ordered log. The observation cut is
process-local coordination only. Every detected gap forces reconciliation; no resume claim crosses
process restart without a separately verified source observation.

Fast stamps are optimization hints, not byte identity. Stable reads record digest or
classification proof, stamp, and observation window. The promotion barrier rechecks
every obligation whose stamp may collide with that window; unknown platform timestamp
granularity is racy. Snapshot seeds perform full discovery and discharge the
`ScopeDiscoveryObligation` plus every `EntryVerificationObligation` before promotion;
the current fixed sample is insufficient.

Before promotion, the source owns fixed scope-enumeration/streaming buffers, execution
allowance, and process-monotonic per-obligation deadlines under a versioned
rolling-verification policy. The fair cursor covers root-scope discovery, byte
identities, and content-derived terminal reclassification; it carries progress rather
than restarting at path zero. Structurally shared obligations retain prior
verification age—delta promotion resets only newly verified inputs.

That progress is lifecycle-owned, bounded, and non-authoritative:

```text
VerificationProgress {
    expected_source_publication: SourcePublicationToken,
    work_id, pass_id, cursor,
    staged_proofs: ChargedBoundedProofSet,
}
```

The lifecycle supervisor preserves it across cancellation/retry only while the exact
source publication and WorkId still own the pass; any publication change discards it.
No query, cache, or formatter can observe it, and it cannot renew a deadline. This is
the fair-work cursor, not a mutable proof ledger.

Successful unchanged verification never mutates `VerifiedGeneration` or an
authoritative side ledger. The lifecycle owner stages a charged `ProofRefreshCandidate` against the exact
Current binding, observer cut, mutation epoch, generation/publication identity, and
owning `VerificationProgress { expected_source_publication, work_id, pass_id }`. It
reuses content identity, advances publication identity,
updates only immutable proof evidence, and installs a successor safety package. The
normal accumulator-to-writer fence publishes it as one new `Current` runtime root;
concurrent queries see wholly old or wholly refreshed proof. The worker is registered
in the lifecycle work gate but is not a second publication authority. A stale fence
discards it. Mismatch/instability instead advances invalidation and publishes
non-Current. If a timer/worker is starved, strict acquisition checks both scope and
entry deadlines. Only the fresh-deadline path is lock-free. On an overdue observation,
the caller drops its captured runtime/generation snapshot, enters the owning
accumulator-to-writer domain, and uses the armed `SafetyPublicationGuard` to publish
non-Current before returning `SourceRefusal`; if a concurrent proof refresh won, it
retries acquisition from the new runtime root. Checkpoint and every other `Current`
consumer use this same acquisition path. `VerificationOverdue` is health/failure evidence. The
release configuration must set a finite full-coverage bound and prove the verifier's
reserved budget can meet it for every admitted corpus; it cannot ship as “best
effort.”

“Current” is not filesystem-linearizable. It means internally consistent and caught
up through all observations delivered before the publication cut. A filesystem event
may be delivered after a query acquires its lease; every response therefore names the
captured claim provenance. Periodic authoritative reconciliation and rolling
obligation verification remain mandatory. This provides bounded eventual detection, not a false
claim of cross-platform filesystem-linearizability.

## 8. Capacity prevention

Per-load ceilings remain useful against individual pathological inputs, but they are
not admission control.

The public factory and the child control lifetime are deliberately distinct:

```text
ProcessFactoryRegistry {
    stable_capacity_domain: Arc<ProcessCapacityDomain>,
    state:
      Vacant
      | Live { factory_incarnation, public_owner_count,
               inner: Arc<ProcessRuntimeInner> }
      | Retiring { factory_incarnation, inner, shutdown_receipt }
      | Stopped { last_factory_incarnation, shutdown_receipt }
}
ProcessIndexRuntime { owner_token: FactoryOwnerToken {
    factory_incarnation, registry, incarnation_control_and_shutdown_receipt
} }
ProcessControlLease(Arc<ProcessRuntimeInner>)
ProcessRuntimeState = Live | Stopping | Stopped
```

The process Module owns this persistent registry independently of public wrapper
lifetime. Its stable capacity domain/parser base does not reset between sequential
factory incarnations and remains the ledger for escaped old charges. Only public
`ProcessIndexRuntime` wrappers own `FactoryOwnerToken`; cloning/dropping a wrapper
linearizes under the same process-registry writer. Cloning a token while its exact
incarnation is `Live` mints one new counted owner of that incarnation. Cloning after
the token observes `Retiring` or `Stopped` is still infallible but mints only an
uncounted terminal token bound to the same shutdown receipt; it cannot register work
or contribute to a future owner count. Drop/release consumes one never-reused token ID
and changes a count only if that exact token is still recorded in that exact Live
incarnation, so an old clone/drop is a terminal no-op against any successor. Embedded
handles, server Adapters, child partitions, managed tasks, responses, and finalizers
retain `ProcessControlLease`/capacity roots, never an owner token. The decrement from
one owner to zero atomically changes registry `Live -> Retiring` and inner
`Live -> Stopping` **before** the token becomes undiscoverable, then transfers the same
drain/receipt to the non-self-joining finalizer. Explicit shutdown invokes that
identical transition. A constructor serialized on the registry joins Live, or
joins/refuses Retiring; it cannot interpret a dead weak pointer as vacancy. A successor
incarnation installs only after the predecessor reaches inner `Stopped` and all
publication/destructive work and executors have drained. It reuses the same stable
capacity domain even while immutable responses/charges from the predecessor remain.
There is no `Arc<Self>` ambiguity, successor-authority window, parser-base duplicate,
or capacity reset.
An explicit shutdown may reach Stopped while revoked wrappers of that old incarnation
still exist. Their tokens retain only the old terminal receipt/control and cannot
register or clone authority into a successor; repeated shutdown returns that receipt.
A successor has a new incomparable incarnation/count on the shared domain.

Embedded-source open, server-Adapter creation, and child-partition minting acquire a
process `DrainRegistration` and register under the inner `Live` gate before any
handle/token is exposed. Shutdown's one store closes all three factory gates before it
snapshots/drains registered sources, adapters, partitions, reapers, and executors. A
racing creation is therefore registered and included or receives `AuthorityRevoked`
with exact-once refund—never between enumeration and executor join. After drain it
stores terminal inner `Stopped`; repeated/concurrent shutdown joins that incarnation's
same receipt. Only the persistent registry may later mint an incomparable successor. No
ordinary capacity is needed for the revocation store.
The `ProcessRegistryWriter` is the sole linearization domain for factory gate changes
and PendingProjectAdmission install/cancel. Shutdown stores Retiring/inner Stopping and
changes every exact pending Open entry to Cancelling before releasing it; an installer
that holds it can commit LiveProjectSlot first only while the process remains Live, in
which case shutdown subsequently Freezes that registered slot. No capacity-pool lock
nests the process-registry writer.

Every entry point in a process uses one public factory backed by the one Live/Retiring
`ProcessRuntimeInner`; scheduling is a private CapacityPool Implementation detail
inside the stable domain. The process registry strongly retains the domain and current
inner/tombstone but never a public owner token, so final-wrapper Drop works without
making the old incarnation disappear from discovery. A daemon shares it across
project slots; local stdio and standalone serve join it. V11 exposes no independent embedded factory that can construct a second
default pool: embedded sources are non-cloneable handles opened from
`ProcessIndexRuntime::open_embedded_source(&self, ...)` and retain only a control
lease. A host that deliberately isolates embedded
runtimes must provide an `ExternallyPartitionedCapacityToken` minted from one parent
`ProcessCapacityDomain`; partition creation atomically debits the parent and child
budgets cannot sum above it. Without one of those two authorities the preventive
guarantee refuses construction rather than making a local budget look process-safe.
The partition token is retained by the child-domain `Arc`, and every child grant,
charge, blocking envelope, response, snapshot, and alias retains that Arc. Parent
units return only after the final child owner actually drops—not when an
`EmbeddedSourceHandle` closes—so escaped response generations or blocking work
cannot outlive their aggregate charge.

The capacity pool has distinct immutable configuration classes:

- per-project logical catalog/content ceilings;
- process persistent residency budget;
- scratch/transient build and checkpoint budget;
- observer/invalidation-accumulator and precharged handoff-phase-publication quota;
- rolling-verifier and armed safety-transition-package quota;
- per-source revocation-publication-package quota, armed in every source state;
- project-slot revocation-publication-package quota;
- process-runtime revocation/finalizer/reaper base quota;
- reserved runtime and allocator headroom.

Capacity accounting is exact over **accounted residency groups**, not claimed exact
RSS. A group is a root allocation or conservative arena whose whole class-vector
charge can be owned/tested. Opaque dependencies and per-worker parser state use
versioned conservative envelopes plus process headroom.

`CapacityGrant` owns reserved class units. Before any allocator or opaque constructor
runs, `begin_allocation(vector)` atomically moves those units into one non-cloneable
`AllocationConstructionGuard`; physical residency may become live only under that
guard. `guard.commit(group)` atomically converts its in-construction debit into one
`AllocationCharge` stored in `Arc<ChargedAllocation<T>>`; total available-reserved plus
in-construction plus charged is unchanged. Abort/Drop owns and final-drops any physical
group **before** returning the debit. Grant sealing forbids new guards and waits every
existing guard to commit or final-drop before refunding its remaining available units.
Aliases clone the charged Arc. Structural shares keep their old
charged roots, so candidate admission reserves only net-new unique residency plus
peak scratch/publication and coexistence—not a duplicate generation total. Promotion
moves candidate roots and performs no capacity operation. Growth/reallocation commits
the successor group while the predecessor charge is still live; the overlap is part
of the grant, then the predecessor releases after ownership switches.

The grant's close cell is one monotonic CAS:
`Open -> ResizeSealing(driver) | DeallocationSealing(driver) -> Sealed(mode)`. The
winning CAS installs a retained, precharged `GrantSealDriver` in the cell; it never
leaves winning mode owned only by the initiating stack. The lifecycle installs that
driver/control obligation in the `WorkLineage` **before** publishing any seal-pending
state. Its pre-registered lifecycle control ticket is already claimable by the
supervisor before that store; publishing pending does not depend on a later wakeup or
allocation by the initiating caller. The driver is outside the predecessor publication Drain set, and every
supervisor/joiner can idempotently help it. Panic or cancellation before/after the CAS,
at a guard wait, after the last guard, during root cleanup, or before the pool
transaction therefore leaves a live owner that completes the winning mode. A losing
closer joins/helps the winning terminal; no second seal/refund exists.
`ResizeSealing` retains the one available-reservation token for cleanup-complete
requeue/terminal, whereas `DeallocationSealing` refunds available units after all
guards settle. If Freeze loses to an existing resize seal, it records the tombstone and
the resize driver must finish guard/root cleanup into `ReturnedTerminal`; if
deallocation seal wins first, the resize-pending lineage takes its explicit
deallocation-winner edge and no resize successor may be minted. Before asking a driver
to wait, the triggering unit commits, drops, or transfers every construction guard it
owns; no driver can wait on its own caller-held guard.

The versioned conservative candidate vector includes new catalog/content/derived
groups, changed persistent-map nodes, bounded parser worker envelopes, verification
state, an armed safety-transition package, an Active-owned next-handoff package,
observer replacement/overlap base, and peak
scratch. Checked scout counts/sizes feed the formula. If construction would cross its grant, it aborts before the next allocation,
returns charged roots safely, and re-enters admission with a larger all-at-once
request; it never waits while holding a partial candidate grant.

Process construction commits the process revocation/finalizer/reaper base charge. A
process-base-charged `PendingProjectAdmission` is the single-flight join cell while one
capacity transaction commits the complete project base/project package and initial
source base/source package; only its successful registry CAS exposes a `ProjectSlot`.
Later source-slot admission likewise commits one complete source base/package before
inserting that source. These fixed safety charges remain in the accounted ledger
through their actual final control-block/reaper drop and cannot be borrowed for
ordinary work.

Admission rules:

- obtain a fixed base reservation before observer registration or source enumeration;
- after bounded scouting, atomically reserve the complete net-new candidate vector;
- never block waiting to grow while retaining a partial candidate reservation; on refusal,
  discard scout allocations, release them, and enqueue the request;
- use a cancellation-aware multidimensional **oldest satisfiable** scheduler over
  explicit capacity keys; each source has one coalesced request and ticket age
  survives resize/retry;
- a bypass increments every older conflicting waiter. At its bound, the oldest
  otherwise feasible waiter receives a drain barrier: released conflicting units
  accrue for it and younger conflicting grants stop, while disjoint-key work proceeds.
  A live pin/non-cancellable blocker parks the waiter as `PinnedResidency` without
  blocking disjoint/fitting followers; the final pin release arms its barrier before
  younger selection;
- reject arithmetic impossibility using **mandatory non-reclaimable residency plus
  incremental peak**, not request alone. If retained G plus a required replacement
  cannot fit and no legal pre-promotion release exists, publish
  `Blocked { InsufficientReplacementHeadroom, action }`. Pure temporal pins remain
  retryable Waiting. Every Waiting state must name an independent capacity-changing
  trigger;
- coalesce watcher bursts and build structurally shared delta candidates so one file
  event does not require an unbounded repository-sized clone;
- prebuild all derived publication artifacts before commit; commit performs token
  validation and pointer transfer only.

Before capacity enqueue, the lifecycle closes the work-registration race by minting
one owning `WorkLineage { WorkId, DrainRegistration, CapacityOwnerKey,
original_scheduling_age }`. Owner identity is a closed product:

```text
CapacityDomainAxis =
    ProcessRoot { process_incarnation }
  | ChildPartition { process_incarnation, partition_identity }

CapacityLifecycleOwner =
    ProcessWork { work_identity }
  | PendingProjectAdmission { admission_identity }
  | ProjectWork { slot_instance_id, work_identity }
  | PendingSourceAdmission { slot_instance_id, source_admission_identity }
  | SourceWork { binding: BindingAuthority, work_identity }
  | ServerAdapter { adapter_identity }
  | DomainControl { work_identity }

CapacityOwnerKey { domain: CapacityDomainAxis, owner: CapacityLifecycleOwner }
```

Each value derives—not accepts from its caller—the canonical ancestor
`RevocationScope` chain: stable process and exact capacity domain/partition; optionally
PendingProject or Project; optionally PendingSource or exact full Source binding; and
exact Adapter where applicable. Thus a partition close matches every source/workspace
lineage inside that domain, while a source close cannot alias another source. Adapter
or admission setup terminalizes its lineage before lifecycle-owned source work mints a
new lineage on the same domain axis; owner identity is never mutated as a transfer
shortcut. Admission/work identities are checked non-reused retained Arcs. A later
source admission registers `PendingSourceAdmission` in the project inventory before
capacity enqueue, so project Freeze owns it before a SourceSlot exists.

Every request, `GrantTicket`, accepted reservation, and resize successor retains the
exact `CapacityOwnerKey`. One lineage cycles through immutable request attempts:

```text
RegisteredQueued -> Dispatching(GrantTicket) -> Accepted(reservation)
Accepted -> Running | ResizeGuardSealPending | ReturnedTerminal
Running -> ResizeGuardSealPending | ReturnedTerminal | DeallocationSealPending
ResizeGuardSealPending { old_grant, complete_new_vector, seal_driver }
  -> ResizePendingCleanup { sealed_old_grant, all_candidate_private_roots,
                            complete_new_vector }
  | DeallocationSealPending { winning_driver, resize_successor=Forbidden }
ResizePendingCleanup { sealed_old_grant, all_candidate_private_roots,
                       complete_new_vector }
  -> ResizeReadyAfterDrop { cleanup_complete_proof, sealed_old_grant,
                            complete_new_vector }
ResizeReadyAfterDrop { cleanup_complete_proof, sealed_old_grant,
                       complete_new_vector }
  -> ResizeRequeued | ReturnedTerminal
ResizeRequeued -> RegisteredQueued(next immutable attempt)
DeallocationSealPending
  -> ConvertedToDeallocationOnly | ReturnedTerminal | <remain drain-registered>
ConvertedToDeallocationOnly { detached_charge_owner } -> <control terminal>
```

On candidate underestimation, the initiating unit first commits, drops, or transfers
any construction guard it owns. The lifecycle then installs a retained
`GrantSealDriver`, enters `ResizeGuardSealPending`, and asks that driver to invoke the
grant-local `seal_for_resize` linearization shared with deallocation seal. If resize
wins, it atomically forbids new construction guards, then waits every already-admitted guard:
abort owns/final-drops its physical group, while commit transfers its charge into the
cleanup-owned candidate-root set. Unlike deallocation seal, resize seal does **not**
refund the remaining available units. Only after guard count reaches zero does it enter
`ResizePendingCleanup` with the sealed old grant and every candidate-private committed
charge. Cleanup owns/final-drops all those roots outside lifecycle and pool locks, while
deliberately retaining the sealed old grant's one unused-reservation token. Freeze or
cancellation during either phase records its tombstone but cannot bypass the guard
barrier or root cleanup. Cancellation/revocation cannot take a direct terminal edge;
only final drop mints the unforgeable proof in
`ResizeReadyAfterDrop { cleanup_complete_proof, sealed_old_grant, ... }`. That state
alone may, under the pool lock, atomically consume/refund that reservation exactly once, check every
tombstone, and either enqueue the complete larger immutable vector or terminalize,
while preserving the same live
`WorkId/DrainRegistration`, exact owner, and original scheduling age. V1 has no path
that calls a vector “all-at-once” while retaining its partial candidate; mandatory
active/base residency is outside this lineage and already participates in the
replacement-headroom arithmetic. Resize is not terminal.

If `DeallocationSealing` wins before the resize driver, the driver discards the larger
vector and atomically moves `ResizeGuardSealPending -> DeallocationSealPending` using
the exact winning-driver receipt from the grant close cell. It neither owns nor
fabricates a resize-sealed reservation. The deallocation driver settles every admitted
construction guard, exact-once refunds the remaining available units, and either
final-drops publication-capable roots or transfers only capability-stripped charges to
`DetachedChargeOwner` before the lineage reaches a real terminal. This is the sole
losing-resize edge; it cannot enqueue a successor or refund again.

`ReturnedTerminal` has no successor and releases the registration after exact-once
refund and after publication-capable owned state is gone. If revoked non-cancellable
work, a response, or a permit may retain real charges longer, it first enters
`DeallocationSealPending` while remaining drain-registered. `CapacityGrant` exposes one
grant-local non-fallible `seal_for_deallocation`: sealing closes admission of new
`AllocationConstructionGuard`s, waits all previously admitted guards to commit their
groups or own/final-drop them, then refunds only still-available grant units exactly
once. Thus physically live memory remains in the reserved/in-construction/charged sum
throughout. The lifecycle-owner supervisor performing the seal is outside the waited
predecessor set; a triggering unit that owns a construction guard must commit, drop, or
transfer that guard before terminalizing, so sealing never waits on itself. Only after
the grant is sealed and no construction guard remains may
`ConvertedToDeallocationOnly` strip every callback/publication/
enqueue/resize capability, release the `DrainRegistration`, and transfer all committed
charges plus the capacity-domain `Arc` into an independent `DetachedChargeOwner`. Matching
tombstones may regard that lineage terminal; the stable ledger remains debited until
the detached owner's final drop, and it can never requeue or call lifecycle code.
An `InFlight` mutation or any worker that can still perform a disk side effect or
invoke publication does not qualify and remains in Drain. A read-only closure behind
an already-revoked result-adoption gate may leave a source/project publication Drain,
but its separate `ExecutorRunRegistration` remains in the process incarnation's join
set until the closure actually exits. Therefore only completed escaped results,
charged allocations, and revoked non-executing handles may outlive process `Stopped`.
Successful pending
project/source admission terminalizes its admission lineage; later live work mints a
new project/source-owned lineage rather than changing owner identity. Under the pool lock,
scheduling only debits and mints a ticket. Delivery/callback runs after unlock and
revalidates the still-open slot/binding before allocation; it never first registers at
Accepted. Revocation/receiver loss returns exactly once. After publishing lifecycle
revocation (or the atomic pending-cancel phase), Freeze records the exact closed
`RevocationScope` under the pool lock and cancels every descendant queued/dispatching
request before Drain. Process shutdown closes Process scope; pending cancellation its
admission scope; project Freeze Project scope (including pending-source work); source-
only Freeze the full `BindingAuthority`; and Adapter/partition close their exact scope.
Every first enqueue and `ResizeReadyAfterDrop -> ResizeRequeued` transaction
checks **all** derived ancestor scopes and consumes the retained old grant. If closure
won, it refunds that grant's unused reservation exactly once and becomes
`ReturnedTerminal` with no successor request; any
already detached deallocation-only owner remains charged but has no lineage/queue
authority. It cannot enqueue behind the one-time cancellation and strand Drain. The pool
tombstone remains until every matching descendant lineage is terminal, then is removed
by bounded reclamation. Capacity release enqueues reconsideration but never calls lifecycle code
under the pool lock. Retired/rejected roots are extracted under
lifecycle locks and final-dropped afterward. After partial candidate charged-root
cleanup has completed, consuming/refunding the one retained old grant, minting/re-
enqueuing its resized request, and rescanning eligibility is one pool transaction. The same
`DrainRegistration` remains live throughout cleanup and requeue, so Freeze cannot
observe a false terminal between “not queued” and “not running.”

All opaque/blocking work crosses `run_charged_blocking`: its task envelope moves into
the closure before enqueue and remains through exit/unwind; cancelling/dropping the
JoinHandle cannot refund it. Persistent results leave only as charged roots. The
candidate vector covers enforced maximum workers times per-worker/parser envelope and
the process-global Rayon pool stack/base once. Cancellation stops future scheduling;
it cannot stop an already-running closure. Every executing closure also owns an
`ExecutorRunRegistration` in its process incarnation. Revoking result adoption may
release a narrower source/project publication drain after the grant is sealed, but
process shutdown cannot store `Stopped`, join/replace the executor, or admit a
successor incarnation until every such execution registration reaches terminal.

Accounting includes Current, candidate, retired-but-pinned generations, charged
runtime-map nodes, daemon base interning, query-addressable caches,
bridge/authority/temporal groups, pre-update snapshots, bounded ranking/response
groups, invalidation accumulators, verifier/safety packages, and checkpoint/export
scratch. Every operation that materializes another representation obtains scratch
capacity first.

Before acquiring a query lease, the request reserves its complete bounded workspace
and owned response/output vector. It performs no capacity wait and no client/network
await while holding the lease: it materializes a charged response, drops source
leases, then awaits transport. Query leases remain charged until actual final drop; a
watchdog may request cooperative cancellation but never invalidates dereferenced
memory. A
request waiting specifically on old query-pinned or non-cancellable residency reports
`WaitingForCapacity { cause: PinnedResidency }`. Health exposes requested class
vector/bytes, queue age, bypass count, blocking slot/source/generation and bytes,
oldest lease/task age, cancellability, and a safe operator action. Age escalates to
`Stalled/attention_required`, not lifecycle `Blocked`; waiting age is evidence, not
proof of permanence.

Snapshot and team-artifact paths participate before allocation: stat and reserve
before read, cap streaming decompression at the granted charge, account compressed,
decoder-window/output, deserialized seed, and rebuild coexistence, and avoid retaining
all forms. Successor ownership is established before predecessor release.
Deserialization produces a candidate seed only.

SymForge-owned collections use checked arithmetic and fallible reservation where
possible. Opaque dependencies receive conservative multipliers and process headroom.
Allocator OOM or a hard-hung parser may still terminate the process; RAII cannot turn
an abort into recoverable in-process state. A requirement for finite hard-task cleanup
would require a killable worker process and is outside the first in-process design.

A hard project limit can still make readiness impossible. The preventive response is
retryable `WaitingForCapacity`, attention-bearing `Stalled`, or arithmetically proven
`Blocked`, never a successful placeholder. Longer term,
disk-backed immutable catalog segments can remove the in-memory metadata cliff, but
that is not required for the first safe lifecycle. A project whose minimum charge
exceeds the process budget receives a deterministic `Blocked` action instead of
starving the queue.

## 9. Query policy and last-known-good state

Keeping an old generation does not automatically make it safe to call Current.

Default policy:

- `Loading`: Generation reads refuse while protocol, static, health, and
  explicitly typed disk/Git observation surfaces remain responsive.
- Any delivered source-affecting observation atomically publishes `Refreshing`;
  strict generation reads refuse until a complete replacement promotes `Current`.
- Any SymForge-owned source mutation publishes `Refreshing` before its first repository
  disk side effect. Strict reads and checkpoints therefore cannot observe changed
  bytes while the old generation is still labeled Current.
- `Refreshing`/`Blocked`: the retained verified generation remains internal recovery
  material. Existing MCP, HTTP, resource, prompt, hook, and embed consumers do not
  silently use it.
- Negative/global absence claims always require `Current` complete generation
  coverage. Disk observations can never establish them.
- Explicit worktree/Git tools may return typed observation/comparison/derivation
  claims; they
  may not borrow generation currentness or completeness.
- Public last-verified/as-of generation reads and capability-scoped partial admission
  are deferred. A later
  design must define exact source identity, acknowledgement, disclosure, invalidation,
  and proof rules before either becomes queryable.

This preserves the strict sidecar decision. A refusal is preferable to injecting a
retained or incomplete answer into an agent prompt.

Selection is explicit and preserves Feature 020 per-source independence:

- `source_scope=current` depends only on the current-worktree source; an unrelated
  blocked local ref cannot refuse it.
- A multi-source request captures the selected project runtime snapshot once and
  requires `ProjectRuntimeState::Live`, then freezes a
  `SourceSelectionReceipt { slot_instance_id, project_runtime_publication_identity,
  runtime_epoch, session_binding_publication_identity, selector,
  canonical_source_ids, project_membership_authority, session_incarnation,
  session_binding_revision }` and
  acquires **every** selected `Current` generation before execution. The lease retains
  this receipt after dropping the runtime snapshot. If any source is unavailable, it
  returns `SourceRefusal`; it never silently omits the source or invents a global
  no-match.
- A cross-project request captures each selected project once in canonical project
  order, retains one selection receipt per project, pins only selected generations,
  and discloses each independent authority; it does not claim one process-global
  filesystem instant.
- Partial positive aggregation is deferred with capability-scoped availability.
  Global no-match/absence uses only the private `SelectedAggregate` constructor: its
  closed project receipt set and generation map must be an exact selected-source
  bijection, so it requires every selected lease and cannot encode an omitted/extra or
  stale generation.

Every Feature 020 `authority_scope` wire value is parsed internally as a
`KnowledgeVoiceFilter` **inside one Current verified generation**. The value currently
spelled `current` is named `CurrentImplementationIncludingUnclassified` internally so
it cannot be confused with lifecycle `Current`. Generation consistency is a separate
closed axis, `GenerationConsistency::StrictCurrent` in V1; future
`LastVerified × KnowledgeVoiceFilter` remains positive-only and an empty filtered
result is Unavailable, never absence.

### 9.1 Future last-verified read contract

The availability insight behind labeled last-known-good reads is valid, but broad
automatic fallback is not. A separately reviewed `LastVerifiedReadContractV1` may add
an explicit consistency mode only after Spec 027 identity disclosure and a uniform
machine-readable/in-band trust envelope ship.

Its minimum Interface is:

```text
Current(CurrentQueryLease)        // all generation claims
LastVerified(LastVerifiedLease)   // explicit opt-in, positive generation evidence only
Unavailable(SourceRefusal)
```

`LastVerified` must name project/root/source/version, generation/content/manifest
identity, capture time, `LAST VERIFIED / NOT CURRENT`, and current runtime work. It
uses generation-owned bytes/structures only—no targeted freshening, raw-disk sweep,
unbound cache, no-match, absence, completeness, mutation, checkpoint, impact,
sidecar/hook, prompt, or parameterless-resource result. This capability cannot be used
to excuse slow strict-current convergence and is outside the first lifecycle.

### 9.2 Compatibility and v11 embed projection

Lifecycle types stay private during migration, but the frozen raw embed facade cannot
express the guarantee. PreventiveV1 therefore ships at a deliberate v11 boundary.

| Surface | PreventiveV1 behavior |
|---|---|
| Public Rust server health types | Preserve fields additively where they do not create a bypass. Any legacy `Degraded` spelling is a compatibility projection of runtime availability, never a degraded generation. |
| Health/status JSON | Preserve existing fields and add runtime work/epoch evidence additively; render from one captured runtime snapshot. |
| MCP generation reads | Strict loading/refusal result; no automatic last-known-good fallback. Every claim carries provenance. |
| Explicit worktree/Git reads | Preserve intended observation behavior, but return atomic receipts or typed comparisons/derivations; never a generation-current envelope. |
| HTTP sidecar and aliases | Return HTTP 503 while strict-current acquisition fails. A caller hook may fail open only by omitting enrichment; it never injects retained context. |
| Resources and prompts | Same strict-current acquisition; parameterless calls cannot opt into as-of state. |
| Daemon `/sessions` | Report closed source runtime state plus current/retained identity from one runtime snapshot. |
| Hook enrichment | On sidecar refusal, a hook may fail open only by emitting no enrichment; daemon fallback remains allowed and retained stale context is never injected. |
| Public embed facade | The sole allowlist is the attested canonical `contracts/public-api-v11.json`; this row is a non-normative summary. It exposes one lifecycle-owned `symforge::embed` Interface, fallible source admission, authority-bearing query results, closed refusals, and close/shutdown receipts exactly as named/signatured there. All raw engine modules, constructors, mutators, snapshot converters, parser/Git handles, `LiveIndex`, deep reexports, and authority-less result types become crate-private or are removed. Queries return `Result<Claim<T>, SourceRefusal>`. No phrase such as “DTOs,” “health/progress,” “receipt types,” or any category in this document authorizes an export absent from the manifest. |

The allowlist is generated and enforced across the manifest's complete supported
target/cfg/feature matrix (whose convenience profiles include default, embed-only, and
all-features) by merged rustdoc graphs, the all-cfg inventory, and an external
dependent-crate positive plus compile-fail/public-API negative suite;
an in-crate test is not evidence that an external path is unnameable. In particular,
neither `symforge::live_index::*`, `symforge::git::*`, nor
`symforge::embed::live_index::*` exists publicly, and Git observations flow only
through authority-bearing runtime methods. The manifest's v10→v11 matrix classifies
every current crate-root/flat/deep export as keep, replace, or remove. This is an
allowlist, not a denylist of known dangerous functions.
Public `Claim`, authority/provenance, selection-receipt, partition-token, and
certificate types are opaque/sealed with private fields and lifecycle-owned
constructors. If wire persistence is supported, deserialization yields an untrusted
DTO that must be revalidated/adopted; bytes cannot mint runtime, capacity, selection,
or generation authority.

PreventiveV1 mints a new authority/cache schema and process mode epoch. Legacy query
execution, response-cache get/put, CCR lookup/store/`symforge_retrieve`, and response
finalization all acquire one non-cloneable `LegacyResponseRegistration` from the exact
mode-epoch gate **before** touching legacy state and retain it through materialized
response completion. Activation atomically changes that gate
`LegacyOpen(epoch) -> LegacyClosing(epoch)`, refusing new registrations; drains every
admitted operation (including late cache puts and retrieval rendering); invalidates
every v10 response cache and CCR registry; installs the new schema/epoch; and only then
stores `PreventiveV1Open(new_epoch)`. Cache/CCR code cannot use a side gate or release
registration between lookup and write/finalization. Thus every operation is wholly
pre-cut and drained before invalidation, or wholly post-cut under V1—never an
unregistered straddler. A v10 persisted cache record or handle loaded after restart is
an untrusted miss/refusal and is never upgraded into v11 authority. New cache/CCR keys preserve the complete claim
operation receipt, provenance (including selected-aggregate receipts), evaluation
receipt, producing runtime/publication identity, and schema/mode epoch. Same-process activation and
restart tests seed apparently valid v10 entries and prove they are unreachable.

`from_indexed_files` is not public authority. If retained internally, caller-provided
files are **untrusted candidate material** only. The lifecycle must still perform
complete root-bound discovery, capacity admission, path/secret/policy classification,
stable byte matching, required-artifact construction, and strict-scope proof before
it alone seals a completeness certificate and promotes `Current`. Forged, missing,
extra, sensitive, and root-mismatched inputs cannot attest anything. Virtual corpora
would require a separately reviewed additional/fifth `AtomicAuthority` plus observer, mutation,
snapshot, and completeness semantics; V1 does not provide that lane.

### 9.3 EmbeddedSourceContractV1

V1 chooses one managed-background Adapter rather than two subtly different progress
models. `ProcessRuntimeInner` owns bounded supervisor/observer/verifier/blocking
threads and the process capacity domain; it requires no caller Tokio runtime.
`ProcessIndexRuntime::open_embedded_source(&self, EmbeddedSourceSpec) ->
Result<EmbeddedSourceHandle, SourceRefusal>` returns one non-cloneable, one-source
registration only after root/membership authorization, capacity admission, process-
gate registration, and exact exposure CAS. Every pre-exposure failure is constructed
through `OperationContractV1`: unauthorized/nonexistent selection remains the
indistinguishable unresolved `InvalidSelection`; an authorized root, capacity, or
shutdown race is `AdmissionUnavailable` with its legal basis/retry. Registration,
reservation, and a losing exposure CAS refund exactly once. The handle retains a
`ProcessControlLease`, never the public factory owner; there is no second public
runtime/factory layer. The managed observer delivers hints/gaps, while
reserved authoritative scope polling closes suppressed-notification and deadline
obligations. A host may request/coalesce refresh and inspect a `RefreshTicket`, but it
cannot drive publication or satisfy proof itself.

V1 deliberately chooses **single exposed owner**, not shared-close refcounting. The
registry derives a canonical `SourceRegistrationKey` from the authorized project/source
identity, source kind, and held physical-root identity. Pending admission and live
registration serialize on that key. Exactly one opener wins the exposure CAS and owns
the one non-cloneable handle/close authority; every concurrent or repeated authorized
open returns `AdmissionUnavailable(SourceAlreadyOpen/OnEvent)` and receives neither a
handle nor a joinable close receipt. Distinct SourceSlots for the same key are
forbidden. Only after the sole registration's close receipt is terminal may a later
open create a new incomparable source/binding incarnation. This keeps close/Drop local
and prevents one independently returned handle from revoking another.

Every source-handle query returns `Claim<T>` or the one `SourceRefusal` algebra. Backpressure is
bounded before query-lease acquisition. Overdue proof is synchronously failed closed;
missing executor progress never leaves a stale `Current` answer.
Every source registration owns one shared idempotent `SourceCloseState`; both its
handle and process shutdown join the same terminal `SourceCloseReceipt`.
`EmbeddedSourceHandle::begin_close() -> SourceCloseReceipt` synchronously Freezes only
that source, transfers the drain to the reaper, and is idempotent. Waiting is a
separate `receipt.wait(deadline)` operation; it detects an invoking registration in
the drain set and returns `WouldSelfWait` while the reaper continues. Drop performs the
same begin transition, then transfers the owning
`DrainHandle` and retained process control lease to a dedicated process reaper. If Drop
runs on a managed worker, that worker first reaches its revoked/no-publication terminal
state, so the reaper can join it without self-join and no publication-capable worker
detaches. If process shutdown already completed the source, a surviving handle's query
returns `AuthorityRevoked`; its later close/Drop observes the terminal close receipt
and releases locally without enqueueing to the stopped reaper.
`ProcessIndexRuntime::begin_shutdown() -> ShutdownReceipt` first atomically publishes
registry `Retiring` and inner `Stopping`, transfers the already-owned process drain to the finalizer, and closes
every source. `receipt.wait(deadline)` is separate and likewise refuses self-wait from
a registered managed callback while finalization continues. Shutdown then drains every already-registered source/
adapter/partition and the reaper, waits every process-incarnation
`ExecutorRunRegistration`, joins shared executors, stores `Stopped`, and reports any
charged deallocation-only work. An actively executing closure cannot outlive that
store merely because result adoption was revoked. Completed escaped response/result
charges may retain the stable process capacity domain until final drop.
Concurrent/repeated shutdown returns the same terminal
receipt. Observation, retry, capacity, progress, and shutdown behavior
are the same lifecycle Module used by daemon, stdio, and serve Adapters.

Managed tasks retain a non-cyclic internal process-control lease to
`ProcessRuntimeInner`, not a `FactoryOwnerToken`. Dropping the final public
`ProcessIndexRuntime` wrapper atomically decrements the persistent registry count to
zero and commits registry Retiring + inner Stopping even if handles/tasks remain, then
transfers the already-owned inner drain/control lease to a dedicated finalizer.
The finalizer cannot self-join; no publication-capable worker survives it. Explicit
shutdown and Drop share the same idempotent `ShutdownReceipt` state instead of
starting competing drains. Default reconstruction joins/refuses Retiring and can
install a successor only after Stopped, always on the same stable capacity domain.

The migration ships a checked-in
`docs/solutions/aap-embed-migration-10-to-11.md`, the public-Interface matrix above,
and an AAP known-consumer migration receipt that pins both repository commits, the
public-API digest, and exact green build/test commands/results. Existing 8→10 and reverse-asks guides are
marked superseded with forward links without rewriting their historical evidence.
Snapshot format and namespace are both bumped. V11 writes only beneath
`.symforge/v11/` and never renames, overwrites, deletes, or restores the v10
`.symforge/index.bin` path. An unmodified v10 process may continue taking its private
in-process mutex and replacing that legacy path without corrupting v11 state. V11's
OS-held crash-releasing exclusive lease coordinates v11 migration/checkpoint writers
only; owner metadata is diagnostic, stale metadata is swept, and unsupported locking
refuses the v11 write rather than pretending to exclude legacy writers.

V10 snapshots and portable artifacts are bounded **untrusted candidate seeds**. To
seed from v10, migration opens the actual legacy file object with platform semantics
that keep that object stable through the read, retains the handle, streams bounded
bytes while computing its digest/identity, and archives exactly those bytes under
`.symforge/v11/migrations/v10/by-digest/<sha256>/index.bin`. Pathname pre/post checks
cannot substitute for the opened-object guarantee; when safe handle/share semantics
are unavailable or the object cannot be proved stable, v11 ignores the seed and
performs a source rebuild. A concurrent v10 path replacement cannot change the opened
object or any v11 destination.

Capacity is reserved before compressed/decode/seed coexistence, and complete root
discovery plus every verification obligation precedes promotion. Invalid input is
quarantined inside the v11 namespace. While holding the v11 migration lease, archive
installation uses a bounded same-filesystem temp, file fsync, verified digest,
no-replace rename, directory fsync, and a durably installed digest/identity receipt.
An existing archive is reused only when its digest and opened-object identity match;
conflicts get a content-addressed path or refuse. Only after a verified v11 checkpoint
is durable does v11 atomically install its own activation marker. Repeated/concurrent
attempts are idempotent and the archive is never deleted automatically.

Rollback stops/drains v11 and starts v10 against the untouched legacy namespace; it
does not copy an archive over the v10 path. Any exceptional in-place legacy restore
requires explicit operator coordination plus proven quiescence of every legacy writer,
otherwise it refuses. The migration guide's cleanup acknowledgement binds the opened
v10 digest/identity to the verified v11 checkpoint digest. A pinned **unmodified v10**
writer campaign races every v11 temp/fsync/archive/receipt/checkpoint/activation
failpoint, including crash after lease acquisition and restart. The only acceptable
outcomes are safe seed refusal/source rebuild or a verified disjoint v11 generation
with a digest-matched archive while the exact v10 namespace remains independently
usable. V11 proof is never interpreted as v10 authority.

## 10. Failure policy

| Cause | Preventive behavior |
|---|---|
| Aggregate capacity temporarily unavailable | Wait with a retained coalesced request; admit older satisfiable work by class; no candidate allocation. |
| Old query/task pins required capacity | Remain retryable `WaitingForCapacity/PinnedResidency`, expose `Stalled` evidence/action, and reconsider on actual release; never forcibly expire live memory. |
| Configured usable class cannot fit request | `Blocked/UnsatisfiableCapacity` with explicit configuration/action; no current placeholder. |
| Mandatory retained residency plus replacement peak cannot fit | `Blocked/InsufficientReplacementHeadroom`; never wait for a release that policy forbids before promotion. |
| Transient read/walk failure | Discard candidate and retry with bounded backoff. |
| Persistent unreadable/unstable/partial-parse file | Keep retained generation unchanged; block strict-current acquisition for that source and expose the remediation class. |
| Parse-failure ratio | Cancel upstream scheduling and discard candidate; do not fold a partial generation. |
| Observer overflow/disconnect/accumulator gap/counter exhaustion | Latch `Gapped`, invalidate candidate, mint a new observer epoch, and perform authoritative re-observation. |
| Racy stamp, obligation mismatch, or verification deadline overdue | Reverify/invalidate via the armed safety package, publish non-Current, and schedule replacement; stamps and worker starvation never override proof. |
| Snapshot mismatch/corruption | Quarantine the candidate seed; retained generation remains unchanged or absent. |
| Disk-observation read/classification failure | Refuse that observation claim; never fall back to generation identity or absence. |
| Loader panic/cancellation | Lifecycle owner observes completion and schedules retry or Blocked; task-owned capacity releases only after actual allocations drop. |
| Mutation permit panic/drop/failure after any side effect | Remain non-Current, retain tracked authority until terminal drop, and require verified candidate promotion; never infer rollback from bytes. |
| Retarget proposal failure | Leave the existing session binding, observer, and Current generation untouched. |
| Project rebind/close | Freeze gates, drain destructive/publication authority and callbacks, then install one never-reused successor. |
| Same-path directory replacement | Immediately freeze old authority; drain before successor installation and never perform a path-based old-root write. |
| Stale capacity grant/callback after revocation | Reject immutable slot/binding identity and refund exactly once outside lifecycle locks. |

Build/verification retries use a finite automatic-attempt budget. Exhaustion transitions to `Blocked`;
only a source change, capacity/configuration change, or explicit operator retry
starts a new attempt series. Lifecycle ownership persists, so “bounded retry” never
means an unobserved task silently stops forever. Pure capacity waiting consumes no
build attempt and remains armed on capacity-release/configuration triggers.

## 11. Evolutionary implementation sequence

### Prerequisite — Refreeze Feature 020

- Create the exact hash-pinned Feature 020 refreeze manifest described in Section 3;
  classify every artifact and apply each amendment across `GOAL.md`, spec/plan/data
  model/tasks/quickstart, all contracts, the requirements checklist, and bound
  `CONTEXT.md` wherever its mapped authority lives.
- Mark completed degraded-publication receipts as historical/superseded without
  rewriting their evidence.
- Run the manifest-aware replacement validator and cross-artifact analysis; block
  implementation on an unclassified file, unmapped clause, hash drift, or
  contradiction.
- After review, emit the externally stored signed `RefreezeApprovalRecordV11` over the
  exact target commit/tree and checked-in detached-attestation digest. The activation
  workflow accepts the trusted record as an immutable input and proves that a
  coordinated in-tree rewrite fails until a new approval record is issued.
- Check in and attest the exact canonical `contracts/public-api-v11.json` allowlist
  and v10 keep/replace/remove matrix before Slice 0. Declare the supported target/cfg/
  feature domain, generate a mechanically exhaustive graph cover plus all-cfg
  inventory and dependent-crate positive/compile-fail fixtures from it, and reject
  unsupported/unknown configurations; no
  later slice may expand the public Interface without a refreeze amendment.
- Record v11 as the breaking embed/lifecycle release boundary.

### Slice 0 — Freeze causal regression oracles

Slice 0 has two explicit classes. **Current positive controls** run against the v10
implementation and must demonstrate the known generation/root, placeholder,
same-stamp, raw-read, and multi-loader failures before fixes. **Acceptance oracles**
that require new types or failpoints are checked in as versioned specifications with
their target slice and become mandatory in the first slice that introduces that seam;
they are not reported as executed early. A checked-in traceability table maps each
invariant to production seam, positive control or acceptance oracle, state model,
implementation slice, exact command, bound, fairness assumption, and CI artifact.

- Positive-control the generation/root split window.
- Prove a root-A blocking mutation cannot commit after root-B promotion.
- Prove two simultaneous first opens invoke one loader.
- Prove catalog refusal creates no queryable project instance or watcher mutation.
- Prove watcher fresh-instance reconciliation cannot mutate a candidate through the
  active interface.
- Prove a failed or panicked load leaves the retained verified generation identical.
- Prove same-path directory replacement invalidates watcher/candidate authority.
- Replace directory A with B at the same canonical path before observer detection,
  then open a new session; fresh physical-root admission must refuse joining A and
  route through PhysicalRootReplacement.
- Pause a granted mutation permit before temp creation and atomic replacement, then
  replace root A with B; prove B is untouched and successor installation drains.
- Pause `start_side_effect` immediately before and after the under-writer
  Granted→InFlight store while Freeze races. Before-mark Freeze publishes
  `RevokedSealPending` and start refuses, but Drain retains the registration until the
  out-of-writer grant seal waits construction guards, refunds/transfers, and commits
  `RevokedDeallocationOnly`; after-mark Freeze waits the InFlight authority. Pause a
  construction guard after allocation/before commit and prove successor Install cannot
  pass Drain or regrant its units. The final revoked handle may then outlive the
  tombstone with its charged root, but cannot start, allocate, write, or publish.
- Insert a symlink/reparse component beneath the bound root that targets outside it;
  no-follow component resolution must refuse before temp creation, and no platform
  path-only fallback may write the target.
- Queue an old-observer event before promotion and deliver it afterward; prove the
  stable ObserverToken makes the promoted generation non-Current.
- During observer replacement, delay one predecessor delivery. Handoff must publish
  non-Current, drain the predecessor, retire any gapped provisional token, then mint a
  fresh post-barrier observer before baseline/final cut; no predecessor callback may
  exist after new registration or successor Current publication.
- Race promotion with mutation-permit grant/side effect; require unchanged mutation
  epoch and zero permits.
- Failpoint promotion before runtime store and after store/before pruning; no hint is
  lost and the runtime store is the only commit point.
- Fail every point after observer/mutation ingress begins. Before the commit section,
  neither accumulator/mutation facts nor runtime root may change; after runtime store,
  Current is already impossible and the staged fact must apply or conservatively
  become Gap before the delivery/permit is acknowledged.
- Panic at every pre-store observation point and unwind the processing stack. The
  lifecycle-owned `IngressEnvelope` must retain both event and DrainRegistration until
  the outer owner retries or commits Gap; stack drop alone can never consume delivery.
- Deliver E1 to Current so its one safety package publishes Refreshing, exhaust
  ordinary capacity, then deliver E2 and a Gap on the same active token. E2 must update
  the accumulator without another runtime allocation/store; Gap must use the precharged
  handoff phase transition. Neither may be lost for lack of a second safety package.
- Cycle one source through field-equal non-current states, then apply an old prepared
  delta; exact retained `SourcePublicationToken` identity must reject it. Static
  visibility tests prove lifecycle code cannot name `SourceStatePublication` or a
  runtime root, registry code cannot construct lifecycle policy, and only the registry
  Adapter can turn a sealed intent into an opaque prepared delta. Overflow the live
  accumulator afterward and prove no copied runtime observer state can still say
  Complete.
- Arm a retry timer in `RetryWait`, supersede it with a successful same-binding
  promotion, then deliver the old timer/capacity/loader callback. Exact owning-state
  identity must make it terminally no-op/refund and leave the newer Current unchanged.
- Exhaust each identity counter at its declared hierarchy boundary and prove the next
  incomparable observer/binding/slot is minted or the outer allocator fail-stops;
  post-store prune with a mismatched observer token is a no-op.
- Cancel admission A, create B for the same canonical key, then deliver A's grant.
  Retained never-reused admission identity and owner scope must reject A; no reused
  numeric value may match B.
- Advance a Live project runtime from `MAX-1`: the reserved terminal epoch and
  precharged revocation package must publish `Stopping`, drain, and install a fresh
  slot/full baseline without wrapping or leaving stale Current queryable.
- Trigger invalidation-sequence exhaustion from inside a predecessor observer callback
  and mutation/binding rollover from registered work. The trigger must close ingress,
  hand off to control authority outside its Drain set, terminalize first, and permit no
  successor before that terminal point; no Drain may self-wait.
- Freeze between async enqueue and claim, timer exposure, callback registration, and
  capacity dispatch; an owning `DrainRegistration` must exist in every interval and
  refund/terminal conversion must be exact once.
- Prove close/reopen cannot create a second slot while blocking work survives.
- Race embedded-source open, server-Adapter creation, and child-partition minting with
  process shutdown at every registration boundary; each creation is registered and
  drained or refuses/refunds, shutdown is idempotent, and none survives executor join.
- Race two authorized `open_embedded_source` calls for the same canonical
  `SourceRegistrationKey` before pending creation, during admission, and at exposure.
  Exactly one receives the sole handle/close authority; the other receives typed
  `SourceAlreadyOpen` with no close receipt. Close/Drop of the winner cannot affect an
  independently returned handle because none exists. Reopen only after terminal close
  and prove the new source/binding incarnation is incomparable.
- Hold an embedded source handle beyond process shutdown and race its close/Drop with
  shutdown. Query must refuse after revocation, all paths join one close receipt, and
  no late work may enter a stopped reaper. Omit explicit shutdown and drop the final
  public runtime owner—including from a managed callback—to prove the independent
  finalizer closes/drains without self-join or detached publication authority.
- Invoke explicit source `begin_close` and process `begin_shutdown` from managed work
  registered in their drain sets. The gate transition and reaper handoff succeed;
  attempting to wait on the returned receipt reports `WouldSelfWait`, the callback can
  terminalize, and an external waiter observes eventual completion.
- Exhaust ordinary capacity and close one source independently from Loading,
  Refreshing, Blocked, and Current. Its precharged source-revocation package must store
  Stopping before drain while unrelated Current sources remain queryable.
- Race whole-project Freeze with and immediately after a source-only Freeze. The
  project package must accept a Live map containing source `Stopping`, revoke every
  remaining gate in one store, and neither wait on nor resurrect the source close.
- Race two-source project Freeze with query acquisition, source add, observation, and
  promotion; the one `Live -> Stopping` store must close every project gate before
  drain, and no selected lease may originate from `Stopping`. Continuously publish an
  unrelated source while Freeze waits; once it acquires the writer it must fill from
  the latest map and commit without optimistic-root retry starvation.
- Acquire `source_scope=all`, drop the project runtime root, then change membership;
  the retained `SourceSelectionReceipt` must keep the exact old selection explicit.
  Global no-match includes that receipt plus every selected generation.
- Race a retarget/membership CAS between every project-query capture step; the one
  runtime-root load must yield wholly pre-CAS or post-CAS source+membership evidence.
  Remove/retarget a member immediately after a valid lease is acquired: finalization
  returns the internally consistent old-scope claim, while a forged/mismatched lease
  refuses and the next strict acquisition sees the successor.
- Prepare a source delta against project root R0, commit a membership delta to R1,
  then commit the source delta. The under-writer latest-root patch must preserve R1's
  membership, revocation package, unrelated sources, and checked epoch while changing
  only the exact expected source and minting a new publication identity.
- Prepare watcher-ingress, proof-expiry, and mutation-terminal `Current -> non-current`
  deltas, then retarget/add/remove/reconnect every session before commit. Exact source/
  lifecycle authority must still commit while preserving the latest session and
  project-root siblings, unless project/source Freeze wins. No session CAS may leave
  stale Current queryable, discard the ingress envelope, or force a session-owned
  retry of lifecycle safety work.
- Race concurrent A→B and A→C retarget proposals, reconnect using the same visible
  session ID, invalidate a target, and replay an idempotency key; the binding-revision
  CAS admits one winner and stale proposals only release provisional membership.
- Pause query acquisition after session-binding capture, after project-root load, and
  immediately after a cross-project retarget CAS. Exact session-publication
  revalidation must yield wholly A or wholly B; a post-CAS load of A cannot authorize
  the session, and target/old-project membership bookkeeping cannot do so either.
- Repeat with additive working-set add/remove while active project stays unchanged;
  the never-reused session publication must yield the complete old or new authorized
  selection, never a torn membership set.
- Prove failed reload leaves the old observer capable of triggering recovery.
- Prove a restored snapshot is not queryable before new-process verification.
- Prove cancellation does not release capacity while blocking memory remains live.
- Positive-control a hybrid read across current `live` and source-bundle ArcSwaps.
- Capture a strict generation lease, replace disk bytes before content resolution,
  and prove returned bytes are generation-owned/digest-identical or refused—not a
  disk observation labeled as that generation.
- Prove a manifest terminal/no-identity path refuses in generation mode without disk
  I/O, while explicit worktree mode returns a typed disk receipt.
- During a disk observation, replace an intermediate directory with an outside-root
  symlink/reparse point and restore it before return; only a beneath-confined pinned
  handle may yield authority. Path-only bytes are refused/non-authoritative.
- Cache and CCR a generation claim, then recreate/rebind the same canonical path with
  an identical manifest and recycled local counters; the old claim must not replay
  because full binding/publication/scope-certificate authority differs.
- Suppress creation and rename notifications for a previously unknown in-scope path;
  the reserved scope-discovery deadline must make the source non-Current or publish a
  candidate that accounts for the path. A policy/scope-version change has the same
  obligation.
- Run a worktree changed-set scan with a gap, incomplete traversal, and root rebind;
  full totals/risk summary must refuse. Only a complete root-bound scope receipt may
  feed the `detect_impact` derivation.
- Complete an unchanged rolling pass, then race proof-refresh publication with an
  invalidation and rebind. It may publish renewed immutable proof only at the exact
  binding/cut/mutation/generation fence; otherwise it is discarded. A mismatch must
  publish non-Current rather than renew proof.
- Cancel/retry a rolling pass repeatedly under the same source publication: its
  lifecycle-owned `VerificationProgress` must resume fairly past early paths. Change
  the source publication and prove the old cursor/staged proofs are discarded; only a
  fenced ProofRefreshCandidate may renew immutable deadlines.
- Preserve controls proving uncommitted `diff_symbols` and `detect_impact` name every
  Git/disk/generation input in their comparison/derivation.
- Rebind between Git, generation, and disk capture; the ClaimContext refuses rather
  than comparing root A with B. Rebind immediately after complete ClaimContext capture;
  the response derived wholly from the captured authorities remains valid and no
  trailing live-state check converts it to a refusal.
- Run two different search predicates/voice filters and two same-version ranking
  stores/sessions recreated under new instance identities. Operation and evaluation
  receipts must keep cache/CCR identities distinct. Nonexistent and unauthorized
  protected selectors must return byte-for-byte equivalent unresolved refusals with
  no resolved project/source/binding evidence.
- Enumerate `OperationContractV1` as a Cartesian-negative oracle: wrong operation
  receipt, provenance form, role/cardinality, selection requirement, evaluation
  requirement, refusal variant, basis/retry pair, or transport mapping cannot be
  constructed. Every observable ranked result requires its evaluation receipt.
- Capture an authorized multi-project selection with mixed Current/Loading/Blocked
  members and different causes. `SelectionUnavailable` must carry canonical receipts
  plus an exact selected-source bijection with at least one unavailable member through
  serialization, HTTP, cache, and CCR; it cannot leak an unauthorized identity or
  become an absence success.
- Rewrite same-length bytes with restored mtime and suppressed notification; prove a
  racy promotion rejects and rolling verification detects it within the coverage bound.
- Repeat for `SensitiveContent`/binary/encoding terminal-to-Indexed reclassification
  and for offline snapshot restore.
- Starve/pause verifier capacity past deadline and continuously promote unrelated
  deltas; strict acquisition must drop its stale snapshot, synchronously publish
  non-Current through the safety guard, and fail. Untouched obligations keep their
  ages, and checkpoint uses the same path.
- Latch `scope_dirty` at sequence S and reject delta admission. A full candidate whose
  cut/certificate covers S and the recorded policy versions may discharge it; inject a
  newer marker after the cut and prove post-store pruning preserves it. Reorder
  same-token pruning for cuts 2 then 1 and assert acknowledged sequence remains 2.
- Positive-control multi-source close/rebind versus observe/promotion, capacity-pool
  callback/drop cycles, multidimensional head-of-line starvation, and exact
  reservation-to-charge conservation.
- Freeze at every undersized-grant resize boundary. First drive
  `ResizeGuardSealPending` until new construction is forbidden and every admitted
  guard has aborted/final-dropped or committed into cleanup ownership, then drive
  `ResizePendingCleanup` until every candidate-private charge is final-dropped. Only
  then may the sealed old-grant refund and complete new request/vector enqueue be one
  pool transaction under the same live `WorkId/DrainRegistration`. A
  no-successor `ReturnedTerminal` or capability-stripping
  `ConvertedToDeallocationOnly` may decrement the publication drain exactly once.
- Exercise the full Accepted/Running→ResizeGuardSealPending→ResizePendingCleanup→
  ResizeReadyAfterDrop→
  ResizeRequeued/ReturnedTerminal graph and Freeze at every edge. A resize successor
  changes request ID/vector but preserves lineage, owner, scheduling age, and live
  DrainRegistration until a real control terminal. With budget 10, let two workers
  each allocate 4 then discover a complete vector of 7; both partial candidates must
  drop before either waits/requeues, so the 8 cannot deadlock the pool.
  Separately start with reservation 6/committed charge 4 and exercise both successful
  requeue and tombstone cancellation: the remaining 2 units refund exactly once and
  reserved-plus-charged conservation holds at every phase.
  Pause a construction guard after physical allocation/before commit while resize and
  Freeze race; no proof, terminal, requeue, or refund may pass until that guard commits
  into cleanup and is final-dropped or aborts/final-drops directly.
- Fail/panic/cancel the resize initiator immediately before and after publishing
  `ResizeGuardSealPending`, before and after the grant close CAS, during guard wait,
  after the final guard, during root cleanup, after cleanup proof, and before the pool
  transaction. The retained `GrantSealDriver` or any helper must finish the exact
  winning mode without an ownerless grant or stranded DrainRegistration. Race Freeze
  on both sides of the CAS: a resize win follows cleanup to tombstoned terminal; a
  deallocation win takes the explicit `ResizeGuardSealPending ->
  DeallocationSealPending` edge, creates no resize token/successor, and refunds once.
  Repeat with the triggering worker initially holding a construction guard; it must
  commit, drop, or transfer that guard before the driver waits, so no self-wait is
  possible.
- Publish Freeze's pool tombstone and cancel an empty queue immediately before an
  accepted worker attempts resize. The resize must observe the tombstone, refund and
  become terminal without a successor; Drain cannot wait on a stranded requeue.
- Give two sources in one slot the same diagnostic binding epoch, then source-close A
  while B queues/resizes. Full SourceWork BindingAuthority scopes must cancel only A.
  Repeat process shutdown, project Freeze during pre-insertion source admission,
  Adapter close, and partition close; every descendant request must see its closed
  ancestor scope and no unrelated scope may alias.
- Exhaust ordinary capacity then invalidate Current; its armed safety-transition
  package must publish non-Current without waiting or allocating under the writer.
  Inject error and unwind after staging but before the runtime-root store; strict
  acquisition must still observe either intact armed Current or the committed
  non-Current successor, never disarmed Current.
- Hold an A-only lease while B churns; old B roots must drop. Slow transport cannot
  retain a source lease after response materialization.
- Construct two embedded runtimes through the default API and prove they debit one
  process domain, including one parser/blocking-pool base. Construct isolated children
  only from parent-minted partition tokens and prove aggregate child budgets never
  exceed the parent. Let a child query generation and blocking task outlive its runtime
  handle; the partition debit must remain until their final child-domain owners drop.
- Hold embedded handles and managed work through dropping the final public factory
  wrapper. Because children retain only `ProcessControlLease`, final-owner Drop must
  close the inner factory gate and drive the same shutdown receipt as explicit
  shutdown. Race fallible source open at authorization, capacity registration, and
  exposure CAS; each loss returns the contract-valid `SourceRefusal` and refunds once.
- Pause final-wrapper release immediately before/after its registry count reaches zero
  while an old InFlight mutation, executor, response charge, and default constructor
  race. Registry Retiring must precede owner disappearance; no successor inner/root
  authority exists before old destructive/publication drain reaches Stopped, and the
  successor reuses the same capacity domain/parser-base ledger while residual charges
  remain.
- Clone a factory wrapper immediately before explicit shutdown, during Retiring, after
  Stopped, and after a successor incarnation installs. Only the pre-cut Live clone is
  counted in the old incarnation; later clones are terminal receipt tokens. Drop every
  ordering and prove no old token changes the successor count or factory state.
- Revoke result adoption while a read-only blocking closure runs. Source/project
  publication Drain may complete after its grant is sealed, but process shutdown must
  remain `Stopping` on that closure's `ExecutorRunRegistration`. After it exits, let
  its escaped response charge become `DetachedChargeOwner`: only then may `Stopped`
  and a successor incarnation proceed on the same still-debited domain; the ledger
  refunds only at the detached owner's actual final drop. Pause an
  `AllocationConstructionGuard` after physical allocation but before commit, then race
  sealing. The pool must not regrant those units until the guard either commits and its
  charge transfers or owns/final-drops the group; assert available-reserved plus
  in-construction plus charged conservation and one refund.
- Expose a `PendingProjectAdmission`, then race capacity grant, concurrent join,
  close/process shutdown, and late delivery. No `ProjectSlot` or source authority may
  exist before all fixed revocation packages are charged; cancellation drains the
  pending record without a project-root store and a late grant refunds once.
- Replace pending admission's physical directory A with B while capacity waits. A B
  opener cannot join; pending work cannot reopen by path; cancel and install contend on
  one exact registry publication, and only the winner may transfer A's pinned lease/
  DrainRegistration or refund it. Race process shutdown at that same commit.
  The serialized/HTTP refusal must be authorized `AdmissionUnavailable` with an
  admission-root attempt basis, never a fabricated `BindingAuthority`; constructor
  negatives reject that basis from source/selection refusals and all claims.
- Fail the **initial** authorized root capture before pending state exists with both
  unsafe traversal and rooted-I/O-unsupported evidence. The pre-I/O attempt identity,
  `held_root=None`, privacy shape, retry/status, and transport serialization must be
  valid without minting a pending admission, slot, or binding; a successful retry
  adopts its new attempt identity exactly once.
- Exhaust observer quota during handoff. The source must first become non-Current/Gap,
  publish `Draining(T0)`, drain/release the predecessor, publish `ObserverFree` without
  retaining T0, then wait/retry until it can register fresh post-barrier `Active(T1)`
  and require a complete baseline; it must not wait forever while the old observer
  owns the only releasable base or attempt to clear a latched Gap in place.
- Repeat observer overflow/disconnect before first promotion and from Blocked; the same
  `Absent/Active/Draining/ObserverFree` phases must work with no retained generation.
  Capture health across Draining→ObserverFree and ObserverFree→Active: it either
  exact-validates the named accumulator or reports explicit no-observer evidence,
  never blocks on or samples a nonexistent/stale token.
- Under exhausted ordinary capacity, disconnect T0, install fresh T1 with its own
  next-handoff package, and disconnect T1 immediately before promotion. Both handoffs
  must reach Draining→ObserverFree without a source-creation credit; no successor may
  become Active until its own next package is charged.
- Deliver a filesystem change exactly between the `Active(T1/BaselinePending)` store
  and external ingress-open; the full baseline must include it. A negative failpoint
  that attempts ingress-open before the Active store must refuse/latch Gap rather than
  consume the event.
- Freeze before observer activation registration, after registration, after Active
  store, after OS open with logical gate closed, and before/after exact post-open
  revalidation. The registration must be present before exposure, Freeze must wait it,
  revoked activation closes/terminalizes, and no late callback may enter a successor.
- Pin the v10 embed bypass as a compile/behavior oracle: empty/snapshot/direct-mutation
  results cannot satisfy the v11 authority-bearing Interface.
- Run a pinned unmodified-v10 process that repeatedly replaces legacy
  `.symforge/index.bin` across every v11 seed/archive/checkpoint/activation failpoint.
  V11 must either reject/ignore the seed and rebuild, or capture one exact opened-file
  object and publish only in `.symforge/v11/`; it never writes the v10 namespace and
  rollback never restores over a live legacy writer.
- Feed internal untrusted indexed-file seeds with a forged certificate, missing/extra
  path, sensitive bytes, stale bytes, and mismatched root; none may mint Current
  without lifecycle-owned full discovery/admission/artifact proof.

### Slice 1 — Atomic mutation authority

- Replace independent sampling with the explicit `BindingAuthority`, stable
  `ObserverToken`, `CandidateAuthority`, and generation-bound `MutationAuthority`.
- Introduce owning `PhysicalRootLease` and handle-relative destructive I/O.
- Track non-cloneable mutation permits and their terminal commit/no-side-effect/drop
  behavior; promotion checks mutation epoch plus zero permits.
- Validate each whole authority under the publication writer; never recapture fields.
- Make rebind or same-path physical replacement mint new epochs and invalidate all
  prior work.
- Implement Freeze -> Drain -> Install for legacy reload/rebind before successor root
  authority can exist.
- Remove `effective_fence_generation`'s inference from separate observations.

This closes the newly proven cross-root corruption class before the larger move.

### Slice 2 — Registry tombstone and capacity foundation

- Insert a process-base-charged `PendingProjectAdmission` before loading and let
  concurrent authorized sessions join it. It is not a slot and exposes no lifecycle
  authority.
- Bind pending admission to one owning physical-root lease and one never-reused
  admission identity; make install/cancel one process-registry-writer transition that
  transfers or drains its registration exactly once.
- Atomically charge the complete project/initial-source fixed base and revocation
  packages before CAS-upgrading pending admission to `LiveProjectSlot`; cancellation
  drains pending state without a runtime-root publication.
- Separate pending/slot existence from `Current`-generation existence.
- Check root eligibility and per-session protected membership before join; slot reuse
  never grants authority.
- Make stopping non-revivable, close/drain scheduling/waiter authority, and coalesce
  reopeners until exactly one never-reused successor slot installs.
- Introduce the shared process runtime and capacity pool for daemon, local stdio, and
  standalone serve.
- Implement the persistent factory-incarnation registry and stable process capacity
  domain; Retiring blocks reconstruction until old destructive/publication drain is
  Stopped, and later incarnations reuse the ledger.
- Add base/scout/accumulator/verifier/safety and complete net-new candidate vectors
  before any retained-plus-candidate path exists.
- Implement `CapacityGrant::begin_allocation`, `AllocationConstructionGuard::commit`,
  charged residency groups,
  `run_charged_blocking`, query-response reservation, and post-lock reclamation.
- Implement immutable grant identity/state, out-of-lock delivery, revocation refund,
  oldest-satisfiable drain barriers, pin-aware parking, and replacement-headroom
  arithmetic.
- Implement closed hierarchical `CapacityOwnerKey`/revocation scopes, pending-source
  inventory, pool tombstones, `DetachedChargeOwner`, and the complete cleanup-before-
  requeue resize transition.
- Capacity refusal cannot construct a `Current` project source.

### Slice 3 — Behavior-neutral seams, provenance, and dark runtime

- Consolidate existing production reads on one `PublishedSourceSet` capture without
  calling any legacy fact product `Current` under the new meaning.
- Split `src/protocol/read_gate.rs` into typed generation-resolution and stable
  disk-observation Adapters. Migrate raw fallback, Tier-2 sweep, validation,
  untracked search, each diff mode, and worktree impact to `ClaimProvenance`.
- Require every formatter/cache/CCR handle/retrieval path to round-trip provenance;
  add operation-specific `ClaimContext` identity compatibility.
- Build the charged persistent runtime map, closed `SourceRuntimeState`, project query
  lease, lifecycle supervisor, and v11 `EmbeddedSourceHandle` behind production-
  unreachable constructors.
- Inventory every legacy writer/publication authority: daemon load/reload, local
  stdio, standalone serve, snapshot/team artifact restore, watcher/reconciliation,
  targeted refresh, edit/curation, local refs, temporal, bridge, authority, checkpoint,
  sidecar freshening, and raw embed update/remove.
- Implement the already-attested v10→v11 public-Interface manifest and its generated
  external dependent-crate harness; draft the AAP migration/consumer receipt and mark
  old guides for supersession at activation. Slice 3 cannot invent or widen exports.
- New strict query acquisition, `Current` construction, and candidate publication stay
  dark. No legacy publication is adapted into `Current`.

Slices 1–3 may merge only when behavior-preserving or unreachable from production
constructors. Existing `PublishedGeneration` remains the sole production authority.

### Slice 4 — Candidate, invalidation, and delta enablement (indivisible)

- Move loader `JoinHandle`, cancellation token, attempt counter, progress,
  classified failure, and retry trigger into the per-source lifecycle module.
- Keep protocol initialization responsive through the slot, not through a mutable
  empty index.
- Turn current outside-lock `ReloadData` construction into a candidate that cannot
  reach active caches or handlers.
- Acquire base and complete net-new capacity before allocation; charged task and
  residency groups remain through actual final drop.
- Treat deserialized snapshots as bounded candidate seeds under the same admission.
- Bump snapshot format and implement the v10/portable untrusted-seed Adapter with
  pre-decode capacity, complete re-observation/verification, quarantine, versioned
  atomic replacement, preserved rollback artifact, and source-rebuild fallback.
- Register a bounded invalidation accumulator before observation. Observer replacement
  always publishes non-Current/Gap, drains/releases the predecessor, mints a new
  incomparable post-barrier observer, then performs a full successor baseline.
- Add observer epochs, physical-root validation, monotonic invalidation cuts, latched
  gap coverage, and the single accumulator/promotion/query linearization domain.
- Start with the simpler full-rebuild Implementation behind a dark feature gate; a
  changed sequence invalidates the attempt and no independent atomic counter exists.
- Add the bounded coalesced path accumulator and structurally shared delta candidates
  before public enablement; unchanged immutable structures retain their original
  capacity owners.
- Promote only complete strict-current scopes.
- Route every delivered observer event and targeted refresh through an isolated,
  capacity-reserved delta/full candidate. No legacy in-place publication lane remains
  once candidate promotion is reachable.
- Route every SymForge-owned source edit/curation intent through the same pre-write
  non-Current transition before its first disk side effect.
- Include temporal, bridge, authority, and other advertised derived work inside the
  complete candidate; none starts as a later mutation of Current state.
- Prebuild publication artifacts and reserve capacity before the commit locks.
- Add the root-scope discovery obligation, all entry verification obligations, racy
  proof, reserved rolling verifier, monotonic deadlines, fenced proof-refresh
  publication, and overdue acquisition guard.
- Add project/single-source strict query leases, multi-project selection rules, and
  separate ranking snapshot composition.
- Define one process-wide, non-user-selectable lifecycle mode chosen once by
  `ProcessIndexRuntime`. Pre-activation builds use legacy authority; the v11 activation
  PR selects `PreventiveV1` for daemon, stdio, serve, embed, snapshot, observer,
  mutation, local-ref, and derived lanes simultaneously. PreventiveV1 contains no
  legacy in-place fallback and cannot vary by request/source/env.
- Use the same mode-epoch registration gate for legacy query execution, cache/CCR
  reads and writes, retrieval, and finalization. The activation cut closes it, drains
  admitted responses, invalidates v10 cache/CCR state, installs the successor epoch,
  and only then opens PreventiveV1; no unregistered cache or retrieval Adapter remains.
- In that same activation PR, make every inventoried legacy constructor, writer,
  callback, public export, and secondary publication root unnameable or unreachable;
  revoke/drain old callbacks before new authority starts. Remove the v10 raw embed
  bypass and expose only the authority-bearing v11 allowlist. Slice 5 may delete dead
  storage/code but may not own an authority-retirement step.
- Public enablement requires versioned `ObservedRefreshGateV1`: pinned baseline
  commit `1521abb0`; named/digested SymForge and maximum-admitted calibration corpora;
  fixed add/modify/delete/rename/terminal-classification and edit-burst workloads;
  cache/quiescence state, concurrency, OS/host class; a versioned receipt schema; and
  checked-in commands/results. The Adapter trigger matrix is explicit: daemon, stdio,
  and serve use their managed observer plus authoritative polling; embed uses
  `EmbeddedSourceContractV1`'s managed observer/poller. Delivered-event,
  `need_rescan`/gap, and intentionally suppressed-notification campaigns all run.
  Measurement starts at controlled write close/fsync or SymForge mutation commit and
  ends at the first normalized visibility receipt containing the new byte identity,
  canonical manifest effect, and every required certificate. The legacy comparator
  uses the first existing externally observable response containing that same effect;
  it does not pretend v10 has a strict lease. Each operation has an exact completion
  oracle, and every delta result must match a clean full rebuild's manifest, sealed
  artifact digests, and representative query corpus before latency counts.
- On those pinned corpora, p95 must be <=2 seconds, maximum <=5 seconds, and p95 no
  worse than 1.25x the pinned baseline. A single-path hint must not request a full
  candidate unless coverage is `Gapped/ScopeDirty`.
- Gate retained-plus-candidate peak memory and sustained burst convergence in the same
  cut: peak admitted accounting is bounded by pre-granted delta residency plus declared
  scratch/headroom, with no admitted residency group outside accounting; observed RSS
  is recorded but not misrepresented as allocator-exact. No shipped intermediate may refuse
  for full-repository rebuild after each edit.
- Add an activation oracle proving legacy and PreventiveV1 publication roots are never
  simultaneously authoritative.
- Add inactive-project eviction only after measured pressure demonstrates need.

### Slice 5 — Remove obsolete mechanisms

- Delete already-unreachable bootstrap/circuit-breaker lifecycle fields, placeholder
  storage, legacy mode branches, obsolete tests, and compatibility comments.
- Remove dead secondary-root structs and v10 embed implementation files only after the
  activation allowlist/negative external suite proves they were unnameable in Slice 4.
- This slice is mechanical cleanup: it cannot change runtime authority, public
  behavior, writer reachability, or activation mode.

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

### Module-local state models

Do not build one state-space-exploding model. Keep four compact models with explicit
assumptions about their adjacent Interfaces:

1. **Registry model:** `pending-open, join, fixed-base-grant, pending-cancel,
   expose-slot, freeze, drain, install, retarget-propose, retarget-commit, close,
   reopen`. Assert one pending/live/stopping entry per identity, no slot before all
   fixed revocation capacity is charged, never-reused slot IDs, no successor before
   destructive/publication authority drains, single-flight join, exact-once late-grant
   refund, exact pending-root lease continuity, atomic cancel-versus-install/registration
   transfer, and failed retarget leaves prior membership untouched.
2. **Source lifecycle model:** `observer-register, observe, gap, capture-cut,
   candidate-start/fail, permit-grant/start/commit/drop, promote-store, prune, query,
   verify-step/overdue, revoke`. Assert no candidate query, closed Current state,
   stable observer token across promotion, mutation-epoch/zero-permit promotion,
   publish-before-prune, unwind-safe safety transition under exhausted ordinary
   capacity, and no
   root-A write/publication into B.
3. **Capacity model:** `enqueue, dispatch, accept, resize, return,
   commit-allocation, share, drop, pin, bypass, drain-barrier, tombstone, cancel,
   close`. Assert conserved
   reserved-plus-charged vectors, one charge per accounted group, exact-once refund,
   exact hierarchical owner/ancestor matching across process/pending/project/source/
   Adapter/partition scopes, no lock callback cycle, no hold-and-wait, oldest-feasible
   liveness, disjoint progress, and `Blocked` exactly when no independent capacity-
   changing transition exists.
4. **Process ownership model:** `construct-wrapper, clone-wrapper, retain-control,
   register, expose, shutdown, drop-wrapper, retire, reconstruct, drain, stop`. Assert no
   source/Adapter/partition is exposed before registration, factory creation loses to
   or joins the one shutdown cut, child control leases cannot keep the public factory
   gate Live, owner disappearance cannot precede Retiring, explicit shutdown/final-
   wrapper Drop share one receipt, no successor before old destructive/publication
   drain, every incarnation reuses one stable capacity/parser ledger, no publication-
   capable survivor detaches, and finalization cannot self-join.

Use proptest command sequences for each pure model. Maintain separate small TLA+
specifications for process ownership, registry identity, source
promotion/invalidation, and capacity admission; do not combine their state spaces. Each model states the guarantees it
assumes from adjacent Modules. Liveness is conditional on a quiescent readable source,
sufficient replacement headroom, bounded task completion, and fair scheduling.
Concurrency transitions live in one production synchronization kernel parameterized
by a small `cfg(loom)` primitive Adapter; Loom invokes the same transition functions
used in production, never a hand-copied replica. The traceability table names that
kernel/failpoint for every modeled transition.

### Provenance and ranking algebra

Pure tests enumerate `AtomicAuthority`, `ClaimInput`, and every closed
`Comparison/Derivation/SelectedAggregate` operation. They prove no
formatter/cache/CCR round trip drops an input; `SelectedAggregate` rejects empty,
missing, extra, not-captured-by-the-sealed-lease, or mismatched selection/generation
maps, while a valid captured lease remains usable after a later live membership/runtime
transition; a Generation atom
cannot replay across same-manifest rebind/restart;
metadata-only estimates and Git non-membership remain representable; path-local disk
observation yields no completeness; and only a complete root-bound
`WorktreeScopeObservation` may support worktree-scan totals. Operation fixtures cover
`detect_impact`, untracked search, Tier-2 disclosure, syntax fallback, and all diff
modes. Cross-query/filter/consistency/algorithm fixtures prove the opaque normalized
`OperationReceipt` is preserved and prevents cache/CCR confusion.
The same generated matrix enumerates `OperationContractV1` and every closed
`SourceRefusal` variant. It rejects mismatched operation/provenance/evaluation,
illegal subject/cause/basis/retry/status combinations, empty or non-bijective
`ResolvedSelectionSet`, and resolved identity before authorization. Multi-project
mixed-readiness refusals round-trip all per-source evidence without becoming a
negative claim.

Ranking is tested in its own Module: one captured never-reused persistent-store
instance and session incarnation plus versions, evaluation time, and policy identity
per `RankingSnapshot`; replacement/reopen with reset counters cannot collide, no
reopen occurs after capture, and a concurrent
commitment bump affects later execution contexts only. Observable order/scores change
the preserved `EvaluationProvenance`/cache envelope but cannot change readiness,
truth, or absence. It is not part of lifecycle model state.

### Concurrency and cross-seam tests

Use Loom for:

- root-pinned pending-admission single-flight, cancel-versus-install, and Freeze ->
  Drain -> Install with queued/claimed/running work;
- mutation permit before temp/replace versus same-path root replacement;
- two-source project Freeze versus query/source-add/observe/promotion;
- promotion versus permit grant and synchronous invalidation at the observation cut;
- runtime store versus post-store idempotent pruning;
- queued old-observer delivery after promotion and observer gap/handoff;
- two-source close/rebind versus observe/promotion under accumulator-to-writer order;
- capacity dispatch/accept/resize/refund versus hierarchical pool tombstones,
  close/reopen, and final charged-root drop;
- grant drain barriers, pin release, disjoint progress, and replacement-headroom
  refusal;
- query acquisition versus promotion and A-only pin versus B churn;
- safety-transition package/transfer guard versus exhausted ordinary capacity and
  unwind immediately before the runtime-root store;
- checked counter exhaustion versus epoch replacement.
- pending project admission grant/exposure/root replacement versus cancel/shutdown,
  and public factory wrapper Drop/reconstruction versus child-only control leases,
  in-flight mutation, residual charges, and open registration.

Cross-seam failpoints cover charge conversion before promotion, prepared runtime
publication, post-lock reclamation, project-query capture, and root-compatible
`ClaimContext` construction. Deterministic filesystem/task tests include Windows
sharing violations, handle-relative temp/replace pauses, rename/delete/recreate ABA,
watcher overflow, already-running charged blocking work after cancellation, bounded
snapshot decompression, and failed observer handoff.

A generation-read failpoint replaces bytes after lease acquisition and proves the
response serves generation-owned/digest-identical bytes or refuses. A rebind between
Git/generation/disk acquisition refuses rather than deriving across roots. Same-stamp
suppressed-notification tests cover Indexed bytes and content-derived terminal
dispositions; snapshot startup must prove every obligation before promotion. Pausing
the rolling worker past its monotonic deadline makes strict query acquisition refuse.

### Release gates

- hash-pinned Feature 020 refreeze manifest and detached design/context/amendment-set
  attestation complete: every artifact classified, every Section 3 replacement mapped
  to requirement/contract/task/test IDs, manifest validator and cross-artifact
  analysis clean; the detached-attestation digest and exact commit/tree also match one
  trusted signed append-only `RefreezeApprovalRecordV11` supplied outside the tree;
- coordinatedly rewrite the manifest, design, context, API digest, and checked-in
  attestation while retaining the prior external approval record; the gate must fail.
  A newly reviewed signed record over the successor commit is the only accepting path;
- formatting and Clippy with warnings denied;
- focused lifecycle/capacity/watcher/snapshot suites;
- invariant→production-seam→positive-control/acceptance-oracle→model→slice→exact-
  command traceability complete, with `cfg(loom)` production kernel and declared
  bounds/fairness/CI artifacts;
- serial all-target test suite;
- release build and canonical tool fixtures;
- cold-start race campaign with a working positive control;
- measured memory test with concurrent projects, retired query-pinned generations,
  retained-current-plus-candidate overlap, snapshot scratch, and invalidation
  accumulators;
- checked-in `ObservedRefreshGateV1` campaign proving coalesced delta latency/memory
  on the pinned baseline and corpora;
- every advertised edit class passes sealed delta-vs-clean-full-rebuild equivalence
  for manifest, RequiredArtifactSet digests, and representative query corpus;
- racy-clean and rolling-verification campaign including same-stamp rewrites and
  intentionally suppressed notifications;
- claim/provenance matrix across `OperationReceipt`, `Generation`,
  `DiskObservation`, `WorktreeScopeObservation`, `GitObservation`, `Comparison`,
  selection-bearing n-ary `Derivation`, exact-bijection `SelectedAggregate`,
  `EvaluationProvenance`, and typed `SourceRefusal` bases, with text/structured/cache/
  CCR/persistence/retrieval round trips that preserve the complete envelope;
- cross-query/filter/consistency cache-confusion negatives and equal-shape
  nonexistent-versus-unauthorized `InvalidSelection` refusal tests;
- same-process activation and restart campaigns seeded with apparently valid v10
  response-cache records and CCR handles; every legacy entry is an untrusted miss or
  refusal and none can be upgraded into v11 authority;
- race every legacy query/cache get/cache put/CCR lookup/store/`symforge_retrieve` and
  response-finalization boundary with `LegacyOpen -> LegacyClosing ->
  PreventiveV1Open`. A pre-cut registration may complete but activation must drain it
  before invalidation; a post-cut attempt cannot touch legacy state; no late write or
  rendered retrieval may appear after invalidation under the new epoch;
- v11 external dependent-crate positive/compile-fail suite across the attested
  supported target/cfg/feature matrix, plus all-cfg inventory and completeness fixture,
  proving the closed allowlist and no inactive-target, negative-cfg, raw/deep,
  unverified lifecycle, or Git bypass;
- attested canonical `contracts/public-api-v11.json` whose generated tests name every
  externally reachable rustdoc/public-item and impl edge for every supported target/
  cfg/feature combination and reject all others, including inactive target/negative-
  cfg edges, direct/auto trait impls, associated items, constants, statics, and macros;
  explicit negatives cover `Clone` on `EmbeddedSourceHandle`,
  authority-minting `Deserialize`/`Default`/`From`, and raw-internal
  `Deref`/`AsRef`/`Borrow`;
  checked-in v10→v11 public-Interface matrix, AAP embed migration guide/consumer
  receipt, supersession links, and snapshot format/rollback/quarantine migration
  suite;
- pinned unmodified-v10 concurrent-writer campaign proving the disjoint v11 snapshot
  namespace, opened-object seed identity, safe source-rebuild fallback, and no
  in-place legacy restore without proven writer quiescence;
- activation oracle proving one process-wide mode and no simultaneous legacy/
  PreventiveV1 publication authority;
- adversarial architecture review before Slice 1 and code review after every slice.

## 14. Rejected alternatives

### Add more readiness predicates

Reject. It preserves distributed lifecycle ownership and makes another illegal state
distinguishable only after it has been constructed.

### Keep the placeholder but prevent only targeted freshening

Reject. Watcher reconciliation, snapshot verification, Git temporal work, future
handlers, and independently sampled identity can still touch it.

### Publish degraded last-valid wrappers

Reject as an automatic default fallback. A visible stale label does not close source
identity, claim-scope, absence, cache, prompt, or sidecar semantics, and “interactive
MCP” is not a closed capability. Keep the retained verified generation unchanged and
expose runtime state separately. A future explicit `LastVerifiedReadContractV1` may
offer generation-owned positive evidence only after Spec 027 identity disclosure and
its own adversarial review; it is not part of strict-current prevention.

### Replace the lifecycle with `Loading | Ready | Refused`

Reject the three-state shape while accepting its core insight that refusal cannot be
encoded as successful index construction. It cannot represent a retained verified
generation while replacement work runs, observer coverage, revocation, or terminal
operator action without splitting authority into side fields. The private closed
`SourceRuntimeState` enum keeps those mutually exclusive facts in one Module while
presenting a smaller query Interface.

### Use one standalone dirty counter and one publish lock

Reject as written. A callback can increment after the builder's final comparison but
before publication, and a scalar increment cannot represent a disconnected observer
or physical-root identity. Use a synchronous per-source invalidation accumulator
inside the lifecycle Module: observation ingress and promotion share the
accumulator-to-writer linearization order, gaps latch, and the runtime state publishes
atomically. The first full-rebuild Implementation may use only its sequence and gap;
the public enablement also requires bounded coalesced path hints for delta candidates.

### Guard only the two largest allocation sites with a byte semaphore

Reject as incomplete process accounting. It omits current, candidate, retained,
query-pinned, structurally shared, snapshot-decode, parser, cache, accumulator-base,
checkpoint/export, and still-running blocking-task residency. The capacity-pool
Module may begin with conservative classed byte estimates, but every
allocation-producing Interface must reserve before construction and attach one exact
RAII charge to each declared accounted residency group through actual final drop.
Opaque dependency residency remains conservatively enveloped; the design does not
pretend to measure every allocator call.

### Keep the raw v10 embed facade beside the preventive API

Reject. `LiveIndex::empty`, raw snapshot conversion, direct mutation, and
authority-less result signatures can bypass readiness/provenance and make the headline
guarantee false. The superior result takes the honest v11 break rather than excluding
one public entry path in a footnote.

### Make lifecycle mode user-selectable

Reject. Per-request/source/environment selection would preserve two production
publication authorities and multiply every invariant. The mode is process-wide,
chosen once, and PreventiveV1 activation has no legacy fallback.

### Capability-scoped partial promotion in the first lifecycle

Reject pending a separate proof-carrying design. Without a closed capability set,
derivation rules, and invalidation rules, this merely renames degraded coverage.
The first lifecycle promotes the closed `StrictScopeContractV1` only and keeps
retained last-known-good state private.

### Incrementally grow a partially allocated candidate lease

Reject. Two candidates can retain partial allocations and wait forever for capacity
held by the other. Use a bounded base scout grant, then atomically acquire the complete
net-new candidate vector or release and queue.

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

The round-two external reviewer must answer these before implementation:

1. Does the design prevent partial promotion, or merely move “degraded” into another
   label?
2. Do the project registry, per-source lifecycle, and capacity pool place each
   ownership rule at one deep interface without creating a God coordinator?
3. Can the invalidation-accumulator protocol lose a delivered event at registration,
   cut capture, gap/disconnect, delayed OS delivery, or promotion?
4. Can any disk/Git observation be attributed to a verified generation, can a mixed
   response omit an input from its provenance, or can a rebind make a derivation
   combine incompatible roots?
5. Do Binding/Observer/Candidate/Mutation authority lifetimes eliminate delayed-event
   rejection, generation/root split-brain, and same-path replacement?
6. Can a granted destructive permit ever write after revocation or into a successor
   root? Does Freeze -> Drain -> Install cover every permit/work/callback state?
7. Is capacity accounting closed over current, candidate, retired query-pinned,
   structurally shared, cache/base, snapshot/checkpoint scratch, invalidation, and
   blocking-task residency without a reservation/charge gap or double charge? Can
   exhausted capacity still publish the safety transition?
8. Can oldest-satisfiable scheduling progress unrelated projects and eventually drain
   for an old feasible vector without expiring a
   live query lease or starving the bypassed request after its blocker releases?
9. Do root-scope discovery plus every Indexed and terminal entry obligation, rolling
   deadlines, proof-refresh publication, and snapshot proof close suppressed-create
   and same-stamp/notification-loss staleness?
10. Is strict-current the correct default while a separately reviewed, explicit
   positive-evidence-only last-verified contract remains deferred?
11. Does project query acquisition preserve current-source independence while
    refusing incomplete explicitly selected multi-source scopes?
12. Does the migration sequence preserve source identity, snapshot quarantine,
   per-source independence, deletion convergence, protected-root membership, and
   strict sidecar refusal?
13. Are all frozen Feature 020 artifacts refrozen consistently, and does any amendment
   weaken a load-bearing safety property?
14. Is there a smaller Module Interface that preserves the same invariants and
   locality?
15. Can snapshot restore, local stdio, standalone serve, or the v11 embed facade
    bypass the runtime and capacity seams?
16. Does activation ever create two publication authorities, an
    uncharged retained-plus-candidate path, or a public full-rebuild-per-edit refusal
    regime?
17. Name a concrete interleaving that still promotes, serves, or writes untrusted
    state.

The design may be frozen for external review only after an internal adversarial pass
has either resolved or explicitly recorded every P0/P1/P2 finding.

## 16. External round-one disposition

The round-one findings are accepted as defects in the frozen first draft. The
reviewer's findings document remains verbatim and hash-bound; this section records
the design changes rather than rewriting that evidence.

| Finding | Disposition in this revision |
|---|---|
| F1 — live disk bytes can be labeled as generation G | Accepted. Primitive source-truth facts carry atomic Generation/path-local Disk/complete WorktreeScope/Git authority; comparisons and n-ary derivations bind an OperationReceipt plus every source/selection input, while observable ranking carries separate EvaluationProvenance. `read_gate.rs` becomes typed root-bound Adapters and mixed tools use identity-compatible ClaimContexts. |
| F2 — stamp collision plus notification loss can preserve staleness | Accepted. Stamps are hints only; promotion adds racy-clean obligation proof, snapshots fully verify all Indexed and terminal obligations, and a capacity-reserved deadline-enforced rolling verifier covers them fairly. |
| F3 — multi-source close/rebind lock order was unspecified | Accepted. Freeze is writer-only; Drain holds at most one accumulator in canonical `SourceId` order and permits only accumulator-to-writer nesting; Install waits for destructive/publication authority. |
| F4 — strict FIFO can convoy unrelated projects behind pinned residency | Accepted. Admission uses multidimensional oldest-satisfiable scheduling with pin-aware parking and bounded-bypass drain barriers; live leases are never forcibly expired, and release reconsiders waiting work. |
| F5 — Slice 4 could ship refusal-per-edit before deltas | Accepted. Candidate activation, observer invalidation, and delta refresh are one indivisible public enablement slice with explicit latency and memory gates. |

The reviewer's simplifications also influenced the Implementation without weakening
the Interface: resource refusal is no longer an index success; the ordered replay
journal is reduced to a bounded invalidation accumulator; and positive last-verified
reads are recognized as a useful future product lane. The three-state slot enum,
standalone dirty counter, broad automatic stale fallback, and two-site byte semaphore
are rejected above because each drops a load-bearing identity, liveness, or aggregate
ownership invariant.
