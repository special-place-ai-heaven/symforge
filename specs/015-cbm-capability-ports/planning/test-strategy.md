# Test Strategy — 10% Validation Layer

**Planning owns fixture design and acceptance criteria (60%).**  
**Coding owns implementation (30%).**  
**This document owns proof obligations (10%).**

## Validation philosophy

Tests are not where we discover requirements — acceptance-matrix.md does that.
The 10% layer **proves** the sprint spec was met and nothing regressed.

## Test pyramid (program 015)

| Layer | Share of V effort | Location | When |
|-------|-------------------|----------|------|
| Unit | 40% | `src/**/tests`, inline `#[cfg(test)]` | With `[C]`, run in `[V]` |
| Integration | 45% | `tests/*.rs`, fixtures | `[V]` sprint end |
| Dogfood MCP | 10% | quickstart.md manual | `[V]` sprint end |
| Perf smoke | 5% | `tests/cbm_spike_*` `--ignored` | S0, S3, release |

**Not in 10%**: Writing acceptance tests skeleton — that's `[P]` (planning).

## Per-sprint validation minimum

| Sprint | Required `[V]` proofs |
|--------|----------------------|
| S0 | 3 spike tests recorded; go/no-go doc |
| S1 | A-US1-01..05, A-US2-01..04, A-US4-01, A-CONST-02 |
| S2 | A-US5-01..03, A-US6-01..03, A-US7-01..02 |
| S3 | A-US8-01..03, A-US9-01 |
| S4 | A-US10-01..03 |
| S5 | A-US11-01, A-US12-01 |
| S6 | A-US13-01, A-US14-01, A-US15-01 |

## Full gate (every sprint `[V]`)

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo check --no-default-features --features embed --lib
cd npm && npm test
```

## Frecency regression suite

Extend `tests/frecency_ranking.rs` in S1 `[V]`:

- `detect_impact_does_not_bump`
- `trace_path_does_not_bump`
- `query_graph_does_not_bump`
- `semantic_find_does_not_bump` (S4)

## Surface regression

- `tests/surface_honesty.rs` — compact schema token budget
- `tests/surface_probe.rs` if present — enforce compact gating

## Embed regression

- `cargo test --no-default-features --features embed`
- Contract test unchanged list in `embed.rs`

## Perf validation (not blocking CI except S0)

| Metric | Tool | Threshold |
|--------|------|-----------|
| BFS p95 | cbm_spike_graph_bfs | <100ms |
| Artifact bootstrap | team_artifact test | ≥80% time saved |
| Resolver accuracy | rust_resolver | ≥80% |
| Hook latency | hook_augment | <100ms |

Record numbers in sprint spec `[V]` sign-off table.

## What we deliberately do NOT test (YAGNI)

- Full CBM parity on 158 languages
- 3D UI
- Every Cypher clause CBM supports
- Multi-repo CROSS_* (defer 012+)

## Validation sign-off template

```markdown
## Sprint N Validation Sign-off
- Date:
- Gate commands: PASS/FAIL
- Acceptance rows verified: (list IDs)
- Perf metrics: (paste)
- Known gaps: (must be empty or decision-log)
- Sign-off:
```
