# Review findings — Feature 020 V11 Slice 1

**Reviewer:** Cursor Grok 4.5  
**Date:** 2026-08-12  
**Branch:** `feature-020-slice-1-mutation-authority`

---

### MAJOR — `outstanding: Option` lets Install revoke while a permit is still live

- **Where**: `src/live_index/index_lifecycle/transition.rs:68`, `80-90`
- **Claim**: Passing `outstanding: None` skips Drain, so `outgoing.revoke()` + `install` run while a `SourceMutationPermit` can still be `InFlight` and finish a `replace_beneath` that already passed `resolve_beneath`'s live check.
- **Why it matters**: Ordering 3 (“revoke before install stops a surviving permit”) is caller convention, not construction. `CurrentMutationGrantAuthority` is sealed; Drain is not. That is a path to a disk write under a root the newly installed authority does not name.
- **Recommended fix**: Take `&PermitDrainSignal` (non-optional), or a sealed `Drained` proof type produced only by terminal permit paths. Do not accept `None`.

### MAJOR — Temp-before-replace is evidenced by receipt labels, not by observed I/O order

- **Where**: `src/live_index/index_lifecycle/physical_root.rs:254-260`; `tests/physical_root_lease_v11.rs:111-127`; `scripts/slice1-mutation-sweep.sh` (`temp-first` mutation)
- **Claim**: The oracle and the sweep only check that `WriteReceipt` pushes `TempCreated` then `Replaced`. The sweep mutates those pushes, not `write`/`rename` order.
- **Why it matters**: The load-bearing claim is disk order. A build that renames/truncates first but still pushes the two enum variants in the “right” order stays GREEN and the sweep still reports the guard caught. Receipt theater, not observation.
- **Recommended fix**: Assert mid-flight: after temp create and before replace, temp bytes exist and target still equals the preimage (hook, or split the function for the test). Point the sweep at an I/O-order mutation (e.g. swap the `write`/`rename` calls), not the `steps.push` lines.

### MAJOR — Module claims absolute link confinement while TOCTOU remains open

- **Where**: `src/live_index/index_lifecycle/physical_root.rs:3-14`, `150-169`, `230-260`
- **Claim**: Docs say a root-A mutation “can never reach root B through a link”, then document a check-then-open window that allows exactly that swap.
- **Why it matters**: For a slice whose purpose is preventing cross-root writes, residual TOCTOU is the defect class being deferred, not an unrelated limit. The absolute “never” is a fact the code does not observe.
- **Recommended fix**: Soften the module claim to “refuses link metadata at check time; not TOCTOU-closed”. Track handle-relative open (`openat` / reparse-aware create) as the Slice that actually closes cross-root I/O, or take `cap-std` now.

### MINOR — `freeze` can return a publication identity nothing stored

- **Where**: `src/live_index/index_lifecycle/authority.rs:535-546`
- **Claim**: On `Loading` / `Blocked` / `Stopping`, `freeze` advances the epoch and returns `published_identity().unwrap_or(publication)` where `publication` was never written into `self.phase`.
- **Why it matters**: Sibling of the fixed “proof naming a publication nobody stored”. `transition::apply` ignores the return today; any caller treating it as “stored identity” is lied to.
- **Recommended fix**: Return `Option<PublicationIdentity>` (None when nothing was published), or refuse `freeze` unless the phase can actually enter `Refreshing` with a stored id.

### MINOR — Transition `Err(OutstandingPermit)` still commits Freeze

- **Where**: `src/live_index/index_lifecycle/transition.rs:72-85`; `tests/project_index_authority_v11.rs:514-535`
- **Claim**: Drain failure returns `Err` after `runtime.freeze()` has already moved the source to `Refreshing` and advanced the epoch; the test only checks the lease was not revoked.
- **Why it matters**: Unlike grant refusal (“no trace”), transition refusal is a partial commit. Callers can read `Err` as “nothing changed”.
- **Recommended fix**: Document that Freeze is committed on this `Err`, assert phase/epoch in the oracle, or roll freeze into a transaction that rewinds on drain failure (only if rewind cannot revive a queryable lie).

---

## Author confidence verdicts

1. **Census granularity** — Defensible. `derivePublicApiAtoms` freezes top-level `pub mod` in `lib.rs` (plus embed). Nesting under `live_index` matches that contract; it is not cheating a reachability freeze that the checker never implemented. If intent is reachability, amend the census — not this slice’s placement.
2. **Deleted epoch guard** — “By construction” holds for `NonCurrentPublicationProof` / `CurrentMutationGrantAuthority`: private fields, only built in `request_mutation_grant` after `freeze`. No other construction path in-tree. The deleted comparison was vacuous; removing it was correct.
3. **TOCTOU** — Not acceptable as silent residue next to a “never” claim. See MAJOR above: this is the cross-root defect class, deferred.
4. **`SourceRuntime` toy** — The 17 oracles are not vacuous for the authority rules they name (ordering, pairing, refusal+accept on one source). They do not prove production writers/registry use them; that limit is already stated (Slice 4). T028 is the only shipped seam and it is real.
5. **`SYMFORGE_LIFECYCLE_EMIT_CLOSURE=1`** — No weakening demonstrated: emit is opt-in stdout only; comparisons and the 103 fail-closed self-tests still gate. Safe affordance.

---

## Negatives

- **Ordering 1 (publish non-Current before grant):** `request_mutation_grant` calls `freeze()` before constructing `CurrentMutationGrantAuthority`. `granting_publishes_non_current_before_the_permit_exists` observes `Refreshing` / no live publication while holding the grant. Sound.
- **Ordering 2 (temp path before rename) in the happy-path implementation:** `fs::write(temp)` precedes `fs::rename(temp, target)` in `replace_beneath`. Sound as written; evidence that the guard is load-bearing is what fails (MAJOR above).
- **Ordering 3 when Drain is actually supplied:** With `Some(&drain)` and a live permit, Install does not run and the outgoing lease stays live (`a_transition_refuses_to_install_over_a_live_permit`). Sound under that calling convention.
- **Grant refusal leaves no trace:** Phase, epoch, and `permits_issued` unchanged on provenance/identity/phase refusal paths; matrix pairs refuse-then-accept. Sound.
- **Root pairing / revoked lease / terminal permit / drop→Drained / monotonic epoch across transition:** Matching oracles assert both negative and positive; sweep mutations for these target real control flow (not only labels), except `temp-first`.
- **T028 single-publication fence:** `effective_fence_generation` reads one `published_generation()` Arc for both generation and `indexed_root`. Removes the two-sample inference. Sound for what T028 claims; watcher-through-`MutationAuthority` correctly out of scope.
- **No BLOCKER found** beyond the MAJOR holes above; nothing here is a silent production writer bypass yet because production permit consumers are Slice 4.
- **EMIT_CLOSURE / census placement / deleted vacuous epoch check:** See verdicts; no additional findings.
- **Known-out-of-scope items** (no first-run RED, Unix-only symlink test, watcher not on `MutationAuthority`, H7 flake without evidence): not re-litigated.
