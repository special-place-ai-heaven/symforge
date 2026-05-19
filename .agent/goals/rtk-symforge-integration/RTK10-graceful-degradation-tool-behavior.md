---
goal_id: RTK10
title: Graceful degradation tool behavior
phase: Wave C - behavior layer
status: "Queued"
depends_on: ["RTK09"]
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK10 - Graceful degradation tool behavior

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK10-graceful-degradation-tool-behavior.md

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
- `RTK09`

Before editing, confirm each predecessor goal file has `status: "Completed"` and a non-empty `completion_commit`:

```bash
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK09-tier2-metadata-lookup-helpers.md
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
- Extend `get_symbol_context` and `find_references` so Tier-2 files return degraded metadata responses and Tier-3 files return typed 404 reasons, without breaking Tier-1 response shape.

non_goals:
- Do not change Tier-1 successful response shape.
- Do not add metadata fields the index does not already track.
- Do not alter unrelated tool handlers.

allowed_files_or_area:
- src/protocol/tools.rs
- Formatting helpers only if needed for stable degraded output
- tests/graceful_degradation.rs

forbidden_files:
- Do not modify live-index internals except to consume RTK09 helpers.
- Do not alter health report formatting.

goal_specific_contracts_or_interfaces:
- Tier-2 response must be explicit and additive/backward-compatible where possible.
- Tier-3 hard skip must be a typed 404-equivalent with reason.

implementation_steps:
- Enumerate handler call sites and current response shapes before editing.
- Use RTK09 helpers in `get_symbol_context`.
- Use RTK09 helpers in `find_references`.
- Add degraded response formatting with explicit `tier: 2` label and warning.
- Add typed Tier-3 hard-skip error path with reason.
- Create `tests/graceful_degradation.rs` covering all three branches per tool.

acceptance_criteria:
- Tier-1 indexed file behavior is unchanged.
- Tier-2 file returns degraded response with explicit `tier: 2` label and warning.
- Tier-2 response includes path, size, and language only.
- Tier-3 hard-skipped file returns typed 404-equivalent with `reason`.
- Tests cover all three branches for `get_symbol_context`.
- Tests cover all three branches for `find_references`.

evidence_required:
- Response examples for Tier 1, Tier 2, and Tier 3.
- Compatibility notes.
- Verification output summary.

stop_conditions:
- Existing callers cannot tolerate an additive degraded response.
- Test fixtures cannot create Tier-2/Tier-3 states without broad index changes.
- The task expands into unrelated tools.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --test graceful_degradation -- --test-threads=1
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run RTK10 only on branch `rtk-symforge-integration`. Wire graceful degradation into the two named tools using the helper from RTK09, preserving Tier-1 output and making Tier-2/Tier-3 behavior explicit.

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
- RTK11 - Structural-search pattern compile cache (RTK11-structural-search-pattern-compile-cache.md)
