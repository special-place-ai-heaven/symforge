---
goal_id: RTK03
title: Strict Rust lints policy
phase: Wave A - trivial parallelizable hygiene
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

# RTK03 - Strict Rust lints policy

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK03-strict-rust-lints-policy.md

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
- Adopt SymForge’s strict lint policy by enabling `unsafe_code = "deny"` and `warnings = "deny"` in `Cargo.toml`, with all all-target checks passing cleanly.

non_goals:
- Do not introduce unsafe code.
- Do not add broad `allow` attributes.
- Do not change runtime behavior.

allowed_files_or_area:
- Cargo.toml
- Source files only if `cargo check --all-targets` exposes warnings that must be fixed.

forbidden_files:
- Do not modify vendored parser code.
- Do not add new dependencies.
- Do not suppress warnings globally.

goal_specific_contracts_or_interfaces:
- Workspace lint policy must be explicit in `Cargo.toml`.
- Any required `#[allow(...)]` must be local and have a one-line reason.

implementation_steps:
- Run `cargo check --all-targets` before editing and capture the warning load.
- Add `[lints.rust] unsafe_code = "deny"` and `warnings = "deny"` to `Cargo.toml`.
- Fix exposed warnings locally, preferring code cleanup over suppression.
- Run all-target check and tests.

acceptance_criteria:
- `Cargo.toml` contains the strict lint block.
- `cargo check --all-targets` passes.
- `cargo test --all-targets -- --test-threads=1` passes.
- No unsafe code is introduced.
- Any `allow` annotations are narrow and justified.

evidence_required:
- Preflight warning summary.
- Lint block diff summary.
- Warnings fixed.
- Any `allow` annotations and reasons.

stop_conditions:
- The lint block causes a large unrelated warning cleanup outside this sprint.
- A warning fix requires changing product behavior.
- Vendored dependency warnings appear to be in scope; they are not.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets -- --test-threads=1
```

## Task Prompt

Run RTK03 only on branch `rtk-symforge-integration`. Enable the strict lint policy, fix only directly exposed local warnings, and keep any exceptions narrow and documented.

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
- RTK04 - Inline extractor test framework (RTK04-inline-extractor-test-framework.md)
