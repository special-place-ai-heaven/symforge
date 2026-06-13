# Phase 0 §12A — evidence index

**Feature:** [specs/001-v8-phase0-preflight](../../specs/001-v8-phase0-preflight/spec.md)  
**Plan:** [plan.md](../../specs/001-v8-phase0-preflight/plan.md)  
**Contract:** [preflight-evidence-contract.md](../../specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md)  
**Updated:** 2026-06-13 (A-019 L0 A/B closed)  
**Evidence commit:** `08f7d14` on `v8/stel-architecture` (A-019 bundle `f26f28b`; remediation `e9f4102` / `c3581a5`)

Central index for Section 12A pre-flight readiness. Final decision: [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md).  
Independent reviewer handoff: [phase0-12a-independent-review-packet.md](./phase0-12a-independent-review-packet.md).

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

## sf-bench / in-repo evidence (T002, T010)

**Status:** **In-repo path active** — external sf-bench optional

See [phase0-12a-sf-bench-path.md](./phase0-12a-sf-bench-path.md). Gather: `scripts/gather-phase0-evidence.ps1`

| Capability | Path | Found |
|------------|------|-------|
| Schema measurement | `scripts/measure-schema-bytes.ps1` | **YES** |
| Gate preflight summary | [G-005-inrepo-preflight.json](./G-005-inrepo-preflight.json) | **YES** |
| MCP shakedown | [A-003-mcp-shakedown.jsonl](./A-003-mcp-shakedown.jsonl) | **YES** |
| Legacy compare-results.js | external sf-bench | **Optional** |

---

## Schema-byte helper (T009)

| Field | Value |
|-------|-------|
| Script | `scripts/measure-schema-bytes.ps1` |
| Command | `powershell -ExecutionPolicy Bypass -File scripts/measure-schema-bytes.ps1` |
| Output | `docs/research/A-005-schema-bytes.json` |
| Assumptions | A-005, A-025 |
| Status | **PASS** (891 B compact; see [A-005-schema-bytes-summary.md](./A-005-schema-bytes-summary.md)) |

---

## §12A checklist traceability (T006, T007)

Binding source: [docs/v8-gap-closure-plan.md](../v8-gap-closure-plan.md) §12A.

| §12A item | Assumption / gap | Evidence artifact | Contract shape | Status |
|-----------|------------------|-------------------|----------------|--------|
| A-001 VALIDATED (2× battery) | A-001 | [A-001-measurement-repeatability.md](./A-001-measurement-repeatability.md) | Assumption Evidence Record | **PASS** (0% session_net variance) |
| A-002 VALIDATED (manual spot-check) | A-002 | [A-002-manual-spotcheck.md](./A-002-manual-spotcheck.md) | Assumption Evidence Record | **PASS** |
| A-003 VALIDATED (harness shakedown) | A-003 | [A-003-harness-shakedown.md](./A-003-harness-shakedown.md) | Measurement Row Classification | **PARTIAL** (MCP PASS) |
| A-004 VALIDATED (equiv audit) | A-004 | [A-004-equiv-audit.md](./A-004-equiv-audit.md) | Assumption Evidence Record | **PASS** (0% FP+FN) |
| compare-results.js `--preflight` | G-005 | [G-005-compare-results-preflight.md](./G-005-compare-results-preflight.md) | Gate Comparator Summary | **PARTIAL** (H1/H7) |
| routes.golden.jsonl 36 rows | A-028 | [A-028-golden-routes.md](./A-028-golden-routes.md) | Golden Route Row | **PASS** |
| RESULTS.md §8.7 + columns | G-005 | [G-005-inrepo-preflight.json](./G-005-inrepo-preflight.json) | Gate Comparator Summary | **PARTIAL** |
| No beat/pin 7.21.1 baseline | — | [phase0-12a-scope-boundary.md](./phase0-12a-scope-boundary.md) | Scope evidence | **PASS** |
| A-005 VALIDATED (H1) | A-005 | [A-005-schema-bytes-summary.md](./A-005-schema-bytes-summary.md) | Schema Measurement Record | **PASS** (891 B) |
| A-025 VALIDATED (edit budget) | A-025 | [A-005-schema-bytes-summary.md](./A-005-schema-bytes-summary.md) | Schema Measurement Record | **PASS** |
| A-019 VALIDATED (L0 locked) | A-019 | [A-019-l0-surface-choice.md](./A-019-l0-surface-choice.md) | Schema Measurement Record | **PASS** — compact-3 wins |
| A-006/A-027 documented | A-006, A-027 | [A-006-host-schema.md](./A-006-host-schema.md) | Bypass/host policy | **DOC PASS** |
| A-012 two-hop OR H3 serve-only | A-012 | [A-012-bypass-policy.md](./A-012-bypass-policy.md) | Bypass Policy Record | **DOC PASS** |
| P-FF + eligible H6 documented | A-032 | [A-012-bypass-policy.md](./A-012-bypass-policy.md) | Golden README rules | **DOC PASS** |
| Phase crosswalk (A-030) | A-030 | [A-030-phase-crosswalk.md](./A-030-phase-crosswalk.md) | Process evidence | **PASS** |
| Decision log updated | — | [ideation.md](../ideation.md) | Decision log | **PASS** |
| No OPEN assumption blocks Phase 1 | §9 | [stel-assumptions.md](../stel-assumptions.md) | Assumption register | **PARTIAL** (B-SIGNOFF only) |

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

**Independent review:** **READY TO SOLICIT** — A-019 closed; **not yet requested** (see sequencing below).

| ID | Status | Reason |
|----|--------|--------|
| B-A019 | **CLOSED** | L0 A/B complete — compact-3 wins ([A-019](./A-019-l0-surface-choice.md)) |
| B-SIGNOFF | **LATENT** | Independent reviewer required for GO; **ready to solicit** |
| B-SFBENCH | **CLOSED** | Superseded by in-repo evidence path |
| B-RESULTS | **DEFERRED** | RESULTS.md §8.7 — post-8.0 baseline; not Phase 0 gate |
| B-HYGIENE | **CLOSED** | Evidence commit references aligned at `08f7d14` |

### Sequencing (normative)

1. ~~**Close A-019**~~ — **done** ([A-019](./A-019-l0-surface-choice.md), [battery](./A-019-l0-ab-results.json)).
2. ~~**Refresh** review packet + signoff~~ — **done** (`08f7d14`).
3. **Request** independent review ([packet](./phase0-12a-independent-review-packet.md)).
4. **Record** GO or NO-GO in [signoff](./phase0-12a-review-signoff.md).

**Phase 0 decision:** **NO-GO** — no independent sign-off yet.

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

## Remediation status (2026-06-13)

| Assumption / gap | Verdict | Artifact |
|------------------|---------|----------|
| A-001 | **VALIDATED** | [run1](./A-001-tool-battery-run1.json), [run2](./A-001-tool-battery-run2.json) — 0% session_net variance |
| A-004 | **VALIDATED** | [A-004-equiv-audit.md](./A-004-equiv-audit.md) — 0% FP+FN (n=20) |
| A-028 | **VALIDATED** | [routes.golden.jsonl](../fixtures/routes.golden.jsonl) — 36 rows |
| A-005 H1 | **VALIDATED** | compact `tools/list` **891 B** ([A-005-schema-bytes.json](./A-005-schema-bytes.json)) |
| A-019 | **VALIDATED** | compact-3 wins L0 A/B — [battery](./A-019-l0-ab-results.json) |
| B-SFBENCH | **CLOSED** | In-repo path supersedes external sf-bench |
| B-SIGNOFF | **LATENT** | Ready to solicit; not requested |

---

## Readiness decision link (T015)

→ [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md)
