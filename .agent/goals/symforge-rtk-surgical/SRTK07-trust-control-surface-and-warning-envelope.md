---
goal_id: SRTK07
title: trust control surface and warning envelope
phase: Wave B - trust integration
status: "Pending"
depends_on: ["SRTK05", "SRTK06"]
target_branch: "symforge-rtk-surgical"
prohibited_branches: ["main"]
started_at: ""
completed_at: ""
completion_commit: ""
---

# SRTK07 - trust control surface and warning envelope

Use this file directly with `/goal`:

    /goal .agent/goals/symforge-rtk-surgical/SRTK07-trust-control-surface-and-warning-envelope.md

## Goal File Workflow

0. Use the Branch Guard below before editing this goal file, status fields, source code, docs, tests, fixtures, or migrations.
1. After Branch Guard passes, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
2. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`, and stop if a stop condition is hit.
3. When acceptance criteria pass, run the verification command, commit the verified goal work, then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
4. Commit the status update as well. If blocked, set `status` to `Blocked`, leave `completion_commit` empty, and record the blocker in the final report.

## Branch Guard

This goal belongs only to the `symforge-rtk-surgical` branch.

Before changing anything, run:

```bash
git branch --show-current
git status --short
```

The branch output must be exactly:

```text
symforge-rtk-surgical
```

If this checkout is `main` or any other branch, do not edit there. Create or pivot to a dedicated worktree for `symforge-rtk-surgical`, then rerun this Branch Guard. Stop if the target worktree is unavailable, dirty with unrelated work, or still does not print `symforge-rtk-surgical`.

## Selection Rationale

Wire the trust core into the SymForge-native surface selected by ADR 0015, adding only the chosen
control commands/tools and LOG_ONLY warning behavior; keep ENFORCE opt-in and call-time resolved.

## Mini-Spec

objective:
- Wire the trust core into the SymForge-native surface selected by ADR 0015, adding only the chosen control commands/tools and LOG_ONLY warning behavior; keep ENFORCE opt-in and call-time resolved.

non_goals:
- Do not add both CLI and MCP surfaces unless ADR 0015 explicitly requires both.
- Do not block daemon startup by default.
- Do not touch search/context trust gates.
- Do not add integrity sidecar logic here.

allowed_files_or_area:
- src/daemon.rs
- src/protocol/edit.rs if warning suffix is attached to edit responses
- src/cli/** only if ADR selects CLI subcommands
- src/protocol/tools.rs only if ADR selects MCP tools
- tests/edit_safety_trust.rs or focused integration test

forbidden_files:
- Do not vendor, copy, or import RTK as a SymForge runtime dependency.
- Do not import RTK shell hooks, hook installers, command rewriting, Claude permission parsing, CLI output filters, OpenClaw plugin code, Homebrew formula code, or HTTP telemetry.
- Do not add `lazy_static`, `ureq`, `flate2`, `quick-xml`, `which`, or `getrandom` for this task.
- Do not add `panic = "abort"` or change the release profile beyond the exact task scope.
- Do not convert SymForge from an MCP code-intelligence server into an RTK-style CLI-output filter.
- Do not change unrelated MCP tool names, aliases, or response contracts.
- Do not write raw prompts, secrets, `.env` contents, provider credentials, private keys, or unbounded source blobs to logs, analytics, memory, or persistent state.
- Do not silently report degraded, skipped, unavailable, disabled, stale, partial, or blocked behavior as success.

contracts_or_interfaces:
- SymForge stays a local-first MCP server for code intelligence. RTK is only a source of selectively borrowed Rust patterns.
- Tool handlers return helpful plain text `String` responses unless an existing SymForge contract explicitly says otherwise.
- Do not use MCP error codes for normal not-found or degraded cases; follow the existing helpful-text style.
- Never hold `RwLock` guards across await points; extract owned data, drop the lock, then format.
- Capability and policy decisions that are user/operator-controlled must resolve at call time unless this goal explicitly proves another model is safe.
- Discovery-only tools must not create new persistent DB files or durable side effects.
- New live behavior must be gated, observable, and reversible until a later goal explicitly enables default rollout.

invariants:
- Current SymForge code is the implementation truth. If planning docs disagree with code, document the gap and stop rather than pretending the plan is implemented.
- A goal that finds the target already implemented must strengthen tests, documentation, or status instead of duplicating code.
- Persistent state must be versioned or migration-safe, and missing/corrupt local state must degrade safely rather than crash the daemon.
- Evidence-gated tasks may close as `Completed` with a no-patch investigation note when measurement shows no worthwhile implementation.
- Each sprint should remain one focused code/doc commit plus the goal-status commit unless the owner redirects.

code_evidence_to_inspect_before_editing:
- `src/protocol/edit.rs` already has `append_response_suffix_to_first_summary` used for tee hints.
- `src/daemon.rs` owns server routing/daemon setup.
- ADR 0016-style call-time resolution is already used elsewhere for policies and capabilities.

implementation_steps:
- Read ADR 0015 and implement only its chosen user-control surface.
- Resolve trust mode at call time.
- Default mode LOG_ONLY: allow operation and surface one-line warning in the next relevant tool response.
- Opt-in ENFORCE: return a typed/helpful failure message without panic.
- Add trust audit/revoke command or MCP equivalent only if ADR selected it.
- Preserve tool name aliases and registry conventions if MCP tools are added.
- Add tests for default LOG_ONLY and opt-in ENFORCE.

acceptance_criteria:
- First launch/first trust path behaves as ADR states.
- Changed config in LOG_ONLY does not crash or refuse startup.
- Changed config in ENFORCE refuses only the configured operation with a helpful typed message.
- Trust mode changes are observed at call time.
- User can audit/revoke trust via the ADR-selected surface, or the task reports ADR chose no surface yet.

risks_and_mitigation:
- Adding tools can violate the tool-consolidation contract; inspect ADR 0001 if MCP tools are added.
- Do not create a prompt loop or user-interactive CLI behavior inside MCP request handling.

evidence_required:
- Branch evidence: `git branch --show-current` output exactly `symforge-rtk-surgical`.
- File paths changed.
- Verification command output summary.
- Any new public type, port, route, migration, event, feature flag, environment variable, or status enum introduced.
- Explicit source-status notes for live, partial, degraded, disabled, blocked, unknown, test-only, or mock behavior touched.
- A short note explaining why this is a selective SymForge enhancement rather than RTK bulk integration.

stop_conditions:
- Current branch is not exactly `symforge-rtk-surgical`.
- The task would require importing RTK runtime code or changing SymForge's product shape.
- The target code path is missing or contradicts this mini-spec.
- The goal would require touching forbidden files or expanding into a later goal's scope.
- Verification cannot run for a reason that is not clearly pre-existing and documented.

verification_command:
```bash
test "$(git branch --show-current)" = "symforge-rtk-surgical"
git diff --check
cargo test --test edit_safety_trust -- --test-threads=1
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run SRTK07 only on branch `symforge-rtk-surgical`. Wire the trust core into the SymForge-native surface selected by ADR 0015, adding only the chosen control commands/tools and LOG_ONLY warning behavior; keep ENFORCE opt-in and call-time resolved. Keep the sprint surgical, prove the current code surface before editing, avoid RTK bulk import, commit only verified work, and report blockers instead of guessing.

## Final Report Format

Objective:
- <repeat this goal's objective>
Changes:
- <focused list of implementation changes>
Files changed:
- <paths>
Verification:
- PASS/FAIL: `<command>` — <summary>
Evidence:
- <source-status notes, test output summaries, route/status evidence, and any benchmark/profiling output required by the goal>
Commit:
- Verified work commit: `<hash>`
Known gaps / blockers:
- <none or explicit blocker>
Next goal:
- SRTK08 - Tier metadata lookup helpers (SRTK08-tier-metadata-lookup-helpers.md)
