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

**Reviewing model:** Kimi (kimi-code/k3)
**Date:** 2026-08-12
**Method:** cloned `fix/feature-020-refreeze-amendment-a21` at `9fcde8fb`; extracted `rustCharacterKinds`, `maskRustCommentsAndLiterals`, `cfgPredicateIsTestOnly`, `rustAttributeAt`, `stripCfgTestItems`, `canonicalReleaseSource`, `normalizeRetirementClosureSource` verbatim into a probe harness and exercised them against synthetic inputs and the real censused files; ran the checker itself green, then green again after a planted production mutation (end-to-end silent-drift demonstration, file restored afterwards). I did not re-run the Rust test suites; Rust-side judgements below are from reading, not execution, and are marked as such.

---

### Finding 1 — BLOCKER

- **Severity:** BLOCKER
- **Location:** `scripts/validate-lifecycle-oracle-traceability.cjs:2269-2345` (`stripCfgTestItems` item-consumption scan); live trigger at `src/protocol/knowledge_curation.rs:49-71`
- **Claim:** The stripper only knows two item terminators — `;` or `{` at paren/bracket depth zero — so a `#[cfg(test)]` on a struct field, enum variant, function parameter, or match arm (comma-terminated constructs) consumes everything from the attribute through the next `;`/`{` it happens to find, deleting production code from the census digest; this is not hypothetical, it is live in a censused file today.
- **Why it is wrong:** The stated security property is "any change to code the release build compiles MUST move the digest." In `src/protocol/knowledge_curation.rs` (censused under `cache`, `writers`, and `callbacks`), the struct fields at lines 49-57 carry `#[cfg(test)]`. I instrumented the shipped code: the cut starting at line 49 runs to line 71, removing the five test fields, the struct's own closing brace, the `#[derive]`/`#[allow]` attributes, and the **entire production enum `CurationWriteStage`** — a type compiled in release and referenced from the production write path (`maybe_fail` callsites at lines 511, 538, 560, 753-776). I then planted a production mutation — a new enum variant `AfterSilentProductionDrift` — and ran the real checker: `lifecycle oracle traceability v11: OK (78 requirements, 24 acceptance oracles, 13 retirement categories)`. Green. The closure digest does not cover that region. (File restored; checker green again.) The surviving canonical form is additionally unbalanced (`struct KnowledgeCurationCoordinator {` is never closed), which no gate inspects. Synthetic probes confirm the same class for enum variants (cut runs past the enum close into the following `match`), function parameters (the param list's own `(` precedes the attribute, so the scanner's relative nesting goes negative at `)`, terminators never fire, and the cut runs to end-of-file — exempting the entire rest of the file), and match arms (cut runs to EOF).
- **Failure scenario:** Any edit to `CurationWriteStage` — add/remove/rename a crash-stage variant, change `maybe_fail`'s stage plumbing — leaves the `preactivation_closure` digests unchanged while the contract claims V10 authority is frozen. Worse, because the digests were *recorded* with this normalizer, the current tree self-validates, so the hole is invisible until someone diffs what the digest actually covers. Any future `#[cfg(test)]` field/variant/param added to any censused file silently enlarges the uncovered region, potentially to the whole remainder of that file.
- **Recommended fix:** (a) Add `,` as a depth-0 terminator in the item-consumption scan, which correctly ends struct fields, enum variants, match arms, and parameters without disturbing `;`-terminated items or `{`-terminated bodies. (b) Add a fail-closed structural gate: after stripping, verify the surviving source has balanced `{}`/`()`/`[]` (the current `knowledge_curation.rs` output fails this) and fail the checker with the offending file and cut span. (c) Add fail-closed self-test fixtures for `#[cfg(test)]` on a struct field, enum variant, function parameter, and match arm, each asserting that a production mutation *after* the gated member moves the digest — the exact case the 20-case matrix and the 103 fail-closed cases do not cover. (d) Re-record the closure digests and roll them into the already-required successor attestation.

### Finding 2 — MAJOR

- **Severity:** MAJOR
- **Location:** `src/protocol/knowledge_curation.rs:724-731` (cut spans lines 724-727, ending mid-statement at `} else {`); root cause at `scripts/validate-lifecycle-oracle-traceability.cjs:2317-2331`
- **Claim:** A `#[cfg(test)] let result = if … { … } else { … };` statement is cut only through the first `{…}` block, leaving the orphaned ` else { durability_probe(&canonical)… };` in the canonical form — so test-only code is present in the digest and test-only edits move it.
- **Why it is wrong:** The amendment's stated purpose is that test-only code is invisible to the census. The `else` arm belongs to a `#[cfg(test)]`-gated statement (release builds use the adjacent `#[cfg(not(test))] let result = …` twin), yet the stripper's block-terminated cut strands it. Verified against the real file: inserting a *comment* inside the orphaned else moves the digest (comments are dropped, but a comment between two code tokens canonicalizes to a separator, so `durability_probe(&canonical)` vs `durability_probe (&canonical)` differ).
- **Failure scenario:** A later slice touches the test-only failpoint branch in this file — exactly the T014-style edit the amendment exists to permit — and the closure digest moves, forcing an unplanned refreeze/re-attestation; or a reviewer, seeing the digest move, cannot tell whether production code changed (it did not).
- **Recommended fix:** When a statement-level cut ends at a `}`, continue consuming through any immediately following `else`/`else if` chain and the terminating `;`. Add a fixture: `#[cfg(test)] let x = if c { a } else { b };` with a mutation inside the else must not move the digest, and the surviving text must remain delimiter-balanced (the Finding 1(b) gate would also catch regressions here).

### Finding 3 — MAJOR

- **Severity:** MAJOR
- **Location:** `specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md:7-10`
- **Claim:** The frozen, attested contract prose still describes raw-byte hashing ("only CRLF pairs are normalized to LF before hashing… no other byte or Unicode normalization is permitted") while the checker now strips `#[cfg(test)]` items, drops comments, and collapses whitespace.
- **Why it is wrong:** The prose is inside the refreeze-pinned file and therefore inside the attested digest. An auditor reading the signed contract concludes the census pins bytes; the checker pins a canonical release form. These materially differ — the canonical form is precisely where Findings 1 and 2 live, so the document an approver signs does not describe the mechanism whose hole they are accepting.
- **Failure scenario:** The successor approval (known-open item 3) is signed over the amended manifest while the contract text still denies that any normalization beyond CRLF exists; a later dispute about what was approved is unresolvable from the signed artifacts.
- **Recommended fix:** Amend the preamble in the same commit that fixes Findings 1-2, describing the canonical release form (test-only items removed, comments dropped, code whitespace collapsed, literals verbatim) and its known limits; roll into the successor attestation.

### Finding 4 — MAJOR

- **Severity:** MAJOR
- **Location:** `tests/project_index_lifecycle_slice0.rs` — `failed_reload_retains_the_recovery_observer`, the `rebuild_failed` precondition (`let rebuild_failed = !failed.starts_with("Indexed ");`, ~line 268)
- **Claim:** The control treats *any* non-success response body as the intended capacity-refusal rebuild failure, so an unrelated failure (auth rejection, routing change, 404/500 text) satisfies the precondition spuriously — and then the control **passes** while the defect remains, because a reload that never ran never stopped the old watcher, which observes the post-failure edit.
- **Why it is wrong:** The control's message claims "the failed build stopped the old watcher and returned before starting its replacement." If the `index_folder` call fails for any reason other than the rebuild error — e.g. the `SYMFORGE_DAEMON_AUTH_TOKEN` guard stops matching what the daemon resolves, or the endpoint path changes — `rebuild_failed` is still true, the old watcher is still alive, `after > before` holds, and both assertions pass with the 2.10 defect fully present. The only net that catches this is the CI-SLICE0 "stops failing" alarm, which then misattributes the cause ("fixed without removing `#[ignore]`" vs. "gone vacuous"). This is the same weakness class as the three controls that initially passed on unrelated present state; this one still has it.
- **Failure scenario:** Any environmental or API drift in the daemon HTTP call turns this RED control green without the defect being fixed; Slice 4 then has no genuine failing oracle for observer retention.
- **Recommended fix:** Assert the failure is *specifically* the capacity refusal — match the response body against the admission-refusal signature (the way the sibling control asserts `indexed.starts_with("Indexed ")` for the success path), and fail the precondition on any other body.

### Finding 5 — MAJOR

- **Severity:** MAJOR
- **Location:** `tests/project_index_lifecycle_slice0.rs` — `configured_capacity_bounds_the_process_not_each_load`, the `assert_eq!(projects, 2, …)` precondition (~line 848)
- **Claim:** The control's precondition that both projects must be open conflicts with the fix shape its own sibling control demands, so it will keep failing — on its precondition, not its assertion — after a correct Slice 2 fix.
- **Why it is wrong:** Control 1 (`capacity_refused_open_creates_no_slot_and_no_watcher`) requires that a capacity refusal surface as a refused open: no slot, no watcher. Once capacity is a process-wide reservation (this control's demanded fix), the second project's open under an exhausted reservation is precisely such a refusal — `open_project_session` errs, `projects == 1`, and the precondition `"both projects must be open for this to measure an aggregate at all"` fails. The control stays RED after the defect is fixed, misattributing the failure to the capacity property. This is the "keeps failing after a correct fix" pattern the request asks to be hunted, on the fix path of the very next slice.
- **Failure scenario:** Slice 2 lands typed admission refusal and a process-wide reservation; this control fails with a precondition message; the slice owner must either weaken the control mid-flight or re-open the design, with the RED indistinguishable from a real regression in the CI-SLICE0 artifact.
- **Recommended fix:** Restate the success property so it survives either fix shape: accept "second open refused" *or* "aggregate admitted ≤ ceiling", i.e. assert `admitted <= CEILING` when both open, and treat a typed refusal of the second open as satisfying the bound (zero admitted beyond the ceiling).

### Finding 6 — MAJOR

- **Severity:** MAJOR
- **Location:** `scripts/slice0-oracle-artifact.cjs` — `runSuite` (lines 79-106) and the artifact assembly (lines 108-120)
- **Claim:** The producer never checks cargo's exit status or signal, never validates that the parsed case set equals the expected set of 13 controls, and imposes no timeout — so a test binary that aborts mid-suite yields a subset artifact that still reports `expected_failures_preserved`, and a hung control stalls CI until the runner limit.
- **Why it is wrong:** The artifact is the contract's positive evidence that each control still fails. `runSuite` emits whatever `^test … (ok|FAILED|ignored)$` lines it finds; a process abort (double-panic, `abort`, OOM) after printing some results leaves `cases.length > 0`, so the "no cases parsed" throw does not fire, and the missing controls' redness is silently unverified. Nothing downstream compares case names against the contract — the checker only validates that the `CI-SLICE0` artifact *id* exists (`EXPECTED_CI_ARTIFACT_IDS`, line 198). The result regex additionally cannot see `should_panic` suffix lines or names outside `[A-Za-z0-9_:]+` (not live today, but the same silent-subset mechanism). On the hang side: the notify-thread leak this slice fixed is precisely a "binary never exits" failure; if it regresses via a precondition panic (Finding 7), `spawnSync` has no `timeout`, and the job burns the GitHub default (hours) instead of failing with evidence.
- **Failure scenario:** A control starts aborting the test process after printing 9 of 10 results; the artifact records 9 cases, all failed, status `expected_failures_preserved`, CI green, artifact uploaded — the tenth control's regression (or vacuous pass) is invisible.
- **Recommended fix:** (1) Embed the expected case list (the 13 control names are enumerable from the contract) and fail unless the parsed set equals it exactly. (2) Treat a non-zero-but-not-test-failure cargo status, or any signal, as an error regardless of parsed lines. (3) Pass a `timeout` to `spawnSync` (the materialized-command path already uses `timeout_ms: 1_800_000`; reuse that magnitude) and fail loudly on timeout. (4) Widen the name class or anchor the regex on ` \.\.\. ` outcome only.

### Finding 7 — MINOR

- **Severity:** MINOR
- **Location:** `tests/project_index_lifecycle_slice0.rs` — control 3 precondition `assert!(before > 0, …)` (~line 246) and `.expect("open project session")` after `spawn_daemon`; control 8 precondition `assert!(indexed.starts_with("Indexed "), …)` (~line 762) with the daemon still running; control 9's `.expect("open project")` after spawn
- **Claim:** The "observe into locals, tear down, then assert" ordering is not applied consistently: several controls assert preconditions (or panic via `.expect()`) after `spawn_daemon` but before `shutdown_tx.send(())`, reintroducing the panic-path daemon leak the ordering fix was created to eliminate.
- **Why it is wrong:** The file's own header states the invariant unconditionally: "Assertions before teardown are how that happens; assertions after it cannot." On a precondition failure — which is exactly when the environment is degraded and evidence matters most — the panic unwinds past the live daemon, the notify OS threads survive, the test binary never exits, and on Windows the next build hits LNK1104. The main RED assertions are correctly placed after teardown in all thirteen controls; the residue is the precondition/expect paths between spawn and shutdown.
- **Failure scenario:** A precondition fails on a loaded CI runner (e.g. `before == 0` because indexing lagged); the binary hangs; combined with Finding 6's missing timeout, the job stalls for hours with no artifact.
- **Recommended fix:** Defer every assertion after the first `spawn_daemon` until after `shutdown_tx.send(())` (capture into locals, as the main assertions already do), or wrap the post-spawn body so shutdown runs on the unwind path. Apply the same rule to `.expect()` calls between spawn and shutdown where a refusal is a survivable outcome.

### Finding 8 — MINOR

- **Severity:** MINOR
- **Location:** `scripts/validate-lifecycle-oracle-traceability.cjs:830` (the char-literal arm of `rustCharacterKinds`)
- **Claim:** The char-literal pattern `/^'(?:\\.|[^'\\\r\n])'/u` misses two legal Rust spellings — non-BMP characters (`'🙂'`, because JS `[^…]` matches one UTF-16 code unit and an astral char is two) and multi-char unicode escapes (`'\u{1F600}'`, because `\\.` allows exactly one char after the backslash) — so the 307-file byte-identical mask comparison does not cover constructs absent from this repo.
- **Why it is wrong:** The comparison validates the *refactor* against constructs present in the corpus; it is no evidence for constructs absent from it. For the digest the fallback is mostly benign (contents are digested as code, still change-sensitive), but there is a structural escalation: an escaped spelling containing a brace, e.g. `'\u{7B}'`, is lexed as code, so its `{` participates in the masked-source brace matching used by `stripCfgTestItems`, `rustModuleIntervals`, and `matchingRustBrace` — a single such literal unbalances every downstream cut in that file and compounds Finding 1 (an over-cut into production code). Byte strings (`b"…"`), C strings (`c"…"`, `cr#"…"#`), and byte chars decompose into code-prefix + correctly-lexed literal and are safe.
- **Failure scenario:** A censused file gains `const OPEN: char = '\u{7B}';` above a `#[cfg(test)] mod tests`; the module's block matching runs one `}` late and the cut swallows the production item following the module — silent drift, same shape as Finding 1 but triggered from the lexer side.
- **Recommended fix:** Extend the pattern to `'(?:\\(?:u\{[0-9A-Fa-f_]+\}|.)|[^'\\\r\n])'` (the `u` flag already gives code-point semantics for the lone-char arm) and add mask fixtures for `'🙂'`, `'\u{1F600}'`, `'\u{7B}'`, and `'\u{7D}'`.

### Finding 9 — MINOR

- **Severity:** MINOR
- **Location:** `scripts/slice0-oracle-artifact.cjs` (the file itself); `.github/workflows/release.yml:856-873` (`FROZEN_PATHS`); `specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md`
- **Claim:** The new producer is invoked from frozen `ci.yml` but is itself in neither `FROZEN_PATHS` nor the refreeze manifest, so it can drift between the approved commit and the release ref with no gate noticing.
- **Why it is wrong:** The release gate diffs `FROZEN_PATHS` between the approved tree and the release tree precisely so that executables wired into frozen workflows cannot change after approval; both sibling checker scripts are listed. The omission appears accidental rather than principled — the step's output is uploaded as contract-named evidence (`CI-SLICE0`). Under the repo's own standard, the thing that reports (the artifact) is produced by a thing nobody pinned.
- **Failure scenario:** A well-meaning edit to the producer (regex tweak, case cap, exit-code change) lands between approval and release; the frozen workflow executes it; the released tree's Slice 0 evidence semantics differ from what the approver saw, with a clean gate.
- **Recommended fix:** Add `scripts/slice0-oracle-artifact.cjs` to `FROZEN_PATHS` (and to the successor approval's scope), or move its logic into the already-frozen checker as a subcommand.

### Finding 10 — NIT

- **Severity:** NIT
- **Location:** `src/live_index/store.rs:3576-3583`; `scripts/validate-lifecycle-oracle-traceability.cjs:2346-2354` (doc comment); `scripts/slice0-oracle-artifact.cjs:74`
- **Claim:** Three small accuracy defects in comments/bounds: (a) the store.rs comment above `install_reload_mid_commit_hook` says the re-export is "written as two stacked attributes rather than `cfg(all(test, feature = "server"))`" while the code now uses the single `all` spelling (the oracles doc confirms the workaround was reverted) — the comment describes code that is not there; (b) `canonicalReleaseSource`'s doc comment claims "Reformatting… [is] invisible", but whitespace runs collapse to one space rather than zero, so `a+b` → `a + b` moves the digest — only line-level rewrapping is invisible; (c) `reasonFor` caps reason text at 512 *UTF-16 code units*, not the 512 *bytes* the bound's name and the contract claim, so multibyte text can exceed the cap.
- **Why it is wrong:** (a) and (b) misdescribe the mechanism the next slice will build against — (b) in particular invites a "pure formatting" edit to a censused file that will break the closure; (c) is a unit mismatch in a bound that exists for determinism.
- **Failure scenario:** A contributor rustfmt-spaces an expression in a censused file believing the comment; the closure digest moves; avoidable refreeze churn.
- **Recommended fix:** (a) Delete or rewrite the stale sentence. (b) Downgrade the claim to what the code does ("line restructuring and comment edits are invisible; token-adjacent spacing is preserved") or normalize runs to nothing where safe. (c) Truncate by UTF-8 byte length.

---

### Per-section verdicts (requesting explicit statements where no issue was found)

**Section 1 — the census normalizer.** Findings 1, 2, 8, and 10(b). On the specific sub-questions: `cfgPredicateIsTestOnly`'s `all`/`any`/`not`/empty/unknown reasoning is **correct** for every shape I could construct, including `all()`, `any()`, nested `all(any(test))`, `all(test, not(windows))`, and trailing commas; its comma-splitter can in theory split inside a quoted value (`feature = "a,b"`) but Cargo feature names cannot contain commas and I could not construct a realistic predicate whose answer flips — no finding. `rustAttributeAt` is **safe** against `#[doc = "]"]`, byte strings, and char literals containing `]` because the mask blanks literal contents before bracket scanning — no finding. The 307-file byte-identical refactor evidence is sufficient for the *refactor* but not for absent constructs — Finding 8.

**Section 2 — `#[ignore]` resolvable for planned cases.** **No issue found; the separation is airtight in the code.** `allowIgnored=true` is passed only on the `planned_exact` path (lines 1300-1302); `inherited_exact`/`executed_exact` resolve through `rustNamedCaseExists` with the default strict flag (line 1326 via 1072), and the materialized release-receipt path is likewise strict (line 3082). Promoting an ignored planned row to executed fails closed as `EXECUTED_TEST_CASE_MISSING`. `cfg_attr` is rejected outright, and it is the only textual escape: a macro-generated test or an attribute the run-regex cannot match (e.g. one containing a literal `]` outside a string) fails to resolve at all, which is the safe direction. The nested-paren limitation of the `cfg(...)` matcher (`[^)]*`) rejects `cfg(all(test))` on a test fn — conservative, fail-closed, no finding.

**Section 3 — the thirteen positive controls.** Findings 4 and 5, plus: the reframed "survives a subsequent clean publication" controls (`observer_replacement_gap…`, `old_observer_delivery…`, `same_path_root_replacement…`) are sound as written — they no longer assert the weak "non-Current now" form — but they embed an unstated assumption that an ordinary clean reload must not count as the completeness proof that clears the latch; if the fixing slice's design treats a full clean reload as exactly that proof, all three fail after a correct fix. The contract should state the latch-clearing condition explicitly (I did not report this as a separate finding because the design doc's "retire the gapped token before a successor may serve Current" language supports the controls' reading). The watcher hook control is **sound**: thread-local, consumed on first fire, RAII-cleared on drop (including unwind), `#[cfg(test)]` at both the definition and the single call site so it cannot fire in production, and the hook's `shared.read()` clones an `Arc` off the arc-swap (`store.rs:2101`) rather than acquiring the write mutex, so firing inside the write lock cannot deadlock. The daemon single-flight control counts production's own `ProjectInstance::load` and its 4-racer barrier makes an accidental single-load pass implausible — no finding. The two daemon/watcher-file controls and the inherited control were read; I did not execute any of them.

**Section 4 — the leak-ordering fix.** Finding 7: applied to the main assertions in all thirteen, but not to precondition asserts/`.expect()`s between `spawn_daemon` and shutdown in controls 3, 8, and 9. No other resource class leaks on the panic path beyond those: tempdirs and tokio runtimes unwind safely; the notify OS threads are the only hazard and they are tied to the daemon/watcher lifetime.

**Section 5 — CI wiring and the bounded artifact.** Findings 6, 9, and 10(c). `shell: false` with argv-only invocation is clean; the `SYMFORGE_LIFECYCLE_{CARGO,GIT}_EXECUTABLE` env overrides are inert in CI. The release-gate interaction is otherwise sound: `ci.yml` is in `FROZEN_PATHS`, so the new step is covered by the successor approval, and the upload step's `if-no-files-found: error` keeps the throw-early paths fail-closed.

**Known-open items.** Not re-reported. On the invited challenge (item 1): I found no evidence that the three unwritten T018 controls are observable today — the multi-loader close/rebind and charge-conservation properties genuinely require a binding epoch and a capacity ledger that V10 does not expose, and the raw-embed bypass requires `symforge::embed` surfaces that do not exist; the claim stands.

**Standard check (does any part report a fact it did not observe?):** yes — the closure digest currently reports "these V10 sources are frozen" while not observing the swallowed regions (Finding 1, demonstrated end-to-end), and the CI-SLICE0 artifact reports `expected_failures_preserved` without observing that every control ran (Finding 6). Those two are the change set's load-bearing claims; both need to land before Slice 1.
