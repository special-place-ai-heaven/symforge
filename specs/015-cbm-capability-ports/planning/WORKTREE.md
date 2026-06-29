# Isolated worktree — Program 015

**Branch**: `015-cbm-capability-ports`  
**Path**: `E:/project/symforge-015`  
**Main repo**: `E:/project/symforge` (stay on `main` for other features)

## Why

015 planning + implementation runs here so parallel agents on `main` are not blocked by
spec churn, ignored spike tests, or CLAUDE.md Speckit pointer changes.

## Agent rules

| Do here (symforge-015) | Do on main (symforge) |
|------------------------|------------------------|
| All `[P]` / `[C]` / `[V]` for program 015 | 012, 013, unrelated fixes |
| SymForge MCP index for 015 dogfood | Other feature work |
| PRs from `015-cbm-capability-ports` | PRs from other branches |

## Commands

```powershell
# Open this worktree (already created)
cd E:/project/symforge-015

# List worktrees
git -C E:/project/symforge worktree list

# Sync with main before long sessions
git fetch origin main
git merge origin/main

# Push feature branch (when ready)
git push -u origin 015-cbm-capability-ports
```

## Sibling worktrees (repo convention)

| Path | Branch |
|------|--------|
| `E:/project/symforge` | `main` |
| `E:/project/symforge-012` | overlay refactor |
| `E:/project/symforge-perl` | perl grammar |
| `E:/project/symforge-015` | **015 CBM ports** |

## Speckit

`.specify/feature.json` → `specs/015-cbm-capability-ports` (local, per worktree)  
`CLAUDE.md` SPECKIT block points at `specs/015-cbm-capability-ports/plan.md` in this tree only.
