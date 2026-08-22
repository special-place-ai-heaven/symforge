# Review request — what PR #609 already did, against the six-seam map

## The question

PR #609 (`cursor/critical-bug-investigation-ca81`, draft) modifies files that
two of the six mapped seams live in. Before anyone closes it or starts seam
work:

> **What has #609 already done to these files, and how does that relate to the
> six-seam map?**

For each change: does it *close* a seam, *partially* touch one, *conflict* with
one, or is it *unrelated* to the map entirely.

This is a read, not a verdict on the PR's fate. Whether it gets closed like
#603 is a separate decision, made after this and not by this.

## Rules of engagement

- **Read-only.** Change no file except your own findings file.
- **No build.** No `cargo build`, `cargo test`, `cargo clippy`.
- **Do not patch, close, reopen, comment on, or push to #609.**
- **Do not un-ignore or reclassify any control.**
- `specs/020-repository-knowledge-index/` is frozen — read only.
- Findings stay off `origin` unless Rob authorizes a push. Say "not on remote"
  rather than implying a branch exists.

## Facts already verified, so you need not re-establish them

| Fact | Value |
|---|---|
| PR head | `65f96c97b68d62333503cda4f0e6f37b662199f8` |
| Branch | `cursor/critical-bug-investigation-ca81` |
| **Merge-base with `main`** | **`fd4de8dc`** — the exact tree the seam map was built on |
| State | draft, mergeable, author `app/cursor` |
| Size | +232 / −18 across five files |

**The merge-base matters**: #609 forked from the identical SHA the six-seam map
describes. Any difference between them is #609's doing, not drift between two
trees.

Per-file:

```
68   3   src/daemon.rs
51   4   src/index_lifecycle/activation.rs
16   4   src/index_lifecycle/adapters.rs
93   3   src/index_lifecycle/registry.rs
4    4   tests/preventive_runtime_dark_v11.rs
```

Its three commits:

```
a4a02746  fix: retire failed daemon admissions
7e10ca38  test: cover failed daemon admission cleanup
65f96c97  test: refresh V11 source fingerprints
```

**Verified**: it touches neither `tests/project_index_lifecycle_slice0.rs` nor
`scripts/slice0-oracle-artifact.cjs`. The control bodies and the fail-closed
roster are untouched.

## The map it is being read against

Ledger `docs/reviews/FEATURE-020-POST-V11-LEDGER.md` @ **`48551187`**, section
"Seam map — the eight code-wrong controls are SIX seams".

The two seams whose files #609 modifies:

- **S5** — path-keyed `PROJECT_AUTHORITIES`, `src/index_lifecycle/activation.rs:894-911`
- **S6** — `ensure_project_slot` / `or_insert`, `src/daemon.rs`

`adapters.rs` and `registry.rs` are not named by any seam. Whether that means
#609 is doing something outside the map, or the map missed something, is one of
the things worth telling me.

## What to produce

```
docs/reviews/REVIEW-FINDINGS-<your-name>-609-vs-seam-map-2026-08-21.md
```

### Part 1 — per-file, what it did

```
### <path>
- **What changed**: mechanism, in your own words
- **Relation to the map**: closes S<n> / partial S<n> / conflicts with S<n> / outside the map
- **If it claims to fix something**: does the code do what the commit says
- **Confidence**: high / medium / low
```

Cover all five files including `tests/preventive_runtime_dark_v11.rs` — a pin
refresh is a claim about what moved, and whether it was derived by the owning
oracle or otherwise is worth knowing.

### Part 2 — open questions

1. Does anything here change runtime authority, public behaviour, writer
   reachability, or activation mode?
2. Does it make any currently-ignored control pass? (A RED control that starts
   passing is a fail-closed error, not a success.)
3. Does any change here make a mapped seam *harder* to close later?
4. Is there anything in it worth keeping regardless of what happens to the PR?
5. What did this brief fail to ask about?

### Part 3 — Outside the questions asked (MANDATORY)

Anything the above does not cover — in the diff, the map, the ledger, the
framing of this request. If nothing, write "nothing" and say what you looked at.

### Part 4 — Negatives

What you checked and found sound, specific enough that a reader can tell.

## Only after Parts 1–4 are written

Open `APPENDIX-609-suspicions-2026-08-21.md` in this directory, then append:

### Part 5 — Delta

Which suspicions you had already reached, which you now think are wrong, which
changed a conclusion above.

**Do not read it first.** I have deliberately not read #609's substance, so the
appendix is thin — but thin is not the same as harmless.
