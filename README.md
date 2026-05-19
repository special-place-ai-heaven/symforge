![SymForge](./symforge-banner_02.png)

A code-native MCP server that gives AI coding agents structured, symbol-aware navigation across your codebase. Built in Rust with tree-sitter, it replaces raw file scanning with tools that understand code as symbols, references, dependency graphs, and git history through a single MCP connection.

Works with MCP-compatible clients including Claude Code, Claude Desktop, Codex, Gemini CLI, VS Code MCP, Kilo Code, Roo Code, Cline, Continue, JetBrains plugins, and custom agents.

> [!IMPORTANT]
> **Rust-native** · **31 tools** · **19 source languages** · **5 config formats** · **6 prompts** · **Built-in resources**
>
> **Use SymForge first** for source-code reads, search, repo orientation, symbol tracing, and structural edits.
> **Use raw file reads** for docs and config when exact wording is the point.
> **Use shell tools** for builds, tests, package managers, Docker, and general system tasks.

## What SymForge provides

SymForge is a local-first code intelligence layer for agents. It keeps an in-memory tree-sitter index of the current workspace, watches the filesystem, and exposes MCP tools that let agents ask targeted questions instead of reading large files or grepping blindly.

- **Repository orientation:** compact repo maps, file outlines, conventions, investigation suggestions, and loaded-context inventory.
- **Code reading:** exact file reads, file outlines with imports and consumers, symbol bodies, symbol context, caller/callee chains, and type dependency context.
- **Search:** symbol search, text search with enclosing symbol context, multi-term and regex search, structural AST search via ast-grep patterns, and ranked file discovery.
- **Impact tracing:** references, dependents, symbol-level diffs, uncommitted/changed-file views, match inspection, and post-edit impact analysis.
- **Structural edits:** symbol-scoped replacement, insertion, deletion, find-and-replace, batch edits, batch inserts, batch renames, and edit planning.
- **Ranking signals:** path matching by default, optional frecency ranking, and optional co-change ranking when a coupling store is available.
- **Runtime observability:** health reports for index state, parser resilience, watcher state, hook adoption, daemon fallback routing, git temporal hotspots, compact capability states, coupling evidence, and worktree-awareness misuse.
- **MCP surfaces:** tools, built-in resources, resource templates, and prompts for review, architecture mapping, failure triage, onboarding, refactoring, and debugging.

### Supported inputs and clients

SymForge parses 19 source languages through tree-sitter: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, PHP, Swift, Perl, Kotlin, Dart, Elixir, HTML, CSS, and SCSS.

It also indexes common project data formats: JSON, TOML, YAML, dotenv/env files, and Markdown.

The installer can configure Claude Code, Claude Desktop, Codex, Gemini CLI, and workspace-local Kilo Code. SymForge also works as a stdio MCP server for compatible clients such as VS Code MCP integrations, Roo Code, Cline, Continue, JetBrains plugins, and custom agents.

## When to use SymForge

Use SymForge when an agent needs to:

- understand a repo without reading large files blindly
- find symbols, call sites, dependencies, and changed code
- search code by AST structure instead of text patterns
- edit code structurally by symbol instead of by raw text
- reindex and inspect impact after edits

Do not expect SymForge to replace normal shell workflows for process execution, runtime debugging, package management, or OS-level tasks.

## Install

**Prerequisite:** Node.js 18+

**Prebuilt binaries:** Windows x64, Linux x64, macOS arm64, macOS x64

```bash
npm install -g symforge
```

This installs the npm wrapper and downloads the platform binary to `~/.symforge/bin/symforge` (or `symforge.exe` on Windows). Set `SYMFORGE_HOME` to override the default home directory.

### Auto-configured clients

During global install, SymForge auto-configures these home-scoped clients if their home directories already exist:

- Claude Code
- Claude Desktop
- Codex
- Gemini CLI

Kilo Code is workspace-local:

```bash
symforge init --client kilo-code
```

Run that from the target project directory. It writes `.kilocode/mcp.json`, `.kilocode/rules/symforge.md`, and `.symforge/` in that workspace.

### Re-run setup manually

```bash
symforge init
symforge init --client claude
symforge init --client claude-desktop
symforge init --client codex
symforge init --client gemini
symforge init --client kilo-code
symforge init --client all
```

After setup, confirm in your client that the SymForge MCP server is connected or ready.

## Tool reference

### Orientation and context

| Tool | Purpose |
|------|---------|
| `health` | Index status, file/symbol counts, watcher state, parse diagnostics, hook-adoption metrics, git temporal status with hotspots and strongest coupling, worktree-awareness misuse |
| `get_repo_map` | Structured overview of the entire repository (auto-adapts detail to token budget) |
| `explore` | Concept-driven exploration with stemmed matching and convention enrichment |
| `ask` | Natural language questions routed to the right tool internally |
| `conventions` | Auto-detect project coding patterns |
| `context_inventory` | See what symbols and files you've already fetched this session |
| `investigation_suggest` | Find gaps in your loaded context |

### Reading code

| Tool | Purpose |
|------|---------|
| `get_file_context` | File outline, imports, consumers — call before reading a source file |
| `get_file_content` | Exact raw text with optional line ranges — for docs, config, or when you need the literal source |
| `get_symbol` | Full source of a function, struct, class, etc. by name (batch mode supported) |
| `get_symbol_context` | Symbol body + callers + callees + type dependencies (supports bundle mode for edit prep) |

### Searching

| Tool | Purpose |
|------|---------|
| `search_symbols` | Find symbols by name, kind, language, path prefix |
| `search_text` | Full-text search with enclosing symbol context. Supports literal, OR-terms, regex, and structural AST patterns (`structural=true`) |
| `search_files` | File path discovery with default fuzzy ranking, `resolve=true` for exact path resolution, `rank_by="path+cochange"` with `anchor_path` for requested coupling-store fusion, and `rank_by="frecency"` for requested frecency ranking. The older `changed_with=path` path remains as deprecated compatibility |

### Tracing impact

| Tool | Purpose |
|------|---------|
| `find_references` | Call sites, imports, type usages, implementations |
| `find_dependents` | File-level dependency graph |
| `get_symbol_context` (with `sections=[...]`) | Multi-hop caller/callee chains for a symbol. `trace_symbol` remains available as a compatibility alias |
| `what_changed` | Files changed since a timestamp, ref, or uncommitted |
| `diff_symbols` | Symbol-level diff between git refs (AST-based for supported languages) |
| `analyze_file_impact` | Re-index a file after editing and report affected dependents |
| `inspect_match` | Deep-dive a search match with full symbol context |

### Editing code

| Tool | Purpose |
|------|---------|
| `edit_plan` | Analyze impact and suggest the right edit tool sequence |
| `replace_symbol_body` | Replace a symbol's entire definition by name |
| `edit_within_symbol` | Scoped find-and-replace within a symbol's range |
| `insert_symbol` | Insert code before or after a named symbol |
| `delete_symbol` | Remove a symbol and its doc comments by name |
| `batch_edit` | Multiple symbol-addressed edits atomically across files |
| `batch_insert` | Insert code before/after multiple symbols across files |
| `batch_rename` | Rename a symbol and update all references project-wide |

### Worktree awareness

All seven edit tools accept an optional `working_directory` parameter pointing at a sibling `git worktree` of the indexed repo. Supplying that parameter is explicit call-time routing consent: SymForge validates the worktree before writing, re-roots the symbol's indexed path onto that worktree, and reports `working_directory`, `wrote_to`, `indexed_path`, and `rerouted` so callers can verify the target. Reads still come from the indexed path; only writes reroute. `SYMFORGE_WORKTREE_AWARE` is an operational policy/default knob: unset allows explicit call-time routing, while false/off/disabled values block requested routing before write.

```json
{
  "path": "src/lib.rs",
  "name": "hello",
  "find": "println!(\"hi\")",
  "replace": "println!(\"hi, world\")",
  "working_directory": "/abs/path/to/sibling/worktree"
}
```

### Ranking signals

`search_files` ranks path matches by default and can opt into additional signals when the caller requests them. SymForge uses a call-time contract for optional ranking capabilities: a requested capability should be applied, prepared with evidence, reported unavailable, or reported disabled by policy. Frecency and co-change ranking follow that contract now; other optional capabilities may still carry transitional gates until their migration tasks land. The variables below are policy/default controls, not the only path for an LLM to request advertised behavior.

`health` and `health_compact` include a compact capability summary so operators can see the current call-time state without turning ranking responses into logs:

```text
Capabilities:
  frecency: ready/session/no-history fallback-used-on-empty
  co-change: preparing/lazy-on-request fallback-used-on-request
  worktree routing: explicit-call enabled
  ranking diagnostics: call-time explain available/default-off
```

The status labels distinguish ready/current, preparing, unavailable, disabled-by-policy, stale, and fallback-used states where they apply. Detailed per-query ranking reasons stay in `search_files(debug_ranking=true)`.

Use `rank_by="frecency"` to request fusion of a per-workspace frecency signal with path matching. Frecency decays on a 7-day half-life, so a file you touched five minutes ago outranks one you hit ten times six months ago. The request uses available current-session or existing persistent frecency history; if there is no useful history, the store is unavailable, or policy disables frecency, the response returns path ranking with explicit capability evidence. Omit `rank_by` for the default path-based order.

```json
{
  "query": "cache",
  "rank_by": "frecency"
}
```

Frecency scores bump on *commitment* tools - every edit tool plus the read tools that imply you're working on a known file (`get_file_context`, `get_file_content`, `get_symbol`, `get_symbol_context`). With `SYMFORGE_FRECENCY` unset, bumps stay in current-process session history. `SYMFORGE_FRECENCY=1` keeps the existing persistent `.symforge/frecency.db` collection. Explicit false/off/disabled values disable collection and requested frecency ranking reports disabled-by-policy evidence. Discovery tools (`search_files`, `search_text`, `search_symbols`) deliberately never bump: searching for a file is not the same as working on it, and a searching-bumps-too policy corrupts rankings via a positive feedback loop. Batch tools dedup bumps per invocation, so editing 20 symbols in one `batch_edit` call bumps each touched path exactly once. Use `debug_ranking=true` on `search_files` to append a compact call-time ranking explanation; `SYMFORGE_DEBUG_RANKING=1` remains an operational default-on/debug knob for search ranking diagnostics and a last-10 bumps list in `health`, while `0`/false/no/off/disabled/disable values suppress requested explanations with disabled-by-policy evidence.

Use `rank_by="path+cochange"` with `anchor_path="<repo-relative-path>"` to request fusion of path matches with the per-workspace co-change coupling store. SymForge now uses ready coupling evidence when it is current for the workspace, starts bounded lazy background preparation when the store is missing or incomplete, or returns path-ranked results with explicit fallback, unavailable, stale, or disabled-by-policy evidence. `SYMFORGE_COUPLING` is an operational policy knob: unset means lazy on request, `1`/truthy values warm the store on startup, and false/off/disabled values block preparation and requested ranking. The older `changed_with=path` mode still works for compatibility and emits a deprecation warning.

```json
{
  "query": "routes",
  "rank_by": "path+cochange",
  "anchor_path": "src/auth/routes.rs"
}
```

### Validation and indexing

| Tool | Purpose |
|------|---------|
| `validate_file_syntax` | Parse diagnostics with line/column location for code and config files |
| `index_folder` | Full reindex of a directory |

### Structural search examples

With `structural=true`, the `search_text` tool uses [ast-grep](https://ast-grep.github.io/) pattern syntax to match code by AST structure rather than text:

```
# Find all functions in Rust
search_text(query="fn $NAME($$$) { $$$ }", structural=true, language="Rust")

# Find all React useState hooks
search_text(query="const [$STATE, $SETTER] = useState($$$)", structural=true, language="TypeScript")

# Find all try-catch blocks in Java
search_text(query="try { $$$ } catch ($E) { $$$ }", structural=true, language="Java")
```

Metavariable syntax: `$NAME` matches a single AST node, `$$$` matches zero or more nodes. Captures are shown in results.

### Practical defaults

- Call `get_file_context` before reading a source file
- Use `search_text` or `search_symbols` before broad grep or raw file scans
- Use `structural=true` when you need pattern matching that respects code structure (ignores comments, whitespace, formatting)
- Use `get_file_content` when exact docs/config text matters
- Run `analyze_file_impact` after small edits; `index_folder` after larger multi-file work
- `edit_plan` accepts a bare symbol, a file path, or `path::symbol`
- `batch_edit` and `batch_insert` accept shorthand strings like `src/lib.rs::helper => delete`
- Use `max_tokens` on any search/navigation tool to control response size — output adapts verbosity automatically

## Agent setup prompt

If your AI agent still falls back to built-in file reads, grep, or text-based edits after SymForge is installed, give it the setup prompt from the wiki:

**[Agent Setup Prompt](https://github.com/special-place-administrator/symforge/wiki/Agent-Setup-Prompt)**

This prompt detects installed clients, configures SymForge for each, updates instruction files, and validates the setup.

## Architecture

SymForge is organized around a tree-sitter index, a set of query layers over that index, and the MCP tool surface. For the full runtime and module map, see [Architecture and How It Works](https://github.com/special-place-administrator/symforge/wiki/Architecture-and-How-It-Works) in the wiki.

### Extension points

Two trait-based registries let feature code plug into the shared edit and ranker paths without amending the handlers themselves.

**`EditHook`** wraps the per-edit lifecycle for the seven edit tools (`replace_symbol_body`, `edit_within_symbol`, `insert_symbol`, `delete_symbol`, `batch_edit`, `batch_insert`, `batch_rename`). Implementations register at startup; the handlers delegate to the registry to resolve the target path before writing and to run bookkeeping after the edit commits. For example, a worktree-aware feature registers a hook that rewrites a symbol's indexed path onto the active worktree before the write lands.

Each of the seven edit tools accepts an optional `working_directory` parameter pointing at a `git worktree` sibling of the indexed repo. Call-time routing treats that parameter as explicit consent: SymForge validates the worktree, reroutes the write there, and includes `working_directory:`, `rerouted:`, `wrote_to:`, and `indexed_path:` lines in the response so callers can verify the target. `SYMFORGE_WORKTREE_AWARE` remains a policy/default knob, not a prerequisite for a supplied `working_directory`. Example:

```json
{
  "path": "src/lib.rs",
  "name": "hello",
  "new_body": "fn hello() { println!(\"hi\"); }",
  "working_directory": "/abs/path/to/sibling/worktree"
}
```

**`RankSignal`** wraps `search_files` scoring contributions. Each signal carries a name, a weight, and a `score()` function, and the ranker combines registered signals into a weighted sum. Path matching, co-change evidence, and frecency use this extension point, so ranking behavior can evolve without rewriting the search handler.

This keeps worktree routing and ranking features isolated from the core tool handlers while still making their call-time behavior explicit in each response.

## Operational notes

- `symforge daemon` is optional if you want a shared index across multiple terminal sessions.
- Index snapshots persist at `.symforge/index.bin` for fast restarts.
- Use `validate_file_syntax` when a config file may be malformed — it reports tree-sitter parse diagnostics with line and column locations.
- PreToolUse hooks auto-suppress when the sidecar is active — no redundant "use SymForge" hints when you're already using it.

## Environment variables

| Variable | Default | Effect |
|----------|---------|--------|
| `SYMFORGE_HOME` | `~/.symforge` | Home directory for the binary and daemon metadata |
| `SYMFORGE_AUTO_INDEX` | `true` | Enables project discovery and startup indexing |
| `SYMFORGE_HOOK_VERBOSE` | unset | Set to `1` for stderr hook diagnostics |
| `SYMFORGE_CB_THRESHOLD` | `0.20` | Parse-failure circuit-breaker threshold |
| `SYMFORGE_RECONCILE_INTERVAL` | `30` | Watcher reconciliation interval in seconds; `0` disables periodic sweeps |
| `SYMFORGE_SIDECAR_BIND` | `127.0.0.1` | Sidecar bind host for local in-process mode |
| `SYMFORGE_DAEMON_BIND` | `127.0.0.1` | Daemon bind host for shared local daemon |
| `SYMFORGE_FRECENCY` | unset | Frecency policy: unset/session keeps collection in current-process memory; `1`/truthy/persistent writes `.symforge/frecency.db`; false/off/disabled blocks collection and requested ranking reports disabled-by-policy evidence |
| `SYMFORGE_DEBUG_RANKING` | unset | Ranking diagnostics policy: unset keeps explanations per-call only; `1`/truthy/default-on enables explanations by default and the last-10 bumps debug section in `health`; `0`/false/no/off/disabled/disable and unknown values block requested explanations with disabled-by-policy evidence |
| `SYMFORGE_COUPLING` | unset | Co-change policy: unset/lazy prepares `.symforge/coupling.db` only when `rank_by="path+cochange"` requests it; `1`/truthy values warm on startup; false/off/disabled blocks preparation and requested ranking |
| `SYMFORGE_WORKTREE_AWARE` | unset | Worktree-routing policy/default knob: unset or truthy allows explicit `working_directory` routing and counts edit calls that omit `working_directory` in `health`; false/off/disabled blocks requested routing before write and suppresses that misuse counter |
| `SYMFORGE_INDEXING_THREAD_STACK_BYTES` | `4194304` (Windows only) | Override the indexing worker-thread stack size. Minimum 3 MiB on Windows; ignored elsewhere |

For platform-specific setup scripts (PowerShell, CMD, bash, zsh), see the wiki:

**[Environment Setup Scripts](https://github.com/special-place-administrator/symforge/wiki/Environment-Setup-Scripts)**

## Deeper reference

- [SymForge Wiki Home](https://github.com/special-place-administrator/symforge/wiki)
- [Architecture and How It Works](https://github.com/special-place-administrator/symforge/wiki/Architecture-and-How-It-Works)
- [Tool Reference](https://github.com/special-place-administrator/symforge/wiki/Tool-Reference)
- [Runtime Model](https://github.com/special-place-administrator/symforge/wiki/Runtime-Model)
- [Supported Languages and Config Formats](https://github.com/special-place-administrator/symforge/wiki/Supported-Languages-and-Config-Formats)
- [Benchmarks and Token Savings](https://github.com/special-place-administrator/symforge/wiki/Benchmarks-and-Token-Savings)

## Build from source

```bash
cargo build --release
cargo test --all-targets -- --test-threads=1
```

The release profile enables LTO and single codegen unit for smaller binaries and better cross-crate optimization. Release builds take longer (~4 min) than dev builds (~15 sec). The Cargo package name is `symforge`.

## License

SymForge is licensed under [PolyForm Noncommercial License 1.0.0](./LICENSE). The official license text is also available from the [PolyForm Project](https://polyformproject.org/licenses/noncommercial/1.0.0/).

You may inspect, study, and use the source code for noncommercial purposes, but commercial use is prohibited unless separately licensed.
