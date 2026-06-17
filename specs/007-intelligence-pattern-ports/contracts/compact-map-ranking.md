# Contract: Importance-Ranked Compact Map

**Feature**: 007 · **Requirements**: FR-007..FR-009, FR-017 · **Story**: US3

## Scope

Applies ONLY to the **`detail=compact`** repo map (the default; rendered by
`src/sidecar/handlers.rs::repo_map_text`), specifically its file-bearing
"entry points" section. The `detail=full` (`format::repo_outline_view`) and
`detail=tree` (`format::file_tree_view`) outputs MUST be byte-identical to their
pre-007 form (FR-009). The alphabetical sort in
`capture_repo_outline_view` (query.rs ~L2295) is NOT the edit site and MUST NOT
change (it feeds full + tree).

## Scoring

```text
rank_key(file) = (dependent_count(file) DESC,
                  churn_score(file)     DESC,
                  relative_path(file)   ASC)   ; stable, deterministic tie-break
```

- `dependent_count` — distinct importing files for `file`
  (`find_dependents_for_file` grouped by file).
- `churn_score` — `git_temporal().files.get(file).churn_score` in `[0.0, 1.0]`;
  `0.0` when temporal is not `Ready` or the file is absent.
- Ties on both numeric keys break by `relative_path` ascending → identical input
  ⇒ identical order (FR-017).

## Display annotation

```text
line := path (" (→" N ")")?     ; the "(→N)" suffix present iff N >= 2
```

- `N` = `dependent_count(file)`.
- Files with `N < 2` render as bare `path` (no annotation).
- Annotation is appended to the existing compact entry-point line format; no
  other columns change.

## Token budget

- Must respect the existing compact-map byte budget (`build_with_budget`); the
  `(→N)` suffix is a few bytes per entry and ranking does not enlarge the listing.
- Ranking changes ORDER and adds a small suffix; it does not add or remove files
  beyond existing truncation behavior.

## Cost / safety

- Rank only the bounded entry-point candidate set, not every indexed file
  (avoid O(files²) dependent lookups).
- No frecency mutation (FR-014).
- No second index; all inputs from existing LiveIndex + `GitTemporalIndex`
  (Principle I).

## Test obligations

- Fixture: `core.rs` (≥8 dependents) vs `leaf.rs` (0) → `core.rs` ranks first.
- A file with `N >= 2` shows `(→N)`; a file with `N < 2` does not.
- `full` and `tree` outputs unchanged (snapshot equality vs pre-007).
- Deterministic order across repeated renders (tie-break proven).
- Compact map render does not create/bump frecency.
