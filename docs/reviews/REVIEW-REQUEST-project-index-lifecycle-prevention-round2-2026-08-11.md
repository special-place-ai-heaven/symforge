# External Review Request — Project Index Lifecycle Prevention, Round 2

**Date:** 2026-08-11  
**Requested reviewer:** Claude Fable  
**Review type:** adversarial architecture closure review; design only  
**Expected verdict:** `CLEAR` or `BLOCK`  

## 1. Immutable review target — verify before reading

Review the committed target, never the ambient working tree:

- Base commit: `1521abb0197dac16e046a2b0b20a66a70c3a909b`
- Target commit: `67752662e1ee20d597bfeb5e01659d40bcc36e6c`
- Target tree: `3aa8d4ee1601909d65343494af486d1e896bbdd6`
- `CONTEXT.md` Git blob: `af8c1a6a04f3511b217fe51e6295dacb7cf7f657`
- Design Git blob: `576a35d3d11c7d4cfba80b882d3ae99a7f57d85a`
- `CONTEXT.md` SHA-256:
  `09999C3943C3B29AFCA037AE409CF3873BF0A0F07DA2D25A9433AA1C5689448E`
- Design-file SHA-256:
  `5B1E3FE608BCE14B1F1AF630B147E9D6FA49585EF91DD2997AE373CF85CC1D3F`
- Git blob hash of `git diff --binary <base> <target>`:
  `64f898f71885e174175395a79d96565ca6e3f286`

The commit containing this request will necessarily be newer than the target. That is
expected. The design under review is exactly the target above.

Minimum identity checks:

```powershell
$base = '1521abb0197dac16e046a2b0b20a66a70c3a909b'
$target = '67752662e1ee20d597bfeb5e01659d40bcc36e6c'

git cat-file -e "$target^{commit}"
git rev-parse "$target^{tree}"
git rev-parse "${target}:CONTEXT.md"
git rev-parse "${target}:docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md"
cmd.exe /d /c "git diff --binary 1521abb0197dac16e046a2b0b20a66a70c3a909b 67752662e1ee20d597bfeb5e01659d40bcc36e6c | git hash-object --stdin"
git diff --check $base $target -- CONTEXT.md docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md
```

The `cmd.exe` line is intentionally byte-preserving; do not substitute the Windows
PowerShell 5.1 native pipeline, which rewrites the stream. PowerShell 7.4+ is also
byte-preserving. Verify both SHA-256 values directly from the exact target blobs using
a byte-preserving reader; do not create or modify a review worktree merely to hash them.
If any identity differs, stop and report target drift. Do not silently review a newer
working-tree state.

## 2. Files to read

Primary normative inputs:

- `docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md`
- `CONTEXT.md`

Round-1 evidence, preserved verbatim:

- `docs/reviews/REVIEW-FINDINGS-claude-fable-project-index-lifecycle-prevention-2026-08-11.md`

Frozen Feature 020 authority being deliberately superseded/refrozen:

- `specs/020-repository-knowledge-index/GOAL.md`
- `specs/020-repository-knowledge-index/spec.md`
- `specs/020-repository-knowledge-index/plan.md`
- `specs/020-repository-knowledge-index/data-model.md`
- `specs/020-repository-knowledge-index/tasks.md`
- `specs/020-repository-knowledge-index/quickstart.md`
- `specs/020-repository-knowledge-index/contracts/*.md`
- `specs/020-repository-knowledge-index/checklists/requirements.md`

Inspect implementation read-only where needed to validate causal claims and migration
feasibility. The most relevant seams are:

- `src/daemon.rs`
- `src/main.rs`
- `src/embed.rs`
- `src/watcher/mod.rs`
- `src/live_index/store.rs`
- `src/live_index/persist.rs`
- `src/live_index/single_file.rs`
- `src/live_index/health_view.rs`
- `src/live_index/frecency.rs`
- `src/protocol/read_gate.rs`
- `src/protocol/edit.rs`
- `src/protocol/mod.rs`
- `src/protocol/tools.rs`
- `src/sidecar/handlers.rs`

The old round-1 request is historical context, not the normative round-2 protocol.
Do not modify the round-1 findings.

## 3. Intended result

This is prevention, not another readiness predicate:

> SymForge may refuse or retain a private last-known-good generation under permanent
> external faults, but it never promotes, checkpoints, labels, or serves a partial,
> unrooted, incompletely observed, incompletely derived, or unverifiable generation as
> Current.

The design intentionally does **not** promise perpetual availability under permanent
permission, capacity, disk, allocator, source-stability, or hard-hung-task failures.
It promises that those failures cannot manufacture trusted currentness.

Load-bearing product decisions:

- there is no active `Degraded` generation;
- only a closed `Current` source variant can yield a strict query lease;
- retained last-known-good data is internal recovery material in V1;
- sidecar/hooks, absence/completeness claims, checkpoints, edits, and current-impact
  lanes remain strict-current;
- explicit Generation, path-local Disk, complete Worktree-scope, and immutable Git
  observations are different authorities and cannot be silently collapsed;
- candidate and restored-snapshot state is isolated until one complete atomic
  promotion;
- a failure changes lifecycle/work availability, never the retained verified bytes;
- the public activation cut includes observer invalidation, safe delta refresh,
  capacity headroom, legacy cache/CCR retirement, and all entry paths at once.

## 4. What changed after round 1

Treat every item below as a claim to attack, not evidence to trust.

### Round-1 F1 — live disk bytes attributed to a generation

- Every primitive source-truth fact now has one closed `AtomicAuthority`:
  `Generation`, `DiskObservation`, `WorktreeScopeObservation`, or `GitObservation`.
- Comparisons and n-ary derivations retain every source/selection input plus an
  `OperationReceipt`; ranking order/scores retain separate `EvaluationProvenance`.
- Generation reads use generation-owned or digest-identical bytes.
- Disk observations use beneath-confined pinned handles and cannot prove repository
  completeness or generation membership.
- Mixed operations capture one identity-compatible `ClaimContext`; there is no
  trailing live-state check after a complete capture.

### Round-1 F2 — stamp collision and permanent staleness

- Every strict manifest entry carries a `VerificationObligation`, including
  content-derived terminal dispositions.
- Each source also carries a root-scope discovery obligation for suppressed creates,
  deletes, renames, and scope/policy changes.
- Promotion discharges a complete proof; reserved rolling verification has monotonic
  deadlines and fair progress.
- Proof renewal is a fenced immutable `ProofRefreshCandidate`, never a mutable side
  ledger. Overdue strict acquisition synchronously fails closed.

### Round-1 F3 — multi-source close/rebind lock order

- Project Freeze publishes revocation under the project writer, releases it, then
  drains sources in canonical order while holding at most one accumulator.
- The only lifecycle nesting is accumulator then writer. Pool/registry/drain locks
  never nest into lifecycle callbacks or final drops.
- Triggering callbacks cannot synchronously drain a set containing themselves;
  revocation hands work to precharged control authority outside the predecessor set.

### Round-1 F4 — strict FIFO starvation and capacity incompleteness

- Capacity is one stable process domain with class vectors, oldest-satisfiable
  scheduling, bounded-bypass drain barriers, pin-aware parking, and arithmetic
  replacement-headroom refusal.
- Allocation begins only under `AllocationConstructionGuard`; reservation-to-charge
  conservation holds until actual final drop.
- Candidate resize drops every candidate-private allocation before all-at-once
  requeue. A retained `GrantSealDriver` makes resize/deallocation closing helpable
  across panic/cancellation, with an explicit winning/losing state transition and one
  refund.
- Active, candidate, retired query-pinned roots, structurally shared allocations,
  runtime-map nodes, observers, verification, snapshots, parser/blocking envelopes,
  query output, reapers, and escaped charges are included by ownership.

### Round-1 F5 — refusal-per-edit shipped before deltas

- Candidate activation, all writer/observer invalidation, structurally shared delta
  refresh, capacity coexistence, and `ObservedRefreshGateV1` are one indivisible public
  enablement release.
- No default-on intermediate may turn every edit into a full-repository rebuild and
  strict refusal.

### Additional closure work

- Project and source runtime states are closed enums; queryability is not reconstructed
  from side fields.
- Pending project admission exists before a slot and owns no query/source authority.
- Root, binding, observer, state-publication, session, request, work, and process
  incarnations are never reused or compared as split facts.
- Watcher ingress, mutation intent, promotion, and strict acquisition share one
  linearization domain. Observer gaps are latched and require a fresh observer epoch.
- Mutations require tracked non-cloneable permits and beneath-root handle-relative I/O.
- One persistent process registry owns the stable capacity domain across factory
  shutdown/recreation; terminal factory clones cannot attach to a successor.
- Embedded V1 permits one exposed handle owner per canonical source key; duplicate
  authorized opens refuse instead of sharing destructive close authority.
- The registry owns runtime representation. The lifecycle crosses it only through an
  opaque `SourcePublicationToken`, sealed transition intent, and opaque prepared delta.
  Lifecycle safety publications never depend on mutable session authority.
- Legacy queries, cache get/put, CCR lookup/store/retrieve, and response finalization
  register under one activation epoch and drain before invalidation/new-mode opening.
- V11 has an exact public Rust API graph allowlist across the supported cfg/target/
  feature domain, not a denylist of known exports.
- V11 persistence uses a disjoint namespace and treats a v10 snapshot as one bounded
  untrusted opened-object seed; an unmodified concurrent v10 writer cannot be made to
  honor a new lock.
- Feature 020 refreeze is internally hash-complete and externally anchored by a signed,
  append-only approval record over the exact commit/tree and attestation digest.

## 5. Required adversarial questions

Answer with exact design/source evidence. Prefer a concrete interleaving over a broad
concern.

### Re-test every round-1 blocker

1. Can any response still attribute live disk bytes, a worktree scan, Git evidence,
   ranking state, or a mixed derived fact to the wrong generation or source identity?
2. Can a same-size/same-stamp edit, suppressed create/rename/delete, terminal-policy
   reclassification, starved verifier, or continuous delta stream preserve false
   Current indefinitely?
3. Can close/rebind/shutdown deadlock through writer, accumulator, pool, registry,
   reaper, executor, callback, or final-drop order? Include self-drain traces.
4. Can oldest-satisfiable admission starve a feasible request, or can reservation,
   construction, resize, sealing, structural sharing, cancellation, or detached
   residency over-credit the process ledger?
5. Can any shipped/default slice expose full-rebuild-per-edit refusal before safe delta
   candidates and the latency/memory gate are enabled?

### Challenge the revised architecture

6. Is `Current` truly unconstructible without complete source, observer, byte,
   artifact, policy, scope, and capacity proof? Is partial capability merely renamed?
7. Can two first opens, a pending-open cancellation, a same-path directory replacement,
   or a late grant expose a slot/source without its fixed revocation packages and
   physical-root authority?
8. Can an event be lost before/after observer activation, final cut, promotion,
   overflow, disconnect, predecessor handoff, callback delay, or counter exhaustion?
9. Can a mutation permit write outside the held root, start after Freeze, resurrect
   Current through rollback, or let successor authority install while destructive work
   remains?
10. Can project/session/source membership be captured torn, or can unrelated session
    churn veto a required watcher/proof-expiry/mutation safety publication?
11. Can a prepared source delta overwrite newer project-root siblings, survive a
    field-equal ABA cycle, or leak private registry representation into lifecycle code?
12. Can duplicate embedded opens, handle Drop, explicit close, terminal factory Clone,
    final-owner Drop, process shutdown, and successor factory creation create two
    owners, two live runtimes, or a self-join?
13. Can `GrantSealDriver` become ownerless at any pending/CAS/guard/cleanup/proof/pool
    failpoint? Can resize/deallocation losers double-refund, requeue after a tombstone,
    or wait on a construction guard owned by the triggering caller?
14. Does `ConvertedToDeallocationOnly` strip every callback, queue, resize, publication,
    and destructive capability while preserving physical capacity charge until final
    drop? Can an executing closure outlive process `Stopped`?
15. Can a legacy cache get/put, CCR retrieve/store, or response finalizer cross the
    activation cut and recreate v10 authority after invalidation?
16. Can any public Rust export, trait implementation, auto trait, associated item,
    macro, target-only cfg, deep re-export, raw constructor, or untrusted deserializer
    bypass the sealed V11 Interface?
17. Can v11 migration corrupt, overwrite, or falsely attest v10 state while an
    unmodified v10 process writes concurrently? Is rollback honest and idempotent?
18. Can a coordinated in-tree refreeze rewrite approve itself without the external
    signed approval record? Is the claimed trust boundary implementable in release CI?
19. Are registry, lifecycle, capacity, claim composition, process ownership, and
    ranking genuinely deep Modules with local invariants? Name a smaller Interface if
    it preserves the same safety and locality.
20. Are all liveness statements conditional and honest? Identify any Waiting/Retrying/
    self-healing state with no independent event capable of changing it.
21. Give at least one concrete adversarial interleaving the design still fails—or state
    why the closed-state/promotion/query model excludes every interleaving you tried.

## 6. Review protocol

- This is a design review, not an implementation task.
- Do not modify source, design, context, Feature 020, or round-1 evidence.
- Do not run Cargo tests, Clippy, release builds, or performance campaigns; the target
  is documentation-only. Read-only source inspection is encouraged.
- The only authorized repository-content write is the round-2 findings document below;
  do not create a detached review worktree or modify the target to perform identity
  checks.
- Do not trust the internal review result. Four independent internal lanes reported no
  P0/P1/P2 against the exact target; your job is to falsify that result.
- Ignore style-only preferences. Report concrete correctness, safety, security,
  liveness, migration, API, or feasibility findings only.
- Classify findings as `PROVEN`, `LIKELY`, or `SPECULATIVE`.
- `P0`, `P1`, or `P2` blocks. `P3` is optional and non-blocking.
- For every blocking finding include:
  - exact severity and confidence;
  - exact design/source location;
  - violated invariant;
  - concrete trace or counterexample;
  - smallest sufficient design correction;
  - regression/model oracle that fails before the correction.
- Do not block merely because the design deliberately supersedes Feature 020. Block if
  the refreeze inventory is incomplete, an amended contract contradicts another
  authority, the design cannot be implemented safely, or a load-bearing guarantee is
  false.

Use the architecture terms precisely: **Module**, **Interface**, **Implementation**,
**Adapter**, **Seam**, **Depth**, **Leverage**, and **Locality**.

## 7. Output

Write only the verdict/findings artifact to:

`docs/reviews/REVIEW-FINDINGS-claude-fable-project-index-lifecycle-prevention-round2-2026-08-11.md`

Begin with:

```markdown
# Review Findings — Project Index Lifecycle Prevention, Round 2

**Target:** `67752662e1ee20d597bfeb5e01659d40bcc36e6c`  
**Target tree:** `3aa8d4ee1601909d65343494af486d1e896bbdd6`  
**Design SHA-256:** `5B1E3FE608BCE14B1F1AF630B147E9D6FA49585EF91DD2997AE373CF85CC1D3F`  
**Identity verified:** yes/no  
**Verdict:** CLEAR/BLOCK
```

If `CLEAR`, explicitly state that no P0/P1/P2 remains and briefly name the
load-bearing invariants independently re-tested. If `BLOCK`, order findings by
severity and stop after the actionable findings plus any optional P3 notes.
