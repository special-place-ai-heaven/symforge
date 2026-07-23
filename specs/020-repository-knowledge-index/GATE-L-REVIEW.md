# Gate L Closure — High-Impact Adversarial Review (Cursor + Kimi)

You are performing an **adversarial code review** of SymForge Feature 020's **Gate L**
(worktrees + local refs) on branch `feat/repository-knowledge-index` in
`E:\project\symforge` (a Rust MCP server). Find defects and contract violations and
**report them** — do not broadly rewrite. Produce findings; the maintainer decides.

Two reviewers are looking at this independently (Cursor and Kimi). Reason from the
code, not the comments. Cite every finding as `path:line`. Empty is a valid answer.

## Ground rules

1. **The frozen SpecKit contracts are the authority.** If code disagrees with a frozen
   contract, the *code* is wrong — never propose weakening a contract. Read first:
   - `specs/020-repository-knowledge-index/{spec,plan,data-model,tasks}.md`
   - `contracts/source-binding-and-state.md`, `contracts/repository-mental-model.md`,
     `contracts/search-knowledge.md`, `contracts/knowledge-authority-hygiene.md`
   - `tasks.md` "Gate L" defines the RED/GREEN/VERIFY IDs (`L-R01`..`L-R14`,
     `L-G01`..`L-G07`, `L-V01`..`L-V04`). Tie findings to IDs.
2. **Code is gospel, docs are testimony.** Verify claims against source.
3. **Report BLOCKER / HIGH / MEDIUM / LOW**, most-severe first: one-line claim, a
   concrete failure scenario (inputs → wrong output/crash), `file:line`, contract ID.

## The review surface (commits on this branch)

- `85d70dc` — earlier review-fix batch (P0-lane preservation `next_after_current_publish`,
  degraded ref coverage, multi-source envelope, parse-once with grammar-flavor key).
- `130f57b` — **engine**: worktree classifier + ref-topology reconcile driver.
- `5773fd9` — **L-R11** protected-membership test.
- (pending, in this review) — **gated production reconcile caller** + **L-R08** isolation test.

### Files & symbols to scrutinize

- **`src/live_index/worktree_topology.rs`** — `checked_out_worktrees(repo)`: enumerates
  linked worktrees via libgit2 (no subprocess), reads each worktree HEAD branch, skips
  stale/pruned. (L-G01 foundation.)
- **`src/live_index/local_ref_scout.rs`**
  - `reconcile_local_ref_topology(handle, repo, repository_id, budget) -> ReconcileOutcome`:
    checked-out set = linked-worktree HEADs ∪ main repo HEAD (each a P0 lane, never P1);
    publishes a P1 lane per **bare** local branch; removes lanes for refs that are deleted
    or newly checked out; returns `checked_out` for a later daemon layer. (L-G05/L-R03.)
  - `route_catalog_files` parse cache keyed by `(object_id, classification, language,
    is_tsx, is_c_header)` (L-R02/L-R14; the flavor keys were a round-2 fix).
  - `source_isolation_never_crosses_ref_and_current_boundaries` test (L-R08).
- **`src/live_index/store.rs`** — `PublishedSourceSet::next_after_current_publish` (P0
  publish preserves P1 lanes), `publish_ref_source`/`remove_ref_source` (locked,
  registry_generation-safe), `build_ref_source_generation` (ref bundle from ref identity).
- **`src/daemon.rs`**
  - `local_ref_lanes_enabled()` / `LOCAL_REF_LANES_ENV` (`SYMFORGE_LOCAL_REF_LANES`,
    default OFF).
  - `spawn_local_ref_reconcile(index, canonical_root)` — gated, `spawn_blocking`,
    fire-and-forget; opens git2 repo (returns on non-git/error), derives `RepositoryId`
    via `capture_repository_source`, calls the reconcile driver. Wired in `reload_with`
    right before `spawn_git_temporal_computation`.
  - `l_r11_second_session_cannot_reach_protected_project_via_wildcard_or_ref_mapping`
    (L-R11 test).

## Highest-value things to attack

1. **P0/P1 race (L-R12/L-R13, L-V04).** `spawn_local_ref_reconcile` is a detached
   `spawn_blocking` that calls `publish_ref_source` while the watcher may run a concurrent
   P0 publish (`swap_and_publish_*` → `next_after_current_publish`). Both take `write_mutex`
   and copy-under-lock. **Can any interleaving drop a lane, resurrect a removed lane, lose a
   P0 content update, or leave `registry_generation` non-monotonic?** Can a failed/aborted
   reconcile ever touch or stall the P0 lane (must not — L-V04)?
2. **L-V02 default-unchanged.** With `SYMFORGE_LOCAL_REF_LANES` unset, is the open/reload
   path provably byte-for-byte unchanged (the gate returns `None` before any work)? Any
   allocation/log/latency the default path now pays?
3. **Reconcile correctness (L-G01/L-R03).** Is the checked-out set exactly right — could a
   detached-HEAD worktree, a bare-repo main, or a ref checked out in *another* linked
   worktree be misclassified and wrongly published as a P1 lane (violating "checked-out
   worktrees are never P1")? Does the deletion pass ever remove a *foreign* repo's lane, or
   fail to remove a lane whose branch was deleted? Is the `symforge:git-ref:<repo>:` prefix
   parse collision-safe (refnames contain `/`; repo ids)? Is the per-pass idempotent
   re-publish (re-bumping `registry_generation` for unchanged lanes) acceptable churn?
4. **L-R08 isolation.** Does the ref lane's bridge/authority ever resolve an anchor to the
   *current* lane's code/doc, or vice versa? Is the test actually strong enough to catch a
   cross-boundary leak, or does it pass vacuously?
5. **L-R11.** Is the session-scoped `project_indexes` really the *only* addressing
   chokepoint? Find any path (id, alias, `projects=["*"]`, subset, source scope, CCR
   retrieval, an internal dispatcher) where a session could reach a project/worktree/ref it
   never opened — especially a protected one. Is the test's `project not open` proof airtight?
6. **Catalog-only / secret parity (L-R04/L-R10).** Still holds after the parse-cache
   refactor? Oversize blobs never materialized; secret-positive blobs withheld identically
   to filesystem ingestion?

## Known-OPEN — do NOT report as bugs

Documented, intentional (see `tasks/todo.md` "Gate L Progress"):
- **Worktree→`ProjectInstance` auto-open** (the daemon auto-opening checked-out worktrees
  as separate instances) is deliberately NOT built: `data-model.md:1260-1263` says they
  "remain separate **existing** instances" (no auto-open mandate), and auto-opening a
  protected worktree would violate `source-binding-and-state.md:54`. Out of Gate L scope.
- **Live `.git/refs` watcher**: reconcile runs on open/reload only, so L-R03 holds across
  reopen but not on live branch movement while open (a documented `// ponytail:` ceiling).
- **Gate M** (health/surface/corpus/embed) and **AAP** blockers — later gates.
- `remove_ref_source` had a dead-code allow; it now has the reconcile driver as caller.

## Verification commands (repo-pinned; clean up after)

```
C:\Users\rakovnik\.cargo\bin\cargo.EXE fmt --all -- --check
C:\Users\rakovnik\.cargo\bin\cargo.EXE clippy -j1 --all-targets --features server -- -D warnings
C:\Users\rakovnik\.cargo\bin\cargo.EXE test -j1 --lib --features server -- --test-threads=1
```

Notes: artifacts go to the repo-local `target/` on `E:` (gitignored) — **`cargo clean`
when done**, the tree fills fast. `.rs` files ≥ ~90 KB may be `UnstableDuringRead`-demoted
on a cold `LiveIndex::load` while another SymForge watcher is live — environment race, not
a code bug. Current state at review time: full lib suite green; clippy `-D warnings` clean.

## Output

Findings list (BLOCKER→LOW): claim, failure scenario, `file:line`, contract ID. If a
piece is clean, say so. If nothing is above LOW, say so plainly — do not invent issues.
