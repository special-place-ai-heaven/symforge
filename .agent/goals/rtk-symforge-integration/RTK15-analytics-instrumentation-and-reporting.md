---
goal_id: RTK15
title: Analytics instrumentation and reporting
phase: Wave E - gated implementation
status: "Queued"
depends_on: ["RTK14"]
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK15 - Analytics instrumentation and reporting

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK15-analytics-instrumentation-and-reporting.md

## Goal File Workflow

0. Use the Branch Guard below before editing this goal file, status fields, source code, docs, tests, or migrations. A wrong branch in the repository root means pivot to the dedicated worktree; a wrong branch inside that target worktree means stop before changing anything.
1. Use the Dependency Guard below. If a required predecessor goal is not completed, stop and report the missing dependency.
2. After Branch Guard and Dependency Guard pass, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
3. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`, and stop if a stop condition is hit.
4. When acceptance criteria pass, run the verification command, commit the verified goal work, then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
5. Commit the status update as well. If blocked, set `status` to `Blocked`, leave `completion_commit` empty, and record the blocker in the final report.

## Branch Guard

This goal belongs only to the `rtk-symforge-integration` branch, based from `main`.

Before changing this file's frontmatter, source code, docs, tests, migrations, or generated fixtures, run:

```bash
git branch --show-current
git status --short
```

The branch output must be exactly:

```text
rtk-symforge-integration
```

If this checkout is `main` or anything else, do not edit there. Pivot to an existing worktree for `rtk-symforge-integration`, or create one from `main` before continuing, for example:

```bash
git fetch origin main
git worktree add -b rtk-symforge-integration ../symforge-rtk-symforge-integration origin/main
cd ../symforge-rtk-symforge-integration
git branch --show-current
git status --short
```

If branch `rtk-symforge-integration` already exists but has no worktree, use:

```bash
git worktree add ../symforge-rtk-symforge-integration rtk-symforge-integration
cd ../symforge-rtk-symforge-integration
```

Stop only if the target worktree is unavailable, dirty with unrelated SymForge work, or still does not print `rtk-symforge-integration`. Do not move unrelated work into this branch and do not perform this goal on `main` unless the owner explicitly edits this goal file to say so.

## Dependency Guard

Required predecessor goals:
- `RTK14`

Before editing, confirm each predecessor goal file has `status: "Completed"` and a non-empty `completion_commit`:

```bash
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK14-analytics-storage-foundation.md
```

If any required predecessor is missing, blocked, still queued, or completed without a commit hash, stop and report the dependency blocker.

## Shared Project Constraints

forbidden_project_wide:
- Do not add SymForge runtime coupling to RTK. This sprint cherry-picks proven Rust idioms and patterns only.
- Do not reopen settled items: `panic = "abort"`, RTK build-time `.scm` embedding, `match_output`, shell lexer, telemetry HTTP ping, OpenClaw plugin, Homebrew packaging, or `RegexSet` replacement without benchmark evidence.
- Do not add forbidden dependencies: `lazy_static`, `ureq`, `flate2`, `quick-xml`, `which`, or `getrandom`.
- Do not log provider credentials, secrets, `.env` contents, private keys, raw query text, or unbounded source blobs.
- Do not silently convert missing, stale, degraded, disabled, blocked, or unknown data into success.
- Do not modify unrelated roadmap, release, frontend, npm, or vendored files unless this goal explicitly names them.

contracts_or_interfaces:
- SymForge remains a Rust MCP server. These goals must not make RTK a dependency or shared runtime component.
- Existing public tool surfaces and aliases must remain backward compatible unless an ADR explicitly authorizes a change.
- Persistent local state must be platform-correct and privacy-preserving, with opt-out or inert defaults where specified.
- Trust/config behavior must resolve at call time when the ADR or goal mentions ADR 0016; do not cache policy decisions at boot.
- Discovery-only tools must not create persistent frecency or analytics stores when their feature is disabled.
- Every goal remains one focused, reviewable implementation commit plus a separate goal-status commit unless the owner redirects.

invariants:
- If the current code and this goal disagree, inspect the code first, preserve the current implementation truth, and report the mismatch instead of guessing.
- A goal that finds the target already implemented must strengthen tests, status, or documentation instead of duplicating code.
- Any new status enum, CLI/MCP surface, environment variable, persistent schema, or feature flag must be named in the final report.
- Any broad `#[allow(...)]` is prohibited. A narrow `allow` must be local and include a one-line reason.

## Mini-Spec

objective:
- Wire analytics instrumentation and reporting so MCP tool calls enqueue local analytics rows through one shared helper and users can view/reset/export aggregates.

non_goals:
- Do not perform per-handler inline SQLite writes.
- Do not collect raw query text.
- Do not collect individual raw file paths beyond the accepted scoping model.
- Do not replace existing tool surfaces.

allowed_files_or_area:
- src/protocol/tools.rs
- CLI/MCP reporting files according to ADR 0017
- src/observability/analytics.rs
- tests/observability_analytics.rs

forbidden_files:
- Do not alter unrelated protocol semantics.
- Do not create analytics DB when disabled.

goal_specific_contracts_or_interfaces:
- Run only if RTK14 completed and analytics is green-lit.
- Handler instrumentation must use one shared helper or wrapper.
- Inserts must be fire-and-forget through mpsc/background task.
- Duration must use an RAII timer.

implementation_steps:
- Read ADR 0017 and RTK14 analytics API.
- Design one shared tool-call wrapper/helper.
- Instrument MCP tool calls through that helper.
- Implement background enqueue and insert path if not fully present from RTK14.
- Implement reporting surface: summary/daily/weekly/monthly, failures-only, JSON, CSV.
- Implement reset requiring explicit confirmation such as `--yes`.
- Add tests for instrumentation, reports, exports, reset, disabled behavior.

acceptance_criteria:
- Each MCP tool call writes one local analytics row with tool name, project path/scope, response bytes, estimated tokens, duration, success, and UTC timestamp.
- Instrumentation uses one shared helper/wrapper.
- Inserts are fire-and-forget via mpsc/background task.
- RAII timer records duration on Drop.
- Discovery-only tools do not create analytics DB when analytics is disabled.
- Summary/daily/weekly/monthly reporting works.
- Failures-only report works.
- JSON and CSV export work.
- Reset requires explicit confirmation.
- Full test suite passes.

evidence_required:
- Shared wrapper location.
- Example report output.
- Reset/export proof.
- Performance and privacy notes.
- Test output summary.

stop_conditions:
- Analytics decision is not green-lit.
- Instrumentation requires raw query text.
- A synchronous insert appears necessary on the hot path.
- Reporting surface would conflict with existing tool naming/aliases.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --test observability_analytics -- --test-threads=1
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run RTK15 only on branch `rtk-symforge-integration`. Wire analytics through a single shared, asynchronous helper and add the reporting/export/reset surface specified by the decision artifact.

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
- <source-status notes, test output summaries, schema/route/status evidence, screenshots only if this goal changes rendered UI>
Commit:
- Verified work commit: `<hash>`
Known gaps / blockers:
- <none or explicit blocker>
Next goal:
- RTK16 - Stateless same-file correction suggestions (RTK16-stateless-same-file-correction-suggestions.md)
