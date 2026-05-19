---
goal_id: RTK17
title: Analytics-trained correction learning
phase: Wave E - gated implementation
status: "Queued"
depends_on: ["RTK15", "RTK16"]
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK17 - Analytics-trained correction learning

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK17-analytics-trained-correction-learning.md

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
- `RTK15`
- `RTK16`

Before editing, confirm each predecessor goal file has `status: "Completed"` and a non-empty `completion_commit`:

```bash
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK15-analytics-instrumentation-and-reporting.md
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK16-stateless-same-file-correction-suggestions.md
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
- Add analytics-backed correction learning so repeated edit-resolution failures can improve suggestions while preserving stateless same-file fallback behavior.

non_goals:
- Do not train on working-directory mismatch or worktree-routing errors.
- Do not weaken the stateless same-file MVP.
- Do not create analytics DB when disabled.

allowed_files_or_area:
- src/protocol/edit.rs
- src/observability/analytics.rs
- Targeted correction-learning tests

forbidden_files:
- Do not broaden analytics schema beyond ADR 0017.
- Do not collect raw query text outside accepted schema.

goal_specific_contracts_or_interfaces:
- Run only if analytics implementation is complete and green-lit. If analytics is blocked or rejected, stop and report that RTK16 is the completed MVP.
- If analytics is disabled/unavailable, fallback to RTK16 behavior.

implementation_steps:
- Confirm RTK15 completed and analytics is enabled by decision.
- Add failure-history ingest/query helpers consistent with accepted analytics schema.
- Filter out working-directory mismatch and worktree-routing errors.
- Use `CORRECTION_WINDOW = 3`, `MIN_CONFIDENCE = 0.6`, and same-file boost.
- Merge learned suggestions additively with RTK16 fallback.
- Add tests for learned suggestion, disabled fallback, and filtered non-training errors.

acceptance_criteria:
- Uses `CORRECTION_WINDOW = 3`.
- Uses `MIN_CONFIDENCE = 0.6`.
- Same-file boost dominates cross-context noise.
- Does not train on working_directory mismatch errors.
- Does not train on worktree-routing errors.
- Suggestions remain additive and top-3.
- If analytics store is unavailable or disabled, fallback is stateless RTK16 behavior.

evidence_required:
- Learned suggestion example.
- Disabled analytics fallback proof.
- Filters applied to non-training errors.
- Tests run.

stop_conditions:
- Analytics is blocked, rejected, or not implemented.
- The accepted analytics schema cannot support this without redesign.
- Training would require storing raw query text or unrelated project data.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --test observability_analytics -- --test-threads=1
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run RTK17 only on branch `rtk-symforge-integration`. Extend correction suggestions with analytics-backed learning only when analytics is available, and preserve stateless fallback when it is not.

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
- RTK18 - Config extractor registry cleanup evaluation (RTK18-config-extractor-registry-cleanup-evaluation.md)
