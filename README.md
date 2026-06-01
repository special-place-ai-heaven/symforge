![SymForge](./symforge-banner_02.png)

SymForge is a local-first MCP server for AI coding agents. It gives an agent a
fast, symbol-aware view of a repository so it can ask precise questions instead
of reading whole files, running broad grep commands, or editing code with blind
text replacement.

It is written in Rust, indexes code with tree-sitter, keeps the active workspace
in memory, and exposes 32 canonical MCP tools plus resources and prompts for
repo orientation, code reading, search, reference tracing, impact analysis, and
structural edits.

> [!IMPORTANT]
> SymForge is for code intelligence and code editing.
>
> Use it before raw file reads, broad text search, or manual string edits when
> the task is about source code. Use shell commands for builds, tests, package
> managers, Docker, and process/runtime work. Use exact file reads when literal
> docs or config text is the thing being inspected.

## Features

- **Live repository index:** Builds and maintains an in-memory index of source
  files, symbols, references, file contents, and git-derived ranking signals so
  agents can query the codebase without repeatedly scanning the filesystem.
- **Symbol-aware reading:** Lets agents inspect file outlines, imports,
  consumers, exact source excerpts, full symbol bodies, and symbol context before
  deciding whether they need a raw file read.
- **Search and exploration:** Searches symbols, text, file paths, natural
  language concepts, and AST-shaped patterns with bounded output and ranking
  reasons.
- **Reference and impact tracing:** Finds call sites, imports, type usages,
  implementations, file dependents, symbol diffs, and changed files so agents
  can understand blast radius before editing.
- **Structural editing:** Replaces, inserts, deletes, batch-edits, and renames
  symbols by indexed structure instead of blind string replacement, then reports
  edit status and affected paths.
- **Safe retry semantics:** Supports optional idempotency keys for indexing and
  structural edit mutations. Replaying the same key with the same canonical
  request returns the stored result; reusing the key for a different request
  fails deterministically.
- **Snapshot and recovery safeguards:** Writes byte-exact index snapshots through
  explicit checkpoints, verifies snapshots when requested, and quarantines
  corrupt or version-incompatible snapshots instead of silently serving them.
- **Malformed-file diagnostics:** Isolates parser failures to the affected file
  and exposes `validate_file_syntax` for line-and-column diagnostics when source
  or config files are malformed.
- **Local daemon mode:** Can run a shared local daemon for multiple agent
  sessions while keeping the query path local-first and workspace-aware.
- **Resources and prompts:** Exposes MCP resources for repo health, outlines,
  maps, changes, file context, file content, and symbol context, plus prompts for
  review, architecture, triage, onboarding, refactoring, and debugging.
- **Local analytics:** Optionally records bounded, local-only tool-call metadata
  in SQLite so operators can inspect usage without exporting source code.
- **npm binary distribution:** Installs as an npm package with a JavaScript
  launcher plus a platform-specific optional package. It does not run a
  postinstall downloader or bootstrap client configs during install.

## How It Works

```mermaid
flowchart LR
    Client["MCP client<br/>Codex, Claude, Gemini, Kilo, etc."] --> Server["symforge stdio MCP server"]

    Server --> Startup["startup planner"]
    Startup -->|local session| Local["in-process LiveIndex"]
    Startup -->|shared sessions| Daemon["optional local daemon"]
    Daemon --> Local

    Workspace["workspace files"] --> Parser["tree-sitter parsers<br/>config extractors"]
    Parser --> Local
    Watcher["filesystem watcher"] --> Local
    Git["git status, diffs, history"] --> Signals["frecency, co-change,<br/>temporal hotspots"]
    Signals --> Local

    Local --> Snapshot[".symforge/index.bin"]
    Snapshot --> Local

    Local --> Tools["32 MCP tools<br/>resources + prompts"]
    Tools --> Client

    Tools --> Edits["structural edit engine"]
    Edits --> Workspace
    Edits --> Impact["analyze_file_impact"]
    Impact --> Local

    Tools --> Analytics["optional analytics queue"]
    Analytics --> AnalyticsDb[".symforge/analytics.db"]
```

The read path is intentionally local. SymForge serves queries from an in-process
index whenever possible, because symbol spans depend on the exact bytes in the
current workspace and agents need low-latency answers.

## Supported Inputs

SymForge parses 19 source languages:

Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, PHP, Swift,
Perl, Kotlin, Dart, Elixir, HTML, CSS, and SCSS.

It also indexes common project formats:

- JSON
- TOML
- YAML
- dotenv/env files
- Markdown
- GitHub Actions workflow YAML facts, including workflow names, triggers,
  permissions, env keys, jobs, needs, runners, matrix strategy, and step fields

Malformed files are isolated. A bad parse can degrade that file, but it should
not poison the whole run. Use `validate_file_syntax` for parser diagnostics with
line and column locations when a config or source file looks malformed.

## Install

Prerequisite: Node.js 18+ and npm.

```bash
npm install -g symforge
```

The npm package installs a JavaScript launcher plus a platform-specific optional
dependency that carries the native binary. The binary also runs as the daemon,
so there is no separate daemon to install. npm automatically selects the correct
platform package for your OS and CPU. There is no postinstall step: install does
not download anything, stop processes, or auto-configure MCP clients.

The command above works on every platform, with one requirement: `symforge` must
install into the npm global prefix that belongs to the OS you are running, and
that prefix's `bin` directory must be on your `PATH`. Confirm the install with:

```bash
symforge --version    # prints the installed version
```

### Windows

```powershell
npm install -g symforge
```

Run it from PowerShell or Windows Terminal. npm's default global prefix
(`%APPDATA%\npm`) is already on `PATH`, so no extra setup is needed.

### macOS and Linux

```bash
npm install -g symforge
```

If `npm install -g` fails with a permissions error, do not use `sudo`. Point npm
at a user-writable prefix once, then reinstall:

```bash
npm config set prefix "$HOME/.npm-global"
export PATH="$HOME/.npm-global/bin:$PATH"   # add to ~/.profile or ~/.zshrc to persist
npm install -g symforge
```

### WSL (Windows Subsystem for Linux)

WSL is Linux and needs the Linux build, but a WSL shell often inherits the
Windows `PATH` and a shared Windows npm prefix (for example a
`C:\Users\<you>\.npmrc` containing `prefix=C:\Users\<you>\.npm-global`). When
that happens, `npm install -g symforge` lands in the Windows prefix and pulls the
Windows binary, which cannot run under Linux — the launcher then reports a
missing `symforge-linux-x64` package.

Give WSL its own Linux npm prefix, then install:

```bash
npm config set prefix "$HOME/.npm-global"
export PATH="$HOME/.npm-global/bin:$PATH"    # ahead of any /mnt/* entries; add to ~/.profile to persist
hash -r
npm install -g symforge
which symforge        # expect /home/<you>/.npm-global/bin/symforge, not /mnt/c/...
symforge --version
```

### Update

Update the npm-managed install explicitly:

```bash
symforge update
```

This runs the same package-manager path as:

```bash
npm install -g symforge@latest
```

`symforge --version` prints the installed version and, when npm can be reached
quickly, reports when a newer npm release is available.

Prebuilt native binaries are produced for:

- Windows x64
- Linux x64
- macOS arm64 (Apple Silicon)
- macOS x64 (Intel)

## Configure A Client

`npm install -g symforge` only installs the launcher and native platform
package. Configure MCP clients explicitly after install or update:

- Claude Code
- Claude Desktop
- Codex
- Gemini CLI

You can rerun setup manually:

```bash
symforge init
symforge init --client claude
symforge init --client claude-desktop
symforge init --client codex
symforge init --client gemini
symforge init --client all
```

Cursor and other desktop harnesses that do not have a SymForge-specific init
target should use their global MCP configuration and point the command at the
installed `symforge` binary. Do not rely on npm install hooks to mutate editor
configuration.

Kilo Code is workspace-local. Run this from the repository you want to use:

```bash
symforge init --client kilo-code
```

That writes workspace-local MCP configuration under `.kilocode/` and `.symforge/`.

## CLI

```bash
symforge --help
symforge init --help
symforge daemon --help
symforge analytics --help
symforge update
```

Top-level commands:

| Command | Purpose |
|---|---|
| `init` | Install MCP client configuration for supported clients |
| `daemon` | Run a shared local daemon for multiple sessions |
| `hook` | Hook subcommands used by Claude Code and compatible workflows |
| `trust` | Trust-control commands for project-local SymForge configuration |
| `analytics` | Inspect, summarize, export, or reset local analytics storage |
| `update` | Explicitly update the npm-managed global install |

Analytics subcommands:

| Command | Purpose |
|---|---|
| `analytics status` | Show whether local analytics storage exists and can be read |
| `analytics summary` | Summarize local analytics records without exporting event rows |
| `analytics export` | Export recent bounded, redacted JSON rows |
| `analytics reset` | Delete only the local analytics database and SQLite sidecar files |

## MCP Tools

SymForge exposes 32 canonical tools through MCP `tools/list`. They are grouped
by how an agent should use them.

### Orient

| Tool | Use |
|---|---|
| `health` | Check index health, watcher state, parse resilience, runtime identity, sidecar state, and capability state |
| `health_compact` | Smaller health summary for prompt budgets |
| `get_repo_map` | Get a bounded repository map |
| `explore` | Explore a broad concept across symbols, files, and patterns with noise filtering and ranking reasons |
| `ask` | Ask a natural-language codebase question and see route confidence, rationale, and the selected invocation |
| `conventions` | Detect local coding and test conventions |
| `context_inventory` | See what context has already been loaded |
| `investigation_suggest` | Find likely gaps in the current investigation |

`ask` is a routing envelope for natural-language questions. It reports the
chosen tool, route confidence, invocation, and rationale before the routed
result so callers can see why the request did or did not become a narrow symbol
or reference lookup.

`explore` is the broad concept-discovery tool. It ranks by concept match,
symbol-token alignment, path proximity, and caller density, and it hides
vendor, generated, test, and personal-tooling noise by default.

### Read

| Tool | Use |
|---|---|
| `get_file_context` | Start here for source files. Returns outline, imports, references, consumers, and git activity |
| `get_file_content` | Exact raw file content, line ranges, chunks, match excerpts, or symbol excerpts |
| `get_symbol` | Full source for one or more symbols |
| `get_symbol_context` | Symbol body plus callers, callees, type dependencies, and edit-prep context |
| `inspect_match` | Deep-dive one search match with enclosing symbol context |

### Search

| Tool | Use |
|---|---|
| `search_symbols` | Find functions, structs, classes, methods, types, modules, and other symbols |
| `search_text` | Search text with enclosing symbol context; supports literal terms, OR terms, regex, and AST structural search |
| `search_files` | Find and rank paths, resolve ambiguous paths, and optionally use frecency or co-change ranking |

### Trace Impact

| Tool | Use |
|---|---|
| `find_references` | Find call sites, imports, type usages, implementations, and qualified usages |
| `find_dependents` | Show file-level dependency relationships |
| `what_changed` | Show changed files since a ref, timestamp, or current uncommitted state |
| `diff_symbols` | Compare symbols between git refs |
| `analyze_file_impact` | Reindex a changed file and report affected dependents |
| `validate_file_syntax` | Report parser diagnostics with line and column locations |

### Edit

| Tool | Use |
|---|---|
| `edit_plan` | Inspect impact and choose the right edit tool before modifying code |
| `replace_symbol_body` | Replace a function, class, struct, method, or similar symbol body |
| `edit_within_symbol` | Perform scoped find/replace inside one symbol |
| `insert_symbol` | Insert code before or after a named symbol |
| `delete_symbol` | Delete a symbol and its attached docs |
| `batch_edit` | Apply multiple symbol-scoped edits atomically |
| `batch_insert` | Insert before or after multiple symbols |
| `batch_rename` | Rename a symbol and update references project-wide |

### Indexing

| Tool | Use |
|---|---|
| `index_folder` | Reindex a repository from scratch, with optional idempotency for safe retries |
| `checkpoint_now` | Atomically write the current in-memory index to `.symforge/index.bin`, optionally verifying after write |

### Recovery

SymForge does not expose placeholder v1 run-lifecycle tools. `repair_index` is
intentionally retired until a real repair workflow has durable state and
machine-readable status. `get_index_run` and `cancel_index_run` remain retired.
No durable run IDs are exposed.

Current replacement workflow:

1. Run `checkpoint_now(verify_after_write=true)` to force a byte-exact snapshot
   write and verification attempt.
2. Use `health` or `health_compact` to inspect snapshot load source, background
   verification state, mismatch counts, and mismatch paths.
3. Inspect `.symforge/quarantine/index-snapshots/` when a corrupt or
   version-incompatible snapshot is detected.
4. Use `index_folder` reset to rebuild from source when health, verification,
   or quarantine evidence shows the snapshot should not be reused.

`index_folder` reset is explicit: run the serving process with
`SYMFORGE_INDEX_FOLDER_RESET=1`, then call `index_folder` for the project root.
The reset deletes only `.symforge/index.bin` and `.symforge/index.bin.tmp`
before reloading source files. The tool response and subsequent `health` or
`health_compact` output include `reset_state=current_project:pN` and
`index_state=index_folder_reset`; ordinary `index_folder` reloads report
`reset_state=none`.

The deprecated daemon compatibility name `trace_symbol` remains available
through v7.x with an explicit deprecation warning and is planned for removal in
v8.0. Generated client allow-lists do not grant it by default. Use
`get_symbol_context` or `find_references`.

## MCP Resources And Prompts

SymForge also exposes protocol resources and prompts. These are current shipped
surfaces, not future-only design notes.

Static resources:

| Resource | Use |
|---|---|
| `symforge://repo/health` | Current runtime and index health |
| `symforge://repo/outline` | Compact file-level repository outline |
| `symforge://repo/map` | Directory and symbol map |
| `symforge://repo/changes/uncommitted` | Current uncommitted-change view |

Resource templates:

| Template | Use |
|---|---|
| `symforge://file/context` | File outline, references, imports, consumers, and git activity |
| `symforge://file/content` | Exact file content and contextual excerpts |
| `symforge://symbol/detail` | Symbol definition body |
| `symforge://symbol/context` | Symbol context with grouped references |

Prompts:

| Prompt | Use |
|---|---|
| `symforge-review` | Code review planning with SymForge context |
| `symforge-architecture` | Architecture mapping |
| `symforge-triage` | Failure triage |
| `symforge-onboard` | Codebase onboarding |
| `symforge-refactor` | Refactor planning |
| `symforge-debug` | Debugging plan |

## Ranking And Search Signals

`search_files` ranks path matches by default. It can also use optional signals
when explicitly requested:

```json
{
  "query": "routes",
  "rank_by": "path+cochange",
  "anchor_path": "src/auth/routes.rs"
}
```

```json
{
  "query": "cache",
  "rank_by": "frecency"
}
```

Frecency favors files recently and repeatedly touched through commitment tools.
Search and guidance tools such as `search_symbols`, `search_text`,
`search_files`, `ask`, `explore`, and `investigation_suggest` do not bump
frecency; even `search_files(rank_by="frecency")` reads that signal without
creating it. This avoids a positive feedback loop where searched-but-ignored
files drift upward as if they were actually used.
Co-change ranking uses git history to surface files that tend to move together.
If a requested capability is unavailable, stale, disabled, or still preparing,
the response says that explicitly and falls back to path ranking.

Use `debug_ranking=true` on `search_files` when you need to inspect why files
were ordered the way they were.

## Structural Edits And Worktrees

The edit tools operate by symbol and validate targets before writing. They also
report edit status and affected paths so callers can tell whether the operation
actually changed code.

Edit mutations accept an optional `idempotency_key` where advertised. A retry
with the same key and the same canonical request returns the stored result
without writing again; the same key with a different request returns an
idempotency conflict. Dry runs do not reserve replay state.

All edit tools accept an optional `working_directory` pointing at a sibling git
worktree. Supplying it is explicit routing consent: SymForge validates the
worktree, maps the indexed path into that worktree, writes there, and reports
the indexed path and actual write path.

```json
{
  "path": "src/lib.rs",
  "name": "hello",
  "new_body": "fn hello() { println!(\"hi\"); }",
  "working_directory": "/abs/path/to/sibling/worktree"
}
```

## Local State

SymForge is local-first. Runtime state lives under `.symforge/` in the
workspace, and home-level binaries/config live under `SYMFORGE_HOME`.

Common files:

| Path | Purpose |
|---|---|
| `.symforge/index.bin` | Warm-start snapshot for the live index |
| `.symforge/quarantine/index-snapshots/` | Preserved corrupt or version-incompatible snapshots with metadata |
| `.symforge/idempotency/` | Retry records and quarantined corrupt idempotency records for mutating tools |
| `.symforge/frecency.db` | Optional persistent frecency signal store |
| `.symforge/coupling.db` | Optional co-change coupling store |
| `.symforge/analytics.db` | Optional local analytics store |
| `.symforge/sidecar.*` | Local sidecar metadata such as PID and port |

Analytics are local and bounded. Disabled analytics should not create a
database. Exported records are capped and redacted.

## Environment

Common configuration variables:

| Variable | Effect |
|---|---|
| `SYMFORGE_HOME` | Home directory for the installed binary and daemon metadata |
| `SYMFORGE_AUTO_INDEX` | Enables startup project discovery and indexing |
| `SYMFORGE_NO_DAEMON` | Forces local in-process mode instead of daemon routing |
| `SYMFORGE_SIDECAR_BIND` | Bind host for local sidecar state |
| `SYMFORGE_DAEMON_BIND` | Bind host for shared daemon state; loopback hosts are accepted by default |
| `SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK` | Explicit truthy opt-in required before the daemon binds a non-loopback host |
| `SYMFORGE_DAEMON_AUTH_TOKEN` | Optional local bearer token for daemon project, session, tool, and sidecar routes |
| `SYMFORGE_RECONCILE_INTERVAL` | Watcher reconciliation interval in seconds; `0` disables periodic sweeps |
| `SYMFORGE_CHECKPOINT_INTERVAL_SECS` | Optional periodic snapshot interval for local in-process mode; unset/`0`/false disables it, nonzero values are bounded to 30-3600 seconds |
| `SYMFORGE_CB_THRESHOLD` | Parse-failure circuit-breaker threshold |
| `SYMFORGE_FRECENCY` | Frecency policy: session-only by default, persistent when truthy, disabled when false/off/disabled |
| `SYMFORGE_COUPLING` | Co-change policy: lazy by default, warm on startup when truthy, disabled when false/off/disabled |
| `SYMFORGE_DEBUG_RANKING` | Ranking diagnostics policy |
| `SYMFORGE_WORKTREE_AWARE` | Worktree routing policy for edit calls |
| `SYMFORGE_ANALYTICS_DB_PATH` | Override analytics database location |
| `SYMFORGE_FRECENCY_DB_PATH` | Override frecency database location |
| `SYMFORGE_COUPLING_DB_PATH` | Override co-change database location |
| `SYMFORGE_PROJECT_CONFIG_TRUST_MODE` | Trust behavior for project-local SymForge configuration |

Daemon HTTP is a local coordination surface, not a remote production API.
The default bind path is loopback-only. If `SYMFORGE_DAEMON_BIND` names a
non-loopback host, SymForge rejects startup unless
`SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK` is truthy; that opt-in emits a warning.
When `SYMFORGE_DAEMON_AUTH_TOKEN` is non-empty, project, session, tool, and
sidecar routes require `Authorization: Bearer <token>`. `/health` remains
unauthenticated so local readiness and compatibility checks can still discover
the daemon, but health output reports only whether auth is required and never
prints the token.

Automatic stale-daemon cleanup is conservative. SymForge only terminates an
incompatible recorded daemon when the pid file matches `/health`, the reported
executable name matches the current SymForge executable, and platform safety
checks pass. On Linux, cleanup also verifies `/proc/<pid>/status` ownership and
`/proc/<pid>/exe` against the daemon's health report before sending a signal. On
Windows and other platforms, where SymForge does not have a portable owner check,
it falls back to the pid plus executable-name guard and logs/cleans stale
metadata instead of terminating when those checks fail.

## Develop

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
```

The npm wrapper has its own tests:

```bash
cd npm
npm test
```

The Rust toolchain is pinned by `rust-toolchain.toml` to Rust 1.95.0 with
`rustfmt` and `clippy`; the crate uses Rust edition 2024. PR and push CI run
version sync, formatting, `cargo check`, clippy with warnings denied, the full
Rust test suite, a release build, and npm tests.

Scheduled and manual CI also run bounded ignored performance evidence:

```bash
cargo test --release --test live_index_integration test_load_perf_1000_files -- --ignored --test-threads=1
cargo test --release --test coupling_calibration calibrate_current_repo_smoke -- --ignored --test-threads=1 --nocapture
```

The full real-repo coupling calibration remains operator-triggered. Provide a
portable corpus with `SYMFORGE_CALIBRATION_REPOS`, for example
`symforge=/repos/symforge;tokio=/repos/tokio;magika=/repos/magika`, then run:

```bash
cargo test --release --test coupling_calibration calibrate_against_real_repos -- --ignored --test-threads=1 --nocapture
```

The release workflow is driven by Release Please on `main`. When a release is
created, GitHub Actions builds platform binaries, builds the npm tarball, uploads
release assets, and publishes the npm package.

## Project Notes

The live implementation backlog is tracked in
[docs/live-code-backlog.md](./docs/live-code-backlog.md). Historical plans,
reviews, and local agent artifacts were pruned from the repository; the README
is the public starting point, and the backlog file is the implementation queue.

## License

SymForge is licensed under the
[PolyForm Noncommercial License 1.0.0](./LICENSE). You may inspect, study, and
use the source code for noncommercial purposes. Commercial use requires a
separate license.
