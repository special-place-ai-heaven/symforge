# Feature Specification: v8 Trust Remediation

**Feature Branch**: `010-v8-trust-remediation`

**Created**: 2026-06-17

**Status**: Draft

**Input**: User description: "Make SymForge trustworthy to the LLMs that call it (the keystone), without rewriting the v8 architecture. Discovery verified in docs/reviews/v8-trust-remediation-ledger.md (two external skeptical reviews + two advisory programs + a 5-agent code-verification panel). The surfaces an LLM reads on every call — status, the economics envelope, error/recovery text, tool descriptions, public docs — overstate what the code delivers. Make every LLM-facing string true or explicitly labeled heuristic/observational/deferred; fix the two real bugs (status reports a working index as empty; the if_match write-guard never enforces). The trust debt is in the presentation layer, not the engine."

## Guiding principle

**LLM trust is the keystone.** A code-intelligence tool exists to be *believed* by
the model that calls it. Every overstated surface teaches the agent the tool is
unreliable; once caught overstating, the model discounts everything after. Two
spine rules govern every change:

1. **Honesty contract** — every LLM-facing string is **true**, or **explicitly
   labeled** heuristic / observational / deferred. No field named `saved`, `net`,
   `active`, or `validated` survives unless the code matches the word.
2. **Relabel is a valid fix** — making a number honest (e.g. renaming a constant
   to `estimated`/`heuristic`) is a legitimate, shippable remediation. But
   **relabel ≠ validate**: a label change never promotes an OPEN assumption to
   VALIDATED.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every reported number/label is honest (Priority: P1)

An agent (or operator) reads the tool's self-reported status banner and
economics envelope and can take every field at face value: a number labeled
"saved" reflects a real saving; a subsystem labeled "active" is wired; anything
not yet proven is plainly marked heuristic, observational, or deferred.

**Why this priority**: This is the keystone and the highest-leverage, lowest-risk
move — it can be delivered with **zero behavior change** (relabeling only), and it
removes the single largest source of agent mistrust today. Shippable alone as the
quick-win.

**Independent Test**: Read every field the status banner and economics envelope
emit; confirm each is either provably accurate against the underlying state or
carries an explicit heuristic/observational/deferred qualifier — and that no field
named "saved"/"net"/"validated" presents a constant or a gross counter as a
measured result.

**Acceptance Scenarios**:

1. **Given** the economics envelope, **When** it reports a savings/prediction figure that is not derived from real measurement, **Then** that figure is explicitly labeled heuristic/estimated (not presented as a measured saving).
2. **Given** a cumulative session figure, **When** it is a running total of work done (not a true net of savings), **Then** its name reflects what it is (e.g. "tokens served"), never an unqualified "net savings" that can only ever grow.
3. **Given** the status banner's subsystem and calibration labels, **When** a subsystem is in-memory-only, deferred, or unwired, **Then** the label says so (enumerated state), not a blanket "active"/"pending" that implies more.
4. **Given** the assumptions/capability record, **When** an assumption is OPEN, **Then** no shipped surface claims it as validated, and any prior "VALIDATED" verdict not backed by an artifact is demoted.

---

### User Story 2 - Status tells the truth about the index (Priority: P1)

An agent checks index health and gets the truth: if the tool just answered a
query from a populated index, the health readout reports that same populated
index — never "empty / not ready" while the tool demonstrably serves results.

**Why this priority**: This is a real bug, not a label: in the default deployment
the health readout reads a different (empty) index than the one serving queries,
so it reports a fully working system as broken. That actively teaches agents the
tool is down when it is up — the most corrosive possible trust failure.

**Independent Test**: In the default deployment topology, run a query that
succeeds against the live index, then read the health/status; confirm the reported
index readiness and counts match what the query actually used (non-zero, ready).

**Acceptance Scenarios**:

1. **Given** the default deployment where queries are served from a warm index, **When** a query succeeds, **Then** a subsequent status read reports that index as ready with matching counts (not empty / not-ready).
2. **Given** a health readout has no honest path to the real index, **When** the agent is on the default (compact) surface, **Then** there is still a status readout that reports the real index health (no "no honest tool exists" gap).
3. **Given** a durable subsystem that failed to open versus one that is simply not wired, **When** status reports it, **Then** the two states are distinguishable (not both collapsed to a single ambiguous "unavailable").

---

### User Story 3 - A guarded edit actually guards (Priority: P1)

An agent applies a symbol edit with an optimistic-concurrency guard ("only apply
if the body still matches what I saw"). If another writer changed the file in the
meantime, the apply is rejected — the agent's stale edit never silently overwrites
the concurrent change, and the response never reports a successful guarded apply
when the guard was not honored.

**Why this priority**: This is a data-integrity bug: the guard is advertised but
never enforced at the write, so under concurrency (including this project's own
multi-agent workflow) a stale edit can silently clobber another writer's change
while reporting success. A safety guarantee that isn't kept is worse than none.

**Independent Test**: Apply a guarded edit; deterministically mutate the file on
disk between the guard check and the write; confirm the apply is **rejected** (no
write), the on-disk concurrent change is preserved, and the response does not
claim a successful guarded apply. Control: same flow without the concurrent change
succeeds.

**Acceptance Scenarios**:

1. **Given** a guarded apply whose target body matched at check time, **When** the on-disk body diverges before the write, **Then** the apply is rejected and the divergent on-disk content is left intact.
2. **Given** the negative control (no concurrent change), **When** the guarded apply runs, **Then** it succeeds and the result matches the requested edit.
3. **Given** any guarded apply, **When** the response is rendered, **Then** it only claims a guarded apply when the guard was actually enforced at the write.

---

### User Story 4 - Recoverable cold start, no dead-end (Priority: P2)

A fresh agent attaching to a freshly installed tool can always make progress: the
workspace is indexed automatically, or — if it isn't — the error tells the agent a
recovery step it can actually perform on its current surface. The agent is never
told to call a capability that its surface forbids.

**Why this priority**: Today a default cold start can bind an empty index and then
emit an error naming a recovery action unavailable on the default surface — an
unrecoverable loop. High user/agent impact, but it depends on the truth/recovery
foundation and is a step beyond the keystone honesty work.

**Independent Test**: Simulate a fresh default attach with no pre-indexed
workspace; confirm either the workspace gets indexed automatically, or the agent
receives a recovery message that names only actions callable on its current
surface (and never a forbidden capability).

**Acceptance Scenarios**:

1. **Given** a fresh default attach, **When** the workspace can be discovered, **Then** it is indexed automatically and queries work without manual intervention.
2. **Given** an empty index on the default surface, **When** a query fails, **Then** the recovery text names only steps available on that surface (re-launch from the project root, or the documented opt-out) and never a forbidden capability.
3. **Given** the default desktop launch path, **When** it starts, **Then** it discovers the project workspace (not an unrelated home directory) so the index is populated.

---

### User Story 5 - Economics grounded in reality (or honestly labeled) (Priority: P2)

The economics figures an agent sees reflect the actual work — predictions derived
from real file/response size, so the adaptive economics behavior (serve / degrade
/ bypass) is genuinely driven by the request rather than parked permanently in one
branch by a constant. Until grounded, every figure is labeled heuristic.

**Why this priority**: The headline "token economics" is currently constant-driven,
so its adaptive branches never fire on real traffic and its numbers can contradict
the recorded outcome. Grounding makes the feature real; but the honest label
(US1) ships first so nothing waits on this.

**Independent Test**: Run the same operation over a small file and a large file;
confirm the predicted figures differ in proportion to the real size, and that at
least one non-default economics branch becomes reachable for an appropriately
small/cheap request.

**Acceptance Scenarios**:

1. **Given** two operations over materially different file sizes, **When** predictions are produced, **Then** the predicted figures differ (not a fixed constant for every request).
2. **Given** a sufficiently small/cheap request, **When** the economics gate evaluates it, **Then** a non-serve economics outcome is reachable (the adaptive behavior is no longer decorative).
3. **Given** any equivalence/accuracy claim in the test corpus, **When** it is presented as measured, **Then** it is actually asserted by a test, or the claim is removed.

---

### User Story 6 - Honest public record + enforced honesty (Priority: P2)

An operator (or evaluating agent) reading the public docs and the capability
record sees the true default surface and a clear map of what is proven vs.
heuristic vs. observational vs. deferred — and the project's automation prevents a
future regression where a shipped claim outruns the evidence.

**Why this priority**: Closes the loop so the honesty work doesn't silently rot:
the docs match reality, the premise is framed as a bet-under-test, and a guard
makes "shipped claim with an OPEN assumption" fail automatically. Important for
durability, but it follows the runtime truth/labels.

**Independent Test**: Read the public docs and the capability record; confirm they
describe the real default surface and map each capability to its proof state; and
confirm the automated honesty guard fails when a product claim is paired with an
unproven assumption.

**Acceptance Scenarios**:

1. **Given** the public docs and client configuration, **When** they describe the tool surface, **Then** they state the real default surface and present the larger legacy surface as a documented opt-out.
2. **Given** the capability record, **When** an operator reads it, **Then** each capability maps to a proof state (implemented / heuristic / observational / deferred) tied to an assumption identifier.
3. **Given** the automated honesty guard, **When** a shipped surface claims a capability whose assumption is OPEN, **Then** the guard fails (the regression cannot merge silently).

---

### Edge Cases

- A subsystem is wired but currently failing (vs. never configured) — status must distinguish "broken" from "off"; both must not collapse to one ambiguous label.
- The reported recovery action is callable on the full surface but not the default surface — recovery text must be computed from the *active* surface, never a fixed string.
- A guarded edit on a file that grows beyond the best-effort backup threshold, or is deleted mid-apply — the guard must still reject on divergence; the best-effort backup must not be presented as a transactional undo.
- An assumption has supporting evidence in one place and is marked OPEN in another — the record must have one source of truth per identifier.
- The honesty guard itself must not block a legitimate change that correctly labels a still-OPEN assumption (labeling honestly is allowed; claiming validation is not).
- A figure that is a coarse approximation (e.g. derived, not measured) must be labeled as such even where it is "honest-ish", so the agent knows its precision.

## Requirements *(mandatory)*

### Functional Requirements

**Honest surfaces (US1)**

- **FR-001**: Every economics-envelope figure that is not derived from real measurement MUST be explicitly labeled heuristic/estimated; no figure may be presented as a measured saving unless it is one.
- **FR-002**: A cumulative session figure MUST be named for what it is; a running total of work performed MUST NOT be presented as an unqualified "net savings".
- **FR-003**: Status subsystem/calibration labels MUST reflect real state via enumerated values (e.g. in-memory-only vs durable vs unavailable; deferred vs observational vs tuned), not blanket "active"/"pending".
- **FR-004**: The deferred-items and assumption records MUST be derived from real state (no stale frozen list that contradicts what actually shipped), and any "VALIDATED" verdict not backed by an artifact MUST be demoted.
- **FR-005**: Relabeling MUST NOT change runtime behavior, and MUST NOT promote any OPEN assumption to validated.

**Status truth (US2)**

- **FR-006**: In every deployment topology, the index health a status read reports MUST reflect the same index that serves queries (a successful query implies a ready, non-zero status).
- **FR-007**: The default (compact) surface MUST provide at least one readout that reports the real index health (no gap where no honest health tool exists on the default surface).
- **FR-008**: A durable subsystem that failed to initialize MUST be reported distinctly from one that is simply not wired.

**Edit safety (US3)**

- **FR-009**: A guarded apply MUST re-verify the guard condition against the bytes actually being written, in the same critical section as the write; on divergence it MUST reject the apply without writing.
- **FR-010**: A response MUST claim a successful guarded apply only when the guard was enforced at the write; best-effort backups MUST NOT be described as transactional rollback.

**Recovery & onboarding (US4)**

- **FR-011**: A fresh default attach MUST index a discoverable workspace automatically, or return a recovery message that names only actions callable on the active surface.
- **FR-012**: No agent-facing error or recovery string MUST name a capability that the active surface forbids.
- **FR-013**: The default launch path MUST discover the project workspace (not an unrelated default directory) so the index is populated, and the registered client environment MUST carry the configuration needed for a populated, correctly-surfaced start.

**Economics grounding (US5)**

- **FR-014**: Economics predictions MUST be derived from real request/result size so that predictions vary with the work, and the adaptive economics outcomes are reachable for appropriate requests — OR, until grounded, every economics figure MUST be labeled heuristic (FR-001) and the adaptive behavior described as not-yet-active.
- **FR-015**: Any equivalence/accuracy figure presented as measured MUST be backed by an actual assertion in the test corpus, or be removed.

**Public record & enforced honesty (US6)**

- **FR-016**: Public docs and client configuration MUST describe the real default surface, with the larger legacy surface presented as a documented opt-out.
- **FR-017**: A published capability record MUST map each capability to a proof state (implemented / heuristic / observational / deferred) tied to an assumption identifier, with one source of truth per identifier.
- **FR-018**: Automated verification MUST fail when a shipped surface claims a capability whose underlying assumption is OPEN.

**Cross-cutting**

- **FR-019**: The full verification gate (format, type/lint, tests, release build, and the network-free embed build) MUST pass after each delivered phase, not only at the end; the three named regression tests (status-matches-served-index, compact-error-never-names-blocked-tools, guarded-apply-rejects-concurrent-divergence) MUST exist.
- **FR-020**: No remediation MUST regress the protected foundations (compact dispatch enforcement, the shipped security batch, embed isolation, index-integrity model, the assumptions register as source of truth) or revert the compact-3 default surface to "fix" anything.

### Key Entities *(include if feature involves data)*

- **LLM-facing surface**: any string an agent reads on a call — status banner, economics envelope, error/recovery text, tool description — governed by the honesty contract.
- **Economics figure**: a predicted/served/saved token quantity, each carrying a proof state (measured vs heuristic) and an honest label.
- **Index health readout**: the reported readiness + counts of the index, which must reflect the index that actually serves.
- **Guard condition (if_match)**: the expected pre-edit body an apply is conditioned on; must be honored at the write.
- **Assumption record entry**: an identified claim (e.g. predictor accuracy, surface premise) with exactly one proof state (OPEN / PARTIAL / VALIDATED-with-artifact), referenced by surfaces and enforced by the honesty guard.
- **Capability matrix entry**: a feature mapped to its proof state and the assumption identifier backing it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of LLM-facing status/economics fields are either provably accurate or carry an explicit heuristic/observational/deferred label (zero fields present a constant or gross counter as a measured result).
- **SC-002**: After a successful query in the default deployment, the status index readiness and counts match the served index in 100% of runs (never "empty/not-ready" while serving).
- **SC-003**: A guarded apply rejects 100% of cases where the on-disk body diverged after the guard check, with zero silent clobbers and zero false "guarded apply succeeded" claims (proven by a deterministic concurrent-change test).
- **SC-004**: From a fresh default attach, the agent reaches a working query OR a recovery message that names only callable actions in 100% of cold-start runs; zero agent-facing strings name a surface-forbidden capability.
- **SC-005**: Economics predictions vary with real request size (two materially different inputs yield different predictions), or — if grounding is deferred — every economics figure is labeled heuristic; in either case zero figures are presented as measured savings without measurement.
- **SC-006**: Public docs, client configuration, and the capability record describe the real default surface and proof states with one source of truth per assumption; the automated honesty guard fails any shipped claim paired with an OPEN assumption.
- **SC-007**: The full verification gate (incl. the embed build) passes after each phase, and the three named regression tests exist and pass.
- **SC-008 (keystone)**: An LLM that trusts SymForge's self-reported numbers and status is no longer misled — no surface asserts more than the code delivers.

## Assumptions

- Discovery is complete and verified in `docs/reviews/v8-trust-remediation-ledger.md`; that ledger (TR-01..TR-20 + panel findings + anchors) is the authoritative finding set and is not re-litigated here.
- The v8 architecture is sound; this is a presentation-layer + two-real-bug remediation, **not** a rewrite. The compact-3 default surface stays; the engine (index integrity, auth, ledger storage, path guards) is protected, not changed.
- Delivery is phased and independently shippable: P1 = honest relabel (US1) + status truth (US2) + edit safety (US3); P2 = recovery/onboarding (US4) + economics grounding (US5) + public record & enforced honesty (US6). The honest relabel (US1) ships before any "token-efficient" public messaging.
- "Relabel ≠ validate": OPEN assumptions (notably the surface premise and the economics predictor) remain OPEN and are framed as a bet-under-test until reproduced with an artifact.
- Verification uses fixtures and the project's own test suite (no mutation of real operator configs); regression tests use deterministic injected interleave points, not timing sleeps.
- A real byte-grounded estimator already exists in the codebase (the economics grounding reuses it rather than building a new one), so grounding is a wiring effort, not new science.
- Out of scope and tracked separately: the deferred P3 hardening items (ledger migration guard, retention, dependency-pin drift); the separately-tracked update-command robustness and tool-parameter schema-rendering fixes; and the parked operator-setup-wizard (009), whose onboarding overlaps US4 but is its own feature.
