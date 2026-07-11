# Tool-Substitution Scorecard — SymForge vs. raw repository tools

**Template created:** 2026-07-11 (018 hardening, plan Task 12 Step 2).
**Filled:** 2026-07-11 (plan Task 13) from controlled runs — release binary
built from `6f6eac6`, local in-process index (`SYMFORGE_NO_DAEMON=1`), this
repository as the corpus, fresh `index_folder` forced before measurement.
Harness: stdio JSON-RPC (same plumbing as `scripts/verify-tools.cjs`); token
figures are `bytes/4` of the exact transcripts, not estimates of estimates.

## Method

Each row compares completing ONE fixed workflow with SymForge tools vs. raw
repository tools (ripgrep/git/windowed reads), on the same pinned commit.

- **Raw context tokens**: measured bytes/4 of the raw-tool output an agent
  must ingest (e.g. row 1 charges the full `rg --files` listing the agent
  filters; row 2 charges the `rg -n` hit list plus a ±30-line read window;
  row 8 charges the call-site list plus ±5-line windows).
- **SymForge tokens**: bytes/4 of the tool response(s) used.
- **Facts required/retained**: enumerated facts the workflow needs vs. facts
  actually present in the SymForge response (file-level ground truth from the
  rg oracle).
- **Evidence**: whether the response carried a trust envelope
  (`Trust:`/`Source authority:` block).
- **Forced-failure recovery**: a nonexistent symbol was requested
  (`get_symbol`, `find_references`); "actionable" means the error named close
  matches or the corrective tool.
- **Aux raw tool?**: whether the SymForge path needed any raw repository tool
  to finish the workflow.

## Rows (measured 2026-07-11)

| # | Workflow | Raw ctx tokens | SymForge tokens | Facts req/ret | Evidence | Forced-failure recovery | Aux raw tool? |
|---|----------|----------------|-----------------|---------------|----------|-------------------------|---------------|
| 1 | File discovery: locate `port_file` module (`search_files` vs `rg --files`+filter) | 7451 | 90 | 1/1 | yes (trust envelope) | n/a | no |
| 2 | Targeted read: one fn body (`get_symbol` vs `rg -n` + ±30-line window) | 734 | 100 | 1/1 | **no envelope** | actionable (close matches + corrective hint) | no |
| 3 | Text search: const usage (`search_text` vs `rg -n`) | 99 | 221 | 2/2 | yes (trust envelope) | n/a | no |
| 4 | Symbol search: partial type name (`search_symbols` vs `rg -n`) | 510 | 194 | 1/1 | yes (trust envelope) | n/a | no |
| 5 | Caller tracing (`find_references compact` vs `rg -n`) | 118 | 125 | 1/1 | yes (trust envelope) | actionable | no |
| 6 | Dependent tracing (`find_dependents` vs `rg -n`) | 1062 | 572 | 1/1 | **no envelope** | n/a | no |
| 7 | Change inspection: HEAD~1 (`what_changed` vs `git diff --stat`+`git diff`) | 2320 | 82 | 1/1 | yes (trust envelope) | n/a | no |
| 8 | Structural edit preview (`edit_plan` vs call-site sweep + windows) | 367 | 231 | 1/1 | **no envelope** | n/a | no |

## Verdict

- **Aggregate:** 12,661 raw tokens vs. 1,615 SymForge tokens across the eight
  workflows (~7.8× cheaper), with every required fact retained (no CCR footer
  was involved in any row — savings are retained-answer savings).
- **Where raw tools win, honestly:** narrow single-string queries. Row 3
  (`rg -n` on a rare constant: 99 vs. 221) and row 5 (118 vs. 125) — a plain
  ripgrep line list is already near-minimal, and SymForge's envelope/grouping
  overhead costs more than it saves there. SymForge's value concentrates
  where raw workflows must ingest large listings/diffs/windows (rows 1, 2, 6,
  7: 4.5×–83×).
- **Evidence gap found:** `get_symbol`, `find_dependents`, and `edit_plan`
  responses carry no trust envelope (rows 2, 6, 8) while the search/context
  tools do. Recorded as a follow-up, not fixed in this campaign.
- **Forced failures** were actionable in both probed rows (close-match list +
  corrective tool hint).
- **No workflow needed an auxiliary raw repository tool.**
- **Measurement caveat (itself a finding):** a fresh local session against a
  repo carrying a stale `.symforge` snapshot initially served STALE index
  content (missing campaign-era symbols; `src/daemon.rs` reported 210 symbols
  vs. current 314) until an explicit `index_folder` re-index. The harness now
  forces the re-index; the staleness behavior is logged in the campaign
  review (`tasks/todo.md`).
