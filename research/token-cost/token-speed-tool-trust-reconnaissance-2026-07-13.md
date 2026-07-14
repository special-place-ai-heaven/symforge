# SymForge Token, Speed, and Tool-Trust Reconnaissance

**Date:** 2026-07-13<br>
**Status:** Research and experimental plan; only the benchmark manifest's compact-read annotation prerequisite is authorized before the powered pilot.<br>
**Prior benchmark:** [end-to-end-feature-benchmark-2026-07-13.md](./end-to-end-feature-benchmark-2026-07-13.md)

## Executive decision

Keep the full 36 semantic tools as SymForge's canonical capability surface. Do not promote the current compact three-tool surface to the default.

The most promising route to a net gain is:

1. preserve semantic leaf names such as `search_symbols`, `find_references`, and `edit_plan`;
2. let capable hosts defer and discover their schemas instead of injecting all 36 up front;
3. reduce result tokens adaptively, measuring the whole agent trajectory rather than one response;
4. improve task-conditioned structural retrieval inside the existing local LiveIndex;
5. make freshness, truncation, provenance, and edit receipts machine-verifiable;
6. benchmark adoption, correctness, wall time, and trust together before changing the product.

The compact surface is not useless. It is a useful ablation and may become an expert/code-mode interface. Its 93.7% schema-byte saving is real, but it has not demonstrated end-to-end net value and currently removes the affordances models use to choose tools.

## Checkpoint 0 — Claude Opus review

Claude Opus returned `APPROVE_WITH_CHANGES` after checking the report, benchmark arithmetic, and cited implementation points. The required changes are incorporated below:

- run a 20-run variance shakedown before attempting the multi-host pilot;
- never pool host-native deferred-discovery results across hosts;
- pre-register total trajectory tokens as the single primary efficiency metric;
- keep Phase 1 trust measurement passive and defer injected failures to Phase 4;
- specify edit-task oracles before any edit task is admitted;
- treat the earlier zero-call strict arm as a scored adoption outcome in future work while clearly warning that reclassification can reduce the prior headline saving.

## What is established

### 1. SymForge already produced a meaningful end-to-end token association

The July 13 paired benchmark measured 5,177,394 native tokens versus 3,894,403 with full-surface SymForge enabled and indexed: 1,282,991 fewer tokens across two trials, or 24.8%. The trial-level reductions were 36.2% and 17.1%.

That result includes schemas, prompts, reasoning, tool requests and responses, retries, and verification. It is therefore more useful than a per-call estimate. It is not yet a pure causal estimate of explicit SymForge use: the agents called `index_folder`, then continued with native command/file events. A strict arm made no MCP calls and was excluded.

For product evaluation, that exclusion rule must change. A configured agent that never calls SymForge is a treatment failure, not missing data. Only a pre-declared infrastructure or preflight failure should be excluded. The earlier strict zero-call run used 1,873,626 tokens, below the two-run SymForge mean of 1,947,202, so reclassifying it can reduce the 24.8% headline. That consequence must be disclosed rather than treated as an inconvenient outlier.

### 2. Compact saves schema bytes but is not presently an equivalent interface

The same benchmark serialized the full surface at 72,757 bytes and compact at 4,581 bytes, a 93.7% reduction. The earlier surface spike measured the same order of magnitude: roughly 16,153 versus 1,135 `o200k_base` tokens.

However, the implementation itself labels the three compact descriptions as a `Phase 0 measurement probe`. Its public verbs are generic—`symforge`, `symforge_edit`, and `status`—and its sparse input schemas do not explain individual parameters. The production compact `tools/list` path also creates fresh default `Tool` objects and does not receive the annotations attached to the canonical full router. In a controlled Codex CLI check, full annotated `health_compact` completed while compact `status` and compact `symforge` were both cancelled before execution. See `ServerHandler::list_tools` in [mod.rs](../../src/protocol/mod.rs#L1335) and `surface_tool`/`compact_surface_tools` in [surface_list.rs](../../src/stel/surface_list.rs#L24).

The compact read facade is more than a simple alias: `symforge_stel_handler` performs planning, admission/economics, routing, cache handling, execution, and result serving. But the handler is gated to compact mode, while full-surface semantic leaf calls bypass that controller. See [mod.rs](../../src/protocol/mod.rs#L9720). This creates two separate questions:

- Can a model discover and correctly control the compact facade?
- Which STEL output/admission ideas improve full-surface leaf calls without causing refusals, extra routing turns, or lost trust?

Those questions must be tested separately. Moving the whole compact controller in front of working leaf tools would combine interface and policy changes and make failures difficult to diagnose.

### 3. The repository already contains evidence against generic staged verbs

The July 3 surface spike found:

- compact organic use was too rare to establish adoption;
- 36 tools did not produce a catastrophic routing failure in the tested hosts;
- tool-tip compliance showed little or no useful lift after accounting for base rates;
- a proposed seven-verb staged vocabulary created a second vocabulary, reveal/reset costs, and generic-name collisions.

The Terminal Commander field report then observed the compact surface hiding granular tools named by repository instructions and routing a natural-language edit-planning request to file search instead of symbol/context work. The full surface fixed the issue.

This means a future client-neutral reduced profile should first be an allowlist of existing canonical leaf names—not another set of generic aliases.

### 4. Current token controls are real but only partly exploited

CCR currently applies to five discovery tools:

- `search_text`, `search_symbols`, and `find_references`: 8,000-token defaults;
- `explore`: 12,000;
- `get_repo_map`: 16,000.

When a result exceeds its budget and compression actually saves space, SymForge stores the complete body and provides a `symforge_retrieve` continuation. See [ccr.rs](../../src/protocol/ccr.rs) and the application point in [mod.rs](../../src/protocol/mod.rs#L692).

These ceilings are safeguards, not evidence that 8K/12K/16K are optimal. Lowering them blindly can create more retrieval calls and increase total trajectory cost. The correct target is task completion at minimum total tokens and wall time.

Session-level reuse already exists for exact reads through `detect_session_cache_hit` in [controller.rs](../../src/stel/controller.rs#L241) and fetch tracking in [session.rs](../../src/protocol/session.rs#L28). Before inventing another cache, the evaluation should measure how often models trigger and benefit from the one that exists.

### 5. Trust envelopes are a real differentiator, but models still need machine-readable evidence

The public [innovations summary](https://github.com/special-place-ai-heaven/symforge#innovations) describes honest routing, parse quarantine, anti-self-feedback ranking, worktree-aware structural edits, idempotency, atomic writes, tee snapshots, and reindexing from exact on-disk bytes. The supplied edit sequence shows the intended trust chain:

`edit_plan` -> caller/blast-radius analysis -> validated structural edit -> tee snapshot -> atomic write -> reindex -> evidence envelope -> `analyze_file_impact`.

Current results already expose `_meta` status and selected-project evidence through `ResultStatus`; edit results also carry outcome and operation metadata. See [result_status.rs](../../src/protocol/result_status.rs#L121).

The current source does not expose MCP `outputSchema`/`structuredContent`. The stable MCP tools specification supports both, while warning that annotations are only hints. Structured output is therefore a plausible trust improvement, but its schema and duplicated compatibility text also cost tokens. It should be proven first on one read workflow and one edit workflow, not rolled across all 36 tools. [MCP tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools).

### 6. Cold indexing and warm retrieval are being conflated

`index_folder` performs a full index from source and is appropriate for an empty, wrong, or deliberately reset workspace. SymForge's runtime model can auto-discover a project and restore a valid local snapshot on startup. Agents that reflexively call `index_folder` add avoidable latency and may reduce trust by making every session look cold.

Future benchmarks must report separately:

- cold build from source;
- warm snapshot restore;
- ready, already-resident query latency;
- explicit rebuild after a detected integrity or project mismatch.

The setup guidance should eventually teach `health_compact`/`health` first and reserve `index_folder` for evidence-backed reset conditions. This is a documentation/configuration candidate, not a server redesign.

### 7. Local telemetry is insufficient for an adoption benchmark

Stored analytics capture tool name, surface, configured scope, response bytes, estimated tokens, duration, success, outcome, and capability state. See [store.rs](../../src/analytics/store.rs#L192) and [schema.rs](../../src/analytics/schema.rs#L7).

They do not establish task identity, randomized arm, ordered agent trajectory, first-tool correctness, native fallback, evidence utilization, final answer quality, or whether an edit receipt matched the actual diff. Host traces and a benchmark manifest are therefore required. Adding more server telemetry before defining the evaluation would be premature.

## External evidence and what it means for SymForge

### Preserve semantics; defer schemas

OpenAI's tool-search guidance says models are trained to search namespace and MCP-server surfaces and recommends deferred loading for large catalogs. Its GPT-5.4 launch reports 47% lower total token usage at the same accuracy on 250 MCP Atlas tasks spanning 36 servers. [OpenAI tool search](https://developers.openai.com/api/docs/guides/tools-tool-search), [GPT-5.4](https://openai.com/index/introducing-gpt-5-4/).

Anthropic recommends tool search once catalogs reach roughly 20 or more tools. Its internal evaluation reported an 85% tool-definition token reduction while loading only three to five relevant tools, with higher accuracy on its MCP evaluation. These are vendor results, not SymForge results, but they directly justify a replication arm. [Anthropic tool search](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool), [advanced tool use](https://www.anthropic.com/engineering/advanced-tool-use).

Progressive discovery is mainly a host capability. MCP servers publish `tools/list`; capable hosts decide which definitions enter the model context. SymForge should therefore keep a standards-compliant stable catalog and provide client-specific integration recipes rather than varying the tool list per connection. [MCP client best practices](https://modelcontextprotocol.io/docs/develop/clients/client-best-practices).

### Tool names and descriptions are part of the control system

Anthropic identifies detailed, high-signal descriptions as the most important tool-performance lever and recommends explaining purpose, use conditions, parameters, caveats, and exclusions. Tool-search implementations match names and descriptions. [Defining tools](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools).

BiasBusters found semantic query/metadata alignment was the strongest predictor of tool selection and that description wording can shift choices. This supports retaining explicit vocabulary such as `symbol`, `reference`, `impact`, `syntax`, and `context`, while treating model and position bias as benchmark variables. [BiasBusters](https://arxiv.org/abs/2510.00307).

EASYTOOL found that semantic distillation of tool documentation reduced average documentation size by about 70% while improving tool use, whereas generic token deletion could remove essential function and parameter information. The transferable lesson is to shorten schemas structurally, not run them through a generic prompt compressor. [EASYTOOL](https://aclanthology.org/2025.naacl-long.44/).

### Few tools can work, but not by opacity alone

Cloudflare's Code Mode exposes three tools over thousands of API operations, but it gives the model searchable documentation and a typed JavaScript execution environment. It demonstrates extreme schema compression, not equivalence for a natural-language dispatcher or coding-task completion. A SymForge code-mode experiment would require a sandbox, authorization boundaries, timeouts, typed results, and output filtering. It belongs behind the lower-risk surface and output experiments. [Cloudflare Code Mode](https://blog.cloudflare.com/code-mode-mcp/).

### Ranking should be task-conditioned and budgeted

SymForge's compact repo map alphabetizes the directory view and separately ranks a small entry-point list using dependent count, churn, and deterministic tie-breaking. See `repo_map_text` in [handlers.rs](../../src/sidecar/handlers.rs#L1698). The implementation has no PageRank-style task-conditioned map today.

Aider's repository map applies graph ranking informed by the active chat and fits the output to a token budget. That is a good algorithmic candidate because SymForge already has a symbol/reference graph and should not need a new database. [Aider repository map](https://aider.chat/docs/repomap.html).

ContextBench reports that coding agents tend to retrieve high-recall but low-precision context and distinguishes explored from actually utilized context. Its gold context labels provide useful retrieval metrics beyond final task success. [ContextBench](https://arxiv.org/abs/2602.05892). CORE-Bench and LocAgent are useful secondary sources for issue-to-edit context and multi-hop graph localization, but their reported gains should not be assumed to transfer without a SymForge end-to-end run. [CORE-Bench](https://arxiv.org/abs/2606.11864), [LocAgent](https://arxiv.org/abs/2503.09089).

## Candidate decisions

| Candidate | Decision now | Evidence gate |
|---|---|---|
| Full 36 semantic tools | Keep as canonical | Must remain the success/trust baseline |
| Current compact three-tool facade as default | Do not promote | Must reach non-inferior success and trust under intent-to-treat analysis |
| Host-native deferred discovery over the full catalog | Highest-priority experiment | Must reduce total tokens without success, trust, or wall-time regression |
| Static reduced profile | Test only if needed | Use 8–12 existing leaf names; no new generic vocabulary |
| STEL economics/admission in full mode | Isolate and test later | Output-policy ablation must beat direct leaf calls end to end |
| Lower/adaptive CCR budgets | High-priority experiment | Optimize total trajectory cost, not response size |
| Task-conditioned graph-ranked repo map | Promising second-wave experiment | Beat current ranking at identical token budgets |
| `outputSchema`/`structuredContent` | Bounded trust experiment | Improve self-correction and reduce duplicate verification net of schema cost |
| Tool annotations | Keep exact and honest | Missing compact read annotations caused a real Codex noninteractive cancellation; never mark edit or mixed read/write tools as read-only |
| Server-side Code Mode | Defer | Only after simpler discovery/output work plateaus |
| Vector database/embeddings | No current case | Reconsider only after a measured lexical/symbol/graph recall gap |
| Tool-tip steering | Stop investing | Reopen only with new evidence showing lift over base rate |
| Generic schema/prompt compression | Reject | Risks deleting the semantic terms needed for routing |

## Experimental program

### Phase 0 — Freeze the benchmark contract

Do this before product changes.

1. Select eight fixed tasks across at least three repositories and two languages:
   - two orientation/localization tasks;
   - two symbol/reference tracing tasks;
   - two change-impact or debugging tasks;
   - two structural-edit tasks that require a trust receipt and post-edit impact check.
2. Record immutable starting commits or worktree snapshots, task prompts, success oracles, allowed tools, timeouts, model versions, and host versions.
   - An edit oracle must specify the expected symbol-level change, allowed file set, relevant focused test, absence of unrelated diffs, edit-receipt agreement with changed bytes, and the expected post-edit impact query.
3. Create two prompt strata:
   - neutral instructions that describe outcomes without naming SymForge tools;
   - realistic repository instructions that name preferred granular tools.
4. Randomize arm order and tool declaration order. Use two model families and at least two repeats for the pilot.
5. Capture complete host trajectories and final diffs. Server analytics are supplementary.
6. Predeclare exclusions: only host launch, authentication, unavailable-model, corrupted-fixture, or trace-capture failures before treatment exposure. Zero tool calls, bad routing, timeouts, retries, native fallback, and wrong answers remain scored outcomes.

Before the full pilot, run a variance shakedown:

- one repository;
- one host/model pair;
- two read-only tasks;
- arms A and C only;
- at least five repeats per task/arm, for 20 runs;
- a host-enforced logger for zero-call rate, first substantive tool, complete token accounting, wall time, and final-task oracle.

The shakedown is descriptive. Its deliverables are a trusted harness, stable task oracles, and an observed variance estimate. Use that variance to power the later pilot; do not assume 96 runs is sufficient or necessary.

### Phase 1 — Surface/discovery ablation

Run three arms first:

| Arm | Catalog semantics | Up-front exposure |
|---|---|---|
| A: full eager | Current 36 leaf tools | All schemas |
| B: full deferred | Same 36 leaf tools and implementations | Host-native tool search/deferred loading |
| C: compact current | Current three facades | All three schemas |

Do not add a rewritten compact schema or new grouped tools to this first experiment; that would mix diagnosis with repair.

Arm B is host-specific. Claude and OpenAI hosts implement different discovery systems, so analyze each host/model stratum separately and never pool B into a single deferred-discovery treatment estimate.

Outcome hierarchy:

- safety gate: exact task success under an intent-to-treat analysis;
- primary efficiency metric: total input + output tokens per assigned run; cached input is an informational subset and is not added again;
- secondary metric: wall-clock time to a verified final answer or diff; it cannot substitute for a token miss.

Adoption and routing outcomes:

- zero-SymForge-call rate;
- first substantive tool correctness;
- time and model turns to first useful evidence;
- invalid arguments, tool errors, immediate retries, and native fallbacks;
- tool yields, not merely tool-call counts;
- redundant full-file reads after equivalent SymForge evidence.

Trust outcomes:

- acceptance of correct freshness/truncation/provenance evidence;
- duplicate native verification after a correct envelope;
- edit receipt agreement with the actual changed bytes and dependent analysis;
- passively observed tool errors and self-correction.

Do not inject stale evidence, corrupt spans, or bad selectors in Phase 1. Those cases require the controlled fault-injection harness in Phase 4.

The shakedown and initial pilot are descriptive and cannot satisfy a confidence-interval gate. Power a confirmatory expansion from the observed variance, then require:

- success is non-inferior to full eager within a predeclared five-percentage-point margin;
- median total tokens improve by at least 15%;
- the confidence interval for the pre-registered token metric excludes no improvement;
- wall time is reported separately and cannot rescue a token miss;
- there is no material passive-trust regression.

Interpretation:

- If B wins, keep full canonical semantics and make deferred discovery the recommended integration for capable hosts.
- If C fails mainly by never being selected or by choosing the wrong intent, the root issue is interface affordance. Only then run a schema-only compact rescue.
- If C routes correctly but loses after execution, investigate STEL planning/admission/output policy separately.
- If A and B tie because schemas are cached or discovery adds a turn, retain eager full for that host.

### Phase 1B — Client-neutral fallback, only if Phase 1 justifies it

For clients that inject all schemas and lack tool search, create no new server behavior initially. Use host-side allowlists over eight to twelve existing leaf tools selected from observed workflows. Existing tool-catalog groups in [smart_query.rs](../../src/protocol/smart_query.rs#L743) provide candidate families.

Compare:

- the winning full configuration;
- canonical leaf allowlist;
- compact with high-signal descriptions and one example only for ambiguous intent families.

This distinguishes catalog size from generic-facade usability without reviving the rejected seven-verb vocabulary.

### Phase 2 — End-to-end output-budget calibration

Hold the winning surface fixed. Compare:

- current CCR defaults;
- a concise fixed policy, initially 2K/4K/8K by tool family;
- an adaptive policy based on match count, confidence, requested task, and continuation cost.

Every concise response must retain stable file/symbol/span identity, project selection, freshness or hash evidence, total hits, ranking reason where material, truncation state, and a deterministic continuation path.

Measure total trajectory tokens, wall time, final correctness, `symforge_retrieve` rate, extra yields, missed gold context, and raw-read fallback. A smaller first response that triggers expensive recovery is a loss.

Only after this result should STEL's existing admission/economic mechanisms be considered for direct full-surface calls. Port one policy at a time behind an experimental flag.

### Phase 3 — Fixed-budget retrieval ranking

At identical 1K, 4K, and 8K result budgets, compare:

- current ranking;
- task-conditioned symbol/reference graph ranking;
- graph ranking plus bounded reference expansion.

Use ContextBench-style evidence recall and precision, explored-versus-utilized context, time to first gold file/symbol, final task success, latency, and trajectory tokens. Implement the smallest algorithmic spike using the existing LiveIndex graph; add embeddings only if a real recall gap remains.

### Phase 4 — Calibrated trust and structured results

Run fault-injection cases for wrong project selection, stale snapshot evidence, parse quarantine, invalid arguments, truncated result sets, bad edit selectors, and concurrent/worktree edits.

Prototype structured output on only:

- one read/discovery tool, likely `search_text` or `get_symbol_context`;
- one structural edit path, paired with `edit_plan` and `analyze_file_impact`.

Compare prose + `_meta` against validated `outputSchema`/`structuredContent`, keeping compatibility text available. Measure self-correction, repeated failures, duplicate verification, schema-token overhead, receipt/diff agreement, and unsafe acceptance.

### Phase 5 — Optional high-complexity technologies

Evaluate programmatic/code-mode calling only if Phases 1–4 leave a demonstrated multi-call latency or context-pollution bottleneck. Require a threat model and sandbox first. Do not add a semantic sidecar or vector database without a measured retrieval-recall problem that the existing symbol/reference graph cannot solve.

## Minimal next action

The reviewed harness found that the current compact treatment is not executable under safe noninteractive Codex: its production `tools/list` objects omit trust annotations, so even the read facade enters approval and is cancelled. Preserve that baseline, amend and re-review the manifest, then make the one metadata prerequisite that truthfully marks only compact `symforge` read-only/closed-world. Keep `status` and `symforge_edit` non-read-only.

Build one pinned candidate binary outside the low-space repository drive, regenerate the golden state, and restart the 20-run A-versus-C variance shakedown from run 01. Use its harness and variance result to size the host-stratified pilot. The confirmatory result—not intuition about 4.6 KB versus 72.8 KB schemas—decides whether later implementation begins with host integration, compact affordances, output policy, or nothing.

The first amended retry exposed a harness-only snapshot invariant error before treatment. A health-only process can legitimately rewrite `index.bin`: fresh worktree mtimes force background reconciliation, clean shutdown re-reads mtimes, and postcard serialization of a randomly seeded `HashMap` is not byte-stable across processes. The corrected gate preserves exact golden bytes only at input, waits for completed zero-mismatch verification, then compares every run to a no-overwrite semantic baseline (fixed Git tree, tracked-source content manifest, deterministic per-file outline digest, index/parse counts, candidate SHA/version). Opus approved this revision after requiring those semantic checks. The measured process still performs stat-all plus 10% spot verification; that smaller symmetric product behavior is an explicit speed confound, not hidden as “race removed.”

## Claims explicitly not made

- The prior 24.8% result does not prove explicit SymForge calls caused all savings.
- A 93.7% schema-byte reduction does not imply a 93.7% session-token reduction.
- Vendor tool-search results do not establish SymForge gains until replicated.
- The compact facade is not proven fundamentally incapable; its current implementation is unproven as the default.
- `outputSchema`, annotations, Code Mode, graph ranking, and adaptive CCR are candidates, not promised improvements.
- No new database, dependency, facade, or code path is justified before the corresponding gate passes.
