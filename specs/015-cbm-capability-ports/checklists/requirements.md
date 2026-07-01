# Specification Quality Checklist: CBM Capability Ports

**Purpose**: Validate specification completeness before implementation  
**Created**: 2026-06-29  
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] Focused on user/agent value (graph intelligence without second index)
- [x] Scope bounded with explicit out-of-scope section
- [x] Written for program planning (multi-sprint)
- [x] All mandatory spec sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements testable (FR-001 through FR-017)
- [x] Success criteria measurable (SC-001 through SC-007)
- [x] Edge cases covered in user stories
- [x] Dependencies on 007/011/012 documented
- [x] Constitution constraints explicit (no Soul Map)

## Feature Readiness

- [x] 15 user stories with independent tests
- [x] Sprint map in sprints.md
- [x] 159 tasks in tasks.md with file paths ([P] 91 / [C] 41 / [V] 27)
- [x] Contracts for all major surfaces
- [x] quickstart.md runnable validation per sprint

## Program Artifacts

- [x] spec.md (+ Clarifications 2026-06-29)
- [x] plan.md (+ execution-model.md)
- [x] research.md
- [x] data-model.md (+ Appendix A SymbolId)
- [x] quickstart.md
- [x] tasks.md
- [x] analyze.md (Speckit analyze — 0 critical)
- [x] sprints.md
- [x] contracts/ (7 files)
- [x] planning/ (sprint specs 0–6, matrices, program-planning-gate.md, **parity-backlog.md**)

## Speckit chain (program planning)

- [x] specify → spec.md
- [x] clarify → spec § Clarifications; D-015-009/011/012
- [x] plan → plan.md + phase-1 artifacts
- [x] checklist → this file
- [x] tasks → tasks.md
- [x] analyze → analyze.md
- [ ] **S1a [P] in progress** — wave 1 mostly done; fixtures + gate remain
- [ ] implement → **after** S0 GO + S1a Planning Gate (not parallel with operator benchmarks)
- [ ] taskstoissues → optional

## Parallel tracks

| Track | Owner | This session |
|-------|-------|--------------|
| Speckit `[P]` | Agent | S1a contracts, fixtures, gate prep |
| CBM vs SymForge benchmarks | Operator terminal | [benchmark-intake.md](./planning/benchmark-intake.md) |
| S0 `[C]` spike | Deferred | Until planning gates clear or operator requests |

## Execution model

- [x] 60% planning / 30% coding / 10% validation defined
- [x] Planning Gate + Release Gate checklists per sprint
- [x] No [C] before [P] rule in tasks.md
- [x] code-evidence.md + drift policy
- [x] PROG + S0 [P] complete (program-planning-gate.md)

## Notes

- **Planning track**: S1a `[P]` wave 1 complete; wave 2 (fixtures, test skeletons, gate) next.
- **Operator track**: benchmark results → [planning/benchmark-intake.md](./planning/benchmark-intake.md).
- **Coding blocked** until S0 GO (spike) + S1a Planning Gate per execution-model.
- Re-run analyze before S1a gate sign-off.
