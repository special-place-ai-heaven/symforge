# Existing Goal Overlap - symforge-code-review-2026-05-22

SFR00 reconciliation artifact.

## Scope

This inventory covers `.agent/goals/symforge-live-code-backlog/SFB*.md` as found in current `HEAD` of the SFR00 worktree. Each listed SFB goal has `status: "Completed"` at line 6 of its goal file, a `completion_commit` at line 14, and that commit was verified as an ancestor of `HEAD` with `git merge-base --is-ancestor`.

Branch-policy note: all SFB files name `backlog-implementation` as their historical target branch. SFR00 treats that branch as prohibited/stale per `.agent/goals/symforge-code-review-2026-05-22/SFR00-reconcile-review-state-and-existing-backlog-goals.md`; completed SFB commits may be used as evidence, but new work must use the SFR goal branches.

## Disposition Legend

- `Use as evidence`: keep the completed SFB as proof for the related SF row; do not duplicate the implementation.
- `Supersede / narrow`: the SFR goal may still run, but it must first verify the completed SFB and limit itself to the remaining gap.
- `Skip duplicate`: no new SFR implementation should recreate this old backlog item.
- `Unknown`: not used; all discovered SFB goals had completed status and reachable commits.

## Overlap Table

| SFB goal | Completed commit | SF / SFR overlap | Disposition |
|---|---:|---|---|
| SFB01 - Mitigate Windows libgit2 lockfile flakes in git-heavy tests | `e88ed59` | SF-037 / SFR16; evidence in `tests/git_commit_retry.rs` and `src/git/test_helpers.rs:72`. | Supersede / narrow: SFR16 should verify residual flake risk before changing helpers. |
| SFB02 - Add actionable untracked-file diagnostics to empty search results | `5594c78` | SF-010 / SFR01; review evidence at `docs/code-review-2026-05-22.md:47`; implementation evidence at `src/protocol/tools.rs:3339`. | Use as evidence: update backlog docs only; skip duplicate diagnostic implementation. |
| SFB03 - Surface sidecar PID and alive state in health output | `03bf46f` | SF-011 / SFR01; review evidence at `docs/code-review-2026-05-22.md:48`; test evidence `test_health_compact_surfaces_dead_sidecar_pid_and_state` at `src/protocol/tools.rs:14519`. | Use as evidence: update docs/registers only; skip duplicate health implementation. |
| SFB04 - Classify Obsidian internals as path noise without hiding normal wiki markdown | `2a4577a` | SF-039/SF-044/SFR24 guidance and noise-filtering context. | Use as evidence: SFR24 should preserve this behavior while tuning ranking/noise. |
| SFB05 - Land top external-evaluator regression tests or convert gaps into concrete backlog items | `5ac3e39` | SF-058/SFR27 release-readiness and conformance evidence. | Use as evidence: no duplicate planning inventory needed. |
| SFB06 - Make partial-parse hygiene distinguish expected vendor noise from unexpected repo partials | `37d7918` | SF-035 / SFR13; current `health` reports expected vendor partial parse noise separately. | Supersede / narrow: SFR13 should focus on quarantine registry gaps, not vendor-noise classification. |
| SFB07 - Pin search_text usage grouping behavior for doc comments and markdown | `96b4954` | SF-036/SF-059/SFR24 guidance behavior. | Use as evidence: preserve grouping behavior during SFR24. |
| SFB08 - Preserve same-line inline docs in replace_symbol_body | `691e0a7` | SF-045; review evidence at `docs/code-review-2026-05-22.md:82`; tests at `src/protocol/tools.rs:20009`. | Use as evidence: skip duplicate inline-doc preservation work. |
| SFB09 - Define machine-readable MCP result-status contract | `8bcb3ab` | SF-030/SFR14; central contract evidence in `src/protocol/result_status.rs`. | Use as evidence: SFR14 audits coverage, not the base contract. |
| SFB10 - Apply result-status semantics to read, search, and reference tools | `ee2207e` | SF-030/SFR14; tests include read/search/reference result-status assertions in `src/protocol/tools.rs:14992`. | Supersede / narrow: SFR14 should find remaining gaps only. |
| SFB11 - Apply result-status semantics to edit and mutate tools | `ba1455f` | SF-030/SF-046/SFR08/SFR14/SFR21; dry-run/result-status interaction must be preserved. | Use as evidence: do not recreate edit status semantics while adding idempotency or extracting modules. |
| SFB12 - Surface runtime project and index identity in status output | `9abdac0` | SF-032/SF-039/SF-050/SFR23; runtime identity tests at `src/protocol/tools.rs:14416`. | Supersede / narrow: SFR23 should verify identity/reset gaps before adding new health fields. |
| SFB13 - Add deterministic fresh-index and reset workflow for evaluations | `5d6ab48` | SF-032/SFR23/SFR27 evaluation readiness. | Use as evidence: preserve deterministic reset workflow. |
| SFB14 - Create replayable public-contract conformance corpus | `a247c61` | SF-030/SF-058/SFR05/SFR14/SFR27; conformance corpus in `tests/conformance.rs`. | Use as evidence: extend corpus for ambiguous symbols instead of replacing it. |
| SFB15 - Calibrate guidance ranking and noise filtering for investigation_suggest, ask, and explore | `2c907ce` | SF-024/SF-036/SF-059/SFR24. | Supersede / narrow: SFR24 should build on prior calibration. |
| SFB16 - Close co-change rerank calibration for weak anchors and chore anchors | `d2b5de6` | SF-049/SFR24 ranking and co-change behavior. | Use as evidence: monitor corruption/staleness paths, do not rerun calibration wholesale. |
| SFB17 - Retire trace_symbol from client guidance while preserving deliberate compatibility policy | `2e8900f` | SF-008/SF-029/SF-041/SFR19; allowlist tests at `src/cli/init.rs:1671` and daemon alias tests in `tests/daemon_aliases.rs`. | Supersede / narrow: SFR19 should decide version boundary/docs, not redo allowlist removal. |
| SFB18 - Upgrade Rust grammar for Rust 2024 raw-reference parsing | `8cdf24e` | SF-025/SFR25; parser/toolchain readiness overlap. | Supersede / narrow: SFR25 should verify raw-reference fixtures before changing grammar. |
| SFB19 - Report deepest actionable validate_file_syntax parse errors | `dbb70fd` | SF-026/SFR26; implementation evidence in `src/parsing/mod.rs:147` and test `test_parse_source_reports_deepest_actionable_nested_error_node`. | Supersede / narrow: SFR26 should verify current diagnostics before adding parser work. |
| SFB20 - Unify truncation phrasing across protocol and sidecar surfaces | `a6c1841` | No direct SF row, but relevant to SFR27 release consistency evidence. | Use as evidence: skip duplicate wording cleanup. |
| SFB21 - Add inline extractor tests for web and typed-script languages | `82e3d98` | SF-044 config/code-intelligence evidence. | Use as evidence: no duplicate extractor-test planning. |
| SFB22 - Add inline extractor tests for systems and backend languages | `3360c5b` | SF-044 config/code-intelligence evidence. | Use as evidence: no duplicate extractor-test planning. |
| SFB23 - Add inline extractor tests for scripting and remaining languages | `8eaf089` | SF-044 config/code-intelligence evidence. | Use as evidence: no duplicate extractor-test planning. |
| SFB24 - Implement local SQLite analytics store foundation with disabled no-footprint behavior | `f8ffa36` | SF-043 analytics/privacy evidence. | Use as evidence: SFR01 can document the privacy stance. |
| SFB25 - Add bounded analytics queue, background writer, and safe tool-call instrumentation | `6bdde6a` | SF-043 analytics/privacy and result-status context. | Use as evidence: skip duplicate analytics foundation work. |
| SFB26 - Add analytics CLI status, summary, export, reset, retention, and redaction coverage | `f968214` | SF-043; review row cites `tests/sfb26_analytics_cli.rs`. | Use as evidence: SFR01 can reference no-MCP-analytics and redaction coverage. |
| SFB27 - Choose first non-code repository intelligence family and define corpus | `64389d8` | SF-044 repository/config intelligence context. | Use as evidence: no duplicate corpus-definition task. |
| SFB28 - Implement first non-code repository intelligence family through existing surfaces | `ee418eb` | SF-044/SF-059 repository intelligence and guidance surface context. | Use as evidence: SFR24 should preserve existing surfaces. |

## SFR Scope Adjustments

- SFR01 should update docs and backlog state for SF-010/SF-011 using SFB02/SFB03 evidence instead of creating product changes.
- SFR13 should treat SFB06 as completed partial-parse hygiene and focus only on the new quarantine registry/health-surfacing requirement.
- SFR16, SFR25, and SFR26 may be fully or partly superseded by SFB01, SFB18, and SFB19; each should begin with verification against current `HEAD`.
- SFR14 should not recreate the result-status substrate or public contract corpus; it should audit and fill residual result-status coverage.
- SFR19 should preserve the deliberate split between removed client guidance and retained daemon compatibility unless it explicitly changes the version policy.
