# Three-arm token and quality benchmark

Spec: [`specs/035-token-benchmark/spec.md`](../../specs/035-token-benchmark/spec.md). Closes the buildable part of review findings SF-BEN-01 and SF-BEN-02.

The harness runs 24 tasks on four pinned public repositories in three arms (native tools only; SymForge with steering; SymForge without steering), five repetitions each, with client hooks and path-scoped rules off and on. A script grades every run. A report applies a rule declared before the run.

> [!IMPORTANT]
> No paid run has been made. The only report in this folder, `samples/dry-run-report.md`, is synthetic: a fake client replayed scripted transcripts against a bundled fixture. It shows the report format and says nothing about SymForge.

## Requirements

- Python 3.10 or later (standard library only) and `git`.
- For live runs: the Claude Code CLI, the `symforge` binary, an `ANTHROPIC_API_KEY` in the environment, and the toolchains for `command` oracles: Python with pip (flask), Go (cobra), Cargo (ripgrep). Zod needs none. Each cell points `HOME` at its own client home, so set `CARGO_HOME`, `RUSTUP_HOME`, `GOPATH` and `GOMODCACHE` explicitly; the runner passes them through to the agent's shell.

## Commands

Run from this directory.

```sh
# Self-checks (about 3.5 minutes; includes two small dry runs)
python -m unittest discover -s tests

# Whole pipeline without a model, network or API key: 8 fixture tasks x 3 arms x 2 modes x 5 reps
python bench.py dry-run --out <scratch>/dry

# Check the four pinned commits against the remotes
python bench.py verify-pins

# Clone the pins, prepare toolchains, prove each oracle evaluates and rejects the base tree
python bench.py calibrate --work <scratch>/work

# One shard of a live run (repeat --repo/--arm/--mode as needed; omit them for the full matrix)
python bench.py matrix --work <scratch>/work --out <scratch>/out --repo flask --arm a --mode off --run-id bench-2026-11-16 --seed 1

# Aggregate whatever records exist into report.md, report.json and cells.jsonl
python bench.py report --out <scratch>/out
```

`matrix` refuses tasks that `calibrate` has not passed, unless `--allow-uncalibrated` is given. It skips cells that already have a record, so an interrupted run resumes. Use `--no-fetch` to forbid cloning.

## What is configuration

`config.json` holds the model id, effort, the client command template, tool lists, the headline pair, the decision gates, prices and the rule sentinel. Nothing in the scripts names a model or a client binary. Check the flags in `client.command` against `claude --help` for the version you pin, and set `client.version_pin`. `fixtures/config.dryrun.json` is merged over it for the dry run.

## Files

| Path | Role |
|---|---|
| `bench.py` | CLI |
| `runner.py` | one cell: clone, isolate, seed, launch client, record |
| `transcript.py` | stream-json parser (adapted from the audit's `parse.py`) and allow-list check |
| `oracles.py` | executable checks; truth derived from the cell's base commit |
| `aggregate.py` | decision rule and report |
| `fake_client.py` | synthetic client for the dry run |
| `tasks/`, `hidden/`, `repos.json` | the suite, hidden tests, pins |
| `overlays/rules/` | path-scoped rule fixture with a sentinel the parser counts |
| `steering.md` | text appended to the system prompt in arm b |
| `audit_evidence/` | verbatim copies of the audit's `parse.py`, `grade.py`, `cost_model.py` |
| `WORKFLOW-PROPOSAL.md` | proposed weekly workflow (not installed) |

## Reading a report

Saving is native minus SymForge arm over native, on median total tokens processed (input, cache write, cache read and output) per task. The rule accepts a whole-task token claim only when, in both hook/rule modes, the total saving and the median per-task saving are both positive, the SymForge arm passes at least as many oracles as the native arm, and at least 95% of cells are valid. Losing categories are always listed. Any percentage taken from a report must cite its run id.

## Note for users: SymForge and path-scoped client rules

Clients such as Claude Code load path-scoped rules (for example `.claude/rules/*.md` with `paths:` globs) when the agent reads or edits a matching file through the client's own Read and Edit tools. Reads and edits made through SymForge's MCP tools do not trigger those rules: the client does not see a file access, so the rules are never loaded. An agent that works only through SymForge therefore never sees the project's path-scoped rules. If those rules matter for your project, put them where the client always loads them (for example the project's root `CLAUDE.md`), or make sure the agent opens at least one matching file with the client's own tools. The 2026-11-09 audit observed this (AUDIT.md section B.2); this benchmark records it per cell as the rule-sentinel count.
