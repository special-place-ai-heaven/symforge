# SymForge v8 — Admin web UI (planning)

**Status:** PLANNED — not blocking Phase 0 or 8.0  
**Branch:** `v8/stel-architecture`  
**Companion:** [`v8-master-plan.md`](v8-master-plan.md) · [`ideation.md`](ideation.md) · [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md)

---

## Why capture this now

v8 is not only an MCP protocol change — it is a **deployable server product**. Operators need to see economics proof, index health, and credentials without reading MCP responses or CLI JSON. Planning the admin UI **up front** keeps one process, one auth story, and one SQLite stack — instead of bolting on a dashboard after `symforge serve` ships.

**Not in scope:** multi-tenant SaaS, OAuth/SSO, billing (see [`ideation.md`](ideation.md) non-goals).

---

## Where it fits in the release map

| Release | Admin / ops surface | Notes |
|---------|---------------------|-------|
| **7.x (today)** | CLI only (`symforge analytics …`); sidecar `GET /health` JSON; daemon REST | Fragmented; no unified UI |
| **8.0.0** | **`symforge_status` MCP tool** + CLI | Agent-facing battery headline; **no web UI yet** |
| **8.1.0** | **`symforge serve`** + **admin UI MVP** on same axum server | MCP `/mcp` + `/admin` + JSON API |
| **8.2+ (optional)** | Dashboard polish, charts, export UX | After H6/H8 green; not gated |

```text
Phase 0   → harness + golden file (no UI)
Phase 1–3 → STEL + L4 ledger schema in SQLite (data layer for future dashboard)
Phase 4   → symforge serve + admin MVP ships with 8.1.0
```

---

## Architecture (one process)

```mermaid
flowchart TB
  subgraph clients [Clients]
    MCP[MCP harnesses]
    Browser[Browser admin UI]
  end
  subgraph serve [symforge serve — axum]
    MCPRoute[POST /mcp]
    AdminStatic[GET /admin/* static SPA]
    AdminAPI[GET/POST /api/v1/*]
    RT[ServerRuntime / ToolExecutor]
  end
  subgraph sqlite [rusqlite — bundled]
    Analytics[(analytics.db)]
    Ledger[(stel_ledger — new v8)]
    Keys[(server_keys — new 8.1)]
    Frecency[(frecency.db — read-only views)]
  end
  MCP --> MCPRoute --> RT
  Browser --> AdminStatic
  Browser --> AdminAPI --> RT
  RT --> Analytics
  RT --> Ledger
  AdminAPI --> Keys
  AdminAPI --> Frecency
```

**Invariant:** Admin routes use the **same** `ServerRuntime` as MCP (G-034) — no duplicate tool dispatch or index handles.

---

## Existing SQLite (reuse, don’t duplicate)

| Store | Path | Today | Admin use |
|-------|------|-------|-----------|
| Analytics | `.symforge/analytics.db` | `analytics_tool_calls` — bytes, tokens, duration, outcome | Tool activity panel |
| Frecency | `.symforge/frecency.db` | File bump scores | Index/search health (read-only) |
| Coupling | coupling store | Co-change evidence | Optional “search quality” panel |
| **STEL ledger** | `.symforge/stel_ledger.db` *(planned)* | Per-row economics events (L4) | **Battery dashboard** — `session_net_accepted`, bypass rate |
| **Server config** | server-local or `.symforge/server.db` *(planned)* | API keys (hashed), bind policy, retention | Settings panel |

Rusqlite is already a dependency (`bundled` feature). Extend schemas with migrations; do not add Postgres/Redis for v8.

---

## Admin UI MVP (8.1 — Phase 4.7)

Ship with **`symforge serve`**, not before.

| Panel | Data source | Operator action |
|-------|-------------|-----------------|
| **Economics battery** | `stel_ledger_events` + cached compare-results rollup | View H1–H8 snapshot, session net |
| **Live session** | In-memory session registry + ledger tail | See active MCP sessions |
| **Projects / repos** | ProjectRegistry (from daemon merge) | List indexed roots, symbol/file counts, reindex trigger |
| **Index health** | LiveIndex published state + watcher | Status, last error, checkpoint |
| **Tool activity** | `analytics_tool_calls` aggregates | Top tools, outcome classes |
| **Settings** | `server_keys` + env | Rotate MCP API key, optional admin key, surface mode, retention |
| **System & processes** | OS metrics + session registry | CPU/RAM/disk, symforge PIDs, parent harness process when known |
| **Harness hub** | Config scanner + key store | Scan → preview → apply MCP entries per client |

**Frontend:** static assets embedded in binary (`rust-embed` / `include_dir`) or `--admin-static DIR`. Prefer small SPA or HTMX — **no separate Node server**.

---

## First-run & post-update onboarding (8.1)

**Goal:** After `symforge install` / `symforge update` / first `symforge serve`, the operator sees **where the server lives** and lands in the dashboard — not a wall of CLI flags.

### CLI / installer message (always)

```text
SymForge server running at http://127.0.0.1:8787
  MCP endpoint:  http://127.0.0.1:8787/mcp
  Admin UI:      http://127.0.0.1:8787/admin

Open admin in browser? [Y/n]  (use --no-open to skip)
Configure harnesses from the Admin → Harnesses tab, or run: symforge init --url …
```

On **Windows/macOS/Linux**: optional `webbrowser` / `xdg-open` / `start` when TTY is interactive and `--no-open` not set.

### First visit wizard (browser)

1. **Welcome** — server URL, version, bind address warning if not loopback.
2. **Create MCP API key** — generate `sf_…`, show once, store hash in `server.db`.
3. **Scan harnesses** — see Harness hub below; optional “configure all found”.
4. **Done** — land on main dashboard.

Persist `onboarding_completed` in server DB so repeat visits skip wizard unless user resets.

**Phase:** 4.7 with admin MVP; hook **`symforge update`** post-step to print admin URL (even if server not auto-started — “start with `symforge serve`”).

---

## Operations dashboard — expose everything safe

Principle: **if SymForge knows it, the operator can see it** (read-only unless action is explicit). No hidden daemon state.

| Panel | Expose | Source (today → target) |
|-------|--------|-------------------------|
| **Server** | Version, uptime, bind, transport (stdio vs HTTP), governor queue depth | `ServerRuntime` |
| **System resources** | CPU %, RAM, disk free (host); optional index dir size | `sysinfo` or lightweight crate; refresh 5s |
| **Processes** | symforge PIDs (serve, daemon child, indexer); **harness PIDs** when session links parent | session registry + `/proc` / Win32 API |
| **Sessions** | session_id, project, harness label, connected since, last tool | daemon `SessionRegistry` |
| **Projects / indexes** | repo root, file/symbol counts, index state, watcher errors, last checkpoint | `LiveIndex` + `ProjectInstance` |
| **Per-repo `.symforge/`** | analytics path, frecency, coupling, ledger — sizes only | path scan |
| **Economics** | `session_net_accepted`, gate snapshot, ledger tail | L4 SQLite |
| **Tool activity** | aggregates from `analytics_tool_calls` | existing rusqlite |
| **Logs tail** | last N lines if file logging enabled | optional 8.2 |

**Not exposed (security):** raw API key material after creation; other users’ home dirs beyond discovered config paths.

---

## Harness provisioning hub (8.1 core UX)

**Problem:** Operators should not hand-edit JSON/TOML in five app-specific locations. SymForge should **discover → preview → apply** MCP server entries with the correct URL + Bearer key per harness.

### Today (7.x baseline — `src/cli/init.rs`)

| Client | Config path | Transport today |
|--------|-------------|-----------------|
| Claude Code | `~/.claude.json` | stdio `command` spawn |
| Claude Desktop | `%APPDATA%/Claude/claude_desktop_config.json` (etc.) | stdio |
| Codex | `~/.codex/config.toml` | stdio |
| Gemini | gemini config paths in init | stdio |
| Kilo Code | `.kilocode/mcp.json` (workspace) | stdio |
| **`symforge init --client all`** | merges above | 32-tool allowlist |

**Gaps vs vision:** no **Cursor** yet; no filesystem **sweep** (fixed enum only); no **HTTP URL + API key**; no per-harness key isolation; no admin UI.

### Target (8.1)

```mermaid
flowchart LR
  Scan[Scan harness configs] --> Preview[Preview diffs]
  Preview --> Keys[Assign key per harness optional]
  Keys --> Apply[Write configs backup first]
  Apply --> Verify[Test MCP Initialize]
```

**Scan (`POST /api/v1/harnesses/scan`):**

- Known locations registry (OS-specific): Cursor, Claude, Codex, Gemini, Kilo, VS Code MCP, Windsurf, etc.
- Optional user-added search roots.
- Parse JSON / JSONC / TOML; locate `mcpServers` / `mcp_servers` blocks.
- Return: `{ id, client, path, existing_symforge_entry, writable }`.

**Configure (`POST /api/v1/harnesses/{id}/apply`):**

- Backup file → `.symforge/backups/harness/…`
- Write Streamable HTTP block:

```json
{
  "symforge": {
    "type": "streamable-http",
    "url": "http://127.0.0.1:8787/mcp",
    "headers": { "Authorization": "Bearer sf_…" }
  }
}
```

- **Per-harness keys (optional):** separate MCP keys with scope `harness:cursor`, revocable independently — stored hashed in `server.db`.
- **Compact surface:** set tool allowlist to 3 tools when `SYMFORGE_SURFACE=compact` (fixes G-036).
- Dry-run mode returns unified diff in UI before apply.

**CLI parity:** `symforge init --url http://127.0.0.1:8787/mcp --scan` calls same API or shared Rust module (`src/harness/`).

Evolve existing **`InitClient::All`** logic into **`HarnessRegistry`** — single source for scan + apply used by CLI and admin.

---

## JSON API sketch (Phase 4)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Liveness (minimal; may stay public) |
| GET | `/api/v1/stats/summary` | Ledger + analytics rollup |
| GET | `/api/v1/stats/gates` | Last compare-results gate pass/fail |
| GET | `/api/v1/projects` | Indexed projects |
| POST | `/api/v1/projects/{id}/reindex` | Trigger reindex (governor-gated) |
| GET | `/api/v1/sessions` | Open MCP sessions |
| GET | `/api/v1/settings` | Redacted config |
| POST | `/api/v1/keys/rotate` | New MCP key (local operator only) |
| POST | `/api/v1/harnesses/scan` | Discover client configs on this machine |
| GET | `/api/v1/harnesses` | Last scan results + link status |
| POST | `/api/v1/harnesses/{id}/apply` | Write MCP entry (dry-run query param) |
| POST | `/api/v1/harnesses/{id}/revoke` | Remove symforge entry; restore backup optional |
| GET | `/api/v1/system` | CPU/RAM/disk + symforge-related PIDs |
| GET | `/api/v1/onboarding/status` | Wizard state |

**Auth:**

- `Authorization: Bearer` on `/api/v1/*` and `/admin` (separate **admin** scope optional).
- Default bind **loopback**; non-loopback requires explicit flag + warning in UI (G-033).
- Retire unauthenticated standalone sidecar HTTP when unified server lands.

CLI may call the same endpoints later (`symforge stats --url …`) — API first, UI second.

---

## Phase-by-phase coding dependencies

| Phase | Admin-related work | Blocks UI? |
|-------|-------------------|------------|
| **0** | None | — |
| **1** | Optional: measure schema bytes script (A-005) | No |
| **2** | Controller emits structured trust envelope (feeds ledger) | No |
| **3** | **`StelLedgerEvent` → rusqlite** (L4); `symforge_status` = battery headline | **Yes — data model** |
| **4.1–4.3** | `symforge serve`, ServerRuntime merge, auth model | **Yes — transport + auth** |
| **4.7** | Admin static + `/api/v1/*` routes | Ships 8.1 MVP |
| **4.8** | First-run wizard + post-update URL banner + optional browser open | Onboarding UX |
| **4.9** | Harness scan/apply module (`src/harness/`) shared with CLI | Harness hub |
| **8.2+** | Charts, export CSV, dark mode, i18n | No |

**Rule:** Do not show hook `TokenStats` as v8 economics in the UI (G-NEW-4). Dashboard reads **ledger rows** only.

---

## Gap register

| ID | Gap | Closure | Phase |
|----|-----|---------|-------|
| **G-037** | No operator web UI | Admin MVP on `symforge serve` | 4.7 / 8.1 |
| **G-038** | No `stel_ledger` SQLite schema | Migration in L4 (Phase 3) | 3 |
| **G-039** | No product API-key store | Hashed keys in server DB; `init --url` | 4.4 |
| **G-040** | No first-run / post-update onboarding | CLI banner + browser open + wizard | 4.8 |
| **G-041** | No harness filesystem scan + config writer | `HarnessRegistry`; admin + CLI share logic | 4.9 |
| **G-042** | No ops telemetry in UI | System + PID + session panels | 4.7 |

Depends on: **G-020** (serve), **G-034** (ServerRuntime), **G-033** (sidecar auth), **G-030b** (init templates → evolves into G-041).

---

## Assumptions (register when implementing)

| ID | Assumption | Validate |
|----|------------|----------|
| **A-040** | Operators want local web UI, not CLI-only, when running `symforge serve` | 2-user smoke on loopback |
| **A-041** | Embedded static UI keeps deploy one-binary | Cross-platform serve test |
| **A-042** | Ledger SQLite query latency OK for dashboard refresh (<100ms on 10k rows) | Benchmark on dev machine |
| **A-043** | Users prefer scan-and-apply over manual MCP config editing | 5-user smoke; track support friction |
| **A-044** | Auto-open browser on first run is acceptable (with `--no-open` escape) | Platform smoke Win/macOS/Linux |

Add to [`stel-assumptions.md`](stel-assumptions.md) when Phase 4 planning starts — not blocking Phase 0.

---

## Decision log

| Date | Decision |
|------|----------|
| 2026-06-12 | Admin web UI planned for **8.1** with `symforge serve`; rusqlite-backed; single-tenant local ops; MVP in Phase 4.7 after L4 ledger exists |
| 2026-06-12 | **First-run onboarding:** post-install/update message with admin URL; optional browser open; wizard for key + harness scan |
| 2026-06-12 | **Harness hub:** OS scan of MCP configs, per-harness API keys, backup-then-apply — evolves `symforge init` + `InitClient::All` |

---

*Update this doc when admin scope or phase order changes; link from ideation decision log.*
