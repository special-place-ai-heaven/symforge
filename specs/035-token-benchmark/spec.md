# Feature Specification: Standing Three-Arm Token and Quality Benchmark

**Feature Branch**: not yet created (files are untracked in the working tree; the chief commits them on their own branch)

**Created**: 2026-11-10

**Status**: Harness built and dry-run verified; no paid run has happened. Every number in this folder that describes a benchmark result is either from the 2026-11-09 audit (cited) or synthetic (labelled).

**Input**: Review ledger `FCR-20261109T215331Z-a7a39c8`, findings SF-BEN-01 (no standing benchmark), SF-BEN-02 (client rule injection confounds comparisons; SymForge reads bypass path-scoped rules) and SF-TE-01 (no whole-task token reduction demonstrated on the current version), plus the owner decisions of 2026-11-09 in that ledger. Design source: `docs/reviews/2026-11-09-product-audit/AUDIT.md` section D.3 on branch `docs/product-audit-redacted-2026-11-09`.

## Owner decisions (binding) and where this spec departs from AUDIT.md D.3

The owner decision overrides D.3 where they differ:

| Topic | AUDIT.md D.3 | Owner decision, used here |
|---|---|---|
| Arm (b) | SymForge tools only | SymForge available **with** steering instructions (native tools stay callable) |
| Arm (c) | Both available, no steering | SymForge available **without** steering (same tool set as b) |
| Repositories | at least three plus SymForge itself | four repositories, pinned public commits, permissively licensed |
| Hooks and rules | record, run each cell off and on | every cell run and recorded with hooks and path-scoped rules both off and on |
| Claim rule | positive total and positive median | same, reported with the losing categories |

SymForge itself is not one of the four repositories: its licence is PolyForm Noncommercial 1.0.0, which is not permissive. See Out of scope.

## User Scenarios and Testing

### User Story 1: A weekly run answers "does SymForge save tokens on whole tasks" (Priority: P1)

A maintainer wants to know, for the current SymForge release, whether an agent with SymForge available finishes realistic tasks with fewer tokens than an agent with native tools only, without getting worse answers. A scheduled run executes the full matrix, grades every run with a script, and publishes a report that states the verdict under a rule written down before the run.

**Independent test**: `python bench.py dry-run --out <dir>` produces `report.md`, `report.json` and `cells.jsonl` with every section filled and the claim marked not admissible (synthetic). A live run is the same pipeline with the real client.

**Acceptance scenarios**:

1. **Given** a completed run, **when** the report is produced, **then** it gives per-category medians, pairs won, the losing categories, the run id, and the rule outcome for the headline pair in each hook/rule mode.
2. **Given** a run where SymForge wins on total tokens but loses on the median task, **when** the rule is applied, **then** the outcome is REJECT.
3. **Given** a run whose records are synthetic, **when** the report is produced, **then** the claim reads "NOT ADMISSIBLE (synthetic data)" whatever the rule outcome.

### User Story 2: Quality is decided by scripts, never by the model (Priority: P1)

Every task carries an oracle that a script evaluates against the run's working tree and answer. A run passes only when every check passes. Where the answer can be computed from the repository (a definition's line, the set of caller files, the methods of a class, the exact rename), the oracle computes it from the cell's base commit at grading time instead of using a typed expectation.

**Acceptance scenarios**:

1. **Given** an answer that claims success but cites the wrong line, **when** graded, **then** it fails.
2. **Given** an oracle whose derivation finds nothing (the anchor moved), **when** graded, **then** the check reports an error, the cell is `oracle_error`, and it is excluded from token math instead of counting as a failure.
3. **Given** an edit task, **when** graded, **then** the hidden test is copied in only after the agent finished and after tree checks ran.

### User Story 3: Arms differ only in what is callable (Priority: P1)

The native arm has no MCP server configured. Arms b and c have the same SymForge server; only b gets the steering text. After the run the harness reads every tool call from the transcript; any call outside the arm's allow-list makes the cell `invalid_allowlist`, and it never enters savings.

### User Story 4: Hooks and path-scoped rules are a controlled variable (Priority: P1, SF-BEN-02)

Every cell runs in mode `off` (no client hooks, no path-scoped rules) and in mode `on` (path-scoped rule fixture present; SymForge client hooks installed for arms b and c). Each cell has its own client home, so the operator's own hooks and settings never leak in. Every record states what was present (rule file paths, sizes and hashes; installed hook events and the settings hash) and how often the rule sentinel appeared in the transcript. The report counts cells whose recorded configuration does not match their mode.

## Requirements

### Task schema

Tasks are JSON objects in `research/token-benchmark/tasks/<repo>.json`:

| Field | Meaning |
|---|---|
| `id` | unique, `<repo>-<category-ish>-<slug>` |
| `repo` | id in `repos.json` |
| `category` | one of the eight categories below |
| `status` | `anchors_verified` (anchors checked against the pinned raw files on 2026-11-10) or `draft` (hidden test not yet compiled against the pin) |
| `prompt` | task text given to the agent; the runner appends a fixed instruction to end with an `ANSWER:` section |
| `seeds` | optional mutations `{id, stage: base or review, path, find, replace, benign?}`; `find` must match exactly once or setup fails; `base` seeds are committed into the base (debugging), `review` seeds stay uncommitted (review) |
| `oracle` | list of checks, all of which must pass |

### Oracle contract

| Check | Passes when | Truth comes from |
|---|---|---|
| `locate` | the answer cites `path:line` of a regex match within `tolerance` lines | regex over the base tree |
| `files_mentioned` | the answer names every file that has a matching line (definition line excluded) | regex over the base tree |
| `names` | the answer names every item matched inside a delimited block | regexes over one base file |
| `seed_located` | the answer cites the seeded file and a line within `tolerance` of the seed | the seed applied in setup |
| `answer_regex` | every fact has at least one matching alternative | typed facts (never the only check of a task) |
| `diff_scope` | something changed, and every changed or new file matches an allowed glob (test caches ignored) | `git diff` against base plus untracked files |
| `exact_rename` | the tree equals the base tree with `\bold\b` replaced by `new`, and nothing else changed | the base tree |
| `command` | the command exits 0 after hidden files are copied in | the repository's toolchain |

An oracle that cannot be evaluated (empty derivation, missing toolchain, git failure) reports `error`, which is an infrastructure fault. `calibrate` must show, per task, that the oracle evaluates without error and rejects the untouched base tree with an empty answer; a live `matrix` refuses uncalibrated tasks.

### The 24 tasks

Eight categories, three tasks each, six tasks per repository:

| Category | flask | cobra | zod | ripgrep |
|---|---|---|---|---|
| discovery | `get_root_path` (locate) | | `prettifyError` (locate) | `struct MaxCount` (locate) |
| tracing | | callers of `commandNameMatches` | callers of `prefixIssues` | users of `GlobSetBuilder::new()` |
| unfamiliar-module | public methods of `Config` (names) | | exported functions of `v4/core/errors.ts` (names) | `pub fn`s of `impl GlobSet` (names) |
| debugging | seeded `get_namespace` case inversion; fix; pytest | seeded `MinimumNArgs` off-by-one; fix; hidden boundary test + `go test .` | seeded `floatSafeRemainder` comparison; diagnose only (seed_located + `< tolerance`) | |
| single-file-edit | `Config.get_bool` (hidden pytest) | `EvenArgs` validator (hidden go test) | | `GlobSet::first_match` (hidden cargo test) |
| cross-module-edit | `config_loaded` signal in `signals.py` sent by `config.py` (hidden pytest) | `Command.HiddenAliases` across lookup, help and completion (hidden go test, `go test ./...`) | | `Glob::is_literal` in `glob.rs` plus `GlobSet::literal_count` in `lib.rs` (hidden cargo test) |
| rename-refactor | `_endpoint_from_view_func` to `_view_func_endpoint` (exact_rename) | `legacyArgs` to `legacyArgsValidation` (exact_rename) | `floatSafeRemainder` to `safeFloatRemainder` (exact_rename) | |
| review | | seeded `RangeArgs` upper-bound defect plus one benign comment (seed_located) | seeded `toDotPath` separator defect plus one benign comment (seed_located) | seeded `matches_all_candidate` negation plus one benign doc edit (seed_located) |

Status: 18 tasks are `anchors_verified`; the 6 single-file and cross-module edit tasks are `draft` because their hidden tests have not been compiled against the pins. No task has been calibrated yet (that needs the clones and toolchains).

### Repositories

Pinned with `git ls-remote <url> HEAD` on 2026-11-10 (`python bench.py verify-pins` repeats the check):

| Repo | Commit | Licence | Language | Size | Why |
|---|---|---|---|---|---|
| `pallets/flask` | `49943b5a94413a530761350f04e896f76e8d620e` | BSD-3-Clause | Python | small | The audit's "small Python" slot; pure-Python test suite runs with `pytest` and a handful of dependencies; readable module boundaries for discovery and unfamiliar-module tasks. |
| `spf13/cobra` | `adbc8813901bba65827259daa8e22ff94ec1f30e` | Apache-2.0 | Go | small-mid | Adds a fourth language; `go test` needs no services; the same commit is already pinned in `research/full-surface-benchmark/corpus.lock.json`, so results stay comparable with SFBENCH. |
| `colinhacks/zod` | `19c40d2072288b4cafec851e9d056a72a88e5973` | MIT | TypeScript | mid | The audit's "mid TypeScript" slot; a monorepo with many small exported functions that stresses search and tracing. Its own toolchain (`nub`) is unusual, so its tasks use tree and answer oracles only. |
| `BurntSushi/ripgrep` | `178cc027e283fd965cb72d0a7395b219eada170c` | MIT OR Unlicense | Rust | large | The audit's "large Rust" slot: a multi-crate workspace close to SymForge's own language and shape; `cargo test -p globset` builds one small crate, keeping edit oracles cheap. |

### Arms and allow-lists

| Arm | MCP config | Steering | Allowed tools (checked post hoc from the transcript) |
|---|---|---|---|
| a | `{"mcpServers": {}}` | no | `Bash, Read, Grep, Glob, Edit, Write` plus client meta `ToolSearch, TodoWrite` |
| b | SymForge stdio server | `steering.md` appended to the system prompt (hash recorded) | arm a list plus `mcp__symforge__*` |
| c | SymForge stdio server | no | same as b |

The client is launched with `--strict-mcp-config`, `--tools`, `--allowedTools` and `--disallowedTools` (WebFetch, WebSearch, Task, Agent, NotebookEdit) from `config.json`. The post-hoc check is the enforcement that counts: it does not depend on the client honouring its flags.

### Hook and rule matrix

| Mode | Path-scoped rules | Client hooks | Applies to |
|---|---|---|---|
| off | none (repo `CLAUDE.md` and `.claude/` removed in every cell) | none (empty per-cell client home) | all arms |
| on | `overlays/rules/bench-source-rules.md` copied to `.claude/rules/` | `symforge init --client claude` into the per-cell home, arms b and c only | all arms; arm a has rules but no SymForge hooks |

The runner accepts the two modes in `config.json`; more combinations (hooks on, rules off) can be added there, but the required weekly run is the two modes above.

### Metrics, per cell

Round trips (distinct assistant messages), tool calls by name, input, cache-write, cache-read and output tokens summed once per message id, total tokens processed (the sum of those four), tool-result size (characters and characters/4, labelled an estimate), wall time (client `duration_ms`), client-reported cost when present, oracle result per check, allow-list violations, rule-sentinel hits, hook events, and the cell's hook/rule configuration.

### Decision rule (predeclared)

Evaluated per mode for the headline pair `a` vs `c` (the product as shipped); `a` vs `b` is reported beside it as the steered ceiling and does not decide the claim.

1. A pair is one (repo, task). An arm's value for a pair is the median `total_tokens` over its oracle-passing, valid repetitions. A pair where either arm has no passing repetition is excluded from token math and listed.
2. `saving_t = (native - other) / native`.
3. ACCEPT in a mode iff all hold:
   - total saving `1 - sum(other) / sum(native)` > 0 (owner rule);
   - median of `saving_t` > 0 (owner rule);
   - the other arm's oracle pass count is at least native's (guardrail added in this spec, so a cheap wrong answer cannot win);
   - at least 95% of the expected cells for the pair are valid (guardrail added in this spec).
4. The whole-task claim is accepted only when the headline pair is ACCEPT in both modes. The report always lists losing categories (category median saving at or below 0), whatever the verdict.
5. Synthetic records can never produce an admissible claim.

Wherever a percentage is quoted from this benchmark, the run id goes with it (SF-TE-01).

### Report format

`report.md` and `report.json` per run: run id, model, headline pair, rule outcome, claim, cell status counts, configuration mismatch count, a decision table per mode and pair (total saving, median saving, pairs won, excluded pairs, pass counts, valid fraction, verdict, losing categories), per-category saving tables, per-category per-arm medians (tokens, round trips, tool-result tokens, wall time, pass rate), and a hook/rule configuration table per arm and mode with rule-injection counts and median price under the configured rates. `cells.jsonl` holds every cell's configuration and metrics.

### Cost estimate per full run (estimate, not a measurement)

Inputs: the audit's 24 Sonnet runs in `evidence/paired-benchmark-runs.json` (read-only tasks on SymForge itself): mean 51,363 fresh input tokens, 352,811 cache-read tokens, 1,316 output tokens per run. Prices: Claude Sonnet 5.5 input $2.00/MTok, cache read $0.10/MTok, output $10.00/MTok (claude-api skill table cached 2026-11-03); cache write assumed at 1.25 times input, $2.50/MTok, and all fresh input is priced as cache write (upper bound).

- Per read-only run: 51,363 x $2.50/M + 352,811 x $0.10/M + 1,316 x $10/M = $0.128 + $0.035 + $0.013 = **$0.177**.
- Edit and execute tasks were never measured; assume twice the tokens. 13 of 24 tasks are read-only (12 in the four read categories plus the zod diagnose task), 11 edit or execute: blend factor (13 x 1 + 11 x 2) / 24 = 1.46, so **$0.258 per run**.
- Runs per full run: 24 tasks x 3 arms x 5 repetitions x 2 modes = **720**.
- Full run: 720 x $0.258 = **about $186**. Range: $127 if every task costs like a read-only one, $382 at three times. Weekly: about $806 a month, about $9,700 a year at the central figure. On Claude Opus 5.5 ($4 / $20 / $0.20) the figures double.
- Wall time: audit median 186 s per read-only run; at the same 1.46 blend about 4.5 minutes per run, 54 hours serial. The workflow proposal shards by repo x mode x arm (24 jobs of 30 runs, about 2.3 hours each) to stay under the 6-hour hosted-runner job limit.

### Out of scope

- SymForge's own repository as a benchmark repository (PolyForm Noncommercial, not permissive). It can be added as an optional fifth repository for internal runs; its numbers must then be reported separately.
- Installing the scheduled workflow under `.github/workflows/`: both workflow files are fingerprint-pinned by `tests/preventive_runtime_dark_v11.rs`; wiring is a separate two-file change (see `research/token-benchmark/WORKFLOW-PROPOSAL.md`).
- Reference ("gold") solutions per task; calibration proves the oracle rejects the base tree but not that it accepts a correct solution.
- Clients other than Claude Code, model comparisons, fixed tool-list cost (reported separately in the audit, B.3).
- Any change to SymForge's `health` savings figure (SF-TE-04 is a separate finding).

## Success Criteria

- **SC-001**: `python -m unittest discover -s tests` passes in `research/token-benchmark/` (met: 37 tests).
- **SC-002**: `python bench.py dry-run` produces a complete synthetic report over 8 fixture tasks x 3 arms x 2 modes x 5 repetitions with every cell's configuration stated and zero configuration mismatches (met; sample in `research/token-benchmark/samples/`).
- **SC-003**: `calibrate` passes for all 24 tasks on the pinned clones (not yet run).
- **SC-004**: A scheduled run produces a results artifact with per-category medians and pairs won (SF-BEN-01 acceptance; not yet run).
- **SC-005**: The report states the hook and rule configuration of every cell (SF-BEN-02 acceptance; met in the dry run, unproven live).

## Assumptions

- Claude Code flag names in `config.json` (`--setting-sources`, `--strict-mcp-config`, `--tools`, `--allowedTools`, `--disallowedTools`, `--append-system-prompt`, `--max-budget-usd`, `--effort`) are taken from `research/full-surface-benchmark/claude_task_runner.py` (pinned to Claude Code 2.1.207) and must be checked against `claude --help` for the version the workflow pins.
- `CLAUDE_CONFIG_DIR` and `HOME` redirect the client's user-level settings; path-scoped rule injection is visible in stream-json output in a way the sentinel count can see. Both are unverified until the first live cell; the dry run only shows the harness counts what it is given.
- `symforge init --client claude` writes hooks under the client home named by `CLAUDE_CONFIG_DIR`/`HOME`.
