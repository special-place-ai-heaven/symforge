# Claude Opus independent Checkpoint-3 audit — A019 compact failures

Reviewer: independent checkpoint session (Fable 5), 2026-07-14.
Scope: read-only audit per `research/token-cost/claude-opus-handoff-checkpoint-3-a019.md`.
Only this report was written. No product source, harness, ledger, raw trace, grader
answer, prior report, task file, or Git state was modified. No measured model session
was run and no frozen grade was touched.

Independence order followed as required: my 17-row table was built from
`shakedown-results.jsonl` and the 20 raw traces (structured `item.completed` /
`server=symforge` / `tool=symforge` / `status=failed` classification, never raw
error-string matching) **before** opening `compact-failure-diagnostic-a019.md`,
the shakedown report, or the post-run-20 report. Source verification used
SymForge against the candidate tree plus the single authorized product diff.

## 1. Independent 17-row comparison

I extracted the failed compact facade calls from the 10 compact-arm traces
(runs 02, 03, 06, 07, 09, 12, 13, 16, 18, 19) and compared row-for-row against
the primary table. **All 17 rows match on every audited attribute**: run and
within-run ordinal, caller argument-key shape, `intent` disposition
(omitted / legal / free-form), pre-dispatch decode vs dispatched, planner-selected
primitive, normalized primitive-argument disposition, primitive outcome,
served `symforge/result_status`, immediate-next-action classification,
retry argument delta and retry result, and native-before-next-SymForge count.

Row-level confirmations worth stating explicitly:

- 02/F1 `search_files` NotFound; 12/F1 `search_files` NotFound — broad prose as
  one path-search query, next item native (1 each).
- 07/F1 and 18/F1 `search_text` with `path_prefix` + broad prose → EmptyResult;
  next item native (native-before-next-SymForge 1 and 50 respectively).
- 07/F2→F3 and 12/F2→F3 bare identifiers (`ccr_retrieve`, `ccr_store`,
  `continuation_id`) as `search_symbols` queries → EmptyResult; 07/F3 was
  followed by 35 native items before the next SymForge call.
- 09/F1, 13/F1, 16/F1, 19/F1: free-form `intent` values
  (`repository code investigation`, `retrieve`, `investigate code`,
  `investigate`) rejected by serde before dispatch — the trace text is the
  literal `failed to deserialize parameters: unknown variant …` message and the
  item carries **no** `symforge/result_status` meta; each was immediately
  retried with the same query and a legal enum value, and each retry failed.
- 13/F2 `find_references` with the whole broad sentence as `name` plus bounded
  `compact`/`limit`/`max_per_file` args → EmptyResult.
- 16/F3 `get_file_context` with `path=surface_profile_from_env` (a symbol name
  in the path position) → NotFound.
- All six successful recoveries verified in the raw traces with the exact
  narrowed operands the diagnostic reports: `SYMFORGE_SURFACE`+`find`
  (runs 09, 16, 19), `symforge_retrieve` (run 12, intent omitted),
  `ccr_store`+`find` (run 13), `compact_surface_tools`+`find` (run 16).

No discrepancy table is needed: zero discrepancies.

## 2. Independently reproduced aggregate counts

All claimed totals reproduce exactly from my private table:

| Claim | Independent count | Match |
|---|---|---|
| Failed compact facade calls | 17 | ✓ |
| Pre-dispatch enum-decode failures | 4 (runs 09, 13, 16, 19, each F1) | ✓ |
| Dispatched failures | 13 | ✓ |
| Primitive outcomes | 10 EmptyResult, 3 NotFound, 0 InvalidRequest | ✓ |
| Immediate same-facade retries | 12 = 6 next-failure + 6 next-success | ✓ |
| Failures whose next completed item is native | 5 (02/F1, 07/F1, 07/F3, 12/F1, 18/F1) | ✓ |
| Successful compact facade calls | 31, all `outcome_class=found` | ✓ |
| Successes: 22 not immediately followed by native, 9 followed | 22 / 9 | ✓ |
| Argument-disposition split of the 13 dispatched | 8 broad prose, 4 identifier-as-symbol, 1 symbol-as-path | ✓ |
| `intent` across 48 completed compact calls | omitted 19, legal 25, free-form 4 | ✓ |

Note the "followed by native" definitions differ between the failed and success
sides exactly as the diagnostic defines them (next completed item of any type
vs `command_execution`); both reproduce only under those stated definitions —
the definitions section of the diagnostic is accurate and necessary.

Custody figures reproduced: ledger SHA-256
`D654D44C4F14D8AA0A958A4C53DF842FD161D7A20DF47FF23CB719CCAC609EC9` ✓;
20 traces totalling 4,865,349 bytes ✓; ordered trace-set SHA-256
`CF649B80…6D35C` ✓ (reproduced as `NN:UPPERCASE-SHA256:length` rows joined by
CRLF with no trailing newline — the diagnostic says only "ordered
`run:sha256:length` rows"; see correction M1); pinned candidate binary present
at its receipt path with matching SHA-256 `6C4176E0…FC3B` ✓.

## 3. Source-backed root-mechanism verdict

Verified against the candidate tree (`30704cf8…f50ea6` + the approved
`src/stel/surface_list.rs` annotation diff), answering the six Phase-2 questions:

1. **Pre-dispatch failures:** exactly 4. `IntentBucket`
   (`src/stel_core/types.rs:13-22`) is a closed 8-variant enum;
   `StelRequest.intent` is `Option<IntentBucket>`. Free-form values fail rmcp
   parameter deserialization before `symforge_stel_handler` runs, which is why
   those four items carry no `symforge/result_status` meta.
2. **Primitive outcomes before facade mapping:** 10 EmptyResult + 3 NotFound,
   read from the served step diagnostics and consistent with the primitive
   classifiers.
3. **Primitive `InvalidRequest`:** none. The classifiers
   (`src/protocol/tools.rs:246-335`) reserve `InvalidRequest` for malformed-input
   prefixes (`…requires`, `Invalid regex`, etc.); none of the 13 outputs matched
   those, and each matched an empty/not-found prefix instead.
4. **The lossy layer:** `serve_chain_outcome_class`
   (`src/stel/executor.rs:329-338`) deliberately maps
   `NotFound | Ambiguous | EmptyResult → InvalidRequest` for a failed chain;
   `chain_failure_decision` (`:341-360`) records the reject.
   `OutcomeClass::is_error` (`src/protocol/result_status.rs:40-42`) treats
   `InvalidRequest` as an error, and `into_call_tool_result` (`:121-143`)
   therefore emits `CallToolResult::error` — so yes, the conversion itself makes
   the MCP item a host-visible error (`status=failed`).
5. **Malformed vs mis-armed:** all 13 dispatched calls were schema-valid.
   The planner (`route_find`/`route_read`/`route_trace`,
   `src/stel/planner.rs:994-1059`, plus the phrase parsers) passes whole prose
   as `search_text`/`search_symbols`/`search_files` queries, the raw query as
   `find_references.name` when `symbol` is absent, and the raw query as
   `get_file_context.path` when `path` is absent. Traces show broad-prose
   operands (8), real identifiers searched in the wrong modality (4), and one
   symbol-in-path (1) — semantically mis-armed, never primitive-rejected.
6. **Schema legibility:** the compact tool description is one sentence
   ("natural-language code intelligence with token economics",
   `src/stel/surface_list.rs`); the `intent` field carries no doc comment, no
   field-level explanation, and no examples distinguishing closed `intent` from
   natural-language `query` or optional `symbol`/`path`. The schema constrains
   values but does not explain them.

`ResultStatus` (`src/protocol/result_status.rs:108-111`) contains only
`contract_version` and `outcome_class` — the diagnostic's claim that no
`error_class`, `retryable`, failed-step, primitive-outcome, or suggested-next
field exists is source-true, and no failed item in the traces carries either key.

**Verdict: the primary diagnostic's three-mechanism decomposition (schema
decode, planner argument construction, lossy facade status mapping) is correct,
complete for the 17 observed failures, and each mechanism is independently
source-attributed to the right layer.**

## 4. Schema-only-rescue assessment

The evidence supports every Phase-4 statement as written:

- An `intent`-enum-only change directly targets exactly the 4 pre-dispatch
  failures. The diagnostic correctly does **not** overclaim: it concedes richer
  schema guidance "might prevent some dispatched failures" by changing future
  caller arguments, while proving that for the identical 13 requests the
  facade's status mapping — not the schema — produced the host errors. Both
  halves are evidence-bounded.
- Planner operands were schema-valid but semantically poor: direct observation
  (13/13 dispatched calls produced Empty/NotFound, 0 primitive InvalidRequest).
- The lossy mapping is a separate, source-proven mechanism from planner
  quality: `serve_chain_outcome_class` collapses truthful primitive outcomes
  regardless of how well-armed the arguments were. Run-level proof exists too —
  the same operand quality yields `found` when the identifier happens to be
  indexed (e.g. `symforge_retrieve`) and a host error when it is not.
- Human-readable suggestions did appear in failure bodies, yet 6 of 12
  immediate retries failed again and 5 failures went straight to native
  (once with 35 and once with 50 native items before the next SymForge call) —
  consistent with, though not strict proof of, the claim that prose suggestions
  do not substitute for typed recovery metadata. The diagnostic frames this as
  a design conclusion, not a measured causal result, which is the correct
  epistemic level.
- No product fix is authorized, and the four proposed rescue factors are
  separable (schema legibility; planner decomposition/modality; preservation of
  primitive status; typed recovery metadata). Direct observations, inferences,
  and design recommendations are kept distinct throughout the document.

I found no overclaim, no missing confound material to the decision, no wrong
source attribution, and no counting ambiguity beyond the two definitional notes
the diagnostic itself already states (retry definition; native-count definition).

## 5. Required corrections, ranked

No blocking corrections. Two minor, non-blocking items:

- **M1 (reproducibility nit):** the trace-set hash row format is underspecified
  ("ordered `run:sha256:length` rows"). The reproducing format is
  `NN:UPPERCASE-SHA256:length` joined with CRLF and no trailing newline. One
  clarifying phrase in a future evidence note would make the hash reproducible
  without a format search. Does not block confirmatory-design work.
- **M2 (custody wording):** the shakedown report says the "pre-receipt wiring
  quarantine" was deleted, and the Checkpoint-3 handoff expects the
  "raw-evidence wiring quarantine" to be absent. A residual
  `quarantine/` subdirectory (~24.9 KB: two timestamped pre-restart wiring
  probe bundles, `wiring-pre-fix-20260713T111257006Z` and
  `wiring-status-fixed-gate-20260713T111545013Z`) still exists inside the
  retained pre-restart invalidated evidence directory
  (`…\symforge-token-shakedown-evidence-a10ff102\quarantine`). This is most
  plausibly part of the deliberately retained pre-restart bundle rather than
  the deleted 34.5 MB quarantine, but the documents' wording does not
  disambiguate it. Recommend one sentence in the eventual cleanup record naming
  it retained-or-deletable. No canonical trace custody is affected; I deleted
  nothing.

## 6. Cleanup and custody verification (live)

- Golden-state directories: **absent** (no matching directories under the temp
  root). Post-closure deletion confirmed effective.
- Fixture worktree / isolated run home `symforge-token-shakedown-a10ff102`:
  **absent**; `git worktree list` shows only the main worktree.
- Candidate process: **none running**. Repository Cargo `target/`: **absent**.
  Disposable campaign Cargo target: **absent**.
- All 20 raw traces: **present** with byte total and per-file hashes matching
  the diagnostic's custody digests.
- Pre-restart invalidated traces: **present** (small run-01 + wiring bundles in
  the older evidence directory), as required — with the M2 wording caveat above.
- Compact in-repo evidence (`research/token-cost/evidence/…-a019/`): **present**
  (ledger, 20 grader files, receipts, semantic baseline).
- Pinned candidate binary: **present**, SHA-256 matches the receipt and ledger.
- Product diff: `git status`/`git diff` show the only product-source change is
  `src/stel/surface_list.rs` — the approved compact `read_only_hint`/
  `open_world_hint` annotation plus its regression test
  (`compact_surface_annotations_are_honest`) in the same file. Remaining
  modified files are docs/task notes; untracked files are research artifacts.
  Scope is exactly as authorized. (The annotation remains uncommitted; the
  diagnostic's gate item 3 — commit it before building a confirmatory
  candidate — still stands.)

## 7. Verdict

The 17-row diagnostic is complete, mechanically reproducible from the raw
traces under its stated definitions, correctly source-attributed at every
layer, and sufficient to gate rescue-design work without authorizing a product
fix.

VERDICT: APPROVE_DIAGNOSTIC
