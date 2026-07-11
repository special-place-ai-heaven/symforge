# Canonical SymForge Dogfood Prompt (v1)

Reusable prompt for a full MCP-surface dogfood run by any agent (Grok, Codex,
Claude, Cursor). v1 hardens the historical prompt with **mandatory
self-disclosure of testing artifacts** — the earlier run silently ~doubled the
index with its own `mcps/` directory, which this protocol would have caught
automatically.

---

## Prompt

You are dogfooding the SymForge MCP surface against this repository. Produce
ONE self-contained report that lets a coder with zero prior context
understand, reproduce (exact MCP calls), locate (file:line + verbatim code),
and fix every issue you find. Follow this protocol exactly.

### 1. Baseline capture (before ANYTHING else)

Record, verbatim from tool output:

- `symforge --version` (or `health`) — **start version**.
- `status` / `health`: **canonical root**, **project ID**, file count, symbol
  count, **per-tier counts** (Tier 1 / Tier 2 metadata-only / Tier 3 skipped),
  degraded/failed file lists.
- The daemon/session identity evidence (`status(detail="projects")` when
  multiple projects are open): project IDs, roots, home marker, `last_seen`,
  `ttl_secs`.
- **Clean-checkout delta**: `git status --short` at start. Anything untracked
  that exists before you begin is PRE-EXISTING — list it so it is not blamed
  on your run.

### 2. Artifact self-disclosure (continuous)

- Every file or directory your run CREATES (reports, scratch dirs, configs,
  indexes) must be named in the report the moment it is created, with its
  size and whether it is inside the indexed tree.
- If you call `index_folder` (any form), record the index counts immediately
  before and after, and attribute the delta.
- At the END of the run: repeat the §1 counts. Any growth in files/symbols vs.
  the start MUST be reconciled line-by-line against your disclosed artifacts.
  Unexplained growth is itself a finding (either yours or SymForge's).

### 3. Surface coverage

Exercise all advertised tools: normal paths, parameter variations, filters,
budgets, error cases, browse vs. explicit queries, `code_only`/noise toggles,
and edit surfaces via dry-run/preview ONLY (no writes, no config changes, no
source modifications). Read non-source docs (CLAUDE.md, AGENTS.md,
`docs/*.md`) for conventions.

### 4. Trust-envelope audits

For every read tool result you cite, check the envelope against the body:

- Does `Completeness: full` coexist with visible truncation or missing
  sections? (Any budget cut must downgrade the claim.)
- Does `parse_state` match reality (a Tier-2/metadata-only file must never
  report `parsed`)?
- Does the project evidence (ID/root) match the project you asked about?
- Is any demotion reason honest (`binary`, `size`, `lockfile`, …) — spot-check
  the file's actual bytes when a reason looks wrong.

### 5. Report requirements

- **Start/end version**, canonical root, project ID, counts/tiers at start
  AND end, clean-checkout delta at start AND end.
- Every issue: severity, exact reproducing MCP call(s), observed vs. expected
  output, root-cause location (file:line + verbatim code), concrete fix.
- Every created/indexed artifact (from §2), each marked created-by-run vs.
  pre-existing.
- **Cleanup**: delete every artifact your run created, then show the final
  `git status --short` and index counts proving the tree and index returned
  to baseline (or name exactly what remains and why).
- A "what works well" section — findings without praise skew triage.

### 6. Hygiene rules

- No source/test/config modifications. No `git` writes. No publishes.
- Edit tools only in preview/dry-run form.
- Never leave a daemon, sidecar, or session you started running at the end.
