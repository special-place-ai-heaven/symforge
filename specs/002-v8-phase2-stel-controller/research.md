# Research: SymForge v8 Phase 2 STEL Controller Maturity

## Decision: Phase 2 starts from Phase 1 `main`, not from feature branch history

**Rationale**: Phase 1 compact-3 is shipped and CI-green on `main` (`66742f1` repair tip; see current [`phase1-stel-checkpoint.md`](../../docs/phase1-stel-checkpoint.md)). Phase 2 planning and implementation branch from `main`, not from stale `v8/stel-architecture`-only state.

**Alternatives considered**: Continuing only on long-lived `v8/stel-architecture` was rejected because PR #300 merged; `main` is authoritative.

## Decision: First implementation slice is multi-hop golden closure

**Rationale**: Three rows are the only explicit Phase 1 golden deferrals. Closing them is the smallest honest product increment and enables full 36-row H2 trajectory claims before battery investment.

**Alternatives considered**: Starting with L2 hardening alone was rejected because golden replay would still report deferred multi-hop, blocking honest Phase 2 status.

## Decision: Multi-step execution stays in-process inside one `symforge` MCP call

**Rationale**: H5 requires external MCP calls ≤ 1 for `chain=single`; multi-hop golden rows have `chain=multi` but still one host call to compact facade. Internal step dispatch matches Phase 3 H5 proof direction without new tools.

**Alternatives considered**: Exposing each step as separate MCP tools would break compact-3 economics and H5.

## Decision: L2 admission states match stel-schema normative set

**Rationale**: `serve | degrade | bypass | cache_hit` are already specified in [`stel-schema.md`](../../docs/stel-schema.md). Phase 1 implements economics + P-FF bypass; Phase 2 completes cache_hit and degrade paths and non-P-FF bypass honesty.

**Alternatives considered**: Adding new decision enums (e.g. `reject`) was rejected — use `InvalidRequest` at L1 validation or bypass with explicit body instead.

## Decision: H3 uses A-012 interim serve-only scope unless two-hop harness ships in Phase 2

**Rationale**: Phase 0 documented serve-only H3 for bypass rows ([`A-012-bypass-policy.md`](../../docs/research/A-012-bypass-policy.md)). Phase 2 must not claim contradictory bypass accounting.

**Alternatives considered**: Implementing full two-hop harness in Phase 2 is optional stretch; if not done, H3 scope documentation must remain serve-only.

## Decision: A-029 spike is mandatory evidence, not optional research

**Rationale**: Binding gap plan lists A-029 under Phase 2 assumption dependency DAG. Spike must produce PASS, P-T2 pivot, or KILL — same pass/pivot/kill discipline as Phase 0.

**Alternatives considered**: Deferring A-029 to Phase 4 was rejected because stel-assumptions blocks Phase 2 narrative without spike artifact.

## Decision: Calibration persistence and B-RESULTS are Phase 3 / post-8.0 boundaries

**Rationale**: Phase 1 checkpoint and gap plan place durable ledger + EMA (S7) in Phase 3 and baseline pin (A-024) at 8.0 tag. Phase 2 observational calibration remains read-only from in-memory ledger.

**Alternatives considered**: "Just SQLite for ledger" in Phase 2 was rejected — violates explicit Phase 1 deferral and expands blast radius before H3/H4 proof.

## Decision: Battery evidence uses compare-results H3/H4/H5 fields on STEL-extended rows

**Rationale**: Gap plan §5.1 defines gate computation; stel-schema defines row extension. Phase 2 exit is compare-results PASS on candidate vs self-diff or pre-8.0 baseline — not RESULTS §8.7 closure.

**Alternatives considered**: Custom ad-hoc metrics were rejected — gates must match sf-bench vocabulary for reviewer portability.

## Open Questions (resolve during implementation, not block spec)

1. **Corpus pinning for multi-hop rows**: `is-plain/multi_files_content` may need fixture corpus under `tests/fixtures/` if cfg-if/records corpora insufficient.
2. **Degrade defaults for T3 large**: Exact `max_tokens` / section caps require battery tuning (A-014).
3. **sf-bench availability**: If external sf-bench path unavailable in CI, battery evidence is operator-triggered with local path recorded in gate report (same pattern as SYMFORGE_CALIBRATION_REPOS).

## References

- [`docs/phase1-stel-checkpoint.md`](../../docs/phase1-stel-checkpoint.md) — Phase 1 exit and deferrals
- [`docs/v8-gap-closure-plan.md`](../../docs/v8-gap-closure-plan.md) §4.2, §5.1, §7 Phase 2
- [`docs/stel-schema.md`](../../docs/stel-schema.md) — S5, S6, controller algorithm
- [`src/stel/golden_replay.rs`](../../src/stel/golden_replay.rs) — `DEFERRED_MULTI_HOP_ROW_IDS`
