# Validation: SymForge Admin GUI (006)

**Date**: 2026-06-16 | **Branch**: `review/v8-004-operator-serve` | **Commit**: `fc007be`

Evidence that feature 006 (operator admin GUI) satisfies its success criteria
(SC-001..SC-006) and functional requirements. All work is `#[cfg(feature =
"server")]`; the embed build is unaffected.

## Render-verification method (HONEST)

The spec/T011 asks for a **headless-browser** render check. **No Rust
headless-browser dependency exists in this crate** (`Cargo.toml` has no
`chromiumoxide` / `headless_chrome` / `fantoccini`), and feature 006's
`research.md` committed to **zero new dependencies**. Adding a browser-driver
crate to the Rust test suite (heavy, CI-brittle) was out of scope.

Therefore the automated render check uses the **explicitly-sanctioned fallback**
from `tasks.md` T011 (`tests/admin_render.rs`):

1. the served `/admin` HTML references `app.js` + `style.css` and contains the
   dashboard view containers the JS binds into;
2. the served `/admin/app.js` references every `/api/v1` endpoint the dashboard
   fetches **and** binds the real economics fields (`total_events`,
   `total_net_vs_manual`) into DOM cards;
3. a `reqwest` fetch of `/api/v1/summary` returns the **seeded** economics
   values (events=4, net=840) — the real data a browser would paint.

This proves the data is real and the end-to-end HTTP wiring is correct. It does
**NOT** claim a browser painted pixels. Chrome IS installed on the host
(`C:\Program Files\Google\Chrome\Application\chrome.exe`); a live manual headless
render against a running `symforge serve` is the recommended supplementary
verification and is left to the operator/CI with a browser driver.

## Success criteria

### SC-001 — Operator loads `/admin` and sees real economics + system state

**Evidence** (reqwest-level fallback, documented above):
- `tests/admin_render.rs::admin_page_references_endpoints_and_summary_has_real_values`
  — HTML loads app.js/style.css, JS fetches all endpoints + binds economics
  fields, `/api/v1/summary` returns seeded `total_events=4`,
  `total_net_vs_manual=840` (not placeholders/zeros). **PASS**
- `tests/admin_api_v1.rs::summary_returns_seeded_economics` — `/api/v1/summary`
  returns `available=true, total_events=3, total_net_vs_manual=630`. **PASS**
- `tests/admin_api_v1.rs::admin_html_is_served` — `/admin` returns 200
  `text/html` containing "SymForge Admin" + `/admin/app.js`. **PASS**

### SC-002 — 100% of non-loopback unauth requests rejected

**Evidence**:
- `tests/admin_api_v1.rs::unauth_keyed_request_is_rejected` — keyed runtime
  (auth required), no Bearer → 401; correct key → 200 with real data. **PASS**
- Reuses the proven `004` `apply_bearer_auth` layer (no logic fork); the
  existing `tests/serve_auth.rs` battery (5 tests) still passes. **PASS**

### SC-003 — Minted key authenticates at `/mcp`; revoked rejected; raw never retrievable

**Evidence**:
- `tests/api_keys_store.rs::minted_key_authenticates_at_mcp_revoked_rejected` —
  a minted key authenticates at `/mcp` (not 401, success); after `revoke` the
  same key is 401. Bootstrap key still works; wrong key 401. **PASS**
- `tests/api_keys_store.rs::mint_persists_hash_only_raw_shown_once` — scans the
  persisted `api-keys.db` bytes and asserts the **raw secret is never present**
  (hash-only); `list` JSON never contains the raw secret. **PASS**
- `tests/api_keys_store.rs::list_never_returns_raw_after_reopen` — after reopen
  the key still verifies (hash persisted) but `list` cannot reveal the raw.
  **PASS**
- Store unit tests (`src/server/api_keys.rs`): mint/rotate/revoke/verify,
  disabled-store degradation, persistence round-trip, per-mint uniqueness,
  empty-secret rejection. **PASS** (lib tests green).

### SC-004 — Dashboard renders empty-state and ledger-unavailable state without error

**Evidence**:
- `tests/admin_api_v1.rs::disabled_ledger_renders_unavailable_not_fake_zeros` —
  a `Disabled` ledger yields `available=false` with `total_events=null` (no
  fabricated zeros). **PASS**
- `src/server/admin/api_v1.rs` unit tests:
  `ledger_view_unavailable_when_no_store`, `ledger_view_unavailable_when_disabled`,
  `ledger_view_reports_real_values_when_seeded`. **PASS**
- The UI (`app.js`) renders distinct "unavailable" (`available=false`) and
  "empty" (`total_events=0`) notes vs. real value cards (FR-006).

### SC-005 — `/api/v1/system` matches real PID/sessions/indexed projects

**Evidence**:
- `tests/admin_system.rs::system_snapshot_matches_real_runtime_state` — PID
  equals `std::process::id()`, `active_sessions=1`, `indexed_projects` equals
  the runtime's project name, index telemetry present (file_count=0 for empty
  index, generation present, uptime present). **PASS**
- `tests/admin_api_v1.rs::system_returns_real_pid` — `/api/v1/system` PID matches
  the process; sessions=1. **PASS**

### SC-006 — No open cross-origin access; disallowed Origin refused

**Evidence**:
- `tests/admin_api_v1.rs::disallowed_origin_is_rejected` — a request with
  `Origin: http://evil.example.com` → 403; a same-origin request → 200 with real
  data. **PASS**
- `src/server/auth.rs` unit tests: `origin_gate_permits_same_origin_and_loopback_aliases`,
  `origin_gate_rejects_foreign_origin`, `origin_gate_explicit_allow_list`.
  **PASS**
- Closes review finding **P1-B** for the browser-facing surface.

## Functional requirement coverage

| FR | Where | Status |
|---|---|---|
| FR-001 `/admin` + `/api/v1/*` same process/port | `admin/mod.rs` merged onto serve router | met |
| FR-002 same auth contract as `004` `/mcp` | reuses `apply_bearer_auth` + `AuthLayerState` | met |
| FR-003 real ledger economics; unavailable not fabricated | `LedgerSummaryView::from_runtime` | met |
| FR-004 hashed key store: mint/list/rotate/revoke; revoked rejected | `ApiKeyStore` + auth integration | met |
| FR-005 `/api/v1/system` telemetry | `SystemSnapshot::from_runtime` | met |
| FR-006 empty/degraded states render | `app.js` unavailable/empty notes | met |
| FR-007 Origin/Host protections | `OriginLayerState` + `apply_origin_gate` | met |
| FR-008 automated render check (real data) | `tests/admin_render.rs` (documented reqwest fallback) | met w/ caveat |

## Gate results

See the final report / `docs/v8-gap-closure-plan.md`. All repo gates
(`cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets -- --test-threads=1`, `cargo build --release`,
`cargo check --no-default-features --features embed`) green at commit `fc007be`+.

## O1-O8 coverage (vs `docs/v8-admin-ui.md`)

This feature delivers the **O1 (partial)** and **O4** operator-acceptance items;
O2/O3 (wizard, URL banner, browser-open) and O5-O8 (harness apply hub) are
later phases (4.8/4.9). See gap-closure plan note.

- **O1** `/mcp`, `/admin`, `/api/v1/*` on documented bind — **/admin + /api/v1
  delivered** (/mcp pre-existing from 004). Browser-open / wizard (O2/O3) not in
  scope here.
- **O4** Dashboard: economics summary, indexed repos, active sessions, system
  resources, symforge PID — **delivered** (resources = std-only telemetry;
  host RAM/CPU deferred, see research.md D1).
- **O5-O8** (harness scan ≥3 clients, apply, per-harness keys, shared registry)
  — the admin **reads** harness status via the shared `005` `HarnessRegistry`
  (no duplicate logic, supports O8); apply/per-harness-key UX is phase 4.9.
