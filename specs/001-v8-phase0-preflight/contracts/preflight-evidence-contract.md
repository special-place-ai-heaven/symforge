# Contract: Phase 0 12A Pre-flight Evidence

This contract defines the evidence records reviewers must be able to inspect before approving the first STEL implementation commit.

## Readiness Decision Record

```yaml
decision: GO | NO-GO
decision_date: YYYY-MM-DD
independent_reviewer: "<name or agent id>"
sign_off_reference: "<path or URL>"
checklist_coverage:
  satisfied: <integer>
  total_applicable: <integer>
blocking_gaps:
  - id: "<assumption or checklist id>"
    reason: "<why GO is blocked>"
evidence_summary: "<path>"
```

**Rules**:

- `decision: GO` requires `independent_reviewer` and `sign_off_reference`.
- `decision: GO` requires `blocking_gaps` to be empty.
- The reviewer must not be the producer of the evidence bundle.

## Assumption Evidence Record

```yaml
id: A-001
statement: "<assumption statement>"
phase_blocked: [0, 1]
validation:
  kind: performance | path | trajectory | research | host_measurement
  method: "<exact command, experiment, or review>"
  artifact: "<path>"
verdict: OPEN | VALIDATED | INVALIDATED
validated_at: YYYY-MM-DD | null
notes: "<pass, pivot, or kill conclusion>"
```

**Rules**:

- `VALIDATED` requires an artifact path and validation date.
- `INVALIDATED` requires a documented pass, pivot, or kill next step.
- Phase 1-blocking `OPEN` records block the readiness decision.

## Measurement Row Classification

Every measured row used for gate computation must expose:

```json
{
  "equivalence": "EQUIVALENT|SYMFORGE-LESS|SYMFORGE-MORE|BYPASS",
  "acceptedServe": true,
  "sGteM": false,
  "decision": "serve|bypass|degrade|cache_hit",
  "mcpCalls": 1,
  "eligibleH6": true
}
```

**Rules**:

- Missing fields block gate-comparator acceptance.
- `acceptedServe` must not hide `sGteM`; H3 evaluates small-file accepted serve losses separately.

## Golden Route Row

Each route-corpus row must follow this shape:

```json
{
  "id": "repo/task_name",
  "query": "...",
  "must_call": ["find_references"],
  "must_not_call": [],
  "expected_decision": "serve",
  "expected_equiv": true,
  "chain": "single",
  "eligible_h6": true,
  "notes": "reviewed expectation"
}
```

**Rules**:

- The corpus must contain exactly 36 rows.
- Row identities must be unique.
- At least 10 rows must have reviewed expected-decision and expected-equivalence semantics.

## Gate Comparator Summary

```yaml
mode: preflight
release_scope: "8.0"
inputs:
  baseline: "<path>"
  candidate: "<path>"
gates:
  H1:
    schemaBytes: <integer>
  H2:
    trajectoryPassRate: <number>
  H3:
    smallServeSGteMCount: <integer>
  H4:
    sessionNetAccepted: <number>
  H5:
    singleChainMcpCallsOk: true
  H6:
    equivalent: <integer>
    eligible: <integer>
  H7:
    acceptedNetVariance: <number>
  H8:
    perLanguageAcceptedLosses: {}
exit_status: pass | fail | diagnostic
```

**Rules**:

- Pre-flight mode must emit all H1 through H8 fields.
- Before the 8.0 baseline exists, self-diff or synthetic fixture inputs are allowed only for readiness computation, not release regression claims.

## Schema Measurement Record

```yaml
surface_candidate: "compact-3 | meta-1 | meta-2 | full-32"
schema_bytes: <integer>
edit_schema_bytes: <integer | null>
selected: true | false
pivot: null | "<accepted pivot>"
artifact: "<path>"
```

**Rules**:

- Selected visible schema must be no greater than 5,000 bytes or carry an accepted pivot.
- Edit schema must be no greater than 1,500 bytes or carry an accepted pivot.

## Bypass Policy Record

```yaml
policy: two-hop-completion | serve-only-h3-scope
affected_rows:
  - "<row id>"
completion_check: "<path or null>"
h3_scope: "<how small-file sGteM is counted>"
h6_eligibility_rule: "<how bypass rows affect eligible denominator>"
contradictions: []
```

**Rules**:

- `contradictions` must be empty for GO.
- If `policy` is `serve-only-h3-scope`, the evidence must explicitly say bypass completion is not yet claimed.
