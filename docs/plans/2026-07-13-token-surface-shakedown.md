# Token Surface Shakedown Harness Implementation Plan

> **For implementation:** run each task in order. The only allowed SymForge product change is the manifest's reviewed compact-`symforge` metadata prerequisite.

**Goal:** Build and verify the smallest Windows harness that can execute the approved 20-run full-versus-compact shakedown reproducibly.

**Architecture:** One PowerShell script owns the frozen schedule, golden snapshot, semantic materialization baseline, fresh Codex/SymForge process lifecycle, JSONL token extraction, oracle inputs, trace summaries, and safe fixture cleanup. Each run recreates the same temporary worktree path from the frozen commit, copies the golden `.symforge/` state, completes zero-mismatch verification, and proves source/index fingerprints against the no-overwrite baseline before measurement. Raw host traces live temporarily on `C:`; compact results and final answers live under `research/token-cost/evidence/`. No dependency, database, dashboard, or generic benchmark framework is added.

**Tech stack:** PowerShell 7, Git, Codex CLI 0.144.2, one SHA-pinned SymForge 8.14.1 candidate artifact, built-in .NET JSON/process/file APIs.

---

### Task 1: Establish the red self-check

**Files:**

- Create: `research/token-cost/run-token-surface-shakedown.ps1`

1. Add parameters with actions `SelfTest`, `Prepare`, `Run`, `All`, and `Cleanup`.
2. In `SelfTest`, assert a 20-entry schedule with exactly five entries per task/arm cell.
3. Feed synthetic `turn.completed` JSONL through `Get-CodexUsage` and assert both supported modes: incremental events sum `input + output`; cumulative events use the final event; cached input is never re-added.
4. Assert fixture deletion is refused unless the resolved path equals the frozen temporary path and is outside the repository.
5. Run:

   `pwsh -NoProfile -File research/token-cost/run-token-surface-shakedown.ps1 -Action SelfTest`

   Expected before implementation: failure because schedule/usage/safety helpers are absent.

### Task 2: Implement only the pure helpers

**Files:**

- Modify: `research/token-cost/run-token-surface-shakedown.ps1`

1. Implement the frozen schedule as literal data from the manifest.
2. Implement newline JSON parsing for Codex events and canonical token summation.
3. Implement unexpected-worktree-change filtering that permits only `.symforge/` state.
4. Implement exact resolved-path checks for fixture cleanup.
5. Run `-Action SelfTest` again.

   Expected: PASS with no network, model, Git mutation, or child process.

### Task 3: Implement preflight and one-run execution

**Files:**

- Modify: `research/token-cost/run-token-surface-shakedown.ps1`
- Generate at runtime: `research/token-cost/evidence/shakedown-results.jsonl`
- Generate at runtime: `research/token-cost/evidence/run-<id>-answer.md`

1. `Prepare`:
   - verify free disk and exact fixture/golden paths;
   - create the detached fixture at `a10ff102546241f1ffd49852ba4d3088c0bb8029`;
   - use the configured pinned SymForge executable over MCP stdio to run `index_folder`, checkpoint, and health;
   - copy the complete verified `.symforge/` state and hash its `index.bin` as the golden snapshot;
   - stop owned processes and remove the preparation fixture through Git;
   - refuse to build Cargo artifacts.
2. `Run -RunId N`:
   - recreate the same fixture path at the frozen commit and copy/verify the golden state;
   - run Codex with `--ephemeral --ignore-user-config --json --sandbox read-only`;
   - configure only one SymForge MCP through command-line config overrides;
   - pass the frozen prompt on stdin;
   - store raw stdout/stderr under the declared temporary `C:` evidence root;
   - extract the final answer, token totals, ordered tool events, exit state, and wall time;
   - append one compact result record and preserve the final answer;
   - impose the same 20-minute timeout on every run and recursively kill the owned Codex process tree on timeout;
   - assert no newly owned Codex/SymForge child remains;
   - halt on incomplete trace or unexpected repository mutation;
   - remove the exact verified fixture through Git after trace verification.
3. `Cleanup`:
   - verify no owned child process is live;
   - verify the resolved fixture path exactly;
   - remove the worktree through Git and prune metadata;
   - retain raw traces until post-run Opus review; never remove global caches.

### Task 4: Harness smoke checkpoint

1. Run `-Action SelfTest`.
2. Run `-Action Prepare`.
3. Run one unmeasured wiring check per arm: direct MCP `tools/list` plus an explicit Codex tool call must confirm CLI injection and the assigned surface.
4. Run only manifest run 01.
5. Compare the real `turn.completed.usage` events from run 01. For one event, record that incremental and cumulative formulas coincide and use final-event input plus output as canonical; if any run emits more than one event, halt for explicit semantics review before continuing.
6. Verify the answer against the S1 checklist without using the arm label.
7. Ask Claude Opus to review:
   - the script diff;
   - self-test receipt;
   - run-01 raw trace and compact record;
   - token arithmetic, process-tree cleanup, fixture state, and trace secret-safety.
8. Fix blockers and re-review before running 02–20.

Checkpoint 3 returned `BLOCK`. The corrected gate found that both compact `status` and compact `symforge` are cancelled by noninteractive Codex because production compact tools omit the annotations present on full-router tools. Personal Codex config contamination is separately fixed through an isolated temporary `CODEX_HOME`. The old run 01 and all failed wiring attempts remain quarantined as pre-amendment baseline evidence.

### Task 4A: Restore compact read executability

**Files:**

- Modify: `src/stel/surface_list.rs`
- Modify: its existing colocated test module only; do not add a new test file

1. Extend the compact surface constructor so only `symforge` carries `read_only_hint=true` and `open_world_hint=false`.
2. Assert `symforge_edit` and `status` are not advertised with `read_only_hint=true`. `status` has a mutating `reset_calibration` mode and must not be mislabeled.
3. Do not change compact descriptions, schemas, routing, output, or STEL policy.
4. Run the narrow metadata test first, then the repository verification gates.
5. Set `CARGO_TARGET_DIR=C:\Users\rakovnik\AppData\Local\Temp\symforge-token-trust-target-8.14.1-a019`; record free space before the build. Never recreate `E:\project\symforge\target`.
6. Copy only the verified executable to `C:\Users\rakovnik\.codex\tools\symforge-token-trust-8.14.1-a019\symforge.exe` and record its SHA-256. Do not replace the currently running global SymForge executable in-place.
7. Remove the exact disposable Cargo target after the artifact is copied and all build/test receipts are captured.
8. Preserve the pre-amendment golden state and traces under quarantine, run `Prepare` with the pinned candidate, and regenerate the golden snapshot.
9. Re-run direct `tools/list` plus Codex wiring. Compact `symforge` must complete; compact `status` and `symforge_edit` need not be invoked and must remain non-read-only.
10. Restart the measured schedule from run 01 so every arm uses the same binary.

### Task 4B: Correct readiness materialization

The first amended run-01 retry stopped before treatment because a health-only process rewrote the golden snapshot to a different byte hash while verification was still pending. Reproduction and source inspection proved this was an invalid harness invariant, not treatment behavior: fresh-worktree mtimes differ, shutdown serializes again, and postcard iteration over `HashMap` is byte-nondeterministic.

1. Keep exact golden `index.bin` equality only before readiness.
2. In one unmeasured MCP process, poll `health_compact` through `pending`/`running` until `snapshot_restore`, `verify=completed`, and `mismatches=0`; fail closed on timeout or malformed evidence.
3. On clean shutdown, record the worktree-materialized snapshot hash as informational input identity.
4. Add `MaterializeBaseline` with a no-overwrite receipt containing the fixed commit/tree, a sorted tracked path/mode/file-hash manifest, full repo-outline digest, index/parse counts, and candidate SHA/version.
5. Probe the exact materialized bytes in a second unmeasured process and require completed zero-mismatch verification plus preserved semantic fingerprints.
6. Before every measured run, repeat materialization and assert all semantic fields against the baseline. Never compare per-process snapshot byte hashes across runs.
7. Record the residual stat-all plus 10% spot verifier as a bounded symmetric speed confound; do not claim all background verification was removed.
8. Run self-tests, the baseline action, a dry per-run readiness pass, and Claude Opus review before retrying run 01.

Claude Opus first returned `CHANGES_REQUIRED` for missing semantic equality and an overclaim about verification concurrency. The revised plan added the tree/source/outline fingerprints and explicit residual confound; Opus then returned `APPROVE_PLAN`.

### Task 5: Execute and close the shakedown

1. Run manifest runs 02–20 in the frozen order; stop on the first harness invariant failure.
2. Blind-grade every final answer twice against the frozen checklist.
3. Produce the descriptive cell table required by the manifest.
4. Ask Claude Opus to review trace completeness, grading, exclusions, and interpretation.
5. Update the research report and `tasks/todo.md` with receipts.
6. Remove the verified fixture and disposable run/session directories after review; report remaining disk usage.

No SymForge source edit beyond Task 4A is permitted in this plan. Any further product implementation requires the later powered experiment and a separate approved plan.
