# Implementation Plan: SymForge Operator Server Spine (v8 8.1)

**Branch**: `004-v8-operator-serve` (work on `main`, uncommitted, under autonomous goal) | **Date**: 2026-06-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/004-v8-operator-serve/spec.md`

## Summary

Promote the existing in-process axum HTTP server (`src/sidecar`) into a transport-agnostic **`ServerRuntime`** that owns the index + STEL economics + request governor + auth. Expose the MCP surface over **Streamable HTTP at `/mcp`** (via rmcp's streamable-http server transport) behind **Bearer auth** that is secure-by-default for non-loopback binds. Make the **compact-3 STEL surface the default** `tools/list` (with `SYMFORGE_SURFACE=full` opt-out). Persist the **STEL L4 ledger to a dedicated SQLite store** mirroring `src/analytics/store.rs`. All new code lives behind the existing `server` cargo feature so the `embed` build stays free of axum/rmcp.

## Technical Context

**Language/Version**: Rust, edition 2024 (workspace pinned)

**Primary Dependencies**: `rmcp` 1.1.0 (enable streamable-http **server** transport feature — exact flag confirmed in research), `axum` 0.8, `tokio`, `socket2`, `rusqlite` (bundled; already pulled in by `analytics`), `clap` (CLI), `parking_lot`, `arc-swap`

**Storage**: dedicated SQLite database (WAL mode) in the SymForge data dir (`paths::ensure_symforge_dir`), table `stel_ledger_events`; persistence pattern mirrors `src/analytics/store.rs` `SqliteAnalyticsStore`

**Testing**: `cargo test --all-targets -- --test-threads=1`; new integration tests under `tests/` driving the HTTP surface with an in-process client; ledger via an in-memory store (`open_in_memory`, mirroring analytics tests)

**Target Platform**: Windows + Linux + macOS (cross-platform single server)

**Project Type**: single Rust crate (`lib` + `bin`), MCP server + CLI

**Performance Goals**: network `/mcp` tool-call latency parity with stdio — in-process dispatch, **no extra inter-process proxy hop** (G-022); ledger writes off the request hot path

**Constraints**: secure-by-default (no unauthenticated non-loopback bind, G-033); `embed` build free of axum/rmcp (G-045); one active project per server session (carried product constraint)

**Scale/Scope**: single-machine server, multiple concurrent harness connections sharing one index + governor + ledger

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is an unfilled template stub — no project-specific principles are encoded. Apply the repository's own binding gates (`CLAUDE.md` Verification + `docs/v8-gap-closure-plan.md`):

- **GATE-1 — Embed isolation (G-045)**: `cargo check --no-default-features --features embed` MUST compile with zero `axum`/`rmcp`/network deps. Satisfied by design — all serve code is behind `#[cfg(feature = "server")]`.
- **GATE-2 — Secure default (G-033)**: a non-loopback bind without a configured key MUST refuse to start; a configured key is enforced on all binds.
- **GATE-3 — Transport parity (FR-005/SC-006)**: `/mcp` tool results equal stdio results on a shared battery; no economics double-count across transports.
- **GATE-4 — Repo gates**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release` all green.
- **GATE-5 — Single runtime (G-034)**: one in-process `ServerRuntime` owns index + STEL + governor + auth; transports are thin adapters.

No constitution violations to justify. Complexity Tracking below is empty.

## Project Structure

### Documentation (this feature)

```text
specs/004-v8-operator-serve/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── cli-serve.md
│   ├── http-mcp.md
│   ├── stel-ledger-store.md
│   └── surface-default.md
└── tasks.md             # Phase 2 output (/speckit-tasks)
```

### Source Code (repository root)

```text
src/
├── server/                 # NEW module (transport-agnostic runtime; #[cfg(feature="server")])
│   ├── mod.rs              # ServerRuntime: owns SharedIndex + STEL state + RequestGovernor + AuthConfig
│   ├── serve.rs            # `symforge serve` async entrypoint: bind (socket2, reuse sidecar pattern), mount /mcp, graceful shutdown
│   ├── auth.rs             # Bearer extraction + constant-time verify; loopback-exemption rule (FR-002..004)
│   └── mcp_http.rs         # rmcp streamable-http server transport mounted as an axum route at /mcp
├── cli/
│   └── serve.rs            # NEW clap subcommand `serve` (--listen, --api-key, --api-key-env)
├── stel/
│   └── ledger_store.rs     # NEW SQLite StelLedgerStore (mirror analytics/store.rs); table stel_ledger_events
├── protocol/
│   └── surface_probe.rs    # MODIFY: default SurfaceProfile Full -> Compact (FR-008); keep `full`/`meta` opt-outs
├── main.rs                 # MODIFY: route `serve` subcommand to server::serve (new run path beside run_remote_mcp_server_async)
└── lib.rs                  # MODIFY: add `#[cfg(feature="server")] pub mod server;`

Cargo.toml                  # MODIFY: enable rmcp streamable-http server transport feature

tests/
├── serve_auth.rs                 # US1: non-loopback no-key refuses; bad key 401; loopback+key enforced
├── serve_http_attach.rs          # US1: attach over /mcp with Bearer; tools/list + a tool call parity vs stdio
├── surface_default_compact.rs    # US2: default tools/list == compact-3; SYMFORGE_SURFACE=full restores legacy
└── stel_ledger_persistence.rs    # US3: events survive restart; degraded store keeps serving
```

**Structure Decision**: Extend the single crate. A new `src/server/` module is the transport-agnostic `ServerRuntime`, reusing `sidecar`'s socket2/SO_REUSEADDR bind and the existing `RequestGovernor`. The existing `src/sidecar` (hook REST endpoints) is left intact; `serve` is the product-grade MCP transport. The protocol tool-dispatch (`src/protocol` `McpServer`) is shared by both stdio and HTTP — no logic fork (GATE-3/GATE-5).

## Complexity Tracking

> No constitution violations. No entries.

## Salvage reference

Branch `backup/local-v8-0612-b4ac04c` holds a prior prototype `ServerRuntime`, admin shell, and an rmcp spike (`docs/research/A-031-rmcp-spike.md`). Reference for shape only — reimplement against main's Phase-2 STEL; do not cherry-pick (it was built on the abandoned Friday STEL skeleton).
