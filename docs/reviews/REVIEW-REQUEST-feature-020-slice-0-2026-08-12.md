# Adversarial code review request — SymForge Feature 020, Slice 0

## What you are being asked to do

Review a body of work that landed across five pull requests on
`special-place-ai-heaven/symforge` on 2026-08-12. Find defects. Be adversarial:
the author of this work also wrote every test that guards it, so nothing here has
had an independent reader.

**Deliverable: one complete Markdown file.** Reproduce the entire contents of this
file unchanged, then append one new section at the end titled `## Recommendations`.

**Name the file** `REVIEW-FINDINGS-<your-model-name>-feature-020-slice-0-2026-08-12.md`,
substituting your own model name in lowercase with hyphens — for example
`REVIEW-FINDINGS-gpt-5-feature-020-slice-0-2026-08-12.md` or
`REVIEW-FINDINGS-gemini-3-pro-feature-020-slice-0-2026-08-12.md`. Several models are
reviewing this independently, so the filename is what keeps their findings distinct.

State your model name and the date at the top of your `## Recommendations` section.

In that section, for each issue you find, give:

| Field | Meaning |
|---|---|
| **Severity** | `BLOCKER` (must fix before Slice 1) / `MAJOR` / `MINOR` / `NIT` |
| **Location** | `file:line`, or the artifact name if not a code line |
| **Claim** | one sentence stating the defect |
| **Why it is wrong** | the reasoning or the counter-example |
| **Failure scenario** | concrete inputs or sequence that produce the bad outcome |
| **Recommended fix** | specific, not "consider reviewing this" |

If you find **no** issues in a section, say so explicitly for that section — a
silent omission is indistinguishable from not having looked. If you cannot verify
something without running it, say that rather than guessing.

Rank your findings most-severe first. Do not pad the list: a confident, short,
correct list is worth more than a long speculative one.

---

## How to obtain the code

This file lives in the repository at
`docs/reviews/REVIEW-REQUEST-feature-020-slice-0-2026-08-12.md`, so you can read it
and everything it references from the same checkout.

```bash
git clone https://github.com/special-place-ai-heaven/symforge
cd symforge
git log --oneline b25fc35f..HEAD          # the five PRs under review
git diff  b25fc35f..HEAD                  # the complete change set
```

If you cannot run git, the files are browsable at
`https://github.com/special-place-ai-heaven/symforge/tree/main` and the change set at
`https://github.com/special-place-ai-heaven/symforge/compare/b25fc35f...main`.

Pull requests, in landing order: **#554, #555, #557, #558, #559**.

The most important single file to read is
`scripts/validate-lifecycle-oracle-traceability.cjs` (the checker), specifically
the functions `rustCharacterKinds`, `maskRustCommentsAndLiterals`,
`cfgPredicateIsTestOnly`, `rustAttributeAt`, `stripCfgTestItems`,
`canonicalReleaseSource`, and `normalizeRetirementClosureSource`.

---

## Background: what this system is

SymForge is a Rust MCP server. Feature 020 replaces its project-index lifecycle
("V10") with a new one ("V11"). Because the replacement is large and risky, the
feature is governed by a **frozen contract corpus** under
`specs/020-repository-knowledge-index/`, protected by several interlocking gates:

- A **refreeze manifest** pins a SHA-256 for every file in the feature corpus.
- A **detached attestation** pins the manifest's own digest.
- An **externally signed approval record** (SSHSIG, held outside the repository)
  pins the attestation digest and an exact commit/tree. The operator holds the
  private key; it is deliberately not obtainable from inside the repo.
- A **retirement contract** carries a `preactivation_closure`: a digest over the
  content of every V10 source file scheduled for removal, so those files cannot
  drift while V11 is built beside them.
- Two checkers enforce all of the above: a traceability checker and its
  self-test, which is a suite of **fail-closed** cases (each mutates a fixture
  and asserts the checker rejects it).

**Slice 0**, the work under review, produces *positive controls*: tests that
reproduce known V10 defects and therefore **must fail**. They are `#[ignore]`d so
a deliberate RED does not turn `main` red.

---

## What changed, and what to attack

### 1. The retirement census normalizer (highest risk)

Originally the census digested raw source bytes. That made it impossible to add a
`#[cfg(test)]` test to a censused file — which `tasks.md` T014 explicitly
instructs. Two amendments changed it to digest a **canonical release form**:
`#[cfg(test)]`-gated items removed, comments dropped, code whitespace collapsed,
string and character literals emitted verbatim.

**The security property that must hold:** any change to code the release build
compiles MUST move the digest. Only test-only code, comments, and formatting may
be invisible.

Attack this specifically:

- Can you construct Rust source where a **production** change does **not** move
  the digest? That is a BLOCKER — it would let a V10 authority drift silently
  while the contract claims it is frozen.
- `cfgPredicateIsTestOnly` treats `all(..)` as test-only if **any** conjunct is,
  `any(..)` only if **every** disjunct is, `not(..)` never, and unknown shapes as
  false. Is that reasoning correct for how `cfg` actually composes? Consider
  `cfg(all(test))`, `cfg(any())`, `cfg(all())`, nested combinations, and
  `cfg_attr`.
- `rustAttributeAt` scans balanced `[` `]` to find an attribute's end. Can a
  token inside an attribute break it — for example a `]` inside a string literal
  in `#[doc = "]"]`, or a byte-string, or a nested attribute macro?
- `stripCfgTestItems` consumes an attributed item by scanning for `;` or `{` at
  paren/bracket depth zero. Where does that mis-parse? Consider `const X: [u8; 4]`,
  generics containing `;`, `macro_rules!` bodies, `where` clauses, raw strings,
  and attributes on `match` arms or expressions.
- `canonicalReleaseSource` collapses whitespace in code regions but emits
  literals verbatim. Is the literal/code boundary correct in every state the
  lexer models — raw strings (`r#"…"#`), byte strings, char literals, lifetimes
  (`'a`), and nested block comments?
- The lexer `rustCharacterKinds` was refactored out of an existing function.
  The author verified the refactor by comparing mask output across 307 Rust files
  (~10 MB) and found it byte-identical. Is that sufficient evidence, or is there a
  construct absent from this repo that would diverge?

### 2. `#[ignore]` became resolvable for planned test cases

The checker previously refused to resolve any catalog-named test carrying
`#[ignore]`. It now permits it **only** on the `planned_exact` path;
`inherited_exact`, `executed_exact`, and the materialized release-receipt path
remain strict, on the reasoning that a receipt claiming `status: "passed"` can
never come from an ignored test.

- Is that separation actually airtight in the code, or can a `planned` row be
  promoted to `executed` while still ignored?
- `cfg_attr` is rejected everywhere because it can add `ignore` conditionally. Is
  that the only such escape?

### 3. Thirteen positive controls

Ten in `tests/project_index_lifecycle_slice0.rs`, one in
`src/watcher/mod.rs::tests`, one in `src/daemon.rs::tests`, plus an inherited one.

**The central question for each: does it fail for the reason its name and message
claim, or would it also fail for an unrelated reason?**

This is not hypothetical. **Three of these controls initially passed** during
development, each because an assertion was satisfied by unrelated present state
rather than by the defect being absent. A fourth failed on its own precondition
rather than its assertion. Specifically:

- V10's `FreshnessStatus` is a **pure function of present state**
  (`recompute_freshness_locked` drops prior reason codes and rederives them). So
  any assertion of the form "must be non-Current" cannot distinguish a real latch
  from incidental degradation. Several controls were reframed to assert that a
  property **survives a subsequent clean publication**. Check whether that
  reframing is sound and whether any remaining control still has the weak form.
- Look for controls that would pass if the defect were fixed *by accident*, or
  that would keep failing after a correct fix (a false alarm for the next slice).
- `src/watcher/mod.rs::tests::generation_before_root_split_…` uses a `#[cfg(test)]`
  one-shot thread-local hook fired inside `reload_for_binding_with_exclusions`
  between the generation advance and the root publication. Is that hook sound —
  can it deadlock, leak across tests, or fire in production?

### 4. A test-process leak, fixed by ordering

Every control is RED, so every run panicked before its teardown, leaving daemon
`notify` OS threads alive; the test binary never exited, and on Windows held its
own `.exe` open (`LNK1104`). All controls now **observe into locals, tear down,
then assert**.

- Is that ordering actually applied consistently in all thirteen?
- Are there other resources (tokio runtimes, tempdirs, spawned tasks) still
  leaked on the panic path?

### 5. CI wiring and the bounded artifact

`scripts/slice0-oracle-artifact.cjs` runs the ignored controls and emits one
deterministic JSON record per case, capped at 512 bytes of reason text. It exits
non-zero when a control **stops** failing. `.github/workflows/ci.yml` now runs it
and uploads the artifact.

- Is the `cargo test` output parsing robust? What happens on a panic with no
  captured stdout, a test that aborts the process, a timeout, or a name
  containing characters the regex does not expect?
- The producer runs `cargo` with `shell: false` and argv only. Any injection or
  path-handling concern?
- Does adding this step to `ci.yml` interact badly with the release gate, which
  diffs `FROZEN_PATHS` (including `ci.yml`) between the approved commit and the
  release ref?

---

## Known-open items — do NOT report these as findings

These are already recorded in
`docs/reviews/FEATURE-020-SLICE0-CAUSAL-ORACLES-v11.md`:

1. **Three of T018's six named controls are not written.** Multi-loader
   close/rebind ordering, charge conservation, and raw embed bypass require V11
   surfaces (a binding epoch, a capacity ledger, `symforge::embed` types) that do
   not exist yet. *Do challenge the claim that they are unobservable today if you
   think one of them is in fact observable.*
2. **The adversarial architecture review itself is outstanding** — that is what
   you are performing.
3. **A successor approval signature is required.** The attestation digest changed;
   the operator must sign a new chained record. Not a defect.
4. **T019 materializes only Slice 0's stubs**, because Slice 4's named tests
   reference types that do not exist and would not compile.

---

## Errors already found and fixed during this work

Listed so you can calibrate, and so you can look for **siblings of these** that
were missed:

- A first stripper implementation left the removed item's surrounding newlines
  behind, so the digest moved anyway. Caught by a positive self-test, not review.
- Its replacement had an off-by-one that consumed a line break belonging to
  surviving code.
- A `#[cfg(all(test, feature = "server"))]` spelling was read as production code
  because the matcher only recognised a literal `#[cfg(test)]`.
- An invented `actions/upload-artifact` SHA (a pin not used anywhere in this
  repo) was committed and then corrected to the pin `release.yml` already uses.
- An edit to `CLAUDE.md` deleted an entire unrelated section because its
  replacement span ran to the next heading.
- A `#[cfg(test)]` re-export broke the `--no-default-features --features embed`
  build, which no default-feature gate can catch.

---

## Verification already performed (verify independently; do not take on trust)

- `cargo fmt --check` — clean
- `cargo clippy --all-targets -- -D warnings` — 0 warnings
- `cargo test --all-targets -- --test-threads=1` — exit 0
- `cargo test --no-default-features --features embed --lib` — 1316 passed
- `node scripts/validate-lifecycle-oracle-traceability.cjs` — OK (78 requirements,
  24 acceptance oracles, 13 retirement categories)
- `node scripts/validate-lifecycle-oracle-traceability.test.cjs` — OK, 103
  fail-closed cases
- `python execution/refreeze_v11.py verify-internal --target-ref HEAD` — passed
- `python execution/test_refreeze_v11.py` — 164 tests, OK
- A 20-case normalization matrix asserting which changes do and do not move the
  census digest

**Green gates are not the question.** Every defect listed in the section above
was green when it shipped. The question is whether the assertions mean what they
claim.

---

## The standard this work is held to

From the repository's own `CLAUDE.md`, binding:

> **A component may not report success for an operation whose completion it did
> not observe.** Not "attempted", not "usually works", not "the code path that
> does it was called" — observed.
>
> The recurring shape: **the thing that reports is not the thing that knows.**
>
> SymForge's product is being trustworthy about what it knows. A wrong answer is
> recoverable; a confidently wrong answer is not.

Judge the change set against that standard. In particular: **does any part of
this work report a fact it did not observe?**
