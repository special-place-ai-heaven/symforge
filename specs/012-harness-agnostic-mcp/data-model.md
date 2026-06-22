# Phase 1 Data Model — Feature 012 (engine-first)

Entities are conceptual (validation rules + relationships), not final Rust signatures. Engine primitive entities live in `src/live_index/view.rs`; registry entity in the shrunk-014 store.

## Engine primitive (in-memory; `live_index::view`)

### BaseKey
Identity of an immutable set of repository facts.
- `canonical_root: PathBuf` — the declared, canonicalized workspace root (keys the shared file-watch layer; different worktrees of one logical repo = different keys).
- `commit: CommitId` — `head_sha` (`git.rs:403`) or a `Dirtyless` sentinel when not a git repo.
- Rule: two consumers with equal `BaseKey` MUST share one `IndexBase` allocation (SC-002).

### IndexBase
Wraps the existing immutable snapshot.
- `key: BaseKey`
- `index: Arc<LiveIndex>` — the existing `ArcSwap<LiveIndex>` snapshot, unchanged (`store.rs:667`).
- `base_generation: u64` — monotonic; NEW counter, distinct from `LiveIndex.project_generation` (which is project identity, `store.rs:675`).
- Rule: immutable once published; a new commit produces a NEW `IndexBase` (new key + incremented generation), never mutates an existing one.

### FileDelta
- `Upsert(Arc<IndexedFile>)` | `Tombstone`.

### Overlay
A single consumer's copy-on-write deltas over one `IndexBase`.
- `base_key: BaseKey` — the base this overlay was derived against.
- `base_generation: u64` — fencing token.
- `deltas: HashMap<rel_path, FileDelta>` — ONLY dirty/uncommitted files; absent key = "see base".
- Rule (isolation, SC-003): an `Overlay` is owned by exactly one consumer; never shared, never read by another consumer's view. Cross-leak is an ownership invariant, not a runtime check.
- Rule (validity, IV): valid iff `base_key == base.key && base_generation == base.base_generation`; otherwise stale → must rebase before read.

### IndexView (the read surface)
- `base: &LiveIndex` + `overlay: Option<&Overlay>`.
- Constructor returns `Err(StaleOverlay)` if the overlay's fence != base's (never serve stale).
- Resolution: `get_file` / `all_files` → overlay shadows base on key collision; `Tombstone` hides a base file. `overlay: None` == today's behavior (migration seam).

### WorkingSet
A consumer's set of open projects.
- `entries: Vec<{ project_id, base: Arc<IndexBase>, overlay: Overlay }>`.
- Rule: cross-project query iterates entries, builds an `IndexView` per entry, tags each hit with `project_id` (FR-004 / SC-001).
- Rule: add/remove/switch projects mutates only this consumer's `WorkingSet` (FR-006).

### State transitions (overlay lifecycle)
```
fresh(base) --consumer edit--> dirty(deltas)
dirty --base commit advances--> rebase(recompute dirty set from uncommitted_paths; drop absorbed; keep still-dirty)
dirty --consumer commits--> rebase(self) (deltas now in base are dropped; other consumers untouched)
any --fence mismatch on read--> StaleOverlay (force rebase)
```

## Tenant/working-set registry (durable metadata; shrunk 014, server-gated)

### RegistryEntry
- `connection_id` / stable identity, `project_id`, `canonical_root`, `last_seen`, `working_set: [project_id]`, lazy-rehydrate metadata (what to rebuild on first access).
- Rule (Principle I): registry holds **metadata only** — NO symbol/reference/query data. The code index is rebuilt from source on lazy access; the registry just records what to rehydrate.
- Rule: tri-state availability (`Durable | Disabled{reason} | Unavailable`, mirror `status.rs:40-49`); never fail a request because the registry is down.
- Storage: own `.symforge/<name>.db`, idempotent `migrate()`, schema-versioned (clone `ledger_store.rs` shape). NOT the STEL economics ledger (DEF-001 stays separate, deferred).

## Surface entities (MCP)

- **StelRequest.query** → becomes `#[serde(default)]` (already derives `Default`); empty validated to a clean `InvalidRequest` ("query is required").
- **StelRequest.path** → unchanged type, but validated as within bound project (out-of-project → clear error). Distinct from project selection.
- **Status context** → gains `project_root: Option<String>` (surfaced in every response).
- **Glossary resource** → a new read-only MCP resource (URI + render text), defining vocabulary.

## Relationships
```
WorkingSet 1—* WorkingSetEntry *—1 IndexBase (shared by Arc across consumers on same BaseKey)
WorkingSetEntry 1—1 Overlay (private per consumer)
IndexBase 1—1 Arc<LiveIndex> (existing snapshot)
RegistryEntry *—1 project_id (durable metadata mirror of live WorkingSet, for rehydrate)
```
