# Phase 0 Research: Operator Server Spine

All Technical-Context unknowns resolved below. Each: Decision / Rationale / Alternatives.

## R1 — Streamable HTTP transport for `/mcp`

**Decision**: Mount rmcp's **streamable-http server transport** as an axum route at `/mcp`, served by the same `ServerRuntime`. The current `rmcp = { version = "1.1.0", features = ["transport-io"] }` (stdio only) gains the streamable-http server transport feature.

**VERIFIED (T001, 2026-06-16)** — inspected the installed crate source under
`~/.cargo/registry/src/index.crates.io-*/rmcp-1.1.0/` (and cross-checked the
lockfile-resolved `rmcp-1.7.0`). A server-side streamable-http transport **does
exist** in 1.1.0; no hand-rolled fallback is required.

- **Feature flag**: `transport-streamable-http-server`. In `rmcp-1.1.0/Cargo.toml`
  `[features]` it pulls in `transport-streamable-http-server-session`,
  `server-side-http`, and `transport-worker`. Enabled in `Cargo.toml` as
  `rmcp = { version = "1.1.0", features = ["transport-io", "transport-streamable-http-server"], optional = true }`
  under the `server` feature. `cargo check` resolves it (T002 done).
- **API path**: `rmcp::transport::streamable_http_server::{StreamableHttpService, StreamableHttpServerConfig}`
  (re-exported from `transport/streamable_http_server.rs`), plus the session
  manager `rmcp::transport::streamable_http_server::session::local::LocalSessionManager`.
- **Constructor** (identical in 1.1.0 and 1.7.0):
  ```rust
  StreamableHttpService::new(
      service_factory: impl Fn() -> Result<S, std::io::Error> + Send + Sync + 'static,
      session_manager: Arc<M>,            // e.g. Arc<LocalSessionManager>
      config: StreamableHttpServerConfig,
  ) -> StreamableHttpService<S, M>
  ```
  `S: rmcp::Service<RoleServer>`. `StreamableHttpService` implements
  `tower_service::Service<http::Request<_>>` (error `Infallible`), so it mounts
  directly as an axum route at `POST /mcp` (+ the GET/DELETE stream half in
  stateful mode). US1/T013 wires the `service_factory` to build the protocol
  server from the shared `ServerRuntime` state.

**Lockfile note**: `Cargo.toml` pins `rmcp = "1.1.0"` (floor); `Cargo.lock`
resolves to `rmcp 1.7.0`. The feature name and `StreamableHttpService::new`
signature are identical across both, so the implementation is version-stable.

**Rationale**: rmcp is already the MCP implementation for stdio; reusing its server transport keeps one protocol code path and avoids a bespoke MCP framing layer. Streamable HTTP is the current standard MCP remote transport that harnesses (Claude Code/Codex/etc.) speak.

**Alternatives**: (a) SSE transport — older/deprecated MCP remote style; rejected. (b) Custom JSON-RPC-over-HTTP — duplicates rmcp framing; rejected unless 1.1.0 truly lacks the server transport.

## R2 — Transport-agnostic `ServerRuntime` (G-034)

**Decision**: Introduce `server::ServerRuntime` that owns `SharedIndex` + STEL state + `RequestGovernor` + `AuthConfig`, and exposes one tool-dispatch entry that delegates to the existing `protocol` `McpServer` handler (same code stdio uses). Both stdio and `/mcp` call this in-process — no proxy hop (G-022).

**Rationale**: `main.rs` already has `run_local_mcp_server_async` / `run_remote_mcp_server_async` and `src/sidecar` already owns a `SharedIndex` + `RequestGovernor` + axum server. The runtime generalizes that ownership; `protocol::McpServer` already implements tool dispatch, so no logic fork (GATE-5, prevents economics double-count).

**Alternatives**: keep daemon string-map proxy (gap-plan pivot) — rejected for the request hot path because it reintroduces an IPC hop (G-022); the daemon map stays only for legacy stdio/daemon reuse.

## R3 — Authentication (G-033, FR-002..004)

**Decision**: Bearer token via an axum auth layer in front of `/mcp`. `AuthConfig` holds an optional single static key (from `--api-key` or `--api-key-env`). Rule: if a key is configured it is enforced on **every** request (constant-time compare); if no key is configured, requests are allowed **only** when the bind is loopback (`127.0.0.0/8`, `::1`). A non-loopback bind with no key **refuses to start** (checked at `serve` startup, before binding).

**Rationale**: secure-by-default; constant-time compare prevents timing oracles. Loopback exemption keeps local single-user convenience.

**Constant-time compare**: use a vetted helper (`subtle::ConstantTimeEq` or `ring`/`constant_time_eq`); if adding a dep is undesirable, a manual fold-compare over equal-length byte slices is acceptable and unit-tested. Confirm an existing dep first.

**Alternatives**: mTLS (overkill for single-user/team spine; deferred), per-request HMAC (no benefit over Bearer for this model).

## R4 — Durable STEL ledger store (G-038, FR-010/011)

**Decision**: New `stel::ledger_store::StelLedgerStore` enum `{ Sqlite(SqliteStelLedgerStore), Disabled }`, a near-clone of `analytics::store` `AnalyticsStore`/`SqliteAnalyticsStore`. Dedicated DB file (e.g. `stel-ledger.db`) in `paths::ensure_symforge_dir`, WAL + busy timeout, `migrate()` creating table `stel_ledger_events`, `record()` / `recent()` / `summary()`. The in-memory `SessionLedger` (`src/stel/ledger.rs`) stays as the per-session view; `capture_ledger` additionally writes through to the store when present. If the store fails to open, the runtime logs and runs **Disabled** (serving continues, economics reported unavailable — FR-011).

**Rationale**: `analytics/store.rs` already proves the rusqlite pattern in this repo (open/open_in_memory/migrate/schema_version/record/recent/summary/retention, bounded text). Mirroring it minimizes risk and matches conventions. Separate DB keeps analytics and economics lifecycles independent.

**Alternatives**: extend the analytics DB with a new table — rejected (couples two independent concerns, complicates retention/versioning). JSON-lines file — rejected (no query surface for the future admin GUI).

## R5 — Compact-3 as default surface (FR-008/009)

**Decision**: In `protocol::surface_probe::surface_profile_from_env`, change the default (no/unrecognized `SYMFORGE_SURFACE`) from `SurfaceProfile::Full` to `SurfaceProfile::Compact`. Keep explicit `full` and `meta` values. `compact` continues to work. Document the `full` opt-out.

**Rationale**: the compact-3 path is fully built and wired (`compact_surface_tools()` consumed at `protocol/mod.rs:700`); only the default arm flips. Backward-compatible escape (`SYMFORGE_SURFACE=full`) prevents a silent break for clients depending on 32 tools.

**Risk**: blast radius on existing stdio installs. Mitigation: gated by the documented env opt-out; ship as the v8 surface cutover; conformance tests assert both default-compact and `full`-restores-legacy.

**Alternatives**: keep env-gated default Full and only flip at a later release — rejected (the spec/vision makes compact the v8 default; deferring contradicts the product promise).

## R6 — Embed isolation (G-045)

**Decision**: All new code (`src/server/**`, `cli/serve.rs`, `stel/ledger_store.rs` if it pulls rusqlite — note rusqlite is already only under `server` via analytics) lives behind `#[cfg(feature = "server")]`. Add a CI/test assertion that `cargo check --no-default-features --features embed` builds clean.

**Rationale**: preserves the existing `embed = []` isolation invariant the AAP path depends on.

**Alternatives**: none — this is a hard invariant.
