# Tasks: Standing Three-Arm Token and Quality Benchmark

**Input**: `specs/035-token-benchmark/spec.md`, `plan.md`

`[X]` means done and checked in this session with the evidence named; `[ ]` means not done.

## Phase 1: Design

- [X] T001 Read SF-BEN-01, SF-BEN-02, SF-TE-01 and the owner decisions in the review ledger; read AUDIT.md B and D.3 and the evidence scripts on `origin/docs/product-audit-redacted-2026-11-09`.
- [X] T002 Pick four permissive repositories and pin them with `git ls-remote` (`python bench.py verify-pins`: all four at remote HEAD on 2026-11-10).
- [X] T003 Check every task anchor against the pinned raw files (function names, seed `find` strings, block delimiters).
- [X] T004 Write spec.md, plan.md, tasks.md.

## Phase 2: Harness

- [X] T010 `transcript.py`: stream-json parser adapted from the audit's parse.py; allow-list check.
- [X] T011 `oracles.py`: eight check types, derived truth from the base commit, `error` distinct from fail.
- [X] T012 `runner.py`: per-cell clone, instruction-file removal, rule fixture, seeds, per-cell client home, hook install, MCP config, client launch with timeout, record.
- [X] T013 `aggregate.py`: decision rule, per-category tables, configuration check, Markdown and JSON report.
- [X] T014 `bench.py`: verify-pins, calibrate, matrix (shuffled, resumable, refuses uncalibrated tasks live), report, dry-run.
- [X] T015 24 task definitions, hidden tests, steering text, rule fixture, `config.json`.
- [X] T016 Copy the audit's parse.py, grade.py and cost_model.py into `audit_evidence/` with a source header.

## Phase 3: Self-checks

- [X] T020 Unit tests: oracles, transcript, allow-list, aggregation and decision rule, suite structure, end-to-end dry run (37 tests pass).
- [X] T021 Full dry run, 240 synthetic cells; sample report in `research/token-benchmark/samples/`.
- [X] T022 README with exact commands and the path-scoped rules user note; WORKFLOW-PROPOSAL.md.

## Phase 4: Before the first paid run (not done)

- [ ] T030 Pin a Claude Code version in `config.json` `client.version_pin` and check every flag in `client.command` against its `--help`.
- [ ] T031 Run one live cell per arm and mode on one repository; confirm that `CLAUDE_CONFIG_DIR` isolation holds, that `symforge init --client claude` writes into the cell home, and that rule injection shows up as sentinel hits in arm a mode on. Adjust the parser if the stream-json shape differs. Confirm the agent's shell finds Go and Cargo with `HOME` redirected (set `CARGO_HOME`, `RUSTUP_HOME`, `GOPATH`, `GOMODCACHE` explicitly).
- [ ] T032 `python bench.py calibrate --work <dir>` on all 24 tasks with toolchains installed; fix any task whose seed or derivation fails. Promote the six `draft` edit tasks once their hidden tests compile against the pin (they must fail on the base tree).
- [ ] T033 Write a reference solution per edit task and prove its oracle accepts it (calibration only proves rejection today).
- [ ] T034 Independent review of the harness (Constitution VI).

## Phase 5: Scheduling (not done)

- [ ] T040 Install the workflow from WORKFLOW-PROPOSAL.md under `.github/workflows/` together with its `WORKFLOW_FINGERPRINTS` entry in `tests/preventive_runtime_dark_v11.rs` (two-file change; the `rust` job is the authority for the pin).
- [ ] T041 Configure the `ANTHROPIC_API_KEY` repository secret and a runner with Python, Go and Rust toolchains.
- [ ] T042 First full run; cite its run id in any token claim; close SF-TE-01 on its outcome or record a decision against it.
- [ ] T043 After the first full run, unblock the structural items SF-ARC-01, 02, 03, 05, 08 per the owner decision.
