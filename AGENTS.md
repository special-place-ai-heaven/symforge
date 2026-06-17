# AGENTS.md

This repository is `symforge`.

It is a Rust-native, coding-first MCP project for code indexing, retrieval, and recovery.

## Mission

Build a world-class MCP for code indexing, retrieval, orchestration, and recovery.

Primary qualities:
- speed
- robustness
- idempotency
- deterministic behavior
- self-healing and self-recovery
- strong edge-case handling
- coding-first ergonomics

## Core Architecture Direction

Use a local-first architecture:
- Rust MCP server for the protocol surface
- in-process LiveIndex as the primary query engine
- local snapshot persistence under `.symforge/` for warm startup and recovery
- tree-sitter-based parsing and symbol/reference extraction in Rust

The read path should stay in-process and memory-resident whenever possible.

Reason:
- code-intelligence queries must be fast and deterministic
- symbol spans depend on exact bytes from the current workspace
- restart recovery should come from local snapshots, not an external control plane

## Product Principles

- Coding-first beats generic document-first behavior.
- Determinism beats convenience.
- Explicit recovery beats hidden retry magic.
- Corruption should be quarantined, not silently served.
- Long-running operations must be resumable.
- Mutating operations must support idempotency.
- Shutdown is not a safe persistence boundary.

## Storage Principles

Use local-first persistence, not an external control plane.

Recommended split:
- In-process LiveIndex:
  - file contents needed for active queries
  - symbol metadata
  - reference metadata
  - reverse indices and search structures
  - watcher and health state
- Local `.symforge/` state:
  - serialized index snapshots
  - temp files and quarantine artifacts
  - sidecar/session coordination metadata
  - future derived artifacts where local persistence is useful

Snapshot and retrieval rules:
- write bytes exactly as read
- never normalize line endings
- never decode and re-encode for persistence
- verify source slices against stored hashes

## Idempotency Rules

Mutating tools must accept an `idempotency_key` when appropriate.

Required behavior:
- normalize request arguments into a canonical hash
- first execution stores `idempotency_key + request_hash + status`
- replay with same key and same hash returns the stored result
- replay with same key and different hash fails deterministically

Likely idempotent current or near-term tools:
- `index_folder`
- `checkpoint_now`
- structural edit and batch mutation tools
- `repair_index` only if a future WorkSpec ships a real repair tool with
  durable state and machine-readable status
- future write or annotation tools when they become real shipped tools

## Recovery Rules

Self-healing means deterministic repair paths.

The system should support:
- startup sweeps for stale leases and temp files
- checkpoint replay for interrupted snapshot state
- quarantine of bad parses or bad spans
- scheduled repair jobs
- integrity verification
- explicit health tools, plus repair tools only when their workflow is real

Failure should degrade safely:
- process crashes should recover from the latest valid snapshot or an explicit
  source rebuild path
- parser failures should isolate a file, not poison a run
- bad symbol spans should never be served silently

Current v7.13.x recovery contract:

- `checkpoint_now(verify_after_write=true)` is the explicit checkpoint path for
  forcing `.symforge/index.bin` persistence before risky operations.
- Use `health` or `health_compact` to inspect snapshot load source, background
  snapshot verification state, and mismatch summaries.
- Bad or version-incompatible snapshots are preserved under
  `.symforge/quarantine/index-snapshots/` with metadata instead of being served
  silently.
- Use `index_folder` reset when the active index must be rebuilt from source
  after health, verification, or quarantine evidence shows a snapshot is not a
  valid recovery source.
- `repair_index` is intentionally retired until a real repair workflow exists.
  `get_index_run` and `cancel_index_run` remain retired. No durable run IDs are
  exposed until resumable run storage exists.

## MCP Surface

The shipped v7.13.x MCP surface includes tools, resources, and prompts. Do not
design for tools only.

The **default** `tools/list` surface is compact-3: `symforge`, `symforge_edit`,
`status`. The full **35-tool** surface below (including `health_compact`) is a
documented opt-out via `SYMFORGE_SURFACE=full`:

- Runtime and index: `health`, `health_compact`, `index_folder`,
  `checkpoint_now`, `analyze_file_impact`, `what_changed`, `diff_symbols`,
  `validate_file_syntax`
- Read and search: `get_repo_map`, `get_file_context`, `get_file_content`,
  `get_symbol`, `get_symbol_context`, `inspect_match`, `search_symbols`,
  `search_text`, `search_files`, `find_references`, `find_dependents`
- Guidance: `explore`, `ask`, `conventions`, `edit_plan`,
  `context_inventory`, `investigation_suggest`
- Structural edits: `replace_symbol_body`, `edit_within_symbol`,
  `insert_symbol`, `delete_symbol`, `batch_edit`, `batch_insert`,
  `batch_rename`

Ranking signal invariants:
- `search_symbols`, `search_text`, `search_files`, `explore`, `ask`, and
  `investigation_suggest` are discovery or guidance tools; they must not bump
  frecency. Frecency is a commitment signal from loaded context and mutation
  paths. Letting searches create it would turn searched-but-ignored files into
  false ranking evidence.

Current resources:

- Static repository resources: `symforge://repo/health`,
  `symforge://repo/outline`, `symforge://repo/map`,
  `symforge://repo/changes/uncommitted`
- Templates: `symforge://file/context`, `symforge://file/content`,
  `symforge://symbol/detail`, `symforge://symbol/context`

Current prompts:

- `symforge-review`, `symforge-architecture`, `symforge-triage`,
  `symforge-onboard`, `symforge-refactor`, `symforge-debug`

Name migration and deferred-surface table:

| Old, removed, or future name | Current v7.13.x status |
|---|---|
| `get_repo_outline` | Use `get_repo_map`; repository outline also exists as a resource. |
| `get_file_outline` | Use `get_file_context`. |
| `get_symbols` | Use `get_symbol` batch mode. |
| `trace_symbol` | Retired from client guidance; daemon compatibility may route to `get_symbol_context` with a deprecation warning. |
| `index_repository` | Deferred; use `index_folder` for current local repository indexing. |
| `get_index_run`, `cancel_index_run` | `get_index_run` and `cancel_index_run` remain retired; no durable run IDs are exposed. |
| `checkpoint_now` | Current recovery surface; use `checkpoint_now(verify_after_write=true)` for explicit snapshot persistence. |
| `repair_index` | `repair_index` is intentionally retired; use `health` or `health_compact`, inspect `.symforge/quarantine/index-snapshots/`, then use `index_folder` reset when a rebuild is required. |
| `invalidate_cache` | Deferred; use `analyze_file_impact` for one changed file or `index_folder` for a full reset. |

## Memory Strategy

Project memory should be layered:
- runtime memory:
  - live index state
  - watcher state
  - recent health and verification state
- persisted local memory:
  - snapshot files
  - file metadata
  - symbol metadata
  - hashes and recovery artifacts
- semantic memory:
  - optional embeddings for fuzzy recall over docs, notes, and conversations

The current architecture does not require an external database for query serving.

If semantic search becomes important:
- start simple
- keep the query path local-first
- add a dedicated sidecar only if scale or latency requires it

## Current Known Context

As of 2026-03-06:
- this repo was freshly created and bootstrapped as a Rust project
- there is an `rmcp`-based stdio server scaffold
- an earlier Python prototype found a real Windows byte-offset bug caused by newline translation during raw cache writes
- that bug is a design warning: byte-exact storage is non-negotiable

## Implementation Guidance

- Prefer clean module boundaries. The shipped layout currently uses:
  - `protocol`
  - `live_index`
  - `parsing`
  - `daemon`
  - `cli`
  - `watcher`
  - `discovery`
  - `git`
  - `analytics`
  - `worktree`
  - `sidecar`
  - `edit_safety`
  - `domain`
- Treat `application`, `storage`, `indexing`, and `observability` as possible
  future boundary names, not current shipped directories.
- Keep domain logic testable without MCP or database runtime dependencies.
- Prefer bounded concurrency and structured shutdown.
- Long-running operations should return durable run ids when appropriate.
- Use Rust everywhere possible.
- If Python tooling is ever needed, use `uv`, not `pip`.

## Working Style

- Be pragmatic, direct, and engineering-focused.
- Avoid unnecessary boilerplate.
- Prefer implementing over theorizing once direction is clear.
- Preserve backward compatibility only when it serves the product.
- This project is ours now; optimize for the best end state, not legacy imitation.

## Tooling Preference

When SymForge MCP is available, prefer its tools for repository and code inspection before falling back to direct file reads.

Use SymForge first for:
- symbol discovery
- text/code search
- file outlines
- repository outlines
- targeted symbol/source retrieval
- inspection of implementation code under `src/`, `tests/`, and similar code-bearing directories

Preferred tools:
- `search_text`
- `search_symbols`
- `get_file_context`
- `get_repo_map`
- `get_symbol`

Default rule:
- use SymForge to narrow and target code inspection first
- use direct file reads only when exact full-file source or surrounding context is still required after tool-based narrowing

Direct file reads are still appropriate for:
- exact document text in `docs/` or planning artifacts when literal wording matters
- configuration files where exact raw contents are the point of inspection

Do not default to broad raw file reads for source-code inspection when SymForge can answer the question more directly.

<!-- SPECKIT START -->
For additional context about technologies to be used, project structure,
shell commands, and other important information, read the current plan
at specs\001-v8-phase0-preflight\plan.md
<!-- SPECKIT END -->
