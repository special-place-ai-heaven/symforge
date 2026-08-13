# Review request — Feature 020 V11, Slice 2 (registry tombstones, process capacity)

## Instructions to the reviewing model

**Keep verbosity low. Do not explain the codebase back to me. Produce one file
and nothing else.**

Write your findings to:

```
docs/reviews/REVIEW-FINDINGS-<your-model-name>-feature-020-slice-2-2026-08-13.md
```

Use your own model name in the filename (e.g. `grok-4-6`, `composer`, `kimi-k3`)
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

## What Slice 2 is

Branch `feature-020-slice-2-registry-capacity`. Phase 4 of Feature 020 V11
(tasks T030–T040). It gives a stopped project slot a tombstone that actually
refuses, and makes index capacity a single process-wide account rather than a
per-load guess.

Read first: `docs/reviews/FEATURE-020-SLICE2-EVIDENCE-v11.md`. It states what was
observed, what was not, and which limits are carried forward rather than closed.

Code under review:

- `src/index_lifecycle/authority.rs` — widened to the frozen `SourceRuntimeState`
  shape: observer phase, plural active permits, revocation residency, and the
  A20 queryability rule
- `src/index_lifecycle/registry.rs` — single-flight admission, enforced tombstones
- `src/index_lifecycle/capacity.rs` — ledger, non-`Clone` grant, refund on `Drop`
- `src/index_lifecycle/process_runtime.rs` — one capacity domain across four surfaces
- `src/index_lifecycle/embedded.rs` — sole handle, close/drop coalescing, self-wait refusal
- `src/index_lifecycle/adapters.rs` — admission planning separated from execution
- `src/index_lifecycle/physical_root.rs` — `cap-std` capability, staged replacement
- `tests/project_index_authority_v11.rs`, `tests/project_registry_lifecycle_v11.rs`,
  `tests/process_capacity_pool_v11.rs`, `tests/physical_root_lease_v11.rs`,
  `tests/embed_lifecycle_v11.rs`

## The property to attack hardest

**Can any holder still act with the authority of a stopped project slot?**

Rust cannot take an `Arc` back. The slice's answer is to make the handle useless
instead of unreachable: `LiveProjectSlot::binding()` returns
`Err(RegistryRefusal::Tombstoned)` once the slot is not live, and `stop()` revokes
before it records the tombstone. Find the path that bypasses that — a field read,
a cached clone taken before the stop, a method that reaches the binding without
going through `binding()`, an ordering where the tombstone is visible but the
revocation is not.

Attack the orderings, not the naming. A concrete interleaving is worth more than
any number of style observations.

## Where the author is least confident

State a verdict on each of these specifically.

1. **`capacity.rs` never blocks.** The argument is that the loader already waits
   on a condvar inside the shared rayon pool, so a process-wide *blocking* pool
   layered over it deadlocks: a worker parked waiting for capacity holds a pool
   thread that the grant it waits for may need. The invariant kept instead is
   "the leaf keeps its own per-load budget". Is that sound, or does it mean
   process capacity is accounted but never actually enforced?
2. **Conservation is defined against physical `Drop`,** not against `release`.
   `release` takes `self` by value so a double refund is unrepresentable, and a
   forgotten release still refunds. Show a sequence that leaks a charge, refunds
   twice, or refunds into the wrong ledger. `unknown_refunds` is meant to catch
   the last case — prove it can be fooled.
3. **`#[path]` module placement.** The files live at `src/index_lifecycle/`
   because every frozen seam names that directory; the module path stays
   `symforge::live_index::index_lifecycle` because a top-level `pub mod` would add
   an atom to a public API census frozen for the whole preactivation period, and
   `introduced_v11_atoms` never names a public lifecycle module. Is that a
   legitimate reading of two contracts that disagree, or is it the frozen public
   surface being evaded a second time? Answer directly.
4. **Amendment A20 — queryability closes on completeness, not recency.** A
   `Refreshing` source keeps serving the complete generation it retains, because
   that generation was `Current` immediately before the refresh; `Blocked` and
   `Stopping` retentions stay unservable because neither has a successor in
   flight. Is there a state where a retained generation is complete and yet must
   NOT be served? If yes, A20 is wrong and the slice ships an availability hole.
5. **The temp-before-replace oracle.** The previous version proved a receipt
   label, not I/O order: a build that renamed first while pushing labels in order
   passed it. Replacement is now staged and committed in two steps so an oracle
   observes on disk that the temporary exists while the target still holds its
   original bytes. Is the new oracle observing disk, or has the label simply
   moved?
6. **Slice 2 is dark by design** — no production code calls it. Do the 45 oracles
   prove properties of anything that will ship, or of a model that will be
   rewritten at Slice 4 when `SourceRuntime` stops being `&mut self` and goes
   under an `ArcSwap` publication root?
7. **The planned RED stub.** `whole_runtime_capacity_is_conserved_under_activation`
   exists only as an `#[ignore]`d panic, because the checker requires every
   `planned_exact` case declared for a file to exist once the file exists. Is that
   honest materialization, or does it create a path where a Slice-4 oracle can be
   receipted without ever running?

## Known and out of scope

Do not spend findings on these; they are recorded already.

- `process_runtime.rs` and `adapters.rs` have no dedicated oracle file; they are
  exercised through the registry and T039 proofs. Whether that is *sufficient*
  coverage IS in scope; noting the absence is not.
- Symlink refusal is verified on Unix only (`#[cfg(unix)]`); Windows reparse
  points share the branch but have no test that creates one.
- No production seam calls this slice. Every candidate seam was refuted
  concretely in the evidence document, and T051 is scheduled to prove the slice
  is dark. Proposing a seam without addressing those refutations is not a finding.

## Errors already found and fixed this slice

Look for siblings of these rather than rediscovering them.

- `StagedReplacement::commit` accepting any lease's receipt rather than its own.
- A drain that was optional, so a caller could skip it and still look drained.
- A deterministic temporary path that let another process pre-create the name.
- Two amendment regressions bound to acceptance oracles that do not exist, in a
  section whose own first sentence says each binds an existing one.
- Oracles leasing the machine's shared temp directory, so another user's leftover
  turned a property failure into an environment failure.

The recurring shape in this repository is **a component reporting something it
did not observe**. Findings of that shape are the most valuable thing you can
return.
