# Implementation Plan: SymForge Harness Onboarding & Config Hub (v8 8.1)

**Branch**: `005-v8-harness-onboarding` (campaign on `review/v8-004-operator-serve`) | **Date**: 2026-06-16 | **Spec**: [spec.md](./spec.md)

## Summary

Extend the existing per-client registration in `src/cli/init.rs` into a **`HarnessRegistry`**: a catalog of known MCP clients (Claude Code, Claude Desktop, Codex, Gemini CLI, Kilo Code, Cursor) with per-client detect / add / refresh of a SymForge attach entry. Add a **scan** (report per-client absent/current/stale), a **backup-then-apply** writer (timestamped `.bak`, restorable, idempotent, dry-run), and a **first-run/post-update onboarding** banner with persisted shown-state. Exposed via `symforge init --scan` / apply. Reuses the BOM-safe config parsing and client-path knowledge already in `init.rs`; writes the `004` serve URL + Bearer key.

## Technical Context

**Language/Version**: Rust edition 2024. **Primary deps**: existing `init.rs` machinery, `serde_json`/`toml_edit` (already in repo) for the various client config shapes, `clap`. **Storage**: timestamped backup files beside each config; onboarding state as a small JSON in the SymForge data dir (`paths::ensure_symforge_dir`). **Testing**: `cargo test --all-targets -- --test-threads=1` against fixture config files under `tests/fixtures/harness/`; dry-run/backup inspection; never touches real user configs. **Project Type**: single Rust crate, CLI surface. **Constraints**: never corrupt a client config; every write backed up; idempotent; embed build unaffected (CLI is server-feature-gated already). **Scale**: ~6 known clients, more added incrementally.

## Constitution Check

Constitution is a stub; apply repo gates + feature invariants:
- **GATE-1 Non-destruction**: no apply without a successful prior backup; malformed/locked config never overwritten; dry-run writes nothing.
- **GATE-2 Idempotency**: second apply with same inputs = no-op; no duplicate entries.
- **GATE-3 Repo gates**: fmt/check/clippy -D warnings/test/build --release green.
- **GATE-4 Embed isolation**: unchanged (CLI/init already behind `server`).
No violations.

## Project Structure

```text
src/cli/
├── init.rs            # MODIFY: factor existing register_* into HarnessRegistry-backed detect/add/refresh
├── harness.rs         # NEW: HarnessRegistry, HarnessTarget, scan(), per-client attach-entry shape
├── harness_apply.rs   # NEW: backup-then-apply writer, dry-run plan, restore, idempotency check
└── onboarding.rs      # NEW: first-run/post-update banner + persisted OnboardingState

tests/
├── harness_scan.rs            # US1: scan reports absent/current/stale on fixtures
├── harness_apply_backup.rs    # US2: dry-run no-op; backup+restore byte-exact; idempotent re-apply
└── onboarding_state.rs        # US3: banner once per version; re-surfaces after version change
tests/fixtures/harness/        # NEW: sample client configs (populated, empty, stale-entry, malformed, BOM)
```

**Structure Decision**: keep it in `src/cli` beside `init.rs` (its natural home — same client knowledge). No new top-level module; reuse `init.rs` path/parse helpers. Onboarding state + backups live in the SymForge data dir / beside configs.

## Phase 0 / Phase 1 pointers

- research.md: backup naming/retention; onboarding-state location + version-keying; per-client config shape catalog (JSON object under `mcpServers` vs Codex TOML vs Gemini settings) — derive from the existing `register_*` functions.
- data-model.md: HarnessTarget, AttachEntry, BackupRecord, OnboardingState.
- Contracts (folded here for this slice): `init --scan` (report, no writes) / `init --scan --apply` (backup-then-write) / `--dry-run` (plan only); restore command/path. Default is non-destructive (scan/dry-run).

## Salvage / dependencies

Depends on `004` `symforge serve` for the attach URL + key. Reuses `src/cli/init.rs` (`register_claude_desktop_mcp_server`, `register_codex_mcp_server`, `register_gemini_mcp_server`, `register_kilo_mcp_server`, BOM-strip parse). No GUI/AAP/multi-key here.
