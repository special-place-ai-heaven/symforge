# Review findings — Feature 020 V11, Slice 1

**Model:** Composer  
**Date:** 2026-08-12  
**Branch reviewed:** `feature-020-slice-1-mutation-authority`

---

### BLOCKER — `commit` accepts a write receipt from any lease

- **Where**: `src/live_index/index_lifecycle/mutation.rs:205-210`
- **Claim**: `SourceMutationPermit::commit` discards the receipt without checking it came from this permit's pinned lease.
- **Why it matters**: A permit on root A can `start_side_effect`, call `replace_beneath` on root B, and `commit` that receipt. A disk side effect happens under a root the authority never named, while the permit reports success.
- **Recommended fix**: In `commit`, require `receipt.target()` to resolve beneath `self.lease` (or store the lease identity inside `WriteReceipt` at creation and compare). Refuse with `WholeAuthorityMismatch` or a new variant on mismatch.

---

### MAJOR — transition can install over a live permit when drain signal is omitted

- **Where**: `src/live_index/index_lifecycle/transition.rs:80-92`
- **Claim**: `apply` only checks `outstanding` when `Some`; passing `None` skips drain entirely and still revokes/installs.
- **Why it matters**: A caller (or a future integration mistake) can run Freeze→Install while a permit is in flight, revoking the outgoing lease mid-mutation. That breaks ordering (3) by construction of the API, not by interleaving.
- **Recommended fix**: Track outstanding permits on `SourceRuntime` (or require `outstanding: &PermitDrainSignal` always). Refuse Install when `!signal.has_ended()` or `permits_issued` exceeds completed permits.

---

### MAJOR — TOCTOU between link check and write remains the real confinement gap

- **Where**: `src/live_index/index_lifecycle/physical_root.rs:150-168`, `physical_root.rs:235-260`
- **Claim**: `symlink_metadata` runs before `fs::write`/`fs::rename` with no handle-relative open; a component can be swapped to a symlink/reparse point in between.
- **Why it matters**: Slice 1's stated purpose is preventing cross-root writes. Documented deferral does not remove the window; an attacker (or race) can still redirect a permitted write outside the leased root after checks pass.
- **Recommended fix**: Accept as a carried-forward limit only if the contract allows it; otherwise slot in handle-relative I/O (`cap-std`/`openat`) before claiming confinement. At minimum add an oracle that simulates check-then-swap if feasible on Unix.

---

### MINOR — `SideEffectBeforeNonCurrentPublication` is dead code

- **Where**: `src/live_index/index_lifecycle/authority.rs:397-398`, `mutation.rs:180-201`
- **Claim**: The refusal variant is defined but never returned; `start_side_effect` relies entirely on "by construction."
- **Why it matters**: Future edits can construct permits through new paths and silently lose the ordering guarantee while the enum still suggests a runtime check exists.
- **Recommended fix**: Either remove the variant or enforce it in `start_side_effect` against a runtime snapshot (phase + stored publication identity), not a tautological epoch compare.

---

### MINOR — `freeze` on non-`Current` phases advances epoch without publishing

- **Where**: `src/live_index/index_lifecycle/authority.rs:535-546`
- **Claim**: For `Loading`/`Blocked`/`Stopping`, `freeze` bumps `mutation_epoch` then returns without moving phase to `Refreshing`.
- **Why it matters**: `transition::apply` always calls `freeze`; a mis-phase caller gets a silent epoch advance with no non-`Current` publication recorded, weakening epoch monotonicity semantics the sweep guards.
- **Recommended fix**: Return `Err` from `freeze` on non-freezable phases, or set an explicit non-`Current` phase before returning.

---

### NIT — stale comment on stacked `#[cfg(test)]` attributes

- **Where**: `src/live_index/store.rs:3579-3585`
- **Claim**: Comment still says census only recognises literal `#[cfg(test)]`, but A21 `cfgPredicateIsTestOnly` handles `all(test, feature = "server")`.
- **Why it matters**: Misleads the next editor into unnecessary attribute stacking.
- **Recommended fix**: Update comment to reference A21 predicate handling.

---

## Author-confidence verdicts

**Census granularity (83 atoms):** Defensible as the contract's own rule — `derivePublicApiAtoms` counts top-level `pub mod` in `lib.rs` only (`scripts/validate-lifecycle-oracle-traceability.cjs:1996-1999`). Nested `symforge::live_index::index_lifecycle::*` is reachable but uncounted. That is coarse, not a slice-invented loophole; if the intent is to freeze total reachability, amend the census, not this module placement.

**Deleted epoch guard / "by construction":** True for grant→permit publication ordering (`request_mutation_grant` calls `freeze` before returning the grant; `NonCurrentPublicationProof` fields are private with no public constructor). False for side-effect *completion*: `commit` does not observe which root was written (BLOCKER above).

**TOCTOU:** Not closed; documented carry-forward is honest, but the slice does not fully deliver "mutation authorized for root A cannot reach root B" against a racing symlink.

**`SourceRuntime` as toy model:** Oracles are not vacuous for the shipped types under `symforge::live_index::index_lifecycle` — they prove real API rules consumers can import today. They do not prove production watcher/daemon paths enforce those rules (explicitly Slice 4 / out of scope).

**`SYMFORGE_LIFECYCLE_EMIT_CLOSURE=1`:** Does not weaken the gate; comparison still fails on mismatch and self-test cases unchanged. Read-only aid for operators.

---

## Negatives

**Ordering 1 (publish non-`Current` before grant):** Sound in `request_mutation_grant` — `freeze()` at `authority.rs:605-606` precedes grant construction; tests assert `Refreshing` and `live_publication() == None` before permit creation (`granting_publishes_non_current_before_the_permit_exists`).

**Ordering 2 (temp before replace):** Sound in `replace_beneath` — `TempCreated` step is pushed before `rename` (`physical_root.rs:254-260`); tested.

**Ordering 3 (revoke before install):** Sound when drain preconditions are met — `outgoing.revoke()` at `transition.rs:90` precedes `runtime.install` at line 91; tested in `a_root_a_permit_cannot_write_after_root_b_is_installed`.

**Grant refusal leaves no trace:** Verified — early returns in `request_mutation_grant` do not touch epoch/permits/phase; tests assert on every refusal path.

**Pairing negative+positive:** Consistently applied across both test files; not a blanket-refusal pattern.

**Watcher T028 fence sampling:** `effective_fence_generation` reads one `Arc<PublishedGeneration>` (`watcher/mod.rs:266-282`); removes two-sample root/generation split. Store under-lock check unchanged.

**Mutation sweep:** Script correctly prioritises observed `FAILED` over `error: test failed`, refuses dirty trees, restores on exit. Ten guards listed; phase/provenance gates correctly excluded as non-expressible one-line reverts.

**Permit terminality / drop drain / lease revoke / root pairing / publication identity / proof-names-stored / epoch monotonic:** Exercised by sweep mapping in evidence doc; no interleaving found that breaks these without the BLOCKER/MAJOR gaps above.

**`SYMFORGE_LIFECYCLE_EMIT_CLOSURE`:** Checked — emit is stdout-only under env flag; no bypass of digest comparison.

**No BLOCKER found** in retirement census normalizer changes for this slice (single surgical category move per evidence doc).

**No finding** on missing first-run RED (recorded out of scope).

**No finding** on Windows reparse lack of test (recorded out of scope).

**No finding** on watcher not using `MutationAuthority` yet (recorded out of scope).

**No finding** on `batch_rename_health_dry_run_stays_under_h7_budget` flakiness — no evidence of regression from this branch.
