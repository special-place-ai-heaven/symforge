# Call-Time Capability Resolution Close-Out

Date: 2026-05-16

Status: task 07 implemented and verification green.

Implementation commit: `096604d` (`Add capability status health visibility`).

## Implementation Summary

- Health visibility now reports compact capability states for frecency, co-change, worktree routing, and ranking diagnostics.
- `health` renders a multi-line `Capabilities:` block; `health_compact` renders the same state on one compact line.
- Integration coverage proves env-vars-unset call-time behavior or explicit capability evidence for frecency, co-change, worktree routing, and ranking diagnostics.
- No multi-process router, multi-index swarm, broad `scope` parameter, or cloud control plane was implemented.

## Sample Status Output

```text
Capabilities:
  frecency: ready/session/no-history fallback-used-on-empty
  co-change: preparing/lazy-on-request fallback-used-on-request
  worktree routing: explicit-call enabled
  ranking diagnostics: call-time explain available/default-off
```

`health_compact` sample from installed-runtime smoke:

```text
Capabilities: frecency=ready/session/no-history fallback-used-on-empty; co-change=preparing/lazy-on-request fallback-used-on-request; worktree=explicit-call enabled; ranking=call-time explain available/default-off
```

## Verification Commands

- `cargo test --test capability_status_integration -- --test-threads=1` - 7 passed.
- `cargo test --test schema_roundtrip -- --test-threads=1` - 23 passed.
- `cargo test --test frecency_ranking -- --test-threads=1` - 21 passed.
- `cargo test --test cochange_fusion -- --test-threads=1` - 6 passed.
- `cargo test --test worktree_awareness -- --test-threads=1` - 19 passed.
- `cargo test --test search_files_ranking_debug -- --test-threads=1` - 5 passed.
- `cargo check` - passed.
- `cargo test --all-targets -- --test-threads=1` - passed; full suite completed with existing ignored perf/AAP full-smoke tests unchanged.
- `cargo build --release` - passed.
- `git diff --check` - passed; Git reported existing LF-to-CRLF working-copy warnings only.
- `rg -n "Capabilities:|frecency:|co-change:|worktree routing:|ranking diagnostics:|call-time capability" src tests README.md docs` - passed and found the health implementation, tests, README sample, and task docs.

## Installed Runtime Smoke

Binary:

```text
target/release/symforge.exe --version
symforge 7.10.0
```

Runtime smoke used `target/release/symforge.exe` as a stdio MCP server against a temporary git repo plus linked git worktree. The smoke initialized MCP, called `health_compact`, called `search_files rank_by="frecency"`, called `search_files rank_by="path+cochange" anchor_path="src/auth/routes.rs"`, called `search_files debug_ranking=true`, and safely ran `replace_symbol_body` with `working_directory` pointing at the temporary worktree.

Observed evidence:

```text
installed-runtime smoke passed
health_compact: Capabilities: frecency=ready/session/no-history fallback-used-on-empty; co-change=preparing/lazy-on-request fallback-used-on-request; worktree=explicit-call enabled; ranking=call-time explain available/default-off
frecency: Capability: frecency ranking fallback used - no frecency history found; path ranking returned.
cochange: Capability: co-change ranking preparing - no coupling store exists for this workspace; bounded background preparation started; path ranking returned.
debug: Ranking explanation:
edit: rerouted: true
```

## Residual Risks and Follow-Ups

- Health reports capability state at workspace/policy level. Per-query ranking reasons remain in `search_files(debug_ranking=true)`.
- Frecency health can detect persistent history presence, but current-process session history is intentionally not enumerated in health.
- Co-change health does not start lazy preparation; it only reports whether a request would use ready/current state, prepare/fallback, stale/fallback, unavailable, or disabled-by-policy state.
