# Feature Specification: Repeat-Call Notice and Ledger Retry Collapse

**Feature Branch**: `feat/032-repeat-call-breaker`

**Created**: 2026-09-01

**Status**: Draft

**Input**: User description: "Ledger-driven repeat-call circuit breaker and retry collapse: (1) detect identical (tool, argument-hash) calls repeated within a session against an unchanged index and append a machine-readable envelope hint telling the client the result cannot differ, grounded in the session ledger; (2) collapse runs of identical consecutive ledger events into one canonical row annotated with an iteration count when rendering ledger views (status, admin ledger_view)"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Repeat-Call Notice (Priority: P1)

An automated client (a coding agent driving SymForge over MCP) gets an unexpected result from a read tool — an empty result, a not-found, an error it does not understand — and retries the exact same call, unchanged, again and again. Nothing about the indexed project has changed between the attempts, so every retry burns the client's tokens and time to produce a byte-identical answer. SymForge, which sees every call and already records each one in its session ledger, notices the repetition and appends a clearly separated notice to the repeated call's response: this exact request has now been served N times against an index that has not changed in between, so the result cannot differ; stop retrying and change the request instead.

**Why this priority**: This is the half of the feature with measured demand. The A019 tool-surface shakedown recorded 17 repeated call errors followed by hundreds of fallback attempts from real agent sessions — degenerate retry loops are an observed failure mode of SymForge's primary user (an LLM agent), and every un-noticed retry is wasted spend for the operator. It also directly extends SymForge's core product: being trustworthy about what it knows — including knowing that it has already answered this exact question.

**Independent Test**: Can be fully tested by issuing the same read call three times in one session against an untouched project and observing that the third response carries the notice while the first two do not, and that the primary result content is unchanged by the notice's presence.

**Acceptance Scenarios**:

1. **Given** a session with a current index, **When** a client issues the same read call (same tool, same arguments, same target project) for the third consecutive-in-kind time with no index change in between, **Then** the third response carries a machine-readable repeat notice naming the repeat count, while the result content itself is identical to the earlier responses.
2. **Given** a client has repeated a call twice, **When** the indexed content changes (a file is re-parsed, re-indexed, or edited) before the third identical call, **Then** the third response carries no repeat notice and the repeat tracking for that request starts over.
3. **Given** a repeated read call, **When** SymForge cannot positively observe that the index is unchanged since the prior identical call, **Then** no "result cannot differ" claim is made — the notice is withheld entirely rather than emitted on assumption.
4. **Given** a client issues the same *mutating* operation twice, **When** the second call arrives, **Then** no repeat notice is emitted by this feature (mutating operations legitimately change state between calls and are already governed by the existing idempotency machinery).
5. **Given** two different sessions, **When** each issues the same call once, **Then** neither receives a notice — repeat tracking never crosses a session boundary.

---

### User Story 2 - Ledger Retry Collapse (Priority: P2)

An operator (or an agent) inspects a session's ledger — via the status surface or the admin ledger view — after a session in which a client looped on the same request many times. Instead of scrolling past dozens of visually identical rows, they see one canonical row annotated with an iteration count and the time span it covers. Nothing is lost: totals still add up, the first and last occurrence times are visible, and the underlying stored events are untouched.

**Why this priority**: P2 because it is a legibility improvement to an inspection surface, not a behavior change on the serve path. It complements User Story 1 (both make repetition visible), but the two patterns are NOT the same: the notice fires on exact request identity, while ledger events record no request identity at all, so a collapsed run means "N ledger-identical events", not necessarily "N identical requests". Rendered views must never describe a collapsed run as an "identical request" repeated — that phrase is reserved for the User Story 1 notice.

**Independent Test**: Can be fully tested by seeding a ledger with a run of identical consecutive events plus distinct neighbors, rendering each ledger view, and confirming the run appears as one annotated row whose count and time span match the seeded events, while the distinct neighbors render unchanged.

**Acceptance Scenarios**:

1. **Given** a ledger containing N consecutive events identical in every identity field (differing only in timestamps and per-call measurements), **When** a ledger view renders them, **Then** the run renders as one canonical row annotated with the iteration count N and the first/last timestamps of the run.
2. **Given** a ledger with an interleaved sequence (A, A, B, A), **When** rendered, **Then** only the consecutive run collapses (A ×2, B, A) — non-adjacent repeats are never merged.
3. **Given** any collapsed rendering, **When** aggregate figures (event totals, token sums) are compared against the uncollapsed ledger, **Then** they are equal — collapse is presentation-only and lossless.
4. **Given** a collapsed rendering, **When** the underlying event store is inspected, **Then** every original event is still individually present — collapse never rewrites stored history.

---

### Edge Cases

- A call repeats with arguments that are semantically equal but textually different (reordered fields, defaulted vs explicit values): the fingerprint may treat them as different requests. That is acceptable — a false negative costs one redundant answer; a false positive would be a wrong "cannot differ" claim, which is never acceptable.
- The index changes *while* a call is being served: the currency observation must be taken so that a concurrent change can only suppress the notice, never let a stale one through.
- A read call whose answer can differ even on an unchanged index (anything time-, ordering-, or environment-dependent): such calls must be excluded from the notice's scope; the notice is only ever attached where an unchanged index genuinely determines the answer.
- The very first call of a session errors and the client retries: the second call is a repeat of count 2 and below the notice threshold; the notice must not fire on the first retry (retrying once after a transient error is legitimate client behavior).
- A ledger view is consumed programmatically (tests or dashboards pinning exact row counts): collapse changes rendered row counts, so every existing consumer of the rendered views must be reconciled as part of delivery.
- An extremely long run (thousands of identical events): the collapsed row's count must not overflow or truncate silently.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST derive a per-call identity fingerprint sufficient to recognize an exact repeat: the tool invoked, the complete request arguments, and the target project. Two calls MUST only be considered identical when all three match exactly.
- **FR-002**: The system MUST track, within a single session, how many times each fingerprint has been served consecutively-in-kind (i.e., without an intervening index change), and MUST reset that count for all fingerprints when the index changes and when the session ends. Calls to *other* fingerprints between serves of a run do NOT reset it — only an observed index change or session end does. (This is deliberately different from User Story 2's collapse, which merges strictly adjacent events only.) Where the serving process cannot positively observe a session identity for a request (a shared transport with no per-session discriminator), repeat tracking MUST NOT accumulate at all on that lane — an unattributable count is an unobserved claim.
- **FR-003**: When an identical read call is served for the third time (threshold: 3, fixed in this version) with the index positively observed unchanged across all three, the system MUST append a machine-readable repeat notice to the response stating the repeat count and that the result cannot differ until the index changes.
- **FR-004**: The repeat notice MUST NOT alter, reorder, truncate, or replace any of the response's primary result content; a response with the notice MUST be identical to the same response without it, apart from the appended notice itself.
- **FR-005**: The "result cannot differ" claim MUST be grounded in an actual observation of index currency (Constitution Principle I). If the system cannot observe that the index is unchanged since the prior identical call, it MUST withhold the notice entirely — it never emits the claim on assumption, and never emits a weaker unverified variant.
- **FR-006**: Repeat detection MUST apply only to read-lane operations whose result is fully determined by the index state. Mutating operations, and any read operation whose answer can vary on an unchanged index, MUST be excluded.
- **FR-007**: Ledger views (the status surface's ledger rendering and the admin ledger view) MUST collapse each run of consecutive events that are identical in every identity field — timestamps and per-call measurements excluded from the comparison — into one canonical row annotated with the run's iteration count and its first and last timestamps.
- **FR-008**: Collapse MUST be lossless at the aggregate level: any total the view reports (event counts, token sums) MUST equal the total computed over the uncollapsed events.
- **FR-009**: Collapse MUST be presentation-only: the stored events — in-memory and durable — MUST NOT be merged, rewritten, or deleted by rendering.
- **FR-010**: Both behaviors MUST be observable by tests without instrumenting internals: the notice is part of the response a client receives; the collapsed row is part of the rendered view an operator receives.

### Key Entities

- **Call fingerprint**: The identity of one request — tool name, complete argument content, target project. Exists only to answer "is this exactly the same request?"; never stored durably.
- **Repeat run**: A per-session record of consecutive identical serves of one fingerprint: the count, and the index-currency marker observed when the run started. Dies with the session or with any index change.
- **Collapsed ledger row**: A rendering of a run of identical stored events — the canonical event's fields, an iteration count, and the run's time span. Exists only in rendered output.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a session against an unchanged project, the third identical read call's response carries the repeat notice and the first two do not — demonstrable in the acceptance suite and reproducible by hand.
- **SC-002**: Zero false claims: across the full acceptance suite, no repeat notice ever appears on a call following an index change, on a mutating operation, on an excluded read operation, or when currency could not be observed. A single counterexample fails the feature.
- **SC-003**: A seeded run of N identical consecutive events renders as exactly one row annotated ×N, per view within that view's honest reach: the status (in-memory) view for N from 2 up to at least 10,000; the admin view for N up to its fetch window, with a run touching the window edge explicitly marked as clipped in the payload — a view never prints ×N for an N it did not count, and never truncates silently. Aggregate totals always equal the uncollapsed ledger's.
- **SC-004**: Byte-stability of results: for a repeated call, the response with the notice differs from the prior response only by the appended notice — verified by direct comparison in the acceptance suite.
- **SC-005**: Every pre-existing test that consumes ledger views or response envelopes still passes (updated deliberately where rendering changed, never loosened).

## Assumptions

- The repeat threshold is fixed at 3 for this version — the first retry after a failure is legitimate; the second retry of an identical request against an unchanged index is the degenerate-loop signal. No configuration knob until real use shows the default wrong.
- Repeat tracking is session-scoped and in-memory only; nothing about it is persisted. Cross-session repeat detection is out of scope.
- The existing response envelope / machine-readable outcome metadata surface is the delivery channel for the notice; no new protocol surface is introduced.
- Repeat tracking is computed on the serve path as calls are handled, not reconstructed from stored ledger events. (Verified 2026-09-01: the session ledger records only compact-facade calls today — granular read tools never append events — so the ledger cannot be the detection mechanism; it remains the motivating telemetry and the subject of User Story 2.)
- The A019 shakedown finding (17 repeated call errors, 232 native fallbacks on the compact facade) stands as the motivating evidence; this feature addresses the retry symptom, not the compact-facade decode failures themselves (those are a separate concern).
- "Index change" includes any observed re-parse, re-index, or applied edit for the target project within the session's view; changes to *other* projects in the working set do not reset runs for calls targeting this project.
- Ledger views today render only counts, aggregates, and the most recent event — no view renders a list of rows (verified 2026-09-01). User Story 2 therefore applies the collapse rule to the row-run renderings this feature itself delivers: the trailing-run annotation on the status surface's last-event lines, and a collapsed recent-events section added to the admin ledger view. Collapse never merges events from different recorded sessions. The durable lane's collapse identity is necessarily coarser than the in-memory event's (it covers the fields the durable row read-back exposes); that coarseness is documented per field in the design, never silent.
- Requests that fan out across an explicit project set (a `projects` argument) never accumulate repeat runs: on that lane the serving process structurally withholds per-project evidence, and per FR-005 an unobservable currency means no claim. This is a documented dead zone (false negatives only), pinned by a non-emission test, not an oversight.
- Repeat tracking requires an observable session identity. On a single-client transport the process is the session; on a shared transport, requests must carry a per-session discriminator the tracker can observe — otherwise that lane never accumulates (Acceptance Scenario 5 is a MUST and is enforced by construction, not by hoping clients don't share a process).
