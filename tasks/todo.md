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
