# Tasks: CBM Capability Ports — 60% Planning · 30% Coding · 10% Validation

**Program**: 015 · **Model**: [execution-model.md](./execution-model.md)

**Hard rule**: No `[C]` until linked `[P]` done + sprint Planning Gate sign-off in sprint spec.

**Code-backed rule**: Every `[P]` task ends with an `EV-*` row in [planning/code-evidence.md](./planning/code-evidence.md) citing SymForge MCP (symforge repo) or CBM clone line anchors. See [execution-model.md](./execution-model.md) § Code-backed planning.

**Totals**: **159 tasks** — 91 `[P]` · 41 `[C]` · 27 `[V]` · **Waves**: [agent-workload.md](./planning/agent-workload.md) · **Parallel/seq**: [parallelism.md](./planning/parallelism.md)

**Planning complete**: PROG + S0 `[P]` (2026-06-29). See [planning/program-planning-gate.md](./planning/program-planning-gate.md), [analyze.md](./analyze.md).

---

## Phase PROG — Program planning (before S0)

### [P] Program-wide

- [x] P-PROG-001 Read execution-model.md + planning/README.md; confirm team agrees 60/30/10 gates.
- [x] P-PROG-002 [P] Complete risk-register.md review; assign owners column.
- [x] P-PROG-003 [P] Clone/read CBM README + arXiv abstract; note claims vs SymForge in planning/dogfood-notes.md §0.
- [x] P-PROG-004 Index symforge via MCP `status`; record baseline in sprint-0-spike-spec Baseline table + `code-evidence.md` dogfood log.
- [x] P-PROG-005 Resolve PD-03 (BM25 defer) → decision-log D-015-011.
- [x] P-PROG-006 Resolve PD-04 (detect_changes alias) → decision-log D-015-012.
- [x] P-PROG-007 Branch `015-cbm-capability-ports` in `E:/project/symforge`; `.specify/feature.json` → 015.
- [x] P-PROG-008 [P] Review constitution.md Principles I–VIII; sign checklist in checklists/requirements.md.
- [x] P-PROG-009 Bootstrap [planning/code-evidence.md](./planning/code-evidence.md): SymForge-verify S0–S1 touch points; grep CBM mcp.c anchors.

---

## Sprint 0 — Spike (planning-heavy)

**Spec**: [planning/sprint-0-spike-spec.md](./planning/sprint-0-spike-spec.md) · **Budget**: [agent-workload.md](./planning/agent-workload.md)

### Wave 1 — [P] spike design (**complete** — P-S0-001..010 in sprint-0-spike-spec.md)

### Wave 2 — [C] then [V] (one wave per agent; max 3 [C] then 3 [V])

#### [C] S0 (~20%)

- [ ] C-S0-001 Scaffold `src/live_index/graph.rs` + `mod` in `live_index/mod.rs`. **S**
- [ ] C-S0-002 Implement minimal BFS for SP-0A in `graph.rs`. **M**
- [ ] C-S0-003 Implement artifact compress/decompress stub in `persist.rs`. **M**

**STOP** — run `cargo check` before next wave.

- [ ] C-S0-004 Scaffold `src/parsing/resolver/{mod,rust}.rs` same-file only. **M**
- [ ] C-S0-005 Create fixture dirs per P-S0-006. **S**

#### [V] S0 (~10%)

- [ ] V-S0-001 Run `tests/cbm_spike_graph_bfs.rs --ignored`; record p95 in sprint-0 spec.
- [ ] V-S0-002 Run `tests/cbm_spike_artifact.rs --ignored`; record hash result.
- [ ] V-S0-003 Run `tests/cbm_spike_rust_resolver.rs --ignored`; record %; write GO/NO-GO in research.md § Spike Results.

---

## Sprint 1a — Impact + artifact (US1–US2)

**Spec**: [planning/sprint-1-quick-wins-spec.md](./planning/sprint-1-quick-wins-spec.md) (§ US1–US2) · **Release**: 8.10.0

### Wave 1 — [P] impact + artifact contracts (max 8 tasks)

- [x] P-S1A-001 Read CBM `mcp.c` detect_changes (~4415–4600); dogfood-notes → **EV-S1-CBM-001**.
- [x] P-S1A-002 SymForge MCP read `what_changed` + `git.rs` → **EV-S1-001..002**.
- [ ] P-S1A-003 Freeze [contracts/detect-impact.md](./contracts/detect-impact.md). *(candidate freeze — gate sign-off)*
- [x] P-S1A-004 Read CBM `artifact.c`; map persist.rs touch points.
- [ ] P-S1A-005 Freeze [contracts/team-artifact.md](./contracts/team-artifact.md). *(candidate freeze — gate sign-off)*
- [x] P-S1A-006 Confirm D-015-009 zstd in decision-log; Cargo.toml plan.
- [x] P-S1A-007 Design `DetectImpactInput` + output JSON in contract.
- [x] P-S1A-008 Design `merge_git_changed_paths` signature for `git.rs`.

**STOP** — mini-gate: contracts frozen?

### Wave 2 — [P] fixtures + gate

- [x] P-S1A-009 Error catalog (impact + artifact) in sprint-1 spec.
- [x] P-S1A-010 Fixture `tests/fixtures/cbm_impact/` + `expected_impact.json`.
- [x] P-S1A-011 STEL impact routing doc (planner.rs before/after).
- [x] P-S1A-012 Skeleton `tests/detect_impact.rs`, `tests/team_artifact.rs`.
- [ ] P-S1A-013 Review risks R-06, R-14; file-touch-matrix S1a column.
- [x] P-S1A-014 Resolve PD-04 alias plan in daemon.rs (if not done).
- [ ] P-S1A-015 **S1a Planning Gate** sign-off.

### Wave 3 — [C] US1 impact (max 4 [C]; 1× L)

- [ ] C-S1A-001 `merge_git_changed_paths` in `src/git.rs`. **M**
- [ ] C-S1A-002 `compute_impact` in `src/live_index/graph.rs`. **L**
- [ ] C-S1A-003 `detect_impact` handler + format in `tools.rs`, `format.rs`. **L**

**STOP** — `cargo test detect_impact` skeleton.

- [ ] C-S1A-004 STEL impact in `stel/planner.rs`, `handler.rs`. **M**

### Wave 4 — [C] US2 artifact + register (max 3 [C])

- [ ] C-S1A-005 Artifact export/import in `persist.rs`. **L**
- [ ] C-S1A-006 `checkpoint_now(export_artifact)` in `tools.rs`. **M**
- [ ] C-S1A-007 Register `detect_impact` + init.rs; daemon alias per D-015-012. **S**

### [V] S1a

- [ ] V-S1A-001 A-US1-01..05 green.
- [ ] V-S1A-002 A-US2-01..04 green.
- [ ] V-S1A-003 quickstart S1a + scoped gate.

---

## Sprint 1b — Search rank + hooks (US3–US4)

**Spec**: [planning/sprint-1b-search-hooks-spec.md](./planning/sprint-1b-search-hooks-spec.md) · **Release**: 8.10.1

### Wave 1 — [P] (max 8)

- [ ] P-S1B-001 SymForge MCP: search + hook touch points → **EV-S1-003..004**.
- [ ] P-S1B-002 CBM `search_graph` pagination in mcp.c; design PaginationEnvelope.
- [ ] P-S1B-003 CBM `hook_augment.c` → `cli/hook.rs` flow diagram.
- [ ] P-S1B-004 Design graph-augmented rank rules for `search.rs`.
- [ ] P-S1B-005 Skeleton `tests/pagination_envelope.rs`, `tests/hook_augment.rs`.
- [ ] P-S1B-006 Review risks R-07, R-10, R-11.
- [ ] P-S1B-007 **S1b Planning Gate** sign-off.

### Wave 2 — [C] (max 6; all M/S)

- [ ] C-S1B-001 Graph-augmented rank in `search.rs`. **M**
- [ ] C-S1B-002 Search `mode` param in `tools.rs`. **S**
- [ ] C-S1B-003 PaginationEnvelope in `format.rs` (+ search/find). **M**
- [ ] C-S1B-004 Hook augment in `hook.rs` + `sidecar/handlers.rs`. **M**

### [V] S1b

- [ ] V-S1B-001 A-US3-01..03 green.
- [ ] V-S1B-002 A-US4-01..03 + frecency extension for detect_impact path. **V-S1-004** merged here.
- [ ] V-S1B-003 quickstart S1b + full gate before S2.

---

## Sprint 1 — Quick wins (US1–US4) — SUPERSEDED

Split into **S1a** + **S1b** (2026-06-29 balance pass). Legacy IDs map in [agent-workload.md](./planning/agent-workload.md).

<!--
### [P] S1 Planning — archived IDs
P-S1-001..020 → P-S1A-* + P-S1B-*; P-S1-013 IndexMode → P-S4-008
### [C] S1 Coding
C-S1-001..012 → C-S1A-* + C-S1B-*; C-S1-011 IndexMode → C-S4-004
### [V] S1 Validation
V-S1-001..005 → V-S1A-* + V-S1B-*
-->

---

## Sprint 2 — Graph (US5–US7)

**Spec**: [planning/sprint-2-graph-spec.md](./planning/sprint-2-graph-spec.md) · **Budget**: [agent-workload.md](./planning/agent-workload.md)

### Wave 1 — [P] + graph core (max 8 [P])

- [ ] P-S2-001 Read CBM `store/store.c` BFS + degree; update graph-projection contract if needed.
- [ ] P-S2-002 Read CBM `cypher/cypher.c` supported subset list; freeze query-graph.md v1 grammar appendix.
- [ ] P-S2-003 Design GraphProjection rebuild/patch algorithm doc in sprint-2 spec.
- [ ] P-S2-004 Design trace_path output format (golden file example).
- [ ] P-S2-005 Design graph-schema resource markdown template.
- [ ] P-S2-006 Create fixture `tests/fixtures/cbm_cypher/` + golden trace for cbm_impact.
- [ ] P-S2-007 Write skeleton tests: `graph_projection.rs`, `trace_path.rs`, `query_graph.rs`.
- [ ] P-S2-008 Sequence diagram: STEL trace → trace_path (planner.rs).

**STOP** — mini-gate.

### Wave 2 — [P] gate + [C] graph engine (max 4 [C]; 1× L)

- [ ] P-S2-009 Review R-04; eager vs lazy graph build; decision-log.
- [ ] P-S2-010 **S2 Planning Gate** sign-off.
- [ ] C-S2-001 Complete GraphProjection in `src/live_index/graph.rs`. **L**
- [ ] C-S2-002 Hook load/patch in `persist.rs`, `store.rs`. **M**
- [ ] C-S2-003 Implement `trace_path` in `protocol/tools.rs`, `format.rs`. **L**
- [ ] C-S2-004 Upgrade STEL trace in `stel/planner.rs`. **M**

**STOP** — trace_path smoke.

### Wave 3 — [C] cypher + resource (max 2 [C]; both L → split across agents)

- [ ] C-S2-005a Cypher lexer/parser subset in `live_index/cypher/`. **L**
- [ ] C-S2-005b Cypher executor + fail-closed errors. **L**
- [ ] C-S2-006 Add `query_graph` handler + `symforge://repo/graph-schema` resource. **M**

### [V] S2

- [ ] V-S2-001 A-US5-01..03 green.
- [ ] V-S2-002 A-US6-01..03 green.
- [ ] V-S2-003 A-US7-01..03 green.
- [ ] V-S2-004 Full gate + quickstart S2 sign-off.

---

## Sprint 3 — Resolver (US8–US9)

**Spec**: [planning/sprint-3-resolver-spec.md](./planning/sprint-3-resolver-spec.md) · **Budget**: [agent-workload.md](./planning/agent-workload.md)

### Wave 1 — [P] Rust focus (max 8 [P])

- [ ] P-S3-001 Read CBM `rust_lsp.c` §1–§6; resolver-port-notes.md.
- [ ] P-S3-002 Read CBM `rust_lsp.c` §7–§12; complete resolver-port-notes.md.
- [ ] P-S3-005 Close PD-01 → D-015-008 snapshot v5 decision.
- [ ] P-S3-006 Finalize benchmark manifests (20 Rust cases first).
- [ ] P-S3-007 Design confidence disclosure in trace_path output format.
- [ ] P-S3-008 Design `SYMFORGE_RESOLVER=0` rollback flag behavior.
- [ ] P-S3-009 Skeleton `rust_resolver.rs` tests.

**STOP**

### Wave 2 — [P] TS + gate

- [ ] P-S3-003 Read CBM `ts_lsp.c` import/JSX sections; TS notes appendix.
- [ ] P-S3-004 Read CBM registry merge; design `resolver/registry.rs`.
- [ ] P-S3-010 Skeleton `typescript_resolver.rs` tests.
- [ ] P-S3-011 **S3 Planning Gate** sign-off.

### Wave 3 — [C] Rust only (max 3 [C]; 1× L)

- [ ] C-S3-001 Rust same-file + `use` resolver in `parsing/resolver/rust.rs`. **L**
- [ ] C-S3-002 `registry.rs` cross-file pass (Rust). **L**

**STOP** — Rust benchmark ≥60%.

### Wave 4 — [C] TS + wire (max 3 [C])

- [ ] C-S3-003 TypeScript resolver in `parsing/resolver/typescript.rs`. **L**
- [ ] C-S3-004 Wire resolver in `parsing/mod.rs`; store ResolvedCall. **M**
- [ ] C-S3-005 Snapshot v5 if D-015-008 yes — `persist.rs` migration test. **M**
- [ ] C-S3-006 Feed resolved edges into `graph.rs`; trace_path disclosure. **M**

### [V] S3

- [ ] V-S3-001 A-US8-01..03 green; record % in sign-off.
- [ ] V-S3-002 A-US9-01 green.
- [ ] V-S3-003 Ignored resolver smoke on symforge src.
- [ ] V-S3-004 Full gate + S3 sign-off.

---

## Sprint 4 — Semantic + index modes (US10)

**Spec**: [planning/sprint-4-semantic-spec.md](./planning/sprint-4-semantic-spec.md) · **Budget**: [agent-workload.md](./planning/agent-workload.md)

### Wave 1 — [P] (max 7)

- [ ] P-S4-001 Read CBM `semantic.c` weights + threshold env; confirm contract match.
- [ ] P-S4-002 Read CBM `minhash.c`; SIMILAR_TO → parity-backlog PB-07 or defer note.
- [ ] P-S4-003 Build fixture `cbm_semantic/` with expected edges JSON.
- [ ] P-S4-004 Design semantic pass CPU budget for Deep mode (max pairs/file).
- [ ] P-S4-005 Design STEL find keyword param schema (full surface).
- [ ] P-S4-006 Skeleton `semantic_edges.rs`, `stel_find_semantic.rs`.
- [ ] P-S4-007 Design IndexMode enum (Fast/Standard/Deep) — moved from S1; pairs with Deep. **P-S1-013**
- [ ] P-S4-008 **S4 Planning Gate** sign-off.

### Wave 2 — [C] (max 4)

- [ ] C-S4-001 Implement `src/live_index/semantic.rs`. **L**
- [ ] C-S4-002 Hook Deep mode + IndexMode in `store.rs`, `index_folder` in `tools.rs`. **M**
- [ ] C-S4-003 Semantic edges + STEL find keywords in `graph.rs`, `planner.rs`. **M**

### [V] S4

- [ ] V-S4-001 A-US10-01..03 + frecency test green.
- [ ] V-S4-002 S4 sign-off.

---

## Sprint 5 — Cross-service (US11–US12)

**Spec**: [planning/sprint-5-cross-service-spec.md](./planning/sprint-5-cross-service-spec.md) · **Budget**: [agent-workload.md](./planning/agent-workload.md)

### Wave 1 — [P] routes (max 5)

- [ ] P-S5-001 Grep CBM pipeline for Route/HTTP; list patterns in sprint-5 spec.
- [ ] P-S5-003 Design axum extractor patterns; fixture `cbm_routes_axum/`.
- [ ] P-S5-004 Design express.ts patterns; minimal fixture or defer doc.
- [ ] P-S5-007 Skeleton `route_extraction.rs`.

### Wave 2 — [P] clusters + gate

- [ ] P-S5-002 Close PD-02 + D-015-010 (architecture surface + cluster algo).
- [ ] P-S5-005 Benchmark label-prop vs Leiden on symforge index (planning measurement).
- [ ] P-S5-006 Mock architecture cluster output in format.rs sketch.
- [ ] P-S5-008 Skeleton `architecture_clusters.rs`.
- [ ] P-S5-009 **S5 Planning Gate** sign-off.

### Wave 3 — [C] (max 3)

- [ ] C-S5-001 Implement `parsing/routes/{mod,axum,express}.rs`. **L**
- [ ] C-S5-002 Implement `live_index/cluster.rs`; wire map architecture detail. **L**
- [ ] C-S5-003 Extend orient intent + prompts for clusters. **S**

### [V] S5

- [ ] V-S5-001 A-US11-01, A-US12-01 green.
- [ ] V-S5-002 S5 sign-off.

---

## Sprint 6 — Ops (US13–US15)

**Spec**: [planning/sprint-6-ops-spec.md](./planning/sprint-6-ops-spec.md) · **Budget**: [agent-workload.md](./planning/agent-workload.md)

### Wave 1 — [P] ADR + diagnostics (max 4)

- [ ] P-S6-001 Read CBM manage_adr handler; design `.symforge/adr.json` schema.
- [ ] P-S6-002 Read CBM diagnostics NDJSON format; design SymForge subset fields.
- [ ] P-S6-006 Skeleton tests: `manage_adr`, `diagnostics`.
- [ ] P-S6-007a **S6a Planning Gate** (ADR + diagnostics scope).

### Wave 2 — [P] CLI + traces

- [ ] P-S6-003 Design CLI mirror dispatch table (tool → handler fn map).
- [ ] P-S6-004 Design trace ingest JSON schema (minimal OTLP).
- [ ] P-S6-005 Document CBM parallel pipeline — 2 ideas max → parity-backlog PB-08.
- [ ] P-S6-006b Skeleton tests: `cli_mirror`, `ingest_traces`.
- [ ] P-S6-007b **S6b Planning Gate** sign-off.

### Wave 3 — [C] ADR + diagnostics (max 2)

- [ ] C-S6-001 Implement manage_adr + resource in tools.rs, resources.rs, paths.rs. **M**
- [ ] C-S6-002 Implement diagnostics.rs + main.rs env hook. **M**

### Wave 4 — [C] CLI + traces (max 2)

- [ ] C-S6-003 Implement cli/mirror.rs + cli subcommand. **M**
- [ ] C-S6-004 Implement traces.rs + ingest_traces handler. **M**

### [V] S6

- [ ] V-S6-001 A-US13..15 green.
- [ ] V-S6-002 Full program acceptance matrix review (all rows).
- [ ] V-S6-003 Program completion checklist in sprint-6 spec.

---

## Phase POLISH — [P] then [C] then [V]

### [P]

- [ ] P-POL-001 Final decision-log review; no open PD-*.
- [ ] P-POL-002 Update Obsidian program report with actuals.
- [ ] P-POL-003 Minimal AGENTS.md MCP table delta draft (review only).

### [C]

- [ ] C-POL-001 Apply init.rs tool list final sync.
- [ ] C-POL-002 frecency + surface_honesty extensions if any gaps.

### [V]

- [ ] V-POL-001 Full gate + npm test.
- [ ] V-POL-002 MCP dogfood script: impact → trace → query on symforge.
- [ ] V-POL-003 Operator release notes bullet list for 8.10–8.15.

---

## Dependency graph (sprints)

```text
PROG → S0 ─gate─→ S1a ─→ S1b ─→ S2 ─→ S3 ─→ S4
                              └─→ S5 (needs S2, S3 rec)
                              └─→ S6 (needs S1+)
```

## MVP path (minimum production value)

```text
PROG → S0 gate → S1a [P]+[C]+[V] → ship 8.10.0 → S1b → ship 8.10.1
```

Defer S2+ until S1b dogfood confirms agent value.

## Task count summary (balanced)

| Phase | [P] | [C] | [V] | Total | Agent waves |
|-------|-----|-----|-----|-------|-------------|
| PROG | 9 | 0 | 0 | 9 | 1 |
| S0 | 10 | 5 | 3 | 18 | 2 |
| S1a | 15 | 7 | 3 | 25 | 4 |
| S1b | 7 | 4 | 3 | 14 | 2 |
| S2 | 10 | 7 | 4 | 21 | 3 |
| S3 | 11 | 6 | 4 | 21 | 4 |
| S4 | 8 | 3 | 2 | 13 | 2 |
| S5 | 9 | 3 | 2 | 14 | 3 |
| S6 | 9 | 4 | 3 | 16 | 4 |
| POLISH | 3 | 2 | 3 | 8 | 1 |
| **Total** | **91** | **41** | **27** | **159** | |

*S1 split + cypher split + S3 rust/TS waves. Cap: ≤6 [C] per agent session — [agent-workload.md](./planning/agent-workload.md).*

---

## Legacy T001–T110 mapping

Original task list superseded by this file. Rough map:

- T001–T007 → P-PROG-007, P-S0-005, C-S0-005
- T008–T016 → S0 [P]/[C]/[V]
- T017+ → S1+ [C] items (planning tasks added above each group)
