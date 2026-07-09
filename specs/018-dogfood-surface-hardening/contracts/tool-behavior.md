# Phase 1 Contracts: Observable Tool-Behavior Deltas

The "contract" for an MCP code-intelligence server is the observable behavior of its tools.
This file states the before/after contract per tool. No tool is added or removed; no input
field is removed. New defaults and one new internal helper only.

## US1 — `what_changed` (uncommitted) + `detect_impact`

**`what_changed`**
- Input: unchanged (`uncommitted`, `git_ref`, `since`, `path_prefix`, `language`, `code_only`, `include_symbol_diff`, `max_tokens`).
- Delta: in **uncommitted mode**, when `code_only` is **unset**, behavior is now source-focused (non-source data paths excluded). `code_only=false` explicitly restores full inclusion. Timestamp and git_ref modes unchanged.
- Disclosure (III): the envelope continues to state mode and that a filter was applied.

**`detect_impact`**
- Input: unchanged (`include_untracked` default stays `true`).
- Delta: the changed-set feeding the impact walk is source-focused by default; data-file-derived symbols no longer dominate the blast radius. Explicit data-inclusion opt-in restores prior behavior.
- Invariant: graph structure and depth semantics unchanged; only the seed set is filtered.

**Regression contract**: with only a dirty `mcps/x.json`, default `what_changed(uncommitted=true)` lists 0 code changes; default `detect_impact` yields no JSON-key high-risk symbols; `code_only=false` reproduces the old inclusive output.

## US2 — `search_symbols` (browse)

- Input: unchanged (`query` optional, `kind`, `path_prefix`, `language`, `limit`).
- Delta: when `query` is empty/whitespace **and** a scope filter (`kind` and/or `path_prefix`) is present, results are ranked by `(reference_count desc, kind_priority, path, line)`. Non-empty query: unchanged.
- Invariant (IV): identical state ⇒ identical order (total tie-break). Frecency-neutral (V).

**Regression contract**: a browse call over a scope containing both a heavily-referenced symbol and generic short names (`add`/`get`/`len`) surfaces the heavily-referenced symbol ahead of the trivial names; the same call twice yields identical ordering.

## US3 — `get_repo_map` (detail=full)

- Input: unchanged.
- Delta: the full outline excludes any file whose resolved path escapes the workspace root (`indexed_root`). In-root files unaffected.
- Invariant: outline file count for a clean in-root repo is identical to today.

**Regression contract**: an index seeded with an out-of-root path returns a full map containing zero out-of-root paths; a normal repo's outline is unchanged.

## US4 — truncation → `symforge_retrieve` footer

- Input: unchanged (`max_tokens` on big-response tools).
- Delta: big-response builders route truncation through `enforce_token_budget_with_ccr`; when truncation occurs the output ends with a `symforge_retrieve` footer whose hash fetches the full payload. Within-budget responses are unchanged (no footer).
- Invariant (IV): CCR hash is content-addressed (deterministic). Parity (VII): shared-path change covered by a parity assertion if a shared formatter signature changes.

**Regression contract**: a large response under a tight `max_tokens` ends with a `symforge_retrieve` hash line; fetching that hash returns the full pre-truncation payload; a small response has no footer.

## Cross-cutting invariants (all four)

- **III Trust**: ranked/truncated/filtered status remains disclosed.
- **IV Determinism**: identical repo state ⇒ identical output/order.
- **V Frecency**: none of these read paths write frecency back.
- **VI Embed**: `cargo check --no-default-features --features embed` stays green.
- **VII Parity**: stdio ≡ serve for each behavior.
