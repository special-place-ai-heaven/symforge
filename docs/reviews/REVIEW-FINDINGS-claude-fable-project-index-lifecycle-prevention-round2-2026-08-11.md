# Review Findings — Project Index Lifecycle Prevention, Round 2

**Target:** `67752662e1ee20d597bfeb5e01659d40bcc36e6c`  
**Target tree:** `3aa8d4ee1601909d65343494af486d1e896bbdd6`  
**Design SHA-256:** `5B1E3FE608BCE14B1F1AF630B147E9D6FA49585EF91DD2997AE373CF85CC1D3F`  
**Identity verified:** yes  
**Verdict:** CLEAR

Identity evidence: commit exists; `rev-parse "<target>^{tree}"` =
`3aa8d4ee1601909d65343494af486d1e896bbdd6`; `CONTEXT.md` blob
`af8c1a6a04f3511b217fe51e6295dacb7cf7f657`; design blob
`576a35d3d11c7d4cfba80b882d3ae99a7f57d85a`; binary-diff blob (byte-preserving
`cmd.exe` pipeline) `64f898f71885e174175395a79d96565ca6e3f286`; SHA-256 of both
exact target blobs (extracted via `git cat-file blob`, hashed with
`Get-FileHash`) match the packet values; `git diff --check` clean. The base→target
diff touches only `CONTEXT.md` and three `docs/` files — the source tree at the
target is identical to `main` at `1521abb0`, so read-only source inspection against
the working tree is inspection of the review target's source.

**No P0, P1, or P2 finding remains.** Four independent internal lanes reported the
same; I attempted to falsify that result across every required question and could
not construct a surviving counterexample. Four optional P3 notes follow the answers.

Load-bearing invariants independently re-tested (attacked, not merely read):

- the closed `SourceRuntimeState` with `Current` as the sole strict-queryable
  variant and no independently combinable side fields (design §5, Invariant 1);
- the four-lane `AtomicAuthority` model with generation-owned/digest-identical
  bytes, path-local disk observation, complete worktree-scope receipts, and the
  exact-bijection `SelectedAggregate` (§5.2, Invariants 11, 12);
- the accumulator-to-writer linearization domain with the runtime-root store as
  sole commit point, staged `IngressEnvelope` ownership, and the
  Current-store-before-side-facts rule (§7, Invariant 12);
- precharged safety/handoff/revocation packages making every
  Current→non-Current safety publication independent of ordinary capacity
  (Invariants 18, 22; §5 package definitions);
- `Freeze → Drain → Install` with supervisor handoff for every self-drain trigger
  (§7, Invariants 4, 21);
- reservation→construction-guard→charge conservation, `GrantSealDriver`
  winning-mode CAS, cleanup-before-requeue resize, and `DetachedChargeOwner`
  (§8, Invariants 23–25);
- the process factory-incarnation registry with the stable capacity domain and
  counted/terminal owner tokens (§8, Invariant 28);
- the single-exposed-owner embedded contract and idempotent close/shutdown
  receipts with `WouldSelfWait` (§9.3, Invariant 26);
- the `LegacyOpen → LegacyClosing → PreventiveV1Open` activation registration gate
  (§9.2, Invariant 27);
- the v11 disjoint persistence namespace and opened-object v10 seed rule (§9.3);
- the externally anchored refreeze approval record and closed public-API-v11
  allowlist (§3).

## 1. Round-1 blockers re-tested (required questions 1–5)

**Q1 — misattributed authority (round-1 F1).** Closed. The F1 lane is now
explicitly routed: generation reads must serve generation-owned bytes or bytes
whose complete digest equals the generation's recorded admitted-byte identity;
terminal/no-identity/absent paths return a typed generation refusal or
`NotInGeneration`, never silent disk fallback (§5.2 generation-authority rules).
`src/protocol/read_gate.rs` is named as the initial Adapter seam to be split
(§5.2), Slice 3 names the split explicitly, and the Slice 0 oracle list contains
the exact F1 failpoint ("Capture a strict generation lease, replace disk bytes
before content resolution…"). I verified in source that `admit_disk_read`
(`src/protocol/read_gate.rs:92-178`) still performs security-only classification
of freshly read bytes — the defect is real today and the design now names and
routes it. Digest-equality substitution is sound even under write-then-restore
races: equal digest means byte-equivalent content. Ranking order/scores cannot be
misattributed because `EvaluationProvenance` is a separate preserved envelope that
"cannot establish source truth" and cache/CCR keys must carry it (§5.1, §5.2).
Mixed lanes acquire one identity-compatible `ClaimContext`; a rebind between
acquisitions refuses, and the no-trailing-live-check rule prevents the inverse
failure of discarding a complete old-root response (§5.2). I could not construct a
lane that attributes disk, worktree, Git, or ranking evidence to a generation.

**Q2 — false Current preserved indefinitely (round-1 F2).** Closed as bounded,
honestly. Same-size/same-stamp edits: `EntryVerificationObligation::ByteIdentity`
plus racy-clean promotion rechecks ("The promotion barrier rechecks every
obligation whose stamp may collide with that window; unknown platform timestamp
granularity is racy", §7). Suppressed create/rename/delete and policy changes:
the `ScopeDiscoveryObligation` with its own deadline (§5.1). Terminal-policy
reclassification: `ContentDisposition` obligations stable-read and reclassify
without persisting sensitive digests (§5.1). Starved verifier: strict acquisition
checks scope and entry deadlines and synchronously publishes non-Current through
the armed safety guard on overdue — the fail-closed direction — and the release
configuration must prove the verifier budget meets a finite full-coverage bound,
"it cannot ship as 'best effort'" (§7). Proof renewal is a fenced immutable
`ProofRefreshCandidate` committed under the normal fence, so a stale fence
discards it and mismatch publishes non-Current. Continuous delta streams keep the
source non-Current (refusal, not false Current); the attempt budget cannot
convert an actively edited source into `Blocked` because a source change starts a
new attempt series (§10). Staleness inside the rolling deadline window remains,
and the design discloses exactly that ("bounded eventual detection", §7). No
indefinite false Current survives.

**Q3 — close/rebind/shutdown deadlock (round-1 F3).** Closed. The lock inventory
is: per-source accumulator → project publication writer (only legal lifecycle
nesting); process-registry writer for factory gates and pending install/cancel;
pool lock never nesting either lifecycle lock or the registry writer (§7, §8,
Invariant 20). Freeze is writer-only and fills the root-agnostic precharged
package from the latest map with no retry loop, so continuous unrelated
publications cannot starve it (§7 Freeze step 1). Drain holds at most one
accumulator, in canonical `SourceId` order, and never waits while holding a
lifecycle lock. Ingress holds accumulator and waits the writer; Freeze holds the
writer and never takes an accumulator — no cycle. Self-drain is excluded
structurally: every trigger (counter exhaustion inside a callback, root
replacement discovered in a watcher callback, `begin_close`/`begin_shutdown` from
managed work) closes ingress, hands a precharged control ticket to a supervisor
explicitly outside the predecessor Drain set, and terminalizes first; `wait` on a
receipt from inside the drain set returns `WouldSelfWait` (§7, §9.3,
Invariant 21). Final-drops and callbacks run after unlock. `GrantSealDriver`
cannot wait on a caller-held construction guard because the caller must commit,
drop, or transfer it first (Invariant 25). I traced writer↔accumulator,
writer↔pool, registry↔project-writer, drain↔reaper, drain↔executor-join, and
drop-time orders and found no reversed edge.

**Q4 — starvation and ledger over-credit (round-1 F4).** Closed. Oldest-
satisfiable scheduling with bounded-bypass drain barriers, pin-aware
`PinnedResidency` parking with barrier arming at final pin release, disjoint-key
progress, and arithmetic `InsufficientReplacementHeadroom` refusal (§8 admission
rules) — a feasible request either schedules, receives a barrier at the bypass
bound, or is proven infeasible and `Blocked` with an action. Leaked leases
surface `Stalled` evidence rather than silently convoying (F4's exact fix).
Conservation: units exist only as available-reserved, in-construction (guard),
or charged, with each transition atomic; abort final-drops physical groups before
returning the debit; sealing waits all guards then refunds exactly once; resize
drops every candidate-private charge before the single pool transaction consumes
the one retained reservation token; deallocation-winner-over-resize takes the one
explicit losing edge and "cannot enqueue a successor or refund again"; late
grants and tombstoned requeues refund exactly once against never-reused
identities (§8, Invariants 8, 23–25). I could not construct a double-refund,
lost-charge, or wait-while-holding-partial-candidate path; the with-budget-10
two-worker oracle in Slice 0 pins the classic deadlock.

**Q5 — shipped refusal-per-edit (round-1 F5).** Closed. Slice 4 is explicitly
indivisible: candidate activation, all writer/observer invalidation, structurally
shared delta refresh, capacity coexistence, and `ObservedRefreshGateV1` are "one
indivisible public enablement release"; the full-rebuild implementation exists
only behind a dark feature gate; the gate pins baseline commit, corpora,
workloads, completion oracles, delta-vs-full equivalence, p95 ≤ 2 s / max ≤ 5 s /
≤ 1.25× baseline, and the retained-plus-candidate memory bound in the same cut
(§11 Slice 4). "No default-on intermediate may turn every edit into a
full-repository rebuild and strict refusal" is now a stated release constraint
with an activation oracle.

## 2. Answers to the required adversarial questions 6–21

**Q6 — is `Current` unconstructible without complete proof; is partial capability
renamed?** Yes, and no. `Current` exists only via promotion, which requires the
complete closed `StrictScopeContractV1`, physical-root continuity,
`ObserverCoverage::Complete` on a post-barrier `Active` token, unchanged cut,
unchanged mutation epoch, zero permits, and precharged successor packages
(Invariant 6; §7). Every "partial" concept is genuinely non-queryable:
`Refreshing`/`Blocked` retain internal-only material, capability certificates
"attest that the closed set above is complete" and cannot authorize partial
promotion in V1 (§5.1), a reduced deployment must omit the surface from protocol
advertisement before lifecycle start rather than promote without it, and
`Unavailable` external evidence is legal only as a versioned contract member
treated as no-evidence. Renaming is structurally excluded because queryability is
the enum variant, not a predicate over side fields.

**Q7 — pending admission and slot exposure.** Two first opens single-flight on
the pending record; joiners exact-compare fresh physical-root identity against
the pinned lease; a replacement directory refuses/join-cancels and requires a
distinct successor identity; install revalidates process Live and exact-compares
a fresh confined handle against the held identity without replacing the lease;
one atomic capacity transaction charges the complete fixed base and every
revocation package before the CAS; a late grant refunds and terminalizes against
the cancelled never-reused admission identity (§5, §7, §8, Invariant 4). ABA
(replace-then-restore) is benign: the pinned lease holds the original object,
which is again the live object. Consequently "a `ProjectSlot` is never observable
without the capacity needed to revoke it or with a path-recaptured/different
physical root" holds.

**Q8 — event loss.** Excluded at every boundary I tried. Registration precedes
S0; the `IngressEnvelope` owns event plus `DrainRegistration` outside the unwind
boundary, so a processing panic cannot consume delivery; pre-commit failure
changes nothing published and only the outer owner may retry or consume the
precharged Gap; post-store failure forces Gap rather than silent loss; `observe`
cannot return having silently dropped the envelope (§7). Pruning is post-store,
idempotent, exact-token-matched, monotonic, preserves Gap and newer hints, and
preserves a newer `scope_dirty` marker (§7). Old-token deliveries after handoff
terminalize as stale against the closed predecessor; the stable-`ObserverToken`
exception makes a queued pre-promotion event delivered after G0→G1 advance the
cut and de-current G1 rather than be rejected (§4, §7). Activation: the
Active(T1) store precedes OS open, which precedes the logically gated
revalidation, which precedes the full authoritative baseline — changes before
ingress opens are covered by the baseline, changes after advance T1, and a
counter at exhaustion latches Gap and retires the observer (§7). Overflow,
disconnect, eviction, and unknown ordering all latch a constant-size Gap or
`scope_dirty` marker whose coalescing "cannot forget an earlier proof
obligation."

**Q9 — mutation permits.** Writes are handle-relative beneath the held
`PhysicalRootLease` with no-follow/reparse-safe per-component resolution ending
at a validated final-parent handle; platforms without equivalent containment
refuse; there is no path-only destructive fallback (§7, CONTEXT). Freeze before
the under-writer `Granted → InFlight` store marks `RevokedSealPending` so start
deterministically refuses; after the store, the InFlight permit is drained.
Rollback cannot resurrect Current: every terminal path that can return the same
live binding to Current — including `NoSideEffectProof` — goes through a fenced
no-op verification candidate at the latest cut and monotonic mutation epoch with
a fresh safety package (§4, §7, Invariant 19). Successor authority cannot install
while any publication-capable/InFlight permit remains in the predecessor Drain
set; revoked deallocation-only handles may outlive the tombstone but "cannot
start, write, or publish."

**Q10 — torn membership and session veto.** One `ProjectRuntimePublication` load
yields sources and `ProjectMembershipPublication` coherently; membership CASes
commit project-root deltas under the same writer, so selection evidence is wholly
pre- or post-CAS (§5). Session capture → project load → exact-identity session
revalidation (or one registry read guard) closes the session-side tear; retarget
and additive/removal changes are each one CAS of the immutable never-reused
`SessionBindingPublication` (§4). The veto direction is closed by construction:
`commit_source_delta` "never requires a `SessionBindingPublication` for observer,
proof-expiry, capacity, mutation-terminal, or promotion work" — session churn
cannot block a required safety publication, and the Slice 0 oracle races every
such delta against retarget/add/remove/reconnect.

**Q11 — prepared delta vs newer siblings, ABA, representation leak.** Under the
writer the registry loads the latest whole Live publication, Arc-identity-compares
only the fields it intends to change, fills preallocated path nodes without an
allocator call, preserves every untouched latest sibling, and mints a new
publication identity — a delta prepared before an unrelated CAS rebases rather
than restoring an old sibling (§5). Field-equal ABA is excluded because the
`SourcePublicationToken` exact-matches one retained `Arc`, not field values, and
epoch arithmetic is checked with a reserved terminal value (§5; Slice 0
field-equal-cycle oracle). Representation cannot leak: lifecycle code cannot name
`SourceStatePublication` or a runtime root, registry code cannot decide a
lifecycle transition, and static visibility/consumer tests enforce both (§4).

**Q12 — embedded ownership.** One exposed handle owner per canonical
`SourceRegistrationKey`; losers get `AdmissionUnavailable(SourceAlreadyOpen)`
with no handle, no close receipt, no binding token (§9.3, Invariant 26). Handle
close and process shutdown join one idempotent `SourceCloseState`/receipt; Drop
transfers the drain to the reaper; a Drop on a managed worker reaches its
revoked/no-publication terminal before the reaper joins it — no self-join. Two
live runtimes are excluded by the persistent factory registry: the final counted
token decrement atomically publishes Retiring/Stopping before the owner becomes
undiscoverable; terminal-state clones are uncounted receipt tokens; a successor
installs only after inner `Stopped` and reuses the same stable capacity domain
(§8, Invariant 28). Post-shutdown handle queries refuse `AuthorityRevoked` and
late close observes the terminal receipt locally, never enqueueing into a stopped
reaper.

**Q13 — `GrantSealDriver` orphaning.** The driver/control obligation is installed
in the `WorkLineage` before any seal-pending state is published and its ticket is
supervisor-claimable before the store, so the pending-before-CAS window is
representable but never ownerless (Invariant 25). The close cell is one monotonic
CAS; losers join the winner's mode via the single explicit losing edge; the
resize loser in a deallocation win "neither owns nor fabricates a resize-sealed
reservation" and cannot refund again; requeue-after-tombstone is excluded because
the one pool transaction checks all ancestor scopes and consumes the retained
grant exactly once; no driver waits on a caller-held guard (§8). The Slice 0
failpoint list covers every claimed window, including panic on both sides of the
CAS with Freeze racing.

**Q14 — `ConvertedToDeallocationOnly`.** Conversion is reachable only after
grant seal (no new guards, all admitted guards settled, exact-once refund of
available units) and strips every callback, enqueue, resize, and publication
capability while transferring all committed charges plus the domain `Arc` into an
independent `DetachedChargeOwner`; the ledger stays debited until actual final
drop and the detached owner "can never requeue or call lifecycle code" (§8,
Invariant 23). An executing closure cannot outlive process `Stopped`: it may
leave a narrower publication drain only after result adoption is revoked, but its
`ExecutorRunRegistration` stays in the process incarnation's join set until exit,
and shutdown cannot store `Stopped` before that (§8, §9.3). Only completed
escaped results, charged allocations, and revoked non-executing handles survive —
which is the honest residual, and the hard-hung-task limit is disclosed rather
than papered over.

**Q15 — activation cut.** Every legacy lane (query execution, cache get/put, CCR
lookup/store/`symforge_retrieve`, response finalization) must hold a
non-cloneable `LegacyResponseRegistration` from the exact mode-epoch gate through
materialization; activation closes registration, drains admitted operations
(including late cache puts and retrieval rendering), invalidates v10 cache/CCR
state, installs the successor schema/epoch, and only then opens PreventiveV1 —
no unregistered straddler exists by construction (§9.2, Invariant 27). Restarted
v10 records are untrusted misses that cannot be upgraded; new keys carry the
complete claim envelope including producing publication identity and mode epoch,
so recycled-counter replay fails. Release gates race every boundary.

**Q16 — sealed V11 interface.** The allowlist is closed and generated: a declared
supported cfg/target/feature domain with unknown configurations rejected; merged
rustdoc graphs plus an all-cfg HIR inventory cross-check; a checked completeness
fixture containing target-only, negative-cfg, trait-impl, associated-item,
auto-trait, and macro exports; external dependent-crate positive and compile-fail
suites; explicit negatives for authority-minting `Deserialize`/`Default`/`From`,
raw-internal `Deref`/`AsRef`/`Borrow`, and `Clone` on `EmbeddedSourceHandle`;
prose categories cannot add an export (§3, §9.2, release gates). Untrusted
deserialization yields DTOs that must be revalidated — bytes cannot mint
authority. `from_indexed_files` is demoted to untrusted candidate material that
cannot attest anything (§9.2). I could not name a bypass class the manifest
scheme fails to enumerate.

**Q17 — v10/v11 migration.** V11 writes only beneath `.symforge/v11/` and never
touches the v10 path; seeding opens the actual legacy file object with
stability-preserving semantics, streams bounded bytes under one digest, archives
by digest, and falls back to source rebuild when the opened-object guarantee is
unavailable — pathname pre/post checks are explicitly rejected (§9.3). The v11
lease coordinates v11 writers only; the design correctly refuses to pretend an
unmodified v10 writer honors a new lock. Rollback starts v10 against the
untouched legacy namespace, is idempotent, and in-place restore requires proven
legacy-writer quiescence or refuses. The pinned unmodified-v10 concurrent-writer
campaign races every failpoint. False attestation is excluded because v11 proof
"is never interpreted as v10 authority" and the archive binds the opened digest.

**Q18 — refreeze self-approval.** The checked-in manifest/attestation is
internally hash-complete but explicitly "not its own trust anchor": activation
accepts only a commit covered by the signed, append-only
`RefreezeApprovalRecordV11` held outside the mutable tree, and the release gate
includes the mandatory negative (coordinated in-tree rewrite retaining the old
record must fail) (§3, §13). This is implementable in release CI with any signed
external store plus a verifier that checks commit/tree/digest/identity; the
design demands exactly that and nothing exotic. The amendment-set ID is a
recomputed domain-separated digest, not an operator label. The trust boundary —
repo-write attacker cannot approve; release-identity holder can — is stated
honestly.

**Q19 — deep Modules; smaller Interface?** The registry (identity, membership,
coherent acquisition), source lifecycle (five operations plus the three-operation
permit), capacity pool (admission/accounting), process ownership (factory
registry), claim composition (one private envelope builder over
`OperationContractV1`), and ranking (one snapshot Adapter) each hide a real
ownership rule, pass the deletion test, and expose narrow Interfaces relative to
their Implementation depth. The one cross-Module publication seam
(`SourcePublicationToken` / sealed intent / opaque `PreparedRuntimeDelta`) is the
minimal shape I can construct that lets the registry own representation while the
lifecycle owns policy; collapsing it either lets lifecycle name runtime roots or
lets the registry decide transitions — both are the defect classes under repair.
I could not name a smaller Interface preserving the same safety and Locality.
The design's honest cost is Implementation complexity, and §13 keeps verification
tractable by refusing a combined state space.

**Q20 — liveness honesty.** All liveness statements are conditional (quiescent
readable source, replacement headroom, bounded task completion, fair scheduling —
§13) and every waiting state I enumerated names an independent event:
`WaitingForCapacity{blocker, retry_trigger}` (release/reconsideration),
`RetryWait{retry_at}` (timer), `ObserverFree{retry_trigger}` (capacity release),
`Blocked{operator_action}` (operator), `PinnedResidency` (actual pin release,
with `Stalled` age evidence), attempt-budget exhaustion (source/config/operator
triggers, with pure capacity waits consuming no attempt). Hard-hung blocking work
is disclosed as unrecoverable in-process. The two residual soft spots are
diagnostic-only and recorded as P3-2 and P3-3 below.

**Q21 — a concrete interleaving the design still fails.** I could not produce
one. The strongest candidates I constructed, and why each is excluded:
(a) round-1 F1 replay — disk bytes under a strict lease — excluded by
digest-identity substitution and typed refusal (§5.2);
(b) double-consumption of the single `SafetyTransitionPackage` by concurrent
ingress, overdue acquisition, and mutation grant — excluded because all three
serialize under accumulator→writer, the first consumer stores non-Current, and
each later consumer revalidates exact state and degrades to its non-Current
behavior;
(c) promotion racing a delivered-late old-token event — excluded by the stable
token making the event advance the cut and de-current the new generation;
(d) prepared-delta clobber of a newer membership CAS — excluded by
latest-root rebase plus Arc-identity compare;
(e) same-path directory replacement joining old authority — excluded by pinned
lease identity compare at join and install, and `PhysicalRootReplacement`
routing;
(f) resize/deallocation double-refund with Freeze racing the close-cell CAS —
excluded by the winning-mode driver and single losing edge;
(g) final-wrapper Drop racing a default constructor — excluded because
Retiring precedes owner undiscoverability under the registry writer;
(h) legacy cache put straddling activation — excluded by the retained
registration and drain-before-invalidation;
(i) an actively edited source being driven to `Blocked` by attempt exhaustion —
excluded because a source change starts a new attempt series.
The closed-state/promotion/query model plus the precharged-package rule excluded
every interleaving I tried.

## 3. Feature 020 refreeze completeness

The §3 conflict inventory names the affected user stories, FRs, NFRs, SCs, and
plan phases; the nineteen amendments cover them; FR-008/FR-010/FR-012/FR-049/
SC-003 preservation is stated; SC-019 is retained as load-bearing with the
memory-only/user-local placement rule; and the manifest scheme (complete
classification, single declared exclusion, detached attestation, external
approval record, amendment-set digest) closes the inventory mechanically rather
than by prose. I checked each amendment against the other authorities named in
the packet and found no contradiction. Round-1's P3 notes (vacuous scenario arms,
`Degraded` spelling inversion, Slice 3 single-publication migration) are all
absorbed: superseded-clause classification, the compatibility-projection row in
§9.2, and Slice 3's `PublishedSourceSet` consolidation plus the Slice 0
hybrid-read positive control respectively.

## 4. P3 notes (optional, non-blocking)

**P3-1 — Delta-reuse soundness is conditional on the dependency contract; say
so.** §7 states the delta lane "proves the result" via authoritative scope/byte
verification plus the sealed `DeltaProof`. For reused derived shards, the proof
is conditional on the correctness of the checked-in
`artifact_dependency_contract`: a missed dependency edge would let a stale reused
shard promote with a valid certificate. The design already carries the right
mitigations — unknown/global causes force full candidates, the contract digest is
sealed into the proof, and the acceptance oracle requires delta-vs-clean-full
equivalence over every edit class **and randomized mutation sequences** — but the
guarantee's honest form is "complete relative to the versioned dependency
contract, whose adequacy is release-gated." One sentence in §5.1 or §7
acknowledging that conditionality would align the text with the reporting
invariant; no mechanism change is needed, since runtime shadow rebuilds are the
only stronger alternative and are rightly out of scope.

**P3-2 — `SourceRegistrationKey` serialization wording.** §9.3 derives the key
from project/source identity, source kind, **and held physical-root identity**,
and says pending admission and live registration "serialize on that key." Read
literally, two authorized opens of the same canonical source whose captured root
objects differ (same-path directory replacement mid-race) have distinct keys and
would not serialize with each other. Invariant 4 ("exactly one … registry entry
per canonical identity") and the §5 pending-admission join rule (exact physical
identity compare inside the one canonical entry, replacement → refuse/cancel)
give the correct behavior; the §9.3 sentence should state that serialization is
per canonical identity with the physical-root component distinguishing
incarnations, not partitioning the serialization domain. The Slice 0 duplicate-
open oracle only covers the same-key race; the different-root-object same-path
race is covered by the pending-admission oracles, so this is a wording-precision
note, not a hole.

**P3-3 — diagnostic work-state staleness under exhausted ordinary capacity.**
Only safety transitions (invalidation, mutation intent, proof expiry, Gap,
revocation) are precharged. A diagnostic-only transition such as
`Building → RetryWait` inside `NonCurrentWork` requires an ordinary charged
runtime publication and can therefore lag arbitrarily under capacity exhaustion,
leaving health reporting a stale phase while the source correctly refuses. No
safety property is affected (the source is already non-Current), but a sentence
acknowledging that work-phase evidence may lag under capacity pressure — and that
`capture_source_view` consumers must not treat phase as fresh — would keep the
health surface inside the reporting invariant.

**P3-4 — `capture_source_view` retry-on-drift is unbounded.** The composite
health capture retries on publication drift while holding the accumulator
briefly per attempt. Under continuous ingress/publication churn on a hot source,
capture can livelock. Diagnostics-only; a bounded retry count returning explicit
"publication churn" evidence instead of blocking would be strictly more honest
than an unbounded loop.

## 5. Verdict rationale

Every round-1 blocker is closed by named machinery with a Slice 0 oracle pinning
its exact failpoint; the §2 causal claims are accurate against source at the
target (re-verified for §2.2, §2.3, §2.5, §2.9, §2.11, §2.12 in addition to
round 1's four); the amendment inventory is complete and externally anchored; the
Modules are genuinely deep with one minimal cross-Module seam; liveness claims
are conditional and honest; and no adversarial interleaving I constructed
survives the closed-state, precharged-package, exact-identity, and
single-commit-point rules. The four P3 notes are wording/diagnostic refinements
that do not weaken any load-bearing guarantee. No P0/P1/P2 remains: **CLEAR**.
