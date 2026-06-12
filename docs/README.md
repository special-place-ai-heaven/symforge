# SymForge v8 documentation

Living design docs for the **8.0.0** effort on branch `v8/stel-architecture`.  
We write ideation down first, validate assumptions, then implement. Details deepen over time.

## Reading order

| Doc | Role |
|-----|------|
| **[`ideation.md`](ideation.md)** | Vision, principles, non-goals, open questions, decision log — **start here** |
| **[`v8-gap-closure-plan.md`](v8-gap-closure-plan.md)** | **Binding pre-flight** — every gap, spike, pivot, harness spec; blocks `src/stel/` until §12 green |
| **[`v8-master-plan.md`](v8-master-plan.md)** | Phased roadmap summary |
| **[`stel-architecture.md`](stel-architecture.md)** | STEL layers, release gates H1–H8, engineering rules |
| **[`stel-schema.md`](stel-schema.md)** | Normative types, controller algorithm, JSON contracts |
| **[`stel-assumptions.md`](stel-assumptions.md)** | Assumption register — **blocks phases until VALIDATED** |

## External measurement

| Artifact | Role |
|----------|------|
| `E:\project\sf-bench\` | Measurement harness (methodology + corpus) |
| `results-v8-8.0-baseline.json` | Pinned at **8.0 tag** — v8 regression reference (not 7.x) |
| `sf-bench/RESULTS.md` | **7.21.1 appendix** — informational only |

## How docs evolve

```text
ideation.md          → why, decisions, open questions
v8-gap-closure-plan  → EVERY gap closed before code (§12 pre-flight)
v8-master-plan       → phase summary
stel-*               → how (architecture, schema, assumptions)
sf-bench             → proof (numbers)
```

When ideation changes, update **`ideation.md` + decision log** first, then ripple to master plan and assumptions if gates or scope shift.

## Phase crosswalk (canonical)

Use **master plan** phase numbers. Other docs must match this table.

| Master plan | stel-architecture | stel-assumptions gates | Delivers |
|-------------|-------------------|------------------------|----------|
| **0** | Phase 0 | A-001..A-007, A-019, A-024 | Harness trust, baseline pin, L0 A/B |
| **1** | Phase 1 | A-005 validated | Compact surface **H1** |
| **2** | Phase 2 | A-008..A-014, A-029 | Router + controller **H3, H4** |
| **3** | Phase 3 | A-015..A-016 | Executor + ledger → **8.0.0** (H1–H5, H7) |
| **4** | Phase 4 | A-020..A-022 | **H6, H8** + `symforge serve` → **8.1.0** |
