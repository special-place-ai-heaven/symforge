# Acceptance Matrix — Program 015

**Format**: Each row is independently verifiable in `[V]` phase. Fixtures MUST exist
before `[C]` starts (created in `[P]` phase).

## US1 — detect_impact (S1)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US1-01 | Fixture repo with fn `core` called by 3 modules | Uncommitted edit to `core` + `detect_impact(depth=2)` | Response lists `core` + ≥3 blast nodes | `tests/fixtures/cbm_impact/` | blast count ≥3 |
| A-US1-02 | New untracked file with fn | impact run | File in `changed_files` | same + untracked rs | CBM #520 parity |
| A-US1-03 | `since=HEAD~1` | impact run | Committed diff files included | git 2 commits | — |
| A-US1-04 | Any impact call | after response | frecency DB unchanged | frecency test | 0 bumps |
| A-US1-05 | Entry-point symbol changed | depth=1 | Risk tier Critical or High | fixture w/ main | tier match |

## US2 — Team artifact (S1)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US2-01 | Indexed fixture | export best + delete index.bin | `.symforge/index.bin.zst` exists | cbm_impact | file exists |
| A-US2-02 | zst only, no bin | cold load | Import + stat-check passes | same | hash match |
| A-US2-03 | Corrupt zst | import | Quarantine + full rebuild path | corrupt bytes | health warns |
| A-US2-04 | First export | complete | `.gitattributes merge=ours` line | — | line present |

## US3 — Graph-augmented search (S1)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US3-01 | High in-degree symbol + test fn match grep | search_text same term | Definition ranks above test | cbm_impact | order |
| A-US3-02 | mode=compact | search_text | No full body lines | — | no `{` blocks |
| A-US3-03 | mode=files | search_text | Paths only | — | format |

## US4 — Pagination + hooks (S1)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US4-01 | Query with >limit hits | search_symbols | `has_more=true`, `total>returned` | symforge index | fields set |
| A-US4-02 | Hook Grep for indexed symbol | hook run | additionalContext non-empty | hook test | <100ms |
| A-US4-03 | Sidecar down | hook run | Exit 0 empty JSON | fail-open | exit 0 |
| A-US4-04 | Read tool hook | never intercepted | — | policy | no gate |

## US5 — GraphProjection (S2)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US5-01 | Known index state | build graph twice | Identical edge count + order | unit | deterministic |
| A-US5-02 | Single file update | patch graph | Edge delta only for file | incremental | — |
| A-US5-03 | symforge repo | BFS depth-5 | p95 latency | spike | <100ms p95 |

## US6 — trace_path (S2)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US6-01 | Chain A→B→C | trace inbound depth=3 | Path [C,B,A] or equivalent | cbm_impact | golden file |
| A-US6-02 | Two same-named fns | trace without path | Disambiguation error | — | error text |
| A-US6-03 | STEL intent=trace | symforge call | Routes to trace_path | MCP | tool name |

## US7 — query_graph (S2)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US7-01 | Zero-caller fn in fixture | dead-code Cypher | Returns that fn | cbm_impact | match |
| A-US7-02 | MERGE clause | query | `unsupported:` error | — | fail-closed |
| A-US7-03 | Resource fetch | graph-schema | Labels + counts | MCP resource | — |

## US8 — Rust resolver (S3)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US8-01 | cbm_resolver_rust fixture | resolve all calls | ≥80% match manifest | manifest json | SC-004 |
| A-US8-02 | Unresolved call | resolve | confidence <1, strategy Unresolved | — | metadata |
| A-US8-03 | Snapshot save/load | round-trip | ResolvedCall preserved | v5 snapshot | — |

## US9 — TS resolver (S3)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US9-01 | TS monorepo fixture | cross-file call | Resolves to defining method | cbm_resolver_ts | ≥75% |

## US10 — Semantic (S4)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US10-01 | publish/send modules | deep index | SemanticallyRelated edge | cbm_semantic | edge exists |
| A-US10-02 | STEL find + keywords | query | Related symbol in results | — | rank |
| A-US10-03 | find call | after | frecency unchanged | — | 0 bumps |

## US11 — HTTP routes (S5)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US11-01 | axum fixture | index + query | Route node linked to handler | cbm_routes_axum | edge |

## US12 — Clusters (S5)

| ID | Given | When | Then | Fixture | Metric |
|----|-------|------|------|---------|--------|
| A-US12-01 | symforge index | get_architecture/map | ≥1 cluster w/ count | integration | count≥1 |

## US13–US15 (S6)

| ID | Given | When | Then |
|----|-------|------|------|
| A-US13-01 | ADR written | reindex | ADR content unchanged |
| A-US14-01 | DIAGNOSTICS=1 | 30s run | ndjson lines with rss only |
| A-US15-01 | cli trace_path | same input as MCP | JSON equivalent |

## Constitution acceptance (all sprints)

| ID | Rule | Verification |
|----|------|--------------|
| A-CONST-01 | No second query authority | grep new sqlite graph store — must be empty |
| A-CONST-02 | Frecency neutral discovery | `tests/frecency_ranking.rs` extended |
| A-CONST-03 | embed builds | `cargo check --features embed` |
| A-CONST-04 | Compact-3 budget | `surface_list` test |
