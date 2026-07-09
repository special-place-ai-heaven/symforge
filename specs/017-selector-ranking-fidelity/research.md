# Phase 0 Research: Selector & Concept-Ranking Fidelity

Investigation was done live against the running 8.13.8 index using SymForge itself
(`get_file_content`, `search_text`, `search_symbols`, `get_file_context`). All line
references are current as of commit `1272899` (8.13.8).

## P1 — `edit_plan` `Type::method` resolution

### Decision
Add a **type-name fallback** to `plan_edit` (`src/protocol/edit_plan.rs`): when a `X::Y`
target's left segment `X` matches no indexed file path, search all files for a symbol named
`Y` (the method) whose **enclosing `impl`/type is `X`**, and treat those as the symbol hits.
Bare-name, `file::symbol`, and file-path selectors are untouched.

### Rationale (root cause, proven)
- `split_path_qualified_target` (`edit_plan.rs:8`) splits on the first `::` into `(path, name)`.
- `plan_edit` (`edit_plan.rs:30`) then loops `for (path, file) in index.all_files()` and calls
  `collect_selector_hits(..., target_name)` **only when** `path == target_path || path.ends_with(target_path)`.
- For `GitRepo::tracked_paths`, `target_path = "GitRepo"` matches no file path → `collect_selector_hits`
  is never called → `symbol_hits` empty → "Target not found". Confirmed live: `tracked_paths` (bare)
  and `src/git.rs::tracked_paths` resolve; `GitRepo::tracked_paths`, `WorktreeCache::new`,
  `SharedIndexHandle::new`, `GitRepo::head_sha`, `WorktreeCache::lookup` all return "not found".
- `resolve_symbol_selector` (`src/live_index/query.rs` / `disambiguation.rs:155`) operates **within a
  single `IndexedFile`**, so it cannot be the type resolver by itself; the file must be chosen first.
  It already does kind-tier disambiguation (`kind_disambiguation_tier`, `disambiguation.rs:142`) — the
  class-vs-constructor case (`test_resolve_selector_class_vs_constructor_returns_class`).

### Mechanism for "method `Y` in type `X`"
The index stores methods hierarchically under their `impl` (outline shows `impl GitRepo L30-386`
containing `tracked_paths L48-63`). Containment is the deterministic signal: a candidate method `Y`
belongs to type `X` when an `impl X` (or `struct/enum/trait X`) symbol's `line_range` **contains**
the method's `line_range` in the same file. Preferred approach:
1. Collect all symbols named `Y` across files (method/function kind).
2. For each, check whether its file has an `impl`/type symbol whose name resolves to `X` and whose
   `line_range` encloses the candidate — reusing `SymbolRecord` ranges already in the index (no new
   data). `impl` display names in this index render as `impl GitRepo` / `impl Trait for GitRepo`, so
   `X` must be matched against the impl's target type token, not the raw display string.
3. If exactly one type-scoped match → Selected. If several (same method on same type across files, rare)
   → return all as ambiguous hits (existing `plan_edit` multi-hit path handles it).
4. If zero type-scoped matches → truthful not-found (FR-004), unchanged message shape.

### Alternatives considered
- **Extend `resolve_symbol_selector` to take a type constraint**: rejected as the primary path — it is
  file-scoped and widely called; adding a cross-file type search there broadens its contract. Keep the
  cross-file type resolution in `plan_edit`/a small helper and let `resolve_symbol_selector` stay
  file-scoped. (A thin shared helper in `disambiguation.rs` for "does symbol S belong to type T by
  range containment" is acceptable and reusable.)
- **Qualified-name string match** (store `GitRepo::tracked_paths` as a symbol name): rejected — the
  index does not store fully-qualified method names, and synthesizing them risks drift; range
  containment uses existing truth.
- **Treat `X::Y` as ambiguous and search both interpretations**: partially adopted — file
  interpretation is tried first (unchanged), type interpretation is the fallback only when no file
  matches, avoiding double resolution and preserving existing behavior for real `file::symbol`.

## P2 — `explore` concept ranking

### Decision
Rebalance the explore symbol scorer so a symbol's score reflects **concept proximity** (it lives in a
file/module the matched concept points at, is imported/co-located with concept-central code) in
addition to raw **name-token overlap**, and dampen the case where a single best name-token match takes
1.00 while conceptually-central symbols crater. The concept mapping in `src/protocol/explore.rs`
(`CONCEPT_MAP`, `match_concept`, stemming) already produces seed terms/files; the fix consumes those
signals in the scorer rather than letting name-token overlap dominate.

### Rationale (observed, root cause partially localized)
- `src/protocol/explore.rs` is **concept mapping only** (query → concept terms/seed files); it has no
  symbol scorer.
- Score→reason labels ("strong match", "query match") are rendered by
  `src/protocol/format.rs::explore_symbol_reason` (`format.rs:5455`), with view-filter tests
  `explore_result_view_filters_weak_trivial_symbols_and_doc_only_patterns` and
  `explore_result_view_keeps_trivial_symbol_when_strongly_contextualized` (`format.rs:6147`, `:6214`).
- The **numeric score** is produced upstream in the live-index explore query (the explore capture view
  in `src/live_index/query.rs` / `src/live_index/search.rs`). **Open item (resolve at implementation):**
  pin the exact scoring function via `find_references(explore_symbol_reason)` and the explore capture
  view, then locate where the 1.00/name-token weight is assigned.
- Live evidence: "worktree routing hook registration in the daemon" scored `worktree_routing_health_status`
  1.00 and dropped `register_if_feature_enabled` / `WorktreeAwareEditHook` / `edit_hooks::register`
  entirely; "watcher interact with analyze_file_impact" put an unrelated `run(SetupCliArgs)` at 1.00.

### Constraints the rebalance MUST honor
- **V Frecency Invariant**: explore is a discovery tool; the scorer MUST NOT read-then-write frecency.
  If frecency is fused into the score at all, it stays read-only (as `SearchFilesTier` fusion already
  is) and a test asserts explore does not bump frecency.
- **IV Determinism**: score ties break deterministically (e.g. by path then line); identical query +
  index ⇒ identical order. Assert with a stable-order test.
- **III Trust Envelopes**: the "ranked / truncated / map orients, tools prove" disclosure stays.
- **Do not over-correct**: an exact-name query (asking for a specific function by name) MUST still rank
  that function at/near the top — guarded by an anchor test.

### Risk / iteration note
P2 is a ranking-quality change with no single "correct" number. Approach: write the two anchor-query
top-N assertions FIRST (they fail on current behavior), then adjust weights until they pass without
regressing the existing `explore_result_view_*` tests. If a principled small rebalance cannot satisfy
both anchors without harming exact-name matches, P2 ships as a narrower improvement (documented) rather
than an over-tuned heuristic — P1 is unaffected either way.

## Cross-cutting decisions

- **Sequencing**: P1 first (crisp, low-risk, high-value), P2 second (separable). Each lands + verifies
  independently; the branch can carry both or split if P2 needs more iteration.
- **No new dependencies, no new persistence, no second index** (Constitution I/VI). All work reuses
  existing `SymbolRecord` ranges, the concept map, and the scorer.
- **Verification**: full gate + `cargo check --no-default-features --features embed` (VI) + a
  transport-parity consideration for any shared formatter touched (VII).
