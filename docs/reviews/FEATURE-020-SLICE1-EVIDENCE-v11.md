# Feature 020 V11 — Slice 1 evidence (T029)

**Scope**: Phase 3, tasks T022–T029 — atomic mutation authority.
**Branch**: `feature-020-slice-1-mutation-authority`.
**Predecessor**: Slice 0 (`a5d65d6e`), whose causal oracles this slice begins to answer.

Volatile state (SHAs, PR numbers, CI results) is deliberately absent per the
documentation-hygiene rule; regenerate with `pwsh scripts/campaign-state.ps1`.

## What Slice 1 establishes

A mutation is authorized by **one whole consumed authority** or it is refused.
Nothing in the slice samples two fields and infers permission from the pair —
that inference is the defect shape Slice 0's T014 oracle describes and the shape
this slice removes at the watcher seam (T028).

Three orderings are load-bearing, and each is enforced rather than documented:

1. **Publish non-`Current` before the grant exists.** `request_mutation_grant`
   advances the epoch and republishes the source as `Refreshing` *before*
   constructing the grant, so no holder of a grant can act while the source is
   still queryable.
2. **Temp before replace.** Destructive replacement writes its temporary beneath
   the target's own resolved parent and only then renames, so a target is never
   removed or truncated before its replacement exists.
3. **Revoke before install.** `Freeze -> Drain -> Install` revokes the outgoing
   root lease before the incoming binding is live, so a permit that survived from
   the previous root can no longer resolve a path beneath it.

A refused grant leaves **no trace**: phase, mutation epoch, and permit count are
all unchanged. This is asserted explicitly in every refusal case, because the
failure mode being prevented is a later step mistaking a rejection for permission.

## RED→GREEN: what was actually observed

**Stated plainly: a first-run RED was not observed for this slice.** The
implementation and its oracles were written together, so the first execution of
`tests/project_index_authority_v11.rs` and `tests/physical_root_lease_v11.rs`
was already GREEN (13 tests). That is a deviation from the ordering the task
roster prescribes, and it is recorded here rather than presented as compliance.

A first-run RED would in any case have proven only "the code is not written yet".
The property that matters — *is each guard load-bearing?* — is established
instead by mutation: each guard is reverted one at a time and the sweep records
which tests fail. A guard whose removal breaks nothing is not a guard.

The sweep is committed as `scripts/slice1-mutation-sweep.sh` so the result is
reproducible rather than a claim in prose.

**The sweep's own first run was wrong, and the way it was wrong is the point.**
It classified a run as a compile failure whenever the output contained a leading
`error:` — but `cargo test` prints `error: test failed` on a *failing test run*.
So four guards that were caught exactly as intended were reported as
"prevented by the type system", a conclusion the script never observed. It is the
same defect this repository keeps producing: the thing that reports is not the
thing that knows. Observed test failures now outrank every inference, and a
genuine build failure is identified by `could not compile` rather than by the
word `error`.

A second correction followed: two mutations left a function parameter unused,
and this crate denies warnings, so the build failed for a reason unrelated to
the guard — again reading as "uncompilable" while proving nothing. Those
mutations now keep their binding in use (`if cond && false`, `let _ = &x`) so
that what changes is behaviour and only behaviour.

### Guard mutation results

| Guard | Reverted to | Caught by |
|---|---|---|
| grant validates the exact live publication identity | `if false` | _pending_ |
| permit pairs a grant only with its own root lease | `if false` | _pending_ |
| transition refuses to install over a live permit | `if false` | _pending_ |
| install revokes the outgoing root lease | statement deleted | _pending_ |
| a terminal permit refuses a second termination | `if false` | _pending_ |
| a dropped permit reports `Drained` | statement deleted | _pending_ |
| a revoked lease resolves nothing | `if false` | _pending_ |
| replacement creates its temporary before replacing | step reordered | _pending_ |

### Guards not covered by the sweep

The phase gate (`PhaseNotCurrent`) and the provenance gate
(`ProvenanceNotLiveCurrent`) are not mutated: removing either produces a
borrow/type error rather than a passing build, so the sweep cannot express them
as a one-line revert. Their non-vacuity rests instead on the pairing rule below.

## Why no oracle in this slice can pass vacuously

Every rejection case asserts the **accepting** path in the same test. Slice 0
produced three controls that passed for reasons unrelated to the property under
test, and the adversarial reviews of that slice found a fourth
(`rebuild_failed` accepting any failure body). A guard that refuses everything
satisfies a lone negative assertion perfectly.

So `grant_provenance_matrix_accepts_only_a_live_current_publication` refuses
candidate, snapshot, retained-generation and stale-publication inputs **and then
grants from the live publication on the same source at the same instant**. The
refusal is therefore about what was presented, not about the source being
unable to grant at all.

## T028 — the watcher seam

`effective_fence_generation` previously read `current_project_generation()` and
then `read().indexed_root` as two independent loads of the published generation.
A reload landing between them pairs one publication's generation with another
publication's root. Its own doc comment conceded the result was "only a *better
guess*".

Both values now come from a single `Arc<PublishedGeneration>`, so the generation
and the root that publication actually served cannot disagree. The store's
under-lock re-check is unchanged and remains the commit-time gate.

**Scope limit, stated rather than implied**: T028 removes the two-sample
inference; it does **not** yet route the live watcher through
`MutationAuthority` itself. The runtime that issues real permits arrives with
Slice 4 activation, and T023 already records that production writer integration
is Slice 4 work. Binding the watcher to a permit type whose runtime does not
exist would be scaffolding, not prevention.

## Known limits carried forward

- **TOCTOU in root confinement.** Components are checked with
  `symlink_metadata` before the open, so a link swapped in between check and
  open is not excluded. Closing this needs handle-relative I/O (`openat`, or
  `FILE_FLAG_OPEN_REPARSE_POINT` on Windows) — in practice a `cap-std`-style
  `Dir` handle, which is a dependency decision deliberately not taken inside
  this slice. `resolve_beneath` already returns the final parent plus leaf name,
  which is the shape that upgrade slots into.
- **Symlink refusal is verified on Unix only.** Symlink creation needs
  privileges on Windows, so `a_link_component_is_refused_rather_than_followed`
  is `#[cfg(unix)]`. CI runs ubuntu, so the property is covered there; the
  Windows path relies on the same `metadata_is_reparse_point` branch without a
  test that creates a reparse point.
- **`SourceRuntime` is a Slice 1 model, not the registry.** It owns one source's
  phase, epoch and permit record so the authority rules can be stated and tested
  now. The registry-owned `ArcSwap<ProjectRuntimePublication>` that the
  data model describes is Slice 4.
