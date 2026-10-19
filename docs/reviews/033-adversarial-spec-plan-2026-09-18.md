# Adversarial spec/plan review — 033-containment-durability

**Date**: 2026-09-18  
**Reviewer**: independent security-reviewer (this campaign)  
**Against**: spec/plan/tasks after analyze remediations; implementation already fanned out.

Accepted controls (folded into spec/tasks the same day):

- Windows F-001 fixture is a **directory** junction to the secret’s parent plus nested `junc/secret`. `mklink /J` cannot target a file.
- Leaf-only `symlink_metadata` is not enough: `admit_worktree_text` and watcher must refuse when `safe_repo_path` / canonicalize is outside the workspace.
- Sidecar source-read oracle asserts **index absence**, not only substring absence.
- Loopback + leftover `SYMFORGE_SIDECAR_TOKEN` must still have no Bearer (HTTP without `Authorization` is not 401).
- Feature 020 T051 pin refresh stays Phase 12 only (033 T060). Story worktrees do not edit `tests/preventive_runtime_dark_v11.rs`.

Already closed in analyze remediations before this review landed (do not re-open):

- T025/T044 comment-oracles deleted; US4 spies `sync_all` before rename; US3 `trust_store_atomic` blocks tmp create.
- T067/T068 release + npm; constitution CHECK `-j 4`.
- Whole-router Bearer except documented `/health`; empty token is missing.
- Daemon token is not on the DurableFile tmp+rename sequence.
- Phase 2 helpers marked done; do not re-add `bind_policy.rs`.

Deferred to story implementers still in flight: US1 nested-junction machinery, US2 loopback+token HTTP test, US8 `init_atomic_write` crash-before-replace oracle.
