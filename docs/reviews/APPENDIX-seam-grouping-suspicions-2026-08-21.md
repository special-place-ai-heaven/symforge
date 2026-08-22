# SEALED APPENDIX — seam grouping suspicions

> **Do not read this before your Parts 1–4 are written.**
>
> Everything below is unverified belief held by the main session. None of it
> has been checked against the code. Some of it is probably wrong. Its only
> value to this exercise is that you formed your grouping without it.
>
> If your Parts 1–4 are not yet written, close this file.

---

## A contamination disclosure, first

Before this brief existed, the main session said in chat to Grove: *"they are
probably two or three seams."* Grove firewalled it and kept the number out of
the dispatch.

**If that number reached you before your pass, say so in Part 5.** It does not
make your work useless, but it means the grouping was not formed independently
and I should weigh it accordingly. I would rather know than have a clean-looking
result I cannot trust. This disclosure is here rather than in the brief for the
obvious reason.

---

## S1 — Controls 3 and 4 look like one mechanism

`observer_replacement_gap_is_latched_as_non_current` and
`old_observer_delivery_after_promotion_is_not_current` both point at the same
rederive path — the recorded misses are "drops the historical gap → Current"
and "same rederive; no token fence". "Same rederive" is doing the work in that
sentence.

Suspicion: one seam, and the missing thing is an identity that survives
promotion rather than being recomputed from current state.

## S2 — Control 2 may belong with them, or may not

`failed_reload_retains_the_recovery_observer` is also observer-shaped, but its
miss is an error path that returns via `?` without installing a replacement.
That reads as a different defect — a missing cleanup on failure rather than a
missing fence — even though it lives in the same neighbourhood.

Suspicion: adjacency by subject is not adjacency by seam, and this is the one I
would most expect to be mis-grouped by someone pattern-matching on the word
"observer".

## S3 — Controls 1, 5 and 8 may share a "published before it was allowed to be"
shape

- 1: a placeholder that should hold nothing accepts `add_file`
- 5: a snapshot seed is queryable before verification
- 8: a mutation during candidate build reaches `swap_and_publish`

Suspicion: all three are the absence of a gate between "state exists" and
"state is servable". If that is one seam, it is the largest of them.

Counter-suspicion I hold at the same time: 5 is persistence and 8 is
publication, and calling them one thing may be a category error that looks
elegant.

## S4 — Controls 6 and 7 are admission, not publication

`same_path_root_replacement_is_not_silently_adopted` (path-keyed map) and
`concurrent_first_open_performs_exactly_one_cold_load` (load outside the lock,
then `or_insert`) both sit at the point where a project enters the registry.

Suspicion: one admission seam, and both are consequences of identity being
positional (a path, a map key) rather than an established fact.

## S5 — The count I guessed

Two or three. See the disclosure above. I have low confidence in it and it is
the single most anchoring thing in this file.

## S6 — The thing I most expect to be wrong

That the grouping is clean at all. Eight controls written months apart, against
a design that then shifted under them, may simply not partition. A grouping
that reports "these four share a seam, these four do not group and here is why"
would be more useful than a tidy taxonomy that flattens a real difference.

## What I deliberately kept out of the brief

- Every grouping above.
- The seam count.
- Which control I think is hardest (5 — the persistence/publication boundary).
- Which I think is cheapest (2 — it reads as a local error-path fix).

If your independent pass reached a different partition, your pass is the more
valuable document. Say so plainly.
