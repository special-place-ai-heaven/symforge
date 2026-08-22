# Review request — seam grouping for the code-wrong Slice 0 controls

## Correction, read this first

Earlier chat between the main session and Grove said **seven** code-wrong
controls. **There are eight.** Eleven RED controls split 3 control-stale + 8
code-wrong. The ledger table on `main` is authoritative and already says 8; the
"seven" was a mistake repeated on both sides of the conversation, not a
narrower scope.

If a pass is already underway against seven, the missing one is whichever of
the eight below is absent from it. All eight are in scope.

## The question

For the eight code-wrong Slice 0 controls:

> **What is the minimal set of seams that closes them, and which of the eight
> share a seam?**

Right now they are recorded as eight separate live misses. Whether that is
eight problems or fewer decides whether implementation is tractable, and
nothing downstream can be planned until it is answered.

Answer with the grouping you actually derive. A grouping of one, or of eight,
is a legitimate answer if that is what the code shows.

## Rules of engagement

- **Read-only.** Change no file except your own findings file.
- **No build.** No `cargo build`, `cargo test`, `cargo clippy`. Everything
  needed is reading source plus `grep`/`sed`.
- **Watch for tooling that looks cheap and is not**: `scripts/slice0-oracle-artifact.cjs`
  spawns `cargo test` per case. Read it, do not run it.
- **Do not read or use PR #609** (`cursor/critical-bug-investigation-ca81`).
  It is an unowned draft from an outside actor touching this same territory and
  is explicitly out of scope. If your conclusions would change on seeing it,
  say so without reading it.
- **Do not un-ignore, patch, or reclassify anything.**
- `specs/020-repository-knowledge-index/` is frozen. Read it; never write to it.
- **Findings stay off the remote** unless Rob authorizes a push. Manual relay
  is the expected path — say "not on remote" rather than implying a branch.

## Tree under review

`main` @ **`fd4de8dc`**

## The eight controls, with the live miss already recorded

Bodies: `tests/project_index_lifecycle_slice0.rs` (7) and `src/daemon.rs` (1).
Each carries a `#[ignore]` string stating its disposition.

| # | Control | Live miss as recorded |
|---|---|---|
| 1 | `empty_placeholder_publication_refuses_watcher_mutation` | `add_file` (`store.rs:2820-2831`) has no EmptyBootstrap gate; the default-suite check at `store.rs:6402-6412` is a paper-over |
| 2 | `failed_reload_retains_the_recovery_observer` | aborts the watcher then `?`; no replacement on `Err` |
| 3 | `observer_replacement_gap_is_latched_as_non_current` | `recompute_freshness_locked` drops the historical gap → `Current` |
| 4 | `old_observer_delivery_after_promotion_is_not_current` | same rederive; no token fence |
| 5 | `snapshot_seed_is_not_queryable_before_verification` | persist hydrates files immediately; `get_file` has no Pending gate; `is_ready()` is status-only |
| 6 | `same_path_root_replacement_is_not_silently_adopted` | path-keyed map; publishes `Current` |
| 7 | `concurrent_first_open_performs_exactly_one_cold_load` | load happens outside the lock then `or_insert`; `admit_project` does not skip bootstrap |
| 8 | `watcher_mutation_during_candidate_build_is_not_discarded` | `store.rs:2403-2436` still reaches `swap_and_publish`; `IsolatedCandidate` appears zero times in `store.rs` |

Those "live miss" entries come from your own earlier pass. Treat them as prior
observations to confirm or correct, not as constraints on what you may find.

## Where the seams are

- `src/daemon.rs` — project admission, cold load, session lifecycle
- `src/watcher/mod.rs` — observation, reconciliation, publication fencing
- `src/live_index/store.rs` — publication, reload, `swap_and_publish`
- `src/live_index/persist.rs` — snapshot restore, hydration
- `src/index_lifecycle/` — `registry.rs`, `capacity.rs`, `observer.rs`,
  `verification.rs`, `supervisor.rs`, `candidate.rs`, `activation.rs`,
  `physical_root.rs`, `authority.rs`
- `docs/reviews/FEATURE-020-POST-V11-LEDGER.md` — the standing ledger
- `specs/020-repository-knowledge-index/` — frozen requirements and contracts

**One trap**: the V11 cut mounts server modules under `src/internals.rs` via
`#[path]`, so lib unit-test paths carry an `internals::` prefix. Any pre-cut
`--exact` filter you find in a document selects nothing and exits 0.

## What to produce

```
docs/reviews/REVIEW-FINDINGS-<your-name>-seam-grouping-2026-08-21.md
```

### Part 1 — the grouping

For each seam you identify:

```
### Seam <N>: <name it in your own words>
- **Where**: the file(s) and the specific mechanism
- **Controls it closes**: which of the eight, by number
- **Why these share it**: the property they have in common
- **What closing it would require**: shape only, not an implementation
- **Confidence**: high / medium / low, and what would raise it
```

Then state plainly: how many seams, and whether any control resists grouping.

### Part 2 — open questions

1. Is any of the eight actually two problems wearing one name?
2. Do any two of the eight *conflict* — closing one making another harder?
3. Which seam, closed first, would tell us the most about the rest?
4. Is any of the eight better answered by a design decision than by code?
5. What did this brief fail to ask about?

### Part 3 — Outside the questions asked (MANDATORY)

Anything you noticed that none of the above covers — in the controls, the
seams, the ledger, the surrounding documents, the framing of this request. If
you found nothing, write "nothing" and say what you looked at. Not optional,
and not a place for padding: the questions above were written by someone with
assumptions, and what matters is usually outside them.

### Part 4 — Negatives

What you checked and found sound, specific enough that a reader can tell you
looked.

## Only after Parts 1–4 are written

Open `APPENDIX-seam-grouping-suspicions-2026-08-21.md` in the same directory,
then append:

### Part 5 — Delta

- Which appendix suspicions your pass had already reached.
- Which you now think are wrong.
- Which changed a conclusion above, and to what.

**Do not read the appendix first.** Its only value is that you did not.
