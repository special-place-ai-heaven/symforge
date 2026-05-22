# CLAUDE.md — SymForge

## Verification (symforge)
- Backend: `cargo fmt --check`, `cargo check`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`
- `npm/` only: `cd npm && npm test`
- Mixed: run both before reporting success

## Architecture

Rust MCP server providing symbol-aware code navigation and editing tools. Current MCP `tools/list` exposes 31 canonical tools, including `health_compact`, with backward-compat aliases for removed tools in `src/daemon.rs`. Resources and prompts are first-class protocol surfaces, not side notes.

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
