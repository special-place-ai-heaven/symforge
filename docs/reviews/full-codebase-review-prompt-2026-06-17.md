# SymForge — Full Codebase Review Brief (external LLM reviewer)

**Repo:** `symforge` (Rust MCP code-intelligence server) · **Target ref:** `origin/main` (currently `39a5eef`, released 7.31.0 — all 00X merged) · **Date:** 2026-06-17

> **Feature 007 (Intelligence Pattern Ports)** merged after this brief was written — review it with the author-supplied focus + invariants in [`007-review-focus-2026-06-17.md`](./007-review-focus-2026-06-17.md).

You are a senior Rust + security + systems reviewer. Review the **entire codebase**, but spend your budget where risk concentrates (the ranked focus areas below). Your job is to find **real, verifiable** problems — bugs, security holes, gaps, fragile/mediocre code, performance traps, and design issues — and report them so a maintainer can act. Quality over quantity: a few confirmed P0/P1 findings beat fifty speculative nits. **Verify claims against the code; do not pad with false positives.**

---

## 0. How to operate

```bash
# build + gates (Windows or Linux/macOS)
cargo build --release
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
# embed isolation (must be free of axum/rmcp/network deps):
cargo check --no-default-features --features embed
cargo tree --no-default-features --features embed   # expect: no axum, no rmcp
```

- Reproduce findings where feasible (write a failing test, run the server, craft a request). Mark each finding **Confirmed** (you reproduced/traced it) or **Suspected** (needs maintainer check).
- Two cargo features matter: **`server`** (default — full MCP server, CLI, admin, axum/rmcp) and **`embed`** (library-only, must NOT pull network/server deps — this is the AAP embedding contract).
- The MCP server speaks the Model Context Protocol over **stdio** and over **Streamable HTTP** (`/mcp`).

## 1. What SymForge is (context)

A Rust server that indexes source repos (tree-sitter parsers) and exposes symbol-aware code navigation/editing as MCP tools, designed to **reduce agent token use** vs raw file reads. v8 added the **STEL** (token-economics) layers and an **operator server**: `symforge serve` exposes the MCP surface over authenticated Streamable HTTP, plus an admin web UI, harness onboarding, and AAP integration. The recently-landed operator surface (`src/server/**`, `src/stel/**`, `src/cli/serve|harness|onboarding`) is the **newest, least-battle-tested code** — weight your review accordingly.

## 2. Codebase map (modules + risk)

| Area | Path | What | Review weight |
|---|---|---|---|
| **Operator server** | `src/server/{serve,auth,mcp_http,mod}.rs` | `symforge serve`, `/mcp` Streamable HTTP, Bearer auth, ServerRuntime, governor | **HIGHEST** |
| **Admin GUI + API** | `src/server/admin/{mod,api_v1}.rs`, `admin/assets/*`, `src/server/api_keys.rs` | `/admin` UI, `/api/v1/*`, hashed API-key store | **HIGHEST** |
| **AAP integration** | `src/server/aap.rs` | sibling detection, embed-pin drift, presets | HIGH |
| **STEL economics** | `src/stel/*` (`planner,controller,executor,ledger,ledger_store,golden_replay,surface*,handler,status,a029,calibration,edit_*`) | L1-L4 routing/economics, durable SQLite ledger | **HIGH** |
| **Protocol/MCP** | `src/protocol/*` (`mod,tools,format,surface_probe,resources,prompts,result_status,smart_query,edit_tools`) | tool dispatch, compact-surface gate, formatters | **HIGH** |
| **CLI / onboarding** | `src/cli/{serve,init,harness,onboarding,hook,update,trust}.rs` | commands, harness scan/apply/backup | HIGH |
| **Live index** | `src/live_index/*` (store, query, persist, coupling, trigram, watcher) | in-memory index, reload atomicity, git temporal coupling | MEDIUM-HIGH |
| **Parsing** | `src/parsing/*` (tree-sitter langs, xref, ast_grep, extractors) | symbol/ref extraction across languages | MEDIUM |
| **Sidecar / daemon** | `src/sidecar/*`, `src/daemon.rs` | legacy hook HTTP, daemon proxy, governor | MEDIUM |
| **Embed contract** | `src/embed.rs`, `Cargo.toml` features | semver-public facade; must stay axum/rmcp-free | HIGH (invariant) |
| **Analytics** | `src/analytics/store.rs` | rusqlite analytics (pattern the STEL ledger mirrors) | LOW-MEDIUM |
| **007 intelligence ports** | `src/protocol/{format,edit_tools,edit_plan,prompts,tools,smart_query}.rs`, `src/sidecar/handlers.rs`, `src/stel/planner.rs` | impact footer, orientation doctrine, ranked compact map, STEL find-fusion, edit_plan co-change | **HIGH** — see [`007-review-focus`](./007-review-focus-2026-06-17.md) |

## 3. Priority focus areas (ranked) + specific questions

### A. Auth & network exposure — `src/server/{auth,serve,mcp_http}.rs` (TOP PRIORITY)
- Bearer check: is it genuinely **constant-time** for all key lengths? (Prior fix folded full `usize` length — verify no truncation/short-circuit remains.)
- **Secure-by-default:** can a **non-loopback** bind ever start/serve without a key? Trace `serve::run` refuse-to-start + the auth layer order. Try to defeat it: bind `0.0.0.0` with no key; `[::]`; IPv4-mapped `::ffff:127.0.0.1`; `0.0.0.0` treated as loopback?
- **Origin gating** (`apply_origin_gate`/`OriginLayerState`): does it actually block cross-origin browser `fetch` to `/api/v1` and `/mcp`? Bypasses via missing/null Origin, case, port, subdomain, `Origin: null`?
- Is auth enforced **before** any tool executes / any DB write? Are there unauthenticated routes (`/admin` assets, `/favicon.ico`, `/api/v1/*`, health) that leak data or allow actions?
- Is the API key ever logged, echoed (startup banner, error messages, tracing), or written to disk in plaintext? (`--api-key` argv exposure is known + mitigated — verify the mitigation: warn + refuse inline key on non-loopback.)
- DNS-rebinding: rmcp `allowed_hosts` — is the Host allow-list correct and not overly permissive?

### B. API-key store — `src/server/api_keys.rs`
- Is **only a hash** persisted (never the raw secret)? Scan the DB bytes. Hash algorithm/strength (sha2 of a high-entropy key is OK; a sha256 of a *low-entropy* key would be brute-forceable — assess key generation entropy).
- `verify`: constant-time over stored hashes? Timing/enumeration via fingerprints? Revoked/rotated keys truly rejected at `/mcp`?
- SQL: parameterized everywhere? Injection via label? Migration idempotency + forward-compat (opening a future-schema DB).

### C. STEL durable ledger — `src/stel/ledger_store.rs` + `src/protocol/{mod,tools}.rs`
- The durable write was moved to `spawn_blocking` (off the async path). Verify: no event loss under load/shutdown; the `spawn_blocking` task can't outlive/leak; ordering vs the in-memory `SessionLedger`; behavior when no tokio runtime (sync/embed path).
- **No economics double-count** across stdio + `/mcp` (single dispatch). Confirm exactly one durable row per accepted invocation.
- Poison-safe mutex (`unwrap_or_else(into_inner)`) — any remaining `.expect("poisoned")`/`.unwrap()` on the connection mutex that could panic the request task?
- SQLite: WAL + busy-timeout correctness; behavior when DB locked/corrupt/unwritable (must degrade to Disabled, never panic, never block the request); **unbounded growth** (no retention — known P3, assess severity).

### D. Compact-surface enforcement — `src/protocol/{mod,surface_probe}.rs`
- The compact-3 surface is the **default** and is enforced at `tools/call` dispatch (`enforce_compact_surface`), not just `tools/list`. Verify: every code path that reaches a tool handler goes through the gate (stdio AND `/mcp`); no bypass (aliases, daemon proxy `src/daemon.rs` backward-compat aliases, resources/prompts, smart_query/`ask` routing). Does `SYMFORGE_SURFACE=full` cleanly restore the legacy surface? Any tool reachable by name that isn't in the advertised set under Compact?

### E. Concurrency / async — server-wide
- `RequestGovernor` now bounds `/mcp` concurrency (permit + 503 shed). Verify: permit always released (panics, early returns, cancellation); no deadlock; the shed path is correct; does it guard the right routes (not `/admin`)?
- Any **blocking** calls (sync file IO, sync SQLite, `std::sync::Mutex` held across `.await`) on tokio worker threads? Panics across await points? Task leaks? Graceful shutdown (SIGINT + SIGTERM on Unix) actually drains?

### F. Embed isolation — `Cargo.toml`, `#[cfg(feature="server")]`, `src/embed.rs`
- Can ANY `axum`/`rmcp`/`clap`/network dep reach the `embed` build? (`cargo tree --features embed`.) Is every server/STEL/admin item correctly `#[cfg(feature="server")]`-gated? Is the `src/embed.rs` semver-public surface stable and minimal?

### G. Harness onboarding — `src/cli/{harness,onboarding,init}.rs`
- Config writes: **always backed up** before mutation? Atomic (temp+rename)? Idempotent (no duplicate entries)? Does it ever corrupt a malformed/BOM/locked config, or abort the whole run on one bad target? Path traversal / writing outside expected client config locations? AAP: never overwrite the embed path dep with a stdio-spawn config.

### H. Engine correctness (whole-repo) — `src/live_index/*`, `src/parsing/*`
- Index reload atomicity (publish vs lookup race), watcher reconcile correctness, project/session identity fences (a known historical bug class). Reference/dependent extraction accuracy (false positives/negatives — known weaker on TypeScript/web stacks). Tree-sitter partial-parse handling; panics on malformed input; unbounded memory on huge files/repos.

## 4. What to hunt for (categories)

- **Correctness:** logic errors, off-by-one, wrong error propagation, intent-vs-implementation mismatch, edge/empty/overflow cases, `unwrap`/`expect`/`panic!`/array-index that can fire on real input or attacker input.
- **Security:** auth bypass, injection (SQL/path/command), secret leakage, missing authz, SSRF/rebinding, unsafe deserialization, TOCTOU, supply-chain (deps/features), unsafe blocks.
- **Concurrency:** data races, deadlocks, lock-across-await, blocking-on-async-runtime, unbounded spawning, lost wakeups, shutdown/cancellation correctness.
- **Resource/Perf:** O(n^2)+ hot paths, per-request allocations, sync IO on hot path, unbounded growth (DB, caches, channels), large clones, missing limits/timeouts.
- **API/Contract:** breaking changes to public/embed/MCP/HTTP surfaces; inconsistent error shapes; versioning.
- **Tests:** coverage gaps on the new surface, weak/implementation-coupled assertions, missing negative/security tests, flaky/env-dependent tests, tests that assert HTTP 200 but not real behavior.
- **Maintainability / "mediocre code":** dead code, premature abstraction, duplication, misleading names, giant functions, copy-paste drift between `analytics/store.rs` and `stel/ledger_store.rs` / `api_keys.rs`, comments that lie, TODO/`// REVIEW` markers, inconsistent error handling.
- **Cross-platform:** Windows vs Unix path/signal/socket behavior (the repo is developed on Windows; check Linux/musl/Docker assumptions, SIGTERM, TIME_WAIT/SO_REUSEADDR, PDB/link quirks).

## 5. Known-deferred items (do NOT re-report as novel; DO assess severity)

These carry inline `// REVIEW P3-x` notes and are tracked in `specs/004-v8-operator-serve/review-findings-2026-06-16.md`:
- **P3-A** STEL ledger migration lacks a forward-compat guard (opening a future-schema DB).
- **P3-B** STEL ledger has no retention/prune (unbounded growth).
- **P3-C** `Cargo.toml` pins `rmcp = "1.1.0"` but the lockfile resolves a newer (1.7+); minimum-version vs resolved-version drift.
- **P3-D** IPv4-mapped-loopback (`::ffff:127.0.0.1`) is not treated as loopback.
A prior external review already fixed P1-A (compact enforcement), P1-B (Origin gating), and P2-A..G — confirm those fixes are actually correct/complete rather than re-finding them, but DO challenge whether any fix is incomplete or introduced a new issue.

## 6. Output format (per finding)

```
[Pn] <one-line title>
  Severity:   P0 (exploitable/data-loss/crash) | P1 (serious) | P2 (should-fix) | P3 (minor)
  Confidence: Confirmed (reproduced/traced) | Suspected (needs check)
  Location:   path/to/file.rs:LINE  (and call sites if relevant)
  Scenario:   concrete failure/exploit path — inputs, steps, observed vs expected
  Impact:     who/what breaks, blast radius
  Fix:        specific, minimal remediation (sketch the diff if small)
```

Then a short **prioritized summary** (top 10 by severity×confidence) and an **area scorecard** (Auth/network, Key store, Ledger, Surface enforcement, Concurrency, Embed isolation, Onboarding, Engine, Tests — each: Solid / Gaps / At-risk, one line why).

## 7. Rules

- Verify before asserting; label Suspected when unsure. No speculative or stylistic-only findings dressed up as bugs.
- Prefer reproductions (failing test, curl against a running `serve`, `cargo` output).
- Call out anything **claimed but unverified** in code comments/docs (e.g., a comment says "constant-time"/"SIGTERM"/"never blocks" — check it's true).
- Read-only: do NOT push, merge, or modify the repo — report findings only.
- If you run out of budget, say what you did NOT cover so it isn't mistaken for "clean."
