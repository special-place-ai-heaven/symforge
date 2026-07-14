# Claude handoff — Codex V2 `close_agent` code review (A019)

## Role and verdict

Act as an independent, adversarial code reviewer. This checkpoint is read-only: do not edit Codex or SymForge product code, do not commit, and do not install the candidate. Write your report to:

`E:\project\symforge\research\token-cost\claude-report-codex-v2-close-agent-code-review-a019.md`

End with exactly one verdict:

- `VERDICT: APPROVE_CONTINUE`
- `VERDICT: CHANGES_REQUIRED`

`APPROVE_CONTINUE` means the primary agent may proceed to the broader Codex gate, standalone Windows sentinel-process smoke, and installation decision. It is not merge approval.

## Read in this order

1. Read `E:\project\codex-v2-close-agent\AGENTS.md` completely.
2. Inspect the pinned checkout and dirty-state receipt before reading this handoff's implementation summary:
   - checkout: `E:\project\codex-v2-close-agent`
   - branch: `fix/multi-agent-v2-close-agent`
   - base: `8c68d4c87dc54d38861f5114e920c3de2efa5876` (`rust-v0.144.4`)
   - expected product diff: six tracked Rust files plus one new Rust handler; `codex-rs/Cargo.lock` must be clean
3. Independently inspect the existing teardown path before judging the bridge:
   - `codex-rs/core/src/agent/control/legacy.rs`
   - V1 close handler and V2 target resolution/interrupt/list handlers
   - V2 namespace override and tool registration in `spec_plan.rs`
4. Review the candidate diff and tests.
5. Only after forming your own findings, read:
   - `E:\project\symforge\research\token-cost\codex-v2-close-agent-reconnaissance-2026-07-14.md`
   - `E:\project\symforge\research\token-cost\claude-report-codex-v2-close-agent-research-a019.md`

## Scope that was authorized

The minimal native Codex fix only:

- expose V2 `close_agent` through the existing internal `AgentControl::close_agent` teardown;
- accept a V2 agent id or canonical task name;
- preserve root/self protections;
- register the handler for default and configured namespaces;
- add `close_agent` to the hard-coded V2 usage hint;
- state that closed agents cannot receive messages or follow-up tasks;
- cover default/custom namespace visibility and real close behavior.

Out of scope at this checkpoint:

- SymForge product changes;
- watchdogs, polling reapers, cleanup daemons, or wrapper hacks;
- changing V2 residency limits or depth behavior;
- fixing open Codex issue #25426 (`wait_until_terminated` can hang indefinitely);
- installing or replacing the user's active Codex binary.

## Expected diff

- `codex-rs/core/src/config/mod.rs`
- `codex-rs/core/src/tools/handlers/multi_agents_spec.rs`
- `codex-rs/core/src/tools/handlers/multi_agents_tests.rs`
- `codex-rs/core/src/tools/handlers/multi_agents_v2.rs`
- `codex-rs/core/src/tools/handlers/multi_agents_v2/close_agent.rs` (new)
- `codex-rs/core/src/tools/spec_plan.rs`
- `codex-rs/core/src/tools/spec_plan_tests.rs`

No dependency or lockfile change is intended.

## Evidence already produced

Pre-change baseline:

- `multi_agent_v2_can_use_configured_tool_namespace`: pass
- `multi_agent_v2_list_agents_omits_closed_agents`: pass

Preserved red:

- `multi_agent_feature_selects_one_agent_tool_family`: failed twice with `expected close_agent in collaboration namespace`
- new behavior coverage failed to compile on the intentionally absent `multi_agents_v2::CloseAgentHandler`

Green:

- `multi_agent_feature_selects_one_agent_tool_family`: pass
- `multi_agent_v2_can_use_configured_tool_namespace`: pass
- `multi_agent_v2_list_agents_omits_closed_agents`: pass; it now closes through the V2 handler, parses the previous status, proves a follow-up is rejected, and proves the agent disappears from `list_agents`
- filtered `multi_agent_v2` family: 65/65 pass, 2575 skipped
- `git diff --check`: exit 0 (Windows LF→CRLF notices only)
- Rust formatter check: exit 0

Environment notes:

- The stable tag's manifests cause Cargo to rewrite 132 workspace package versions in `Cargo.lock` from the repository placeholder to the release version. That generated drift was independently inspected and removed; the review diff must not contain it.
- Windows `rusty_v8` required a target-local `codex-rs\target\debug\gn_root` junction because this user lacks symbolic-link privilege. It is disposable target scaffolding, not source or product behavior.
- The repository-wide `just fmt` wrapper ran its Rust formatter group but returned nonzero because this checkout lacks a discoverable nested `just` and `dotslash` for unrelated Just/Bazel formatting. A direct invocation of the wrapper's exact Rustfmt check exited 0.
- After verification, no `cargo.exe`, `rustc.exe`, `cargo-nextest.exe`, `just.exe`, or `link.exe` process remained.

If you rerun Cargo tests, inspect the clean diff first. The release-tag lockfile may be regenerated; treat that only as build drift and leave `Cargo.lock` clean afterward. Do not delete the shared target during this review because the primary agent still needs it for the broader gate and Windows smoke.

## Required review questions

1. Does V2 `close_agent` resolve both thread ids and canonical task names using the production V2 resolver?
2. Does it reject root/self targets consistently with V2 `interrupt_agent`?
3. Does it invoke the existing teardown that removes the thread, forgets V2 residency, and releases the spawned-thread slot?
4. Is the event/output behavior compatible with the existing V1 close and V2 wait contracts without a protocol expansion?
5. Are default and configured namespace registration both proven, including code-mode namespace exposure?
6. Is the tool description honest about residency and post-close messaging/follow-up behavior?
7. Does immediate close of a freshly spawned agent exercise the existing busy/cooperative shutdown semantics without pretending to solve #25426?
8. Is the new handler the smallest safe root-cause fix, or is there avoidable duplication/risk that should block continuation?
9. Are there any missing error-path tests or lifecycle regressions that must be fixed before a standalone process-tree smoke?
10. Confirm no unrelated source, dependency, lockfile, SymForge, or generated artifact is in the product diff.

Report concrete file/line evidence. Separate blockers from non-blocking improvements. Do not broaden the implementation merely for stylistic preference.
