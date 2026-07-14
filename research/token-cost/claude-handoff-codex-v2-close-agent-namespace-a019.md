# Claude handoff — Codex V2 `close_agent` live namespace checkpoint

You are the independent reviewer. This checkpoint was triggered by a live API
failure found only after your prior source review returned
`APPROVE_CONTINUE`. Work read-only. Do not edit Codex, SymForge, user config,
or process state.

## Required order

1. Read this handoff completely.
2. Read
   `research/token-cost/codex-v2-close-agent-namespace-diagnostic-2026-07-14.md`.
3. Read your prior report
   `research/token-cost/claude-report-codex-v2-close-agent-code-review-a019.md`.
4. Inspect the uncommitted Codex worktree at
   `E:\project\codex-v2-close-agent`, pinned to `rust-v0.144.4` base
   `8c68d4c87dc54d38861f5114e920c3de2efa5876`.
5. Independently verify the relevant configuration, spec-plan, handler, and
   test facts before judging the recommendation.
6. Write your report to
   `research/token-cost/claude-report-codex-v2-close-agent-namespace-a019.md`.

## New live fact that must govern the review

The standalone candidate is SHA-256
`86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00`.
Against the ChatGPT-backed API, its default V2 tool plan is rejected before
model execution with HTTP 400 because
`collaboration.close_agent` is not allowed in the reserved `collaboration`
namespace. The identical candidate is accepted when the existing setting
`features.multi_agent_v2.tool_namespace='agents'` is supplied.

The configured-namespace process observation was:

- root MCP PID 62076;
- root plus worker MCP PIDs 62076 and 59356;
- `close_agent` completed with previous status `completed` and worker message
  `WORKER_READY`;
- worker PID 59356 exited at close while root PID 62076 remained;
- post-close `followup_task` was rejected because canonical path
  `/root/sentinel_worker` no longer existed;
- root PID 62076 exited only at normal root-session exit;
- a separately owned unrelated sentinel stayed alive until explicit cleanup.

The controller self-graded that run `FAIL` only because its two 15-second MCP
hold calls were client-cancelled. Do not relabel it a full passing smoke. Judge
it as an A/B namespace diagnostic and close-path mechanism proof. The proposed
final smoke shortens each hold to 5 seconds.

## Questions you must answer

1. Does the HTTP 400 plus accepted `agents` control isolate the deployment
   blocker to the reserved namespace contract, or is another local cause still
   plausible?
2. Is using the existing whole-surface V2 `tool_namespace='agents'` setting the
   smallest reversible local deployment fix?
3. Would changing `DEFAULT_MULTI_AGENT_V2_TOOL_NAMESPACE` in this patch be an
   unjustified compatibility expansion because it renames all V2 tools?
4. Is a split namespace for only `close_agent` worse than the supported
   whole-surface override?
5. Can the seven-file handler patch remain unchanged for an upstream review,
   provided the live reserved-namespace dependency is disclosed, or must the
   source patch itself change before commit?
6. Does the observed 1 → 2 → 1 → 0 MCP sequence prove normal worker teardown
   at close while preserving the root?
7. Is the rejected post-close follow-up sufficient evidence that the V2
   residency/target entry was removed?
8. Are 5-second holds a sound repair for the final smoke, or should a different
   deterministic observation gate be used?
9. Did cleanup remove every disposable auth-bearing/smoke artifact while
   retaining only what is needed for the next checkpoint?
10. Given the helper-complete package result (2,582 pass, 11 fail, 1 timeout,
    none in V2 close/spec-plan coverage), is it reasonable to continue with a
    scoped final gate while reporting the full Windows package gate red?

## Required verdict

Choose exactly one:

- `APPROVE_CONFIG_DEPLOY` — keep product diff unchanged, use `agents` for the
  local install/final smoke, disclose the backend dependency upstream;
- `CHANGES_REQUIRED` — list the minimum exact source/test/evidence changes;
- `BLOCKED_BACKEND` — no honest local deployment can proceed without a server
  allowlist change.

Include:

- independent findings for all ten questions;
- any blocker separated from non-blocking upstream recommendations;
- exact recommended final-smoke protocol;
- exact commit/install scope if progression is approved;
- a concise note on what must be reported to upstream Codex maintainers.

Do not run the model-backed smoke, alter user config, install a binary, commit,
or delete the retained Codex worktree/target. Those remain primary-agent work
after your verdict.
