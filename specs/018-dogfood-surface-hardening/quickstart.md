# Quickstart: Validating Dogfood Surface Hardening

Runnable validation for each story. Automated regression tests are the source of truth
(Constitution VIII); the live MCP checks are the human/agent-perspective confirmation.

## Prerequisites

- Branch `018-dogfood-surface-hardening` checked out.
- Full gate available: `cargo test --all-targets -- --test-threads=1`, plus
  `cargo check --no-default-features --features embed`.

## Per-story validation

### US1 — source-focused change/impact
**Automated**: `cargo test --all-targets -- --test-threads=1 what_changed detect_impact`
- A test creates a fixture repo whose only dirty file is a non-source data file (e.g. `data/x.json`)
  and asserts default `what_changed` (uncommitted) returns 0 code changes, while `code_only=false`
  returns it. A second test asserts default impact from a real source edit contains no
  data-file-derived symbols.

**Live (agent perspective)**: on a repo with untracked JSON files, call `what_changed` (uncommitted,
no flags) → source files only; `detect_impact` (defaults) → blast radius is source-only; then pass
the explicit data-inclusion opt-in → old inclusive result returns.

### US2 — browse importance ranking
**Automated**: `cargo test --all-targets -- --test-threads=1 search_symbols browse`
- A fixture scope with one heavily-referenced symbol plus generic short names; assert the
  heavily-referenced symbol outranks `add`/`get`/`len`, and that two identical browse calls
  produce identical ordering (determinism), and a frecency-neutrality assertion.

**Live**: `search_symbols(kind="fn", path_prefix="src/", limit=20)` with no query → notable symbols
first, not `add`/`get`/`fmt`.

### US3 — repo-map root guard
**Automated**: `cargo test --all-targets -- --test-threads=1 repo_map outline root`
- Build an index whose file set includes an escaping path; assert `capture_repo_outline_view`
  (full) omits it; assert a clean in-root repo's outline file count is unchanged.

**Live**: `get_repo_map(detail="full", max_files=50)` → every path is under the workspace root.

### US4 — CCR footer on truncation
**Automated**: `cargo test --all-targets -- --test-threads=1 ccr footer truncat`
- A big-response builder with a tight `max_tokens` asserts the output contains a
  `symforge_retrieve` hash footer and that fetching the hash returns the full payload; a
  within-budget response asserts no footer.

**Live**: `get_repo_map(detail="full", max_tokens=300)` → truncated output ends with a
`symforge_retrieve ... hash="..."` line; `symforge_retrieve(hash=...)` returns the full map.

## Full gate (Constitution VIII) — required before "done"

```
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed   # VI embed isolation
```

All green, with the four new fail-first regression tests included, is the completion bar.
Each story is independently landable; US1 alone is a shippable MVP.
