# Tasks: Selector & Concept-Ranking Fidelity

**Feature**: `specs/017-selector-ranking-fidelity` | **Branch**: `017-selector-ranking-fidelity`
**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [quickstart.md](./quickstart.md)

**Approach**: TDD (tests written to fail first, per user request). Two independent user stories:
US1 (P1) = `edit_plan` `Type::method` resolution; US2 (P2) = `explore` concept ranking. US1 lands and
verifies fully before US2 begins. Constitution gates (V frecency, IV determinism, III trust, VI embed,
VII parity, VIII verification) are enforced per-story and in Polish.

---

## Phase 1: Setup

- [ ] T001 Confirm baseline is green before changing anything: run `cargo build` and `cargo test --all-targets -- --test-threads=1` on branch `017-selector-ranking-fidelity`; record that the current `edit_plan("GitRepo::tracked_paths")` fails and the two explore anchor queries misrank (baseline evidence for the before/after proof).

---

## Phase 2: Foundational (blocking prerequisites)

*P1 and P2 are code-disjoint (edit_plan/selector vs explore scoring) and share no foundational task.
No blocking prerequisite work — proceed directly to user stories.*

---

## Phase 3: User Story 1 — Resolve `Type::method` (P1) 🎯 MVP

**Goal**: `edit_plan("Type::method")` resolves valid methods (unique and disambiguating cases), other
selector forms unchanged, nonexistent methods give a truthful not-found.

**Independent test**: `cargo test --all-targets -- --test-threads=1 edit_plan symbol_disambiguation`
plus live `edit_plan("GitRepo::tracked_paths")` after build. Shippable with no dependency on US2.

### Tests first (must fail on current code)

- [ ] T002 [US1] Add failing regression test: `Type::method` with a unique method resolves to the same symbol as the bare name — assert `edit_plan(index, temporal, "GitRepo::tracked_paths")` yields the `tracked_paths` hit in `src/git.rs`. File: `tests/edit_plan_symbol_line.rs` (fixture index with an `impl GitRepo { fn tracked_paths }`-shaped file, or reuse an existing fixture).
- [ ] T003 [P] [US1] Add failing test: `Type::` disambiguates a method name shared across types — two types each with `fn new`, assert `Type::new` resolves to the named type's `new` only. File: `tests/symbol_disambiguation.rs`.
- [ ] T004 [P] [US1] Add regression-guard tests (should PASS now, must still pass after): bare name, `file-path::symbol`, and plain file-path selectors resolve exactly as before. File: `tests/edit_plan_symbol_line.rs`.
- [ ] T005 [P] [US1] Add failing test: a `Type::nonexistent_method` returns a truthful not-found that names what was searched (no wrong hit). File: `tests/edit_plan_symbol_line.rs`.

### Implementation

- [ ] T006 [US1] Add a reusable containment helper in `src/live_index/disambiguation.rs`: given a candidate method `SymbolRecord` and a type name `X`, return true when the same file has an `impl`/`struct`/`enum`/`trait` symbol resolving to type `X` whose `line_range` encloses the candidate. Match against the impl's target-type token (handle `impl X` and `impl Trait for X` display forms), not the raw display string. Deterministic; reuses existing `SymbolRecord` ranges only.
- [ ] T007 [US1] Wire the type-name fallback into `plan_edit` in `src/protocol/edit_plan.rs`: when `split_path_qualified_target` yields `(X, Y)` and NO indexed file path matches `X`, iterate `index.all_files()` collecting symbols named `Y` (method/function kind) that pass the T006 containment check for type `X`; push them as symbol hits (single → Selected; multiple → existing multi-hit path). Leave the existing file-path interpretation and all other selector forms byte-identical.
- [ ] T008 [US1] Run `cargo test --all-targets -- --test-threads=1 edit_plan symbol_disambiguation` — T002/T003/T005 now pass, T004 still passes. Confirm the five anchor selectors (SC-001) resolve.

**Checkpoint**: US1 complete and independently verifiable. Can commit/land here as the MVP.

---

## Phase 4: User Story 2 — Concept-central `explore` ranking (P2)

**Goal**: concept queries surface conceptually-central symbols in top-N; no unrelated symbol dominates
at 1.00; exact-name queries still rank their target top; deterministic; frecency-neutral.

**Independent test**: `cargo test --all-targets -- --test-threads=1 explore` + the two live anchor queries.

### Research (superior approach, not blind tuning)

- [ ] T009 [US2] Dispatch the **tech-researcher** agent: survey best-practice concept/relevance ranking for symbol search (name-token overlap vs concept-proximity vs hybrid BM25-style fusion; how to blend a concept-map seed signal with lexical overlap; deterministic tie-breaking). Deliverable: a recommended, minimal rebalance approach grounded in the existing scorer, with the trade-offs — not a new retrieval backend (Constitution I).
- [ ] T010 [US2] Pin the exact scorer: `find_references(explore_symbol_reason)` and trace the explore capture view in `src/live_index/query.rs` / `src/live_index/search.rs` to the function that assigns the numeric score. Record the file:line of the weighting and how the concept-map seed terms/files reach it.

### Tests first (anchor assertions must fail on current code)

- [ ] T011 [US2] Add failing test: query "worktree routing hook registration in the daemon" — top-N includes `register_if_feature_enabled` AND `WorktreeAwareEditHook`. File: near the scorer (`src/live_index/query.rs` tests) or a `tests/explore_ranking.rs` integration test over a representative fixture index.
- [ ] T012 [P] [US2] Add failing test: query "watcher interact with analyze_file_impact" — the single top score is a watcher/impact-related symbol, not an unrelated one (assert the previously-spurious symbol class does not hold rank 1).
- [ ] T013 [P] [US2] Add guard test (must stay green): an exact-name query for a specific function ranks that function at/near the top (no over-correction away from legitimate strong name matches).
- [ ] T014 [P] [US2] Add determinism test (Constitution IV): identical query + index ⇒ identical ordering (stable sort / deterministic tie-break).
- [ ] T015 [P] [US2] Add frecency-neutrality test (Constitution V): running `explore` does not bump frecency signals.

### Implementation

- [ ] T016 [US2] Rebalance the explore scorer (from T009/T010) so concept-proximity (concept-map seed file/import/co-occurrence) contributes alongside name-token overlap, and a lone best-name match cannot crater everything else. Keep it read-only w.r.t. frecency (V), deterministic (IV), and preserve trust/truncation disclosure (III). Smallest principled change that passes T011–T013.
- [ ] T017 [US2] Run `cargo test --all-targets -- --test-threads=1 explore` — T011/T012 pass, T013/T014/T015 pass, and the existing `explore_result_view_filters_weak_trivial_symbols_and_doc_only_patterns` / `explore_result_view_keeps_trivial_symbol_when_strongly_contextualized` tests in `src/protocol/format.rs` still pass (no regression). If a principled rebalance can't satisfy both anchors without harming exact-name matches, ship the narrower improvement and document the residual (per research.md risk note).

**Checkpoint**: US2 complete and independently verifiable.

---

## Phase 5: Polish & Cross-Cutting

- [ ] T018 If any shared protocol formatter changed (P2 touches `explore_symbol_reason`/explore view in `format.rs`), confirm stdio↔serve parity (Constitution VII) — the change is in a shared formatter both transports call; add/settle a parity assertion if a formatter signature changed.
- [ ] T019 Embed isolation (Constitution VI): `cargo check --no-default-features --features embed` stays green — no server/network dep pulled into the edit_plan/query/disambiguation paths.
- [ ] T020 Full verification gate (Constitution VIII): `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release` — all green with the new tests.
- [ ] T021 Update `CHANGELOG`/release notes accurately (release-please conventional `fix:` commits): P1 "edit_plan resolves Type::method selectors"; P2 "explore ranking surfaces concept-central symbols" — and do NOT over-claim (the 8.13.7 changelog over-promised the P1 behavior; this one must be true).
- [ ] T022 Commit on the review branch (do NOT push/merge without user approval per session policy); run `/speckit-analyze` if desired to cross-check spec/plan/tasks consistency before implement sign-off.

---

## Dependencies & Execution Order

- **US1 (T002–T008)** → land + verify **before** **US2 (T009–T017)** (per request; also lets the crisp MVP ship first).
- Within US1: T002–T005 (tests) → T006 (helper) → T007 (wire-up) → T008 (verify). T003/T004/T005 are [P] (independent test files/cases).
- Within US2: T009 (research) + T010 (pin scorer) → T011–T015 (tests, several [P]) → T016 (rebalance) → T017 (verify).
- Polish (T018–T022) after both stories, or after US1 alone if shipping the MVP first.

## Parallel Opportunities

- US1 test authoring: T003, T004, T005 in parallel (distinct cases/files) after T002 establishes the fixture.
- US2 test authoring: T012, T013, T014, T015 in parallel after T011 establishes the fixture/harness.
- T009 (tech-researcher, read-only) can run in parallel with T010 (pin scorer).

## MVP Scope

**US1 alone** (T001–T008 + gate) is a complete, shippable MVP: it fixes the concrete, high-frequency
`Type::method` defect and corrects the over-claimed 8.13.7 changelog. US2 is a separable quality
improvement that follows.

## Independent Test Criteria

- **US1**: `edit_plan("GitRepo::tracked_paths")` (+ the 4 other anchors) resolve; disambiguation works;
  other selector forms unregressed; nonexistent method → truthful not-found.
- **US2**: both anchor queries surface their concept-central symbols in top-N with no unrelated 1.00;
  exact-name still tops; deterministic; frecency-neutral; existing explore-view tests green.
