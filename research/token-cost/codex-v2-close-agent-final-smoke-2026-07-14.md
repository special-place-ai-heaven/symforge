# Codex V2 `close_agent` final smoke and deployment boundary

Date: 2026-07-14

## Outcome

The reviewed standalone candidate completed the deterministic Windows process
smoke through the supported `agents` namespace. A worker-owned MCP process
exited at each `close_agent` call, the root-owned MCP process survived both
closes, a replacement worker spawned at a deliberately saturated root-plus-one
capacity, and every owned MCP process was gone after normal root exit. The
separately owned sentinel was unaffected.

After the final independent review returned `APPROVE_COMMIT_INSTALL_CLEANUP`,
the exact reviewed patch was committed, the hash-pinned binary and reversible
namespace override were installed, the installed tool surface was exercised,
and the disposable target and linked worktree were removed.

## Candidate and source custody

- Branch: `fix/multi-agent-v2-close-agent`
- Pinned parent: `8c68d4c87dc54d38861f5114e920c3de2efa5876`
  (`rust-v0.144.4`)
- Retained commit: `288cdc6ec16c6d7c6bd0f6eceb09ac40a5cf7e0a`
- Installed binary: `C:\Users\rakovnik\.npm-global\codex.exe`
- Installed version: `codex-cli 0.144.4`
- Installed SHA-256:
  `86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00`
- Installed size: 375,386,624 bytes
- The retained commit contains exactly the seven reviewed paths. Its parent is
  the pinned base, `Cargo.lock` is unchanged, and no SymForge, dependency, or
  unrelated changes are present.
- The original npm shell and command shims are byte-identical to their
  pre-install hashes.

## Verification before the smoke

- Focused V2 family: 65/65 passed.
- Default namespace, configured namespace, code-mode exposure, close result,
  post-close follow-up rejection, and list omission all passed.
- Required `just fix -p codex-core`: exit 0.
- `cargo fmt --all -- --check`: exit 0; the stable toolchain emitted only the
  repository's existing unstable rustfmt-option warnings.
- Helper-complete `just test -p codex-core`: 2,594 tests, 2,582 passed,
  11 failed, 1 timed out, and 46 skipped. No V2 close/spec-plan coverage
  failed. This package gate remains red and must not be described as green;
  the residual failures are outside this patch's scope and are documented in
  the prior namespace review.

## Deterministic final-smoke configuration

- Standalone candidate by absolute path; no npm-cache substitution.
- Isolated empty workspace and isolated `CODEX_HOME`.
- `features.multi_agent_v2 = true`.
- `features.multi_agent_v2.tool_namespace = "agents"`.
- `features.multi_agent_v2.max_concurrent_threads_per_session = 2`, making
  root plus one worker the hard capacity.
- One `test_stdio_server.exe` MCP sentinel per live Codex thread.
- A 250 ms process-transition observer.
- One separately owned unrelated Node sentinel.
- Five-second MCP `sync` calls used only as observation holds. Their client
  cancellation is recorded as a harness note, never as a treatment grade, per
  the approved namespace-review protocol.

## Observed sequence

The admissible process timeline was:

| Time | Owned MCP PIDs | Meaning |
|---:|---|---|
| 190,206 ms | 49256 | root only |
| 204,674 ms | 44988, 49256 | first worker live |
| 218,998 ms | 49256 | first worker exited at close |
| 224,982 ms | 29112, 49256 | replacement worker live |
| 239,174 ms | 49256 | replacement exited at close |
| 245,718 ms | none | root exited normally |

Additional deterministic observations:

- First worker returned `WORKER_READY`; close returned previous status
  `completed`.
- Post-close `followup_task` rejected `/root/sentinel_worker` because the
  target no longer existed.
- The replacement spawn succeeded while the configured limit was root plus
  one. Because the first worker had occupied the only worker slot, this is a
  live capacity-release proof, not an inference from source.
- Replacement returned `REPLACEMENT_READY`; close returned previous status
  `completed`.
- Root MCP PID 49256 survived both worker closes and exited only with the root
  process.
- The unrelated sentinel remained live after candidate exit and was then
  stopped explicitly.
- Candidate exit code was 0. Its final structured result reported
  `sequence_complete: true`, both ready markers, both completed close statuses,
  rejected post-close follow-up, and both workers absent.

Terminal Commander receipts:

- candidate job `job_019f60c76a317571bfc882482baa2418`, bucket
  `bkt_019f60c76a317571bfc8822e06caa893`;
- corrected transition observer job
  `job_019f60c48a3d72308169430ba8ed01d2`;
- unrelated sentinel job `job_019f60c30a3777208efcfa0e2fd2abf4`.

## Cleanup and retained state

- Isolated auth-bearing `CODEX_HOME` absent.
- Isolated smoke workspace absent.
- `test_stdio_server.exe` process count is zero.
- The user config has exactly one `[features.multi_agent_v2]` table and one
  `tool_namespace = "agents"` assignment, with no `multi_agent_v2 = true`
  scalar. The check was structural and did not print the secret-bearing file.
- `cargo clean --target-dir` removed 48,707 files and reported 27.8 GiB
  removed. The target directory is absent.
- The linked worktree `E:\project\codex-v2-close-agent` is absent and pruned
  from the worktree list.
- The bare repository `E:\project\.codex-v2-close.git`, branch, and retained
  seven-path commit remain present.
- Final measured free space on `E:` was 17,539,604,480 bytes. This measured
  value is authoritative; the Cargo-reported removal size is not presented as
  the drive's net free-space increase.

## Completed reversible local installation

The npm JavaScript launcher has no supported binary override. The existing
user PATH contains `C:\Users\rakovnik\.npm-global`. The cache-preserving
deployment completed as follows:

1. The reviewed seven-file patch was committed on the pinned Codex branch.
2. The hash-pinned standalone candidate was copied atomically to
   `C:\Users\rakovnik\.npm-global\codex.exe`, adjacent to but outside the
   npm package and its platform-binary cache.
3. Only this nested table was appended atomically to the user config:

   ```toml
   [features.multi_agent_v2]
   tool_namespace = "agents"
   ```

   No `multi_agent_v2 = true` scalar was added; V2 selection remains governed
   by the host's existing model/session precedence.
4. The installed SHA-256 and version passed. Config parsing passed. A sanitized
   fresh Win32 environment resolved a bare `codex --version` spawn to the new
   executable and returned `codex-cli 0.144.4`. The exact PowerShell
   `Get-Command codex` check was not run because Terminal Commander's enforced
   policy forbids the shell lane; that policy was not bypassed. Git Bash still
   resolves the untouched extensionless npm shim.
5. A live call through the installed product invoked `agents.close_agent` on
   `/root` and returned `root is not a spawned agent`. This handler-level
   rejection proves the installed binary registered and dispatched the tool;
   the prior reserved-namespace HTTP 400 did not recur. Terminal Commander job
   receipt: `job_019f60e03c91711283a15c39ba840f6a`.
6. The disposable Cargo target and linked worktree were removed only after
   install verification. The bare repository was retained so the commit remains
   recoverable.

Rollback remains one executable deletion plus removal of the two-line nested
config table. The untouched npm shim then becomes active again.

The namespace line is temporary. Remove it when the Responses backend accepts
`collaboration.close_agent` in its reserved namespace.
