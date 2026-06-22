# Phase 1 Contracts — Feature 012 (engine-first)

Capability-level contracts (not final code). Three surfaces: the engine primitive (library), the embed-facade split, and the MCP surface changes.

## 1. Engine primitive (`live_index::view`) — UNSTABLE library API (NOT in embed contract test)

```
// Identity + base
BaseKey { canonical_root, commit }
IndexBase::from_snapshot(key: BaseKey, index: Arc<LiveIndex>, base_generation: u64) -> IndexBase

// Per-consumer overlay
Overlay::empty(base_key, base_generation) -> Overlay
Overlay::upsert(&mut self, rel_path, Arc<IndexedFile>)
Overlay::tombstone(&mut self, rel_path)
Overlay::rebase(&mut self, new_base: &IndexBase, dirty: &[rel_path]) -> Result<()>   // O(dirty)

// Read surface
IndexView::new(base: &LiveIndex, overlay: Option<&Overlay>) -> Result<IndexView, StaleOverlay>
IndexView::get_file(&self, path) -> Option<&IndexedFile>
IndexView::all_files(&self) -> impl Iterator<Item=&IndexedFile>
// search_symbols over a view = iterate all_files; search_text/find_references base-only + overlay post-filter in MVP

// Working set (cross-project)
WorkingSet::add(&mut self, project_id, base: Arc<IndexBase>)
WorkingSet::remove(&mut self, project_id)
WorkingSet::query(&self, targets: Targets, q) -> Vec<Hit{ project_id, .. }>   // Targets = One|Subset|All
```

**Contract guarantees**: shared base by `Arc` (SC-002); overlay isolation by ownership (SC-003); `IndexView::new` rejects a stale overlay (IV); `overlay: None` is byte-for-byte current behavior (no-regression seam).

## 2. Embed-facade split (`src/embed.rs`) — SC-011

- **FROZEN (unchanged, in contract test `embed.rs:52-181`)**: `LiveIndex`, `SharedIndex`, `IndexedFile`, `PublishedIndexState`, search free-fns, `GitRepo` (3 methods). Zero change.
- **UNSTABLE (reachable via `embed::live_index::view`, NOT in contract test)**: everything in §1.
- **Doc note** added near `embed.rs:15-21`: "`live_index::view::*` is opt-in, unstable, not part of the frozen contract."
- **Gate**: `cargo check --no-default-features --features embed` stays green and network-free (Principle VI).

## 3. MCP surface changes

### 3a. Retarget (reuses `index_folder`)
- `index_folder { path }` documented as the explicit retarget verb (existing handler `tools.rs:6033-6054` → `index_folder_for_session` `daemon.rs:815`). No new tool.
- Client `roots` retarget: un-gate `bind_workspace_from_client_roots` (`mod.rs:814`) for the daemon-proxy path, preserving `env > roots`.
- (Optional later) `StelRequest { project?/root? }` field — deferred.

### 3b. Bound-root visibility (every response)
- `status` + `symforge` envelopes include `project_root` and index readiness (reuse `runtime_status_for` `tools.rs:5627`). Contract: a consumer can always read which project answered.

### 3c. Honest errors
- Omitted `query` → `OutcomeClass::InvalidRequest` "query is required" (not an opaque deserialize error).
- `path:` outside bound project → `InvalidRequest` "path is outside the bound project <root>" (not empty/misleading). `path:` remains a within-project filter.
- `if_match` compares normalized on-disk bytes (line endings/BOM) — a valid edit matching current file is NOT falsely rejected; write-time byte-exact splice guard preserved.

### 3d. Glossary resource (read-only)
- New MCP resource `symforge://glossary` (register in `resources.rs:62-95`). Renders: project-target vs within-project filter; intents; server vs background-service vs install; base+overlay model; that economy figures are ESTIMATES; status fields.

### 3e. Shrunk 013 — read-only tenancy/telemetry JSON
- `GET /api/v1/<tenancy>` returns a `…View` DTO (`from_runtime`, honest `available:false`/null when absent) from the daemon registry summaries. One route in `server/admin/mod.rs:128-138`. Host (AAP) renders its own UI; SymForge ships no multi-tenant console.

### 3f. Shrunk 014 — minimal durable registry
- A `…Store { Sqlite | Disabled }` (clone `ledger_store.rs` shape), own `.symforge/*.db`, server-gated. Holds registry + lazy-rehydrate metadata ONLY. No query data (I), no durable economics (DEF-001 deferred).

## Transport scope (Principle VII)
- stdio-per-connection multi-project: in scope (rides existing per-session routing).
- remote `/mcp` multi-tenant: **deferred, transport-specific scope** (`mcp_http.rs` stateless singleton). Single-project behavior stays stdio<->serve equivalent.
