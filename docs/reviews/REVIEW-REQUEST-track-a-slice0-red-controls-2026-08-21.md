# Review request — Track A: the eleven still-RED Feature 020 Slice 0 controls

## What this is

Eleven acceptance controls in this repository are failing. They were written
deliberately failing, as a record of behaviour the system did not yet have.
Every slice that was supposed to make them pass has since shipped. They are
still failing.

Nobody has independently examined whether the controls are still asking for the
right thing.

**Your job**: for each of the eleven, return a verdict on one question.

> Is the control right and the code wrong — or has the code moved such that the
> control is now the wrong test?

Those are not the only two answers. If a third is true for some control, say so
and name it.

## Rules of engagement

- **Read-only.** Change no file except your own findings file.
- **Do not build.** No `cargo build`, `cargo test`, `cargo clippy`. A cold build
  here takes ~25–40 minutes and killing one mid-write corrupts `target/`.
  Everything you need is reading source, plus `grep`/`sed`.
- **Do not un-ignore anything, do not patch anything, do not open a PR.**
- `specs/020-repository-knowledge-index/` is a frozen tree. Read it; never
  write to it.
- This work is unrelated to PR #603 (closed, unmerged). Do not use it as input.

## Where things are

Branch: `feature-029-mechanical-removal` (or `main` — the eleven controls are
identical on both).

**The roster — authoritative, and it fails closed:**

`scripts/slice0-oracle-artifact.cjs` — `RED_CASES` (11) and `RESOLVED_CASES`
(1). Read the header comment; it explains why a control that *starts* passing
is an error rather than a success, and how a fixed control is meant to be
reclassified rather than deleted.

> **Read this file; do not run it.** It spawns `cargo test` once per case. It
> is the roster's authority as *source*, not as a command you should execute
> here — running it is a build, and builds are out of scope for this review.

**The control bodies:**

| # | Control | Location |
|---|---|---|
| 1 | `capacity_refused_open_creates_no_slot_and_no_watcher` | `tests/project_index_lifecycle_slice0.rs:109` |
| 2 | `empty_placeholder_publication_refuses_watcher_mutation` | `tests/project_index_lifecycle_slice0.rs:156` |
| 3 | `failed_reload_retains_the_recovery_observer` | `tests/project_index_lifecycle_slice0.rs:263` |
| 4 | `observer_replacement_gap_is_latched_as_non_current` | `tests/project_index_lifecycle_slice0.rs:375` |
| 5 | `old_observer_delivery_after_promotion_is_not_current` | `tests/project_index_lifecycle_slice0.rs:483` |
| 6 | `watcher_mutation_during_candidate_build_is_not_discarded` | `tests/project_index_lifecycle_slice0.rs:576` |
| 7 | `whole_project_publication_preserves_latest_siblings` | `tests/project_index_lifecycle_slice0.rs:674` |
| 8 | `snapshot_seed_is_not_queryable_before_verification` | `tests/project_index_lifecycle_slice0.rs:777` |
| 9 | `configured_capacity_bounds_the_process_not_each_load` | `tests/project_index_lifecycle_slice0.rs:858` |
| 10 | `same_path_root_replacement_is_not_silently_adopted` | `tests/project_index_lifecycle_slice0.rs:922` |
| 11 | `concurrent_first_open_performs_exactly_one_cold_load` | `src/daemon.rs:10054` |

Line numbers are the `#[ignore]` attribute; the function follows it. Each
`#[ignore]` string states which defect the control was written against and
which slice was expected to resolve it.

**The seams they drive:**

- `src/daemon.rs` — project admission, cold load, session lifecycle
- `src/watcher/mod.rs` — observation, reconciliation, publication fencing
- `src/index_lifecycle/` — `registry.rs`, `capacity.rs`, `observer.rs`,
  `verification.rs`, `supervisor.rs`, `candidate.rs`, `activation.rs`,
  `physical_root.rs`, `authority.rs`
- `src/live_index/persist.rs` — snapshot restore

**Background, if you want it:**

- `docs/reviews/FEATURE-020-POST-V11-LEDGER.md` — what the whole feature still
  owes; this is its "Track A"
- `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` §5 and §6 —
  what the most recent slice did and did not discharge
- `specs/020-repository-knowledge-index/tasks.md` — the frozen roster
- `.specify/memory/constitution.md` — the six rules this codebase is held to
- `CLAUDE.md` — repository rules and known traps

**One trap that will waste your time if you hit it blind**: the V11 cut mounts
the server modules under `src/internals.rs` via `#[path]`. Lib unit-test paths
therefore carry an `internals::` prefix, and any pre-cut
`cargo test --lib … -- --exact` filter silently selects *nothing* and exits 0
having run nothing. You are not running tests, but you will see such filters in
documents and scripts.

## What to produce

Write to:

```
docs/reviews/REVIEW-FINDINGS-<your-name>-track-a-slice0-2026-08-21.md
```

### Part 1 — a verdict per control (all eleven, none skipped)

```
### <N>. <control name> — <CONTROL-RIGHT | CONTROL-WRONG | OTHER>
- **What it asserts**: one or two sentences, in your own words.
- **What the code does**: where you looked, and what you found there.
- **Verdict**: which of the above, and why.
- **Confidence**: high / medium / low, and what would raise it.
```

If you cannot reach a verdict on a control from reading alone, say
`INSUFFICIENT` and name exactly what you would need. That is a legitimate
answer and more useful than a guess dressed as a finding.

### Part 2 — open questions

Answer these in your own words. They are deliberately not leading.

1. What is the strongest argument that these eleven should be **deleted**
   rather than fixed?
2. What is the strongest argument they should be fixed **exactly as written**?
3. Do any of the eleven contradict each other, or assert something a later
   design decision deliberately reversed?
4. Are any of them testing the same underlying property twice?
5. If you had to fix only three, which three, and why those?
6. What did this brief fail to ask about?

### Part 3 — Outside the questions asked (MANDATORY)

Anything you noticed that none of the above covers — in the controls, the
roster script, the seams, the surrounding documents, the process. If you found
nothing, write "nothing" and say what you looked at. This section is not
optional and is not a place for padding; it exists because the questions above
were written by someone with assumptions, and the things worth knowing are
usually outside them.

### Part 4 — Negatives

What you checked and found sound. Be specific enough that a reader can tell you
actually looked. A silent omission is indistinguishable from not having looked.

## Only after Parts 1–4 are written

Open [`APPENDIX-track-a-suspicions-2026-08-21.md`](APPENDIX-track-a-suspicions-2026-08-21.md)
(same directory). It contains what the requester already suspects.

Read it only once your own pass is on disk. Then append:

### Part 5 — Delta

- Which appendix suspicions your independent pass had already reached.
- Which you now think are wrong.
- Which changed a verdict above, and to what.

**Do not read the appendix first.** Its entire value is that you did not.
