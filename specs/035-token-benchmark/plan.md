# Implementation Plan: Standing Three-Arm Token and Quality Benchmark

**Spec**: `specs/035-token-benchmark/spec.md` | **Date**: 2026-11-10

## Summary

A Python harness under `research/token-benchmark/` runs each (repo, task, arm, mode, repetition) cell in a fresh clone of a pinned commit with its own client home, launches the configured client, parses its stream-json transcript, enforces the arm allow-list from the transcript, grades with executable oracles whose truth is derived from the cell's base commit, and aggregates records into a report that applies the predeclared rule. A dry run swaps the client for a fake that replays scripted transcripts against a bundled fixture, so the whole pipeline is exercised without a model or a network.

## Technical Context

- **Language**: Python 3.10+ standard library only. No new dependency: the existing runners use the stdlib plus `tiktoken`; this harness counts tokens from the client's own usage fields instead, so it needs no tokenizer.
- **External tools at run time**: `git`; the model client (Claude Code CLI); the `symforge` binary; per-repo toolchains for `command` oracles (Python with pytest and Flask's dependencies, Go, Cargo). Zod tasks need no toolchain.
- **Reuse**: `transcript.py` adapts the audit's `evidence/parse.py` to stream-json; `audit_evidence/` holds verbatim copies of `parse.py`, `grade.py` and `cost_model.py` with a source header; `runner.py` copies the environment allow-list from `research/full-surface-benchmark/claude_task_runner.py` and borrows its client flag set; the cobra pin matches `research/full-surface-benchmark/corpus.lock.json`.
- **Storage**: one JSON record per cell under `<out>/records/`, resumable; scratch clones under `<work>/`.
- **Testing**: `python -m unittest discover -s tests` (oracles, transcript parsing, allow-list, aggregation and decision rule, suite structure, end-to-end dry run).
- **Platforms**: Windows (developed and tested) and Linux (CI target).

## Constitution Check

- **I. Reporting invariant**: a cell is `valid` only when the transcript has a result event, the allow-list held and every oracle check evaluated; an oracle that cannot evaluate reports `error`, never a pass or a fail. Reports carry a synthetic banner and refuse an admissible claim from synthetic records.
- **II. RED-first evidence**: oracle tests include the failing case beside each passing case; `calibrate` requires every oracle to reject the base tree before a task enters a live run.
- **III. Frozen contracts**: the decision rule, arms, modes and headline pair live in `config.json` and the spec before any paid run.
- **IV. Verification gates**: no cargo command is involved in building the harness; the CI workflow edit is a separate two-file change under the `WORKFLOW_FINGERPRINTS` pin.
- **VI. Independent review**: owed before the chief commits the files.

## Project Structure

```text
specs/035-token-benchmark/        spec.md, plan.md, tasks.md
research/token-benchmark/
  bench.py                        CLI: verify-pins, calibrate, matrix, report, dry-run
  runner.py                       cell setup, client launch, record
  transcript.py                   stream-json parser, allow-list check
  oracles.py                      executable checks
  aggregate.py                    decision rule, report rendering
  fake_client.py                  synthetic client for the dry run
  config.json repos.json steering.md
  tasks/{flask,cobra,zod,ripgrep}.json
  hidden/<repo>/...               hidden test files copied in at grading time
  overlays/rules/                 path-scoped rule fixture with a sentinel
  fixtures/                       dry-run repo, tasks, script, config override
  audit_evidence/                 verbatim audit scripts
  tests/                          unittest suite
  samples/                        synthetic dry-run report
  README.md WORKFLOW-PROPOSAL.md
```

## Design notes

- **Cell isolation**: `git clone --shared` of a pristine clone per cell, `core.autocrlf=false`, repo-provided `CLAUDE.md` and `.claude/` removed, rule fixture added in mode on, base seeds committed as the `bench base` commit, review seeds left uncommitted. `HOME`, `USERPROFILE` and `CLAUDE_CONFIG_DIR` point at a per-cell home.
- **Ordering bias**: cells are shuffled with a recorded seed so no arm always pays the first cache write (audit B.1).
- **Resume**: a recorded cell is never rerun; deleting its record is the explicit way to rerun it.
- **Secrets**: the client environment is an allow-list of OS names plus `client.pass_env` names; values are never written to records.

## Complexity Tracking

None beyond the spec: one module per concern, no framework.
