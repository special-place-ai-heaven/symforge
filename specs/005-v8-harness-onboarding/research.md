# Research: Harness Onboarding & Config Hub

Source of truth for the known-client catalog is `src/cli/init.rs`
(`register_*_mcp_server` functions + `InitPaths`). This feature factors that
path knowledge into a `HarnessRegistry` and adds an **HTTP attach** entry shape
(URL + Bearer key from `004 serve`) distinct from the existing **stdio** entries
init writes.

## Known clients (from `init.rs`)

| id | config path (from `InitPaths`) | format | entry location |
|----|--------------------------------|--------|----------------|
| `claude` (Claude Code) | `~/.claude.json` | JSON | `mcpServers.symforge` |
| `claude-desktop` | OS-specific `claude_desktop_config.json` (`claude_desktop_config_path`) | JSON | `mcpServers.symforge` |
| `codex` | `~/.codex/config.toml` | TOML | `[mcp_servers.symforge]` |
| `gemini` | `~/.gemini/settings.json` | JSON | `mcpServers.symforge` |
| `kilo-code` | `<workspace>/.kilocode/mcp.json` | JSON | `mcpServers.symforge` |
| `cursor` | `~/.cursor/mcp.json` | JSON | `mcpServers.symforge` |

Claude Desktop path resolution (`claude_desktop_config_path`):
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Linux: `~/.config/Claude/claude_desktop_config.json`

Cursor is added here (init.rs does not register it yet, but it uses the same
`mcpServers` JSON shape as Claude/Gemini/Kilo; `~/.cursor/mcp.json`). It is part
of the spec's known-harness set.

## Attach entry shape (HTTP, this feature)

The `004 serve` surface is Streamable HTTP at `/mcp` with `Authorization: Bearer
<key>`. The attach entry written by this feature is therefore **HTTP**, not the
stdio `command` entry the existing init flow writes.

JSON clients (`mcpServers.symforge`):

```json
{
  "type": "http",
  "url": "http://HOST:PORT/mcp",
  "headers": { "Authorization": "Bearer <key>" }
}
```

Codex TOML (`[mcp_servers.symforge]`):

```toml
[mcp_servers.symforge]
url = "http://HOST:PORT/mcp"
bearer_token = "<key>"
```

Note: when the serve URL is loopback with no key, the `headers`/`bearer_token`
is omitted (auth not required); the entry still reflects exactly the
operator-supplied URL + key, never a guess.

## BOM-safe parsing

`read_config_text` in `init.rs` strips a leading UTF-8 BOM (`\u{feff}`) before
parsing and the merged file is rewritten without it. This feature reuses that
exact read boundary (now `pub(crate)`), so BOM-encoded configs parse and write
back clean.

## Status comparison (scan)

Per client, `scan()` reports:
- **NotInstalled**: config path's parent dir absent (client not on the host).
- **Absent**: config exists (or parent dir exists) but has no `symforge` entry.
- **PresentCurrent**: a `symforge` entry exists whose URL + key match the target.
- **PresentStale**: a `symforge` entry exists but URL or key differs.
- **Malformed**: config exists but does not parse (reported, never overwritten).

## Backup naming / retention

`<config>.<RFC3339-compact-timestamp>.bak` written beside the config before any
write (e.g. `claude.json.20260616T130501123Z.bak`). Restore copies the backup
back over the live path byte-for-byte. No retention pruning in this slice.

## Onboarding state location + version keying

`OnboardingState { last_shown_version: Option<String> }` persisted as JSON at
`<symforge-data-dir>/onboarding.json` (`paths::resolve_symforge_dir`). The
banner shows when `last_shown_version != current_version`; on show it records
the current version. "Version" is `CARGO_PKG_VERSION` (the build version).
