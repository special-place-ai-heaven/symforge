# Data Model: Harness Onboarding & Config Hub

## HarnessTarget

A known MCP client and how its SymForge attach entry is expressed.

```text
HarnessTarget {
    id: HarnessId,            // claude | claude-desktop | codex | gemini | kilo-code | cursor
    config_path: PathBuf,     // resolved from home/workspace (init.rs knowledge)
    format: HarnessFormat,    // Json | Toml
}
```

`HarnessFormat`:
- `Json` — `mcpServers.symforge` object (Claude Code, Claude Desktop, Gemini,
  Kilo Code, Cursor). HTTP entry: `{ type, url, headers.Authorization }`.
- `Toml` — `[mcp_servers.symforge]` table (Codex). HTTP entry: `url`,
  `bearer_token`.

Gemini settings is structurally the same `mcpServers` JSON object, so it uses
the `Json` format rather than a distinct variant.

## AttachEntry

The SymForge MCP server entry written into a client config.

```text
AttachEntry {
    url: String,           // 004 serve URL, e.g. http://127.0.0.1:8787/mcp
    bearer_key: Option<String>,  // 004 Bearer key; None for keyless loopback
}
```

## HarnessStatus / HarnessState

Result of `scan()` per client.

```text
HarnessStatus {
    id: HarnessId,
    config_path: PathBuf,
    format: HarnessFormat,
    state: HarnessState,
}

HarnessState =
    NotInstalled        // client config dir absent
  | Absent              // config present, no symforge entry
  | PresentCurrent      // symforge entry matches target url+key
  | PresentStale        // symforge entry present but differs
  | Malformed(String)   // config exists but unparseable (reported, never written)
```

## ApplyPlan / PlannedChange

```text
ApplyPlan { entry: AttachEntry, changes: Vec<PlannedChange> }

PlannedChange {
    id: HarnessId,
    config_path: PathBuf,
    format: HarnessFormat,
    action: PlannedAction,   // Add | Refresh | Skip(reason) | Error(reason)
}
```

## ApplyOutcome

```text
ApplyOutcome =
    Wrote { id, config_path, backup: Option<BackupRecord> }
  | Skipped { id, reason }
  | Failed { id, reason }
```

## BackupRecord

```text
BackupRecord {
    source: PathBuf,   // live config path
    backup: PathBuf,   // <config>.<timestamp>.bak
}
```

## OnboardingState

```text
OnboardingState { last_shown_version: Option<String> }
```

Persisted as JSON at `<symforge-data-dir>/onboarding.json`.
