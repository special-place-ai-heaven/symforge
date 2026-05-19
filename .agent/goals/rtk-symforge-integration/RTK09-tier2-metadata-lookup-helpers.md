---
goal_id: RTK09
title: Tier-2 metadata lookup helpers
phase: Wave C - behavior layer
status: "Queued"
depends_on: []
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK09 - Tier-2 metadata lookup helpers

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK09-tier2-metadata-lookup-helpers.md

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

This goal has no required predecessor goal. Still inspect the current repo state before editing, because previous manual work may already have implemented the target.

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
- Add metadata-only lookup helpers in `src/live_index/query.rs` so tool handlers can distinguish fully indexed, metadata-only, and hard-skipped files.

non_goals:
- Do not change `get_symbol_context` or `find_references` response shapes in this goal.
- Do not invent new metadata fields.
- Do not change health tier labels.

allowed_files_or_area:
- src/live_index/query.rs
- Focused unit tests or integration fixtures directly required by the helper

forbidden_files:
- src/protocol/tools.rs
- src/protocol/format.rs unless test compilation requires a small import fix.

goal_specific_contracts_or_interfaces:
- Tier labels remain consistent with health output: Tier 1 indexed, Tier 2 metadata only, Tier 3 hard-skipped.
- Tier-2 metadata may expose only already-captured fields: path, size, language.

implementation_steps:
- Inspect existing live-index metadata and hard-skip structures.
- Design a small return type that identifies Tier 1, Tier 2, and Tier 3 with reason where applicable.
- Implement helpers in `src/live_index/query.rs`.
- Add focused tests for fully indexed, metadata-only, and hard-skipped cases.
- Do not wire protocol handlers yet.

acceptance_criteria:
- Helper returns Tier 1 for fully indexed files.
- Helper returns Tier 2 metadata for partial-parse or metadata-only files.
- Helper returns Tier 3 with reason for hard-skipped files.
- Tier-2 metadata exposes path, size, and language only.
- Existing health tier labels remain unchanged.

evidence_required:
- Helper names and return type.
- Test cases for all three tiers.
- Any assumptions about current live-index data structures.

stop_conditions:
- Current index does not retain enough metadata for a Tier-2 response.
- The helper requires protocol response-shape changes.
- The helper would need to change health label semantics.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run RTK09 only on branch `rtk-symforge-integration`. Build the live-index helper layer only. Leave protocol behavior changes for the next goal.

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
- RTK10 - Graceful degradation tool behavior (RTK10-graceful-degradation-tool-behavior.md)
