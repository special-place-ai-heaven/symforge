# Dogfood findings — 2026-07-06 (two concurrent sessions)

Two independent Claude sessions used SymForge 8.11.1 for real work on the same
day: one on the AAP repo (external, large Rust workspace), one on the symforge
repo itself (this session: surface flip, Perl fixtures, tips overhaul). Both
hit real defects. Findings are ordered by severity; each is reproducible.

## From the AAP session (external repo, real refactor task)

### 1. SILENT Tier-2 exclusion breaks reference completeness — trust-critical

- Repro: `find_references(name="PortRegistry")` returned 3 references in
  2 files with trust line `parsed | full for current scope`. Raw grep found
  the only **construction site** in a 1.2 MB first-party file — excluded
  because >1 MB puts it at Tier-2 (metadata only), so it was never
  reference-scanned.
- Impact: a would-be compile-breaking miss (adding a struct field). The trust
  envelope claimed completeness while a first-party source file was excluded.
  **This is the only finding that actively lies.**
- Fix directions: (a) trust envelope must enumerate Tier-2/excluded
  first-party files whenever they contain the queried token — even one line:
  `1 Tier-2 file matches textually and was not reference-scanned: <path>`;
  (b) on-demand symbol-indexing of a Tier-2 file when directly named by a
  query, or a higher tier cutoff — 1 MB load-bearing Rust files exist.

### 2. get_symbol cross-language ambiguity resolves silently

- Repro: `get_symbol(name="ProjectId")` (no path) returned the frontend
  TypeScript alias with no notice that a Rust candidate existed.
- Fix: return the ambiguity list, or footnote `also matched: <lang> <path>`.

### 3. Macro-generated symbols invisible to search_symbols

- Repro: `search_symbols(query="ProjectId", language="Rust")` → no symbols,
  although `define_id_type!(ProjectId)` generates a `pub struct` used
  workspace-wide. Text fallback listed usage files (saving grace).
- Fix (cheap heuristic): index `macro_invocation(ident)` argument tokens as
  declaration-ish symbols, kind `macro-generated`, trust-flagged.

### 4. Symbol-scoped editing lacks sub-symbol granularity

- Scenario: change an identical line in 2 of 3 match arms of a 334-line fn.
  `edit_within_symbol` scopes to the whole fn → `old_text` ambiguous at
  exactly the granularity the tool offers. On the Tier-2 file, symbol
  editing was unavailable entirely.
- Fix ideas: `occurrence: N` / `near_line:` disambiguators; match arms and
  impl items as addressable sub-ranges; on-demand parse for Tier-2 edits.

### 5. Minor

- `search_text(query="pub type ProjectId")` → zero hits, no hint that the
  declaration doesn't exist textually (macro-generated). Combined with #3,
  name-lookup AND text-lookup both dead-end; only `regex=true` on the bare
  name found the trail.
- Hook-injected prompt-context on plain-English prompts burns ~800–1000
  tokens on heuristic no-reference reports (see #8 below — reproduced).

## From the symforge session (this repo, surface/Perl/tips work)

### 6. Shared-daemon project retarget hijacks every other session

- Repro: a benchmark subagent ran `index_folder` on a scratchpad clone
  (mojo). The daemon's ACTIVE project switched for all sessions sharing it:
  the main session's PostToolUse hooks began reporting
  `not found on disk — no index record remains` for every repo file it
  edited, `status` showed `project_root: .../scratchpad/mojo`, and
  `search_text(projects=["*"])` found nothing that existed in the repo.
- Impact: silent, confusing, and long-lived — every hook readout was a false
  alarm until a manual re-`index_folder`. Feature 012's additive
  `add:true` exists, but nothing warns when a plain retarget happens under
  a session that didn't request it.
- Fix directions: per-session active project; or hook/facade queries pinned
  to the session's workspace root; at minimum a loud line in every hook
  readout when the daemon root ≠ the session's cwd repo.

### 7. External edits EVICT files from the index instead of re-indexing

- Repro: after Edit-tool (non-symforge) writes to `src/protocol/tools.rs`,
  the file vanished from the index: `search_text` no longer matched content
  that was on disk, and `edit_within_symbol` failed with
  `File not found: src/protocol/tools.rs` while `get_file_content` (disk
  fallback) still worked. `analyze_file_impact` restored it; symbol edits
  then succeeded.
- Suspect: daemon root is UNC-prefixed (`//?/C:/...`); watcher events for
  externally-written files may arrive non-UNC and fail the repo-bound path
  match → treated as outside-repo → evicted. Same signature as the false
  hook messages in #6 (observed both before AND after re-rooting).
- Fix directions: canonicalize both sides before comparing; on watcher event
  for a known indexed path, always re-read rather than evict.

### 8. Hook prompt-context noise on conversational prompts (confirms 5b)

- Repro (this session): prompt "pull latest from git" → heuristic
  `surface`-token report (~1000 tokens); PostToolUse Grep hooks emitted
  ~1000-token "No references found in the index" reports for regex patterns
  (`Tip:`, `compact|SYMFORGE_SURFACE`) that are not symbols.
- Fix: suppress injection when the matched "symbol token" is a conversational
  word or a regex/multi-token pattern; a no-evidence report should be 1 line.

## Status

- #1 is the priority: it violates the trust-envelope contract (envelope says
  full; scan wasn't). Candidate spec: extend the completeness label with a
  Tier-2 textual-match sweep.
- #6/#7 degrade the daemon-shared workflow that agent teams actually use.
- #2/#3/#4/#5/#8 are ergonomics/recall gaps, real but honest failures.

## Fix log

Wave 1 (trust honesty, branch `fix/trust-envelope-tier2`, 2026-07-06):

- **#1 FIXED** — `find_references` now runs a bounded Tier-2 textual sweep
  (`tier2_reference_disclosure`, tools.rs): size-demoted (>1MB) files
  containing the queried name are named in the output, the completeness label
  gains "Tier-2 exclusions apply", and sweep-budget overruns are reported as
  unswept rather than silently skipped. Tests:
  `test_find_references_discloses_tier2_textual_match` (real 1.1MB fixture,
  zero indexed refs — the exact field-failure shape) and the
  no-match-stays-silent guard.
- **#2 ROOT-CAUSED + MITIGATED** — the "silent wrong-language pick" was not a
  resolution bug: `resolve_symbol_path_by_name` already returns the ambiguity
  list for >1 exact hits. The AAP pick was Unique *because the Rust candidate
  was macro-generated and absent from the index* — #2 is a symptom of #3.
  Mitigation shipped: bare-name `get_symbol` output now opens with
  `Resolved bare name to <path>` so the picked file/language is always
  visible. Full fix rides #3.
- **#5a FIXED** — zero-hit `search_text` with a multi-word literal now
  suggests retrying the bare identifier (declarations may be macro-generated
  or formatted differently).
- Live corroboration during the fix work: `src/protocol/tools.rs` itself
  crossed 1MB and was repeatedly demoted to Tier-2 mid-session (#1's cutoff
  hitting the repo's own load-bearing file), and every external (non-symforge)
  edit to it triggered the #7 eviction until `analyze_file_impact`.

Wave 2 (daemon correctness, PR #418, merged 2026-07-06):

- **#7 FIXED (root-caused)** — not a UNC path bug: the impact path
  (`analyze_file_impact` → `process_file` + `update_file`) force-admitted any
  file, bypassing the admission gate the bulk walk and watcher both apply, so
  files flapped admit/demote until evicted. `impact_admission_refusal`
  (sidecar/handlers.rs) now runs `classify_admission` before impact and
  records demotions in the skip registry. Companion fix: code languages get a
  4MB `METADATA_ONLY_CODE_BYTES` threshold (data formats keep 1MB), so
  first-party code just over 1MB — the AAP field failure and symforge's own
  tools.rs — stays Tier-1. Tests: `tests/impact_admission.rs`.
- **#6 MITIGATED (cheap half)** — edit-impact NotFound messages now name the
  daemon root ("not found under <root> — … this daemon is rooted at a
  different project than your session"), so a hijacked daemon is diagnosable.
  The full fix (per-session active project) needs a written spec first —
  wave 4.

Wave 3 (ergonomics/noise, branch `fix/hook-noise-edit-disambiguators`,
2026-07-06):

- **#8 FIXED** (and #5b) — three noise sources cut: (1) the Grep PostToolUse
  hook only forwards bare-identifier patterns to `/symbol-context`; regexes
  and multi-token phrases (`Tip:`, `compact|SYMFORGE_SURFACE`) fail open
  instead of buying a guaranteed zero-hit report; (2) the bare-token heuristic
  prompt hint is now a one-line pointer (was a full ~1000-token symbol
  context); (3) the no-evidence prompt report and the sidecar zero-reference
  report are each one line.
- **#4 FIXED** — `edit_within_symbol` gains `occurrence: N` (1-based) and
  `near_line: L` disambiguators for old_text that appears several times
  within one symbol (the match-arm case); targeting modes are mutually
  exclusive; an untargeted multi-match edit now discloses "occurred N times;
  edited the FIRST" instead of silently rewriting the first match.

Wave 4 (recall + shared-state routing, branch
`feat/macro-generated-symbols`, 2026-07-06):

- **#3 FIXED** — module-level Rust `macro_invocation` identifier arguments are
  indexed as symbols with the new trust-flagged kind `macro-generated`
  (`define_id_type!(ProjectId)` → symbol `ProjectId`, kind label
  `macro-generated`, anchored to the declaring invocation so `get_symbol`
  returns the real line). Function-body macro calls never produce symbols;
  names are deduplicated and capped at 8 per invocation to bound block-macro
  pollution. The kind label IS the trust flag: the index has the declared
  NAME, not the compile-time-synthesized body.
- **#2 CLOSED (via #3)** — with the Rust candidate now indexed, bare-name
  lookups that previously resolved "Unique" to the wrong language now see
  both candidates and return the ambiguity list (plus Wave 1's resolved-path
  echo).
- **#6 full fix ROUTED TO SPEC 012** — the per-session active project is
  FR-006/-007/-008 of `specs/012-harness-agnostic-mcp/spec.md`; added
  **FR-006b** (retarget MUST be connection-scoped; one session's
  `index_folder` must never swap another session's binding) with the dogfood
  repro as field evidence. The council's objection stands recorded: the
  Wave 2 root-naming warning is a bandage, not the fix; the ticket closes
  only when FR-006b ships.

Wave 5 (session-root guard + system-path hardening, branch
`fix/session-root-guard-and-system-dirs`, 2026-07-06):

- **#6 HOOK HALF FIXED (FR-006b partial)** — hooks now pin every sidecar
  request with `caller_root` (their canonicalized cwd); a sidecar middleware
  compares it against the CURRENT index's `indexed_root` and answers 409 on
  mismatch. The hook's existing error path then falls back to the daemon,
  which resolves the project BY ROOT (`try_daemon_fallback`) — so a session
  retargeted by another agent's `index_folder` produces correct answers or an
  honest fail-open, never false "not found" alarms. `/health` and `/stats`
  stay root-agnostic. Tests: `tests/hook_root_guard.rs` (409 on mismatch,
  pass on match, back-compat without the param, health exemption). The full
  per-session binding (spec 012 FR-006b proper) remains open.
- **NEW: system-path guard ordering** (field report, same day) — `index_folder`
  and `open_project_session` ran `exists()`/`canonicalize()` BEFORE the
  sensitive-path guard, so a protected tree (`C:\Windows\System32\...`) could
  surface a raw OS access error instead of the refusal. All three entry points
  (local tool, daemon session index, daemon session open) now refuse on the
  RAW input first, keeping the post-canonicalize check as the symlink belt.
  Tests: `tests/system_path_refusal.rs` (System32/Windows/Program Files/
  ProgramData/drive root, unresolvable system subpaths, forward-slash forms).
