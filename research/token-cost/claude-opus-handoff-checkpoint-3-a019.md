# Independent Checkpoint-3 audit handoff — A019 compact failures

You are the independent reviewer for SymForge A019 Checkpoint 3. Work read-only except for the single report file named below. Do not modify product source, the harness, the ledger, raw traces, grader answers, prior reports, task files, or Git state. Do not run a new measured model session and do not re-grade any frozen A019 answer.

## Objective

Independently determine whether the primary agent's 17-call compact-failure diagnostic is complete, mechanically reproducible, and sufficient to gate rescue-design work without authorizing a product fix.

Write your report to exactly:

`research/token-cost/claude-opus-report-checkpoint-3-a019.md`

## Required order — preserve independence

### Phase 1: raw-trace extraction before reading the primary diagnostic

Read only:

- `research/token-cost/evidence/token-surface-shakedown-a019/shakedown-results.jsonl`
- the 20 `raw_trace` files named by those records
- `research/token-cost/token-speed-tool-trust-benchmark-manifest-2026-07-13.md` only as needed for run/task identity

Before opening `research/token-cost/compact-failure-diagnostic-a019.md`, privately build your own row for every completed compact `symforge` facade item with `status=failed`. For each row record:

- run and within-run failure ordinal;
- caller argument-key shape and whether `intent` was omitted, legal, or free-form;
- whether decode failed before dispatch;
- planner-selected primitive sequence and constructed primitive arguments, normalized so no home path or raw command is reproduced;
- primitive outcome (`Found`, `EmptyResult`, `NotFound`, `Ambiguous`, `InvalidRequest`, `InternalFailure`);
- served `symforge/result_status`, item status, and whether either `error_class` or `retryable` exists;
- whether the next completed item is the same facade;
- if it is, the argument delta and retry result;
- completed native command/file items before the next completed SymForge call or run end. Count them, but never reproduce command strings.

Classify by structured item status. Do not count failures by searching raw text for `invalid_request`; one completed run-13 result contains that string as source evidence.

### Phase 2: source mechanism

Use SymForge for Rust inspection. Verify against the candidate source tree and the only authorized product diff:

- `src/stel_core/types.rs`: `IntentBucket`, `StelRequest`
- `src/stel/surface_list.rs`: `compact_surface_tools`
- `src/stel/planner.rs`: `build_plan`, `plan_step`, phrase routing, `route_find`, `route_read`, `route_trace`, argument construction
- `src/protocol/tools.rs`: primitive output classifiers and `symforge_stel_handler`
- `src/stel/executor.rs`: `serve_chain_outcome_class`, `chain_failure_decision`
- `src/protocol/result_status.rs`: `OutcomeClass`, `is_error`, `into_call_tool_result`

Answer independently:

1. How many failures occur before facade dispatch because `IntentBucket` rejects a free-form value?
2. For every dispatched failure, what primitive outcome was produced before facade mapping?
3. Did any primitive classify its observed output as `InvalidRequest`?
4. Which exact layer converts empty/not-found/ambiguous primitive outcomes into facade `InvalidRequest`, and does that conversion make the MCP result an error item?
5. Are the primitive arguments malformed, or schema-valid but semantically mis-armed? Separate broad prose, identifier modality, and query/symbol/path placement.
6. Does the compact schema/description contain enough field-level explanation or examples to distinguish natural-language `query` from closed `intent` and optional `symbol`/`path`?

### Phase 3: compare with the primary diagnostic

Only now read:

- `research/token-cost/compact-failure-diagnostic-a019.md`
- `research/token-cost/token-surface-shakedown-report-a019.md`
- `research/token-cost/claude-opus-report-post-run-20-a019.md`

Compare your private table row-for-row. Reproduce or refute these claimed totals:

- 17 failed compact facade calls;
- 4 pre-dispatch enum-decode failures;
- 13 dispatched failures;
- primitive outcomes: 10 `EmptyResult`, 3 `NotFound`, 0 primitive `InvalidRequest`;
- 12 immediate same-facade retries, split into 6 next-failure and 6 next-success;
- 5 failures whose immediate next completed item is native;
- 31 successful compact facade calls, all typed `found`; 22 not immediately followed by native and 9 immediately followed by native.

Verify every sanitized row's selected primitive, normalized argument disposition, primitive-to-facade status mapping, retry result, and native-before-next-SymForge count.

### Phase 4: challenge the decision

Determine whether the evidence supports each statement:

- an `intent`-enum-only change directly targets four failures; richer schema might alter future caller arguments, but schema alone cannot repair the observed internal status mapping for identical requests;
- the planner produced schema-valid but often semantically poor primitive operands rather than primitive malformed-input errors;
- the facade's lossy status mapping is a separate, source-proven mechanism from planner quality;
- human-readable suggestions do not substitute for typed recovery metadata;
- no product fix is yet authorized, and future rescue factors should be separable rather than bundled.

Identify any overclaim, missing confound, wrong source attribution, counting ambiguity, or sanitization/custody risk. Distinguish a direct observation from an inference or design recommendation.

### Phase 5: hygiene and scope

Verify live state without deleting anything:

- both golden-state directories and the raw-evidence wiring quarantine are absent;
- all 20 raw traces, the small pre-restart invalidated traces, compact in-repo evidence, and pinned candidate remain;
- no candidate process, fixture worktree, isolated run home, repository Cargo target, or disposable campaign Cargo target remains;
- the product diff is still only the approved compact annotation plus its regression test.

Never print a secret or native command string. Report diagnostic counts and file locations only.

## Report requirements

Your report must include:

1. independent 17-row comparison or a precise discrepancy table;
2. independently reproduced aggregate counts;
3. source-backed root-mechanism verdict;
4. assessment of whether schema-only rescue is unsupported as sufficient, without overclaiming that richer schema could never change caller behavior;
5. any required corrections ranked by severity;
6. cleanup/custody verification;
7. one exact final verdict line:
   - `VERDICT: APPROVE_DIAGNOSTIC`
   - or `VERDICT: CHANGES_REQUIRED`

If you choose `CHANGES_REQUIRED`, state the smallest exact correction and whether it blocks confirmatory-design work. Do not implement it.
