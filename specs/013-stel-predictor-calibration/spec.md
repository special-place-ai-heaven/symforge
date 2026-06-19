# Feature Specification: STEL Predictor Calibration

**Feature Branch**: `013-stel-predictor-calibration`

**Created**: 2026-06-19

**Status**: Draft

**Input**: User description: "Full predictor calibration: cross-session ledger persistence in stdio mode plus an auto-tune consumer that corrects the planner token-estimate constants from accumulated predicted-vs-actual error, transitioning calibration from `deferred` to a validated `tuned` state without claiming measured savings."

**Origin**: Closes the `calibration_auto_tune` item in `DEFERRED_ITEMS` (`src/stel/status.rs:19`). Surfaced as friction during AAP/operator dogfooding: the economics predictor reports a heuristic prediction error on every call but never improves, because (a) the auto-tune seam is inert by design (`src/stel/calibration.rs` is observational-only — "does not adjust L2 margins, fudge multipliers, or route decisions") and (b) in the default stdio/embed deployment the L4 ledger is in-memory and resets every session, so there is never enough accumulated data to learn from. Feature 010 grounded the *estimator* in real byte sizes and made calibration honestly `deferred`; this feature closes the loop by making the prediction actually *improve* from observed reality — honestly.

## Guiding principle

**LLM trust is the keystone (010 carries forward, non-negotiable).** This feature
exists to make the predictor *more accurate*, never to make it *look* more
accurate. Two spine rules govern every change:

1. **Honesty contract** — every LLM-facing figure stays an estimate. Calibration
   tuning grounds the estimate in observed history; grounding is still not
   measurement. No served figure may claim a measured saving. `calibration:` may
   only read `tuned` when backed by an artifact showing tuning *reduced*
   prediction error on data it did not train on; otherwise it stays `accumulating`
   or `deferred`. **Relabel never promotes an unproven state to `tuned`.**
2. **Calibration is a token-estimate concern only** — auto-tune adjusts the
   planner's response/schema/invoke token *estimates*. It MUST NOT alter routing
   correctness, policy/deny decisions, or any safety guard. A worse-than-baseline
   tuning MUST NOT be applied.

## Clarifications

### Session 2026-06-19

- Q: Scope — auto-tune only (serve mode), or full predictor calibration including stdio? → A: **Full.** Auto-tune alone is inert in the operator's actual deployment (stdio MCP), where the ledger is in-memory and resets per session, so calibration can never accumulate. This feature therefore includes cross-session ledger persistence for stdio/embed, not only the serve-mode durable store that already ships (010 FR-004), plus the auto-tune consumer.
- Q: Does tuning change served behavior or just labels? → A: Real behavior change — the tuned constants flow into L2 economics so predictions track reality and the adaptive branches (degrade/bypass) fire on better-grounded numbers. The honest `(est.)` label from 010 remains the floor for any figure not yet tuned.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Calibration data survives across stdio sessions (Priority: P1)

An agent (or operator) runs SymForge as a stdio MCP server across many short
sessions (the default deployment). Each call records a predicted-vs-actual
ledger event. Those events accumulate in a durable store that survives process
restart, so by the Nth session there is enough real data to calibrate from —
instead of resetting to zero every session and forever reporting `deferred`.

**Why this priority**: This is the root blocker for the operator's environment.
Without cross-session persistence the auto-tune consumer (US2) has nothing to
learn from in stdio mode, so US2 alone would silently never activate where it is
actually used. The durable SQLite ledger already ships in serve mode (010
FR-004); this story extends that durability to stdio/embed.

**Independent Test**: Run several stdio MCP sessions in sequence (start, issue
queries that serve results, stop) against the same state directory. After the
restarts, read the calibration/ledger surface and confirm `ledger_events`
reflects the cumulative count across all sessions (monotonic, non-reset), not the
single-session count.

**Acceptance Scenarios**:

1. **Given** a stdio session that recorded ledger events, **When** the process exits and a new stdio session starts against the same state directory, **Then** the prior events are still counted (the durable ledger restored, not reset to zero).
2. **Given** a long-lived deployment, **When** the durable ledger grows, **Then** it is bounded (oldest events pruned past a documented retention limit) so the store cannot grow without limit.
3. **Given** a deployment with no durable store available (read-only FS, or store open fails), **When** calibration runs, **Then** it degrades honestly to in-memory/`deferred` and says so (distinguishable from "off"), never silently claiming durable accumulation.

---

### User Story 2 - The predictor improves from observed error (Priority: P1)

An agent reads the economics prediction (predicted response tokens, predicted net
vs manual). After the tool has observed enough real predicted-vs-actual outcomes,
those predictions track reality: the planner's static estimate constants
(`400/800` per-step floors, schema/invoke constants) are corrected from the
accumulated error so the *next* prediction is closer to what actually happens.

**Why this priority**: This is the feature — the predictor that "never
calibrates" becomes a predictor that does. It delivers the core value (accurate
economics → correct adaptive degrade/bypass decisions) and is the thing the
operator asked for.

**Independent Test**: Replay a corpus of recorded events whose predictions were
systematically off (e.g. consistently under-predicting response tokens by a known
factor). Confirm the tuned constants, applied to a held-out slice of that corpus,
reduce mean absolute prediction error versus the static `400/800` floors — and
that a corpus with no systematic bias produces no harmful adjustment.

**Acceptance Scenarios**:

1. **Given** an adequate sample of events (>= the documented minimum) with a consistent prediction bias, **When** calibration runs, **Then** the derived estimate constants reduce mean prediction error on held-out events, and the new constants feed subsequent L2 economics.
2. **Given** a tuning candidate that would *increase* held-out prediction error, **When** calibration evaluates it, **Then** it is rejected and the prior (or static floor) constants stay in force — calibration never makes the predictor worse.
3. **Given** tuned constants are in force, **When** a prediction is rendered, **Then** it remains explicitly labeled an estimate; no served figure claims a measured saving.
4. **Given** calibration adjusts a constant, **When** the adjustment is applied, **Then** it is recorded in the audit trail (old value, new value, sample size, measured error delta) per the constitution's gated-action audit.

---

### User Story 3 - The calibration state is reported honestly (Priority: P2)

An agent or operator inspects calibration state and gets the truth at every stage:
`deferred` when there is no/insufficient data, `accumulating (n/min)` while
gathering, `tuned (error: before% -> after%)` only when an adjustment provably
reduced held-out error. Never a blanket "tuned"/"validated" without the artifact
behind it.

**Why this priority**: Honesty is the keystone (010). A calibration feature that
overstated its own maturity would be the exact trust failure 010 fixed. This makes
the new state machine legible and auditable, but it depends on US1/US2 existing.

**Independent Test**: Drive the state from cold (no events) through accumulating to
tuned using a known corpus; at each stage read `status detail: full` (and the
opt-in full envelope) and confirm the reported state, sample count, and (when
tuned) the measured before/after error delta match the underlying data — and that
`tuned` never appears without the error-reduction artifact.

**Acceptance Scenarios**:

1. **Given** zero or sub-threshold events, **When** status is read, **Then** calibration reads `deferred`/`accumulating (n/min)`, never `tuned`.
2. **Given** a tuning that reduced held-out error, **When** status is read, **Then** calibration reads `tuned` and surfaces the sample size and the before/after error figures that justify it.
3. **Given** any calibration state, **When** the surface is rendered, **Then** no field reads `validated`/`saved`/`active` unless the code and an artifact match the word (010 honesty contract, still binding).

---

### Edge Cases

- **Cold start**: no events -> stays `deferred`, no adjustment, no error. (Not a failure state.)
- **Corrupted / partial durable ledger**: open/read fails -> honest degrade to in-memory `deferred` with a distinguishable reason (mirrors 010 `DurableLedgerState::Disabled { reason }` vs `Unavailable`), never a crash or a silent wrong count.
- **Concurrent stdio sessions** writing the same state directory: the durable store must tolerate concurrent append without corruption or lost events. [NEEDS CLARIFICATION: is concurrent multi-process access to one state dir a supported topology, or single-writer assumed?]
- **Tool/version or estimator change** invalidates old samples: samples recorded under a different estimator must not silently pollute a new calibration. [NEEDS CLARIFICATION: invalidate-on-version-change, or tag samples with estimator version and filter?]
- **Tuning oscillation / instability**: repeated re-tuning must converge, not flip constants each session. Bounded step + hysteresis.
- **Multi-project**: one operator indexes several repos. [NEEDS CLARIFICATION: is calibration global to the install or scoped per indexed project?]

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST persist predicted-vs-actual ledger events across process restarts in stdio/embed deployments, not only in serve mode, anchored to the deployment's state directory.
- **FR-002**: The durable ledger MUST be bounded by a documented retention policy (count and/or age); growth past the bound prunes oldest events.
- **FR-003**: When a durable store is unavailable or fails to open, the system MUST degrade to in-memory observational calibration and report the degraded state distinguishably (broken vs. never-configured), never silently report durable accumulation it does not have.
- **FR-004**: The system MUST derive corrected token-estimate constants (response-token floors and schema/invoke constants) from accumulated predicted-vs-actual error once a documented minimum sample size is reached.
- **FR-005**: The system MUST validate a tuning candidate against held-out (not-trained-on) events and MUST reject any candidate that does not reduce mean prediction error versus the constants currently in force. Calibration MUST NOT make the predictor worse.
- **FR-006**: Accepted tuned constants MUST flow into L2 economics so subsequent predictions and adaptive (serve/degrade/bypass) decisions use them.
- **FR-007**: Auto-tune MUST affect token estimates only. It MUST NOT alter routing correctness, policy/deny decisions, or any safety guard; routing/golden-replay behavior MUST be unchanged by calibration state.
- **FR-008**: Every applied calibration adjustment MUST be auditable: old value, new value, sample size, and measured before/after error delta (constitution gated-action audit).
- **FR-009**: The calibration surface MUST report a truthful state machine — `deferred` / `accumulating (n/min)` / `tuned (error before -> after)` — and MUST NOT read `tuned`/`validated` without the backing error-reduction artifact (010 honesty contract).
- **FR-010**: Every served token figure MUST remain explicitly labeled an estimate even when tuned constants are in force; grounding in history is not measurement.
- **FR-011**: The operator MUST be able to inspect calibration state and reset/clear accumulated calibration (start over) without rebuilding the index.
- **FR-012**: Calibration MUST be reproducible/deterministic given a fixed event corpus (same inputs -> same tuned constants), so held-out validation and tests are stable.

### Key Entities *(include if feature involves data)*

- **DurableLedger (stdio-capable)**: cross-session store of predicted-vs-actual events; bounded, restart-surviving; the stdio/embed extension of the serve-mode SQLite ledger that ships today.
- **PredictionErrorSample**: one observed event's predicted vs actual response/schema/invoke tokens (+ decision class, est. byte size, estimator version), the unit calibration learns from.
- **TunedEstimateConstants**: the calibrated replacements for the static `400/800` floors and schema/invoke constants, plus the metadata (sample size, before/after error) that justifies them.
- **CalibrationState**: the honest state machine `deferred` -> `accumulating(n/min)` -> `tuned(before->after)`, surfaced on `status` and the opt-in full envelope.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a stdio deployment, after a documented number of real sessions the calibration state reaches `tuned` with a recorded before/after error artifact — i.e. the predictor that previously stayed `deferred` forever now calibrates in the operator's actual environment.
- **SC-002**: On a held-out slice of a biased event corpus, tuned constants reduce mean absolute prediction error by a meaningful margin versus the static `400/800` floors (target margin set in `/plan`); on an unbiased corpus, error does not increase.
- **SC-003**: The durable stdio ledger survives at least 3 consecutive process restarts with cumulative (non-reset) event counts, and never exceeds its documented storage bound.
- **SC-004**: Zero regressions in routing/golden-replay and policy/deny behavior across all calibration states (calibration changes estimates, not decisions).
- **SC-005**: No LLM-facing surface reads `tuned`/`validated`/`saved` without a matching artifact in any state (the 010 surface-honesty regression, extended to cover calibration states, stays green).

## Assumptions

- The `chars/4` token approximation remains the estimation unit; calibration corrects the *constants*, not the unit, and the result is still an estimate (010 honesty contract is binding).
- The serve-mode durable SQLite ledger (010 FR-004, `src/stel/ledger_store.rs`) is the basis to extend to stdio/embed; this feature does not rewrite the ledger schema beyond what persistence + sample tagging require.
- Routing, policy, and safety guards are out of scope for calibration to modify (FR-007); this feature touches the economics/estimate path only.
- `b_results` and `multi_step_planner` (the other `DEFERRED_ITEMS`) are out of scope for this feature and remain separately queued.

## Dependencies

- Builds on 010 (honest economics envelope + byte-grounded estimator + durable serve-mode ledger) and the compact-envelope default (branch `fix/stel-default-compact-envelope`): the tuned figures must render honestly in BOTH the compact one-liner (the new default) and the opt-in full block.
