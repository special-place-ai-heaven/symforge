# A-029 T2 — tokio/t2_spawn Residual Ranking Analysis

**Artifact type:** docs-only evidence packet (no code, no fixture, no posture change)
**Author slice:** post-TX04 residual micro-analysis (read-only)
**Date:** 2026-06-15
**Binary identity:** symforge 7.26.0 @ `5bbde13182a6ab4daa3fb52c33c040194353285f`
**Corpus:** tokio @ `7892f6020d9c914a41d0c350693fb71937d43c03` (pinned)
**Source artifacts:** [`A-029-t2-replay.json`](./A-029-t2-replay.json),
[`a029-tx04-results.json`](./a029-tx04-results.json),
[`rg-hits/tokio/t2_spawn.json`](./rg-hits/tokio/t2_spawn.json)

> **Purpose.** Correct the deferred-work framing. The 8.1 ledger listed
> "TX-03 benches" as the next lever toward full A-029/T2 closure. Current
> post-TX04 evidence **disproves** that framing for the gating row
> `tokio/t2_spawn` and identifies the real lever as a ranking/fairness
> behavior under the 100-file compact cap. This packet is the reviewable
> evidence required **before** any change to the ranking algorithm.

---

## 1. Current authoritative replay state (post-TX04)

From [`A-029-t2-replay.json`](./A-029-t2-replay.json) row `tokio/t2_spawn`
(`replay_id: 81-index-recall-t2-replay-post-tx04`, measured 2026-06-15T11:52Z):

| Field | Value |
|-------|-------|
| baseline_paths (denominator) | **252** |
| matched_paths | **87** |
| baseline_recall | **0.3452 (34.5%)** |
| min_baseline_recall (threshold) | **0.35** |
| equivalence | **SYMFORGE-LESS** |
| decision | serve |

**Exact gap math:** threshold count = `ceil(0.35 x 252)` = `ceil(88.2)` = **89**
(88/252 = 34.92% fails; 89/252 = 35.32% passes). Current matched = 87.
**Additional matched files needed to cross 35% = +2 (87 -> 89).**

This supersedes the old T2.1 baseline (6.3%, 16/252) in
[`A-029-tokio-recall-spike.md`](./A-029-tokio-recall-spike.md) and
[`A-029-gap-taxonomy.md`](./A-029-gap-taxonomy.md), which is pre-TX01/TX02/TX04
discovery history. TX01/TX02/TX04 lifted matched 16 -> 87. The current residual
is small.

---

## 2. Current residual bucket state

From [`rg-hits/tokio/t2_spawn.json`](./rg-hits/tokio/t2_spawn.json)
(regenerated post-TX04, measured 2026-06-15T11:53Z, baseline_commit
`fe6c42fb...`):

| Bucket | baseline | matched | missed |
|--------|---------:|--------:|-------:|
| source | 82 | 42 | **40** |
| test | 155 | 30 | **125** |
| bench | 14 | **14** | **0** |
| example | 1 | 1 | 0 |
| **total** | 252 | 87 | 165 |

- cited_paths_count = **100** (compact output ceiling — see Section 4)
- missed_bucket_counts = `{test: 125, source: 40}` — **no `bench` key**
- All **14 bench baseline paths are already matched** (matched bucket bench = 14).

---

## 3. Why TX-03 / FM-BENCH is rejected

TX-03 (FM-BENCH, [`A-029-gap-taxonomy.md`](./A-029-gap-taxonomy.md) row TX-03,
ranked #4 of 5) proposed bench/criterion ref extraction in
`src/parsing/xref.rs` to reduce the tokio missed-bench bucket.

- **No current bench misses remain.** Post-TX04 `missed_bucket_counts` has zero
  bench entries; all 14 baseline bench paths are matched. TX-04's test-path
  fair ordering + test-idiomatic xref recovered the benches as a side effect.
- **A bench fix would not move the gating row.** With 0 bench misses, TX-03 has
  nothing to recover for `tokio/t2_spawn`. Implementing it would be theater: a
  green PR with **zero** matched-file delta on the row it claims to advance.

**Verdict: TX-03 / FM-BENCH is closed as a no-op for the current state,
superseded by evidence.** The bench lever is exhausted.

---

## 4. Discovered root cause: ranking/fairness under the 100-file cap

The residual misses are **not an extraction gap** for the load-bearing files.
They are a **ranking/ordering gap** under a hard file cap.

### 4a. The cited set is hard-bound at 100 files

- Compact STEL serve injects `limit=100` via
  `COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT: u32 = 100`
  (`src/stel/executor.rs:18`; per-file budget
  `COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE = 10` at line 20).
- `find_references` builds `OutputLimits::new(limit, max_per_file)`
  (`src/protocol/tools.rs:6541`); `OutputLimits::new` clamps
  `max_files = max_files.min(100)` (`src/protocol/format.rs:17-23`).
- The compact renderer emits at most `min(view.files.len(), max_files) = 100`
  file paths (`find_references_compact_view`, `src/protocol/format.rs:3322,3342`).

`cited_paths_count = 100` in the data is exactly this ceiling. The binding limit
is the **100-file cap**, not the per-file hit budget (1000 hits is not reached
before 100 files at ~3-10 hits/file).

### 4b. 13 cited slots are consumed by non-baseline files

Of the 100 cited paths, **13 are NOT in the rg baseline** (cited-but-unmatched):
`tokio/src/blocking.rs`, `tokio/src/fs/{mod,read,read_dir,read_to_string,write}.rs`,
`tokio/src/io/mod.rs`, `tokio/src/net/addr.rs`,
`tokio/src/runtime/{blocking/mod,dump}.rs`,
`tokio-util/src/io/{mod,sync_bridge}.rs`,
`tokio-util/tests/io_sync_bridge.rs`.

These are real `spawn` / `spawn_blocking` ref sites symforge found that rg's
`\bspawn\b` word-boundary baseline missed (captured as `Call` /
`method_call` / `Import` per `RUST_XREF_QUERY`, `src/parsing/xref.rs:13-42`;
`ReferenceKind`, `src/domain/index.rs:488-504`). They are legitimate finds — but
they occupy 13 of 100 slots with files that do not count toward the rg metric.

### 4c. Lexicographic non-test ordering pushes spawn-home modules past slot 100

The order that decides which 100 of the total ref files survive the cap is
`order_find_references_file_paths_fair` (`src/live_index/query.rs:1932-1958`):

1. split into `tests` / `non_tests` (`file_path_is_test`),
2. **`non_tests.sort()` — plain lexicographic** (line 1943),
3. 1:1 interleave non_test, test, non_test, test (lines 1947-1956; this
   interleave is the TX-04 fairness pass that protects `tokio/tests/**`).

Plain lexicographic non-test order is the problem:

- Cross-crate, `tokio/` sorts **last** (`benches` < `examples` < `stress-test`
  < `tokio-macros` < `tokio-stream` < `tokio-test` < `tokio-util` < `tokio`).
- Within `tokio/src/`, the alphabetical walk
  (`blocking` -> `fs` -> `io` -> `lib` -> `loom` -> `macros` -> `net` ->
  `process` -> `runtime`) is **cut off mid-`runtime/`**.
- Everything after the cutoff falls past slot 100: `runtime/mod.rs`,
  `runtime/runtime.rs`, `runtime/scheduler/**`, `runtime/task/**`, and the
  entire `src/sync/**` and `src/task/**` (the spawn definition home:
  `task/spawn.rs`, `task/mod.rs`).

This cutoff pattern matches the 40 missed-source list almost exactly (all 40 are
`runtime/*` after `handle`, `sync/*`, `task/*`, `signal/*`, `io/stdin.rs`,
`util/trace.rs`). It is the fingerprint of lexicographic truncation, not
extraction failure: the missed `runtime/mod.rs`, `runtime/runtime.rs`,
`runtime/scheduler/**`, `runtime/task/join.rs`, and `task/mod.rs` are real
`spawn` call/import references symforge **already extracts** and drops solely to
cap ordering.

> **Note (not a single pure global boundary).** The "cut off mid-`runtime/`"
> framing is the dominant fingerprint, not an exact global lexicographic line.
> The TX-04 test/non-test 1:1 interleave (L1947-1956) and per-module emission
> create local exceptions: at least one source miss, `tokio/src/io/stdin.rs`,
> sorts *before* the visible non-test cutoff (`runtime/handle.rs`) yet is still
> dropped past slot 100. The exact surviving set is the interleaved result, not
> a plain prefix of the sorted non-test list. This does not change the
> conclusion — the proposed definition-subtree ranking key (Section 5) promotes
> spawn-home paths regardless of where the per-module/interleave boundary falls,
> so it addresses `io/stdin.rs` and the bulk `runtime/`/`sync/`/`task/` misses
> alike.

### 4d. rg false-positive caveat (honest)

A fraction of the 40 source misses are rg counting the word `spawn` in
doc-comments, string literals, or the `fn spawn` **definition** token (e.g.
`tokio/src/task/spawn.rs`, where the only occurrences may be the definition plus
prose). The XREF query never captures a definition identifier as a reference, so
symforge correctly cites 0 refs there. The **recoverable** source misses are
therefore fewer than 40 — but still comfortably exceed the +2 needed, because
the `runtime/*`, `sync/*`, and `task/mod.rs` re-export/call sites are genuine
extracted refs displaced only by ordering.

---

## 5. Proposed implementation shape (NOT IMPLEMENTED IN THIS PACKET)

- **Surface:** `src/live_index/query.rs`,
  `fn order_find_references_file_paths_fair` (L1932-1958).
- **Change type:** ranking — replace the bare `non_tests.sort()` (L1943) with a
  sort key that ranks the queried symbol's **definition subtree** (e.g. files
  under the def file's parent module/crate, such as `tokio/src/task/**` and
  `tokio/src/runtime/**` for `spawn`) ahead of alphabetically-early but
  unrelated files (`fs/`, `blocking.rs`, `io/`).
- **Plumbing:** the ordering fn currently has no symbol-location context. The
  definition file path / module prefix must be threaded from the find-references
  view builder into the ordering call (a signature change in the call chain
  `build_find_references_view` -> `order_find_references_file_paths_fair`).
- **Preserve:** the TX-04 test/non-test 1:1 interleave (L1947-1956) — do not
  collapse or reorder the test lane.
- **Do NOT raise the cap.** Bumping
  `COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT` above 100 is rejected: "win recall
  by emitting more output" is exactly the FM-CAP gaming the program's taxonomy
  warns against, and the cap was deliberately set to the schema max of 100 in
  TX-01 ([`A-029-tx01-cap-evidence.md`](./A-029-tx01-cap-evidence.md)).

**Honest +matched estimate:** +2 to +6 source files (each baseline-true file
promoted past one of the 13 non-baseline cited files is a net +1 matched).
Reaches 89/89 (35.0%) with margin.

---

## 6. Risk

**Risk level: MEDIUM.**

- Editing a TX-04-tuned ranking function: the test/non-test interleave protects
  `tokio/t2_block_on` (EQUIVALENT 70.9%); a careless reorder could regress it.
- Requires a signature change to thread definition-location into the ordering
  path (not a one-liner).
- Re-ordering changes which files appear for **every** find_references serve, not
  just `tokio/t2_spawn` — django rows and any other consumer are affected and
  must be re-measured.
- Metric-vs-utility tension: displacing the 13 non-baseline cited files improves
  the rg metric but slightly reduces "found a real `spawn_blocking` site"
  coverage a human might value. Disclose in the implementing PR.

**Low regression risk surfaces (for completeness):**

- In-repo golden `t4_refs` rows (`src/stel/golden_replay.rs`): those fixtures
  have far fewer than 100 ref files, so the 100-cap ordering never engages — out
  of scope per [`A-029-gap-taxonomy.md`](./A-029-gap-taxonomy.md) and effectively
  zero regression risk.
- No unit test pins `order_find_references_file_paths_fair` output (its only
  caller is the view builder); the existing compact-cap test
  (`test_find_references_compact_view_total_limit_caps_across_files`) exercises
  hit-budget truncation on synthetic files, not path ordering.

---

## 7. Measurement plan (required before any merge)

The tokio/django corpora are gitignored (clone-on-demand under
`tests/fixtures/a029-t2/<repo>`); recall cannot be measured locally without
re-cloning. The implementing slice MUST:

1. Clone/refresh the tokio corpus at the recorded SHA
   `7892f6020d9c914a41d0c350693fb71937d43c03` (and django at
   `f1440a752ec034277ccdad914995c3f164308e41` for regression rows).
2. Re-run the A-029 T2 spike for **all four** rows
   (`node scripts/a029-t2-spike.cjs <symforge-bin>`).
3. Acceptance gates:
   - `tokio/t2_spawn`: matched **87 -> >=89** (recall >= 35%).
   - `tokio/t2_block_on`: remains **EQUIVALENT** (>= 35%).
   - `django/t2_model`: remains **EQUIVALENT** (>= 25%).
   - `django/t2_queryset`: must **not regress** below current posture (26.8%).
4. Record before/after for all four rows in an evidence doc; update
   [`A-029-t2-replay.json`](./A-029-t2-replay.json) only via a fresh replay
   artifact, not by hand.

If the live re-measure does not reach 87 -> >=89, the ranking change is **not**
landed as a recall win (no posture change, honest negative result recorded).

---

## 8. Decision

- **Next implementation slice:** `spawn-ranking-residual` — a ranking/fairness
  fix in `order_find_references_file_paths_fair`, gated on the Section 7 live
  re-measure.
- **Do not call it TX-03.** TX-03 / FM-BENCH is **closed** as a no-op /
  superseded by evidence (Section 3).
- **No posture change in this packet:** `routes.golden.jsonl` frozen,
  `tests/fixtures/a029-t2/tasks.jsonl` unchanged, `eligible_h6` unchanged, no
  H6/H7/H8 claim, P-T2 partial unchanged. tokio/t2_spawn remains SYMFORGE-LESS /
  bypass-only until a live re-measure proves >= 35% AND a separate T2.4-style
  restoration sign-off is recorded.
