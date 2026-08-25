# Review findings — post-cleanup, 2026-08-25

**Baseline:** `main` at `dd441ae2` (clean tree at review start; a later upstream push
`bd29447b` appeared on origin during the session and was not reviewed here).
**Method:** every claim in the five PR descriptions treated as unverified and
re-derived from source; SymForge MCP used as the primary code-intelligence path
throughout; frozen trees read but never edited; repository otherwise untouched.
**Verification receipts observed during this review:**

| Gate | Result |
|---|---|
| `cargo test --no-default-features --features embed --lib -- --test-threads=1` | exit 0, observed (57 s) |
| targeted lib tests (10 filters spanning all three code commits) | 10 passed / 0 failed |
| `--test conformance` | 20 passed / 0 failed |
| `--test rmcp3_protocol` | 15 passed / 0 failed |
| `--test preventive_runtime_dark_v11` (darkness seal incl. FULL_SOURCE_PIN + WORKFLOW_FINGERPRINTS + CARGO_LINES) | 10 passed / 0 failed |
| independent FNV fingerprint recompute of both workflows (Python, seal algorithm) | `release.yml 113a444332929e37:112348`, `ci.yml 26d8df149f93dc45:14056` — byte-consistent with pins |

**Dispositions (2026-08-25, later the same day; reviewer re-verified each):**

| Finding | Disposition |
|---|---|
| d4c5b6c9 HIGH — echoed refusal phrases downgrade a reached apply path | Fixed in #665 (envelope-first classifier); reviewer ran the hostile-source e2e green |
| c636b078 MEDIUM — explicit-prefix file route leaks `?` | Fixed in #665 |
| dd441ae2 HIGH — fail-after-tagging deadlock | Fixed in #664 (clamp to tag with warning); reviewer SHIP |
| dd441ae2 LOW — gate never completed a push run | Superseded: the floor step passed its first real run during the 11.0.11 release (`bd29447b`) |
| 1cdcc897 MEDIUM — p95 smoke unreachable from CI | Open: wire into the scheduled perf lane (workflow edit, fingerprint reseal) after #664 lands |
| Outside 1 — pin refusal arms untested | **Retracted** (see below) |
| Outside 2, 3 — facade analytics silence; unknown-shape fall-through | Ledger items |
| New residual (#665 review) — `torn_after_envelope` phrases are `contains` matches, so an echoed source line quoting `ROLLBACK INCOMPLETE`, `Write failed` or `File disappeared:` can still push an envelope'd success to `InternalFailure` | Ledger item, LOW/MEDIUM; same echo family, rarer phrases |

---

## Commit d4c5b6c9 — fix(edit): mutations that applied nothing are MCP errors (#657)

**Verdict: SHIP-WITH-FOLLOWUP**

The core rule is correctly implemented at every enumerated public mutation exit:

- Predicate: `EditResultStatus::is_terminal_failure` = "not `Found`"
  (`src/protocol/edit_tools.rs:301`) drives `statused_edit_tool_result`
  (`edit_tools.rs:332`). Read side unchanged: `into_call_tool_result` still keys off
  `OutcomeClass::is_error()` (`src/protocol/result_status.rs:196-216`), so
  NotFound/Ambiguous reads stay `isError:false`; conformance case
  `read_get_file_content_not_found_v1` pins it.
- Seven granular tools: `replace_symbol_body` had the wrapper before; `insert_symbol`,
  `delete_symbol`, `edit_within_symbol`, `batch_rename` gained wrappers with pinned
  registration names (`edit_tools.rs:1173-1198`, `1399-1424`, `1618-1643`, `2166-2191`)
  and dispatch entries (`src/protocol/mod.rs:1967-1972`). All seven proxy through
  `proxy_tool_call` and are re-classified from body text on return, so the daemon hop
  cannot silently downgrade a failure to success as long as shapes stay taught.
- Compact facade `symforge_edit`: all five pre-flight refusal exits plus the terminal
  exit route through `mutation_refusal_outcome` +
  `ResultStatus::into_mutation_call_tool_result`
  (`src/protocol/tools.rs:11639-11643`, `11667-11669`, `11707-11710`, `11764-11768`,
  `11776-11779`, `11843`).
- Analytics/wire agreement: `record_tool_completion_with_success`
  (`mod.rs:1058-1079`) passes the exact wire predicate for all seven wrappers;
  read tools keep the old derivation, which still matches their wire predicate. The
  facade records no usage observation at all (token accounting only), so there is no
  path where analytics claims success against an error wire result.
- Batch metadata: `batch_insert` failures now name the operation index
  (`src/protocol/edit.rs:3069`; parser `edit_tools.rs:411-430`; pinned by
  `batch_insert_target_failures_name_the_failed_operation` and the tool-level
  assertion on `_meta...operations[0].operation_index`).
- RED-first evidence accepted: the wrapper-level test asserts serialized
  `isError:true` + `_meta` status for eight distinct no-apply scenarios, file-byte
  invariance, and positive dry-run/apply controls; the HTTP transport test pins the
  contract over real JSON-RPC. Conformance corpus covers all six `OutcomeClass`
  variants at least once.

Findings:

1. **HIGH — successful `batch_rename` outputs can be misclassified as failures**
   `execute_batch_rename` embeds raw source lines into *success* output:
   confident-site preview lines ``L{line}: {src_line}`` (dry run,
   `src/protocol/edit.rs:2716`) and uncertain-match context lines
   ``  {path}:{line}  {ctx}`` (dry run `edit.rs:2676-2679`, applied rename summary
   `edit.rs:2888-2897`). #657 added lowercase refusal arms to
   `classify_edit_output` (`src/protocol/edit_tools.rs:364-366`:
   `"symbol not found:"`, `"file not found in index:"`) that are checked *before*
   the dry-run/success arms. Any renamed symbol referenced near code containing
   those substrings (error-message strings and comments make them common) flips a
   successful apply or preview to `NotFound` → `isError:true` + `_meta.status=
   "not_found"` for work that was observed to succeed — the dual of the defect this
   PR fixes. Reproduce: repo with `fn helper() { bail!("symbol not found: {id}"); }`
   plus a second reference site; `batch_rename(name="helper", new_name="helper2",
   dry_run=true)` → preview renders, file untouched, host sees an error. The new
   unit tests only feed pure refusal strings; there is no positive control of a
   successful output containing refusal-shaped text. Fix direction: classify on
   envelope/header lines rather than the whole body, or require refusal phrases at
   line starts, RED-first on the echo scenario.
2. **MEDIUM — mutation-side wire rule is publicly pinned only for Found/NotFound**
   The conformance corpus's edit cases cover `dry_run_success` (×3) and `not_found`
   (×2) (`tests/conformance.rs:201,215,226,237,248`). No public corpus case
   exercises a mutation ending Ambiguous, InvalidRequest, or InternalFailure, so the
   new `expected_is_error` branch is only half-pinned at the contract layer;
   classification of those shapes exists only in lib unit tests. Followup: add one
   public case per remaining failure class (e.g. ambiguous selector via
   `replace_symbol_body`, safety-refusal internal failure).
3. **LOW — classification order can mask internal failures as NotFound**
   `classify_symforge_edit_outcome` scans the full body for refusal phrases before
   the internal-failure check (`tools.rs:11872-11878`), so e.g. a rollback message
   quoting "symbol not found" would be labeled `not_found` instead of
   `internal_failure`. Wire stays `isError:true`; only `_meta` semantics drift.
4. **LOW — batch-failure index parser accepts a bare space separator**
   `failed_batch_operation_statuses` now treats any digits followed by `' '`
   (`edit_tools.rs:420-428`), so a hypothetical failure text like `Edit 12 files…`
   would attribute the failure to operation 12. Current producers never emit that
   shape; noted as hardening only.

---

## Commit c636b078 — fix(index): exact test stems + strip sentence marks (#658)

**Verdict: SHIP-WITH-FOLLOWUP**

- C1 verified: exact stems added once in the shared classifier
  (`src/domain/index.rs:412`); consumers delegate
  (`src/live_index/query.rs:2236-2240` `file_path_is_test` falls back to
  `FileClassification::for_code_path`). Positive controls pin `contest.rs`,
  `latest.rs`, `attest.rs`, `protest_handler.rs`, `testament.rs`,
  `testing_ground.rs` as source; the rule is stem-exact so none regress. Both new
  tests ran green locally.
- C2 verified on its two touched routes: the bare path heuristic strips terminal
  sentence punctuation before `looks_like_path` and before building the hint
  (`src/protocol/smart_query.rs:342-350`), and scope-hint tails are stripped inside
  `split_trailing_scope_hint` (`smart_query.rs:702`). `src/lib.rs?`,
  `src/lib.rs.`, `src/protocol/mod.rs?!` now resolve; interior dots
  (`v1.2/`, `lib.rs.bak`) survive (tests green).
- Dark-seal FULL_SOURCE_PIN recomputation present in the commit and the seal test
  passed on the merged tree during this review.

Findings:

1. **MEDIUM — the explicit-prefix FindFile sibling still leaks punctuation**
   `find file …`, `where is file …`, `path to …`, `locate file …`, `which file …`
   build their hint from the raw remainder without stripping
   (`smart_query.rs:193-209`, hint at `:205`). `ask("find file src/lib.rs?")` routes
   to `search_files` with pattern `src/lib.rs?` → false absence, exactly the class
   C2 set out to kill, one route over from where it was fixed. Reproduce: ask query
   above; observe routed invocation keeps `?`. Fix: run the same
   `strip_terminal_punctuation` when constructing prefixed hints (RED-first).
2. **LOW (pre-existing residue, same family) — direct-object paths and symbols keep
   sentence marks**: `references to src/lib.rs?` extracts its object through
   `clean_symbol_and_optional_path`, whose no-separator fallthrough does not strip
   (`smart_query.rs:689-696`, fallthrough at `:709`); likewise `where is Foo?`
   yields symbol `Foo?` (`:246-255` first-token). Not introduced here, but C2's
   stated goal ("sentence marks must not travel into routes") remains unfinished on
   these lanes.
3. **Note, no action demanded by this commit** — case-sensitivity gaps remain by
   design scope: `FooTest.java` (Java suffix convention) and `conftest.py` do not
   match any affix/stem rule; ledger C1 promised only the exact-stem family.

---

## Commit 9eb2ad5a — docs(reviews): ledger PR 3 marked blocked (#660)

**Verdict: SHIP**

Ledger-only change; every citation independently confirmed:

- Frozen clause exists as quoted: "The shared `.gitignore` mutation runs after
  successful explicit normal `index_folder` binding…" with steps 1-6
  (`specs/020-repository-knowledge-index/contracts/source-binding-and-state.md:280-298`).
- Invariant 14 exists as paraphrased (byte-for-byte empty/BOM/CRLF/LF/final-newline/
  equivalent/negated/raced/symlinked matrix; automatic paths never mutate)
  (`source-binding-and-state.md:391-394`).
- Both named tests exist on main and assert the write:
  `explicit_normal_index_folder_reconciles_existing_root_gitignore`
  (`src/protocol/tools.rs:17235`, asserts `gitignore_hygiene=effective changed=true`
  and exact appended bytes) and `daemon_index_folder_reconciles_existing_root_gitignore`
  (`src/daemon.rs:8131`).
- "`ObserveOnly` already exists and is already used by health" checks out:
  `GitignoreHygieneAuthority::ObserveOnly` (`src/gitignore_hygiene.rs:13`) consumed
  by the observe-only status path (`src/protocol/tools.rs:3701`).

The note correctly derives "adjudicate, do not implement yet" from Constitution III
plus the frozen amendment authority (see adjudication C0/PR 3 below).

---

## Commit 1cdcc897 — test(sidecar): bound /health functionally (#661)

**Verdict: SHIP-WITH-FOLLOWUP**

- Removing the `<50 ms` wall-clock assertion is justified: it measured the shared
  runner (ledger O0; flaked at 111 ms on 2026-08-24). The functional deadline claim
  is real: `raw_http_get_with_status` bounds connect/read/write at 500 ms socket
  timeouts (`tests/sidecar_integration.rs:98-130`), so a hung `/health` still fails
  the correctness test.
- Method: 50 sequential samples, sorted, p50 = `[25]`, p95 = `[47]`
  (`tests/sidecar_integration.rs:289-298`), assert p95 < 250 ms (`:301`). Against
  the PR's local measurement (p50 ≈ 4.1 ms, p95 ≈ 4.6 ms) the bound is a generous
  regression tripwire, appropriate for shared runners. Correctness assertions
  (JSON shape, fields) retained ahead of it.

Findings:

1. **MEDIUM — the latency oracle is unreachable by automation.** Nothing in
   `.github/workflows/{ci,release}.yml`, `scripts/`, or `execution/` invokes
   `health_latency_p95_smoke` or `sidecar_integration --ignored` (CI's only
   `--ignored` invocations are `live_index_integration`, `coupling_calibration`, and
   the slice0 oracle scripts). Latency coverage went from flaky-but-live to dead
   unless a human runs the documented command. Ledger O0 asked for a *repeated*
   benchmark/distribution check. Followup: schedule the smoke (e.g. non-blocking
   step in release.yml or a nightly job) or record an explicit adjudication that
   manual-only is intended.
2. **LOW — percentile index nit**: `(SAMPLES * 95) / 100 = 47` selects the 48th of
   50 ordered samples (~p96), marginally stricter than p95. Harmless given the
   55× headroom; worth normalizing if the sample count ever changes.

---

## Commit dd441ae2 — ci(release): floor subject-validation baseline (#662)

**Verdict: SHIP-WITH-FOLLOWUP**

Consistency: `WORKFLOW_FINGERPRINTS` matches both workflow files byte-for-byte
(independently recomputed with the seal's FNV-1a+length algorithm and confirmed by
the green darkness-seal test); the step itself sits at
`.github/workflows/release.yml:265-275`.

Dry-run of the next three plausible pushes against real history/tags/runs:

1. *Ordinary conventional push after v11.0.10* — FLOOR=`v11.0.10` (tag commit
   `b1cf25fa`, ancestor of HEAD); LAST = latest successful Release run (currently
   `9eb2ad5a`, run 32875417788) → ancestor check passes; range subjects all squash-
   conventional → **passes**.
2. *Next release-PR merge (v11.0.11)* — tag not yet created at validation time,
   FLOOR still v11.0.10, LAST recent → **passes**; the `chore(main): release …`
   subject is conventional.
3. *Push after a stale-lookup recurrence* (the 2026-08-24 incident: LAST resolves
   pre-v11.0.x) — floor check fails closed with an explicit `::error::` instead of
   sweeping two legacy non-conventional commits (verified they sit below v11.0.0)
   into range → **fails loudly**, same availability as before but diagnosable.

Findings:

1. **HIGH — the floor can block legitimate pushes after a fail-after-tagging
   release run.** Tag creation happens mid-run in `prepare-release` (release POST
   creates the tag, `release.yml:2177` region and the Create-GitHub-release step),
   *before* the build matrix, asset upload, and cargo/npm publish jobs. If any of
   those later jobs fails, the tag exists while the run is failed. Every subsequent
   main push then computes FLOOR=<new tag commit> ∉ ancestors(LAST=<older success>)
   → `exit 1` at `release.yml:269-271` — including pushes whose own subjects are
   perfectly conventional and which `check-push-range LAST..HEAD` would have
   passed. Because subsequent runs keep failing *at this step*, they can never
   become the next success: main stays red until a human re-runs the failed run to
   success (transient failure) or removes/moves the tag (persistent failure). The
   comment's premise — "every tagged release was itself a successful Release push"
   (`release.yml:262-267`) — is false for exactly this window. A floor-*as-baseline*
   variant (if FLOOR is not an ancestor of LAST, validate `FLOOR..HEAD`, since the
   tag commit was itself validated by its merge run) cures the original incident
   without the deadlock. Recommend rework or an explicit recorded acceptance.
2. **MEDIUM — deviation from the governing ledger without recorded rationale.**
   Ledger O2/T11 prescribes validating subjects in the current push range
   (`event.before..SHA`), matching ci.yml; this commit keeps the gh-run baseline and
   adds the floor instead. The chosen design does cure the silent-widen failure
   mode, but the divergence from the written plan should be adjudicated in the
   ledger (one line), not left implicit.
3. **LOW — the new gate has not yet completed a CI push run.** Runs for
   `1cdcc897` (32876030637) and `dd441ae2` (32876308034) were cancelled; at review
   time `bd29447b`'s run was in progress. Logic above was verified by local
   dry-run only; the first completed push run should be eyeballed against scenario
   1's receipt.
   *Superseded 2026-08-25:* `bd29447b`'s run completed green and was the floor
   step's first real execution. Finding 1 (deadlock) is fixed by #664's clamp.

---

## Adjudication A0 — public project selectors vs the daemon-private route pin

**Decision: keep the private pin as the mechanism today; derive public selectors as
schema sugar over the same seam under the S0/A0 authority gate. Do not land either
"just add the field" or "delete the pin" alone.**

Both sides, from the code:

- What the pin actually does: the front-end proxy snapshots the connection's ACTIVE
  project (or explicit selector) and injects `_symforge_project_route_pin` for the
  nine selector-less tools (`src/protocol/mod.rs:1326-1347`), stripping any
  caller-supplied pin first (`mod.rs:1330`). The daemon accepts it only for the
  allow-listed tools, rejects combination with public selectors, requires a nonblank
  canonical id (`src/daemon.rs:4231-4278`), and resolves it through the ONE shared
  resolver with typed refusals and no home fallback (`daemon.rs:4487-4496`). Public
  selectors sent to these tools are refused outright (`daemon.rs:4348-4357`).
- The ledger's P0 framing ("repo-scoped tools can observe or mutate the wrong
  project") described the pre-pin race; the omission-race leg is closed — a
  concurrent ACTIVE switch between snapshot and execution lands on the canonical id,
  which is precisely what PR 1 demanded from `runtime_for_target`. Wrong-project
  outcomes now require an authorized caller deliberately naming another open
  project, which is explicit addressing, not a leak.
- What genuinely remains missing is expressiveness and uniformity: an MCP agent
  cannot target a non-ACTIVE open project with `conventions`/`inspect_match`/
  `checkpoint_now`/`detect_impact` without mutating session ACTIVE state (switch →
  call → switch back), a stateful three-step dance that misbehaves across retries.
  That is the surviving A0 gap, and the nine-tool allow-list is invisible to schema
  surfaces and surface tests.
- Race consequences of each option: adding public fields *without* the pin machinery
  would reintroduce snapshot/execute divergence; adding them *as* thin schema fields
  that the front-end maps onto the existing pin (or daemon-side
  `single_project_routed_tool` arm at `daemon.rs:4497-4534`) inherits the solved
  semantics. Deleting the pin in favor of "ACTIVE only" reopens the concurrent-ACTIVE
  hazard the comments document at `mod.rs:1320-1325`.
- Coverage debt found during adjudication: the four refusal arms of
  `take_private_project_route_pin` (wrong tool, combined-with-public-selector,
  non-string, blank/untrimmed — `daemon.rs:4257-4277`) have **no tests anywhere** — *retracted below*;
  only the happy binding path is pinned
  (`src/protocol/mod.rs:3228`, `:3628`). Whatever the decision, these negatives need
  RED-first tests before anything else touches the seam.

Sequencing recommendation: (1) add the missing negative tests now;
(2) fold public-selector schemas for the four tools into the same authority pass
that settles S0 (one atomic schema+dispatch+docs change), mapping onto the identical
resolver so single-project dispatch and cross-project merged execution keep their
existing contract lines (ledger PR 1 rules 3-7).

## Adjudication C0 / PR 3 — observe-only indexing and default-all search_text vs Feature 020

**Decision: the ledger's reading is CONFIRMED — both proposed behaviors contradict
frozen clauses; neither may ship before a derived successor contract is approved.**

- Observe-only indexing: frozen
  `source-binding-and-state.md:280-298` *requires* the shared `.gitignore` mutation
  after successful explicit normal `index_folder` binding (steps 1-6), and invariant
  14 (`:391-394`) binds index_folder and project-aware init to one byte-for-byte
  behavior matrix. Two tests on main assert the mutation
  (`src/protocol/tools.rs:17252`, `src/daemon.rs:8131ff`). Making indexing
  observe-only edits normative frozen behavior → blocked, exactly as the #660 note
  says. `ObserveOnly` (`src/gitignore_hygiene.rs:13`, used by
  `src/protocol/tools.rs:3701`) means the eventual code change is small; the gate is
  authority, not difficulty.
- Default-all search_text: frozen compatibility clause "Existing `search_text`
  remains code-scoped by default" (`specs/020-repository-knowledge-index/contracts/
  search-knowledge.md:246`); prose-in-code-results separation is restated across the
  contract's security/lane clauses. The unified-lane proposal (ledger §C) is a
  public routing/default change → CONTRACT PROPOSAL, not a ticket.
- What a derived amendment MUST contain (from the frozen tree's own authority text):
  1. citation of every replaced clause (at minimum
     `source-binding-and-state.md:264-298` steps 1-6 + invariant 14;
     `search-knowledge.md:244-251` compatibility lines; any lane/security clauses the
     quota model touches);
  2. an amendment record held *outside* the frozen tree (tasks.md forbids editing
     checkbox bytes or declaring supersession in place);
  3. a regenerated REFREEZE manifest, fresh attestation, exact-target review, and
     trusted signed external approval binding the exact target commit/tree
     (`specs/020-repository-knowledge-index/tasks.md:862-865`, T009-T012 shape);
  4. retention of the sound invariants the ledger already names: one resident index,
     `IndexTargets` targeting, secret filtering before publication, deterministic
     publication, no duplicate document store;
  5. authorization strictly before implementation — merging code first would be
     retroactive, which the frozen gate forbids.

---

## Findings outside the questions (mandatory section)

1. **Retracted 2026-08-25 — `take_private_project_route_pin` refusal arms untested.**
   All four refusal arms are driven by
   `daemon_private_route_pin_is_exact_allowlisted_and_conflict_free`
   (`src/daemon.rs`: non-allowlisted tool, blank, untrimmed, non-string, and pin
   combined with `project` / `projects`, each asserted `is_err()`), plus the
   end-to-end ACTIVE-override proof later in the same test module. Only the
   refusal message text is unpinned. Reviewer search error: index searches and
   message-literal greps cannot see `.is_err()` assertions; the test filter
   `cargo test private_route_pin` surfaces the test immediately.
2. **LOW — facade analytics silence**: `symforge_edit` records token summaries only
   (`tools.rs:11760-11761`), never a usage observation with a success flag, so M0-style
   net-accounting will systematically miss the compact edit path. Observation, not a
   reporting-invariant violation (no false success is claimed anywhere).
3. **LOW — `EditResultStatus` default fall-through is Success**: any future producer
   of an unrecognized failure shape silently becomes a false success
   (`src/protocol/edit_tools.rs:393-401`). The daemon-proxy path depends on shape
   recognition staying complete; consider an explicit unknown-shape tripwire in the
   darkness seal for new `format!` bodies under `src/protocol/edit*`.
4. **Note — release-please squash-body discipline** (Constitution, Build &
   Verification Constraints) continues to matter: the two legacy non-conventional
   subjects that motivated O2 sit below v11.0.0 and are excluded only because tags
   floor the range; any future tag-range regression re-exposes them.
5. **Process observation — cancelled Release runs on landing day** (32876030637,
   32876308034): two of the five reviewed commits' own release runs never completed.
   Whatever the cause (concurrency group churn), a landing-day convention of
   confirming the Release run green before stacking more pushes would have made
   finding dd441ae2-3 moot.

*End of findings.*
