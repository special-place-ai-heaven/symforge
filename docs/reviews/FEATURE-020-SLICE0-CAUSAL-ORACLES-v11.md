# Feature 020 — Slice 0 causal oracle contract validation (T013)

Task T013. Validates the frozen
`specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md`
against the requirement set actually defined in `spec.md`, and records the table
digest, bounds, fairness assumptions, inherited-test resolution, and intended Slice 0
CI artifacts. **The frozen contract was not modified**, and no `tasks.md` checkbox was
touched.

Validated 2026-08-12 against `main` at `26846ab27ae8396e66f737aa7760daf6845cda21`.

## Immutable table digest

| | |
|---|---|
| Path | `specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md` |
| SHA-256 | `32021e8d8ec441dbedef42c797187e0fa16b3ed9e77b6b3a83bbca10cd3d43a3` |
| Line endings | `i/lf w/lf attr/text=auto eol=lf` — index and worktree bytes are identical |

That digest equals the `raw_bytes` entry recorded for this path in
`REFREEZE-MANIFEST-v11.md`, so the table validated here is the externally approved
one, not a locally normalized copy.

## Requirement coverage

`spec.md` defines exactly 78 identifiers in `- **ID**` form, each exactly once:
`FR-001`–`FR-052` and `SC-001`–`SC-026`, with no gaps, duplicates, or identifiers
outside those two ranges. The frozen table carries exactly 78 requirement rows in the
same order.

This closes a gap in the automated checker rather than duplicating it:
`scripts/validate-lifecycle-oracle-traceability.cjs` derives `EXPECTED_REQUIREMENTS`
from the literal counts 52 and 26 and never reads `spec.md`, so it proves the table is
internally complete but cannot notice the spec growing an `FR-053`. This document is
the cross-read.

Every reference in every row resolves inside the contract's own catalogs — tests,
seams, invariants, state models, bounds, fairness, CI artifact — with zero
unresolved references, and every row is `planned_not_executed` / `executed: false`, as
the contract's own preamble requires.

## Bounds

Six bounds, all referenced by at least one row:

| Bound | Rows | What it caps |
|---|---|---|
| `BOUND-SOURCE` | 32 | fixture matrices: sources, files, encodings, sparse entries, mutation schedules |
| `BOUND-QUERY` | 27 | one immutable root per lease per request; finite retries; typed refusal terminates |
| `BOUND-REPLAY` | 24 | journal replay bounded by a captured tail; later records force another finite round |
| `BOUND-ARTIFACT` | 21 | one deterministic JSON record per case; no unbounded logs or repository bytes |
| `BOUND-CAPACITY` | 10 | finite process/project/source/residency/headroom/reservation ceilings |
| `BOUND-VERIFICATION` | 8 | ≤17179869184 bytes, ≤200000 entries; ≥33554432 B/s and ≥1000 entries/s reserved |

`BOUND-VERIFICATION` is the arithmetically load-bearing one. Its own terms give a
reachable worst case of `ceil(17179869184/33554432) + ceil(200000/1000) = 512 + 200 =
712` seconds, under the 720-second pass ceiling; with the 180-second start deadline the
reachable maximum is 892 seconds, strictly inside the fixed 900-second overdue
interval. The interval is never extended — only a complete whole-scope verification
record advances it.

## Fairness assumptions

| Fairness | Rows | Assumption |
|---|---|---|
| `FAIR-RETRY` | 58 | finite retry budgets end in a proved promotion or a typed terminal/refusal state; churn cannot report false `Current` |
| `FAIR-CANCEL` | 25 | cancellation and stop acknowledgements outrank later grants for the revoked incarnation |
| `FAIR-PROJECT` | 10 | oldest fully-satisfiable multidimensional grant runs first; older drain barriers block younger conflicting grants; resize cleanup precedes requeue |
| `FAIR-OBSERVER` | 8 | a continuously healthy observer with capacity is eventually scheduled to a stable cut; gaps are never starved or silently cleared |

These are assumptions the oracles rely on, not properties Slice 0 proves. Nothing in
Slice 0 exercises a scheduler; `FAIR-PROJECT` and `FAIR-OBSERVER` first become
testable in Slice 2 and Slice 4 respectively.

## Inherited-test resolution

The table contains exactly one `inherited_exact` row, and T013 owns it:

| | |
|---|---|
| Test ID | `TEST-OPAQUE-PATH-INHERITED` |
| Target | `src/discovery/mod.rs::tests::metadata_first_scout::non_utf8_path_is_opaque_catalog_only_without_lossy_collision` |
| Command | `cargo test --lib discovery::tests::metadata_first_scout::non_utf8_path_is_opaque_catalog_only_without_lossy_collision -- --exact` |
| Requirement | `FR-025` (jointly with the planned `TEST-OPAQUE-PATH`) |

Resolved in the release tree: `mod metadata_first_scout` opens at
`src/discovery/mod.rs:3801` and the named function is declared at
`src/discovery/mod.rs:4169`. The path and function name match the frozen target
exactly, so the contract's `planned_case_policy` — "every inherited_exact Rust target
must exist as the exact named test in the release tree" — holds today.

Resolution here means identity, not execution. The row stays
`planned_not_executed`; the contract routes execution to the materialized release
validation, which runs the command through an absolute outside-repository Cargo
executable in a clean pinned tree and requires a freshly created command receipt.
Recording a pass here would be exactly the "evidence JSON alone is never execution
proof" failure the contract forbids.

The other Slice 0 test, `TEST-PUBLICATION`
(`tests/project_index_lifecycle_slice0.rs::whole_project_publication_preserves_latest_siblings`,
`introduced_slice: 0`), is owned by T017 and does not exist yet. It backs `FR-008`,
`FR-009`, and `SC-005`.

## Intended Slice 0 CI artifact

`CI-SLICE0` = `target/ci/lifecycle-v11/slice-0-oracle-contract.json`.

**Observation, recorded rather than fixed.** Every one of the 78 requirement rows
carries `target_slice: 4` and `ci_artifact_id: CI-SLICE4`. `CI-SLICE0` through
`CI-SLICE3` appear in the `ci_artifacts` catalog but are referenced by no requirement
row. That is consistent, not contradictory: a requirement can only be *claimed*
satisfied after the activation cut, so requirement-level artifacts all land in Slice 4,
while the per-slice artifacts belong to the slice tasks. It does mean the earlier
per-slice artifacts have no requirement-row obligation forcing them to exist, so if a
slice silently skipped its artifact, the requirement table alone would not detect it.
Slices 0–3 must therefore be checked against their task receipts, not against this
table. The contract is frozen; this is noted for the reviewers of T020 and T090.

## T014 — observed RED oracle for design defect 2.8

**Status: landed.** This section first recorded the oracle as blocked, because the
retirement census digested whole source bytes and so forbade the very edit T014
requires. That contradiction was resolved by amending the refreeze (see "Two
contradictions inside the frozen corpus" below): the census now digests a canonical
release form, in which `#[cfg(test)]` items are invisible and production bytes are
not. The oracle and its `cfg(test)` seam therefore land as T014 specifies, with the
census reporting `OK` and still failing on any production edit to the same files.

The design document's §2.8 says `effective_fence_generation`
(`src/watcher/mod.rs:255`) assumes reload publishes the new root before advancing
`project_generation`, while `reload_for_binding_with_exclusions` does the opposite.
That is confirmed in the current tree: `project_generation.fetch_add` is
`src/live_index/store.rs:2409` and `swap_and_publish` is `src/live_index/store.rs:2414`.
The doc comment at `src/watcher/mod.rs:249-251` asserts the reverse ordering and is
wrong.

The oracle pauses inside that window using a `#[cfg(test)]` one-shot thread-local
hook — the same shape as the existing `write_interleave` hook in
`src/protocol/edit.rs:208`, fired between the generation advance and the publication,
compiled out of release builds. A production seam was needed because no crate-visible
operation advances the generation without also publishing; a two-thread race would
have been flaky rather than deterministic.

Observed, running exactly the command T014 names:

```
cargo test --lib watcher::tests::generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b -- --exact --nocapture

root A's file must not be written into root B's publication (observed fence 1,
spawn generation 0, root published at observation time Some("...\.tmpePTrxJ"));
a mid-commit observer must not be able to authorize a root-A reindex against root B
test result: FAILED. 0 passed; 1 failed
```

The mid-commit observer read generation `1` while root A was still the published
root, adopted it as a same-project reload, and the resulting reconcile wrote root A's
`src/a.rs` into root B's publication. That is the §2.8 interleaving, reached without
a sleep or a second thread.

The oracle asserts the consequence, not the intermediate fence value, so it stays
valid under either fix — reordering the commit, or binding root and generation into
one authority.

**Gating.** The oracle carries `#[ignore]` naming Slice 1 (T022–T029) as the owner
that must remove it, so a deliberately RED control does not turn `main` red. Append
`--ignored` to the frozen command to reproduce the failure. Both the oracle and its
seam live in `#[cfg(test)]` scope, which the amended census does not digest; adding
any production byte to `src/watcher/mod.rs` or `src/live_index/store.rs` still fails
`RETIREMENT_CLOSURE_MISMATCH`, and that was verified directly rather than assumed.

## T015–T017 — observed RED positive controls

`tests/project_index_lifecycle_slice0_controls.rs`, run with
`cargo test --test project_index_lifecycle_slice0_controls -- --ignored --test-threads=1`.
Each fails for the reason its name states:

| Control | Defect | Observed | Task |
|---|---|---|---|
| `capacity_refused_open_creates_no_slot_and_no_watcher` | 2.1 admission refusal crosses the seam as success | refusal returned `Ok(project-v1-bc1bfed7…)` and registered a project | T015 |
| `empty_placeholder_publication_refuses_watcher_mutation` | 2.2 / 2.3 mutable-empty placeholder, watcher as competing loader | watcher admitted 8 paths into a never-published placeholder | T015 |
| `failed_reload_retains_the_recovery_observer` | 2.10 failed reload removes the recovery observer | edit after a failed reload never observed (8 files before, 8 after) | T015 |
| `observer_replacement_gap_is_latched_as_non_current` | 2.6 readiness rederived from present state | `Degraded{[ObservationFailed, ReconciliationPending]}` → `Current` after a reload that proved nothing | T016 |
| `old_observer_delivery_after_promotion_is_not_current` | 2.8 no observer token or epoch | a pre-promotion delivery applied to the promoted generation, still `Current` | T016 |
| `watcher_mutation_during_candidate_build_is_not_discarded` | 2.7 / 2.9 no candidate isolation | observer mutation destroyed by the swap, publication reports success | T016 |
| `publication_of_one_source_preserves_sibling_latest` | FR-008 / FR-009 / SC-005 partial publication | sibling B's latest dropped by source A's whole-index swap | T017 |

Each carries `#[ignore]` naming the slice that must remove it, so a deliberately
RED control does not turn `main` red and the fix's acceptance is the control
passing without the attribute.

The T015 single-flight control is not in this set because it needs crate-internal
access; it lands as a unit test in `src/daemon.rs::tests` — see below.

### The single-flight control counts production's own loader

`ensure_project_slot_for_session_with` takes the loader as a closure parameter, so a
unit test can pass `ProjectInstance::load` — exactly what
`ensure_project_slot_for_session` supplies in production — wrapped in a counter. Four
threads entering a shared barrier and racing one first open produce:

```
cargo test --lib daemon::tests::concurrent_first_open_performs_exactly_one_cold_load -- --exact --nocapture --ignored

4 concurrent first opens of one root must perform exactly one cold load; every
extra load is a complete project index built, paid for, and discarded by `or_insert`
  left: 4
 right: 1
```

An earlier attempt added a `cold_project_loads` counter to `DaemonState` to make the
waste observable from an integration test. That was abandoned: it changed production
code to observe a defect, and the injected loader counts the same real loads at the
seam that owns them with no production change at all. Slice 2 (T030–T040) inherits an
assertion that needs no instrument to keep working.

### The gap control had to be reframed before it meant anything

Written the obvious way — "a change made while no observer existed must leave the
index non-Current" — it **passed**. V10 does mark that window
`Degraded{[ObservationFailed, ReconciliationPending]}`, so the crude property is not
the defect.

The real defect is narrower and worse. `recompute_freshness_locked`
(`src/live_index/store.rs:1840-1909`) explicitly drops the previous
`ObservationFailed`, `ReconciliationPending`, and `SnapshotVerificationFailed` reasons
and rederives them from present state — currently-unreadable entries and current
scout coverage. A gap is a historical fact; freshness is a pure function of the
present. So the first publication that happens to look clean reports `Current` again
with nothing having proved the missed window was recovered.

The control now performs an ordinary clean reload after the gap and asserts the latch
survives it. It does not: `Degraded{…}` → `Current`. That is a materially better
oracle than the one that passed, and it names the exact mechanism a fix must change.

The 2.8 old-observer control fell into the identical trap and was reframed the same
way. Three of the ten controls attempted in this slice initially passed. All three
were passing because the assertion happened to be satisfied by unrelated present
state, never because the defect was absent — which is precisely why a positive
control that passes has to be chased rather than shipped.

**The general lesson for the remaining slices**: `FreshnessStatus` in V10 is a pure
function of present state, so no assertion of the form "X must be non-Current" can
distinguish a real latch from incidental degradation. Any lifecycle property about a
*historical* fact must be asserted as surviving a subsequent clean publication.

### Two controls needed a specific reachability fix

Both were initially written against paths that never reach the defect, and both
*passed*, which is the failure mode this slice exists to prevent — a green positive
control is a vacuous one.

- **2.10** first drove a failing *first* open. That fails inside
  `ensure_project_slot_for_binding` while the old watcher is still running, so
  `reload_with` is never entered. Reaching it requires an already-open slot whose
  in-place rebuild then fails: open the project, impose
  `SYMFORGE_MAX_INDEX_FILES=1`, and re-issue `index_folder` for the same root.
  Capacity refusal is converted to `Ok(empty)` only on the cold bootstrap path; on
  reload it propagates, and that is precisely the `?` that skips the watcher restart.
- **2.2 / 2.3** first called an internal reconcile helper that integration tests
  cannot see. Driving the real `run_watcher_with_stop` against the placeholder is both
  reachable and more faithful to the defect, which is about the watcher's own first
  action.

### Deliberately-RED tests must tear down before they assert

The controls were first written with teardown after the assertions. Because they are
RED, every run panicked before its `shutdown_tx.send(())`, leaving the daemon's
`notify` OS threads alive — so the test binary never exited. On Windows a live process
holds its own `.exe` open, and the next `cargo test` failed to link it (LNK1104)
against a completely unrelated target, twice, before the cause was found.

Every control now observes into locals, tears down, and asserts last, which cannot
skip cleanup on any path. This is a general rule for this feature's RED suites, not a
one-off: an always-failing test is exactly the test whose cleanup never runs.

## Two contradictions inside the frozen corpus

Both were found by executing the frozen gates, not by reading. Both block part of
Slice 0 as literally worded, and neither can be resolved by a code change, because
every artifact involved is frozen and byte-pinned.

### 1. T014 requires editing a byte-censused source

`contracts/v10-authority-retirement-v11.md` carries a `preactivation_closure` that
digests the **whole content** of every V10 retirement-member source. The checker
normalizes only line endings (`normalizeRetirementClosureSource`,
`scripts/validate-lifecycle-oracle-traceability.cjs:2177`) and, in the ordinary
(non-materialized) lifecycle, reads the **working tree** via `currentRustSourceMap()`.
Any byte change to a censused file — including adding a `#[cfg(test)]` test — fails
the gate.

The closures cover `src/daemon.rs`, `src/live_index/store.rs`, and
`src/watcher/mod.rs`, among others. T014 instructs adding an oracle **in
`src/watcher/mod.rs::tests`**.

Measured, not assumed: clean `main` (`b25fc35f`) in a scratch worktree reports
`OK (78 requirements, 24 acceptance oracles, 13 retirement categories)`; adding the
oracle plus its observation seam reports `RETIREMENT_CLOSURE_MISMATCH` on exactly
`cache`, `callbacks`, and `publication_roots` — the three categories containing the
three touched files.

**The census is right and T014 is the outlier.** Every task from T022 to T052 creates
new `src/index_lifecycle/*` files and never edits a V10 source; Slice 4 is where the
activation cut and retirement (`slice4_owner`: T064–T067) make those edits legal.
T014 is the only Slice 0–3 task naming an existing censused file, so it is a drafting
error against its own retirement contract rather than a real instruction.

### 2. A catalog-named test cannot be both present and RED

`TEST-PUBLICATION` is `planned_exact`, owned by T017, targeting
`tests/project_index_lifecycle_slice0.rs::whole_project_publication_preserves_latest_siblings`.
`rustNamedCaseBodyInSource` (checker line 1033) rejects any candidate carrying
`#[ignore]`:

```js
!/#\s*\[\s*(?:ignore\b|cfg_attr\b)/u.test(attributes[0])
```

So the named case must exist un-ignored. T017 requires it to be a RED positive
control. `.github/workflows/ci.yml` — frozen — runs `cargo test --all-targets`. An
un-ignored RED test turns `main` red; an ignored one fails the checker with
`PLANNED_TEST_CASE_MISSING`. There is no third state, and all three artifacts are
frozen.

The check only fires when the target file exists (`isRegularFile(file)`), which is the
only reason Slice 0 can land at all.

## How each contradiction was resolved

**Contradiction 1 was fixed at the source.** The refreeze was amended so the census
digests a canonical release form — `#[cfg(test)]` items removed, comments dropped,
code whitespace collapsed, string and character literals verbatim — instead of raw
source bytes. The census still freezes V10 authority, because any production byte
moves the digest; test-only code, which the release build never compiles, no longer
does. T014's oracle and its `cfg(test)` seam therefore land exactly as `tasks.md`
specifies, and the 2.4 single-flight control lands as a unit test in
`src/daemon.rs::tests`. Both were verified against the amended census, and adding a
production line to the same files was verified to still fail.

**Contradiction 2 is worked around, deliberately.** Slice 0's integration controls
live in `tests/project_index_lifecycle_slice0_controls.rs`, leaving the catalog's
reserved `tests/project_index_lifecycle_slice0.rs` unwritten so `TEST-PUBLICATION`
stays dormant until the slice that can make it pass. The control that would have
carried that name is `publication_of_one_source_preserves_sibling_latest`, renamed so
no reader mistakes it for the catalog case being satisfied.

That second one is a real remaining inconsistency in the frozen corpus, not a solved
problem: a `planned_exact` case cannot simultaneously be present, RED, and green in
CI. It is left for the T021 adversarial review to rule on, since unlike the first it
does not block any Slice 0 work.

### Known gap in the amended stripper

`normalizeRetirementClosureSource` recognises a literal `#[cfg(test)]` attribute. It
does **not** recognise `#[cfg(all(test, feature = "…"))]`, which is equivalent to
rustc and is the natural spelling when a test-only helper's consumer sits behind a
feature gate — so the census reads such an item as production code and fails.

Found immediately: the T014 hook's re-export has to be server-gated (its consumer is
`src/watcher/mod.rs::tests`, and `watcher` is `#[cfg(feature = "server")]`, so under
`--no-default-features --features embed` the bare `#[cfg(test)]` form is an unused
import that `-D warnings` rejects). Writing that as `cfg(all(test, feature =
"server"))` moved the `publication_roots` digest.

The workaround is exact and costs nothing: two stacked attributes, `#[cfg(test)]`
first, then `#[cfg(feature = "…")]`. The cut runs from the first attribute to the
item's terminator, so the second is absorbed with it. `CLAUDE.md` records this as the
supported spelling.

The proper fix is for the stripper to evaluate test-only cfg predicates — `test`, and
`all(...)` with `test` as a conjunct, while correctly *rejecting* `any(test, …)` and
`not(test)`, which are not test-only. That needs another refreeze amendment and
signature, so it is deferred rather than done here; it blocks nothing while the
stacked spelling is used.

Nothing was faked, weakened into a vacuous pass, or silently dropped.

## Scope note

Two Slice 0 tests are introduced (`TEST-OPAQUE-PATH-INHERITED`, `TEST-PUBLICATION`)
but zero requirements are marked satisfied by Slice 0. Slice 0's product is working
positive controls plus this frozen-contract validation — not requirement closure. The
remaining Slice 0 tasks (T014–T021) add the RED oracles that must be observed failing
for the right reason before any Slice 1 production code exists.
