# Tasks: Dogfood Surface Hardening

**Feature**: `specs/018-dogfood-surface-hardening` | **Branch**: `018-dogfood-surface-hardening`
**Input**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/tool-behavior.md](./contracts/tool-behavior.md), [quickstart.md](./quickstart.md)

**Approach**: TDD (tests written to fail first, per Constitution VIII / FR-011). Four
code-disjoint, independently-shippable user stories in priority order. **US1 (P1) is the MVP**
and lands + verifies before US2. Constitution gates (III trust, IV determinism, V frecency,
VI embed, VII parity, VIII verification) enforced per-story and in Polish.

---

## Phase 1: Setup

- [ ] T001 Confirm baseline is green before changing anything: on branch `018-dogfood-surface-hardening`, run `cargo build` and `cargo test --all-targets -- --test-threads=1`. Record the current (pre-fix) behavior as before/after evidence: default `what_changed(uncommitted=true)` lists non-source data paths; `search_symbols` browse returns generic short names first; `get_repo_map(full)` has no root guard; big-response truncation omits the `symforge_retrieve` footer.

---

## Phase 2: Foundational (blocking prerequisites)

*The four stories are code-disjoint (US1 tools.rs/graph.rs · US2 search.rs/view.rs · US3 query.rs · US4 ccr.rs/mod.rs/tools.rs response builders) and share no foundational task. No blocking prerequisite work — proceed directly to user stories.*

---

## Phase 3: User Story 1 — Source-focused change/impact (P1) 🎯 MVP

**Goal**: `what_changed` (uncommitted) and `detect_impact` default to source-focused so
non-source data files (and their symbols) don't dominate; explicit opt-in still includes them;
committed-diff modes and explicit callers unchanged.

**Independent test**: `cargo test --all-targets -- --test-threads=1 what_changed detect_impact`
plus a live dirty-data-file check. Shippable with no dependency on US2–US4.

### Tests first (must fail on current code)

- [ ] T002 [US1] Add failing test: with a fixture repo whose only uncommitted change is a non-source data file (e.g. `data/x.json`), default `what_changed` in uncommitted mode returns zero code-change paths. File: colocated `#[cfg(test)]` in `src/protocol/tools.rs` (near existing `what_changed` tests) or `tests/what_changed_code_only_default.rs`.
- [ ] T003 [P] [US1] Add failing/guard test: the same call with explicit `code_only=false` still returns the data file (opt-in preserved, FR-003). File: same as T002.
- [ ] T004 [P] [US1] Add failing test: default `detect_impact` from a real source edit alongside dirty data files produces no data-file-derived symbols in the blast radius (SC-002). File: `src/protocol/tools.rs` impact tests or `tests/detect_impact_source_focus.rs`.
- [ ] T005 [P] [US1] Add guard test (must stay green): `what_changed` timestamp and `git_ref` modes are unchanged by the default flip (edge case: committed modes keep their meaning). File: same as T002.

### Implementation

- [ ] T006 [US1] Flip the `code_only` default to source-focused **only** in the uncommitted branch: change `params.0.code_only.unwrap_or(false)` → `.unwrap_or(true)` at `src/protocol/tools.rs:7035` (uncommitted mode). Leave the timestamp (`:6958`) and git_ref (`:7109`) sites at `false`. Keep the envelope disclosure honest (state the applied filter).
- [ ] T007 [US1] Make `detect_impact`'s changed-set source-focused by default: the impact input defaults `include_untracked=true` (`src/protocol/tools.rs:501/540-543`), pulling untracked data files into the seed. Filter the impact seed-set to source before the inbound walk (reuse `filter_paths_by_prefix_and_language(..., code_only=true)` at `tools.rs:724`, and/or skip non-programming-language symbols in `src/live_index/graph.rs compute_impact`). Preserve an explicit data-inclusion opt-in. Do **not** perform an admission-tier rewrite (deferred per research.md).

### Verify

- [ ] T008 [US1] Run `cargo test --all-targets -- --test-threads=1 what_changed detect_impact` — T002/T004 now pass, T003/T005 still pass. Confirm SC-001 (0 data paths) and SC-002 (no data-derived impact symbols) live.

**Checkpoint**: US1 complete and independently shippable as the MVP.

---

## Phase 4: User Story 2 — Browse-mode importance ranking (P2)

**Goal**: `search_symbols` with an empty query + scope filter (`kind`/`path_prefix`) ranks by
importance (reference count → kind → path → line), not path order; non-empty-query behavior
byte-identical; deterministic; frecency-neutral.

**Independent test**: `cargo test --all-targets -- --test-threads=1 search_symbols browse` + a
live browse call. Testable without the other stories.

### Tests first (must fail on current code)

- [ ] T009 [US2] Add failing test: a browse call (empty query + `kind`/`path_prefix`) over a fixture scope containing one heavily-referenced symbol and generic short names (`add`/`get`/`len`) ranks the heavily-referenced symbol ahead of the trivial names. File: `src/live_index/search.rs` tests or `tests/search_symbols_browse.rs`.
- [ ] T010 [P] [US2] Add determinism test (Constitution IV): the same browse query twice ⇒ identical ordering (total tie-break). File: same as T009.
- [ ] T011 [P] [US2] Add frecency-neutrality test (Constitution V): running the browse query does not bump frecency. File: same as T009.
- [ ] T012 [P] [US2] Add guard test (must stay green): a non-empty query returns the same results/order as before (FR-005). File: same as T009.

### Implementation

- [ ] T013 [US2] In `src/live_index/search.rs` (browse path around `:870`), detect browse intent (empty/whitespace query **and** a `kind` and/or `path_prefix` filter); skip the `!name_lower.contains(&query_lower)` short-circuit for that case and rank candidates by `(reference_count desc, kind_priority, path, line)` using the existing reverse-reference data (read-only w.r.t. frecency). Leave non-empty-query behavior byte-identical.
- [ ] T014 [US2] Apply the sibling guard in `src/live_index/view.rs:346` (same empty-query passthrough) so the browse-vs-name-match distinction is consistent across both call sites.

### Verify

- [ ] T015 [US2] Run `cargo test --all-targets -- --test-threads=1 search_symbols browse` — T009 passes; T010/T011/T012 pass. Confirm SC-003 live (no trivial name outranks a materially more-referenced symbol).

**Checkpoint**: US2 complete and independently verifiable.

---

## Phase 5: User Story 3 — Repo-map root guard (P3)

**Goal**: `get_repo_map` full detail never surfaces a path outside the workspace root;
in-root files unaffected.

**Independent test**: `cargo test --all-targets -- --test-threads=1 repo_map outline root` + a
live full-map check. Testable without the other stories.

### Tests first (must fail on current code)

- [ ] T016 [US3] Add failing test: an index whose file set includes an out-of-root / `..`-escaping path yields a `capture_repo_outline_view` (full) that omits it (SC-004). File: `src/live_index/query.rs` tests or `tests/repo_map_root_guard.rs`.
- [ ] T017 [P] [US3] Add guard test (must stay green): a clean in-root repo's full outline file count is unchanged vs. current behavior. File: same as T016.

### Implementation

- [ ] T018 [US3] Add a workspace-root containment guard inside `capture_repo_outline_view` (`src/live_index/query.rs:2388`): reuse the index's stored `indexed_root` and drop any file whose resolved path escapes it, reusing `normalize_lexically` (`tools.rs:76`) / `path_is_within_bound_project` (`tools.rs:108`) rather than new path math. Confirm the `indexed_root` accessor is reachable from `&self`; if not, sanitize in the `get_repo_map` full handler instead (research.md open choice).

### Verify

- [ ] T019 [US3] Run `cargo test --all-targets -- --test-threads=1 repo_map outline root` — T016 passes, T017 stays green. Confirm SC-004 live.

**Checkpoint**: US3 complete and independently verifiable.

---

## Phase 6: User Story 4 — CCR footer on truncation (P4)

**Goal**: big-response builders emit a `symforge_retrieve` footer when they truncate;
within-budget responses gain no footer; the hash resolves to the full payload.

**Independent test**: `cargo test --all-targets -- --test-threads=1 ccr footer truncat` + a live
tight-budget check. Testable without the other stories.

### Tests first (must fail on current code)

- [ ] T020 [US4] Add failing test: a big-response builder under a tight `max_tokens` truncates AND emits a `symforge_retrieve` footer whose hash resolves (via the CCR store) to the full pre-truncation payload (SC-005). File: `src/protocol/ccr.rs` tests or `tests/ccr_footer_on_truncation.rs`.
- [ ] T021 [P] [US4] Add guard test (must stay green): a within-budget response gains no footer (FR-008). File: same as T020.

### Implementation

- [ ] T022 [US4] Add a shared helper `enforce_token_budget_with_ccr(store, tool_name, result, max_tokens)` (lifting the logic at `src/protocol/mod.rs:705-709`): run the CCR decision on the complete payload before the hard cut and emit the retrieve footer only when truncation occurs.
- [ ] T023 [US4] Route the big-response builders through the helper instead of bare `format::enforce_token_budget`: start with the highest-value builders (`get_repo_map` full, the widest `search_symbols`/`search_text`, `explore`) at the relevant `src/protocol/tools.rs` sites (e.g. `:4041/:4233/:5413/:8082`). If routing every site cleanly is larger than expected, ship the highest-value builders first and document the residual (P4, narrowest-viable per research.md risk note).

### Verify

- [ ] T024 [US4] Run `cargo test --all-targets -- --test-threads=1 ccr footer truncat` — T020 passes, T021 stays green. Confirm SC-005 live (truncated `get_repo_map(full, max_tokens=300)` ends with a `symforge_retrieve` hash that fetches the full map). Document any residual builders not yet routed.

**Checkpoint**: US4 complete (or narrowed-with-residual documented).

---

## Phase 7: Polish & Cross-Cutting

- [ ] T025 Transport parity (Constitution VII): if any shared protocol formatter signature changed (most likely US4's response-builder/footer path), confirm stdio↔serve equivalence and add/settle a parity assertion. If no shared formatter signature changed, record that explicitly.
- [ ] T026 Embed isolation (Constitution VI): `cargo check --no-default-features --features embed` stays green — no server/network dep pulled into the touched query/format/git/graph paths.
- [ ] T027 Full verification gate (Constitution VIII): `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release` — all green with the new tests.
- [ ] T028 Update `CHANGELOG`/release notes accurately (release-please conventional `fix:` commits), one truthful claim per shipped story: US1 "what_changed/detect_impact default to source-focused"; US2 "search_symbols browse ranks by importance"; US3 "get_repo_map full guards against out-of-root paths"; US4 "big-response truncation offers a symforge_retrieve handle". Do NOT over-claim beyond what shipped (e.g. if US4 narrowed, say which builders).
- [ ] T029 Commit on the review branch `018-dogfood-surface-hardening` (do NOT push/merge without user approval per session policy); optionally run `/speckit-analyze` to cross-check spec/plan/tasks consistency before implement sign-off.

---

## Dependencies & Execution Order

- **US1 (T002–T008)** → land + verify **before** US2 (per request; MVP first). US2/US3/US4 are mutually independent (disjoint files) and may proceed in priority order after US1.
- Within each story: tests (several `[P]`) → implementation → verify.
- Polish (T025–T029) after the stories that shipped (or after US1 alone if shipping the MVP first).

## Parallel Opportunities

- US1 test authoring: T003, T004, T005 in parallel after T002 establishes the fixture.
- US2 test authoring: T010, T011, T012 in parallel after T009.
- US3 test authoring: T017 in parallel after T016.
- US4 test authoring: T021 in parallel after T020.
- Because US2/US3/US4 touch disjoint files, they can be implemented by separate agents in parallel once US1 has landed — with strict file ownership (US2 = search.rs/view.rs · US3 = query.rs · US4 = ccr.rs/mod.rs/tools.rs response builders).

## MVP Scope

**US1 alone** (T001–T008 + gate) is a complete, shippable MVP: it fixes the highest-impact,
highest-frequency defect (noisy change/impact on any repo with data files). US2–US4 are
separable quality/hardening improvements that follow in priority order.

## Independent Test Criteria

- **US1**: default `what_changed`(uncommitted) and `detect_impact` are source-focused; `code_only=false` restores inclusion; committed modes unregressed.
- **US2**: browse ranks by importance; deterministic; frecency-neutral; non-empty query unchanged.
- **US3**: full repo map excludes out-of-root paths; clean repo outline unchanged.
- **US4**: truncated big responses carry a resolving `symforge_retrieve` footer; within-budget responses do not.
