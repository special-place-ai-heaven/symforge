# Tasks: SymForge Admin GUI (v8 8.1)

**Feature dir**: `specs/006-v8-admin-gui/`. Inputs: [plan.md](./plan.md), [spec.md](./spec.md).
**Gates each phase**: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, embed-clean. Frontend: render evidence (headless) for the dashboard, not just HTTP 200. All under `#[cfg(feature="server")]`.

## Phase 1: Setup
- [x] T001 Confirm deps in `specs/006-v8-admin-gui/research.md`: `sysinfo` vs std-only telemetry; `rust-embed` vs `include_str!`; reuse `sha2` for key hashing; axum Origin-gating approach. Add only approved deps to `Cargo.toml` under `server`. → **0 net-new deps** (std telemetry, `include_str!`, existing `sha2`, OS entropy via bundled SQLite `randomblob`, header-check Origin middleware). See `research.md`.
- [x] T002 [P] Scaffold `src/server/admin/{mod.rs,api_v1.rs,assets/}` + `src/server/api_keys.rs`; register modules under `#[cfg(feature="server")]`.

## Phase 2: Foundational
- [x] T003 Implement `src/server/api_keys.rs`: SQLite `ApiKeyStore { Sqlite, Disabled }` (mirror `stel/ledger_store.rs`): table `api_keys` (id, label, fingerprint, hash, created_ms, rotated_ms NULL, revoked_ms NULL); `mint(label)->(raw, record)` (raw shown once, store hash via sha2), `list()` (no raw), `rotate(id)`, `revoke(id)`, `verify(presented)->bool` (active, non-revoked). `open_in_memory()` for tests.
- [x] T004 Modify `src/server/auth.rs`: key check consults BOTH the bootstrap `--api-key` AND `ApiKeyStore::verify`; add an Origin-gating layer for browser routes (allow same-origin/configured origins, reject arbitrary cross-origin) — closes review P1-B. → `AuthLayerState::with_key_store` + `OriginLayerState`/`apply_origin_gate`.
- [x] T005 Read adapters: `LedgerSummaryView` from `StelLedgerStore::summary()`, `SurfaceView` from `surface_profile_from_env`, `HarnessStatusView` from `005` `HarnessRegistry::scan()`, `SystemSnapshot` (PID/uptime/sessions/indexed projects/resources). Place in `api_v1.rs`.
- [x] T006 **GATE** embed isolation: `cargo check --no-default-features --features embed` clean (admin/api_keys server-gated). → PASS (36.49s, 0 warnings).

## Phase 3: US1 — Dashboard renders live (P1) 🎯 MVP
- [x] T007 [US1] `api_v1.rs`: `GET /api/v1/summary` (ledger economics), `GET /api/v1/surface`, `GET /api/v1/harness` — JSON via the T005 adapters; behind `004` auth.
- [x] T008 [US1] `admin/mod.rs`: serve embedded `index.html`+`app.js`+`style.css` at `/admin`; `app.js` fetches `/api/v1/{summary,surface,harness}` and renders the dashboard (real values; clean empty-state + ledger-unavailable state).
- [x] T009 [US1] Mount `/admin`+`/api/v1` on the serve router (`mcp_http.rs`/`serve.rs`) behind the auth+Origin layer.
- [x] T010 [P] [US1] `tests/admin_api_v1.rs`: summary/surface/harness return real data for an in-process runtime with seeded ledger rows; unauth non-loopback → 401; disallowed Origin → rejected. → 7 tests pass.
- [x] T011 [US1] Render check `tests/admin_render.rs`: headless load of `/admin` confirms real economics values appear in the DOM (browser-tester/Charlotte). If headless browser is unavailable in CI, fall back to asserting the served HTML+JS reference the endpoints AND a reqwest fetch of `/api/v1/summary` returns the seeded values — document the fallback honestly. → **Used the documented reqwest-level fallback** (no Rust headless-browser dep in the crate; zero-new-deps decision). Honestly recorded in `validation.md`.
- [x] T012 [US1] **GATE** repo gates green; dashboard render evidence recorded.

## Phase 4: US2 — API-key management (P2)
- [x] T013 [US2] `api_v1.rs`: `GET/POST /api/v1/keys` (list/mint), `POST /api/v1/keys/{id}/rotate`, `DELETE /api/v1/keys/{id}` (revoke) → `ApiKeyStore`.
- [x] T014 [US2] Admin UI keys view: list (label/fingerprint/created, never raw), mint (show raw once), rotate, revoke.
- [x] T015 [P] [US2] `tests/api_keys_store.rs`: mint stores hash only + raw shown once; minted key authenticates at `/mcp`; revoked key rejected; list never returns raw. → 3 tests pass (incl. DB-bytes scan proving raw never persisted).
- [x] T016 [US2] **GATE** repo gates green.

## Phase 5: US3 — System & ops diagnostics (P3)
- [x] T017 [US3] `api_v1.rs`: `GET /api/v1/system` → `SystemSnapshot` (PID/uptime/sessions/indexed projects/resources).
- [x] T018 [US3] Admin UI diagnostics view rendering the system snapshot.
- [x] T019 [P] [US3] Tests: `/api/v1/system` matches the runtime's real PID/sessions/indexed projects. → `tests/admin_system.rs` passes.
- [x] T020 [US3] **GATE** repo gates green.

## Phase 6: Polish
- [x] T021 [P] Close G-037/G-039/G-042 in `docs/v8-gap-closure-plan.md`; note O1-O8 coverage vs `docs/v8-admin-ui.md`.
- [x] T022 [P] `specs/006-v8-admin-gui/validation.md`: SC-001..006 evidence (incl. render).
- [~] T023 **GATE** Final: all repo gates green; embed clean; render evidence captured. Checkpoint → commit. **Merge to main / push intentionally NOT done** — work is committed to the `review/v8-004-operator-serve` branch; the orchestrator owns the checkpoint merge (per campaign isolation rules).

## Dependencies
```text
Setup(T001-2) → Foundational(T003-6) → US1(T007-12, MVP) → US2(T013-16) → US3(T017-20) → Polish(T021-23)
```

## Out of scope
AAP panel (008: G-043/044); OAuth/SSO/multi-tenant (v8 non-goal); new STEL/MCP tools.
