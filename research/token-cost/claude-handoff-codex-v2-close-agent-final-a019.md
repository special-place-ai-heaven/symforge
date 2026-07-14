# Claude handoff — Codex V2 `close_agent` final smoke and local deployment

You are the independent final-checkpoint reviewer. Work read-only. Do not edit
Codex, SymForge, user configuration, installed binaries, npm state, GitHub, or
process state.

## Required order

1. Read this handoff completely.
2. Read
   `research/token-cost/codex-v2-close-agent-final-smoke-2026-07-14.md`.
3. Read your prior report
   `research/token-cost/claude-report-codex-v2-close-agent-namespace-a019.md`.
4. Inspect the uncommitted Codex worktree at
   `E:\project\codex-v2-close-agent`, pinned to `rust-v0.144.4` base
   `8c68d4c87dc54d38861f5114e920c3de2efa5876`.
5. Independently verify the seven-file source custody, candidate hash, smoke
   interpretation, current installed-binary boundary, and proposed rollback.
6. Write your report to
   `research/token-cost/claude-report-codex-v2-close-agent-final-a019.md`.

## Facts that must govern the review

- Your prior verdict was `APPROVE_CONFIG_DEPLOY`: keep the seven-file patch,
  use the supported whole-surface namespace `agents`, and disclose the backend
  reserved-namespace dependency upstream.
- The final smoke used the same reviewed candidate SHA-256
  `86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00`.
- The configured capacity was root plus exactly one worker. The sequence was
  1 -> 2 -> 1 -> 2 -> 1 -> 0 owned MCP processes; therefore the replacement
  spawn after the first close is a live slot-release proof.
- Worker closes preserved the root and unrelated sentinel, post-close
  follow-up was rejected, both close results were `completed`, and the
  candidate exited 0 with `sequence_complete: true`.
- Five five-second `sync` holds were client-cancelled. Per your approved
  protocol, these are observation-scaffolding notes and do not override the
  deterministic process/tool-result grade.
- The helper-complete Windows package gate remains honestly red at 2,582 pass,
  11 fail, 1 timeout, and 46 skipped. No V2 close/spec-plan test failed.
- Fresh custody still shows exactly seven source paths, clean `Cargo.lock`,
  `git diff --check` exit 0, no disposable smoke home/workspace, and zero test
  MCP processes.
- No installation or live configuration mutation has occurred.

## Questions you must answer

1. Does the final smoke satisfy your approved deterministic grading protocol?
2. Does root-plus-one saturation followed by a successful replacement spawn
   conclusively demonstrate V2 residency/slot release?
3. Do process exit, follow-up rejection, list absence, and root preservation
   jointly prove the intended teardown without overclaiming protection from
   issue #25426's separate wedged-termination case?
4. Is the seven-file source custody unchanged and still suitable for commit?
5. Is the red full-package Windows result reported honestly enough to permit a
   scoped local commit/install without claiming the package gate is green?
6. Is placing the pinned candidate at
   `C:\Users\rakovnik\.npm-global\codex.exe` the smallest reversible way to
   shadow the adjacent npm `.cmd` launcher while leaving `node_modules` and
   the npm platform-binary cache untouched?
7. Is adding only `[features.multi_agent_v2]` plus
   `tool_namespace = "agents"` correct given that the live config has a
   `[features]` table but no `multi_agent_v2` scalar/table?
8. Are the planned post-install checks sufficient: hash, version, command
   resolution, config parse, and one fresh tool-list observation?
9. Is rollback complete by deleting the standalone `.exe` and the nested
   namespace table, after which the untouched npm shim resumes control?
10. May the 29.8 GB Cargo target and external worktree be removed immediately
    after the commit and installed-binary verification, while retaining the
    branch/commit and in-repository evidence?

## Required verdict

Choose exactly one:

- `APPROVE_COMMIT_INSTALL_CLEANUP` — the primary agent may commit the reviewed
  patch, perform the exact reversible local deployment, verify it, and remove
  the disposable target/worktree;
- `CHANGES_REQUIRED` — list the minimum exact source, test, smoke, install, or
  rollback correction;
- `BLOCKED` — state the external dependency that prevents an honest local
  deployment.

Separate blockers from non-blocking upstream recommendations. Include the
exact commit scope, exact install/config scope, rollback steps, cleanup scope,
and the concise upstream disclosure. Do not run another model-backed smoke,
commit, install, mutate config, or delete retained artifacts.
