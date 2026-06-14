# A-029 — TX-02 Structured-Reference / Xref Remediation Evidence

**Program row:** TX-02 / FM-RG-TEXT, FM-SYMBOL (8.1 index-recall)
**SymForge commit (measurement):** `830ad8f` + TX-02 branch (uncommitted xref changes)
**T2.1 before baseline:** [`rg-hits/summary.json`](./rg-hits/summary.json) @ T019 — django 7/17 cited
**Post-TX-01 baseline:** [`a029-tx01-results.json`](./a029-tx01-results.json) — 1/4 equiv, django flat
**Live results JSON:** [`a029-tx02-results.json`](./a029-tx02-results.json)

## Change summary

Python structured-reference capture in `src/parsing/xref.rs` (TX-02 only — no TX-03/TX-04):

| Fix surface | What changed |
|-------------|--------------|
| `models.Model` inheritance | Attribute superclass pattern (`class Foo(models.Model)`) |
| Generic type annotations | `QuerySet[Model]` via `generic_type` + `type_parameter` |
| Attribute type annotations | `models.QuerySet` under `(type (attribute ...))` |
| Call-argument type tokens | `extract_python_value_refs` — uppercase PEP-8 names in arg lists (`isinstance(x, Model)`, `ForeignKey(Model)`) |
| Bare inheritance | Preserved paired `class Foo(Bar)` implements capture |

Rust / tokio xref paths unchanged. Compact serve caps (TX-01) unchanged.

## Before / after (four T2 tasks)

Authoritative replay: `node scripts/a029-t2-spike.cjs` (compact `symforge` chain).

| Task | cited before (TX-01) | cited after (TX-02) | recall before | recall after | equiv after |
|------|----------------------|---------------------|---------------|--------------|-------------|
| `tokio/t2_spawn` | 100 | **100** | 32.9% | **32.9%** | SYMFORGE-LESS |
| `tokio/t2_block_on` | 100 | **100** | 70.9% | **70.9%** | **EQUIVALENT** |
| `django/t2_queryset` | 7 | **19** | 9.9% | **26.8%** | SYMFORGE-LESS |
| `django/t2_model` | 17 | **100** | 4.8% | **18.1%** | SYMFORGE-LESS |

**Matched paths (spike):** `t2_model` 17 → **64**; `t2_queryset` 7 → **19**.

Inventory-only direct `find_references` measurement (rg-hits refresh): `t2_model` matched **100/354 (28.2%)**, `t2_queryset` **19/71 (26.8%)** — index recall higher than compact-chain cited-path extraction for high fan-out `Model`; spike path remains binding for A-029 verdict math.

## Django analysis

| Task | TX-01 | TX-02 | Lift | Cap-bound? |
|------|-------|-------|------|------------|
| `t2_queryset` | 7 cited, 9.9% | 19 cited, 26.8% | **+12 files, +17pp** | No (19 ≪ 100) |
| `t2_model` | 17 cited, 4.8% | 100 cited, 18.1% matched | **+47 matched files, +13pp** | Partially at 100-file serve cap |

**Verdict:** Xref remediation ** materially improved django queryset/model recall** as predicted (TX-02-bound). Primary gains from `models.Model` inheritance + generic/value-position captures. Remaining misses skew to `source` (215) and `test` (39) per refreshed [`rg-hits/django/t2_model.json`](./rg-hits/django/t2_model.json) — **TX-04 (tests/**) and residual text mentions not yet in scope.

## Tokio regression check

| Task | TX-01 recall | TX-02 recall | Regression? |
|------|--------------|--------------|-------------|
| `t2_spawn` | 32.9% | 32.9% | **No** |
| `t2_block_on` | 70.9% equiv | 70.9% equiv | **No** |

TX-01 cap gains **preserved**; Python xref changes do not affect Rust corpora.

## A-029 machine / program verdict

| Field | Value |
|-------|-------|
| **t2_equiv_pass** | **1 / 4** (unchanged) |
| **Machine verdict** | **PIVOT** (`A029Verdict::Pivot`) |
| **Program label** | **PARTIAL** (overlay on Pivot) |
| **A-029 PASS claimed?** | **No** (threshold ≥2/4 not met) |
| **P-T2** | **Retained** (<2/4) |
| **Golden / eligible_h6** | **Unchanged** |
| **T2.4 sign-off for golden rows?** | **Not required** (equiv <2/4) |

Phase 2 baseline [`a029-t2-results.json`](./a029-t2-results.json) (0/4) preserved. Post-TX-01 [`a029-tx01-results.json`](./a029-tx01-results.json) preserved. Post-TX-02 replay: [`a029-tx02-results.json`](./a029-tx02-results.json).

## Next program step

**TX-04** (`tests/**` recall) — django test-bucket misses remain large. **TX-03** (benches) deferred for tokio.

## Verification

| Check | Result |
|-------|--------|
| `cargo fmt --check` | pass |
| `cargo check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets -- --test-threads=1` | pass |
| `cargo build --release` | pass |
| Python xref unit + integration tests | pass |
| Live A-029 replay (4 tasks) | [`a029-tx02-results.json`](./a029-tx02-results.json) |
| rg-hits inventory refresh | [`rg-hits/summary.json`](./rg-hits/summary.json) |
| `git diff --check` | pass |

## Operator re-run

```bash
cargo build --release -p symforge
node scripts/a029-t2-spike.cjs "$(pwd)/target/release/symforge" docs/research/a029-tx02-results.json
node scripts/a029-t21-rg-inventory.cjs "$(pwd)/target/release/symforge"
```
