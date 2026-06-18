# Contract: per-harness `symforge-admin` install (US3, FR-016)

**Surface**: `cli::harness_command` (install) + `protocol::prompts` (the MCP affordance).

## Universal affordance — MCP prompt `symforge-admin`
- Registered in `src/protocol/prompts.rs` (first-class MCP surface, Constitution II).
- Returns the running dashboard URL, running the reachability → reuse/start path (admin-cli).
- Present for EVERY MCP-speaking harness, with or without a command file — the floor.

## Convenience layer — command file (where supported)
- **Claude Code**: write `~/.claude/commands/symforge-admin.md` (markdown slash-command)
  that invokes `symforge admin`. Installed by the standard config path.
- **Harnesses without a documented command-file format** (Codex, Gemini, Cursor, KiloCode,
  Claude Desktop): NO guessed command file — they use the MCP prompt only. Installing a
  format we can't verify would ship a broken affordance (a nonworking feature) — explicitly
  not done.

## Rules
- Command-file capability is confirmed against the real `HarnessId` configs during
  implement; default to MCP-prompt-only when a format is unknown.
- A command-file write reuses the restorable-backup path (no unbacked overwrite).

## Guarantee
Every configured harness has at least the MCP-prompt affordance; command-file-capable
harnesses additionally get the one-keystroke command. No harness gets a broken command file.
