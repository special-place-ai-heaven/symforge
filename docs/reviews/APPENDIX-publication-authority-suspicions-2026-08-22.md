# SEALED APPENDIX — publication-authority invariant suspicions

> **Do not read this before your Parts 1–5 are written.**
>
> This question was routed to you specifically *because* you had not seen the
> tension. Another reader raised it and therefore could not be the first lens.
> Everything below is unverified belief.
>
> If your Parts 1–5 are not on disk, close this file.

---

## S1 — Where the tension came from

The chain that produced this brief, so you can judge whether it was reasoned or
merely alarming:

1. HOLMES, reading seam 3, wrote: *"The machine is already there; the data
   plane is not"*, citing `activation.rs:337-339` — the `ObservationLane` doc
   saying the LiveIndex data plane keeps serving admissions mid-cut while the
   lane runs dark beside it.
2. That comment says the arrangement lasts *"until C4/C5 make it the root."*
3. C4 and C5 are commit groups in spec 028. Both executed on 2026-08-19. Per
   the execution map, C4 was ownership structure and C5 was **"THE EXPOSURE
   FLIP"** — the public census, the `internals` remount, the embed surface,
   `server_api` going `pub`. Neither made the lane the publication root.
4. So the comment names two owners that ran and did something else.
5. Then the companion invariant at `:19-24` says the V11 publication root
   "serves only in `PreventiveV1Open`" — and the process is in
   `PreventiveV1Open`.

Suspicion: 1–5 cannot all be true under the plain reading of "serves".

## S2 — The most likely resolution, and I hold it loosely

"Serves" probably means **write/publication authority** — which root may
*publish* state — rather than which structure answers a read. Under that
reading the invariant is about not having two things publishing at once, the
V10 data plane is a cache/serving layer rather than a publication root, and
nothing is violated.

If that is right, the invariant is **sound and its wording is misleading**,
which is a documentation defect worth fixing precisely because it cost this
much attention to resolve.

## S3 — The reading I am afraid of

That "the V11 publication root serves only in `PreventiveV1Open`" was written
as a statement of what *would be true after the cut*, and the cut delivered the
mode without delivering the serving. That would make it an unearned guarantee
in a shipped release: true of the design, false of the artifact.

I do not think this is the likely answer. I think it is the one worth ruling
out explicitly rather than by assumption, because this repository's stated
product is being trustworthy about what it knows, and an invariant that
overclaims is that failure in its purest form.

## S4 — What the counts made me think, and why I distrust it

226 `data_plane()` sites versus zero production callers of a type documented as
*"The SOLE publication root"* looks damning. But "sole" may be a statement of
intended design in a type that is legitimately ahead of its wiring — this
codebase lands dark machinery on purpose and has a whole discipline around it.
A count is not a verdict, and I put that sentence in the brief because I needed
to hear myself say it.

## S5 — The pattern I think this belongs to

Three artifacts now name a future owner that already came and went without
discharging the obligation:

- `#[ignore]` strings predicting "remove in Slice 1/2" after those slices shipped
- "precondition window unreachable", which was false on the tree
- `until C4/C5 make it the root`, after C4/C5 ran

Suspicion: this is one defect class — **prose that predicts, aging into prose
that asserts** — and it is worth naming as such rather than fixing three times.
If your pass finds a fourth, that is more valuable than the answer to the main
question.

## S6 — What I most expect to be wrong

That I have framed this as binary. "Holds / does not hold" may be the wrong
shape: the honest answer may be that the sentence mixes a type-level guarantee
with a runtime claim and only one half is enforced. If so, say that rather than
picking a side to satisfy the brief.

## What I kept out of the brief

- Every reading above.
- That I consider S2 most likely and S3 the one to rule out.
- The C4/C5 chain, except as a bare citation of `:337-339`.
- Any suggestion that this is or is not a release defect.

If your independent reading differs from all of the above, yours is the
document that matters.
