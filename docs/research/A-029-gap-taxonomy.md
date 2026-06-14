# A-029 — Gap Taxonomy (8.1 index-recall T2.1)

**Audit ID:** 81-T2.1-taxonomy-2026-06-14 (T019 cleanup revision)
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
| **FM-CAP** | Compact output truncation | `find_references` compact view can emit up to **20 files** (`OutputLimits::new(20,10)` default); A-029 recall counts **cited file paths** in serve output | **Tokio binding:** both tasks cite **20/20** files — metric is output-bound at cap. **Django not cap-bound at 20:** `QuerySet` cites **7**, `Model` cites **17** — misses are not explained by hitting the file budget |
| **FM-RG-TEXT** | rg text baseline ⊃ structured refs | rg `-l` marks any word-boundary token occurrence; index stores parsed `ReferenceKind` records | Large `source` bucket misses; dominant on django (`Model`: 293 source misses) |
| **FM-TEST** | Test-tree reference gaps | Missed paths concentrated under `tests/**`, `tokio/tests`, etc. | Missed bucket counts: tests 16–149 per task |
| **FM-BENCH** | Bench file gaps (Rust) | rg baseline includes repo-root `benches/*.rs`; subset cited, subset missed | `t2_spawn`: **13** missed bench; `t2_block_on`: **7** missed bench (from `missed_bucket_counts`) |
| **FM-SYMBOL** | Symbol identity / resolution | Common names (`spawn`, `Model`) match many unrelated occurrences in rg; index ties to symbol records | High baseline cardinality (252 / 354 files) |
| **FM-MARKDOWN** | Markdown / docs not in baseline | §6.1 hypothesis; current A-029 proxy uses `*.rs`/`*.py` rg globs only | Zero `.md` paths in rg-hits JSON; **audit gap** — not disproved, not measured |
| **FM-POLICY** | P-T2 bypass envelope | Tasks designed to lose vs grep+Read should bypass | **Out of scope for index fix** — policy row, not taxonomy implementation target |

---

## Taxonomy rows (root cause → fix surface → acceptance)

| Row | §6.1 class | Failure mode(s) | Proposed fix surface (post-T019) | Acceptance test | Est. recall lift |
|-----|------------|-----------------|----------------------------------|-----------------|------------------|
| **TX-01** | cross-file text | FM-CAP (tokio-primary) | `src/stel/planner.rs` + `src/protocol/tools.rs` / `format.rs` — raise compact `find_references` file/hit budget for trace intent OR paginate with honest completeness label | **Per-repo re-measure after TX-01:** tokio cited files should exceed 20 when index has more; django judged separately so flat recall is not misread as TX-01 failure | **High on tokio**; **uncertain on django** until post-TX-01 per-repo measure |
| **TX-02** | cross-file text | FM-RG-TEXT, FM-SYMBOL | `src/live_index/query.rs` + `src/parsing/xref.rs` — broaden ref capture for value/type uses; disambiguation for common symbols | Unit tests + rg-hits matched_paths ↑ on `Model`/`QuerySet`/`spawn` | **High** — **django-primary** |
| **TX-03** | benches | FM-BENCH | `src/parsing/xref.rs` — bench macro / criterion ref extraction | `benches/*.rs` missed bench bucket ↓ on tokio tasks | **Medium** (tokio-only) |
| **TX-04** | cross-file text | FM-TEST | Index + xref in test modules; verify parse tier not skipping tests | Missed `test` bucket counts ↓ | **Medium–High** |
| **TX-05** | markdown | FM-MARKDOWN | `src/parsing/*` — optional md/code reference extraction OR exclude md from equiv baseline with documented policy | Separate md-glob audit OR taxonomy amendment | **Unknown** (not measured) |
| **TX-06** | — | FM-POLICY | STEL policy only | P-T2 rows stay bypass | **N/A (P-T2-only)** |

---

## Explain-power ranking

Ranked by expected impact on A-029 **file-recall** metric if fixed (evidence-weighted):

| Rank | Row | Rationale |
|------|-----|-----------|
| 1 | **TX-01** FM-CAP | **Tokio:** cited files at 20/20 cap — cheap measurement/remediation first. **After TX-01, re-measure per repo** so django flatness is not misread as TX-01 failure |
| 2 | **TX-02** FM-RG-TEXT / FM-SYMBOL | **Django-primary:** 293 source misses on `t2_model`; 48 on `t2_queryset` — not cap-bound. Django recall likely **TX-02-bound** |
| 3 | **TX-04** FM-TEST | Dominates missed `test` buckets on all four tasks |
| 4 | **TX-03** FM-BENCH | Tokio: 13 + 7 missed **bench** paths; zero django bench baseline |
| 5 | **TX-05** FM-MARKDOWN | Hypothesis untested (rg glob limitation) |
| — | **TX-06** | Policy only — not an index-recall fix |

**Recommended T2.2 implementation order (T019 GO):** TX-01 → TX-02 → TX-04 → TX-03; **re-measure per repo after TX-01**; TX-05 requires baseline policy decision first.

---

## Missed-site inventory (aggregate)

From [`rg-hits/summary.json`](./rg-hits/summary.json) (`missed_bucket_counts`, bench bucketing fix applied):

| Task | missed | primary buckets |
|------|--------|-----------------|
| `tokio/t2_spawn` | 236 | test 149, source 73, **bench 13**, example 1 |
| `tokio/t2_block_on` | 121 | test 72, source 42, **bench 7** |
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

**Status:** **GO** — see [`81-index-recall-taxonomy-signoff.md`](./81-index-recall-taxonomy-signoff.md). Cleanup items #1–#3 applied on PR #314 before implementation branch opens.
