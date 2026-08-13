# Slice 2 plan — after a reality-fit investigation and its refutation

Produced by mapping the **real** production code (five parallel readers, 1.4M
tokens, 427 tool calls) into a fit report, then attacking that report with a
skeptic. The skeptic refuted the fit report's central remedy, so what follows is
the plan *after* that correction, not the fit report's own recommendations.

Nothing here is written from spec prose alone. Where a claim is inherited rather
than verified, it says so.

## PROGRESS LEDGER — durable, does not depend on session state

A harness task list is not a ledger; it may not survive a context compaction.
This section, the git history, and the agentmemory `[symforge]` bootstrap are the
things that do. Update this when a step lands.

| Step | State | Evidence |
|---|---|---|
| Step 0 — widen `authority.rs` to the frozen shape | DONE | 19 pre-existing + 5 new oracles green |
| `capacity.rs` (T032/T035/T036) | DONE | 6 oracles, incl. pinned `capacity_is_conserved_until_physical_drop` |
| `process_runtime.rs` (T034) | DONE | one capacity domain across four surfaces; spawns nothing |
| `registry.rs` (T033) | DONE | 5 oracles, incl. pinned `protected_membership_and_state_placement`; revocation enforced |
| `embedded.rs` (T037) | WRITTEN, oracles pending | pinned name `one_handle_close_and_drop_coalesce` not yet written |
| `adapters.rs` (T038) | NOT STARTED | |
| T039 refusal/cancellation proofs | NOT STARTED | |
| T040 evidence + adversarial review | NOT STARTED | |

### Deferrals closed rather than carried

The standing rule is that no known gap crosses a slice boundary.

- **Temp-before-replace proved only a receipt label** (grok-4-5). Closed: the
  write is staged and committed in two steps, an oracle observes the target
  holding its preimage while the stage exists, and the mutation sweep now reverts
  real I/O instead of a label.
- **Oracles leased the machine's shared temp directory** and left probe files in
  it. Closed: every test owns its root.
- **TOCTOU in root confinement.** Closed properly with a `cap-std` directory
  capability — every path is opened relative to the handle, so a link swapped in
  after a check cannot be followed. This replaced a documented hazard with an
  enforced one.

## BLOCKER — V11's frozen semantics contradict currently-shipping behaviour

This is the finding that matters, and it is not fixable inside Slice 2.

**Today**, a reload keeps serving the previous generation. That is pinned green:

- `test_same_project_reads_prior_generation_during_reload` (`daemon.rs:9291-9369`)
  asserts `Arc::ptr_eq(&prior, &during)` with the message "reads must retain the
  prior published index while reload builds".
- `open_project_for_session`'s own doc (`daemon.rs:1617-1618`): "activate and
  reload through the slot's mutation lane (same-project reads keep serving the
  previously published generation)".

**V11 says the opposite.** `Refreshing` retains one generation and *none* is
queryable (`spec.md:14-17`, `data-model.md:1539-1542`: "No public Interface can
acquire any retained generation"). Slice 1 encoded that faithfully
(`authority.rs:336-337`, `:482-485`).

The fit report proposed escaping this by routing content refresh through the
candidate pipeline "while the source stays `Current`". **The skeptic refuted
that from the frozen model**: `data-model.md:1427-1435` places
`work: NonCurrentWork` *inside* `Refreshing`, and `NonCurrentWork` includes
`Building { candidate_authority }` (`data-model.md:1462-1467`); a discarded
candidate routes to `Loading | Refreshing | Blocked` (`data-model.md:2117-2122`).
**Candidate building is the non-queryable state in this model.**

So the conflict is in the V11 semantics themselves. At Slice 4, whoever wires
`index_folder` to `transition::apply(TransitionKind::Reload)` produces a project
that is unqueryable for the duration of a full reindex — a regression against a
green test and against the product's current behaviour.

**This needs a decision before Slice 4, and it is not mine to make**, because
resolving it in the spec's favour means accepting a user-visible regression, and
resolving it in production's favour means amending a frozen contract — which
requires regenerating the manifest and attestation and obtaining a human
signature. It is recorded here so the decision happens deliberately rather than
being discovered as a failing test with 41 tasks in flight.

Slice 2 proceeds on the parts unaffected by it.

## The "wire something live" rule does not survive contact with this slice

The stated methodology was: no zero-consumer slices — wire at least one real
seam behaviour-neutrally so real traffic exercises the code. For Slice 2 that is
**not achievable**, and each candidate was refuted concretely:

| Proposal | Why it fails |
|---|---|
| In-flight read counter on `SessionRuntime` | `#[derive(Clone)]` returned by value (`daemon.rs:752-753`, `:1875-1879`); a `Drop` decrement double-counts per clone and measures value lifetime, not read duration |
| `registry.rs` naming `ProjectSlot`/`DaemonState` | `index_lifecycle` is ungated, `daemon` is `#[cfg(feature = "server")]` (`lib.rs:40-42`); gating `registry.rs` contradicts T034's process-wide scope |
| Surfacing counters in `status detail=projects` | `status` is an advertised MCP tool pinned by `test_client_allow_lists_match_registered_tool_surface` and by conformance tests reading `AGENTS.md`/`README.md` |
| Capacity shadow observer in `admit_and_parse_entries` | `live_index` is ungated, so it executes under `--features embed`, violating `embed.rs:44-53`'s no-implicit-background-machinery contract; and it sits in the `par_iter` body (`store.rs:4264`) — an atomic per discovered file on the hottest cold-load loop |
| Stopping tombstone in the removal path | `finish_removed_session` removes under `projects.write()` then `slot.stop()` blocks on `slot.mutation` (`daemon.rs:1304-1306`, `:3240`); no neutral configuration exists |

**And the spec schedules T051 to prove Slice 2 IS dark.** Fighting that is
fighting the design, not improving it. The rule is therefore amended for this
slice: Slice 2 stays dark by design, and the risk it creates is retired by
*shaping against measured production facts* instead of by premature wiring.

## What Slice 2 actually does, in order

### Step 0 — widen `authority.rs` to the frozen shape, before anything else

Slice 1 is a narrower sketch of `SourceRuntimeState`, not a foundation to build
on. Every phase variant is missing fields the model requires:

| Frozen `SourceRuntimeState` (`data-model.md:1409-1449`) | Slice 1 `SourcePhase` (`authority.rs:344-359`) |
|---|---|
| `Loading { binding, observer_phase, mutation_epoch, source_revocation_publication_package, work }` | unit variant |
| `Refreshing { …, active_permits, retained, work }` | `{ retained, binding, publication }` |
| `Stopping { revocation, retained, committed_source_revocation_residency }` | `{ retained }` |
| `Current { generation, … }` with **no** side fields (`:1543-1546`) | `CurrentPublication` carries `binding` and `observer_cut` as side fields |

Two of the missing fields are exactly Slice 2's deliverables: `active_permits`
is **plural** (Slice 1's `transition::apply` refuses on a single outstanding
permit — `transition.rs:68`, `:80-82`), and
`committed_source_revocation_residency` is the pre-charged teardown capacity
T032's "fixed safety precharge" refunds.

The 19 Slice 1 oracles and 12 mutation guards are pinned to the narrow shape and
move with it. Doing this first costs one commit; discovering it at T060 costs a
rewrite with 41 tasks in flight.

### Step 1 — the three RED oracle files, using the contract-pinned names verbatim

| Test ID | Pinned target |
|---|---|
| `TEST-REGISTRY` | `tests/project_registry_lifecycle_v11.rs::protected_membership_and_state_placement` |
| `TEST-EMBED-FOUNDATION` | `tests/embed_lifecycle_v11.rs::one_handle_close_and_drop_coalesce` |
| `TEST-CAPACITY` | `tests/process_capacity_pool_v11.rs::capacity_is_conserved_until_physical_drop` |

None of these files exist yet. Slice 1 lost a CI cycle by inventing oracle names
when the contract pinned them; that is not repeated.

### Step 2 — modules, in dependency order

`capacity.rs` → `process_runtime.rs` → `registry.rs` → `embedded.rs` →
`adapters.rs`. Capacity first because registry admission charges against it.

### Step 3 — run the embed gate on the FIRST commit that adds a file

`cargo test --no-default-features --features embed --lib -- --test-threads=1`,
through Terminal Commander. Not at T040. `live_index` is ungated while `daemon`
is server-gated, so every new file is a potential embed break invisible to the
default gates — the exact class that has cost this repo CI cycles before.

## Binding constraints for this slice

- **`capacity.rs` ships as accounting only, never as a live blocking budget.**
  Today's per-load `InflightByteBudget` (512 MiB, `store.rs:163-218`) blocks on a
  `Condvar` inside the shared fixed-size rayon pool (`store.rs:4264`). A
  process-wide *blocking* pool there is a deadlock, not a refactor. The invariant
  "the leaf keeps a per-load budget" is binding until a loom or stress proof says
  otherwise, and T036's "out-of-lock dispatch" language is not permission to skip
  that proof.
- **Name collision**: `ProjectSlot` already exists as the daemon's live registry
  entry (`daemon.rs:305-310`). Slice 2's `LiveProjectSlot` must be documented
  against it explicitly or reviewers will conflate them.
- **`ActivationState` has no de-activation transition today** — nothing returns
  it to `Inactive` (`daemon.rs:276-284`, `:3193-3201`). A tombstone design that
  assumes one is designing against a system that does not exist.
- **`Arc<ProjectSlot>` clones outlive map membership** and nothing revokes them
  (`daemon.rs:231`, removal at `:1290`, `:1773`, `:1851`). That is the concrete
  motivation for a non-revivable tombstone, and it is a measured fact rather than
  a spec assertion.

## Claims inherited, not verified

Stated so a later reader does not mistake them for observations:

- The fit report's `RequestGovernor` and constructor-funnel counts were corrected
  by the skeptic (two production sites, not four; `ProjectSlot::new` has two call
  sites, not one). Neither figure is load-bearing for this plan.
- No cargo command has been run against the widened `authority.rs`, because it
  does not exist yet.
