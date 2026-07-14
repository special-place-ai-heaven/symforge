# Independent review handoff: Codex V2 close-agent research

Perform a read-only, adversarial review of the proposed Codex MultiAgentV2
lifecycle repair. Do not edit code, configuration, tasks, caches, or process
state.

Write your report to exactly:

`research/token-cost/claude-report-codex-v2-close-agent-research-a019.md`

## Required review order

1. Independently inspect the official OpenAI Codex source at tag
   `rust-v0.144.4` and current `main` before reading the primary report.
2. Build a private fact table for:
   - V1 versus V2 registered collaboration tools;
   - V2 target resolution and agent-control close path;
   - multi-agent-version selection precedence;
   - V2 residency/LRU behavior and effective concurrency/depth controls;
   - MCP shutdown behavior on normal session closure;
   - the unbounded termination wait described by issue #25426.
3. Then read:
   `research/token-cost/codex-v2-close-agent-reconnaissance-2026-07-14.md`.
4. Compare the report against your private extraction and inspect the local
   task/lesson corrections in `tasks/todo.md` and `tasks/lessons.md`.

## Questions you must answer

1. Is `STAY-AND-FIX` supported, or is the defect actually patchable in a
   plugin, MCP, JavaScript launcher, or configuration layer?
2. Does model-catalog/session metadata genuinely explain V2 despite a local
   `multi_agent_v2=false` result?
3. Is the report precise that V2 residency is bounded retention rather than
   proof of an unlimited V2 agent-count leak?
4. Is adding a V2-native `close_agent` the smallest safe repair that preserves
   task-path routing and reusable-agent semantics?
5. Are the proposed code locations and red/green tests complete enough to catch
   a handler that is listed but cannot resolve/close by canonical task name?
6. Does the Windows sentinel-MCP smoke prove the user-visible outcome without
   killing unrelated processes?
7. Does the plan correctly separate normal completed-agent closure from the
   pathological hang in issue #25426, without falsely claiming failure-proof
   cleanup?
8. Are any claims about `agents.max_threads`, `agents.max_depth`, or V2 limits
   still inaccurate?

## Required output

Start with exactly one verdict:

- `VERDICT: APPROVE_IMPLEMENTATION_PLAN`
- `VERDICT: CHANGES_REQUIRED`
- `VERDICT: BLOCKED`

Then provide:

- independently verified facts;
- blocking findings, if any;
- non-blocking risks;
- the minimum corrected implementation/test sequence;
- an explicit judgment on whether Codex-core work may begin without changing
  SymForge product code.

Use primary sources and direct links. Do not expose any credential or secret
value; names and locations only if one is encountered.
