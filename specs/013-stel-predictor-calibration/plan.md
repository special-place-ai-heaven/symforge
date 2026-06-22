# Implementation Plan: STEL Predictor Calibration

**Branch**: `013-stel-predictor-calibration` | **Date**: 2026-06-22 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/013-stel-predictor-calibration/spec.md`

## Summary

Close the `calibration_auto_tune` deferred seam, honestly. Two pieces: (1) make the durable SQLite ledger — which already ships in serve mode under the project's symforge data dir — available in the stdio/embed deployment so predicted-vs-actual events accumulate across sessions instead of resetting to zero; (2) add an auto-tune consumer that corrects the planner's plan-floor token-estimate constants (`400/800`) and the `schema/invoke` constants (`45/80`) from accumulated error, gated by held-out validation so it can only ever reduce prediction error. Calibration becomes a truthful `deferred -> accumulating -> tuned` state machine; every served figure stays an estimate (010 honesty contract). Routing, policy, and safety guards are untouched.

## Technical Context

**Language/Version**: Rust (workspace toolchain, `rust-toolchain.toml`).

**Primary Dependencies**: existing `rusqlite` (SQLite, WAL) used by `src/stel/ledger_store.rs`; the L2 economics in `src/stel/controller.rs`; the observational summary in `src/stel/calibration.rs`. No new external dependency anticipated.

**Storage**: the existing per-project SQLite store `stel-ledger.db` (`StelLedgerStore::open(dir, session_id)`, WAL + `busy_timeout=5000`), located under the project's symforge data dir (`src/server/serve.rs:297-328`). This feature extends it (estimator-version tagging, bounded retention, tuned-constant persistence) and wires it into the stdio/embed dispatch path.

**Testing**: `cargo test --all-targets -- --test-threads=1` (server feature), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, embed `--lib` build; deterministic corpus-replay unit tests for tuning + held-out validation.

**Target Platform**: stdio MCP (the operator's actual deployment), serve (`/mcp` + admin), and the `embed` facade. The `stel` module is `#[cfg(feature = "server")]`; embed must keep compiling.

**Project Type**: single Rust crate (compiler/CLI/MCP server).

**Performance Goals**: durable writes stay off the request hot path (the existing `record` is non-blocking, `protocol/mod.rs:312-316` P2-C); tuning runs on accumulated history, not per-call; a calibration pass must add no measurable per-call latency.

**Constraints**: estimates remain `chars/4`-derived estimates (never measured); calibration MUST NOT change routing/policy/safety (FR-007); a tuning that does not reduce held-out error MUST be rejected (FR-005); durable store bounded (FR-002).

**Scale/Scope**: bounded ledger (retention cap, default target set in research.md); per-project calibration; single SQLite db per project data dir.

## Constitution Check

*GATE: must pass before Phase 0 and re-checked after Phase 1.*

- **Honesty contract (010 keystone)** — PASS by design: `calibration: tuned` only with a held-out error-reduction artifact (FR-009); served figures stay `(est.)` (FR-010); no `validated`/`saved`/`active` without code+artifact. The 010 `surface_honesty` regression is extended to cover the new states (SC-005).
- **Calibration is estimate-only** — PASS: FR-007 forbids touching routing/policy/safety; golden-replay + policy behavior asserted unchanged across all calibration states (SC-004).
- **Evidence over confidence** — PASS: tuning is rejected unless it provably reduces held-out error (FR-005); every adjustment is audited with sample size + before/after error (FR-008).
- **Honest degradation** — PASS: no durable store -> in-memory `deferred`, reported distinguishably (FR-003), reusing the existing `StelLedgerStore::Disabled` / `Unavailable` distinction.
- **No false success** — PASS: cold start stays `deferred` (not a failure, not a fake `tuned`).

No constitution violations -> Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/013-stel-predictor-calibration/
├── spec.md          # done
├── plan.md          # this file
├── research.md      # Phase 0: the 4 clarifications resolved
└── data-model.md    # Phase 1: entities + schema changes
```

### Source Code (repository root)

```text
src/stel/
├── ledger_store.rs   # EXTEND: estimator_version tagging; bounded retention (closes P3-B, ledger_store.rs:39); tuned-constant persistence
├── calibration.rs    # EXTEND: from observational summary -> CalibrationState machine + auto-tune candidate derivation + held-out validation
├── controller.rs     # WIRE: estimate_economics() reads tuned constants when in force (falls back to 400/800 + 45/80 floors)
└── status.rs         # SURFACE: honest deferred/accumulating/tuned + before/after error in `status detail: full`

src/server/serve.rs   # reference wiring (durable store already opened here)
src/server/<stdio entry>  # WIRE: open + share the same Arc<StelLedgerStore> on the stdio/embed dispatch path (today it is in-memory only)

tests/
├── stel_calibration_tuning.rs   # NEW: deterministic corpus replay — tuning reduces held-out error; rejects worse-than-baseline; reproducible
├── stel_ledger_persistence.rs   # NEW: cross-session accumulation across >=3 restarts; bounded retention; degrade-to-disabled
└── surface_honesty.rs           # EXTEND: tuned/accumulating states stay honest (no tuned without artifact)
```

**Structure Decision**: single-crate, in-place extension of the existing `stel` ledger + economics path. No new modules/abstractions beyond a `CalibrationState` type and a tuned-constants record; the durable store, WAL concurrency, non-blocking write, and Disabled-degrade machinery already exist and are reused.

## Phases

- **Phase 0 — Research (this plan):** resolve the 4 `[NEEDS CLARIFICATION]` items -> [research.md](./research.md). Outcome: per-project scope; retention cap; estimator-version sample tagging; WAL concurrency confirmed.
- **Phase 1 — Design:** entities + schema deltas -> [data-model.md](./data-model.md). The `CalibrationState` machine, `PredictionErrorSample` (estimator-version-tagged), `TunedEstimateConstants` (persisted + audited), and the L2 read path.
- **Phase 2 — Tasks (`/tasks`, not produced here):** ordered, independently-testable tasks per user story (US1 persistence -> US2 auto-tune -> US3 honest surfacing), each gated by the verification commands above. US1 is the MVP slice (durable accumulation in stdio) and is shippable alone.

## Complexity Tracking

No constitution violations; no added abstractions requiring justification. (Empty by design — reuse over new structure.)

## Open decision for confirmation

Phase 0 recommends **per-project** calibration (follows the existing per-project store location). If a single global calibration across all indexed repos is preferred instead, say so before `/tasks` — it changes the store location and the retention model. All other clarifications are resolved as engineering defaults in research.md.
