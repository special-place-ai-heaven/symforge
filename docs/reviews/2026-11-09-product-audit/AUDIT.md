# SymForge product and engineering audit

Date: 2026-11-09. Audited: `main` at `a7a39c85`, installed binary `symforge 11.11.0`, Windows 11.
Scope: inspection only. No application source, dependency or release configuration was changed.
Companion files: [`findings.json`](findings.json) (register), [`COVERAGE.md`](COVERAGE.md) (what was and was not examined), [`evidence/`](evidence/) (benchmark runs, scripts, surface measurement).

Every statement below is tagged where it matters: **measured** (this audit ran it), **code-confirmed** (the auditor read the cited lines), **delegate-reported** (a subagent read it; the auditor did not re-read every line), or **estimate / hypothesis**.

---

## A. Executive report

**The central question: does SymForge make AI coding agents substantially more efficient?**
It has not been demonstrated. The evidence does not show that SymForge fails; it shows that a whole-task gain has never been established on the current version, and that the gain, where it exists, is concentrated in a few task types and offset in others.

What the evidence says:

1. **The repo's own benchmarks do not show a win.** The only 11.x cohort (five pairs, 2026-10-05) returned `NO_WIN`: three of five pairs cost more with SymForge available. In that cohort no session called a SymForge tool at all, so it measured the cost of having it installed, not of using it. Its totals still differed by 603,609 tokens (about 19%) between arms, which shows how large run-to-run noise is. The one positive result (24.8%) is two pairs on 8.14.1. The README states this honestly (`README.md:193`).
2. **This audit's paired pilot (36 runs, measured) shows a modest aggregate edge that is not distinguishable from noise.** On six read-only tasks in this repo, on a price-weighted basis with the largest known confound removed, the SymForge arm cost 12% less on Sonnet and 2% less on Haiku. It was cheaper on three of six tasks on Sonnet and two of six on Haiku. SymForge was far cheaper for "who calls X" on both models, cheaper for "show me function Y" on Sonnet only, and dearer for literal counting and for reading several regions of one file. No difference in answer correctness was detectable, with a loose grader.
3. **A round trip costs more than the content it fetches.** Each agent turn re-read about 83,000 tokens of fixed context. At an assumed cache-read price of one tenth, one extra round trip costs about as much as 6,600 newly added tokens. A Sonnet run retrieved about 2,800 tokens of content on average, so one extra call outweighs two to three times a task's whole payload. SymForge is designed to reduce bytes; it did not reduce round trips (53 against 55 on Sonnet, 31 against 26 on Haiku).
4. **The mechanism meant to steer agents toward SymForge is mostly failing.** On this machine 55% of all hook events failed open (18,354 of 33,641). The cause is code-confirmed: one unreadable file marks index freshness "degraded" indefinitely, and the hook sidecar refuses to answer unless freshness is "current", while the main tools keep reporting `Status: Ready`.
5. **The product reports savings it has not measured.** `health` printed "Efficiency: 2.5x (60% reduction)" for a session that consisted mostly of this audit's benchmark agents. Across those 36 runs the SymForge arm retrieved 32% less content than the native arm (8% less on Sonnet, 52% less on Haiku). The product's figure compares against an assumed baseline, not an observed one. This is the failure the project's own "reporting invariant" forbids.
6. **The fixed entry cost is high.** The tool list is 69,295 bytes, about 17,300 tokens (measured; token figure is bytes/4). 83% of it is parameter schema, not descriptions. Clients that load every schema up front pay all of it; clients that defer schemas pay per tool on first use.

On engineering: `cargo fmt`, `cargo clippy -D warnings` and the npm tests pass. The full Rust suite passed on its second run; the first stopped on one timing-sensitive test under load, and the same test needed a retry in the passing run (SF-REL-05). Reviewers who read the read-confinement, daemon-authentication and batch-commit code reported them sound; the auditor did not re-read those, and no edit tool was executed. The weaknesses are structural: a 34,000-line handler file, a second implementation of the tool surface for the embed lane, and tests that pin bytes instead of behaviour.

**Overall assessment.** SymForge is a carefully built index with a few clearly valuable tools (`find_references`, `get_symbol`, `search_symbols`) wrapped in a surface, an envelope and a hook system whose costs offset much of what those tools save. The highest-return work is not new capability. It is: fix the hook pipeline, cut fixed and per-call overhead, optimise for fewer round trips, and replace estimated savings with measured ones.

The register holds 42 findings: 7 at P1, 24 at P2, 11 at P3. No P0.

---

## B. Token-efficiency assessment

### B.1 What was measured in this audit

Paired runs, same model, same question, same repo state, same agent type and system prompt. Arm A: `Read`/`Grep`/`Glob` only. Arm B: SymForge MCP tools only. Six tasks, two repetitions on Sonnet 5.5, one on Haiku 5.5. 36 runs, all answers graded against fixed facts. Method and limits are in section D.

**Sonnet, mean of two repetitions per task (measured):**

| Task | Type | Tool calls A / B | Round trips A / B | Retrieved content, chars A / B | Change in content |
|---|---|---|---|---|---|
| T1 | Understand one function | 8.0 / 5.0 | 7.5 / 5.0 | 21,759 / 13,200 | -39% |
| T2 | Find all callers | 8.5 / 2.0 | 6.0 / 2.5 | 16,636 / 3,972 | -76% |
| T3 | Facts from one module | 3.5 / 7.0 | 3.0 / 4.5 | 8,766 / 11,546 | +32% |
| T4 | Count every spelling (rename impact) | 2.0 / 2.5 | 3.0 / 3.5 | 1,281 / 8,139 | +535% |
| T5 | Locate and list one function | 2.0 / 2.0 | 3.0 / 3.0 | 4,247 / 2,085 | -51% |
| T6 | Debug a reporting bug | 6.5 / 10.5 | 5.0 / 8.0 | 14,534 / 22,882 | +57% |
| **All 12 runs per arm** | | **61 / 58** | **55 / 53** | **134,448 / 123,648** | **-8%** |

**Cost views, summed over all runs in the cohort (measured; produced by `evidence/cost_model.py`, output in `evidence/cost-model-output.txt`):**

| View | Sonnet A / B | Change | Haiku A / B | Change |
|---|---|---|---|---|
| Round trips | 55 / 53 | -4% | 26 / 31 | +19% |
| Retrieval tool calls | 61 / 58 | -5% | 29 / 42 | +45% |
| Retrieved content, chars | 134,447 / 123,647 | -8% | 155,761 / 74,376 | -52% |
| Tokens processed, unweighted | 5.06M / 4.68M | -7.5% | 2.50M / 2.80M | +12% |
| Price-weighted cost, shared prefix treated as cached | 766k / 622k | -19% | 485k / 445k | -8% |
| Same, with injected rule files removed from arm A (B.2) | 706k / 622k | -12% | 456k / 445k | -2% |
| Tasks where SymForge was cheaper on the last view | 3 of 6 | | 2 of 6 | |

The price weights are assumptions for comparison (cache read 0.1, new input 1.25, output 5, relative to plain input). The shared prefix is treated as cached for every run because the first run launched in each batch, always an arm-A run, paid the cache write for all the others; the raw fresh-token split (701,654 against 531,058 on Sonnet) carries that launch-order bias and should not be read as a result.

Correctness: graded by keyword facts. Scores were identical across arms in every cell (1.0 everywhere except T6, where both arms scored 0.8 on Sonnet and 0.6 on Haiku). The grader is a loose keyword check and 34 of 36 scores sit at the ceiling, so this means no difference was detectable, not that answers were verified in depth. The auditor also read 16 of the 36 answers in full and judged them substantively right; that reading is not recorded in the evidence files.

**Reading:** SymForge wins clearly when the question maps to one index query (T2: `find_references` answered in one to three calls what took seven to ten greps and reads). It loses when the agent needs text around several matches in one file (T3, T6), where one `Read` of a window beats three or four symbol and search calls. It loses on literal counting (T4), where `Grep` count mode returns 160 characters. With two repetitions per cell, per-task figures show direction only: two runs of the same cell differed by more than 2x in tokens processed (B-T1: 622k and 259k).

### B.2 A confound that flatters SymForge in this environment

Raw context growth favoured SymForge by 53% on Sonnet (76,810 against 36,349 tokens, summed task means). Much of that is not SymForge. The first `Read` of a `.rs` file makes the client inject five path-scoped rule files: a median of 4,165 tokens (characters/4) in 15 of the 18 arm-A runs and in none of the arm-B runs (measured from transcripts). Those tokens are then re-read on every later turn.

Total non-payload context (growth minus retrieved characters/4 minus output) had a median of 10,364 tokens in the 15 arm-A runs that used `Read`, 920 in the 3 that used only `Grep`, and 1,949 in arm B. Rule files explain about half of that gap. The rest was not broken down; part of it is that characters/4 undercounts tokens for raw code.

Removing the rule files moves the Sonnet price-weighted difference from -19% to -12% and the Haiku difference from -8% to -2%. The adjustment subtracts all of a run's rule text from every turn after its first `Read`, which slightly over-corrects in the native arm's favour. The unweighted -7.5% on Sonnet is small enough that this confound alone could account for it.

This cuts both ways. It is a real saving in this setup. It also means an agent that reads and edits only through SymForge never has the project's path-scoped rules loaded. Any future benchmark must control for it (section D).

### B.3 Fixed overhead (measured)

| Item | Bytes | Approx. tokens (bytes/4) |
|---|---|---|
| `tools/list`, full surface, 40 tools | 69,295 | 17,300 |
| of which input schemas | 57,459 | 14,400 |
| of which tool descriptions | 6,464 | 1,600 |
| parameter descriptions (inside schemas) | 30,610 | 7,650 |
| `project` / `projects` parameter entries (34 entries) | 6,670 | 1,670 |
| nullable `["string","null"]` unions (244) | 5,988 | 1,500 |
| `$defs` blocks | 7,926 | 1,980 |
| `tools/list`, compact surface, 3 tools | 4,630 | 1,160 |
| resources + templates + prompts | 4,304 | 1,080 |
| `initialize` instructions string | 0 | 0 |

The largest single tool is `search_text` at 4,622 bytes. Clients that defer tool schemas pay this per tool on first use instead of up front. Clients that do not defer pay 17,300 tokens in every session, used or not. Whether that explains the 2026-10-05 cohort was not tested; which clients load schemas eagerly was not surveyed. Raw output: `evidence/tool-surface-measurement.txt`.

### B.4 Per-call overhead (measured in dogfooding)

- **`get_repo_map` appends a "Repository knowledge" block to every response.** It is about 9,300 characters of 64-hex digests and unit ids, roughly 2,500 tokens. The useful part of a `src/` depth-1 tree was about 350 tokens. Three orientation calls cost about 8,000 tokens of repeated appendix. Code: `src/protocol/tools.rs:2079-2085` appends unconditionally.
- **When over budget it keeps the appendix and drops the answer.** `get_repo_map(detail=tree, path=src, depth=2, max_tokens=3000)` returned the full appendix and the line "Code topology omitted from the bounded summary". Code: `src/protocol/tools.rs:2091-2096`.
- **The default compact map truncated before reaching `src/`.** It listed `.claude`, `.specify`, `docs`, `specs` and stopped ("61 additional output line(s) not shown").
- **`estimate=true` is wrong.** It returned about 16,000 tokens for that `src/` tree because the formula is `file_count * 10` and ignores `path` (`tools.rs:1965`). It returns a constant 500 for the compact map, which measured about 3,500 with the appendix.
- **Search envelopes are large relative to small answers.** A one-hit `search_text` returned six header lines and a trailing admission note around a three-line payload.
- **A hook injects index health JSON after non-identifier greps.** Every such `Grep` in this session was followed by `{"file_count":…,"index_state":"Ready","uptime_secs":…}`. Code: `src/cli/hook.rs:1024-1025` routes these to `/health` as a "no-op", but the body is injected into context and the event is logged as routed.

### B.5 The product's own savings number

`health` for this session reported: "Tokens served: 172,285. Competent-manual equivalent: 437,064. Efficiency: 2.5x (60% reduction)", with `search_symbols` at "13.6x". That session consisted mostly of this audit's benchmark agents, plus the auditor's and reviewers' own calls. For the benchmark agents the measured difference in retrieved content against a real native arm was 32% over all 36 runs (8% on Sonnet, 52% on Haiku, the latter driven by one native run that read 95,000 characters). The two figures are not the same metric, which is the point: one is an assumption, the other an observation.

The number is an estimate against an assumed alternative: a 50-line by 80-character read window per result (delegate-reported: `src/index_lifecycle/guidance/file_read.rs:702-727`, `read_context.rs:1029-1044`, `src/protocol/format.rs:656-692`). The real alternative to `search_symbols` is a `Grep`, which returns lines, not windows. The label "vs competent-manual windowed read" is present, but the figure is presented as session efficiency.

### B.6 Answer to the mission question

- **Where it delivers (measured):** caller and reference tracing on both models; fetching a named symbol and locating a definition on Sonnet (39 to 76% less content and up to 4x fewer calls). On Haiku the function-understanding task cost more with SymForge.
- **Where it does not (measured):** literal counting and completeness sweeps, reading several nearby regions of one file, short debugging loops. 32 to 535% more content and up to 2x the calls.
- **Net on a mixed set (measured, small sample):** 2 to 12% cheaper on a price-weighted, confound-adjusted basis; no reduction in round trips; no detectable difference in correctness. Not distinguishable from noise at this sample size.
- **Not measured by anyone:** multi-file edits and refactors, where the owner expects the largest gain. The only refactor data point in the repo is a `batch_rename` recipe that failed (8.14.0).

---

## C. Dogfooding assessment

SymForge was used to investigate SymForge throughout: orientation, search, symbol fetch, and by 18 benchmark agents and three reviewers.

**Worked well**
- `search_text` with `group_by=symbol` returned the enclosing function and line range in one compact answer.
- `get_symbol` returned a 170-line function exactly, with no wrapper noise. Batch mode (`targets[]`) returned symbols from three files in one call.
- `find_references` was called once in every SymForge-arm run of T2; one run needed no other call, the others made one or two confirming searches.
- Honest negatives: a missing symbol returned close matches and a next step instead of an empty result.
- The index was current and fast; no stale answer was observed in about 250 calls.

**Got in the way**
- Orientation was the worst experience. Three `get_repo_map` calls were needed to see the `src/` tree, and they cost roughly 10,000 tokens, mostly appendix (SF-TE-02).
- `get_file_context` with `include_tests=false` and a 1,800-token budget still expanded the `mod tests` block of `src/cli/hook.rs`, spending about 40% of the budget on test names (observed once).
- `get_symbol` output carries no line numbers inside the body. One SymForge-arm run that relied on it cited `tools.rs:2024` for a statement at `tools.rs:2062` (run B-T1-r1). A SymForge-arm run that used `get_file_content`, which is numbered, cited 2062 correctly, as did the native arm. One observed instance, not a rate.
- Trust signals contradict each other. The same `health` output says `Status: Ready`, `freshness=degraded[ObservationFailed,ReconciliationPending]` and `coverage=degraded`. Search results say "Completeness: full for indexed scope".
- `status` prints roadmap labels (`l4_ledger: in_memory`, `deferred: b_results`, `superseded: multi_step_planner`) that are hard-coded strings (delegate-reported: `src/stel/status.rs:38,371,520-522`). An agent cannot use them and may read them as live state.
- Agents did not use the efficient forms. On T6 the SymForge arm called `get_symbol` two to eight times singly instead of once with `targets[]`.
- The server returns no `instructions` at `initialize` (measured: 0 bytes). Guidance on when to use which tool exists only in a client file the user must install.

**Cases where SymForge made its own investigation harder or dearer than plain tools**
- Orientation (`get_repo_map`), as above.
- The rename-impact count (T4).
- The hook failure investigation: the adoption log has no failure reason, so the cause had to be found by reading code (SF-ADOPT-02).

---

## D. Benchmark gap analysis

### D.1 What exists (delegate-reported inventory, key items re-read)

| Measurement | Version | Design | Result | Why it cannot answer the question |
|---|---|---|---|---|
| End-to-end feature task, 2026-07-13 | 8.14.1 | 2 valid pairs, one task | 24.8% fewer tokens | n=2; a third arm excluded; agents mixed native commands |
| Availability cohort, 2026-10-05 | 11.x (version not recorded) | 5 pairs | `NO_WIN`, median -40,011 | No SymForge call in any session; measures presence, not use |
| SFBENCH recipes, 2026-07-12 | 8.14.0 | Deterministic tool recipes, 10 repos, tiktoken | SymForge median 4,032 vs 2,262 tokens; only `get_symbol` saved | Tool-level, no agent; old version |
| A019 shakedown, 2026-07-13 | 8.14.1 | 20 runs, full vs compact surface | No claim | Oracle pass 0/20; oracles judged flawed |

None of the harnesses runs in CI. No task category has repeated measurements on the current version. Debugging and review were never measured before this audit. Schema-size figures in the repo disagree (17,894 vs 21,402 tokens) and are unreconciled; this audit measured 69,295 bytes.

### D.2 What this audit added, and its limits

36 paired runs (section B.1). Raw per-run data: `evidence/paired-benchmark-runs.json`. Scripts: `evidence/parse.py`, `evidence/grade.py`.

Limits, stated plainly:
- One repository (SymForge itself, Rust, 1,604 indexed files). No other language or size.
- Read-only tasks only. No edit, refactor or multi-file change, because the audit was inspection-only.
- Two repetitions per cell on Sonnet, one on Haiku. Enough to see direction per task, not to estimate variance.
- Tasks were written by the auditor after reading the code, which biases toward questions with crisp answers.
- Tool restriction was by instruction. No native-arm run called a SymForge tool and no SymForge-arm run called `Read`, `Grep`, `Glob` or a shell (checked from transcripts). Three Haiku runs also sent a harness message, which is excluded from the retrieval call counts.
- Grading was a keyword check with a ceiling effect (B.1). It can show a gross failure, not a subtle one.
- The price-weighted view rests on assumed price ratios, and the confound adjustment removes only the rule files that could be identified.
- Both arms had every tool schema loaded, so fixed surface cost is equal across arms and excluded from the comparison. It is reported separately in B.3.
- Client hooks were active in both arms. The conventional arm received hook-injected context and rule files (B.2).
- Runs overlapped with builds and each other, so wall-clock times are indicative only.
- A stop-hook in this environment made some agents add a turn after answering; metrics were cut at the answer message.

### D.3 Recommended methodology

1. **Unit of analysis: a paired task run.** Same model, prompt, repo commit, verification command. Arms differ only in which tools are callable, enforced by the client's allow-list, not by instruction.
2. **Three arms, not two.** (a) native tools only; (b) SymForge only; (c) both available with no steering. Arm (c) is the product as shipped and is the one the 2026-10-05 cohort showed agents ignore.
3. **Primary metrics, in this order:** task success by an executable oracle; model round trips; total tokens processed across all calls; retrieved-content tokens; wall time. Report cache-read and fresh tokens separately.
4. **Report fixed and marginal cost separately.** Tool-list bytes per client; then per-task marginal tokens.
5. **Control the environment.** Record and hold constant: hooks on/off, path-scoped rules, deferred-schema behaviour. Run each cell with hooks off and on.
6. **Task suite of at least 24 tasks across eight categories** (discovery, tracing, unfamiliar module, debugging, single-file edit, cross-module edit, rename/refactor, review), each with an oracle: a test that must pass, or a fact list as in `grade.py`.
7. **At least three repos** by size and language (small Python, mid TypeScript, large Rust or C++), plus SymForge itself.
8. **Five repetitions per cell minimum.** Report median, interquartile range, and the count of pairs won, with the predeclared rule the 2026-10-05 cohort already used (positive total and positive median).
9. **Run it on a schedule** (weekly, and on release candidates), and publish the losing categories as prominently as the winning ones.
10. **Gate claims on it.** No percentage in the README, wiki or `health` output without a run id from this harness.

The existing `research/full-surface-benchmark/` runner and the transcript parser in `evidence/parse.py` are a workable starting point; the missing parts are executable oracles, allow-list enforcement and scheduling.

---

## E. Engineering audit

Checks run in this audit (measured): `cargo fmt --all -- --check` passed. `cargo clippy --workspace --all-targets -- -D warnings` passed (4m 46s). `npm --prefix npm run test` passed, 0 failures. `cargo nextest run --workspace`: see SF-REL-05 and `COVERAGE.md`.

Full detail for every finding, including evidence and recommended action, is in `findings.json`. The findings that matter most:

### Adoption and reliability

- **SF-ADOPT-01 (P1): one unreadable file disables hook enrichment while the tools report Ready.** `manifest_requires_degraded_coverage` returns true for any `Unreadable` entry (`src/live_index/store.rs:5784-5793`). The sidecar serves only when freshness is `Current` (`src/sidecar/handlers.rs:350-352`) and otherwise returns 503, which the hook records as a sidecar error (`src/cli/hook.rs:552-617`). Live state during the audit: `unreadable=2`, `freshness=degraded`, and 3,548 of 4,211 hook events in the current daemon session failed open, 3,493 of them as sidecar errors. Post-edit impact routed 0 of 1,965. Code-confirmed; that this explains the historical 55% is a strong hypothesis, not proven, because the reason is not logged.
- **SF-ADOPT-02 (P2): the failure reason is thrown away.** `emit_live_refusal_fail_open` records the outcome without its reason (`src/cli/hook.rs:667`).
- **SF-ADOPT-03 (P2): "Hook Adoption (current session)" is not the current session.** Session ids restart at 1 in each daemon process (delegate-reported: `src/daemon.rs:1172`, `:1522`); the log is machine-wide and append-only; when no session resolves the filter is skipped (`src/cli/hook.rs:1731-1735`). Observed: the label showed 6,861 events under `session-1`, then 4,211 under `session-2` an hour later.
- **SF-REL-01 (P2): a degraded index rescans continuously.** The reconcile loop retries up to five times per sweep while coverage is degraded (`src/watcher/mod.rs:626-701`). Observed: `index_generation` moved from 104 to 1,710 in about 65 minutes with a clean working tree.
- **SF-REL-05 (P2): a wall-clock assertion can hide most of the suite.** `large_rust_file_indexing_is_linear_and_unchanged` (`src/parsing/mod.rs:2297`) failed twice under load in the first run; with fail-fast, 2,661 of 4,790 tests did not run.

### Security

Security findings SF-SEC-01 to SF-SEC-08 are listed here by generic title and priority only. Evidence, attack scenarios and fixes are held privately and will be published after the fixes ship.

- **SF-SEC-01 (P1): input validation gap in root binding on Windows.** Details withheld until a fix ships.
- **SF-SEC-02 (P2): a safety override is controllable by the caller it restricts.** Details withheld until a fix ships.
- **SF-SEC-03 (P2): a loopback service lacks request-origin checks.** Details withheld until a fix ships.
- **SF-SEC-04 (P2): a refusal can be cleared by the caller it refused.** Details withheld until a fix ships.
- **SF-SEC-09 (P2): no automated dependency advisory gate.** No `deny.toml` at the repo root and no `cargo-deny`/`cargo audit` step in `.github/` (measured by search). Advisories are checked by hand: commit #849 records a manual `cargo deny check advisories` run.

Found sound (delegate-reported, not re-read): read confinement against `..`, absolute paths and symlink escape; secret policy applied before reads; daemon bearer check with constant-time compare and owner-only token file; `serve` Host allow-list and Origin gate; batch edit commit with ordered locks, preflight and verified rollback.

### Architecture and maintainability (delegate-reported measurements)

- **SF-ARC-01 (P2): the embed lane re-implements the tool surface.** About 21,000 lines in `src/index_lifecycle/embed_*.rs` mirror handlers in `src/protocol/tools.rs`, held together by parity tests. Medium confidence; not diffed line by line.
- **SF-ARC-03 (P2): very large files.** `src/protocol/tools.rs` 34,120 lines; `src/daemon.rs` 19,818; 94 of 310 source files exceed 800 lines and hold 80% of the code. Functions of 400 to 550 lines exist.
- **SF-ARC-02 (P2): cyclic module dependencies**, and `src/index_lifecycle/mod.rs:24` states three of its modules have no production callers.
- **SF-ARC-04 (P2): byte-pinning tests.** A whole-`src/` seal and workflow fingerprints fail on any edit, not on behaviour change.
- **SF-ARC-08 (P2): scope has outgrown the mission.** 41 tools, a knowledge index, curation, secret remediation, an admin GUI and an embed lane surround three or four tools that carry the measured value.

---

## F. Strategic recommendations

Ranked by expected return for the mission against cost.

| # | Recommendation | Problem | Proposal | Expected benefit | Cost | Risk | Success measure |
|---|---|---|---|---|---|---|---|
| R1 | Optimise for round trips | Fixed context (about 83k tokens re-read per turn) makes each extra call cost more than a typical task's retrieved content | Make "answer in one call" the design rule: search results that include the enclosing symbol body on request; default batch forms; a combined "definition + callers" lookup | Fewer turns on T3/T6-type tasks, where SymForge now loses | Medium | Larger single responses if defaults are wrong | Round trips per task in the harness, by category |
| R2 | Repair the hook pipeline | 55% of hook events fail open; the cause is a strict freshness fence | Serve hook answers with a staleness note when freshness is degraded; settle persistently unreadable files; log the reason; make the `/health` fallback emit nothing | Restores the main adoption path | Small | Serving slightly stale hints | Routed share above 95%; reasons present in the log |
| R3 | Halve the fixed surface | 17,300 tokens before any call | Ship a lean default (8 to 10 core tools); move `project`/`projects` text to one shared note; drop nullable unions and numeric format noise; shorten parameter text | 8,000 to 11,000 fewer tokens per session (estimate from B.3) | Small to medium | Clients relying on rarely used tools must opt in | `tools/list` bytes under 30,000; no drop in task success |
| R4 | Make orientation cheap | `get_repo_map` spends about 2,500 tokens per call on digests | Remove the knowledge appendix from the default; budget the code tree first; make `estimate` path-aware | About 2,500 tokens per orientation call | Small | None identified | Bytes per `get_repo_map` call; estimate within 20% of actual |
| R5 | Report only what was observed | "60% reduction" printed from an assumed baseline; 32% measured against a real native arm | Rename to "estimated against a windowed-read baseline", or replace with observed counters (calls, bytes served); remove multipliers | Restores trust in the product's numbers | Small | Loses a marketing-friendly figure | No savings figure without a harness run id |
| R6 | Stand up the benchmark as a standing gate | No reliable evidence for the mission | Section D.3 | Lets every later change be judged | Medium | Model cost per run | Weekly run with published medians and losses |
| R7 | Fill the cheap-mode gaps | Counting and line citation are worse than `Grep` | Count mode and an `include_tests` reminder in `search_text`; line-numbered `get_symbol` | Removes the T4 loss and citation errors | Small | None identified | T4 content at or below the native arm; citation accuracy |
| R8 | Tell agents how to use it | No server instructions; agents ignore or misuse tools | A short `instructions` string at `initialize`: three rules and when not to use SymForge | Higher correct use in the unsteered arm | Small | Adds a few hundred tokens | Share of sessions with a SymForge call in arm (c) |
| R9 | Close the security gaps | SF-SEC-01 to 04, 09 | Fixes for SF-SEC-01 to 04 are withheld until they ship; add an advisory gate | Closes the P1 and P2 security findings | Small to medium | Friction for legitimate overrides | Tests for each refusal |
| R10 | Reduce structural drag | Two tool-surface implementations; 34k-line file; byte pins | Thin server handlers over the shared guidance layer; split `tools.rs` by tool family; replace the source seal with a targeted check | Faster, safer change | Large | Regression during the move | Parity-layer lines; files over 800 lines; pin-only CI failures |

What not to do: add more tools or more envelope fields. Each one raises the fixed cost that already offsets much of the benefit.

---

## G. Prioritised roadmap

**Phase 0, correctness of what the product tells agents (days).**
SF-TE-02, SF-TE-03 (repo map appendix and estimate); SF-TE-09 (hook health injection); SF-ADOPT-02, SF-ADOPT-03 (log the reason; fix the session label); SF-TE-04 (relabel savings); SF-SEC-01; SF-DOG-02 (line numbers in `get_symbol`).
Exit: orientation under 600 tokens; no unmeasured figure labelled as a result; SF-SEC-01 closed with a test.

**Phase 1, adoption (one to two weeks).**
SF-ADOPT-01, SF-REL-01 (freshness fence and rescan loop); SF-TE-05 (lean default surface); SF-ADOPT-04 (server instructions).
Exit: hook routed share above 95% on this machine for a week; `tools/list` under 30,000 bytes.

**Phase 2, proof (two to four weeks).**
Benchmark harness per D.3 with three arms and executable oracles; first full run on four repos; SF-TE-07, SF-TE-08 (count mode, one-call forms) judged by that harness.
Exit: a published table with medians and losing categories; a go/no-go on each proposed tool change.

**Phase 3, hardening (in parallel from Phase 1).**
SF-SEC-02, 03, 04, 09; SF-REL-05 (move the timing test out of the default suite).

**Phase 4, structure (after Phase 2 shows what is worth keeping).**
SF-ARC-01, 02, 03, 04, 08. Use the benchmark to decide which of the 41 tools stay in the default surface.

---

## Delegation record

| Work | Model tier | Outcome |
|---|---|---|
| Benchmark evidence inventory | Haiku | Reliable; key claims re-read by the auditor |
| Tool-surface measurement | Sonnet | Reliable; raw capture kept and re-analysed |
| Security review | Sonnet | Reliable; four main findings re-read by the auditor |
| Reliability review | Sonnet | Most valuable; root-caused the hook failures; chain re-read by the auditor |
| Architecture review | Sonnet | Measurements accepted; one finding kept at medium confidence |
| 36 paired benchmark runs | 24 Sonnet, 12 Haiku | All kept to their arm's tools |
| Independent review of this report | Opus | Recomputed every figure and checked 20 citations (all held); its corrections to the cost accounting and headline wording were applied |

The strongest model was used for: design of the benchmark and its grading, finding the rules-injection confound, verifying each high-impact claim in code, and synthesis. No task needed escalation to a stronger worker. Delegation problems: three of five reviewers and all benchmark agents returned only a post-answer note or nothing to the orchestrator because of a stop-hook in this environment; full reports and metrics were recovered from transcripts.
