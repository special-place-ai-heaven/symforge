# Research: SymForge v8 Phase 0 12A Pre-flight

## Decision: Treat Phase 0 as an evidence gate, not an implementation phase

**Rationale**: The binding gap-closure plan states that the first `src/stel/` commit is blocked until section 12A is fully green. The feature therefore plans evidence collection, validation, and reviewer sign-off only.

**Alternatives considered**: Starting STEL types while evidence is collected in parallel was rejected because it violates the explicit stop condition and risks building on an untrusted measurement ruler.

## Decision: Use `docs/v8-gap-closure-plan.md` section 12A as the readiness checklist

**Rationale**: The feature spec and repo docs identify this file as the execution truth. Other docs provide context, but section 12A contains the binary Phase 1 pre-flight requirements.

**Alternatives considered**: Deriving a new checklist from the master plan was rejected because it would duplicate and potentially drift from the binding checklist.

## Decision: Final GO requires independent reviewer sign-off

**Rationale**: The repo rules forbid self-approved completion claims. Independent sign-off makes the final decision auditable and prevents threshold-passing artifacts from being accepted without review of scope, contradictions, and artifact links.

**Alternatives considered**: Automated thresholds alone were rejected because they do not catch missing context or contradictory evidence. Release-owner-only sign-off was rejected because the evidence producer could become the approver.

## Decision: Gate comparator must support pre-flight mode before an 8.0 baseline exists

**Rationale**: The binding plan allows pre-flight or self-diff execution before the first v8 baseline is pinned. The readiness requirement is that H1 through H8 fields are computable and row classifications exist, not that a release regression baseline already exists.

**Alternatives considered**: Requiring `results-v8-8.0-baseline.json` during Phase 0 was rejected because the docs pin that artifact at the 8.0 tag, after STEL economics ship.

## Decision: Golden route corpus is a first-class contract

**Rationale**: The golden route corpus validates path and decision semantics separately from implementation tests. It must contain exactly 36 rows with expected decision, expected equivalence, chain classification, and H6 eligibility, with at least 10 rows reviewed for semantic correctness.

**Alternatives considered**: Relying only on aggregate battery results was rejected because it would not prove the route and bypass decisions that Phase 1 depends on.

## Decision: Schema-byte feasibility is measured before locking the L0 surface

**Rationale**: The H1 budget and edit-surface budget determine whether the compact public surface is feasible. The plan must allow documented pivots when the measured shape exceeds budget.

**Alternatives considered**: Choosing compact-3 by design preference was rejected because A-019 and A-025 are still validation gates.

## Decision: Bypass evidence may use two-hop completion or an explicit serve-only H3 interim scope

**Rationale**: The binding checklist permits A-012 to pass by implementing two-hop completion or by scoping H3 to accepted serve rows until completion checking exists. The plan must make that choice explicit to avoid contradictory bypass accounting.

**Alternatives considered**: Treating bypass rows as normal equivalence rows was rejected because the binding docs exclude bypass-policy rows from the H6 denominator and score bypass economics separately.

## Decision: Assumption register is the source for unlock state

**Rationale**: `docs/stel-assumptions.md` defines the workflow for OPEN, VALIDATED, and INVALIDATED assumptions. The final evidence summary must map Phase 1-blocking assumptions to verdicts and artifact links.

**Alternatives considered**: Keeping readiness state only in a release note or checklist was rejected because it would split authority from the assumption register.

## Decision: 7.x benchmark results remain informational

**Rationale**: The v8 docs explicitly say the 7.x run motivated the paradigm shift but does not gate v8 readiness. Phase 0 readiness must not require beating or pinning a 7.x baseline.

**Alternatives considered**: Comparing Phase 0 artifacts against 7.x results was rejected because it would measure the old product on the wrong success criteria.
