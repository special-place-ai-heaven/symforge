# A-029 — TX-04 Test-Path Recall Remediation Evidence

**Program row:** TX-04 / FM-TEST (8.1 index-recall)
**SymForge commit (measurement):** `fe6c42f` + TX-04 branch
**Post-TX-02 baseline:** [`a029-tx02-results.json`](./a029-tx02-results.json) — 1/4 equiv
**Live results JSON:** [`a029-tx04-results.json`](./a029-tx04-results.json)

## Change summary

TX-04 only — no TX-03 bench work, no golden / `eligible_h6` changes.

| Fix surface | What changed |
|-------------|--------------|
| `src/live_index/query.rs` | **Test-lane fair ordering** in `build_find_references_view`: interleave test and non-test file paths so compact `find_references` output is not starved by lexicographic ordering under the 100-file cap |
| `src/parsing/xref.rs` | **Test-idiomatic Python xref**: attribute chains (`models.Model.__hash__`), string class tokens in call args (`RenameField("Model", …)`), dotted mock paths (`"django.db.models.Model"`) |

Discovery/admission unchanged — tests were already Tier 1 indexed; gap was output ordering + test-heavy xref patterns.

## Before / after (four T2 tasks)

Authoritative replay: `node scripts/a029-t2-spike.cjs` (compact `symforge` chain).

| Task | recall (TX-02) | recall (TX-04) | equiv (TX-04) |
|------|----------------|----------------|---------------|
| `tokio/t2_spawn` | 32.9% | **34.5%** | SYMFORGE-LESS (need 35%) |
| `tokio/t2_block_on` | 70.9% | **70.9%** | **EQUIVALENT** |
| `django/t2_queryset` | 26.8% | **26.8%** | SYMFORGE-LESS |
| `django/t2_model` | 18.1% | **28.2%** | **EQUIVALENT** |

**Overall:** **2/4** equivalence (was 1/4 after TX-02).

## Test-path analysis

### Recovered (examples)

| Pattern | Mechanism | Example |
|---------|-----------|---------|
| `tokio/tests/**` spawn calls | Test-lane interleaving | `matched_bucket_counts.test`: **30** files on `tokio/t2_spawn` (was **0** cited pre-TX-04) |
| `tests/**/models.py` `models.Model` | TX-02 xref + fair ordering | `django/t2_model` matched test bucket **10** files in first 100 cited |
| Migration string `"Model"` | `extract_python_string_type_refs` | `tests/migrations/test_operations.py`-class paths |
| `mock.patch("django.db.models.Model")` | Dotted string terminal segment | Serializer/admin test modules |

### Remaining (why)

| Bucket / prefix | Count (django `t2_model`) | Why still missed |
|---------------|---------------------------|------------------|
| `tests/migrations` | 19 top-missed prefix | Many rg hits are **string prose / operation labels**, not structured refs |
| `test` bucket (missed) | 44 | rg text ⊃ structured refs; docstrings, comments, lowercase `"model"` keys |
| `source` bucket (missed) | 210 | Cap at 100 cited files; high fan-out symbol |
| `tokio/t2_spawn` test misses | 125 | Still majority of baseline; **TX-03 benches** not in scope; need further xref or cap policy |

## Regression checks

| Slice | TX-02 → TX-04 | OK? |
|-------|---------------|-----|
| tokio `t2_block_on` | 70.9% equiv | **Yes** |
| tokio `t2_spawn` | 32.9% → 34.5% | **Improved** |
| django TX-02 xref (`t2_queryset`) | 26.8% flat | **Preserved** |

## A-029 machine / program verdict

| Field | Value |
|-------|-------|
| **t2_equiv_pass** | **2 / 4** |
| **Machine verdict (replay)** | **Pass** (`A029Verdict::Pass` at ≥2/4 threshold) |
| **Golden / eligible_h6** | **Unchanged in this PR** |
| **T2.4 sign-off** | **Required** before any golden-row restoration or `eligible_h6=true` |
| **P-T2** | Replay meets pass threshold; **policy row restoration deferred** to T2.4 |
| **`docs/stel-assumptions.md`** | **Not updated** in TX-04 (T2.4 exit scope) |

Phase 2 baseline [`a029-t2-results.json`](./a029-t2-results.json) and post-TX-01/TX-02 artifacts preserved.

## Next program step

**T2.4 replay sign-off** for policy/golden reconsideration. **TX-03** (bench indexing) remains separate.

## Verification

| Check | Result |
|-------|--------|
| `cargo fmt --check` | pass |
| `cargo check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets -- --test-threads=1` | pass |
| `cargo build --release` | pass |
| Live A-029 replay (4 tasks) | [`a029-tx04-results.json`](./a029-tx04-results.json) |
| rg-hits inventory | [`rg-hits/summary.json`](./rg-hits/summary.json) |
| `git diff --check` | pass |

## Operator re-run

```bash
cargo build --release -p symforge
node scripts/a029-t2-spike.cjs "$(pwd)/target/release/symforge" docs/research/a029-tx04-results.json
node scripts/a029-t21-rg-inventory.cjs "$(pwd)/target/release/symforge"
```
