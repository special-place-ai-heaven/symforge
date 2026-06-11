# Semantic Tier Roadmap — tree-sitter as Tier-0, semantic engines as depth backends

Status: design direction, agreed 2026-06-11. Not yet scheduled.
Audience: external research agents + future implementers. Everything in
"Current reality" is verifiable in this repo today; everything in "Roadmap"
is proposal.

## Decision

Tree-sitter remains SymForge's universal Tier-0 index. Per-language semantic
engines (LSP servers, compiler frontends) become optional depth backends that
SymForge escalates to when syntactic confidence is insufficient. We do not
replace tree-sitter; we layer on top of it.

Rationale: tree-sitter gives uniform, fast (~ms/file), fault-tolerant,
offline parsing for 19 languages with one architecture. Semantic engines give
precise name resolution, types, and cross-file truth for ONE language each,
at the cost of process management, memory (rust-analyzer: GBs on large
repos), cold-start time (seconds to minutes), and per-engine integration
work. These are different layers, not competitors.

## Current reality (what exists in this repo today)

- Parsing: tree-sitter grammars per language, selected in
  `src/parsing/mod.rs`. Dart uses `tree-sitter-dart-orchard` (Dart 3
  capable); TSX vs TypeScript use split grammars with separate query caches.
- Cross-references: `src/parsing/xref.rs` — per-language tree-sitter queries
  producing heuristic refs (calls, imports, type usages). Name-based: an
  identifier match, not a resolved symbol. Overloaded/shadowed names across
  types over-match by design. This is the precision ceiling of Tier-0/Tier-1.
- Query layer: `src/live_index/query.rs` answers `find_references`,
  `get_symbol_context`, `find_dependents` from the in-memory index.
- Outcome metadata: `src/protocol/result_status.rs` already carries
  machine-readable result status per tool call — the natural carrier for a
  future confidence signal.
- Process architecture: daemon + sidecar with spawn-on-demand and idle
  lifecycle. Precedent for managing helper processes lazily.
- Derived-data persistence: coupling store (SQLite, generation-stamped,
  prunable via `prune_dead_paths`, VACUUM cadence). Precedent for an
  evictable on-disk semantic cache.

### Naming collision to resolve first

SymForge already uses "Tier 1/2/3" for ADMISSION tiering (full parse /
metadata-only / skipped) in the live index. The query-depth ladder below must
NOT reuse the word "tier" unqualified. Proposal: call query depth "Depth
0..3" (or "D0..D3") in code and docs; keep "Tier" for admission.

## Roadmap (proposal, in dependency order)

### D-ladder: multi-depth query strategy

- D0 — tree-sitter index (today's index): symbols, outlines, file structure.
  Always available, always fast.
- D1 — heuristic xrefs (today's xref.rs): name-based refs with enclosing
  symbol context. Always available.
- D2 — language server escalation: targeted LSP requests
  (definition/references/hover/typeDefinition) against a lazily-started
  language server, scoped to the query at hand.
- D3 — compiler-grade semantic graph: batch-extracted SCIP indexes
  (rust-analyzer scip, scip-typescript, scip-python, scip-java, ...)
  ingested into a persistent cross-repo symbol graph.

Escalation rule: answer at the cheapest depth whose confidence clears the
caller's need. Confidence drops (and D2+ becomes attractive) when: the name
is overloaded across types, refs span dynamic import/re-export chains, the
symbol is a method on an inferred receiver type, or the caller explicitly
asks for semantic precision.

### Step 1 — Confidence surface (no new engines; cheap; do first)

Make D0/D1 answers self-describing: every refs/context result carries
`depth: d0|d1` and a `confidence` grade derived from observable predicates
(unique name in repo vs overloaded; receiver typed vs inferred; match count
dispersion). Carrier: `result_status.rs`. This is useful standalone (agents
already mis-trust over-matched refs) and is the contract every later depth
plugs into.

### Step 2 — Optional LSP adapters with lazy activation (D2)

- One adapter trait; first implementations: rust-analyzer (Rust),
  tsserver or typescript-language-server (TS/TSX/JS), Pyright (Python),
  gopls (Go). Dart analysis server, JDT LS (Java), Roslyn/OmniSharp (C#)
  later.
- Lazy activation: server starts on first D2 escalation for its language,
  idles out on a timer (sidecar lifecycle precedent). Never started by
  indexing alone.
- Hard requirements: binary discovery must be explicit-config-first with
  PATH fallback (Windows PATH shadowing burned us before); per-server memory
  ceiling + restart policy; graceful degradation to D1 with honest
  `depth: d1, escalation_failed: <why>` metadata when the server is absent
  or sick. No fake success.
- Windows caveat: process tree cleanup and handle inheritance need explicit
  testing (orphaned-process lessons from sidecar_integration).

### Step 3 — SCIP-based stable symbol IDs (D3 substrate)

Adopt SCIP symbol syntax (`scheme manager package version descriptor`) as
SymForge's stable cross-file/cross-version symbol identity where a semantic
indexer is available; fall back to today's path+name+kind identity at D0/D1.
This makes refs durable across renames-with-history and enables cross-repo
linking. Ingest path: run `rust-analyzer scip` / `scip-typescript` etc. as
batch jobs, parse the protobuf, join against the tree-sitter index by
file+range.

### Step 4 — Semantic cache (D3)

Persist D2/D3 answers (definitions, references, type hierarchies, call
graphs) in a per-repo SQLite store keyed by SCIP symbol + content hash of
the defining file. Invalidate by hash, prune by generation (coupling-store
pattern). Cache turns expensive LSP round-trips into index-speed lookups for
hot symbols.

## Non-goals

- Replacing tree-sitter parsing or making any LSP a hard dependency.
- Always-on language servers. Activation is lazy and per-query-need.
- Bundling language-server binaries. Discovery + clear "not installed,
  degraded to D1" messaging instead.
- Semantic indexing of quarantined/Tier-2 (metadata-only) files.

## Risks / open questions for research agents

1. LSP protocol overhead per one-shot query: is warm-server latency
   (10-100ms/request) acceptable inside MCP tool budgets, and what is the
   real cold-start cost per server on mid-size repos?
2. SCIP coverage gaps: which of our 19 languages have production-quality
   SCIP emitters today? (Known good: Rust, TS/JS, Python, Java, Go via
   scip-go. Unknown: Dart, Swift, Elixir, Perl, PHP, Ruby, Kotlin, C/C++,
   C#.)
3. Version skew: tree-sitter AST view vs LSP view of the same file during
   rapid edits — what reconciliation does D2 need (didChange sync vs
   re-read-from-disk)?
4. Memory budget policy for concurrent servers on multi-language monorepos.
5. Confidence calibration: which observable predicates actually predict
   D1 wrong-answer rates? Needs measurement against ground truth (SCIP
   index as oracle over a corpus).
