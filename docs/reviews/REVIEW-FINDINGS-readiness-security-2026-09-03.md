# REVIEW-FINDINGS — Production-Readiness Diagnosis, Security Spectrum (Phase 5)

- **Date:** 2026-09-03
- **Reviewer:** Phase5Security (diagnosis-only lane)
- **Baseline:** `main` @ `6188c5af` (executed from worktree `1b570f1c`, which adds only `docs/plans/2026-09-03-production-readiness-diagnosis.md`; every source citation below is identical at both revs)
- **Spec:** `docs/plans/2026-09-03-production-readiness-diagnosis.md` §Phase 5
- **Scope:** dependency audit, vendored grammar, auth surface, trust model, secrets hygiene. Read-only; no source edits; no cargo build/test/clippy (main-lane discipline).

## Findings

### SEC-1 — No dependency vulnerability/license scanning anywhere; tooling absent and not installable this session — **medium, proven**

`cargo audit` and `cargo deny check` are configured nowhere:

- `.github/workflows/ci.yml` (full read, 321 lines): no `cargo audit`, no `cargo deny`, no RustSec action. Security-adjacent CI steps are limited to the rmcp single-major assertion and supply-chain refreeze gates.
- `.github/workflows/release.yml`: grep for `audit|deny` matches nothing (only `secrets.` contexts).
- No `deny.toml` exists at repo root or under `.cargo/` (`.cargo/config.toml` only sets `build.target-dir`).
- Neither tool is installed locally (`cargo audit --version` / `cargo deny --version` → "no such command"; no `cargo-audit`/`cargo-deny` binaries on PATH).
- `cargo install --locked cargo-audit cargo-deny` was attempted per the campaign's allowance and **failed**: `curl failed … Could not connect to index.crates.io port 443`. No offline advisory DB exists on this machine, so no advisory scan could be executed at all this session.

**Impact:** a repo shipping compiled binaries, npm packages, and a `Cargo.lock` with ~700 transitive crates has no automated tripwire for newly disclosed advisories (e.g. in `rmcp`, `axum`, `rusqlite`, `git2`/`libgit2`, `zstd`) and no license/source policy enforcement. The only standing dependency guard is the rmcp major assertion (`.github/workflows/ci.yml`, "Assert single rmcp major (spec 025 FR-A4)").

**Remediation size:** small — add `cargo deny check advisories` (or `cargo audit`) as a CI job plus a `deny.toml`; locally `cargo install --locked cargo-deny` when network is available. The unexecuted scan itself should be re-run once network access exists; this campaign cannot certify the current dependency tree as advisory-clean.

### SEC-2 — Project-config trust gate ships in observe-only mode by default; `trust status` does not disclose the gap — **medium, proven**

The `.symforge` project-config trust control exists and is wired into **every** edit tool, but enforcement is opt-in via a process env var and defaults to log-only:

- `src/protocol/edit_tools.rs:41-55` — `ProjectConfigTrustMode::current()` reads `SYMFORGE_PROJECT_CONFIG_TRUST_MODE`; only the exact value `enforce` enables `Enforce`; **everything else (including unset) is `LogOnly`**.
- `src/protocol/edit_tools.rs:233-243` — in `LogOnly`, an `Untrusted` or `ContentChanged` evaluation returns `Ok(Some("ProjectConfigTrustWarning: … mode=LOG_ONLY; operation_allowed=true"))` — the edit proceeds with a warning suffix. Only `Enforce` returns `Err("ProjectConfigTrustEnforced: … operation_allowed=false …")`.
- Gate coverage is uniform: `project_config_trust_response_suffix` is invoked by all seven edit handlers in `edit_tools.rs` (lines 959, 1272, 1494, 1713, 2131, 2250, 2369) and the suffix is appended to their outputs.
- `src/cli/trust.rs:171-186` — `print_evaluation` (the `symforge trust project-config status` output) prints `status`, hashes, `project_key`, warnings, and store path, but **never the effective enforcement mode**. An operator can read `status: Untrusted` and reasonably believe edits are blocked; they are not, by default.

**Mitigations verified (why this is medium, not high):**

- At this rev **nothing in `src/` parses or acts on `.symforge/config.toml` content**: the only readers are the trust hasher itself (`src/edit_safety/trust.rs:365-372`) and the existence probe (`src/protocol/edit_tools.rs:215-218`). So "untrusted config steers edits" has **no observed mechanism today**; the gate is a tripwire armed for a config consumer that does not yet exist.
- The trust store is user-local, never project-relative (`src/paths.rs:104-117`: `SYMFORGE_HOME` or `~/.symforge`; `resolve_control_state_placement` explicitly "never turns into a relative `.symforge` path"), so a cloned repo cannot carry its own trust record.
- `SYMFORGE_TRUST_PROJECT_CONFIG=1` is honored only in a recognized CI environment, otherwise ignored with a warning (`src/edit_safety/trust.rs:139-148`, `TRUST_ENV_OVERRIDE` at line 4).

**Risk:** when a real `.symforge/config` consumer lands, the default-off enforcement and the non-disclosing status output mean the control will *look* active while allowing everything. **Remediation size:** small-to-moderate — flip the default to `Enforce` (or at minimum print `mode=LOG_ONLY` in `trust status` output and the CLI help) before any config consumer ships.

### SEC-3 — Vendored `tree-sitter-scss` patch exceeds its documented scope, and the patch comment misdescribes it — **low, proven**

The `[patch.crates-io]` comment (`Cargo.toml:178-181`) says: *"Upstream build.rs uses `-Wno-unused-parameter` which MSVC rejects. Patched copy uses `flag_if_supported` so the flag is silently skipped on MSVC."* The actual delta vs the crates.io 1.0.0 artifact is **two files, and neither matches that description**:

Byte-exact diff of `vendor/tree-sitter-scss/` against the local cargo registry cache copy `tree-sitter-scss-1.0.0.crate` (60,293 bytes, sha256 `33909a9ca86390ebbf3461e9949c4bbe2767d2d024b486306d27616641d4ba24`; this cache file is the exact bytes cargo downloaded from crates.io):

- 13 of 15 packaged files are **byte-identical**; the only local extra is `.cargo-ok` (cargo's extraction marker — expected).
- `bindings/rust/build.rs` — differs: the `c_config.flag("-Wno-unused-parameter");` line is **deleted outright**. `flag_if_supported` appears **nowhere** in the vendored copy.
- `src/scanner.c` — differs and is **undocumented**: the empty scanner stubs gain explicit `(void)payload;` / `(void)buffer;` / `(void)length;` casts (and `(void)payload;` at the top of `scan`), i.e. the same unused-parameter warnings are silenced in the C source instead of via compiler flag.

Both files were read in full on both sides: the scanner.c changes are no-op casts plus brace reformatting; `scan()` logic is otherwise identical. The patch is semantically benign — but the vendored tree diverges from upstream in two files where the repo's own comment claims one, and claims a mechanism (`flag_if_supported`) that was not used. Note that under `[patch]`, crates.io checksum verification is bypassed for this crate by design (`Cargo.lock:3213-3217` has no `source`/`checksum` for tree-sitter-scss), so the vendor tree *is* the trust anchor and its documented scope is the only review baseline.

**Verification caveat:** outbound network was blocked this session (node `fetch` to static.crates.io failed; cargo could not reach index.crates.io), so the diff baseline is the local cargo registry cache rather than a fresh download. The cache holds cargo's own crates.io download and the vendored dir carries cargo's extraction marker (`.cargo-ok`), so provenance is strong; a network re-check is a one-command follow-up (`curl -L https://static.crates.io/crates/tree-sitter-scss/tree-sitter-scss-1.0.0.crate | sha256sum` → expect `33909a9c…`).

**Remediation size:** trivial — update the comment to describe both files, or redo the patch with `flag_if_supported` to match the comment.

### SEC-4 — Stale security-relevant comment: REVIEW P3-C describes a superseded rmcp constraint — **low, proven**

`Cargo.toml:84-88` says: *"version requirement is `"1.1.0"` but the lockfile resolves >=1.7 (the `allowed_hosts` DNS-rebinding behavior depends on >=1.7 APIs). Future fix: pin an exact/min version … so a downgrade can't slip in."* Reality at this rev:

- The actual requirement is `rmcp = "3.1"` (`Cargo.toml:89`); the lockfile resolves **rmcp 3.1.4 + rmcp-macros 3.1.4** (`Cargo.lock:2153-2154`, `2185-2186`). The `>=1.7` concern is obsolete by two majors.
- The `allowed_hosts` DNS-rebinding API is in active use: `src/server/mcp_http.rs:138` (`.with_allowed_hosts(host_allow_list(bind_host))`), with the loopback-defaults-plus-bind-host list at `mcp_http.rs:147-158`.
- CI already asserts both `rmcp` and `rmcp-macros` resolve to a single major `3` (`.github/workflows/ci.yml`, "Assert single rmcp major (spec 025 FR-A4)"), which is stronger downgrade protection than the comment's proposed min-version check for the major axis.

The comment's residual truth (no advisory scanning — see SEC-1) survives, but its stated premise and "future fix" are dead. **Remediation size:** trivial — rewrite or delete the comment.

### SEC-5 — Admin/API router has no Host-header validation; DNS-rebinding hardening lives only on `/mcp` — **low, proven**

rmcp's `allowed_hosts` (Host-header allow-list: `localhost`, `127.0.0.1`, `::1`, plus the operator bind host) is configured **only** on the `StreamableHttpService` mounted at `/mcp` (`src/server/mcp_http.rs:121-143`, `147-158`). The admin router (`src/server/admin/mod.rs:116-139`) — `/admin` assets plus `/api/v1/summary|surface|harness|system|aap|keys` and key mint/rotate/revoke — is gated by the Origin middleware and Bearer auth only (`src/server/serve.rs:664-676`).

Residual exposure analysis (why low, not medium):

- Cross-origin browser `fetch`/form POST/DELETE always carry an `Origin` header in modern browsers → rejected by `require_allowed_origin` (`src/server/auth.rs:248-271`, exact-match allow-list built at `auth.rs:212-231`). State-changing routes (mint/rotate/revoke are POST/DELETE) are therefore unreachable from a rebinding web page.
- Remaining gap: no-`Origin` GETs (e.g. `<img>`, `<script src>`, top-level navigation) to `/api/v1/*` reach the handler on the default no-key loopback configuration. Those GET endpoints are read-only, and `/api/v1/keys` list returns only 12-hex hash prefixes (`src/server/api_keys.rs` `fingerprint_of`); responses are not cross-origin readable without CORS. No sensitive data or state change is exposed via this path as designed.
- A non-loopback bind always requires a key (`AuthConfig::refuse_to_start`, `src/server/auth.rs:56-65`), so the no-auth + rebinding combination exists only on loopback.

**Remediation size:** small — apply the same Host allow-list as an axum layer over the merged router in `serve::run`.

### SEC-6 — `api-keys.db` gets no explicit restrictive file mode — **informational, proven**

The repo hardens some state files (`0o700` on control-state dirs — `src/paths.rs:153`, `src/discovery/mod.rs:1694`; `0o600` on a daemon file — `src/daemon.rs:535`), but `src/server/api_keys.rs` contains no `set_permissions` call: `api-keys.db` is created by SQLite with process-umask defaults (typically world-readable `0644` on Unix). Impact is negligible by design — the DB stores only SHA-256 hashes of 256-bit random `sf_<64hex>` tokens (`generate_secret` uses `randomblob(32)`, `api_keys.rs:370-382`; `hash_secret` at `api_keys.rs:387-397`) plus labels/fingerprints — but the inconsistency with the repo's own 0o600/0o700 discipline is worth a one-line fix. Note the DB path itself is inside a `.symforge` state dir that is gitignore-covered (`.gitignore:27`), so commit-leak is not a vector.

### SEC-7 — Secrets hygiene sweep: clean; `.env.example` does not exist — **informational, proven**

- High-entropy patterns across the entire repo — `sf_[0-9a-f]{64}`, `ghp_*`, `github_pat_*`, `gho_*`, `AKIA*`, PEM private-key blocks, `xox[baprs]-*`, `sk-*` — **zero matches**.
- Literal-assignment sweep (`(api_key|secret|password|token) = "<≥20 chars>"`): one match, `specs/023-raw-read-admission-gate/REVIEW-FINDINGS-cursor.md:86`, which quotes a *test fixture* string in prose — not a live secret.
- `.gitignore:33` covers `.env*`; `.gitignore:27` covers `**/.symforge/` (so `api-keys.db`, `hook-adoption.log`, `coupling.db`, `index.bin` cannot be accidentally committed).
- **No `.env.example` exists anywhere in the repo** (glob `**/.env*` empty): the spec's "confirm .env.example is template-only" check is vacuous. CI workflows consume secrets only via `${{ secrets.* }}` contexts (`.github/workflows/release.yml:671-674, 2025, 2048, 2204, 2462, 2497, 2549, 2572`) — normal.
- Live `hook-adoption.log` census (`.symforge/hook-adoption.log`, 1,199,318 bytes, 10,189 rows): every row is 3 or 7 tab-separated fields — `session_id`, `workflow`, `outcome`, optionally `reason=<enum>`, `searched_path=<port-file path>`, `suggestion=<enum>`, `project_root=<abs path>`; **maximum field length 64 chars; no payload content, no prompts, no tool_input values**. This matches the writers (`src/cli/hook.rs:1534-1582`), which emit fixed-shape lines, and the fail-open entry (`src/cli/hook.rs:226-231`, 311-315), which emits the empty-JSON pass-through and records outcome labels only. Verbose mode (`SYMFORGE_HOOK_VERBOSE=1`, `hook.rs:1422-1425`) logs ports/paths/outcomes to stderr — never payload fields.

## Verified clean / positive confirmations (no finding)

- **Bearer resolution & policy:** `resolve_api_key` prefers `--api-key` inline over `--api-key-env`, treats empty as unset (`src/server/serve.rs:93-105`, unit-tested at 819-863). Inline key **refused** on non-loopback binds, warned on loopback (`serve.rs:126-148`). Non-loopback bind with no key refuses to start before any socket opens (`src/server/auth.rs:56-65` via `serve.rs:557-558`). Default bind is loopback `127.0.0.1:8787` (`serve.rs:27`).
- **Constant-time comparison:** both the bootstrap key and minted-key paths use length-folding constant-time equality (`src/server/auth.rs:290-330`, `src/server/api_keys.rs:343-368` — iterates all active hashes without early-exit on the match flag).
- **Minted-key hygiene:** raw secret shown exactly once at mint, never persisted, never returned by list/get; list exposes 12-hex fingerprints only (`api_keys.rs` module doc + `MintedKey`, lines 39-97); store open failure degrades to `Disabled` with bootstrap key still working (fail-safe direction for availability, and `verify` returns `false` when `Disabled` — fail-closed for minted keys).
- **Origin gate:** exact-match, case-insensitive allow-list; no `Origin` → pass (non-browser), unknown `Origin` → 403 (`auth.rs:204-271`).
- **Admin static bypass:** exact-match `matches!` list of six asset paths only; `/api/v1/*` never bypasses (`src/server/admin/mod.rs:39-48`).
- **Admin UI key handling:** key held in an in-memory JS variable, prompted at runtime, sent only as an `Authorization: Bearer` header; no localStorage/sessionStorage/URL-query usage (`src/server/admin/assets/app.js:6-37`).
- **Attach registry:** the bootstrap bearer is compared against harness configs but never serialized into the admin status payload — `StaleFields::description` returns `&'static str` so it *cannot* carry a token (`src/server/admin/api_v1.rs:330-368`).
- **`serde_yml` alias:** resolves to `serde_yaml_ng 0.10.0` (`Cargo.lock:2505-2506`), matching the maintained-fork rationale in the `Cargo.toml:91-96` comment.
- **Trust store placement:** user-local only (`src/paths.rs:104-117`); `EnvOverride` honored only under recognized CI env vars (`src/edit_safety/trust.rs:139-148`); trust-record hashes validated as SHA-256 hex before comparison (`trust.rs:180-186`).

## Summary

| Severity | Count | Findings |
|---|---|---|
| Critical | 0 | — |
| High | 0 | — |
| Medium | 2 | SEC-1 (no advisory scanning), SEC-2 (trust gate observe-only default) |
| Low | 3 | SEC-3 (vendored patch scope/comment), SEC-4 (stale rmcp comment), SEC-5 (no Host validation on admin/api) |
| Informational | 2 | SEC-6 (api-keys.db file mode), SEC-7 (secrets sweep clean / .env.example absent) |

**Top risks:** (1) SEC-1 — the dependency tree ships with no advisory tripwire and none could be run this session; (2) SEC-2 — the trust control presents as active (`trust status` prints Untrusted) while defaulting to allow; (3) SEC-3 — the vendored grammar diverges from upstream in one more file than documented, and the `[patch]` bypass means the vendor tree is the only trust anchor.

**Operational gaps (unverified checks, need rerun with network):** `cargo audit`, `cargo deny check` (install blocked: index.crates.io unreachable); fresh-download cross-check of the vendored-crate baseline sha256 `33909a9c…` against static.crates.io.
