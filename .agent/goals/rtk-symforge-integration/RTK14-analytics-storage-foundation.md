---
goal_id: RTK14
title: Analytics storage foundation
phase: Wave E - gated implementation
status: "Queued"
depends_on: ["RTK13"]
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK14 - Analytics storage foundation

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK14-analytics-storage-foundation.md

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
- `RTK13`

Before editing, confirm each predecessor goal file has `status: "Completed"` and a non-empty `completion_commit`:

```bash
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK13-analytics-product-decision.md
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
- Implement the local analytics storage foundation, including schema, WAL/busy-timeout setup, retention cleanup, opt-out checks, and aggregation queries, without instrumenting all MCP handlers yet.

non_goals:
- Do not instrument all MCP handlers in this goal.
- Do not add synchronous hot-path inserts.
- Do not collect raw query text.
- Do not create a DB when analytics is disabled.

allowed_files_or_area:
- src/observability.rs or src/observability/mod.rs
- src/observability/analytics.rs
- src/lib.rs only if module registration requires it
- Cargo.toml for `chrono = "0.4"` only if missing
- tests/observability_analytics.rs

forbidden_files:
- src/protocol/tools.rs
- CLI/MCP reporting surface files
- Do not alter existing tracing behavior except to move `src/observability.rs` into `src/observability/mod.rs` if needed.

goal_specific_contracts_or_interfaces:
- Before implementing, read ADR 0017 or the analytics decision artifact. If analytics is not accepted/green-lit, stop and report blocked.
- If `src/observability.rs` already exists, refactor it to `src/observability/mod.rs` while preserving `init_tracing`, then add `analytics.rs`.
- Store path must be `dirs::data_local_dir()/symforge/tracking.db`.

implementation_steps:
- Read the analytics decision artifact. Stop if not green-lit.
- Inspect current observability module shape; preserve existing tracing API.
- Create or migrate `src/observability/mod.rs`.
- Create `src/observability/analytics.rs`.
- Implement DB open with WAL and 5s busy timeout.
- Implement `tool_calls` schema and indexes.
- Implement 90-day cleanup.
- Implement GLOB project scoping.
- Implement opt-out helpers for env and `.symforge/analytics.toml`.
- Implement summary/daily/weekly/monthly aggregation queries.
- Add storage-focused tests.

acceptance_criteria:
- Existing tracing initialization still compiles and behaves.
- Analytics DB path is `dirs::data_local_dir()/symforge/tracking.db`.
- DB opens with WAL mode and 5s busy timeout.
- Schema has `id`, `timestamp_utc`, `tool_name`, `project_path`, `response_bytes`, `est_tokens`, `duration_ms`, and `success`.
- Indexes exist on project path and timestamp.
- 90-day cleanup function is implemented and tested.
- GLOB project scoping is used, not LIKE.
- Opt-out helpers honor env and `.symforge/analytics.toml`.
- Aggregation queries for summary/daily/weekly/monthly are implemented and tested.
- Disabled analytics does not create a DB.

evidence_required:
- Decision artifact status.
- Module migration summary.
- Schema proof.
- Opt-out proof.
- Retention proof.
- Aggregation test summary.

stop_conditions:
- ADR 0017 or the decision note is missing.
- The decision is not Accepted/green-lit.
- Existing `src/observability.rs` cannot be migrated without breaking tracing.
- Analytics would create a DB while disabled.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --test observability_analytics -- --test-threads=1
cargo check
```

## Task Prompt

Run RTK14 only on branch `rtk-symforge-integration`. Implement analytics storage only if the decision is green-lit, preserving existing tracing and keeping disabled analytics inert.

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
- RTK15 - Analytics instrumentation and reporting (RTK15-analytics-instrumentation-and-reporting.md)
