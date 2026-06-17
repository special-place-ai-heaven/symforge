# Phase 1 Data Model: Intelligence Pattern Ports (007)

**Date**: 2026-06-16 · **Branch**: `007-intelligence-pattern-ports`

This feature introduces **no persistent storage** and **no new authoritative
index** (Constitution Principle I). All "entities" are transient view/render
structures derived from existing in-memory indices (LiveIndex reverse-import
index + `GitTemporalIndex`). The data model below documents the shapes produced
and the existing types reused.

---

## Reused existing types (not modified except where noted)

| Type | Location | Role in 007 |
|------|----------|-------------|
| `LiveIndex` | `src/live_index/store.rs` | Source of dependents (`find_dependents_for_file`, `capture_find_dependents_view`) and temporal (`git_temporal()`) |
| `FindDependentsView { files: Vec<DependentFileView> }` | `src/live_index/query.rs:875` | `files.len()` = distinct dependent file count |
| `GitTemporalIndex { files: HashMap<String,GitFileHistory>, stats, state }` | `src/live_index/git_temporal.rs:243` | Co-change + churn source |
| `GitTemporalState { Pending, Computing, Ready, Unavailable(String) }` | `src/live_index/git_temporal.rs:255` | Readiness gate; only `Ready` yields data |
| `GitFileHistory { co_changes: Vec<CoChangeEntry>, weak_co_changes, churn_score: f32, commit_count: u32, … }` | `src/live_index/git_temporal.rs:128` | Per-file co-change partners + churn |
| `CoChangeEntry { path: String, coupling_score: f32, shared_commits: u32 }` | `src/live_index/git_temporal.rs:175` | Co-change partner record; `path` is what the footer/line lists |
| `GitTemporalStats { hotspots: Vec<(String,f32)>, … }` | `src/live_index/git_temporal.rs:225` | Cheap churn ranking input |
| `RankCtx { query, tokens: Vec<String>, target_path, co_change_count, co_change_weighted_score, … }` | `src/live_index/rank_signals.rs:60` | Fusion context for find ranking (multi-word via `tokens`) |
| `RepoOutlineFileView { relative_path, language, symbol_count, noise_class }` | `src/live_index/query.rs:687` | **full/tree** view rows — left UNCHANGED by 007 |

> `RepoOutlineFileView` and `capture_repo_outline_view`'s sort are deliberately
> NOT modified (they feed `full` + `tree`, which Q3/FR-009 preserve). The compact
> ranking operates on the `repo_map_text` entry-point set instead.

---

## Entity 1: Impact Footer (transient render value)

A success-only suffix appended to structural-edit responses.

- **Shape (rendered string)**: `[impact: N dependents · cochanges: a, b, c]`
  - `N` — `usize`, distinct dependent file count.
  - `cochanges: …` clause — present only when temporal `Ready` and partners exist;
    `a, b, c` are up to K (default 3) `CoChangeEntry.path` values.
  - Degraded form: `[impact: N dependents]` (no co-change clause).
- **Derivation**:
  - `N` ← `index.capture_find_dependents_view(path).files.len()`.
  - partners ← `index.git_temporal()`, gate `state == Ready`,
    `files.get(path).co_changes.iter().take(K).map(|e| e.path)`.
- **Lifecycle**: Built at the inner-handler success tail, before
  `complete_mutation_replay` (persisted in idempotency replay). Never built on
  failed/rejected/dry-run-only-without-write paths (see contract).
- **Validation/invariants**:
  - Success-only (FR-004).
  - Trust envelope unchanged (FR-004).
  - Text free of `classify_edit_output` sentinels (`Error`, `unavailable`,
    `byte range`, `Write failed`, `[DRY RUN]`, `Ambiguous:`, `Symbol not found:`).
  - Path keys forward-slash-normalized before lookup (Windows safety).

## Entity 2: Ranked Map Entry (transient render value)

A file line in the **`detail=compact`** map's entry-point section.

- **Shape (rendered line)**: `path (→N)` when `N >= 2`, else `path`.
- **Ordering key**: `(dependent_count desc, churn_score desc, relative_path asc)`.
  `relative_path asc` is the deterministic stable tie-break (FR-017).
- **Derivation**:
  - `dependent_count` ← distinct importing files for the entry-point path.
  - `churn_score` ← `git_temporal().files.get(path).churn_score` (or
    `stats.hotspots`), `0.0` when temporal not Ready.
- **Lifecycle**: Computed when the compact map is rendered (`repo_map_text`);
  bounded to the entry-point candidate set (not every file → no O(files²)).
- **Validation/invariants**:
  - `full` and `tree` modes byte-unchanged (FR-009).
  - Deterministic order across runs (FR-017).
  - No frecency mutation (FR-014).

## Entity 3: Find Result (transient ranked entry)

A merged symbol-or-path hit from the STEL find intent.

- **Shape (conceptual)**: `{ kind: Symbol | Path, label, relevance: f32 }`,
  rendered in the existing find/search output format (no new tool, no new public
  schema field).
- **Ordering key**: `relevance desc` where `relevance` comes from
  `rank_signals::combine(path, &RankCtx)` (path-match tier + gated co-change
  boost), with a deterministic tie-break.
- **Derivation**: tokenize query → `RankCtx.tokens`; symbol matcher + path matcher
  produce candidates; `combine` scores each; merge + sort.
- **Validation/invariants**:
  - No new public MCP tool (FR-011).
  - Stays on `search_symbols`/`search_text`/`search_files` surfaces; never calls
    `bump_frecency` (FR-014).
  - Co-change floors identical to `search_files`
    (`FILE_LEVEL_CO_CHANGE_FLOOR = 2`).

## Entity 4: Orientation Doctrine Text (static content)

Canonical wording embedded in prompts + the compact map footer.

- **Shape**: 1-2 short lines, e.g.:
  - "The map orients; the tools prove."
  - "Absence from the map is not absence from the repo — confirm with
    `search_symbols` / `search_text`."
- **Placement**: `build_onboard_instructions`, `build_architecture_map_instructions`
  (prompts), and the `get_repo_map` compact footer (covers tool + resource).
- **Validation/invariants**:
  - Disclosure vocabulary consistent with existing "Completeness" /
    "truncated by result cap" envelopes (Principle III).
  - Pinned by snapshot/substring assertions in prompts + resources tests.

---

## State transitions

Only the temporal-readiness gate matters; everything else is stateless render:

```text
GitTemporalState:
  Pending|Computing  -> co-change unavailable -> footer/line/find degrade to
                        dependents-or-path-only (NO error, NO placeholder)
  Unavailable(r)     -> same degradation (non-git dir / git missing)
  Ready              -> co-change partners available -> full footer/line/boost
```

No other entity has lifecycle state; all are recomputed per request from the
authoritative in-memory indices.
