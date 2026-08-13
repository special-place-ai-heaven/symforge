# Feature 020 V11 — Slice 2 evidence (T040)

**Scope**: Phase 4, T030–T040 — registry tombstones and process-wide capacity.
**Branch**: `feature-020-slice-2-registry-capacity`.

Volatile state (SHAs, PR numbers, CI results) is deliberately absent per the
documentation-hygiene rule; regenerate with `pwsh scripts/campaign-state.ps1`.

## What shipped

Six modules under `src/index_lifecycle/`, 56 executing oracles across five test
files plus one planned Slice-4 stub, every gate green including the embed
configuration.

Those 56 are not one population and should not be quoted as one number. **23 of
them, all in `project_index_authority_v11.rs`, pin `SourceRuntime` — a
`&mut self` state model with a single owner.** Slice 4 replaces it with an
`ArcSwap` publication root read concurrently, and the failures that matter there
are interleavings `&mut self` makes unrepresentable, so those 23 do not survive
T060 as evidence of the shipping thing. **The other 33 pin `Arc` + `Mutex` +
atomic primitives — registry, capacity, embedded, physical root — which carry
forward intact.** That split was drawn by the contract review and is recorded
here rather than averaged away.

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
**completeness**, not recency. A reload-entered `Refreshing` still serves the
complete generation it retains, because it was `Current` immediately before the
refresh and is complete. A mutation-entered refresh stays unqueryable until a
successor `Current` installs — retiring the permit is not an install, and
FR-043 forbids restoring the prior publication. `Blocked` and `Stopping`
retentions stay non-queryable — neither has a successor in flight, so a remnant
is not a refresh. Candidates, snapshot seeds and partial artifacts remain
unservable from every state.

The permit condition is not decoration. `Refreshing` is reached two ways: a
reload builds a candidate elsewhere and leaves the retained generation's bytes
alone, while a mutation reaches it through `request_mutation_grant`, whose
entire purpose is to stop the source serving before a disk write is authorized.
A20 as first written extended the reload argument to the mutation case silently
and reopened the window the freeze ordering exists to close. The later reading
that restored reads on permit retire was the same window wearing a drain.

Two regressions encode it. `F020-V11-R20A` is the availability half, pinned by
`a_refreshing_source_still_serves_its_complete_retained_generation` on a
reload-entered refresh, with a mutation-entered refresh as its paired negative
— including after that permit retires. `F020-V11-R20B` is the safety half,
pinned by `blocked_and_stopping_retentions_are_never_queryable`, which pairs
with a `Refreshing` source holding the identical retention so the refusal is
demonstrably about having no successor rather than a blanket refusal. Both bind
`ORACLE-QUERY-ATOMIC-LEASE`, the oracle that owns strict selection, which now
carries an assertion for the availability half so it can fail when A20 is
violated.

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
- **Four Slice-2 types did not carry the names their own oracles pin.**
  `ORACLE-CAPACITY-PHYSICAL-OWNERSHIP` and `ORACLE-EMBED-FOUNDATION` are
  Slice-2 oracles, and their frozen `production_seams` name
  `capacity.rs::ProcessCapacityPool`, `capacity.rs::CapacityPermit`,
  `process_runtime.rs::ProcessIndexRuntime` and
  `embedded.rs::EmbeddedSourceFactory`. The slice shipped that behaviour as
  `CapacityLedger`, `ChargedAllocation`, `ProcessRuntime` and
  `EmbeddedRegistration` — right conduct, wrong names, and a rename Slice 4
  would have had to perform under the postactivation seam check. Renamed.
  Every Slice 0, 1 and 2 seam anchor now resolves; the checker resolves them by
  a plain walk of `src/**/*.rs`, so this is a property of file and symbol names
  only, verified by re-running that resolution. `CapacityGrant` keeps its name:
  no frozen seam claims it.
- **The oracle the A20 regressions bind did not encode A20's availability half.**
  `ORACLE-QUERY-ATOMIC-LEASE` asserted only refusal — "strict mode refuses
  anything not proved Current and complete" — so a build that refused every read
  during a refresh would have satisfied it, which is precisely the availability
  regression A20 exists to prevent. Binding a regression to an oracle that
  cannot fail when the amendment is violated is the reporting defect this
  feature was written to prevent, in the contract rather than the code. The
  oracle now asserts that a source refreshing a successor still leases the
  complete generation it retains and that a retention with no successor in
  flight is refused, and lists the refreshing state among the states its
  preconditions make available. Both edits make the oracle strictly harder to
  satisfy. Its frozen digest is regenerated, and the checker's own pin moves
  with it.
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

## What the adversarial review changed (T040)

An independent review of the ordering and lifetime surface returned four
blockers, seven majors and four minors; findings are in
`REVIEW-FINDINGS-claude-orderings-feature-020-slice-2-2026-08-13.md`. All are
closed. The four that mattered most:

1. **A refusal consumed what it refused.** `install`, `cancel` and `stop` matched
   their expected occupancy with a `let`-else over `remove`, and `remove` runs
   before the pattern is tested. Refusing therefore evicted a live slot from the
   map with no revocation and no tombstone — the exact defect this module was
   written to prevent — under a comment claiming a restore that no line
   performed.
2. **Revocation did not reach authority already handed out.** `binding()`
   refuses once the slot is stopped, which stops the holder that asks again;
   `BindingAuthority` is `Clone`, so a clone taken before the stop still named
   the right root and held a live lease, and `SourceMutationPermit::grant`
   accepted it. "It refuses reads through the slot" is not the claim worth
   making. A binding's liveness is now shared across its clones and `stop`
   retires it.
3. **A20 was wrong as first written.** `request_mutation_grant` freezes to
   `Refreshing` precisely so the source stops serving before a source-disk write
   is authorized. A20 made `Refreshing` queryable, so a reader was served the
   very files a permit was replacing — and R20A, which reaches `Refreshing`
   through `request_mutation_grant`, asserted it. Queryability now additionally
   requires no outstanding permit; the grant records the permit it issues; R20A
   is retargeted at a reload-entered refresh with the mutation case as its
   paired negative.
4. **Conservation broke on a three-level hierarchy.** A child's limit is charged
   to its parent and never recorded as outstanding, so an owner with live
   children looked drained; releasing it returned its whole limit while its
   children kept spending against a limit nothing backed.

Both registry blockers were mutation-verified: reverting each fix turns its own
oracle red and leaves every other oracle green.

A second review, on a contract-and-claims lens, found three more — all real,
all reproduced by execution rather than reading:

5. **A grant leaked its charge.** `reserve` charges immediately and only the
   *permit* had a `Drop`, so a grant abandoned between reserve and redeem lost
   those bytes permanently, with no refund and no counter, and wedged
   `release_owner` for that owner forever. The module's stated invariant is that
   every charged byte is held or refunded exactly once; a grant was neither.
6. **A transition attested a freeze it never performed.** `freeze` returns `None`
   for a phase with no publication to freeze, `transition::apply` was its only
   caller and discarded the result, and `Option` is not `#[must_use]`. On a
   `Stopping` source it recorded a Freeze step and installed `Current` —
   resurrecting a revoked source with a receipt for a publication that never
   happened.
7. **A20 was amended in two documents out of eight.** Six further sites across
   four contracts and the quickstart still asserted the pre-amendment rule,
   including "Only lifecycle `Current` is queryable" in the very contract Slice 4
   implements its query lease against, and an acceptance step instructing a
   reader to prove that a `Refreshing` source refuses. Both documents are V11 and
   both claim supremacy, so this was a contradiction inside the frozen corpus,
   not V11 superseding V10. All eight now state one rule; every edit inside an
   existing clause range is line-count-neutral, checked range by range.

Amending those four contracts moved A19's clause hashes, because A20's
corrections land inside ranges A19 already owns and clause ranges may not
overlap across amendments. A20 therefore declares those contracts in its
`contract_clause_ids` rather than claiming replacement ranges inside A19's — the
attribution the manifest can actually express, stated rather than fudged.

Two oracles were themselves wrong and were fixed rather than bent.
`concurrent_opens_join_one_admission` handed its two opens two different
tempdirs — two different projects under one key — and passed. And the join check
compares physical roots rather than binding identities, because
`BindingAuthority::bind` mints a fresh identity per call, so comparing
identities would have refused the single-flight join the registry exists to
provide. The first version of that check did exactly that, and the old oracle
caught it.

`unknown_refunds` is now unreachable through the public API: `redeem` refuses a
foreign grant, `release_owner` refuses while charges or children are
outstanding, and a permit refunds exactly once by construction. It stays as a
fail-closed backstop, and the evidence for it is those three refusals rather
than a test that drives the counter — which is why the review's request to
"drive it above zero" is answered by structure instead of by a test-only
backdoor into production code.

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
