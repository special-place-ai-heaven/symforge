# Audit coverage register

Audit of SymForge at `a7a39c85` (binary 11.11.0), 2026-11-09. Inspection only. See [`AUDIT.md`](AUDIT.md) and [`findings.json`](findings.json).

Depth key: **deep** = the auditor or a reviewer read the code and traced behaviour; **sampled** = targeted reads of the lines behind specific claims; **measured** = a command or tool call was run; **not assessed** = no meaningful examination.

## Checks run

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed, 4m 46s |
| `npm --prefix npm run test` | Passed, 0 failures |
| `cargo nextest run --workspace`, first run (machine under load from 17 concurrent agents) | Failed: 2,129 of 4,790 run, 1 failed after retry (`large_rust_file_indexing_is_linear_and_unchanged`), 2,661 not run because of fail-fast |
| `cargo nextest run --workspace`, second run (quiet machine) | Passed: 4,790 run, 4,790 passed, 1 flaky (the same test failed its first try and passed on retry), 2 slow, 5 leaky, 21 skipped; 9m 52s |
| `git status` after all work | Clean except the new files in this folder |
| MCP surface probe over stdio against the installed binary, full and compact surfaces | Completed; raw analysis in `evidence/tool-surface-measurement.txt` |
| 36 paired agent runs (6 tasks, 2 arms; Sonnet x2, Haiku x1) | Completed; all answers graded; `evidence/paired-benchmark-runs.json` |
| Hook adoption log tally (`~/.symforge/hook-adoption.log`) | 33,641 rows, 18,354 `sidecar-error` |

Not run: `cargo build --release`, the criterion bench smoke, the embed-feature test and clippy cells, ignored performance tests, any real-repo calibration. No dependency advisory scan (no checker installed; installing one was out of scope).

## Areas

| Area | Depth | By | Notes |
|---|---|---|---|
| Product mission, README claims | sampled | auditor | README benchmark section read; wiki pages not fetched |
| Existing benchmark evidence | deep (inventory) | Haiku reviewer, key items re-read | Raw provider captures live outside the repo and were not seen |
| Fixed MCP surface cost | measured | Sonnet worker, re-analysed by auditor | Token figures are bytes/4, not a tokenizer count |
| Per-call response overhead | measured, sampled | auditor | `get_repo_map`, `search_text`, `get_file_context`, `get_symbol`, `health`, `status` exercised; the other 30+ tools were not exercised individually |
| Paired agent benchmark | measured | auditor design; Sonnet and Haiku runs | One repo, read-only tasks only; see limits in `AUDIT.md` D.2 |
| Hook client and sidecar routing | deep | Sonnet reviewer; chain re-read by auditor | Which two files are unreadable was not determined |
| Index lifecycle, watcher, snapshots | sampled | Sonnet reviewer | Lock ordering, write starvation, rename handling and snapshot-verify internals not examined |
| Edit safety (atomic write, batch commit) | sampled | Sonnet reviewers | Batch commit judged sound; `physical_root.rs` not audited line by line |
| Structural editing behaviour | not assessed | | No edit tool was executed; the audit was inspection-only. Proposed experiment: run the edit and rename benchmark categories in throwaway worktrees |
| Filesystem boundary and root binding | deep | Sonnet reviewer; two findings re-read by auditor | Zero-hit untracked sweep and `find_references` disk fallback gating not examined |
| HTTP surfaces (daemon, serve, sidecar) | deep | Sonnet reviewer; one finding re-read by auditor | No live request was sent to any listener |
| Secret handling | sampled | Sonnet reviewer; one finding re-read by auditor | Content-rule coverage for specific credential files not examined |
| Parsing and indexing correctness per language | not assessed | | 16 indexed languages; none had extraction accuracy checked. Covered only indirectly by the passing suite |
| Search precision, recall and ranking | not assessed beyond the benchmark tasks | | No relevance evaluation was run |
| Performance: CPU, memory, large repositories | not assessed | | No profiling. Whole-index capture per publication was flagged by a reviewer as a likely cost on very large repos and not measured. Peak memory of the test run was 18.8 GB (suite, not product) |
| Architecture and module structure | measured (counts), sampled | Sonnet reviewer | Embed/server duplication not diffed line by line |
| Tests | measured (suite run), sampled | auditor, Sonnet reviewer | Test quality judged from structure and the pin tests, not from a coverage tool |
| Dependencies | sampled | Sonnet reviewer, auditor | Advisory status unknown; licences not reviewed |
| CI and release workflows | sampled | Sonnet reviewer | Workflow logic not read end to end; release process not exercised |
| Compatibility: Linux, macOS, other filesystems, other MCP clients, HTTP transport | not assessed | | Everything was run on one Windows 11 machine with one client over stdio |
| npm launcher and self-update | not assessed beyond its passing tests | | |
| Embed lane | not assessed | | Only counted and compared structurally |
| Knowledge index, curation, review tools | not assessed | | Only observed through `health` output |
| Resources and prompts | measured (size only) | Sonnet worker | Content not reviewed |

## Verification of delegated conclusions

Re-read by the auditor before acceptance: `get_repo_map` implementation; hook outcome classification and log writer; `/health` fall-through in `endpoint_for`; `manifest_requires_degraded_coverage`; sidecar freshness fence; reconcile retry loop; the code behind SF-SEC-01 to SF-SEC-04; absence of `deny.toml` and of advisory steps in workflows.

Accepted on a reviewer's reading without re-reading every line (marked delegate-reported in the register): session id minting; savings formulas; status literals; module import counts; file and function sizes; workflow sizes; the P3 security and reliability items.

## Known weaknesses of this audit

- The benchmark sample is small and single-repo; it supports direction per task type, not a general percentage.
- The root cause of the hook failures is confirmed for the live state observed. That it also explains the historical failure rate is inferred.
- SF-SEC-01 was not exercised.
- Wall-clock comparisons are unreliable because runs overlapped.
- A stop-hook in the audit environment altered some agent runs after their answers; metrics were cut at the answer message.
