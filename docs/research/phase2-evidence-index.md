# Phase 2 STEL evidence index

**Status:** **Phase 2 exit — PASS** (H3/H4/H5 PASS; A-029 PIVOT documented) — 2026-06-14

**Created**: 2026-06-14

## Spec Kit inputs (T001)

| Artifact | Path |
|----------|------|
| Feature spec | [`specs/002-v8-phase2-stel-controller/spec.md`](../../specs/002-v8-phase2-stel-controller/spec.md) |
| Implementation plan | [`specs/002-v8-phase2-stel-controller/plan.md`](../../specs/002-v8-phase2-stel-controller/plan.md) |
| Research decisions | [`specs/002-v8-phase2-stel-controller/research.md`](../../specs/002-v8-phase2-stel-controller/research.md) |
| Data model | [`specs/002-v8-phase2-stel-controller/data-model.md`](../../specs/002-v8-phase2-stel-controller/data-model.md) |
| Quickstart | [`specs/002-v8-phase2-stel-controller/quickstart.md`](../../specs/002-v8-phase2-stel-controller/quickstart.md) |
| Gate evidence contract | [`specs/002-v8-phase2-stel-controller/contracts/phase2-gate-evidence-contract.md`](../../specs/002-v8-phase2-stel-controller/contracts/phase2-gate-evidence-contract.md) |
| Requirements checklist | [`specs/002-v8-phase2-stel-controller/checklists/requirements.md`](../../specs/002-v8-phase2-stel-controller/checklists/requirements.md) |
| Tasks | [`specs/002-v8-phase2-stel-controller/tasks.md`](../../specs/002-v8-phase2-stel-controller/tasks.md) |

## Baseline

- Phase 1 shipped: [`phase1-stel-checkpoint.md`](../phase1-stel-checkpoint.md)
- Phase 2 Slice 1 merged: multi-hop golden closure
- Phase 2 Slice 2 merged: PR #306 / `896840f` (L2 admission hardening)
- Phase 2 Slice 3 merged: PR #308 / `b1f6019` (H3/H4/H5 gate harness)
- Phase 2 Slice 4 merged: PR #309 / `c56f669` (H3 remediation)
- Phase 2 Slice 5 merged: PR #311 / `a63be80` (A-029 PIVOT / P-T2)
- **Phase 2 exit:** [`phase2-stel-checkpoint.md`](../phase2-stel-checkpoint.md)

## T002 spec reviewer sign-off

| Field | Value |
|-------|-------|
| Artifact | [`phase2-spec-review-signoff.md`](./phase2-spec-review-signoff.md) |
| Decision | **GO** (2026-06-14) |
| Baseline commit | `bc738c3` (PR #303 merge) |
| First slice | P2-S1/P2-S2 — multi-hop golden closure (T010–T016) |

## Evidence slots

| Slot | Path | Status |
|------|------|--------|
| **Phase 2 checkpoint** | [`phase2-stel-checkpoint.md`](../phase2-stel-checkpoint.md) | **COMPLETE** — exit PASS; A-029 PIVOT |
| Gate report (curated) | [`phase2-gate-report.md`](./phase2-gate-report.md) | **COMPLETE** — H3/H4/H5 PASS |
| Gate report (generated) | [`phase2-gate-report.generated.md`](./phase2-gate-report.generated.md) | **COMPLETE** — compare-results script output |
| Candidate battery JSON | [`results-v8-phase2-candidate.json`](./results-v8-phase2-candidate.json) | **COMPLETE** — 36/36 rows, STEL fields |
| Compare-results script | [`scripts/compare-results.cjs`](../../scripts/compare-results.cjs) | **COMPLETE** |
| Compact battery script | [`scripts/phase2-compact-battery.cjs`](../../scripts/phase2-compact-battery.cjs) | **COMPLETE** |
| Gate computation (Rust) | [`src/stel/gates.rs`](../../src/stel/gates.rs) | **COMPLETE** |
| Gate test fixtures | [`tests/fixtures/phase2-gate/`](../../tests/fixtures/phase2-gate/) | **COMPLETE** |
| Gate integration tests | [`tests/stel_battery_gates.rs`](../../tests/stel_battery_gates.rs) | **COMPLETE** |
| A-029 spike | [`A-029-t2-spike.md`](./A-029-t2-spike.md) | **COMPLETE** — PIVOT (0/4 T2 equiv; P-T2) |
| A-029 results JSON | [`a029-t2-results.json`](./a029-t2-results.json) | **COMPLETE** |
| A-029 spike script | [`scripts/a029-t2-spike.cjs`](../../scripts/a029-t2-spike.cjs) | **COMPLETE** |
| A-029 verdict (Rust) | [`src/stel/a029.rs`](../../src/stel/a029.rs) | **COMPLETE** |
| Assumption register | [`stel-assumptions.md`](../stel-assumptions.md) | **COMPLETE** — A-008..A-014, A-029 updated |
| Exit record | per gate evidence contract | **COMPLETE** — see checkpoint |

## P2-S4 gate commands (operator)

```bash
cargo build -p symforge
node scripts/phase2-compact-battery.cjs target/debug/symforge docs/research/results-v8-phase2-candidate.json
node scripts/compare-results.cjs docs/research/results-v8-phase2-candidate.json --report docs/research/phase2-gate-report.generated.md
cargo test --test stel_battery_gates -- --test-threads=1
```

## P2-S5 A-029 commands (operator)

```bash
cargo build -p symforge
node scripts/a029-t2-spike.cjs target/debug/symforge docs/research/a029-t2-results.json
cargo test --test stel_a029_spike -- --test-threads=1
```

Requires tokio + django clones per [`tests/fixtures/a029-t2/README.md`](../../tests/fixtures/a029-t2/README.md). Exit 0 on PASS; exit 1 on PIVOT/KILL (truthful non-pass).

Curated reviewer narrative lives in [`phase2-gate-report.md`](./phase2-gate-report.md); re-running compare-results writes only [`phase2-gate-report.generated.md`](./phase2-gate-report.generated.md).

## Local corpus note (golden replay)

`cargo test --test stel_golden_replay` may fail corpus-gated rows when `tests/fixtures/phase0-corpus/*` is missing or differs from the pinned clone content (empty index / not-found outcomes). This reproduces at the P2-S4 parent commit (`896840f`) and is **not caused by P2-S4** (gate code + ledger metadata only). Checked-in gate fixtures (`tests/fixtures/phase2-gate/`) and compare-results math remain deterministic; multi-hop replay against checked-in `tests/fixtures/stel_multi_hop/` passes in CI.

## Phase 2 exit summary

| Item | Result |
|------|--------|
| H3 / H4 / H5 | **PASS** |
| A-029 | **PIVOT** (0/4 T2 equiv) |
| P-T2 | T2 reference tasks bypass-only until 8.1 program |
| H6 eligibility | **Not claimed** |
| ~71-token H3 margin | `records/t8_explore` — noted in gate report |

## Deferred from Phase 2 (explicit)

- Calibration / ledger persistence → Phase 3
- B-RESULTS / RESULTS.md §8.7 → post–8.0 tag (A-024)
- H6/H7/H8 PASS → Phase 3–4 / 8.1
- T2 index-recall remediation → 8.1 program (not Phase 2 runtime)
