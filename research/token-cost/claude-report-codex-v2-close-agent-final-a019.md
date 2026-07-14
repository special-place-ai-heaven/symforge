# Independent final review: Codex V2 `close_agent` smoke + local deployment (A019)

Reviewer: Claude (Fable 5), 2026-07-14. Read-only; no smoke run, no commit,
no install, no config mutation, no deletion. Order followed: handoff → final
smoke report → my prior namespace report → independent worktree/host
inspection.

## Independently re-verified custody

- Worktree `E:\project\codex-v2-close-agent` at pinned base `8c68d4c8…`
  (`rust-v0.144.4`); dirty state is exactly the reviewed seven paths
  (6 modified + new `multi_agents_v2/close_agent.rs`, 69 insertions);
  `Cargo.lock` clean; `git diff --check` exit 0. Identical to both prior
  reviews — the patch has not drifted.
- Candidate `codex-rs/target/debug/codex.exe` re-hashed:
  `86B876536D06A70C58A03CB5104FC4F8D33875E4DB0B79FE64C79A84D7CD2E00` —
  matches the smoke receipt; the smoked binary is the reviewed binary.
- Installed-binary boundary: `C:\Users\rakovnik\.npm-global` currently
  contains `codex` (extension-less sh shim) and `codex.cmd`, **no**
  `codex.exe`; `where codex` confirms. No npm `node_modules` or
  platform-binary cache path is implicated by the plan.
- Live user config: `[features]` table exists (line 13); zero occurrences of
  `multi_agent_v2` or `tool_namespace` anywhere in the file (checked by name
  only; no values read). The proposed append cannot collide.
- Zero `test_stdio_server.exe` processes; no smoke home/workspace remains.
- **New structural finding:** the checkout is a *linked worktree* of the
  bare repository `E:/project/.codex-v2-close.git` (`.git` file →
  `gitdir: E:/project/.codex-v2-close.git/worktrees/codex-v2-close-agent`;
  `git worktree list` confirms; origin = `https://github.com/openai/codex.git`,
  blob-filtered). The branch/commit live in the bare repo, not the worktree —
  this materially sharpens the cleanup scope (Q10).

## Answers to the ten questions

**1. Does the smoke satisfy the approved protocol?** Yes, fully. It executed
every step of my prescribed protocol including the newly required
replacement-spawn step: isolated home/workspace, `tool_namespace='agents'`,
250 ms transition observer, unrelated sentinel, deterministic grading on
process transitions/tool results/exit codes. The five cancelled 5-second
holds were recorded as harness notes, exactly as the protocol demands — they
were observation scaffolding, and every deterministic assertion they were
scaffolding for was independently observed (the census table shows the
transitions the holds existed to expose).

**2. Does saturation + replacement spawn prove slot release?** Yes,
conclusively. With `max_concurrent_threads_per_session = 2`, the single
worker slot was provably occupied by the first worker; the replacement spawn
at 224,982 ms could only succeed if `close_agent` released that slot. This
is the live proof of `release_spawned_thread`/`forget_v2_residency` that my
namespace review said was still only code-verified. The
1 → 2 → 1 → 2 → 1 → 0 sequence with matching PIDs closes the loop.

**3. Teardown proven without #25426 overclaim?** Yes. Jointly: worker MCP
PID exited at each close (process teardown), post-close `followup_task`
rejected `/root/sentinel_worker` (registry/target removal), both close
results returned `previous_status=completed` (status contract), root MCP
survived both closes and exited only at normal root exit (no collateral),
unrelated sentinel unaffected (no over-kill). The smoke report makes no
claim about wedged-termination protection; issue #25426 remains a disclosed,
separate hardening concern. Correctly scoped.

**4. Source custody suitable for commit?** Yes — re-verified above,
byte-consistent with the `APPROVE_CONTINUE` review and untouched since.

**5. Red package gate reported honestly?** Yes. The smoke report states the
helper-complete gate "remains red and must not be described as green"
(2,582/11/1/46), attributes the residuals to documented out-of-scope classes,
and confirms no V2 close/spec-plan failure. That is the honest framing my
prior review required. The commit message and any install note must repeat
this framing (scoped gates green; full Windows package gate red with
pre-existing environmental failures).

**6. Is `.npm-global\codex.exe` the smallest reversible shadow?** Yes, with
one sharpening. Placing `codex.exe` beside the shims is additive (no npm
file modified), reversible by single-file deletion, and correct for
cmd/PowerShell/Win32 resolution, where PATHEXT ranks `.EXE` before `.CMD`.
Sharpening: the directory also contains an **extension-less `codex` sh
shim** — POSIX-style shells (Git Bash, MSYS) will keep resolving that shim,
bypassing the shadow entirely. Not a blocker (the user's Codex entry points
are PowerShell/cmd), but the post-install verification must resolve `codex`
from the shell the user actually launches Codex from, not only via
`where codex` (see Q8).

**7. Is the two-line config append correct?** Yes. TOML permits defining the
nested table `[features.multi_agent_v2]` after a previously defined
`[features]` table, provided `multi_agent_v2` was not already defined — and
I verified it is absent. The client-side namespace validator accepts
`agents`. Appending exactly:

```toml
[features.multi_agent_v2]
tool_namespace = "agents"
```

is the minimal, collision-free, reversible edit. Note the smoke's
`features.multi_agent_v2 = true` scalar was an isolated-home setting; the
live config must NOT get that scalar (it would collide with the table form
and, per the diagnostic history, V2 selection on this host comes from model
metadata anyway).

**8. Are the planned post-install checks sufficient?** Yes, with two
additions. Keep: SHA-256 of the installed file, `codex --version` in a clean
subprocess, `where codex`, config parse, one post-restart tool-list
observation showing `agents.close_agent`. Add: (a) resolve `codex` from the
actual launch shell (PowerShell `Get-Command codex` → must be the new
`.exe`; note Git Bash `type codex` will still name the sh shim — expected,
document it); (b) confirm the npm shim files (`codex`, `codex.cmd`) are
byte-unchanged after install, proving the boundary claim.

**9. Is rollback complete?** Yes. Deleting
`C:\Users\rakovnik\.npm-global\codex.exe` restores `.cmd`/shim resolution
(the npm package and platform-binary cache were never touched), and removing
the two-line nested table restores the config byte-state (it contained no
`multi_agent_v2` content before). Both steps are independent and
order-free. Rollback leaves zero residue.

**10. May the 29.8 GB target and worktree be removed after commit +
verification?** Yes — with the scope corrected for the worktree topology.
The branch/commit live in the bare repo `E:/project/.codex-v2-close.git`,
NOT in the worktree. Therefore, after the commit is made **in the worktree**
and the installed binary is verified:

1. `cargo clean` / delete `codex-rs/target` (~29.8 GB, regenerable);
2. `git worktree remove E:/project/codex-v2-close-agent` then
   `git worktree prune` (run from the bare repo);
3. **retain `E:/project/.codex-v2-close.git`** — deleting it would destroy
   the branch and commit the handoff requires retained. It is small
   (blob-filtered clone) and is now the sole custody of the patch.

In-repository evidence under `research/token-cost/` is retained as-is.

## Blockers

None.

## Non-blocking upstream recommendations (unchanged from prior reviews)

1. Add root/self-rejection and a capacity-release test at the tool surface
   before upstream submission (the live smoke now proves the latter, but an
   upstream CI cannot run this smoke).
2. Pin one red package-gate test to the clean base tag to convert
   "categorically pre-existing" into "measured pre-existing".
3. Disclose the reserved-namespace dependency (below) in the PR body.

## Exact approved scopes

**Commit scope** (in the worktree, branch `fix/multi-agent-v2-close-agent`):
exactly the seven files —
`codex-rs/core/src/config/mod.rs`,
`tools/handlers/multi_agents_spec.rs`,
`tools/handlers/multi_agents_tests.rs`,
`tools/handlers/multi_agents_v2.rs`,
`tools/handlers/multi_agents_v2/close_agent.rs` (new),
`tools/spec_plan.rs`, `tools/spec_plan_tests.rs`.
No `Cargo.lock`, no smoke scripts, no evidence files. Commit message states:
V2 `close_agent` bridge to `AgentControl::close_agent`; scoped gates green
(V2 family 65/65, fmt, `just fix -p codex-core`); full Windows package gate
red with pre-existing out-of-scope failures; live-verified via
`tool_namespace='agents'` due to the backend reserved-namespace allowlist.

**Install/config scope:** copy the hash-pinned candidate
(`86B8…2E00`) to `C:\Users\rakovnik\.npm-global\codex.exe`; atomically
append only the two-line `[features.multi_agent_v2]` table above to the user
config; nothing else. Then the Q8 verification set.

**Rollback:** delete `C:\Users\rakovnik\.npm-global\codex.exe`; remove the
two-line table. Untouched npm shim resumes control.

**Cleanup scope (after verified install):** delete `codex-rs/target`;
`git worktree remove` + `prune` the checkout; retain the bare repo
`E:/project/.codex-v2-close.git`, the branch/commit, and all in-repo
evidence. The namespace config line is temporary — remove it if/when the
backend allowlists `collaboration.close_agent`.

## Upstream disclosure (concise)

Codex MultiAgentV2 exposes no `close_agent`, so completed V2 residents hold
their process stacks until LRU pressure. This seven-file patch adds a
V2-native `close_agent` bridging to the existing `AgentControl::close_agent`
teardown. Live-verified end to end (worker MCP teardown at close, root
preserved, post-close follow-up rejected, slot release proven by replacement
spawn at saturated capacity) — but only under
`features.multi_agent_v2.tool_namespace` override, because the ChatGPT-backed
API rejects the default with HTTP 400: "Function 'collaboration.close_agent'
is not allowed in reserved namespace 'collaboration'". Merging requires the
server-side reserved-namespace allowlist to add `collaboration.close_agent`.

VERDICT: APPROVE_COMMIT_INSTALL_CLEANUP
