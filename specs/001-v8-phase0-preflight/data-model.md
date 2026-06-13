# Data Model: SymForge v8 Phase 0 12A Pre-flight

This is an artifact data model for planning and evidence review. It is not a runtime database model.

## Entity: Pre-flight Readiness Decision

**Purpose**: Captures the final GO or NO-GO decision for starting the first STEL implementation commit.

**Fields**:

- `decision`: GO or NO-GO.
- `decision_date`: Date of decision.
- `independent_reviewer`: Person or agent that reviewed the evidence and did not produce the evidence bundle.
- `sign_off_reference`: Link or path to the signed review note.
- `checklist_coverage`: Count of satisfied section 12A items over total applicable items.
- `blocking_gaps`: List of remaining assumptions, artifacts, or contradictions blocking GO.
- `evidence_summary`: Link to the reviewer-facing evidence summary.

**Validation rules**:

- GO is invalid without independent reviewer sign-off.
- GO is invalid while any Phase 1-blocking assumption is OPEN or INVALIDATED without replacement.
- GO is invalid when any applicable section 12A checklist item lacks evidence or explicit binding-doc exemption.

**State transitions**:

- `draft` -> `ready_for_review` when all evidence links are present.
- `ready_for_review` -> `GO` when independent reviewer signs off and no blockers remain.
- `ready_for_review` -> `NO-GO` when a blocker is found.
- `NO-GO` -> `ready_for_review` only after blockers have pass, pivot, or kill updates.

## Entity: Assumption Evidence

**Purpose**: Records proof for one assumption in the Phase 0 readiness path.

**Fields**:

- `assumption_id`: Example: A-001.
- `statement`: Assumption being validated.
- `phase_blocked`: Phase or phases blocked by the assumption.
- `validation_kind`: performance, path, trajectory, research, or host measurement.
- `method`: Exact experiment or review method.
- `artifact`: Link or path to evidence.
- `verdict`: OPEN, VALIDATED, or INVALIDATED.
- `validated_at`: Date, when validated.
- `notes`: Pass, pivot, or kill conclusion.

**Validation rules**:

- VALIDATED requires an artifact path and date.
- INVALIDATED requires a documented next action.
- OPEN assumptions that block Phase 1 force NO-GO.

## Entity: Measurement Run

**Purpose**: Captures repeatable economics measurements used to trust the ruler.

**Fields**:

- `run_id`: Unique run identifier.
- `binary_identity`: Branch binary or artifact identity.
- `input_identity`: Corpus and pinned input identity.
- `accepted_session_net`: Accepted-session net value.
- `row_count`: Count of measured rows.
- `row_classification_complete`: Whether required row fields exist for every measured row.
- `variance_pair`: Link to paired run for A-001.
- `status`: complete, incomplete, or failed.

**Validation rules**:

- Two paired runs must use the same binary and pinned inputs.
- Accepted-session net variance must be no greater than 2%.
- Every measured row must include required classification fields for gate computation.

## Entity: Golden Route Row

**Purpose**: Defines one route and outcome expectation for trajectory validation.

**Fields**:

- `id`: Stable row identity.
- `request`: User-facing request or query.
- `must_call`: Internal actions expected to run.
- `must_not_call`: Internal actions that must not run.
- `expected_decision`: serve or bypass.
- `expected_equiv`: Expected equivalence outcome.
- `chain`: single or multi.
- `eligible_h6`: Whether the row belongs in the H6 denominator.
- `notes`: Reviewer notes.

**Validation rules**:

- The corpus must contain exactly 36 valid rows.
- All rows must have expected decision, expected equivalence, chain, and H6 eligibility.
- At least 10 rows must have reviewed expected-decision and expected-equivalence semantics.
- Duplicate row identities are invalid.

## Entity: Gate Comparator Result

**Purpose**: Shows that H1 through H8 fields are computable in pre-flight mode.

**Fields**:

- `inputs`: Baseline and candidate artifact references, or self-diff references.
- `release_scope`: Target gate scope, such as 8.0 pre-flight.
- `h1_schema_bytes`: Measured schema bytes.
- `h2_trajectory_pass_rate`: Golden route pass rate.
- `h3_small_serve_losses`: Count of small-file accepted serve rows with sGteM.
- `h4_session_net_accepted`: Accepted-session net.
- `h5_single_chain_call_count`: Single-chain MCP call count result.
- `h6_equiv_over_eligible`: Equivalence ratio over eligible rows.
- `h7_repeatability_variance`: Repeatability variance.
- `h8_language_loss_summary`: Per-language accepted serve loss summary.
- `exit_status`: pass, fail, or diagnostic.

**Validation rules**:

- Pre-flight mode must emit every H1 through H8 field even when the 8.0 baseline is not pinned.
- Missing row classification fields force failure or diagnostic NO-GO.

## Entity: Schema Measurement

**Purpose**: Records whether the selected public surface fits the H1 and edit budgets.

**Fields**:

- `surface_candidate`: Candidate public surface shape.
- `schema_bytes`: Measured total visible schema bytes.
- `edit_schema_bytes`: Measured edit-surface schema bytes.
- `selected`: Whether this candidate is selected.
- `pivot`: Pivot decision if budgets fail.
- `artifact`: Measurement artifact path.

**Validation rules**:

- Selected surface must be no greater than 5,000 bytes or have an accepted pivot.
- Edit surface must be no greater than 1,500 bytes or have an accepted pivot.

## Entity: Bypass Policy Evidence

**Purpose**: Documents how bypass rows are scored during pre-flight readiness.

**Fields**:

- `policy`: two-hop completion or serve-only H3 scope.
- `affected_rows`: Rows covered by the policy.
- `completion_check`: Evidence path for bypass completion, if available.
- `h3_scope`: How H3 treats bypass rows.
- `h6_eligibility_rule`: How H6 eligibility is set.
- `contradictions`: Any conflicting accounting statements found.

**Validation rules**:

- The policy must be explicit before GO.
- Contradictory bypass accounting statements force NO-GO.
- If two-hop completion is absent, H3 must be explicitly scoped to accepted serve rows.
