# Phase 0 §12A — independent reviewer packet

> **Review readiness: READY** — A-019 closed (compact-3 wins L0 A/B). Independent review **may be solicited**; producer has **not** self-signed. See §0 sequencing.

**Prepared:** 2026-06-13 (A-019 L0 A/B closed)  
**Evidence commit:** `08f7d14` on `v8/stel-architecture` (A-019 bundle `f26f28b`; remediation `e9f4102` / `c3581a5`)  
**Producer:** Cursor agent (speckit.implement)  
**Purpose:** Template for T048 when pre-review gates clear.

**Do not sign if you authored the artifacts below.**

---

## 0. Pre-review gates (must clear first)

| Gate | Status | Close by |
|------|--------|----------|
| **B-A019** | **CLOSED** | L0 A/B complete — [A-019](./A-019-l0-surface-choice.md), [battery](./A-019-l0-ab-results.json) |
| **B-HYGIENE** | **CLOSED** | Evidence commit references aligned at `08f7d14` |
| **B-SIGNOFF** | **LATENT** | Request independent review; record GO/NO-GO |

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
| **A-019** | **VALIDATED** | [A-019](./A-019-l0-surface-choice.md), [L0 A/B](./A-019-l0-ab-results.json) | compact-3 wins (session_net 14,389; tie-break vs meta-1) |

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
| 11 | **A-019 L0 locked** | **VALIDATED** (compact-3) | No |
| 12–14 | A-006, A-012, P-FF docs | **DOC PASS** | No |
| 15–16 | A-030, ideation | **PASS** | No |
| 17 | §9 no OPEN blockers | **PARTIAL** (B-SIGNOFF only) | No |
| 18 | Independent sign-off | **NOT REQUESTED** | Yes |

**Producer coverage:** **16 / 18** satisfied for strict §12A (independent sign-off not counted).

---

## 5. True blockers today

| ID | Status | Note |
|----|--------|------|
| **B-SIGNOFF** | **LATENT** | Independent review ready to solicit; not requested |
| **B-HYGIENE** | **CLOSED** | Aligned at `08f7d14` |

**Closed:** B-A019, B-SFBENCH, B-A001, B-A004, B-A028, B-A005, B-A025  
**Deferred:** B-RESULTS

---

## 6. A-019 closure (Path A — completed)

L0 A/B run via `scripts/phase0-l0-ab-battery.cjs` on pinned 20-row corpus. Winner: **compact-3**. See [A-019](./A-019-l0-surface-choice.md).

Re-run: `node scripts/phase0-l0-ab-battery.cjs target/debug/symforge.exe docs/research/A-019-l0-ab-results.json`

---

## 7. Minimum checks

1. A-001 run1/run2 `session_net_accepted` match
2. Spot-check ≥5 A-004 rows vs [battery run1](./A-001-tool-battery-run1.json)
3. A-005 compact **891 B** in [A-005-schema-bytes.json](./A-005-schema-bytes.json)
4. `node scripts/validate-routes-golden.cjs`
5. Confirm A-019 **VALIDATED** in [A-019-l0-ab-results.json](./A-019-l0-ab-results.json) (compact-3 winner)

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

**Producer attestation:** Not independently reviewed. **Sign-off not requested** (ready to solicit).

**First `src/stel/` commit:** **NOT AUTHORIZED**
