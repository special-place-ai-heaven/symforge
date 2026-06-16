# Implementation Plan: SymForge Admin GUI (v8 8.1)

**Branch**: `006-v8-admin-gui` (campaign on `review/v8-004-operator-serve`) | **Date**: 2026-06-16 | **Spec**: [spec.md](./spec.md)

## Summary

Serve an operator web UI at `/admin` and a versioned `/api/v1` JSON API from the same `004` `serve` process/router, reusing `ServerRuntime` (index + STEL + ledger + auth). The UI is **embedded static assets** (single small HTML/CSS/JS bundle via `include_str!`/`rust-embed` — no npm build step) that fetch `/api/v1`. Add a **hashed API-key store** (new SQLite store mirroring `stel/ledger_store.rs`) for mint/list/rotate/revoke. `/admin`+`/api/v1` reuse the `004` Bearer auth layer AND add **Origin gating** (closes review P1-B for the browser surface). Verify with reqwest-level API tests + a headless render check.

## Technical Context

**Language**: Rust edition 2024 + minimal vanilla JS/HTML/CSS (no framework/build). **Deps**: `axum` 0.8 (routes on the serve router), `rust-embed` or `include_str!` for assets, `sysinfo` (system telemetry — confirm/aprove dep) or std, `rusqlite` (key store, already server-gated), `serde`. **Storage**: new `api-keys.db` SQLite in the SymForge data dir, table `api_keys` (hash only). **Testing**: `cargo test --all-targets -- --test-threads=1`; `/api/v1` via reqwest against an in-process `ServerRuntime`; headless render check (browser-tester/Charlotte) as a verification pass. **Constraints**: same secure-default as `004`; Origin-gated for browser; embed build unaffected (all under `#[cfg(feature="server")]`); render-evidence required (SC-001/008). **Project Type**: single crate + embedded static UI.

## Constitution Check

Stub constitution → repo gates + feature invariants:
- **GATE-1**: `/admin`+`/api/v1` enforce `004` auth on non-loopback (no open access); Origin gated (no arbitrary cross-origin).
- **GATE-2**: key store persists only hashes; raw secret shown once; revoked key rejected at `/mcp`.
- **GATE-3**: ledger-unavailable / empty-state render cleanly (no fake data, no crash).
- **GATE-4**: repo gates green; embed build clean.
- **GATE-5 (frontend)**: render evidence — `/admin` shows real data via a headless check, not just HTTP 200.

## Project Structure

```text
src/server/
├── admin/
│   ├── mod.rs        # /admin router: serve embedded UI assets; mounts api_v1
│   ├── api_v1.rs     # /api/v1/* JSON handlers (summary, surface, harness, system, keys)
│   └── assets/       # index.html, app.js, style.css (embedded; vanilla, no build)
├── api_keys.rs       # NEW SQLite hashed key store (mirror stel/ledger_store.rs): mint/list/rotate/revoke
├── auth.rs           # MODIFY: Origin gating layer (P1-B) for browser-facing routes; key check consults api_keys store + bootstrap key
├── mcp_http.rs/serve.rs # MODIFY: mount /admin + /api/v1 on the serve router behind auth
tests/
├── admin_api_v1.rs       # US1/US3: endpoints return real ledger/surface/system data; auth + Origin enforced
├── api_keys_store.rs     # US2: mint(hash-only)/list(no raw)/revoke→/mcp rejects
└── admin_render.rs       # US1: headless check /admin renders real values (or documented reqwest-level fallback)
```

**Structure Decision**: all on the existing serve router/`ServerRuntime` — one process, one auth path. UI is embedded vanilla assets (no toolchain). Key store mirrors the proven `ledger_store.rs` rusqlite pattern.

## Phase pointers
- research.md: confirm `sysinfo` dep (or std-only telemetry); rust-embed vs include_str!; Origin-gating approach in axum (header check middleware); key-hash algorithm (reuse `sha2`, already a dep).
- data-model.md: ApiKeyRecord (label, fingerprint, hash, created/rotated/revoked), and the `/api/v1` response DTOs (LedgerSummaryView, SurfaceView, HarnessStatusView, SystemSnapshot).
- contracts (folded): `/api/v1/{summary,surface,harness,system,keys}` request/response + auth/Origin rules; `/admin` serves UI.

## Dependencies
Builds on `004` (ServerRuntime, auth, StelLedgerStore) + `005` (HarnessRegistry::scan). Addresses review P1-B (Origin gating) as part of the browser surface. Out of scope: AAP panel (008), multi-key per-harness issuance beyond mint/rotate/revoke, OAuth/SSO.
