# Feature Specification: Feature 020 Slice 5 — Mechanical Removal

**Feature Branch**: `029-mechanical-removal`

**Created**: 2026-08-21

**Status**: Draft

**Input**: Slice 5 of Feature 020 (repository-knowledge-index): mechanical removal. Delete ONLY code already proven unreachable by the Slice 4 activation cut, per the frozen roster T074–T077 (`specs/020-repository-knowledge-index/tasks.md` Phase 7, lines 958–966).

## Governing constraint (frozen, verbatim)

> "Delete only code already proven unreachable in Slice 4; do not change runtime
> authority, public behavior, writer reachability, or activation mode."
> — `specs/020-repository-knowledge-index/tasks.md:960-961`

The frozen spec tree `specs/020-repository-knowledge-index/` is immutable input,
**including its checkbox bytes**. This spec derives from it and must not
contradict it. Where this spec and the frozen tree disagree, the frozen tree wins
and this spec is amended.

This slice is unusual and its unusualness is the point: **it is the only slice
whose success condition is that nothing observable changes.** Every other slice
is judged by what it makes true. This one is judged by what it leaves identical
while the tree gets smaller.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A removal can be proven behaviour-neutral (Priority: P1)

A maintainer removes something from `src/` and needs to establish, from evidence
rather than assertion, that runtime authority, public behaviour, writer
reachability, and activation mode are unchanged. Today no bracketing artifact
exists: the only way to argue neutrality is to reason about the diff.

**Why this priority**: Every subsequent removal depends on it, and it carries
standalone value — once the bracket exists, *any* future removal in this
codebase can be held to the same standard, whether or not Slice 5 removes a
single line. It is also the only part of the slice that cannot be skipped if the
recon finds nothing removable.

**Independent Test**: Capture the baseline, introduce a deliberate control
change, re-capture, and confirm the comparison **names the field that moved**.
Then discard the control. The story is closed by the bracket detecting a real
change — not by it staying quiet across an empty diff, which a permanently
broken comparison would also do.

> **Amended 2026-08-21** after independent review. The original Independent
> Test was the null re-run alone. Spec-kit Independent Tests are what an
> executor uses to close a story on its own, so that wording let US1 close
> without ever exercising the only thing that makes a bracket a bracket — the
> exact vacuous-negative shape Constitution Principle II forbids, in the same
> document that invokes Principle II three sections later. The null re-run
> remains valuable and is kept as Acceptance Scenario 2; it is no longer the
> standalone proof.

**Acceptance Scenarios**:

1. **Given** a clean tree at the current release ref, **When** the baseline is
   captured, **Then** it records the public API atom set, the activation-mode
   result, the writer-reachability verdict, and the behavioural gate outcomes,
   each with the exact command that produced it.
2. **Given** a captured baseline, **When** it is re-captured with no
   intervening source change, **Then** every recorded field is byte-identical
   and the comparison reports zero differences.
3. **Given** a captured baseline, **When** a deliberate behaviour-changing edit
   is introduced as a control, **Then** the comparison reports a difference and
   names the field that moved. A bracket that stays quiet through a real change
   is providing false assurance and fails this scenario.

---

### User Story 2 - Retired V10 authority is gone from the tree (Priority: P2)

A reader of `src/` currently cannot distinguish V10 authority that the cut
retired from V11 machinery that is live. Both compile; both are named; only one
runs. The retirement inventory records the disposition, but the source does not.

**Why this priority**: This is the slice's headline product, but it is worthless
and dangerous without US1's bracket — an unbracketed removal is exactly the
"confidently wrong" failure this project treats as unrecoverable.

**Independent Test**: For each candidate, produce the evidence that it is
unreachable *before* deleting it, delete it, and show the US1 bracket reports no
difference. A candidate whose unreachability cannot be evidenced is not removed
and is recorded as retained with the reason.

**Acceptance Scenarios**:

1. **Given** a removal candidate, **When** its unreachability is asserted,
   **Then** the assertion cites the executed Slice 4 reachability case or
   retirement-inventory disposition that establishes it — never task wording,
   never a name that merely looks legacy.
2. **Given** a candidate that is a frozen V11 production seam, **When** removal
   is considered, **Then** it is refused, because those seams require a
   same-tree source receipt that deletion would destroy.
3. **Given** a candidate whose only consumers are tests, **When** the candidate
   is removed, **Then** those tests are removed in the same change, and no test
   with a surviving subject is removed.
4. **Given** any removal, **When** the whole-source seals are re-derived,
   **Then** the recorded file count and byte total move in the direction of the
   removal and the new digests are taken from the tool's own actuals, never
   hand-computed.

---

### User Story 3 - The dead V10 embed implementation is gone, or proven already gone (Priority: P3)

The roster expects a dead V10 embed implementation to remain in `src/embed.rs`.
Recon suggests the activation cut already retired most of it. Either outcome is
acceptable; asserting the wrong one is not.

**Why this priority**: Smallest and most contained, and gated on a proof that
must exist first.

**Independent Test**: Run the allowlist negative suite and read its verdict. If
it proves the V10 embed surface unnameable and dead code remains, remove it. If
no dead code remains, record that finding with its evidence and remove nothing.

**Acceptance Scenarios**:

1. **Given** the allowlist negative suite has not been run, **When** removal
   from the embed surface is attempted, **Then** it is refused — the frozen
   ordering is "only after ... proves it unnameable".
2. **Given** the suite proves the surface unnameable, **When** the
   implementation is inspected, **Then** either dead code is found and removed,
   or its absence is recorded as a discharged expectation with the evidence
   that discharged it.

---

### Edge Cases

- **The slice removes nothing.** If no candidate survives its evidence
  requirement, that is a legitimate closure, not a failure. The slice still
  produces the bracket, the baseline, the re-run, and an evidence document
  stating that the expected dead code did not exist. What must never happen is
  removing something to make the slice feel productive.
- **A removal moves a seal that a frozen document pins.** Whole-source seals
  live in the test tree and are refreshed from tool actuals. Digests inside the
  frozen spec tree are never edited; if a removal would require editing one, the
  removal is refused and the conflict recorded.
- **A candidate is unreachable in the default build but reachable in another
  feature configuration.** Reachability is a property of a configuration set,
  not of one build. A candidate must be unreachable in every configuration the
  project builds, or it stays.
- **A removal is neutral in isolation but not in combination.** The bracket is
  re-run over the accumulated state, not per-item in isolation.
- **The baseline and the re-run disagree for an unrelated reason** (a flaky
  gate, an environment difference). The disagreement is investigated to root
  cause before it is attributed to the removal, and the cause is recorded. A
  difference explained away without a cause is a failed slice.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The slice MUST capture a pre-cleanup baseline covering public API,
  authority reachability, behaviour, and activation result, each field paired
  with the exact command that produced it.
- **FR-002**: The slice MUST re-capture that baseline after removal and compare
  the two records field by field, reporting either zero differences or the exact
  fields that moved.
- **FR-003**: The comparison MUST be demonstrated to detect a real change before
  it is trusted to certify the absence of one.
- **FR-004**: Every removal MUST cite pre-existing evidence of unreachability.
  Task wording, a legacy-sounding name, and "it looks unused" are not evidence.
- **FR-005**: The slice MUST NOT remove any frozen V11 production seam.
- **FR-006**: The slice MUST NOT change runtime authority, public behaviour,
  writer reachability, or activation mode.
- **FR-007**: The tree is observed `postactivation`. After removal the derived
  3-segment lifecycle atom set MUST still equal the frozen postactivation set
  exactly. Additionally the full 64-atom introduced set MUST still resolve, and
  the consumer fixtures MUST still compile as expected — the lifecycle checker
  alone cannot see the 34 four-segment atoms, so its green is not sufficient.
- **FR-008**: The slice MUST NOT edit any byte of the frozen spec tree,
  including checkbox bytes.
- **FR-009**: Tests MUST be removed only when their subject was removed in the
  same change.
- **FR-010**: Whole-source seals affected by removal MUST be refreshed from the
  producing tool's own observed output, never hand-computed.
- **FR-011**: An expectation that recon discharges (dead code the roster
  predicts, which does not exist) MUST be recorded as discharged with its
  evidence, not silently dropped and not satisfied by removing something else.
- **FR-012**: The slice MUST end with an independent adversarial review whose
  findings are fixed or explicitly adjudicated with rationale, never silently
  dropped.
- **FR-013**: Every claim in the slice's evidence document MUST state what was
  observed and what the observation would have emitted had it failed.

### Key Entities

- **Removal candidate**: a named item in `src/`, its evidence of
  unreachability, its disposition (removed / retained), and, when retained, the
  reason.
- **Neutrality baseline**: the recorded pre-removal state — public API atom
  set, activation result, writer-reachability verdict, behavioural gate
  outcomes — with the command that produced each field.
- **Neutrality comparison**: the field-by-field diff of two baselines, plus the
  control result proving the comparison can detect a real change.
- **Discharged expectation**: a roster-predicted removal that recon proves
  unnecessary, with the evidence that proved it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The baseline re-run over an unchanged tree reports **zero**
  differing fields.
- **SC-002**: The control run over a deliberately changed tree reports **at
  least one** differing field and names it.
- **SC-003**: **100%** of removals cite pre-existing unreachability evidence;
  removals citing none: **zero**.
- **SC-004**: The post-removal 3-segment lifecycle atom set matches the frozen
  postactivation set exactly (atoms outside it: **zero**), **and** the refreeze
  allowlist resolves all 64 introduced atoms, **and** the consumer fixtures
  compile as expected. All three, not any one.
- **SC-005**: Bytes changed inside the frozen spec tree: **zero**.
- **SC-006**: Frozen V11 production seams removed: **zero**.
- **SC-007**: Tests removed whose subject still exists: **zero**.
- **SC-008**: Every verification gate that was green before the slice is green
  after it; newly failing gates: **zero**.
- **SC-009**: Every roster-predicted removal is either performed with evidence
  or recorded as discharged with evidence; silently dropped predictions:
  **zero**.
- **SC-010**: Adversarial review findings left neither fixed nor explicitly
  adjudicated: **zero**.

## Assumptions

- **An empty removal is a valid outcome.** The slice's obligation is honest
  disposition of every predicted removal, not a minimum line count. Slice 4's
  retirement work may already have discharged most of Phase 7's scope; the
  recon that suggested this is treated as a hypothesis the slice must confirm,
  not as a conclusion.
- **"Legacy mode branches" does not mean the activation machine's own bootstrap
  states.** `LegacyOpen` and `LegacyClosing` are the live entry states of the
  machine the cut installed — the process boots into the first, drains through
  the second, and only then opens the preventive mode. They are reachable on
  every start. The phrase refers to the V10 authority branches that machine
  replaced. This assumption is recorded because acting on the other reading
  would delete the startup path while looking like faithful task execution.
- **Deleting retired V10 code cannot break the retirement census**, because
  preactivation members resolve against the externally approved refreeze
  ancestor tree rather than the current tree. Frozen V11 production seams are
  the opposite case and are protected by FR-005.
- **Reachability evidence already exists** as the executed Slice 4 reachability
  cases and the retirement inventory dispositions. The slice consumes that
  evidence; it does not re-derive it, and it does not invent new evidence for a
  candidate that lacks it.
- **The verification gate set is the project's existing one.** This slice adds
  no gate and relaxes none.
- **Removal is staged, not atomic.** Unlike Slice 4, no part of this slice is
  indivisible; candidates may land in separate changes provided each carries its
  own bracket result.
