# SymForge End-to-End Feature Token Benchmark

**Date:** 2026-07-13<br>
**Benchmark target:** `Agent_Army_Professionals` at `b1423aab350d1b065a550c42bf5f2b98c7d2c069`<br>
**Host:** Codex CLI 0.144.2, `gpt-5.6-sol`, high reasoning<br>
**SymForge:** 8.14.1, full 36-tool surface

## Verdict

For this feature task, across two clean paired trials:

```text
net token saving = tokens without SymForge - tokens with SymForge
                 = 5,177,394 - 3,894,403
                 = 1,282,991 tokens across two repetitions
                 = 641,496 tokens per completed feature run on average
                 = 24.8% fewer tokens
```

Both paired trials were positive: **36.2% saved** in Trial 1 and **17.1% saved**
in Trial 2.

That is the measured end-to-end result. It includes tool schemas, prompts,
reasoning turns, tool requests and responses, retries, verification, and the
final answer. No per-call estimate is substituted for the session total.

### Causal limitation

This result is an **enabled-and-indexed versus disabled host benchmark**, not a
clean proof that explicit symbol-aware retrieval caused the full saving.
The event traces show that both SymForge-enabled agents explicitly called
`index_folder`, but then continued to emit native command and file-change
events. A stricter third arm was excluded because it made zero MCP calls and
therefore was not a SymForge run.

The honest claim is:

> In this Codex/AAP benchmark, sessions with SymForge enabled and indexed used
> 641,496 fewer tokens per completed feature run on average (24.8%).

The data does **not** justify claiming that every project, feature, model, or
pure explicit-SymForge workflow will save 24.8%.

## What was benchmarked

The identical prompt asked each clean session to implement per-plan-item
research linkage in AAP's pre-coding gate:

- add a backward-compatible `PlanItem::research_refs: Vec<String>`;
- require at least one non-blank research reference per plan item;
- preserve existing missing-file evidence;
- report missing item IDs deterministically in plan order;
- update only necessary constructors, parsers, fixtures, and tests;
- run formatting and focused `aap-core` tests;
- finish with a verified implementation and final summary.

The prompt selected tools conditionally: use SymForge exclusively when its MCP
surface is available; otherwise use native `rg`, bounded file reads, and
patching. The two arms differed only in
`mcp_servers.symforge.enabled=true|false` and their clean detached worktree
paths. Trial order was reversed to reduce order bias.

## Score calculation

Codex's `turn.completed.usage` object is the source of truth.

```text
total tokens = input_tokens + output_tokens
```

`cached_input_tokens` is already a subset of `input_tokens`, and
`reasoning_output_tokens` is already represented within output accounting;
neither was added a second time.

| Trial | Order | Native input | Native output | Native total | SymForge input | SymForge output | SymForge total | Net saved | Saved |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | Native → SymForge | 2,077,548 | 7,541 | 2,085,089 | 1,324,293 | 5,951 | 1,330,244 | 754,845 | 36.2% |
| 2 | SymForge → Native | 3,079,662 | 12,643 | 3,092,305 | 2,551,540 | 12,619 | 2,564,159 | 528,146 | 17.1% |
| **Combined** | — | — | — | **5,177,394** | — | — | **3,894,403** | **1,282,991** | **24.8%** |
| **Mean/run** | — | — | — | **2,588,697** | — | — | **1,947,202** | **641,496** | **24.8%** |

The 19.1-point spread between trials is substantial. Two repetitions establish
an observed positive result for this setup, not a stable population estimate.

## Outcome equivalence

Every included arm:

- changed only `crates/aap-core/src/plan_document.rs` and
  `crates/aap-core/src/pre_coding_gate.rs`;
- implemented the same defaulted public field and the same non-blank reference
  test;
- preserved the exact missing-file evidence;
- emitted missing item IDs in plan order;
- added regression coverage;
- passed the focused pre-coding-gate tests during its run;
- passed `git diff --check`.

Trial 1's native arm added one extra regression test (23 focused tests versus
22); this strengthened rather than weakened its outcome. Trial 2 reported 22
focused tests in both arms.

`cargo fmt --check` failed in every arm on pre-existing formatting drift in
unrelated files. Changed-file formatting checks passed. A later independent
re-link attempt hit disk exhaustion after six disposable Rust target trees;
the benchmark worktrees were then removed. That later environmental failure
does not replace the green focused test results captured inside the completed
sessions.

No benchmark patch was merged or retained.

## Invalid strict arm

A third SymForge arm was given an identical hard repository policy requiring
named SymForge read and structural-edit tools. Its event trace contained zero
MCP calls and native file-change events, so it failed the treatment condition.
Its 1,873,626-token total was excluded, and no paired score was calculated from
it.

This exclusion matters: counting a session that merely had a server configured
but did not call it would create a false causal result.

## Fixed surface cost

The current server's real `tools/list` payload was measured with the repository
script:

| Surface | Tools | `tools/list` bytes |
|---|---:|---:|
| Full | 36 | 72,757 |
| Compact | 3 | 4,581 |

These bytes are not the benchmark score and were not manually added to session
totals; the host usage counter already includes the context it sent. They
explain why indiscriminate full-surface use can lose on tiny tasks. The compact
surface was not benchmarked end to end here.

The claim that “each SymForge call costs about 7k tokens” mixes different
costs. In this environment:

- surface/schema context is a host/session cost;
- request arguments are a call cost;
- returned content is a call cost;
- repeated model turns reprocess cached and uncached context;
- CCR retrieval is an optional second call only when the summary is
  insufficient.

Only the final session total answers the benchmark question.

## Economic map of all 36 tools

This is behavioral analysis, not a second score.

| Economic role | Tools | Expected token effect |
|---|---|---|
| Indexed discovery and targeted reads | `get_repo_map`, `get_file_context`, `get_symbol`, `get_symbol_context`, `search_symbols`, `search_text`, `search_files`, `find_references`, `find_dependents`, `inspect_match`, `explore`, `ask`, `investigation_suggest` | Best chance of net savings on non-trivial code: replaces broad file enumeration, grep fan-out, whole-file reads, and manual caller tracing with ranked bounded context. Loses when the answer is already one obvious line or when the agent ignores the result and re-reads files anyway. |
| Raw indexed content | `get_file_content` | Saves only when ranges, estimates, budgets, or session dedup avoid a whole-file native read. A full uncapped fetch is economically similar to a normal full read plus schema overhead. Use native tools for prose/Markdown. |
| Change, impact, and preparation | `analyze_file_impact`, `detect_impact`, `what_changed`, `diff_symbols`, `conventions`, `edit_plan`, `context_inventory` | Can collapse several searches and reads into one response before an edit. Strongest on shared symbols and multi-file changes; unnecessary for isolated obvious edits. |
| Structural mutation | `replace_symbol_body`, `edit_within_symbol`, `insert_symbol`, `delete_symbol`, `batch_edit`, `batch_insert`, `batch_rename`, `symforge_edit` | Saves tokens when the agent can address symbols directly without loading and replaying whole files. Batch tools also remove round trips. No saving if the agent first reads full files and then uses built-in patching, which happened in the invalid strict arm. |
| Runtime, recovery, and syntax | `health`, `health_compact`, `status`, `index_folder`, `checkpoint_now`, `validate_file_syntax` | Primarily correctness/operations tools, not token reducers. Their cost is justified when health, recovery, or authoritative parsing is needed; gratuitous calls are overhead. |
| Reversible overflow | `symforge_retrieve` | Defers the full payload. It saves tokens when the ranked summary is sufficient and costs an extra round trip when the full result is actually needed. |

Current CCR eligibility is deliberately narrow:
`search_text`, `search_symbols`, `find_references`, `explore`, and
`get_repo_map`. Large outputs are summarized under a token budget and stored
behind a retrieval handle. This is economically sound only when most callers
do not retrieve the overflow.

## Code versus documentation

The benchmark covered Rust feature work, SymForge's intended domain. It does
not support using SymForge for prose, specifications, or Markdown. For those
artifacts, headings-as-symbols add little value, while native search and bounded
reads avoid the full MCP surface cost.

## Bottom line

**Observed net saving for this completed feature:** **641,496 tokens per run,
24.8%**.

That positive number survived two run orders and included the full 36-tool
surface overhead. It is useful evidence that enabling SymForge can improve
whole-session economics on a non-trivial codebase.

Confidence is limited by two facts: only two valid pairs were run, and the
agents did not stay on explicit SymForge retrieval/edit tools after indexing.
A causal product claim requires a host-enforced allowlist that rejects native
code reads/edits, records every tool call, and runs a larger paired task suite.
