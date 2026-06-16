# Research: SymForge Admin GUI (006) — dependency + approach decisions

**Date**: 2026-06-16 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

This is the T001 decision record: which dependencies (if any) the admin GUI
adds, and the chosen approach for each open question in the plan. The guiding
constraint is **embed isolation** (`cargo check --no-default-features --features
embed` must stay clean) and **minimal new surface** — reuse what `004`/`005`
already pull in before adding anything.

## Decisions

### D1 — System telemetry: **std-only**, no `sysinfo`

**Question**: `sysinfo` crate vs std-only telemetry for `/api/v1/system`.

**Decision**: **std-only.** The spec/FR-005 telemetry that is load-bearing and
asserted by tests (SC-005) is PID, uptime, active sessions, and indexed
projects — all of which are available without a new crate:

- **PID** — `std::process::id()`.
- **Uptime** — a process-start `std::time::Instant` captured when the
  `ServerRuntime` is built; `elapsed()` at query time.
- **Indexed projects** — the runtime's project name + the index
  `published_state()` (`file_count`, `symbol_count`, `generation`).
- **Active sessions** — the serve runtime is one session today (the durable
  ledger's `session_count` gives historical distinct sessions for context).

Host-wide resource usage (total RAM / CPU%) is the only thing `sysinfo` would
add, and it is **not** asserted by any success criterion (SC-005 keys on
PID/sessions/indexed-projects). Adding `sysinfo` (a large transitive tree:
`windows-sys`, `ntapi`, etc.) for one non-load-bearing field violates the
"don't pull a large crate when std is enough" rule. We expose an honest
`resource` block that reports what std can measure (process count of indexed
files/symbols, uptime) and omits host RAM/CPU rather than fabricating it. If a
future task makes host resource usage load-bearing, `sysinfo` can be added then
under the `server` feature only.

**Net new deps for telemetry: 0.**

### D2 — Embedded UI assets: **`include_str!`**, no `rust-embed`

**Question**: `rust-embed` vs `include_str!` for the embedded UI.

**Decision**: **`include_str!`.** The UI is exactly three small static files
(`index.html`, `app.js`, `style.css`) served at fixed paths. `include_str!`
compiles them into the binary with zero new dependencies and zero build script;
each is served by a dedicated handler returning the right `Content-Type`.
`rust-embed` earns its keep when there are many assets or a directory walk is
needed at build time — neither applies here. Using `include_str!` keeps the
embed build trivially clean (the whole `admin` module is `#[cfg(feature =
"server")]`, so even the `include_str!` calls never compile in an embed build).

**Net new deps for assets: 0.**

### D3 — Key hashing: **reuse `sha2`** (already a non-optional dep)

**Question**: key-hash algorithm.

**Decision**: **SHA-256 via the existing `sha2 = "0.11"` dependency** (already
in `[dependencies]`, used by `src/hash.rs`, `edit_safety/trust.rs`,
`live_index/persist.rs`). The key store hashes the presented raw secret and
compares the hex digest against the stored hash. This mirrors the existing
`hash::digest_hex` helper pattern. The raw secret is generated with enough
entropy (32 bytes, rendered as a `sf_` + hex token) that a plain SHA-256 of a
high-entropy random secret is appropriate — these are bearer tokens, not
human-chosen passwords, so a slow password-KDF (argon2/bcrypt) is not required
and would add a dependency for no security gain on a 256-bit random token.
The fingerprint shown in listings is a short prefix of the hash (never the raw
secret). Constant-time compare on `verify` reuses the same discipline `auth.rs`
already applies for the bootstrap key.

**Net new deps for hashing: 0.**

### D4 — Random token generation: **OS entropy via `rusqlite`'s bundled
`randomblob`** is avoided; use std + `getrandom` already in the tree

**Decision**: generate the 32-byte secret from the OS CSPRNG. The repo already
transitively depends on `getrandom` (via `ahash`/`rand`-adjacent crates and
rustls); however, to avoid relying on a transitive that could change, the key
store sources entropy from `std`-reachable randomness through the already-present
`rusqlite` bundled SQLite `randomblob(32)` function (deterministically available
because `rusqlite` is a non-optional dependency with the `bundled` feature). This
keeps entropy generation inside an already-audited dependency and adds **0** new
crates. (If a direct `getrandom`/`rand` dep is later preferred for clarity, it is
a one-line addition under `server`; not needed for this feature.)

**Net new deps for entropy: 0.**

### D5 — Origin gating (review P1-B): **header-check axum middleware**, no CORS
crate

**Question**: axum Origin-gating approach.

**Decision**: a small `from_fn`-style middleware layered in front of
`/admin` + `/api/v1` (and reusable for `/mcp`) that inspects the `Origin`
request header. The browser surface is same-origin by construction (the UI is
served from the same host:port it calls), so the rule is:

- **No `Origin` header** (non-browser client: curl/reqwest/the MCP client) →
  allowed (Origin gating targets *browser* cross-origin `fetch`, not API
  clients).
- **`Origin` present and matching the server's own scheme://host:port (or an
  explicitly-allowed origin)** → allowed.
- **`Origin` present and NOT allowed** → `403 Forbidden`.

This closes P1-B (a malicious web page doing `fetch('http://127.0.0.1:8787/...')`
is rejected because its `Origin` is the attacker's site, not the server's own).
No `tower-http` CORS crate is added — the rule is a few lines and a dedicated
crate would be heavier and less precise than the exact same-origin check we
need. The allow-list seam is parameterized so a future config can extend it.

**Net new deps for Origin gating: 0.**

## Summary

**Total net-new crate dependencies for feature 006: 0.** Everything is built on
deps already present (`axum`, `rusqlite` bundled, `sha2`, `serde`,
`serde_json`, `tokio`, `reqwest` for tests) plus `include_str!`. The entire
admin + key-store surface is `#[cfg(feature = "server")]`, preserving embed
isolation (verified by the T006 gate `cargo check --no-default-features
--features embed`).

| Open question | Decision | New deps |
|---|---|---|
| sysinfo vs std telemetry | std-only (PID/uptime/sessions/index) | 0 |
| rust-embed vs include_str! | include_str! (3 static files) | 0 |
| key-hash algorithm | sha2 SHA-256 (existing dep) | 0 |
| token entropy | OS CSPRNG via bundled SQLite randomblob | 0 |
| Origin gating | header-check middleware (same-origin) | 0 |
