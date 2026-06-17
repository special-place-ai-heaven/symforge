# Contract: STEL Find Fusion

**Feature**: 007 · **Requirements**: FR-010, FR-011, FR-014, FR-017 · **Story**: US4

## Surface

No new public MCP tool (FR-011). Fusion lives inside the existing find intent and
is implemented over the existing `search_symbols` / `search_text` / `search_files`
surfaces and their underlying ranking (`src/live_index/rank_signals.rs`). The
STEL planner (`src/stel/planner.rs::route_find`) is plan-only and must not
fabricate merged results; merging/ranking happens on the search/ranking layer.

## Behavior

Input: a multi-word, possibly-fuzzy query (e.g. `"stel planner find"`).

```text
1. tokenize query -> tokens[]                       (RankCtx.tokens)
2. candidates = symbol matches ∪ path/file matches   (both surfaces)
3. for each candidate: relevance = rank_signals::combine(path, RankCtx{
        tokens, target_path/co-change fields from search_files_coupling_neighbors })
4. sort candidates by relevance DESC, deterministic tie-break
5. return one merged ranked list in the existing output format
```

## Ranking rules

- Path side reuses `query::search_files_rank_score` (PathMatchSignal +
  CoChangeSignal). Symbol side contributes its tier match; both feed one ranked
  list.
- Co-change boost gating identical to `search_files`:
  `FILE_LEVEL_CO_CHANGE_FLOOR = 2`, `CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR = basename`.
- When co-change evidence is unavailable (policy disabled / no cold build / stale
  HEAD / no repo root), fusion degrades to pure path/name ranking — no error, no
  dropped results.
- Existing symbol tier ordering (Exact > Prefix > Substring) MUST be preserved
  (no golden-route regression).
- Deterministic: identical repo state + query ⇒ identical ordering (FR-017).

## Frecency invariant (hard gate)

- Fusion MUST stay on `search_symbols` / `search_text` / `search_files`, which
  never call `bump_frecency`.
- Fusion MUST NOT call `get_symbol` / `get_file_context` / `get_symbol_context` /
  `get_file_content` to obtain ranking inputs (those DO bump frecency).
- A query through the find intent MUST bump frecency zero times (FR-014).

## Test obligations

- Multi-word query matching both a symbol and a path → both appear, ranked, with
  co-change neighbors boosted.
- No new tool name appears in the tool list / STEL surface.
- Find query does not create/bump the frecency DB (mirror
  `tests/frecency_ranking.rs` `*_does_not_bump`, `FlagGuard::on()`).
- Co-change-unavailable repo → still returns sensible path/name-ranked results.
- Existing planner/golden-route tests updated, not broken.
