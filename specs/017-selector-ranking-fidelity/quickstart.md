# Quickstart / Acceptance: Selector & Concept-Ranking Fidelity

Runnable checks that prove the two fixes. "Before" reflects 8.13.8 (defective);
"After" is the acceptance bar. Live MCP checks require the running daemon to be
rebuilt/reinstalled from the fix branch; the automated tests are the primary,
CI-enforced proof.

## Prerequisites
- Build the fix branch: `cargo build --release`.
- Full gate (Constitution VIII): `cargo fmt --check` · `cargo check` ·
  `cargo clippy --all-targets -- -D warnings` ·
  `cargo test --all-targets -- --test-threads=1` · `cargo build --release`.
- Embed isolation (Constitution VI): `cargo check --no-default-features --features embed`.

## P1 — `edit_plan` resolves `Type::method`

### Automated (primary proof)
Regression tests in `tests/edit_plan_symbol_line.rs` (and/or `tests/symbol_disambiguation.rs`):
- `Type::method` with a unique method resolves to the same symbol as the bare name.
- `Type::method` disambiguates a method name shared across types (resolves to the named type's method).
- Bare name, `file::symbol`, and file-path selectors resolve unchanged (regression guard).
- `Type::nonexistent_method` returns a truthful not-found (no wrong hit).

Run: `cargo test --all-targets -- --test-threads=1 edit_plan symbol_disambiguation`

Before → After for the five anchor selectors (SC-001): **0/5 → 5/5** resolve.

### Live (secondary, after reinstall)
```
edit_plan("GitRepo::tracked_paths")   → resolves to Method tracked_paths in src/git.rs (was: not found)
edit_plan("WorktreeCache::new")       → resolves to WorktreeCache::new                  (was: not found)
edit_plan("SharedIndexHandle::new")   → resolves to SharedIndexHandle::new              (was: not found)
edit_plan("tracked_paths")            → unchanged (still resolves)
edit_plan("src/git.rs::tracked_paths")→ unchanged (still resolves)
edit_plan("GitRepo::does_not_exist")  → truthful "not found", names what was searched
```

## P2 — `explore` surfaces concept-central symbols

### Automated (primary proof)
Anchor-query top-N assertions (unit tests near the scorer in `src/live_index/query.rs`
or an integration test over a fixture index), plus no regression to the existing
`explore_result_view_*` tests in `src/protocol/format.rs`:
- Query "worktree routing hook registration in the daemon": top-N includes the registration
  entry point and the worktree-aware hook type (SC-003).
- Query "watcher interact with analyze_file_impact": top result is watcher/impact-related; the
  previously-spurious unrelated top hit no longer holds the single highest score (SC-004).
- Determinism (IV): identical query + index ⇒ identical order (stable-sort assertion).
- Frecency neutrality (V): running `explore` does not bump frecency.
- Exact-name guard: a query naming a specific function still ranks that function at/near the top
  (no over-correction).

Run: `cargo test --all-targets -- --test-threads=1 explore`

### Live (secondary, after reinstall)
```
explore("worktree routing hook registration in the daemon")
  → top results include register_if_feature_enabled and WorktreeAwareEditHook
    (was: worktree_routing_health_status at 1.00, registration symbols absent)
explore("watcher interact with analyze_file_impact")
  → top result relates to the watcher/impact interaction
    (was: unrelated run(SetupCliArgs) at 1.00)
```

## Done criteria (maps to Success Criteria)
- [ ] SC-001 5/5 `Type::method` selectors resolve
- [ ] SC-002 no regression to bare/`file::symbol`/file-path selectors
- [ ] SC-003 registration symbols present in explore top-N for anchor query 1
- [ ] SC-004 anchor query 2 no longer topped by an unrelated symbol
- [ ] SC-005 full gate + embed check green with new tests
- [ ] SC-006 `Type::method` succeeds first call (no failed-retry round-trip)
- [ ] Constitution IV/V/III/VI/VII checks covered by tests where mechanical
