# SymForge tool smoke test — 2026-07-07 (8.13.0)

Ad-hoc MCP exercise on the symforge repo after pulling `main` @ `acb1953`
(version **8.13.0**). Full-repo `index_folder(E:/project/symforge)` → 691
files, 21178 symbols. Issues below are ordered by severity; tools not
exercised are listed at the end.

## Setup / indexing

### 1. Cold start is empty until `index_folder`

- Repro: first `health` / `status` showed `project_root` unbound and **0**
  indexed files; most read tools fail or return empty until indexing runs.
- Impact: agents that assume SymForge is warm on connect get silent empties.
- Note: expected for local-process mode, but worth a one-line nudge in
  `health_compact` when `index_files=0` (“run `index_folder` on your repo
  root”).

### 2. Partial index breaks repo-relative paths

- Repro: `index_folder(.../src)` indexed 160 files; `get_file_context("src/cli/hook.rs")` → **File not found**; same file worked as `cli/hook.rs`.
- Impact: path conventions in tool descriptions assume repo root; subfolder
  indexing silently shifts the path namespace.
- Fix direction: reject or warn when `index_folder` target ≠ detected git
  root; or always normalize paths against git root even when index scope is
  narrower.

## Discovery / search

### 3. `search_text` hides tests by default

- Repro: `search_text(query="hook_root_guard")` → no matches; `search_text(..., include_tests=true)` or searching `src/` finds hits. Same for Perl fixtures under `tests/fixtures/perl/`.
- Impact: dogfood tests and fixture corpora are invisible unless the caller
  knows the flag.
- Fix direction: when zero hits in production paths but matches exist in
  tests, always emit the existing hint (already done for some queries) and
  mention `include_tests=true` in the tool description’s first paragraph.

### 4. `ask` misroutes conceptual questions to the wrong symbol

- Repro: `ask(query="How does caller_root_guard interact with index_folder retarget?")` routed to **`get_symbol_context(name="index_folder")`**, not `caller_root_guard` or an explore/diff answer.
- Impact: NL entry point sends agents down a misleading rabbit hole.
- Fix direction: prefer `explore` for “how does X interact with Y” when
  multiple tokens match; or require two-symbol routing.

### 5. Middleware / qualified registrations invisible to reference tools

- Repro: `find_references(name="caller_root_guard")` → **0**; `edit_plan` reports **0 call sites**; `search_text` finds 2 files (`handlers.rs` definition, `router.rs` `handlers::caller_root_guard` registration).
- Impact: edit planning under-counts impact for Axum middleware and similar
  value-passing registrations.
- Mitigation today: footer already suggests `search_text` fallback — good.
- Fix direction: index `handlers::symbol` middleware layer registrations as
  references (same class as qualified calls).

### 6. `macro-generated` browse is noisy, not declaration-useful

- Repro: `search_symbols(kind="macro-generated", path_prefix="src/parsing/languages/")` returns four names at the **same line** (`AST_WALK_DEPTH`, `Cell`, `cell`, `std`) — macro-expansion debris, not `define_id_type!`-style declared names.
- Impact: dogfood #3 (macro-generated ProjectId) may be partially addressed
  but browse mode still unusable for “list generated types”.
- Fix direction: filter expansion tokens; keep macro-invocation argument
  names only.

### 7. Perl fixtures: syntax ok, symbols empty, symbol search empty

- Repro: `validate_file_syntax(tests/fixtures/perl/use_parent.pl)` → ok,
  **0 symbols**; `search_symbols(query="parent", path_prefix="tests/fixtures/perl/")` → no symbols; content only visible via `search_text(..., include_tests=true)`.
- Impact: Perl hardening fixtures are second-class for symbol-first workflows.
- Note: may be acceptable for bare `use` lines; worth confirming whether
  subs/packages in richer fixtures index.

### 8. `search_files` weak on fixture paths

- Repro: `search_files(query="perl corpus")` ranked docs/tests driver files
  but not `tests/fixtures/perl/*.pl` paths.
- Impact: file discovery for fixture work still needs `search_text` or known
  paths.

## Session / runtime

### 9. Sidecar dead while hooks report heavy fail-open

- Repro: `health` → `Sidecar: ... state=dead`; same health block shows
  **702 fail-open (no sidecar)** hook outcomes in the session.
- Impact: expected in local-process + Cursor hook mode, but the dead sidecar
  line reads like an error when fail-open is intentional.
- Fix direction: clarify in `health` that local-process mode may not keep a
  sidecar alive; downgrade to informational when hooks are fail-open by design.

### 10. `context_inventory` duplicate paths after retarget

- Repro: after earlier partial `src/` index, inventory listed both
  `src/cli/hook.rs` and `cli/hook.rs` as separate file entries.
- Impact: token accounting double-counts the same file under two roots.
- Fix direction: normalize inventory keys to repo-relative paths after
  rebind.

## Minor / informational

- **`get_file_content` on system paths** — `C:\Windows\System32\kernel32.dll` → clear “outside repository root” message (good). `/etc/passwd` via `get_file_context` → generic “File not found” (acceptable; not indexed).
- **`diff_symbols` on `main`** — default `main...HEAD` with no local commits → “No file changes”; use explicit `base`/`target` refs for history (works: `acb1953~15...HEAD` showed sidecar symbol adds).
- **`get_repo_map(detail=medium)`** — truncates with doctrine footer; use `symforge_retrieve` for full map (documented behavior).
- **`detect_impact`** — returned empty vs `origin/main` (clean tree); JSON payload is verbose but correct.

## Tools exercised successfully

| Tool | Notes |
|------|-------|
| `health`, `health_compact`, `status` | 8.13.0, full surface, admission tiers visible |
| `index_folder` | Full repo bind |
| `get_repo_map` | Language/symbol counts incl. Perl: 26 |
| `get_file_context`, `get_file_content` | Line ranges, trust lines |
| `get_symbol`, `get_symbol_context` | Bodies + callees |
| `search_text`, `search_symbols` | Literal + browse (`kind=fn`) |
| `search_files` | Path ranking |
| `find_references`, `find_dependents` | With helpful zero-ref fallback text |
| `explore` | Concept search (dogfood symbols) |
| `what_changed` | Git temporal |
| `conventions` | Rust project summary |
| `investigation_suggest` | Session gap hints |
| `edit_plan` | Tool sequence (refs under-count, see #5) |
| `diff_symbols` | Git ref diff |
| `analyze_file_impact` | Unchanged file |
| `validate_file_syntax` | Perl fixture |
| `inspect_match` | Enclosing symbol + siblings |
| `detect_impact` | Empty blast radius on clean tree |
| `ask` | Routed (misroute, see #4) |

## Not exercised (no issues filed)

Mutating / write path: `replace_symbol_body`, `edit_within_symbol`,
`insert_symbol`, `delete_symbol`, `batch_*`, `symforge_edit`, `checkpoint_now`.
Compact-only aliases (`symforge`, `symforge_edit` routing) not re-tested.
MCP **resources** and **prompts** not exercised in this pass.

## Environment

- Client: Cursor MCP (`user-symforge`)
- Repo: `E:/project/symforge`
- Index: 691 files, 21178 symbols, watcher active
- Parse quarantine: 5 failed (intentional malformed fixtures), 4 expected
  vendor partial
