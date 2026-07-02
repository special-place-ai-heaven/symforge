# Contract: detect_impact

**Feature**: 015 · **Sprint**: S1a · **US**: US1  
**Status**: **frozen** 2026-06-30 (S1a Planning Gate — P-S1A-003)  
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

**Defaults (serde)**: `depth` = 2 via a custom default fn — **not** `u8::default()` (0), since depth 0 yields an empty blast radius (a silent no-op impact). Clamp to max 5 with a warning. `scope` = `Symbols`; `include_untracked` = `true`. These mirror the Input table above.

Git path union: `GitRepo::merge_git_changed_paths` (see sprint-1a spec P-S1A-008).

## Git sources (merged, deduped)

1. `git diff <base>...HEAD --name-only`
2. `git diff --name-only` (unstaged)
3. `git status --porcelain` (untracked + staged-new)

## Output

(pagination shape amended 2026-07-02 — see addendum)

```json
{
  "changed_files": ["..."],
  "changed_symbols": [{"name","path","kind"}],
  "blast_radius": [{"symbol","hop","risk"}],
  "risk_summary": {"critical":0,"high":1,"medium":2,"low":5},
  "pagination": {
    "changed_files":   {"total": N, "returned": M, "truncated": false},
    "changed_symbols": {"total": N, "returned": M, "truncated": false},
    "blast_radius":    {"total": N, "returned": M, "truncated": false}
  }
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

## Addendum — 2026-07-02 (dogfood defect Wave 1)

Defect-fix amendment to the frozen 2026-06-30 contract. The **input** is
unchanged; these clarify **default resolution** and **output bounds** the frozen
text did not specify.

### Default base resolution (Fix 6)

When the caller supplies neither `base_branch` nor `since`, the DEFAULT
substitution now resolves in this order:

1. `origin/main` if it exists (the shared remote truth), else
2. local `main`, else
3. the existing "Invalid git ref" error when neither exists.

Rationale: a local `main` that lags `origin/main` (observed 83 commits behind)
produced a confidently-wrong blast radius against a stale base. An **explicit**
caller-passed `base_branch: "main"` still means local `main` (unchanged). The
response header discloses the resolved ref (`base: origin/main`) and appends a
one-line note when local `main` differs from `origin/main`.

### Bounded output + per-list pagination (Fix 1)

A raw dump on a large repo reached 54 MB / 291K changed symbols (89.6s). Every
list in the payload is now capped at 200 entries: `changed_files`,
`changed_symbols`, and `blast_radius` (sorted by risk severity desc, then hop
asc). `risk_summary` still counts the FULL blast set.

The frozen § Output `pagination` object was a single flat
`{total, returned, offset, has_more}` that only described the blast list and did
not anticipate the `changed_files`/`changed_symbols` explosion. It is replaced by
a **per-list** shape (a documented, honest widening of the looser frozen shape):

```json
"pagination": {
  "changed_files":   {"total": N, "returned": M, "truncated": bool},
  "changed_symbols": {"total": N, "returned": M, "truncated": bool},
  "blast_radius":    {"total": N, "returned": M, "truncated": bool}
}
```
