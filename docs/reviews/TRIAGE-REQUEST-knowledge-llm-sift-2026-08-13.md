# Investigation request — grok 4.6

Three independent tasks, ordered by value. Do them in order; **Task 2 is the one most
likely to matter**, so do not run out of budget before reaching it.

## Instructions

**Keep verbosity low. Produce one file and nothing else.** Write to:

```
docs/reviews/TRIAGE-FINDINGS-grok-4-6-2026-08-13.md
```

(in the MAIN checkout: `C:\AI_STUFF\PROGRAMMING\symforge\docs\reviews\`)

Structure it with one `##` section per task. Every finding uses this shape:

```
### <BLOCKER|MAJOR|MINOR|NIT> — <one-line title>
- **Where**: path:line
- **Claim**: one sentence, falsifiable.
- **Why it matters**: one or two sentences.
- **Recommended fix**: concrete, minimal.
```

End with `## Negatives` naming what you checked and found sound. A silent omission is
indistinguishable from not having looked.

**Reporting invariant (binding in this repo):** a component may not report success for an
operation whose completion it did not observe. Apply it to yourself — separate what you
**verified by execution** (paste the command and its real output) from what you **inferred
by reading**. Never present the second as the first.

## Your two checkouts

| Path | Contents |
|---|---|
| `C:\AI_STUFF\PROGRAMMING\symforge` | `main` at `7966c9f8` — current shipping code. Write your report here. |
| `C:\AI_STUFF\PROGRAMMING\symforge-sift-review` | detached at `9cbe6a91` — the abandoned branch, ready to build/test in isolation |

The second is a git worktree, deliberately detached so there is no branch to push. Build
there freely; it has its own `target/`. **Do not merge, rebase, push, or delete anything in
either tree.** Read-only except for your one report file.

Disk is not a constraint (1.4 TB free). But: this crate **denies warnings**, and a cargo
run killed mid-write corrupts `target/` into phantom `E0463` / `E0786` / rustc-ICE errors
that are *not* code defects. Scope your builds (`cargo check`, `cargo test --lib <filter>`)
rather than running `--all-targets` cold.

**Do not touch** `src/live_index/index_lifecycle/`, `tests/project_registry_lifecycle_v11.rs`,
`tests/embed_lifecycle_v11.rs`, or `tests/process_capacity_pool_v11.rs` — Feature 020 Slice 2
is being implemented on `main` concurrently.

---

# Task 1 — Triage the orphaned `feat/knowledge-llm-sift` branch

## Situation

17 commits, all on 2026-07-27 within about five hours, then abandoned. **No pull request
was ever opened** — it was not rejected or blocked, it was dropped when the session ended.

```
git diff --stat origin/main...origin/feat/knowledge-llm-sift
24 files changed, 4758 insertions(+), 284 deletions(-)
```

It targets `search_knowledge`, a **shipping** MCP tool. Production changes:
`src/protocol/knowledge_search.rs` (+1304), `src/live_index/knowledge_authority.rs` (+465),
`src/live_index/knowledge_bridge.rs`, `src/protocol/ccr.rs`, `src/protocol/knowledge_model.rs`,
`src/protocol/knowledge_review.rs`, `src/protocol/tools.rs`, `tests/search_knowledge.rs`
(+223). Plus 16 new spec files under `specs/020-repository-knowledge-index/sift/` and six
review/handoff documents (KIMI and GPT56 reviews are on the branch — read them, they are
prior art on this exact code).

Claimed improvements, per its own commit log: answer-first hit blocks, bounded excerpts,
type-aware IDs, global composition/ranking/limiting across sources, whole-block budgeting
that never truncates mid-record, a shared code-anchor formatter with reserved CCR footer
space, `Unknown` lifecycle when no evidence exists, heading evidence ranked above tokenized
path conventions.

## The blocker nobody hit, because nobody tried to merge it

`git merge-tree` reports a **clean textual merge**. That is misleading.
`execution/refreeze_v11.py` enumerates the whole feature root and requires the frozen
manifest inventory to match it exactly:

```python
expected_paths = git.inventory_paths(commit, FEATURE_ROOT, CONTEXT_PATH)
if set(entries) != expected_paths:
    raise RefreezeError("INVENTORY_SET_MISMATCH")
```

The V11 refreeze was created ~2 weeks **after** this branch. Its 16 new files under
`specs/020-repository-knowledge-index/sift/` therefore fail `INVENTORY_SET_MISMATCH`, which
fails the `release-version` CI job. Landing the spec half needs a regenerated manifest and
attestation plus a **human signature**, because the approval chain binds the manifest hash.

## Answer these

1. **Is the production code still worth landing?** For each claimed improvement: is it
   still true of current `main`, or has `main` since changed the same path? Is it a real
   improvement or churn — judge the diff, not the message. Does it change
   `search_knowledge`'s observable output contract, and if so does anything on `main`
   depend on the old shape (`tests/search_knowledge.rs`, `scripts/verify-tools.cjs`
   fixtures, CCR budget accounting)? Rank every production change **land / rework / drop**
   with a one-line reason.
2. **Cheapest honest path** to landing the valuable part: split (production-only PR, spec
   files dropped), cherry-pick named commits, rewrite from the design docs, or drop with a
   written record. Recommend one, with a rough size estimate.

---

# Task 2 — Hunt a live reporting-invariant defect on `main` (highest value)

Two of that branch's own commits are `fix(020): report Unknown lifecycle when no evidence
exists` and `test(020): add mutation receipts, and retract two unproven claims`.

The first implies `knowledge_authority` was **reporting a lifecycle it had never
observed** — which is this repository's binding invariant, in shipping code, since July.

**Determine whether that defect is still live on `main`.** Do not take the commit message's
word for it; read `src/live_index/knowledge_authority.rs` on `main` and find the code path
that assigns a lifecycle. Specifically:

- When there is **no** evidence for a symbol's lifecycle, what does `main` report? A real
  lifecycle value, or an explicit unknown?
- Is there any path where a default/fallback value is presented to a user as though it were
  derived from evidence?
- Same question for whatever the "two unproven claims" were — find them in the branch diff,
  then check whether the corresponding claims are still asserted on `main`.

**If this defect is live, it must be fixed on `main` regardless of whether the branch ever
lands.** That makes it the most valuable thing in this whole packet. Report it as a
BLOCKER with the exact code path, and propose the minimal fix.

If it is NOT live — say so plainly and show the code that makes it not live.

---

# Task 3 — Adjudicate two decisions the Slice 1 author would not decide alone

Context: `docs/reviews/FEATURE-020-SLICE1-EVIDENCE-v11.md`, section "Two open discrepancies
for the next slice to decide". Both were recorded rather than resolved because in each case
the author of Slice 1 is the interested party. **You are the independent party.** Give a
direct verdict on each; "it depends" is not an answer.

**3a. Where does the lifecycle module belong?** `tasks.md` names `src/index_lifecycle/` at
top level for twelve files across Slices 1–4. Slice 1 put it at
`src/live_index/index_lifecycle/` because a new top-level `pub mod` moves the frozen
public-API census from 83 atoms to 84 (`derivePublicApiAtoms`,
`scripts/validate-lifecycle-oracle-traceability.cjs:1996-1999`), and
`symforge::index_lifecycle` is in neither the frozen preactivation set nor
`introduced_v11_atoms`. Options: keep `live_index/`; amend the public-API contract (needs
refreeze + re-signature); or make it a private top-level module (which the
contract-mandated integration oracles in `tests/` cannot reach). **Slice 2 adds five more
files under whichever root wins, so this is decided now.** Which, and why?

Note the sharper version of the question: the census counts only top-level modules, so
nesting keeps the frozen number at 83 while the types remain publicly reachable as
`symforge::live_index::index_lifecycle::…`. Is that the contract's intended granularity, or
is it the frozen surface being evaded? Answer that directly.

**3b. Which slice owns a still-red Slice 0 control?**
`tests/project_index_lifecycle_slice0.rs::same_path_root_replacement_is_not_silently_adopted`
carries `#[ignore = "... remove this attribute in Slice 1 (T022-T029) when physical root
identity is canonical and fenced"]` and is still RED after Slice 1.

The Slice 1 author argued it belongs to Slice 4: T018 (Slice 0) added it, no Phase 3 task
assigns fixing it, T027 implements physical-root replacement inside the new module only,
T023 states production writer integration is Slice 4, and the control drives the production
`LiveIndex` (`load`/`reload`/`freshness_status`) which Slice 1 never touches.

That argument is self-serving — it is the author concluding their own slice is complete.
**Check it.** Is Slice 1 actually incomplete, or is the annotation simply wrong? If the
annotation should change, say to which slice and on what evidence.
