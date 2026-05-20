# SFB05 - Land top external-evaluator regression tests or convert gaps into concrete backlog items

## Plan

- [x] Run branch guard and move work to `backlog-implementation`.
- [x] Copy the SFB05 goal file into the worktree.
- [x] Mark SFB05 in progress.
- [x] Audit current test/code/backlog evidence for external-evaluator bug candidates.
- [x] Map selected candidates to categories: parser, search, edit, git/ranking, daemon/session, protocol contract, npm/client setup, or sidecar.
- [x] Add at least three deterministic regression tests, or add concrete backlog entries with file targets and verification for untestable top gaps.
- [x] Confirm no historical planning directories or forbidden files were restored.
- [x] Run focused verification for changed tests/code.
- [x] Run the exact goal verification command.
- [x] Run default verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Mark SFB05 completed and commit goal status.

## Evidence Log

- Branch guard passed on `backlog-implementation` with a clean worktree before goal edits.
- Source goal file was absent from the worktree and copied from the original checkout per Branch Guard.
- `docs/live-code-backlog.md` does not contain the exact phrase `external evaluator`; audit must use current code, tests, and backlog evidence without reviving historical reports.
- Audited retained external-review evidence from `CHANGELOG.md`:
  - `430a86a` / `0e773e6`: external review feedback waves covering `batch_rename(code_only)`, `search_text(group_by="names")`, and `what_changed(include_symbol_diff)`.
  - `61d2757`: external code reviews covering C# async Task parsing, `analyze_file_impact` watcher race, and replace orphaned doc handling.
  - `80303a9`: external codebase testing remediation covering MCP output quality and related protocol surfaces.
- Selected top gaps and categories:
  - `search_text(group_by="names")` did not have a focused regression and the formatter path needed the promised flat unique symbol-name output. Category: search.
  - `what_changed(include_symbol_diff=true, code_only=true)` did not have an output regression proving path list and appended compact symbol diff stay aligned. Category: git/ranking.
  - `batch_rename(code_only=true)` did not have an observable regression proving the qualified-usage scan excludes docs/non-code references. Category: edit.
- Audited but not selected because current tests already cover them or they are broader backlog work:
  - C# async `Task` method naming: covered by `test_csharp_async_task_method_name`.
  - `find_dependents` non-public/common constructor false positives: covered by existing query/tool tests.
  - JSONC config comment stripping and `diff_symbols` compact/const handling: covered by existing parser/format tests.
  - Sidecar impact pre-update race, same-line JSDoc preservation, public result-status semantics, and runtime identity/reset clarity are broader or already tracked as backlog-sized work.
- Added deterministic regression tests:
  - `protocol::tools::tests::test_search_text_group_by_names_returns_flat_unique_symbol_names`.
  - `protocol::tools::tests::test_what_changed_include_symbol_diff_appends_compact_symbol_summary`.
  - `edit_hook_behavior::batch_rename_code_only_excludes_docs_from_qualified_usage_scan`.
- Tiny required fix: `src/protocol/format.rs::search_text_result_view` now renders `group_by="names"` as a flat, deduplicated enclosing-symbol name list.
- Focused verification passed:
  - `cargo test test_search_text_group_by_names_returns_flat_unique_symbol_names -- --test-threads=1`.
  - `cargo test batch_rename_code_only_excludes_docs_from_qualified_usage_scan -- --test-threads=1`.
  - `cargo test test_search_text_group_by -- --test-threads=1` after formatting.
  - `cargo test test_what_changed_include_symbol_diff_appends_compact_symbol_summary -- --test-threads=1` after formatting.
- No `plans/**`, `.planning/**`, `openspec/**`, historical ADRs, old reports, or `npm/**` files were restored or edited.
- Elegance check: the only production change is localized to the existing search result formatter branch, avoiding a new abstraction or search-core churn.
- Exact goal verification passed after implementation commit `5ac3e3959db88ef837ac9b6bde3178c42303eaaf`:
  - `cargo fmt --check`.
  - `cargo check`.
  - `cargo test --all-targets -- --test-threads=1`.
  - `git diff --name-only HEAD~1..HEAD` showed `src/protocol/format.rs`, `src/protocol/tools.rs`, `tasks/todo.md`, and `tests/edit_hook_behavior.rs`.
- Default verification passed:
  - `git branch --show-current` returned `backlog-implementation`.
  - `git diff --check`.
  - `cargo fmt --check`.
  - `cargo check`.
  - `cargo test --all-targets -- --test-threads=1`.
  - `cargo build --release`.
- Verified implementation commit: `5ac3e3959db88ef837ac9b6bde3178c42303eaaf`.

## Review

- SFB05 acceptance criteria passed: three deterministic regression tests landed and passed, selected gaps are mapped to search, git/ranking, and edit categories, and forbidden historical/planning/npm paths were not restored.
