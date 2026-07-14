# Codex V2 `close_agent` reconnaissance

Date: 2026-07-14<br>
Scope: read-only investigation; no SymForge or Codex product code changed<br>
Decision: **STAY-AND-FIX**

## Executive result

The completed-worker process retention is not caused by SymForge, Terminal
Commander, an MCP bridge, or an updateable Codex plugin. It is a gap in the
compiled Codex MultiAgentV2 tool surface:

- V1 exposes `close_agent` and routes it to `AgentControl::close_agent`.
- V2 exposes spawn, send, follow-up, wait, interrupt, and list, but no close.
- V2 deliberately keeps terminal agents resident for reuse and only LRU-unloads
  one when a later spawn needs capacity.
- The model catalog can select V2 before local feature configuration is used,
  which explains why `codex features list` reports `multi_agent_v2=false` while
  this session still receives the V2 collaboration tools.

The minimal durable repair is to expose a V2-native `close_agent` that resolves
a task path or thread id and calls the existing core close path. Do this in a
separate `openai/codex` checkout, not in SymForge and not by modifying the npm
launcher/cache.

## Facts established

| Fact | Primary evidence | Consequence |
|---|---|---|
| Installed Codex is current stable `0.144.4`. | [Release 0.144.4](https://github.com/openai/codex/releases/tag/rust-v0.144.4); local `codex --version` returned `codex-cli 0.144.4`. | Updating the package is not the fix. |
| The npm package is a launcher for the native executable. | Local `@openai/codex/bin/codex.js`; no collaboration implementation was found in installed plugin/skill sources. | A plugin or MCP update cannot reach internal `AgentControl`. |
| V1 already has explicit closure. | [`spec_plan.rs` V1 registration](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/tools/spec_plan.rs#L814-L835), [`close_agent.rs`](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/tools/handlers/multi_agents/close_agent.rs). | Reuse the existing close machinery. |
| V2 omits closure. | [`spec_plan.rs` V2 registration](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/tools/spec_plan.rs#L765-L813), [`multi_agents_v2.rs`](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/tools/handlers/multi_agents_v2.rs#L29-L41). Current `main` also has no V2 close module. | The missing native bridge is the direct product gap. |
| Model/session metadata wins over the local feature fallback. | [`resolve_multi_agent_version_for_model`](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/session/mod.rs#L3109-L3122). | `features.multi_agent_v2=false` cannot override a host-selected V2 session. |
| V2 retention is bounded and intentional. | [`residency.rs`](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/agent/control/residency.rs#L79-L149). | Call it missing explicit release/resource retention, not an unlimited V2 agent-count leak. |
| V2 does not use the V1 concurrency/depth settings as previously claimed. | [`Config::effective_agent_max_threads`](https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/config/mod.rs#L1431-L1443) and the V2 branch in `collab_tools_enabled`. | `[agents].max_threads=4` and `max_depth=1` do not prove the V2 limits; the current V2 default is independently root plus three residents, and V2 bypasses the V1 depth check. |
| Normal session shutdown now explicitly drains MCP clients and terminates stdio process groups. | Merged [PR #19753](https://github.com/openai/codex/pull/19753); the tagged shutdown path calls `latest_mcp_runtime().manager_arc().shutdown().await`. | A successful close should reclaim a completed agent's MCP stack on the normal path. |
| Existing close can still hang pathologically. | [`shutdown_live_agent`](https://github.com/openai/codex/blob/d7ba5ff9553a6aa0898a8e3bd5cb3bc00d0c9ddf/codex-rs/core/src/agent/control/legacy.rs#L6-L24), open [issue #25426](https://github.com/openai/codex/issues/25426). | Do not claim failure-proof cleanup from the V2 surface patch alone. |

Upstream reports remain consistent with the observed lifecycle family:
[subagent MCP stacks #17574](https://github.com/openai/codex/issues/17574) and
[missing lifecycle controls #19197](https://github.com/openai/codex/issues/19197).

## Why the tempting alternatives are wrong

| Option | Verdict | Reason |
|---|---|---|
| Update a plugin/bridge/MCP | Reject | The tool registry and `AgentControl` are compiled into native Codex. |
| Patch `codex.js` or npm package internals | Reject | The JavaScript file only selects and launches the platform binary. It has no thread registry handle. |
| Set only `features.multi_agent_v2=false` | Insufficient here | Model metadata selects V2 before the feature fallback. |
| Start a new session on a V1-tagged model | Valid short-term containment | It restores the V1 close surface, but changes the model/tool contract and cannot repair the already-selected version of this session. |
| Add a SymForge cleanup daemon or process poller | Reject | It cannot safely release Codex's in-memory task graph/slot and would kill resources it does not own. |
| Rely on SymForge daemon idle shutdown | Defense in depth only | A resident Codex proxy keeps authenticated traffic alive; the daemon is still useful only after the proxy is actually gone. |
| Reduce orchestration to one agent | Reject as the main answer | It sacrifices useful parallelism and does not repair lifecycle semantics. |
| Patch native V2 closure | Choose | It preserves V2 task paths/reuse while giving the orchestrator an explicit release operation. |

## Minimal implementation checkpoint

Base a small branch on the exact Codex version used for validation (initially
`rust-v0.144.4`; rebase the same change onto current `main` for upstream review).

1. Red first: add `close_agent` to the expected V2 tool/namespace list in
   `codex-rs/core/src/tools/spec_plan_tests.rs` and prove it fails.
2. Add `create_close_agent_tool_v2()` in
   `codex-rs/core/src/tools/handlers/multi_agents_spec.rs`. It must be a normal
   V2 function spec whose `target` accepts a canonical task path or thread id.
3. Add `codex-rs/core/src/tools/handlers/multi_agents_v2/close_agent.rs`.
   Follow the V2 interrupt handler for target resolution and root/self guards,
   capture the previous status, then call the existing
   `session.services.agent_control.close_agent(agent_id)` path.
4. Export the handler from `multi_agents_v2.rs` and register it beside
   interrupt/list in `spec_plan.rs`, including the optional V2 namespace
   override used by this host.
5. Change the existing `multi_agent_v2_list_agents_omits_closed_agents` test to
   invoke the new model-visible handler by task name instead of calling core
   closure directly. This proves surface -> V2 resolution -> core closure ->
   list removal without inventing another fixture.
6. Keep the first patch scoped to successful explicit closure. Do not mix in a
   speculative process reaper, automatic idle policy, or unsafe force-detach.

Required tool wording: completed V2 agents remain reusable/resident until
closed or LRU-unloaded; callers should close an agent when no follow-up work is
expected. This gives models an honest reason and time to call it.

## Verification gates

The change is not complete until all of the following are green:

1. Tool-plan/spec red then green for both plain and namespaced V2 surfaces.
2. Handler test closes by canonical task name and rejects root/self targets.
3. Existing list test proves the closed task disappears while siblings remain.
4. Registry-capacity test proves closing a completed worker immediately permits
   a replacement spawn.
5. Sentinel stdio-MCP smoke on Windows:
   - start a worker whose session owns a sentinel helper process;
   - let the worker complete;
   - call V2 `close_agent`;
   - assert the agent process and the entire helper tree exit;
   - assert the primary Codex process and unrelated processes survive.
6. Focused core tests, formatting, clippy, and the proportionate workspace gate.
7. Run the patched binary from a pinned standalone path. Never overwrite the
   npm cache in place.

Because disk is constrained, use one shallow/filtered Codex checkout and one
disposable Cargo target. Record its size before/after, then delete the target
and temporary worktree immediately after the checkpoint.

## Separate hardening checkpoint

The V2 close bridge fixes the observed completed-idle case, but the existing
close path can wait forever if a thread does not terminate.

Safe behavior for that later checkpoint is:

- bound the wait;
- return an explicit `shutdown_timed_out`/failure result;
- retain the live registry/residency entry unless real teardown is confirmed;
- never report success or release a slot while an MCP/session process may still
  be alive.

Actual forced cleanup requires a verified thread-session abort/MCP teardown
primitive. Merely timing out and dropping the registry entry would hide the
leak rather than fix it. Treat [issue #25426](https://github.com/openai/codex/issues/25426)
as a separate required trust-hardening checkpoint.

## Temporary containment until the patch is running

- Keep the V2 resident total at four (root plus three workers). This bounds the
  retained stacks without eliminating useful parallel work.
- If immediate built-in orchestration is essential, use a fresh session on a
  model whose catalog selects V1; do not present that model switch as the
  durable repair for V2.
- Do not claim `[agents].max_depth=1` prevents recursive V2 fan-out.
- Prefer one-shot external/headless workers or the existing second-terminal
  artifact workflow when practical; process exit exercises normal shutdown.
- After a built-in worker completes, verify and remove only its exact process
  tree. Restart the root Codex session after a heavy burst if immediate slot
  reclamation matters.
- Do not spawn additional built-in agents in this session after this research
  checkpoint; the completed researcher remains logically registered because
  this V2 surface still lacks close.

## This checkpoint's cleanup receipt

The single tech-researcher worker owned a verified 22-process tree rooted at
its launcher. After its final result was received, all 22 exact descendants
were terminated; zero remained, and the primary Codex process stayed alive.
This is containment evidence only. The collaboration registry still has no
model-visible close operation in the current session.
