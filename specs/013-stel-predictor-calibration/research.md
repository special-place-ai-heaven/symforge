# Phase 0 Research: STEL Predictor Calibration

Resolves the four `[NEEDS CLARIFICATION]` items from [spec.md](./spec.md). Each decision is grounded in the existing code so the design extends reality rather than inventing it.

## R1 — Calibration scope: per-project vs global

**Decision: per-project.** Calibration data and tuned constants live with the project they were observed in.

**Rationale**: the durable ledger already lives per-project — `StelLedgerStore::open` is called "under the project's symforge data dir" (`src/server/serve.rs:297-328`), one `stel-ledger.db` per project. Response-token sizes are a property of the codebase being served (a 50-file repo and a 1M-LOC monorepo produce different actual-vs-predicted distributions), so a global pool would average across incomparable populations and tune worse. Per-project follows the existing storage boundary with no new location concept.

**Alternatives rejected**: global pool (simpler, one db) — rejected: mixes incomparable size distributions, lowering tuning quality and muddying the held-out validation. A per-intent/per-tool split within a project — deferred: start with project-level constants; finer granularity is a later refinement only if held-out error justifies it (YAGNI until measured).

## R2 — Retention bound

**Decision: bounded retention, prune-on-write, default cap (count-based, tunable).** Keep the most recent N events per project (default target N in the low thousands, finalized in data-model.md), pruning oldest beyond the cap.

**Rationale**: this directly closes the already-identified deferred debt — `ledger_store.rs:39-42`: "REVIEW P3-B (deferred): `stel_ledger_events` grows unbounded — no TTL, prune, or capped-table retention. Future fix: TTL/archival or a capped table." A count cap is deterministic (aids reproducible tuning, FR-012) and trivially bounds disk. Calibration only needs a recent, adequate sample (`TUNING_REVIEW_MIN_EVENTS = 5` is today's floor; tuning will want more), not full history.

**Alternatives rejected**: unbounded (status quo) — rejected, that is the P3-B bug. Age/TTL-based — viable but less deterministic for tests; count-based is simpler and bounds disk directly. Capped SQLite table (trigger) — equivalent; prune-on-write in `record` keeps the logic in one place and testable.

## R3 — Estimator-version sample invalidation

**Decision: tag every sample with the estimator version; calibration filters to the current version.** Samples recorded under a different estimator are retained (audit) but excluded from the active tuning population.

**Rationale**: 010 changed the estimator (byte-grounded vs the old flat `520/900` floor — see `controller.rs:253,310,401`). A future estimator change would otherwise let stale-estimator error silently pollute a new calibration, violating the honesty contract (tuning on data that no longer reflects how the estimator behaves). The store already versions via `stel_ledger_meta` (`ledger_store.rs:44, schema_version`), so adding an `estimator_version` column to `stel_ledger_events` and filtering on read is a small, honest extension. Never silently delete old samples (they stay auditable); just exclude them from tuning.

**Alternatives rejected**: invalidate-on-version-change (wipe) — rejected: destroys audit history and is a blunt instrument. Ignore versioning — rejected: the exact silent-pollution failure the honesty contract forbids.

## R4 — Concurrent stdio writers (multi-process, one state dir)

**Decision: already handled — SQLite WAL + busy_timeout; treat as last-writer-safe append, no new locking.** Document multi-process append as supported.

**Rationale**: `SqliteStelLedgerStore::open` already sets `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000` (`ledger_store.rs:312-313`), explicitly "mirrors analytics store pattern." WAL permits concurrent readers + a serialized writer with the busy timeout absorbing contention; appends from multiple stdio processes against one project db are tolerated without corruption. The existing `record` is non-blocking off the request path (`protocol/mod.rs:312-316`). No new locking is needed; the stdio store reuses the same `open()` and therefore the same PRAGMAs.

**Alternatives rejected**: single-writer lock / advisory file lock — rejected as unnecessary given WAL + busy_timeout. Per-process separate dbs — rejected: would fragment the per-project calibration population (defeats R1).

## Cross-cutting: how tuning stays honest (binds Phase 1)

- Tuning derives candidate constants from accumulated predicted-vs-actual error, then **validates on a held-out slice** (events not used to derive the candidate). A candidate is applied only if held-out mean absolute error drops versus the constants currently in force (FR-005). This is the artifact that lets `calibration` read `tuned` (FR-009).
- The result is still an estimate: tuned constants replace the static floor, but the served figure stays `(est.)` (FR-010). Grounding in history is not measurement.
- Tuning is deterministic given a fixed corpus (FR-012) -> the held-out validation and tests are stable; same events in, same constants out.
- Tuning never touches routing/policy/safety (FR-007); only `estimate_economics` reads the tuned constants, and only for token figures.

## R5 — SC-002 numeric margin

**Decision: tuned constants must reduce held-out mean absolute prediction error (MAE) by >= 20% (relative) versus the static `400/800` + `45/80` floors on a biased corpus; and must NOT increase held-out MAE on an unbiased corpus.** This is the "meaningful margin" SC-002 left to `/plan`.

**Rationale**: dogfood observed predictor errors of 40-194% against actuals, so a calibration worth shipping should close a real fraction of that, not a rounding-error sliver. 20% relative MAE reduction is a defensible, deterministic, testable bar (the corpus is fixed, FR-012) that is well above noise yet not so aggressive it over-fits a small sample. US2's accept-path test (`tests/stel_calibration_tuning.rs`, task T024/T029) asserts this specific margin, not merely "strictly drops"; the reject gate (FR-005) rejects any candidate below it. The threshold is a tunable const so it can be raised once real per-project data accrues.

**Alternatives rejected**: "strictly reduces" (any improvement) — rejected: too weak, would ship a 0.5% gain as `tuned` and over-promote the surface. A fixed absolute token margin — rejected: not comparable across codebases of different size (R1 per-project).

## R6 — Deployment reality: daemon-default + embed reachability

**Decision: durability must reach BOTH the local stdio path AND the default daemon-backed stdio worker; the durable store is embed-reachable via `any(server, embed)`.**

**Rationale**: the operator's default stdio deployment is daemon-backed (`run_remote_mcp_server_async` -> `new_daemon_proxy`, main.rs:223/257); the daemon WORKER, not the proxy, executes tool calls. Wiring only the `SYMFORGE_NO_DAEMON` local path (main.rs:418) would pass US1's local test while the operator's real sessions still reset — fake-green against FR-001/SC-001/SC-003. So US1 owns both paths (T020 local, T021 daemon worker). Separately, the `ledger_store` module is `#[cfg(server)]` today, so the durable store does not exist in embed; FR-001 requires stdio/embed durability, so it is un-gated to `any(server, embed)`. rusqlite is an unconditional dependency (Cargo.toml), so this pulls in no server/network stack — Principle VI (embed isolation) holds.
