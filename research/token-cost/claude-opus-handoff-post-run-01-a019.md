# Claude Opus handoff — independent post-run-01 checkpoint

Use Claude Code with the **Opus** model in `E:\project\symforge`.

## Objective

Independently audit the SymForge token/speed/tool-trust shakedown at the post-run-01 checkpoint and decide whether runs 02–20 may proceed. This is a read-only review. Do not implement fixes, alter evidence, commit, push, or create a PR.

Write the completed review to exactly:

`research/token-cost/claude-opus-report-post-run-01-a019.md`

Do not modify any other file. If that report path already exists, stop and report the collision instead of overwriting it.

## Mandatory safety and repository rules

1. Read and follow `AGENTS.md` first, including the no-secrets rule.
2. Never reproduce a secret value in the report or terminal output. If one is found, report only its name and location.
3. Use SymForge for source-code inspection where available. Direct reads are appropriate for the exact Markdown/JSON/JSONL/PowerShell evidence named below.
4. Do not run Cargo, create a `target/`, create another worktree, install/update dependencies, or launch a measured benchmark run.
5. You may run the harness `SelfTest` and read-only Git/process/disk checks. Do not run `Prepare`, `WiringCheck`, `MaterializeBaseline`, `Run`, `Grade`, `All`, or `Cleanup`.
6. Treat existing working-tree changes as shared/user-owned. Do not revert, normalize, or reformat them.

## Review inputs

Read these completely or inspect the relevant structured records:

- `AGENTS.md`
- `tasks/lessons.md`
- `tasks/todo.md` — current campaign section
- `research/token-cost/token-speed-tool-trust-reconnaissance-2026-07-13.md`
- `research/token-cost/token-speed-tool-trust-benchmark-manifest-2026-07-13.md`
- `docs/plans/2026-07-13-token-surface-shakedown.md`
- `research/token-cost/run-token-surface-shakedown.ps1`
- `src/stel/surface_list.rs`
- `src/main.rs` — local MCP startup/shutdown and background verification
- `src/live_index/persist.rs` — snapshot load/build/verify/write behavior
- `src/protocol/format.rs` — snapshot verification rendering
- `research/token-cost/evidence/token-surface-shakedown-a019/prepare-receipt.json`
- `research/token-cost/evidence/token-surface-shakedown-a019/wiring-receipt.json`
- `research/token-cost/evidence/token-surface-shakedown-a019/semantic-baseline.json`
- `research/token-cost/evidence/token-surface-shakedown-a019/shakedown-results.jsonl` — inspect only `run_id=1`
- `research/token-cost/evidence/token-surface-shakedown-a019/run-01-grader.md`
- `research/token-cost/evidence/token-surface-shakedown-a019/run-01-opus-checkpoint.json` — prior summary, not authoritative
- Raw run-01 files under `C:\Users\rakovnik\AppData\Local\Temp\symforge-token-shakedown-evidence-a10ff102-a019\`

The raw JSONL is primary evidence. Parse it selectively; do not paste the complete trace into the report.

## Facts to verify independently

Do not accept these merely because they are stated here:

1. Branch is `feat/token-speed-tool-trust`; only the authorized compact-read annotation changed SymForge product code.
2. Candidate is SymForge `8.14.1` with the same SHA-256 in the script, prepare receipt, wiring receipt, semantic baseline, and run record.
3. Compact `symforge` alone is truthfully read-only/closed-world; `status` and `symforge_edit` are not advertised read-only.
4. Run 01 is the frozen `S1` / `A-full` assignment and starts only after semantic materialization.
5. Golden input hash, Git tree, tracked-source manifest, repo-outline digest, index/parse counts, version, and candidate SHA all match the semantic baseline.
6. Snapshot verification reached `snapshot_restore`, `completed`, and zero mismatches. The per-process materialized byte hash is informational, not a cross-run equality key.
7. Exactly one `turn.completed` usage event exists. Independently recompute canonical tokens using the manifest rule and verify the compact record.
8. Re-tally completed SymForge calls, failures, first substantive tool, native events, and normalized event count from raw JSONL.
9. Verify configuration/secret diagnostic counts and inspect the trace for evidence that personal Codex configuration contaminated the run. Do not quote any sensitive material.
10. Verify the fixture, candidate process, isolated Codex home, Cargo targets, and disposable worktree were removed. Do not kill unrelated processes.
11. Re-grade the redacted answer blindly against the frozen S1 criteria. Check that `oracle_grade_count=1` and the stored failures match your independent grade.
12. Decide whether a wrong S1 answer remains a valid intent-to-treat observation.

## Questions that require explicit judgment

1. Does the current readiness/materialization design produce comparable logical starting states across arms?
2. Can any harness path create a false pass, silent exclusion, duplicate grade, contaminated token count, or cross-arm asymmetry?
3. Is the frozen S1 oracle valid? In particular, assess the tension between required `compact_probe_tools` in non-shipping probe scaffolding and production `compact_surface_tools`. Do not silently change the existing grade. State what must happen before the later confirmatory pilot.
4. Run 01 has one usage event, so incremental and cumulative interpretations coincide. Is continuation safe given that the harness halts if a later run emits multiple usage events?
5. Are raw evidence retention and disk/worktree cleanup proportionate to the checkpoint?

## Required verification

Run only safe, read-only checks that materially support the verdict. At minimum:

- `pwsh -NoProfile -File research/token-cost/run-token-surface-shakedown.ps1 -Action SelfTest`
- `git status --short --branch`
- `git diff --check`
- read-only process/worktree/path existence checks for the exact candidate and fixture paths

If a check cannot run, record the limitation; do not substitute an unsupported claim.

## Report format

The report must contain:

1. `# Claude Opus independent post-run-01 review`
2. `## Verdict` with exactly one of:
   - `APPROVE_CONTINUE`
   - `CHANGES_REQUIRED`
   - `BLOCK`
3. `## Evidence verified` — concise table of claimed versus independently observed values.
4. `## Findings` — severity-ranked, with exact file/evidence references. Do not invent style findings.
5. `## Oracle assessment` — deterministic run-01 grade plus the probe-vs-production validity issue.
6. `## Cleanup and safety` — processes, worktrees, disk artifacts, configuration, and secret diagnostics.
7. `## Required actions` — use `None before runs 02–20` only if genuinely approved; otherwise give bounded changes and verification.
8. `## Confirmatory-pilot note` — what must be corrected or redesigned before results are used for product decisions.

The final line of the report must be exactly one of:

`VERDICT: APPROVE_CONTINUE`

`VERDICT: CHANGES_REQUIRED`

`VERDICT: BLOCK`
