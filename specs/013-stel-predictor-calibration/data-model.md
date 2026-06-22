# Phase 1 Data Model: STEL Predictor Calibration

Entities and storage deltas. Builds on the existing `stel-ledger.db` (`src/stel/ledger_store.rs`); all changes are additive and migration-gated (the store already carries a `schema_version` in `stel_ledger_meta`).

## Entity: PredictionErrorSample

One observed event — the unit calibration learns from. Already largely captured in `stel_ledger_events` (predicted vs actual response tokens per event); this feature adds the version tag.

| Field | Source | Notes |
|---|---|---|
| `predicted_response_tokens` | existing event | from `estimate_economics` |
| `actual_response_tokens` | existing event | `chars/4` of the real served body |
| `predicted_schema_tokens` / `predicted_invoke_tokens` | existing (`45/80`) | the `COMPACT_SCHEMA_TOKENS`/`COMPACT_INVOKE_TOKENS` in force at record time |
| `decision` | existing event | serve/degrade/bypass/cache_hit |
| `est_byte_size` | existing (010 grounding) | distinguishes byte-grounded vs plan-floor events |
| **`estimator_version`** | **NEW column** | the estimator that produced the prediction (R3); calibration filters to current |

**Schema delta**: `ALTER TABLE stel_ledger_events ADD COLUMN estimator_version TEXT` (migration bump). Backfill existing rows with a sentinel (`pre-013`) so they are excluded from the active tuning population (retained for audit).

**Retention (R2)**: prune-on-write in `record` — after insert, delete rows beyond the most-recent `LEDGER_RETENTION_MAX` per project. Default `LEDGER_RETENTION_MAX = 2000` (count-based, deterministic; tunable const). Closes `ledger_store.rs:39` P3-B.

## Entity: TunedEstimateConstants

The calibrated replacements for the static floors, plus the evidence that justifies them. Persisted so tuning survives restart and is auditable.

| Field | Notes |
|---|---|
| `response_floor` | tuned replacement for the `400` plan-floor (`controller.rs`) |
| `manual_floor` | tuned replacement for the `800` manual baseline |
| `schema_tokens` / `invoke_tokens` | tuned replacements for `45` / `80` |
| `estimator_version` | the estimator these were tuned for (must match current to be in force) |
| `sample_size` | events used to derive |
| `error_before` / `error_after` | held-out mean absolute error before/after (the artifact for `tuned`) |
| `tuned_at` | timestamp (audit) |

**Storage**: new `stel_calibration` row(s) in the db (or `stel_ledger_meta` keys for a single active set). Single active set per project + estimator_version. Writing a new set is the audited gated action (FR-008).

**In-force rule**: `estimate_economics` (`controller.rs:319`) reads the active `TunedEstimateConstants` when present AND `estimator_version` matches current; otherwise falls back to the existing `400/800` + `45/80` floors. 010's byte-grounded path is unchanged — tuning corrects the FLOOR that applies when byte grounding is absent, and the schema/invoke constants.

## Entity: CalibrationVerdict

The honest state machine surfaced on `status detail: full` and the opt-in full envelope. Replaces today's hard-coded `tuning_note` ("auto-tuning still deferred", `calibration.rs:74-80`).

> **Name (collision avoided)**: this is a NEW type named `CalibrationVerdict` — NOT `CalibrationState`, which already exists at `types.rs:344` as an inert per-tool EMA struct. It lives in `calibration.rs` and, like the durable store, is reachable under `any(feature="server", feature="embed")` (rusqlite is an unconditional dep; no server/network stack enters embed).

> **T007 (Foundational) verified note**: the Foundational phase (T004–T013) introduces NO honest-surface state type. The ONLY new public type it adds is the DB-layer storage primitive `TunedEstimateConstants` (a pure POD in `ledger_store.rs`, no I/O, no derivation); the honest `Deferred/Accumulating/Tuned` state machine lands later as `CalibrationVerdict` (US3), never overloading `CalibrationState`. `types::CalibrationState` (per-tool EMA) is left untouched and still compiles (`cargo check --features server` green). The `CalibrationVerdict` lives at `any(server, embed)` per US3; for the Foundational phase the durable store stays `#[cfg(feature="server")]` (the embed un-gate is US1 T018).

```text
Deferred                       # no/insufficient samples for current estimator_version
  -> Accumulating { n, min }   # samples gathering toward the tuning minimum
  -> Tuned { sample_size, error_before, error_after }   # a candidate reduced held-out error and is in force
```

**Transitions**:
- `Deferred -> Accumulating`: first current-version sample recorded.
- `Accumulating -> Tuned`: sample_size >= minimum AND a derived candidate reduces held-out MAE vs the in-force constants (FR-005). Otherwise stay `Accumulating` (a candidate that does not improve is rejected, never applied).
- `* -> Deferred`: estimator_version change invalidates the active population (R3); or operator reset (FR-011).

**Honesty invariants (FR-009, SC-005)**:
- `Tuned` MUST carry `error_before`/`error_after` proving the reduction; the word `tuned` never appears without it.
- No state renders `validated`/`saved`/`active`; the served figure stays `(est.)` in every state.
- A `Disabled`/`Unavailable` durable store renders distinctly (reuse `DurableLedgerState`, `status.rs:41-49`) and pins state at `Deferred` (in-memory only) — never a silent `Tuned`.

## Internal contract (no external API change)

This feature adds no new public schema. The only MCP-surface change is the FR-011 operator reset/clear, exposed as a MODE/param on an existing tool (constitution II, MCP-native — `SYMFORGE_TOOL_NAMES` kept in sync), never injected context. The internal seams:

- `calibration.rs`: `derive_tuning_candidate(samples) -> Option<TunedEstimateConstants>` and `validate_candidate(candidate, held_out) -> bool` (held-out MAE must drop). Pure + deterministic (FR-012) -> unit-testable on a fixed corpus without a live store.
- `ledger_store.rs`: `samples_for_estimator(version, limit)`, `load_active_tuning()`, `store_active_tuning(constants)` (audited), plus retention in `record`.
- `controller.rs`: `estimate_economics` consults `load_active_tuning()` (cheap, cached) before applying the static floor.

The `surface_honesty` test corpus (010) is extended to assert every `CalibrationVerdict` renders honestly; `stel_calibration_tuning.rs` asserts the held-out-error-reduction and worse-than-baseline-rejection on a deterministic fixture corpus; `stel_ledger_persistence.rs` asserts cross-restart accumulation and bounded retention.
