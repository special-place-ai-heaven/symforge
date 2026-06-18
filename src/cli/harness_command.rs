//! Per-harness `symforge-admin` command-file install (009 US3, D2).
//!
//! Installs a `symforge-admin` command file where the harness supports one (e.g.
//! Claude Code `~/.claude/commands/symforge-admin.md`); harnesses without a
//! command-file convention rely on the universal MCP prompt instead (no guessed
//! or broken file — the MCP prompt is the floor). Reuses the restorable-backup
//! write path (FR-016).
//!
//! Phase 1 (T003) is a compiling skeleton: the installer + per-harness format
//! resolution land in Phase 5 (US3, T025). Logic is intentionally deferred here.

/// On-disk filename for the Claude Code `symforge-admin` slash-command file
/// installed under `~/.claude/commands/`.
pub const CLAUDE_ADMIN_COMMAND_FILE: &str = "symforge-admin.md";
