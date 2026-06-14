# A-029 — Gap Taxonomy (8.1 index-recall T2.1)

**Audit ID:** 81-T2.1-taxonomy-2026-06-14
**Inputs:** [`rg-hits/summary.json`](./rg-hits/summary.json), [`A-029-t2-task-crosswalk.md`](./A-029-t2-task-crosswalk.md), Phase 2 [`a029-t2-results.json`](./a029-t2-results.json)
**Tasks:** T017–T018

## Summary recall (baseline)

| Task | recall | min | gap to threshold |
|------|--------|-----|------------------|
| `tokio/t2_spawn` | 6.3% | 35% | −28.7 pp |
| `tokio/t2_block_on` | 14.2% | 35% | −20.8 pp |
| `django/t2_queryset` | 9.9% | 35% | −25.1 pp |
| `django/t2_model` | 4.8% | 25% | −20.2 pp |

---

## Failure-mode classification

| ID | Failure mode | Description | Evidence |
|----|--------------|-------------|----------|
| **FM-CAP** | Compact output truncation | `find_references` compact view emits ~7–20 **files** despite larger in-index ref sets; A-029 recall counts **cited file paths** in serve output | Cited count ≈20 tokio / ≈7–17 django; `OutputLimits::new(20,10)` default in `find_references` handler; planner passes `compact: true` without raised `limit` |
| **FM-RG-TEXT** | rg text baseline ⊃ structured refs | rg `-l` marks any word-boundary token occurrence; index stores parsed `ReferenceKind` records | Large `source` bucket misses with no corresponding index refs; common on high fan-out symbols (`Model`) |
| **FM-TEST** | Test-tree reference gaps | Missed paths concentrated under `tests/**`, `tokio/tests`, `tests/migrations`, etc. | Missed bucket counts: tests 44–149 per task |
| **FM-BENCH** | Bench file gaps (Rust) | rg baseline includes root `benches/*.rs`; subset cited, subset missed | `t2_spawn`: 13 missed bench paths; `t2_block_on`: 7 missed |
| **FM-SYMBOL** | Symbol identity / resolution | Common names (`spawn`, `Model`) match many unrelated occurrences in rg; index ties to symbol records | High baseline cardinality (252 / 354 files) |
| **FM-MARKDOWN** | Markdown / docs not in baseline | §6.1 hypothesis; current A-029 proxy uses `*.rs`/`*.py` rg globs only | Zero `.md` paths in rg-hits JSON; **audit gap** — not disproved, not measured |
| **FM-POLICY** | P-T2 bypass envelope | Tasks designed to lose vs grep+Read should bypass | **Out of scope for index fix** — policy row, not taxonomy implementation target |

---

## Taxonomy rows (root cause → fix surface → acceptance)

| Row | §6.1 class | Failure mode(s) | Proposed fix surface (post-T019) | Acceptance test | Est. recall lift |
|-----|------------|-----------------|----------------------------------|-----------------|------------------|
| **TX-01** | cross-file text | FM-CAP | `src/stel/planner.rs` + `src/protocol/tools.rs` / `format.rs` — raise compact `find_references` file/hit budget for trace intent OR paginate with honest completeness label | A-029 replay: cited file count scales with index refs; tokio/django recall ↑ without changing rg baseline | **High** (unblocks metric ceiling) |
| **TX-02** | cross-file text | FM-RG-TEXT, FM-SYMBOL | `src/live_index/query.rs` + `src/parsing/xref.rs` — broaden ref capture for value/type uses; disambiguation for common symbols | Unit tests + rg-hits matched_paths ↑ on `Model`/`spawn` | **High** |
| **TX-03** | benches | FM-BENCH | `src/parsing/xref.rs` — bench macro / criterion ref extraction | `benches/*.rs` missed paths ↓ on tokio tasks | **Medium** (tokio-only) |
| **TX-04** | cross-file text | FM-TEST | Index + xref in test modules; verify parse tier not skipping tests | Missed `tests/**` prefix counts ↓ | **Medium–High** |
| **TX-05** | markdown | FM-MARKDOWN | `src/parsing/*` — optional md/code reference extraction OR exclude md from equiv baseline with documented policy | Separate md-glob audit OR taxonomy amendment | **Unknown** (not measured) |
| **TX-06** | — | FM-POLICY | STEL policy only | P-T2 rows stay bypass | **N/A (P-T2-only)** |

---

## Explain-power ranking

Ranked by expected impact on A-029 **file-recall** metric if fixed (evidence-weighted):

| Rank | Row | Rationale |
|------|-----|-----------|
| 1 | **TX-01** FM-CAP | Cited files plateau at ~20 regardless of corpus size; metric is output-bound |
| 2 | **TX-02** FM-RG-TEXT / FM-SYMBOL | 293+ source misses on `django/t2_model`; core index/xref gap |
| 3 | **TX-04** FM-TEST | Dominates missed buckets on all four tasks |
| 4 | **TX-03** FM-BENCH | Material for tokio (13+7 bench misses); zero django bench baseline |
| 5 | **TX-05** FM-MARKDOWN | Hypothesis untested in this audit (rg glob limitation) |
| — | **TX-06** | Policy only — not an index-recall fix |

**Recommended T2.2 implementation order (if T019 GO):** TX-01 → TX-02 → TX-04 → TX-03; TX-05 requires baseline policy decision first.

---

## Missed-site inventory (aggregate)

From [`rg-hits/summary.json`](./rg-hits/summary.json):

| Task | missed | primary buckets |
|------|--------|-----------------|
| `tokio/t2_spawn` | 236 | test 149, source 86 |
| `tokio/t2_block_on` | 121 | test 72, source 49 |
| `django/t2_queryset` | 64 | source 48, test 16 |
| `django/t2_model` | 337 | source 293, test 44 |

Full path lists: per-task JSON under [`rg-hits/`](./rg-hits/).

---

## Out-of-scope / P-T2-only

- Golden `*/t4_refs` row changes
- `eligible_h6` restoration
- Host grep bypass envelope (P-T2) — retained until T2.4 replay
- H6 denominator / B-RESULTS / persistence

---

## T019 gate

Independent reviewer must accept or reject this taxonomy in [`81-index-recall-taxonomy-signoff.md`](./81-index-recall-taxonomy-signoff.md) before any `src/**` work (T2.2/T2.3).

**Producer status:** Taxonomy **COMPLETE** — awaiting reviewer **GO**.
