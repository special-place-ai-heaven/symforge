# Phase 0 Research: Intelligence Pattern Ports (007)

**Date**: 2026-06-16 · **Branch**: `007-intelligence-pattern-ports`

This document resolves the technical unknowns for the four ports (impact footer,
orientation doctrine, ranked compact map, STEL find fusion + impact intent). All
findings are grounded in the current `E:\project\symforge` tree via SymForge MCP
(symbol/line anchors below are from the canonical tree, not the parallel
`symforge-review` checkout). Git temporal is live in this repo (Ready, 500
commits / 90d), so co-change paths are exercisable.

---

## R0. 004 dependency / sequencing

- **Decision**: Build the stdio-side patterns now; do NOT implement or touch 004
  serve/auth/ledger scope. Transport parity (Principle VII) is satisfied because
  every change lands in **shared protocol formatters** (`src/protocol/format.rs`,
  `prompts.rs`, edit handlers, STEL planner) that both stdio and serve route
  through — there is no transport-specific code in this feature.
- **Rationale**: Brief §6 Q5 default is "ship after 004 lands unless waived." The
  operator running this chain through implement is the waiver. The feature's value
  (footer, doctrine, ranking, find) is independent of the serve transport.
- **Alternatives rejected**: Block until 004 merges (rejected — operator waived;
  no shared dependency on serve code). Add serve-only behavior (rejected — would
  violate transport parity and scope).

## R1. Post-edit impact footer — data sources & wiring

- **Brief premise correction**: The brief implies
  `src/sidecar/handlers.rs::workflow_post_edit_impact_handler` (~L737) already
  computes "dependent count + co-change partners." It does NOT. That handler is a
  thin alias → `impact_handler` → `handle_edit_impact` (L891-1199), which computes
  a **symbol diff + type-scoped callers**, not a file dependent count or
  co-changes. The dependent count and co-change rendering actually live in
  `outline_text` (L451-719, dependents at L567-590 via `find_dependents_for_file`,
  co-changes at L663-671 via `git_temporal()`).
- **Decision**: The footer's two data sources are:
  1. **Dependents (always available, synchronous)** —
     `LiveIndex::capture_find_dependents_view(path).files.len()`
     (`src/live_index/query.rs:1654`). This matches the `find_dependents` tool's
     own count (distinct importing files), avoiding the per-reference
     double-count of `find_dependents_for_file(path).len()`.
  2. **Co-changes (conditional, async)** — `index.git_temporal()`
     (`src/live_index/store.rs:1181`, lock-free `Arc` snapshot), gate on
     `GitTemporalState::Ready`, look up `temporal.files.get(path)` →
     `GitFileHistory.co_changes: Vec<CoChangeEntry>` and take `entry.path`.
     `analyze_file_impact` (`src/protocol/tools.rs:4060-4131`, co-change at
     L4097-4127) is the canonical fetch+gate+format flow to mirror.
- **Footer grammar (Q2)**: `[impact: N dependents · cochanges: a, b, c]`. The
  `· cochanges: …` clause is present only when temporal is Ready and
  `co_changes` is non-empty; otherwise the suffix is `[impact: N dependents]`.
  Net-new formatter (no existing helper emits the single-line form) — add beside
  `co_changes_result_view` in `src/protocol/format.rs`.
- **Wiring (Q1 = all structural mutations)**: Add one helper
  `append_impact_footer(output: &mut String, …)` in `src/protocol/edit_tools.rs`
  mirroring `append_project_config_trust_suffix` (L105-110), and call it at the
  **seven inner-handler success tails**: `replace_symbol_body` (after L719),
  `insert_symbol` (after L909), `delete_symbol` (after L1089),
  `edit_within_symbol` (after L1387), and the `Ok` arms only of `batch_edit`
  (after L1502), `batch_rename` (after L1578), `batch_insert` (after L1686).
  Append **before** `complete_mutation_replay` so the footer is persisted in the
  idempotency replay cache (consistent first-apply vs replay output). The unified
  `symforge_edit` apply path inherits the footer for free via
  `dispatch_tool_for_tests` → inner handler → `tool_body`
  (`src/protocol/tools.rs::symforge_edit_stel_handler` L8258).
- **Success-only (scenario 4)**: Single-symbol tools early-return on every error
  before the tail; batch tools split `Ok`/`Err` arms (append in `Ok` only). The
  `symforge_edit` `AlreadyApplied` branch (`format_already_applied_body`) does NOT
  invoke an inner handler → out of scope (no fresh mutation = no fresh blast
  radius); document as an explicit non-target.
- **Classifier hazard**: The three `_tool` wrappers re-run `classify_edit_output`
  (substring matching on the body). Footer wording MUST avoid sentinel phrases
  (`Error`, `unavailable`, `byte range`, `Write failed`, `[DRY RUN]`,
  `Ambiguous:`, `Symbol not found:`) to avoid success→failure misclassification.
- **Alternatives rejected**: Lifting from `handle_edit_impact` (rejected — wrong
  data: symbol diff, not file dependents). A shared CallToolResult-layer hook
  (rejected — `statused_edit_tool_result` only wraps 3 of 7 tools and is the wrong
  layer for text). Forking `outline_text` logic (rejected — reuse
  `find_dependents_for_file`/`git_temporal` directly per the no-fork rule).

## R2. Orientation doctrine — surfaces & wording

- **Two surfaces**:
  1. **Prompt text (fully under our control)** — `src/protocol/prompts.rs`:
     `build_onboard_instructions` (L345-381) and
     `build_architecture_map_instructions` (L268-300) are `format!` bodies;
     insert doctrine lines near their "Read the repo map resource" step. (Optional
     dedicated `PromptMessage` via `onboard_prompt` L146-170 /
     `architecture_map_prompt` L86-110.)
  2. **Repo-map resource body (indirect)** — `symforge://repo/map` →
     `render_resource_text` RepoMap arm → `get_repo_map(detail=compact)` →
     `repo_map_text` + `compact_next_step_hint`. The single highest-leverage hook
     is the **compact arm footer** in `src/protocol/tools.rs` (~L3526-3534,
     `format!("{result}{hint}")`), which covers BOTH the `get_repo_map` tool and
     the resource in one edit. NOTE: editing `src/protocol/resources.rs` alone
     does NOT change the map body — it only routes.
- **Decision**: Add the doctrine as 1-2 short lines: "the map orients, tools
  prove" and "absence from the map does not mean absence from the repo — confirm
  with search_symbols/search_text." Place the map-body doctrine in the
  `get_repo_map` compact footer (after the budgeted body) to avoid the 4000-byte
  `build_with_budget` truncation that could silently drop real map content.
- **Wording consistency (Principle III)**: There is currently NO "map orients /
  absence != absence" wording anywhere — this is net-new doctrine. Phrase any
  truncation/completeness disclosure using the established vocabulary:
  `format_context_envelope` "Completeness" (`src/sidecar/handlers.rs:198-221`) and
  `search_completeness_label` "truncated by result cap" (`tools.rs:878-897`).
- **Pinning**: No test currently pins the prompt body text; add positive
  assertions in `prompts.rs::tests` and extend
  `resources.rs::test_read_static_repo_map_resource` (L553-564) so a silent
  doctrine regression is caught.

## R3. Ranked compact map — the "compact" collision (key decision)

- **Finding**: The brief §8 cites `capture_repo_outline_view`
  (`src/live_index/query.rs:2283-2304`, alphabetical `files.sort_by` at L2295) as
  "the compact repo map." That function actually feeds the **`detail=full`** view
  (`format::repo_outline_view`, per-file lines) AND the **`tree`** view
  (`file_tree`). The **literal `detail=compact`** (the default agents read first)
  is `src/sidecar/handlers.rs::repo_map_text` (L1499+), which renders
  **directory-level aggregates + a "Key entry points" file list**, sorted
  alphabetically — it has no per-file `path lang N symbols` lines.
- **Decision (honors §5 "rank detail=compact" AND Q3 "preserve full/tree
  exactly")**: Apply importance ranking + the `(→N)` annotation to the
  **`detail=compact`** path's file-bearing section (the "Key entry points" list in
  `repo_map_text`), ranking by `(dependent_count desc, churn_score desc, path asc)`
  with `path` as the deterministic stable tie-break (FR-017). Leave
  `capture_repo_outline_view`'s sort, `repo_outline_view` (full), and
  `file_tree_view` (tree) **byte-unchanged** (FR-009). The brief's L2295 anchor is
  therefore explicitly NOT the edit site for this feature; the edit site is the
  compact `repo_map_text` entry-point ordering/annotation.
- **Ranking inputs (cheap, no second index — Principle I)**:
  - Dependent count per file: `find_dependents_for_file(path)` grouped by file
    (or `capture_find_dependents_view(path).files.len()`). For the compact
    entry-point set (small, bounded), per-file dependent lookup is acceptable;
    avoid O(files²) by ranking only the candidate entry-point set, not every file.
  - Churn: `git_temporal().files.get(path).churn_score` (rank-normalized 0..1) or
    the precomputed `GitTemporalStats.hotspots` (`git_temporal.rs:225-238`,
    cheaper, bounded by `HOTSPOT_CAP`).
- **`(→N)` display (Q3/FR-008)**: annotate a file with `path (→N)` when its
  distinct dependent count `N >= 2`.
- **Alternatives rejected**: Change the L2295 sort in `capture_repo_outline_view`
  (rejected — would reorder `full` and `tree`, violating Q3/FR-009 since they
  share that function). New `detail=ranked` mode (rejected — Q3 default is to
  change compact ordering, not add a mode). Per-file `(→N)` on the full view
  (rejected — that's the `full` mode, must stay unchanged).

## R4. STEL find fusion — plan-only planner vs real ranking

- **Two layers**:
  1. **STEL planner (`src/stel/planner.rs`)** is **plan-only**: `route_find`
     (L372-395) does a binary "symbol"-substring split → one `PlannedStep`; it
     never merges or ranks symbol vs path results. `build_plan` (L23-29) emits
     ordered tool-call steps, it does not execute searches.
  2. **Real relevance ranking lives in `src/live_index`.** Path ranking is already
     a clean weighted fusion: `rank_signals::combine` (L324-330) over
     `PathMatchSignal` (multi-token path tiering) + `CoChangeSignal` (gated
     co-change boost), assembled by `query::search_files_rank_score` (L837-855)
     using neighbors from `tools.rs::search_files_coupling_neighbors` (L1092-1291).
     Symbol ranking (`search::search_symbols_with_options` L807-925) is a separate,
     **single-term, name-tier** sort with NO co-change and NO multi-word fusion.
     There is currently NO path that merges symbol + path hits into one ranked
     list. (The brief's "caller-weighted search.rs" ranking is not present in
     symbol search; the reusable relevance machinery is `rank_signals`.)
- **Decision (no new tool — Q4/FR-011)**: Implement fusion inside the existing
  find intent on the **search_* surfaces** (frecency-safe): tokenize the
  multi-word query, run the symbol matcher and the path/file matcher, score each
  candidate via `rank_signals::combine` (reuse `RankCtx.tokens` for multi-word and
  `co_change_*` fields for the boost, sourced from `search_files_coupling_neighbors`),
  and merge into one ranked list. Keep `FILE_LEVEL_CO_CHANGE_FLOOR = 2` and
  `CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR = basename` identical to `search_files` so
  find co-change behaves consistently.
- **Frecency invariant (Principle V / R6)**: Fusion MUST stay on
  `search_symbols`/`search_text`/`search_files`, which provably never call
  `bump_frecency`. It MUST NOT reach for `get_symbol`/`get_file_context` to obtain
  ranking inputs (those DO bump frecency).
- **Alternatives rejected**: Add a 4th compact/public tool (rejected — Q4).
  Merge results inside the planner (rejected — planner is plan-only; fake-merged
  results would violate determinism/trust). Pull co-change from `weak_co_changes`
  (rejected — advisory noise; use gated strong evidence only).

## R5. Impact intent + edit_plan co-change line

- **Impact intent (FR-012)**: Chain dependents + co-change into one envelope using
  the same `find_dependents_for_file` + `git_temporal` sources as R1 (the
  `analyze_file_impact` path already composes both; the impact intent surfaces them
  together).
- **edit_plan co-change (FR-013)**: `src/protocol/edit_plan.rs::plan_edit`
  (L22-125) takes `index: &LiveIndex`. CORRECTION (confirmed at implementation):
  `git_temporal()` lives on the **shared index handle**, not on `&LiveIndex`, so
  `plan_edit` cannot reach temporal through its `index` param — it required a
  signature change to thread the `GitTemporalIndex` snapshot in (the `edit_plan`
  tool handler holds the handle and passes `self.index.git_temporal()`), mirroring
  the impact-footer `edit_impact_summary(index, temporal, path)` shape. The impact
  intent likewise routes through STEL `route_impact` (a single `find_dependents`
  step) and was augmented in `symforge_stel_handler` to append co-changes (reusing
  `co_changes_result_view`), not via `plan_edit`. Insert a single
  `Co-change partners: a, b, c` line in the
  symbol branch after the `References:` block (after L59), gated on
  `state == Ready` AND non-empty `co_changes`. **Graceful omission (scenario)**:
  when temporal is not Ready or `co_changes` is empty, push NOTHING (no
  placeholder) — unlike `analyze_file_impact`, `plan_edit` stays terse. Existing
  `tests/edit_plan_symbol_line.rs` builds a LiveIndex with Pending/Unavailable
  temporal, so clean omission is required to keep those assertions valid (and is a
  ready-made guard for the omission criterion).

## R6. Frecency invariant (binding gate)

- **Verified by index**: `bump_frecency` (`src/protocol/mod.rs:197-201`) is the
  single read-side frecency mutator; `find_references` shows exactly 9 call sites,
  100% inside the four commitment tools (`get_symbol`, `get_file_context`,
  `get_symbol_context`, `get_file_content`). `search_files`, `search_text`,
  `search_symbols`, `get_repo_map`, `find_references`, `find_dependents`,
  `analyze_file_impact` call it ZERO times.
- **Decision**: New discovery/find/map/impact code stays neutral by simply NOT
  calling `bump_frecency`. Because `frecency::bump` is infallible/silent, the ONLY
  guard against an accidental bump is a test. Mirror the
  `tests/frecency_ranking.rs` `*_does_not_bump` trio (L397-474): with
  `FlagGuard::on()` (collection enabled), call the new path, assert
  `!fx.db_path().exists()`. `search_files` `rank_by=frecency` READS frecency but
  must not WRITE it (pinned by `search_files_frecency_rank_does_not_create_db_when_empty`).

## R7. Embed isolation (Principle VI / G-045)

- **Decision**: All edits land in `src/protocol/*`, `src/live_index/*`,
  `src/stel/*`, `src/sidecar/*` reusing existing in-crate helpers (`rank_signals`,
  `git_temporal`, `find_dependents_for_file`). No new crates, no `rusqlite` for
  queries, no server/network deps. Verify with
  `cargo check --no-default-features --features embed` in the gate.

---

## Decision summary

| # | Decision | Rationale | Rejected |
|---|----------|-----------|----------|
| R0 | Build stdio patterns now; no 004 serve/auth | Operator waiver; value is transport-independent; shared formatters give parity | Block on 004; serve-only behavior |
| R1 | Footer = `capture_find_dependents_view.len()` + `git_temporal.co_changes`; helper at 7 success tails before replay | Correct data sources; `symforge_edit` inherits via dispatch | Lift from `handle_edit_impact` (wrong data); CallToolResult layer |
| R2 | Doctrine in 2 prompt builders + `get_repo_map` compact footer | Covers tool + resource in one place; avoids budget truncation | Edit `resources.rs` body (no effect) |
| R3 | Rank the literal `detail=compact` entry points; leave full/tree byte-unchanged | Honors §5 (rank compact) AND Q3 (preserve full/tree); brief's L2295 anchor feeds full/tree | Change L2295 sort; add `ranked` mode |
| R4 | Fuse in find intent via `rank_signals::combine` on search_* surfaces | No new tool; reuses calibrated path+co-change ranking; frecency-safe | 4th tool; merge in planner; weak co-changes |
| R5 | Impact intent one envelope; edit_plan co-change line w/ silent omission | Reuses R1 sources; `plan_edit` already has index | Print loading/unavailable in plan |
| R6 | Stay off `bump_frecency`; add `*_does_not_bump` tests | Invariant verified; only a test guards it | Trust review only |
| R7 | In-crate reuse only; verify embed build | Keeps G-045 isolation | New deps |
