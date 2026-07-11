# Phase 0 Research: Dogfood Surface Hardening

Investigation was done against the current tree on branch `018-dogfood-surface-hardening`
(off `main` @ `62ccae5`, which includes the 017 fixes and the 8.13.9 bump), using
grep/read over the actual source. Line references are current as of this branch.

## US1 — Source-focused change/impact by default

### Decision
Two surgical, mode-scoped default changes; **no admission-tier rewrite** (deferred as an
alternative):
1. **`what_changed` uncommitted mode**: flip the `code_only` default from `false` → `true`
   at the **uncommitted call site only** (`tools.rs:7035`). Timestamp (`6958`) and GitRef
   (`7109`) keep `false` — the noise is untracked working-tree data files, which only appear
   in uncommitted mode.
2. **`detect_impact`**: its input defaults `include_untracked=true` (`tools.rs:501/540-543`),
   so untracked `mcps/*.json` enter the changed-set and their JSON-key symbols drive the
   inbound blast-radius walk (`graph.rs compute_impact`). Default the impact changed-set to
   **source-focused** (reuse `filter_paths_by_prefix_and_language(..., code_only=true)` /
   language filter, `tools.rs:724-755`) so the walk starts from source symbols only.
   Preserve an explicit opt-in that restores full inclusion.

Explicit callers are untouched: `params.code_only` is still honored when set
(`unwrap_or` only changes the *default*), so `code_only=false` restores the old inclusive
result (FR-003).

### Rationale (root cause, proven)
- `filter_paths_by_prefix_and_language` already excludes non-source paths when `code_only`
  is set (`tools.rs:752-755`, via `LanguageId::from_extension`). The machinery exists; only
  the default is wrong. This is a defaulting fix, not new capability (matches the spec).
- The three `code_only.unwrap_or(false)` sites are exactly the three `what_changed` modes;
  flipping only the uncommitted one is a one-line, mode-scoped change that satisfies SC-001
  without touching committed-diff semantics (edge case in spec).
- For `detect_impact`, filtering the changed-set to source before the graph walk means JSON
  keys never seed the BFS; since data files are not referenced by source code, they cannot
  re-enter the blast radius — so the changed-set filter fixes SC-002 without a graph rewrite.

### Alternatives considered
- **Admission-tier demotion** (report finding #6): classify pure-data directories as
  metadata-only at index time so JSON keys never become first-class symbols. Rejected as the
  *primary* fix — it is a broader, riskier change (affects `search_symbols`/`get_symbol`/
  ranking globally and several existing tests) for a problem the two defaulting changes
  already solve for the change/impact surface. Kept as a documented follow-up if a future
  finding shows data symbols leaking into other surfaces; out of scope for 018 unless the
  defaulting proves insufficient in testing.
- **Filter in the git layer** (`git.rs uncommitted_paths`): rejected — pushes a source-only
  policy into a faithful `git status` primitive that other callers rely on; the filter
  belongs at the tool boundary where `code_only` already lives.

## US2 — Browse-mode importance ranking

### Decision
In `search.rs` (browse path around `:870`) and the sibling `view.rs:346`, detect **browse
intent** = empty/whitespace query **AND** at least one scope filter (`kind` or `path_prefix`).
In that case, skip the `!name_lower.contains(&query_lower)` short-circuit (which passes
everything for an empty query) and rank candidates by **reference count (desc) → kind
priority → path → line** instead of the current path+line order. Non-empty-query behavior is
byte-identical (FR-005).

### Rationale
- `if !name_lower.contains(&query_lower) { continue; }` at `search.rs:870` / `view.rs:346`
  passes every symbol when `query_lower == ""`, then results sort by path+line, surfacing
  generic short names first (confirmed).
- Reference counts are already available via the index's reverse-reference data (the same
  source `find_references` uses); reusing them adds no new persisted data (Constitution I/VI).
- Determinism (IV): the full tie-break chain (refs → kind → path → line) is total, so
  identical state ⇒ identical order (SC-003 test asserts stability).

### Alternatives considered
- **Kind-priority only** (no ref counts): simpler but weaker — wouldn't lift a heavily-used
  function above a rarely-used one of the same kind. Use ref-count as primary, kind as
  secondary.
- **Reinterpret empty-query-without-scope as browse**: rejected (spec edge case) — an
  unscoped empty query keeps today's meaning to avoid changing that call's semantics.

## US3 — Repo-map workspace-root containment guard

### Decision
Add a root-containment guard inside `capture_repo_outline_view` (`query.rs:2388`): reuse the
index's stored canonical root (the `indexed_root` field, present on the index — seen at
`query.rs:3100/5288/5327`) and drop any file whose resolved path escapes it, reusing the
existing `normalize_lexically` (`tools.rs:76`) / `path_is_within_bound_project`
(`tools.rs:108`) helpers rather than writing new path math. Legitimate in-root files are
unaffected (FR-006, SC-004).

### Rationale
- `capture_repo_outline_view` currently maps `all_files()` (`query.rs:2391-2399`) with no
  containment check; a stored path that is absolute-outside-root or `..`-escaping surfaces in
  the outline (confirmed). The reported `.grok/config.toml` example was induced by the
  reporter's own `index_folder(add=true)` test, but the missing guard is real.
- The helpers already encode "is this path within the bound project root"; reuse keeps the
  guard consistent with the rest of the codebase and minimal.

### Open implementation choice (small, deferred to tasks)
- Guard *inside* `capture_repo_outline_view` (needs `indexed_root` reachable from `&self` —
  it is) vs. sanitize in the `get_repo_map` handler after collection. Prefer in-view
  (collection-time) so every caller of the view benefits; confirm `indexed_root` accessor
  during implementation.

## US4 — CCR retrieval footer on truncation

### Decision
Introduce one shared helper — `enforce_token_budget_with_ccr(store, tool_name, result,
max_tokens)` — that runs the CCR decision on the **complete** payload before the hard cut and
emits the `symforge_retrieve` footer when truncation occurs (the logic already at
`mod.rs:705-709`). Route the **big-response builders** (e.g. `get_repo_map` full, wide
`search_symbols`/`search_text`, `explore`) through it instead of the bare
`format::enforce_token_budget`. Responses within budget get no footer (FR-008).

### Rationale (root cause, proven)
- `apply_ccr_overflow` has exactly **one** production call site (`mod.rs:705-709`); every
  other big-response path calls `format::enforce_token_budget` **directly** (~16 sites incl.
  `tools.rs:4041/4233/5413/8082/10573/10692`, `format.rs:3922`), which hard-truncates with no
  retrieve footer. That is precisely why the footer is "frequently absent" (report finding #5,
  confirmed by call-graph).
- Wrapping the decision in one helper and swapping the big-response sites is the smallest
  principled fix: it centralizes the "truncate → offer retrieval" contract instead of
  duplicating the `mod.rs:705` pattern per site.

### Constraints / risk
- **Transport parity (VII)**: `mod.rs:705` is on a shared path; if the new helper changes a
  formatter both transports call, add a parity assertion (Polish).
- **Determinism (IV)**: the CCR hash is content-addressed over the full payload — deterministic
  for identical state.
- **Scope risk**: US4 touches several call sites. If routing every big-response site cleanly is
  larger than expected, ship the highest-value builders first (`get_repo_map` full + the widest
  search) and document the residual — the other three stories are unaffected (P4, lowest
  priority).

## Cross-cutting

- **Sequencing**: US1 → US2 → US3 → US4 (priority order). Each lands + verifies independently;
  the branch carries all four but any subset is shippable.
- **No new dependencies, no new persistence, no second index** (Constitution I/VI). All four
  reuse existing filters, reverse-reference data, path helpers, and the CCR store.
- **Verification**: full gate + `cargo check --no-default-features --features embed` (VI) +
  transport-parity consideration for any shared formatter touched (VII, esp. US4).
- **Frecency (V)**: all four are read paths; US2 ranking reads reference counts (not frecency)
  and writes nothing back — a frecency-neutrality test guards it.
