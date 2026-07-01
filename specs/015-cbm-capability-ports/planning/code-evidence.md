# Code Evidence Registry — Program 015

**Rule**: Every planning claim and every `[C]` task MUST cite a row here (or add one via SymForge first).

**Verification method**: SymForge MCP (`symforge`, `status`) on `E:/project/symforge` unless noted.
CBM rows use clone path + grep until CBM `src/` is indexed (see [§ Indexing CBM](#indexing-cbm)).

**Last verified**: 2026-06-29 · **index**: 573 files, 19,476 symbols · **symforge_version**: 8.9.7

> **Drift is expected.** SymForge is under active development; line numbers and even
> symbols move. Evidence rows are **point-in-time**, not permanent truth. See
> [§ Drift policy](#drift-policy).

---

## Indexing CBM

SymForge MCP is bound to `project_root: E:/project/symforge`. For CBM code-backed planning:

1. Operator indexes `E:/project/codebase-memory-mcp/src` via daemon `index_folder` (full surface) OR
2. `[P]` tasks cite CBM clone paths with line anchors from direct read (secondary)

**Target**: Before S1 Planning Gate, run SymForge on CBM `src/mcp/mcp.c` and record anchors in § CBM below.

---

## Evidence ID index

| ID | Sprint | Topic |
|----|--------|-------|
| EV-S1-001 | S1 | STEL impact routing (planner.rs) |
| EV-S1-002 | S1 | what_changed + git.rs primitives |
| EV-S1-003 | S1 | persist / checkpoint / quarantine |
| EV-S1-004 | S1 | hooks + pagination gaps |
| EV-S2-001 | S2 | graph.rs greenfield + BFS reuse |
| EV-S3-001 | S3 | xref + resolver greenfield |
| EV-S1-CBM-001 | S1 | CBM detect_changes |
| EV-S2-CBM-001 | S2 | CBM BFS |

---

## S1 — US1 detect_impact (SymForge-verified)

### Gap: no unified detect_impact

| Claim | SymForge evidence | Gap vs CBM |
|-------|-------------------|------------|
| STEL `impact` → `find_dependents` by path | `route_impact` @ `src/stel/planner.rs:1020-1031` | CBM merges git + blast BFS + risk |
| STEL `impact` + symbol → `find_references` | `symbol_impact_step` @ `src/stel/planner.rs:358-384` | No git diff |
| `what_changed` git modes | `determine_what_changed_mode` @ `src/protocol/tools.rs:612-650` (SymForge search_text 2026-06-29) | Files only, no symbol blast |
| `what_changed` handler | `what_changed` @ `src/protocol/tools.rs:6473+` | Separate tool |
| Edit footer dependents (007) | `edit_impact_summary` @ `src/protocol/format.rs:5715-5732` | Post-edit only, not git impact |

**SymForge trace** (`capture_find_dependents_view`):
- Defined: `src/live_index/query.rs:1654`
- Call sites (SymForge trace 2026-06-29): `format.rs:5722` (edit_impact_summary), `tools.rs:7351` (find_dependents), `query.rs:2104` (capture_trace_symbol_view)
- Test: `tests/impact_intent.rs:148` (impact envelope + co-change)

### Git primitives already exist (reuse in C-S1A-001)

| API | Location | Notes |
|-----|----------|-------|
| `GitRepo::uncommitted_paths` | `src/git.rs:68-87` | **Includes untracked** via git2 status |
| `GitRepo::untracked_paths` | `src/git.rs:90+` | Untracked only |
| `GitRepo::changed_paths_between_refs` | `src/git.rs:121-157` | For `since` / base branch |
| `collect_diff_paths` | `src/git.rs:355-365` | Helper |
| Tests untracked | `test_untracked_paths_returns_only_worktree_new_files` @ `src/git.rs:536+` | CBM #520 parity exists |

**Planning implication**: `detect_impact` = **new** `merge_git_changed_paths` combining ref diff + uncommitted (may wrap existing APIs, not duplicate porcelain).

### CBM reference (clone — grep verified 2026-06-29)

| ID | API | Location |
|----|-----|----------|
| EV-S1-CBM-001 | `handle_detect_changes` | `E:/project/codebase-memory-mcp/src/mcp/mcp.c:4436` |
| EV-S2-CBM-001 | `cbm_store_bfs` (trace in/out) | `mcp.c:1306`, `mcp.c:1311` |
| EV-S2-CBM-002 | `cbm_store_bfs` (search_graph) | `mcp.c:2567` |
| EV-S1-CBM-002 | `handle_search_graph` | `mcp.c:1670` |
| EV-S1-CBM-003 | dispatch routes | `mcp.c:4800`, `mcp.c:4829` |

---

## S1 — US2 team artifact (SymForge-verified)

| Claim | SymForge evidence | Gap |
|-------|-------------------|-----|
| Snapshot v4 postcard | `CURRENT_VERSION = 4` @ `src/live_index/persist.rs:25` (SymForge search_text 2026-06-29) | No zstd export |
| Atomic write | `write_snapshot` @ `persist.rs:194-239` | — |
| Quarantine pattern | `quarantine_bad_snapshot` @ `persist.rs:241-301` | Reuse for bad artifact |
| Checkpoint API | `checkpoint_now` @ `src/protocol/tools.rs:6186+` | No `export_artifact` param |
| Quarantine dir | `paths.rs:121-128` `.symforge/quarantine/index-snapshots` | — |
| **zstd dep** | `Cargo.toml` — **NOT PRESENT** (grep 2026-06-29) | D-015-009 **closed** — add at C-S1A-005 |

**CBM reference**: `pipeline/artifact.c` (zstd two-tier)

---

## S1 — US3 search rank (SymForge-verified)

| Claim | SymForge evidence |
|-------|-------------------|
| Text search engine | `src/live_index/search.rs` |
| Trigram index | `src/live_index/trigram.rs` (via store) |
| Symbol tiers | `SymbolMatchTier` in search.rs |
| No structural rank by in-degree on text hits | **Gap** — implement C-S1B-001 |

---

## S1 — US4 hooks + pagination (SymForge-verified)

| Claim | SymForge evidence | Gap |
|-------|-------------------|-----|
| PreToolUse suggestions | `pre_tool_suggestion` @ `src/cli/hook.rs:586-612` | Text hints only, **no index inject** |
| Grep workflow | `HookWorkflow::SourceSearch` @ `hook.rs:632` | CBM injects graph hits |
| Fail-open | `fail_open_json` @ `hook.rs:805+` | — |
| Sidecar HTTP | `endpoint_for` @ `hook.rs:738+` | Extend for symbol lookup |
| Pagination on search | **Gap** — no `has_more` struct in format.rs | C-S1B-003 |

---

## S2 — Graph layer (SymForge-verified)

| Claim | SymForge evidence | Gap |
|-------|-------------------|-----|
| `live_index/graph.rs` | **DOES NOT EXIST** (glob 2026-06-29) | **Greenfield** C-S2-001 |
| Single-hop refs | `find_references_for_name` @ `query.rs:2343+` | No BFS |
| Overlay refs | `view.rs:533+` find_references overlay | Multi-hop deferred |
| STEL trace → find_references | `route_trace` @ `planner.rs:1000-1017` | Upgrade to trace_path |

**CBM**: `cbm_store_bfs` @ `mcp.c:1306,2567` + `store/store.c`

---

## S3 — Resolver (SymForge-verified)

| Claim | SymForge evidence | Gap |
|-------|-------------------|-----|
| Xref extraction | `extract_references` @ `xref.rs:1002+` | Syntactic only |
| Per-lang queries | `RUST_XREF_QUERY` @ `xref.rs:13-42`, etc. | No type resolution |
| `parsing/resolver/` | **DOES NOT EXIST** | Greenfield C-S3-* |
| Reference kinds | `ReferenceKind::Call` in `domain/index.rs` | No ResolvedCall type yet |

**CBM**: `internal/cbm/lsp/rust_lsp.c` (~3300 LOC)

---

## S4–S6 — Semantic / routes / ops (SymForge-verified)

| Module | Exists? | Evidence |
|--------|---------|----------|
| `live_index/semantic.rs` | **NO** | Greenfield S4 |
| `live_index/cluster.rs` | **NO** | Greenfield S5 |
| `parsing/routes/` | **NO** | Greenfield S5 |
| `manage_adr` tool | **NO** | grep tools.rs — absent |
| `cli/mirror.rs` | **NO** | Greenfield S6 |
| Deep index mode | **NO** | `index_folder` no mode param yet |

---

## STEL routing map (SymForge-verified baseline)

| Intent | Function | Tool today | 015 target |
|--------|----------|------------|------------|
| trace | `route_trace` L1000 | `find_references` | `trace_path` |
| impact | `route_impact` L1020 | `find_dependents` | `detect_impact` |
| impact+symbol | `symbol_impact_step` L358 | `find_references` | part of detect_impact |
| find | `route_find` L953 | search_* fusion | + semantic keywords S4 |
| orient | `route_orient` L1034 | `get_repo_map` | + clusters S5 |

---

## Existing assets to reuse (do not rewrite)

| Asset | Location | Used by |
|-------|----------|---------|
| Impact footer | `format.rs:5715+`, `edit_tools.rs` append | US1 blast display pattern |
| 007 co-change | `git_temporal.rs`, `edit_impact_summary` | detect_impact co-change line |
| Idempotency | `idempotency.rs` | detect_impact if mutating (no — read-only) |
| CCR compression | `protocol/ccr.rs` | large trace/query results S2 |
| Frecency guard tests | `tests/frecency_ranking.rs` | extend per US |

---

## SymForge dogfood log (append per [P] task)

| Date | Query | Result summary |
|------|-------|----------------|
| 2026-06-29 | MCP `status` | index_ready=true, 574 files, v8.9.7 |
| 2026-06-29 | `trace capture_find_dependents_view` | 13 refs; format.rs:5722, tools.rs:7351 |
| 2026-06-29 | `find graph.rs` in live_index | empty — confirms greenfield S2 |
| 2026-06-29 | grep symforge src | no graph.rs, no parsing/resolver/, no zstd in Cargo.toml |
| 2026-06-29 | symforge read `tools.rs` what_changed | WhatChangedInput L447; determine_what_changed_mode L612 |

---

## How to add evidence (required workflow)

For each `[P]` task that claims "SymForge has X" or "CBM does Y":

```text
1. symforge intent=read path=<file> query=<topic>
2. OR symforge intent=trace symbol=<fn>
3. OR symforge intent=find path=<dir> query=<terms>
4. Record: EV id, file, symbol, line hint, verified date, symforge_version
5. Link row ID in sprint spec (e.g. EV-S1-012)
```

For each `[C]` task:

```text
1. Confirm EV-* row exists for touch point
2. If new file, add "WILL CREATE" row before coding
3. After merge, re-run symforge read to refresh anchors
```

---

## Drift policy

SymForge and this program evolve in parallel. The registry stays useful if we treat
**intent** as stable and **lines** as disposable hints.

### Stable keys (what we preserve)

| Priority | Anchor | Example |
|----------|--------|---------|
| 1 | `EV-*` ID + claim | "STEL impact → find_dependents, not git blast" |
| 2 | File + symbol/handler | `route_impact` in `src/stel/planner.rs` |
| 3 | SymForge query that re-resolves | `trace route_impact`, `find merge_git` |
| 4 | Line range | `planner.rs:1020-1031` — refresh often |

Line numbers **will** drift. A moved function is not a planning failure; a missing
function or renamed responsibility **is**.

### When to refresh

| Trigger | Action |
|---------|--------|
| Sprint **Planning Gate** | Re-run SymForge on every `EV-*` row for that sprint; bump `verified` date |
| Any `[C]` merge touching a cited file | Refresh affected rows before Release Gate |
| SymForge **minor** version bump | Spot-check S1+ rows if gate is within 2 weeks |
| SymForge **major** / STEL routing change | Full refresh of all SymForge rows before next `[C]` |
| Claim no longer true (greenfield shipped, tool renamed) | Update claim + contract; do not silently delete EV row |

### Gate pass with drift

Planning Gate **passes** when SymForge still resolves the same **symbol or gap**,
even if lines shifted. **Fails** only when:

- SymForge cannot find the cited symbol/handler, **and**
- No replacement anchor is recorded, **or**
- The underlying claim (reuse vs greenfield, gap vs shipped) changed without spec update

Record refresh in the dogfood log: `verified=<date> symforge=<version> drift=lines|symbol|claim`.

---

## Planning gate requirement

Sprint Planning Gate **FAIL** if any `[C]` task lacks ≥1 EV row or any US acceptance row lacks SymForge-confirmed touch point.
