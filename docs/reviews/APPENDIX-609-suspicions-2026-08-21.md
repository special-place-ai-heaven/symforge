# SEALED APPENDIX — #609 suspicions

> **Do not read this before your Parts 1–4 are written.**
>
> This appendix is deliberately thin: **I have not read #609's substance.** I
> looked at its metadata, its diffstat, and which files it touches, then
> stopped, so that this brief could be honest about being uninformed rather
> than pretending to be.
>
> Thin is not the same as harmless. Four guesses below, all unverified.

---

## S1 — The title points at a control-stale disposition

"retire failed daemon admissions" and "cover failed daemon admission cleanup"
read as though they target
`capacity_refused_open_creates_no_slot_and_no_watcher` — a control both your
seats classified **CONTROL-STALE**, meaning the body asserts a retired encoding
(`Err` + zero slots) that V11 deliberately replaced (`Ok` + typed
`SourceRefusal` + non-ready slot).

Suspicion: if #609 changed production to satisfy the old encoding, it moved the
code *away* from the frozen oracle while looking like a fix. That is precisely
the failure mode the control-stale disposition exists to prevent, and the
reason "retarget the body, do not switch production" was written down.

I could be entirely wrong about what it targets. The title is all I have.

## S2 — It refreshed the pins, which means it moved seals

The third commit is `test: refresh V11 source fingerprints`, and
`tests/preventive_runtime_dark_v11.rs` shows 4 added / 4 removed — consistent
with two `(digest, files, bytes)` tuples being rewritten.

Suspicion worth checking: was the refresh taken from the owning Rust oracle's
observed actuals, or computed some other way? Constitution Principle V makes
the oracle the only recompute authority, and the rule exists because a
hand-rolled recompute silently diverged once already. A pin that is *plausible*
but not oracle-derived is worse than one that is obviously wrong.

Also worth checking: whether both pins moved, and whether they should have.
`EXCLUDED_RUNTIME_SOURCE_PIN_V1` covers only the 19 `index_lifecycle/*.rs`
files plus `server_api.rs`. `daemon.rs` is outside it. A refresh that moved
that pin for a `daemon.rs`-only change would be wrong in the same direction
that a contract clause of mine was wrong last week.

## S3 — Same actor as #603

#603 came from `app/cursor`, went red, was never adjudicated on substance, and
was closed as leftover. #609 is the same actor working the same territory.

Suspicion: it may be the same class of artifact — work generated against a
picture of the tree that no longer holds. Or it may be genuinely good and
merely unowned. I do not know, and the previous PR's fate is not evidence about
this one's quality.

## S4 — `registry.rs` got the most lines, and no seam names it

`registry.rs` takes +93, more than `activation.rs` at +51, yet no mapped seam
lives there. `adapters.rs` likewise.

Suspicion: either #609 is doing something broader than the two seams it
overlaps, or the six-seam map missed a seam that `registry.rs` owns. **The
second possibility is the one I would most like tested**, because it would mean
your map is incomplete rather than that this PR is off-scope — and I would
rather learn that from you than defend the map.

## What I deliberately did not do

- Read the diff.
- Form a view on whether it should be closed.
- Look at its CI state.

If your Part 1 shows I guessed the target wrong, say so plainly. Being wrong
about an unread PR is the expected outcome, not an embarrassment.
