# Feature Specification: SymForge v8 Phase 0 12A Pre-flight

**Feature Branch**: `v8/stel-architecture`

**Created**: 2026-06-13

**Status**: Draft

**Input**: User description: "Create a SymForge v8 Phase 0 12A pre-flight feature spec from docs/v8-gap-closure-plan.md. Objective: validate the measurement harness, golden route corpus, compare-results gate computation, schema-byte feasibility, bypass harness policy, and assumption-register evidence needed before any src/stel/ implementation."

## Clarifications

### Session 2026-06-13

- Q: Who must accept the evidence bundle before the final GO decision? → A: Independent reviewer sign-off is required for the final GO decision.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Decide Phase 1 Readiness (Priority: P1)

As the SymForge release owner, I need one auditable pre-flight gate that tells me whether Phase 1 STEL work may begin, so implementation does not start on unvalidated measurement assumptions.

**Why this priority**: This is the blocker for all later STEL implementation. If this gate is ambiguous, the branch can drift into code before the measurement ruler is trusted.

**Independent Test**: A reviewer can inspect the pre-flight evidence bundle and produce a GO or NO-GO decision for the first STEL implementation commit without reading implementation code.

**Acceptance Scenarios**:

1. **Given** every Phase 1-blocking pre-flight item has accepted evidence and independent reviewer sign-off, **When** the release owner reviews the readiness gate, **Then** the decision is GO for the first STEL implementation commit.
2. **Given** any Phase 1-blocking assumption remains OPEN, failed, missing evidence, or lacks independent reviewer sign-off, **When** the release owner reviews the readiness gate, **Then** the decision is NO-GO and cites the exact failed assumption, missing artifact, or missing sign-off.
3. **Given** a failed validation has a documented pass, pivot, or kill path, **When** the readiness gate is reviewed, **Then** the next action is explicit and the feature does not silently continue as if validated.

---

### User Story 2 - Trust The Measurement Ruler (Priority: P1)

As a technical reviewer, I need repeatable measurement evidence for token accounting, manual baseline correctness, branch-binary shakedown, and equivalence judging, so later savings claims are based on trusted inputs.

**Why this priority**: The v8 north star depends on measured economics. STEL controller work is not meaningful until the measurements are stable and reviewable.

**Independent Test**: The reviewer can evaluate the measurement evidence and confirm whether A-001, A-002, A-003, and A-004 are validated.

**Acceptance Scenarios**:

1. **Given** two same-binary measurement runs over the same pinned inputs, **When** variance is evaluated, **Then** the accepted-session net variance is no more than 2%.
2. **Given** six manual-baseline spot checks, **When** the reviewer compares expected manual behavior to recorded measurement rows, **Then** all six checks match the agreed competent-manual comparator.
3. **Given** an equivalence audit sample, **When** false positives and false negatives are counted, **Then** the combined error rate is no more than 10%.
4. **Given** a branch-binary shakedown run, **When** the reviewer checks the output, **Then** the run completed and produced all row fields required for gate computation.

---

### User Story 3 - Lock Route, Surface, And Bypass Policies (Priority: P2)

As a STEL implementer, I need the route corpus, public surface choice, schema budget, and bypass policy settled before implementation, so Phase 1 work is aimed at the selected product shape rather than a guess.

**Why this priority**: The compact surface and bypass behavior determine whether v8 can satisfy the economics promise. An unresolved surface or bypass policy would cause rework.

**Independent Test**: The implementer can inspect the route corpus and surface evidence and determine the selected L0 shape, schema feasibility, and bypass scoring rules.

**Acceptance Scenarios**:

1. **Given** the golden route corpus is complete, **When** it is validated, **Then** it contains exactly 36 valid rows with expected decision, expected equivalence, chain classification, and H6 eligibility.
2. **Given** the surface-choice measurements are complete, **When** the alternatives are compared, **Then** the selected public surface is justified by accepted-session net value and equivalence, or a documented pivot blocks Phase 1.
3. **Given** schema-byte measurements are complete, **When** the reviewer checks the public surface budget, **Then** the compact surface is within 5,000 bytes and the edit surface is within 1,500 bytes or an explicit pivot is accepted.
4. **Given** bypass policy evidence is complete, **When** the reviewer checks the scoring rules, **Then** bypass rows have either a two-hop completion policy or an explicit serve-only interim scope for H3.

---

### User Story 4 - Preserve The Pre-Implementation Boundary (Priority: P3)

As a maintainer, I need the spec to keep Phase 0 bounded to evidence and readiness, so STEL implementation, 8.1 transport work, admin UI work, and AAP integration changes do not enter this phase by accident.

**Why this priority**: The docs explicitly separate Phase 0 pre-flight from Phase 1 implementation and Phase 4 deploy/operator work. This feature should enforce that boundary.

**Independent Test**: The scope can be checked by confirming that all acceptance evidence relates to Phase 0 12A readiness and that no STEL implementation deliverable is required to complete this feature.

**Acceptance Scenarios**:

1. **Given** a proposed task under this feature, **When** it requires STEL implementation code, **Then** it is rejected as out of scope until the pre-flight gate is green.
2. **Given** a proposed task under this feature, **When** it belongs to Phase 4 deploy, admin UI, or operator convenience, **Then** it is deferred unless it is only documenting a Phase 0 readiness dependency.
3. **Given** the final pre-flight report, **When** a reviewer checks scope, **Then** every completed item traces to a Phase 0 12A checklist item, assumption verdict, or supporting evidence artifact.

### Edge Cases

- A measurement run completes but one or more required row fields are missing.
- The two repeated measurement runs exceed the allowed variance threshold.
- Manual spot-check evidence contradicts the recorded manual baseline.
- The equivalence audit exceeds the allowed false-positive and false-negative rate.
- The golden route corpus has fewer or more than 36 rows, invalid rows, duplicate task identities, or missing expected-decision fields.
- Schema-byte measurements exceed budget and no pivot decision is recorded.
- Bypass two-hop evidence is incomplete and H3 scope is not explicitly narrowed to serve-only rows.
- A 7.x baseline result is mistakenly treated as a v8 gate requirement.
- Repository docs conflict; the binding gap-closure plan takes precedence for this feature.
- Someone attempts to claim Phase 1 readiness while a Phase 1-blocking assumption remains OPEN.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The pre-flight feature MUST produce a single GO or NO-GO readiness decision for starting the first STEL implementation commit.
- **FR-002**: The readiness decision MUST be NO-GO unless every Phase 0 12A checklist item is accepted with linked evidence, independently reviewed, or explicitly marked as not a requirement by the binding docs.
- **FR-003**: The feature MUST validate A-001 through two repeated measurement runs over the same pinned inputs and MUST fail readiness if accepted-session net variance exceeds 2%.
- **FR-004**: The feature MUST validate A-002 through six manual-baseline spot checks and MUST fail readiness if any spot check contradicts the agreed competent-manual comparator.
- **FR-005**: The feature MUST validate A-003 by proving the v8 branch binary can complete a harness shakedown and produce the row fields required for gate computation.
- **FR-006**: The feature MUST validate A-004 through an equivalence audit over 20 stratified samples and MUST fail readiness if combined false positives and false negatives exceed 10%.
- **FR-007**: The feature MUST verify that the gate comparator can compute H1 through H8 in pre-flight mode before an 8.0 baseline exists.
- **FR-008**: The feature MUST verify that required row classifications are present for each measured task: equivalence outcome, accepted-serve flag, sGteM flag, controller decision, MCP call count, and H6 eligibility.
- **FR-009**: The feature MUST verify that the golden route corpus contains exactly 36 valid rows and includes expected decision, expected equivalence, route-chain classification, H6 eligibility, and reviewer notes where needed.
- **FR-010**: The feature MUST require human review of expected golden-route semantics for at least 10 rows before readiness can be GO.
- **FR-011**: The feature MUST validate A-005 by measuring whether the selected compact public surface fits within the 5,000-byte schema budget.
- **FR-012**: The feature MUST validate A-025 by measuring whether the edit surface fits within the 1,500-byte schema budget or by recording an accepted pivot.
- **FR-013**: The feature MUST validate A-019 by comparing candidate public surface shapes and selecting the winner by accepted-session net value while preserving equivalence.
- **FR-014**: The feature MUST document A-006 and A-027 schema amortization policy, including conservative accounting when host amortization remains unvalidated.
- **FR-015**: The feature MUST validate A-012 by either implementing a bypass completion check or explicitly scoping H3 to accepted serve rows until that check exists.
- **FR-016**: The feature MUST document P-FF and eligible-H6 rules in the golden-route evidence set, while leaving full enforcement for the later phase if permitted by the binding checklist.
- **FR-017**: The feature MUST update the assumption register with each Phase 0 verdict, validation date, artifact link, and any pass, pivot, or kill conclusion.
- **FR-018**: The feature MUST update the phase crosswalk and decision log when Phase 0 evidence changes readiness or resolves a previously open decision.
- **FR-019**: The feature MUST explicitly state that beating or pinning a 7.x baseline is not required for Phase 0 12A readiness.
- **FR-020**: The feature MUST keep STEL implementation work, 8.1 transport work, admin UI work, and AAP integration changes outside this feature unless they are only referenced as future dependencies.
- **FR-021**: The feature MUST fail readiness when any Phase 1-blocking assumption remains OPEN, INVALIDATED without a replacement, or missing a referenced artifact.
- **FR-022**: The feature MUST produce a reviewer-facing evidence summary that maps every completed readiness item to its source assumption or checklist item.
- **FR-023**: The feature MUST require final GO sign-off from an independent reviewer who did not produce the evidence bundle.

### Key Entities *(include if feature involves data)*

- **Pre-flight Readiness Decision**: The final GO or NO-GO outcome for starting the first STEL implementation commit. Key attributes: decision, independent reviewer, sign-off date, checklist coverage, blocking gaps, and evidence links.
- **Assumption Evidence**: Validation proof for an assumption. Key attributes: assumption ID, status, validation method, artifact path, verdict date, pass threshold, and pivot or kill notes.
- **Measurement Run**: A repeatable economics run used to validate measurement trust. Key attributes: run identity, pinned inputs, binary identity, accepted-session net, row classifications, variance contribution, and completion status.
- **Golden Route Row**: One route-corpus scenario used for trajectory and equivalence expectations. Key attributes: row identity, request, expected decision, expected equivalence, required and forbidden internal actions, chain type, H6 eligibility, and notes.
- **Gate Comparator Result**: A gate-computation output that reports H1 through H8 readiness fields in pre-flight mode. Key attributes: input files, gate fields, row-field completeness, pass/fail status, and diagnostics.
- **Schema Measurement**: Evidence that a proposed public surface fits within the allowed schema budget or requires a pivot. Key attributes: surface candidate, measured bytes, pass threshold, selected outcome, and pivot rationale.
- **Bypass Policy Evidence**: Proof that bypass rows either preserve completion through a second host action or are excluded from inappropriate small-file serve-loss accounting. Key attributes: affected rows, completion policy, H3 scope, H6 eligibility, and documented rationale.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of Phase 0 12A readiness items have accepted evidence or an explicit binding-doc exemption, plus independent reviewer sign-off, before the final decision is GO.
- **SC-002**: Two repeated measurement runs over the same pinned inputs show accepted-session net variance no greater than 2%.
- **SC-003**: Manual-baseline validation passes 6 of 6 spot checks.
- **SC-004**: Equivalence audit over 20 stratified samples shows combined false positives and false negatives no greater than 10%.
- **SC-005**: The branch-binary shakedown produces every row field required for gate computation for 100% of measured rows.
- **SC-006**: The gate comparator reports all H1 through H8 fields in pre-flight mode and gives an explicit pass/fail status.
- **SC-007**: The golden route corpus contains exactly 36 schema-valid rows, and at least 10 rows have reviewed expected-decision and expected-equivalence semantics.
- **SC-008**: The selected public surface has measured schema bytes at or below 5,000, and the edit surface has measured schema bytes at or below 1,500 or a documented accepted pivot.
- **SC-009**: The pre-flight evidence set documents either bypass two-hop completion or serve-only H3 scoping with no contradictory bypass accounting statements.
- **SC-010**: The assumption register contains verdicts and artifact links for every Phase 1-blocking assumption referenced by the final readiness decision.
- **SC-011**: An independent reviewer can determine the GO or NO-GO outcome from the evidence summary in 15 minutes or less.
- **SC-012**: The final Phase 0 report contains zero claims that 7.x benchmark results are a v8 readiness gate.
- **SC-013**: No STEL implementation deliverable is required to complete this feature, and any STEL implementation attempt remains blocked until the final readiness decision is GO.
- **SC-014**: Zero final GO decisions are accepted without recorded independent reviewer sign-off.

## Assumptions

- The binding source for this feature is `docs/v8-gap-closure-plan.md`, especially the Phase 0 12A checklist and related harness specifications.
- Companion docs such as `docs/stel-assumptions.md`, `docs/v8-master-plan.md`, `docs/stel-schema.md`, and `docs/ideation.md` provide supporting context but do not override the binding gap-closure plan.
- Existing progress noted in the binding docs may be reused only when the artifact exists and can be linked from the readiness evidence.
- Phase 0 can use pre-flight or self-diff evidence before an 8.0 baseline exists; pinning the first v8 baseline is a later release event.
- This feature is an evidence and readiness workflow, not the implementation of STEL layers.
- Independent reviewer means a reviewer other than the person who produced the pre-flight evidence bundle.
- The project constitution file is still a placeholder, so repo docs and AGENTS guidance govern this spec.
- External 8.1 work, including remote serve transport, admin UI, operator onboarding, and AAP convenience surfaces, is out of scope for this Phase 0 feature except where explicitly referenced as a future dependency.
