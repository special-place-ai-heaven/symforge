---
goal_id: RTK21
title: Regex glob and Aho-Corasick cache investigation
phase: Wave F - evidence-gated audit follow-up
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

# RTK21 - Regex glob and Aho-Corasick cache investigation

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK21-regex-glob-aho-corasick-cache-investigation.md

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
- Investigate dynamic regex/glob/Aho-Corasick matcher caching and implement only a bounded cache when repeated-query evidence exists.

non_goals:
- Do not add unbounded process-wide caches for user input.
- Do not cache one-off user queries.
- Do not replace Aho-Corasick with RegexSet without benchmark evidence.

allowed_files_or_area:
- Search paths that construct regex, glob, or Aho-Corasick matchers
- Targeted search tests
- Benchmark or docs note if deferred

forbidden_files:
- Do not add dependencies.
- Do not add `lazy_static`.
- Do not alter invalid regex/glob error behavior.

goal_specific_contracts_or_interfaces:
- Any user-input-keyed cache must have bounded size and eviction policy.
- Invalid pattern behavior must remain unchanged.

implementation_steps:
- Inspect regex, glob, and Aho-Corasick matcher construction paths.
- Gather repeated-query evidence.
- If no evidence exists, close as deferred with notes.
- If implemented, design bounded size/eviction policy.
- Add tests for valid and invalid patterns.
- Run full test suite.

acceptance_criteria:
- Current repeated compile hotspots are identified, if any.
- If no repeated-query evidence exists, close as deferred with notes.
- If implemented, cache has bounded size/eviction policy.
- Invalid regex/glob behavior remains unchanged.
- User-input keys do not grow unbounded.
- Tests cover invalid and valid patterns.

evidence_required:
- Evidence.
- Cache design or deferral.
- Invalid-pattern test proof.
- Tests run.

stop_conditions:
- No repeated-query evidence exists.
- A safe bounded eviction policy cannot be designed.
- Implementation would change invalid-pattern semantics.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run RTK21 only on branch `rtk-symforge-integration`. Investigate matcher caching with evidence and bounded-cache discipline. Defer cleanly if the data does not justify a patch.

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
- None - RTK goal chain complete.
