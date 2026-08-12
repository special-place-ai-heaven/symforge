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

## Recommendations

**Reviewer:** Cursor Grok 4.5  
**Date:** 2026-08-12  
**Checkout reviewed:** `fix/feature-020-refreeze-amendment-a21` @ `9fcde8fb` (`b25fc35f..HEAD`)

Findings are ranked most severe first. Section verdicts that found nothing are stated explicitly after the ranked list.

---

### 1. BLOCKER — `stripCfgTestItems` over-strips comma-terminated constructs, so production edits can leave the census digest unchanged

| Field | Content |
|---|---|
| **Severity** | `BLOCKER` |
| **Location** | `scripts/validate-lifecycle-oracle-traceability.cjs:2296-2323` (`stripCfgTestItems` item consumer) |
| **Claim** | After any `#[cfg(test)]` on a struct field, enum variant, or match arm, the stripper consumes through the next `;` or `{` at `()`/`[]` depth zero and can delete neighboring **release** code from the census. |
| **Why it is wrong** | The consumer tracks only paren/bracket nesting, not braces, and treats `;`/`{` as the only terminators. Comma-separated members never hit those terminators at the member boundary, so the cut continues into production siblings and even past a closing `}` into later items. Verified by executing the shipped functions: for `pub struct S { #[cfg(test)] test_field: u8, prod_field: u8, } fn keep() {}`, renaming `prod_field` does **not** change `normalizeRetirementClosureSource`; the stripped text is `pub struct S {\n    \n` (production field and `keep` removed). The same digest-invisibility holds for enum variants and match arms. Item-level `fn`/`mod`/`const` cases and `#[cfg(all(test, feature = "server"))]` still behave correctly — the hole is specifically comma-shaped syntax. The fail-closed self-tests and the 20-case matrix only append item-level `#[cfg(test)]` modules; they never plant a field/variant/arm attribute, so green gates cannot see this. |
| **Failure scenario** | During Slice 1 work on a censused V10 file, add a test-only field/variant/arm (`#[cfg(test)] …,`) beside production members, or land such a construct while editing. Then change a production sibling (rename a field, alter a match arm body, edit a following `fn`). `validateRetirementClosure` still reports the old digest: V10 authority drifted while the contract claims it is frozen. |
| **Recommended fix** | Stop stripping unless the attributed construct is a clear **item** at a known boundary. Minimal sound fix: track brace depth while scanning the file and only honor `#[cfg(test)]` runs at brace depth 0 (module item position). That preserves T014's `#[cfg(test)] mod` / item helpers and refuses to strip inside `struct`/`enum`/`match`/`impl` bodies (fail-closed: leaving test-only members in the census moves the digest, which is safe). Add fail-closed self-tests that (a) put `#[cfg(test)]` on a field/variant/arm beside production siblings and assert a production rename **moves** the digest, and (b) assert the test-only member alone does not need to be invisible if you adopt the depth-0 restriction. |

---

### 2. MAJOR — Slice 0 CI artifact does not pin the expected case set, so a deleted control still looks like success

| Field | Content |
|---|---|
| **Severity** | `MAJOR` |
| **Location** | `scripts/slice0-oracle-artifact.cjs:30-57`, `108-119` |
| **Claim** | The producer fails only when a parsed case is not `FAILED`; it never checks that a fixed allowlist of control names is present, so removing a RED control still exits 0. |
| **Why it is wrong** | The integration suite is invoked as `cargo test --test project_index_lifecycle_slice0 -- --ignored` with no name filter and no expected-case table. `unexpected = cases.filter(observed !== "failed")` is empty when every *remaining* case still fails. That reports `status: "expected_failures_preserved"` after the control set shrinks — success claimed without observing that the named Slice 0 roster is still intact. The watcher/daemon suites use `--exact` and throw on zero parsed cases, so they are safer; the ten-file suite is not. This is the CLAUDE.md failure mode: the artifact reports preservation it did not measure. |
| **Failure scenario** | Delete or rename `observer_replacement_gap_is_latched_as_non_current` (or any other integration control). CI still runs the producer, gets nine `FAILED` rows, writes the JSON, exits 0, and uploads "expected_failures_preserved". Slice 0 silently lost a positive control. |
| **Recommended fix** | Hard-code the exact expected case names (all twelve ignored RED controls; the inherited opaque-path case is not RED and does not belong here). After parsing, require `cases.map(c => c.case).sort()` to equal that allowlist; otherwise exit non-zero with `SLICE0_ORACLE_MISSING` / `SLICE0_ORACLE_EXTRA`. Prefer `--exact` per case (as the watcher/daemon suites already do) so a missing name yields zero parsed rows rather than a quiet shrink. |

---

### 3. MAJOR — Timing-raced controls can fail their preconditions (or, worse, pass) for reasons other than the named defect

| Field | Content |
|---|---|
| **Severity** | `MAJOR` |
| **Location** | `tests/project_index_lifecycle_slice0.rs:525-598` (`watcher_mutation_during_candidate_build_is_not_discarded`), `623-710` (`whole_project_publication_preserves_latest_siblings`), and to a lesser degree `432-508` (`old_observer_delivery_after_promotion_is_not_current`) |
| **Claim** | Several RED controls gate the real assertion on wall-clock races (`sleep(150ms)`, "large enough" trees, debounce windows). A scheduling miss fails a precondition assert; a lucky ordering can make the defect-assert pass even while the defect remains. |
| **Why it is wrong** | The review request's central question is whether failure is for the claimed reason. On a fast runner, `reload.is_finished()` before the watcher lands `mutated_during_build.rs` makes `landed_before_swap` false — failure message is the precondition, not "destroyed by the swap". On a slow runner where the edit is applied only after swap into a new publication that happens to include the file, `survived` can be true and the control goes green while candidate isolation is still absent — exactly the "stops failing / gone vacuous" case the artifact treats as a real fix. The freshness controls that assert latch-after-clean-reload are soundly reframed; these build/swap races are not. |
| **Failure scenario** | (a) CI machine finishes `reload` of 1500 files before the 150ms write is observed → RED for precondition, artifact still happy, later Slice 4 un-ignore stays flaky. (b) Mutation only becomes visible after swap → `survived == true` → producer exits 1 claiming the defect is gone. |
| **Recommended fix** | Replace sleeps with a deterministic seam (the mid-commit hook pattern already used for the watcher generation oracle, or a test-only barrier around the out-of-lock build). Assert the race window was entered by an observed signal (hook fired / generation advanced / build-started flag), not by `sleep`. Keep the final defect assert, but make the precondition an observed fact. |

---

### 4. MINOR — Stale comment still claims the census cannot see `cfg(all(test, …))`

| Field | Content |
|---|---|
| **Severity** | `MINOR` |
| **Location** | `src/live_index/store.rs:3579-3585` |
| **Claim** | The comment says the re-export must use stacked `#[cfg(test)]` attributes because the stripper only recognises a literal `#[cfg(test)]`, but the code now uses `#[cfg(all(test, feature = "server"))]` and the stripper evaluates `all`. |
| **Why it is wrong** | The comment reports a restriction the checker no longer has. That is the same "thing that reports is not the thing that knows" shape, in documentation form: a future editor will reintroduce the stacked workaround (or distrust the `all` form) based on a false claim. |
| **Failure scenario** | An agent reads the comment, "fixes" the attribute back to stacked form or avoids `all(...)` elsewhere, churning censused files for no reason. |
| **Recommended fix** | Delete the obsolete paragraph; keep the real reason for `feature = "server"` (embed build / unused-import under `--no-default-features --features embed`). |

---

## Section verdicts (explicit)

### §1 Retirement census normalizer
**Issues found:** finding 1 (BLOCKER).  
Also checked and **not** raised as defects:
- `cfgPredicateIsTestOnly` composition for `all` / `any` / `not` / empty / unknown shapes is fail-closed and matches the stated policy (`not` never stripped; unknowns kept in census).
- `rustAttributeAt` on **masked** source correctly ignores `]` inside string literals (`#[doc = "]"]` still strips only the following test item and keeps production).
- Item-level `const X: [u8; 4]`, `#[cfg(all(test, feature = "server"))]`, lifetimes vs char literals, and raw strings behaved correctly in direct execution of the shipped normalizer.
- Repo-wide mask byte-identity is evidence the **refactor** did not change the old lexer on this tree; it is **not** evidence the stripper's item consumer is sound for constructs the matrix never builds (finding 1).

### §2 `#[ignore]` resolvable only for `planned_exact`
**No issues found.**  
`planned_exact` passes `allowIgnored=true` (`validate-lifecycle-oracle-traceability.cjs:1300-1303`). `inherited_exact` / `executed_exact` call `rustNamedCaseExists` → `allowIgnored=false` (`1072-1077`, `1320-1327`). Materialized case receipts also resolve without `allowIgnored` and require `status: "passed"` (`3057-3084`), so an ignored body cannot underwrite a passed receipt. `cfg_attr` is rejected on every path (`1045-1051`). Promoting a catalog row to `executed_exact` while leaving `#[ignore]` fails resolution. I did not find another in-tree escape that both keeps the case resolvable as executed and still ignored.

### §3 Thirteen positive controls
**Issues found:** finding 3 (MAJOR) on the timing-raced build/swap controls.  
Also checked:
- Freshness latch reframes (`observer_replacement_gap_*`, `old_observer_delivery_*`, `same_path_root_replacement_*`) assert survival across a subsequent clean `reload`; no remaining weak "must be non-Current right now" sole assert.
- Mid-commit hook (`store.rs:3533-3570`, fire at `2412-2413`): `#[cfg(test)]` module + fire site; one-shot `take()`; RAII clears the thread-local; `read()` / generation loads are lock-free so the hook does not deadlock on `write_mutex`. **No issue** on deadlock / cross-test leak / production fire.
- Inherited `TEST-OPAQUE-PATH-INHERITED` is identity resolution of an existing non-ignored test, not a RED control; not exercised by `slice0-oracle-artifact.cjs` (correct).
- I did **not** re-run the ignored suite end-to-end here (long / expensive); claims about current RED outcomes rely on code reading plus the author's recorded observations.

### §4 Observe / tear down / assert ordering
**No material issues found** for the daemon/watcher RED controls that hold OS threads: they assign observations to locals, stop watchers / `shutdown_tx`, then assert (e.g. `slice0` lines 121-133, 186-194, 286-298, 379-406, 478-507, 574-598, 681-710, 763-791, 831-852). The daemon unit control and the watcher generation control do not spawn `notify` watchers. Panic-before-assert can still skip `EnvVarGuard` drop only if the panic occurs outside the normal unwind path; under `--test-threads=1` that is the same class of leak the suite already accepted for process env, not a new LNK1104 path. Tokio runtimes built inside `run_daemon_test` drop when the wrapper returns after assert/panic unwind.

### §5 CI wiring and bounded artifact
**Issues found:** finding 2 (MAJOR).  
Also checked and **not** raised:
- Parse-none / abort / compile-fail: `cases.length === 0` throws — fail-closed.
- `shell: false` + argv-only `spawnSync` — no shell injection surface in the producer.
- `ci.yml` ∈ `FROZEN_PATHS` (`release.yml:856-876`): wiring requires a successor approval. That is **known-open item 3**, not a new defect.
- Panic with `FAILED` but empty stdout yields `reason: null` while still counting as failed — acceptable for the RED-preservation signal.

### Known-open challenges
I do **not** challenge the claim that T018's three unwritten controls need V11 surfaces; nothing in today's public API exposes a binding epoch, process capacity ledger, or `symforge::embed` raw-bypass seam that would make those oracles executable as stated.
