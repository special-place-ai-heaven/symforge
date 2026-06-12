# SymForge v8 — AAP integration (committed)

**Status:** **COMMITTED** — retain and improve tight integration with [Agent Army Professionals](E:\project\Agent_Army_Professionals)  
**Branch:** `v8/stel-architecture`  
**Companion:** [`v8-admin-ui.md`](v8-admin-ui.md) · [`embed.rs`](../src/embed.rs) · AAP `crates/aap-code-intel/`

---

## Two integration paths (both stay first-class)

AAP is **not** “just another MCP harness.” It uses a **library embed** path today; MCP/HTTP is additive for ops and optional AAP features.

| Path | Mechanism | Consumer | v8 rule |
|------|-----------|----------|---------|
| **AAP embed (primary)** | `symforge` crate, `default-features = false`, `features = ["embed"]` | `aap-code-intel` → `NativeSymForgeCodeIntelBridge` → `CodeIntelPort` | **Semver-stable `symforge::embed` facade** — STEL/MCP must not break embed CI |
| **AAP guest agent** | Same engine linked in `aap-guest-agent` (in-VM) | vsock code-intel ops | Same embed engine; watcher/index contracts from AAP smoke tests |
| **MCP / HTTP (secondary)** | `symforge serve` `/mcp` or legacy stdio | AAP MCP playground, external agents, admin | STEL compact surface; optional AAP-specific URL preset in admin |

```text
Agent_Army_Professionals/
  crates/aap-code-intel/     ← single adapter file (adapter.rs) on symforge engine
  symforge = { path = "../symforge", features = ["embed"] }

symforge/
  src/embed.rs               ← semver-public contract (compile-time tripwire tests)
  src/stel/                  ← MCP-only (8.0+) — NOT compiled in embed builds
```

**Hard rule:** v8 MCP work (`src/stel/`, rmcp, admin UI) lives behind the `server` feature. **`embed` builds must remain free of axum/rmcp/clap** (already enforced in `Cargo.toml` + CI).

---

## What AAP uses today (must keep working)

| AAP surface | SymForge dependency |
|-------------|---------------------|
| `CodeIntelPort` (agents) | `LiveIndex`, search, refs, outlines via adapter |
| `CodeIntelBridge` / code-map packages | Same engine + formatting paths |
| Backend orchestration | `install_native_code_intel_bridge` |
| Guest agent (Firecracker VM) | In-process symforge via `aap-code-intel` |
| CI | Sibling checkout `../symforge`, pin in `Cargo.lock`, embed + musl jobs |

Existing SymForge tests shaped for AAP: `tests/watcher_aap_shaped_fixture.rs`, embed contract in `src/embed.rs`, find_dependents AAP collision test.

---

## Committed improvements (roadmap)

### E1 — Embed contract gate (blocks 8.0 tag)

| ID | Requirement |
|----|-------------|
| **E1** | `cargo test -p symforge --features embed` + embed contract module pass on every release |
| **E2** | AAP sibling CI job (or documented local gate): `Agent_Army_Professionals` builds against v8 branch before symforge tag |
| **E3** | Breaking `symforge::embed` API → **MAJOR** semver bump + coordinated AAP adapter change (single file) |

### E2 — Engine parity (8.0 / 8.1)

| ID | Requirement |
|----|-------------|
| **E4** | T2/T3/index fixes in `live_index/` benefit **both** MCP and embed (same L3 handlers) |
| **E5** | Economics ledger (L4) is MCP-session scoped; embed exposes optional **read-only stats** hook for AAP telemetry (no STEL controller inside embed) |

### E3 — Operator convenience (8.1 — with admin UI)

| ID | Requirement |
|----|-------------|
| **E6** | Admin **AAP panel**: detect sibling `../Agent_Army_Professionals`, show embed pin version, index health for AAP-open projects |
| **E7** | **One-click presets**: “AAP native embed” (no MCP edit) vs “AAP + symforge serve URL” for MCP playground / external agents |
| **E8** | Harness hub **recognizes AAP** configs (backend env, MCP server CRUD in AAP DB) — separate from Cursor/Claude JSON sweep; backup-before-write |
| **E9** | Post-install banner mentions **both** `/admin` and AAP embed path when sibling repo detected |

### E4 — Optional future (not 8.1 blockers)

- Shared `symforge serve` instance for AAP host + multiple harnesses (one index, AAP backend + Cursor clients).
- Vsock proxy from AAP host to guest-agent intel (document only until requested).

---

## Admin UI: AAP panel (Phase 4.7)

Add to committed operator stack ([`v8-admin-ui.md`](v8-admin-ui.md) O4 extension):

| Widget | Purpose |
|--------|---------|
| **Integration mode** | Embed active / MCP URL configured / both |
| **Sibling repo** | `E:\project\Agent_Army_Professionals` or `../Agent_Army_Professionals` detected |
| **Versions** | symforge crate version ↔ AAP `Cargo.lock` pin (warn if drift) |
| **Projects** | AAP-indexed roots visible when backend registers them (via API or filesystem probe) |
| **Actions** | Open admin docs; copy embed `Cargo.toml` snippet; “Register serve URL in AAP MCP settings” (E8) |

---

## Harness hub: AAP is a special case (Phase 4.9)

Generic harness scan (O5–O8) targets **MCP client JSON/TOML** (Cursor, Claude, Codex).

**AAP-specific scan** (committed, **A9**):

1. Detect AAP workspace root (env `AAP_ROOT` or sibling path).
2. Read AAP MCP server records / backend config (not only `~/.cursor/mcp.json`).
3. Offer presets:
   - **Embed-only** (default for AAP agents) — verify `aap-code-intel` path dep only.
   - **HTTP MCP** — write serve URL + key into AAP’s MCP server UI backing store or config template.
4. Never overwrite AAP embed path dep with stdio spawn config (7.x init anti-pattern).

---

## Phase map

| Phase | AAP work |
|-------|----------|
| **0** | No AAP code changes; optional AAP row in golden/bench corpus already present |
| **1–3** | STEL under `server` feature; **embed CI green**; adapter.rs only if query API shifts |
| **4.7** | Admin AAP panel (E6, E9) |
| **4.9** | AAP harness presets (E7, E8, A9) |
| **8.1 tag** | E1–E3 + operator O1–O8; AAP embed must build on pinned symforge |

---

## Gap register

| ID | Gap | Closure | Blocks |
|----|-----|---------|--------|
| **G-043** | Embed contract not in v8 release gate | E1/E2 CI + semver policy in CHANGELOG | **8.0** embed consumers |
| **G-044** | No AAP-specific operator path | Admin AAP panel + harness presets (E6–E9, A9) | **8.1** convenience |
| **G-045** | STEL accidentally linked into embed | `cfg(feature = "server")` audit in Phase 1 | **8.0** |

Detail in [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md) §3.8.

---

## Decision log

| Date | Decision |
|------|----------|
| 2026-06-12 | **AAP embed path is non-negotiable** — v8 MCP/STEL/admin must not regress `symforge::embed` or `aap-code-intel` |
| 2026-06-12 | **Improve convenience** via admin AAP panel + dedicated harness presets, not by forcing AAP through generic MCP-only setup |

---

*Repo path: `E:\project\Agent_Army_Professionals` · SymForge sibling: `E:\project\symforge`*
