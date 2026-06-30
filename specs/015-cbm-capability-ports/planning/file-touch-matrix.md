# File Touch Matrix — Program 015

**Legend**: ● new file | ◐ major change | ○ minor/wiring | — not touched

## Program-wide

| Path | S0 | S1 | S2 | S3 | S4 | S5 | S6 | Notes |
|------|----|----|----|----|----|----|-----|-------|
| `src/live_index/mod.rs` | ◐ | ○ | ◐ | ◐ | ◐ | ◐ | ○ | mod declarations |
| `src/live_index/store.rs` | ○ | ◐ | ◐ | ◐ | ◐ | ○ | ○ | graph patch, modes |
| `src/live_index/persist.rs` | ◐ | ● | ○ | ◐ | ○ | ○ | ○ | artifact, snap v5 |
| `src/live_index/query.rs` | — | ○ | ○ | ○ | ○ | ○ | — | |
| `src/live_index/search.rs` | — | ◐ | ○ | ○ | ○ | ○ | — | rank |
| `src/live_index/graph.rs` | ● | ◐ | ● | ◐ | ◐ | ◐ | ○ | core new |
| `src/live_index/cypher/` | — | — | ● | ○ | ○ | ◐ | — | new dir |
| `src/live_index/semantic.rs` | — | — | — | — | ● | ○ | — | |
| `src/live_index/cluster.rs` | — | — | — | — | — | ● | — | |
| `src/live_index/diagnostics.rs` | — | — | — | — | — | — | ● | |
| `src/live_index/traces.rs` | — | — | — | — | — | — | ● | |
| `src/domain/index.rs` | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | ○ | types |
| `src/parsing/mod.rs` | ◐ | ○ | ○ | ● | ○ | ◐ | — | resolver hook |
| `src/parsing/xref.rs` | ○ | ○ | ○ | ◐ | ○ | ○ | — | baseline |
| `src/parsing/resolver/` | ◐ | — | — | ● | ○ | ○ | — | new dir |
| `src/parsing/routes/` | — | — | — | — | — | ● | — | new dir |
| `src/git.rs` | — | ● | ○ | — | — | — | — | merged diff |
| `src/protocol/tools.rs` | — | ● | ● | ○ | ◐ | ◐ | ◐ | new tools |
| `src/protocol/format.rs` | — | ◐ | ◐ | ○ | ○ | ◐ | ○ | output |
| `src/protocol/resources.rs` | — | ○ | ◐ | — | — | ◐ | ◐ | schema, adr |
| `src/protocol/search_tools.rs` | — | ◐ | — | — | — | — | — | |
| `src/stel/planner.rs` | — | ◐ | ◐ | — | ◐ | ◐ | — | intents |
| `src/stel/handler.rs` | — | ◐ | ◐ | — | ○ | ○ | — | |
| `src/stel/surface_list.rs` | — | ○ | ○ | — | ○ | ○ | ◐ | descriptions |
| `src/cli/hook.rs` | — | ● | — | — | — | — | ○ | augment |
| `src/cli/mirror.rs` | — | — | — | — | — | — | ● | |
| `src/cli/init.rs` | — | ◐ | ◐ | ◐ | ◐ | ◐ | ◐ | tool names |
| `src/cli/mod.rs` | — | ○ | — | — | — | — | ◐ | cli subcmd |
| `src/sidecar/handlers.rs` | — | ◐ | ○ | — | — | ◐ | — | hook + arch |
| `src/daemon.rs` | — | ○ | ○ | ○ | — | — | ○ | proxy |
| `src/main.rs` | ○ | ◐ | — | — | — | — | ◐ | import, diag |
| `src/paths.rs` | — | ◐ | — | — | — | — | ◐ | adr path |
| `Cargo.toml` | ◐ | ◐ | — | — | — | — | — | zstd? |

## S1a touch set (gate blast radius — P-S1A-013, 2026-06-30)

The program-wide `S1` column above predates the S1a/S1b split. For the **S1a
Planning Gate**, the precise `[C]` file touches are:

| Path | Mark | S1a task | Note |
|------|------|----------|------|
| `src/git.rs` | ● | C-S1A-001 | `merge_git_changed_paths` (3 git sources, deduped) |
| `src/live_index/graph.rs` | ◐ | C-S1A-002 | `compute_impact` (builds on S0 spike scaffold) |
| `src/protocol/tools.rs` | ◐ | C-S1A-003,006 | `detect_impact` + `checkpoint_now(export_artifact)` |
| `src/protocol/format.rs` | ◐ | C-S1A-003 | impact output + risk summary |
| `src/stel/planner.rs`,`handler.rs` | ◐ | C-S1A-004 | `impact` intent upgrade |
| `src/live_index/persist.rs` | ● | C-S1A-005 | zstd artifact export/import |
| `src/cli/init.rs`,`src/daemon.rs` | ○ | C-S1A-007 | register tool + `detect_changes` alias (D-015-012) |
| `src/paths.rs` | ○ | C-S1A-005 | `.symforge/index.bin.zst`, `artifact.json` paths |
| `Cargo.toml` | ◐ | C-S1A-005 | add `zstd` (R-11 / D-015-009) |

S1b (deferred to its own gate P-S1B-007): `search.rs` rank, `format.rs`
pagination envelope, `cli/hook.rs` + `sidecar/handlers.rs` augment.

### Risk review (R-06, R-14)

- **R-06 Windows git porcelain diff** (M) — mitigated by the frozen
  `detect-impact.md` "Git sources" union (diff `base...HEAD` + unstaged diff +
  `status --porcelain` untracked, deduped — ports CBM #520). **Gate condition**:
  the `detect_impact` Windows CI path must assert untracked-file detection.
- **R-14 Team artifact secret leak** (M) — **mitigated**: `team-artifact.md`
  now carries a code-backed Security clause. The artifact snapshots only the
  index, which never ingests `.env`/dotfiles (`ignore::WalkBuilder` `.hidden(true)`)
  or git-ignored paths (`src/discovery/mod.rs:196–228`). Best-tier MUST NOT add
  excluded paths.

### Blast-radius answers (S1a)

1. **Snapshot version change?** NO — artifact is zstd of the existing
   `index.bin`; `ResolvedCall` (snapshot v5) deferred to S3 (R-02).
2. **New default (compact) MCP tools?** NO — `detect_impact` is full-surface +
   STEL `impact` intent; no 4th compact tool (R-05).
3. **New persistent store?** NO — artifact is a bootstrap cache, not query
   authority (Constitution I); ADR store deferred to S6.
4. **Daemon protocol change?** Minor/back-compat only — `detect_changes` →
   `detect_impact` alias with deprecation warning (D-015-012).

No YES requiring a new decision-log entry beyond the already-logged D-015-009
(zstd) and D-015-012 (alias).

## Test files (created per sprint)

| Path | Sprint | Purpose |
|------|--------|---------|
| `tests/cbm_spike_*.rs` | S0 | Spikes |
| `tests/detect_impact.rs` | S1 | US1 |
| `tests/team_artifact.rs` | S1 | US2 |
| `tests/graph_augmented_search.rs` | S1 | US3 |
| `tests/pagination_envelope.rs` | S1 | US4 |
| `tests/hook_augment.rs` | S1 | US4 |
| `tests/graph_projection.rs` | S2 | US5 |
| `tests/trace_path.rs` | S2 | US6 |
| `tests/query_graph.rs` | S2 | US7 |
| `tests/rust_resolver.rs` | S3 | US8 |
| `tests/typescript_resolver.rs` | S3 | US9 |
| `tests/semantic_edges.rs` | S4 | US10 |
| `tests/route_extraction.rs` | S5 | US11 |
| `tests/architecture_clusters.rs` | S5 | US12 |
| `tests/manage_adr.rs` | S6 | US13 |
| `tests/diagnostics_ndjson.rs` | S6 | US14 |
| `tests/cli_mirror.rs` | S6 | US15 |

## Files explicitly frozen (no touch without decision-log)

- `src/protocol/edit*.rs` — edit moat unless US requires re-index hook only
- `src/embed.rs` contract test list
- `src/stel_core/*` — economics orthogonal

## Blast radius review (before each sprint Planning Gate)

Answer in sprint spec:

1. Does this sprint change snapshot version?
2. Does it add default MCP tools (compact surface)?
3. Does it introduce new persistent stores?
4. Does it affect daemon protocol?

If any YES → decision-log entry required.
