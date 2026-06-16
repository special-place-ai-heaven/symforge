# Tasks: SymForge Admin GUI (v8 8.1)

**Feature dir**: `specs/006-v8-admin-gui/`. Inputs: [plan.md](./plan.md), [spec.md](./spec.md).
**Gates each phase**: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, embed-clean. Frontend: render evidence (headless) for the dashboard, not just HTTP 200. All under `#[cfg(feature="server")]`.

## Phase 1: Setup
- [ ] T001 Confirm deps in `specs/006-v8-admin-gui/research.md`: `sysinfo` vs std-only telemetry; `rust-embed` vs `include_str!`; reuse `sha2` for key hashing; axum Origin-gating approach. Add only approved deps to `Cargo.toml` under `server`.
- [ ] T002 [P] Scaffold `src/server/admin/{mod.rs,api_v1.rs,assets/}` + `src/server/api_keys.rs`; register modules under `#[cfg(feature="server")]`.

## Phase 2: Foundational
- [ ] T003 Implement `src/server/api_keys.rs`: SQLite `ApiKeyStore { Sqlite, Disabled }` (mirror `stel/ledger_store.rs`): table `api_keys` (id, label, fingerprint, hash, created_ms, rotated_ms NULL, revoked_ms NULL); `mint(label)->(raw, record)` (raw shown once, store hash via sha2), `list()` (no raw), `rotate(id)`, `revoke(id)`, `verify(presented)->bool` (active, non-revoked). `open_in_memory()` for tests.
- [ ] T004 Modify `src/server/auth.rs`: key check consults BOTH the bootstrap `--api-key` AND `ApiKeyStore::verify`; add an Origin-gating layer for browser routes (allow same-origin/configured origins, reject arbitrary cross-origin) — closes review P1-B.
- [ ] T005 Read adapters: `LedgerSummaryView` from `StelLedgerStore::summary()`, `SurfaceView` from `surface_profile_from_env`, `HarnessStatusView` from `005` `HarnessRegistry::scan()`, `SystemSnapshot` (PID/uptime/sessions/indexed projects/resources). Place in `api_v1.rs`.
- [ ] T006 **GATE** embed isolation: `cargo check --no-default-features --features embed` clean (admin/api_keys server-gated).

## Phase 3: US1 — Dashboard renders live (P1) 🎯 MVP
- [ ] T007 [US1] `api_v1.rs`: `GET /api/v1/summary` (ledger economics), `GET /api/v1/surface`, `GET /api/v1/harness` — JSON via the T005 adapters; behind `004` auth.
- [ ] T008 [US1] `admin/mod.rs`: serve embedded `index.html`+`app.js`+`style.css` at `/admin`; `app.js` fetches `/api/v1/{summary,surface,harness}` and renders the dashboard (real values; clean empty-state + ledger-unavailable state).
- [ ] T009 [US1] Mount `/admin`+`/api/v1` on the serve router (`mcp_http.rs`/`serve.rs`) behind the auth+Origin layer.
- [ ] T010 [P] [US1] `tests/admin_api_v1.rs`: summary/surface/harness return real data for an in-process runtime with seeded ledger rows; unauth non-loopback → 401; disallowed Origin → rejected.
- [ ] T011 [US1] Render check `tests/admin_render.rs`: headless load of `/admin` confirms real economics values appear in the DOM (browser-tester/Charlotte). If headless browser is unavailable in CI, fall back to asserting the served HTML+JS reference the endpoints AND a reqwest fetch of `/api/v1/summary` returns the seeded values — document the fallback honestly.
- [ ] T012 [US1] **GATE** repo gates green; dashboard render evidence recorded.

## Phase 4: US2 — API-key management (P2)
- [ ] T013 [US2] `api_v1.rs`: `GET/POST /api/v1/keys` (list/mint), `POST /api/v1/keys/{id}/rotate`, `DELETE /api/v1/keys/{id}` (revoke) → `ApiKeyStore`.
- [ ] T014 [US2] Admin UI keys view: list (label/fingerprint/created, never raw), mint (show raw once), rotate, revoke.
- [ ] T015 [P] [US2] `tests/api_keys_store.rs`: mint stores hash only + raw shown once; minted key authenticates at `/mcp`; revoked key rejected; list never returns raw.
- [ ] T016 [US2] **GATE** repo gates green.

## Phase 5: US3 — System & ops diagnostics (P3)
- [ ] T017 [US3] `api_v1.rs`: `GET /api/v1/system` → `SystemSnapshot` (PID/uptime/sessions/indexed projects/resources).
- [ ] T018 [US3] Admin UI diagnostics view rendering the system snapshot.
- [ ] T019 [P] [US3] Tests: `/api/v1/system` matches the runtime's real PID/sessions/indexed projects.
- [ ] T020 [US3] **GATE** repo gates green.

## Phase 6: Polish
- [ ] T021 [P] Close G-037/G-039/G-042 in `docs/v8-gap-closure-plan.md`; note O1-O8 coverage vs `docs/v8-admin-ui.md`.
- [ ] T022 [P] `specs/006-v8-admin-gui/validation.md`: SC-001..006 evidence (incl. render).
- [ ] T023 **GATE** Final: all repo gates green; embed clean; render evidence captured. Checkpoint → commit, merge to main, push (per standing rule).

## Dependencies
```text
Setup(T001-2) → Foundational(T003-6) → US1(T007-12, MVP) → US2(T013-16) → US3(T017-20) → Polish(T021-23)
```

## Out of scope
AAP panel (008: G-043/044); OAuth/SSO/multi-tenant (v8 non-goal); new STEL/MCP tools.
