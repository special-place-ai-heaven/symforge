# Quickstart — Feature 020 Slice 5 (Mechanical Removal)

How to run and validate this slice end to end. This is a run guide, not an
implementation guide: it says what to execute and what the result must look
like. The rules behind each step are in
[`contracts/neutrality-bracket-v1.md`](contracts/neutrality-bracket-v1.md).

## Prerequisites

- A clean checkout at the current release ref, `git status` empty.
- Long cargo runs go through Terminal Commander, one cargo invocation at a
  time (Constitution IV). The full suite is ~25–40 minutes cold; the Bash tool
  will kill it mid-write at ten minutes and corrupt `target/`.
- Node and Python available for the checkers.

## Step 1 — Observe the lifecycle phase before anything else

The traceability checker prints counts, **not the phase**, so it cannot fill
`lifecycle_phase` on its own. Run both:

```
node scripts/validate-lifecycle-oracle-traceability.cjs
node scripts/lifecycle-phase-probe.cjs
```

**Expected**: `OK (…)` from the first; from the second, a line naming the phase
and the three set sizes. At the time of writing that is
`PHASE: postactivation` with `actual: 34  pre: 83  post: 34`.

Paste the probe's stdout into `commands.lifecycle_phase`. Per research R1, a
`postactivation` tree means **no 3-segment public atom may be removed at all**,
which bounds the entire slice — so this field decides the slice's scope and may
not be inherited from a document. Observe it on the tree you are working on.

## Step 2 — Capture the baseline (T074)

Run every gate in the set and record each result with the command that
produced it, into `docs/reviews/FEATURE-020-SLICE5-BASELINE-v11.md`:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib --bins --tests -- --test-threads=1
cargo test --no-default-features --features embed --lib -- --test-threads=1
cargo build --release
node scripts/verify-tools.cjs --bin target/release/symforge
node scripts/validate-lifecycle-oracle-traceability.cjs
node scripts/lifecycle-phase-probe.cjs
python execution/refreeze_v11.py verify-internal --target-ref HEAD
node scripts/slice0-oracle-artifact.cjs
cd npm && npm test
```

The remaining baseline fields, with the command each one comes from — the
data model requires a command per field, so none of these is "also record":

| Field | Command |
|---|---|
| `writer_reachability_verdict` | `cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact` |
| `activation_result` | `cargo test --test activation_cut_v11 preventive_v1_is_the_only_live_mode -- --exact` |
| `source_pins` | the two constants observed in `tests/preventive_runtime_dark_v11.rs` after the dark-seal test runs above |

**Expected**: every gate green, every field paired with its command. A field
without a command is not a captured field.

`slice0-oracle-artifact.cjs` spawns `cargo test` per case — budget for it as a
build, not a quick check.

## Step 3 — Arm the bracket with a control (do this before removing anything)

Make one deliberate edit that changes a field the baseline claims to cover.
Re-capture. The comparison **must** name the field that moved.

**Expected**: `control_result = detected(<field>)`.

**If the comparison stays quiet**: the bracket is void. Fix the comparison and
re-run the control. Do not proceed — a quiet bracket will be equally quiet
about a real regression, which is the whole failure mode this step exists to
rule out.

Discard the control edit. Confirm `git status` is clean before Step 4.

## Step 4 — Enumerate candidates, and refuse the ones that fail their gate

For each item considered:

| Check | Refuse if |
|---|---|
| Visibility | public — the postactivation set requires it |
| Frozen V11 seam | yes — its same-tree receipt would be orphaned |
| Unreachability evidence | absent — "unused" and "the roster says so" are not evidence |

Record retained candidates with their reason. A short candidate list is a
result, not a shortfall.

## Step 5 — Remove, in order (T075, then T076)

T076 is gated: run the allowlist negative suite first and read its verdict.

```
python execution/refreeze_v11.py verify-internal --target-ref HEAD
```

Only after it proves the V10 embed surface unnameable may `src/embed.rs` be
touched. If no dead code remains there, record a discharge (C-6) and remove
nothing.

After each removal: `cargo fmt --check` **first**, then refresh the two
whole-source pins from the Rust oracle's observed actuals.

**Expected**: pin file and byte counts move *downward* by the amount removed.
Counts that rise or stay flat across a non-empty removal indicate a mistake —
investigate rather than transcribe.

## Step 6 — Re-run the baseline (T077)

Repeat Step 2 and compare field by field into
`docs/reviews/FEATURE-020-SLICE5-EVIDENCE-v11.md`.

Re-run the full public-surface trio, not just the lifecycle checker:

```
node scripts/validate-lifecycle-oracle-traceability.cjs
node scripts/lifecycle-phase-probe.cjs
python execution/refreeze_v11.py verify-internal --target-ref HEAD
```

**Expected**: `differing_fields` is empty, apart from any source pin whose own
file set the removal actually intersected. A pin the removal did not touch must
be byte-identical — see C-5.

**If any other field moved**: investigate to root cause before attributing it
to the removal, and record the cause. A difference explained away without a
cause is a failed slice.

## Step 7 — Independent adversarial review

One independent review including a cfg-lens sweep (Constitution VI). Every
finding is fixed RED-first or explicitly adjudicated with recorded rationale.
Silently dropping a finding is forbidden.

## Validation checklist

- [ ] Lifecycle phase observed with `lifecycle-phase-probe.cjs`, not inferred from a green checker
- [ ] Baseline captured with a command behind every field
- [ ] Control detected a real change **before** any removal
- [ ] `refreeze_v11.py verify-internal` re-run after **every** removal, not only the embed one
- [ ] Control edit discarded; working tree clean before removal
- [ ] Every removal cites admissible unreachability evidence
- [ ] No public atom removed; no frozen V11 seam removed
- [ ] Pins refreshed after `fmt`, from the oracle, counts moving downward
- [ ] Re-run reports no unexplained differing field
- [ ] Every roster prediction performed or discharged with evidence
- [ ] Review findings fixed or adjudicated, none dropped
- [ ] Frozen tree byte-identical (`git diff` over `specs/020-…` is empty)

## The empty outcome is a pass

If Steps 4–5 find nothing removable, the slice closes with the baseline, the
armed bracket, the re-run, and a discharge record for every prediction. That
is a conforming result. Deleting something unevidenced to make the slice look
productive is not.
