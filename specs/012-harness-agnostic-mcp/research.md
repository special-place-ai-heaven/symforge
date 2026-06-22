# Phase 0 Research — Feature 012 (engine-first)

Decisions resolved by two read-only design investigations (rust-pro) grounded in the working tree. Format: Decision / Rationale / Alternatives. All file:line anchors verified statically (no build run).

---

## D1 — Base+overlay `IndexView` generalizes the existing snapshot (does NOT replace it)

**Decision**: Add an engine-internal `IndexView` read surface = an immutable shared base (`Arc<LiveIndex>`, the existing snapshot) + an optional per-consumer copy-on-write `Overlay` of dirty/uncommitted deltas. New module `src/live_index/view.rs`. `LiveIndex` and `ArcSwap<LiveIndex>` are unchanged. The degenerate `IndexView { base, overlay: None }` is byte-for-byte today's behavior — the migration seam.

- Base identity: `BaseKey { canonical_root, commit }` (commit from `head_sha`, `git.rs:403`), carried on a new `IndexBase` wrapper — NOT added to `LiveIndex` (avoids touching the frozen `LiveIndex` contract).
- Overlay: `HashMap<rel_path, FileDelta::{Upsert(Arc<IndexedFile>)|Tombstone}>`; absent key = "see base". Resolution: overlay shadows base on key collision.
- Working set: `WorkingSet { entries: Vec<{project_id, base: Arc<IndexBase>, overlay}> }`. Two consumers with the same `BaseKey` hold the **same `Arc<IndexBase>`** → memory = base+epsilon (SC-002). Cross-project search iterates entries, tags hits with `project_id` (FR-004/SC-001).

**Rationale**: Reuses `Arc<LiveIndex>` sharing (already CoW at whole-index granularity, `store.rs:738`); keeps the hot single-consumer path allocation-free (`overlay: None`); confines volatility to one new module. Mirrors rust-analyzer/salsa: snapshot/fork primitive in the library.

**Alternatives rejected**: (a) per-consumer full `LiveIndex` clone → N×memory, the lock/reload storm the field hit; (b) mutable shared index + per-consumer locks → cross-consumer blocking (violates SC-003/005); (c) salsa-style query interning → large dep + rewrite of every plain-function query, wrong cost/benefit here.

**Anchors**: `store.rs:599-643` (LiveIndex), `:667-745` (ArcSwap read/write CoW), `query.rs:1028,1041` (get_file/all_files), `search.rs:789,919` (query fns over `&LiveIndex`), `git.rs:403/68/90`.

**Scoped sub-decision (derived indices)**: For the spike/MVP the overlay carries file-map deltas only; `search_text`/`find_references` (which use repo-wide `trigram_index`/`reverse_index`, `store.rs:616-622`) run against the base with overlay applied as a post-filter over the small dirty set. Per-consumer delta derived-indices are **deferred (L)**. This keeps overlay cost O(dirty files). Open risk: confirm text/reference search can be correctly served base+patch (UNVERIFIED statically).

---

## D2 — CoW invalidation = a single generation-fence equality check (reuse existing pattern)

**Decision**: An overlay is valid iff `overlay.base_key == base.key && overlay.base_generation == base.base_generation`. `IndexView::new(base, overlay)` returns `Err(StaleOverlay)` on mismatch (never serves stale). Three triggers:
- (a) **Base commit advances**: register a new `IndexBase` (new key + incremented generation); on next access the consumer **rebases** — recompute the dirty set from `uncommitted_paths()` (`git.rs:68`), drop deltas now absorbed by the base, re-derive only still-dirty deltas. Cost = O(dirty set).
- (b) **On-disk change via watcher**: feeds the **base** only (existing `update_file_at_generation`, `store.rs:865`, carries `project_generation` fence); never touches overlays. A consumer's overlay shadows the base for its dirty files until commit/discard.
- (c) **Consumer commits**: reduces to (a) for that consumer; other consumers' overlays untouched (they rebase independently).

**Rationale**: Reduces all three to one equality check on a fencing token already proven in `update_file_at_generation` (`store.rs:872-883`). Isolation (SC-003) is structural — separate `Overlay` values, `IndexView` only borrows its own; 0 cross-leak is not a runtime check but an ownership invariant.

**Alternatives rejected**: mtime-based validity (brittle — see the `touch_mtime` loop note `store.rs:929-934`); eager rebuild of all overlays on base advance (thundering herd, violates SC-009).

**Use a NEW `base_generation` counter on `IndexBase`**, distinct from `project_generation` (`store.rs:675`, bumped on reload/reset = project identity, not commit identity). Confirm distinctness in impl.

---

## D3 — Embed-facade boundary: expose via deep-path module, NOT the frozen contract (SC-011)

**Decision**: Do NOT add `IndexView`/`Overlay`/`IndexBase`/`BaseKey`/`FileDelta`/invalidation fns to `src/embed.rs` or its `#[cfg(test)] contract` (`embed.rs:52-181`). Expose them as `pub` in `live_index::view`, reachable by embedders via the existing deep-path re-export (`embed.rs:49` `pub use crate::live_index`). Add one doc note near the SEMVER banner (`embed.rs:15-21`): "`live_index::view::*` is opt-in, unstable, intentionally NOT part of the frozen contract."

| Item | Surface | Semver |
|---|---|---|
| `LiveIndex`, `IndexedFile`, `SharedIndex`, search fns, `GitRepo` | `embed.rs` flat facade + contract test | FROZEN (MAJOR on change) |
| `IndexView`, `Overlay`, `IndexBase`, `WorkingSet`, `FileDelta`, rebase fns | `live_index::view` (via `embed::live_index` deep path) | UNSTABLE (MINOR-churn OK, NOT in contract test) |
| `head_sha`/`uncommitted_paths` | `git.rs` (already NOT in `GitRepo` contract `embed.rs:141-145`) | leave off facade; promote only if AAP needs |

**Rationale**: The contract test IS the freeze mechanism — an item is frozen iff named there. Not naming the overlay types makes them structurally non-contracted yet reachable. Base payload type is `LiveIndex` (already contracted) → base layer needs zero facade change; SC-011 satisfied by addition-elsewhere. Honors Principle VI (compiles under `embed`, network-free).

**Alternatives rejected**: adding the primitive to the frozen facade → every invalidation fix = MAJOR bump + lockstep AAP upgrade (the spec's named anti-goal).

---

## D4 — Retarget reuses existing per-session retarget; the only blocker is one gate

**Decision**: The daemon already retargets a live session (`index_folder_for_session`, `daemon.rs:815-949`), and the proxy `index_folder` handler already drives it (`tools.rs:6033-6054`). The C4 fix is:
- **A (un-gate roots)**: condition the early-return in `bind_workspace_from_client_roots` (`mod.rs:814` `if self.capture_repo_root().is_some() { return; }`) so it does NOT fire on the daemon-proxy path — but ONLY skip when the bound root came from the `SYMFORGE_WORKSPACE_ROOT` env override (preserve `env > roots` precedence; the body passes `env=None` at `mod.rs:863`). Then the existing body resolves client roots and calls `self.index_folder(...)` → retargets the session. No new index plumbing.
- **B (defer launch pin)**: in `main.rs:211-223`, only defer the launch-CWD bind when `find_project_root()` is `None` (home-CWD launchers like Cursor); keep the existing pin when a root is found (single-harness happy path byte-for-byte unchanged — protects FR-016/SC-010).
- **C (explicit retarget)**: document `index_folder { path }` as the explicit retarget verb (zero code); optionally add a `project`/`root` field to `StelRequest` later (M).

**Rationale**: Smallest possible change; reuses the proven, lock-disciplined reassignment (`daemon.rs:867-911`). The daemon's per-session routing (`session_runtime`, `daemon.rs:951-970`) already gives N connections N independent project bindings.

**Anchors**: `daemon.rs:815-949,951-970,1163-1226`; `mod.rs:810-888,1129-1132`; `main.rs:211-223`; `discovery/mod.rs:702-724`.

---

## D5 — Transport: ship multi-project over stdio-per-connection; defer remote `/mcp` multi-tenant

**Decision**: stdio-per-connection multi-project is essentially "done once D4 lands" — each stdio process is a session with its own `project_id`, routed per-session by the daemon. The `/mcp` HTTP path is a deliberate **stateless single-`SymForgeServer`-over-one-index** design (`mcp_http.rs:1-28`, `stateful_mode=false`) with no per-connection session dimension; making it per-connection multi-tenant is L-effort and regression-prone. **Scope 012 v1 to stdio + daemon sessions; keep `/mcp` single-tenant (one serve = one project); record the deferral as a Principle-VII transport-specific scope** (plan Complexity Tracking).

**Rationale**: Delivers the high-value fix cheaply without a risky transport rewrite. The path to remote multi-tenant later is known: `stateful_mode=true` + `LocalSessionManager` (already imported `mcp_http.rs:42`) or a per-connection proxy `SymForgeServer` that opens its own daemon session — reusing D4 primitives.

---

## D6 — Surface-honesty fix points (exact seams)

- **Bound `project_root` + readiness in every response**: add `project_root: Option<String>` to `StelStatusContext` (`status.rs:51-69`), set in `from_server` (`:72-102`), print in `format_compact_status` (`:125-153`); for the `symforge` read facade reuse `runtime_status_for` (`tools.rs:5627-5655`, already formats `project_root` with backslash normalization). (Principle III.)
- **`path:` outside project → clear error**: in `symforge_stel_handler` (`tools.rs:8389`) before planning, canonicalize `request.path` and verify prefix of `capture_repo_root()`; else return `OutcomeClass::InvalidRequest`. Mirror `symbol_contract_violation` (`planner.rs:60-69`). Keep `path:` a within-project filter (FR-009/FR-014).
- **Omitted `query` → clean error**: root cause is `StelRequest.query: String` (non-Option, `types.rs:74`) failing rmcp `Parameters` deserialization (these are front-end-local tools, no `daemon::execute_tool_call` arm). Make `query` `#[serde(default)]` (already derives `Default`) and validate explicitly at the top of `symforge_facade_tool` (`tools.rs:8343`): empty → `InvalidRequest` "query is required".
- **`if_match` normalized compare**: add a shared normalize helper (line endings, optional BOM) used by both `verify_index_matches_disk` (`edit_apply.rs:147-162`, raw compare `:156`) and the `if_match` guard (`:91-98`). Normalize the **pre-flight** compare only; keep the write-time byte-exact splice guard (`:138-146`) intact (Principle IV idempotency).
- **Glossary MCP resource**: register beside `TOOLS_CATALOG_URI` in `resource_definitions` (`resources.rs:62-95`); add `Glossary` arm to `ResourceRequest` (`:29-59`) + render in `render_resource_text` (`:142-260`). Param descriptions live on the `#[tool(description)]` attrs (`tools.rs:8340/8818/8638`) + input-struct fields (`types.rs:73-86`). (Principle II — resource, not chat injection.)

---

## D7 — Shrunk 013/014 reuse existing patterns (no platform)

- **013 read-only `/api/v1` tenancy/telemetry**: add a `…View` DTO in `server/admin/api_v1.rs` (the established `from_runtime(&ServerRuntime)` + honest `available:false`/null pattern, e.g. `LedgerSummaryView:65-104`, `SystemSnapshot:218-257`) sourced from the daemon registry summaries (`SessionSummary`, `daemon.rs:802-810`), plus one `GET /api/v1/<name>` handler + one route in `server/admin/mod.rs:128-138`. Auth/origin inherited.
- **014 minimal durable registry**: a third store file in the exact `ledger_store.rs` shape (`enum Store { Sqlite(Connection), Disabled }`, idempotent `migrate()`, own `.symforge/<name>.db`, schema-versioned, bounded-text, `#[cfg(feature="server")]`), wired through `ServerRuntime` like `ledger_store()`. Tri-state honesty (`Durable | Disabled{reason} | Unavailable`, `status.rs:40-49`). **MUST hold registry/lazy-rehydrate metadata only — no symbol/reference query data (Principle I), no durable economics (DEF-001 stays in the existing STEL store; do not persist the un-grounded economy figures, audit C1-B).**

---

## Open risks carried into design/impl

- **Derived-index overlay cost** (D1 sub-decision): whether `search_text`/`find_references` can be served correctly from base + small overlay patch — UNVERIFIED statically; resolve in the full tier after the spike.
- **Rebase perf cliff** (D2): the spike's explicit falsifier — rebase must be O(dirty set), not O(repo), as the base commit advances.
- **`/mcp` stateless-singleton** (D5): the one transport linchpin; deferred and scoped.
- **Regression on single-harness flow** (D4-B): mitigated by deferring the launch pin only on `None`-root; `surface_honesty`/status tests assert exact lines and will need updates for the new `project_root` line (intentional, test-visible).
- **AAP's actual overlay needs**: confirm with the AAP owner before stabilizing any `view` signature (it currently consumes the frozen facade only).
