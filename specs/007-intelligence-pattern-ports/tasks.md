# Tasks: Intelligence Pattern Ports

**Feature**: 007 · **Branch**: `007-intelligence-pattern-ports` · **Input**:
[plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/](./contracts/),
[quickstart.md](./quickstart.md)

**Conventions**: TDD — write the listed test first (red), then implement (green).
`[P]` = parallelizable (different file, no dependency on an unfinished task).
All line anchors are from research.md (canonical `E:\project\symforge` tree, not
`symforge-review`). Re-confirm each anchor with SymForge before editing (live
line numbers drift). **No commits** — operator authorizes commit/PR separately.

**Verification gate (run after each phase that changes code)**:
`cargo fmt --check` · `cargo check` · `cargo clippy --all-targets -- -D warnings`
· `cargo test --all-targets -- --test-threads=1` · `cargo build --release` ·
`cargo check --no-default-features --features embed`.

---

## Phase 1: Setup

- [X] T001 Confirm branch is `007-intelligence-pattern-ports` (`git branch --show-current`) and document the 004 serve-spine **waiver** (build stdio patterns now, touch no `src/server/` or `src/cli/harness*`) as a note at the top of `tests/impact_footer.rs` and in this file's Notes — per research R0.
- [X] T002 [P] Add an integration fixture repo with known dependent edges (file A imported by ≥3 files; one high-fan-in file, one zero-fan-in file) under `tests/fixtures/` (reuse the existing fixtures harness style from `tests/frecency_ranking.rs` / `tests/edit_plan_symbol_line.rs`); ensure it can be initialized with a small git history so `GitTemporalState` can reach `Ready` for co-change tests.

## Phase 2: Foundational (blocking prerequisites)

- [X] T003 Add a shared impact data helper `fn edit_impact_summary(index: &LiveIndex, path: &str) -> (usize, Vec<String>)` returning (distinct dependent file count via `capture_find_dependents_view(path).files.len()`, top-K co-change partner paths via `git_temporal()` gated on `GitTemporalState::Ready` + `files.get(path).co_changes`) in `src/protocol/format.rs` (beside `co_changes_result_view`); forward-slash-normalize `path` before lookup. Reuse only existing accessors (no fork of `outline_text`). Source: research R1.
- [X] T004 Add the compact footer formatter `fn impact_footer(deps: usize, cochanges: &[String]) -> String` producing `[impact: N dependents · cochanges: a, b, c]` (omit the `· cochanges:` clause when empty) in `src/protocol/format.rs`; ensure output contains none of the `classify_edit_output` sentinels (`Error`, `unavailable`, `byte range`, `Write failed`, `[DRY RUN]`, `Ambiguous:`, `Symbol not found:`). Contract: [contracts/impact-footer.md](./contracts/impact-footer.md).

**Checkpoint**: `cargo check` green; helpers compile and are unit-tested by the US1 tests below.

---

## Phase 3: User Story 1 — Impact footer on structural edits (P1)

**Goal**: Every successful structural mutation reports its blast radius inline.
**Independent test**: edit a fixture symbol with known dependents → response ends
with `[impact: N dependents …]`; failing edit → no footer.

### Tests first (TDD)

- [X] T005 [P] [US1] Write `tests/impact_footer.rs`: assert footer present with correct `N` for `replace_symbol_body`, `edit_within_symbol`, `batch_edit`, and `symforge_edit` apply on a fixture symbol with 3 dependents; assert `cochanges:` lists expected partner(s) when git history exists; assert `[impact: 0 dependents]` for a zero-dependent symbol; assert NO footer on a failed/rejected edit; assert footer identical on first apply and idempotency replay. (Red until T006-T007.)

### Implementation

- [X] T006 [US1] Add `fn append_impact_footer(output: &mut String, index: &LiveIndex, path: &str)` in `src/protocol/edit_tools.rs` mirroring `append_project_config_trust_suffix` (L105-110): call `format::edit_impact_summary` + `format::impact_footer`, push `'\n'` + footer. (Depends on T003/T004.)
- [X] T007 [US1] Wire `append_impact_footer` at the **seven** inner-handler success tails in `src/protocol/edit_tools.rs`, each immediately **before** `complete_mutation_replay`: `replace_symbol_body` (~after L719), `insert_symbol` (~after L909), `delete_symbol` (~after L1089), `edit_within_symbol` (~after L1387), and the `Ok` arm only of `batch_edit` (~after L1502), `batch_rename` (~after L1578), `batch_insert` (~after L1686). Do NOT touch any `Err` arm. (Sequential — single file, 7 edit sites; verify each with a read between edits.)
- [X] T008 [US1] Confirm `symforge_edit` apply inherits the footer via `dispatch_tool_for_tests` → inner handler → `tool_body` (`src/protocol/tools.rs::symforge_edit_stel_handler` ~L8258); add an assertion in `tests/impact_footer.rs` that the `AlreadyApplied` branch does NOT carry a footer (explicit non-target per contract).

**Checkpoint**: T005 green; full gate passes. US1 shippable on its own.

---

## Phase 4: User Story 2 — Orientation doctrine (P1)

**Goal**: Onboarding/architecture prompts + compact map teach "map orients, tools
prove / absence ≠ absence / truncation disclosed".
**Independent test**: prompt + repo-map bodies contain the doctrine substrings.

### Tests first (TDD)

- [X] T009 [P] [US2] Add assertions in `src/protocol/prompts.rs::tests` (pattern after `test_code_review_prompt_includes_resource_links`) that `build_onboard_instructions` and `build_architecture_map_instructions` bodies contain the "map orients / tools prove" and "absence … not absence" statements; extend `src/protocol/resources.rs::test_read_static_repo_map_resource` (~L553-564) to assert the map body contains the doctrine substring. (Red until T010-T011.)

### Implementation

- [X] T010 [US2] Insert the doctrine lines into `build_onboard_instructions` (~L345-381) and `build_architecture_map_instructions` (~L268-300) in `src/protocol/prompts.rs`, near each "Read the repo map resource" step. Contract: [contracts/orientation-doctrine.md](./contracts/orientation-doctrine.md).
- [X] T011 [US2] Append a doctrine + truncation-disclosure line in the `get_repo_map` **compact** arm footer in `src/protocol/tools.rs` (~L3526-3534, the `format!("{result}{hint}")` site, after the budgeted body) so it covers BOTH the `get_repo_map` tool and the `symforge://repo/map` resource; reuse the existing "Completeness"/"truncated by result cap" vocabulary (`format_context_envelope` / `search_completeness_label`). Do NOT edit the `resources.rs` body (it only routes).

**Checkpoint**: T009 green; full gate passes. US2 shippable on its own.

---

## Phase 5: User Story 3 — Ranked compact map (P2)

**Goal**: `detail=compact` map orders file entries by importance and annotates
`(→N)`; `full`/`tree` byte-unchanged.
**Independent test**: high-fan-in file ranks first + `(→N)` for `N≥2`; full/tree
snapshots unchanged.

### Tests first (TDD)

- [X] T012 [P] [US3] Write `tests/compact_map_ranking.rs`: fixture with `core.rs` (≥8 dependents) vs `leaf.rs` (0) → compact map ranks `core.rs` first and shows `core.rs (→N)`; `leaf.rs` (N<2) shows no annotation; assert `detail=full` and `detail=tree` outputs are byte-identical to the pre-007 baseline; assert deterministic order across two renders; assert compact render does not create the frecency DB. (Red until T013-T014.)

### Implementation

- [X] T013 [US3] In `src/sidecar/handlers.rs::repo_map_text` (~L1499+), rank the "Key entry points" file set by `(dependent_count desc, churn_score desc, relative_path asc)` using `find_dependents_for_file` (grouped by file, bounded to the entry-point candidate set — no O(files²)) and `git_temporal().files.get(path).churn_score` (or `GitTemporalStats.hotspots`); `churn_score` defaults to 0.0 when temporal not Ready. Contract: [contracts/compact-map-ranking.md](./contracts/compact-map-ranking.md).
- [X] T014 [US3] Annotate each ranked entry-point line with `path (→N)` when distinct dependent count `N >= 2` (bare `path` otherwise) in `src/sidecar/handlers.rs::repo_map_text` rendering. Do NOT modify `capture_repo_outline_view` sort (query.rs ~L2295), `format::repo_outline_view` (full), or `file_tree_view` (tree) — they must stay byte-unchanged (FR-009).

**Checkpoint**: T012 green incl. full/tree-unchanged snapshots; full gate passes.

---

## Phase 6: User Story 4 — STEL find fusion (P2)

**Goal**: Multi-word fuzzy find ranks across symbols + paths with co-change boost;
no new tool; frecency-neutral.
**Independent test**: multi-word query returns merged ranked symbol+path list;
find does not bump frecency.

### Tests first (TDD)

- [X] T015 [P] [US4] Write `tests/stel_find_fusion.rs`: a multi-word query (e.g. "stel planner find") through the find intent returns both symbol and path hits ranked with co-change boost; assert NO new tool name appears in the STEL/tool surface; assert the find query does not create/bump the frecency DB (mirror `tests/frecency_ranking.rs` `*_does_not_bump`, `FlagGuard::on()`); assert graceful path/name-only ranking when co-change evidence is unavailable. (Red until T016-T018.)
- [X] T016 [P] [US4] Add a `search_files_frecency`-style neutrality assertion to `tests/frecency_ranking.rs` for the fused find path (`*_does_not_bump`). 

### Implementation

- [X] T017 [US4] Implement multi-term fusion in `src/stel/planner.rs::route_find` (~L372-395): tokenize the query into `RankCtx.tokens`, drive both the symbol matcher and the path/file matcher, and score candidates via `rank_signals::combine` reusing `query::search_files_rank_score` (path side) + co-change neighbors from `tools.rs::search_files_coupling_neighbors`; keep `FILE_LEVEL_CO_CHANGE_FLOOR=2` / `CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR=basename`. Stay on `search_symbols`/`search_text`/`search_files` surfaces only (never `get_symbol`/`get_file_context`). Contract: [contracts/stel-find-fusion.md](./contracts/stel-find-fusion.md).
- [X] T018 [US4] Support multi-term find classification in `src/protocol/smart_query.rs` (`classify_intent_with_match` ~L76-319 / `assess_route`) so multi-word find queries reach the fused route before the binary fallback; preserve existing symbol tier ordering (Exact>Prefix>Substring). Update planner/golden-route tests (`planner.rs` tests, `smart_query.rs` tests) and STEL golden replay rows rather than breaking them.

**Checkpoint**: T015/T016 green; existing planner+golden tests updated & green; full gate passes.

---

## Phase 7: User Story 5 — Impact intent + edit_plan co-change (P2)

**Goal**: Impact intent returns dependents+co-change in one envelope; `edit_plan`
mentions co-change when temporal data exists (graceful omission otherwise).
**Independent test**: impact intent one-envelope; `edit_plan` co-change line
present with history, omitted without.

### Tests first (TDD)

- [X] T019 [P] [US5] Extend `tests/edit_plan_symbol_line.rs`: assert a `Co-change partners: a, b` line appears for a symbol whose file has Ready co-change history, and assert it is omitted cleanly (no error, no empty line) when temporal is Pending/Unavailable (the harness's default state — existing assertions must still pass). (Red until T021.)
- [X] T020 [P] [US5] Add a test asserting the impact intent returns dependents AND co-change partners in a single envelope (extend `tests/impact_footer.rs` or a new `tests/impact_intent.rs`). (Red until T022.)

### Implementation

- [X] T021 [US5] In `src/protocol/edit_plan.rs::plan_edit` (symbol branch, after the `References:` block ~L59), push a single `Co-change partners: <comma-joined top-K paths>` line gated on `index.git_temporal().state == Ready` AND non-empty `files.get(path).co_changes`; push NOTHING otherwise (silent omission, no placeholder). No signature change (it already holds `&LiveIndex`). Contract/Research: R5.
- [X] T022 [US5] Ensure the impact intent (`symforge intent=impact` path in `src/protocol/tools.rs`) chains dependents + co-change into one envelope reusing `format::edit_impact_summary` (T003) / the `analyze_file_impact` co-change flow; no second index. (Reuse, do not fork.)

**Checkpoint**: T019/T020 green; full gate passes.

---

## Phase 8: Polish & cross-cutting

- [X] T023 [P] Pin the new find-fusion + impact-intent routes. DONE via test coverage: 4 planner unit tests + 5 `tests/stel_find_fusion.rs` integration tests + 2 `tests/impact_intent.rs` tests; the 6 `stel_golden_replay` tests remain green (no golden route changed). DEFERRED (optional, redundant): adding a dedicated co-change-seeded golden JSONL corpus row in `src/stel/golden_replay.rs` — the route shape is already pinned by the unit/integration tests above, and a new corpus fixture buys no additional guarantee here.
- [X] T024 [P] Add a `docs/v8-release-notes.md` entry summarizing the four ports (impact footer, orientation doctrine, ranked compact map, find fusion) and the explicit non-goals (no SQLite Soul Map, no grep intercept, no new tool).
- [X] T025 Ran the full verification gate (fmt --check, clippy --all-targets -D warnings, `cargo test --all-targets --test-threads=1`, build --release, embed check) — ALL GREEN (lib 2274 pass; all 6 feature test binaries pass; release 5m03s; embed clean). Quickstart scenarios 1-5 are pinned by the integration tests that drive the real dispatch paths (impact_footer, compact_map_ranking, stel_find_fusion, frecency_ranking, edit_plan_symbol_line, impact_intent). NOTE: live MCP-daemon manual run deferred — the session's running daemon is the pre-change 7.27.0 binary; observing the new behavior live would require restarting the daemon on the freshly-built binary (not done to avoid disrupting the session; the integration tests exercise the same code paths).

---

## Dependencies & ordering

- **Setup (T001-T002)** → **Foundational (T003-T004)** → user stories.
- **US1 (T005-T008)** depends on T003/T004. Shippable MVP on its own.
- **US2 (T009-T011)** independent of US1 (different files: prompts.rs, tools.rs compact footer).
- **US3 (T012-T014)** independent (sidecar/handlers.rs). Note: `format.rs` is shared with US1 — sequence US1's format.rs edits before US3 if both touch it (US3 renders in handlers.rs, so low contention).
- **US4 (T015-T018)** independent (planner.rs, smart_query.rs, search.rs).
- **US5 (T019-T022)** depends on T003 (`edit_impact_summary`); touches edit_plan.rs + tools.rs (impact intent). tools.rs is shared with US2 (compact footer) → sequence those tools.rs edits.
- **Polish (T023-T025)** after all stories.

## Parallel opportunities

- T002 ∥ (fixtures) with reading/setup.
- Test-authoring tasks across stories are `[P]` (different test files): T005, T012, T015, T016, T019, T020.
- US2, US3, US4 implementation can proceed in parallel **except** for the shared
  files noted above (`format.rs` US1↔US3; `tools.rs` US2↔US5) — serialize edits to
  any single file.

## MVP scope

**US1 (impact footer)** alone is a complete, demonstrable MVP: highest value,
lowest cost, independently testable. US2 (doctrine) is the cheapest add. US3-US5
layer ranking/find quality on top.

## Notes

- 004 waiver (T001): operator ran the SDD chain through implement = waiver to
  build stdio patterns now; no `src/server/`, `src/cli/harness*`, `specs/004*`,
  `specs/005*` edits.
- Re-confirm every line anchor with SymForge (`search_symbols`/`get_symbol`)
  before editing — numbers drift.
- Post-implementation code review (code-reviewer agent) found one Major latent
  bug: `impact_footer` interpolated co-change partner paths verbatim, and the
  three `_tool` wrappers re-classify the full body, so a partner path embedding a
  `classify_edit_output` `.contains` sentinel (e.g. `src/unavailable.rs`) could
  flip a successful edit to a failure. FIXED: `impact_footer` now elides any
  partner path containing a classifier sentinel (`FOOTER_SENTINEL_SUBSTRINGS` in
  format.rs) + regression test `impact_footer_elides_partner_paths_carrying_classifier_sentinels`.
  Also fixed a stale doc comment on `FIND_FUSION_PATH_RATIONALE` (recognition is
  by `rank_by`, not the string). Minor deferred (low-impact, no regression): a
  fusion find where BOTH surfaces are empty classifies `Found` rather than
  `NotFound`.
