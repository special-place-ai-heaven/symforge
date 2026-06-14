# A-029 — T2 equivalence spike (Phase 2 P2-S5)

**Spike ID:** A-029-phase2-2026-06-14  
**Surface:** `compact` (`SYMFORGE_SURFACE=compact`)  
**Baseline commit:** `061583cbc8bc87d9a133a3a44ff24d44e03a1abd` (post-#309 H3 remediation)  
**Machine-readable results:** [`a029-t2-results.json`](./a029-t2-results.json)  
**Task definitions:** [`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl)

## Verdict

| Field | Value |
|-------|-------|
| **A-029 verdict** | **PIVOT** |
| **t2_equiv_pass** | 0 / 4 |
| **Pivot policy** | **P-T2** — T2 reference tasks bypass-only (grep envelope; `eligible_h6=false`) |
| **H6 impact** | Remove 4 T2 rows from H6 eligible denominator when P-T2 registered (per gap plan §6.1) |

**PASS threshold not met** (requires ≥2/4 T2 equivalence on tokio+django). Spike does **not** claim T2 reference parity; registers P-T2 pivot per binding gap plan §4.2.

## Method (T040)

1. Clone shallow **tokio** + **django** per [`tests/fixtures/a029-t2/README.md`](../../tests/fixtures/a029-t2/README.md).
2. Define **4 T2 tasks** (`find_references` routing) in `tasks.jsonl` — 2 per repo.
3. For each task:
   - Build **rg baseline**: unique `.rs`/`.py` files referencing symbol (sidecar-parity proxy).
   - `index_folder` corpus; one compact **`symforge`** call per query.
   - Extract cited file paths from response; compute **baseline_recall** = matched / baseline_paths.
   - **EQUIVALENT** iff `decision=serve`, `find_references` routed, recall ≥ task `min_baseline_recall`.
4. Verdict: `PASS` if ≥2 equiv; else `PIVOT` if tasks measured; else `KILL`.

**Operator command:**

```bash
cargo build -p symforge
node scripts/a029-t2-spike.cjs target/debug/symforge docs/research/a029-t2-results.json
```

**Deterministic verdict math:** `src/stel/a029.rs` + `tests/stel_a029_spike.rs`

## Target set (T041)

| ID | Repo | Query | Symbol | min recall |
|----|------|-------|--------|------------|
| `tokio/t2_spawn` | tokio | who references spawn | spawn | 35% |
| `tokio/t2_block_on` | tokio | references to block_on | block_on | 35% |
| `django/t2_queryset` | django | who references QuerySet | QuerySet | 35% |
| `django/t2_model` | django | references to Model | Model | 25% |

## Results (T042)

| Row | decision | tool | baseline files | matched | recall | equiv |
|-----|----------|------|----------------|---------|--------|-------|
| `tokio/t2_spawn` | serve | find_references | 252 | 18 | 7.1% | SYMFORGE-LESS |
| `tokio/t2_block_on` | serve | find_references | 141 | 20 | 14.2% | SYMFORGE-LESS |
| `django/t2_queryset` | serve | find_references | 71 | 7 | 9.9% | SYMFORGE-LESS |
| `django/t2_model` | serve | find_references | 354 | 17 | 4.8% | SYMFORGE-LESS |

**Observations:**

- **Routing correct:** all 4 rows `decision=serve` with `find_references` (H5 preserved; no runtime changes in this slice).
- **Reference parity gap:** index-backed `find_references` surfaces a small fraction of rg baseline files (markdown, benches, cross-module imports likely missing — aligns with gap plan §6.1 root-cause hypothesis).
- **Not a compact-surface regression:** in-repo phase0 golden `t4_refs` rows remain EQUIVALENT on small corpora (see P2-S4 battery).

## P-T2 pivot recommendation

Per [`docs/v8-gap-closure-plan.md`](../v8-gap-closure-plan.md) §6.1:

1. Register **P-T2**: T2 reference tasks become mandatory **bypass** with host grep envelope + line window.
2. Set `eligible_h6=false` on T2 golden rows when policy lands (4 rows).
3. **Next program work (8.1, out of P2-S5):** index ref-source audit (`live_index/query.rs`), markdown/bench/import reference capture — do **not** mask with runtime hacks in Phase 2 spike slice.

## T043 — T3 large-row degrade (A-014 note)

**Deferred in this spike run.** In-repo golden `t2_context` outline rows serve on small corpora; full T3-large degrade vs competent-manual window validation remains **A-014 OPEN** for Phase 3 / 8.1 program. No degrade regression introduced in P2-S5 (no runtime changes).

## H3/H4/H5 preservation

P2-S5 does not modify gate math or compact runtime. Post-#309 evidence unchanged:

- **H3 PASS** (0 violations; `records/t8_explore` S=929, M=1000, ~71-token margin noted)
- **H4 PASS** (`session_net_accepted = +13753`)
- **H5 PASS**

Re-verify:

```bash
node scripts/compare-results.cjs docs/research/results-v8-phase2-candidate.json --report docs/research/phase2-gate-report.generated.md
```

## Scope boundaries (confirmed)

- No A-029 runtime remediation (no index/planner changes)
- No persistence / SQLite / EMA→L2
- No B-RESULTS / §8.7 / H6–H8 claims
- No new MCP tools; compact-3 names unchanged

## Assumption register (A-029)

**Status:** **PIVOT** — T2 reference parity not validated on tokio+django; P-T2 bypass-only policy recommended before H6 eligibility claims on T2 rows.
