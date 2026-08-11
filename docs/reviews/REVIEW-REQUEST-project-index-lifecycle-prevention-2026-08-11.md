# External Review Request — Project Index Lifecycle Prevention

**Date:** 2026-08-11  
**Requested reviewer:** Claude Fable  
**Review type:** adversarial architecture review; design only  
**Expected verdict:** `CLEAR` or `BLOCK`  

## 1. Review target — verify before reading

Review the immutable design commit, not the ambient working tree:

- Base commit: `1521abb0197dac16e046a2b0b20a66a70c3a909b`
- Target commit: `5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5`
- Target tree: `2e7eec650a9784b1c8c496747a3cd32ebccae806`
- Design Git blob: `ba33a94acdf61e119e618eb1d6fd441805676652`
- Context Git blob: `667b66c929b500daf3b4993f078e2717965be339`
- Design-file SHA-256:
  `a8f2e679965fddf8dadcc6d2eb12311de361a57f8c1a25a48e87681b8fdabad2`
- Git blob hash of the exact binary diff from base to target:
  `1cb38d3f65600d0002da9d5db263682496fd42e8`

The packet commit containing this request may be newer than the target. That is
expected. The architecture under review is exactly `base...target` above.

Before reviewing:

```powershell
git cat-file -e 5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5^{commit}
git rev-parse 5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5^{tree}
git rev-parse 5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5:docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md
git rev-parse 5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5:CONTEXT.md
git diff --check 1521abb0197dac16e046a2b0b20a66a70c3a909b...5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5
```

If any identity differs, stop and report target drift. Do not silently review a
newer working-tree state.

## 2. Files to read

Primary:

- `docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md`
- `CONTEXT.md`

Frozen contract being deliberately amended:

- `specs/020-repository-knowledge-index/spec.md`
- `specs/020-repository-knowledge-index/plan.md`

Inspect current implementation where needed to validate causal claims and migration
feasibility, especially:

- `src/daemon.rs`
- `src/main.rs`
- `src/watcher/mod.rs`
- `src/live_index/store.rs`
- `src/live_index/health_view.rs`
- `src/live_index/single_file.rs`
- `src/live_index/frecency.rs`
- `src/protocol/edit.rs`
- `src/protocol/mod.rs`
- `src/sidecar/handlers.rs`

## 3. Problem and intended guarantee

This is not a readiness-label patch. The current architecture can represent and
activate incomplete placeholders because lifecycle ownership is distributed across
startup, daemon registration, watcher reconciliation, snapshots, targeted refresh,
capacity checks, health, and several publication roots.

The design intentionally chooses prevention:

> A partial, unrooted, unverifiable, or incompletely derived generation is never
> promoted or presented as Current. Failure leaves the prior verified generation
> byte-for-byte unchanged and changes runtime work/availability state instead.

It does **not** claim SymForge can always remain Ready under permanent permission,
capacity, disk, source-stability, allocator, or hard-hung-task failures. Cold start
may refuse. During source-affecting work, the first lifecycle is strict-current and
public index reads refuse; retained last-known-good state is private recovery material.

## 4. Architecture under review

The design uses three concrete deep Modules:

1. Project registry — stable slots, protected membership, binding, tombstones, and
   per-source inventory.
2. Per-source index lifecycle — one owned candidate, observer journal, retry series,
   promotion, runtime snapshots, and strict query leases.
3. Process-wide capacity pool — reservations owned by the real allocations/tasks/
   immutable generations until final drop.

Load-bearing choices include:

- `active: Option<VerifiedGeneration>` is distinct from `WorkState`;
- no active `Degraded` generation exists;
- a closed `StrictScopeContractV1` must be complete before promotion;
- mutable frecency/time-decay evidence is captured separately in one immutable,
  versioned `QueryOperationalSnapshot` per request;
- candidates and restored snapshots are isolated and non-queryable;
- one binding token carries slot, project, source, physical-root, observer, and base
  generation identity;
- watcher events are bounded hints, not a durable log;
- `SourceJournal` then `ProjectSlot` publication writer is the normative lock order;
- a SymForge-owned source write publishes `Dirty` before its first disk side effect;
- promotion requires full candidate capacity, complete strict scopes, physical-root
  continuity, and a gap-free current-observer watermark;
- Slice 4 removes every in-place watcher/targeted publication lane; Slice 5 only
  optimizes safe full candidates into structurally shared deltas.

## 5. Required adversarial questions

Answer each with evidence. A concrete counterexample is more valuable than a general
concern.

1. Does this actually prevent partial/degraded promotion, or merely rename degraded
   coverage as `Blocked`, `Dirty`, `Unavailable`, or a completeness certificate?
2. Can any startup, reload, watcher, targeted refresh, structural edit, curation,
   snapshot restore/verify, Git temporal, bridge, authority, checkpoint, daemon,
   sidecar, hook, resource, prompt, standalone, local-stdio, or embed path bypass the
   lifecycle Interface and expose/mutate a candidate or retained non-current state?
3. Can a source write commit new bytes while a strict query/checkpoint still sees the
   old generation as Current? Audit intent, failure, rollback, and watcher-delay
   interleavings.
4. Can journal registration, cursor assignment, final replay, OS delivery delay,
   overflow, disconnect, handoff, or promotion lose a pre-cut event or acknowledge an
   event not applied to the candidate?
5. Is the `SourceJournal` → `ProjectSlot` publication-writer lock order sufficient and
   deadlock-free across append, Dirty publication, promotion, stop/rebind, and query?
6. Does one immutable binding token eliminate both the generation/root split-brain
   race and same-path physical-directory replacement? Can slot close/reopen cause ABA?
7. Is `StrictScopeContractV1` genuinely closed? Do terminal policy exclusions and
   optional `Unavailable` evidence preserve truth, or smuggle partial capability back
   into Current? Are FR-022, FR-031, FR-039, Phases 4/6/7/9, and SC-019 amended fully?
8. Can source-derived ranking and mutable operational frecency/time decay mix logical
   instants inside one response? Is the proposed operational snapshot deep enough to
   prevent SQLite/session/wall-clock recapture after query acquisition?
9. Is capacity accounting closed over base/scout/journal, active, candidate, retired
   query-pinned generations, caches, operational snapshots, snapshot/team-artifact
   representations, checkpoint/export scratch, parser high-water state, and blocking
   work that cancellation cannot stop? Can FIFO/full-charge admission deadlock or
   exceed the process budget?
10. Does a failed reload retain an observer/retry path? Can a stopped/reopened slot
    coexist with publication-capable old work? Is the narrowed tombstone lifetime safe?
11. Does the migration sequence ever create two active authorities, expose an
    uncharged active-plus-candidate path, or temporarily retain legacy in-place
    publication after candidates become reachable?
12. Are the project registry, source lifecycle, and capacity pool deep Modules with
    local ownership, or is this a God coordinator / shallow forwarding architecture?
    Name a smaller Interface if it preserves every invariant.
13. Does the compatibility projection preserve semver-public exhaustive Rust enums,
    HTTP/hook behavior, source identity, protected-root membership, snapshot
    quarantine, deletion convergence, and per-source independence without lying?
14. Are the stated liveness limits honest? Identify any scenario described as
    self-healing that can actually stall forever without an explicit operator action.
15. Give at least one concrete adversarial interleaving that the design still fails,
    or state why the promotion/query model excludes every interleaving you tried.

## 6. Review protocol

- This is a design review, not an implementation task.
- Do not modify source, design, context, or frozen Feature 020 files.
- Do not run Cargo tests, Clippy, release builds, or performance campaigns; the target
  changes documentation only. Read-only source inspection is encouraged.
- The only authorized write is the findings document named below.
- Ignore style-only preferences. Report P0/P1/P2 correctness, safety, migration, or
  feasibility findings. P3 notes are optional and non-blocking.
- Classify every finding as `PROVEN`, `LIKELY`, or `SPECULATIVE`.
- For every blocking finding include:
  - severity;
  - exact design/source location;
  - violated invariant;
  - concrete trace or counterexample;
  - smallest sufficient design correction;
  - a regression/model check that would fail before the correction.
- Do not reject the design merely because it conflicts with frozen Feature 020; that
  conflict is intentional. Block only if the amendment list is incomplete, weakens a
  load-bearing safety property without disclosure, or cannot be implemented safely.

## 7. Output

Write the verdict and findings to:

`docs/reviews/REVIEW-FINDINGS-claude-fable-project-index-lifecycle-prevention-2026-08-11.md`

Start with:

```markdown
# Review Findings — Project Index Lifecycle Prevention

**Target:** `5227277ac7c586ac40d60ad37c7beb6f7fa0f5c5`  
**Identity verified:** yes/no  
**Verdict:** CLEAR/BLOCK
```

If there are no P0/P1/P2 findings, say so explicitly and briefly list the major
invariants you independently verified. If blocked, order findings by severity and do
not continue into implementation advice beyond the smallest sufficient correction.
