# 006 Admin GUI — Live Render Evidence (2026-06-16)

Live browser verification (Charlotte / real headless Chromium) of `/admin` on the built release binary, per the project's frontend render-evidence rule. **Verdict: PASS** — renders as a true operator dashboard with real, non-faked data and clean empty-states.

## Setup
- Built `symforge.exe` (release) from `symforge-review`, ran `serve --listen 127.0.0.1:8799` (loopback, no key → browser-openable dev mode).
- `/admin` → HTTP 200; `/api/v1/summary` → `{"available":true,"total_events":0,...}` (fresh server = empty ledger).

## Verified rendered (1440x900 desktop)
- **Dashboard shell**: banner/nav/main/contentinfo landmarks; tabs dashboard/keys/diagnostics with working click handlers.
- **Economics**: clean empty-state ("No economics activity recorded yet.") — matches `summary` API; no crash/fake/NaN.
- **Surface**: PROFILE=`compact`, TOOLS=3 (`symforge, symforge_edit, status`) — matches `surface` API.
- **System (diagnostics)**: real PID `45328`, uptime ~325s, sessions 1, indexed files 424, symbols 17145, project `symforge-review` — matches `system` API.
- **Harness**: 6 real clients with real states (Claude Code/Desktop/Codex/Gemini/Cursor = present-stale, Kilo = not-installed) + real config paths — matches `harness` API.
- **Keys**: empty-state + mint form — matches `keys` API (`[]`).
- **Network**: all of `/admin`, `/admin/style.css`, `/admin/app.js`, `/api/v1/{summary,surface,harness,keys,system}` → 200; 10 `/api/v1` calls, 0 non-200; Refresh re-fetch works.

## Findings (minor — tracked, non-blocking)
- **MINOR (responsive)**: at 390x844 the Harness table's long config paths cause horizontal overflow (`scrollWidth 615 > innerWidth 390`). Desktop unaffected. Fix: wrap/scroll-contain or truncate paths. → follow-up polish.
  - **RESOLVED (2026-06-16):** the harness and keys tables are now wrapped in a `.table-scroll` horizontal-scroll container (`overflow-x: auto`, width-bounded), and a `@media (max-width: 480px)` rule wraps long config paths (`overflow-wrap: anywhere`) on the `.path-cell` plus reduces `main` padding and lets cards flex full-width. Desktop layout unchanged. Files: `src/server/admin/assets/{style.css,app.js,index.html}`. Asset-guard test: `mobile_overflow_guards_present_in_assets`.
- **COSMETIC**: `GET /favicon.ico` → 404 (the only console error). Fix: add favicon or a no-op route. → follow-up polish.
  - **RESOLVED (2026-06-16):** an embedded SVG favicon (`FAVICON_SVG` in `src/server/admin/mod.rs`) is served at both `/favicon.ico` (the browser's default request) and `/admin/favicon.svg` (linked from `index.html` via `<link rel="icon">`), returning `200 image/svg+xml`. No binary asset / build step (matches the `include_str!`-only policy). Asset-guard test: `favicon_asset_present_and_linked`.

## Not verified here (honest)
- Mint-key write path not exercised (only empty-state render).
- Non-empty economics render not testable on a fresh server (total_events=0).
- Keyboard-only tab traversal not exhaustively driven.

Screenshots (gitignored `.charlotte-evidence/`): 01-dashboard, 02-diagnostics, 03-keys, 04-mobile.
