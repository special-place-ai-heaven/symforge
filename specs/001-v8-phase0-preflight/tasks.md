# Tasks: SymForge v8 Phase 0 12A Pre-flight

**Input**: Design documents from `specs/001-v8-phase0-preflight/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/preflight-evidence-contract.md`, `quickstart.md`

**Scope**: Phase 0 Section 12A readiness evidence only. Do not modify `src/stel/**`, do not begin Phase 1 STEL implementation, and do not add Phase 4 deploy/admin/AAP implementation work.

**Tests**: No TDD source-code tests are requested. Validation tasks below are evidence checks and command runs that must produce artifacts.

**Organization**: Tasks are grouped by user story so each story can be completed and reviewed as an independent readiness increment.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel because it writes different files and does not depend on incomplete tasks.
- **[Story]**: Maps to user stories from `spec.md`.
- Every task includes an exact repo-local or external evidence path.

---

## Phase 1: Setup (Shared Evidence Structure)

**Purpose**: Create the evidence workspace and pin the external harness locations before collecting validation data.

- [x] T001 Create the `docs/research/` directory and `docs/research/phase0-12a-evidence-index.md` with sections for Measurement, Surface choice, Bypass harness, Process, blockers, and final decision links
- [x] T002 Resolve the sf-bench workspace by checking `E:\project\sf-bench` and `..\sf-bench`, then record the selected path or NO-GO blocker in `docs/research/phase0-12a-sf-bench-path.md`
- [x] T003 [P] Record the active Spec Kit inputs from `specs/001-v8-phase0-preflight/spec.md`, `specs/001-v8-phase0-preflight/plan.md`, and `specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md` in `docs/research/phase0-12a-evidence-index.md`
- [x] T004 [P] Create assumption-evidence placeholders for A-001, A-002, A-003, A-004, A-005, A-006, A-012, A-019, A-025, A-027, A-028, and A-032 in `docs/research/phase0-12a-assumption-evidence.md`
- [x] T005 [P] Create the independent reviewer sign-off template with GO, NO-GO, evidence producer, reviewer identity, checklist coverage, and blocker fields in `docs/research/phase0-12a-review-signoff.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish the binary Section 12A checklist, evidence contract, and scope guard before any story-specific evidence work starts.

**Critical**: No user story evidence should be accepted until this phase is complete.

- [x] T006 Copy the Section 12A checklist items from `docs/v8-gap-closure-plan.md` into a traceability table in `docs/research/phase0-12a-evidence-index.md`
- [x] T007 Map every checklist item to the evidence record shapes in `specs/001-v8-phase0-preflight/contracts/preflight-evidence-contract.md` and record the mapping in `docs/research/phase0-12a-evidence-index.md`
- [x] T008 Add a scope guard section to `docs/research/phase0-12a-evidence-index.md` that marks `src/stel/**`, Phase 4 deploy/admin work, and AAP convenience work as forbidden for this feature
- [x] T009 [P] Confirm `scripts/measure-schema-bytes.ps1` is the schema-byte helper for A-005 and A-025, then record its command line and output path in `docs/research/phase0-12a-evidence-index.md`
- [x] T010 [P] Confirm the selected sf-bench workspace from `docs/research/phase0-12a-sf-bench-path.md` contains `compare-results.js`, `routes.golden.jsonl`, and `RESULTS.md`, then record absolute paths or a NO-GO blocker in `docs/research/phase0-12a-evidence-index.md`

**Checkpoint**: Evidence structure and blockers are explicit. User story evidence work can proceed.

---

## Phase 3: User Story 1 - Decide Phase 1 Readiness (Priority: P1) [MVP]

**Goal**: Provide one auditable gate that can return GO or NO-GO for the first STEL implementation commit.

**Independent Test**: A reviewer can inspect `docs/research/phase0-12a-evidence-index.md` and `docs/research/phase0-12a-review-signoff.md` and determine a NO-GO if any required evidence is missing.

### Implementation for User Story 1

- [x] T011 [US1] Define the readiness decision procedure and NO-GO rules in `docs/research/phase0-12a-review-signoff.md`
- [x] T012 [US1] Add checklist coverage fields for satisfied, total applicable, exempt, and blocked Section 12A items in `docs/research/phase0-12a-review-signoff.md`
- [x] T013 [US1] Add a blocker table for OPEN assumptions, failed thresholds, missing artifacts, and missing reviewer sign-off in `docs/research/phase0-12a-review-signoff.md`
- [x] T014 [P] [US1] Add reviewer instructions that 7.x results are informational only and cannot be used as a v8 gate in `docs/research/phase0-12a-review-signoff.md`
- [x] T015 [US1] Link the readiness decision record from `docs/research/phase0-12a-evidence-index.md` to `docs/research/phase0-12a-review-signoff.md`

**Checkpoint**: User Story 1 can produce an auditable NO-GO without reading implementation code.

---

## Phase 4: User Story 2 - Trust The Measurement Ruler (Priority: P1)

**Goal**: Validate repeatability, manual baseline correctness, branch-binary harness execution, equivalence judging, and pre-flight gate computation.

**Independent Test**: A technical reviewer can confirm A-001 through A-004 and compare-results pre-flight readiness from the linked artifacts.

### Implementation for User Story 2

- [x] T016 [P] [US2] Run the first same-binary measurement battery and save the result path plus binary/input identity in `docs/research/A-001-measurement-repeatability.md`
- [x] T017 [P] [US2] Run the second same-binary measurement battery and save the result path plus binary/input identity in `docs/research/A-001-measurement-repeatability.md`
- [x] T018 [US2] Compute accepted-session net variance between the two A-001 runs and record PASS only if variance is no greater than 2% in `docs/research/A-001-measurement-repeatability.md`
- [x] T019 [P] [US2] Complete six competent-manual baseline spot checks and record all rows, expected manual behavior, measured M value, and PASS or FAIL in `docs/research/A-002-manual-spotcheck.md`
- [x] T020 [P] [US2] Build the branch binary and run the harness shakedown using the workspace in `docs/research/phase0-12a-sf-bench-path.md`, then record the command, binary identity, and shakedown JSON link in `docs/research/A-003-harness-shakedown.md`
- [x] T021 [US2] Validate the shakedown JSON contains equivalence outcome, accepted-serve flag, sGteM flag, controller decision, MCP call count, and H6 eligibility for every measured row in `docs/research/A-003-harness-shakedown.md`
- [x] T022 [P] [US2] Complete the A-004 equivalence audit over 20 stratified samples and record false positives, false negatives, sample rows, reviewer, and PASS only if FP plus FN is no greater than 10% in `docs/research/A-004-equiv-audit.md`
- [x] T023 [US2] Run the selected sf-bench `compare-results.js --preflight --release 8.0` from `docs/research/phase0-12a-sf-bench-path.md` on shakedown or self-diff inputs and record all H1 through H8 fields plus exit status in `docs/research/G-005-compare-results-preflight.md`
- [x] T024 [US2] Confirm the selected sf-bench `RESULTS.md` from `docs/research/phase0-12a-sf-bench-path.md` documents Section 8.7 and compare-results columns for v8 runs only, then record the evidence link in `docs/research/G-005-compare-results-preflight.md`
- [x] T025 [US2] Update A-001, A-002, A-003, A-004, and A-026 evidence links and verdicts in `docs/stel-assumptions.md`, then record G-005 gap evidence in `docs/research/phase0-12a-evidence-index.md`

**Checkpoint**: Measurement evidence is complete enough for a reviewer to trust or reject the ruler before STEL implementation starts.

---

## Phase 5: User Story 3 - Lock Route, Surface, And Bypass Policies (Priority: P2)

**Goal**: Settle the golden route corpus, public surface feasibility, schema-byte budgets, L0 surface choice, schema amortization policy, and bypass scoring rules.

**Independent Test**: The implementer can inspect route and surface evidence and determine the selected L0 shape, schema feasibility, and bypass accounting rules without reading `src/stel/**`.

### Implementation for User Story 3

- [x] T026 [P] [US3] Populate or validate the selected sf-bench `routes.golden.jsonl` from `docs/research/phase0-12a-sf-bench-path.md` with exactly 36 JSONL rows containing id, query, must_call, must_not_call, expected_decision, expected_equiv, chain, eligible_h6, and notes fields
- [x] T027 [US3] Run JSONL validation, duplicate-id detection, and required-field checks for the selected sf-bench `routes.golden.jsonl` from `docs/research/phase0-12a-sf-bench-path.md`, then record PASS or FAIL in `docs/research/A-028-golden-routes.md`
- [x] T028 [US3] Human-review at least 10 golden-route rows for expected_decision and expected_equiv semantics and record reviewer notes in `docs/research/A-028-golden-routes.md`
- [x] T029 [P] [US3] Run `scripts/measure-schema-bytes.ps1` and write the raw measurement artifact to `docs/research/A-005-schema-bytes.json`
- [x] T030 [US3] Evaluate A-005 public schema bytes against the 5,000-byte budget and A-025 edit schema bytes against the 1,500-byte budget, then record PASS, pivot, or NO-GO in `docs/research/A-005-schema-bytes-summary.md`
- [x] T031 [P] [US3] Run the L0 surface A/B comparison for compact-3, meta-tool, and full-32 candidates and save the result reference in `docs/research/A-019-l0-surface-choice.md`
- [x] T032 [US3] Select the L0 surface winner by accepted-session net while preserving equivalence, or record the blocking pivot, in `docs/research/A-019-l0-surface-choice.md`
- [x] T033 [P] [US3] Document A-006 and A-027 host schema amortization evidence or conservative worst-case accounting in `docs/research/A-006-host-schema.md`
- [x] T034 [P] [US3] Choose the A-012 bypass policy path and document either two-hop completion evidence or serve-only H3 interim scope in `docs/research/A-012-bypass-policy.md`
- [x] T035 [US3] Apply the chosen A-012 path to the selected sf-bench evidence surface from `docs/research/phase0-12a-sf-bench-path.md` by linking bypass-hop evidence or the compare-results serve-only H3 scope from `docs/research/A-012-bypass-policy.md`
- [x] T036 [P] [US3] Document P-FF and eligible H6 rules, including full-file bypass rows and `eligible_h6=false`, in the selected sf-bench golden-file README path recorded in `docs/research/phase0-12a-sf-bench-path.md`
- [x] T037 [US3] Update A-005, A-006, A-012, A-019, A-025, A-027, A-028, and A-032 evidence links and verdicts in `docs/stel-assumptions.md`

**Checkpoint**: Route, surface, schema, and bypass policy evidence is complete or explicitly blocks Phase 1.

---

## Phase 6: User Story 4 - Preserve The Pre-Implementation Boundary (Priority: P3)

**Goal**: Keep this feature bounded to Phase 0 readiness and prevent accidental STEL, Phase 4, admin UI, or AAP implementation work.

**Independent Test**: Scope evidence proves every completed item traces to Section 12A readiness and no `src/stel/**` deliverable is required.

### Implementation for User Story 4

- [x] T038 [P] [US4] Audit `specs/001-v8-phase0-preflight/tasks.md` for forbidden implementation tasks and record the result in `docs/research/phase0-12a-scope-boundary.md`
- [x] T039 [P] [US4] Verify the final evidence set contains no requirement to beat or pin `results-7.21.1-baseline.json` and record the result in `docs/research/phase0-12a-scope-boundary.md`
- [x] T040 [P] [US4] Review the Phase crosswalk for A-030 and record any drift, no-op result, or required doc update in `docs/research/A-030-phase-crosswalk.md`
- [x] T041 [US4] Update the decision log in `docs/ideation.md` with Phase 0 decisions for L0 surface, schema budget, bypass policy, and GO or NO-GO state
- [x] T042 [US4] Run a git diff path audit and record that no `src/stel/**` files changed for this feature in `docs/research/phase0-12a-scope-boundary.md`
- [x] T043 [US4] Update only the accepted Section 12A checkboxes in `docs/v8-gap-closure-plan.md` after evidence links are present and reviewed

**Checkpoint**: The boundary is explicit, documented, and reviewable.

---

## Phase 7: Polish & Cross-Cutting Validation

**Purpose**: Final evidence packaging, independent sign-off, and command validation.

- [x] T044 Run `.specify\scripts\powershell\check-prerequisites.ps1 -Json -PathsOnly` and record the output in `docs/research/phase0-12a-evidence-index.md`
- [x] T045 Run the unresolved-placeholder scan from `specs/001-v8-phase0-preflight/quickstart.md` and record the output in `docs/research/phase0-12a-evidence-index.md`
- [x] T046 Run final evidence-link validation across `docs/research/phase0-12a-evidence-index.md`, `docs/stel-assumptions.md`, and `docs/v8-gap-closure-plan.md`, then record missing links as blockers in `docs/research/phase0-12a-review-signoff.md`
- [x] T047 Run a timed reviewer dry-run and record whether the independent reviewer can reach and record GO or NO-GO within the SC-011 15-minute timebox in `docs/research/phase0-12a-review-signoff.md`
- [x] T048 Obtain independent reviewer sign-off or explicit NO-GO from a reviewer who did not produce the evidence bundle in `docs/research/phase0-12a-review-signoff.md`
- [x] T049 Record the final GO or NO-GO decision, checklist coverage, blocking gaps, and next action in `docs/research/phase0-12a-review-signoff.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup completion and blocks all user story phases.
- **User Story 1 (Phase 3)**: Depends on Foundational. Can complete the decision framework before evidence is ready.
- **User Story 2 (Phase 4)**: Depends on Foundational. Produces measurement evidence consumed by final sign-off.
- **User Story 3 (Phase 5)**: Depends on Foundational and can run after or alongside User Story 2 except where A-019 needs measurement outputs.
- **User Story 4 (Phase 6)**: Depends on Foundational and should run again after User Stories 2 and 3.
- **Polish (Phase 7)**: Depends on all desired evidence phases and produces the final GO or NO-GO record.

### User Story Dependencies

- **US1 (P1)**: Creates the readiness decision framework and can produce NO-GO independently after Phase 2.
- **US2 (P1)**: Independent measurement evidence, required before any final GO.
- **US3 (P2)**: Independent route, surface, schema, and bypass policy evidence, required before any final GO.
- **US4 (P3)**: Scope-boundary evidence, required before any final GO.

### Blocking Rules

- Any failed threshold, missing artifact link, unresolved Phase 1-blocking OPEN assumption, or missing independent sign-off results in NO-GO. External sf-bench workspace is **optional** (B-SFBENCH superseded by in-repo evidence per `docs/research/phase0-12a-sf-bench-path.md`).
- `src/stel/**` remains forbidden until every Section 12A checkbox is accepted and T048/T049 record independent sign-off.
- Section 12B items are not required for first `src/stel/**` commit and must not be pulled into this feature unless they are explicitly documented as future dependencies.

### Post-implementation doc refresh (2026-06-13)

Independent §12A review **GO** (Codex agent). Evidence commit `08f7d14`. T048/T049 complete. First `src/stel/` commit **authorized**. B-RESULTS deferred (not Phase 0 gate).

---

## Parallel Execution Examples

### User Story 2

```text
Task: "T019 manual spot checks in docs/research/A-002-manual-spotcheck.md"
Task: "T020 branch-binary shakedown in docs/research/A-003-harness-shakedown.md"
Task: "T022 equivalence audit in docs/research/A-004-equiv-audit.md"
```

### User Story 3

```text
Task: "T026 golden route corpus using docs/research/phase0-12a-sf-bench-path.md"
Task: "T029 schema-byte measurement in docs/research/A-005-schema-bytes.json"
Task: "T033 schema amortization policy in docs/research/A-006-host-schema.md"
Task: "T034 bypass policy in docs/research/A-012-bypass-policy.md"
```

### User Story 4

```text
Task: "T038 forbidden-task audit in docs/research/phase0-12a-scope-boundary.md"
Task: "T039 7.x non-gating audit in docs/research/phase0-12a-scope-boundary.md"
Task: "T040 phase crosswalk review in docs/research/A-030-phase-crosswalk.md"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 and Phase 2.
2. Complete Phase 3 to create the readiness decision framework.
3. Stop and validate that missing evidence yields an explicit NO-GO with blocker IDs.

### Incremental Delivery

1. Add User Story 2 to validate the measurement ruler.
2. Add User Story 3 to validate route, surface, schema, and bypass evidence.
3. Add User Story 4 to prove the pre-implementation boundary.
4. Complete Phase 7 only after all required evidence links exist.

### Final Readiness Rule

Final GO is valid only when `docs/v8-gap-closure-plan.md` Section 12A is fully checked, `docs/stel-assumptions.md` has Phase 1-blocking verdicts and artifact links, and `docs/research/phase0-12a-review-signoff.md` records independent reviewer sign-off.

---

## Notes

- [P] tasks write different files or independent evidence sections.
- Every user story is independently reviewable, but final GO depends on all required evidence.
- If any task discovers missing external artifacts, record a NO-GO blocker instead of inventing a success path.
- Do not update `src/stel/**` in this feature.
