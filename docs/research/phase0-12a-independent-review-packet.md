# Phase 0 §12A — independent reviewer packet

**Prepared:** 2026-06-13 (evidence refresh)  
**Evidence commit:** `c3581a5` on `v8/stel-architecture`  
**Producer:** Cursor agent (speckit.implement)  
**Purpose:** Hand to a reviewer who **did not** produce this evidence bundle (T048).

**Do not sign if you authored the artifacts below.**

---

## 1. Start here (≤15 min dry-run path)

1. Read producer decision stub: [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md) — current decision **NO-GO**
2. Scan checklist traceability: [phase0-12a-evidence-index.md](./phase0-12a-evidence-index.md) §12A table
3. Verify scope boundary: [phase0-12a-scope-boundary.md](./phase0-12a-scope-boundary.md) (`src/stel/**` untouched)
4. Confirm binding §12A source: [v8-gap-closure-plan.md](../v8-gap-closure-plan.md) §12A (lines 429–460)
5. Optional re-run: `powershell -ExecutionPolicy Bypass -File scripts/gather-phase0-evidence.ps1`
6. **Sign or reject** — fill § Independent sign-off in [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md)

---

## 2. Decision contract

From [preflight-evidence-contract.md](../../specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md):

| Outcome | Requires |
|---------|----------|
| **GO** | Empty `blocking_gaps`; all §12A items accepted (including A-019 if you accept interim lock); your identity + sign-off reference |
| **NO-GO** | Any failed threshold, rejected interim A-019, Phase 1-blocking OPEN assumption, or you decline to sign |

**7.x bench is informational only** — do not use `results-7.21.1-baseline.json` as a v8 gate.

**B-SFBENCH is CLOSED** — external sf-bench is optional. In-repo evidence is canonical per [phase0-12a-sf-bench-path.md](./phase0-12a-sf-bench-path.md). **Do not NO-GO on missing sf-bench.**

---

## 3. Validated assumptions (verify artifacts)

| ID | Verdict | Key evidence | Recorded result |
|----|---------|--------------|-----------------|
| **A-001** | **VALIDATED** | [A-001](./A-001-measurement-repeatability.md), [run1](./A-001-tool-battery-run1.json), [run2](./A-001-tool-battery-run2.json) | 0% session_net variance (14,389 both runs; 20 rows) |
| **A-004** | **VALIDATED** | [A-004](./A-004-equiv-audit.md) | 0% FP+FN over 20 stratified samples |
| **A-028** | **VALIDATED** | [A-028](./A-028-golden-routes.md), [corpus](../fixtures/routes.golden.jsonl) | 36 rows; `node scripts/validate-routes-golden.cjs` PASS |
| **A-005** | **VALIDATED** | [A-005-schema-bytes.json](./A-005-schema-bytes.json) | compact `tools/list` **891 B** (budget 5,000 B) |

---

## 4. Checklist quick reference (18 rows)

| # | §12A item | Producer status | Reviewer action |
|---|-----------|-----------------|-----------------|
| 1 | A-001 2× battery | **VALIDATED** | Confirm run1/run2 `session_net_accepted` match |
| 2 | A-002 manual 6/6 | **PASS** | Spot-check formula vs `format.rs` |
| 3 | A-003 harness shakedown | **PARTIAL** (MCP PASS) | Confirm compact `tools/list` in [jsonl](./A-003-mcp-shakedown.jsonl) |
| 4 | A-004 equiv audit ≤10% | **VALIDATED** | **Spot-check ≥5 rows** vs battery JSON |
| 5 | compare-results `--preflight` | **PARTIAL** (in-repo H1/H4/H7) | Accept [G-005-inrepo-preflight.json](./G-005-inrepo-preflight.json) or re-run |
| 6 | golden 36 rows | **VALIDATED** | Run `node scripts/validate-routes-golden.cjs` |
| 7 | RESULTS.md §8.7 | **DEFERRED** | Not a Phase 0 GO gate |
| 8 | No 7.21.1 gate | **PASS** | — |
| 9 | A-005 H1 ≤5kB | **VALIDATED** (**891 B**) | Optional: `measure-schema-bytes.ps1` |
| 10 | A-025 edit ≤1.5kB | **PASS** | `cargo test -p symforge --lib surface_probe` |
| 11 | A-019 L0 locked | **INTERIM** (compact-3 on H1) | Accept interim **or** require full L0 A/B |
| 12 | A-006/A-027 doc | **DOC PASS** | — |
| 13 | A-012 bypass/H3 | **DOC PASS** (serve-only interim) | — |
| 14 | P-FF / H6 rules | **DOC PASS** (4 P-FF rows seeded) | — |
| 15 | A-030 crosswalk | **PASS** | — |
| 16 | ideation decision log | **PASS** | — |
| 17 | §9 no OPEN blockers | **PARTIAL** (A-019 interim) | — |
| 18 | Independent sign-off | **PENDING YOU** | Required for GO |

### Coverage counting (producer)

| Bucket | Count | Notes |
|--------|-------|-------|
| Satisfied | **15** | PASS, VALIDATED, DOC PASS, and accepted PARTIAL rows (A-003, G-005) |
| Interim | **1** | A-019 — compact-3 on H1 only |
| Deferred | **1** | RESULTS.md §8.7 (excluded from Phase 0 GO) |
| Not satisfied | **2** | §9 (A-019 interim blocks strict register), sign-off pending |
| **Total applicable** | **18** | Review packet rows above |

---

## 5. Active blockers (verify independently)

| ID | Status | Verify by |
|----|--------|-----------|
| B-A019 | **OPEN (interim)** | [A-019](./A-019-l0-surface-choice.md) — full L0 A/B not run |
| B-SIGNOFF | **OPEN** | Your signature in sign-off doc |

**Deferred (not Phase 0 GO gates):**

| ID | Status | Note |
|----|--------|------|
| B-RESULTS | **DEFERRED** | RESULTS.md §8.7 — post-8.0 baseline |

**Closed / superseded:**

| ID | Status | Note |
|----|--------|------|
| B-SFBENCH | **CLOSED** | In-repo evidence path; external sf-bench optional |
| B-A001 | **CLOSED** | VALIDATED — 2× battery |
| B-A004 | **CLOSED** | VALIDATED — equiv audit |
| B-A028 | **CLOSED** | VALIDATED — golden corpus |
| B-A005 / B-A025 | **CLOSED** | VALIDATED — 891 B compact; edit ≤1.5 kB |

---

## 6. How to sign GO vs reject NO-GO

**Sign GO** only if you independently confirm:

- A-001, A-004, A-028, A-005 artifacts match recorded numbers
- ≥5 A-004 spot-checks pass against [A-001-tool-battery-run1.json](./A-001-tool-battery-run1.json)
- You **accept** A-019 interim compact-3 lock (or full L0 A/B is complete)
- `src/stel/**` remains untouched per scope boundary

**Record NO-GO** if:

- Any threshold fails on re-check (variance >2%, equiv FP+FN >10%, H1 >5kB without pivot)
- You reject A-019 interim without full L0 A/B
- You cannot verify artifacts or you produced this bundle

**Producer recommendation (not binding):** **NO-GO** until B-SIGNOFF clears. If you accept A-019 interim after spot-checks, **GO** is defensible at **15/18** with RESULTS.md deferred.

---

## 7. Sign-off template (reviewer fills in)

Copy into [phase0-12a-review-signoff.md](./phase0-12a-review-signoff.md):

```yaml
independent_reviewer: "<your name or agent id>"
sign_off_reference: "<PR comment | dated email | review note path>"
review_date: YYYY-MM-DD
decision: GO | NO-GO
checklist_satisfied: <n>
checklist_total_applicable: 18
blocking_gaps:
  - id: "<only if NO-GO>"
    reason: "<why>"
notes: "<cite A-004 spot-checks; A-019 interim accept/reject>"
```

**Producer attestation:** Evidence at commit `c3581a5`. **Not** independently reviewed as of this packet refresh.

**First `src/stel/` commit:** **NOT AUTHORIZED** without independent GO on refreshed sign-off.
