# Phase 0 §12A — assumption evidence placeholders

**Created:** 2026-06-13  
**Task:** T004  
**Contract:** [preflight-evidence-contract.md](../../specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md)

Phase 1-blocking assumptions tracked for §12A pre-flight. Update verdicts only when linked artifacts satisfy contract rules.

---

## A-001 — Measurement repeatability

```yaml
id: A-001
statement: "sf-bench S/M/N token method is stable across re-runs on same binary"
phase_blocked: [0, 1]
validation:
  kind: performance
  method: "Re-run battery 2×; accepted-session net variance ≤ 2%"
  artifact: docs/research/A-001-measurement-repeatability.md
verdict: OPEN
validated_at: null
notes: "BLOCKED: sf-bench workspace missing (B-SFBENCH)"
```

---

## A-002 — Manual baseline spot-check

```yaml
id: A-002
statement: "Competent-manual baseline matches sf-bench M"
phase_blocked: [0, 1]
validation:
  kind: performance
  method: "6 spot checks: manual harness vs judge expectations"
  artifact: docs/research/A-002-manual-spotcheck.md
verdict: OPEN
validated_at: null
notes: "BLOCKED: sf-bench workspace missing (B-SFBENCH)"
```

---

## A-003 — Harness shakedown

```yaml
id: A-003
statement: "v8 branch release binary runs full harness without error"
phase_blocked: [0, 1]
validation:
  kind: performance
  method: "results-v8-harness-shakedown.json on target/release"
  artifact: docs/research/A-003-harness-shakedown.md
verdict: OPEN
validated_at: null
notes: "BLOCKED: sf-bench workspace missing (B-SFBENCH)"
```

---

## A-004 — Equivalence audit

```yaml
id: A-004
statement: "Equivalence judge correlates with human good-enough on sampled rows"
phase_blocked: [0, 1]
validation:
  kind: performance
  method: "20 stratified samples; FP+FN ≤ 10%"
  artifact: docs/research/A-004-equiv-audit.md
verdict: OPEN
validated_at: null
notes: "BLOCKED: sf-bench workspace missing (B-SFBENCH)"
```

---

## A-005 — Public schema budget (H1)

```yaml
id: A-005
statement: "Compact 3-tool MCP surface ≤ 5,000 B JSON schema"
phase_blocked: [0, 1]
validation:
  kind: host_measurement
  method: "scripts/measure-schema-bytes.ps1"
  artifact: docs/research/A-005-schema-bytes-summary.md
verdict: OPEN
validated_at: null
notes: "BLOCKED: sf-bench missing (B-SFBENCH). Stub proposal: docs/research/A-005-compact-stub-proposal.md (awaiting approval)"
```

---

## A-006 — Host schema amortization

```yaml
id: A-006
statement: "Hosts amortize schema so per-call tax < sf-bench ÷50 on long sessions"
phase_blocked: [0, 1]
validation:
  kind: host_measurement
  method: "Host measurement or conservative worst-case accounting"
  artifact: docs/research/A-006-host-schema.md
verdict: OPEN
validated_at: null
notes: "POLICY DOCUMENTED: conservative worst-case until host-validated"
```

---

## A-012 — Bypass policy

```yaml
id: A-012
statement: "Bypass eliminates sGteM while preserving task completion"
phase_blocked: [0, 1, 2]
validation:
  kind: performance
  method: "Two-hop harness OR serve-only H3 interim scope"
  artifact: docs/research/A-012-bypass-policy.md
verdict: OPEN
validated_at: null
notes: "INTERIM POLICY SELECTED: serve-only H3 scope until two-hop lands"
```

---

## A-019 — L0 surface choice

```yaml
id: A-019
statement: "Selected L0 surface beats alternatives on session_net and equivalence"
phase_blocked: [0, 1]
validation:
  kind: performance
  method: "A/B compact-3 vs meta-tool vs full-32 on pinned battery"
  artifact: docs/research/A-019-l0-surface-choice.md
verdict: OPEN
validated_at: null
notes: "BLOCKED: sf-bench battery + compact stub required"
```

---

## A-025 — Edit schema budget

```yaml
id: A-025
statement: "symforge_edit JSON Schema ≤ 1,500 B or accepted pivot"
phase_blocked: [1]
validation:
  kind: host_measurement
  method: "Measured list_tools bytes for edit surface"
  artifact: docs/research/A-005-schema-bytes-summary.md
verdict: OPEN
validated_at: null
notes: "PIVOT DOCUMENTED: merge edit into symforge with intent=edit until measured"
```

---

## A-027 — Battery schema divisor

```yaml
id: A-027
statement: "Battery schema divisor ÷50 is harness-only until A-006 host-validated"
phase_blocked: [0, 1]
validation:
  kind: research
  method: "Document in sf-bench spec; controller uses conservative max"
  artifact: docs/research/A-006-host-schema.md
verdict: OPEN
validated_at: null
notes: "POLICY DOCUMENTED: linked with A-006 conservative accounting"
```

---

## A-028 — Golden route semantics

```yaml
id: A-028
statement: "Golden rows include expected_equiv and expected_decision"
phase_blocked: [0, 1, 2]
validation:
  kind: path
  method: "36-row JSONL validation + 10-row human semantic review"
  artifact: docs/research/A-028-golden-routes.md
verdict: OPEN
validated_at: null
notes: "BLOCKED: sf-bench routes.golden.jsonl missing (B-SFBENCH)"
```

---

## A-032 — P-FF full-file bypass rows

```yaml
id: A-032
statement: "Full-file review tasks use P-FF (bypass, eligible_h6=false)"
phase_blocked: [0, 1]
validation:
  kind: path
  method: "4 rows in routes.golden.jsonl + README rules"
  artifact: docs/research/A-012-bypass-policy.md
verdict: OPEN
validated_at: null
notes: "RULES DOCUMENTED; corpus enforcement blocked on B-SFBENCH"
```
