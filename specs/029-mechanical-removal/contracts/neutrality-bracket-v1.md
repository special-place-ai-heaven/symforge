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

## C-2 — The public atom set does not move

**Requires**: the derived public atom set after removal equals the frozen
**postactivation** set exactly (kept ∪ introduced).

**Refuses**: any removal that changes the set, in either direction.

**Emits on refusal**:
`RETIREMENT_LIFECYCLE_PHASE_INVALID: public API is neither frozen preactivation (N) nor postactivation (M); actual=K`

**Owner**: `scripts/validate-lifecycle-oracle-traceability.cjs`,
`ordinaryRetirementLifecycle`.

> This is not a rule Slice 5 invents; it is a gate that already exists and
> already fails closed. It is written here because its consequence is
> counter-intuitive for a slice named "mechanical removal": **no public atom is
> removable**, since every surviving one is required by the postactivation
> definition. The removal surface is non-public code only.

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

**Audit property**: file and byte counts must move **downward** by the amount
removed. A refresh whose counts rise, or stay flat across a non-empty removal,
is a defect and not a formality.

---

## C-6 — A predicted removal that does not happen leaves a record

**Requires**: a `DischargedExpectation` for each roster-predicted removal not
performed, carrying the observation that discharged it.

**Refuses**: silent omission; substituting an adjacent deletion to make the
slice look productive.

**Emits on refusal**: the predicted item and the phrase
`prediction neither performed nor discharged`.

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
