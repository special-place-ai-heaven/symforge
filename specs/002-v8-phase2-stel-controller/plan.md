# Implementation Plan: SymForge v8 Phase 2 STEL Controller Maturity

**Branch**: `cursor/v8-phase2-stel-controller` (planned) | **Date**: 2026-06-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/002-v8-phase2-stel-controller/spec.md`

**Baseline**: Phase 1 shipped on `main` — see [`docs/phase1-stel-checkpoint.md`](../../docs/phase1-stel-checkpoint.md)

## Summary

Extend STEL from Phase 1 truthful compact-3 single-step behavior to Phase 2 controller maturity. The plan is schema-first and evidence-gated: close the three deferred multi-hop golden rows, harden L2 admission (`serve | degrade | bypass | cache_hit`), prove H3/H4/H5 on compact-surface battery output, and run A-029 T2/T3 spike — without Phase 3 persistence or B-RESULTS closure.

Binding sources: [`docs/v8-gap-closure-plan.md`](../../docs/v8-gap-closure-plan.md) §7 Phase 2, [`docs/stel-schema.md`](../../docs/stel-schema.md) S5–S6, [`docs/stel-assumptions.md`](../../docs/stel-assumptions.md) A-008..A-014 and A-029.

## Technical Context

**Language/Version**: Rust-native SymForge (`src/stel/`, `src/protocol/tools.rs` compact handlers); sf-bench battery + compare-results for gate evidence.

**Primary Dependencies**: Phase 1 STEL modules (planner, controller, executor, ledger); `smart_query` routing seed; golden corpus `docs/fixtures/routes.golden.jsonl`; sf-bench measurement harness (external or configured workspace).

**Storage**: In-memory session ledger only (Phase 1 contract preserved). Phase 2 adds research artifacts under `docs/research/` and optional battery JSON under operator-controlled paths — **no** new durable STEL store in Phase 2.

**Testing**: `tests/stel_golden_replay.rs` (36-row classification); new multi-hop replay cases; L2 admission unit tests; existing STEL integration suites must remain green; compact-surface battery diff for H3/H4/H5.

**Target Platform**: Compact surface (`SYMFORGE_SURFACE=compact`) on local sf-bench corpus; CI runs unit/integration tests without full battery unless scheduled/manual calibration workflow.

**Project Type**: Rust MCP server — STEL layer maturity increment inside existing `symforge` facade.

**Performance Goals**: H3 PASS (zero sGteM on accepted small-file serve rows per policy); H4 PASS (`session_net_accepted ≥ 0`); H5 PASS for single-chain golden rows; token estimate within ±20% on battery sample (A-011).

**Constraints**: No calibration persistence; no B-RESULTS; no compact surface expansion; preserve guarded `symforge_edit` apply; binding gap-closure plan wins conflicts; implementation blocked until this spec is approved.

**Scale/Scope**: 3 multi-hop golden rows; 4 admission states; H3/H4/H5 gate evidence; A-029 spike (≥2/4 T2 or P-T2 pivot); assumption updates A-008..A-014, A-029.

## Constitution Check

*GATE: Must pass before Phase 2 research/design. Re-check before implementation slices merge.*

Pre-research gate status: **PASS**

- Spec is bounded to Phase 2 controller maturity; Phase 3 persistence explicitly excluded.
- Phase 1 baseline on `main` is documented; no re-litigation of compact-3 L0 choice (A-019 VALIDATED).
- Exit criteria tie to binding H3/H4/H5 gates, not subjective feature count.
- B-RESULTS and 8.0 baseline pin remain out of scope (A-024).

Post-design gate status: **PASS** (pending spec approval)

- Data model extends existing STEL types; no new MCP tools required for Phase 2 exit.
- Contracts define battery and spike evidence, not premature RESULTS closure.
- Quickstart preserves stop conditions for persistence and B-RESULTS scope creep.

## Project Structure

### Documentation (this feature)

```text
specs/002-v8-phase2-stel-controller/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── phase2-gate-evidence-contract.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source Code (repository root — implementation phase only)

```text
src/stel/
├── planner.rs          # extend: multi-step StelPlan builder
├── controller.rs       # harden: serve | degrade | bypass | cache_hit
├── executor.rs         # extend: multi-step in-process chain
├── types.rs            # extend: multi-step plan types if needed
├── golden_replay.rs    # remove DEFERRED_MULTI_HOP classification
└── ...

src/protocol/tools.rs   # symforge_stel_handler: multi-step dispatch

tests/
├── stel_golden_replay.rs
├── stel_multi_hop.rs   # optional dedicated suite
└── stel_l2_admission.rs # optional dedicated suite

docs/research/
├── A-029-t2-spike.md
└── phase2-gate-report.md

docs/
└── phase2-stel-checkpoint.md   # created at Phase 2 exit
```

**Structure Decision**: Extend existing `src/stel/` modules rather than new top-level crates. Evidence artifacts live in `docs/research/` mirroring Phase 0 pattern. Battery JSON paths reference sf-bench workspace; not committed if large/ephemeral.

## Complexity Tracking

No constitution violations. Multi-step internal chaining is required by golden corpus and H5 (one external MCP call); complexity is justified by binding Phase 2 exit gates.

## Phase Boundaries (normative)

| Work | Phase 2 | Phase 3+ |
|------|---------|----------|
| Multi-step L1 + in-process executor | **Yes** | — |
| L2 admission hardening | **Yes** | Calibration fudge (A-016) |
| H3/H4/H5 battery PASS | **Yes** | H6/H7/H8 |
| A-029 spike | **Yes** | Full T2/T3 program |
| In-memory ledger | **Yes** (preserve) | Persist + analytics |
| B-RESULTS / §8.7 | **No** | After 8.0 tag |
| 8.0 baseline pin | **No** | A-024 at tag |

## Implementation Order (recommended)

| Step | Deliverable | Proof |
|------|-------------|-------|
| **P2-S1** | Multi-step `StelPlan` + planner for 3 golden rows | Golden replay 36/36; 0 deferred multi-hop |
| **P2-S2** | In-process multi-step executor (one MCP call) | Integration tests; H5 on single-chain rows |
| **P2-S3** | L2 `cache_hit`, `degrade`, non-P-FF `bypass` hardening | Unit tests per admission state |
| **P2-S4** | sf-bench STEL row fields + compact battery run | compare-results H3/H4/H5 report |
| **P2-S5** | A-029 spike artifact | `A-029-t2-spike.md` PASS or P-T2 pivot |
| **P2-S6** | Assumption register + Phase 2 checkpoint doc | Reviewer sign-off |

**Nothing in P2-S4 merges without P2-S1..S3 green in CI.**
