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
