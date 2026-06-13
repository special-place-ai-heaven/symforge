# Phase 0 §12A — evidence index

**Feature:** [specs/001-v8-phase0-preflight](../../specs/001-v8-phase0-preflight/spec.md)  
**Plan:** [plan.md](../../specs/001-v8-phase0-preflight/plan.md)  
**Contract:** [preflight-evidence-contract.md](../../specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md)  
**Updated:** 2026-06-13

Central index for Section 12A pre-flight readiness. Final decision: [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md).

---

## Spec Kit inputs (T003)

| Document | Path |
|----------|------|
| Feature spec | `specs/001-v8-phase0-preflight/spec.md` |
| Implementation plan | `specs/001-v8-phase0-preflight/plan.md` |
| Evidence contract | `specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md` |
| Tasks | `specs/001-v8-phase0-preflight/tasks.md` |
| Quickstart | `specs/001-v8-phase0-preflight/quickstart.md` |

---

## Scope guard (T008)

**Forbidden for this feature:**

| Area | Rule |
|------|------|
| `src/stel/**` | No implementation until §12A 100% green + independent GO |
| Phase 4 deploy/admin | Out of scope (document dependencies only) |
| AAP convenience work | Out of scope |
| Phase 1 STEL runtime | Evidence-only; no controller/router code |

Audit: [phase0-12a-scope-boundary.md](./phase0-12a-scope-boundary.md)

---

## sf-bench workspace (T002, T010)

**Status:** **NO-GO — B-SFBENCH**

See [phase0-12a-sf-bench-path.md](./phase0-12a-sf-bench-path.md).

| Required artifact | Path | Found |
|-------------------|------|-------|
| compare-results.js | `<sf-bench>/compare-results.js` | **NO** |
| routes.golden.jsonl | `<sf-bench>/routes.golden.jsonl` | **NO** |
| RESULTS.md | `<sf-bench>/RESULTS.md` | **NO** |

---

## Schema-byte helper (T009)

| Field | Value |
|-------|-------|
| Script | `scripts/measure-schema-bytes.ps1` |
| Command | `powershell -ExecutionPolicy Bypass -File scripts/measure-schema-bytes.ps1` |
| Output | `docs/research/A-005-schema-bytes.json` |
| Assumptions | A-005, A-025 |
| Status | **PARTIAL** (see [A-005-schema-bytes-summary.md](./A-005-schema-bytes-summary.md)) |

---

## §12A checklist traceability (T006, T007)

Binding source: [docs/v8-gap-closure-plan.md](../v8-gap-closure-plan.md) §12A.

| §12A item | Assumption / gap | Evidence artifact | Contract shape | Status |
|-----------|------------------|-------------------|----------------|--------|
| A-001 VALIDATED (2× battery) | A-001 | [A-001-measurement-repeatability.md](./A-001-measurement-repeatability.md) | Assumption Evidence Record | **BLOCKED** |
| A-002 VALIDATED (manual spot-check) | A-002 | [A-002-manual-spotcheck.md](./A-002-manual-spotcheck.md) | Assumption Evidence Record | **BLOCKED** |
| A-003 VALIDATED (harness shakedown) | A-003 | [A-003-harness-shakedown.md](./A-003-harness-shakedown.md) | Measurement Row Classification | **BLOCKED** |
| A-004 VALIDATED (equiv audit) | A-004 | [A-004-equiv-audit.md](./A-004-equiv-audit.md) | Assumption Evidence Record | **BLOCKED** |
| compare-results.js `--preflight` | G-005 | [G-005-compare-results-preflight.md](./G-005-compare-results-preflight.md) | Gate Comparator Summary | **BLOCKED** |
| routes.golden.jsonl 36 rows | A-028 | [A-028-golden-routes.md](./A-028-golden-routes.md) | Golden Route Row | **BLOCKED** |
| RESULTS.md §8.7 + columns | G-005 | [G-005-compare-results-preflight.md](./G-005-compare-results-preflight.md) | Gate Comparator Summary | **BLOCKED** |
| No beat/pin 7.21.1 baseline | — | [phase0-12a-scope-boundary.md](./phase0-12a-scope-boundary.md) | Scope evidence | **PASS** |
| A-005 VALIDATED (H1) | A-005 | [A-005-schema-bytes-summary.md](./A-005-schema-bytes-summary.md) | Schema Measurement Record | **OPEN** |
| A-025 VALIDATED (edit budget) | A-025 | [A-005-schema-bytes-summary.md](./A-005-schema-bytes-summary.md) | Schema Measurement Record | **OPEN** (pivot doc) |
| A-019 VALIDATED (L0 locked) | A-019 | [A-019-l0-surface-choice.md](./A-019-l0-surface-choice.md) | Schema Measurement Record | **BLOCKED** |
| A-006/A-027 documented | A-006, A-027 | [A-006-host-schema.md](./A-006-host-schema.md) | Bypass/host policy | **DOC PASS** |
| A-012 two-hop OR H3 serve-only | A-012 | [A-012-bypass-policy.md](./A-012-bypass-policy.md) | Bypass Policy Record | **DOC PASS** |
| P-FF + eligible H6 documented | A-032 | [A-012-bypass-policy.md](./A-012-bypass-policy.md) | Golden README rules | **DOC PASS** |
| Phase crosswalk (A-030) | A-030 | [A-030-phase-crosswalk.md](./A-030-phase-crosswalk.md) | Process evidence | **PASS** |
| Decision log updated | — | [ideation.md](../ideation.md) | Decision log | **PASS** |
| No OPEN assumption blocks Phase 1 | §9 | [stel-assumptions.md](../stel-assumptions.md) | Assumption register | **FAIL** |

---

## Measurement (User Story 2)

| Artifact | Link |
|----------|------|
| Repeatability | [A-001-measurement-repeatability.md](./A-001-measurement-repeatability.md) |
| Manual spot-check | [A-002-manual-spotcheck.md](./A-002-manual-spotcheck.md) |
| Harness shakedown | [A-003-harness-shakedown.md](./A-003-harness-shakedown.md) |
| Equivalence audit | [A-004-equiv-audit.md](./A-004-equiv-audit.md) |
| compare-results preflight | [G-005-compare-results-preflight.md](./G-005-compare-results-preflight.md) |

---

## Surface choice (User Story 3)

| Artifact | Link |
|----------|------|
| Schema bytes (raw) | [A-005-schema-bytes.json](./A-005-schema-bytes.json) |
| Schema summary | [A-005-schema-bytes-summary.md](./A-005-schema-bytes-summary.md) |
| L0 surface A/B | [A-019-l0-surface-choice.md](./A-019-l0-surface-choice.md) |
| Host amortization | [A-006-host-schema.md](./A-006-host-schema.md) |
| Golden routes | [A-028-golden-routes.md](./A-028-golden-routes.md) |

---

## Bypass harness (User Story 3)

| Artifact | Link |
|----------|------|
| Bypass policy | [A-012-bypass-policy.md](./A-012-bypass-policy.md) |

---

## Process / boundary (User Story 4)

| Artifact | Link |
|----------|------|
| Scope boundary | [phase0-12a-scope-boundary.md](./phase0-12a-scope-boundary.md) |
| Phase crosswalk | [A-030-phase-crosswalk.md](./A-030-phase-crosswalk.md) |
| Assumption placeholders | [phase0-12a-assumption-evidence.md](./phase0-12a-assumption-evidence.md) |
| Assumption register | [stel-assumptions.md](../stel-assumptions.md) |

---

## Blockers (summary)

| ID | Reason |
|----|--------|
| B-SFBENCH | sf-bench workspace missing — blocks A-001..A-004, G-005, A-028, A-019 battery |
| B-A005 | Compact surface stub not measured ≤5kB |
| B-A019 | L0 surface not locked |
| B-SIGNOFF | Independent reviewer sign-off not obtained |
| B-ASSUMPTIONS | Phase 1-blocking assumptions remain OPEN |

---

## Verification runs (T044, T045)

### T044 — check-prerequisites.ps1 -PathsOnly

```json
{"REPO_ROOT":"C:\\AI_STUFF\\PROGRAMMING\\symforge","BRANCH":"","FEATURE_DIR":"C:\\AI_STUFF\\PROGRAMMING\\symforge\\specs\\001-v8-phase0-preflight","FEATURE_SPEC":"C:\\AI_STUFF\\PROGRAMMING\\symforge\\specs\\001-v8-phase0-preflight\\spec.md","IMPL_PLAN":"C:\\AI_STUFF\\PROGRAMMING\\symforge\\specs\\001-v8-phase0-preflight\\plan.md","TASKS":"C:\\AI_STUFF\\PROGRAMMING\\symforge\\specs\\001-v8-phase0-preflight\\tasks.md"}
```

**Result:** PASS — feature paths resolve correctly.

### T045 — unresolved placeholder scan

```powershell
rg -n "NEEDS CLARIFICATION|\[FEATURE\]|\[###|ACTION REQUIRED|REMOVE IF UNUSED" specs/001-v8-phase0-preflight -g '!quickstart.md' -g '!**/checklists/**'
```

**Result:** PASS — no matches (exit 1 / empty output).

---

## Readiness decision link (T015)

→ [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md)
