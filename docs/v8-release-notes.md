# SymForge v8 Release Notes

## 8.1.x — Feature 007: Intelligence Pattern Ports

Selective code-intelligence UX patterns ported onto the existing LiveIndex + STEL
stack (competitive analysis vs SoulForge). SymForge remains an MCP
code-intelligence server — no terminal-agent stack, no second index. All four
ports ride shared protocol formatters, so stdio and `symforge serve` stay at
parity.

### Added

- **Post-edit impact footer** — every successful structural mutation
  (`replace_symbol_body`, `insert_symbol`, `delete_symbol`, `edit_within_symbol`,
  `batch_edit`, `batch_rename`, `batch_insert`, and the unified `symforge_edit`
  apply path) now ends with a compact, machine-friendly blast-radius suffix:

  ```text
  [impact: 3 dependents · cochanges: src/protocol/format.rs, src/daemon.rs]
  [impact: 0 dependents]
  ```

  Dependent count comes from the reverse-import index
  (`capture_find_dependents_view`); co-change partners come from the git temporal
  index when it is `Ready`. Success-only; the trust envelope is unchanged.

- **Orientation doctrine** — the onboarding and architecture-map MCP prompts and
  the compact repo map / `symforge://repo/map` resource now state: *the map
  orients, the tools prove*, and *absence from the map is not absence from the
  repo — confirm with `search_symbols` / `search_text`*. Truncation is disclosed
  using the existing "Completeness" / "truncated by result cap" vocabulary.

- **Importance-ranked compact map** — the default (`detail=compact`) repo map
  orders its file-bearing entries by `(dependents desc, churn desc, path asc)` and
  annotates high-fan-in files with `path (→N)` (N ≥ 2). The `full` and `tree`
  modes are byte-unchanged.

- **STEL find fusion** — the find intent answers multi-word fuzzy queries by
  fusing path/file ranking (with co-change boost) and symbol/content matching into
  one envelope. No new public tool; the fusion runs on the frecency-safe
  `search_files` / `search_text` surfaces only.

- **Impact intent + edit_plan co-change** — the `impact` intent returns dependents
  and co-change partners in one envelope; `edit_plan` adds a terse
  `Co-change partners: …` line when git temporal data is `Ready` (omitted cleanly
  otherwise).

### Invariants preserved

- **Frecency**: no discovery / find / map / impact path bumps frecency (pinned by
  `*_does_not_bump` tests).
- **Single authoritative index**: no SQLite "Soul Map" / parallel persistent
  index; all data from the in-memory LiveIndex + `GitTemporalIndex`.
- **Embed isolation (G-045)**: `cargo check --no-default-features --features embed`
  stays green.

### Explicitly NOT built (reject list)

SQLite Soul Map as a primary index; grep/glob interception;
`request_tools`/`release_tools` lazy schema loading; terminal-agent features (TUI,
sessions, task router, providers); LLM-generated symbol summaries in the index;
MinHash clone detection (deferred to 8.2+); a hard 10k file cap.

### Spec

`specs/007-intelligence-pattern-ports/` (spec, plan, research, data-model,
contracts, quickstart, tasks).
