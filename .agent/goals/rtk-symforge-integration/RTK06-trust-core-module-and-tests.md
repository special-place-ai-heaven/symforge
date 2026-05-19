---
goal_id: RTK06
title: Trust core module and tests
phase: Wave B - foundation
status: "Queued"
depends_on: ["RTK05"]
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK06 - Trust core module and tests

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK06-trust-core-module-and-tests.md

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
- `RTK05`

Before editing, confirm each predecessor goal file has `status: "Completed"` and a non-empty `completion_commit`:

```bash
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK05-adr-0015-rtk-symforge-trust-gate.md
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
- Implement the pure trust-gate core for `.symforge/` config hashing, trust-store persistence, canonical project keying, and four-state status evaluation, without daemon or CLI wiring.

non_goals:
- Do not wire daemon startup or tool response envelopes yet.
- Do not add trust CLI/MCP commands yet.
- Do not refuse daemon startup in this goal.
- Do not add a filesystem watcher.

allowed_files_or_area:
- src/edit_safety/trust.rs
- src/edit_safety/mod.rs
- Cargo.toml for `chrono = "0.4"` only if missing
- tests/edit_safety_trust.rs

forbidden_files:
- src/daemon.rs
- src/protocol/tools.rs
- src/protocol/edit.rs except if tests require a tiny fixture import.

goal_specific_contracts_or_interfaces:
- Expose or internally define `TrustStatus::{Trusted, Untrusted, ContentChanged { expected, actual }, EnvOverride}`.
- Trust record must include canonical project key, hash, and RFC3339 `trusted_at`.
- Trust store path is `dirs::data_local_dir()/symforge/trust.json`.
- Trust API must record a precomputed hash rather than re-hashing on write.

implementation_steps:
- Inspect `src/edit_safety/tee.rs` and its tests for style.
- Add `chrono = "0.4"` only if it is not already present.
- Create `src/edit_safety/trust.rs`.
- Export `trust` from `src/edit_safety/mod.rs`.
- Implement deterministic `.symforge/` tree hashing.
- Implement canonical project keying using `std::fs::canonicalize` and `dunce` normalization.
- Implement fail-secure trust-store loading.
- Implement CI-gated override helper honoring only `CI`, `GITHUB_ACTIONS`, `GITLAB_CI`, `JENKINS_URL`, or `BUILDKITE`.
- Create `tests/edit_safety_trust.rs` covering core states.

acceptance_criteria:
- First trust check with no record returns `Untrusted` and computed hash evidence.
- Recording trust persists the precomputed hash and RFC3339 timestamp.
- Unchanged config returns `Trusted`.
- Changed config returns `ContentChanged` with expected and actual hashes.
- Corrupt trust store does not panic; it returns `Untrusted` and surfaces/logs a warning path.
- Env override returns `EnvOverride` only under recognized CI env.
- Non-CI env override is ignored and logged/surfaced as ignored.
- Canonical path keying uses `dunce` normalization.
- A Windows-specific path normalization test exists behind `#[cfg(windows)]`.

evidence_required:
- Public/internal API summary.
- Trust store schema.
- Tests and edge cases covered.
- New dependency confirmation if `chrono` was added.

stop_conditions:
- Implementing core trust requires daemon or protocol changes.
- The trust store path cannot be made platform-correct.
- Canonicalization behavior is unclear and cannot be tested safely.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
cargo test --test edit_safety_trust -- --test-threads=1
cargo check
```

## Task Prompt

Run RTK06 only on branch `rtk-symforge-integration`. Implement only the pure trust core and its tests. Preserve fail-secure behavior and TOCTOU-safe precomputed-hash recording.

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
- RTK07 - Trust daemon and user control surface (RTK07-trust-daemon-and-user-control-surface.md)
