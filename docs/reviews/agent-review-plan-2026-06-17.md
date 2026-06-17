# SymForge — Agent-Orchestrated Internal Review Plan

**Target:** post-007-merge `origin/main` (all 00X items). **Shared context:** every agent reads [`full-codebase-review-prompt-2026-06-17.md`](./full-codebase-review-prompt-2026-06-17.md) first (module map, focus areas, finding format, known-deferred P3s) AND [`007-review-focus-2026-06-17.md`](./007-review-focus-2026-06-17.md) (the 007 ports + invariants, since 007 merged after the panel was first scoped). **Run from:** the dedicated `E:\project\symforge-review-main` worktree (refreshed to the merged `origin/main`).

This is the **internal** review track (our specialist agents). It complements the **external** cross-model LLM pass (same prompt, opposing model). Two independent eyes.

## Orchestration

- **Wave 1 — parallel fan-out (read-only, ≤5 in flight):** security-reviewer, rust-pro, database-architect, performance-engineer, code-reviewer. All read-only analysis → no compile contention; rust-pro/performance may build to verify a finding → if so, serialize their builds on the one warm `target/`.
- **Wave 2 — live UI (separate):** browser-tester against a running `serve` (loopback, no key → `/admin` open). Needs Wave 0 to start the server.
- **Wave 0 — gate baseline (test-runner):** fresh `fmt/check/clippy/test/build/embed` + coverage-gap scan, so the panel reviews a known-green tree and findings aren't gate noise.
- **Wave 3 — synthesis:** dedup + severity-rank all findings (drop duplicates across agents), cross-check against the external review, produce one merged report. **debugger** dispatched reactively only to reproduce/root-cause any P0/P1 a reviewer flags as Suspected.

Each agent uses the **finding format** from the shared prompt (severity P0-P3 × confidence Confirmed/Suspected × file:line × scenario × impact × fix) and ends with its area scorecard.

---

## Wave 0 — `test-runner` (baseline)

**Scope:** whole repo, gate + coverage. **Dispatch:**
> Run the full gate on `E:\project\symforge-review-main`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, `cargo check --no-default-features --features embed`. Report exact pass/fail counts. Then identify **test coverage gaps** on the new operator surface (`src/server/**`, `src/stel/ledger_store.rs`, `src/cli/{serve,harness,onboarding}.rs`, `src/server/aap.rs`): which public behaviors / error paths / security checks have NO test, and which tests assert weakly (HTTP 200 without behavior, no negative/auth/Origin cases). Do not change code. Output: gate result + a ranked list of coverage gaps.

## Wave 1 — parallel fan-out

### `security-reviewer` — auth / network / secrets (TOP)
**Scope:** `src/server/{auth,serve,mcp_http,mod}.rs`, `src/server/admin/{mod,api_v1}.rs`, `src/server/api_keys.rs`, `src/protocol/surface_probe.rs`, `Cargo.toml` features. **Dispatch:**
> Read the shared review prompt §3.A/§3.B/§3.D. Adversarially review SymForge's operator server for exploitable issues. Try to DEFEAT: (1) secure-by-default — start/serve a non-loopback bind with no key (`0.0.0.0`, `[::]`, `::ffff:127.0.0.1`); (2) Bearer auth — constant-time? bypass on `/admin` assets / `/favicon.ico` / `/api/v1/*` / health? key in logs/banner/errors? (3) Origin gating — cross-origin `fetch` to `/api/v1`+`/mcp`, `Origin: null`, case/port/subdomain tricks; (4) API-key store — only hashes persisted? key entropy? revoked keys rejected? SQL injection via label; (5) compact-surface enforcement — call a legacy tool by name under the default compact surface (stdio + `/mcp` + daemon aliases). Reproduce where you can (run `serve`, curl). Report per the finding format.

### `rust-pro` — async / concurrency / correctness
**Scope:** `src/server/**`, `src/stel/{ledger_store,ledger,controller,executor,handler}.rs`, `src/sidecar/governor.rs`, `src/protocol/{mod,tools}.rs`. **Dispatch:**
> Read the shared review prompt §3.C/§3.E. Deep Rust review of the async/concurrency surface. Hunt: `std::sync::Mutex`/blocking SQLite held across `.await` or run on tokio workers; the durable-ledger `spawn_blocking` path (event loss on shutdown, task leak, ordering vs in-memory ledger, no-runtime path); `RequestGovernor` permit always released on panic/early-return/cancel, deadlock, shed correctness; panics across await; `unwrap`/`expect`/index that fire on real/attacker input; graceful shutdown (SIGINT + SIGTERM on Unix actually drains); any `unsafe`. Build/test to confirm a suspected bug. Report per the finding format.

### `database-architect` — SQLite stores
**Scope:** `src/stel/ledger_store.rs`, `src/server/api_keys.rs` (+ `src/analytics/store.rs` as the mirrored pattern). **Dispatch:**
> Read the shared review prompt §3.B/§3.C. Review the two SQLite stores. Check: parameterization (no injection), migration idempotency + **forward-compat** (opening a future-schema DB — known P3-A, assess severity), WAL + busy-timeout correctness, transaction boundaries, behavior when DB is locked/corrupt/unwritable (must degrade, never panic/block), **unbounded growth/retention** (P3-B), schema/index correctness, and copy-paste drift between the three stores. Report per the finding format.

### `performance-engineer` — hot paths / footprint
**Scope:** the request/tool-call path (`src/protocol/{mod,tools,format}.rs`), `src/stel/**`, `src/server/mcp_http.rs`, `src/live_index/query.rs`. **Dispatch:**
> Read the shared review prompt §3.C/§3.E. Find performance traps on the served request path: blocking I/O / sync SQLite on the async hot path, per-request allocations/large clones, O(n^2)+ work, unbounded structures (ledger DB, caches, channels), missing limits/timeouts, governor sizing. Measure where feasible (before/after, p50/p95). No perf claim without a number. Report per the finding format.

### `code-reviewer` — broad correctness / maintainability / "mediocre code"
**Scope:** the full v8 diff (`git diff <pre-campaign-base>..origin/main` — focus `src/server/**`, `src/stel/**`, `src/cli/**`, `src/protocol/**`) + a whole-repo skim. **Dispatch:**
> Read the shared review prompt (all). Broad pre-merge-quality review of the v8 operator surface: logic/edge/state/error-propagation bugs, intent-vs-implementation mismatches, **dead code, premature abstraction, duplication, misleading names, giant functions, coupling, comments that lie, TODO/`// REVIEW` markers**, inconsistent error handling, weak/missing tests. Flag anything a senior perfectionist would reject in review. Report per the finding format + a maintainability scorecard.

## Wave 2 — `browser-tester` (live UI, separate)

**Scope:** `/admin` dashboard + AAP panel (006/008). **Pre-req:** start `serve` (loopback, no key) on the release binary. **Dispatch:**
> The SymForge admin UI is running at `http://127.0.0.1:<PORT>/admin` (loopback, no auth). With Charlotte: verify it RENDERS as an operator dashboard with real data (economics, surface, system PID/uptime/index, harness, **AAP panel** — detection/drift/presets, keys). Check console errors + failed `/api/v1` requests + mobile (390px) overflow. Exercise: mint an API key (the write path), toggle views, refresh. Screenshot each view. Report what genuinely renders vs broken/blank/undefined — honestly, no success claim if it errors.

## Wave 3 — synthesis (orchestrator)

Collect all agent reports + the external-LLM report. Dedup (same file:line/issue across agents → one finding, highest severity). Rank by severity × confidence. Cross-validate: a finding both internal+external agents hit = high confidence; a solo finding = verify. Produce one merged `docs/reviews/review-findings-<date>.md` with a top-10 + per-area scorecard + a triage (fix-now / track / wontfix). Dispatch **debugger** to reproduce any Confirmed-needed P0/P1. Then plan the fixes (likely a `specs/009-review-remediation` Spec-Kit pass, gated + checkpoint-merged like the rest).

## Notes
- Throttle: only rust-pro/performance/test-runner compile — serialize their builds; the 3 read-only reviewers + security-reviewer fan out freely.
- Alternative: swap Wave 1 for the `ce-code-review` skill (adversarial-persona panel: ce-correctness/ce-security/ce-reliability/ce-performance/ce-maintainability/ce-testing) if you prefer the compound-engineering tiered orchestration. The base-specialist panel above is leaner + less redundant.
- This internal panel + the external cross-model LLM = two independent verdicts on the same tree.
