# Feature Specification: Pure-Rust Code Mode (`symforge code` + `SYMFORGE_SURFACE=code`)

**Feature Branch**: `feat/037-code-mode`

**Created**: 2026-11-21

**Status**: Draft — Spike Verified (`monty` 1.6.0 on Windows MSVC + Rust 1.100.0)

**Input**:
- Code-mode briefs (`2026-11-19-code-mode-for-symforge.md`, `2026-11-19-code-mode-brief.md`)
- Audit findings (`docs/reviews/2026-11-09-product-audit/AUDIT.md` §A.3, §A.6, §B.1–B.4: 69,295 B / ~17,300-token upfront `tools/list`, ~83,000-token per-turn context re-read, per-call prose envelope overhead)
- Owner mandate (2026-11-21): pure Rust only (no JavaScript/Node/Deno/V8/C-FFI); adapt SymForge for post-MCP CLI-first and Code Mode agentic harnesses.
- External reference evaluation (2026-11-21): `pydantic/monty` (`1.6.0`), `legend-of-alan/ghostmode`, `rivet-dev/code-mode-language-benchmark`, `SouzaRodrigo61/codemode-cli`, `universal-tool-calling-protocol/code-mode`, `pctx_code_mode`, `codemode`.

## Background

Traditional MCP tool calling imposes two compounding taxes on coding agents:
1. **Upfront schema tax**: SymForge's default 40-tool `tools/list` payload is 69,295 bytes (~17,300 tokens), 83% of which is JSON Schema boilerplate.
2. **Round-trip & intermediate-payload tax**: Dependent workflows ("find symbol, list non-test callers, read the three hottest caller bodies") require 4–8 sequential model turns, re-reading ~83,000 cached context tokens per turn and dumping every intermediate table into conversation history.

Meanwhile, coding harnesses are shifting away from 40-tool MCP servers toward **CLI + Skill execution** (for shell-capable agents such as Claude Code, Codex, OMP, OpenCode, Rerun coworkers) and **Code Mode** (where the model writes a short Python snippet that chains typed host queries inside a zero-authority sandbox and returns only the distilled JSON answer).

On 2026-11-21 we ran a compile-and-execution spike of Pydantic's pure-Rust Python 3.14 subset VM (`monty = "1.6"`, `monty-types = "1.6"`) against SymForge's pinned `rust-toolchain = "1.100.0"` on `x86_64-pc-windows-msvc` under `unsafe_code = "deny"` and `warnings = "deny"`:
- **Zero C/C++/JS dependencies**: pure Rust (`ruff_python_parser` + bytecode VM).
- **Sub-millisecond startup & host suspension**: 65 µs `MontyRepl` init, 746 µs end-to-end script parse + `ReplProgress::FunctionCall` round-trip + list comprehension formatting.
- **Deterministic resource & capability containment**: `while True: pass` preempted cleanly at 50.04 ms (`TimeoutError`); `'a' * 100_000_000` rejected before allocation (`MemoryError`); `import os; os.listdir('.')` rejected with `PermissionError` (`OsCall::no_handler_result()`).
- **Advisory compatibility**: `cargo deny check advisories` surfaced only `RUSTSEC-2026-0543` (`chrono`), already recorded in SymForge's `deny.toml`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Post-MCP Shell Agent Runs Multi-Step Queries in One Turn (Priority: P1)

A shell-capable coding agent (Claude Code, Codex, OMP, OpenCode, or Rerun factory coworker) invokes `symforge code` from `Bash` without configuring any MCP server. It inspects typed Python stubs on demand (`symforge code stubs`), runs a multi-step Python script (`symforge code run`) or a single structured query (`symforge q`), and receives pure JSON on stdout—reusing the warm local SymForge daemon when running or opening the local snapshot read-only when absent.

**Why this priority**: Shell-capable coding agents are SymForge's primary audience, require zero MCP upfront schema tokens, and directly replace external wrappers such as `tools/factory/code-q.mjs`.

**Independent Test**: With no MCP server running, invoke `symforge code stubs`, `symforge q`, and `symforge code run` against a fixture repository in both warm-daemon and `SYMFORGE_NO_DAEMON=1` modes; verify JSON stdout, zero stderr pollution on success, and identical answers.

**Acceptance Scenarios**:

1. **Given** a repository with a symbol ` target_fn ` called from three files, **When** an agent runs `symforge code run` with a Python snippet chaining `search_symbols` → `find_references` → `get_symbol(targets=...)`, **Then** stdout is a single valid JSON object containing `"status": "ok"` and the script's final expression as `"value"`, completed in one process invocation.
2. **Given** a warm local SymForge daemon serving the repository, **When** `symforge code run` or `symforge q` executes, **Then** each host query routes over the authenticated local daemon (`daemon.<os>.port` + `daemon.<os>.token`) without cold-loading the index.
3. **Given** no running daemon (or `SYMFORGE_NO_DAEMON=1`), **When** `symforge code run` or `symforge q` executes, **Then** it opens the repository's `.symforge/index.bin` snapshot (or builds in-process read-only) and returns the same JSON structure without spawning a background daemon.
4. **Given** `symforge code idioms`, **When** invoked by an agent or skill, **Then** it prints the concise self-contained guide (available functions, return dict keys, Monty Python subset rules, resource caps, and examples) verified by CI never to drift from the registered host functions.

---

### User Story 2 - MCP-Only Client Uses `SYMFORGE_SURFACE=code` Under 1,200 Upfront Tokens (Priority: P1)

An MCP client without shell access (or an operator running `symforge serve --surface code` / `SYMFORGE_SURFACE=code`) connects over stdio or Streamable HTTP. `tools/list` advertises only four tools: `code_search`, `code_execute`, `symforge_edit`, and `status`. All legacy read tools are dispatch-gated on that surface so the agent discovers typed Python stubs via `code_search`, runs read/trace chains in `code_execute`, and performs explicit, hash-guarded mutations through `symforge_edit`.

**Why this priority**: Delivers the 93%+ upfront schema reduction on MCP while keeping source mutations explicit and atomic outside the script sandbox.

**Independent Test**: Start `SymForgeServer` with `SYMFORGE_SURFACE=code`; assert `tools/list` byte size ≤ 4,800 bytes (~1,200 tokens), verify `code_search` + `code_execute` + `symforge_edit` + `status` succeed, and verify direct `tools/call` to `search_text` is rejected at dispatch.

**Acceptance Scenarios**:

1. **Given** `SYMFORGE_SURFACE=code`, **When** a client calls `tools/list`, **Then** only `["code_search", "code_execute", "symforge_edit", "status"]` are advertised and total serialized `tools/list` JSON is ≤ 4,800 bytes.
2. **Given** `code_search(query="references")`, **When** called, **Then** it returns keyword-only Python function stubs AND `TypedDict` return definitions generated from the single canonical read-API registry.
3. **Given** `code_search()` with an empty or omitted query, **When** called, **Then** it returns one-line signatures for every callable sandbox function so the model can orient in < 350 tokens.
4. **Given** `SYMFORGE_SURFACE=code`, **When** a client attempts `tools/call` for an unlisted legacy read tool (e.g. `get_symbol`), **Then** `enforce_compact_surface` (generalized to `enforce_surface_gate`) rejects the call with an `InvalidRequest` naming `code_execute`.

---

### User Story 3 - Hostile or Runaway Model Scripts Cannot Escape, Hang, or Mutate State (Priority: P1)

A prompt-injected file comment or buggy model script attempts to read `.env` via Python `open()`, make an HTTP call, loop forever, allocate 500 MiB, recurse 5,000 frames deep, call a mutating tool, or return 5 MiB of raw source into context. Every attempt is stopped deterministically inside the pure-Rust sandbox, while SymForge's own secret-withholding and root-confinement policies hold on every host tool call.

**Why this priority**: Code execution over untrusted repository text turns prompt injection into script generation; the sandbox and host boundary must be capability-safe by construction.

**Independent Test**: Unit and integration suite feeding adversarial Python snippets (`open`, `os`, `socket`, `urllib`, `eval`, `while True: pass`, `'x' * 10**9`, deep recursion, 100 tool calls, reading a secret-withheld file, `..` path traversal) into `CodeModeRunner`.

**Acceptance Scenarios**:

1. **Given** a script calling `open(".env")`, `os.environ`, `pathlib.Path(".env").read_text()`, or `time.sleep(60)`, **When** executed, **Then** OS calls receive `no_handler_result()` (`PermissionError`) and sleeps resolve in zero wall time (`SleepMode::Zero`).
2. **Given** a script containing `while True: pass`, **When** executed with `max_feed_duration = 10s` (or test override `50ms`), **Then** execution aborts with `TimeoutError` within budget + poll granularity without wedging the Tokio worker or daemon.
3. **Given** a script allocating past `max_memory = 64 MiB` or recursing past `max_recursion_depth = 150` or making more than `max_calls = 64` host calls, **When** executed, **Then** execution terminates cleanly with a structured error naming the tripped limit and the calls completed so far.
4. **Given** a repository file withheld by SymForge's secret detector (`#927` / `#929`), **When** a script calls `file_content(path=...)` or `search_text(...)`, **Then** the host binding routes through `embed::parity` and withholds the secret bytes with the exact rule/line diagnostic—never raw disk I/O.
5. **Given** a script whose return value or `print()` output exceeds `max_output_chars = 16_000`, **When** serialized, **Then** inline output is capped at a UTF-8 char boundary and the full output is written to a session/temp spill file named in `spill_path` (with `total_bytes`), never silently truncated.

---

### User Story 4 - Whole-Task Token & Pass-Rate Proof on Standing Benchmark (Priority: P2)

Before promoting `code` or `symforge code` in default `symforge init` skill installation, the standing benchmark (`specs/035-token-benchmark/`) runs two new arms (`d: SYMFORGE_SURFACE=code` and `e: CLI symforge code skill, no MCP`) against `a: native`, `b: full steered`, and `c: full unsteered`.

**Why this priority**: Enforces Constitution Principle I (Reporting Invariant) and SF-TE-01—no efficiency claim without whole-task paired oracle evidence.

**Independent Test**: `python bench.py dry-run` exercises arms `d` and `e` end-to-end with transcript allow-list checks and deterministic oracle grading.

**Acceptance Scenarios**:

1. **Given** arm `e` (CLI skill, `mcp: false`), **When** transcript validation runs, **Then** any `mcp__symforge__*` call marks the cell `invalid_allowlist`, while `Bash` invocations of `symforge code` / `symforge q` are recorded and itemized in cell metrics.
2. **Given** completed benchmark runs, **When** `aggregate.py` renders the report, **Then** upfront schema tokens, round trips, total price-weighted tokens, and oracle pass counts are reported side by side for `native`, `full`, `compact`, `code` (MCP), and `cli-code` (Skill).

---

## Functional Requirements

- **FR-001 (Pure-Rust VM)**: Script execution MUST use `monty` + `monty-types` in pure Rust behind `feature = "server"` (or shared internal engine feature if opted into `embed` later), compiling cleanly under `unsafe_code = "deny"` and `warnings = "deny"` with zero C/C++ or JS/V8 dependencies.
- **FR-002 (Read-Only Parity Host)**: Sandbox host functions MUST bind exclusively to read-only `symforge::embed::parity::QueryRequest` projections (never raw `std::fs`, never mutating tools: `symforge_edit`, `replace_symbol_body`, `edit_within_symbol`, `insert_symbol`, `delete_symbol`, `batch_edit`, `batch_insert`, `batch_rename`, `index_folder`, `checkpoint_now`, `curate_knowledge`, `secret_remediate`, `analyze_file_impact`).
- **FR-003 (Discovery Frecency Invariant)**: Discovery calls inside scripts (`search_symbols`, `search_text`, `search_files`, `explore`, `ask`) MUST NOT bump file frecency; commitment/read calls follow `embed::parity` rules.
- **FR-004 (Keyword-Only Python API + Typed Return Stubs)**: Every sandbox function MUST accept keyword arguments only (positional args raise `TypeError` with signature hint) and return structured Python `dict` / `list` values (never formatted human prose envelopes). `code_search` / `symforge code stubs` MUST render both input keyword types and `TypedDict` return schemas from one canonical Rust descriptor table. `find_references` and `search_text` accept `context` (0 to 20, default 0); above 0 each row gains `context`, the numbered source lines from N before to N after the hit, taken from the index content the hit came from. A refusal for an unknown keyword ends with the function's signature line.
- **FR-004a (Lean MCP result text, amendment 2026-11-25)**: FR-004 governs values inside the script; operations still return the same `dict` / `list` values and scripts compose them unchanged. Only the text of the MCP `code_execute` tool result changes. For an `ok` outcome that was not truncated, the text is `render_lean(value)` (`src/codemode/lean.rs`), followed by `\nstdout:\n<stdout>` only when `stdout` is non-empty. On this lane the inline-or-spill decision of User Story 3 scenario 5 measures that lean text against `max_output_chars`, not the compact JSON: a value whose JSON is over the cap but whose lean text fits returns inline, and one whose lean text is over the cap spills with the unchanged preview envelope and spill file. `symforge code run` and `SYMFORGE_CODE_OUTPUT=json` keep measuring JSON. The script runs in the MCP process on the daemon-proxied lane too (only host calls cross to the daemon), so both lanes make the same decision; `calls`, `duration_ms` and `output_bytes` move to `_meta["symforge/code_execute"]`. Error outcomes, truncated/spilled outcomes, `symforge code run`, `symforge q` and the daemon proxy's internal transport keep the JSON envelope. `SYMFORGE_CODE_OUTPUT=json` restores the JSON text exactly. Grammar of `render_lean` (TAB is the field separator, `@ ` starts a group or record line, keys keep map order):
  - string: raw, unescaped. Other scalar: JSON (`null`, `true`, `3`). Empty list `[]`, empty object `{}`.
  - object: one section per key. Scalar or single-line string: `key: value`. Non-empty object: its sections with labels prefixed `key.` (so nesting is spelled `a.b.c: 1`). Multi-line string, list or empty object: a `key:` line, then the rendered value.
  - list of objects with identical key sets whose values are all scalars, with no newline in any string and no TAB in any string but the last column: a table. Columns keep key order except `text` and `preview`, which move last. Line 1 is the column names joined by TAB; then one line per row, cells joined by TAB, `null` as an empty cell. When a `path` column exists alongside others, it is dropped from the rows and a `@ <path>` line is printed before each run of rows sharing that path (a path that recurs after another path gets a new `@` line).
  - any other list whose items are all objects: one record per object. Record line `@ k=v<TAB>k=v...` holds every non-null scalar field without a newline or TAB and every list/object field as compact JSON; each string field containing a newline or TAB follows as a `key:` line and then the raw string.
  - any other list (scalars, mixed items): compact JSON.
  No scalar is dropped: every scalar leaf of the value appears in the text, raw or (inside compact JSON) JSON-escaped; `null` prints as an empty cell in tables and is left out of record lines, so a successful `get_symbol` row no longer carries `refusal=` and `withheld_reason=`. Known ambiguities, accepted: `null` and `""` look alike in a cell, a string `"5"` and the number `5` look alike, and a raw multi-line body could in principle contain a line shaped like a label or `@` line.
- **FR-005 (Named Resource Limits)**: Default sandbox limits MUST be named constants:
  - `DEFAULT_MAX_FEED_DURATION`: `10s` (hard ceiling `30s`)
  - `DEFAULT_MAX_MEMORY_BYTES`: `64 MiB` (hard ceiling `256 MiB`)
  - `DEFAULT_MAX_RECURSION_DEPTH`: `150` (safe on standard worker thread stacks without `stacker` C/asm)
  - `DEFAULT_MAX_HOST_CALLS`: `64`
  - `DEFAULT_MAX_OUTPUT_CHARS`: `16_000` (with full payload spill file when exceeded)
- **FR-006 (Stateless Isolation by Default)**: Each `code_execute` call and `symforge code run` invocation MUST start a fresh `MontyRepl` with `OsPolicy { sleep: SleepMode::Zero, ..Default::default() }` so state/globals cannot leak across unrelated requests or projects.
- **FR-007 (CLI Subcommands)**:
  - `symforge code run [FILE | -] [-c <CODE>] [--project <ID>] [--timeout-ms <N>]` — executes Python script against the sandbox, printing structured JSON `{ status, value, stdout, calls, duration_ms, spill_path? }` to stdout.
  - `symforge code stubs [QUERY] [--limit <N>]` — prints Python function stubs + `TypedDict` return types (or compact 1-line index when `QUERY` is omitted).
  - `symforge code idioms` — prints the self-contained agent manual (syntax rules, Monty subset notes, anti-one-liner guidance, worked examples), pinned by a drift test against the registered tool table.
  - `symforge q <OP> [JSON_ARGS] [--project <ID>]` — zero-script fast path for single structured `embed::parity` JSON queries (replaces `code-q.mjs` with zero VM overhead when 1 call suffices).
- **FR-008 (Warm Daemon + Local Fallback)**: Both `symforge code run` and `symforge q` MUST attach to the running authenticated local daemon when available (unless `SYMFORGE_NO_DAEMON=1`), and fall back cleanly to in-process read-only snapshot load without spawning a daemon.
- **FR-009 (MCP `code` Profile)**: `SYMFORGE_SURFACE=code` (and `symforge serve --surface code`) MUST advertise `["code_search", "code_execute", "symforge_edit", "status"]`, enforce dispatch gating against unlisted tools, thread `CONNECTION_SURFACE_HEADER` (`"code"`) across daemon proxy hops, and supply concise `CODE_SURFACE_INSTRUCTIONS` (≤ `SERVER_INSTRUCTIONS_MAX_BYTES = 600`).
- **FR-010 (Harness Skill Option in `init`)**: `symforge init --code-skill` (opt-in until benchmark gate passes) writes a compact `symforge-code` `SKILL.md` teaching shell agents to run `symforge code idioms` / `symforge code run` / `symforge q`.
- **FR-011 (V11 Public API & Darkness Preservation)**: `src/lib.rs` public exports (`symforge::embed`, `symforge::server_api`) and `specs/020-repository-knowledge-index/` frozen contracts MUST remain untouched; new modules mount under `src/internals.rs` (`#[cfg(feature = "server")]`) and update `tests/darkness_baseline/full_source.pins` via the standard oracle.

## Out of Scope (v1)

- Mutating files inside `code_execute` / `symforge code run` scripts (all edits remain explicit, atomic, hash-checked calls).
- Persistent multi-turn REPL globals across separate `code_execute` calls (can be added later via `monty::dump` if benchmark traces show repeated state rebuilds).
- Compiling `monty` into the `--no-default-features --features embed` library cell before an embedder (e.g. AAP) requests it—keeping `monty` behind `feature = "server"` preserves current `embed` compile times and binary footprint.
