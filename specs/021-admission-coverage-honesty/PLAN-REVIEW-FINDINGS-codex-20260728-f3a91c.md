# Plan review findings — Codex GPT-5 — 2026-07-28 — review `f3a91c`

**Intent under review:** If a competent engineer implements Feature 021 exactly as planned, all nine ledger defects plus the adopted index-identity defect should close with non-vacuous, reproducible evidence and without weakening the frozen sensitive-file invariant.

**Review status:** Complete.

### [BLOCKER] The SC-015 baseline can require legitimate security exclusions to become Tier 1
**Where:** `specs/021-admission-coverage-honesty/tasks.md:71-76,229-244`; `specs/021-admission-coverage-honesty/spec.md:298-312,450-452`
**Claim under test:** T102 inventories the demotion blast radius, and T120/SC-015 prove the root-cause fix by requiring every path in that inventory to return to Tier 1 without weakening the frozen `SensitivePath`/`SensitiveContent` invariant.
**What I found:** T102 asks for the total demoted population across all languages, broken down by every `MetadataOnlyReason`, and makes its explicit list the SC-015 baseline. T120 then requires **every path in T102's list** to be back at Tier 1. That list can contain genuinely sensitive paths/content, LFS pointers, unsupported encodings, platform collisions, or other legitimate Tier-2 dispositions. T121 simultaneously requires detector-positive sensitive files to remain metadata-only. An implementer cannot satisfy both instructions exactly.
**Why it matters:** Built literally, the gate either fails forever on legitimate exclusions or pressures the implementer to admit files the security contract requires withholding. It also makes T174 incapable of honestly closing SF-DOG-004.
**Suggested fix:** Split T102 into (a) the false-positive set attributable to the corrected detector rule, which must return to Tier 1, and (b) the legitimate-exclusion inventory, which must retain its honest non-Tier-1 reason. Rewrite T120 and SC-015 to assert those two opposite outcomes explicitly.

### [BLOCKER] ACH-02 cannot enforce the frozen security boundary from `format.rs`
**Where:** `specs/021-admission-coverage-honesty/tasks.md:47-48,83-93,251-317`; `specs/021-admission-coverage-honesty/plan.md:219-239`; `src/protocol/format.rs:3061-3100,3176-3225`
**Claim under test:** ACH-02 can safely honor Tier-2 `around_match` and chunk selectors entirely in `src/protocol/format.rs`, with no `tools.rs` edit, while preserving the rule that `SensitivePath` and `SensitiveContent` files are never lexically read.
**What I found:** `render_file_content_bytes(path, content, ContentContext)` receives bytes that have already been read and receives no admission disposition. T127-T129 only refactor and dispatch over those bytes; none of T122-T131 requires the caller to consult `MetadataOnlyReason` before reading. T104 explicitly says ACH-02's fallback scope depends on the full-file-demotion ruling, but no ACH-02 GREEN task consumes that ruling. The plan's deliberate “no edit to `tools.rs`” boundary removes the only cited caller seam at which a security disposition could prevent the disk read.
**Why it matters:** A generic Tier-2 selector implementation can service `around_match` for a `SensitiveContent` or `SensitivePath` entry exactly like a lockfile. That violates the feature's frozen, non-negotiable security invariant even though the happy-path fixture passes.
**Suggested fix:** Add a RED test for both security dispositions and a GREEN task that checks admission before raw bytes are loaded, returning a structured refusal. If that check must live in `src/protocol/tools.rs`, acknowledge the PR #479 dependency and sequence ACH-02 accordingly; a formatter-only change cannot prove the invariant.

### [BLOCKER] The planned detector correction cannot pass its own canary gate and has no real-secret oracle
**Where:** `specs/021-admission-coverage-honesty/plan.md:292-299`; `specs/021-admission-coverage-honesty/tasks.md:63-76,229-244`; `src/knowledge/mod.rs:89-95,130-157`
**Claim under test:** Adding a left word boundary and constraining the value class restores the falsely demoted files, recognizes `password={canary}` / `token={canary}` as placeholders, and does not create a genuine-secret false negative.
**What I found:** The current value class `[^\\s\"'#]{8,}` still matches `{canary}`: it is exactly eight allowed characters. The proposed left boundary also holds before `password` or `token` inside a quoted fixture. `is_placeholder` recognizes `${...}` and `{{...}}`, but not single-brace `{...}`. Therefore the described regex correction leaves at least `src/knowledge/mod.rs` detector-positive and Tier 2, so T120/T121 cannot pass as written. In the opposite direction, adding a boundary directly before `token`, `secret`, or `password` can stop matching common genuine identifiers such as `access_token`, `refresh_token`, or `db_password`; no task pins these positive cases.
**Why it matters:** The Phase 4 gate is unreachable with the specified change, while an improvised attempt to make it pass can create the exact sensitive-content false negative the frozen invariant forbids.
**Suggested fix:** Amend T101/T066's **fix scope**, not only its elimination list: require an explicit `{canary}` placeholder decision (`is_placeholder` or a deliberately equivalent rule change). Add RED tests for the repository canaries, ordinary-code false positives, and realistically shaped genuine assignments using prefixed/snake/kebab/camel credential identifiers. Choose the implementation from those bidirectional oracles.

### [HIGH] T153's four-language VERIFY is green on the behavior the plan says is broken
**Where:** `specs/021-admission-coverage-honesty/tasks.md:406-426,457-490`; `src/discovery/mod.rs:2204-2211`; `src/domain/index.rs:1392-1396`
**Claim under test:** A clean untracked `.ts`/`.js`/`.rs`/`.py` fixture followed by one full index fails before ACH-04 and turns green only after T149-T152.
**What I found:** The source contract says `SYMFORGE_EXCLUDE_UNTRACKED` is opt-in and off by default; clean recognized-extension files are not demoted merely because they are untracked. T143 exists precisely because the reproduced TypeScript file's actual exclusion reason is still unknown. Once Phase 4 corrects the detector false positive, ordinary clean fixtures should already be admitted by a full index. T153 nevertheless asserts that the pre-fix build returns an unqualified zero, without pinning any disposition that would make that true. It can therefore pass before T149-T152 land.
**Why it matters:** The principal VERIFY for SF-DOG-008 does not prove any Phase 7 fix, violating FR-026. It also hides the real split: clean-untracked admission may already work, while excluded-file diagnostics and `new_file=true` recovery remain broken.
**Suggested fix:** Make the clean four-language full-index case a baseline guard, not a RED oracle. Add separate failing cases for (1) an eligible file created *after* the index and admitted by one `new_file=true` call, and (2) a deliberately excluded non-security file whose search result is qualified and whose recovery guidance is honest. Bind any reproduction-specific test to T143's recorded reason instead of assuming “untracked” caused the exclusion.

### [MEDIUM] 021 duplicates two inherited WS5 implementation seams despite claiming one amendment only
**Where:** `specs/021-admission-coverage-honesty/tasks.md:63-70,152-188,220-244,433-479`; `specs/020-repository-knowledge-index/sift/tasks.md:234-245`
**Claim under test:** 021 references `T062-T082` without re-owning them and makes exactly one amendment, T101.
**What I found:** T111/T115 add the metadata-only planner disclosure and suppress the blind `search_symbols` recovery in `edit_plan.rs`; inherited T072-T074 already require the same typed metadata-only outcome and viable recovery in the same planner seam. T148/T152 add a qualified zero and coverage handling for excluded search matches in `tools.rs`; inherited T068-T071 already own the qualified-negative envelope and bounded non-security fallback for the same `search_text` path. These are implementation tasks, not merely extra acceptance checks.
**Why it matters:** The plan has two owners editing the same behavior in different phases. ACH-01 is called independent and lands before WS5C, so the later inherited task can rework it; ACH-04 can create a second special-case coverage path instead of consuming WS5B's trust envelope. That is exactly the drift risk the “reference by ID” rule was meant to prevent.
**Suggested fix:** Assign each shared seam once. Let ACH-01 own only the path-shaped cascade veto and consume/extend WS5C's metadata-only outcome; let ACH-04 consume WS5B's coverage envelope and own only untracked admission/recovery specifics. Alternatively, formally supersede the overlapping inherited tasks and amend the prerequisite ledger; do not keep the “T101 only” claim.

### [MEDIUM] Five VERIFY tasks do not run the behavior they claim to certify
**Where:** `specs/021-admission-coverage-honesty/tasks.md:114-132,194-203,306-317,387-400,481-491`
**Claim under test:** Every VERIFY names a runnable command and an assertion that fails without its fix.
**What I found:** T109 permits T107's test to live in `tests/impact_admission_consistency.rs` but runs only `cargo test --lib sidecar`, which cannot execute that integration test. T118 adds/repeats the `.js`/`.rs`/`.py` cases after T117's focused test run, then runs only fmt/clippy. T131 requires a new annotation/first-line assertion after T130's test run, then runs only fmt/clippy. T142 says to repeat the active-watcher reproduction but names no runnable command or test target. T154 likewise says to confirm reason equality and `new_file=true` behavior but names only fmt/clippy. Compilation and lint do not execute these assertions.
**Why it matters:** Each phase can reach its checkpoint while the newly added behavior is wrong or was never exercised. T174 can still list the intended assertion, so the exit table does not automatically expose the gap.
**Suggested fix:** Give each task an exact behavioral command after its assertions are added. Remove T107's integration-test alternative or include `cargo test --test impact_admission_consistency`; rerun the ACH-01 and ACH-02 integration targets after T118/T131; turn T142/T154 into deterministic integration tests or provide exact scripted reproductions with pass/fail predicates.

### [BLOCKER] D5 makes deliberately retained stale content authoritative over its current disposition
**Where:** `specs/021-admission-coverage-honesty/plan.md:241-252`; `specs/021-admission-coverage-honesty/tasks.md:355-385`; `specs/020-repository-knowledge-index/plan.md:128-158,345-355`; `src/live_index/store.rs:2675-2699`; `src/live_index/health_view.rs:264-309`
**Claim under test:** When `manifest_entries` and `files` disagree, checking `files` first is the safe single-oracle fix because the parsed record is “live” and the terminal manifest entry may be stale.
**What I found:** `publish_terminal_disposition_at_generation` deliberately retains the last parsed `files` entry only for `Unreadable` and `UnstableDuringRead`, while publishing the new terminal manifest entry; every other terminal disposition removes the parsed file. Thus, in the designed both-present state, the `files` entry is known stale and the manifest is current. `capture_admission_tier_lookup_view` is currently manifest-first. T139 instructs the implementation to reverse that order and report the retained file as `Tier 1` / `reason: None`.
**Why it matters:** Context and impact can be made to agree by returning the same false answer while the index knowingly serves last-valid rather than current content. SC-006 would pass precisely in the state it should expose.
**Suggested fix:** Make both-present disagreement a typed `stale_content` / `inconsistent_generation` outcome and retain the current terminal disposition; do not resolve it by files-first preference. Add a RED test that publishes `Unreadable` or `UnstableDuringRead` for an indexed path and proves the public response flags retained stale content rather than claiming clean Tier 1.

### [HIGH] No GREEN task makes the context side of SC-006 observable
**Where:** `specs/021-admission-coverage-honesty/spec.md:159-171,429-432`; `specs/021-admission-coverage-honesty/tasks.md:349-396`; `specs/021-admission-coverage-honesty/plan.md:127-150`
**Claim under test:** T134/T141 will prove that public `get_file_context` and `analyze_file_impact` responses expose the same tier, admission reason, byte size, project, and index generation.
**What I found:** ACH-03's GREEN tasks add a watcher outcome, change the admission lookup, and change `impact_skipped_text`. None targets the Tier-1 `get_file_context` renderer/envelope. The current Tier-1 context response does not expose a numeric project/generation stamp, admission reason, or byte size in a form that can be compared with impact. The plan's source-impact map likewise lists no protocol context formatter for ACH-03. T141's “transport parity” instruction concerns stdio versus serve impact behavior; it does not add the missing context fields.
**Why it matters:** T134 cannot assert the stated public contract, and SC-006 can remain false after every listed GREEN edit lands. An internal test that reads `LiveIndex` directly would not prove the two tool surfaces agree.
**Suggested fix:** Add an explicit GREEN task and source target for one shared admission/identity envelope rendered by both tools, then make T134 parse the public responses. If changing `get_file_context` is intentionally out of scope, narrow SC-006 to the invariant actually implemented and add a different public stale-generation assertion.

### [BLOCKER] Five inherited defects depend on 21 unchecked tasks that 021 never schedules
**Where:** `specs/021-admission-coverage-honesty/tasks.md:209-244,617-666,694-732`; `specs/020-repository-knowledge-index/sift/tasks.md:218-273`
**Claim under test:** Executing Feature 021 closes all nine ledger findings plus the adopted index-identity defect, while referencing SIFT WS5 by ID instead of duplicating it.
**What I found:** All of T062-T082 are unchecked. Phase 4 contains only three confirmation gates: T119 covers T062-T065, T120 covers T066, and T121 covers T070's decision. Nothing in 021 schedules or assigns T068-T080, and the plan has no gate for T068/T069/T071, T072-T074, T075-T077, or T078-T080. Nevertheless the exit table credits those tasks with closing SF-DOG-001 through SF-DOG-005; SF-DOG-003 has no native task or gate at all.
**Why it matters:** A competent engineer can complete every native implementation task, reach Phase 4 with the prerequisite still absent, and be unable to enter Phases 6/7 or produce five required closure receipts. “Reference by ID” prevents duplication; it does not create an owner or a schedule.
**Suggested fix:** Add one prerequisite engagement—not 21 restated tasks—that assigns and schedules T062-T082, gates the omitted WS5B-WS5D tasks, and defines a stop condition that leaves the affected SF-DOG rows explicitly open if WS5 does not land. Make the close-out gate rerun the inherited named tests rather than accept receipts by citation alone.

### [HIGH] ACH-01 can still resolve a nonexistent path to the wrong file after T114
**Where:** `specs/021-admission-coverage-honesty/tasks.md:152-188`; `src/protocol/edit_plan.rs:84-121`
**Claim under test:** Gating the zero-suffix cascade arm with `!path_shaped` makes a path-shaped target resolve literally or return a typed miss.
**What I found:** T114 gates only `collect_selector_hits`. In the same zero-match arm, `path.ends_with(target)` still assigns `file_hit`. For target `service.spec.ts` and indexed file `generator.service.spec.ts`, the guarded `"/service.spec.ts"` lookup finds nothing, then the bare suffix check selects the longer, wrong file. Because `file_hit` is non-empty, the typed miss at T115 is bypassed even if the symbol cascade is correctly vetoed.
**Why it matters:** The plan's only wrong-write workstream can still recommend mutation against a different file while all named T110/T112 cases pass.
**Suggested fix:** Make the path-shaped zero-match branch return the typed miss before **both** symbol and bare-file suffix fallbacks. Add a RED case where an indexed filename merely ends with the requested basename (`generator.service.spec.ts` vs `service.spec.ts`) and assert that no file or mutation recommendation is emitted.

### [HIGH] ACH-01's “real reason” is sourced through the collapse it claims not to depend on
**Where:** `specs/021-admission-coverage-honesty/tasks.md:161-188,209-228,661-664`; `src/live_index/query.rs:1237-1256`; `src/live_index/store.rs:3360-3380`
**Claim under test:** ACH-01 is independently landable and T111/T115 can disclose a Tier-2 path's real reason via `metadata_only_skipped_paths()`.
**What I found:** `metadata_only_skipped_paths()` calls `compatibility_admission_decision()` and stringifies its `SkipReason`. That conversion currently maps seven distinct metadata-only reasons—and three terminal dispositions—to `UnsupportedLanguage`. Before Phase 4 lands, a `SensitiveContent` path therefore becomes `metadata_only — reason: unsupported language`, not the real reason T111 requires. T111's “textually distinct from file_not_found” clause can pass on that false label.
**Why it matters:** The declared MVP either fails its own reason assertion or ships a fresh SF-DOG-004-style lie. The “no relationship to admission” sequencing rationale is factually false for the response contract as written.
**Suggested fix:** Keep the independent phase limited to a typed `metadata_only` versus `file_not_found` distinction, and gate the **honest reason** assertion on the reason-code work; or expose the uncollapsed manifest disposition directly. Do not call the compatibility projection “real.”

### [HIGH] Phase 4 blocks two P1 workstreams without the causal dependency claimed
**Where:** `specs/021-admission-coverage-honesty/tasks.md:251-259,323-332,617-654`; `src/live_index/health_view.rs:264-309`
**Claim under test:** Phase 4 must precede ACH-02 for fixture validity and ACH-03 because a real policy exclusion is indistinguishable from `reason: None`.
**What I found:** ACH-02 already requires a fixture that is Tier 2 by deliberate policy (lockfile or oversized data), so T066 cannot restore it; the phase is logically independent by the plan's own words. For ACH-03, `AdmissionTierLookupView.reason` is `Option<SkipReason>`: the manifest branch returns `Some(reason)`, while the parsed-file branch returns `tier: Normal, reason: None`. No reason-code split is needed to distinguish those branches or implement the typed publication-race outcome. The split is needed for ACH-04's specific reason truth, not for ACH-03.
**Why it matters:** Two P1 workstreams, including the HIGH post-edit honesty defect, are placed behind 21 unscheduled tasks for no executable benefit.
**Suggested fix:** Schedule ACH-02 independently using its deliberate-policy fixture. Make ACH-03 depend only on the shared size renderer. Keep ACH-04 behind the honest reason-code gate and strengthen its test to assert the fixture's expected specific disposition, not equality of two possibly collapsed strings.

### [HIGH] ACH-02's recommended refusal still emits an annotation claiming the refused mode
**Where:** `specs/021-admission-coverage-honesty/tasks.md:90-93,287-317`; `src/protocol/tools.rs:8532-8538,8588-8628`
**Claim under test:** `around_symbol` can be explicitly refused entirely inside `format.rs`, and T131 can prove the mode annotation is truthful with no `tools.rs` edit.
**What I found:** `tools.rs` constructs `── mode: symbol (explicit) ──` before the raw read and prepends it after `render_file_content_bytes` returns. A formatter-level refusal cannot suppress or relabel that caller-owned prefix. Under T105's recommended refusal, the response therefore asserts `mode: symbol` and then says the symbol mode was not serviced; SC-005 forbids exactly that. T131's proposed annotation/first-line-number assertion also has no defined refusal case because a refusal has no selected first line.
**Why it matters:** The plan precommits to “no tools.rs edit” while its preferred owner ruling makes that boundary impossible. The phase can satisfy selector safety but still fail its stated honesty criterion.
**Suggested fix:** Resolve T105 before scheduling ACH-02. If refusal wins, change the caller-owned annotation to distinguish `requested` from `honored` (or suppress it on refusal), add a refusal-specific assertion, and acknowledge the PR #479 dependency. If lexical substitution wins, define and test the changed semantics explicitly.

### [MEDIUM] The nonexistent-file recovery pointer currently points to an operation that returns 404
**Where:** `specs/021-admission-coverage-honesty/tasks.md:181-188`; `src/sidecar/handlers.rs:941-975`
**Claim under test:** A `file_not_found`/new-file plan pointing at `analyze_file_impact(new_file=true)` is actionable recovery.
**What I found:** `handle_new_file_impact` calls `admit_and_index_single_path`; `NotFound` and `Removed` return HTTP 404. Therefore invoking the suggested operation while the path is still nonexistent cannot create it, index it, or produce a plan. The pointer becomes actionable only after some separate operation creates the file, which T115 does not say.
**Why it matters:** ACH-01 correctly closes the wrong-write path but can replace it with a dead-end recovery instruction, contrary to the review requirement that refusal be paired with actionable recovery.
**Suggested fix:** State the required order explicitly: create the file through the normal edit/create path, then call `analyze_file_impact(new_file=true)` to admit and index it. If a supported creation tool should be recommended, name it and test the full sequence; otherwise return a typed refusal without pretending the analysis call creates files.

### [MEDIUM] ACH-05 pre-implements every branch before its investigation decides which branch exists
**Where:** `specs/021-admission-coverage-honesty/tasks.md:509-584`; `src/sidecar/port_file.rs:274-320`
**Claim under test:** T155 determines whether descriptor hygiene suffices or registration is unreliable, after which ACH-05 applies the proportional fix.
**What I found:** T155 explicitly says its answer decides between a small hygiene fix and a materially larger registration fix, and says `project_root: None` may be latent. Yet T158-T166 already mandate absent-root rejection, legacy fallback changes, comparator consolidation, hook root behavior, live-session validation, pruning, and legacy migration regardless of the answer. The liveness wording is also imprecise: descriptor selection already probes each port and records `Alive`/`Dead`; the missing behavior is enforcement before returning the selected port.
**Why it matters:** The largest native workstream mixes the reproduced foreign-count defect with several unobserved hardening branches and one discovery task whose result cannot change the prewritten plan.
**Suggested fix:** Land the reproduced core (caller-root gating, response identity stamp, deterministic mismatch test) first. Make descriptor, registration, comparator, and legacy cleanup follow-ups conditional on T155's evidence. Reuse the existing liveness result and test that a synthetic dead descriptor is not returned; do not add a second probe.

### [MEDIUM] T112's `Foo::bar` half cannot detect the proposed veto regression
**Where:** `specs/021-admission-coverage-honesty/tasks.md:166-180`; `src/protocol/edit_plan.rs:10-20,47-67,84-121`
**Claim under test:** `Type.Method` and `Foo::bar` form a bidirectional guard against an over-broad path-shaped veto.
**What I found:** `Foo::bar` is split by `split_path_qualified_target` and takes the qualified branch before the path-shaped predicate or suffix-match block. It passes identically before and after T113/T114. The meaningful boundary is an unqualified symbol containing a dot whose tail resembles a known extension; the proposed predicate can classify that as a path and fail closed.
**Why it matters:** Half of the named regression guard is structurally incapable of exercising the change, and `Type.Method` may also miss the known-extension-tail boundary depending on the extension set.
**Suggested fix:** Keep `Foo::bar` as a general regression guard but stop crediting it as proof of this veto. Add cases such as `Config.go`, `Model.py`, `Node.rs`, or another actual symbol whose suffix is in the known-extension set, and prove the predicate distinguishes it from a literal file path.

### [HIGH] T119's stated “mechanical proof” is green before the reason split
**Where:** `specs/021-admission-coverage-honesty/tasks.md:220-228`; `src/live_index/store.rs:3360-3410`
**Claim under test:** `cargo check` exhaustiveness mechanically proves every admission-reason arm was split and the reverse mapping round-trips totally.
**What I found:** The forward mapping is already exhaustive because seven `MetadataOnlyReason` variants share one `|`-joined arm, and three terminal dispositions share another. `cargo check` is green with the full collapse and remains green if a partial fix leaves one variant in a collapsed arm. The cited reverse range also stops before `AdmissionTier::Normal => MetadataOnly(UnsupportedTextEncoding)`, an obviously non-round-tripping arm at lines 3407-3409.
**Why it matters:** The gate that is supposed to establish honest reason codes can certify the pre-fix behavior or a partial split. Downstream same-string checks can then agree on the same false label.
**Suggested fix:** Replace exhaustiveness-as-proof with data assertions: enumerate every source disposition, require the intended distinct `SkipReason`/display value, and assert the supported forward/reverse pairs explicitly. Include the `Normal` arm in the test and either make its conversion lawful or define why it is intentionally excluded from round-trip claims.

## Independent-review reconciliation

Three independent adversarial lenses (skeptic, architect, minimalist) were used as hypothesis generators, not as votes. Claims accepted into this report were rechecked against the review artifacts and the cited source. Four proposed claims were deliberately narrowed or rejected:

- The D5 `SensitiveContent` coexistence example was rejected: normal publication removes parsed bytes for that disposition. The accepted defect uses the actual both-present state, where `Unreadable`/`UnstableDuringRead` deliberately retains stale parsed content.
- A proposed mixed-version `HealthResponse` deserialization blocker was rejected: the hook path proxies the raw health body and does not deserialize that struct at the cited boundary.
- PR #479 was not reported as a newly discovered blocker because the plan already records it as ACH-04's hard precondition; only contradictions that extend its impact are findings.
- “Add a liveness probe” was narrowed to “enforce the existing liveness result”: descriptor selection already probes and records `Alive`/`Dead`.

## Verdict

VERDICT: FIX-FIRST (5 blockers)

The RED→GREEN→VERIFY workstream shape is salvageable, but implementation should not start from this revision. Correct the SC-015 population, frozen-invariant enforcement, detector/canary oracle, D5 authority rule, and inherited-WS5 ownership first; then repair the sequencing and non-vacuous proof gaps identified above.

**Crux:** The plan assumes its Phase 4 prerequisite will both arrive and prove honest admission recovery, yet 021 neither schedules that prerequisite nor specifies a detector correction capable of passing Phase 4's own canary gate.

**Confidence and limits:** High confidence in the document/source consistency findings: all required 021 artifacts, the external ledger, SIFT WS5 tasks, the current Feature 020 plan, and the cited implementation paths were inspected. `src/protocol/tools.rs` was metadata-only in the live index, so its narrowly cited ranges were read through SymForge's explicit raw-content fallback. No build, test suite, live curl reproduction, or implementation mutation was run because this was a read-only plan review. The adversarial-review package's referenced `brain/principles.md` was unavailable; all three reviewer lenses otherwise completed, and their claims were independently adjudicated as described above.
