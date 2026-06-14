# Data Model: SymForge v8 Phase 2 STEL Controller Maturity

Planning and evidence data model. Extends Phase 1 runtime types in `src/stel/types.rs`; does not introduce Phase 3 persistence.

## Entity: MultiStepStelPlan (L1 extension)

**Purpose**: Ordered internal tool chain for one compact `symforge` call.

**Fields**:

- `plan_id`: Stable identifier for ledger correlation.
- `steps`: Ordered list of `{ tool, args, step_index }` — minimum 2 for multi-hop golden rows.
- `confidence`: `exact | inferred | fallback` (existing L1 vocabulary).
- `source_row_id`: Optional golden row id when replaying corpus.
- `chain_class`: `single | multi` — mirrors golden `chain` field.

**Validation rules**:

- `steps.len() >= 1`; multi-hop golden rows require `steps.len() >= 2`.
- Each `tool` must be a registered legacy tool allowed on full surface.
- `must_not_call` from golden row must not appear in any step.
- Step order must match `must_call` order for golden replay PASS.

**State transitions**: Built by L1 planner → evaluated by L2 as a unit → executed stepwise by L3 on `serve`/`degrade`.

## Entity: StelDecision (L2 — hardened)

**Purpose**: Admission outcome driving L3 behavior and battery classification.

**Fields** (extends Phase 1):

- `decision`: `serve | degrade | bypass | cache_hit`.
- `predicted_tokens`, `manual_baseline_tokens`, `predicted_net`.
- `degrade_flags`: Optional list (e.g. `outline_only`, `no_hints`, `max_tokens_cap`).
- `bypass`: Optional `StelBypassBody`.
- `cache`: Optional `StelCacheBody` (session hit summary).
- `margin_band`: Which threshold triggered degrade/bypass.

**Validation rules**:

- `bypass` requires non-null `bypass` body and `legacy_executed=false`.
- `cache_hit` requires session evidence and `legacy_executed=false`.
- `degrade` requires at least one degrade flag when net ≤ margin_low.
- `serve` on small-file accepted rows must not produce `sGteM=true` in battery post-processing (H3).

## Entity: MultiStepExecutionResult (L3)

**Purpose**: Aggregated outcome of in-process step chain.

**Fields**:

- `plan_id`: Links to plan and ledger.
- `step_results`: Per-step `{ tool, outcome_class, output_tokens, legacy_executed }`.
- `combined_output`: Final text returned to host.
- `total_legacy_executed`: True if any step performed durable write.

**Validation rules**:

- Failed mid-chain step must not silently continue unless plan defines fallback (Phase 2: fail fast with envelope error).
- Ledger event records aggregate economics, not only final step.

## Entity: GoldenReplayClassification (updated)

**Purpose**: Honest partition of 36-row corpus.

**Fields** (per row):

- `row_id`: Golden id string.
- `category`: `SupportedServe | SupportedPffBypass | DeferredMultiHop | DeferredPlannerMismatch`.
- `planned_tools`: Actual L1 plan tool sequence.
- `decision`: L2 outcome when replayed.

**Validation rules**:

- Phase 2 exit: `DeferredMultiHop` count must be **0**.
- `DeferredPlannerMismatch` count must be **0** unless golden corpus changes.

## Entity: BatteryRowSTEL (sf-bench extension)

**Purpose**: Row-level economics for compare-results.

**Fields** (per stel-schema performance test schema):

```json
{
  "stel": {
    "plan_id": "string",
    "decision": "serve|degrade|bypass|cache_hit",
    "tools_called": ["string"],
    "predicted_tokens": 0,
    "actual_tokens": 0,
    "net_vs_manual": 0,
    "route_confidence": "exact|inferred|fallback"
  }
}
```

Plus row-level gate fields:

- `acceptedServe`: boolean
- `sGteM`: boolean
- `mcpCalls`: integer (external)
- `eligibleH6`: boolean

**Validation rules**:

- Missing `stel.decision` or `acceptedServe` blocks gate report acceptance.
- H3 evaluates `*_small` accepted serve rows only (A-012 scope).

## Entity: Phase2GateReport

**Purpose**: Phase 2 exit evidence bundle.

**Fields**:

- `candidate_results_path`: Path to battery JSON.
- `baseline_results_path`: Self-diff or pre-8.0 baseline path.
- `surface`: `compact` (required).
- `gates`: `{ H3: PASS|FAIL, H4: PASS|FAIL, H5: PASS|FAIL, ... }`.
- `h3_policy`: Reference to A-012 serve-only or two-hop policy used.
- `session_net_accepted`: Numeric headline for H4.
- `diagnostics`: Free text for failures.

**Validation rules**:

- Phase 2 exit requires H3, H4 PASS; H5 PASS recommended in same report.
- Must not claim H6/H7/H8 PASS without separate Phase 3+ evidence.

## Entity: A029SpikeRecord

**Purpose**: T2/T3 equivalence spike outcomes.

**Fields**:

- `spike_id`: e.g. `A-029-phase2-2026-06`.
- `repos_tested`: List (tokio, django, …).
- `t2_equiv_count`: Integer of 4 reference tasks.
- `verdict`: PASS | PIVOT | KILL.
- `pivot_policy`: e.g. `P-T2` bypass-only registration text if PIVOT.
- `artifact_path`: Link to detailed log or JSON.
- `validated_at`: Date.

**Validation rules**:

- PASS requires ≥2/4 T2 equivalence on compact surface.
- PIVOT requires documented H6 denominator change proposal.
- KILL requires next research action; blocks Phase 2 exit claim.

## Entity: Phase2ScopeBoundary (documentation)

**Purpose**: Explicit reject list for scope creep reviews.

**Fields**:

- `rejected_item`: e.g. `ledger_sqlite`, `b_results_closure`.
- `reason`: Phase 3 / A-024 reference.
- `review_date`: When scope was checked.

## Relationships

```text
GoldenRouteRow (36) ──► MultiStepStelPlan (L1)
MultiStepStelPlan ──► StelDecision (L2)
StelDecision ──► MultiStepExecutionResult (L3) ──► StelLedgerEvent (L4 in-memory)
BatteryRun ──► BatteryRowSTEL[] ──► Phase2GateReport
A029SpikeRecord ──► AssumptionEvidence (A-029)
```

Phase 3 adds persistence edge: `StelLedgerEvent` → durable store (out of Phase 2 model).
