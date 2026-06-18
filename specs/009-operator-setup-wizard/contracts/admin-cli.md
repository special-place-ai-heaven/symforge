# Contract: `symforge admin` verb + in-harness affordance (US3)

**Surface**: `Commands::Admin` (cli/mod.rs) → `cli::admin::run`, plus the MCP prompt
`symforge-admin` (protocol::prompts) and the Claude Code command file.

## CLI verb (`symforge admin`)
1. Read `OperatorSetupProfile.port` (the remembered port).
2. Reachability check (HTTP GET `/api/v1/summary`, short timeout) on that port.
3. **If serving** → reuse it: print/return `http://<addr>/admin`, offer browser open. Start
   nothing (FR-015, SC-004 — never a duplicate).
4. **If not** → start serve on a verified-free port (D1), persist the port, report + open.

## In-harness affordance (FR-016)
- **MCP prompt `symforge-admin`** (universal): registered in `protocol::prompts`; returns
  the running dashboard URL (running the same reachability→reuse/start path). Every
  MCP-speaking harness gets this even without a command file.
- **Claude Code command file** (convenience): `~/.claude/commands/symforge-admin.md`
  installed by the standard config path (init/setup), invoking `symforge admin`. Harnesses
  without a command-file convention rely on the MCP prompt.

## Guarantees
- One action from inside the harness opens the dashboard (reuse if running, else start) (SC-004).
- Reported URL reachable (FR-020).
- No second server when one already serves the remembered port (FR-015).
