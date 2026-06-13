# Phase 0 §12A — independent reviewer packet

> **Review readiness: NOT READY** — do **not** distribute for sign-off until **B-A019** is closed or explicitly declared non-blocking. See §0 sequencing.

**Prepared:** 2026-06-13 (pre-review gate refresh)  
**Evidence commit:** `f7207b7` on `v8/stel-architecture`  
**Producer:** Cursor agent (speckit.implement)  
**Purpose:** Template for T048 when pre-review gates clear.

**Do not sign if you authored the artifacts below.**

---

## 0. Pre-review gates (must clear first)

| Gate | Status | Close by |
|------|--------|----------|
| **B-A019** | **OPEN** | Full L0 A/B battery **or** explicit non-blocking declaration in [A-019](./A-019-l0-surface-choice.md) + [stel-assumptions.md](../stel-assumptions.md) |
| **B-HYGIENE** | **CLOSED** | Evidence commit references aligned at `f7207b7` |
| **B-SIGNOFF** | **LATENT** | Request independent review **only after** B-A019 closes |

**Normative sequence:** close A-019 → refresh packet/signoff → request human review → record GO/NO-GO.

---

## 1. Start here (when gates clear — ≤15 min dry-run)

1. Read producer decision stub: [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md) — current decision **NO-GO**
2. Scan checklist: [phase0-12a-evidence-index.md](./phase0-12a-evidence-index.md) §12A table
3. Verify scope: [phase0-12a-scope-boundary.md](./phase0-12a-scope-boundary.md) (`src/stel/**` untouched)
4. Binding §12A: [v8-gap-closure-plan.md](../v8-gap-closure-plan.md) §12A
5. Optional re-run: `powershell -ExecutionPolicy Bypass -File scripts/gather-phase0-evidence.ps1`
6. **Sign or reject** in sign-off § Independent sign-off

---

## 2. Decision contract

From [preflight-evidence-contract.md](../../specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md):

| Outcome | Requires |
|---------|----------|
| **GO** | Empty `blocking_gaps`; A-019 **VALIDATED or accepted non-blocking**; all §12A items accepted; your identity + sign-off reference |
| **NO-GO** | Any failed threshold, A-019 still interim without pivot, or you decline to sign |

**B-SFBENCH is CLOSED** — in-repo evidence is canonical ([phase0-12a-sf-bench-path.md](./phase0-12a-sf-bench-path.md)).

---

## 3. Validated assumptions (verify when review opens)

| ID | Verdict | Key evidence | Recorded |
|----|---------|--------------|----------|
| **A-001** | **VALIDATED** | [A-001](./A-001-measurement-repeatability.md), [run1](./A-001-tool-battery-run1.json), [run2](./A-001-tool-battery-run2.json) | 0% session_net variance (14,389; 20 rows) |
| **A-004** | **VALIDATED** | [A-004](./A-004-equiv-audit.md) | 0% FP+FN (n=20) |
| **A-028** | **VALIDATED** | [corpus](../fixtures/routes.golden.jsonl), [A-028](./A-028-golden-routes.md) | 36 rows |
| **A-005** | **VALIDATED** | [A-005-schema-bytes.json](./A-005-schema-bytes.json) | compact **891 B** (budget 5,000 B) |

---

## 4. Checklist (18 rows)

| # | §12A item | Producer status | Blocks sign-off request? |
|---|-----------|-----------------|--------------------------|
| 1 | A-001 2× battery | **VALIDATED** | No |
| 2 | A-002 manual 6/6 | **PASS** | No |
| 3 | A-003 harness shakedown | **PARTIAL** (MCP PASS) | No |
| 4 | A-004 equiv audit | **VALIDATED** | No |
| 5 | G-005 preflight | **PARTIAL** (in-repo) | No |
| 6 | golden 36 rows | **VALIDATED** | No |
| 7 | RESULTS.md §8.7 | **DEFERRED** | No (not Phase 0 gate) |
| 8 | No 7.21.1 gate | **PASS** | No |
| 9 | A-005 H1 ≤5kB | **VALIDATED** (891 B) | No |
| 10 | A-025 edit ≤1.5kB | **PASS** | No |
| 11 | **A-019 L0 locked** | **INTERIM** | **Yes — primary gate** |
| 12–14 | A-006, A-012, P-FF docs | **DOC PASS** | No |
| 15–16 | A-030, ideation | **PASS** | No |
| 17 | §9 no OPEN blockers | **FAIL** (A-019 interim) | Yes |
| 18 | Independent sign-off | **NOT REQUESTED** | Yes (after A-019) |

**Producer coverage:** **14 / 18** satisfied for strict §12A (A-019 interim + §9 + sign-off not counted).

---

## 5. True blockers today

| ID | Status | Note |
|----|--------|------|
| **B-A019** | **OPEN** | Full L0 A/B not run; interim compact-3 on H1 only |
| **B-HYGIENE** | **CLOSED** | Aligned at `f7207b7` |
| **B-SIGNOFF** | **LATENT** | Do not solicit until B-A019 closes |

**Closed:** B-SFBENCH, B-A001, B-A004, B-A028, B-A005, B-A025  
**Deferred:** B-RESULTS

---

## 6. Closing A-019 (producer paths — pick one)

**Path A — VALIDATED:** Run L0 A/B (compact-3 vs alternatives) on pinned battery; record winner in [A-019](./A-019-l0-surface-choice.md).

**Path B — Non-blocking pivot:** Document explicit acceptance that interim compact-3 is sufficient for Phase 1 pre-flight in A-019 + stel-assumptions §9; gap plan §12A A-019 row updated.

Until Path A or B lands: **do not request independent review.**

---

## 7. When review opens — minimum checks

1. A-001 run1/run2 `session_net_accepted` match
2. Spot-check ≥5 A-004 rows vs [battery run1](./A-001-tool-battery-run1.json)
3. A-005 compact **891 B** in [A-005-schema-bytes.json](./A-005-schema-bytes.json)
4. `node scripts/validate-routes-golden.cjs`
5. Confirm A-019 is **VALIDATED** or **accepted non-blocking** (not interim)

---

## 8. Sign-off template (reviewer — only when §0 gates clear)

```yaml
independent_reviewer: "<name or agent id>"
sign_off_reference: "<PR comment | email | review note>"
review_date: YYYY-MM-DD
decision: GO | NO-GO
checklist_satisfied: <n>
checklist_total_applicable: 18
blocking_gaps: []
notes: "<A-004 spot-checks; A-019 path A or B accepted>"
```

**Producer attestation:** Not independently reviewed. **Sign-off not requested** while B-A019 open.

**First `src/stel/` commit:** **NOT AUTHORIZED**
