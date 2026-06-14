# A-029 — Django Recall Spike (T2.1 inventory)

**Repo:** django (shallow clone)
**Corpus SHA:** `f1440a752ec034277ccdad914995c3f164308e41`
**SymForge commit:** `470826a`
**Measured:** 2026-06-14

## Tasks

| Task ID | Symbol | min recall | baseline files | cited files | matched | recall | equiv (A-029) |
|---------|--------|------------|----------------|-------------|---------|--------|---------------|
| `django/t2_queryset` | QuerySet | 35% | 71 | **7** | 7 | **9.9%** | SYMFORGE-LESS |
| `django/t2_model` | Model | 25% | 354 | **17** | 17 | **4.8%** | SYMFORGE-LESS |

Artifacts: [`rg-hits/django/t2_queryset.json`](./rg-hits/django/t2_queryset.json), [`rg-hits/django/t2_model.json`](./rg-hits/django/t2_model.json)

## Missed-site bucket summary

From committed `missed_bucket_counts`:

| Task | missed total | source | test |
|------|--------------|--------|------|
| `t2_queryset` | 64 | 48 | 16 |
| `t2_model` | 337 | 293 | 44 |

## Top missed prefixes (`t2_model`)

| Prefix | Count |
|--------|-------|
| `django/contrib` | 27 |
| `tests/migrations` | 19 |
| `django/db` | 15 |
| `tests/gis_tests` | 13 |

## Observations

1. **High fan-out symbol (`Model`):** 354 rg baseline files; **17 cited** — worst recall row (4.8%). **Not cap-bound at 20** (17 < 20).
2. **`QuerySet`:** 71 baseline files, **7 cited** — **not** at compact output file budget (7 ≪ 20). Low recall is **not** explained by FM-CAP.
3. **Django is TX-02-bound:** misses skew to `source` bucket (48 / 293) — structured xref / symbol resolution gaps dominate over output truncation.
4. **FM-CAP / TX-01:** apply TX-01 first for cheap tokio cap proof, then **re-measure per repo**. Django flat recall after TX-01 must not be misread as TX-01 failure — expect django lift from **TX-02** (xref) and **TX-04** (tests).
5. **No bench paths** in django rg baseline for these symbols. **Markdown/docs:** not in rg baseline (`*.py` glob).

## Reproduce

```bash
node scripts/a029-t21-rg-inventory.cjs "$(pwd)/target/debug/symforge"
```

Phase 2 alignment: [`a029-t2-results.json`](./a029-t2-results.json) (same recall 9.9% / 4.8%).

**No A-029 PASS claimed.** P-T2 retained.
