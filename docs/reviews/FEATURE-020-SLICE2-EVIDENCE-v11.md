# Feature 020 V11 — Slice 2 evidence (T040)

**Scope**: Phase 4, T030–T040 — registry tombstones and process-wide capacity.
**Branch**: `feature-020-slice-2-registry-capacity`.

Volatile state (SHAs, PR numbers, CI results) is deliberately absent per the
documentation-hygiene rule; regenerate with `pwsh scripts/campaign-state.ps1`.

## What shipped

Six modules under `src/index_lifecycle/`, 45 oracles across five test files,
every gate green including the embed configuration.

| Module | Task | What it establishes |
|---|---|---|
| `authority.rs` (widened) | T024 | phases carry the fields the frozen model requires |
| `capacity.rs` | T032/T035/T036 | conservation defined against physical drop |
| `process_runtime.rs` | T034 | one capacity domain across four surfaces |
| `registry.rs` | T033 | single-flight admission, enforced tombstones |
| `embedded.rs` | T037 | sole handle, close/drop coalescing, self-wait refusal |
| `adapters.rs` | T038 | dark planning, separated from execution |

All three contract-pinned oracle names are used verbatim:
`protected_membership_and_state_placement`, `one_handle_close_and_drop_coalesce`,
`capacity_is_conserved_until_physical_drop`. Slice 1 lost a CI cycle by inventing
its own; that is not repeated.

## The slice began by discovering its own foundation was wrong

Before any Slice 2 code, five parallel readers mapped the real daemon, store,
capacity and embed code; a fit report drew conclusions; a skeptic attacked it and
refuted its central remedy. Three findings changed the plan:

1. **Slice 1 was a sketch, not a foundation.** Every `SourcePhase` variant was
   missing fields the frozen `SourceRuntimeState` requires — including the two
   this slice exists to add. `active_permits` is **plural** in the model while
   Slice 1 tracked a single drain signal, and
   `committed_source_revocation_residency` did not exist at all. So Step 0 was
   widening `authority.rs`, with the 19 existing oracles moving with it. Doing
   that first cost one commit; finding it at T060 would have cost a rewrite with
   41 tasks in flight.

2. **`Arc` clones outlive registry membership and nothing revokes them**
   (`daemon.rs:231`, removal at `:1290`, `:1773`, `:1851`). That is the measured
   fact behind the tombstone design — not a spec assertion.

3. **`ActivationState` has no de-activation transition**; nothing returns it to
   `Inactive`. A tombstone design assuming one would have been designing against
   a system that does not exist.

## The amendment this slice forced

The investigation surfaced a contradiction that no amount of code could resolve:
V11 said a `Refreshing` source retains a generation that **nothing may query**,
and places candidate building inside `Refreshing`. Production has always served
the previous complete generation while a reload builds, pinned green by
`test_same_project_reads_prior_generation_during_reload`.

Wiring `index_folder` to the reload transition at Slice 4 would therefore have
taken every project offline for the duration of every reindex.

**F020-V11-A20** resolves it, operator-decided and signed: queryability closes on
**completeness**, not recency. The single generation `Refreshing` retains stays
queryable because it was `Current` immediately before the refresh and is
complete. `Blocked` and `Stopping` retentions stay non-queryable — neither has a
successor in flight, so a remnant is not a refresh. Candidates, snapshot seeds
and partial artifacts remain unservable from every state.

Two regressions encode it: `F020-V11-R20A`
(`a_refreshing_source_still_serves_its_complete_retained_generation`) is the
availability half; `F020-V11-R20B`
(`blocked_and_stopping_retentions_are_never_queryable`) is the safety half, and
pairs with a `Refreshing` source holding the identical retention so the refusal
is demonstrably about having no successor rather than a blanket refusal.

## Deferrals closed rather than carried

The standing rule is that no known gap crosses a slice boundary. Three were
open; all three are closed.

- **The temp-before-replace oracle proved a receipt label, not I/O order**
  (found by grok-4-5). A build that renamed first while pushing the labels in
  order would have passed it. Replacement is now staged and committed in two
  steps, so an oracle observes on disk that the temporary exists while the target
  still holds its original bytes. The mutation sweep reverts real I/O now.
- **Oracles leased the machine's shared temp directory** and left probe files
  there, so on a multi-user box another user's leftover would make a rename fail
  and turn a property failure into an environment failure. Every test owns its
  root.
- **The module sat at `src/live_index/index_lifecycle/`** while every frozen
  seam in the contract names `src/index_lifecycle/…`. The Slice-1 evidence
  recorded this as "defensible" and moved on; it is now closed rather than
  carried, because the postactivation seam check resolves those exact file
  paths and Slice 3 and 4 would otherwise write thousands more lines at the
  wrong address. The files now live at `src/index_lifecycle/`, reached by
  `#[path]` from `live_index/mod.rs`. That split is deliberate: `introduced_v11_atoms`
  never names a public `symforge::index_lifecycle`, so the module must not be a
  top-level `pub mod` — the V11 surface is re-exported through `embed` at
  activation. File location and module path answer to different contracts, and
  `#[path]` is what satisfies both. At T060 the declaration is deleted and
  `lib.rs` gains a private `mod index_lifecycle;`; no file moves then.
- **Two amendment regressions named oracles that do not exist.** `F020-V11-R20A`
  and `R20B` were written against invented IDs
  (`ORACLE-REFRESHING-SERVES-COMPLETE-RETENTION`,
  `ORACLE-REMNANT-RETENTION-NOT-QUERYABLE`), against the rule stated at the top
  of that very section: each regression binds one *existing* acceptance oracle
  and its exact executable case. Both now bind `ORACLE-QUERY-ATOMIC-LEASE`,
  whose preconditions already enumerate the stopping state and whose assertion
  "strict mode refuses anything not proved Current and complete" is the property
  A20 sharpens. The two Slice-2 authority tests that carried those names remain
  and remain green; they are the model-level half, and the contract binding
  names the Slice-4 oracle that proves it end to end.
- **A Slice-4 case name was missing from a file Slice 2 created.** The checker
  requires every `planned_exact` case declared for a file to exist once the file
  exists, and `tests/process_capacity_pool_v11.rs` carries both TEST-CAPACITY
  (Slice 2) and TEST-CAPACITY-INTEGRATION (Slice 4, T069). The Slice-4 name is
  materialized as a RED-by-construction `#[ignore]` stub whose body panics with
  the reason it cannot yet prove anything — the same shape Slice 0 used. The
  release runner independently refuses an ignored-only run as execution evidence,
  so the stub cannot become a silent pass.
- **TOCTOU in root confinement.** Closed with a `cap-std` directory capability:
  every path is opened relative to the handle, so a component swapped to a link
  after a check cannot be followed, because the open is handle-relative too. The
  previous design documented this hazard honestly and did not fix it; documenting
  is not fixing.

## Design decisions worth challenging

Stated plainly so a reviewer can attack them rather than discover them.

- **`capacity.rs` never blocks.** The loader already waits on a condvar inside
  the shared rayon pool; a process-wide *blocking* pool layered over it is a
  deadlock, because a worker parked waiting for capacity holds a pool thread the
  grant it waits for may need. The invariant "the leaf keeps its own per-load
  budget" is binding until a loom or stress proof says otherwise.
- **Conservation is defined against physical `Drop`.** A holder that forgets to
  release still refunds, because an un-refunded charge permanently leaks process
  capacity. `release` takes `self` by value, so a double refund is
  unrepresentable rather than discouraged.
- **Revocation is enforced, not advised.** A stopped `LiveProjectSlot` refuses
  every authority-conferring read. Rust cannot take an `Arc` back, so the handle
  is made useless instead. Asking holders to check `is_current` first would have
  been documenting the hazard.
- **A child's capacity is charged to its parent at promise time**, not at spend
  time, because capacity promised to a child is capacity the parent can no longer
  promise elsewhere.

## Known limits carried forward

- **Slice 2 is dark by design.** No production code calls it. Every candidate
  seam was refuted concretely — the read counter does not compile as described on
  a `Clone` type returned by value, `registry.rs` cannot name daemon types
  without a feature gate that contradicts its own scope, `status` is a pinned
  protocol surface, a capacity observer would execute under `embed` and add an
  atomic per discovered file on the hottest loop, and the tombstone path has no
  neutral configuration. The spec also schedules T051 to prove this slice is
  dark. Fighting that is fighting the design.
- **`SourceRuntime` is still `&mut self`,** while the real store publishes
  through `ArcSwap` and is read concurrently. Slice 2 shaped the types against
  that fact but did not place them under a publication root; that is Slice 4.
- **`process_runtime.rs` and `adapters.rs` have no dedicated oracle file.** They
  are exercised through the registry and T039 proofs. A reviewer should judge
  whether that is sufficient coverage for T034 and T038 or whether each needs its
  own.
