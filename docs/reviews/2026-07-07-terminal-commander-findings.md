# SymForge Tooling Findings — Terminal Commander session

Date: 2026-07-07  
Source repo: [terminal-commander](C:/AI_STUFF/PROGRAMMING/terminal-commander)  
SymForge version observed: 8.13.6

> **Path note:** Every file path below (`crates/daemon/...`, `tests/fixtures/...`, etc.) is
> relative to the **terminal-commander** repository, not symforge. Reproduce by checking out
> that repo or substituting equivalent fixtures here.
>
> Original live log (same content, terminal-commander-relative links):
> `terminal-commander/docs/2026-07-07-symforge-tooling-findings.md`

SymForge's absolute goal is to reduce noise, save tokens, and increase LLM trust in its tools.
Findings below are written against that bar: a bug matters when it adds noise, burns
tokens/round-trips, or makes the agent trust raw reads / git diff over SymForge output.

## Current Baseline

- SymForge version observed in-session: 8.13.6.
- Surface after config update: full granular MCP tools are available.
- Full health observed in-session: ready; all indexed files parsed successfully.
- Compact health is only acceptable as a quick liveness check; use full `health` output for
  diagnostics and report evidence.

## Findings

### 2026-07-07 - Compact surface hid granular tools from Codex

Severity: Integration friction  
SymForge status: **Config / docs** — set `SYMFORGE_SURFACE=full` (or use init defaults that
already write `full` for Cursor/Codex).

Intent: Use SymForge for Terminal Commander code work, including `get_file_context`,
`search_text`, `edit_plan`, and symbol-aware edits.

Expected: The coding agent can discover and call the granular tools described by the repo's
AGENTS.md instructions.

Actual: Only the compact facade tools were visible initially: `symforge`, `status`, and
`symforge_edit`.

Workaround:

```toml
[mcp_servers.symforge.env]
SYMFORGE_SURFACE = "full"
```

Then restart the MCP client so tool discovery sees the granular tools.

Impact: High. The compact surface undermines repo-local AGENTS.md instructions that name
granular tools directly.

### 2026-07-07 - Compact natural-language routing was a poor edit-planning substitute

Severity: Workflow friction  
SymForge status: **Known compact limitation** — not a substitute for `edit_plan` /
`get_symbol_context`.

Intent: Plan changes to policy defaults and shell handling from a broad natural-language
request.

Expected: The compact facade should route edit-planning intent to a useful code navigation
result, or tell the agent which granular tool would have answered the request.

Actual: The facade routed the request to file search and did not provide the needed
symbol/edit context.

Workaround: Use direct granular tools once `SYMFORGE_SURFACE=full` is active.

Impact: Medium.

### 2026-07-07 - `edit_plan` did not resolve an impl-method selector

Severity: Workflow friction  
SymForge status: **Fixed** — `edit_plan` now uses the same qualification-stripping selector
cascade as `get_symbol` / `edit_within_symbol` (`PolicyEngine::new` → `new` in file).

Tool: `edit_plan`

Intent: Plan a targeted edit to the `PolicyEngine::new` method after `get_file_context` showed
it in `crates/daemon/src/policy.rs`.

Input:

```json
{"target":"crates/daemon/src/policy.rs::PolicyEngine::new"}
```

Expected: Resolve the impl method and return an edit plan, or suggest accepted selector syntax.

Actual (before fix):

```text
Target 'crates/daemon/src/policy.rs::PolicyEngine::new' not found.
Try: search_symbols(query="...") to find the correct name.
```

Impact: Medium.

### 2026-07-07 - `analyze_file_impact` over-reported a header-only Rust comment edit

Severity: Signal noise  
SymForge status: **Fixed** — symbol diff compares core body bytes, not range drift from
prefix insertions.

Tool: `analyze_file_impact`

Intent: Refresh the index after editing only the top module doc comment.

Expected: Report a file-level/comment-only change or no symbol body changes.

Actual (before fix): Reported every symbol in the file as changed.

Repro paths (terminal-commander):

- `crates/mcp/tests/shell_live_e2e.rs`
- `crates/daemon/tests/shell_runtime.rs`
- `crates/daemon/tests/ipc_command.rs`
- `crates/mcp/src/tools.rs`
- `crates/daemon/src/ipc/handlers/command.rs`
- `crates/ipc/src/protocol.rs`

Workaround (before fix): Treat the report as an index refresh receipt; use `what_changed` /
git diff for exact scope.

Impact: Medium.

### 2026-07-07 - `analyze_file_impact` over-reported a one-string JSON fixture edit

Severity: Signal noise  
SymForge status: **Fixed** (same root cause as comment-only edits).

Tool: `analyze_file_impact`

Intent: Refresh the index after changing exactly one string in a contract JSON fixture.

Expected: Report the specific changed key/list item, or a concise file-level JSON change.

Actual (before fix): Reported many unrelated top-level and nested keys as changed.

Repro paths (terminal-commander):

- `tests/fixtures/contracts/mcp-tools/shell_exec.v1.json`
- `tests/fixtures/contracts/mcp-tools/system_discover.v1.json`

Impact: Medium.

### 2026-07-07 - Structural replace preserved stale leading doc comments outside the editable symbol

Severity: Workflow friction  
SymForge status: **Open** — `replace_symbol_body` intentionally preserves leading docs outside
the symbol body; `edit_within_symbol` cannot edit outside the resolved range. Rename flows may
need a direct text patch for orphaned `///` blocks.

Tools: `replace_symbol_body`, then `edit_within_symbol`

Impact: Medium.

## Entry Template

### YYYY-MM-DD - Short title

Severity:

SymForge version:

Tool:

Intent:

Input:

Expected:

Actual:

Workaround:

Impact:

Raw evidence:
