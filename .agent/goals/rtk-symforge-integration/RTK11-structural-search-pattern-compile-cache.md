---
goal_id: RTK11
title: Structural-search pattern compile cache
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

# RTK11 - Structural-search pattern compile cache

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK11-structural-search-pattern-compile-cache.md

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
- Reduce repeated structural-search pattern compilation so each `(LanguageId, pattern)` is compiled at most once per request, without an unbounded process-wide cache.

non_goals:
- Do not add a global user-input cache.
- Do not change structural-search semantics.
- Do not hide invalid pattern errors.

allowed_files_or_area:
- src/protocol/tools.rs
- ast-grep helper modules where structural pattern compilation occurs
- Focused structural-search tests or benchmarks

forbidden_files:
- Do not add `lazy_static`.
- Do not add a process-wide cache unless `Pattern: Send + Sync` and a bounded policy are proven and documented.

goal_specific_contracts_or_interfaces:
- Request-scoped caching comes first.
- Invalid structural-pattern errors must remain compatible.
- User input must not create unbounded process lifetime state.

implementation_steps:
- Locate structural pattern compilation in `search_structural` / `ast_grep::structural_search`.
- Add instrumentation, a counter, or a focused regression test that proves repeated compiles currently occur.
- Implement request-scoped cache keyed by `(LanguageId, pattern)`.
- Preserve invalid-pattern error behavior.
- Add or update tests/benchmark proof.

acceptance_criteria:
- Within one request, repeated use of the same `(LanguageId, pattern)` reuses the compiled pattern.
- Invalid structural-pattern errors are preserved.
- No unbounded process-wide global cache is added.
- A test or benchmark demonstrates fewer repeated compiles on the selected path.
- No `lazy_static` is added.

evidence_required:
- Before/after compile behavior.
- Whether process-wide caching was explicitly deferred.
- Invalid-pattern test summary.
- Targeted test or benchmark output.

stop_conditions:
- Compile reuse cannot be observed on a request path.
- The only feasible design is an unbounded global cache.
- `Pattern` ownership/lifetime constraints would require unsafe code.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run RTK11 only on branch `rtk-symforge-integration`. Cache structural pattern compilation within a request only, prove fewer compiles, and keep invalid-pattern behavior intact.

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
- RTK12 - Frecency read-path store reuse (RTK12-frecency-read-path-store-reuse.md)
