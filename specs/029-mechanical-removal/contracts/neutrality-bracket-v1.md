# Contract — Neutrality Bracket v1

The interface Slice 5 exposes is not an API. It is an **evidence contract**:
what a removal must present before it is believed. This document is that
contract, stated so a reviewer can reject a non-conforming removal without
re-litigating the reasoning.

Derived from `specs/020-repository-knowledge-index/tasks.md` Phase 7
(T074–T077), which is frozen and is not edited by this slice.

---

## C-1 — A removal is authorized only by an armed bracket

**Requires**: a `NeutralityComparison` whose `control_result` is
`detected(<field>)`.

**Refuses**: any removal presented with a comparison that is `void`, absent, or
whose control was not run before the removal.

**Emits on refusal**: the name of the missing or void artifact.

> A neutrality bracket reports "nothing changed". So does a broken one. The
> control is the only thing that distinguishes them, and it must precede the
> removal it authorizes — a control run afterwards has already been influenced
> by the change it was meant to be independent of.

---

## C-2 — The public surface does not move, and THREE owners enforce that

**Requires**: all three of the following after every removal —

| # | Requirement | Owner |
|---|---|---|
| C-2a | The derived 3-segment lifecycle atom set still equals the frozen postactivation set (34 atoms on this tree) | `scripts/validate-lifecycle-oracle-traceability.cjs`, `ordinaryRetirementLifecycle` |
| C-2b | The full 64-atom `introduced_v11_atoms` set still resolves | `python execution/refreeze_v11.py verify-internal --target-ref HEAD` |
| C-2c | The consumer fixtures still compile as expected | `tests/fixtures/public-api-v11-consumer/` compile-fail + dependent-positive |

**Refuses**: any removal that changes any of the three, in either direction.

**Emits on refusal (C-2a)**:
`RETIREMENT_LIFECYCLE_PHASE_INVALID: public API is neither frozen preactivation (N) nor postactivation (M); actual=K`

> **Amended 2026-08-21** after independent review. The original clause named
> C-2a alone and called it the definition of public-behaviour neutrality. It is
> not. `directPublicAtoms` filters to `split("::").length <= 3`, and 34 of the
> 64 introduced atoms are 4-segment associated methods — all under `embed`,
> which is also excluded from the regex scan. **Deleting
> `symforge::embed::Claim::value` would have left C-2a green** while real
> public API shrank. C-2b is the owner that catches it, and it runs in about
> six seconds, so it belongs on *every* removal landing rather than only the
> T076 embed path.
>
> The practical rule is unchanged and still counter-intuitive for a slice named
> "mechanical removal": **no public atom is removable.** What changed is which
> gate you must run to know that.

---

## C-3 — Frozen V11 production seams are not removable

**Requires**: every frozen V11 production seam retains a same-tree source
receipt.

**Refuses**: removal of any such seam.

**Emits on refusal**: the seam's identifier and the receipt that would be
orphaned.

> Retired V10 members are safe to delete because their anchors resolve on the
> approved refreeze ancestor, which the current tree cannot change. V11 seams
> are the mirror image: their receipts must bind *this* tree, so deleting one
> destroys the evidence rather than merely the code.

---

## C-4 — Unreachability is cited, never argued

**Requires**: each removed item cites an executed Slice 4 reachability case or
a retirement-inventory disposition.

**Refuses**: "unused", "legacy-looking name", "no callers found by search",
"the task list says to remove it".

**Emits on refusal**: the candidate identifier and the phrase
`no admissible unreachability evidence`.

> The last of those refusals is the subtle one. The frozen roster naming an
> item is what puts it on the candidate list; it is not what proves the item is
> dead. Slice 5's own goal text illustrates why: it says to remove "legacy mode
> branches", and the machine's `LegacyOpen`/`LegacyClosing` states match that
> description while being the live bootstrap path on every process start.

---

## C-5 — Seals are refreshed by their oracle, after formatting

**Requires**: `cargo fmt --check` green, then the owning Rust oracle's observed
actuals transcribed into the pins.

**Refuses**: any hand-computed digest; any pin refresh preceding `fmt`.

**Emits on refusal**: the pin name and both values.

**Audit property, per pin** — evaluated against *that pin's own file set*:

| Situation | Required outcome |
|---|---|
| Deletion intersects the pin's file set | Byte count moves **down**. File count moves down **only if a whole file was deleted**; flat is correct for an in-file deletion. |
| Deletion does **not** intersect the pin's file set | The pin is **byte-identical**. Refreshing it at all is the defect. |
| Any pin | Counts must never **rise**. |

> **Amended 2026-08-21** after independent review. The original required both
> pins' counts to move downward and called a flat count across a non-empty
> removal a defect. That is false twice over. `EXCLUDED_RUNTIME_SOURCE_PIN_V1`
> covers only 19 `index_lifecycle/*.rs` plus `server_api.rs` — T076's target
> `src/embed.rs` is outside it — and deleting *within* a file never changes a
> file count. The original clause would have refused the correct refresh for
> the one removal the roster explicitly predicts, and an executor obeying it
> would have "fixed" a pin that legitimately never moved. A seal corrupted to
> satisfy a contract clause is worse than the drift the seal exists to catch.

---

## C-6 — A predicted removal that does not happen leaves a record

**Requires**: a `DischargedExpectation` for each roster-predicted removal not
performed, carrying **the discharging command and its output verbatim** — not a
prose summary of what the command showed.

**Refuses**: silent omission; substituting an adjacent deletion to make the
slice look productive; a discharge whose evidence is an assertion rather than a
transcript.

**Emits on refusal**: the predicted item and the phrase
`prediction neither performed nor discharged`.

> **Amended 2026-08-21** after independent review flagged that nothing
> machine-checks this document, so "already gone" with no command would still
> parse as a filled-in record. Requiring the command and its verbatim output
> is the cheapest thing with teeth: a reviewer can re-run the transcript. No
> new gate is added — T077's review remains the enforcer, and a slice that
> needed a new gate to police its own honesty would have a worse problem than
> this clause solves.

---

## C-7 — The slice may remove nothing

**Requires**: nothing. An empty removal is a conforming outcome provided C-1
produced an armed bracket, C-6 discharged every prediction, and the evidence
document states plainly that no code was removed.

> Recorded as a contract clause rather than a footnote because the pressure
> runs the other way. A slice that finds nothing to delete feels like a failed
> slice, and the cheapest way to make it feel successful is to delete something
> that was not evidenced. C-7 exists to make "nothing was removable" a passing
> result with a name, so nobody has to invent a removal to close it.
