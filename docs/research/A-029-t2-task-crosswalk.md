# A-029 — T2 Task Crosswalk (8.1 index-recall T2.1)

**Audit ID:** 81-T2.1-crosswalk-2026-06-14
**Baseline commit:** `470826a`
**Binding tasks:** T010–T012

## Phase 2 → 8.1 handoff

| Item | Phase 2 state | 8.1 program action |
|------|---------------|-------------------|
| A-029 verdict | **PIVOT** (0/4 T2 equiv) | T2.4 replay after T2.2/T2.3 fixes |
| Machine verdict | `A029Verdict::Pivot` | Unchanged until replay ≥2/4 |
| P-T2 policy | Bypass-only; grep envelope | Retained until replay proves ≥2/4 |
| `eligible_h6` | Not yet applied to golden rows | **Do not change** in T2.1 |
| Root-cause hypothesis | §6.1: markdown, benches, cross-file text | Tested via rg + index measurement |
| Deferred work | Index ref-source audit | **This audit slice** |

Sources: [`phase2-stel-checkpoint.md`](../phase2-stel-checkpoint.md), [`A-029-t2-spike.md`](./A-029-t2-spike.md), [`v8-gap-closure-plan.md`](../v8-gap-closure-plan.md) §6.1.

## Golden T2 rows vs A-029 external tasks

Golden corpus [`docs/fixtures/routes.golden.jsonl`](../../docs/fixtures/routes.golden.jsonl) uses `*/t4_refs` rows for **in-repo sf-bench fixtures** (small corpora). A-029 [`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl) defines **external reference repos** (tokio, django).

| Golden row ID | Query (abbrev) | Corpus | A-029 task ID | Relationship |
|---------------|----------------|--------|---------------|--------------|
| `cfg-if/t4_refs` | who references cfg_if | cfg-if fixture | — | In-repo; **PASS** on battery |
| `records/t4_refs` | references to Connection | records fixture | — | In-repo; golden serve |
| `is-plain/t6_refs` | references isPlainObject | is-plain fixture | — | In-repo; golden serve |
| `compression/t4_refs` | references reconcile | compression fixture | — | In-repo; golden serve |
| — | — | tokio | `tokio/t2_spawn` | External T2 reference |
| — | — | tokio | `tokio/t2_block_on` | External T2 reference |
| — | — | django | `django/t2_queryset` | External T2 reference |
| — | — | django | `django/t2_model` | External T2 reference |

**Policy scope:**

- **P-T2** applies to **external A-029 T2 reference tasks** (4 rows in `tasks.jsonl`), not automatically to golden `t4_refs` rows (which pass on small corpora).
- Golden restoration rules (T2.4) apply per-row only after replay ≥2/4 with independent sign-off.
- No golden edits in this audit slice.

## Index reference pipeline touchpoints (link-only)

Crosswalk from §6.1 hypothesis classes to code modules (**no changes in T2.1**):

| Hypothesis class | Likely touchpoints | Role |
|------------------|-------------------|------|
| Markdown / docs text | `src/parsing/config_extractors/markdown.rs`, `LanguageId::Markdown` | Config-style extract; xref skips Markdown for `extract_references` |
| Bench / example paths | `src/discovery/mod.rs`, `src/parsing/xref.rs` | Discovery includes `.rs`/`.py`; refs from tree-sitter xref |
| Cross-file text / plain mentions | `src/parsing/xref.rs`, `src/live_index/query.rs` | Structured `ReferenceRecord` vs rg word match |
| Output truncation | `src/protocol/format.rs` `OutputLimits`, `src/stel/planner.rs` `find_references` args | Compact serve caps files/hits in response |
| Query aggregation | `src/live_index/query.rs` `find_references_for_name`, `capture_find_references_view` | Serves grouped refs under read lock |
| Reference kind filter | `src/live_index/disambiguation.rs` | call / import / type_usage / etc. |

## Measurement proxy alignment

| Proxy | A-029 spike | This audit |
|-------|-------------|------------|
| Baseline | `rg -l` on `*.rs` or `*.py` | Same (`scripts/a029-t21-rg-inventory.cjs`) |
| Index output | Compact `symforge` → cited paths | Same (measurement only) |
| Recall | matched baseline files / baseline count | Same |
| Markdown in baseline | **Not included** (rg glob) | Documented limitation; §6.1 md hypothesis needs separate md-glob audit if pursued |

## Conclusion

Program T2 targets **external repo reference parity**, not golden `t4_refs` regression. T2.1 evidence maps missed rg-baseline files to taxonomy rows for post-T019 implementation.
