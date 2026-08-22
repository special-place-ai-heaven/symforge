# Review request — what "serves" means in the publication-authority invariant, and whether it holds

## The question

`src/index_lifecycle/activation.rs:19-24` states a companion invariant:

```
//! Companion invariant (enforced by construction across the cut, recorded
//! here): the two publication roots are never simultaneously authoritative —
//! legacy authority serves only in `LegacyOpen`, the V11 publication root
//! serves only in `PreventiveV1Open`, and the window between them is
//! drain-only.
```

The shipped process is in `PreventiveV1Open`.

> **What does "serves" mean in that sentence, and is the invariant satisfied on
> `main` @ `fd4de8dc`?**

Answer it as a reading of this tree. If "serves" has a narrower technical
meaning than answering a query, say what it is and what establishes it. If the
invariant does not hold, say that. If the sentence cannot be evaluated as
written, say that — that is a legitimate verdict and possibly the most
important one.

## Why it is being asked

A campaign is about to be specified on top of this area. Its scope depends
entirely on this answer, and no one has checked it. It is being asked cold
rather than confirmed, so treat every framing below as a fact to verify, not a
conclusion to agree with.

## Rules of engagement

- **Read-only.** Change no file except your own findings file.
- **No build.** No `cargo build`, `cargo test`, `cargo clippy`.
- **No patch, no PR, no un-ignore, no reclassify.**
- `specs/020-repository-knowledge-index/` is frozen — read only.
- **PR #609 is out of scope.** It stays draft and touches none of this.
- Findings stay off `origin` unless Rob authorizes a push. Say "not on remote"
  rather than implying a branch exists.

## Tree

`main` @ **`48d1d0ab`**

Every fact in the table below was established on `fd4de8dc`. `src/` is
**byte-identical** between the two — verified with
`git diff --stat fd4de8dc 48d1d0ab -- src/`, which is empty. The two commits
since then touched only `.github/workflows/ci.yml`, `tests/`, and `docs/`.
Read `48d1d0ab`; nothing in scope moved.

## Facts already verified, so you need not re-establish them

Verify any you doubt — these are given to save time, not to constrain you.

| Fact | Value |
|---|---|
| Invariant text | `src/index_lifecycle/activation.rs:19-24` |
| Activation mode machine | `ActivationMode` `activation.rs:57-65`; `LegacyOpen → LegacyClosing → PreventiveV1Open`, monotonic, no reverse edge, non-configurable |
| Current mode | `PreventiveV1Open` — the Slice 4 cut shipped as 11.0.0 |
| **`data_plane()` call sites in `src/`** | **226** |
| — `src/protocol/tools.rs` | 78 |
| — `src/sidecar/handlers.rs` | 72 |
| — `src/daemon.rs` | 31 |
| — `src/protocol/edit_tools.rs` | 27 |
| — `src/protocol/mod.rs` | 16 |
| — `src/server/mod.rs` | 1 |
| — `src/server/admin/api_v1.rs` | 1 |
| `data_plane()` returns | `&crate::live_index::store::SharedIndex` (`activation.rs:984-986`) |
| `ProjectPublicationRoot` | `runtime.rs:190`, documented at `:186` as *"The SOLE publication root for a project's runtime state"* |
| **Its production callers outside `runtime.rs`** | **none found** |
| `ProjectArtifactRoot` | `candidate.rs:158` — a second type carrying "root" in its role |
| The phrase "simultaneously authoritative" | appears **only** in the `activation.rs` doc comment; **no test names it**, despite "enforced by construction" |
| A related comment | `activation.rs:933` refers to *"every `index: SharedIndex(Handle)` field as a V10 publication root"* |
| Another related comment | `activation.rs:337-339` — the `ObservationLane` doc, on what the data plane does mid-cut |

## Things worth being careful about

- **"Enforced by construction" is a claim about types, not tests.** The absence
  of a test naming the invariant is not by itself evidence it is unenforced —
  unrepresentable-over-checked is this codebase's stated preference
  (Constitution Principle V). Whether construction actually enforces it here is
  part of the question.
- **Two types carry "root" in their name** (`ProjectPublicationRoot`,
  `ProjectArtifactRoot`) and a comment calls `SharedIndex` fields a "V10
  publication root". Whether "the two publication roots" in the invariant means
  the pair you would guess is worth establishing before evaluating the claim.
- **A count is not a verdict.** 226 `data_plane()` sites and zero
  `ProjectPublicationRoot` callers are facts; what they imply about "serves"
  is the thing being asked.

## What to produce

```
docs/reviews/REVIEW-FINDINGS-<your-name>-publication-authority-2026-08-22.md
```

### Part 1 — the reading

- **What "serves" means here**, with what establishes that meaning.
- **Which two roots** the invariant names, and how you determined it.
- **Verdict**: holds / does not hold / cannot be evaluated as written.
- **What would change the verdict** if you are wrong.
- **Confidence**, and what would raise it.

### Part 2 — if it does not hold

- Since when, as far as the tree shows.
- Whether it is a documentation defect (the wording overclaims) or a runtime
  defect (the property is genuinely absent).
- What a reader of the shipped release would be entitled to assume that is not
  true.

### Part 3 — open questions

1. Is "enforced by construction" true here, and by which construction?
2. Does `ProjectPublicationRoot` having no production callers mean anything, or
   is it legitimately ahead of its wiring?
3. Does any ingress key off `ActivationMode` rather than data-plane readiness?
4. Is the invariant falsifiable at all as written, or is it prose that reads
   like a guarantee?
5. What did this brief fail to ask about?

### Part 4 — Outside the questions asked (MANDATORY)

Anything the above does not cover. If nothing, write "nothing" and say what you
looked at.

### Part 5 — Negatives

What you checked and found sound, specific enough that a reader can tell.

## Only after Parts 1–5 are written

**The sealed appendix is deliberately not in this commit.**

A sealed appendix that ships alongside its brief is not sealed — nothing but
good manners stops a reader opening it first, and the whole value of this
exercise is that your reading was formed without mine. Under a remote-only
protocol the seal cannot be enforced by an instruction, so it is enforced by
publication order instead.

So: commit your Parts 1–5. Once they are on `origin`, say so, and
`APPENDIX-publication-authority-suspicions-2026-08-22.md` will be pushed to this
directory. Then append:

### Part 6 — Delta

Which suspicions you had already reached, which you now think are wrong, which
changed a verdict above.

If a verdict in Parts 1–5 does not survive the appendix, **amend it in place and
say what changed it.** A Part 6 that quietly contradicts Part 1 is worse than
either.

This question is being asked cold on purpose — one reader already has a view of
it, which is why the request came to you.
