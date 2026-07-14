# Independent review: Codex V2 close-agent research (a019)

VERDICT: APPROVE_IMPLEMENTATION_PLAN

Reviewer: Claude (Fable 5), 2026-07-14. Read-only. Method: independent shallow
clones of `openai/codex` at tag `rust-v0.144.4` and current `main` were
inspected and a private fact table built BEFORE reading
`research/token-cost/codex-v2-close-agent-reconnaissance-2026-07-14.md`, per
the handoff order. Issue and PR metadata pulled live via the GitHub API.

## Independently verified facts

All file references are to `codex-rs/core/src/` at tag `rust-v0.144.4` unless
noted; `main` (cloned 2026-07-14) was diff-checked for each.

1. **V1 vs V2 tool surfaces.** V1 handlers:
   `tools/handlers/multi_agents/{spawn,send_input,resume_agent,wait,close_agent}.rs`,
   all registered in `tools/spec_plan.rs` (`CloseAgentHandler` at ~L835). V2
   handlers: `tools/handlers/multi_agents_v2/{spawn,send_message,followup_task,wait,interrupt_agent,list_agents}.rs`
   (+ internal `message_tool.rs`), registered at `spec_plan.rs` ~L764–813.
   **No V2 close handler exists at the tag or on current `main`**
   (`multi_agents_v2/` directory contents identical on both). The default V2
   usage hint (`config/mod.rs` ~L252) enumerates exactly the six tools —
   `spawn_agent`, `send_message`, `followup_task`, `wait_agent`,
   `interrupt_agent`, `list_agents` — confirming close is absent from the
   model-facing contract, not merely unregistered.
   https://github.com/openai/codex/blob/rust-v0.144.4/codex-rs/core/src/tools/handlers/multi_agents_v2.rs

2. **V2 target resolution and close path.** V2 handlers resolve targets via
   `agent/agent_resolver.rs::resolve_agent_target` → raw `ThreadId` parse,
   else `AgentControl::resolve_agent_reference` (`agent/control.rs` L326),
   which resolves a canonical `AgentPath` (task name) against live agents. The
   core close machinery exists and is V2-aware:
   `AgentControl::close_agent` → `shutdown_agent_tree` → `shutdown_live_agent`
   (`agent/control/legacy.rs`), and `shutdown_live_agent` already calls
   `forget_v2_residency(agent_id)` and `release_spawned_thread(agent_id)`. So
   a V2 tool bridging to this path needs no new core teardown logic.

3. **Version selection precedence.**
   `Session::resolve_multi_agent_version_for_model` (`session/mod.rs`
   L3109–3123): (a) session-sticky value (OnceLock; inherited/resumed history
   via `resolve_multi_agent_version`, L450) wins; (b) else
   `ModelInfo.multi_agent_version` from the model catalog; (c) only then
   `Config::multi_agent_version_from_features` (`config/mod.rs` L1410). So a
   local `multi_agent_v2=false` feature result is genuinely overridden by
   model-catalog or stored-session metadata. Verified as claimed.

4. **V2 residency/LRU.** `agent/control/residency.rs`: capacity =
   `effective_agent_max_threads(V2)` =
   `max_concurrent_threads_per_session - 1` (`config/mod.rs` L1431–1443);
   default `DEFAULT_MULTI_AGENT_V2_MAX_CONCURRENT_THREADS_PER_SESSION = 4`
   (`config/mod.rs` L208) → root plus three residents. At capacity,
   `try_unload_one_resident` unloads one LRU resident that is terminal
   (`Completed|Errored|Interrupted`, no active turn), calling
   `shutdown_and_wait` before `remove_thread`. Retention is therefore
   **bounded** and unload-on-pressure is real; there is no unbounded
   agent-count leak, only no explicit release primitive.

5. **Concurrency/depth controls.** `agents.max_threads` applies only to
   Disabled/V1; setting it with `features.multi_agent_v2` enabled is a hard
   config validation error (`validate_multi_agent_v2_config`, `config/mod.rs`
   L1420). `exceeds_thread_spawn_depth_limit` is enforced only in V1 spawn
   (`multi_agents/spawn.rs` L67), V1 resume (L54), and the V1 arm of
   `collab_tools_enabled` (`spec_plan.rs` L346); the V2 arm returns `true`
   unconditionally. So `[agents].max_depth` does NOT gate V2 fan-out; V2
   depth is bounded only indirectly by the shared residency capacity.

6. **MCP shutdown on normal closure.** `session/handlers.rs`:
   `Op::Shutdown` (L841) → `shutdown` → `shutdown_session_runtime` (L599),
   which calls `latest_mcp_runtime().manager_arc().shutdown().await`;
   `rmcp_client.rs::shutdown` (L731) terminates the stdio server process, and
   the process-group kill path exists in `stdio_server_launcher.rs`. PR
   #19753 ("Terminate stdio MCP servers on shutdown to avoid process
   leaks") merged 2026-04-28 — verified via API. So a successful close of a
   completed agent's session does reclaim its MCP stack on the normal path.

7. **Issue #25426 (open, verified via API).** `close_agent` persists the
   spawn edge as closed, then `shutdown_live_agent` awaits
   `wait_until_terminated()` **with no timeout**; the registry slot releases
   only afterward. If the child thread never terminates, close hangs forever
   and the slot is never freed. This code is unchanged on `main`. The
   pathological-hang risk is real and correctly scoped as a separate concern.

8. **Plan anchors exist.** `create_close_agent_tool_v1()` at
   `multi_agents_spec.rs` L285 (natural neighbor for a `_v2` variant);
   `interrupt_agent.rs` shows the exact root/self-guard + `resolve_agent_target`
   + status-capture pattern the plan says to copy; test
   `multi_agent_v2_list_agents_omits_closed_agents` exists at
   `multi_agents_tests.rs` L1690 and currently spawns via the V2 handler but
   would close via core directly — exactly the seam the plan proposes to
   re-route through the new handler. The V2 namespace override wrapper
   (`MultiAgentV2NamespaceOverride`, `spec_plan.rs` ~L1022) applies at
   registration, so registering the new handler beside interrupt/list covers
   the namespaced surface too.

## Answers to the eight questions

1. **STAY-AND-FIX supported?** Yes. The tool registry, `AgentControl`, and
   residency are `pub(crate)` internals of the compiled `codex-core` crate.
   No plugin, MCP server, extension tool, or launcher-JS layer can reach
   `AgentControl::close_agent`; MCP tools are model-callable but execute in
   external processes with no thread-registry handle. No configuration knob
   produces immediate release (residency unload is spawn-pressure-driven
   only). The defect is patchable only in Codex core.

2. **Model-catalog/session metadata explains V2 despite local
   `multi_agent_v2=false`?** Yes — precedence verified at
   `session/mod.rs:3109` (fact 3). The report's claim is precise; it is also
   correct that the version is session-sticky (OnceLock), so flipping the
   feature cannot repair an already-selected session.

3. **Bounded retention vs. leak?** The report is precise: "bounded and
   intentional", "missing explicit release/resource retention, not an
   unlimited V2 agent-count leak" matches `residency.rs` exactly, including
   the root-plus-three default.

4. **Is a V2-native `close_agent` the smallest safe repair?** Yes. It reuses
   the existing, V2-residency-aware core close path (fact 2), preserves task
   paths (resolution via `resolve_agent_reference`), and leaves the
   reuse/residency model intact for agents not explicitly closed. Every
   rejected alternative in the report's table is rejected for reasons my
   extraction confirms.

5. **Code locations and red/green tests complete?** Yes, adequate. Step 5
   (re-route `multi_agent_v2_list_agents_omits_closed_agents` through the
   model-visible handler by task name) plus gate 2 (root/self rejection) plus
   gate 4 (capacity release enables replacement spawn) together catch a
   handler that is listed but cannot resolve or close by canonical task name.
   One small addition recommended (non-blocking, below).

6. **Windows sentinel smoke sound?** Yes. Asserting the worker's exact
   process tree exits while the primary Codex process and unrelated processes
   survive is the correct user-visible outcome, and the tag's normal-shutdown
   MCP teardown (fact 6) makes the assertion achievable without any external
   killing. It is a positive-path smoke; it does not (and does not claim to)
   cover the #25426 wedge.

7. **Normal closure vs. #25426 hang separated correctly?** Yes. The report
   explicitly refuses to claim failure-proof cleanup, defers the unbounded
   `wait_until_terminated` to a separate hardening checkpoint, and correctly
   warns that timing out and dropping the registry entry would hide the leak.
   Its proposed hardening semantics (bounded wait, explicit
   `shutdown_timed_out`, retain the slot until real teardown is confirmed)
   are consistent with the code's slot-release ordering.

8. **Any remaining inaccuracies about `agents.max_threads`/`max_depth`/V2
   limits?** No. The corrected claims in the report, `tasks/todo.md` (item
   "Confirm V2's current resident bound…"), and `tasks/lessons.md`
   (2026-07-14 entry) all match source. One point is even understated: with
   `features.multi_agent_v2` locally enabled, setting `agents.max_threads` is
   a hard validation **error**, not merely ignored (`config/mod.rs` L1420).

## Blocking findings

None.

## Non-blocking risks

- **Namespaced dispatch test.** Gate 1 covers spec presence for plain and
  namespaced surfaces; add one dispatch assertion that the handler is
  invocable under the `collaboration` namespace override, since the override
  wraps the executor (`MultiAgentV2NamespaceOverride`), not just the spec.
- **Usage-hint drift.** `DEFAULT_MULTI_AGENT_V2_SHARED_USAGE_HINT_TEXT`
  (`config/mod.rs` L252) hard-enumerates the six existing tools. The patch
  must add `close_agent` there (and to the concurrency-slot phrasing if
  relevant), or models will be told the tool set excludes the new tool — this
  is easy to miss because it lives in config, not in the tool plan.
- **`followup_task` after close.** A closed agent is removed from the live
  registry; a subsequent `followup_task` by the same task name will fail with
  "live agent path not found". That is acceptable semantics but should be
  stated in the close tool description so models don't close-then-follow-up.
- **Close of a currently-running agent.** `AgentControl::close_agent` will
  send `Op::Shutdown` to a busy thread and wait; combined with the unbounded
  wait this is the most likely place a first user hits #25426 territory.
  Consider restricting the first patch to terminal-or-idle targets (respond
  to model: "interrupt first"), or explicitly test the busy-target path.
- **Upstream drift.** `main` currently matches the tag in every inspected
  area, but the rebase-onto-main step should re-diff `spec_plan.rs`,
  `multi_agents_v2.rs`, and `legacy.rs` at rebase time.

## Minimum corrected implementation/test sequence

The report's checkpoint steps 1–6 stand as written, with two insertions:

1. (report step 1) Red spec/tool-plan test — plain AND namespaced V2 lists.
2. (report step 2) `create_close_agent_tool_v2()` in `multi_agents_spec.rs`,
   target = canonical task path or thread id; description states residency
   semantics AND that closed agents cannot receive `followup_task`.
3. (report step 3) `multi_agents_v2/close_agent.rs` modeled on
   `interrupt_agent.rs` (resolve → `ensure_agent_known` → root/self guards →
   status capture → `agent_control.close_agent` → `SubAgentActivity` emit).
4. (report step 4) Export + register beside interrupt/list, under the
   namespace override; **update the shared usage-hint constant** in
   `config/mod.rs` to include `close_agent`.
5. (report step 5) Re-route `multi_agent_v2_list_agents_omits_closed_agents`
   through the handler by task name; **add one namespaced-dispatch case and
   one busy-target case** (assert defined behavior, whichever is chosen).
6. (report step 6 + gates 4–7) Capacity-release test, Windows sentinel smoke,
   fmt/clippy/focused tests, pinned standalone binary, target-dir cleanup.

Hardening for #25426 remains a separate later checkpoint, as the report says.

## May Codex-core work begin without changing SymForge product code?

**Yes.** The defect is entirely inside compiled Codex core; no SymForge
product code change is required or appropriate. SymForge's daemon idle
shutdown remains defense-in-depth only, exactly as the report classifies it.
Work should proceed on a separate pinned `openai/codex` checkout
(`rust-v0.144.4` first, rebased onto `main` for upstream review), never by
mutating the npm-installed binary in place.

## Sources

- https://github.com/openai/codex/tree/rust-v0.144.4 (inspected via local shallow clone)
- https://github.com/openai/codex (main, cloned 2026-07-14)
- https://github.com/openai/codex/issues/25426 (open; body verified via API)
- https://github.com/openai/codex/pull/19753 (merged 2026-04-28; verified via API)
- Key files: `codex-rs/core/src/tools/spec_plan.rs`,
  `tools/handlers/multi_agents_v2.rs` (+ subdir),
  `tools/handlers/multi_agents_spec.rs`, `tools/handlers/multi_agents_tests.rs`,
  `agent/agent_resolver.rs`, `agent/control.rs`, `agent/control/legacy.rs`,
  `agent/control/residency.rs`, `agent/registry.rs`, `session/mod.rs`,
  `session/handlers.rs`, `config/mod.rs`, `rmcp-client/src/rmcp_client.rs`
