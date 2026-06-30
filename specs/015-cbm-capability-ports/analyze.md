# Specification Analysis Report — Program 015

**Feature**: `specs/015-cbm-capability-ports`  
**Date**: 2026-06-29  
**Speckit**: `/speckit-analyze` (post-clarify)  
**Constitution**: `.specify/memory/constitution.md` v1.0.0

> **Patched 2026-06-30** (operator): task counts + S1a/S1b coverage IDs realigned to tasks.md (159 tasks; S1 split into S1a/S1b). The 2026-06-29 Findings table below is the historical record.
>
> **Manual consistency pass 2026-06-30** (speckit agent): the `/speckit-analyze`
> command is **not installed in this environment** (`.specify/` ships only the
> agent-context extension + templates), so the automated re-run could not be
> performed. A manual equivalent was done before the S1a Planning Gate and is
> recorded below — no automated artifact was fabricated.
>
> **Manual pass result (clean):**
> - **Counts** 159 (91 `[P]` · 41 `[C]` · 27 `[V]`) agree across tasks.md summary
>   table + per-section line count, this file, checklists/requirements.md, and the
>   task-index.md rollup.
> - **Coverage** 17/17 FR + 5/5 NFR each map to ≥1 existing `[C]`/`[V]` task.
> - **Operator residual fixed**: tasks.md:144 archived map `P-S1-013 → P-S4-008`
>   corrected to **P-S4-007** (the real task carrying that legacy tag).
> - **Contracts** detect-impact.md + team-artifact.md **frozen 2026-06-30**;
>   `DetectImpactInput` consistent between contract and sprint-1a spec; team
>   artifact now carries a code-backed R-14 security clause.
> - **New findings**: 0 critical · 0 high.

## Findings

| ID | Category | Severity | Location(s) | Summary | Remediation |
|----|----------|----------|-------------|---------|-------------|
| I1 | Inconsistency | MEDIUM | tasks.md header vs summary | Header ~167 tasks; table 83+41+26=150; actual lines 84+41+26=**151** | **Fixed** — unified counts in tasks.md + checklist |
| I2 | Inconsistency | MEDIUM | checklists/requirements.md | Line 27 "110 tasks" contradicts line 38 "166 tasks" | **Fixed** — single count 151 |
| I3 | Inconsistency | MEDIUM | sprint-1-quick-wins-spec.md | References P-S1-022, P-S1-031, P-S1-032 not in tasks.md | **Fixed** — mapped to P-S1-011, P-S1-012 |
| I4 | Inconsistency | LOW | cbm-source-map.md | Task IDs P-S1-020, P-S1-030, P-S2-015, P-S3-015 don't exist | **Fixed** — aligned to tasks.md IDs |
| I5 | Inconsistency | LOW | sprint-0-spike-spec.md | Linked tasks P-S0-001..015; only P-S0-001..010 exist | **Fixed** — P-S0-001..010 |
| C1 | Coverage | LOW | spec FR-001..017 | All FRs mapped to sprint [C] tasks in tasks.md | No action |
| C2 | Constitution | — | spec/plan/tasks | No SQLite Soul Map; frecency NFR-002; embed NFR-003 | **Pass** |
| U1 | Underspec | LOW | PD-01, PD-02 | Deferred to S3/S5 gates per clarify | Accepted deferral |
| A1 | Ambiguity | — | spec SC-002 | "≥80% cold-start" operator-measured | quickstart.md covers; OK |
| G1 | Gap | MEDIUM | analyze.md | Missing Speckit analyze artifact | **Fixed** — this file |
| G2 | Gap | MEDIUM | program gate | PROG [P] not formally signed | **Fixed** — program-planning-gate.md |

**Critical issues: 0** · **High: 0** · **Medium: 5 (all remediated)** · **Low: 4**

## Coverage Summary

| Requirement | Has Task? | Task IDs (representative) |
|-------------|-----------|---------------------------|
| FR-001 detect_impact | Yes | C-S1A-001..003 |
| FR-002 zstd artifact | Yes | C-S1A-005..006 |
| FR-003 search rank | Yes | C-S1B-001..002 |
| FR-004 pagination | Yes | C-S1B-003 |
| FR-005 hook augment | Yes | C-S1B-004 |
| FR-006 graph projection | Yes | C-S2-001..002 |
| FR-007 trace_path | Yes | C-S2-003..004 |
| FR-008 query_graph fail-closed | Yes | C-S2-005..006 |
| FR-009 in-process resolver | Yes | C-S3-001..004 |
| FR-010 confidence metadata | Yes | C-S3-004..006 |
| FR-011 semantic no network | Yes | C-S4-001..003 |
| FR-012 semantic frecency-neutral | Yes | V-S4-001 |
| FR-013 route detection | Yes | C-S5-001 |
| FR-014 architecture clusters | Yes | C-S5-002 |
| FR-015 ADR CRUD | Yes | C-S6-001 |
| FR-016 diagnostics privacy | Yes | C-S6-002 |
| FR-017 CLI mirror | Yes | C-S6-003 |
| NFR-001..005 | Yes | V-* gates per sprint |
| SC-001..007 | Yes | acceptance-matrix + V-* |

**Coverage**: 17/17 FR + 5/5 NFR with ≥1 task · **100%**

## Constitution Alignment

| Principle | Status | Notes |
|-----------|--------|-------|
| I Local-first index | Pass | GraphProjection derived; artifact bootstrap only |
| II MCP-native | Pass | Tools + resources planned |
| III Trust envelopes | Pass | detect_impact read-only; diagnostics no source |
| IV Determinism/recovery | Pass | quarantine pattern reused |
| V Frecency | Pass | V-S1B-002 (frecency ext), V-S4-001 explicit |
| VI Embed isolation | Pass | NFR-003 in spec |
| VII Transport parity | Pass | NFR-004; CLI mirror S6 |
| VIII Verification | Pass | quickstart + full gate per sprint |

## Metrics

| Metric | Value |
|--------|-------|
| Total tasks | **159** ([P] 91 · [C] 41 · [V] 27) |
| Requirement coverage | 100% |
| Critical issues | 0 |

## Next Actions

1. ~~**S1a [P]** — P-S1A-010..013 + contract freeze~~ **DONE 2026-06-30** (003/005 frozen, 013 risk+matrix).
2. ~~**Operator** — log benchmarks in benchmark-intake.md~~ **DONE** (operator session 2026-06-30).
3. **S0 [C]** — in progress (spike running); S1a `[C]` unblocks after S0 GO in research.md.
4. ~~Re-analyze before S1a Planning Gate sign-off~~ **DONE** — manual pass above (`/speckit-analyze` not installed here).
5. **S1a Planning Gate** — signed 2026-06-30 (planning complete; coding still gated on S0 GO).
