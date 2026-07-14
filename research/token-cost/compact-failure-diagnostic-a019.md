# Compact failure diagnostic — A019 Checkpoint 3

Date: 2026-07-14<br>
Branch: `feat/token-speed-tool-trust`<br>
Status: independent audit `APPROVE_DIAGNOSTIC`; checkpoint closed<br>
Product changes made by this checkpoint: none

## Decision

A schema-only rescue is **not supported as sufficient** by A019. An `intent`-enum-only change directly targets the four pre-dispatch decode failures. Richer schema guidance could change future model arguments and therefore might prevent some dispatched failures, but schema alone cannot repair the source-proven lossy status mapping for identical requests.

Those thirteen calls separate cleanly:

- the primitive classifier returned `EmptyResult` 10 times and `NotFound` 3 times;
- no primitive returned `InvalidRequest`;
- the compact executor deliberately maps `EmptyResult`, `NotFound`, and `Ambiguous` to facade `InvalidRequest`;
- `InvalidRequest` is emitted as an MCP error result, so Codex records the item as failed.

The continuation-trust failure therefore has three observed mechanisms, not one:

1. **Schema use:** four invented free-form values violated the closed `IntentBucket` enum before dispatch.
2. **Planner argument construction:** thirteen syntactically valid calls reached a primitive, but eight passed broad prose as one literal/path/symbol argument, four treated field/metric-like identifiers as symbol names, and one treated a symbol name as a file path.
3. **Lossy facade status:** all thirteen truthful empty/not-found primitive outcomes became undifferentiated `invalid_request` host errors with no typed failed-step, primitive-outcome, retryability, or suggested-next-tool fields.

No product fix is authorized by this diagnostic. It narrows the hypotheses that a later rescue design must isolate.

## Evidence and definitions

- Ledger SHA-256: `D654D44C4F14D8AA0A958A4C53DF842FD161D7A20DF47FF23CB719CCAC609EC9`.
- Raw evidence: 20 event traces, 4,865,349 bytes.
- Ordered trace-set SHA-256: `CF649B80274FAB288EFA0F48E5E4E49ED34F828C0905DCFFAAFA6E823916D35C` over run-ascending `NN:UPPERCASE-SHA256:length` rows joined with CRLF and no trailing newline.
- Candidate: SymForge `8.14.1`, SHA-256 `6C4176E03299B768793ACB64012FDD95783476B6AE59662FC4AD7B8C310FFC3B`.
- Candidate build receipt used base commit `30704cf80723d4c40a0ac6bb65faf8aeaef50ea6` plus the independently approved compact annotation diff in `src/stel/surface_list.rs`. That exact prerequisite is now isolated in commit `0260760ac19e10f2f158411bf94201aaeed601e5`; the measured candidate itself was not rebuilt or substituted.
- Failure classification uses `item.completed`, `server=symforge`, `tool=symforge`, and `item.status=failed`. Raw error-string matching is forbidden because one completed run-13 search result contains the string `invalid_request` as source evidence.
- **Immediate retry** means the next completed item of any type is another `symforge` facade call.
- **Native before next SymForge** counts completed native command/file items after the failure until the next completed SymForge call or run end. It is an episode-local recovery signal, not a whole-run fallback count.
- All thirteen dispatched failed plans were single-step. Their intended sequence is therefore exactly the selected primitive shown below; “multi-hop chain failed at step 1” does not imply that another primitive was attempted.

## Sanitized call-level rows

| Run/call | Request shape | Intended primitive sequence | Normalized primitive arguments | Primitive → facade status | Immediate next action | Native before next SymForge |
|---|---|---|---|---|---|---:|
| 02/F1 | broad multi-concept query; intent omitted | `search_files` | `query=<residual prose after “files for”>` | `NotFound → InvalidRequest` | native | 1 |
| 07/F1 | broad query mentioning usage; intent omitted | `search_text` | `path_prefix=src`, `query=<broad residual prose>` | `EmptyResult → InvalidRequest` | native | 1 |
| 07/F2 | bare `ccr_retrieve`; intent omitted | `search_symbols` | `query=ccr_retrieve` | `EmptyResult → InvalidRequest` | retry with `ccr_store`; failed | 0 |
| 07/F3 | bare `ccr_store`; intent omitted | `search_symbols` | `query=ccr_store` | `EmptyResult → InvalidRequest` | native | 35 |
| 09/F1 | broad query; invented intent `repository code investigation` | none — decode failed | no primitive arguments | no result status | same query, legal `find`; failed | 0 |
| 09/F2 | broad query with legal `find` | `search_text` | `query=<whole broad prose>` | `EmptyResult → InvalidRequest` | narrow `SYMFORGE_SURFACE`; succeeded | 0 |
| 12/F1 | broad multi-concept query; intent omitted | `search_files` | `query=<whole broad prose>` | `NotFound → InvalidRequest` | native | 1 |
| 12/F2 | bare `continuation_id`; intent omitted | `search_symbols` | `query=continuation_id` | `EmptyResult → InvalidRequest` | retry with `ccr_store`; failed | 0 |
| 12/F3 | bare `ccr_store`; intent omitted | `search_symbols` | `query=ccr_store` | `EmptyResult → InvalidRequest` | narrow `symforge_retrieve`; succeeded | 0 |
| 13/F1 | broad query; invented intent `retrieve` | none — decode failed | no primitive arguments | no result status | same query, legal `trace`; failed | 0 |
| 13/F2 | broad query with legal `trace` | `find_references` | `name=<whole broad prose>`, bounded limits | `EmptyResult → InvalidRequest` | narrow `ccr_store` + `find`; succeeded | 0 |
| 16/F1 | broad query; invented intent `investigate code` | none — decode failed | no primitive arguments | no result status | same query, legal `find`; failed | 0 |
| 16/F2 | broad query with legal `find` | `search_text` | `query=<whole broad prose>` | `EmptyResult → InvalidRequest` | narrow `SYMFORGE_SURFACE`; succeeded | 0 |
| 16/F3 | bare symbol in `query` with legal `read` | `get_file_context` | `path=surface_profile_from_env` | `NotFound → InvalidRequest` | narrow `compact_surface_tools` + `find`; succeeded | 0 |
| 18/F1 | broad query mentioning usage; intent omitted | `search_text` | `path_prefix=src`, `query=<broad prose>` | `EmptyResult → InvalidRequest` | native | 50 |
| 19/F1 | broad query; invented intent `investigate` | none — decode failed | no primitive arguments | no result status | same query, legal `find`; failed | 0 |
| 19/F2 | broad query containing “symbol/function”; legal `find` | `search_symbols` | `query=<whole broad prose>` | `EmptyResult → InvalidRequest` | narrow `SYMFORGE_SURFACE`; succeeded | 0 |

Totals reproduced from the rows: 17 failures; 4 decode failures; 13 dispatched failures; 10 primitive `EmptyResult`; 3 primitive `NotFound`; 12 immediate same-facade retries; 5 failures whose immediate next item was native.

## Source-corroborated mechanism

### 1. Closed enum with weak behavioral affordance

`IntentBucket` is a closed snake-case enum (`src/stel_core/types.rs:10-22`) and `StelRequest.intent` is `Option<IntentBucket>` (`src/stel_core/types.rs:73-112`). The generated schema constrains the values, but the compact tool description only says “natural-language code intelligence” (`src/stel/surface_list.rs:37-61`) and the `intent` field has no field-level explanation or examples distinguishing it from the natural-language `query`.

Across 48 completed compact facade calls, the caller omitted `intent` 19 times, supplied legal enum values 25 times, and supplied four free-form task labels. All four free-form labels failed before dispatch. This is direct schema-legibility evidence, but it explains only 4/17 failures.

### 2. Planner routes are syntactically valid but semantically brittle

The planner prioritizes literal phrase parsers and bucket fallbacks (`src/stel/planner.rs:276-301`, `590-601`, `787-1075`):

- “files for” can turn the remaining sentence into one `search_files.query`;
- any `find ... usage` phrasing can turn a broad research sentence into one literal `search_text.query` scoped to `src`;
- explicit `find` falls back to whole-query `search_text` or `search_symbols`;
- explicit `trace` without `symbol` uses the whole query as `find_references.name`;
- explicit `read` without `path` uses the whole query as `get_file_context.path`.

The four identifier misses are not malformed primitive calls. They are correct empty symbol searches for identifiers that are not indexed symbol declarations. The traces prove route modality matters: `ccr_store` fails as omitted-intent `search_symbols` but succeeds as explicit-find `search_text`; `symforge_retrieve` succeeds as a real indexed symbol. The broad-query failures likewise use valid schemas but poor search operands.

### 3. Primitive truth is collapsed into a host error

Primitive classifiers explicitly distinguish these cases (`src/protocol/tools.rs:246-335`):

- `No symbols matching` and `No matches` → `EmptyResult`;
- missing source paths and files → `NotFound`;
- the observed malformed-input prefixes would → `InvalidRequest`, but none occurred in these thirteen calls.

The executor then maps `NotFound`, `Ambiguous`, and `EmptyResult` to `InvalidRequest` in `serve_chain_outcome_class` (`src/stel/executor.rs:329-338`) and records a reject decision (`src/stel/executor.rs:341-360`). `ResultStatus::into_call_tool_result` emits only `InvalidRequest` and `InternalFailure` as MCP error results (`src/protocol/result_status.rs:10-17`, `40-42`, `121-143`). That exact remapping explains why all thirteen items are host-visible failures despite truthful primitive outcomes.

## Recovery and success anatomy

The twelve immediate retries split evenly:

- six immediately failed again;
- six immediately succeeded after narrowing to an exact code identifier and/or selecting a better modality.

The six successful recoveries were:

| Recovery | Working primitive | Why it found evidence |
|---|---|---|
| runs 09, 16, 19: `SYMFORGE_SURFACE` + `find` | `search_text` | exact source token exists as text |
| run 12: `symforge_retrieve` | `search_symbols` | exact indexed function symbol exists |
| run 13: `ccr_store` + `find` | `search_text` | token exists in source/metrics though not as the searched symbol |
| run 16: `compact_surface_tools` + `find` | `search_text` | exact source identifier exists as text |

There were 31 successful compact facade calls, all with typed result status `found`. Twenty-two were not followed immediately by a native command; nine were. A found result therefore improves immediate continuation but does not guarantee the model will avoid native reads later, especially for exact citation work.

## Answers to the checkpoint questions

1. **Did enum descriptions/examples constrain `intent`?** No. The schema enumerated legal values, but 4/29 calls that set `intent` invented task-language labels. There are no field-level examples or explanation. This is real but minority failure evidence.
2. **Did the planner construct invalid leaf arguments from broad questions?** It constructed schema-valid arguments, not primitive `InvalidRequest`s. Eight were semantically poor broad-prose operands, four were symbol searches for non-symbol identifiers, and one put a symbol in the path position.
3. **Did valid empty/not-found leaf responses become invalid requests?** Yes, mechanically and in all thirteen dispatched failures. Ten `EmptyResult` and three `NotFound` outcomes were deliberately collapsed to facade `InvalidRequest`.
4. **Did the envelope provide typed recovery guidance?** No. It preserved human-readable primitive feedback, often with useful suggestions, but the structured contract exposed only `outcome_class=invalid_request`; `ResultStatus` has no failed-step, primitive-outcome, retryable, or suggested-next fields.
5. **Which successes avoided immediate native fallback?** Twenty-two of 31 successful facade calls did. The clearest failure-linked recoveries used exact identifiers with a primitive that matched whether the identifier was a symbol or source text. Route modality, not only query length, determined success.

## Fact-backed next gate

Do not design a schema-only rescue as the sole rescue arm. Before product changes:

1. treat the independent reproduction as complete: `research/token-cost/claude-opus-report-checkpoint-3-a019.md` returned `APPROVE_DIAGNOSTIC` after reproducing every row, aggregate, hash, and source mapping;
2. repair and blind-validate the S1/S2 oracles and citation rule in a new manifest;
3. treat the annotation prerequisite as committed in isolated commit `0260760ac19e10f2f158411bf94201aaeed601e5`; build any confirmatory candidate from that committed source;
4. design separable rescue factors rather than one bundled compact change:
   - schema legibility/omission behavior for `intent`,
   - planner decomposition and `query`/`symbol`/`path` modality,
   - preservation of primitive `EmptyResult`/`NotFound` in structured status,
   - typed recovery metadata and suggested next primitive;
5. keep deferred discovery/catalog arms identified as prior-reconnaissance hypotheses, not A019-derived winners.

The audit's two non-blocking custody notes are resolved in this record. The trace-set hash recipe is now byte-exact. The current A019 pre-receipt wiring quarantine was deleted after the post-run-20 audit; two legacy wiring-probe bundles remain intentionally retained under the older pre-restart invalidated-evidence tree required by Amendment A. Those bundles are historical evidence, not active run state. After the approved source diff was committed and this audit closed, the exact-path pinned candidate directory was deleted with zero live holders, freeing 60,908,544 bytes; primary raw traces remain retained.
