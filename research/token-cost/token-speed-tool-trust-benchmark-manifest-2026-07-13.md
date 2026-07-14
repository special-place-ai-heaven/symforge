# SymForge Token/Trust Benchmark Manifest

**Date:** 2026-07-13<br>
**Status:** Phase 0 shakedown contract; amended for one reviewed executability prerequisite and one harness-readiness correction<br>
**Design:** [token-speed-tool-trust-reconnaissance-2026-07-13.md](./token-speed-tool-trust-reconnaissance-2026-07-13.md)

## Purpose

Validate the benchmark harness and measure within-cell variance before sizing a multi-host experiment. This shakedown compares one pinned SymForge binary under the full and compact surface toggles. It is descriptive and cannot justify any product change beyond the narrow prerequisite below.

## Amendment A — compact executability prerequisite

The original read-only harness discovered a blocking shipped defect before run 02. Under the same isolated Codex CLI `0.144.2` configuration, full-surface `health_compact` completed, while compact-surface `status` and then compact-surface `symforge` both ended as `user cancelled MCP tool call`. Direct MCP calls succeeded. The corrected harness rejected both false passes.

This is attributable with high confidence to the production compact `tools/list` path: `ServerHandler::list_tools` returns fresh objects from `compact_surface_tools`, whose `surface_tool` helper starts from `Tool::default()` and sets only name, description, and input schema. Full-router annotations therefore do not flow into compact tools. MCP `readOnlyHint` defaults to false and `openWorldHint` defaults to true; current Codex noninteractive approval behavior cancels missing/untrusted MCP annotations when no interactive approver exists. The full annotated read tool is the local control. The pre-fix traces and the original run-01 trace are retained as a separate invalidated baseline; they are not observations in the restarted series.

Before any new measured run, one prerequisite product change is permitted:

1. advertise only compact `symforge`—the read/explore facade—as `read_only_hint=true` and `open_world_hint=false`;
2. do not advertise compact `symforge_edit` or compact `status` as read-only: edit can write, and `status(reset_calibration=true)` clears durable calibration state;
3. change no input schema, description, routing, controller, output, or admission behavior;
4. add a metadata regression test over the production compact `tools/list` objects;
5. build one candidate binary into a disposable target directory on `C:`, copy only the final executable to a pinned artifact path, record its SHA-256, and remove the build target after verification;
6. use that exact binary for both arms and all 20 runs, regenerate the golden state, and restart at run 01.

This amendment restores an executable treatment under the safe host; it does not rescue measured zero-call behavior. After wiring succeeds, zero SymForge calls, wrong routing, tool errors, and native fallback remain valid intent-to-treat outcomes under the assigned arm.

## Amendment B — snapshot materialization and semantic readiness

The restarted run 01 stopped before treatment exposure because its readiness gate required `index.bin` to remain byte-identical after a health-only process. That invariant is false: a fresh worktree has new mtimes, background verification was still `pending`, clean stdio shutdown always serializes, `build_snapshot` re-reads mtimes, and postcard serializes a randomly seeded `HashMap`. Two clean health-only cycles produced three different snapshot hashes while preserving the same logical repository.

Claude Opus approved this harness-only correction as `APPROVE_PLAN` after one `CHANGES_REQUIRED` review:

1. exact golden byte-hash equality is required only at readiness input;
2. one unmeasured MCP process polls `health_compact` until `snapshot_restore`, `verify=completed`, and `mismatches=0`, then cleanly persists a worktree-mtime-matched snapshot;
3. a no-overwrite semantic baseline binds the fixed commit/tree, an order- and mtime-independent SHA-256 manifest over sorted tracked path/mode/file hashes, a deterministic full repo-outline digest, index/parse counts, and the candidate SHA/version;
4. every run must match that semantic baseline before Codex receives the prompt; the per-run materialized snapshot byte hash is recorded only as informational input identity;
5. a second unmeasured probe must load the exact materialized bytes, complete verification with zero mismatches, and preserve the semantic fingerprints;
6. measured startup still performs stat-all plus 10% spot verification. Matched mtimes remove the full-reparse race, but this smaller symmetric product behavior remains a declared speed confound.

No additional SymForge product code is permitted by Amendment B.

## Frozen environment

| Field | Value |
|---|---|
| Repository | `symforge` |
| Fixture commit | `a10ff102546241f1ffd49852ba4d3088c0bb8029` |
| Fixture path | `C:\Users\rakovnik\AppData\Local\Temp\symforge-token-shakedown-a10ff102` |
| Candidate binary | `C:\Users\rakovnik\.codex\tools\symforge-token-trust-8.14.1-a019\symforge.exe`; SHA-256 receipt required |
| Disposable Cargo target | `C:\Users\rakovnik\AppData\Local\Temp\symforge-token-trust-target-8.14.1-a019` |
| Amended golden state | `C:\Users\rakovnik\AppData\Local\Temp\symforge-token-shakedown-golden-a10ff102-a019` |
| Amended raw evidence | `C:\Users\rakovnik\AppData\Local\Temp\symforge-token-shakedown-evidence-a10ff102-a019` |
| Amended compact evidence | `research/token-cost/evidence/token-surface-shakedown-a019` |
| Host | Codex CLI `0.144.2` |
| Model | `gpt-5.6-sol`, high reasoning |
| SymForge | single amended `8.14.1` candidate artifact; path and SHA-256 frozen before restarted run 01 |
| Runs | 20 |
| Repetitions | 5 per task/arm cell |
| Other MCP servers | Disabled in the run-specific configuration |

At most one disposable detached fixture worktree exists at a time. `Prepare` creates the golden state, then removes the fixture. `MaterializeBaseline` creates and probes the semantic receipt, then removes its fixture. Every run recreates the same fixture path at the frozen commit, copies the complete golden `.symforge/` state, materializes and semantically verifies that state outside model accounting, starts fresh Codex and SymForge server processes, and removes the fixture after trace verification. It must have no `target/` directory and must not contain this uncommitted research material. No on-disk or in-memory per-run state survives into the next run.

## Treatment arms

| Arm | Only treatment difference |
|---|---|
| `A-full` | `SYMFORGE_SURFACE=full` |
| `C-compact` | `SYMFORGE_SURFACE=compact` |

Both arms use the same amended SymForge executable, model, host, task prompt, repository, snapshot state, timeouts, native tools, and project instructions. Native reads/searches remain available because fallback and zero-adoption are product outcomes. The prompt does not mandate a call sequence or name a SymForge leaf tool.

The environment variable is the only configured treatment toggle, but it changes two mechanisms: catalog names/schema size and whether reads route through the compact STEL controller. Therefore A-versus-C gaps cannot be attributed to schema exposure alone. This is acceptable for a descriptive variance/adoption shakedown and forbidden as a causal surface-cost claim.

## Index and timing state

The shakedown measures warm, ready query behavior—not cold indexing.

Before the first measured run:

1. create the detached fixture at the frozen commit;
2. build its index once outside model token accounting;
3. call `checkpoint_now(verify_after_write=true)`;
4. restart and poll `health_compact` until snapshot verification is completed with zero mismatches;
5. preserve a golden copy of the complete verified `.symforge/` directory and hash its `index.bin` outside the fixture;
6. materialize and probe the semantic baseline described in Amendment B;
7. stop owned processes and remove the preparation fixture through Git.

Before every run, the harness recreates the exact fixture path at the frozen commit, copies the golden `.symforge/` directory, verifies the golden input hash and repository cleanliness outside `.symforge/`, then completes the Amendment B materialization and semantic-baseline comparison outside the model session. A run is not started against an empty, wrong-project, full-reparse-in-progress, semantically divergent, dirty, or unhealthy state. `index_folder` is never part of the measured task prompt.

After every run, the harness stops and verifies owned processes and checks repository state. If tracked files or unexpected untracked files changed, the run remains scored as assigned, its diff is preserved, and execution halts. After trace verification, the harness removes only the exact verified disposable fixture path through Git with force permitted for `.symforge/` artifacts, then prunes worktree metadata. It never resets or cleans the shared development worktree.

Model-session wall time begins immediately before Codex receives the task and ends when the process exits. Materialization/readiness time is logged separately and cannot be mixed into the surface comparison. The measured SymForge process's stat-all plus 10% spot verifier remains inside real product runtime and is reported as a bounded symmetric speed confound.

## Frozen tasks and exact oracles

### `S1-surface-routing`

Prompt:

> Investigate this repository and answer four questions: (1) which environment variable controls the MCP tool-surface profile, (2) which profile is selected by default, (3) what are the exact three tool names in the compact profile, and (4) which functions read the environment, choose the profile-specific tool list, and construct the compact list. Cite file and line evidence. Do not change files or run builds or tests.

Binary pass checklist—all items are required:

1. States control variable `SYMFORGE_SURFACE`.
2. States default `SurfaceProfile::Full` and that explicit `compact` and `meta` select their corresponding profiles.
3. Lists exactly the three compact names `symforge`, `symforge_edit`, and `status`.
4. Identifies and correctly describes all three functions:
   - `surface_profile_from_env`, `src/protocol/surface_probe.rs:26-41`;
   - `list_tools_for_profile`, `src/protocol/surface_probe.rs:167-177`;
   - `compact_probe_tools`, `src/protocol/surface_probe.rs:253-285`.
5. Gives a correct path and a line inside each frozen symbol range. Equivalent prose is accepted; substitute symbols are not, because the prompt names these three roles exactly.

Extra correct context is allowed. Any missing checklist item, wrong default/name/control variable, or fabricated citation fails the task.

### `S2-ccr-overflow`

Prompt:

> Trace how oversized code-discovery results are limited and later recovered. Report: (1) every eligible tool and its default token budget, (2) when the complete result is returned versus stored, (3) how the continuation is exposed, (4) how continuation identifiers are validated and retrieved, and (5) what usage accounting changes on retrieval. Cite source files and symbols. Do not change files or run builds or tests.

Binary pass checklist—all items are required:

1. Identifies `TOOL_OUTPUT_PROFILES`, `src/protocol/ccr.rs:23-49`, and gives the complete exact table:
   - `search_text`: 8,000;
   - `search_symbols`: 8,000;
   - `find_references`: 8,000;
   - `explore`: 12,000;
   - `get_repo_map`: 16,000.
2. Identifies `apply_ccr_overflow`, `src/protocol/ccr.rs:194-217`, and correctly states all branches:
   - return the full result when it fits `max_tokens * 4` bytes;
   - return the summary without storing when the summary saves nothing;
   - otherwise store the full result and append a retrieval footer.
   Equivalent descriptions of `summary.len() >= full.len()` such as “the summary saves no bytes” are accepted.
3. States the footer names `symforge_retrieve` and includes a handle/hash.
4. Identifies `SymforgeRetrieveInput`, `src/protocol/read_tools.rs:396-399`, and `symforge_retrieve`, `src/protocol/tools.rs:10792-10804`, and correctly states:
   - trim and lowercase;
   - require exactly 12 ASCII hexadecimal characters;
   - return stored content or an explicit unknown/expired message;
5. Identifies `CcrStore::retrieve`, `src/protocol/ccr.rs:159-165`, and states that a hit increments retrieve count and bytes retrieved before returning a clone of stored formatted content.
6. Gives a correct path and a line inside each cited frozen symbol range.

Extra correct context is allowed. Any missing checklist item, wrong budget/tool/branch/hash rule, claim that overflow is discarded, or fabricated citation fails the task.

## Predetermined run order

No ordering choice is made after results are visible.

| Run | Block | Task | Arm |
|---:|---:|---|---|
| 01 | 1 | S1 | A-full |
| 02 | 1 | S2 | C-compact |
| 03 | 1 | S1 | C-compact |
| 04 | 1 | S2 | A-full |
| 05 | 2 | S2 | A-full |
| 06 | 2 | S1 | C-compact |
| 07 | 2 | S2 | C-compact |
| 08 | 2 | S1 | A-full |
| 09 | 3 | S1 | C-compact |
| 10 | 3 | S2 | A-full |
| 11 | 3 | S1 | A-full |
| 12 | 3 | S2 | C-compact |
| 13 | 4 | S2 | C-compact |
| 14 | 4 | S1 | A-full |
| 15 | 4 | S2 | A-full |
| 16 | 4 | S1 | C-compact |
| 17 | 5 | S1 | A-full |
| 18 | 5 | S2 | C-compact |
| 19 | 5 | S1 | C-compact |
| 20 | 5 | S2 | A-full |

## Outcome hierarchy

### Safety gate: exact task success

Each run receives a binary pass/fail from the frozen oracle. The evaluator checks final-state facts, not the route taken. A failed task cannot be redeemed by low token use.

### Primary efficiency metric

For Codex CLI `0.144.2`, `cached_input_tokens` is retained only as an informational subset and is never added again. Run 01 must compare every real `turn.completed.usage` event and determine whether `input_tokens`/`output_tokens` are incremental or cumulative. If incremental, the canonical total is the sum of `input_tokens + output_tokens`; if cumulative, it is the final event's `input_tokens + output_tokens`. Record that receipt and pin the rule before run 02. The total includes model context containing schemas/prompts/tool traffic, reasoning/output, retries, and the final answer as reported by the host.

### Secondary metrics

- model-session wall time;
- zero-SymForge-call rate;
- first substantive tool and whether it matches task intent;
- time/turns to first useful evidence;
- SymForge tool yields;
- invalid arguments, tool errors, and immediate retries;
- native search/read fallback count;
- redundant native reads after equivalent SymForge evidence;
- final-answer citation accuracy.

Call count alone is not a quality metric.

## Required trace record

Each run record contains:

- `run_id`, block, task, arm;
- fixture commit and clean-status check;
- host, model, reasoning level, SymForge version, and surface;
- golden input hash, per-run materialized input hash, snapshot load/verify evidence, and readiness milliseconds;
- Git tree, tracked-source manifest, full repo-outline digest, index/parse counts, and semantic-baseline verdict;
- start/end timestamps and model-session wall milliseconds;
- process exit/timeout state;
- canonical total tokens plus the complete host token breakdown;
- ordered MCP and native-tool events with timestamps and outcome class;
- SymForge call count and first substantive SymForge tool;
- raw final answer and a grader copy with run ID, arm, surface, and tool-route disclosures redacted;
- oracle pass/fail plus explicit failed criteria;
- exclusion reason, if and only if it matches the rules below.

Do not dump process environments or configuration values into traces. Logs must not contain credentials.

## Exclusions

Exclude only failures that occur before treatment exposure:

- host process cannot launch;
- configured model is unavailable;
- authentication is unavailable;
- frozen fixture is missing/corrupt;
- trace capture fails before the prompt is sent;
- SymForge cannot reach the predeclared ready state because of a reproducible infrastructure cause recorded before prompt delivery and before any arm result exists.

The ready-state check must verify the identical golden input hash and identical semantic fingerprints in both arms. Materialized snapshot byte hashes may differ and are not semantic equality keys. If the cause cannot be established from the preflight record without looking at the assigned arm's outcome, the run is scored under its assigned arm rather than excluded.

Do not exclude zero SymForge calls, wrong routing, invalid arguments, tool errors, timeouts after prompt delivery, native fallback, wrong answers, or compact/full-specific failures. Record and score them under the assigned arm.

If any exclusion occurs, fix the infrastructure cause and rerun the same run ID before continuing. Preserve the failed preflight record separately.

## Shakedown analysis

Report each task/arm cell separately:

- passes out of five;
- all five token totals, median, range, and median absolute deviation;
- all five wall times, median, range, and median absolute deviation;
- zero-call, first-tool-correct, fallback, retry, and citation-error counts.

Also report run order so warm-cache or drift patterns are visible. Do not pool tasks, calculate a headline saving, run significance tests, or claim causality from these 20 runs.

The shakedown passes as a harness validation only if:

1. all 20 traces contain complete token and ordered tool-event records;
2. repository state and semantic-readiness evidence are identical across assigned arms; per-run materialized byte hashes are retained only as informational input identities;
3. oracle scoring is deterministic on a blind second pass using only the redacted final answer and frozen checklist;
4. no run is silently excluded;
5. within-cell variance is measurable and can be used for power planning.

If the harness fails any item, repair the harness and repeat the shakedown before designing the larger pilot.

## Checkpoints

1. Claude Opus reviews this manifest before fixture/harness construction.
2. Before run 01, an unmeasured wiring check for each arm must prove that the same Codex CLI configuration mechanism exposes the expected SymForge server and that the server reports the assigned surface. The compact host call must complete `symforge`, not `status`; the direct compact `tools/list` receipt must show the amended read-only/closed-world annotations on `symforge` and must not show `read_only_hint=true` on `status` or `symforge_edit`.
3. Before run 01, `MaterializeBaseline` must produce the no-overwrite semantic receipt and its second-process probe; a separate dry readiness pass must match it.
4. Claude Opus reviews Amendment B and the harness diff before the measured retry. After run 01, Claude Opus verifies real-event token semantics, MCP wiring, process-tree teardown, fixture state, and absence of configuration/environment data in the raw trace.
5. After the 20 runs, Claude Opus reviews trace completeness, oracle decisions, exclusions, and statistical interpretation.
6. Except for Amendment A's one metadata prerequisite, no SymForge source implementation begins until a later powered, host-stratified confirmatory experiment passes the design gate.

## Disk and cleanup contract

- Check free space before creating the fixture and before every build-heavy later phase.
- Put the prerequisite Cargo target on `C:`, never in the low-space repository drive; after the candidate executable is copied and verified, remove only that exact disposable target directory.
- This read-only shakedown must not create a Cargo `target/` in the fixture.
- Keep at most one fixture worktree and one run's live processes at a time.
- Store only the compact manifest, trace records, final answers, and analysis needed to reproduce the result.
- Remove run-specific session directories after their trace has been verified.
- After the post-run Claude review, verify the resolved fixture path is the declared temporary path, confirm no process uses it, then remove the worktree through Git and prune worktree metadata.
- Delete the pinned bridge workaround only after an upstream-compatible release is installed and its MCP smoke test passes.
