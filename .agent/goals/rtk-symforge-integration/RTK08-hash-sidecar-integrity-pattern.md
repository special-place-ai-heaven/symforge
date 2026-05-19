---
goal_id: RTK08
title: Hash-sidecar integrity pattern
phase: Wave B - foundation
status: "Queued"
depends_on: ["RTK06"]
target_branch: "rtk-symforge-integration"
base_branch: "main"
prohibited_branches: ["frontend"]
started_at: ""
completed_at: ""
completion_commit: ""
source_brief: "2026-05-19-rtk-integration-state-for-planning.md"
---

# RTK08 - Hash-sidecar integrity pattern

Use this file directly with `/goal`:

    /goal .agent/goals/rtk-symforge-integration/RTK08-hash-sidecar-integrity-pattern.md

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
- `RTK06`

Before editing, confirm each predecessor goal file has `status: "Completed"` and a non-empty `completion_commit`:

```bash
grep -E '^(status|completion_commit):' .agent/goals/rtk-symforge-integration/RTK06-trust-core-module-and-tests.md
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
- Add the hash-sidecar integrity pattern for trusted `.symforge/` files, with creation and verification covered by tests.

non_goals:
- Do not treat read-only permissions as a security boundary.
- Do not block trust-gate MVP behavior if sidecars are missing.
- Do not write sidecars for untrusted files.

allowed_files_or_area:
- src/edit_safety/integrity.rs
- src/edit_safety/mod.rs
- docs/decisions/0015-rtk-trust-gating-symforge-config.md
- tests/edit_safety_integrity.rs or tests/edit_safety_trust.rs

forbidden_files:
- Do not modify daemon or protocol surfaces unless ADR 0015 already requires a tiny warning update.
- Do not add dependencies.

goal_specific_contracts_or_interfaces:
- Sidecar format is sha256sum-compatible: `<hex_hash>  <filename>
`.
- Unix sidecar permission is `0o444` as a speed bump only.
- Status enum must include `Verified`, `Tampered { expected, actual }`, `NoBaseline`, `OrphanedHash`, and `NotInstalled`.

implementation_steps:
- Create `src/edit_safety/integrity.rs`.
- Export `integrity` from `src/edit_safety/mod.rs`.
- Implement sidecar write for trusted `.symforge/` files.
- Implement verification and status enum.
- Add Unix permission behavior under `#[cfg(unix)]`.
- Update ADR 0015 to document the sidecar decision if not already documented.
- Add tests for verified, tampered, missing sidecar, orphaned sidecar, missing target, and Unix permissions.

acceptance_criteria:
- Trusted `.symforge/` file gets a sidecar.
- Unchanged file verifies as `Verified`.
- Changed file reports `Tampered`.
- Missing sidecar reports `NoBaseline`.
- Sidecar with no corresponding file reports `OrphanedHash`.
- Missing target reports `NotInstalled`.
- Unix permission behavior is tested under `#[cfg(unix)]`.

evidence_required:
- Sidecar format proof.
- Status enum proof.
- ADR 0015 update summary.
- Test output summary.

stop_conditions:
- The sidecar would need to recurse beyond trust-hash depth/path constraints.
- Sidecar writes would occur for untrusted files.
- The implementation would rely on chmod as a security guarantee.

## Verification Command

```bash
test "$(git branch --show-current)" = "rtk-symforge-integration"
git diff --check
cargo fmt --all -- --check
if [ -f tests/edit_safety_integrity.rs ]; then cargo test --test edit_safety_integrity -- --test-threads=1; else cargo test --test edit_safety_trust -- --test-threads=1; fi
cargo check
```

## Task Prompt

Run RTK08 only on branch `rtk-symforge-integration`. Implement the small integrity sidecar pattern as a complement to trust, keep it test-driven, and document that permissions are only a speed bump.

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
- RTK09 - Tier-2 metadata lookup helpers (RTK09-tier2-metadata-lookup-helpers.md)
