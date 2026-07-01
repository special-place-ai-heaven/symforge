# Feature Specification: CBM Capability Ports (Graph Intelligence Program)

**Feature Branch**: `015-cbm-capability-ports`

**Created**: 2026-06-29

**Status**: Planning complete (PROG + S0) — implementation at S0 `[C]`

**Speckit**: clarify ✓ · analyze ✓ · implement → S0 spike next

**Program goal**: Make SymForge **strictly superior** to today and to CBM on agent
workflows — **faster**, **fewer tokens**, **more capability** — by porting every
genuinely beneficial CBM idea into the LiveIndex + STEL stack while **keeping and
extending** SymForge's existing moat. No second authoritative query store
(Constitution I; spec 007 FR-015).

**North Star**: [§ Superiority doctrine](#superiority-doctrine) governs every port decision.

**Reference analysis**: Competitive review of CBM vs SymForge (2026-06-29);
arXiv [2603.27277](https://arxiv.org/abs/2603.27277).

## Superiority doctrine

**Default: adopt.** Port every CBM capability that makes SymForge **faster**, **lower-token**, or **more capable** for agents — reimplemented on LiveIndex + STEL, not vendored wholesale.

**Only skip inferior parts** — ideas that are worse on our architecture or for our users when measured honestly:

| Inferior on SymForge | Why we don't copy the mechanism | What we adopt instead |
|----------------------|--------------------------------|------------------------|
| SQLite graph as query authority | Second truth, drift, latency | Same graph *queries* via derived `GraphProjection` |
| 14-tool surface without STEL | Token-heavy agent loops | Same capabilities via STEL + full surface |
| Vendored C / 158-grammar monolith | Fights embed size, Rust quality bar | Same breadth via phased grammars + generic tier |
| 3D graph UI | No MCP/agent win | Graph *data* via tools/resources |
| Features that add latency/tokens without agent value | Fails superiority | Redesign or drop |

**SymForge moat is kept, not sacrificed:** symbol edits, compact STEL, byte-exact recovery, idempotency, trust envelopes, frecency discipline — then **extended** with CBM's best graph/resolver/semantic/discovery ideas.

**Ship check** (quick, before merge): faster or same? fewer tokens or same? strictly more capability? no constitution regression? If yes → adopt. If no → inferior; skip or redesign.

## Context

CBM ships a persistent SQLite knowledge graph with Hybrid LSP call resolution,
Cypher queries, semantic search, change-impact blast radius, team graph artifacts,
and graph-augmented discovery. SymForge leads on symbol-addressed editing,
compact STEL surface, byte-exact recovery, idempotency, MCP resources/prompts, and
frecency discipline.

This program ports CBM's **query and analysis capabilities** into derived
projections over the existing LiveIndex — preserving SymForge's moat on edits and
trust envelopes — and schedules **parity expansions** in [planning/parity-backlog.md](./planning/parity-backlog.md) (BM25 S7, embeddings S8, language Tier B S9, …).

## Clarifications

### Session 2026-06-29

Planning-phase resolutions (Speckit clarify; encoded before analyze/implement):

- Q: BM25 / SQLite FTS for S1 search rank? → **A: Defer BM25** — S1 uses structural rank only (reference count, definition vs test); BM25 optional backlog post-S1.
- Q: Daemon alias `detect_changes` for CBM migrators? → **A: Yes with deprecation warning** — `daemon.rs` routes to `detect_impact`; compact-3 unchanged.
- Q: Team artifact compression format? → **A: zstd** — add `zstd` crate; gzip fallback documented in decision-log only if zstd rejected at S1 gate.
- Q: Snapshot v5 for ResolvedCall in S3? → **Deferred to S3 Planning Gate** (PD-01).
- Q: `get_architecture` tool vs repo map mode? → **Deferred to S5 Planning Gate** (PD-02).

## Sprint Map

| Sprint | Theme | User stories | Target release |
|--------|-------|--------------|----------------|
| **S0** | Spike & falsifiers | — | Gate go/no-go |
| **S1a** | Impact + team artifact | US1–US2 | 8.10.0 |
| **S1b** | Search rank + hooks | US3–US4 | 8.10.1 |
| **S2** | Graph query layer | US5–US7 | 8.11.x |
| **S3** | Hybrid resolution | US8–US9 | 8.12.x |
| **S4** | Algorithmic semantic | US10 | 8.13.x |
| **S5** | Cross-service & architecture | US11–US12 | 8.14.x |
| **S6** | Operational parity | US13–US15 | 8.15.x |

Detailed task breakdown: [tasks.md](./tasks.md). Sprint calendar: [sprints.md](./sprints.md).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Change impact with blast radius (Priority: P1) — Sprint 1

An agent asks "what breaks if I ship my current changes?" and receives changed
files, affected symbols, inbound caller blast radius, and risk classification in
one response — without manually chaining `what_changed`, `find_references`, and
grep.

**Independent Test**: Make local edits on a symbol with known callers; call impact
tool; confirm changed symbols + N-hop callers + risk labels.

**Acceptance Scenarios**:
1. **Given** uncommitted + untracked + branch-diff changes, **When** impact runs,
   **Then** all three sources appear in changed files.
2. **Given** a changed function, **When** depth=2, **Then** inbound callers within
   2 hops return with distance and risk tier.
3. **Given** impact results, **When** returned, **Then** frecency is not bumped.

### User Story 2 — Team-shared index artifact (Priority: P1) — Sprint 1

A team commits a compressed index artifact beside the repo; a new clone bootstraps
from the artifact then runs incremental indexing for local diffs only.

**Independent Test**: Export artifact → fresh clone → import → incremental index
completes in <20% of full index time on symforge repo.

**Acceptance Scenarios**:
1. **Given** explicit checkpoint with export, **When** complete, **Then**
   `.symforge/index.bin.zst` exists with integrity metadata.
2. **Given** artifact present and no local snapshot, **When** index starts, **Then**
   import runs before incremental walk.
3. **Given** first export, **When** written, **Then** `.gitattributes merge=ours`
   line is created for the artifact path (opt-in commit).

### User Story 3 — Graph-augmented search ranking (Priority: P1) — Sprint 1

Text search results rank by structural importance (high caller count, definitions
before tests) and offer compact signature mode.

**Independent Test**: Grep-common term returns deduplicated symbol hits ranked by
inbound reference count; `mode=compact` omits bodies.

### User Story 4 — Pagination honesty + hook augment (Priority: P1) — Sprint 1

Search and reference tools return structured `{ total, returned, offset, has_more }`;
Claude Code Grep/Glob hooks inject symbol matches as non-blocking additionalContext.

**Independent Test**: Query exceeding cap sets `has_more=true`; hook completes
<100ms fail-open with matches when index ready.

### User Story 5 — In-memory graph projection (Priority: P1) — Sprint 2

LiveIndex exposes a rebuildable directed graph (nodes = symbols, edges = references
+ resolved calls) for BFS without SQLite.

**Independent Test**: Build graph from fixture index; BFS depth-5 inbound on known
symbol returns expected path in <50ms on symforge repo.

### User Story 6 — Multi-hop trace (Priority: P1) — Sprint 2

Agents trace call chains inbound/outbound/both with depth 1–5 via STEL `trace` intent
and full-surface `trace_path` tool.

**Independent Test**: `trace_path(name, direction=inbound, depth=3)` matches
manual `find_references` chain on fixture.

### User Story 7 — Graph query subset (Priority: P2) — Sprint 2

Power users run read-only graph queries (MATCH/WHERE/RETURN/LIMIT, NOT EXISTS,
count) compiled to in-memory iterators — not SQL.

**Independent Test**: Dead-code pattern `NOT EXISTS inbound CALLS` returns known
zero-caller fixture symbols.

### User Story 8 — Hybrid resolver: Rust (Priority: P1) — Sprint 3

Call edges from Rust code resolve to defining symbols across modules with confidence
scores — no external rust-analyzer process.

**Independent Test**: ≥80% resolved calls on idiomatic symforge `src/` sample set.

### User Story 9 — Hybrid resolver: TypeScript/JavaScript (Priority: P1) — Sprint 3

Import-aware method and function call resolution for TS/JS/TSX with cross-file
registry merge.

**Independent Test**: Fixture monorepo resolves cross-file `foo.bar()` to defining
method.

### User Story 10 — Algorithmic semantic relations (Priority: P2) — Sprint 4

Index-time `SemanticallyRelated` edges from TF-IDF + signature features (no
embeddings required for v1); discovery via STEL find with optional keyword list.

**Independent Test**: Vocabulary-mismatch pair ("publish"/"send") linked at index
time; find intent surfaces related symbol.

### User Story 11 — HTTP route & cross-service edges (Priority: P2) — Sprint 5

REST route handlers and HTTP client call sites link as first-class edges (Axum,
Express, common frameworks first).

**Independent Test**: Fixture axum route + reqwest call produces linkable edge in
graph query.

### User Story 12 — Architecture clusters (Priority: P2) — Sprint 5

Deep repo map / architecture view includes call-graph community clusters with
cohesion score and representative symbols.

**Independent Test**: `get_architecture` or deep map returns ≥1 cluster with
member count on symforge repo.

### User Story 13 — ADR persistence (Priority: P3) — Sprint 6

Architecture Decision Records survive reindex via `.symforge/adr.json` and MCP
resource `symforge://repo/adr`.

**Independent Test**: Write ADR → reindex → read ADR unchanged.

### User Story 14 — Diagnostics & index modes (Priority: P3) — Sprint 6

`SYMFORGE_DIAGNOSTICS=1` emits NDJSON memory trajectory; `index_folder(mode=)`
selects fast/standard/deep cost/quality.

**Independent Test**: Diagnostics file grows while server runs; fast mode skips
deep passes with documented delta.

### User Story 15 — CLI mirror & trace ingest (Priority: P3) — Sprint 6

Selected tools invokable via `symforge cli <tool> '<json>'`; OTLP trace JSON can
boost HTTP edge confidence (ingest only, no runtime collector).

**Independent Test**: CLI trace_path matches MCP output; ingest marks edge
validated.

## Requirements *(mandatory)*

### Functional Requirements

**Sprint 1**
- **FR-001**: System MUST expose change impact combining git diff sources
  (committed, unstaged, untracked) with symbol mapping and N-hop caller BFS.
- **FR-002**: System MUST support optional export/import of zstd-compressed index
  artifacts under `.symforge/` with integrity verification.
- **FR-003**: System MUST rank text search hits using structural signals (reference
  count, definition vs test classification).
- **FR-004**: System MUST return structured pagination fields on search and
  reference tools.
- **FR-005**: Hook augment MUST inject symbol context on Grep/Glob without blocking
  or gating Read tools.

**Sprint 2**
- **FR-006**: Graph projection MUST rebuild from LiveIndex on load; MUST NOT be a
  second authoritative store.
- **FR-007**: System MUST expose multi-hop trace with direction and depth caps.
- **FR-008**: Graph query MUST fail closed with explicit errors for unsupported
  syntax; MUST NOT return empty results on parse failure.

**Sprint 3**
- **FR-009**: Hybrid resolver MUST run in-process; MUST NOT spawn language servers.
- **FR-010**: Resolved calls MUST store confidence and strategy metadata.

**Sprint 4**
- **FR-011**: Semantic relations MUST be computable without network or API keys.
- **FR-012**: Semantic discovery MUST NOT bump frecency.

**Sprint 5**
- **FR-013**: Route detection MUST cover at least Rust (axum) and TypeScript
  (express) in v1.
- **FR-014**: Architecture clusters MUST derive from call/import adjacency only.

**Sprint 6**
- **FR-015**: ADR CRUD MUST survive full reindex.
- **FR-016**: Diagnostics MUST contain no source code or query text.
- **FR-017**: CLI mirror MUST produce equivalent JSON to MCP tools for mirrored
  commands.

### Non-Functional Requirements

- **NFR-001**: Single authoritative LiveIndex preserved (Constitution I).
- **NFR-002**: Discovery/search/trace/impact paths frecency-neutral (Constitution V).
- **NFR-003**: `embed` feature compiles without new server deps (Constitution VI).
- **NFR-004**: stdio and serve return equivalent results (Constitution VII).
- **NFR-005**: Full backend verification gate before each sprint merge.

### Key Entities

- **GraphProjection**: Derived adjacency over symbols; rebuilt from index.
- **ResolvedCall**: Caller/callee pair + confidence + strategy.
- **ImpactResult**: Changed files, symbols, blast nodes, risk labels.
- **IndexArtifact**: Compressed snapshot + metadata + tier (fast/best).
- **SemanticEdge**: Pair of symbols + combined score + signal breakdown.
- **RouteNode**: HTTP method/path linked to handler symbol.
- **ArchitectureCluster**: Community id, members, cohesion, representatives.
- **AdrDocument**: Project-scoped markdown sections with content hash.

## Success Criteria *(mandatory)*

- **SC-001**: Impact tool returns blast radius for 100% of changed symbols with
  parseable definitions in integration fixtures.
- **SC-002**: Team artifact bootstrap reduces cold-start index time by ≥80% on
  symforge repo (operator-measured).
- **SC-003**: Multi-hop trace depth-3 completes in <100ms on symforge repo (p95).
- **SC-004**: Rust resolver ≥80% on symforge `src/` benchmark set.
- **SC-005**: Semantic keyword bridging finds ≥1 related symbol for 90% of curated
  vocabulary-mismatch pairs in fixture set.
- **SC-006**: Zero constitution gate failures across all sprint PRs.
- **SC-007**: Compact-3 default surface unchanged unless STEL intents absorb new
  capabilities (no schema budget regression per surface_list tests).

## Assumptions

- CBM source at `E:/project/codebase-memory-mcp` remains reference-only; no code
  copy; algorithms reimplemented in Rust under SymForge conventions.
- Feature 012 multi-project daemon work continues in parallel; graph tools are
  project-scoped first, cross-project in Phase 3 of 012.
- **BM25 / CBM-style SQLite FTS** — [parity-backlog PB-01](./planning/parity-backlog.md) (S7); S1 structural rank only.
- Embeddings (Nomic-style) — [PB-02](./planning/parity-backlog.md) (S8); after S4 algorithmic semantic.
- **Language long-tail** — [PB-03 Tier B](./planning/parity-backlog.md) (S9); Tier A (~17) stays deep in S0–S6.
- 3D graph UI explicitly out of scope.

## Dependencies

- **012-harness-agnostic-mcp**: project binding, cross-project reads (partial).
- **007-intelligence-pattern-ports**: impact footer, ranked map (complementary).
- **011-ccr-output-compression**: bulk discovery output for graph search results.

## Out of Scope

- SQLite graph as query authority
- BM25 / FTS5 in S1 only (structural rank for 8.10.x; BM25 scheduled as parity expansion)
- Shallow clone of CBM **mechanisms** that are inferior on SymForge (see [superiority doctrine](#superiority-doctrine))
- Symbol editing in graph tools (SymForge edit stack remains sole mutation path)
- 158-language breadth sprint (incremental grammar expansion only)
- Built-in LLM query translation
- 3D graph visualization UI
