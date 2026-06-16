# 004 Operator Server Spine — External Review Findings (2026-06-16)

Cross-model adversarial review of `review/v8-004-operator-serve` @ `dff4bb1` (diff vs `e38afe0`). Verdict: core works (refuse-to-start verified live; transport parity; embed isolation pass; parameterized SQL; no economics double-count), but real hardening gaps. Triaged below; **must-fix before any production-routable bind**.

## P1 — must fix

- **P1-A — Compact default not enforced at `tools/call`** (`src/protocol/mod.rs:767-776` list-filter; `src/protocol/tools.rs:7997+` only `symforge`/`symforge_edit`/`status` gate on `Compact`; legacy handlers `health`/`search_text`/`get_symbol`/… have NO surface gate). A client on the default compact-3 surface can still call any legacy tool by name — `tools/list` hides them but `tools/call` runs them. FR-008 is list-only, not enforced.
  - **Fix**: central dispatch gate that rejects non-advertised tool names when profile != Full (or register only compact handlers in compact mode). Add an HTTP+stdio integration test: "legacy tool rejected on compact default".
- **P1-B — Loopback-no-key is open MCP on localhost, not "authenticated loopback"** (`src/server/serve.rs`; rmcp default `allowed_origins` empty). Any local process / browser `fetch` (Host: 127.0.0.1) can call `tools/list`+`tools/call` incl. structural edits. Origin not gated (only Host/DNS-rebind is, via rmcp `allowed_hosts`).
  - **Fix**: document loudly; set `with_allowed_origins` for browser-facing binds; consider opt-in `--insecure-loopback-open` vs. default auto-generated key; require key even on loopback in production docs.

## P2 — should fix (hardening)

- **P2-A — `constant_time_eq` length fold truncates to u8** (`src/server/auth.rs:166-178`): `(a.len() ^ b.len()) as u8` → length pairs differing by a multiple of 256 (e.g. 256 vs 0) zero the length term. Fix: `subtle::ConstantTimeEq` or fold full `usize` without `as u8` truncation.
- **P2-B — SIGTERM graceful shutdown not implemented** (`serve.rs:229,277-281`): only `ctrl_c()` wired; doc claims SIGINT/SIGTERM. Under Docker/K8s/systemd SIGTERM won't drain. Fix: on Unix `select!` ctrl_c + `unix::signal(SignalKind::terminate())`; align docs.
- **P2-C — Blocking `std::sync::Mutex<Connection>` on the async tool path** (`ledger_store.rs:246,342-383` via `finalize_symforge_with_ledger`): sync INSERT under up-to-5000ms busy-timeout can stall the tokio worker serving MCP. Fix: `spawn_blocking` / dedicated writer thread / bounded background writer; keep request path non-blocking.
- **P2-D — Poisoned mutex panics instead of degrading** (`ledger_store.rs:296,342,389,438` `lock().expect("…poisoned")`): contradicts FR-011 "never panic". Fix: `lock().unwrap_or_else(|e| e.into_inner())` or map poison → Disabled for the session.
- **P2-E — `--api-key` exposes secret via process listing** (`cli/serve.rs:19-21`, `resolve_api_key`): inline key in argv visible to `ps`/Task Manager. Fix: warn when `--api-key` used; refuse inline key on non-loopback bind; document `--api-key-env` as the only production path.
- **P2-F — `RequestGovernor` wired but unused on HTTP path** (`server/serve.rs:212`, `server/mod.rs:41-84`): created+stored, never consulted → no concurrency cap on operator server. Fix: enforce via dispatch/axum middleware, or remove the dead field until implemented.
- **P2-G — Surface conformance test doesn't exercise the production list path** (`tests/surface_default_compact.rs:28,46` uses `list_tools_for_profile`/`compact_probe_tools`; production uses `compact_surface_tools()` at `mod.rs:774`). Test can pass while prod schemas diverge. Fix: test through the `SymForgeServer` `list_tools` handler or assert `compact_surface_tools()` directly.

## P3 — minor / follow-up

- **P3-A — Ledger migration no forward-compat guard** (`ledger_store.rs:294-316`): opening a future-version DB re-applies v1 DDL and downgrades `schema_version`. Fix: if `schema_version > CURRENT` → Disabled / refuse migrate-down.
- **P3-B — Unbounded ledger growth** (`ledger_store.rs:45-64`): no retention/prune. Fix: TTL/archival or capped table; document operator maintenance. (Was already noted in tasks as a later option.)
- **P3-C — `Cargo.toml` pins `rmcp = "1.1.0"` but lock resolves 1.7.0**; `allowed_hosts` behavior depends on ≥1.7 APIs. Fix: pin exact version or document lockfile as source of truth; add a minimum-version/deny check.
- **P3-D — `[::ffff:127.0.0.1]` not treated as loopback** (`serve.rs:69-72` `IpAddr::is_loopback()` == false for IPv4-mapped). With key → binds (fine); without key → refuses (safe). Optional: normalize IPv4-mapped loopback before the policy check.

## Confirmed-good (no action)
- Non-loopback-without-key refuse-to-start: verified live (`0.0.0.0:9876` → exits before bind with the documented message).
- 401 before tool dispatch (auth wraps `/mcp`); key not logged/echoed.
- One shared `Arc<SymForgeServer>` dispatch (parity); exactly 1 durable ledger row per invocation.
- Embed isolation: no network stack in the embed tree.
- DNS-rebind via Host: mitigated (live 403 on `Host: evil.example`).

## Suggested merge gate (before production routable bind)
1. Enforce compact surface at dispatch, not just `tools/list` (P1-A).
2. Require `--api-key-env` (or generated key) even for loopback in prod docs/CI smoke; gate Origin (P1-B, P2-E).
3. Fix SIGTERM + non-blocking ledger writes + poison-safe mutex (P2-B/C/D).
4. Wire or remove `RequestGovernor` on `/mcp` (P2-F).
5. Pin rmcp explicitly; add "legacy tool rejected on compact default" HTTP integration test (P3-C, P1-A).
