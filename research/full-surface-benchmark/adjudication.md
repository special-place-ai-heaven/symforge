# SymForge benchmark adjudication specification

This document defines how to turn benchmark JSONL evidence into correctness and
token-economics results. It is subordinate to the
[benchmark protocol](../../docs/dogfood/2026-07-12-symforge-8.14.0-full-surface-benchmark-protocol.md)
and the [frozen case manifest](./cases.json). The campaign identity, tokenizer
locks, and safety settings come from [campaign.config.json](./campaign.config.json).

The core rule is:

> `case_complete` means that capture finished. It is not evidence that the tool
> was correct, complete, deterministic, or economical.

An efficient wrong answer is `INVALID_INCORRECT`. It never counts as a token
saving.

## 1. Inputs and fail-closed ingestion

The adjudicator consumes:

- one or more sanitized campaign JSONL files;
- the exact `cases.json`, campaign config, corpus lock, corpus manifest, and
  fixture `oracle.json` named by the campaign;
- the executable, runner, harness, parser, and asset hashes recorded at capture;
- paired-agent provider usage when the paired phase is enabled.

Before evaluating a case, perform these checks in order:

1. Verify every frozen input and executable hash.
2. Verify the SUT version, surface, transport, repository commit, fixture hash,
   tokenizer version, and tokenizer vocabulary hashes.
3. Group records by `run_id`, `case_id`, repetition, cohort, and request step.
4. Require exactly one `case_start`, the declared measured requests in order,
   required per-step state records, and one terminal record.
5. Reject unresolved placeholders, duplicate steps, unexpected measured calls,
   missing token fields, or sanitizer redactions that obscure a mandatory fact.
6. Parse tool output with a versioned parser. Store the parser version and hash.
   A parse failure is `UNEVALUATED`, never a pass.
7. Normalize only fields allowed by `parity-normalization.json`. Never normalize
   paths, counts, source text, symbol identity, result ordering where ordering is
   contractual, or Git state.

Raw response text must never be persisted when it could contain a secret. Exact
byte checks use an in-memory SHA-256 computed before sanitization plus sanitized
semantic facts. If that fingerprint is absent, an exact-byte claim is not
adjudicable.

## 2. Correctness-first status model

Artifact status and correctness status are separate.

| Status | Meaning |
|---|---|
| `CAPTURED` | The runner reached `case_complete`; no correctness is implied. |
| `ARTIFACT_INVALID` | Frozen identity, ordering, schema, or required evidence is missing or inconsistent. |
| `UNEVALUATED` | Evidence exists, but a mandatory parser, oracle, or independent review is unresolved. |
| `PASS` | Every mandatory automatic check and required independent review passed. |
| `FAIL` | At least one mandatory correctness, determinism, or mutation check failed. |

A happy case reaches `PASS` only when:

1. every declared request completed in order with its expected status;
2. every mandatory oracle check below passed;
3. source and runtime mutation policies passed after every relevant step;
4. required fresh repetitions completed with deterministic normalized facts;
5. every fixed-rubric review is resolved;
6. the case-specific stop condition, interpreted as all requests completed and
   the objective oracle evaluated, is satisfied.

All happy-path RPCs expect `ok`. A malformed source/config case still expects an
`ok` tool response whose semantic result says invalid. It does not expect a
JSON-RPC failure.

## 3. Normalized result record

Produce one adjudication record per case, repetition, and economics unit with
these fields:

```text
identity:
  run_id, case_id, primary_tool, repo, language, surface, transport
  repetition, cohort, cache_state, sut_hash, input_hashes
artifact:
  status, parser_version, parser_hash, missing_records, redaction_conflicts
steps[]:
  step, label, economics_role, expected_status, actual_status, rpc_ms
  raw_content_sha256, normalized_facts_sha256, evidence_record_ids
checks[]:
  check_id, oracle_source, expected_digest, observed_digest
  status, evidence_record_ids, review_rubric
correctness:
  automatic_status, independent_review_status, final_status
determinism:
  required_repetitions, completed_repetitions, normalized_hashes, mismatches
mutation:
  policy, changed_paths, allowed_paths, violations
latency:
  samples, median_ms, iqr_ms, p95_ms, failures, timeouts
tokens:
  cl100k, o200k, provider_usage
baseline:
  arm, equivalence_status, correctness_status, token_totals, concerns
economics[]:
  view, workload_size, baseline_tokens, symforge_tokens, saved_tokens
  savings_percent, cost_multiplier, verdict, reason
```

`checks[].status` is one of `pass`, `fail`, `manual_pending`, or
`not_observable`. Any mandatory `manual_pending` or `not_observable` keeps the
case `UNEVALUATED`.

## 4. Mandatory happy-case checks

The oracle notation below refers to independently generated fixture facts, Git
plumbing, frozen native rubrics, or an economics-excluded evaluator call. An
evaluator call may confirm state after measured calls, but its tokens and latency
must not enter the task economics.

### 4.1 Mutation, runtime, and state tools

| Case/tool | Mandatory checks | Economics unit and pre-freeze requirements |
|---|---|---|
| `SF-analyze_file_impact-001` / `analyze_file_impact` | Modified symbols are exactly `sfbench_leaf`; added/removed sets are empty; affected dependents equal the graph-derived contract set; an excluded follow-up symbol read matches the mutated source hash. | One measured call. **Pre-freeze:** add the follow-up evidence needed to prove refreshed indexed bytes. |
| `SF-batch_edit-001` / `batch_edit` | Dry run leaves both target hashes unchanged; apply produces independently frozen hashes for both files; replay changes no bytes and identifies stored replay; Rust syntax is valid. | Preview+apply is the normal workflow; replay is a diagnostic. **Pre-freeze:** freeze combined per-file output hashes and per-step state fingerprints. |
| `SF-batch_insert-001` / `batch_insert` | Preview changes zero bytes; apply inserts exactly twice before the two anchors with correct indentation; final hash matches an independently generated oracle; Rust syntax is valid. | Preview+apply. **Pre-freeze:** the claim says replay-safe but has no replay request; add one or remove that claim. |
| `SF-batch_rename-001` / `batch_rename` | Preview changes zero bytes; apply changes exactly the definition and four code references; two comment/string mentions remain; final hash and Rust syntax match the oracle. | Preview+apply. **Pre-freeze:** add the claimed replay request or narrow the claim. |
| `SF-checkpoint_now-001` / `checkpoint_now` | Snapshot exists and is nonempty; verify-after-write succeeded; no partial temp remains; an excluded restart loads the snapshot and reproduces project identity and counts. | One task call; restart is evaluator evidence. Baseline is capability-only. **Pre-freeze:** add restart/load evidence. |
| `SF-context_inventory-001` / `context_inventory` | Fetched-file count, fetched-symbol count, and associated content tokens are zero; no prior commitment retrieval occurred. | One call. Automatic, but the recipe baseline is only a narrow fresh-session control. |
| `SF-delete_symbol-001` / `delete_symbol` | Preview changes zero bytes; apply equals the oracle final hash; target symbol is absent; adjacent blank lines are normalized; Rust syntax is valid. | Preview+apply. **Pre-freeze:** add the claimed replay request or narrow the claim. |
| `SF-detect_impact-001` / `detect_impact` | Impact marker is present; payload is valid JSON with the documented schema; changed file/symbol equal the dirty leaf; depth-1 blast radius equals the direct caller set with correct hop/risk; data files are absent. | One call. Fully automatable. |
| `SF-edit_within_symbol-001` / `edit_within_symbol` | Preview changes zero bytes; final hash equals the oracle; both in-symbol literals change; the one outside literal remains; Rust syntax is valid. | Preview+apply. **Pre-freeze:** add the claimed replay request or narrow the claim. |
| `SF-health-001` / `health` | Loading response is bounded and honest; project identity and counts agree with independent admitted-file/symbol evidence; reported load source agrees with lifecycle and snapshot evidence. | Cold, snapshot-warm, and process-warm are separate cohorts. **Pre-freeze:** the current two requests do not deterministically cover all three claimed states. |
| `SF-health_compact-001` / `health_compact` | Shared state, project, count, issue, and snapshot fields equal the full-health reference; compact response content tokens are lower. | Compact is the task; full health is an excluded reference call. |
| `SF-index_folder-001` / `index_folder` | First call targets the expected project identity and produces expected index anchors/counts; replay is identified as replay, performs no rebuild, and duplicates no watcher/project state. | First call is the task; replay is diagnostic. Baseline is capability-only. **Pre-freeze:** define admitted count/anchor oracles. |
| `SF-insert_symbol-001` / `insert_symbol` | Preview changes zero bytes; apply matches an independently frozen final hash; placement before the anchor, indentation, and Rust syntax are exact. | Preview+apply. **Pre-freeze:** add the claimed replay request or narrow the claim. |
| `SF-replace_symbol_body-001` / `replace_symbol_body` | Preview changes zero bytes; apply equals oracle file and diff hashes; unrelated Unicode/newline bytes remain exact; Rust syntax is valid; guard use is reported. | Preview+apply. **Pre-freeze:** add the claimed replay request or narrow the claim. |
| `SF-status-001` / `status` | Version, surface, and wiring agree with the live manifest; project identity and index fields agree with excluded health evidence; projects detail contains exactly one home project in isolated mode. | Compact/full/projects are separate variants. Baseline is capability-only. **Pre-freeze:** capture excluded manifest/health parity evidence. |
| `SF-symforge_edit-001` / `symforge_edit` | Preview changes zero bytes; apply equals the replacement oracle hash; indentation is not doubled; exact-body guard is accepted; Rust syntax is valid. | Preview+apply. **Pre-freeze:** add the claimed replay request or narrow the claim. |
| `SF-what_changed-001` / `what_changed` | Normalize the exact six staged/unstaged/untracked/rename/add/delete states; file set equals Git plumbing; fused symbol delta equals an independent before/after parser oracle. | One call. **Pre-freeze:** freeze the fused symbol-delta oracle. |

### 4.2 Read, search, guidance, and facade tools

| Case/tool | Mandatory checks | Economics unit and pre-freeze requirements |
|---|---|---|
| `SF-ask-001` / `ask` | Route category is definition lookup; path, name, kind, and start line equal the unique Python leaf oracle. | One call. Allow a frozen equivalent-route set; explanation quality uses a rubric. |
| `SF-conventions-001` / `conventions` | Extract naming, error handling, imports, tests, and layout facts; compare them with independent stratified counts and sampled native files; reject contradictions. | One call. **Pre-freeze:** freeze the ripgrep convention rubric and thresholds. Qualitative synthesis requires fixed-rubric review. |
| `SF-diff_symbols-001` / `diff_symbols` | Per-file added/removed/modified symbol sets equal an independently parsed Git ref diff. | One call. **Pre-freeze blocker:** current `sfbench-v1` maps to final replay `HEAD`, and the Rust path does not change. Choose a real differing ref/path or make the expected empty result explicit. |
| `SF-edit_plan-001` / `edit_plan` | Exact target identity and reference count match the oracle; suggested sequence includes impact/discovery, the correct structural edit family, and post-edit impact without irrelevant operations. | One call. Count checks are automatic; “minimal” ordering uses a frozen allowed-sequence rubric. |
| `SF-explore-001` / `explore` | Required native ripgrep anchors and paths are present; signatures are present; reported first-hop edges resolve to real symbols; no invented anchor appears. | One call. **Pre-freeze:** freeze the native command-parsing anchors. Relevance/coverage requires rubric review. |
| `SF-find_dependents-001` / `find_dependents` | Dependent set for Python protocol is exactly `core.py`; edge kind is import; no unrelated cycle/test file appears. | One call. Fully automatable. |
| `SF-find_references-001` / `find_references` | Full set equals the leaf call from `sfbench_mid`, classified as a call with exact location; compact normalized set equals full. | Full and compact are separate variants, not a summed task. |
| `SF-get_file_content-001` / `get_file_content` | Strip documented framing, reconstruct returned source, and compare line-range, offset/limit, and full-file bytes/lines with the oracle; verify headers and line numbers independently. | Three separate variants. Exact checks require pre-sanitization fingerprints. |
| `SF-get_file_context-001` / `get_file_context` | Outline equals symbols in Python core; imports equal import oracle; consumers equal dependent-file oracle; references equal call graph; Git anchors equal frozen history. | One call. Set checks are automatic. |
| `SF-get_repo_map-001` / `get_repo_map` | File, language, and top-directory counts equal pre-session native inventory under documented admission rules; required native anchors are present; estimate error is recorded against actual content tokens. | Actual is the task; estimate is diagnostic. **Pre-freeze:** distinguish indexed/admitted counts from raw tracked counts. |
| `SF-get_symbol-001` / `get_symbol` | Source-body hash, byte span, line span, kind, and path equal the oracle; force-refresh normalized facts equal actual; estimate error is recorded. | Actual and refresh are separate cache cohorts; estimate is excluded. |
| `SF-get_symbol_context-001` / `get_symbol_context` | Definition identity is exact; callers equal `{sfbench_mid}`; callees are empty; every real verbosity preserves its required identity/edge facts. | Each verbosity is separate. **Pre-freeze:** the current summary request is estimate-only, so add an actual summary call before scoring summary parity or estimate accuracy. |
| `SF-inspect_match-001` / `inspect_match` | Enclosing symbol, span, parent chain, sibling set, and excerpt equal independently derived structure around the requested line. | One call. **Pre-freeze:** the target leaf is top-level while the claim says nested; fix the claim or select a nested symbol. |
| `SF-investigation_suggest-001` / `investigation_suggest` | Initial step loads only entry; suggestions include real unloaded `sfbench_mid`/worker dependencies; loaded entry is not repeated; every suggestion resolves to a real path/symbol. | `get_symbol` plus suggestion is one required workflow. Membership is automatic; ranking usefulness uses a rubric. |
| `SF-search_files-001` / `search_files` | Fuzzy results include both duplicate paths; resolve returns the exact `a/` path; ranking explanation cites real current-file/path-prefix evidence. | Fuzzy and resolve are separate variants. Set checks are automatic; ranking explanation uses a rubric. |
| `SF-search_symbols-001` / `search_symbols` | Result set contains exactly the Rust leaf with oracle path/kind/start line; no test/generated/vendor/personal result appears; precision and recall are both one. | One call. Fully automatable. |
| `SF-search_text-001` / `search_text` | Exact Rust source-marker hit set, enclosing symbol, and caller enrichment match the oracle; no test/generated/vendor/personal noise appears. | One call. Fully automatable. |
| `SF-symforge_retrieve-001` / `symforge_retrieve` | Trigger yields exactly one handle; retrieved raw payload contains the expected generated symbols/count and equals the captured uncompressed payload fingerprint; repeat bytes are identical. | Summary task is trigger only; exact-detail task is trigger+retrieve; repeated retrieve is diagnostic. |
| `SF-validate_file_syntax-001` / `validate_file_syntax` | Valid JSON is accepted; malformed JSON is semantically rejected while RPC remains successful; diagnostic line agrees with the authoritative parser where promised; estimate error is recorded. | Valid and malformed are separate tasks; estimate is diagnostic. |
| `SF-symforge-001` / `symforge` | For each intent, extract route identity and facts; compare with economics-excluded granular evidence for definition, callers, source body, graph orientation, impact, exact file resolution, and next-context suggestion. | Seven separate intent tasks, never one summed task. Fact parity is automatic; route optimality/explanation quality uses a rubric. |

## 5. Economics roles

Every request must have one role:

| Role | Included in ordinary task economics? |
|---|---|
| `task` | Yes. |
| `required_prerequisite` | Yes, when the task cannot be completed without it. |
| `comparison_variant` | No; report as a separate task variant. |
| `estimate_diagnostic` | No, unless the user task explicitly asks for an estimate. |
| `oracle_reference` | No. |
| `replay_diagnostic` | No; report idempotency/replay overhead separately. |
| `warmup` | No. |

Apply these case rules:

- edit preview+apply is one normal workflow;
- replay calls are diagnostics;
- estimate calls are diagnostics;
- full health in compact/full parity is an oracle reference;
- reference/verbosity/read/detail modes are separate variants unless the user
  task explicitly requires more than one;
- `investigation_suggest` includes the initial symbol load as a required
  prerequisite;
- CCR summary-only is trigger-only, exact-detail is trigger+retrieve, and repeat
  is diagnostic;
- each of the seven `symforge` intents is a separate task.

## 6. Token accounting

Calculate every independent view for both `cl100k_base` and `o200k_base`.
Provider-reported totals are primary for paired tasks.

For encoding `e`, over task-role measured calls:

```text
Q_e = sum(schema_free_tool_request_e)
C_e = sum(schema_free_tool_response_e)
P_e = sum(schema_free_direct_payload_e)
```

Validate all three recorded fields independently. Do **not** require
`P_e == Q_e + C_e`: BPE tokenization is not additive across a serialization
boundary, so tokenizing the canonical combined direct payload can legitimately
differ from tokenizing its request and response components separately. `P_e` is
the primary task-cost measurement; `Q_e` and `C_e` are diagnostic decompositions.

### 6.1 Content-only and direct payload

```text
content_only_symforge_e = C_e
content_only_baseline_e = sum(baseline output tokens)

direct_payload_symforge_e = P_e
direct_payload_recipe_baseline_e = baseline_total.direct_payload_e
```

Content-only is diagnostic. Direct payload is the direct-RPC comparison. Neither
is interchangeable with the complete paired-agent task total.

### 6.2 Cold eager schema

```text
cold_eager_symforge_e = P_e + S_surface_e
cold_eager_baseline_e = B_payload_e + S_baseline_surface_e
```

`S_surface_e` is the exact model-visible surface when captured. Serialized
`tools/list` is a theoretical fallback, not proof that a client sent every
schema. Full tools use the full-36 surface; compact/meta tools use their actual
3/1-tool surfaces. Do not calculate schema-inclusive savings when the baseline
schema is absent.

### 6.3 Tool-specific lazy schema

```text
S_lazy_unique_e =
  sum(individual schema tokens for distinct top-level tools exposed for task)

lazy_unique_symforge_e = P_e + S_lazy_unique_e

S_lazy_per_turn_e =
  sum(exact model-visible schema definitions on each actual model turn)
```

For facades such as `ask` and `symforge`, internal server routing does not add
routed-tool schema. Only top-level model-visible definitions count.

### 6.4 Five- and twenty-task amortization

The one-time-schema calculation is a labeled counterfactual only:

```text
amortized_e(N) =
  (sum(P_e for N tasks) + S_surface_e + one_time_visible_setup_e) / N
```

For identical tasks:

```text
amortized_e(N) = P_e + (S_surface_e + one_time_visible_setup_e) / N
```

Report `N=5` and `N=20`, plus `N=1`. Label these
`theoretical_one_time_schema`. Clients may resend schema every turn.

The actual longitudinal result is:

```text
actual_session_per_task_e(N) = provider_session_total_tokens_e / N
```

Provider totals already containing schema, cached input, reasoning, or transcript
replay must not receive those components a second time.

### 6.5 Savings and break-even

```text
saved_tokens = baseline_total_tokens - symforge_total_tokens
savings_percent = 100 * saved_tokens / baseline_total_tokens
cost_multiplier = symforge_total_tokens / baseline_total_tokens
negative_trial_rate = token_negative_trials / valid_paired_trials
```

If baseline tokens are zero, keep the absolute delta but set percentage and
multiplier to null.

```text
break_even_tasks = ceil(
  (symforge_setup_cost - baseline_setup_cost) /
  (baseline_query_cost - symforge_query_cost)
)
```

If the denominator is zero or negative, report `no break-even observed`.

Aggregate tokens before calculating percentages:

- micro: pool valid tokens for the declared workload;
- macro: pool within each task/repository stratum, then equal-weight strata;
- report median, IQR, p95, all failures/timeouts, negative-trial rate, and the
  paired bootstrap confidence interval.

## 7. Economics verdict precedence

Reject `ARTIFACT_INVALID` and incomplete-accounting records before economics.
Then apply exactly this order:

1. `INVALID_INCORRECT` if either arm fails the same correctness oracle.
2. `N/A_NO_EQUIVALENT_BASELINE` when the baseline is capability-only,
   materially partial, or incorrect. Record a reason such as `capability_gain`.
3. `TOKEN_NEGATIVE`, `POSITIVE`, or `NEUTRAL` for each valid view/trial.
4. `MIXED` only for an aggregate containing both positive and token-negative
   valid trials.

Never turn a failed, partial, or capability-only baseline into zero tokens or
infinite savings.

## 8. Baseline-equivalence caveats

- Happy edit recipes for `batch_edit`, `batch_insert`, `batch_rename`,
  `delete_symbol`, `edit_within_symbol`, `insert_symbol`,
  `replace_symbol_body`, and `symforge_edit` reference patches that are not
  present in the frozen oracle. Recipe economics are unavailable until exact
  patches and expected hashes are asset-locked.
- `checkpoint_now`, `index_folder`, and `status` are capability-only.
- `health` and `health_compact` shell recipes observe file/snapshot existence,
  not parser, watcher, load-source, or trust state. Split observable subclaims or
  use `N/A_NO_EQUIVALENT_BASELINE` for the full claim.
- `context_inventory` uses a synthetic empty list rather than an independent
  observation. Treat it as a narrow control, not general equivalence.
- The `get_file_content` PowerShell recipe is not byte exact. Replace it with raw
  byte reading and a deterministic representation or hash.
- `conventions`, `explore`, `edit_plan`, `investigation_suggest`, and `symforge`
  recipes are retrieval lower bounds, not completed equivalent answers. Their
  paired free-agent baseline is the headline.
- `detect_impact`, `diff_symbols`, `get_file_context`,
  `get_symbol_context`, and `inspect_match` recipes expose raw evidence but do
  not independently perform the claimed classification. Keep recipe totals as
  sensitivity lower bounds.
- Mutation recipes verify `git diff --check`, not Rust syntax. Add an
  authoritative parser/compiler check.
- `symforge_retrieve` has an equivalent ordinary path for exact content, but no
  ordinary equivalent for CCR handle/session semantics. Split task-outcome and
  capability economics.
- A baseline arm that does not pass the same objective oracle is not a SymForge
  win; it is an invalid comparison.

The paired free-agent LLM arm remains the headline comparison. Recipe results
are stable lower-bound sensitivity measurements and must be labeled as such.
