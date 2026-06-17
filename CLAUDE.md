# CLAUDE.md — SymForge

## Verification (symforge)
- Backend: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`
- `npm/` only: `cd npm && npm test`
- Mixed: run both before reporting success

## CI Gates

- PR and push CI run version sync, `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`, the full Rust test suite,
  `cargo build --release`, and npm tests.
- Scheduled and manual CI additionally run ignored performance smoke coverage:
  `test_load_perf_1000_files` and `calibrate_current_repo_smoke`.
- Full real-repo coupling calibration is operator-triggered with
  `SYMFORGE_CALIBRATION_REPOS`; standard CI must not depend on local paths.

## Merging PRs (release-please double-count guard)

GitHub's default merge commit puts the PR title in the commit BODY;
release-please parses merge-commit bodies for conventional messages, so a
plain `gh pr merge --merge` lands every PR in the changelog TWICE (merge
commit + inner commit). Always override the body with non-conventional text:

```
gh pr merge <N> --merge --delete-branch --body "PR #<N>"
```

Subject stays GitHub's default (`Merge pull request #N ...`, ignored by
release-please); inner commits are counted exactly once.

## Architecture

Rust MCP server providing symbol-aware code navigation and editing tools. Current MCP `tools/list` exposes 32 canonical tools, including `health_compact`, with backward-compat aliases for removed tools in `src/daemon.rs`. Resources and prompts are first-class protocol surfaces, not side notes.

Key source files:
- `src/protocol/tools.rs` — Tool handlers, input structs, tests
- `src/protocol/format.rs` — Output formatters
- `src/daemon.rs` — Daemon proxy with backward-compat aliases
- `src/cli/init.rs` — Tool name list for client init
- `src/live_index/query.rs` — Index query functions
- `src/protocol/resources.rs` — MCP resource handlers
- `src/protocol/prompts.rs` — MCP prompt handlers
- `src/protocol/result_status.rs` — Machine-readable outcome metadata

## Tool Consolidation Pattern

When merging tools A into B:
1. Add new params to B's input struct (with `#[serde(default)]`)
2. Add mode branch in B's handler
3. Remove `#[tool]` attribute from A (keep the method for internal use)
4. Add backward-compat alias in `src/daemon.rs` `execute_tool_call`
5. Remove A from `SYMFORGE_TOOL_NAMES` in `src/cli/init.rs`
6. Update cross-reference descriptions in other tools
7. Update tests: add new field initializers, add mode-specific tests

<!-- SPECKIT START -->
For additional context about technologies to be used, project structure,
shell commands, and other important information, read the current plan
at specs/010-v8-trust-remediation/plan.md
<!-- SPECKIT END -->
