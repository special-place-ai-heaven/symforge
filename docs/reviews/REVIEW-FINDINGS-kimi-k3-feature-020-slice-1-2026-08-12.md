# Review findings — kimi-k3 — Feature 020 V11, Slice 1

Scope: `authority.rs`, `physical_root.rs`, `mutation.rs`, `transition.rs`,
`watcher/mod.rs` (`effective_fence_generation` only), both slice test files,
`scripts/slice1-mutation-sweep.sh`, and the two digest-gate regions of
`scripts/validate-lifecycle-oracle-traceability.cjs`.

---

### MAJOR — A permit can act while the source is queryable again: `install` re-publishes `Current` with no permit check
- **Where**: `src/live_index/index_lifecycle/authority.rs:558` (`pub fn install`), consumed by nothing except `transition.rs:91`; `src/live_index/index_lifecycle/mutation.rs:180-201` (`start_side_effect`)
- **Claim**: The interleaving `request_mutation_grant` → `SourceRuntime::install(CurrentPublication::promote(..))` → `permit.start_side_effect()` → write succeeds, leaving a source-disk side effect executing while the source is `Current` and queryable.
- **Why it matters**: This is the exact property the slice asks reviewers to attack ("no holder of a grant can act while the source is still queryable"). Ordering 1 is enforced only at grant time; the proof attests to a *past* publication. `start_side_effect` re-checks lease liveness but never the source's phase or epoch, and `install` is a `pub` method that bypasses the drain gate in `transition::apply`. One line of caller code re-opens the queryable window the freeze was supposed to close.
- **Recommended fix**: Make `install` unreachable except through the drained path (`pub(crate)` plus keeping `transition::apply` as the only caller is enough today), and have `start_side_effect`/`commit` re-validate against the live `SourceRuntime` — epoch equality and still-non-`Current` phase — rather than treating the construction-time proof as a present-tense guarantee.

### MAJOR — `commit` reports `Committed` for a write it never authorized: the receipt is not bound to the pinned root
- **Where**: `src/live_index/index_lifecycle/mutation.rs:205-211`; `src/live_index/index_lifecycle/physical_root.rs:227` (`replace_beneath` takes any lease)
- **Claim**: `permit.commit(receipt)` accepts a `WriteReceipt` produced by `replace_beneath` through *any* lease; nothing compares `receipt.target()` against `permit.lease().root()`, so a permit pinned to root A can report `Committed` for a write that landed under root B.
- **Why it matters**: The house failure mode — a component reporting something it did not observe. The grant↔lease pairing check in `SourceMutationPermit::grant` (`mutation.rs:136-138`) validates the permit's creation, not the side effect; the actual write path is a free function the caller can feed a different lease. "Under a root the authority does not name" is therefore reachable at the type level, not just by adversarial callers.
- **Recommended fix**: Move the destructive write onto the permit (`permit.replace_beneath(relative, contents)` using `self.lease`), so the lease that writes is the pinned one by construction; or minimally, in `commit`, require `receipt.target().starts_with(self.lease().root())` and `self.lease().is_live()`.

### MAJOR — The temporary's predictable name plus `fs::write` makes the cross-root escape deterministic, not a race
- **Where**: `src/live_index/index_lifecycle/physical_root.rs:239-250`
- **Claim**: The temp path is `<leaf>.symforge-tmp-<pid>` — fully predictable — and is never passed through `refuse_link`; `std::fs::write` follows symlinks, so a link planted at that path inside root A at any earlier time makes the replacement write outside root A with no TOCTOU window to win.
- **Why it matters**: The module's stated purpose is "a mutation authorized for root A can never reach root B through a link planted inside A." The documented symlink-metadata TOCTOU is a race; this is not — the plant can precede the mutation by days, and symforge becomes the confused deputy that writes outside the root. This is the defect class the slice exists to close, and it survives the documented-upgrade path (handle-relative I/O) only if the temp is also created handle-relative with no-follow semantics.
- **Recommended fix**: Create the temp with `OpenOptions::new().write(true).create_new(true)` (fails if anything, including a link, already occupies the name) plus an unpredictable component (counter or random suffix), and treat `AlreadyExists` as retry-with-new-name. One line of `refuse_link` on the temp path is not sufficient — it reintroduces the race.

### MINOR — `freeze` from `Loading`/`Blocked`/`Stopping` returns a publication identity nothing stored
- **Where**: `src/live_index/index_lifecycle/authority.rs:535-550`, specifically the early return at line 545
- **Claim**: For the three phases with no stored publication, `freeze` mints a fresh `PublicationIdentity`, stores nothing, and returns the fresh identity via `.unwrap_or(publication)` — contradicting its own contract ("the identity of the publication actually stored").
- **Why it matters**: This is the same defect shape this slice already fixed once ("a proof naming a publication identity nothing had stored"). No current caller is misled — `request_mutation_grant` reaches `freeze` only from `Current`, and `transition::apply` discards the return — but the method is `pub` and one caller away from attesting to a publication that does not exist.
- **Recommended fix**: Return `Option<PublicationIdentity>` (`None` when the phase stored nothing), or record the identity in the phase so the claim is true.

### MINOR — A refused transition is not traceless: Freeze has already happened when `OutstandingPermit` is returned
- **Where**: `src/live_index/index_lifecycle/transition.rs:77-86`
- **Claim**: `apply` freezes (publishes non-`Current`, advances the epoch) before checking the drain signal, so an `Err(OutstandingPermit)` return conceals a real state change with no receipt.
- **Why it matters**: The slice's own discipline — "a refused request leaves no trace that a later step could mistake for permission" — is applied to grants but not to transitions. A caller that retries on `Err` is now operating on a source whose phase and epoch moved underneath it, and the receipt that would have recorded the Freeze is never returned.
- **Recommended fix**: Check `outstanding` before `freeze()`. The check is a pure read and weakens nothing: an outstanding permit implies the source already published non-`Current` when its grant was issued, so freeze-first buys no additional safety on the refusal path.

### MINOR — `permits_issued` counts grants, not permits
- **Where**: `src/live_index/index_lifecycle/authority.rs:607`
- **Claim**: The counter increments when the grant is constructed; a grant that is dropped without ever reaching `SourceMutationPermit::grant` is counted as an issued permit.
- **Why it matters**: Another report-not-observed: the doc says "how many permits this source has ever issued," and tests assert on it as exactly that. A dropped grant inflates the record the drain logic and future Slice 4 registry will read.
- **Recommended fix**: Rename to `grants_issued`, or move the increment into `SourceMutationPermit::grant` (requires the source to learn of the conversion — the rename is cheaper and honest).

### MINOR — `NoSideEffectProof::observed()` is a public constructor; the proof attests to nothing
- **Where**: `src/live_index/index_lifecycle/mutation.rs:76-88`
- **Claim**: The doc says "Constructible only by the lane that actually observed the absence," but `observed()` is `pub` and any caller anywhere can fabricate the proof.
- **Why it matters**: The sibling proof (`NonCurrentPublicationProof`) gets its force from private fields and a single construction site; this one has neither, so the `no_side_effect(proof)` parameter is ceremony — it cannot distinguish an observed absence from an asserted one. It reads as the same guarantee and is not.
- **Recommended fix**: Either make construction `pub(crate)` and route it through the write-lane machinery when Slice 4 exists, or drop the parameter and say plainly that `no_side_effect` is the holder's own declaration.

### NIT — Temp file leaks on crash and collides under same-process concurrency
- **Where**: `src/live_index/index_lifecycle/physical_root.rs:241`
- **Claim**: A crash between `TempCreated` and `Replaced` leaves `*.symforge-tmp-<pid>` behind forever, and two concurrent replacements of the same target in one process share one temp name.
- **Why it matters**: Small, but the project principle "long-running operations must be resumable" and the startup-sweep discipline elsewhere both argue for naming the cleanup owner.
- **Recommended fix**: Note the orphan in the module doc and let the startup temp sweep (or the Slice 4 runtime) own cleanup; add a per-process atomic counter to the temp name to kill the collision.

### NIT — The sweep can still report a caught guard as "NO TEST FAILED"
- **Where**: `scripts/slice1-mutation-sweep.sh:100`
- **Claim**: A harness-level abort (panic outside a test body, or `error: test failed` with no per-test `... FAILED` lines) matches neither the FAILED-line pattern nor the `could not compile|^error\[E` pattern, so it falls through to "*** NO TEST FAILED *** guard is not covered".
- **Why it matters**: The failure direction is conservative (false alarm, not false pass), so this is not the first-run bug recurring; but it is once again the reporter emitting a conclusion ("guard is not covered") it did not observe.
- **Recommended fix**: Treat a nonzero `cargo test` exit with no parsed failures as its own verdict (`INCONCLUSIVE — harness failed without per-test results`) instead of the uncovered-guard verdict.

### NIT — `start_side_effect` reports `PermitAlreadyTerminal` for the `InFlight` state
- **Where**: `src/live_index/index_lifecycle/mutation.rs:183`
- **Claim**: A second `start_side_effect` on an in-flight (not terminal) permit is refused with `PermitAlreadyTerminal`.
- **Why it matters**: The refusal is correct; the reason names a state the permit is not in — a small version of reporting what was not observed, in the variant enum consumers will match on.
- **Recommended fix**: Add `SideEffectAlreadyInFlight` (or reuse one `InvalidPermitState { actual }` carrying the real state).

---

## Verdicts on the five named questions

1. **Census granularity.** Defensible, with one caveat I verified rather than assumed: `derivePublicApiAtoms` (`validate-lifecycle-oracle-traceability.cjs:1996-1999`) counts only top-level `pub mod` in `lib.rs`, so nesting under `live_index` is the contract's own granularity, not an invented loophole. The caveat: `pub mod index_lifecycle` (`src/live_index/mod.rs:11`) is forced because the oracles are integration tests in `tests/` — external crates that cannot see `pub(crate)`. So reachability is test-driven, not consumer-driven. If the frozen surface is meant to freeze total reachability, the census is what must change; this slice's placement is compliant with the contract as written. Not a finding against the slice.
2. **Deleted epoch guard.** "By construction" is true for proof *provenance*: `NonCurrentPublicationProof` has private fields and exactly one construction site, inside `request_mutation_grant` after `freeze` — I found no other path that yields a grant or a proof. But the construction guarantees a *past* publication, and the first MAJOR above shows the permit never re-validates at act time, so the property the deleted guard gestured at (no side effect while queryable) is not currently delivered by construction either. Deleting the vacuous comparison was correct; the replacement claim overstates what is enforced.
3. **TOCTOU in root confinement.** The documented check-then-open window is the *lesser* residual. The unnamed one — predictable temp path, `fs::write` following a pre-planted link, no race required (third MAJOR) — is deterministic and sits squarely in the defect class the slice exists to close. As documented, the TOCTOU deferral would have been arguable; with the temp path unhandled, the confinement claim ("can never reach root B through a link planted inside A") is false today. Fix the temp creation; then the documented window is a defensible residual with a named upgrade path.
4. **`SourceRuntime` as a slice-local model.** The oracles are not vacuous for the model: every refusal pairs an acceptance on the same source at the same instant, and the sweep demonstrates each guard is load-bearing. They do not bind anything that ships — the model is `&mut self` single-threaded, and the Slice 4 registry (`ArcSwap`-published, concurrent) will need the ordering claims re-proven under shared access. More pointedly: the model itself does not yet enforce its headline property (first MAJOR), so the vacuity question is premature — close the model gap before asking whether the oracles transfer.
5. **Digest emit affordance.** No weakening demonstrable. I read both sites (`validate-lifecycle-oracle-traceability.cjs:2498-2500` and `2570-2572`): the emit is stdout-only, gated on the env var, and both comparisons (`record.digest !== actualDigest`, `actual !== spec.hash`) run unconditionally afterward. The gate's strength was never secrecy of the digest — it is review of the diff to the frozen record, which the affordance does not touch.

## Negatives

Checked and found sound:

- **Ordering 1 as coded**: `request_mutation_grant` (`authority.rs:583-618`) clones the live publication, validates provenance and exact identity, and calls `freeze()` *before* constructing the grant; refusals return before any mutation of phase, epoch, or permit count, and the tests assert exactly that. Verified in source, not just via the sweep table.
- **Ordering 2 as coded**: `replace_beneath` resolves beneath the lease, refuses a link leaf (`physical_root.rs:236`), writes the temp beneath the target's own resolved parent, then renames; the receipt records steps as they happen. The gap is the temp path (above), not the ordering.
- **Ordering 3 as coded**: `transition::apply` revokes `outgoing` (line 90) before `runtime.install` (line 91); a permit pinned to the revoked lease fails `is_live` at both `grant` and `start_side_effect`. Verified.
- **Epoch monotonicity**: `freeze`/`install` mutate in place; `transition::apply` no longer constructs a fresh `SourceRuntime`; `the_mutation_epoch_never_rewinds_across_a_transition` covers it. No rewind path found.
- **Grant consumption**: `CurrentMutationGrantAuthority` is non-cloneable, consumed by move in `into_parts`; one grant → at most one permit. `SourceMutationPermit` is not `Clone`.
- **Permit terminality and drop**: `finish` sets terminal before recording; `PermitDrainSignal::record` is first-write-wins; `Drop` records `Drained` only from non-terminal states. Double-termination and stranded-source paths are both covered by tests.
- **T028 watcher seam**: `effective_fence_generation` (`watcher/mod.rs:261-281`) loads one `Arc<PublishedGeneration>` and reads both `project_generation` and `live.indexed_root` from it. I traced the publication paths (`store.rs:2178-2193`, `2243-2259`, `3285-3311`): the store is copy-on-write (`write()` clones, `reload` builds a fresh `LiveIndex` then `store`s it), so the `Arc<LiveIndex>` inside an older publication is frozen and the generation/root pair cannot be split by a later reload. The residual commit-time gate under the write lock is intact.
- **Sweep restore safety**: refuses to run with uncommitted changes in the four mutated files, restores via `git checkout` under a `trap EXIT`, and asserts needle uniqueness before replacing. The destroy-uncommitted-work defect is genuinely closed for the pre-run case.
- **Symlink/refusal oracle pairing**: every refusal test I read also exercises the accepting path on the same fixture; no lone negative assertions in either test file.
- **`refreshing()` constructor**: the identity it mints *is* stored in the phase, so `published_identity()` reporting it is accurate — not the phantom-publication shape.
- **`start_side_effect` lease liveness**: re-checked at act time (mutation.rs:197-199), so lease revocation between grant and act is caught. (Phase is the unchecked half — first MAJOR.)
