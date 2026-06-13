# Implementation Plan: SymForge v8 Phase 0 12A Pre-flight

**Branch**: `v8/stel-architecture` | **Date**: 2026-06-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-v8-phase0-preflight/spec.md`

## Summary

Create the evidence workflow that decides whether SymForge v8 may begin the first `src/stel/` implementation commit. The plan is artifact-first: validate the measurement ruler, golden route corpus, gate comparator, schema budget, bypass policy, assumption-register evidence, and independent reviewer sign-off before any STEL implementation begins.

The binding source is `docs/v8-gap-closure-plan.md` section 12A. Companion context comes from `docs/stel-assumptions.md`, `docs/v8-master-plan.md`, `docs/stel-schema.md`, and `docs/ideation.md`.

## Technical Context

**Language/Version**: Rust-native SymForge repository; Phase 0 evidence also uses repository scripts, Markdown, JSON, JSONL, and the sibling measurement harness described in the binding docs.

**Primary Dependencies**: Existing Spec Kit scripts, the SymForge Rust build/test toolchain, the sf-bench measurement corpus/comparator, and existing repo docs. No STEL runtime dependency is introduced by this feature.

**Storage**: File-based planning and evidence artifacts under `specs/001-v8-phase0-preflight/`, `docs/research/`, and the canonical sf-bench artifact locations named by `docs/v8-gap-closure-plan.md`. No new runtime database or persistent product storage is planned for this phase.

**Testing**: Spec Kit prerequisite checks; gate-comparator pre-flight execution; JSON/JSONL schema validation; repeated measurement variance review; manual-baseline spot checks; equivalence audit; schema-byte measurement; independent reviewer sign-off. Standard repository checks remain the guard for any implementation work that later touches source.

**Target Platform**: Current SymForge v8 development branch on the local workstation, with evidence structured so later validation can be reproduced on the pinned corpus and branch binary.

**Project Type**: Rust MCP/code-intelligence server with a planning/evidence feature. This plan does not implement application runtime behavior.

**Performance Goals**: Accepted-session net variance no greater than 2%; manual baseline passes 6 of 6 spot checks; equivalence audit combined false positives and false negatives no greater than 10%; compact public surface no greater than 5,000 schema bytes; edit surface no greater than 1,500 schema bytes or accepted pivot.

**Constraints**: No `src/stel/` implementation before a GO decision; GO requires independent reviewer sign-off; 7.x benchmark results are informational only; Phase 4 deploy/admin/AAP convenience work remains out of scope; binding gap-closure doc wins conflicts.

**Scale/Scope**: 36 golden route rows; at least 10 reviewed golden-route semantic rows; two repeated measurement runs; six manual-baseline spot checks; H1 through H8 fields computable in pre-flight mode; all Phase 1-blocking assumptions mapped to evidence or explicit blockers.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

The Spec Kit constitution file is still a generated placeholder, so it provides no concrete gates. The stricter governing constraints come from repository `AGENTS.md` and the binding SymForge v8 docs.

Pre-research gate status: PASS.

- Spec exists and is bounded to Phase 0 readiness evidence.
- `docs/v8-gap-closure-plan.md` section 12A is binding for scope.
- The plan does not start STEL implementation or alter the public runtime surface.
- Completion requires evidence and independent reviewer sign-off, not self-approval.
- Any failed validation must produce pass, pivot, or kill evidence before forward motion.

Post-design gate status: PASS.

- Research, data model, contract, and quickstart artifacts keep Phase 0 as an evidence workflow.
- Contracts define evidence records and reviewer acceptance, not runtime STEL APIs.
- The quickstart preserves the `src/stel/` stop condition until GO.
- No constitution or repo rule violations require complexity justification.

## Project Structure

### Documentation (this feature)

```text
specs/001-v8-phase0-preflight/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── preflight-evidence-contract.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
docs/
├── v8-gap-closure-plan.md
├── stel-assumptions.md
├── v8-master-plan.md
├── stel-schema.md
├── ideation.md
└── research/
    ├── phase0-12a-evidence-index.md
    ├── phase0-12a-assumption-evidence.md
    ├── phase0-12a-review-signoff.md
    ├── phase0-12a-scope-boundary.md
    ├── A-001-measurement-repeatability.md
    ├── A-002-manual-spotcheck.md
    ├── A-003-harness-shakedown.md
    ├── A-004-equiv-audit.md
    ├── A-005-schema-bytes.json
    ├── A-005-schema-bytes-summary.md
    ├── A-006-host-schema.md
    ├── A-012-bypass-policy.md
    ├── A-019-l0-surface-choice.md
    ├── A-028-golden-routes.md
    ├── A-030-phase-crosswalk.md
    └── G-005-compare-results-preflight.md

scripts/
└── measure-schema-bytes.ps1

../sf-bench/ or configured sf-bench workspace
├── compare-results.js
├── routes.golden.jsonl
├── fixtures/
└── out/
```

**Structure Decision**: Use an evidence-artifact structure rather than a runtime module structure. Source edits during implementation should be limited to docs, research evidence, harness/comparator artifacts, schema-byte measurement helpers, and assumption-register updates until the final GO decision is independently signed off.

## Complexity Tracking

No constitution violations or unnecessary complexity are introduced by this planning design.
