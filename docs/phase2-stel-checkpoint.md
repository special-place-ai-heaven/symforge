# Phase 2 STEL checkpoint (controller maturity + gate evidence)

**Status:** **Phase 2 closed — exit record PASS on compact surface gates; A-029 PIVOT documented.**  
**Main tip (source of truth):** `a63be80` — *Merge pull request #311 (A-029 T2 spike)*  
**Exit decision date:** 2026-06-14

Phase 2 extends Phase 1 compact-3 STEL with **multi-hop L1**, **four admission states (L2)**, **H3/H4/H5 gate evidence**, **H3 remediation**, and **A-029 T2 spike (PIVOT / P-T2)**. This document is the Phase 2 exit record per [`specs/002-v8-phase2-stel-controller/contracts/phase2-gate-evidence-contract.md`](../specs/002-v8-phase2-stel-controller/contracts/phase2-gate-evidence-contract.md).

This document captures evidence state after merge. It does **not** change runtime behavior.

---

## Exit record (contract)

```yaml
phase: 2
decision: PASS
decision_date: 2026-06-14
main_commit: a63be80
reviewer: Phase 2 evidence producer (Cursor agent session)
golden_replay:
  total_rows: 36
  deferred_multi_hop: 0
  supported_serve: 32
  supported_pff: 4
  artifact: tests/stel_golden_replay.rs
gate_report: docs/research/phase2-gate-report.md
a029_spike: docs/research/A-029-t2-spike.md
assumption_updates:
  A-008: PARTIAL
  A-009: VALIDATED
  A-010: OPEN
  A-011: OPEN
  A-012: PARTIAL
  A-013: VALIDATED
  A-014: OPEN
  A-029: PIVOT
blocking_gaps: []
```

**Phase 2 exit interpretation:**

- **H3 / H4 / H5:** **PASS** on compact-surface battery ([`phase2-gate-report.md`](research/phase2-gate-report.md)).
- **A-029:** **PIVOT** (0/4 T2 equiv); **P-T2** bypass-only policy registered — **not PASS**.
- **H6 / H7 / H8:** **NOT_CLAIMED** (Phase 3–4 / 8.1 program).
- **T2 reference tasks:** bypass-only under P-T2 until **8.1 index-recall program**; `eligible_h6=false` when policy lands.

---

## Merge anchors (Phase 2 slices)

| Slice | PR / commit | Summary |
|-------|-------------|---------|
| **P2-S1/S2** | multi-hop closure (`3d64b96` lineage) | 3 deferred multi-hop rows → SupportedServe; in-process chain |
| **P2-S3** | [#306](https://github.com/special-place-ai-heaven/symforge/pull/306) / `896840f` | L2 admission: serve, degrade, bypass, cache_hit |
| **P2-S4** | [#308](https://github.com/special-place-ai-heaven/symforge/pull/308) / `b1f6019` | H3/H4/H5 gate harness + battery evidence (H3 initially FAIL) |
| **P2-S4.1** | [#309](https://github.com/special-place-ai-heaven/symforge/pull/309) / `c56f669` | H3 remediation — `records/t8_explore` cap; H3 PASS |
| **P2-S5** | [#311](https://github.com/special-place-ai-heaven/symforge/pull/311) / `a63be80` | A-029 T2 spike — **PIVOT** / P-T2 |
| **P2-S6** | (this PR) | Exit docs + assumption register closure |

Prior baseline: [`phase1-stel-checkpoint.md`](phase1-stel-checkpoint.md) (`66742f1`).

---

## Gate evidence (H3 / H4 / H5)

| Gate | Status | Evidence |
|------|--------|----------|
| **H3** | **PASS** | 0 sGteM violations in serve scope (24 rows); [`results-v8-phase2-candidate.json`](research/results-v8-phase2-candidate.json) |
| **H4** | **PASS** | `session_net_accepted = +13753` |
| **H5** | **PASS** | 0 single-chain MCP violations |
| H6–H8 | NOT_CLAIMED | — |

**H3 note:** `records/t8_explore` remediated (S=929, M=1000); ~71-token margin documented — monitor on future explore sizing drift.

**Reproduce:**

```bash
node scripts/compare-results.cjs docs/research/results-v8-phase2-candidate.json --report docs/research/phase2-gate-report.generated.md
cargo test --test stel_battery_gates -- --test-threads=1
```

---

## A-029 T2 spike (PIVOT — not PASS)

| Field | Value |
|-------|-------|
| Verdict | **PIVOT** |
| T2 equiv | **0 / 4** (tokio + django) |
| Pivot policy | **P-T2** — T2 reference tasks bypass-only; `eligible_h6=false` |
| Artifact | [`research/A-029-t2-spike.md`](research/A-029-t2-spike.md) |

**Does not claim:** T2 reference parity on external repos, H6 eligibility, or A-029 PASS.

**Next program work (8.1, out of Phase 2):** index ref-source audit and markdown/bench/import capture per gap plan §6.1 — not runtime masking in Phase 2.

---

## Golden replay (36/36 classification)

| Partition | Count | Status |
|-----------|------:|--------|
| Supported serve | 32 | Replay contract in `tests/stel_golden_replay.rs` |
| Supported P-FF bypass | 4 | P-FF enforcement unchanged |
| Deferred multi-hop | 0 | Closed in P2-S1/S2 |
| Deferred planner mismatch | 0 | — |

Multi-hop checked-in fixtures: `tests/fixtures/stel_multi_hop/`.

---

## Assumption register (A-008..A-014, A-029)

Phase 2 evidence verdicts (full register: [`stel-assumptions.md`](stel-assumptions.md)):

| ID | Phase 2 verdict | Notes |
|----|-----------------|-------|
| **A-008** | **PARTIAL** | Golden `must_call` replay on 32 serve rows; 95% NL trajectory metric not numerically measured |
| **A-009** | **VALIDATED** | Multi-hop internal chain on 3 golden rows; one MCP call preserved |
| **A-010** | **OPEN** | Intent-bucket A/B not run |
| **A-011** | **OPEN** | ±20% token predictor not validated on full battery |
| **A-012** | **PARTIAL** | Serve-only H3 scope documented; H3 PASS on battery; two-hop bypass completion not shipped |
| **A-013** | **VALIDATED** | `cache_hit` path + tests (`tests/stel_l2_admission.rs`) |
| **A-014** | **OPEN** | Degrade caps shipped; T3-large equivalence battery deferred |
| **A-029** | **PIVOT** | 0/4 T2 equiv; P-T2 registered |

---

## Scope audit (T052)

Phase 2 **did not** ship:

| Excluded item | Status |
|---------------|--------|
| SQLite / durable ledger persistence | **Not present** — L4 remains in-memory |
| Calibration EMA → L2 auto-tuning | **Not present** |
| `results-v8-8.0-baseline.json` pin (A-024) | **Not present** |
| B-RESULTS / RESULTS.md §8.7 closure | **Not claimed** |
| H6 / H7 / H8 PASS | **Not claimed** |
| New compact MCP tools (beyond symforge, symforge_edit, status) | **Not added** |
| T2 reference recall remediation | **Not present** — P-T2 policy only |

Phase 2 **did** ship: multi-hop planner/executor, L2 admission states, gate scripts/tests, H3 explore cap, A-029 spike harness.

---

## Evidence index

Master index: [`research/phase2-evidence-index.md`](research/phase2-evidence-index.md).

---

## Deferred to Phase 3+

- Ledger persistence, calibration EMA → L2 (Phase 3)
- B-RESULTS / 8.0 baseline pin (A-024, post–8.0 tag)
- H6/H7/H8 and T2/T3 quality program execution (8.1)
- P-T2 golden row policy enforcement (`eligible_h6=false` on T2 rows)
- Streamable HTTP / unified server (Phase 4)

---

## Reviewer checklist (5 minutes)

1. Golden replay: **0** deferred multi-hop? **Yes**
2. Gate report: **H3 + H4 PASS** on compact? **Yes** (+ H5 PASS)
3. A-029: **PASS or P-T2 pivot** documented? **Yes — PIVOT / P-T2**
4. Assumption register updated A-008..A-014, A-029? **Yes** ([`stel-assumptions.md`](stel-assumptions.md))
5. No persistence / B-RESULTS claims? **Confirmed**

**Phase 2 exit:** **PASS** with truthful **A-029 PIVOT** — not A-029 PASS, not H6 eligibility.
