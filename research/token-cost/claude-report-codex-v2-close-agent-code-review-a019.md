# Independent code review: Codex V2 `close_agent` bridge (A019)

Reviewer: Claude (Fable 5), 2026-07-14. Read-only; nothing edited, committed,
or installed. Review order followed: `AGENTS.md` → checkout/dirty-state
receipt → existing teardown path (`agent/control/legacy.rs`, V1 close handler,
V2 interrupt/list/resolver, namespace override in `spec_plan.rs`) → candidate
diff and tests → only then the reconnaissance and prior research report.
Independent baseline: this reviewer had already built a private fact table of
the same teardown/registration surfaces from a separate shallow clone of
`rust-v0.144.4` and `main` during the A019 research review.

## Checkout and diff integrity (Q10)

- Branch `fix/multi-agent-v2-close-agent`, HEAD = `8c68d4c8…` =
  `rust-v0.144.4`. Verified via `git log`/`git merge-base`.
- Working-tree diff vs base: exactly the six expected modified files plus new
  `codex-rs/core/src/tools/handlers/multi_agents_v2/close_agent.rs`
  (69 insertions, 11 deletions total). `Cargo.lock` untouched. No dependency,
  SymForge, generated-artifact, or unrelated-source change is present.
  LF→CRLF notices are checkout-local line-ending noise, not diff content.

## Findings per required question

**1. Target resolution — yes.** `close_agent.rs:35` calls
`resolve_agent_target(&session, &turn, &args.target)`, the same production V2
resolver used by `interrupt_agent.rs` and `send_message`/`followup_task`. That
resolver accepts a raw `ThreadId` string or resolves a canonical task
name/path via `AgentControl::resolve_agent_reference`
(`agent/agent_resolver.rs`; `agent/control.rs:326`). The green
`multi_agent_v2_list_agents_omits_closed_agents` test closes by task name
`"worker"` through the handler, proving the name path end to end. The
thread-id path is the resolver's first branch and is shared, unmodified
production code.

**2. Root/self protection — yes, byte-consistent with interrupt.**
`close_agent.rs:41-49` rejects root (`"root is not a spawned agent"`, same
string as `interrupt_agent.rs`) and `close_agent.rs:50-55` rejects self with
an appropriately adapted message ("an agent cannot close itself; return your
result and let the parent close you…"). Guard order and mechanics
(`ensure_agent_known` → `AgentPath::is_root` → `agent_id ==
session.thread_id`) mirror the interrupt handler exactly.

**3. Existing teardown invoked — yes.** `close_agent.rs:76` calls
`session.services.agent_control.close_agent(agent_id)` — the same internal
path the V1 handler uses (`multi_agents/close_agent.rs`). In this checkout
`agent/control/legacy.rs` is unchanged from the tag:
`close_agent` → `shutdown_agent_tree` → `shutdown_live_agent`, which does
`remove_thread`, `forget_v2_residency(agent_id)`, and
`release_spawned_thread(agent_id)`. No teardown logic was duplicated or
reimplemented.

**4. Event/output compatibility — yes, no protocol expansion.** The handler
emits `TurnItem::CollabAgentToolCall` with the pre-existing
`CollabAgentTool::CloseAgent` variant (already used by V1), started
`InProgress` with empty `receiver_agents`/`agents_states`, completed with
`collab_tool_call_status(&status, …)` and populated `CollabAgentRef` +
`agents_states` — the same shape V1 close emits. On a core-close error the
completed item is still emitted before the error propagates (`result?` at
`close_agent.rs:101` comes after `emit_turn_item_completed`), so no dangling
in-progress item. The tool result is the same
`{ previous_status: AgentStatus }` JSON contract as V1 close and V2
interrupt, using the shared `agent_previous_status_output_schema`. No new
protocol types, events, or wire fields were added.

**5. Namespace registration proven — yes, all three surfaces.** Registration
at `spec_plan.rs:811-814` goes through `multi_agent_v2_handler(…,
tool_namespace)` with the same `override_tool_exposure` wrapper as
interrupt/list, so the default (`collaboration`) and configured namespaces
plus the `non_code_mode_only` exposure flag are all inherited. Tests updated:
`multi_agent_feature_selects_one_agent_tool_family` (plain V2 family,
preserved red per the evidence receipt),
`multi_agent_v2_can_use_configured_tool_namespace` (asserts `close_agent` is
hidden as a plain tool, registered as `agents/close_agent`, and present in
the namespace function list — verified in the test body at
`spec_plan_tests.rs:1402-1431`), and
`code_mode_only_can_expose_namespaced_multi_agent_v2_as_normal_tools`
(code-mode exposure). All three name lists now include `close_agent`.

**6. Description honesty — yes.** The V2 spec description states: close when
no longer needed; returns previous status; "Completed agents remain resident
and count toward the concurrency limit until closed"; "A closed agent is
removed and can no longer receive messages or follow-up tasks." That matches
`residency.rs` semantics and the observed post-close behavior, and the
hard-coded shared usage hint (`config/mod.rs`,
`DEFAULT_MULTI_AGENT_V2_SHARED_USAGE_HINT_TEXT`) now enumerates
`close_agent` — the drift risk flagged in the research review is closed. The
follow-up-rejection claim is proven by the test (`followup_task` on the
closed `"worker"` returns `RespondToModel`).

**7. Busy/cooperative shutdown without #25426 claims — yes.** The green list
test spawns and immediately closes the worker through the handler, which
drives `Op::Shutdown` + `wait_until_terminated` on a freshly spawned (not
necessarily terminal) thread — exactly the existing cooperative semantics.
Nothing in the diff touches the unbounded wait, adds timeouts, or claims
hang-proof cleanup; issue #25426 remains correctly out of scope.

**8. Smallest safe root-cause fix — yes.** 69 inserted lines; the handler is
a faithful composite of the V1 close body (status capture, turn items,
result type) and the V2 interrupt head (resolver + guards). The private
`CloseAgentResult` duplication between the V1 and V2 modules matches the
codebase's own per-module result-type convention (interrupt does the same).
The only non-mechanical product change outside the handler is one string
constant edit and one registration block. No residency, depth, or concurrency
behavior was altered (Q-scope confirmed: `config/mod.rs` diff is the usage
hint string only). `deny_unknown_fields` on args matches every other V2
handler.

**9. Missing tests / lifecycle regressions — nothing blocking.** The
behavior test covers: resolution by task name, non-`NotFound` previous
status, follow-up rejection after close, and disappearance from
`list_agents`. Filtered `multi_agent_v2` family 65/65 per the receipt.
Non-blocking gaps listed below; none prevents the standalone process-tree
smoke, which is itself the intended proof of the user-visible outcome.

**10. Diff purity — confirmed** (see integrity section above).

## Blockers

None.

## Non-blocking improvements (do not broaden the patch for these now)

1. **Root/self rejection is untested for close.** The guards are copied from
   interrupt (whose guards are exercised elsewhere), but a two-assert test
   invoking `CloseAgentHandlerV2` against root and self would pin the copied
   behavior cheaply. Recommended before any upstream PR, not before the smoke.
2. **No capacity-release test.** "Close a completed worker → a replacement
   spawn immediately succeeds at the residency limit" is the economic point
   of the fix; core `release_spawned_thread` makes it near-certain, but one
   test would prove it at the tool surface. The Windows sentinel smoke will
   demonstrate the process-level equivalent.
3. **Double-close behavior is unpinned.** After a successful close,
   `ensure_agent_known` on a second call will likely return a model-visible
   error; that is acceptable, but its wording/behavior is untested and could
   drift.
4. **V1 handler has `search_info()`; the V2 handler does not.** Consistent
   with the other V2 handlers (none implement it), so correct as-is — noted
   only so an upstream reviewer's question is pre-answered.
5. **Upstream rebase re-check.** When rebasing onto `main`, re-diff
   `spec_plan.rs`, `multi_agents_v2.rs`, `multi_agents_spec.rs`, and
   `legacy.rs`; as of 2026-07-14 `main` matched the tag in all reviewed areas.

## Judgment

The candidate is the authorized minimal scope, implemented in the codebase's
own idiom, reusing the existing teardown with correct guards, honest tool
wording, full three-surface registration coverage, and a behavior test that
proves close-by-task-name end to end. Evidence receipts (preserved red,
green family run, clean format check, clean `git diff --check`, clean
lockfile) are consistent with what the working tree shows.

VERDICT: APPROVE_CONTINUE
