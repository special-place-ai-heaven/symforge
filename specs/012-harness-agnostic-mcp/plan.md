# Implementation Plan: Engine-First Multi-Project Index Primitive + Per-Connection Retarget

**Branch**: `fix/stel-symbol-aware-routing` (no dedicated feature-branch hook) | **Date**: 2026-06-19 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification `specs/012-harness-agnostic-mcp/spec.md` (engine-first; records the pivot away from a multi-tenant server platform).

## Summary

Build a **base+overlay multi-project `IndexView`** as an engine primitive that generalizes the existing `ArcSwap<LiveIndex>` snapshot (immutable base keyed by `(canonical-root, commit)`, shared read-only; per-consumer copy-on-write overlay of dirty/uncommitted deltas). Fix the real field bug (wrong-repo binding) by **un-gating the existing client-roots rebind** and reusing the daemon's existing per-session retarget (`index_folder_for_session`) — no new index plumbing. Add surface-honesty fixes (bound `project_root` in every response; `path:` stays within-project with a clear out-of-project error; clean "query required" error; `if_match` normalized on-disk compare; a glossary MCP resource). Ship multi-project over the **stdio-per-connection** model (the daemon already routes per session); explicitly **defer remote `/mcp` multi-tenant** (the one stateless-singleton linchpin). Companion 013/014 stay **shrunk** (read-only `/api/v1` JSON + minimal durable registry; no platform, no durable economics until grounded). De-risk with a spike before any further build.

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge`. Parsing via tree-sitter.

**Primary Dependencies**: `arc_swap` (`ArcSwap<LiveIndex>` snapshot, `store.rs:667`), `rmcp` (MCP; `LocalSessionManager` already imported `mcp_http.rs:42`), `axum` (serve), `rusqlite` (durable stores, `ledger_store.rs`/`analytics/store.rs`), git (`head_sha` `git.rs:403`, `uncommitted_paths` `git.rs:68`).

**Storage**: In-process `LiveIndex` + local `.symforge/` snapshots remain the single authoritative read path. New: a **minimal durable registry** (SQLite, 014) for tenant/working-set + lazy-rehydrate metadata ONLY — not a query store, not a second authoritative index.

**Testing**: `cargo test --all-targets -- --test-threads=1`; plus a behavioral **multi-consumer dogfood** (two consumers, shared base, isolated overlays, retarget, cross-project search) and the spike's perf-cliff micro-bench.

**Target Platform**: Local-first MCP server, stdio + `symforge serve` `/mcp`; Windows/Linux/macOS.

**Project Type**: Single Rust crate, `embed` (library) vs `server` feature split. This feature adds an engine primitive (compiles under `embed`) + server-side retarget/surface wiring (`server`).

**Performance Goals**: Cross-project query across a working set; **base+epsilon** memory for consumers sharing a `(root, commit)` base (SC-002); overlay rebase on base-commit-advance must be **O(dirty set), not O(repo)** (the named load-bearing risk; spike falsifier).

**Constraints**: `embed` stays network-free (Principle VI / G-045); single authoritative in-memory index (Principle I); transport parity for single-project behavior (Principle VII); no regression to the single-harness flow (FR-016).

**Scale/Scope**: Multiple projects per consumer; multiple concurrent stdio connections each bound to its own project; one shared daemon registry (already exists).

## Constitution Check

*GATE: evaluated against the 8 principles (constitution v1.0.0).*

| Principle | Verdict | Note |
|---|---|---|
| I. Local-First In-Process Index | **PASS** | Base+overlay stays in-memory; base is an `Arc<LiveIndex>` snapshot. 014's durable store holds registry/metadata ONLY — it MUST NOT store symbol/reference query data (that would be a second authoritative index). Explicit guard in spec + data-model. |
| II. MCP-Native Surface | **PASS** | Glossary is an MCP **resource** (`resources.rs`), not chat injection; retarget is a tool input; no client-tool shadowing. |
| III. Trust Envelopes | **PASS (advances)** | Surfacing bound `project_root` + readiness in every response and honest `path:`/`query` errors strengthen completeness/honesty disclosure. |
| IV. Determinism & Recovery | **PASS** | Stale overlay rejected via the existing generation-fence pattern (`store.rs:872-883`); `if_match` idempotency preserved (normalize the pre-flight compare ONLY; keep the write-time byte-exact guard `edit_apply.rs:138-146`); lazy rehydrate is resumable. |
| V. Frecency Invariant | **PASS** | Retarget + cross-project queries stay frecency-neutral; add a test asserting no frecency bump on retarget/discovery. |
| VI. Embed Isolation (G-045) | **PASS (key design)** | Primitive lives in `live_index::view`, reachable via the deep-path re-export but **deliberately omitted from the frozen `embed.rs` contract test** — volatile overlay internals are NOT welded into the semver contract. `cargo check --no-default-features --features embed` must stay green. |
| VII. Transport Parity | **PASS w/ explicit scope** | Multi-project ships over stdio-per-connection first; remote `/mcp` multi-tenant is the stateless-singleton linchpin and is **explicitly scoped as deferred/transport-specific** (see Complexity Tracking). Single-project behavior stays stdio<->serve equivalent. |
| VIII. Verification Before Done | **PASS** | Full backend gate (`fmt/check/clippy/test/build`) + behavioral multi-consumer dogfood; spike carries an explicit falsifier. |

No unjustified violations → **gate passes**. One scoped exception recorded below.

## Project Structure

### Documentation (this feature)
```text
specs/012-harness-agnostic-mcp/
├── plan.md            # this file
├── spec.md            # engine-first spec (authoritative)
├── research.md        # Phase 0 — decisions
├── data-model.md      # Phase 1 — primitive + registry entities
├── contracts/         # Phase 1 — engine API + MCP surface + embed-facade split
├── quickstart.md      # Phase 1 — spike + behavioral validation
└── tasks.md           # Phase 2 — /speckit-tasks (NOT created here)
```

### Source Code (repository root) — concrete touch points
```text
src/live_index/view.rs   # NEW engine primitive: IndexBase, Overlay, IndexView, WorkingSet (engine-internal; NOT in embed contract test)
src/live_index/store.rs  # reuse ArcSwap snapshot (:667), generation fence (:865-927) — base producer, semantics unchanged
src/embed.rs             # deep-path reach to live_index::view + doc note; contract test UNCHANGED (SC-011)
src/main.rs              # defer launch-CWD pin only when find_project_root()==None (:211-223)
src/protocol/mod.rs      # un-gate bind_workspace_from_client_roots early-return (:814), preserve env>roots precedence
src/protocol/tools.rs    # project_root+readiness in symforge/status envelopes (reuse runtime_status_for :5627); path-outside guard; query-required validate
src/stel/status.rs       # add project_root to StelStatusContext + format
src/stel/edit_apply.rs   # shared normalize helper for if_match pre-flight compare (:91-98, :147-162); keep write-time byte guard
src/stel/types.rs        # query #[serde(default)] (already derives Default)
src/protocol/resources.rs# register glossary MCP resource (mirror TOOLS_CATALOG)
src/server/admin/api_v1.rs + mod.rs  # SHRUNK 013: read-only /api/v1 tenancy/telemetry DTO + one route
src/<tenancy>_store.rs   # SHRUNK 014: minimal durable registry, clone of ledger_store.rs shape (server-gated)
```

**Structure Decision**: Single crate, feature-split preserved. The primitive is engine-internal (`live_index`), reachable by embedders via the existing deep-path re-export but not frozen; all server wiring stays under `server`.

## Implementation sequencing (for /speckit-tasks)

1. **Spike (de-risk, first):** minimal `IndexBase`/`Overlay`/`IndexView` + generation fence; prove two consumers share one base (`Arc::ptr_eq`), overlays isolated, stale-overlay rejected; micro-bench rebase = O(dirty set) not O(repo). Falsify-or-proceed.
2. **Engine primitive:** complete `view.rs` (get_file/all_files resolution; WorkingSet + cross-project attributed search); embed-facade deep-path exposure + doc note (no contract change).
3. **Retarget + honesty (the field fix; parallelizable with 2):** un-gate roots hook (preserve env precedence); defer-bind in main.rs (None-root branch only); project_root in every response; path-outside guard; query-required validate; if_match normalize; glossary resource.
4. **Thin shim:** confirm stdio-per-connection multi-project end-to-end via existing per-session routing; document `/mcp` remote multi-tenant as deferred.
5. **Shrunk 013/014:** read-only `/api/v1` tenancy view; minimal durable registry + lazy rehydrate. No durable economics (DEF-001 deferred).

## Complexity Tracking

| Exception | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Transport Parity scoped exception: remote `/mcp` multi-tenant deferred (stdio-only multi-project in v1) | `/mcp` is a deliberate stateless single-`SymForgeServer`-over-one-index design (`mcp_http.rs:11-28`); per-connection sessions is an L-effort, regression-prone transport-mode change | Shipping `/mcp` multi-tenant now would block the cheap, high-value stdio fix on a risky transport rewrite; single-project parity is preserved and the deferral is explicitly spec-scoped per Principle VII |
