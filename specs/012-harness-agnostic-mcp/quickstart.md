# Quickstart / Validation Guide — Feature 012 (engine-first)

Runnable validation scenarios that prove the feature works end-to-end. Implementation detail lives in tasks.md; this is the run/verify guide. Backend gate per Principle VIII: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release` (+ `cargo check --no-default-features --features embed` for VI).

## S0 — De-risk spike (do FIRST; gate the rest)

Prove the primitive before building on it.

- **Setup**: load one small real tree via `LiveIndex::load`; wrap in `Arc<IndexBase>`; create TWO `Overlay`s over the SAME `Arc<IndexBase>`.
- **SC-002 (shared base)**: assert `Arc::ptr_eq` on the two bases; assert adding consumer #2 adds only overlay-sized bytes (no second `files` map; inner `files` Arc strong_count does not double).
- **SC-003 (isolation)**: consumer A `Upsert("foo.rs")`; assert `view_B.get_file("foo.rs")` == base content and `view_A.get_file("foo.rs")` == A's overlay; a `Tombstone` in A is invisible to B.
- **No forced reload**: mutate base via `update_file_at_generation` (A's reindex); assert B's existing `IndexView` over the prior snapshot still reads (ArcSwap immutability).
- **Stale fence**: advance `base_generation`; assert `IndexView::new(new_base, old_overlay)` → `Err(StaleOverlay)`.
- **FALSIFIER (the named risk)**: micro-bench rebase as a function of dirty-set size K vs base size N (N ∈ {1k, 10k} files). PASS iff rebase ≈ O(K), independent of N. FAIL → revisit the primitive design before proceeding.

## S1 — Engine primitive: cross-project search (SC-001)

- Index 3 repositories into one `WorkingSet`.
- Run a cross-project search; assert matches return from each targeted project, each tagged with the correct `project_id`; assert subset/single-target scoping works.

## S2 — Retarget / wrong-repo fix (SC-004) — behavioral dogfood

- Launch the MCP server bound to repo A (or with home CWD); confirm `status` shows `project_root` = A and every response carries the bound root.
- Change declared workspace roots to repo B (or call `index_folder { path: B }`); assert the next query and `status` reflect B; assert 0 silent wrong-repo answers across a query set.
- Regression: a CWD-launched single-harness client still binds correctly (no defer) — single-harness behavior unchanged (SC-010).

## S3 — No-lockout (SC-005)

- Two consumers index the same canonical root; from an external editor, save a file under that root; assert the save is NOT blocked by SymForge and both consumers converge.

## S4 — Surface honesty (SC-006/SC-007)

- Omit `query` → assert clean "query is required" (no opaque deserialize stack).
- `path:` outside the bound project → assert clear "path outside project" error (not empty results).
- Guarded edit whose `if_match` equals current on-disk bytes (incl. a CRLF round-trip) → assert it is APPLIED, not falsely rejected; preview leaves git clean; restore byte-exact.
- Fetch `symforge://glossary` → assert it returns and documents project-target vs within-project filter and that economy figures are estimates.

## S5 — Shrunk 013/014 (SC-009)

- `GET /api/v1/<tenancy>` returns the read-only DTO (honest `available:false` when registry absent); no SymForge-owned dashboard is added.
- Restart the server; assert NO all-at-once rebuild — `(consumer, project)` bases rehydrate lazily on first access, driven by the durable registry metadata.

## Multi-consumer dogfood (the headline behavioral proof)

Two harness connections to one server (stdio), different + one overlapping repo:
1. Each queries its own repo → correct, source-scoped, 0 cross-leak (SC-003).
2. Both on the same repo → shared base (memory ≈ base+ε, SC-002).
3. One retargets / reindexes → the other sees 0 forced reload/interruption (SC-003/SC-004).
4. One server process, 0 extra listeners, 0 stray console windows (SC-008).

## Embed isolation (SC-011 / Principle VI)

- `cargo check --no-default-features --features embed` green; the `embed.rs` contract test UNCHANGED; `live_index::view::*` reachable via `embed::live_index` but absent from the contract test.
