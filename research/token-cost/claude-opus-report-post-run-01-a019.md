# Claude Opus independent post-run-01 review

Reviewer: independent second-terminal Claude session (Fable 5), 2026-07-13.
Scope: read-only audit of the token/speed/tool-trust shakedown at the post-run-01
checkpoint, per `research/token-cost/claude-opus-handoff-post-run-01-a019.md`.
No source, evidence, or harness file was modified; only this report was written.

## Verdict

`APPROVE_CONTINUE`

Runs 02–20 may proceed under the frozen manifest. The one substantive validity
issue (S1 oracle criterion 4/5 probe-vs-production tension) is real but
preregistered, deterministic, and arm-symmetric; it must be resolved before the
confirmatory pilot, not before the descriptive shakedown continues.

## Evidence verified

| Claim | Independently observed | Match |
|---|---|---|
| Branch `feat/token-speed-tool-trust` | `git status --short --branch` → same | ✓ |
| Only authorized product change | `git diff src/stel/surface_list.rs`: annotations on compact `symforge` only (`read_only_hint=Some(true)`, `open_world_hint=Some(false)`) + colocated regression test asserting `symforge_edit`/`status` are NOT read-only; no schema/description/routing change. Other modified tracked files are `tasks/*.md` only | ✓ |
| Candidate 8.14.1, one SHA everywhere | Live `Get-FileHash` on pinned exe = `6C4176…FC3B`; identical in script, prepare receipt, wiring receipt, semantic baseline, and run-01 record; version 8.14.1 in health text and all receipts | ✓ |
| Compact annotations truthful | Wiring receipt: `symforge_read_only=true`, `symforge_open_world=false`, `symforge_edit_read_only=null`, `status_read_only=null`; matches the source diff and MCP defaults (absent ⇒ not advertised read-only) | ✓ |
| Run 01 = frozen S1/A-full, post-materialization | JSONL record: run_id=1, block=1, task=S1, arm=A-full, `readiness_verdict=ready` with `snapshot_load_source=snapshot_restore`, verify states `pending,running,completed`, `mismatches=0`, readiness 30,752 ms logged outside model wall time | ✓ |
| Semantic baseline equality | Golden input `5941FB…54D5`, tree `30704cf8…`, manifest `8CB1C3…60C7`, 851 tracked, outline `006DEB…2ADD`, 726/720/4/2 files, 21,830 symbols, version+SHA — all identical between `semantic-baseline.json` and the run-01 record | ✓ |
| Byte hash informational only | Run-01 `snapshot_hash` (`924920…B5AB`) differs from baseline's first/probe materialized hashes; record carries the explicit `snapshot_hash_semantics` disclaimer; harness (`Assert-SemanticBaselineMatch`) never compares it | ✓ |
| Exactly one usage event; canonical tokens | Raw `run-01.events.jsonl` reparsed independently: 1 `turn.completed` usage event; input 237,616 + output 3,029 = **240,645**; incremental and cumulative coincide; cached 193,792 retained as informational subset, never re-added | ✓ |
| Tool-event tallies | Raw reparse: 20 tool events, 8 completed SymForge calls (0 failed), order `search_symbols, search_text, get_symbol, search_symbols, get_symbol, get_file_context, search_text, get_symbol`, first substantive = `search_symbols`, 2 completed native events | ✓ |
| Config/secret diagnostics | Independent recount over raw stdout+stderr with the harness's exact patterns: 0 configuration-diagnostic lines, 0 potential-secret lines; no `.codex-home-*` residue in the raw trace | ✓ |
| Frozen S1 symbol ranges | `surface_profile_from_env` 26–41, `list_tools_for_profile` 167–177, `compact_probe_tools` 253–285 in `src/protocol/surface_probe.rs` — all exact | ✓ |
| SelfTest | `-Action SelfTest` → `PASS`, exit 0 | ✓ |
| Grading state | `oracle_grade_count=1`, `record_status=graded`, failures = criteria 4 and 5 only | ✓ |

## Findings

1. **MEDIUM (validity, not a blocker for the shakedown): the frozen S1 oracle requires the wrong constructor for the production question.**
   `ServerHandler::list_tools` (`src/protocol/mod.rs:1341-1343`) dispatches
   `SurfaceProfile::Compact` directly to `crate::stel::compact_surface_tools()`
   (`src/stel/surface_list.rs:37`); `compact_probe_tools`
   (`src/protocol/surface_probe.rs:253`, self-documented as "H1 / A-005
   measurement... not STEL runtime") is reachable only through
   `list_tools_for_profile`, whose compact arm production never takes. The
   run-01 answer named the production-true constructor and was failed on
   criteria 4/5 for it. See "Oracle assessment." The frozen grade must stand;
   the oracle must be corrected before any confirmatory use.
2. **LOW (symmetric, disclosed by design): candidate binary vs. fixture source divergence.**
   The pinned binary includes the uncommitted Amendment A annotation change;
   the fixture at `a10ff102` does not. A model reading
   `src/stel/surface_list.rs` in the fixture sees annotation-free source while
   the serving binary advertises annotations. Neither S1 nor S2 asks about
   annotations, and the divergence is identical across arms, so it cannot bias
   this shakedown; commit the change before the confirmatory pilot so binary
   and fixture source agree.
3. **LOW (measurement semantics, cosmetic): `tool_event_count=20` counts both `item.started` and `item.completed` events.**
   The completed-call fields (`symforge_call_count`, `native_tool_count`) are
   correct and are what the analysis uses; just don't read
   `tool_event_count` as a call count in the final report.
4. **INFO: retained quarantine evidence.** One pre-receipt wiring quarantine
   directory exists under the raw evidence root
   (`quarantine\wiring-pre-receipt-20260713T122407567Z`), consistent with the
   manifest's rule that failed preflight attempts are preserved separately.

No false-pass, silent-exclusion, duplicate-grade, token-contamination, or
cross-arm-asymmetry path was found in the harness: grading is single-shot and
fail-closed (`Set-OracleGrade` refuses non-`captured_ungraded` records and
enforces verdict/failure consistency), rerun of any graded run is refused,
incomplete traces halt after being preserved, both arms pass the identical
semantic-readiness gate, and the readiness phase runs in a separate unmeasured
process before Codex receives the prompt.

## Oracle assessment

**Deterministic re-grade of run 01 (blind, redacted answer vs. frozen S1 checklist):**

- Criterion 1 (`SYMFORGE_SURFACE`): **pass**.
- Criterion 2 (default `Full`; explicit `compact`/`meta`): **pass** — cites the
  `_ => SurfaceProfile::Full` arm at surface_probe.rs:37, verified correct.
- Criterion 3 (exactly `symforge`, `symforge_edit`, `status`): **pass**.
- Criterion 4 (all three named functions incl. `compact_probe_tools`): **fail**
  — the answer substitutes `compact_surface_tools`; the frozen checklist
  states "substitute symbols are not [accepted]".
- Criterion 5 (a correct path+line inside each frozen range): **fail** — no
  citation inside surface_probe.rs:253–285.

Independent verdict: **Fail, criteria 4 and 5** — byte-for-byte the stored
`oracle_failures`. Grading is deterministic and reproducible;
`oracle_grade_count=1` confirmed.

**Probe-vs-production validity issue:** the run-01 answer is arguably *more*
correct about production than the frozen oracle: the function that "constructs
the compact list" actually served over MCP is `compact_surface_tools`
(mod.rs:1342), and `compact_probe_tools` is non-shipping probe scaffolding.
The frozen oracle therefore penalizes a truthful production answer. This is a
task-oracle defect, not a harness defect. The correct handling — which the
prior checkpoint also chose — is: do not rewrite the grade mid-series (that
would be outcome-driven re-grading), keep S1 scoring identical for all 20 runs
(it is applied to both arms 5× each, so it cannot create cross-arm asymmetry),
and treat run 01's Fail as a valid intent-to-treat observation of the assigned
arm. Before the confirmatory pilot, S1 criterion 4/5 must be redesigned to
accept the production constructor (`compact_surface_tools`,
src/stel/surface_list.rs) — either as the required answer or by requiring the
answer to distinguish the production path from the probe path.

**Intent-to-treat:** yes — a wrong answer after successful treatment exposure
is exactly the outcome class the manifest requires to be scored, not excluded.
`exclusion=null` is correct.

**Continuation safety on token semantics:** with one usage event, incremental
and cumulative interpretations coincide (240,645 both ways, recomputed). The
recorded canonical rule (final-event input+output) plus the hard halt on
`usage_event_count > 1` (`Invoke-OneRun` throws before grading) is fail-closed:
no later run can silently mix semantics. Safe to continue.

## Cleanup and safety

- **Fixture:** `C:\...\symforge-token-shakedown-a10ff102` does not exist;
  `git worktree list` shows only the development checkout.
- **Processes:** zero `symforge.exe` processes from the candidate path; the
  harness's own before/after PID audit is present in `Invoke-CodexProcess`.
- **Disposable Cargo target:** `symforge-token-trust-target-8.14.1-a019` is
  removed. No `target/` exists in the repository root beyond the gitignored
  configured location (none created by this campaign).
- **Isolated Codex homes:** none remain under the raw evidence root; the
  harness deletes them in `finally` and path-guards every mutation.
- **Retained by design:** golden state, semantic baseline, compact evidence,
  raw run-01/wiring traces (with quarantined pre-receipt attempts) — all
  required inputs for the post-20-run review; proportionate to the checkpoint.
- **Secrets/config:** independent recount of the harness's sensitive and
  configuration patterns over raw stdout+stderr found 0 hits; the only
  credential handling is copying `auth.json` into the ephemeral isolated home,
  which is deleted per run and whose value never enters traces or evidence.
- **PATH-shadow warning** in the prepare receipt (bare `symforge` resolves to
  an npm shim) is informational; the harness always invokes the pinned
  absolute path and verifies its SHA-256 before every connection.

## Required actions

`None before runs 02–20.`

## Confirmatory-pilot note

Before any result is used for product decisions:

1. **Redesign S1 criterion 4/5** so the required "constructs the compact list"
   answer is the production `compact_surface_tools`
   (`src/stel/surface_list.rs`) as dispatched by `ServerHandler::list_tools`
   (`src/protocol/mod.rs:1341-1343`), with `compact_probe_tools` at most an
   accepted-with-distinction alternative. Re-freeze the oracle before the
   pilot; do not retroactively regrade shakedown runs.
2. **Commit the Amendment A annotation change** so the pilot's fixture source
   matches the pinned binary's advertised metadata (finding 2).
3. Carry forward the declared confounds verbatim: the surface toggle changes
   both catalog exposure and STEL routing (no causal surface-cost claims), and
   measured startup retains the stat-all + 10% spot verifier as a symmetric
   speed confound.
4. Re-verify token-usage semantics on the first multi-usage-event trace before
   trusting any cross-run token comparison; the shakedown's single-event rule
   has not yet been exercised against a multi-turn usage stream.

VERDICT: APPROVE_CONTINUE
