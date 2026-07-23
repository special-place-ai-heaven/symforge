# Cursor Review Prompt — SymForge Feature 020, Gate L (worktrees + local refs)

You are performing an **adversarial code review** of an in-progress feature on the
branch `feat/repository-knowledge-index` in `E:\project\symforge` (a Rust MCP server).
Your job is to **find defects and contract violations and report them** — do **not**
rewrite or "fix" broadly. Produce findings; the maintainer decides what to act on.

## Ground rules

1. **The frozen SpecKit contracts are the authority.** If code disagrees with a
   frozen contract, the *code* is wrong — never propose weakening a contract to fit
   code. Read these before judging anything:
   - `specs/020-repository-knowledge-index/spec.md`, `plan.md`, `data-model.md`, `tasks.md`
   - `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`
   - `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`
   - `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
   - `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
   - `tasks.md` "Gate K" and "Gate L" sections define the RED/GREEN/VERIFY criteria
     (IDs like `L-R01`, `L-G07`). Tie findings to these IDs where possible.
2. **Code is gospel, docs are testimony.** Verify claims against the actual source,
   not comments. Cite every finding as `path:line`.
3. **Report by severity**: BLOCKER / HIGH / MEDIUM / LOW, most-severe first, each with:
   a one-line claim, a concrete failure scenario (inputs → wrong output/crash), the
   `file:line`, and the contract ID it violates (if any). Empty is a valid answer.

## What was built this session (the review surface)

The **Gate L engine** ingests knowledge/code from local Git refs (and, in principle,
linked worktrees) as additional "sources" alongside the current worktree, and lets
`search_knowledge` / `review_knowledge` compose across them. Primary files:

- **`src/live_index/local_ref_scout.rs`** (NEW): the whole ref pipeline.
  - `scout_local_ref` — bounded, in-process libgit2 walk of a ref tree; blobs keyed by
    object ID; size from `Odb::read_header` so oversize blobs are `CatalogOnly`
    (bytes never read); entry budget degrades coverage. **No Git/LFS subprocess.**
  - `materialize_ingest_blobs` / `RefBlobBytes` — read each distinct ingest object ID
    once; never read catalog-only blobs.
  - `route_ref_blob` — routes blob bytes through the **shared** adapters
    (`IndexTargets::for_path`, `knowledge::classify_stable_content`,
    `parsing::process_file_with_classification`). No second parser/index.
  - `build_ref_source_index` — assembles a root-less `LiveIndex` from routed files.
  - `ingest_and_publish_local_ref` — end-to-end scout→index→publish.
  - The `#[cfg(test)] mod tests` at the bottom is the focused Gate-L test suite.
- **`src/live_index/store.rs`**:
  - `LiveIndex::from_source_files` — build a queryable index from an in-memory file map.
  - `SharedIndexHandle::build_ref_source_generation` — wrap a ref-source index in a full
    `PublishedGeneration` (GitRef `SourceIdentity`, `SourceVersion working_tree=NotApplicable`,
    per-source manifest/bridge/authority via the current-lane builders, Pending temporal).
  - `SharedIndexHandle::publish_ref_source` / `remove_ref_source` — reconcile a ref lane
    into `published_source_set` (`ArcSwap<PublishedSourceSet>`) under `write_mutex`:
    copy the source map, replace/drop only that lane, bump `registry_generation`, swap once.
- **`src/protocol/knowledge_search.rs`**: `select_scoped_sources` + `search_scoped`
  (compose current/worktrees/local_refs/all from one captured `PublishedSourceSet`).
- **`src/protocol/knowledge_review.rs`**: `review_scoped` (same composition for review;
  aggregates per-source `(source_id, review_hash)` into the top result hash).
- **`src/protocol/search_tools.rs`**: `AdvertisedSearchKnowledgeSourceScope` (advertises
  all four scopes for both tools).

## Specific things to scrutinize (highest value)

1. **Publication generation-fence (L-R12/L-R13, data-model.md).** In `publish_ref_source`
   / `remove_ref_source`, confirm a P1 (ref) add/update/remove bumps `registry_generation`
   but leaves the **current** lane's `publication_generation` / `content_generation` /
   `project_generation` byte-identical. Look for any accidental shared mutation of the
   current bundle. Is copy-under-lock actually race-safe against a concurrent P0 publish?
2. **Source identity isolation (L-R08, repository-mental-model.md §Bridge).** Does the
   ref source's bridge/authority ever resolve links to *another* source's code? The
   builders (`build_knowledge_bridge`, `build_published_authority`) are called with the
   ref's own `SourceIdentity` — verify no cross-source anchor leakage.
3. **Catalog-only guarantee (L-R04).** Confirm `scout_local_ref` never reads bytes of a
   blob over the budget (only `read_header`). Confirm `materialize_ingest_blobs` and
   `build_ref_source_index` skip `CatalogOnly` entries.
4. **Shared-adapter parity (L-R10).** `route_ref_blob` must reuse the *exact* filesystem
   ingestion functions; verify it doesn't diverge (e.g., different target routing, a
   bypassed secret gate). Secret-positive / LFS / undecodable bytes must be withheld.
5. **Determinism (L-R14, search-knowledge.md test 14).** `search_scoped` / `review_scoped`
   ordering: current first for `all`, then `SourceId` order; equal generations →
   byte-identical output; `review_scoped`'s `combined_result_hash` folds per-source pairs.
   Check the typed-empty (`no_sources_in_scope`) and per-source unavailable-lane paths.
6. **`remove_ref_source` carries `#[allow(dead_code)]`** — it's tested but has no production
   caller yet (the ref-topology reconcile driver is unbuilt). Confirm that's the only dead
   path and flag if the `allow` hides something real.
7. **Gate K durability** (`src/protocol/knowledge_curation.rs`, closed earlier this branch):
   temp-image digest verification, live pre-image fencing (`indeterminate_conflict`),
   read-only pre-lock replay, foreign-record quarantine under the lock. Worth a second look.

## Known-OPEN items — do NOT report these as bugs

These are intentionally unbuilt (documented in `tasks/todo.md` "Gate L Progress"):
- Checked-out **linked worktrees as separate `ProjectInstance`s** (cross-project
  dispatcher) — only local-ref P1 lanes on the owning instance are wired so far.
- **L-R11** (second-session protected-worktree membership) — session-membership layer.
- Gate L **VERIFY** battery (L-V01..L-V04) and formal box-checking in `tasks.md`.
- **Gate M** (health/surface/corpus/embed) and the **AAP blockers** SF-AAP-001/002/003.

## Verification commands (repo-pinned)

```
C:\Users\rakovnik\.cargo\bin\cargo.EXE fmt --all -- --check
C:\Users\rakovnik\.cargo\bin\cargo.EXE clippy -j1 --all-targets --features server -- -D warnings
C:\Users\rakovnik\.cargo\bin\cargo.EXE test -j1 --lib live_index::local_ref_scout::tests -- --test-threads=1
C:\Users\rakovnik\.cargo\bin\cargo.EXE test -j1 --lib -- --test-threads=1
```

Notes for the reviewer's environment:
- Build artifacts go to the repo-local `target/` on `E:` (gitignored). **Run
  `cargo clean` when you finish** — repeated full-suite builds fill the disk (~26 GB).
- `.rs` files ≥ ~90 KB may be `UnstableDuringRead`-demoted on a cold `LiveIndex::load`
  when another SymForge watcher is live on the same tree — that is an environment race,
  not a code bug (see `tests/batch_rename_perf.rs` comment).
- Current state: full lib suite 2968/0/2; `clippy --all-targets --features server -D
  warnings` exit 0; all ~108 integration suites verified green.

## Output

Return a findings list (BLOCKER→LOW). For each: claim, failure scenario, `file:line`,
contract ID. If you find nothing above LOW, say so plainly — do not invent issues.
