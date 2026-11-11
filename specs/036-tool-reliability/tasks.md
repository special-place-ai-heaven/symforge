# Tasks: Tool reliability, correctness floor

**Input**: `specs/036-tool-reliability/` (spec.md, plan.md, research.md, data-model.md, contracts/tool-outcome.md, quickstart.md)

Rules for every slice:

- Tests come first and are seen failing; the Terminal Commander job id of the failing run goes in the commit body.
- Every refusal test carries an accepting control in the same test.
- One cargo process at a time. Gates are the list in quickstart.md.
- One slice is one PR. A slice starts when the previous one has passed its gates. Each pauses for the owner's approval before merge.
- After `rustfmt`, refresh the source pin in `tests/preventive_runtime_dark_v11.rs` from its own oracle output. Regenerate only goldens the change explains and list them in the commit.
- No `[P]` on tasks that edit `src/protocol/tools.rs`.

## Phase 1: Setup

- [ ] T001 Create the fixture crate `tests/fixtures/reliability/` with an answer table `tests/fixtures/reliability/ANSWERS.md`: `src/lib.rs`, `src/parent/mod.rs` (declares `mod leaf;`, calls `leaf::target_fn` and `super::` paths), `src/parent/leaf.rs` (defines `target_fn`, calls it locally, has a local shadowing the name in one function), `src/other/leaf.rs` (same module leaf name, different module), `src/user.rs` (imports `parent::leaf::target_fn`), `src/unrelated.rs` (calls `.target_fn()` on its own type; mentions the name in a comment and a string), `src/long.rs` (300 lines), `src/short.rs` (8 lines), `docs/notes.md` (mentions a term absent from code). Add `.ignore` entries so the fixture does not index its own goldens.
- [ ] T002 Add `tests/reliability_support/mod.rs` with a helper that indexes the fixture and calls a tool through `dispatch_tool_for_tests` (`src/protocol/mod.rs:1920`), returning text and `_meta`.

## Phase 2: Foundational

- [ ] T003 Create `src/protocol/tool_contract.rs` with the `ToolContract` type from data-model.md (conditions, primary, optional sections in drop order, explicit arguments, typical call) and an empty static table; add a test in the same file that every name in `SYMFORGE_TOOL_NAMES` (`src/cli/init.rs`) has exactly one contract. The test fails until T004.
- [ ] T004 Fill the table in `src/protocol/tool_contract.rs` for all registered tools, reading each handler for its real primary content and sections; T003's test passes.

## Phase 3: A1 Output, User Story 2 (P1)

**Goal**: what the caller asked for is in the first answer. **Independent test**: `tests/reliability_output.rs` plus quickstart scenarios 4, 5, 6, 10, 11.

- [ ] T005 [US2] Write failing tests in `tests/reliability_output.rs`: 75-line range of `src/long.rs` returns 75 numbered lines with no budget argument; range 1-20 of `src/short.rs` returns 8 lines and states the last line; symbol body of a function longer than the default is whole; file context of `src/parent/leaf.rs` contains its outline, imports and consumers; symbol context names every caller and callee; repository map with defaults contains `src/`; map with `path="src", depth=1` lists only that level; multi-word symbol query returns all-words matches and says so; text search with matches only in `docs/` reports their count and the revealing call; `what_changed` path count equals `git status --porcelain`; impact levels are not all equal. Record the failing job id.
- [ ] T006 [US2] Trace the symbol-context cut seen at about 376 tokens (start at `src/index_lifecycle/guidance/symbol_context.rs:239` and `src/protocol/tools.rs:2574`) and write the cause into `specs/036-tool-reliability/research.md` D4.
- [ ] T007 [US2] Add a sectioned budget function beside `enforce_token_budget_flagged` in `src/index_lifecycle/guidance/source.rs`: takes primary text, optional sections in drop order and a budget; drops sections last-first; never cuts primary below the hard cap; returns what was dropped. Unit tests in the same file.
- [ ] T008 [US2] In `src/protocol/tools.rs` remove the input-size-derived output budget at lines 1934, 2357, 6007 and 6140 (`resolve_read_max_tokens(None, raw_chars)` used as an output cap) and route those handlers through T007 using each tool's contract; an explicit `max_tokens` from the caller still applies.
- [ ] T009 [US2] In `src/index_lifecycle/guidance/file_read.rs` make explicit ranges exempt from the default budget up to `GET_FILE_CONTENT_MAX_BYTES`, number lines by default on ranged reads, and append the last-line note when the range passes the end of the file; past the hard cap return the first part and the literal continuation call.
- [ ] T010 [US2] In the repository map handler (`src/protocol/tools.rs:2003`) rank source roots, then tests, then documentation and specification directories, and honour `path` and `depth` in every detail level; correct the doc comments in `src/protocol/read_tools.rs:279-284`.
- [ ] T011 [US2] Symbol search fallback: when a multi-word query has no exact match, return symbols containing all words, labelled, in the handler in `src/protocol/tools.rs` and its query in `src/live_index/query.rs`.
- [ ] T012 [US2] Text search zero-hit: count matches in documentation, test and policy-withheld files and print the count with the call that shows them (`src/live_index/search.rs`, formatter in `src/protocol/format.rs`).
- [ ] T013 [US2] `what_changed`: count untracked directories the way `git status --porcelain` does (`src/index_lifecycle/guidance/changes.rs`). `detect_impact`: rank by distance and reference count so levels differ (`src/index_lifecycle/guidance/impact.rs`).
- [ ] T014 [US2] Apply the same output changes to the embed lane where it shares the guidance functions (`src/embed/parity/embed_read.rs`, `embed_symbol.rs`, `embed_reference.rs`); regenerate the embed goldens and harness snapshots that the change explains.
- [ ] T015 [US2] Run the gates in quickstart.md; check tools/list byte budgets still hold; independent review with cfg sweep; commit, push, open the PR.

## Phase 4: A2 Request contract, User Story 3 (P2)

**Goal**: no argument is silently ignored. **Independent test**: `tests/reliability_contract.rs`, quickstart scenario 7.

- [ ] T016 [US3] Write failing tests in `tests/reliability_contract.rs`: for every registered tool, a call with an extra key `zz_unknown` is refused, names the key and lists valid arguments, and the same call without it succeeds; a nested unknown key inside `edits[]` is refused; the serde aliases `symbol` and `body` are accepted; each condition in the contract table is refused or selects its mode outside its mode; `sections=[]` on `get_symbol_context` keeps its published meaning and no longer demands `path` wrongly; a refused call does not advance the repeat-notice counter. Record the failing job id.
- [ ] T017 [US3] Add `serde_ignored` to `Cargo.toml`; implement `decode_strict::<P>` in `src/protocol/tool_contract.rs` returning the decoded value or the list of unconsumed paths.
- [ ] T018 [US3] Read `vendor/rmcp/src/handler/server/wrapper/parameters.rs` and wire the server lane to `decode_strict` through a SymForge parameter wrapper used in handler signatures (preferred) or a recorded vendor patch (`vendor/rmcp/SYMFORGE-PATCH.md`); the refusal is produced before `RepeatKey::observe` (`src/protocol/mod.rs:2243`) and lists valid arguments from `self.tool_router.get(..)`.
- [ ] T019 [US3] Use `decode_strict` in the daemon lane's `decode_params` (`src/daemon.rs:7588`), after its internal alias translation.
- [ ] T020 [US3] Implement the mode-condition check from the contract table (select the mode or refuse naming it) and bring the doc comments at `src/protocol/read_tools.rs:162, 279, 281, 284, 361` and `src/protocol/tools.rs:416` in line with the code; add a test that compares the doc phrases with the table.
- [ ] T021 [US3] Align the embed lane's unknown-field refusal text with the server lane (`src/embed/parity/*.rs`) and pin it in `tests/embed_parity.rs`.
- [ ] T022 [US3] Check this repository's own callers send no ignored arguments: hook scripts, `npm/`, `scripts/verify-tools.cjs` cases, `src/cli/init.rs` templates. Fix any that do.
- [ ] T023 [US3] Gates, tools/list byte budgets, independent review with cfg sweep, commit, push, PR. The PR body states the behaviour change for clients that send unknown arguments.

## Phase 5: A3 Disk verification, User Story 4 (P2)

**Goal**: trust statements are about this answer. **Independent test**: `tests/reliability_freshness.rs`, unit tests beside `StableReadAccess`, quickstart scenarios 8 and 9.

- [ ] T024 [US4] Write failing tests in `tests/reliability_freshness.rs`: a file edited outside the tool so it gains a match is found by the next text search; a file edited within the same second with the same size is detected by content; a stale unreadable record for file A leaves a query on file B without any freshness line; an empty result while observations are lost carries the FR-017 line and a non-empty one does not; health names each unreadable file with its cause. Record the failing job id.
- [ ] T025 [US4] Write failing unit tests in `src/live_index/store.rs` beside `StableReadAccess` (line 733) injecting: sharing violation then success (retried), delete-pending, vanish between stat and read, an unknown raw code (reported verbatim, outcome cannot determine), and a call touching many failing files that stays within the 1-second call budget.
- [ ] T026 [P] [US4] Create `src/discovery/os_response.rs`: the table from research D6 and the in-memory class type; extend the full-error classifier at `src/paths.rs:267` to consult `raw_os_error()` through the table. `AccessErrorKind` (persisted) is not changed; add the mapping down to it.
- [ ] T027 [US4] Capture the NT status at the failing call inside the access layer on Windows (`extern "system" fn RtlGetLastNtStatus`, `cfg(windows)`), carry it with the error, and add the parent-listing probe for access denied.
- [ ] T028 [US4] Move the callers of `discovery::access_error_kind` (`src/discovery/mod.rs:1220, 1288, 1528, 2318-2342`, `src/live_index/store.rs:861, 889`) to the classifier; add the per-call retry budget for transient classes.
- [ ] T029 [US4] Extend `freshen_file_if_stale` (`src/live_index/single_file.rs:1064`): compare size as well as modification time; classify a failed `stat` instead of returning `Fresh` (line 1099); add the in-memory `indexed_at` beside `IndexedFile` (`src/live_index/store.rs:953`, not serialized) and compare by content hash inside the racy window or when `indexed_at` is absent.
- [ ] T030 [US4] Add the per-answer check in `src/index_lifecycle/guidance/freshen.rs`: freshen the files of the answer's rows before the response generation is captured, re-run the query once if any were stale; for empty and complete claims check every file in scope up to a file-count bound and report the uncompared count beyond it.
- [ ] T031 [US4] Derive the envelope from the files checked in the call (`src/index_lifecycle/guidance/search_envelope.rs:40` and its callers `embed_reference.rs:346, 482`, `embed_symbol_context.rs:650`, `changes.rs:464`, `file_search.rs:1032`); keep the lost-observation line only on empty and complete claims.
- [ ] T032 [US4] Health output names each unreadable file and its classified cause, and counts unknown OS codes (`src/protocol/tools.rs` health handler, `src/live_index/store.rs:3292`).
- [ ] T033 [US4] Regenerate the goldens and snapshots that lose the blanket trust line; gates; independent review with a cfg sweep that lists the `cfg(unix)` rows as unverified until Linux CI; commit, push, PR.

## Phase 6: A4a Resolver and references, User Story 1 (P1)

**Goal**: nothing that matches is silently dropped. **Independent test**: `tests/reliability_resolver.rs`, quickstart scenario 1.

- [ ] T034 [US1] Write failing tests in `tests/reliability_resolver.rs` against `ANSWERS.md`: every occurrence of `target_fn` in the fixture appears in the resolved set with the expected class and rule (import, qualified `super::` path, module declaration, same-file unshadowed, the shadowed local as probable, the method on another type as probable, comment and string as excluded); references scoped to the defining file are a subset of the unscoped set and the removed sites are counted; no completeness line while an unresolved site exists; the occurrence set equals a ripgrep run filtered by the same boundary rule, on the fixture and on this repository for a sampled identifier list. Record the failing job id.
- [ ] T035 [US1] Create `src/live_index/resolve.rs`: the occurrence scan (case-sensitive, uncapped, byte offsets, per-language-family identifier boundaries), `CandidateSite`, `ResolvedSet` and the class enum from data-model.md.
- [ ] T036 [US1] In `src/live_index/resolve.rs` join occurrences to `ReferenceRecord` by byte range and implement the confirmation rules for Rust (import path, qualified path normalisation through `self::`, `super::`, `crate::` and re-exports, module declaration, indexed reference, same-file unshadowed) and the probable rules; other languages confirm through import records and are otherwise probable.
- [ ] T037 [US1] Comment and string exclusion for un-joined occurrences: re-parse only files that have them, cache per index generation; measure a common name (`new`) on this repository and record the timing in `specs/036-tool-reliability/research.md` D3 before T038.
- [ ] T038 [US1] Make `find_references` a view of the resolved set (`src/protocol/tools.rs:6308`, `src/live_index/query.rs:2184-2199` and `:2936`), with the listing shape from contracts/tool-outcome.md and test or non-test labels.
- [ ] T039 [US1] Apply the same to the embed reference lane (`src/embed/parity/embed_reference.rs`); regenerate explained goldens.
- [ ] T040 [US1] Gates, bench smoke, independent review with cfg sweep, commit, push, PR.

## Phase 7: A4b Dependents, User Story 1 (P1)

**Independent test**: quickstart scenario 2.

- [ ] T041 [US1] Write failing tests in `tests/reliability_resolver.rs`: dependents of `src/parent/leaf.rs` include `src/parent/mod.rs` and `src/user.rs` and exclude the file that imports `other::leaf`; a file with no dependents returns the proven-empty text stating what was searched. Record the failing job id.
- [ ] T042 [US1] Rewrite `find_dependents_for_file` (`src/live_index/query.rs:3221`) as a view of the resolved sets of the file's exported symbols plus module declarations; delete `matches_target_stem` (`query.rs:296`) if it has no other caller.
- [ ] T043 [US1] Update the handler and formatter (`src/protocol/tools.rs:6646`), file context's consumers section, and the embed lane; regenerate explained goldens.
- [ ] T044 [US1] Gates, independent review, commit, push, PR.

## Phase 8: A4c Rename, impact, edit planning, User Story 1 (P1)

**Independent test**: quickstart scenario 3.

- [ ] T045 [US1] Write failing tests in `tests/reliability_resolver.rs`: a rename dry run of `target_fn` lists exactly the confirmed references as "will change" and the probable and unresolved sites as "left alone"; applying it changes only those sites and the fixture still compiles (`cargo check` on the fixture copy); the method `.target_fn()` on the other type is untouched; edit planning with a path pins one definition among same-named ones. Record the failing job id.
- [ ] T046 [US1] `batch_rename` takes its sites from the resolved set and its write path accepts only the confirmed payload (`src/protocol/edit_tools.rs:2167`, `src/edit_safety/rename.rs`).
- [ ] T047 [US1] `edit_plan` accepts `path` and reads the resolved set (`src/protocol/tools.rs:6922`, `src/protocol/edit_plan.rs`); `detect_impact` and `get_symbol_context` read it too (`src/index_lifecycle/guidance/impact.rs`, `symbol_context.rs`).
- [ ] T048 [US1] Embed lane for the same tools (`src/embed/parity/`), `tests/embed_edit_plan.rs`, `tests/embed_file_impact.rs`, `IMPACT_EDIT_GOLDEN`; regenerate explained goldens.
- [ ] T049 [US1] Gates, independent review with a separate security-minded read of the write path, commit, push, PR.

## Phase 9: A5 Outcome, User Story 5 (P3)

**Independent test**: `tests/reliability_outcome.rs`.

- [ ] T050 [US5] Write failing tests in `tests/reliability_outcome.rs`: each tool driven into each outcome it can produce carries `_meta["symforge/outcome"]` with the fields of its kind; `symforge/result_status` keeps contract version 1 and the mapping in contracts/tool-outcome.md; for every tool the typical call from its contract contains its declared primary content (FR-038). Record the failing job id.
- [ ] T051 [US5] Add the outcome type and the `symforge/outcome` key in `src/protocol/result_status.rs`; fill the version 1 status from it in `into_call_tool_result`.
- [ ] T052 [US5] Move handlers that return a bare string onto the statused result path, one group per commit (read tools, search tools, edit tools, health and lifecycle) in `src/protocol/tools.rs` and `src/protocol/edit_tools.rs`.
- [ ] T053 [US5] Embed lane and daemon proxy pass the new key through (`src/embed/parity/`, `src/daemon.rs`).
- [ ] T054 [US5] Gates, independent review, commit, push, PR.

## Phase 10: Acceptance

- [ ] T055 Install the release build of the branch as the MCP server and re-run the 13 failing tasks from `specs/036-tool-reliability/inputs/adoption-sweep-2026-11-11.md` with the same prompts and a contextless agent; record calls-to-answer, built-in tool use and answer size per task in `docs/reviews/2026-11-tool-reliability/acceptance.md` against SC-001 and SC-008.
- [ ] T056 Update `README.md` and the wiki pages for the changed behaviours (refusals, listing classes, outcome key) after grepping `tests/` for pinned phrases; list for the owner the lines of the project `CLAUDE.md` that the change makes stale.

## Dependencies

Setup and Foundational first. Then A1 → A2 → A3 → A4a → A4b → A4c → A5 → Acceptance, strictly in order: they share `src/protocol/tools.rs` and the goldens. Within a slice, the only parallel task is T026 (new file).

## Implementation strategy

A1 alone removes the most frequent cause of extra calls and is the first PR. A1 to A3 need no new index machinery. A4a is the largest piece of new code and is measured (T037) before any consumer depends on it.
