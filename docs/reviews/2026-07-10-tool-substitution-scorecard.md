# Tool-Substitution Scorecard — SymForge vs. raw repository tools

**Template created:** 2026-07-11 (018 hardening, plan Task 12 Step 2).
**Rows are filled by the Task 13 controlled runs** (pinned commit, release
binary, isolated `SYMFORGE_HOME`). Until a row carries run evidence it is
`UNFILLED` — an unfilled row is not a claim.

## Method

Each row compares completing ONE fixed workflow with SymForge tools vs. raw
repository tools (Read/Grep/Glob/git), on the same pinned commit.

Recorded per row:

- **Raw context tokens**: tokens of file/search output a raw-tool agent must
  ingest to reach the answer (measured, not estimated, from the actual raw
  transcript).
- **SymForge response tokens**: total tokens of all SymForge responses used.
- **Facts required / retained**: the enumerated facts the workflow needs, and
  how many the SymForge path actually surfaced (a CCR footer or truncation
  that drops a required fact counts as NOT retained — savings claims require
  retained-answer proof).
- **Project/freshness evidence**: whether the responses carried project
  identity + freshness evidence (project ID/root, generation, last_seen/TTL).
- **Forced-failure recovery**: inject one failure (wrong path, stale index,
  foreign project selector) — record whether the error was actionable
  (named the correction) or dead-ended.
- **Auxiliary raw tool needed?**: yes/no — did the SymForge path have to fall
  back to any raw repository tool to finish the workflow.

Include unfavorable cases honestly; a scorecard with only wins is marketing,
not measurement.

## Rows

| # | Workflow | Raw ctx tokens | SymForge tokens | Facts req/ret | Evidence | Forced-failure recovery | Aux raw tool? |
|---|----------|----------------|-----------------|---------------|----------|-------------------------|---------------|
| 1 | File discovery: locate the file defining a named subsystem (`search_files` vs `Glob`+`Grep`) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |
| 2 | Targeted read: exact body of one known function (`get_symbol` vs `Read` window) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |
| 3 | Text search: all uses of a string constant (`search_text` vs `Grep`) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |
| 4 | Symbol search: find a type by partial name (`search_symbols` vs `Grep`) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |
| 5 | Caller tracing: every caller of one function (`find_references` vs `Grep` sweep) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |
| 6 | Dependent tracing: files importing one module (`find_dependents` vs `Grep`) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |
| 7 | Change inspection: what changed since HEAD~1 and what it impacts (`what_changed`+`detect_impact` vs `git diff`+manual) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |
| 8 | Structural edit preview: rename plan across call sites (`edit_plan`/`batch_rename` dry-run vs manual grep+patch plan) | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED | UNFILLED |

## Verdict (after fill)

UNFILLED — written by Task 13 after all rows carry evidence: per-row winner,
aggregate token ratio, and every case where the raw tool won or SymForge
needed an auxiliary raw tool.
