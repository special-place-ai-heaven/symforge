# Branch isolation — Program 015

**Repo**: `E:/project/symforge` (same tree as `main`)  
**Branch**: `015-cbm-capability-ports`

## Why a branch, not a sibling folder

All 015 work happens in the normal symforge checkout on a dedicated branch.
Other agents stay on `main` (or their own branches) in **their** checkouts/worktrees —
we do not require a second symforge directory for 015.

## Agent rules

| On branch `015-cbm-capability-ports` | On `main` |
|--------------------------------------|-----------|
| Program 015 `[P]` / `[C]` / `[V]` | Other features |
| Speckit → `specs/015-cbm-capability-ports/` | Other specs |
| PRs from this branch | Other PRs |

## Commands

```powershell
cd E:/project/symforge
git checkout 015-cbm-capability-ports   # 015 work
git checkout main                       # leave 015

# Stay current with main
git fetch origin main
git merge origin/main

git push -u origin 015-cbm-capability-ports   # when ready
```

## Speckit (this branch only)

- `.specify/feature.json` → `specs/015-cbm-capability-ports`
- `CLAUDE.md` SPECKIT block → `specs/015-cbm-capability-ports/plan.md`

Switch back to `main` before changing those pointers for another program.

## Optional: worktrees for *parallel* agents

If **two agents must edit the same repo path at once**, use a worktree (e.g. `symforge-012`).
That is an operator choice for concurrency — **not** the default 015 workflow.
