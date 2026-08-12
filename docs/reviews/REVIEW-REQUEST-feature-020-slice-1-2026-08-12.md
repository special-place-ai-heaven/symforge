# Review request — Feature 020 V11, Slice 1 (atomic mutation authority)

## Instructions to the reviewing model

**Keep verbosity low. Do not explain the codebase back to me. Produce one file
and nothing else.**

Write your findings to:

```
docs/reviews/REVIEW-FINDINGS-<your-model-name>-feature-020-slice-1-2026-08-12.md
```

Use your own model name in the filename (e.g. `grok-4-5`, `composer`, `kimi-k3`)
so several independent reviews can sit side by side and be diffed.

Every finding uses exactly this shape:

```
### <BLOCKER|MAJOR|MINOR|NIT> — <one-line title>
- **Where**: path:line
- **Claim**: one sentence, falsifiable.
- **Why it matters**: one or two sentences.
- **Recommended fix**: concrete, minimal.
```

End the file with a `## Negatives` section listing, explicitly, the things you
checked and found sound. A silent omission is indistinguishable from not having
looked, which is why this section is mandatory.

If you find nothing at a severity, say so. Do not pad.

## What Slice 1 is

Branch `feature-020-slice-1-mutation-authority`, PR #560. Phase 3 of Feature 020
V11 (tasks T022–T029). It makes cross-root mutation and publication impossible
before the larger lifecycle runtime exists.

Read first: `docs/reviews/FEATURE-020-SLICE1-EVIDENCE-v11.md`. It states what was
observed, what was not, and which limits are carried forward rather than closed.

Code under review:

- `src/live_index/index_lifecycle/authority.rs` — identities, sealed grant, source runtime
- `src/live_index/index_lifecycle/physical_root.rs` — owning lease, confinement, replacement
- `src/live_index/index_lifecycle/mutation.rs` — non-cloneable permits
- `src/live_index/index_lifecycle/transition.rs` — Freeze → Drain → Install
- `src/watcher/mod.rs` — `effective_fence_generation` only (T028)
- `tests/project_index_authority_v11.rs`, `tests/physical_root_lease_v11.rs`
- `scripts/slice1-mutation-sweep.sh`

## The property to attack hardest

**Can a source-disk side effect happen while the source is still queryable, or
under a root the authority does not name?**

Slice 1 claims three orderings make that impossible:

1. `request_mutation_grant` publishes non-`Current` and advances the epoch
   *before* the grant value exists.
2. `replace_beneath` writes its temporary beneath the target's own resolved
   parent *before* renaming over the target.
3. `transition::apply` revokes the outgoing lease *before* installing the
   incoming binding.

Attack the orderings, not the naming. A concrete interleaving that breaks one is
worth more than any number of style observations.

## Where the author is least confident

State a verdict on each of these specifically.

1. **The census granularity argument.** `derivePublicApiAtoms` counts only
   top-level `pub mod` in `lib.rs`. Slice 1 was moved under `live_index` to keep
   the frozen count at 83, which means its types are reachable by a consumer but
   uncounted. The evidence document argues this is the contract's own
   granularity. Is that defensible, or is it the frozen public surface being
   evaded? Answer directly.
2. **A deleted guard.** `start_side_effect` no longer compares the proof's epoch
   against the authority's, because both are assigned from one value and the
   comparison could never fail. The ordering is now claimed to be enforced by
   construction, since `NonCurrentPublicationProof` is constructible only inside
   the grant path. Is "by construction" actually true, or is there a path that
   produces a permit without that publication having happened?
3. **TOCTOU in root confinement.** Components are checked with
   `symlink_metadata` before the open. The window is documented and the upgrade
   path (handle-relative I/O) is named, but not taken. Is the residual risk
   acceptable for a slice whose entire purpose is preventing cross-root writes,
   or is this the defect the slice was meant to close?
4. **`SourceRuntime` is a slice-local model**, not the registry the data model
   describes. Does that make any of the 17 oracles vacuous — i.e. do they prove
   properties of a toy rather than of anything that will ship?
5. **The digest emit affordance.** Both frozen-digest gates now print the value
   they computed under `SYMFORGE_LIFECYCLE_EMIT_CLOSURE=1`. The comparison is
   untouched and the self-test still rejects 103 fail-closed cases. Does this
   weaken the gate in any way you can demonstrate?

## Known and out of scope

Do not spend findings on these; they are recorded already.

- No first-run RED was observed for this slice. The mutation sweep is the
  substitute evidence and the substitution is stated in the evidence document.
- Symlink refusal is verified on Unix only (`#[cfg(unix)]`); Windows reparse
  points share the branch but have no test that creates one.
- The watcher is not yet routed through `MutationAuthority`; that is Slice 4.
- `batch_rename_health_dry_run_stays_under_h7_budget` failed once locally under
  CPU contention. If you can show it is a real regression from this branch, that
  IS in scope; asserting flakiness without evidence is not.

## Errors already found and fixed this slice

Look for siblings of these rather than rediscovering them.

- A guard that could not fail (the epoch comparison above).
- A proof naming a publication identity nothing had stored.
- `transition::apply` rewinding the monotonic mutation epoch to zero on every
  reload and rebind.
- The mutation sweep reporting caught guards as "did not compile", because it
  matched a leading `error:` and `cargo test` prints one when tests fail.
- The sweep destroying uncommitted work through its own restore step.

The recurring shape in this repository is **a component reporting something it
did not observe**. Findings of that shape are the most valuable thing you can
return.
