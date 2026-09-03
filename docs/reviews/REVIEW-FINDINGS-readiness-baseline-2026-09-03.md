# REVIEW-FINDINGS — readiness baseline — 2026-09-03

**Baseline:** `main` @ `6188c5af` (worktree `1b570f1c` = 6188c5af + this plan doc only; clean tree at start).
**Machine:** Windows 11 Pro 10.0.26200, Intel Core Ultra 7 265, x64. Disk at start: C: 113.4 GB free, E: 25.3 GB free.
**Instrument note:** SymForge MCP tools and Terminal Commander were NOT mounted this session (the execution plan's session-delta assumed they were). Fallbacks: symforge queried via stdio MCP client against `target/debug/symforge.exe` (SF-MCP harness, temp script), builds/tests via supervised `hub` processes with real exit codes. All wall times below are supervised-process uptimes, not estimates.

## Phase 0 gate table

| Gate | Command | Wall time | Result |
|---|---|---|---|
| fmt | `cargo fmt --check` | 3.0 s (with diff) | PASS (clean) |
| diff | `git diff --check` | — | PASS (clean) |
| clippy | `cargo clippy --all-targets -- -D warnings` | 4m55s | PASS (exit 0) |
| serial test gate | `cargo test --lib --bins --tests -- --test-threads=1` | 19m12s | PASS — **4324 passed, 0 failed, 24 ignored, 135 suites** |
| release build | `cargo build --release` | 10m47s | PASS (exit 0) |
| embed check | `cargo check --no-default-features --features embed` | 28.5 s | PASS (exit 0) |
| npm wrapper | `npm test` (in `npm/`) | 1.4 s | PASS (0 fail) |
| bench smoke | `cargo bench --bench observed_refresh_gate_v1 -- --test` | 7m26s | PASS (exit 0) |
| stel replay non-vacuous | `cargo test --test stel_golden_replay -- --test-threads=1 --nocapture` (after cloning corpora) | 8.2 s | PASS — 7/7 real passes, zero skip lines under `--nocapture` |

## Phase 0 step 6 — corpora

`tests/fixtures/phase0-corpus/` contained README + .gitignore only (as the execution plan noted). Cloned per README: `cfg-if-rust`, `records-python`, `is-plain-obj-ts` (all `--depth 1`). The serial gate then ran the replay tests for real; the explicit `--nocapture` rerun proves no vacuous-skip path fired.

## Phase 0 step 7 — SymForge self-index stats (instrument)

Via stdio MCP `health` against the debug binary (11.0.12; npm install is 11.1.0 — **version drift on the dogfood daemon**, carried to Phase 7 findings):

- 1144 indexed files (1138 parsed, 4 expected-vendor partial — all `vendor/tree-sitter-scss`, 2 failed — both intentionally malformed test fixtures), 35,734 symbols.
- Admission: 1178 discovered → 1144 Tier-1, 34 Tier-2, 0 Tier-3.
- `load_source=snapshot_restore`, `index_state=snapshot_loaded_reused`, `index_generation=0`; snapshot verify `state=pending`.
- Git temporal ready (500 commits/90d, computed 1580ms).
- Knowledge curation: `capability=unavailable reason=atomic_durability_unavailable` — noteworthy; flagged for Phase 3.
- Hook adoption telemetry (this dogfood session): 4296/13747 routed (31%), 9451 fail-open outcomes (8471 no-sidecar, 980 sidecar errors) — the sidecar-error count is called out by the tool itself as "real routing failures worth investigating"; carried to Phase 3.

## Baseline verdict

**All gates green. No P0 at baseline.** Later phases proceed per plan.
