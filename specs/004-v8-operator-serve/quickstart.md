# Quickstart / Validation Guide: Operator Server Spine

Validates the feature end-to-end without the published MCP harness (the dev MCP stays pinned to the npm release). Run from repo root on `main`.

## Prerequisites

```bash
cargo build --release            # builds the `server`-feature binary (default features)
```

## Scenario 1 — Serve over IP with Bearer auth (US1, FR-001/002/005)

```bash
# Start the server bound to a non-loopback address with a key
cargo run --release -- serve --listen 0.0.0.0:8787 --api-key sf_demo_key &

# From another machine (or same box), attach an MCP client / curl the handshake:
curl -s -H "Authorization: Bearer sf_demo_key" \
     -H "Content-Type: application/json" \
     -X POST http://HOST:8787/mcp \
     -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

**Expected**: HTTP 200, JSON-RPC result advertising the compact-3 tools; a follow-up `tools/call` returns results equivalent to the local stdio surface for the same repo.

**Verified (T020, 2026-06-16)** — live against the release binary on loopback
(`serve --listen 127.0.0.1:8799 --api-key sf_demo_key`): stdout printed
`http://127.0.0.1:8799/mcp`; an authenticated `POST /mcp` `tools/list` returned
`{"jsonrpc":"2.0","id":1,"result":{"tools":[…]}}` advertising the active surface
(the full surface here — the compact-3 default flip is US2/T022, not yet landed).
`tools/call(status)` parity vs `ServerRuntime::dispatch_tool_call` is asserted byte-equal
in `tests/serve_http_attach.rs`. Note: with a key configured, auth is enforced on
**every** bind including loopback (key set ⇒ `requires_auth`), so the Bearer header is
required even on `127.0.0.1`.

## Scenario 2 — Secure default: non-loopback without key refuses (US1, FR-003/SC-002)

```bash
cargo run --release -- serve --listen 0.0.0.0:8787   # no --api-key
```

**Expected**: process exits non-zero with a clear message ("refusing to bind a non-loopback address without --api-key"); nothing listens on 8787.

**Verified (T020, 2026-06-16)** — `serve --listen 0.0.0.0:8787` (no key) printed
`error: refusing to bind a non-loopback address without an API key: pass --api-key
or --api-key-env (a routable bind must be authenticated)` and exited with **code 2**;
no socket bound (refuse-to-start runs before any bind).

## Scenario 3 — Bad / missing key rejected (US1, FR-002)

```bash
cargo run --release -- serve --listen 0.0.0.0:8787 --api-key sf_demo_key &
curl -s -o /dev/null -w "%{http_code}\n" -X POST http://HOST:8787/mcp -d '{}'                       # missing
curl -s -o /dev/null -w "%{http_code}\n" -H "Authorization: Bearer wrong" -X POST http://HOST:8787/mcp -d '{}'  # wrong
```

**Expected**: both return `401`; no tool executes.

**Verified (T020, 2026-06-16)** — against the live loopback server with a key:
missing Bearer → `401`; wrong Bearer (`Authorization: Bearer wrong`) → `401`. Also
asserted in `tests/serve_auth.rs` (missing/wrong → 401; correct → not-401, request
reaches the transport).

## Scenario 4 — Compact-3 is the default surface (US2, FR-008/009/SC-004)

```bash
# Default stdio surface (no SYMFORGE_SURFACE set) — count advertised tools
SYMFORGE_SURFACE= cargo run --release -- --print-tools   # or tools/list over /mcp from Scenario 1
# Opt back into legacy full surface
SYMFORGE_SURFACE=full cargo run --release -- --print-tools
```

**Expected**: default lists exactly 3 tools (`symforge`, `symforge_edit`, `status`); `SYMFORGE_SURFACE=full` lists the legacy surface.

> If no `--print-tools` flag exists, validate via the `/mcp` `tools/list` call in Scenario 1 (default run) and a `SYMFORGE_SURFACE=full` run.

## Scenario 5 — Economics ledger survives restart (US3, FR-010/SC-003)

```bash
cargo run --release -- serve --listen 127.0.0.1:8787 --api-key k &   # drive a few tool calls
# ... issue several /mcp tool calls that record economics ...
kill %1
cargo run --release -- serve --listen 127.0.0.1:8787 --api-key k &   # restart
# query the ledger summary (via status tool or a ledger inspection path)
```

**Expected**: the post-restart ledger summary includes the pre-restart events/totals (rows present in `stel-ledger.db` `stel_ledger_events`).

## Scenario 6 — Embed build stays clean (G-045/SC-005)

```bash
cargo check --no-default-features --features embed
cargo tree --no-default-features --features embed | grep -Ei "axum|rmcp"   # expect: no matches
```

**Expected**: compiles; no `axum`/`rmcp` in the embed dependency tree.

**Verified (T012 GATE, 2026-06-16)** — after landing Phase 1 + Phase 2 scaffolds
(all `src/server/**` behind `#[cfg(feature = "server")]`):

- `cargo check --no-default-features --features embed` → `Finished` (compiles clean).
- `cargo tree --no-default-features --features embed | grep -Ei "axum|rmcp"` →
  **no matches** (grep exit code 1). Zero `axum`/`rmcp` in the embed dependency
  tree; embed isolation invariant (G-045) intact. The new `rmcp`
  `transport-streamable-http-server` feature is enabled only under the `server`
  feature and never reaches the `embed` build.

## Repo gates (run before declaring done)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
```

Detailed acceptance: see [spec.md](./spec.md) Success Criteria SC-001..006 and [contracts/](./contracts/).
