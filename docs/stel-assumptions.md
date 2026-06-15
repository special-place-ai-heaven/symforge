# STEL assumption register

**RULE (hard gate):** Every assumption MUST be tested and validated before it drives the next phase.  
If validation **fails** → stop forward work on that path, **research** (more data, revised design), re-register assumption, validate again.  
**Unvalidated assumptions do not unlock implementation or ship as satisfied gates** (OPEN assumptions may appear in design docs as planned work).

**Performance superiority overrides preservation:** any assumption of the form “we must keep X” is invalid until X beats the alternative on the pinned battery. If X loses → X is research fodder, not a constraint.

Companion: [`ideation.md`](ideation.md), [`v8-master-plan.md`](v8-master-plan.md), [`stel-architecture.md`](stel-architecture.md), [`stel-schema.md`](stel-schema.md).

---

## Workflow

```text
1. REGISTER  — state assumption, risk if wrong, validation method
2. VALIDATE  — run performance test, path test, or cited research (pinned artifact)
3. VERDICT   — VALIDATED | INVALIDATED | OPEN
4. FORWARD   — only VALIDATED assumptions unlock the next phase item
5. INVALIDATED → research spike → new assumption(s) → back to step 2
```

No skipping step 2 because “it’s obvious” or “Anthropic said so.” External claims are **hypotheses** until reproduced on **our** pinned corpus/binary.

### Assumption record (required fields)

```yaml
id: A-001
statement: "…"
phase_blocked: [0, 1, 2]   # phases that depend on this
risk_if_wrong: "…"
validation:
  kind: performance | path | trajectory | research | host_measurement
  method: "exact command or experiment"
  artifact: "path/to/results or doc link"
verdict: OPEN | VALIDATED | INVALIDATED
validated_at: null | ISO date
notes: ""
```

Store records in this file (human) and optionally `docs/stel-assumptions.json` (CI).

---

## Phase gates (assumption dependencies)

| Phase | May start only when |
|-------|---------------------|
| **0 baseline** | Phase 0 **exits** when A-001..A-004 validated (measurement harness trustworthy) |
| **0 L0 choice** | A-019 validated (compact-3 vs meta-tool — **before** locking Phase 1 tools) |
| **1 types + L0** | A-005, A-025 validated (compact schema ≤5kB including edit) |
| **2 L1 + L2** | A-008..A-014 evidence recorded (PARTIAL/VALIDATED/OPEN); A-029 spike complete (VALIDATED 2/4; P-T2 partial) |
| **3 executor + 8.0** | A-015..A-016 validated |
| **4 quality + deploy + 8.1** | 8.0.0 shipped; A-020..A-022 validated |

*Phase numbers match [`v8-master-plan.md`](v8-master-plan.md) and [`README.md`](README.md) crosswalk.*

---

## Register (initial — v8 kickoff)

Status as of branch `v8/stel-architecture`. **Most are OPEN.**

### Measurement & baseline

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-001** | sf-bench S/M/N token method (`ceil(bytes/4)`) is stable across re-runs on same binary | Re-run battery 2×; compare session_net variance ≤ ±2% | **VALIDATED** |
| **A-002** | Competent-manual baseline (grep + ~50-line window) matches sf-bench `M` and is the right product comparator | Spot-check 6 rows: manual harness output vs judge expectations | **VALIDATED** |
| **A-003** | v8 branch release binary runs full harness without error | `results-v8-harness-shakedown.json` on `target/release` | **PARTIAL** |
| **A-004** | Equivalence judge correlates with human “good enough” on 10 sampled rows | Manual review sample; document false pos/neg | **VALIDATED** |

### Schema & surface

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-005** | Compact 3-tool MCP surface ≤ **5,000 B** JSON schema | Implement stub `list_tools` filter; measure bytes (H1) | **OPEN** |
| **A-006** | Hosts (Cursor) amortize schema across calls so per-call tax \< sf-bench ÷50 on long sessions | Host measurement or documented Cursor behavior; if false, bypass must account full schema | **OPEN** |
| **A-007** | Models use ≤4 SymForge tools per session in practice | Analytics or client telemetry; else treat as hypothesis only | **OPEN** |

### Router & paths

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-008** | `smart_query` + NL achieves ≥ **95%** trajectory pass on `routes.golden.jsonl` | Build golden file; replay via `symforge` | **PARTIAL** |
| **A-009** | Multi-step internal chain (search→symbol) improves equivalence **without** increasing tokens vs single-hop | A/B on failing T1/T4 rows | **VALIDATED** |
| **A-010** | Structured `intent` bucket reduces fallback rate vs NL-only | A/B NL-only vs intent-hint on golden corpus | **OPEN** |

### Controller & economics

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-011** | Index `raw_chars` + line count predict response tokens within **±20%** | Compare `est_response_tokens` vs actual on full battery | **OPEN** |
| **A-012** | **Bypass** on small files eliminates `sGteM` while preserving task completion via host Read | Battery `*_small` rows with controller; **two-hop harness** (BYPASS → simulated Read → completion check) or H3 scoped to **serve-only** small rows | **PARTIAL** |
| **A-013** | **cache_hit** via `SessionContext` saves tokens without equivalence loss | Path tests with duplicate fetch scenarios | **VALIDATED** |
| **A-014** | Degrade (outline-only, cap 1000 tok) beats 7.x on T3 large **and** raises equivalence | Battery diff fmt/tokio T3 large rows | **OPEN** |

### Trust & calibration

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-015** | Trust envelope `session_net_vs_manual` matches L4 ledger within ±1% | Linked battery + ledger export | **OPEN** |
| **A-016** | EMA calibration reduces predictor error over successive battery runs | 3 consecutive runs; error trend down | **OPEN** |

### External (hypothesis until reproduced)

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-017** | Tool selection accuracy degrades past ~30–50 exposed tools (Anthropic) | A/B compact vs full surface on **same tasks** with LLM in loop OR proxy via path confusion rate | **OPEN** (cited, not reproduced) |
| **A-019** | Replacing entire 32-tool surface with 1–2 meta-tools beats compact-3 on session_net **and** equivalence | Full battery A/B: meta-tool vs STEL compact (same corpus) | **VALIDATED** (compact-3 wins) |

### Server & deploy (Phase 4 — after 8.0.0)

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-020** | MCP Streamable HTTP on `/mcp` matches stdio path on sf-bench (S, M, equiv unchanged) | Full battery both transports; same binary + SHAs | **OPEN** |
| **A-021** | Unified server (daemon + sidecar merged, no local duplicate stack) does not regress tokens or latency | Battery + p99 on governor path | **OPEN** |
| **A-022** | In-process tool dispatch (no HTTP proxy hop) beats proxy for local attach without losing multi-session sharing | Latency A/B; optional if single-server in-process | **OPEN** |

### Post–adversarial review (2026-06-12)

| ID | Assumption | Validation | Status |
|----|------------|------------|--------|
| **A-023** | BYPASS rows excluded from **H6** numerator and denominator; scored separately in bypass ledger | compare-results.js + RESULTS.md columns | **OPEN** |
| **A-024** | **`results-v8-8.0-baseline.json`** pinned at **8.0.0 tag**; all later diffs vs v8 baselines only | artifact path + git tag SHA | **OPEN** |
| **A-025** | `symforge_edit` JSON Schema ≤ **1,500 B**; else merge into `symforge` with `intent=edit` | Measured `list_tools` bytes | **OPEN** |
| **A-026** | **H4** uses **`session_net_accepted`** (accepted serve rows only); `session_net_all36` reported separately | RESULTS.md §8.2 + compare-results.js | **OPEN** |
| **A-027** | Battery schema divisor (**÷50**) is harness-only until **A-006** host-validated | Document in sf-bench spec; controller uses conservative max | **OPEN** |
| **A-028** | Golden rows include **`expected_equiv`** and **`expected_decision`**, not route shape alone | routes.golden.jsonl schema | **VALIDATED** |
| **A-029** | T2 spike: ≥**2/4** equiv on tokio+django **or** bypass-only policy registered for reference tasks | [`research/A-029-t24-restoration-signoff.md`](research/A-029-t24-restoration-signoff.md) | **VALIDATED** (2/4 equiv post-TX-04; P-T2 **partial** — 2 serve-eligible, 2 bypass-only; 2026-06-15) |
| **A-031** | Phase 0.12 rmcp Streamable HTTP **compile spike** passes before Phase 4 code | `docs/research/A-031-rmcp-spike.md` | **OPEN** |
| **A-032** | Full-file review tasks use policy **P-FF** (bypass, `eligible_h6=false`) | 4 rows in `routes.golden.jsonl` | **OPEN** |

*(Register new “must keep X” beliefs here — they default OPEN and block nothing until validated.)*

## Phase 0 §12A evidence links (2026-06-13)

Updated by [speckit.implement](../specs/001-v8-phase0-preflight/tasks.md). Index: [`research/phase0-12a-evidence-index.md`](research/phase0-12a-evidence-index.md). Decision: **GO** — [`research/phase0-12a-review-signoff.md`](research/phase0-12a-review-signoff.md).

| ID | Artifact | Verdict | Notes |
|----|----------|---------|-------|
| **A-001** | [`research/A-001-measurement-repeatability.md`](research/A-001-measurement-repeatability.md) | **VALIDATED** | 2× battery 0% session_net variance |
| **A-002** | [`research/A-002-manual-spotcheck.md`](research/A-002-manual-spotcheck.md) | **VALIDATED** | 6/6 spot checks in-repo |
| **A-003** | [`research/A-003-harness-shakedown.md`](research/A-003-harness-shakedown.md) | **PARTIAL** | MCP shakedown PASS; battery row fields OPEN |
| **A-004** | [`research/A-004-equiv-audit.md`](research/A-004-equiv-audit.md) | **VALIDATED** | 20-sample audit 0% FP+FN |
| **A-005** | [`research/A-005-schema-bytes-summary.md`](research/A-005-schema-bytes-summary.md) | **VALIDATED** | Compact 891 B |
| **A-006** | [`research/A-006-host-schema.md`](research/A-006-host-schema.md) | **OPEN** | Conservative worst-case policy documented |
| **A-012** | [`research/A-012-bypass-policy.md`](research/A-012-bypass-policy.md) | **OPEN** | Serve-only H3 interim scope selected |
| **A-019** | [`research/A-019-l0-surface-choice.md`](research/A-019-l0-surface-choice.md) | **VALIDATED** | compact-3 wins L0 A/B |
| **A-025** | [`research/A-005-schema-bytes-summary.md`](research/A-005-schema-bytes-summary.md) | **VALIDATED** | Edit schema ≤1,500 B |
| **A-026** | [`research/G-005-compare-results-preflight.md`](research/G-005-compare-results-preflight.md) | **PARTIAL** | H1/H7 in-repo preflight |
| **A-027** | [`research/A-006-host-schema.md`](research/A-006-host-schema.md) | **OPEN** | Harness ÷50 documented as non-product |
| **A-028** | [`research/A-028-golden-routes.md`](research/A-028-golden-routes.md) | **VALIDATED** | 36 rows [`fixtures/routes.golden.jsonl`](fixtures/routes.golden.jsonl) |
| **A-032** | [`research/A-012-bypass-policy.md`](research/A-012-bypass-policy.md) | **PARTIAL** | 4 P-FF rows seeded; battery enforcement §12B |

## Phase 2 evidence links (2026-06-14)

Updated by [speckit.implement](../specs/002-v8-phase2-stel-controller/tasks.md) P2-S6. Index: [`research/phase2-evidence-index.md`](research/phase2-evidence-index.md). Exit: [`phase2-stel-checkpoint.md`](phase2-stel-checkpoint.md).

| ID | Artifact | Verdict | Notes |
|----|----------|---------|-------|
| **A-008** | [`tests/stel_golden_replay.rs`](../tests/stel_golden_replay.rs) | **PARTIAL** | 32 serve + 4 P-FF replay; 95% trajectory metric not numerically measured |
| **A-009** | [`tests/stel_multi_hop_chain.rs`](../tests/stel_multi_hop_chain.rs) | **VALIDATED** | 3 multi-hop golden rows; one external MCP call |
| **A-010** | — | **OPEN** | Intent-bucket A/B not run in Phase 2 |
| **A-011** | [`research/phase2-gate-report.md`](research/phase2-gate-report.md) | **OPEN** | Predictor ±20% not validated on full battery |
| **A-012** | [`research/A-012-bypass-policy.md`](research/A-012-bypass-policy.md), [`research/phase2-gate-report.md`](research/phase2-gate-report.md) | **PARTIAL** | Serve-only H3 scope; H3 PASS; two-hop completion not shipped |
| **A-013** | [`tests/stel_l2_admission.rs`](../tests/stel_l2_admission.rs) | **VALIDATED** | cache_hit admission + tests |
| **A-014** | [`src/stel/executor.rs`](../src/stel/executor.rs) degrade caps | **OPEN** | Degrade shipped; T3-large battery deferred |
| **A-029** | [`research/A-029-t24-restoration-signoff.md`](research/A-029-t24-restoration-signoff.md) | **VALIDATED** | 2/4 T2 equiv post-TX-04; P-T2 partial — `tokio/t2_block_on` + `django/t2_model` serve-eligible (`eligible_h6=true`), `tokio/t2_spawn` + `django/t2_queryset` bypass-only |

## When an assumption is invalidated

```text
INVALIDATED
  → freeze dependent phase work
  → document what failed (artifact + numbers)
  → research spike (code, battery row, external source)
  → update or replace assumption
  → re-validate
  → only then resume phase
```

**Example:** If A-005 fails (compact surface still \>5kB), research: slimmer JSON Schema, merge edit into symforge, or resource-first reads — **new assumption**, measure again. Do not raise H1 threshold to “make it pass.”

---

## Research outputs (required format)

When validation fails and research is needed:

```text
assumption_id: A-00X
failure: what the measurement showed
research: what was investigated (links, code paths, alternate designs)
conclusion: new assumption(s) with validation plan
resume: which phase item unblocks
```

Append as dated section below or link PR / note.

### Research log

#### A-029 — T2 reference parity (2026-06-14)

```text
assumption_id: A-029
failure: 0/4 T2 equivalence on tokio+django (rg baseline recall 5–14% vs index find_references)
research: docs/research/A-029-t2-spike.md; scripts/a029-t2-spike.cjs
conclusion: PIVOT — register P-T2 bypass-only for T2 reference tasks; eligible_h6=false; 8.1 index-recall program
resume: Phase 3 executor + 8.1 T2/T3 quality program — not Phase 2 runtime masking
```

#### A-029 — T2 reference parity, post-TX-04 resolution (2026-06-15)

```text
assumption_id: A-029
result: VALIDATED — 2/4 T2 equivalence after TX-01/TX-02/TX-04 remediation (was 0/4 at Phase 2)
evidence: docs/research/A-029-t2-replay.json (matches a029-tx04-results.json); T2.4 sign-off GO
rows: tokio/t2_block_on EQUIVALENT 70.9% and django/t2_model EQUIVALENT 28.2% (>=25%) → serve, eligible_h6=true;
      tokio/t2_spawn SYMFORGE-LESS 34.5% (<35%) and django/t2_queryset SYMFORGE-LESS 26.8% (<35%) → bypass, eligible_h6=false
policy: P-T2 becomes PARTIAL — row-level, not blanket. Non-equivalent rows remain bypass-only.
scope: row-level posture recorded in tests/fixtures/a029-t2/tasks.jsonl (external fixture home);
       docs/fixtures/routes.golden.jsonl (frozen 36-row in-repo corpus) unchanged.
non-claims: no H6/H7/H8 gate PASS; not full T2 closure (2/4 != 4/4); TX-03 bench deferred
```

---

## CI hook (future)

```text
stel-assumptions check
  → every OPEN assumption referenced by current phase → FAIL
  → every VALIDATED assumption has artifact path exists → PASS
```

Phase 0 deliverable includes seeding this file and validating **A-001..A-004** before any `src/stel/` code.
