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
