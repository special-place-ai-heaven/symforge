# Sprint 1a Planning Spec — Impact + artifact

**Status**: **planning gate signed** 2026-06-30 (coding gated on S0 GO)  
**Release**: 8.10.0  
**User stories**: US1–US2 only  
**Depends on**: S0 GO ([research.md](../research.md) § Spike Results)

**S1b** (US3–US4): [sprint-1b-search-hooks-spec.md](./sprint-1b-search-hooks-spec.md)

## Objective

Ship highest agent-visible value for **git-aware change impact** and **zstd team artifact**
before search/hooks (S1b) and graph DSL (S2+).

## In scope

| US | Tool / surface | Contract | Evidence |
|----|----------------|----------|----------|
| US1 | `detect_impact`, STEL `impact` upgrade | [detect-impact.md](../contracts/detect-impact.md) | EV-S1-001, EV-S1-002, EV-S1-CBM-001 |
| US2 | artifact export/import | [team-artifact.md](../contracts/team-artifact.md) | EV-S1-003 |

## Out of scope (explicit)

| Item | Sprint |
|------|--------|
| Search rank, pagination, hook augment | S1b |
| IndexMode Fast/Standard/Deep | S4 |
| trace_path, query_graph | S2 |
| BM25 SQLite FTS | S7 (D-015-011) |

## Resolved decisions (no Planning Gate reopen)

| ID | Decision | Record |
|----|----------|--------|
| D-015-009 | zstd for artifact | decision-log |
| D-015-011 | S1 structural rank only; BM25 deferred | decision-log |
| D-015-012 | Daemon alias `detect_changes` → `detect_impact` + warn | decision-log |

## SymForge code evidence

Confirm before **S1a Planning Gate**: rows **EV-S1-001..003**, **EV-S1-CBM-001** in [code-evidence.md](./code-evidence.md).

Operator benchmarks: [benchmark-intake.md](./benchmark-intake.md) (parallel track).

## CBM deep-read list ([P] P-S1A-001)

| Order | File | Focus | Anchor |
|-------|------|-------|--------|
| 1 | `mcp.c` | detect_changes handler | `handle_detect_changes` L4436 |
| 2 | `artifact.c` | two-tier export | EV-S1-003 mapping |
| 3 | dispatch | tool registration | L4828–4829 |

## SymForge touch points ([P] P-S1A-002)

| File | Symbol / area | Reuse for |
|------|---------------|-----------|
| `git.rs` | `uncommitted_paths`, `changed_paths_between_refs` | C-S1A-001 merge |
| `protocol/tools.rs` | `WhatChangedInput`, `determine_what_changed_mode`, `what_changed` | parallel handler pattern |
| `stel/planner.rs` | `route_impact`, `symbol_impact_step` | STEL upgrade C-S1A-004 |
| `live_index/graph.rs` | S0 spike BFS | C-S1A-002 blast (after S0 GO) |
| `persist.rs` | `write_snapshot`, quarantine | C-S1A-005 artifact |
| `daemon.rs` | alias table | D-015-012 |

## API — detect_impact ([P] P-S1A-007)

Rust input (mirrors [detect-impact.md](../contracts/detect-impact.md)):

```rust
struct DetectImpactInput {
    base_branch: Option<String>,  // default "main"
    since: Option<String>,        // git ref; overrides base_branch
    depth: u8,                    // default 2, max 5
    scope: ImpactScope,           // Files | Symbols (default Symbols)
    include_untracked: bool,      // default true
}
```

## Git merge helper ([P] P-S1A-008)

```rust
impl GitRepo {
    /// Union of ref-range diff + working-tree changes; repo-relative POSIX paths; deduped.
    pub fn merge_git_changed_paths(
        &self,
        base_branch: Option<&str>,
        since: Option<&str>,
        include_untracked: bool,
    ) -> Result<Vec<String>, String>;
}
```

**Algorithm**:
1. If `since` set → `changed_paths_between_refs(since, "HEAD")` (or working-tree variant if ref is `WORKTREE`).
2. Else if `base_branch` set → three-dot diff vs `HEAD`.
3. Always merge `uncommitted_paths()` when `include_untracked` (already includes untracked per EV-S1-002).
4. Dedupe, sort, reject paths outside repo root.

> **`main` default (contract):** `merge_git_changed_paths` itself treats a
> `None` `base_branch` + `None` `since` as uncommitted-only (its unit tests pin
> this low-level shape). The `detect_impact` handler applies the frozen
> contract default *before* calling the helper: when the caller supplied
> neither `base_branch` nor `since`, it substitutes `base_branch = "main"`, so
> the STEL-upgraded path (`route_impact`, which passes only `scope=files`)
> diffs against `main` rather than returning an empty blast radius on a clean
> tree with committed-but-unmerged work. A repo whose default branch is not
> `main` surfaces the step-2 ref-resolution error (`Invalid git ref`), not a
> silent empty result — pass an explicit `base_branch`/`since` there.

## STEL impact routing ([P] P-S1A-011)

| | Before (8.9.x) | After (8.10.0) |
|---|----------------|----------------|
| `symforge` intent=impact, path only | `route_impact` → `find_dependents` | `detect_impact` scope=files subset |
| intent=impact + symbol | `symbol_impact_step` → `find_references` | unchanged short path; full git blast via `detect_impact` tool |
| Full surface | N/A | `detect_impact` with blast_radius + risk_summary |

## Error catalog ([P] P-S1A-009)

| Error | When | User message |
|-------|------|--------------|
| `git unavailable` | not a git repo | use path-only / index-only mode hint |
| `invalid ref` | bad `since` / `base_branch` | reject; no shell metachar |
| `index not ready` | circuit breaker | existing loading_guard text |
| `artifact corrupt` | bad zst / hash mismatch | quarantine path + `index_folder` reset hint |
| `depth capped` | depth > 5 | clamp to 5 + warning footer |

## Fixture spec ([P] P-S1A-010)

### `tests/fixtures/cbm_impact/`

```
cbm_impact/
  Cargo.toml
  src/
    lib.rs      # pub fn core()
    a.rs, b.rs, c.rs  # each calls core()
    main.rs
  .git/         # ≥2 commits
  README.md
  expected_impact.json
```

Manifest: changed file → expected blast symbols (hop, risk tier).

## Test skeletons ([P] P-S1A-012)

- `tests/detect_impact.rs` — fixture + git unavailable + depth cap
- `tests/team_artifact.rs` — round-trip hash + quarantine path

## Acceptance

A-US1-01..05, A-US2-01..04 — [acceptance-matrix.md](./acceptance-matrix.md)

## Surface plan

| Capability | Compact STEL | Full surface |
|------------|--------------|--------------|
| impact | upgrade `impact` intent | `detect_impact` |
| artifact | — | `checkpoint_now(export_artifact=true)` |
| alias | — | `detect_changes` → detect_impact + deprecation warn |

## Risk focus

R-06 (git porcelain drift), R-14 (blast scope creep)

## Planning Gate checklist

- [x] P-S1A-001 CBM detect_changes read → EV-S1-CBM-001
- [x] P-S1A-002 SymForge what_changed + git.rs → EV-S1-001..002
- [x] P-S1A-003 Freeze detect-impact contract
- [x] P-S1A-004 CBM artifact.c → persist touch points
- [x] P-S1A-005 Freeze team-artifact contract
- [x] P-S1A-006 D-015-009 zstd confirmed
- [x] P-S1A-007 DetectImpactInput + output in contract
- [x] P-S1A-008 merge_git_changed_paths designed
- [x] P-S1A-009 Error catalog (this spec)
- [x] P-S1A-010 Fixture tree + expected_impact.json
- [x] P-S1A-011 STEL before/after (this spec)
- [x] P-S1A-012 Test skeletons on disk
- [x] P-S1A-013 file-touch-matrix S1a + risks R-06, R-14
- [x] P-S1A-014 PD-04 alias → D-015-012
- [x] **S1a Planning Gate** sign-off

**Sign-off**: Speckit agent (Claude) — all S1a `[P]` complete; detect-impact +
team-artifact contracts frozen; risk review R-06/R-14 + S1a touch-set recorded;
manual consistency pass clean (counts 159, coverage 100%). **S1a `[C]` coding
remains gated on S0 GO** ([research.md](../research.md) § Spike Results). Date: 2026-06-30

## Rollback

- Revert tool registrations in `init.rs`
- `SYMFORGE_ARTIFACT=0` disables import path
- Daemon alias removable without data loss

## Linked tasks

[tasks.md](../tasks.md) § Sprint 1a
