# Token Surface Shakedown Report — A019

Date: 2026-07-13<br>
Branch: `feat/token-speed-tool-trust`<br>
Status: shakedown closed; independent post-run-20 verdict `APPROVE_SHAKEDOWN_CLOSURE`

## Decision

The 20-run shakedown is mechanically complete but cannot support a successful-task token or speed comparison: every run failed at least one frozen oracle criterion. No run was excluded, timed out, changed the fixture, or lacked usage/tool evidence.

This is still useful reconnaissance:

- Amendment A made compact `symforge` callable, and every run made at least one substantive SymForge call.
- Initial routing was relevant in every run.
- Compact execution then produced 17 failed calls, 12 immediate retries across the two compact cells, and 232 native read/search fallbacks.
- Full execution produced no SymForge call errors, but it still used 65 native read/search fallbacks and made 219 completed SymForge calls.
- The frozen task oracles are not suitable for a confirmatory pilot. S1 requires a probe-only constructor instead of the production compact constructor; S2 requires an incidental implementation-order statement that the prompt does not naturally solicit.

Therefore:

1. preserve the frozen grades and do not re-grade this series;
2. do not calculate a headline saving or make a causal claim;
3. redesign and independently validate both task oracles before power planning;
4. diagnose the compact invalid-request paths before deciding whether the compact facade merits a rescue experiment;
5. carry provider/host deferred discovery over the semantic leaf catalog forward as a prior-reconnaissance design hypothesis, not as an A019-derived ranking.

## Evidence integrity and custody

- Exactly 20 unique records exist for run IDs 01–20.
- Every record has `exit_code=0`, `timed_out=false`, one `turn.completed` usage event, semantic readiness `ready`, and zero snapshot verification mismatches.
- Every record used the pinned `8.14.1` candidate with SHA-256 `6C4176E03299B768793ACB64012FDD95783476B6AE59662FC4AD7B8C310FFC3B`.
- The fixed source tree, 851-file source manifest, repository-outline digest, and 726/720/4/2/21,830 index counts match the no-overwrite semantic baseline in every run.
- Configuration diagnostics: 0. Potential secret-bearing lines: 0. Repository changes caused by runs: 0.
- Oracle grades: 20 written exactly once; 0 passes, 20 fails, 0 exclusions; every failure names at least one criterion.
- After capture: no fixture worktree, candidate process, isolated Codex home, repository Cargo target, or disposable Cargo target remained. Git listed only the main worktree.
- Free space after capture and grading: C: 116.18 GiB; E: 10.29 GiB.
- Independent review approved closure. The two golden states and the current A019 pre-receipt wiring quarantine were then deleted after path, live-process, and Git-worktree guards, freeing 34,539,032 bytes. Raw traces, the small pre-restart invalidated evidence (including its two historical wiring-probe bundles), compact records, and the pinned candidate remained retained through Checkpoint 3.
- Retained sizes before the approved cleanup were bounded: golden a019 state 16.46 MiB; the complete raw-evidence directory was 5,051,709 bytes (4.818 MiB), of which the 20 run event traces are 4,865,349 bytes (4.640 MiB); compact evidence is 0.37 MiB; and the pinned candidate is 58.09 MiB.
- After `APPROVE_DIAGNOSTIC` and isolated commit `0260760ac19e10f2f158411bf94201aaeed601e5`, the exact pinned-candidate directory was removed with zero exact-path process holders, freeing another 60,908,544 bytes. The 20 primary traces and the small pre-restart invalidated evidence remain retained.
- Post-series Amendment C is harness-test-only: the final verification exposed a `SelfTest` assertion that hardcoded run 20 as a missing artifact. `Get-CompactRunArtifactPaths` now accepts an optional root whose default remains the real evidence root, while the assertion supplies a unique nonexistent temporary root. No measured-run, rerun-protection, grade, or evidence path changed; the full harness `SelfTest` passes after the correction.

`tool_event_count` is not a call count: it contains both started and completed events. All call figures below use `symforge_call_count`, which counts completed calls to the `symforge` server and excludes host resource calls. Run 06 is the worked example: three MCP calls completed, but two were Codex-host resource calls, so `symforge_call_count=1`.

## Independent closure

The post-run-20 Opus audit independently reparsed all 20 records and raw traces, blind-reproduced the 0/20 grades, reproduced every reported statistic, confirmed the 17 compact failures and zero full-surface failures, and found no custody or product-scope blocker. Its verdict is `APPROVE_SHAKEDOWN_CLOSURE`. The annotation prerequisite is now isolated in commit `0260760ac19e10f2f158411bf94201aaeed601e5`, and the call-level diagnostic independently closed with `APPROVE_DIAGNOSTIC`. The remaining pre-pilot truth gates are repaired S1/S2 oracles, symbol-identity-pinned citations, and a separable confirmatory design.

## Frozen-oracle result

All four cells scored 0/5. This does not mean all answers were factually poor.

### S1 failure pattern

All ten S1 answers reported production behavior: compact `tools/list` uses `compact_surface_tools`. The frozen oracle instead requires `compact_probe_tools` and a citation inside its probe-only range. Consequently all ten failed S1 criteria 4 and 5 even when the answer was production-truthful.

Eight S1 answers also omitted the explicit statement that the literal `compact` and `meta` environment values select their corresponding profiles, failing criterion 2. Runs 01 and 14 expressed that mapping sufficiently.

The run-01 independent audit already identified the production/probe mismatch. The same mismatch repeated deterministically; it was not repaired or re-graded mid-series.

### S2 failure pattern

All ten S2 answers correctly described the five budgets, overflow/store branches, retrieval handle, validation, stored-content return, and counter changes. None explicitly stated that the counters are incremented *before* the stored string is cloned and returned, so all ten failed frozen criterion 5.

Seven S2 answers also failed criterion 6 because at least one citation was one or two lines outside a frozen symbol range: runs 02, 04, 07, 10, 12, 18, and 20. Runs 05, 13, and 15 satisfied the frozen citation ranges.

Two isolated blind graders independently reproduced these failure classes from redacted answers only. They disagreed only on S1 criterion 2 for run 14; the final grade accepts its “resolves Compact, Meta, or default Full” wording as equivalent, so run 14 fails criteria 4 and 5 only.

## Per-cell descriptive results

Token totals are canonical host totals: `input_tokens + output_tokens` from the single usage event. Cached input is an informational subset and is not added again. Values are in predetermined run order, not sorted order.

| Task / arm | Runs | Passes | Token totals | Median | Range | MAD |
|---|---|---:|---|---:|---:|---:|
| S1 / A-full | 01, 08, 11, 14, 17 | 0/5 | 240,645; 546,208; 476,423; 502,022; 395,056 | 476,423 | 240,645–546,208 | 69,785 |
| S1 / C-compact | 03, 06, 09, 16, 19 | 0/5 | 427,308; 503,993; 312,490; 314,223; 249,740 | 314,223 | 249,740–503,993 | 64,483 |
| S2 / A-full | 04, 05, 10, 15, 20 | 0/5 | 1,264,600; 925,568; 819,569; 1,071,433; 706,929 | 925,568 | 706,929–1,264,600 | 145,865 |
| S2 / C-compact | 02, 07, 12, 13, 18 | 0/5 | 2,592,199; 1,038,580; 1,278,192; 887,334; 855,726 | 1,038,580 | 855,726–2,592,199 | 182,854 |

Model-session wall time excludes per-run snapshot materialization/readiness. Seconds are shown in predetermined run order.

| Task / arm | Wall times (s) | Median (s) | Range (s) | MAD (s) |
|---|---|---:|---:|---:|
| S1 / A-full | 76.364; 111.575; 102.695; 100.483; 82.284 | 100.483 | 76.364–111.575 | 11.092 |
| S1 / C-compact | 110.534; 201.588; 90.778; 79.779; 80.629 | 90.778 | 79.779–201.588 | 10.999 |
| S2 / A-full | 220.535; 193.226; 217.209; 239.206; 189.425 | 217.209 | 189.425–239.206 | 21.997 |
| S2 / C-compact | 361.496; 202.462; 308.411; 231.422; 232.785 | 232.785 | 202.462–361.496 | 30.323 |

### Route and trust metrics

All completed native commands matched the predeclared read/search fallback classifiers (`rg`, `Get-Content`, or `Select-String`); no native build/test command occurred. “Immediate retry” means the next completed tool event after a failed SymForge call was another call to the same facade.

| Task / arm | Zero-call runs | First tool correct | SymForge calls (success / error) | Native fallback (runs / commands) | Immediate retries | Citation-error runs |
|---|---:|---:|---:|---:|---:|---:|
| S1 / A-full | 0/5 | 5/5 | 43 (43 / 0) | 5/5 / 45 | 0 | 5/5 |
| S1 / C-compact | 0/5 | 5/5 | 21 (14 / 7) | 5/5 / 46 | 7 | 5/5 |
| S2 / A-full | 0/5 | 5/5 | 176 (176 / 0) | 5/5 / 20 | 0 | 3/5 |
| S2 / C-compact | 0/5 | 5/5 | 28 (18 / 10) | 5/5 / 186 | 5 | 4/5 |

The first substantive actions were `search_symbols`/`search_text` on full, `symforge` on compact, and one relevant `symforge://repo/map` resource read. Initial adoption was therefore not the compact arm's main problem. Continuation quality and trust were.

## Predetermined run-order receipt

| Run | Task / arm | Tokens | Wall (s) | SymForge success / error | Native fallback |
|---:|---|---:|---:|---:|---:|
| 01 | S1 / A-full | 240,645 | 76.364 | 8 / 0 | 2 |
| 02 | S2 / C-compact | 2,592,199 | 361.496 | 3 / 1 | 29 |
| 03 | S1 / C-compact | 427,308 | 110.534 | 5 / 0 | 11 |
| 04 | S2 / A-full | 1,264,600 | 220.535 | 34 / 0 | 8 |
| 05 | S2 / A-full | 925,568 | 193.226 | 25 / 0 | 3 |
| 06 | S1 / C-compact | 503,993 | 201.588 | 1 / 0 | 17 |
| 07 | S2 / C-compact | 1,038,580 | 202.462 | 3 / 3 | 37 |
| 08 | S1 / A-full | 546,208 | 111.575 | 10 / 0 | 9 |
| 09 | S1 / C-compact | 312,490 | 90.778 | 3 / 2 | 8 |
| 10 | S2 / A-full | 819,569 | 217.209 | 42 / 0 | 4 |
| 11 | S1 / A-full | 476,423 | 102.695 | 9 / 0 | 16 |
| 12 | S2 / C-compact | 1,278,192 | 308.411 | 2 / 3 | 57 |
| 13 | S2 / C-compact | 887,334 | 231.422 | 8 / 2 | 13 |
| 14 | S1 / A-full | 502,022 | 100.483 | 9 / 0 | 13 |
| 15 | S2 / A-full | 1,071,433 | 239.206 | 39 / 0 | 4 |
| 16 | S1 / C-compact | 314,223 | 79.779 | 3 / 3 | 5 |
| 17 | S1 / A-full | 395,056 | 82.284 | 7 / 0 | 5 |
| 18 | S2 / C-compact | 855,726 | 232.785 | 2 / 1 | 50 |
| 19 | S1 / C-compact | 249,740 | 80.629 | 2 / 2 | 5 |
| 20 | S2 / A-full | 706,929 | 189.425 | 36 / 0 | 1 |

Run order exposes large within-cell variance, especially run 02. There is no defensible monotonic warm-cache or drift conclusion from five interleaved observations per cell.

## Compact error anatomy

All 17 SymForge call errors occurred on compact; full had zero.

- Four calls failed during input decoding because the model supplied descriptive free-form `intent` strings where the schema expects the closed `IntentBucket` enum. These failures had no `symforge/result_status` metadata because dispatch decoding failed before the facade ran. The enum is defined at `src/stel_core/types.rs:13-22`.
- Thirteen facade calls returned structured `outcome_class=invalid_request` from routed search/read execution. Their selected leaves were `search_files`, `search_text`, `search_symbols`, `find_references`, and `get_file_context`; `explore` appeared only in alternative-route text on five failures. The envelopes exposed neither `error_class` nor `retryable` metadata. One additional completed call contained the `invalid_request` string, so diagnostic tooling must classify structured item status rather than raw text.
- Twelve failures were followed immediately by another compact call across the two compact cells. The remaining failures commonly triggered native fallback instead.

This separates two product questions:

1. **Schema legibility:** models invented task-language intent values despite a typed enum.
2. **Dispatcher recovery/trust:** planned leaf execution failed as `invalid_request` without typed recovery fields, encouraging expensive native fallback.

The raw trace must be inspected call-by-call before attributing the thirteen routed failures to planner selection, argument construction, leaf-result classification, or result-envelope policy.

## What the shakedown does and does not establish

Established:

- The amended compact annotation fixes host willingness to call the read facade.
- Compact initial adoption can be high while downstream tool trust remains poor.
- For S2, compact produced far more native fallback and some higher raw token/wall observations despite exposing much less schema.
- Full semantic tools were error-free in this sample, but full still used native fallback and high call volume.
- The current frozen oracles are too brittle or production-inaccurate for confirmatory success gating.
- Variance is measurable, but there are no successful-task observations from which to size a success-conditioned efficiency pilot.

Not established:

- No arm is a token, speed, or quality winner.
- Raw token differences are not successful-task savings.
- The compact facade itself is not proven irredeemable.
- The 93.7% compact schema-byte reduction does not predict end-to-end savings.
- No causal surface effect can be isolated because the treatment changes both catalog/schema exposure and compact routing behavior.

## Fact-backed next plan — no rescue changes yet

### Checkpoint 1: independent post-run-20 audit — complete

The independent reviewer reparsed the 20 raw traces, reproduced the grades and four-cell statistics, verified cleanup/custody, and challenged the compact error classification. The resulting report approved closure; its recommended golden-state and current A019 wiring-quarantine cleanup is complete. The older invalidated-evidence tree intentionally retains two historical wiring-probe bundles required by Amendment A; they are not live run state.

### Checkpoint 2: repair experimental truth before a pilot

Write a new, versioned manifest; never mutate or re-grade A019.

- S1 must ask for and grade production `ServerHandler::list_tools` → `compact_surface_tools`. Keep probe symbols only in an explicitly probe-focused task.
- S2 should grade the counter changes and stored-content return. If internal mutation-before-clone order is important, ask for that order explicitly; otherwise remove it as an incidental wording trap.
- Pin symbol identity plus candidate commit/tree. Avoid making correctness hinge on a one-line citation drift when the cited symbol and source content are correct.
- Blind-replay revised oracles against saved answers as a diagnostic only. Require two independent graders to agree before freezing new tasks.
- The truthful compact annotation prerequisite is committed at `0260760ac19e10f2f158411bf94201aaeed601e5`; build the confirmatory candidate from that committed source so binary and fixture source share custody.

### Checkpoint 3: diagnose compact failures without changing code — complete

`research/token-cost/compact-failure-diagnostic-a019.md` persists all 17 sanitized rows and answers the five mechanism questions. Four calls failed closed-enum decoding before dispatch; the other thirteen reached a leaf and returned 10 `EmptyResult` plus 3 `NotFound` outcomes that the executor collapsed to facade `InvalidRequest`. The independent report `research/token-cost/claude-opus-report-checkpoint-3-a019.md` reproduced every row, aggregate, hash, and source mapping and returned `APPROVE_DIAGNOSTIC`. The result rules out a schema-only change as a sufficient rescue; it does not yet authorize a product fix.

### Checkpoint 4: redesign the confirmatory arms

The prior reconnaissance—not A019—supplies this provisional design order:

1. canonical 36-tool semantic catalog;
2. the same catalog with provider/host deferred discovery when supported;
3. a client-neutral allowlist of observed semantic leaf tools for hosts without deferred discovery;
4. compact-3 only as a separate rescue arm after its invalid-request mechanisms are understood.

Hold task, output semantics, model, host, candidate, repository state, and readiness constant. Measure successful-task-normalized tokens, wall time, first-tool accuracy, invalid arguments, typed errors, immediate retries, native fallback, redundant reads, and citation correctness.

Power planning begins only after revised tasks produce equivalent successful outcomes in the comparison arms. The A019 raw variance may inform timeout and storage planning, but not a success-conditioned sample-size estimate.

## Evidence index

- Frozen manifest: `research/token-cost/token-speed-tool-trust-benchmark-manifest-2026-07-13.md`
- Harness: `research/token-cost/run-token-surface-shakedown.ps1`
- Compact records and redacted grader answers: `research/token-cost/evidence/token-surface-shakedown-a019/`
- Semantic baseline: `research/token-cost/evidence/token-surface-shakedown-a019/semantic-baseline.json`
- Independent run-01 report: `research/token-cost/claude-opus-report-post-run-01-a019.md`
- Raw traces: paths recorded per run in `shakedown-results.jsonl`; retained outside the repository as primary A019 evidence
- Product prerequisite: `src/stel/surface_list.rs`, isolated in commit `0260760ac19e10f2f158411bf94201aaeed601e5`
- Independent post-run-20 report: `research/token-cost/claude-opus-report-post-run-20-a019.md`
- Independent Checkpoint-3 report: `research/token-cost/claude-opus-report-checkpoint-3-a019.md`
