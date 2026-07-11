# SymForge v8 Architecture Review

## Plan

- [x] Checkout and confirm `v8/stel-architecture`.
- [x] Index the repo with SymForge and check project memory.
- [x] Read `docs/v8-bootstrap.md` fully.
- [x] Inspect §10 code paths and verify gap-vs-reality claims.
- [x] Read binding linked specs needed for §13.
- [x] Check `src/stel/` pre-flight invariant.
- [x] Synthesize concrete architecture findings and answer §13.
- [x] Document review results here.

## Evidence Log

- Branch confirmed as `v8/stel-architecture` after `git fetch origin`, `git checkout v8/stel-architecture`, `git pull`, and `git branch --show-current`.
- SymForge initially reported an empty index, then indexed `E:\project\symforge`: 250 files, 11750 symbols.
- Working tree had an existing local modification to `docs/v8-bootstrap.md`; this review treats that working-tree version as the active brief and does not overwrite it.
- agentmemory recall surfaced one relevant prior lesson: generation fences and cancellation are required when long-lived async state can outlive a project/session identity change.
- `Cargo.toml` still has `rmcp = { version = "1.1.0", features = ["transport-io"] }`; no Streamable HTTP feature is enabled.
- `src/main.rs` still chooses daemon-backed stdio first and falls back to local stdio plus a separate HTTP sidecar.
- `src/protocol/tools.rs` and `src/protocol/edit_tools.rs` still expose the legacy 32-tool router; `src/protocol/smart_query.rs` and `ask` route directly to one core tool, not to a STEL plan/controller.
- `src/stel/` does not exist, which is correct because `docs/v8-gap-closure-plan.md` §12A is still not fully green.

## Review

- Complete: [`docs/reviews/v8-architecture-review-codex-resume.md`](../docs/reviews/v8-architecture-review-codex-resume.md)
- Verdict: design sound; Phase 0 §12A is the blocker; proceed only through harness/golden file before `src/stel/`.
- Net-new gaps: G-032..G-036 in gap-closure plan (from Codex addendum).

---

# Phase 0 pre-flight (§12A)

## Plan

- [x] `compare-results.js` with `--preflight` (sf-bench commit `16acb4b`)
- [x] `routes.golden.jsonl` 36-row skeleton + `fixtures/preflight-minimal.json`
- [x] `scripts/measure-schema-bytes.ps1` stub (symforge `f7af058`)
- [ ] Human review of golden `expected_decision` / `expected_equiv` (≥10 rows)
- [ ] A-001..A-004 validated on real battery output
- [ ] `battery.js` emits v8 row fields (`decision`, `acceptedServe`, …)
- [ ] A-012 two-hop bypass harness
- [ ] A-005 / A-019 / A-025 validated

## Run

```powershell
cd E:\project\sf-bench
node compare-results.js --preflight --release 8.0
cd E:\project\symforge
.\scripts\measure-schema-bytes.ps1
```

## Review

- Preflight gate script verified (synthetic fixture passes H1–H5, H7).
- Still blocked on real harness trust + golden semantics before `src/stel/`.

---

# Init All-Client Durable Binary Failure

## Plan

- [x] Reproduce/trace the CI failure from the supplied panic.
- [x] Identify where the temporary binary guard loses the injected test home.
- [x] Patch `run_init_with_context` so all client branches use the same injected home context.
- [x] Run the focused failing init integration test.
- [x] Run format/check verification.

## Evidence Log

- Failure: `test_run_init_all_updates_both_clients` panics because the Claude Desktop branch refuses `/tmp/.../.symforge/bin/symforge` and asks for `/home/runner/.symforge/bin/symforge`.
- Root cause: `run_init_with_context` resolves the registration binary with the injected `home_dir`, then calls `register_claude_desktop_mcp_server`, whose public wrapper re-reads `dirs::home_dir()` and re-applies the temporary-binary guard against the real CI home.
- Added regression assertions that all-client init writes Claude Desktop config under the injected home, points it at the injected durable binary directory, and does not persist the temporary extraction binary. Before the fix, the focused test failed on Windows because the config was written to `%APPDATA%\Claude\claude_desktop_config.json`.
- Fix: split production path construction (`InitPaths::from_current_environment`) from injected path construction (`InitPaths::from_home_and_working_dir`), and route `run_init_with_context` through `register_claude_desktop_mcp_server_with_home`.
- Focused verification passed after the fix: `cargo test --test init_integration test_run_init_all_updates_both_clients -- --nocapture`.
- Inspected the real Claude Desktop config after the earlier test pollution. Current `symforge` points at the durable `C:\Users\rakovnik\.symforge\bin\symforge-desktop.cmd`; the available May 19 backup already had a temporary SymForge wrapper entry, so deleting or reverting the entry would be destructive and less correct.
- `cargo fmt --check` initially failed on one rustfmt wrapping change; `cargo fmt` applied it and the latest rerun passed.
- `git diff --check` passed with CRLF conversion warnings only.
- `cargo check` passed.
- `cargo test --test init_integration test_run_init_all_updates_both_clients -- --nocapture` passed with the JSON-parsed Claude Desktop command assertions.
- `cargo test --test init_integration -- --nocapture` passed: 24 passed, 0 failed.
- `cargo test --all-targets init -- --test-threads=1` first hit local disk exhaustion while writing `target/debug` artifacts. After removing the generated repo-local `target/debug/incremental` cache, the latest rerun passed: 95 selected tests passed across all targets, 0 failed.

## Review

- The CI failure was a real implementation bug, not a flaky test: all-client init lost the injected home only when it reached Claude Desktop registration. The fix preserves production `%APPDATA%` behavior while making the injected test path deterministic and isolated.

---

# SFB10 - Apply result-status semantics to read, search, and reference tools

## Plan

- [x] Run Branch Guard from the original checkout.
- [x] Switch to `.worktrees/backlog-implementation` and confirm branch/status there.
- [x] Index the target worktree with SymForge and check agentmemory for prior context.
- [x] Copy the SFB10 goal file into the worktree and mark it `In progress`.
- [x] Validate SFB09 dependency artifacts in current code because the SFB09 goal file is absent from this worktree.
- [x] Inspect current read/search/reference response construction and status contract helpers.
- [x] Add or update contract tests for found, not_found, ambiguous selector, invalid request, and empty/no-match states across read/search/reference surfaces.
- [x] Apply result-status metadata to `get_symbol`, `get_file_content`, `search_*`, and `find_references` while preserving existing human-readable text.
- [x] Capture before/after sample output for one found and one not-found response.
- [x] Run exact goal verification:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo test --all-targets -- --test-threads=1`
  - `rg "result_status|ResultStatus|outcome_class" src/protocol src/live_index tests`
- [x] Run default full verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Update SFB10 frontmatter to `Completed` with the verified work commit hash.
- [x] Commit the SFB10 goal-status update separately.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- The SFB10 goal file was absent from the worktree and was copied from the original checkout per Branch Guard.
- SymForge indexed the worktree: 191 files, 9292 symbols.
- agentmemory recall for SFB10/result-status/read-search-reference context returned no matching prior observations.
- `tasks/lessons.md` is absent in this worktree; no prior lesson file was available to review.
- SFB09 dependency file is absent, but dependency artifacts are present in code: `src/protocol/result_status.rs`, `src/protocol/mod.rs`, `tests/conformance.rs`, and the existing read-tool fixture in `src/protocol/tools.rs`.
- Goal status changed to `In progress` at `2026-05-20T14:40:39.2214133+02:00`.
- Implementation finding: the public RMCP router can return `CallToolResult`, while the existing tool bodies return human-readable `String` used by daemon/proxy/internal tests. The lowest-impact path is statused wrapper methods registered under the same tool names, keeping existing text renderers unchanged.
- Added status classifiers and registered wrappers for `get_symbol`, `get_file_content`, `search_symbols`, `search_text`, `search_files`, and `find_references` in `src/protocol/tools.rs`.
- Added contract tests covering:
  - read: `found`, `not_found`, `invalid_request`, `ambiguous`;
  - search: `found`, `empty_result`, `invalid_request`, `not_found`, `ambiguous`;
  - references: `found`, `empty_result`, `not_found`, `ambiguous`.
- Initial focused red check: `cargo test result_status_contract -- --nocapture` failed before wrappers because tool responses had no `_meta["symforge/result_status"]`.
- Focused verification passed:
  - `cargo test result_status_contract -- --nocapture`: 3 passed, 0 failed.
  - `cargo test test_get_file_content -- --test-threads=1`: passed.
  - `cargo test test_search -- --test-threads=1`: passed.
  - `cargo test test_find_references -- --test-threads=1`: passed.
  - `cargo test test_get_symbol -- --test-threads=1`: passed.
- Found sample:
  - before SFB10 wrapper: `content[0].text = "fn present() {}\nfn duplicate() {}\nfn duplicate() {}\n"`;
  - after SFB10 wrapper: same `content[0].text`, plus `_meta["symforge/result_status"] = {"contract_version":1,"outcome_class":"found"}`.
- Not-found sample:
  - before SFB10 wrapper: `content[0].text` begins `File not found: src/missing.rs`;
  - after SFB10 wrapper: same human text, plus `_meta["symforge/result_status"] = {"contract_version":1,"outcome_class":"not_found"}`.
- Exact goal verification passed:
  - `cargo fmt --check`: exit 0 after applying rustfmt.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; observed key totals include `src/lib.rs` 1761 passed, `src/main.rs` 6 passed, and all integration targets passed.
  - `rg "result_status|ResultStatus|outcome_class" src/protocol src/live_index tests`: exit 0 and showed the status module, conformance pins, wrappers, and SFB10 contract tests.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; full all-targets suite passed again.
  - `cargo build --release`: exit 0; finished release profile successfully.
- Verified implementation commit: `ee2207eca5a20f6c9a5241dc36aa58c2073fc3e7`.
- Goal frontmatter completed at `2026-05-20T15:26:17.1425475+02:00` with that implementation commit hash.

## Review

- SFB10 acceptance criteria passed before commit: public read/search/reference tool registrations now attach additive result-status metadata, existing human-readable text remains unchanged, and tests cover found, not_found, empty_result, ambiguous, and invalid_request states across the requested surfaces.
- Changes stayed within the allowed tracked areas: `src/protocol/tools.rs` and `tasks/todo.md`. No daemon, sidecar, npm, docs, plans, `.planning`, openspec, or edit-tool implementation files were modified.

---

# SFB09 - Define machine-readable MCP result-status contract

## Plan

- [x] Run Branch Guard from the original checkout.
- [x] Switch to `.worktrees/backlog-implementation` and confirm branch/status there.
- [x] Index the target worktree with SymForge and check agentmemory for prior context.
- [x] Copy the SFB09 goal file into the worktree and mark it `In progress`.
- [x] Inspect existing protocol response construction and RMCP content constraints.
- [x] Decide whether structured metadata is safe or whether a delimited footer/envelope is required.
- [x] Inspect existing schema/conformance/read/search tests for the lowest-blast-radius fixture.
- [x] Add failing contract tests for the status vocabulary and serialization/envelope shape.
- [x] Add a failing fixture showing one existing read/search tool can emit machine status while preserving human text.
- [x] Implement the central result-status type/formatter.
- [x] Run focused tests and capture example response output.
- [x] Run exact goal verification:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo test --all-targets -- --test-threads=1`
  - `rg "ResultStatus|result_status|outcome_class|not_found|ambiguous" src tests`
- [x] Run default full verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Update SFB09 frontmatter to `Completed` with the verified work commit hash.
- [x] Commit the SFB09 goal-status update separately.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- The SFB09 goal file was absent from the worktree and was copied from the original checkout per Branch Guard.
- SymForge indexed the worktree: 190 files, 9272 symbols.
- agentmemory recall for SFB09/result-status context returned no matching prior observations.
- `tasks/lessons.md` is absent in this worktree; no prior lesson file was available to review.
- Goal status changed to `In progress` at `2026-05-20T13:59:47.0838734+02:00`.
- Response construction finding: most existing read/search handlers still return human-readable `String`, while RMCP `CallToolResult` supports `content`, `structuredContent`, `isError`, and `_meta`.
- Decision: use RMCP `_meta["symforge/result_status"]` as the additive machine contract. No footer is needed because `_meta` is available, and `structuredContent` is avoided here because the goal requires preserving existing human text instead of converting read tools to JSON.
- Added central status contract in `src/protocol/result_status.rs`:
  - stable `OutcomeClass` vocabulary: `found`, `not_found`, `ambiguous`, `invalid_request`, `empty_result`, `internal_failure`;
  - `ResultStatus { contract_version: 1, outcome_class }`;
  - `into_call_tool_result` formatter that keeps the text content unchanged and attaches the namespaced `_meta` payload.
- Red test evidence:
  - `cargo test --test conformance result_status -- --nocapture` initially failed with unresolved import `symforge::protocol::result_status`.
  - The first integration fixture attempt also exposed that `SymForgeServer::new()` requires test setup arguments, so the read-tool preservation fixture was moved into the existing `src/protocol/tools.rs` test module.
- Focused verification passed:
  - `cargo test --test conformance result_status -- --nocapture`: 2 passed, 0 failed for the filtered contract tests at that point.
  - `cargo test --test conformance -- --test-threads=1`: 12 passed, 0 failed.
  - `cargo test test_get_file_content_text_can_carry_result_status_without_changing_text -- --nocapture`: matching read-tool fixture passed.
- Example response shape preserving human text:
  - human text: `src/lib.rs\nfn present() {}`;
  - machine status: `_meta["symforge/result_status"] = {"contract_version":1,"outcome_class":"found"}`;
  - serialized response keeps `content[0].text` exactly equal to the original human text and does not add `structuredContent`.
- Exact goal verification passed:
  - `cargo fmt --check`: exit 0 after rustfmt formatting.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; observed key totals include `src/lib.rs` 1758 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `rg "ResultStatus|result_status|outcome_class|not_found|ambiguous" src tests`: exit 0 and showed the new status module, conformance tests, read-tool fixture, and existing not-found/ambiguous call sites.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed again.
  - `cargo build --release`: finished release profile successfully.
- Verified implementation commit: `8bcb3ab8aabf73cfac52d07b221ec08a4e73a449`.

## Review

- SFB09 acceptance criteria passed before commit: the central contract/formatter exists, conformance tests pin vocabulary and `_meta` serialization shape, `invalid_request` maps to `isError`, and an existing `get_file_content` fixture demonstrates status attachment without changing human text.
- Changes stayed inside the allowed tracked areas: `src/protocol/**`, `tests/conformance.rs`, and `tasks/todo.md`. No live-index, daemon, sidecar, npm, docs, plans, `.planning`, or openspec files were modified.

---

# SFB08 - Preserve same-line inline docs in replace_symbol_body

## Plan

- [x] Run Branch Guard and switch to the dedicated `backlog-implementation` worktree.
- [x] Index the target worktree with SymForge and check memory/lessons context.
- [x] Mark the SFB08 goal file `In progress` with a start timestamp.
- [x] Inspect current `replace_symbol_body` formatting/span behavior and existing edit tests.
- [x] Add a failing regression test for preserving same-line inline docs when `new_body` is docless.
- [x] Implement the minimal span/formatting fix without changing unrelated edit behavior.
- [x] Run task-specific verification:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo test --all-targets -- --test-threads=1`
  - `rg "replace_symbol_body|inline doc|deprecated|raw_line_start" src tests`
- [x] Run default full verification when task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Update SFB08 frontmatter to `Completed` with the verified work commit hash.
- [x] Commit the goal-status update separately.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- SymForge indexed the worktree: 190 files, 9261 symbols.
- Goal status changed to `In progress` at `2026-05-20T13:16:53.2047193+02:00`.
- Root cause: docless `replace_symbol_body` used `raw_line_start` as the splice start. When `raw_line_start < sym.byte_range.0`, same-line doc text before the parsed symbol was removed with the old signature.
- Red test evidence:
  - `cargo test preserves_same_line -- --nocapture` failed with both new fixtures losing `/** @deprecated */`.
  - The TypeScript fixture failed with disk output starting `export function legacy`.
  - The Rust block-doc fixture failed with disk output starting `pub fn legacy`.
- Implementation:
  - Added `docless_replacement_splice_start` in `src/protocol/edit.rs`.
  - The helper detects same-line `/** ... */`, `/*! ... */`, and `#[doc ...]` prefixes before the parsed symbol and returns the first non-whitespace byte after the doc marker.
  - `replace_symbol_body` now uses that helper only when `new_body` does not supply docs.
  - Added TypeScript/JSDoc and Rust block-doc same-line regression tests.
- Focused verification:
  - `cargo test docless_replacement_splice_start -- --nocapture`: 3 passed, 0 failed.
  - `cargo test preserves_same_line -- --nocapture`: 4 passed, 0 failed after the helper tests were added.
  - `cargo test replace_symbol_body -- --nocapture`: 18 unit tests passed plus matching integration tests including dry-run, attached-doc, orphan-doc, and same-line fixtures.
- Task-specific verification passed:
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: exit 0; observed key totals include `src/lib.rs` 1757 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `rg "replace_symbol_body|inline doc|deprecated|raw_line_start" src tests`: exit 0 and showed the new same-line `@deprecated` fixtures plus `raw_line_start` call sites.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`: exit 0.
  - `cargo check`: exit 0.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed again after the helper tests were added.
  - `cargo build --release`: finished release profile successfully.
- Verified implementation commit: `691e0a713035309d910b78b3cdf2d540112a4d37`.

## Review

- SFB08 acceptance criteria passed: same-line TypeScript/JSDoc and Rust block-doc fixtures preserve `/** @deprecated */`, existing attached-doc and orphan-doc replacement tests pass, and required verification completed.
- Changes stayed inside the allowed tracked files/areas: `src/protocol/edit.rs`, `src/protocol/tools.rs`, and `tests` coverage inside the inline `tools.rs` test module. No forbidden files were modified.

---

# SFB07 - Pin search_text usage grouping behavior for doc comments and markdown

## Plan

- [x] Run Branch Guard and move work to `backlog-implementation`.
- [x] Copy the SFB07 goal file into the worktree.
- [x] Mark SFB07 in progress.
- [x] Inspect current `search_text(group_by="usage")` implementation and existing tests.
- [x] Choose and document the usage contract for doc comments and markdown.
- [x] Add regression tests covering ordinary comments, doc comments, and markdown.
- [x] Implement the minimal code change needed for the chosen contract, if current behavior is not already correct.
- [x] Capture search output samples for the chosen behavior.
- [x] Run focused `search_text` regression verification.
- [x] Run the exact goal verification command.
- [x] Run default verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Mark SFB07 completed and commit goal status.

## Evidence Log

- Branch Guard from the original checkout returned `main` with a clean status, so edits moved to `.worktrees/backlog-implementation`.
- Branch Guard in the worktree returned `backlog-implementation` with a clean status before goal edits.
- The SFB07 goal file was absent from the worktree and was copied from the original checkout per Branch Guard.
- SymForge indexed the worktree: 190 files, 9259 symbols.
- Goal status changed to `In progress` at `2026-05-20T12:35:27.8286992+02:00`.
- Implementation evidence:
  - `src/protocol/format.rs::is_noise_line` already documents and implements a non-doc-comment filter.
  - `src/protocol/format.rs::search_text_result_view` applies that line-noise filter only in `group_by="usage"` / `"purpose"`.
  - Existing test `test_search_text_group_by_usage_filters_imports` covered import filtering but did not cover ordinary comments, doc comments, or markdown.
- Decision: KEEP_DOC_MARKDOWN_USAGE_VISIBLE. Usage grouping stays a line-noise filter for imports and ordinary comments. Rust doc comments remain searchable context, and Markdown body text remains visible; hash-heading lines keep the existing comment-like filtering.
- Added regression tests:
  - `protocol::tools::tests::test_search_text_group_by_usage_keeps_doc_comments_visible`.
  - `protocol::tools::tests::test_search_text_group_by_usage_keeps_markdown_body_visible`.
- No production behavior change was required; current behavior already matched the chosen contract.
- Search output samples:
  - Doc-comment sample: `search_text(query="non-doc comment", path_prefix="src/protocol/format.rs", group_by="usage")` shows `/// Returns true if the line looks like an import statement or a non-doc comment.`
  - Ordinary-comment sample: `search_text(query="Should exclude the \"use\" import line", path_prefix="src/protocol/tools.rs", group_by="usage", include_tests=true)` shows `(1 import/comment match(es) excluded by usage filter)`.
  - Markdown sample: `search_text(query="SFB07", path_prefix="tasks", group_by="usage")` shows body/list matches under `tasks/todo.md` and `(1 import/comment match(es) excluded by usage filter)` for the heading.
- Focused verification:
  - `cargo test test_search_text_group_by_usage -- --test-threads=1`: 3 passed, 0 failed.
  - `cargo test test_search_text -- --test-threads=1`: 44 passed, 0 failed.
  - `cargo fmt --check`: passed after rustfmt formatting.
- Exact goal verification passed:
  - `cargo fmt --check`.
  - `cargo check`.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed; observed key totals include `src/lib.rs` 1752 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `rg "group_by.*usage|usage" src tests`: completed and showed the new usage tests plus existing usage-related call sites.
- Default verification passed:
  - `git branch --show-current`: `backlog-implementation`.
  - `git diff --check`: exit 0; Git reported CRLF replacement warnings for touched files, not whitespace errors.
  - `cargo fmt --check`.
  - `cargo check`.
  - `cargo test --all-targets -- --test-threads=1`: full all-targets suite passed again; observed key totals include `src/lib.rs` 1752 passed and `src/main.rs` 6 passed, plus integration test targets.
  - `cargo build --release`: finished release profile successfully.
- Verified implementation commit: `96b4954e4458dc79f10012e28222c8588916cc9f`.

## Review

- SFB07 acceptance criteria passed: the KEEP_DOC_MARKDOWN_USAGE_VISIBLE decision is recorded in test comments, regression tests cover ordinary comments, doc comments, and markdown, and existing `search_text` tests plus full verification passed.
- Changes stayed inside the allowed tracked files/areas plus the copied ignored goal-status file. No `docs/**`, `plans/**`, `.planning/**`, `openspec/**`, `npm/**`, daemon, edit protocol, or parsing files were modified.

---

# Outstanding-Work Hardening (2026-07-10)

## Plan

- [x] Audit `docs/OUTSTANDING-WORK.md` against current code, tests, runtime,
  memory, vault, releases, and live dogfood.
- [x] Approve and commit the code-first architecture design.
- [x] Convert product intent into explicit trust, tool-substitution, and token-
  economy acceptance gates.
- [x] Close Feature 018 browse/frecency code residuals.
- [ ] Close Feature 018 documentation/task residuals with the canonical dogfood
  artifacts in Task 12.
- [x] Replace inline daemon project instances with per-project slots and
  partition per-session protocol/cache state.
- [x] Make daemon home immutable and `index_folder` additive/persistent.
- [~] Route read, guidance, compact, and structural-edit tools explicitly by
  project. (Daemon-route core DONE 2026-07-11: `runtime_for_target` shared
  resolver + `single_project_routed_tool` peek/strip routing in
  `call_tool_handler` for the 16 read/guidance verbs; parity table + resolver
  contract tests green. REMAINING: `project` fields in tool input schemas +
  strict-client schema pins, local-mode explicit-project refusal guards,
  set-valued `search_files` cross-target merge, compact `symforge` facade
  routing through stel planner/executor, project-explicit structural edits.)
- [x] Replace the global snapshot write lock with same-path serialization.
- [ ] Carry selected-project/freshness evidence and expose project inventory.
- [ ] Make reconnect and runtime descriptors multi-session safe.
- [ ] Enforce daemon uniqueness and reap expired sessions.
- [x] Preserve generated-output admission through watcher single-file updates.
- [ ] Add native, preserving Grok initialization.
- [ ] Create the canonical Grok dogfood prompt and common-tool substitution
  scorecard.
- [ ] Resolve every outstanding-work ledger entry with executable evidence.
- [ ] Run focused/full Terminal Commander gates, release-binary multi-project
  dogfood, and adversarial review.
- [ ] Stop for explicit approval before push/PR/merge/publish/`cargo clean`.

## Evidence Log

- Design: `docs/superpowers/specs/2026-07-10-outstanding-work-hardening-design.md`
  committed as `1608433`.
- Executable plan: `docs/superpowers/plans/2026-07-10-outstanding-work-hardening.md`.
- Current code truth: the daemon already has deterministic project IDs and a
  multi-project `WorkingSet`, but ordinary `index_folder` still destructively
  mutates `active_project_id` while holding the project-map write lock through
  reload.
- Product gate: hardening is enabling work. Completion requires proving common
  repository-tool substitution and measured token savings with retained-answer
  checks, not merely green infrastructure tests.
- Feature 018 browse closure: `a646f23`; the repeated generic-name RED failed
  with four `new` hits, then the exact diversity and real-store frecency tests
  passed.
- Snapshot isolation: `3e756ee`; 42 persistence tests passed, including
  same-path serialization, distinct-path independence, reset locking, unique
  temp names, stale-temp cleanup, and failed-write cleanup.
- Daemon project isolation: `b729164`; exact cross-project, prior-generation,
  reload-serialization, and cross-session cache tests passed; daemon suite
  passed 68/68.
- Integrated verification after all three slices: `cargo fmt --check`,
  `cargo check`, `cargo clippy --lib -- -D warnings`, and the full library suite
  passed (`2709 passed; 0 failed; 2 ignored`).
- Immutable-home/additive `index_folder` (2026-07-11, resumed on
  `C:\AI_STUFF\PROGRAMMING\symforge`): destructive retarget removed;
  `open_project_for_session` is the one canonical open path for omitted `add`
  and `add=true` (shared durable idempotency ledger stored under the HOME
  project so same-key/different-target conflicts reject before load); reload
  persists an atomic snapshot with `checkpoint=written` / honest
  `checkpoint=degraded: ...` receipts; proxy success no longer resets
  local home fallback; daemon-proxy failure refuses destructive local
  fallback; stale per-session server replaced via `Arc::ptr_eq` index
  identity; slot cleanup/reinsertion closed by `ensure_project_slot_for_session`
  (join under authoritative registry write lock, reused by session open);
  session-close attach race closed (close removes the session record first,
  `add_project_to_session` undoes the join via `detach_project_membership`);
  proxy-failure test made hermetic (ephemeral bound-then-released port +
  pre-degraded flag, no port-1 assumption, no reconnect autospawn); new
  checkpoint-failure coverage
  (`test_index_folder_open_reports_degraded_checkpoint_on_snapshot_failure`).
  Receipts: `cargo test --lib daemon::tests::test_index_folder -- --test-threads=1`
  = 10 passed / 0 failed; `cargo test --lib daemon:: -- --test-threads=1` =
  72 passed / 0 failed; full `cargo test --lib -- --test-threads=1` =
  2714 passed / 5 failed — the 5 failures are exactly the still-red watcher
  generated-output fixtures owned by the next slice; `cargo clippy --lib -- -D
  warnings`, `cargo fmt --check`, `git diff --check` all exit 0.
- Watcher generated-output parity (2026-07-11): extracted the ONE path-shape
  rule (`shallowest_generated_output_prefix`) shared by the bulk demotion walk
  and a new per-event `discovery::is_untracked_generated_output_path`; wired it
  into `read_and_index` after the admission gate (path-shape checked first so
  ordinary events never touch git; git evidence consulted only for
  generated-looking components; fail-open on non-git trees; opt-in env honored;
  tracked file or tracked sibling under the prefix rescues to Tier 1; skip
  records deduped by the existing `demote_to_skipped_at_generation`). Receipts:
  `cargo test --lib watcher::tests:: -- --test-threads=1` = 38 passed / 0
  failed (all five previously-red fixtures green); `cargo test --lib --
  discovery:: live_index::store:: --test-threads=1` = 134 passed / 0 failed;
  full `cargo test --lib -- --test-threads=1` = 2719 passed / 0 failed /
  2 ignored; `cargo clippy --lib -- -D warnings`, `cargo fmt --check`,
  `git diff --check` all exit 0.
- Recovered-review blockers (2026-07-11, code slice): #1 `detect_impact`
  payload now carries a `source_filter` object (applied/excluded_paths/hint
  naming `include_data=true`); #2 empty filtered `what_changed` (uncommitted)
  disclosure now reports the filtered-out count, the source-focused default,
  and `code_only=false`; #3 `code_only` keeps unknown-extension source via
  `is_unparsed_source_path` allowlist (.sql/.sh/.bash/.zsh/.ps1/.psm1/.psd1/
  .bat/.cmd/.proto/.tf/.tfvars/.cmake/.gradle + Dockerfile/Makefile/
  GNUmakefile/justfile); #7 compact repo-map `is_intra_workspace_path` now
  also rejects `..` segments, UNC, and backslash-rooted paths; #8 CCR
  duplicate insert (same content-addressed handle) refreshes age instead of
  double-counting `total_bytes`/economics; #10 `quarantine_bad_snapshot` now
  holds the per-path snapshot lock (red test mirrors the reset-lock witness);
  #4/#18 018 tool-behavior contract reconciled (browse `(name,kind)` dedup,
  compact/tree containment parity, both new disclosures). Receipts: red
  witnesses failed first (3 FAILED), then targeted suites green
  (what_changed/detect_impact 15 passed; ccr+persist+sidecar 117 passed);
  full `cargo test --lib -- --test-threads=1` = 2725 passed / 0 failed /
  2 ignored; `cargo clippy --lib -- -D warnings`, `cargo fmt --check`,
  `git diff --check` all exit 0.
- Explicit project routing, daemon-route core (2026-07-11):
  `DaemonState::runtime_for_target(session_id, project)` is the one shared
  resolver (omission -> immutable home; open project ID first; unique current
  `project_name` among the session's OPEN projects as display text only;
  unknown/ambiguous -> deterministic candidate errors, no indexing, no
  frecency); `call_tool_handler` peeks/strips the `project` field for the 16
  routed read/guidance verbs and dispatches the existing per-project
  implementation; the three cross-project discovery verbs keep their own
  `project`/`projects` handling. Receipts:
  `cargo test --lib daemon::tests::test_project_routing_parity_table -- --exact
  --test-threads=1` = 1 passed;
  `daemon::tests::test_runtime_for_target_resolution_contract` = 1 passed;
  `cargo test --lib daemon:: -- --test-threads=1` = 74 passed / 0 failed.
- Explicit project routing, schema + local-guard slice (2026-07-11): added the
  optional `project` selector field (serde default, documented) to the 15
  routed input structs (GetSymbol/GetSymbolContext/GetFileContext/
  GetFileContent/GetRepoMap/SearchFiles/FindDependents/DiffSymbols/
  WhatChanged/AnalyzeFileImpact/ValidateFileSyntax/Explore/SmartQuery/
  EditPlan/Investigation), including both manual `Deserialize` Raw structs;
  added `SymForgeServer::foreign_project_refusal` and wired it after the proxy
  attempt in all 16 local handlers (`ask` included) so a stdio/embed/degraded
  server refuses a non-matching explicit selector instead of silently serving
  the bound project; 200 struct-literal sites updated mechanically from cargo
  E0063 spans. Receipts: `cargo test --test strict_client_schema_compat` = 1
  passed; focused `test_local_server_refuses_foreign_project_selector` = 1
  passed; full `cargo test --lib -- --test-threads=1` = 2728 passed / 0
  failed; `cargo clippy --all-targets -- -D warnings` exit 0. NOTE: Terminal
  Commander daemon became unavailable mid-session (health probe
  daemon_unavailable); remaining commands ran through the harness's headless
  shell — no visible terminals — until TC returns.
- Project-explicit structural edits (2026-07-11): the 7 edit verbs
  (replace_symbol_body/edit_within_symbol/insert_symbol/delete_symbol/
  batch_edit/batch_insert/batch_rename) joined the routed set — the batch-level
  `project` selector resolves through the same `runtime_for_target`, so
  worktree/`working_directory` validation runs against the SELECTED project;
  the selector was added to the 7 edit input structs only (NOT SingleEdit/
  InsertTarget — no nested conflicting routing); local handlers refuse foreign
  selectors via `foreign_project_refusal`; 51 more struct-literal sites updated
  from cargo spans. `tests/watcher_reload_cancellation.rs` updated from the old
  destructive-retarget contract to the immutable-home additive contract
  (2 projects after open, B healthy, nothing evicted). Receipts:
  `daemon::tests::test_explicit_project_edit_routes_and_preserves_worktree` =
  1 passed (explicit-B mutates only B, omitted mutates home A, unknown writes
  nothing); full `cargo test --lib -- --test-threads=1` = 2729 passed / 0
  failed; full `cargo test --all-targets -- --test-threads=1` = 0 failures;
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  `git diff --check` exit 0.
- Project inventory surfaces (2026-07-11, Task 7 part 1):
  `DaemonState::render_session_project_inventory` renders one row per open
  project (deterministic ID, display name/root, home marker, published
  counts/index state, generation, opened timestamp, snapshot presence) plus
  session last-seen; `status(detail="projects")` (new `StelStatusDetail::
  Projects` variant; local render lists the single bound project) is
  intercepted on the daemon route to serve the full session inventory; full-
  surface `health`/`health_compact` append the same inventory once MORE than
  one project is open (single-project outputs stay byte-compatible). Receipts:
  `daemon::tests::test_status_projects_detail_lists_session_inventory` = 1
  passed; full lib = 2730 passed / 0 failed; strict schema + stel param
  disposition tests pass; clippy/fmt clean. REMAINING from Task 7: the typed
  daemon tool receipt (machine-readable project/generation/load-source
  metadata through call_tool_value -> proxy -> statused wrapper).
- Session reaper (2026-07-11, Task 9 part 1): daemon-owned bounded interval
  task (TTL default 86400s via SYMFORGE_SESSION_TTL_SECS, min 60; sweep period
  ttl/4 clamped [10s, 600s]); candidates collected as (session_id,
  observed_last_seen) under the read lock, `close_session_if_expired` rechecks
  the SAME observation under the sessions write lock and atomically claims the
  record before shared project cleanup — a racing heartbeat wins; a claimed
  session's later heartbeat fails rather than resurrecting; teardown reuses the
  extracted `finish_removed_session` (shared with interactive close, so orphan
  watchers/projects are removed exactly once). Reaper holds a Weak on daemon
  state (exits on daemon drop) and is aborted in run_daemon_until_shutdown.
  Receipts: `daemon::tests::test_reaper_rechecks_heartbeat_before_close` = 1
  passed (first attempt failed on a same-millisecond fixture bug — fixed to a
  realistic ancient-observation/past-cutoff shape); daemon suite 77 passed / 0
  failed; full lib 2731 passed / 0 failed; clippy/fmt clean. REMAINING from
  Task 9: the guarded-start seam for foreground `symforge daemon` vs auto-spawn
  (tests/daemon_singleton.rs) and last-seen/TTL evidence in detailed status.
- Typed project-evidence receipt (2026-07-11, Task 7 part 2): new
  `ProjectEvidence` contract (project_id, project_name, canonical_root,
  generation, index_state, load_source, index counts) in
  `protocol::result_status`; the daemon returns it OUT-OF-BAND as the
  `x-symforge-project-evidence` response header built from the RESOLVED
  runtime (so an explicitly routed sibling is attested as itself, never home)
  while the text body stays byte-identical; `call_tool_value` parses the
  typed header into a per-dispatch task-local slot (same bound-to-the-future
  pattern as the D23 connection surface — never reconstructed from body
  text); `ServerHandler::call_tool` seeds the slot with the LOCAL bound
  project so stdio/embed responses attest themselves; statused results attach
  the current evidence under `_meta["symforge/project_evidence"]`. Receipts:
  `daemon::tests::test_tool_receipt_carries_project_evidence` +
  `protocol::tools::tests::test_local_tool_meta_carries_project_evidence` =
  2 passed; full lib 2733 passed / 0 failed; full all-targets suite 0
  failures; clippy/fmt/diff-check clean.
- New dogfood defect (2026-07-11, unfiled): the watcher demoted
  `src/protocol/tools.rs` (UTF-8 Rust source, ~1.1 MB) to Tier 2 with reason
  "binary, size 1.1 MB" after an edit — the size-threshold demotion mislabels
  a large text source file as binary, and a Tier-2 core source file breaks
  symbol navigation on the project's own biggest module. Needs a red fixture
  (admission reason honesty + threshold review) before the final gate.
- New defect observed while dogfooding (2026-07-11, unfiled): `get_file_context`
  on a conflict-markered Rust file reported `Completeness: full` with a symbol
  count in the header while rendering no outline entries (only the tail of the
  next-steps list) under `max_tokens=3000` — either the partial-parse path drops
  the outline while stamping `full`, or budget trimming fails to downgrade
  completeness to `budget-limited`. Needs a red fixture before the final gate.

## Review

- In progress. Add verified commits, focused/full gate receipts, scorecard,
  adversarial findings, and remaining operator gates here before completion.
