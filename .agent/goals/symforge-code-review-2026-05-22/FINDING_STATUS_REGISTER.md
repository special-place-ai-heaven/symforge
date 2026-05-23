# Finding Status Register - symforge-code-review-2026-05-22

SFR00 reconciliation artifact.

## Method

- Source review: `docs/code-review-2026-05-22.md`, especially the master table at lines 38-97.
- Goal mapping: `.agent/goals/symforge-code-review-2026-05-22/FINDING_TO_GOAL_MATRIX.md`.
- Existing backlog evidence: `.agent/goals/symforge-live-code-backlog/SFB*.md`.
- Branch guard evidence: `git branch --show-current` returned `goal/sfr00-reconcile-review-state-and-existing-backlog-goals`; `git rev-parse --short HEAD` returned `a39021c`; `git log --oneline -1` returned `a39021c docs: add full-spectrum code review for 2026-05-22`.
- Current-main overlap evidence: every completed SFB `completion_commit` in `.agent/goals/symforge-live-code-backlog/` was checked with `git merge-base --is-ancestor <commit> HEAD` and was an ancestor of `HEAD`.
- SymForge evidence: the worktree was indexed with `index_folder` and `health_compact` reported Ready for project root `.worktrees/sfr00-reconcile-review-state-and-existing-backlog-goals`.

## Classification Legend

- `Live issue`: a real gap that should remain assigned to the listed SFR goal.
- `Already fixed / docs drift`: implementation evidence exists in current `HEAD`; do not duplicate product work, only update docs/registers or narrow follow-up scope.
- `Partial / verify residual`: a substrate exists, but the review's broader coverage gap still needs the listed SFR audit or extension.
- `Evidence-only`: positive existing behavior; use as release evidence or documentation input.
- `Deferred strategic`: valid long-term or hygiene item, not a blocker for earlier security/correctness waves.
- `Operational note`: review-process issue, not a product defect.
- `False positive / no action`: current evidence shows no product work is required.

## Master Register

| Finding | Class | Current evidence | Disposition |
|---|---|---|---|
| SF-001 | Live issue | Review row `docs/code-review-2026-05-22.md:38` shows AGENTS still names retired/v1 APIs while init exposes v7 names. | SFR01 updates operator docs or adds an explicit migration table. |
| SF-002 | Live issue | Review row `docs/code-review-2026-05-22.md:39` says mutating tools lack `idempotency_key` and replay storage. | SFR06 creates the substrate; SFR07 and SFR08 apply it to index/edit tools. |
| SF-003 | Live issue | Review row `docs/code-review-2026-05-22.md:40` records missing repair/checkpoint/run-lifecycle tools and INFR-05 guard tests for removed v1 names. | SFR09 and SFR10 decide/implement the current repair and checkpoint surface. |
| SF-004 | Live issue | Review row `docs/code-review-2026-05-22.md:41` identifies unauthenticated daemon routes and configurable bind behavior. | SFR03 owns daemon bind/auth hardening. |
| SF-005 | Live issue | Review row `docs/code-review-2026-05-22.md:42` says ambiguous `get_symbol_context` picks the first candidate path. | SFR05 returns an explicit ambiguous outcome and adds conformance coverage. |
| SF-006 | Live issue | Review row `docs/code-review-2026-05-22.md:43` says clean shutdown is the only serialization boundary. | SFR09 adds checkpointing; SFR10 reconciles repair/run lifecycle expectations. |
| SF-007 | Live issue | Review row `docs/code-review-2026-05-22.md:44` says `health_compact` is in conformance but absent from generated allowlists. | SFR02 synchronizes tool allowlists and conformance. |
| SF-008 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:45` shows a deliberate daemon compatibility alias for `trace_symbol`; tests exist in `tests/daemon_aliases.rs`. | SFR19 decides alias lifecycle; do not remove it incidentally. |
| SF-009 | Live issue | Review row `docs/code-review-2026-05-22.md:46` shows `CLAUDE.md` says 31 tools while conformance has 32 including `health_compact`. | SFR01/SFR02 update docs and allowlists consistently. |
| SF-010 | Already fixed / docs drift | Review row `docs/code-review-2026-05-22.md:47` says backlog #2 appears done; SFB02 is `Completed` with commit `5594c785172d5582bc372b60c5bc6b524e6edd03` at `.agent/goals/symforge-live-code-backlog/SFB02-add-actionable-untracked-file-diagnostics-to-empty-search-results.md:6` and `:14`; code evidence includes `src/protocol/tools.rs:3330`, `:3339`, `:4245`, `:6048`, and `:6467`; regression evidence includes assertions at `src/protocol/tools.rs:12826` and `:13598` for the untracked-file diagnostic and `analyze_file_impact(..., new_file=true)` recovery call. | Use SFB02 as evidence; SFR01 should mark/narrow backlog docs, not reimplement diagnostics. |
| SF-011 | Already fixed / docs drift | Review row `docs/code-review-2026-05-22.md:48` says backlog #3 appears done; SFB03 is `Completed` with commit `03bf46fa2515821a040e985dbba16583e923e5c1` at `.agent/goals/symforge-live-code-backlog/SFB03-surface-sidecar-pid-and-alive-state-in-health-output.md:6` and `:14`; test evidence includes `test_health_compact_surfaces_dead_sidecar_pid_and_state` in `src/protocol/tools.rs:14519`. | Use SFB03 as evidence; SFR01 should close/narrow backlog docs. |
| SF-012 | Live issue | Review row `docs/code-review-2026-05-22.md:49` identifies the large `src/protocol/tools.rs` module. | SFR20 and SFR21 split read/search and edit handlers without behavior change. |
| SF-013 | Live issue | Review row `docs/code-review-2026-05-22.md:50` identifies the large `src/live_index/query.rs` module. | SFR22 extracts disambiguation, bundle, and health-view helpers. |
| SF-014 | Live issue | Review row `docs/code-review-2026-05-22.md:51` shows AGENTS architecture names modules not present in `src/lib.rs`. | SFR01 either aligns docs to current shipped layout or records a migration plan. |
| SF-015 | Live issue | Review row `docs/code-review-2026-05-22.md:52` says corrupt snapshots are dropped with warning but no quarantine artifact. | SFR11 owns corrupt snapshot quarantine. |
| SF-016 | Live issue | Review row `docs/code-review-2026-05-22.md:53` says parse/span quarantine is only implicit via `ParseStatus::Failed`. | SFR13 adds a bounded quarantine registry and health surfacing. |
| SF-017 | Live issue | Review row `docs/code-review-2026-05-22.md:54` identifies a daemon `close_session` unwrap. | SFR04 removes the unwrap and covers stale state behavior. |
| SF-018 | Live issue | Review row `docs/code-review-2026-05-22.md:55` flags daemon lifecycle process control and PID safety. | SFR04 strengthens PID ownership and termination safety. |
| SF-019 | Evidence-only | Review row `docs/code-review-2026-05-22.md:56` says edit trust gating is good and covered by `tests/edit_safety_trust.rs`. | Use as SFR01 documentation evidence; no product implementation is implied by SFR00. |
| SF-020 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:57` says snapshot bytes/hash handling aligns with byte-exact rules but lacks explicit CRLF regression coverage. | SFR17 adds byte-exact CRLF and watcher-read tests. |
| SF-021 | Live issue | Review row `docs/code-review-2026-05-22.md:58` says spot verification exists but mismatch/progress surfacing is missing. | SFR12 exposes mismatch/progress details; SFR10 uses the evidence for repair decisions. |
| SF-022 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:59` says a 1000-file load perf test is ignored. | SFR15 decides CI/nightly/perf-smoke handling. |
| SF-023 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:60` says real-repo coupling calibration is ignored. | SFR15 documents or schedules calibration without burdening normal PR CI. |
| SF-024 | Live issue | Review row `docs/code-review-2026-05-22.md:61` says CI lacks fmt, clippy, and release build gates. | SFR15 owns CI hardening. |
| SF-025 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:62` flags Rust edition/toolchain risk; SFB18 completion also covers raw-reference parser support. | SFR15 covers toolchain rationale/CI; SFR25 should first verify SFB18 before adding parser work. |
| SF-026 | Live issue | Review row `docs/code-review-2026-05-22.md:63` says npm Windows launcher smoke is skipped. | SFR18 owns the Windows launcher smoke path. |
| SF-027 | Evidence-only | Review row `docs/code-review-2026-05-22.md:64` confirms MCP resources exist and match much of AGENTS. | SFR01 documents resource URIs; no product code change required by this row. |
| SF-028 | Evidence-only | Review row `docs/code-review-2026-05-22.md:65` confirms six prompts exist. | SFR01 maps prompts to AGENTS language; no product code change required by this row. |
| SF-029 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:66` records `search_files changed_with=` deprecation and v8 removal tracking. | SFR19 aligns compatibility/migration notes. |
| SF-030 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:67` says result-status metadata exists but coverage should be extended; SFB09, SFB10, SFB11, and SFB14 are completed and in `HEAD`; baseline contract evidence includes `src/protocol/result_status.rs`, `tests/conformance.rs:421`, `tests/conformance.rs:451`, and result-status tool assertions at `src/protocol/tools.rs:14992`, `:15090`, `:15183`, and `:15258`. | Use SFB09-SFB11/SFB14 as evidence; SFR14 should audit remaining public read/search/edit gaps instead of rebuilding the substrate. |
| SF-031 | Evidence-only | Review row `docs/code-review-2026-05-22.md:68` identifies generation-fenced watcher mutation rejection as good behavior. | SFR01 should document this in recovery guidance. |
| SF-032 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:69` says warm start/background verify exists but progress is not exposed. | SFR09/SFR12/SFR23 should expose checkpoint and runtime identity/progress; use SFB12/SFB13 as existing identity/reset evidence. |
| SF-033 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:70` says watcher reads bytes with `fs::read`, but historical CRLF regression coverage is still needed. | SFR17 owns the explicit regression tests. |
| SF-034 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:71` confirms per-file parse status isolation but asks for compact-health surfacing. | SFR12/SFR13 own health/quarantine visibility. |
| SF-035 | Already fixed / docs drift | Review row `docs/code-review-2026-05-22.md:72` points at backlog #6; SFB06 is `Completed` with commit `37d7918`, and SymForge health now distinguishes expected vendor partial parse noise. | Use SFB06 as evidence; SFR13 should avoid duplicating partial-parse hygiene and focus on quarantine registry gaps. |
| SF-036 | Evidence-only | Review row `docs/code-review-2026-05-22.md:73` says search frecency behavior is deliberately designed. | SFR01 can document it; SFR24 can preserve it while tuning guidance ranking. |
| SF-037 | Already fixed / docs drift | Review row `docs/code-review-2026-05-22.md:74` points at backlog #1; SFB01 is `Completed` with commit `e88ed59e0d654209ed843d5c77636cc5e06dbdf3`, and test evidence includes `tests/git_commit_retry.rs` and `src/git/test_helpers.rs:72`. | SFR16 should verify/narrow residual Windows flake risk rather than reimplement SFB01 blindly. |
| SF-038 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:75` identifies sidecar governor defaults as a tuning opportunity. | Keep for future performance tuning or SFR27 release notes; no SFR00 product work. |
| SF-039 | Live issue | Review row `docs/code-review-2026-05-22.md:76` says startup ordering/hook adoption docs need improvement. | SFR01 and SFR23 own docs/runtime identity clarity. |
| SF-040 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:77` says localhost sessions are partially tested but daemon identity/port validation needs hardening. | SFR03/SFR04 own daemon hardening and PID/identity safety. |
| SF-041 | Already fixed / docs drift | Review row `docs/code-review-2026-05-22.md:78` says retired `trace_symbol` is excluded from allowlists; SFB17 is completed with commit `2e8900f`, and tests exist at `src/cli/init.rs:1671`. | SFR19 should document lifecycle and avoid undoing the deliberate compatibility policy. |
| SF-042 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:79` identifies `tasks/todo.md` as historical session evidence in repo. | Defer to SFR27 or a separate hygiene task; no source-code change in SFR00. |
| SF-043 | Evidence-only | Review row `docs/code-review-2026-05-22.md:80` says analytics are disabled by default and no MCP analytics tools are advertised; SFB24-SFB26 are completed. | SFR01 can document privacy stance; no product implementation implied. |
| SF-044 | Evidence-only | Review row `docs/code-review-2026-05-22.md:81` confirms config extractors and tests; SFB21-SFB23 cover inline extractor test expansion. | SFR01/SFR24 may document and tune guidance around this evidence. |
| SF-045 | Already fixed / docs drift | Review row `docs/code-review-2026-05-22.md:82` says inline doc preservation tests exist; SFB08 is `Completed` with commit `691e0a713035309d910b78b3cdf2d540112a4d37` at `.agent/goals/symforge-live-code-backlog/SFB08-preserve-same-line-inline-docs-in-replace-symbol-body.md:6` and `:14`; implementation/test evidence includes inline-doc splice assertions at `src/protocol/edit.rs:5312`, `src/protocol/edit.rs:5325`, `src/protocol/tools.rs:20009`, and `src/protocol/tools.rs:20051`. | Use SFB08 as evidence; do not duplicate in a new product patch. |
| SF-046 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:83` says edit `dry_run` exists but is not idempotency. | SFR06/SFR08 add idempotency; SFR21 must preserve dry-run semantics during extraction. |
| SF-047 | Evidence-only | Review row `docs/code-review-2026-05-22.md:84` identifies edit tee snapshots as a recovery path. | SFR01 documents restore workflow; SFR08/SFR21 preserve behavior. |
| SF-048 | Evidence-only | Review row `docs/code-review-2026-05-22.md:85` confirms optional persistent frecency policy. | SFR01 documents default session vs persistent behavior. |
| SF-049 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:86` says co-change store has tests but corruption paths should be monitored. | Track in SFR27/future reliability backlog unless SFR24 touches co-change ranking. |
| SF-050 | Evidence-only | Review row `docs/code-review-2026-05-22.md:87` confirms worktree routing and tests. | SFR01 documents `working_directory`; SFR23 can reuse runtime identity evidence. |
| SF-051 | Evidence-only | Review row `docs/code-review-2026-05-22.md:88` says tracing/env-filter is sufficient for local MCP and OpenTelemetry is optional. | No immediate SFR product work. |
| SF-052 | Evidence-only | Review row `docs/code-review-2026-05-22.md:89` says global unsafe deny with narrow exceptions is good discipline. | Use as release evidence; periodic audit only. |
| SF-053 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:90` identifies an allowed dead-code helper that may be future API. | Defer to a focused hygiene task after higher-priority waves. |
| SF-054 | False positive / no action | Review row `docs/code-review-2026-05-22.md:91` classifies dead-code test helpers as test-only and OK. | No action beyond preserving test-only isolation. |
| SF-055 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:92` notes deprecated `home_dir` comment/usage. | Defer to discovery cleanup; not part of SFR00. |
| SF-056 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:93` records patched SCSS vendor crate tracking. | Track upstream merge; no immediate code-changing goal in this chain. |
| SF-057 | Deferred strategic | Review row `docs/code-review-2026-05-22.md:94` flags `diff.txt`; current worktree has tracked `diff.txt`, so this is repo hygiene rather than ignored local debris. | Defer to a specific hygiene decision; do not delete in SFR00. |
| SF-058 | Partial / verify residual | Review row `docs/code-review-2026-05-22.md:95` says public contract corpus exists but needs ambiguous symbol cases. | SFR05 and SFR14 expand conformance coverage using SFB14 evidence. |
| SF-059 | Evidence-only | Review row `docs/code-review-2026-05-22.md:96` says `ask` exists and routes natural-language intent. | SFR24 documents and tunes `ask` vs `explore` boundaries. |
| SF-060 | Operational note | Review row `docs/code-review-2026-05-22.md:97` says the review session had no SymForge index loaded; this SFR00 run called `index_folder` and `health_compact` reported Ready. | Treat as process evidence: future agents should index first; no product bug. |

## Summary

- Live issues routed to code/docs goals: SF-001, SF-002, SF-003, SF-004, SF-005, SF-006, SF-007, SF-009, SF-012, SF-013, SF-014, SF-015, SF-016, SF-017, SF-018, SF-021, SF-024, SF-026, SF-039.
- Already fixed or backlog-doc drift: SF-010, SF-011, SF-035, SF-037, SF-041, SF-045.
- Partial existing substrate with residual audit/coverage needed: SF-020, SF-025, SF-030, SF-032, SF-033, SF-034, SF-040, SF-046, SF-058.
- Evidence-only positive observations: SF-019, SF-027, SF-028, SF-031, SF-036, SF-043, SF-044, SF-047, SF-048, SF-050, SF-051, SF-052, SF-059.
- Deferred strategic or hygiene items: SF-008, SF-022, SF-023, SF-029, SF-038, SF-042, SF-049, SF-053, SF-055, SF-056, SF-057.
- False positive / no action: SF-054.
- Operational note: SF-060.

## SFR27 Final Burndown

SFR27 closure audit date: 2026-05-23.

Branch evidence: `git branch --show-current` returned `goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence`; starting `HEAD` was `2c98649 feat: land SFR26 validate_file_syntax diagnostics`.

All SFR00-SFR26 goal files in `.agent/goals/symforge-code-review-2026-05-22/` were marked `Completed` before this closure section was written. This closure goal changed no source code, tests, npm files, Cargo metadata, or GitHub workflow files.

| Finding | Final disposition | Evidence pointer |
|---|---|---|
| SF-001 | Fixed | SFR01 `2b844a5`; docs aligned to shipped v7 tool/resource/prompt surface. |
| SF-002 | Fixed | SFR06 `0dfd598c36501519772e23e08a5f4ca9e4e50d3f`, SFR07 `cb8dac218b70f0ca4cce327b2a13b920c6251598`, SFR08 `c8254953a4d8acb817d4008228c2d4d8014fac37`; mutating-tool idempotency substrate and applications completed. |
| SF-003 | Fixed | SFR09 `1c5f896`, SFR10 `5b76927`; checkpoint/repair/run-lifecycle surface resolved for current v7 behavior. |
| SF-004 | Fixed | SFR03 `b21b24b`; daemon bind/auth hardening completed. |
| SF-005 | Fixed | SFR05 `0d95de72b21aa8afb555f71480a4c2c94f2c00fa`; ambiguous `get_symbol_context` outcome and conformance coverage completed. |
| SF-006 | Fixed | SFR09 `1c5f896`, SFR10 `5b76927`; checkpointing and lifecycle reconciliation completed. |
| SF-007 | Fixed | SFR02 `7551932`; `health_compact` allowlist/conformance alignment completed. |
| SF-008 | Superseded | SFR19 `70214e7b3ca0dfc663dfe25467747574d29430d4`; v7 compatibility alias policy retained deliberately and v8 removal path documented. |
| SF-009 | Fixed | SFR01 `2b844a5`, SFR02 `7551932`; tool count and `health_compact` documentation/allowlists aligned. |
| SF-010 | Fixed | SFR01 `2b844a5`; backlog item closed as SFB02 evidence-backed docs drift. |
| SF-011 | Fixed | SFR01 `2b844a5`; backlog item closed as SFB03 evidence-backed docs drift. |
| SF-012 | Fixed | SFR20 `3bfc45fe628a681ce514b060e4ec873756d1dde5`, SFR21 `7ca22cac0df939f155f5732ea875470ba718eafc`; protocol read/search/edit handler extraction completed. |
| SF-013 | Fixed | SFR22 `ee6c3d2314089b91609bdb90ef07edeb3b131a9a`; live-index query split completed. |
| SF-014 | Fixed | SFR01 `2b844a5`; architecture docs reconciled to current layout. |
| SF-015 | Fixed | SFR11 `0c05e43`, SFR10 `5b76927`; corrupt snapshot quarantine and repair policy completed. |
| SF-016 | Fixed | SFR13 `10985b3`; parse/span quarantine registry and health surfacing completed. |
| SF-017 | Fixed | SFR04 `63badac545b5477667352141e84a4e3871189c65`; daemon close-session unwrap removed. |
| SF-018 | Fixed | SFR04 `63badac545b5477667352141e84a4e3871189c65`; PID ownership and process-safety checks strengthened. |
| SF-019 | False positive | Positive edit-trust behavior retained; no implementation defect was identified. |
| SF-020 | Fixed | SFR17 `51ab5c7`, SFR11 `0c05e43`; byte-exact CRLF/watch-read coverage and snapshot quarantine completed. |
| SF-021 | Fixed | SFR12 `c1bf125`, SFR10 `5b76927`; snapshot mismatch/progress surfacing and repair policy completed. |
| SF-022 | Deferred | SFR15 `38f3a92` added CI/performance-smoke coverage; full 1000-file ignored perf test remains a deliberate manual/nightly-scale concern. |
| SF-023 | Deferred | SFR15 `38f3a92` added CI/performance-smoke coverage; real-repo coupling calibration remains manual/nightly-scale. |
| SF-024 | Fixed | SFR15 `38f3a92`; fmt, clippy, release-build, and quality-gate coverage completed. |
| SF-025 | Fixed | SFR15 `38f3a92`, SFR25 `e0c652cce92eed59837d635b2d986c3fb6067169`; toolchain rationale/gates and Rust raw-reference parser coverage completed. Current project policy accepts Rust 1.95.0 as latest stable. |
| SF-026 | Fixed | SFR18 `b3df776`; Windows launcher executable smoke path completed. |
| SF-027 | Fixed | SFR01 `2b844a5`; resource URI documentation completed. |
| SF-028 | Fixed | SFR01 `2b844a5`; prompt mapping documentation completed. |
| SF-029 | Superseded | SFR19 `70214e7b3ca0dfc663dfe25467747574d29430d4`; compatibility/deprecation lifecycle is now explicit. |
| SF-030 | Fixed | SFR14 `56b1287`, SFR05 `0d95de72b21aa8afb555f71480a4c2c94f2c00fa`, SFR02 `7551932`; result-status coverage completed across targeted read/search/edit outcomes. |
| SF-031 | Fixed | SFR01 `2b844a5`; generation-fence behavior documented as release evidence. |
| SF-032 | Fixed | SFR09 `1c5f896`, SFR12 `c1bf125`, SFR23 `915e4e9bb531804f3a630b42af47dfa3302b648b`; checkpoint/background verify/runtime identity evidence completed. |
| SF-033 | Fixed | SFR17 `51ab5c7`; watcher byte-read regression coverage completed. |
| SF-034 | Fixed | SFR12 `c1bf125`, SFR13 `10985b3`; parse status and compact-health/quarantine visibility completed. |
| SF-035 | Fixed | SFR13 `10985b3`; vendor partial parse hygiene and quarantine surfacing completed without duplicating SFB06. |
| SF-036 | Fixed | SFR01 `2b844a5`, SFR24 `882b33092aae66786d9189f500c6c6cbeb6c6d89`; frecency/search ranking behavior documented and preserved. |
| SF-037 | Fixed | SFR16 `6f52074`; Windows libgit2 lockfile flake mitigation verified. |
| SF-038 | Deferred | Sidecar governor tuning for large repos remains a future performance-calibration item; no release-blocking correctness defect was identified. |
| SF-039 | Fixed | SFR01 `2b844a5`, SFR23 `915e4e9bb531804f3a630b42af47dfa3302b648b`; startup/runtime identity guidance completed. |
| SF-040 | Fixed | SFR03 `b21b24b`, SFR04 `63badac545b5477667352141e84a4e3871189c65`; daemon localhost/auth/PID identity safety completed. |
| SF-041 | Superseded | SFR19 `70214e7b3ca0dfc663dfe25467747574d29430d4`; retired `trace_symbol` allowlist exclusion and daemon alias lifecycle are pinned. |
| SF-042 | Deferred | Historical `tasks/todo.md` repo-hygiene decision remains outside this release closure; no source behavior is affected. |
| SF-043 | Fixed | SFR01 `2b844a5`; analytics privacy stance documented; SFB24-SFB26 remain implementation evidence. |
| SF-044 | Fixed | SFR01 `2b844a5`, SFR24 `882b33092aae66786d9189f500c6c6cbeb6c6d89`; config-intelligence and guidance behavior documented/tuned. |
| SF-045 | Fixed | SFR01 `2b844a5`; inline-doc preservation recognized as SFB08-backed closed evidence. |
| SF-046 | Fixed | SFR06 `0dfd598c36501519772e23e08a5f4ca9e4e50d3f`, SFR08 `c8254953a4d8acb817d4008228c2d4d8014fac37`, SFR21 `7ca22cac0df939f155f5732ea875470ba718eafc`; dry-run semantics preserved while idempotency was added. |
| SF-047 | Fixed | SFR01 `2b844a5`, SFR08 `c8254953a4d8acb817d4008228c2d4d8014fac37`, SFR21 `7ca22cac0df939f155f5732ea875470ba718eafc`; edit tee snapshot behavior documented and preserved. |
| SF-048 | Fixed | SFR01 `2b844a5`; frecency policy documented. |
| SF-049 | Deferred | Co-change store corruption monitoring beyond existing tests remains a future reliability-hardening item. |
| SF-050 | Fixed | SFR01 `2b844a5`, SFR23 `915e4e9bb531804f3a630b42af47dfa3302b648b`; worktree routing and runtime identity evidence documented. |
| SF-051 | False positive | Existing tracing/env-filter posture remains sufficient for local-first MCP; OpenTelemetry is optional future work. |
| SF-052 | False positive | Existing `unsafe_code = deny` discipline with narrow exceptions remains release evidence, not a defect. |
| SF-053 | Deferred | Allowed dead-code helper remains a low-priority hygiene item. |
| SF-054 | False positive | Test-only helper dead-code allowances are isolated and require no action. |
| SF-055 | Deferred | Deprecated `home_dir` cleanup remains future discovery hygiene. |
| SF-056 | Deferred | Patched SCSS vendor crate upstream tracking remains future vendor hygiene. |
| SF-057 | Deferred | `diff.txt` repo-hygiene decision remains future cleanup; no file deletion was authorized in this closure goal. |
| SF-058 | Fixed | SFR05 `0d95de72b21aa8afb555f71480a4c2c94f2c00fa`, SFR14 `56b1287`; ambiguous symbol and public contract corpus coverage completed. |
| SF-059 | Fixed | SFR24 `882b33092aae66786d9189f500c6c6cbeb6c6d89`; `ask` vs `explore` boundaries and guidance behavior documented/tuned. |
| SF-060 | Fixed | SFR00 `4dd4a4a`; operational indexing discipline captured. SFR27 also re-indexed the worktree before closure with SymForge `health_compact` Ready. |

## Release Readiness Summary

Release-readiness status: **Ready for release-candidate packaging after normal operator review of the deferred non-blocking items below.**

Verification evidence already run in the SFR26 worktree immediately before this closure goal:

| Command | Result | Evidence |
|---|---|---|
| `git branch --show-current` | PASS | Returned `goal/sfr26-report-deepest-actionable-validate-file-syntax-diagnostics`. |
| `git diff --check` | PASS | No whitespace errors; Git emitted only the Windows LF-to-CRLF working-copy warning. |
| `cargo fmt --check` | PASS | Formatting clean. |
| `cargo test validate_file_syntax syntax diagnostics -- --test-threads=1` | FAIL, documented pre-existing command-form issue | Cargo accepts only one positional test filter and rejected `syntax`; split equivalents were run. |
| `cargo test validate_file_syntax -- --test-threads=1` | PASS | 3 validate-file-syntax tests passed, including nested ERROR and missing-node diagnostics. |
| `cargo test syntax -- --test-threads=1` | PASS | 5 matching tests passed. |
| `cargo test diagnostics -- --test-threads=1` | PASS | Matching diagnostics tests passed. |
| `cargo check` | PASS | Dev check completed successfully. |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS | No clippy warnings. |
| `cargo test --all-targets -- --test-threads=1` | PASS | 1837 unit tests plus integration suites passed; expected ignored long-running tests remained ignored. |
| `cargo build --release` | PASS | Release build completed successfully. |
| `npm test --prefix npm` | PASS | 28 Node tests passed. |
| post-land `cargo check` on `main` at `2c98649` | PASS | `main` check completed after SFR26 was squash-landed. |

Additional SFR27 branch verification for this closure artifact:

| Command | Result | Evidence |
|---|---|---|
| `git diff --check` | PASS | No whitespace errors; Git emitted only the Windows LF-to-CRLF working-copy warning for this Markdown file. |
| `rg "SF-001|SF-060|Blocked|Deferred|Completed|Superseded|False positive" .agent/goals/symforge-code-review-2026-05-22 -n` | PASS | Broad goal-directory evidence returned matches for completed goal statuses, deferred/superseded/false-positive rows, and SF-001/SF-060 coverage. |
| `git branch --show-current` | PASS | Returned `goal/sfr27-final-review-burndown-audit-and-release-readiness-evidence`. |
| `cargo fmt --check` | PASS | Formatting clean. |
| `cargo check` | PASS | Dev check completed successfully. |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS | No clippy warnings. |
| `cargo test --all-targets -- --test-threads=1` | PASS | 1837 unit tests plus integration suites passed; expected ignored long-running tests remained ignored. |
| `cargo build --release` | PASS | Release build completed successfully. |
| `npm test --prefix npm` | PASS | 28 Node tests passed. |

SFR27 is a documentation/status closure only. Its own task-specific verification for this section is:

```bash
git diff --check
rg "SF-001|SF-060|Blocked|Deferred|Completed|Superseded|False positive" .agent/goals/symforge-code-review-2026-05-22 -n
```

## Residual Risk Register

| Risk | Disposition | Release impact |
|---|---|---|
| SF-022 full 1000-file load performance test remains ignored/manual-scale | Deferred | Not a release blocker after SFR15 CI/perf-smoke gate coverage; run manually/nightly before scale-sensitive releases. |
| SF-023 real-repo coupling calibration remains ignored/manual-scale | Deferred | Not a release blocker after SFR15; run manually/nightly when changing co-change ranking. |
| SF-038 sidecar governor tuning for very large repositories is not calibrated here | Deferred | Not a correctness/security blocker; revisit with benchmark data. |
| SF-042 historical `tasks/todo.md` repo hygiene | Deferred | No runtime impact; cleanup requires explicit owner decision because the file is outside this closure goal's allowed area. |
| SF-049 co-change store corruption monitoring beyond existing tests | Deferred | Existing generation-fence/store tests remain evidence; add targeted corruption tests if future failures appear. |
| SF-053 allowed dead-code helper | Deferred | Low-priority hygiene; no known runtime path affected. |
| SF-055 deprecated `home_dir` discovery cleanup | Deferred | Low-priority cleanup; no current failure evidence. |
| SF-056 patched SCSS vendor crate upstream tracking | Deferred | Vendor maintenance item; current vendor partial parses remain classified as expected noise. |
| SF-057 `diff.txt` repo-hygiene decision | Deferred | No source behavior impact; do not delete without explicit cleanup scope. |
