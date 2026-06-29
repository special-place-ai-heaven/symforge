# Contract: detect_impact

**Feature**: 015 · **Sprint**: S1a · **US**: US1  
**Status**: candidate freeze (S1a Planning Gate)  
**Evidence**: EV-S1-001, EV-S1-002, EV-S1-CBM-001

## Tool surface

**Full**: `detect_impact`  
**STEL**: `symforge` intent=`impact` (upgraded from what_changed chain)

## Input

| Field | Required | Default |
|-------|----------|---------|
| `base_branch` | no | `main` |
| `since` | no | overrides base_branch (git ref) |
| `depth` | no | 2 (max 5, clamp + warn) |
| `scope` | no | `symbols` (alt: `files`) |
| `include_untracked` | no | true |

### Rust struct (implementation)

```rust
struct DetectImpactInput {
    base_branch: Option<String>,
    since: Option<String>,
    depth: u8,
    scope: ImpactScope,  // Files | Symbols
    include_untracked: bool,
}
```

Git path union: `GitRepo::merge_git_changed_paths` (see sprint-1a spec P-S1A-008).

## Git sources (merged, deduped)

1. `git diff <base>...HEAD --name-only`
2. `git diff --name-only` (unstaged)
3. `git status --porcelain` (untracked + staged-new)

## Output

```json
{
  "changed_files": ["..."],
  "changed_symbols": [{"name","path","kind"}],
  "blast_radius": [{"symbol","hop","risk"}],
  "risk_summary": {"critical":0,"high":1,"medium":2,"low":5},
  "pagination": {"total":...,"returned":...,"offset":0,"has_more":false}
}
```

## Risk tiers

| Hops | Default tier |
|------|--------------|
| 0 | (changed symbol — not in blast list) |
| 1 | High |
| 2 | Medium |
| 3+ | Low |
| entry_point at hop 1 | Critical |

## Non-goals

- Does not re-index files (use `analyze_file_impact` for that).
- Does not bump frecency.

## Backward compatibility

Daemon routes `detect_changes` → this tool with deprecation warning (D-015-012).
