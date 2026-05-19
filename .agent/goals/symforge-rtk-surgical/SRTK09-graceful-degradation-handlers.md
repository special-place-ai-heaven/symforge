---
goal_id: SRTK09
title: graceful degradation handlers
phase: Wave C - behavior layer
status: "Pending"
depends_on: ["SRTK08"]
target_branch: "symforge-rtk-surgical"
prohibited_branches: ["main"]
started_at: ""
completed_at: ""
completion_commit: ""
---

# SRTK09 - graceful degradation handlers

Use this file directly with `/goal`:

    /goal .agent/goals/symforge-rtk-surgical/SRTK09-graceful-degradation-handlers.md

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

Use the new tier lookup helpers so `get_symbol_context` and `find_references` return explicit
degraded/helpful text for Tier 2 and Tier 3 paths, while preserving the existing Tier 1 response
shape.

## Mini-Spec

objective:
- Use the new tier lookup helpers so `get_symbol_context` and `find_references` return explicit degraded/helpful text for Tier 2 and Tier 3 paths, while preserving the existing Tier 1 response shape.

non_goals:
- Do not return JSON for these normal degraded cases.
- Do not break existing Tier 1 output.
- Do not invent symbol/reference data for metadata-only files.
- Do not rewrite unrelated tools.

allowed_files_or_area:
- src/protocol/tools.rs
- src/protocol/format.rs only if formatting helper is necessary
- tests/graceful_degradation.rs

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
- `src/protocol/tools.rs` comments state handlers return plain text strings and helpful text, not MCP error codes.
- Tier labels exist in health output, but handler-level per-file fallback is absent.
- SRTK08 provides helper classification.

implementation_steps:
- Find the exact `get_symbol_context` and `find_references` handlers.
- Before changing output, add or update tests that capture Tier 1 current behavior.
- For Tier 2 metadata-only path, return a concise degraded response with tier label, warning, path, size, and available language/extension.
- For Tier 3 hard-skipped path, return a helpful not-available response with skip reason.
- Keep the response additive and plain text.
- Test all three branches for both target tools where practical.

acceptance_criteria:
- Tier 1 response shape is unchanged.
- Tier 2 response says it is degraded/metadata-only and does not claim symbol/reference data exists.
- Tier 3 response includes reason.
- `tests/graceful_degradation.rs` passes.
- Full all-target test suite passes.

risks_and_mitigation:
- Existing clients may assert exact text; protect Tier 1 output and keep new degraded output tightly scoped.
- Do not expose internal structs or raw debug dumps in user-visible responses.

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
cargo test --test graceful_degradation -- --test-threads=1
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run SRTK09 only on branch `symforge-rtk-surgical`. Use the new tier lookup helpers so `get_symbol_context` and `find_references` return explicit degraded/helpful text for Tier 2 and Tier 3 paths, while preserving the existing Tier 1 response shape. Keep the sprint surgical, prove the current code surface before editing, avoid RTK bulk import, commit only verified work, and report blockers instead of guessing.

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
- SRTK10 - stateless same-file symbol suggestions (SRTK10-stateless-same-file-symbol-suggestions.md)
