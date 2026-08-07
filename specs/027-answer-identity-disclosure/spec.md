# Spec 027 — answer identity disclosure

**Status**: proposed (2026-08-07).
**Origin**: reproduced during the README/wiki overhaul campaign, 2026-08-07.
A read-only documentation agent was instructed to work in
`E:/project/symforge-policy` (main), reported that its SymForge index was
bound to `E:/project/symforge` (a worktree 82 commits behind), and declined
to use any SymForge result. It caught this only because it independently
called `health` and read `project_root`. Nothing in the answers themselves
disclosed the mismatch.

## Problem

A code-navigation answer never names the index that produced it, so an
answer from the wrong project is indistinguishable from a fact about the
right one.

Reproduced directly, same machine, same session:

```
status detail=projects
  → project-v1-2dff4413… home=yes name=symforge
    root=//?/E:/project/symforge files=919 symbols=26057 index=Ready

search_symbols query="admit_disk_read"
  → No symbols matching 'admit_disk_read'. Try: search_text(query=…),
    or explore(query=…) for concept-based discovery.
```

`admit_disk_read` is `src/protocol/read_gate.rs:92` in the repository the
operator was working in — the function `CLAUDE.md` calls "the single
admission gate for raw-disk content reads". `read_gate.rs` does not exist at
all in the bound tree. The answer is correct about the tree it consulted and
actively misleading about the question that was asked.

Three properties make this worse than a plain miss:

1. **The envelope asserts currency without identity.** `find_references` on
   the same session renders `Trust: heuristic | current index | parsed |
   full`. Every project on the machine renders those same three words.
   "current index" tells the reader the index is current; it never tells them
   *which* index, so there is nothing to notice.
2. **The suggestion steers away from the real cause.** Offering
   `search_text` and `explore` frames the outcome as "this is not a symbol",
   when the true cause is "this is not that repository". A cooperative agent
   follows the suggestion, gets a second empty result, and concludes the code
   does not exist.
3. **Empty results are where disclosure matters most.** A wrong answer looks
   wrong under review; a wrong *absence* looks like a fact. Agents whose
   entire output is conclusions drawn from this surface — `code-reviewer`,
   `security-reviewer`, `readme-architect` — are precisely the ones with no
   independent signal to catch it.

This is the reporting invariant recorded in `CLAUDE.md` ("a component may not
report success for an operation whose completion it did not observe") applied
one level out: the verb reports a *conclusion* — "no such symbol" — without
disclosing the *basis* it concluded from.

Multiple daemons and multiple worktrees are normal on this project (the
`symforge` / `symforge-policy` pair exists by design), so binding drift is not
an exotic state to be engineered away. It must be **visible** instead.

## Requirements

- **FR-027-1**: Every empty / no-match response from a code-navigation verb
  MUST disclose the identity of the project that answered it — at minimum the
  project name and root, plus the generation. It MUST appear before any
  suggested alternative tool.
- **FR-027-2**: Where a trust envelope is already rendered, the identity MUST
  be carried on the existing `Source:` axis rather than as a new trailing
  line, so the envelope remains one block. `source_authority` stays a
  statement about currency; identity is a separate, always-present field.
- **FR-027-3**: The disclosure MUST be derived from the bound project state,
  never from a caller-supplied path or working directory. Callers do not
  reliably pass cwd, and a caller-supplied value would be attesting to itself.
- **FR-027-4**: No automatic rebinding, no cwd sniffing, no heuristic "did you
  mean another project" guess. The system discloses; the caller decides. Every
  agent carrying SymForge read tools now also carries `index_folder`, so the
  corrective action is available once the mismatch is visible.
- **FR-027-5**: Applies to `search_symbols`, `search_text`, `search_files`,
  `find_references`, `find_dependents`, `get_symbol`, and `get_file_context`.
  These are the verbs whose empty result is routinely read as evidence of
  absence.
- **FR-027-6**: Non-empty responses SHOULD carry the same identity field. They
  are lower risk — the returned content is itself checkable — but there is no
  reason for the field to be conditional, and a conditional field trains
  readers to ignore it.
- **FR-027-7**: The added disclosure MUST be bounded. Identity is a name, a
  root and a generation; it is not a project inventory. `status detail=projects`
  remains the place to enumerate open projects.

## Success criteria

- **SC-027-1**: A `search_symbols` query with no matches, executed against a
  project whose root does not contain the symbol, renders the bound project
  name, root and generation, and renders them above the alternative-tool
  suggestion. Regression test asserts ordering, not merely presence.
- **SC-027-2**: The reproduction above stops reproducing: with the session
  bound to a tree lacking `read_gate.rs`, the response for
  `admit_disk_read` names `//?/E:/project/symforge` explicitly, so the
  mismatch is legible from the answer alone with no `health` call.
- **SC-027-3**: A test asserts that the identity field is present on a
  non-empty response of at least one verb (FR-027-6), so the field cannot
  silently become empty-only.
- **SC-027-4**: Token cost of the added disclosure is measured on a real
  response and recorded here. If it exceeds ~40 tokens per response the
  format is wrong and gets tightened — this is a correctness disclosure, not
  a diagnostics dump.
- **SC-027-5**: Full serial suite green. Existing envelope-format assertions
  are updated deliberately and the update is stated in the PR, never implied
  — the envelope literals are asserted in `tests/` and in
  `src/protocol/format/tests.rs`.

## Explicitly out of scope

- Detecting that the caller *meant* a different project. Nothing in the
  protocol carries that intent, and inferring it would be the same class of
  error this spec exists to remove.
- Preventing multiple daemons or multiple bound worktrees. Both are legitimate
  and the orphan-daemon work (PR #538) already made a divergent daemon
  disclose itself in `status`. This spec covers the per-answer surface, which
  that work did not reach.
- Changing what `source_authority` means. FR-027-2 adds identity beside it;
  the currency claim itself is unchanged and remains governed by the
  `FreshnessStatus` derivation landed in #538.
