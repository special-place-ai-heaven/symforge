# Phase 2 STEL evidence index

**Status:** P2-S4 battery gates — H4/H5 PASS; H3 FAIL documented (2026-06-14)

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
- Phase 2 Slice 1 merged: `3d64b96` (multi-hop golden closure)
- Phase 2 Slice 2 merged: PR #306 / `896840f` tip (L2 admission hardening)

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
| Gate report | [`phase2-gate-report.md`](./phase2-gate-report.md) | **COMPLETE** — H3 FAIL (1 row), H4/H5 PASS |
| Candidate battery JSON | [`results-v8-phase2-candidate.json`](./results-v8-phase2-candidate.json) | **COMPLETE** — 36/36 rows, STEL fields |
| Compare-results script | [`scripts/compare-results.cjs`](../../scripts/compare-results.cjs) | **COMPLETE** |
| Compact battery script | [`scripts/phase2-compact-battery.cjs`](../../scripts/phase2-compact-battery.cjs) | **COMPLETE** |
| Gate computation (Rust) | [`src/stel/gates.rs`](../../src/stel/gates.rs) | **COMPLETE** |
| Gate test fixtures | [`tests/fixtures/phase2-gate/`](../../tests/fixtures/phase2-gate/) | **COMPLETE** |
| Gate integration tests | [`tests/stel_battery_gates.rs`](../../tests/stel_battery_gates.rs) | **COMPLETE** |
| A-029 spike | `docs/research/A-029-t2-spike.md` | NOT STARTED (P2-S5 blocked) |
| Phase 2 checkpoint | `docs/phase2-stel-checkpoint.md` | NOT STARTED (P2-S6) |
| Exit record | per gate evidence contract | IN_PROGRESS |

## P2-S4 gate commands (operator)

```bash
cargo build -p symforge
node scripts/phase2-compact-battery.cjs target/debug/symforge docs/research/results-v8-phase2-candidate.json
node scripts/compare-results.cjs docs/research/results-v8-phase2-candidate.json --report docs/research/phase2-gate-report.md
cargo test --test stel_battery_gates -- --test-threads=1
```

## Deferred from Phase 2 (explicit)

- Calibration / ledger persistence → Phase 3
- B-RESULTS / RESULTS.md §8.7 → post–8.0 tag (A-024)
- H6/H7/H8 PASS → Phase 3–4
- A-029 T2 spike → P2-S5 (blocked until P2-S4 merge)
