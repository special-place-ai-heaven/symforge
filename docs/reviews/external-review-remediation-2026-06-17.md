# SymForge — External Review Remediation (2026-06-17)

Tracks remediation of the senior Rust / security / systems external review of
`origin/main` @ `4044767` (released 7.31.0). Work branch:
`fix/v8-review-remediation`.

Every finding was **verified against live code before action** — code is gospel.
The reviewer flagged that its own `cargo build`/`test` was incomplete (disk full
on the review host), so several traces were source-only; each was re-traced here.

No **P0** and no novel **P1** were found: the prior P1-A (compact enforcement) and
P1-B (Origin gating) fixes held.

## Verdicts

| # | Title | Sev | Verdict | Evidence |
|---|-------|-----|---------|----------|
| P2-1 | Admin GUI 401 when `--api-key` is set | P2 | **FIXED** | Read-only admin static assets skip Bearer (`is_admin_public_static_path` in `require_bearer`); `/api/v1/*` + `/mcp` stay gated. Test: `keyed_loopback_admin_static_assets_load_without_bearer_api_stays_gated`. |
| P2-5 | Bootstrap key echoed in `/api/v1/aap` preset | P2 | **FIXED** | `serve_url_preset_for_admin` / `build_for_admin_panel` emit an `<API_KEY>` placeholder; the real bootstrap/minted secret never leaves via JSON (closes minted→bootstrap disclosure). Tests: `aap_view_serve_preset_redacts_bootstrap_key`, `serve_url_preset_for_admin_redacts_bootstrap_key`. |
| P2-3 | Durable ledger events lost on shutdown | P2 | **FIXED** | `LedgerWriteTracker` counts in-flight `spawn_blocking` writes; `serve::run` drains them (bounded 5s) after `axum::serve` returns. Tests: `drain_returns_after_pending_write_completes`, `drain_times_out_without_hanging_on_a_stuck_write`. |
| P3-7 | Find-fusion both-surfaces-empty reports `Found` | P3 | **FIXED** | A fusion union where every surface is empty now reports `EmptyResult` (not `Found`), matching plain `search_text`/`search_files`. Not an error (`isError` unchanged). Test: `fused_find_with_both_surfaces_empty_reports_empty_result`. |
| P3-8 (P3-D) | `::ffff:127.0.0.1` not treated as loopback | P3 | **FIXED** | `is_loopback_addr` normalizes an IPv4-mapped IPv6 address before the check. Test: `is_loopback_addr_classifies_v4_and_v6` (added `[::ffff:127.0.0.1]` case). |
| P3-10 | No keyed-loopback admin render test | P3 | **FIXED** | Added alongside P2-1. |
| P2-2 | Loopback-open API is unauthenticated | P2 | **BY DESIGN** | No-key + loopback is the documented zero-config local mode (the secure-default *refuses* a routable bind without a key). On a shared host the operator passes `--api-key`. Not a code defect; no behavior change. |
| P2-4 | Daemon HTTP bypasses compact-surface gate | P3 (downgraded) | **DOC** | `daemon.rs::execute_tool_call` is SymForge's **internal IPC** (hook sidecar + CLI dogfooding), not the external compact-3 harness surface. SymForge's own hooks legitimately call full-surface tools through it; adding `enforce_compact_surface` there would break dogfooding. Consistency note only — the external `/mcp` + stdio surfaces remain gated (P1-A). |
| P3-6 | Bearer verify does a sync SQLite scan on the async path | P3 | **ACCEPTED** | The common bootstrap-key path is a constant-time in-memory compare; only the minted-key path touches SQLite, and only when the bootstrap key did not match. The lock is **not** held across `.await`, and active key sets are tiny. An in-memory hash cache adds coherence risk for negligible gain. Future option if many-key/high-concurrency deployments emerge: move `verify` to `spawn_blocking`. |
| P3-9 | No Bearer brute-force rate limiting | P3 | **WONTFIX** | Minted keys are 256-bit (`randomblob(32)`), compared in constant time, behind the secure-default. Online brute force is computationally infeasible; a limiter adds state for no real gain. |

## Known-deferred (severity unchanged, from the review brief)

- **P3-A** ledger migration has no forward-compat guard — low ops risk until a schema v2.
- **P3-B** ledger has no retention (unbounded growth) — operational; documented.
- **P3-C** `Cargo.toml` rmcp `"1.1.0"` vs lockfile `1.7+` drift — supply-chain hygiene; tracked, not load-bearing.
- **P3-D** resolved above as **P3-8**.

## Gate (on `fix/v8-review-remediation`)

`cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` ·
`cargo check --all-targets` · `cargo check --no-default-features --features embed`
(verified axum/rmcp-free) · `cargo test --all-targets -- --test-threads=1` — all green.

## Release

These fixes land with the **v8.0.0** major bump (the operator/CLI/onboarding
surface is a breaking change vs the 7.x daemon+33-tools model: new `serve` IP/API,
compact-3 default surface, changed install/init, harness reconfiguration, README
rewrite). The **embed facade is unchanged and remains semver-stable** — the major
bump is the operator surface, not the AAP embed contract.
