# Feature Specification: Selector & Concept-Ranking Fidelity

**Feature Branch**: `017-selector-ranking-fidelity`

**Created**: 2026-07-09

**Status**: Draft

**Input**: User description: two live-dogfood defects on the 8.13.8 build — `edit_plan` cannot resolve `Type::method` selectors, and `explore` ranking over-weights symbol-name token overlap and misses conceptually-central code.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Resolve a method addressed as `Type::method` (Priority: P1)

An agent (or human) wants to plan an edit to a method it knows by its Rust-canonical name, e.g. `GitRepo::tracked_paths`. It calls `edit_plan` with that `Type::method` target. Today the tool answers "not found" even though the method exists, forcing the caller to retry with a bare name or a `file::symbol` selector — a wasted round-trip that erodes trust in the tool.

**Why this priority**: This is the concrete, high-frequency defect. `Type::method` is the form an LLM reaches for first when it knows the owning type. The 8.13.7 changelog already claims this is supported, so the gap is also a correctness-of-claim issue. It is well-characterized and independently shippable.

**Independent Test**: Call `edit_plan("GitRepo::tracked_paths")` (and the other `Type::method` examples) and confirm it resolves to the same method the bare-name and `file::symbol` selectors resolve to. Fully testable on its own; delivers value with no dependency on the ranking work.

**Acceptance Scenarios**:

1. **Given** a repository containing `impl GitRepo { fn tracked_paths(...) }`, **When** a caller runs `edit_plan("GitRepo::tracked_paths")`, **Then** the plan resolves to `tracked_paths` in that file — the same result as `edit_plan("tracked_paths")` and `edit_plan("<file>::tracked_paths")`.
2. **Given** a method name that exists on more than one type (e.g. several `new` constructors), **When** a caller runs `edit_plan("WorktreeCache::new")`, **Then** the plan resolves specifically to the `new` on `WorktreeCache`, not to another type's `new` and not to "not found".
3. **Given** the existing selector forms (bare name, `file::symbol`, plain file path), **When** any of them is used after this change, **Then** each resolves exactly as it did before (no regression).
4. **Given** a `Type::method` where the method genuinely does not exist on that type, **When** `edit_plan("GitRepo::does_not_exist")` is run, **Then** the tool reports a clear not-found result that names what was searched (a truthful negative, not a silent wrong hit).

---

### User Story 2 - Discover conceptually-central code that doesn't share query words (Priority: P2)

An agent asks `explore` a concept question — e.g. "worktree routing hook registration in the daemon" — to find the code that implements that concept. Today `explore` awards a single 1.00 "strong match" to whatever symbol's *name* best overlaps the query tokens, then scores everything else near zero, and omits the symbols that actually implement the concept but happen not to share query words (e.g. `register_if_feature_enabled`, `WorktreeAwareEditHook`, `edit_hooks::register`). In the worst case the 1.00 hit is unrelated to the concept entirely.

**Why this priority**: Ranking quality is softer and more exploratory than US1, but it is the difference between `explore` being a concept-discovery tool and a symbol-name substring matcher. It is independently shippable after (or alongside) US1.

**Independent Test**: Run the two example concept queries and confirm the conceptually-central symbols appear in the top results and that no unrelated symbol dominates the ranking at 1.00. Testable without US1.

**Acceptance Scenarios**:

1. **Given** the query "worktree routing hook registration in the daemon", **When** `explore` runs, **Then** the top results include the symbols that implement hook registration (the registration function and the worktree-aware hook type), not only the one symbol whose name shares the most query tokens.
2. **Given** the query "watcher interact with analyze_file_impact", **When** `explore` runs, **Then** the impact-handling code central to that interaction appears in the top results, and a symbol unrelated to both the watcher and impact analysis does NOT occupy the single top score.
3. **Given** any concept query, **When** results are ranked, **Then** the ordering is deterministic for identical repository state (same query + same index ⇒ same order), and the result set continues to disclose that it is ranked/truncated (trust metadata preserved).
4. **Given** `explore` is a discovery tool, **When** it ranks results, **Then** it does not bump frecency as a side effect (discovery stays frecency-neutral).

---

### Edge Cases

- `Type::method` where the leading segment collides with a file/module name (e.g. a type named the same as a directory): resolution MUST prefer the correct symbol and not silently pick an unrelated file.
- `Type::method` where the method name is unique across the repo: MUST resolve identically to the bare-name form.
- Associated function vs method (`Type::new` static vs `&self` method): both MUST resolve.
- Enum/trait qualified forms (`Enum::Variant`, `Trait::method`): out of scope for guaranteed resolution in this feature unless trivially covered; if unsupported, the not-found message MUST be truthful (never a wrong hit).
- `explore` query whose tokens genuinely match a symbol name exactly (e.g. asking for a specific function): the exact-name symbol MUST still rank at or near the top — the fix must not over-correct away from legitimate strong name matches.
- `explore` query with zero conceptually-relevant results: MUST return an honest empty/low-confidence result, not a spurious 1.00.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `edit_plan` MUST resolve a `Type::method` target to the method defined on that type, producing the same plan as the bare-name and `file::symbol` selector forms for the same underlying symbol.
- **FR-002**: When a method name is shared across multiple types, `edit_plan` with a `Type::method` target MUST resolve to the method on the named type specifically (disambiguation), not report "not found" and not pick a different type's method.
- **FR-003**: All pre-existing `edit_plan` selector forms — bare symbol name, `file-path::symbol`, and plain file path — MUST continue to resolve exactly as before this change (no regression).
- **FR-004**: When a `Type::method` target genuinely does not exist, `edit_plan` MUST return a truthful not-found result that states what was searched; it MUST NOT return a wrong-but-plausible symbol.
- **FR-005**: `explore` MUST surface conceptually-central symbols for a concept query even when those symbols' names do not literally share the query's tokens, such that the symbols implementing the queried concept appear in the top results.
- **FR-006**: `explore` MUST NOT allow a symbol that is unrelated to the query concept to occupy the single highest score purely on a spurious signal; the top result MUST be defensibly related to the query.
- **FR-007**: `explore` ranking MUST remain deterministic for identical query + repository state (Constitution IV), and MUST continue to disclose ranked/truncated status via trust metadata (Constitution III).
- **FR-008**: `explore` (and the `edit_plan`/selector path) MUST remain frecency-neutral — ranking MUST NOT write frecency back as a side effect (Constitution V).
- **FR-009**: Both behaviors MUST be equivalent across the `stdio` and `serve` transports (Constitution VII), and MUST NOT break the `embed` build (Constitution VI).
- **FR-010**: The changes MUST ship with regression tests that fail on the current (pre-fix) behavior and pass after the fix — for US1, tests asserting `Type::method` resolution and disambiguation; for US2, tests asserting the expected central symbols appear in the top-N for the two example queries.

### Key Entities

- **Symbol selector**: the string a caller passes to address a symbol — bare name, `file::symbol`, plain file path, or (newly) `Type::method`. The feature widens the accepted grammar of this selector without changing the others' meaning.
- **Concept-ranking result**: an ordered list of symbols `explore` returns for a natural-language query, each with a relevance score and disclosed as ranked/truncated. The feature changes how scores are assigned so conceptual relevance is not reduced to symbol-name token overlap.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `edit_plan("GitRepo::tracked_paths")`, `edit_plan("GitRepo::head_sha")`, `edit_plan("WorktreeCache::new")`, `edit_plan("WorktreeCache::lookup")`, and `edit_plan("SharedIndexHandle::new")` all resolve to the correct method (0 of 5 currently resolve; target 5 of 5).
- **SC-002**: For every symbol reachable by a bare-name selector today, the equivalent `Type::method` selector (using its declaring type) resolves to the same symbol — verified across a representative sample with zero regressions to the bare-name / `file::symbol` / file-path forms.
- **SC-003**: For the query "worktree routing hook registration in the daemon", the top results include at least the registration entry point and the worktree-aware hook type (both currently absent from the surfaced set).
- **SC-004**: For the query "watcher interact with analyze_file_impact", the top result is related to the watcher/impact interaction and the previously-surfaced unrelated top hit no longer occupies the single highest score.
- **SC-005**: The full verification gate (Constitution VIII) passes: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release` — all green, with the new regression tests included.
- **SC-006**: No net increase in wasted round-trips: a caller addressing a known method by `Type::method` succeeds on the first call instead of failing and retrying.

## Assumptions

- The `explore` ranking fix targets ordering/coverage quality, not a new retrieval backend; it stays within the in-process LiveIndex read path (Constitution I) — no second index or external service.
- "Top results" / "top-N" is interpreted against `explore`'s existing default result cap; the fix improves what lands in that cap, not the cap size itself, unless a small cap change is separately justified.
- `Type::method` disambiguation reuses the existing symbol/disambiguation resolution machinery (`resolve_symbol_selector`) rather than introducing a parallel resolver.
- The two example `explore` queries are representative acceptance anchors, not the only cases; the fix should generalize, but these are the measurable checks.
- Enum-variant and trait-qualified selector forms are out of scope for guaranteed resolution in this feature (may be a follow-up); only `Type::method` (inherent and associated functions on structs/types) is in scope.
