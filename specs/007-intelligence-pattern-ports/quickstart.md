# Quickstart / Validation Guide: Intelligence Pattern Ports (007)

**Branch**: `007-intelligence-pattern-ports` · **Date**: 2026-06-16

This guide validates the four ports end-to-end. It is a run/validation guide, not
an implementation guide — implementation lives in `tasks.md` and the source.

## Prerequisites

- On branch `007-intelligence-pattern-ports` (`git branch --show-current`).
- Rust toolchain (2024 edition); repo builds clean on `main` baseline.
- A test repo with git history (the symforge repo itself has Ready git temporal:
  500 commits / 90d) so co-change paths exercise.

## Full verification gate (Constitution Principle VIII)

Run all of these; all must pass before "done":

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
# Embed isolation (Principle VI / G-045):
cargo check --no-default-features --features embed
```

Feature-specific test files:

```bash
cargo test --test impact_footer -- --test-threads=1
cargo test --test compact_map_ranking -- --test-threads=1
cargo test --test stel_find_fusion -- --test-threads=1
cargo test --test frecency_ranking -- --test-threads=1   # invariant guard
cargo test --test edit_plan_symbol_line -- --test-threads=1
```

## Scenario 1 — Impact footer (US1, FR-001..004)

1. In a fixture repo, edit a symbol with known dependents via any structural tool
   (`replace_symbol_body`, `edit_within_symbol`, `batch_edit`, `symforge_edit`
   apply).
2. **Expected**: response ends with `[impact: N dependents · cochanges: …]` where
   `N` matches the `find_dependents` count for that file.
3. Edit a zero-dependent symbol with no history → `[impact: 0 dependents]`.
4. Submit a failing edit → **no** footer line.
5. Re-submit the identical successful edit (idempotency replay) → footer is
   identical to the first apply.

## Scenario 2 — Orientation doctrine (US2, FR-005..006)

1. Request the onboarding and architecture-map MCP prompts.
   **Expected**: both contain "map orients / tools prove" and "absence from the
   map is not absence from the repo".
2. Read the `symforge://repo/map` resource (and call `get_repo_map` compact).
   **Expected**: body footer carries the doctrine + a truncation/completeness
   disclosure.

## Scenario 3 — Ranked compact map (US3, FR-007..009, FR-017)

1. Build a fixture where `core.rs` has many dependents + high churn and `leaf.rs`
   has none. Request `get_repo_map` (compact / default).
   **Expected**: `core.rs` ranks above `leaf.rs`; files with `≥2` dependents show
   `path (→N)`.
2. Request `get_repo_map detail=full` and `detail=tree`.
   **Expected**: output byte-identical to the pre-007 baseline (no reordering, no
   `(→N)`).
3. Render the compact map twice → identical order (deterministic tie-break).

## Scenario 4 — Find fusion (US4, FR-010..011, FR-014)

1. Issue a multi-word fuzzy query through the find intent (e.g.
   `"stel planner find"`).
   **Expected**: one merged ranked list spanning symbols and paths, co-change
   neighbors boosted; no new tool appears in the tool list.
2. Run the find query under `FlagGuard::on()` (frecency collection enabled).
   **Expected**: the frecency DB is NOT created/bumped (discovery is neutral).

## Scenario 5 — Impact intent + edit_plan co-change (US5, FR-012..013)

1. Invoke the impact intent on a symbol with dependents and co-change history.
   **Expected**: one envelope reports both dependents and co-change partners.
2. Run `edit_plan` on the same symbol.
   **Expected**: output includes a `Co-change partners: a, b` line.
3. Run `edit_plan` in a repo without git history (temporal not Ready).
   **Expected**: the co-change line is omitted cleanly (no error, no empty line) —
   existing `tests/edit_plan_symbol_line.rs` assertions still pass.

## Done signal

All gate commands green + all five scenarios observed as specified + `tasks.md`
items checked `[X]`. No commits (operator authorizes commit/PR separately).
