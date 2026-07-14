# Independent review: Codex V2 `close_agent` live namespace checkpoint (A019)

Reviewer: Claude (Fable 5), 2026-07-14. Read-only; no Codex, SymForge, config,
or process state touched. Order followed: handoff → namespace diagnostic →
prior code-review report → independent worktree inspection.

## Custody verified independently

- Worktree `E:\project\codex-v2-close-agent` still at base
  `8c68d4c8…` (`rust-v0.144.4`); dirty state is exactly the reviewed
  seven-file patch (6 modified + new `multi_agents_v2/close_agent.rs`,
  69 insertions); `Cargo.lock` clean. The product diff is unchanged since my
  `APPROVE_CONTINUE` review.
- Candidate binary `codex-rs/target/debug/codex.exe` hashes to
  `86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00` —
  byte-identical to the binary named in the diagnostic. The A/B evidence and
  this review judge the same artifact.
- `config/mod.rs:251`: `DEFAULT_MULTI_AGENT_V2_TOOL_NAMESPACE = "collaboration"`.
- `config/mod.rs:2859` `validate_multi_agent_v2_tool_namespace`: the CLIENT
  reserved list covers Responses-API namespaces (`functions`, `browser`,
  `python`, …) and does **not** include `collaboration` or `agents`; `agents`
  is a legal value of the existing whole-surface setting
  `features.multi_agent_v2.tool_namespace`. Nothing client-side rejects the
  default plan — consistent with the 400 originating server-side.
- The namespace plumbing is whole-surface by construction: one
  `tool_namespace` value feeds `multi_agent_v2_handler(…, tool_namespace)`
  for all seven V2 handlers in `spec_plan.rs`; there is no per-tool namespace
  mechanism in the codebase.

## Answers to the ten questions

**1. Is the blocker isolated to the reserved-namespace contract?** Yes, to
the limit of what a client-side reviewer can verify. The A/B held the binary
(hash-verified identical), model, auth route, prompt, isolated `CODEX_HOME`,
and MCP sentinel constant, changing only `tool_namespace`. The 400 text
("Function 'collaboration.close_agent' is not allowed in reserved namespace
'collaboration'") names a server-side function allowlist inside the reserved
namespace, and rejection occurred at tool declaration, before model
execution. No local cause is plausible: client validation permits both
values, the plan is identical apart from the namespace prefix, and the same
declaration was accepted under `agents`. The server allowlist itself is
unobservable from here — that residual uncertainty is inherent and does not
change the decision.

**2. Is `tool_namespace='agents'` the smallest reversible local fix?** Yes.
It is an existing, validated, documented-in-code configuration setting; it
changes zero source lines; it is reversible by deleting one TOML line; and it
is exactly the configuration the patch's own
`multi_agent_v2_can_use_configured_tool_namespace` test already proves. Live
evidence confirms it end to end.

**3. Would changing the source default be unjustified?** Yes. Flipping
`DEFAULT_MULTI_AGENT_V2_TOOL_NAMESPACE` renames all seven V2 functions for
every V2 session of every user of the patched build, contradicts the
shared usage-hint's own example (`to=functions.collaboration.spawn_agent`),
and silently diverges from upstream's intended contract — models and
server-side behavior may be tuned to the `collaboration` names. That is a
product/compatibility decision for upstream, not something to smuggle into a
lifecycle bridge. Keep the default untouched.

**4. Is a split namespace for only `close_agent` worse?** Yes, strictly. It
would require new per-tool namespace plumbing that does not exist (the
setting is whole-surface), enlarging the patch; it would fragment the
collaboration surface across two namespaces, confusing models and breaking
the "one namespace, one tool family" assertions in the existing tests; and
it still depends on the server accepting the new name somewhere. The
supported whole-surface override dominates it on every axis.

**5. Can the seven-file patch go upstream unchanged?** Yes. The handler is
correct source; the blocker is a deployment-environment allowlist that
upstream itself controls — when OpenAI merges a V2 `close_agent`, the
server-side `collaboration` allowlist addition is their half of the change.
The upstream submission MUST disclose: (a) the exact 400 rejection text
against the ChatGPT-backed API, (b) that the tool is live-verified only via
the `tool_namespace` override, and (c) that the server allowlist needs
`collaboration.close_agent` before the default namespace works. No source or
test change is required before commit; the existing configured-namespace
test already covers the deployed shape.

**6. Does 1 → 2 → 1 → 0 prove worker teardown preserving root?** Yes, as a
mechanism proof. The owned-MCP census rose to two on spawn, fell to one at
the moment `close_agent` completed (worker PID 59356 exited, root PID 62076
survived), and reached zero only at normal root-session exit — while a
separately owned unrelated sentinel survived the whole sequence, ruling out
collateral kills. This is exactly the user-visible outcome the fix exists
for: explicit release of a completed worker's process stack without touching
the root or unrelated processes.

**7. Is the rejected post-close follow-up sufficient?** Sufficient for what
it claims: `followup_task` failing with `live agent path
'/root/sentinel_worker' not found` proves the live registry/target entry was
removed (that error can only come from `resolve_agent_reference` after
`remove_thread`/`forget_v2_residency`). It proves target-entry removal, not
slot arithmetic; the residency slot release is code-verified
(`shutdown_live_agent` → `release_spawned_thread`) but not yet live-proven.
A one-line addition to the final smoke closes this: after close, spawn a
replacement worker and assert it succeeds (see protocol below).

**8. Are 5-second holds sound?** Acceptable, with one grading correction.
The 15-second holds were client-cancelled; 5 seconds sits below the observed
cancellation boundary and preserves the observation window against the
250 ms transition monitor. But the deeper flaw in the failed run was grading
on hold completion at all: the holds are scaffolding, not treatment. The
final smoke should grade PASS/FAIL exclusively on deterministic
observations — process-census transitions, tool-call results, and exit
codes — and record hold cancellation as a harness note, not a failure. With
that rule, 5-second holds are fine; even a cancelled hold would not corrupt
the verdict.

**9. Cleanup complete?** Yes, per the diagnostic's custody section and
spot-checked: the isolated smoke `CODEX_HOME` (containing a copied auth
file) and the empty smoke workspace were deleted; the monitor and the
unrelated sentinel were stopped explicitly; no auth-bearing or smoke
artifact exists under `research/token-cost/evidence/` (grep for
auth-material references returned nothing) and no smoke directory remains
beside the worktree. Retained items are exactly what the next checkpoint
needs: the pinned worktree and its Cargo target. No credential value appears
in any evidence file.

**10. Continue on a scoped final gate with the package gate red?** Yes,
honestly framed. 2,582/2,594 passed; the 11 failures + 1 timeout are all
outside the seven-file patch and match known environmental classes (Windows
symlink/elevation, a missing unrelated helper binary, network/mock timing,
hook-log flakiness, CLI-stream leakage, remote compaction timeout), and no
V2 close/spec-plan test failed. Continuing with a scoped gate is reasonable
**provided** the report to any downstream consumer states the full Windows
package gate is red. Non-blocking hardening: re-run one or two of the
failing tests on the clean base tag to positively pin them as pre-existing
rather than argued-from-category; this is cheap and removes the last
inference.

## Blockers

None for the config-deploy path.

## Non-blocking upstream recommendations

1. Disclose the reserved-namespace 400 verbatim and the `tool_namespace`
   dependency in the upstream PR; request the server allowlist addition of
   `collaboration.close_agent`.
2. Add root/self-rejection and capacity-release (close → replacement spawn)
   tests before upstream submission (carried over from the prior review).
3. Pin one failing package-gate test to the clean base to convert
   "categorically unrelated" into "measured pre-existing".

## Recommended final-smoke protocol (exact)

Standalone candidate (hash `86B8…2E00`), isolated `CODEX_HOME` + empty
workspace as in the diagnostic, `features.multi_agent_v2=true`,
`tool_namespace='agents'`, model `gpt-5.6-sol` low reasoning, 250 ms
transition monitor, separately owned unrelated sentinel:

1. Init root MCP → assert owned census 1.
2. Spawn `/root/sentinel_worker`; await `WORKER_READY` → assert census 2.
3. Hold 5 s (observation only — cancellation is a note, never a failure).
4. `close_agent` on `/root/sentinel_worker` → assert result
   `previous_status=completed`, census 2 → 1 with the worker PID the one
   that exited, root PID alive.
5. `followup_task` on the closed path → assert rejection
   (`live agent path … not found`).
6. **New:** spawn a replacement worker → assert success and census 1 → 2,
   then close it → census 2 → 1 (proves slot release live).
7. Hold 5 s; assert unrelated sentinel alive.
8. Exit root session normally → assert census → 0; stop monitor and
   sentinel explicitly; delete the isolated home (contains copied auth)
   and the workspace immediately.
9. Grade PASS solely on assertions 1–2 and 4–8's deterministic observations
   and process exits.

## Approved commit/install scope (on progression)

- Commit: exactly the current seven-file product diff on
  `fix/multi-agent-v2-close-agent`; `Cargo.lock` stays clean; no default
  namespace change; no smoke scripts or evidence in the Codex repo.
- Install: the pinned standalone binary (never overwriting the npm cache in
  place) plus the single reversible user-config line
  `features.multi_agent_v2.tool_namespace = "agents"`, documented alongside
  the install so it is removed if/when upstream allowlists
  `collaboration.close_agent`.
- After install + restart: verify the live tool surface once, then clean the
  disposable Cargo target and worktree per the finishing checklist.

## Note for upstream Codex maintainers

The V2 collaboration surface omits `close_agent` (V1 has it), so completed
V2 residents cannot be explicitly released. This seven-file patch adds a
V2-native `close_agent` bridging to `AgentControl::close_agent`. It is
live-blocked by the backend: the ChatGPT-backed API returns HTTP 400
"Function 'collaboration.close_agent' is not allowed in reserved namespace
'collaboration'" — the reserved-namespace function allowlist must add
`collaboration.close_agent` server-side. The identical build works today via
`features.multi_agent_v2.tool_namespace` overriding the namespace, which is
how the close path (worker MCP teardown at close, root preserved, post-close
follow-up rejected) was verified end to end.

VERDICT: APPROVE_CONFIG_DEPLOY
