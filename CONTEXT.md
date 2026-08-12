# SymForge Domain Context

This file names the domain concepts used by SymForge architecture and design
work. These names describe product behavior and invariants; implementation type
names may differ while a design is still under review.

## Project index lifecycle

**Pending project admission** — A bounded, process-base-charged registry record that
gives concurrent authorized opens one single-flight attempt before a project slot
exists. Immediately after membership authorization and before root I/O, the opener
mints a never-reused non-authoritative admission-attempt identity. A successful root
capture adopts that same identity into the pending record. The record owns
cancellation/capacity state and a drain registration but no runtime root, query,
source, observer, work, or mutation authority. It owns the exact pinned physical-root
lease captured before creation;
joiners must exact-match a fresh root identity and work never reopens the path. Its
closed Open/Cancelling publication and Live-slot installation contend under the
process-registry writer, transferring or draining the root lease and registration
exactly once; install also proves the canonical path still names that held object.
Because no binding exists yet, an authorized pre-slot root failure is described by a
sealed **admission-root attempt**, never by fabricating `BindingAuthority`. That
non-authoritative basis owns the attempt identity, optional successfully held root,
and typed attempted/open-failure evidence. It is legal only in
`AdmissionUnavailable` and can never enter claim
provenance or establish source truth.
Only one transaction that
charges the complete project and initial-source fixed bases plus every revocation
package may CAS it into a live project slot. Close/shutdown cancels and drains it
without publishing a project runtime root.

**Project slot** — The stable per-project ownership record. A slot may exist while
no index is queryable. Sessions join the canonical registry entry—pending admission
or this slot—rather than constructing independent loads. A stopping slot remains registered as a
tombstone until observers and publication-capable workers are quiescent. Immutable
retired generations may outlive the slot under independent capacity ownership.

**Slot instance ID** — A process-unique, never-reused identity for one project-slot
incarnation. Close/reopen cannot make stale work authoritative by recreating the
same logical project ID.

**Source slot** — The per-source lifecycle record within a project slot. The current
worktree and each admitted local-ref source have one independent runtime state,
observer identity, invalidation sequence, and candidate owner.

**Source runtime state** — The private closed publication that makes queryability
explicit: `Loading`, `Current`, `Refreshing`, `Blocked`, or `Stopping`. Stable binding/
observer identity, retained state, and work are fields of the applicable enum variant,
not independently combinable side fields. Live observer coverage and sequence remain
solely in the lifecycle-owned invalidation accumulator and are exposed with runtime
state only through one composite capture. Only `Current` contains a strictly queryable
generation. `Refreshing` and `Blocked` may retain an immutable last-known-good
generation for recovery without silently making it `Current`.

**Source publication token** — A registry-minted opaque identity for one exact
`SourceStatePublication`. The source-lifecycle Module may retain and compare the token
but cannot name, clone, inspect, or construct the registry's private runtime root. A
sealed lifecycle transition intent is adapted by the registry into an opaque prepared
delta outside the publication writer; commit exact-matches the embedded token and
patches the latest whole project root. This is the only cross-Module publication seam.

**Observer handoff phase** — The closed non-stopping `ObserverPhase = Absent |
Active { token, next_handoff_package } | Draining { old_token, package_remainder } |
ObserverFree { handoff_id, retry_trigger, package_remainder }`. It truthfully
represents the observer-free interval after predecessor release and before successor
admission while optionally retaining last-known-good state. The same closed
`Absent | Active | Draining | ObserverFree` phase is used by Loading, Refreshing, and
Blocked, so cold-start observer failure is representable too. Successor accumulator
installation and the `Active/BaselinePending` runtime store precede opening external
ingress. One `ObserverActivationRegistration` exists before Active/callback exposure,
external callbacks remain logically gated until exact post-open revalidation, and
Freeze waits that registration. Only Active+Complete with an unchanged cut can contribute to promotion. Phase
publications are precharged. Every Active observer owns a fresh non-borrowable package
for its next complete handoff; a successor cannot become Active or open ingress until
its own package is charged. Repeated immediate disconnects therefore remain fail-safe
under exhausted ordinary capacity.

**Candidate generation** — A complete index build in progress. It is isolated from
queries, caches, checkpoints, and every committed-state mutation until promotion.

**Verified generation** — An immutable, source-bound generation whose manifest,
identity, observation cut, and capability completeness have passed the
promotion checks.

**Current generation** — The verified generation inside `SourceRuntimeState::Current`.
Only promotion creates this state. Failed work leaves the retained verified
generation byte-for-byte unchanged but does not call it Current.

**Binding epoch** — A monotonic incarnation number for one project/root/source
binding. It is one member of mutation authority and is never sufficient authority by
itself.

**Binding authority** — The stable, indivisible authority for one binding: slot
instance ID, project ID, binding epoch, physical-root identity, source identity, and
canonical root. It contains neither an observer epoch nor a base generation and is
accepted or rejected as one value.

**Observer token** — A binding authority plus one observer epoch. It remains valid
across successful generation promotions for that binding and is revoked only by
observer replacement, binding revocation, or slot revocation.

**Candidate authority** — An observer token plus candidate ID, observation cut,
captured mutation epoch, attempt, and optional base generation. Cold candidates have
no base generation.

**Mutation authority** — A binding authority plus the exact `Current` generation,
mutation epoch, permit ID, and bounded path scope. It is never reused across
promotion or revocation.

**Physical root lease** — An owning platform handle plus file identity proving which
directory object is authoritative for a binding. Observation opens and destructive
writes resolve every relative component beneath that handle with no-follow/reparse-
safe semantics to a validated final-parent handle; temp/create/replace all use that
parent. A directory handle plus an unchecked multi-component path is not containment.
Byte/metadata observations also retain the actually opened confined handle through
the receipt. Pre/post pathname validation cannot create authority because replacement
ABA can restore the path. A path-only read may produce non-authoritative diagnostics
outside `ClaimProvenance`, never `DiskObservation`. Destructive mutation has no
path-only fallback; a platform without equivalent safe beneath-root component
resolution refuses.

**Capacity reservation** — Preallocation permission for a declared capacity-class
vector. It becomes a `CapacityGrant` when admitted. Uncommitted units are refundable;
reservation alone is not proof that allocated residency remains charged.

**Allocation construction guard** — A grant-local, non-cloneable debit acquired
before the allocator or opaque constructor runs. Its units remain reserved while
physical memory is being built. It either commits that memory into one allocation
charge, or owns/final-drops it before refunding. Grant sealing forbids new guards and
cannot refund or complete deallocation conversion until every existing guard reaches
one of those terminals. Resize-seal and deallocation-seal are mutually exclusive
monotonic close modes; a losing closer joins the winner, so no unit can refund twice.

**Grant seal driver** — A lifecycle-owned, precharged control obligation registered in
the `WorkLineage` before a resize/deallocation pending state is published. It remains
claimable independently of the initiating stack frame and outside the predecessor
publication Drain set. The later grant close-cell CAS installs exactly one winning
mode/driver; if another closer wins, the pending driver joins/helps that winner and
takes the explicit losing-mode edge. Panic, cancellation, or a losing closer therefore
cannot orphan guard settlement, candidate-root cleanup, proof creation, or the exact-
once pool transaction. A caller cannot wait on a construction guard it still owns: it
first commits, drops, or transfers that guard to the driver.

**Accounted residency group** — One root allocation or conservative allocation arena
whose whole class-vector charge is owned and testable as a unit. This is the exact
accounting unit; it does not claim allocator-level knowledge of every dependency
allocation.

**Allocation charge** — The unique charge owner stored in an accounted residency
group's shared control block. `AllocationConstructionGuard::commit` atomically converts
the in-construction debit into this charge without changing total available-reserved
plus in-construction plus charged units.
Aliases and structurally shared generations retain the same charged root. The charge
releases only after the final root/alias drops.

**Safety-transition package** — A precharged, preallocated publication package held
immutably by every `Current` source. A `SafetyPublicationGuard` may stage the successor
non-current state in that package, but it does not destructively take or disarm the
published package before the runtime-root store. Abort or unwind before that store
leaves the package armed; the store transfers the same charged residency into the
successor state. It is not permission to allocate later. Project/query leases never
retain it, and promotion must install its successor.

**Next-handoff publication package** — The charged preallocated runtime-root shells
owned by every Active observer, including the observer embedded in Current. It can
publish Active→Draining, Draining→ObserverFree, and a successor activation without
ordinary capacity. The remainder travels with handoff state; successor Active is
forbidden until a fresh package for its *next* handoff is installed. Charges release
only with the final retained phase-publication root. Non-handoff transitions preserve
the package for the same observer; a different observer must bring a fresh one.

**Process capacity domain** — The one aggregate capacity authority for all index
runtimes in a process, including the process-global blocking/parser pool. Default
runtime construction and every sequential factory incarnation joins the stable process
domain; final-wrapper Drop never resets its ledger while old charges survive. An isolated embedded domain is legal
only with an explicit partition token minted from a host-owned parent domain; child
partitions cannot sum beyond the parent budget. Every child reservation, allocation
charge, task envelope, response, snapshot, and alias owns the child-domain `Arc`; the
partition returns parent units only after the final such owner drops, never when the
runtime handle alone drops.

**Capacity owner key** — A closed product of (1) a capacity-domain axis—process root or
an exact child-partition identity—and (2) one lifecycle owner: process work,
pending-project admission, project work, pending-source admission, exact source
`BindingAuthority`, server Adapter, or domain-control work. Each value derives its
canonical domain/process/project/source/owning revocation scopes; callers cannot add or
omit ancestors. Requests, grant tickets, accepted reservations, and resize successors
retain the same value. An Adapter/admission lineage terminalizes before a new source
lineage is minted; owner identity is never mutated as a transfer shortcut. Enqueue and
resize check every closed ancestor, so a child-partition tombstone reaches its source
work, two sources cannot alias, and work created before a `SourceSlot` exists is still
revocable.

**Work lineage** — The publication/callback authority of one scheduled capacity job,
identified by a never-reused `WorkId`, exact capacity owner, original scheduling age,
and owning drain registration. It may cycle through immutable queue/grant attempts,
but candidate underestimation first seals that grant against new construction guards,
waits every admitted guard to commit into cleanup ownership or abort/final-drop, then
discards and final-drops every candidate-private charge before a complete larger
request is enqueued. The old grant's one unused-
reservation token remains owned until the subsequent pool-locked tombstone-or-requeue
transition refunds it exactly once. V1 has no retained-partial-candidate resize lane.

**Detached charge owner** — The deallocation-only terminal produced when revoked or
escaped work may retain accounted memory after all callback, enqueue, resize, and
publication authority is gone. Conversion atomically releases the work lineage's
drain registration only after its grant is sealed: uncommitted units refund once and
no new construction guard can begin; every earlier guard has committed its charge or
final-dropped before refund. All committed charges plus capacity-domain ownership move
into an independent retained owner. Lifecycle
tombstones may then drain, while the stable capacity ledger remains debited until that
owner's final `Arc` drops. An executing closure may leave a source/project publication
drain only after result adoption is revoked, but it remains in the process executor-
join set until exit; only completed escaped results/charges may outlive process
`Stopped`.

**Invalidation accumulator** — The bounded per-source process-local observer state:
observer epoch, monotonic invalidation sequence, latched coverage (`BaselinePending`,
`Complete`, or `Gapped`), a monotonic acknowledged sequence, an optional sequence-tagged
scope-dirty marker whose causes and required scope/policy versions merge monotonically,
and coalesced latest-path hints. Hints optimize candidate rebuilding but
are never source truth. Overflow, disconnect, or unknown ordering latches `Gapped`
and requires full authoritative observation.

**Verification progress** — Lifecycle-owned, bounded, non-authoritative work state for
one exact source publication and WorkId: pass ID, fair cursor, and charged staged
proofs. It survives retry only while that exact publication/work owns the pass and is
discarded on publication change. It is never queryable and cannot renew immutable proof
deadlines; only a fenced `ProofRefreshCandidate` can do that.

**Observation cut** — One observer epoch plus invalidation sequence captured for a
candidate. Cuts from different observer epochs are incomparable.

**Promotion** — The single atomic commit that validates binding, physical root,
observer coverage, invalidation sequence, strict scope, and capacity ownership, then
publishes a prebuilt candidate as `Current`. Runtime publication is the linearization
point; accumulator acknowledgement/pruning occurs idempotently afterward.

**Runtime snapshot** — One immutable closed project publication:
`Live { sources, project_membership_publication, runtime_epoch,
revocation_publication_package }` or
`Stopping { revocation, retained_sources, runtime_epoch }`. It uses charged structural
sharing. Only `Live` can grant query/source-add/work/permit authority; one project-root
load yields sources and protected project/source membership coherently. Project
membership changes publish a prepared project-root delta under the same writer; no
caller combines a runtime root with a separately loaded project membership map. A source state never stores a separately
combinable binding or observer fact beside its enum.

**Session binding publication** — The registry-owned, immutable, never-reused active-
project and authorized working-set membership authority for one session incarnation.
Retarget and additive/removal membership changes commit exactly one CAS of this
publication. Query acquisition captures it, loads each selected named project runtime root,
then exact-identity revalidates the session publication; the receipt retains both
identities. Provisional target membership and old-project cleanup are not authority.

**Revocation publication package** — A project-slot base charge containing the
root-agnostic, preallocated, unwind-safe shell for a `Live -> Stopping` runtime
publication. Under the writer it fills from the latest Live persistent-map root with
no allocation; it does not optimistic-compare an older whole-project root. It is
separate from per-source safety packages and remains armed regardless of whether
the Live map contains Loading, Current, Refreshing, Blocked, or source-Stopping
members. Freeze never needs ordinary
capacity and cannot starve behind unrelated source publications.

**Source revocation publication package** — A separate source-slot base charge armed
in every source state. Under the project writer it fills from the latest Live map and
exact current source publication without ordinary allocation, then stores that source
as `Stopping` while unrelated sources remain live. It is distinct from the
Current-only safety-transition package and project-wide revocation package.

**Ranking snapshot** — An immutable, versioned per-request view of mutable
non-source evidence such as session/persistent frecency, with one evaluation time and
ranking policy version. If order/scores are observable it yields an
`EvaluationProvenance` receipt (never-reused persistent-store instance and session
incarnation, their versions, evaluation time, and policy identity) preserved in the
claim envelope/cache key. Reopen/recreate mints new identities even when counters
restart. It may affect ordering only; it
cannot establish source truth, readiness, or absence.

**Atomic authority** — The provenance of one primitive **source-truth** fact: `Generation`,
`DiskObservation` (bytes, metadata, or final-parent-backed `PathMissing`), `WorktreeScopeObservation` (one complete
root-bound scan interval), or `GitObservation` (object membership or non-membership).
Generation authority includes the full never-reused binding authority. It never
silently changes lanes.

**Claim provenance** — `Single`, a typed binary `Comparison`, a closed-operation
`Derivation` with a non-empty input set of atomic/selection authorities, or
`SelectedAggregate`. The aggregate carries one or more source-selection receipts and
an exact bijection from every selected project/source to its generation authority;
private construction rejects missing, extra, not-captured-by-the-sealed-lease, or
mismatched generations. A later runtime or membership transition does not invalidate
a completed claim derived from the captured lease. Every
source-truth claim names every input; formatters, caches, and CCR handles preserve the
full claim envelope.

**Operation receipt** — The opaque normalized identity of one public operation:
operation/schema version, canonical argument hash, selector/filter/consistency IDs,
and every value-affecting algorithm/policy version. Every claim and refusal
carries it. Cache, persistence, CCR, and retrieval keys cannot confuse two predicates
or filters merely because they share source authority.

**Operation contract** — The checked-in closed `OperationContractV1` table keyed by
operation kind/schema. It fixes allowed provenance forms and input roles/cardinality,
selection and evaluation requirements, and legal refusal/transport mappings. One
private builder constructs operation receipt, claim provenance, evaluation provenance,
or refusal together; callers cannot independently combine a valid-looking envelope.

**Activation response gate** — The process-wide mode-epoch gate shared by legacy
query execution, cache get/put, CCR lookup/store/retrieve, and response finalization.
Each operation retains one exact epoch registration through materialization. Activation
closes registration, drains admitted operations, invalidates legacy state, installs the
successor schema/epoch, and only then opens PreventiveV1; no late write or retrieval can
straddle the cut unregistered.

**Refreeze approval record** — The signed, append-only release-provenance record that
anchors Feature 020's checked-in detached attestation outside the mutable repository
tree. It binds the exact target commit/tree and detached-attestation digest under the
trusted release identity. Internal hashes prove consistency; this external record
proves which consistent set was approved. Any coordinated rewrite therefore requires
a new reviewed approval record rather than passing by rewriting its own anchor.

**Disk observation** — One root-bound receipt containing binding/physical-root
identity, path, observation time, and either stable-read byte identity, metadata, or
`PathMissing` evidence from a beneath-confined pinned file/final-parent handle.
`PathMissing` proves only that named path was absent from that validated parent at the
observation time. A disk receipt may report the bytes or facts actually observed, but
never generation membership, repository completeness, or repository-wide absence.
Path-only fallback is not this authority.

**Worktree scope observation** — One binding/root-bound, policy-versioned receipt for
a complete authoritative enumeration performed over a named scan interval. It proves
only that every path in the declared worktree scope was considered by that scan; it is
not a generation, a filesystem-wide atomic snapshot, or evidence beyond the interval.
Instability or incomplete traversal refuses the aggregate instead of returning a
partial complete-scan claim.

**Scope discovery obligation** — The generation proof that its canonical manifest
still accounts for every in-scope path under the declared scope and policy versions.
It has its own last-verified/next-due deadline and reserved full-enumeration resources,
so suppressed creation of a previously unknown path cannot leave `Current` forever.

**Proof-refresh publication** — An isolated, charged publication that updates an
immutable generation's verification ledger after unchanged rolling verification. It
retains content identity, advances publication identity, installs a successor safety
package, and commits only through the normal binding/cut/mutation fences. A mismatch
publishes non-current instead; proof ages are never mutated or side-published.

**Current query lease** — A request-scoped, owning, read-only, non-retargetable capture
of only the selected verified generation plus lightweight source/runtime identity.
It drops the project runtime snapshot after selection, pins the selected charged roots
until drop, creates no second charge, and carries no publication authority.

**Project query lease** — One project-slot acquisition over an explicit selected
source set. It captures the project runtime snapshot once, requires every selected
source to be `Current`, clones only those generation roots/evidence, then drops the
project snapshot. It retains a `SourceSelectionReceipt`; single-source acquisition is
its specialization.

**Source selection receipt** — The immutable proof of what a project query selected:
slot instance ID, project-runtime publication identity/epoch, session-binding
publication identity, selector, canonical selected source IDs, and protected
membership authority/session incarnation and revision. Global no-match/absence uses the sealed
`SelectedAggregate` constructor, which requires an exact receipt↔Current-generation
bijection. Dropping the project runtime snapshot never drops the selection proof.

**Resolved selection set** — The sealed refusal evidence for one captured authorized
multi-source or cross-project selection: canonical non-empty selection receipts plus
an exact bijection from every selected project/source to either its captured Current
generation-authority ID or its typed unavailability cause. At least one member is
unavailable. It preserves per-source evidence but cannot establish success, no-match,
or absence.

**Claim** — One public authority-bearing result: a value plus its `OperationReceipt`,
complete `ClaimProvenance`, optional `EvaluationProvenance`, and producing runtime/publication
identity. Selection-dependent claims carry selection inside provenance, never as an
uncoupled side field. Formatting, caching, persistence, CCR, and retrieval may encode
it but cannot discard or recompute the envelope.

**Source refusal** — The one sealed public refusal sum for lifecycle-backed
operations: unresolved `InvalidSelection`, pre-exposure `AdmissionUnavailable`, one
resolved `SourceUnavailable`, or authorized multi-source `SelectionUnavailable` with
a `ResolvedSelectionSet`. Each reason variant owns its only legal basis and retry
classes; basis/retry are not freely combinable fields. Authorized pre-slot root
failures may carry only the sealed non-authoritative admission-root attempt; bound
observation failures carry `BindingAuthority`, and neither can substitute for the
other. A generation-policy basis
attests only why disclosure was withheld, never observed bytes. Before authorization,
only a canonical request/selector hash may appear, so nonexistent and unauthorized
protected selectors have the same public shape/status/body and expose no slot, source,
runtime, or binding identity. It never encodes an empty success or an unowned retry.
After authorization, a repeated embedded open of the one canonical source-registration
key is `AdmissionUnavailable(SourceAlreadyOpen/OnEvent)` and conveys no handle or close
authority.

**Process index runtime** — The cloneable public factory wrapper. Each wrapper owns a
`FactoryOwnerToken`; the persistent process registry—not an owner Arc—holds the stable
capacity domain and `Vacant | Live | Retiring | Stopped` factory-incarnation state.
Embedded handles and managed work retain a separate `ProcessControlLease` to the
current `ProcessRuntimeInner`. The final token decrement atomically publishes
Retiring/inner Stopping before the owner becomes undiscoverable, then transfers drain
to a non-self-joining finalizer. Construction joins Live or refuses/joins Retiring;
no successor installs before old destructive/publication drain is Stopped, and every
successor reuses the same capacity ledger. It is the only factory for embedded source
handles and server Adapters; publication-capable work cannot detach.
Revoked old-incarnation tokens retain only their terminal shutdown receipt/control;
they cannot register against or become a new incarnation.
`Clone` linearizes under the registry writer: a `Live` token creates one new counted
owner of the same incarnation, while a `Retiring`/`Stopped` token creates only an
uncounted terminal token bound to the same shutdown receipt. Token release exact-
matches its never-reused incarnation/token identity, so an old clone/drop cannot alter
a successor's owner count.

**Process runtime state** — The inner closed lifetime gate `Live | Stopping | Stopped`
paired with the persistent registry's `Vacant | Live | Retiring | Stopped` incarnation
state. Embedded-source open, server-Adapter creation, and child-partition minting
must register under `Live` before exposure. Shutdown performs one preallocated
registry Live→Retiring plus inner Live→Stopping transition before enumerating registrations; racing creation is either
registered-and-drained or refuses/refunds. Inner `Stopped` and its shutdown receipt are
terminal/idempotent; the persistent registry may later install a new incomparable
incarnation only on the same capacity domain.

**V11 snapshot namespace** — The disjoint `.symforge/v11/` persistence authority.
V11 never overwrites, renames, deletes, or restores the v10 `.symforge/index.bin`
path. A legacy snapshot may be read only as a bounded untrusted seed through one
stable opened-file object; otherwise v11 rebuilds from source. V11 verification and
activation live wholly in its namespace, so an unmodified concurrent v10 writer
cannot corrupt migration or rollback.

**Embedded source handle** — A non-cloneable v11 Adapter returned only by
fallible `ProcessIndexRuntime::open_embedded_source(&self, spec) ->
Result<EmbeddedSourceHandle, SourceRefusal>`. It owns exactly one source registration
but no executor, capacity pool, raw `LiveIndex`, or public factory owner; its
`ProcessControlLease` drives observation, polling, retries, deadlines, and
backpressure.
There is exactly one exposed handle owner for a canonical source-registration key.
Concurrent or repeated authorized opens of that key serialize at the pending/live
registry entry: one wins exposure and every other call returns typed
`SourceAlreadyOpen` without acquiring close authority. V1 never shares a close state
between independently returned handles. After the sole handle reaches its terminal
close receipt, a later open creates an incomparable source/binding incarnation.
The handle and process shutdown share one idempotent `SourceCloseState` and terminal
receipt. `begin_close()` synchronously Freezes only that source and returns a joinable
receipt; `receipt.wait(deadline)` is a separate operation and refuses self-wait from a
registration included in the drain. Drop performs the same begin transition, then
transfers the owning drain handle to the process reaper so worker-self-drop cannot
self-join and no revoked work detaches. If process shutdown already completed the
source, later queries refuse `AuthorityRevoked` and later close/Drop only observes the
terminal receipt and releases locally; it never enqueues into a stopped reaper.
Process shutdown drains all sources/reapers and owns all executors. Queries return
`Claim<T>` or `SourceRefusal`.

**Claim context** — An operation-specific, identity-compatible capture used by mixed
authority tools. It binds selected generation leases, physical-root leases, and Git
repository/object identity once so a rebind cannot combine evidence from different
roots. Its outputs carry `ClaimProvenance`.

**Retarget authority** — A session incarnation, expected old binding revision,
proposal ID, and exact target slot/source/current identity. Retarget commit is a CAS
on the session binding revision; proposal membership is provisional and cannot answer
queries. A stale or superseded proposal can only release its provisional membership.

**Delta proof** — A lifecycle-sealed proof that a candidate is equivalent to a clean
full rebuild for the advertised `RequiredArtifactSet`: base generation, complete
scope certificate, artifact-set identity, scope/policy versions, closed cause set,
complete discovery diff, dependency-contract digest, impacted closure, and semantic
digests of every reused artifact. The completeness certificate commits its digest.
Unknown/global causes or dependencies make the scope dirty and force a full candidate.
Path hints never mint this proof.

**Strict-current scope contract** — The versioned, closed set of source-derived
facts that must be terminal and complete before a generation can promote. An
advertised surface cannot silently accept truncated, unreadable, partially parsed,
or unknown coverage.

**Source mutation permit** — A tracked, non-cloneable destructive authority granted
only from an exact `Current` generation and owning that binding's physical-root lease.
Grant advances the mutation epoch and publishes `Refreshing` with a closed
`Granted` permit record. Before the first side effect, `start_side_effect` performs
fallible path preparation, then under the project writer exact-validates Live/source/
permit/binding/root authority and stores `Granted -> InFlight`; only that InFlight
authority may write. Freeze before the mark refuses; Freeze after it drains the permit.
At its writer cut, Freeze changes every still-Granted permit only to
`RevokedSealPending`, so later start refuses while its drain registration remains live.
After releasing the writer, the supervisor seals its allocation grant, waits any
construction guards, refunds/transfers exactly once, then converts it to revoked
deallocation-only authority and releases the registration; its root/capacity charge
remains until the handle actually drops.
`commit(WriteReceipt)` schedules candidate work;
`rollback(NoSideEffectProof)` schedules a cheaper no-op verification candidate but
never directly restores the prior state. Every granted permit terminal path that can
make the **same live binding** `Current` again requires a fresh candidate publication
at the latest observer cut. A `Stopping` revocation may seal/unregister the unstarted
predecessor permit without building a candidate that can never publish; the successor
binding still requires its own complete fresh candidate. Drop, panic, or any side
effect fails closed and requires verified candidate promotion. Close/rebind freezes
new permits and drains every publication-capable/
InFlight permit before successor authority can install. Revoked Granted
deallocation-only handles may outlive the tombstone but cannot start, write, or publish.

**Last-known-good generation** — The previously verified generation retained
unchanged while replacement work runs or fails. It is not silently relabeled
`Current`.

**Source index lifecycle module** — The lifecycle-owning module behind each source
slot. Its deep Interface owns single-flight loading, invalidation, candidate
construction, tracked mutations, retries, promotion, and health projection. Project
membership/coherent selected-source acquisition remains in the project registry
Module; resource admission and oldest-satisfiable scheduling remain in the
process-wide capacity-pool Module; claim composition and ranking are separate
Adapters above those source-truth Interfaces.
