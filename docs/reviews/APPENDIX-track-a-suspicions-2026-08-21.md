# SEALED APPENDIX — Track A suspicions

> **Do not read this before your independent pass is written to disk.**
>
> This file exists because the requester has opinions, and opinions in a review
> brief become the reviewer's findings. Everything below is unverified belief.
> None of it has been independently checked. Some of it is probably wrong —
> that is the point of you having written Parts 1–4 first.
>
> If your Parts 1–4 are not yet on disk, close this file.

---

## S1 — Every owning slice has already shipped

Each control's `#[ignore]` names the slice expected to resolve it. All of those
slices have landed:

| Controls | Named owner | Owner status |
|---|---|---|
| `same_path_root_replacement_is_not_silently_adopted` | Slice 1 (T022–T029) | shipped |
| `capacity_refused_open_creates_no_slot_and_no_watcher`, `configured_capacity_bounds_the_process_not_each_load`, `concurrent_first_open_performs_exactly_one_cold_load` | Slice 2 (T030–T040) | shipped |
| the remaining seven | Slice 4 (spec 028) | shipped as 11.0.0 |

So no unshipped slice owns any of them. Suspicion: this is why nobody has
looked — each was somebody's future problem, and the future arrived without a
handoff.

## S2 — Four of them still carry prose predicting a slice that already landed

The four Slice-1/Slice-2 rows still read "remove this attribute in Slice 2
(T030–T040) when …". Those slices shipped months ago. The seven Slice-4 rows
were rewritten on 2026-08-20 to say "observed still red … after the Slice 4
activation cut", but the older four were not.

Suspicion: a stale prediction left standing in a test attribute is itself a
defect, independent of whether the control is right. It tells the next reader
the item is somebody else's job.

## S3 — Two of them may not be fixable by production code at all

`watcher_mutation_during_candidate_build_is_not_discarded` and
`whole_project_publication_preserves_latest_siblings` were rewritten to say
"observed still red (precondition window unreachable)". If the control cannot
reach the state it asserts on, then no amount of correct production code turns
it green.

Suspicion: these two are a different category from the other nine, and treating
them as ordinary failures is a category error. They may need a different seam,
an explicit adjudication, or deletion — but not a fix.

This is the specific pair a second opinion was reserved for.

## S4 — "Still red at the daemon seam" may be doing a lot of work

Five controls were rewritten to say they remain red "at the daemon/watcher
seams they drive". That phrasing was written by the same process that ran them
un-ignored and observed the failure — it records *that* they failed, not *why*.

Suspicion: nobody has distinguished "the behaviour is genuinely absent" from
"the control's setup no longer constructs the situation it was written for". A
control written against a V10 daemon may be assembling a world the V11 cut no
longer has.

## S5 — The roster's fail-closed rule may be load-bearing in an unintended way

`scripts/slice0-oracle-artifact.cjs` treats a RED control that starts passing
as an **error**. That is deliberate and defensible. But it also means: if the
V11 cut accidentally made one of these pass for the wrong reason, CI would go
red and the cheapest fix would look like reclassifying it to `RESOLVED_CASES`.

Suspicion: worth checking whether any control is currently passing-but-listed,
or whether the roster has drifted from the test file in either direction.

## S6 — The controls might be better evidence than the code

The unexamined assumption in the whole framing is that failing controls are
debt. The opposite reading is available: eleven controls that survived four
slices of pressure without being quietly deleted are the most honest artifact
in the repository, and the correct response is to fix the code.

Suspicion: I lean toward this reading and am aware that I lean toward it, which
is exactly why it is in a sealed appendix rather than in the brief.

## What I deliberately did not tell you in the brief

- Which controls I think are wrong.
- That I suspect S3's pair is unfixable.
- That I consider S2's stale prose a defect in its own right.
- Any ranking of the eleven by severity or difficulty.

If your independent pass reached different conclusions from these, your pass is
the more valuable document. Say so plainly in Part 5.
