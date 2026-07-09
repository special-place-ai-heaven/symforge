# Phase 1 Data Model: Dogfood Surface Hardening

This feature adds **no new persisted entities and no schema change** — all four fixes are
read-path/formatting behavior changes over data the LiveIndex already holds. The "entities"
below are the existing structures whose *observable behavior* changes.

## Touched structures

| Structure | Where | Change | Invariants preserved |
|-----------|-------|--------|----------------------|
| `WhatChangedInput.code_only` | `tools.rs:469` | Default becomes source-focused for **uncommitted mode only** (`tools.rs:7035`). Field type unchanged (`Option<bool>`); explicit values still honored. | III (disclosure states the filter), IV (deterministic), no persistence change. |
| Impact input (`include_untracked=true`) + new `include_data: Option<bool>` | `tools.rs:501/540-543` | Impact changed-set defaults to source-focused before the `graph.rs compute_impact` inbound walk; the new backward-compatible `include_data=true` opt-in restores full inclusion (FR-001/FR-003). | Graph structure unchanged; only the seed set is filtered. Old callers unaffected (serde-default). V frecency-neutral. |
| Browse result ordering | `search.rs:~870`, `view.rs:346` | Empty-query-with-scope results ranked by `(reference_count desc, kind_priority, path, line)` instead of path+line. | Non-empty-query path byte-identical; IV determinism via total tie-break. |
| `RepoOutlineView` collection | `query.rs:2388 capture_repo_outline_view` | Files whose resolved path escapes `indexed_root` are dropped/quarantined before building the view. | In-root files unaffected; reuses `normalize_lexically`/`path_is_within_bound_project`. |
| Truncation path | `ccr.rs:186 apply_ccr_overflow`, `mod.rs:705`, ~16 `enforce_token_budget` sites | Big-response builders route through a shared `enforce_token_budget_with_ccr` helper that emits a `symforge_retrieve` footer on truncation. | Within-budget responses gain no footer (FR-008); CCR hash content-addressed → IV deterministic. |

## Reused signals (no new data)

- **`code_only` / language filter** — `filter_paths_by_prefix_and_language` (`tools.rs:724`) already classifies source vs. non-source via `LanguageId::from_extension`. US1 reuses it.
- **Reverse-reference counts** — the same data `find_references` consumes; US2 reads it for importance ranking. Read-only w.r.t. frecency (V).
- **`indexed_root`** — canonical workspace root already stored on the index (`query.rs:3100/5288/5327`). US3 reuses it.
- **CCR store + `symforge_retrieve`** — existing content-compression store and retrieval tool. US4 changes *when* the decision runs and *whether* the footer is emitted, not the backend.

## Non-goals (explicitly not modeled)

- No admission-tier reclassification of data directories in 018 (deferred alternative; would be a real data-model change if adopted later).
- No multi-project retrieval model (finding #2, out of scope).
- No new index, table, snapshot format, or persisted ranking signal.
