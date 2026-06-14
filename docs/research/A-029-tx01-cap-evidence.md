# A-029 — TX-01 Cap Remediation Evidence

**Program row:** TX-01 / FM-CAP (8.1 index-recall)
**SymForge commit (measurement):** `3e0dc78` + TX-01 branch
**T2.1 before baseline:** [`rg-hits/summary.json`](./rg-hits/summary.json) @ T019 (`470826a`) — cited 20/20 tokio
**Live results JSON:** [`a029-tx01-results.json`](./a029-tx01-results.json)

## Change summary

Compact `symforge` **serve** path raises `find_references` output budget for trace queries:

| Constant | Value | Location |
|----------|-------|----------|
| `COMPACT_SERVE_FIND_REFERENCES_FILE_LIMIT` | **100** | `src/stel/executor.rs` |
| `COMPACT_SERVE_FIND_REFERENCES_MAX_PER_FILE` | **10** | `src/stel/executor.rs` |

Applied via `apply_compact_serve_caps` (serve-only) and planner default args for `find_references`. No fabricated references — only additional indexed hits from existing `FindReferencesView`.

## Before / after (four T2 tasks)

| Task | cited (before) | cited (after) | recall before | recall after | equiv after |
|------|----------------|---------------|---------------|--------------|-------------|
| `tokio/t2_spawn` | 20 | **100** | 6.3% | **32.9%** | SYMFORGE-LESS (need 35%) |
| `tokio/t2_block_on` | 20 | **100** | 14.2% | **70.9%** | **EQUIVALENT** |
| `django/t2_queryset` | 7 | **7** | 9.9% | **9.9%** | SYMFORGE-LESS |
| `django/t2_model` | 17 | **17** | 4.8% | **4.8%** | SYMFORGE-LESS |

**Measured:** 2026-06-14 · compact surface · `scripts/a029-t2-spike.cjs` + refreshed [`rg-hits/`](./rg-hits/)

## TX-01 assessment

| Repo | TX-01 effect | Interpretation |
|------|--------------|----------------|
| **Tokio** | **Large recall lift**; cited files **20 → 100** (new cap) | FM-CAP confirmed; `t2_block_on` reaches equiv; `t2_spawn` near threshold (32.9% vs 35%) |
| **Django** | **Flat** (7 / 17 cited unchanged) | **TX-02-bound** as predicted — cap was not the limiting factor |

## A-029 machine / program verdict

| Field | Value |
|-------|-------|
| **t2_equiv_pass** | **1 / 4** |
| **Machine verdict** | **PIVOT** (`A029Verdict::Pivot`) |
| **Program label** | **PARTIAL** (1/4 — overlay on Pivot; not a new enum variant) |
| **A-029 PASS claimed?** | **No** (threshold ≥2/4 not met) |
| **P-T2** | **Retained** |
| **Golden / eligible_h6** | **Unchanged** (T2.4 sign-off required even if ≥2/4 later) |

Phase 2 baseline artifact [`a029-t2-results.json`](./a029-t2-results.json) (0/4 PIVOT) is **preserved**; post-TX-01 replay is in [`a029-tx01-results.json`](./a029-tx01-results.json).

## Next program step

**TX-02** (xref / structured refs) — django-primary; expected to move `QuerySet` / `Model` rows.

## Verification (this PR)

| Check | Result |
|-------|--------|
| `cargo fmt --check` | pass |
| `cargo check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets -- --test-threads=1` | pass |
| `cargo build --release` | pass |
| `apply_compact_serve_caps_find_references_tx01_file_limit` | pass |
| Live A-029 replay (4 tasks) | [`a029-tx01-results.json`](./a029-tx01-results.json) |

## Operator re-run

```bash
cargo build --release -p symforge
node scripts/a029-t2-spike.cjs target/release/symforge docs/research/a029-tx01-results.json
node scripts/a029-t21-rg-inventory.cjs "$(pwd)/target/release/symforge"
```
