# Cursor Review Prompt — SymForge Feature 020, Gate L (round 2: fix verification)

You already reviewed Gate L once and returned five findings (1 BLOCKER, 3 HIGH,
1 LOW). This round is an **adversarial re-review of the fixes** on branch
`feat/repository-knowledge-index` in `E:\project\symforge` (Rust MCP server).
Your job: confirm each fix actually closes its finding **and introduces no new
defect or contract violation**. Find problems and report them — do not rewrite.

## Ground rules (unchanged)

1. **Frozen SpecKit contracts are the authority.** Code disagreeing with a frozen
   contract means the *code* is wrong. Read before judging:
   - `specs/020-repository-knowledge-index/{spec,plan,data-model,tasks}.md`
   - `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`
   - `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`
   - `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
   - `tasks.md` Gate L RED/GREEN/VERIFY IDs (`L-R01`..`L-R14`, `L-G01`..`L-G07`,
     `L-V01`..`L-V04`). Tie findings to IDs.
2. **Code is gospel, docs are testimony.** Verify against source. Cite `path:line`.
3. **Report BLOCKER / HIGH / MEDIUM / LOW**, most-severe first: one-line claim,
   concrete failure scenario (inputs → wrong output/crash), `file:line`, contract
   ID. Empty is a valid answer.

## What the last round found, and how it was fixed (the re-review surface)

### FIX 1 — was BLOCKER: P0 publish paths wiped every P1 ref lane
- Root cause you found: `swap_and_publish_with_content_change_and_hook`,
  `publish_prepared_bridge`, `publish_prepared_authority` each rebuilt
  `PublishedSourceSet.sources` as a fresh single-entry map (current lane only),
  dropping published refs on any P0 commit.
- Fix: new `PublishedSourceSet::next_after_current_publish(&self, current_source_id,
  current_generation)` in `src/live_index/store.rs` (near the `impl PublishedSourceSet`
  block). It clones the existing `sources`, replaces **only** the current lane, bumps
  `registry_generation`, and — if the current source identity changed — drops the
  prior current lane so it is not stranded. All three P0 paths now route through it
  (mirroring the P1 discipline in `publish_ref_source`).
- Regression test: `store.rs` → `p0_publishes_preserve_published_ref_lanes` (publishes
  a ref lane, then does a P0 content publish AND a P0 authority publish, asserting the
  ref lane survives each and the current lane advances).
- **Scrutinize**: is the identity-change branch correct — can `current_source_id` ever
  collide with a *ref* lane's `SourceId` (GitRef identity) such that the `remove`
  drops a real ref lane? Is copy-under-`write_mutex` still race-safe vs a concurrent
  P1 `publish_ref_source`? Contract IDs: **L-R13**, **L-G07**, data-model.md.

### FIX 2 — was HIGH: degraded ref scout published as `Complete`
- Fix: `build_ref_source_generation` now takes a `coverage: CoverageStatus` argument
  and stamps it into the manifest (was hardcoded `Complete`).
  `ingest_and_publish_local_ref` maps `catalog.coverage` (`RefScoutCoverage`) →
  `CoverageStatus` and passes it.
- Test: `local_ref_scout.rs` → `degraded_scout_publishes_degraded_ref_manifest_coverage`
  (`max_entries=2` on a 4-file tree → published lane manifest `coverage == Degraded`).
- **Scrutinize**: should an oversize **catalog-only** blob (within a `Complete`
  enumeration) *also* degrade the published scope, or is entry-enumeration coverage the
  correct semantic? Is any *other* call site still assuming Complete? Contract: **L-R07**.

### FIX 3 — was HIGH: multi-source composed response lacked the top-level envelope
- Fix (`src/protocol/knowledge_search.rs`): `render_source_scope_identity` now emits
  per-source working-tree state, freshness, manifest coverage, and manifest digest
  (was identity + generations only). `search_scoped` adds top-level
  `Overall coverage: {worst}` (= worst included source) and the active secret-policy
  version. New shared helpers `source_coverage` / `worst_source_coverage` (pub(crate)).
  `review_scoped` (`src/protocol/knowledge_review.rs`) adds `overall_coverage={worst}`
  to its header, reusing `worst_source_coverage`.
- **Scrutinize**: does "worst included source" correctly degrade when ANY source is
  degraded or manifest-less? Is `source_coverage`'s default (`Degraded` when no
  manifest) the right fail-safe? Does the contract's full top-level list
  (`search-knowledge.md` "Top-level response MUST include", ~108–128) still have gaps at
  the **compose** boundary vs what single-source `search_current` already emits per
  source? Contract IDs: **L-R06**, search-knowledge.md.

### FIX 4 — was HIGH: same OID re-parsed per path mapping
- Fix (`src/live_index/local_ref_scout.rs`): split the path-dependent admission into
  `classify_ref_blob` (shared `IndexTargets::for_path` → `classify_stable_content` →
  classification — L-R10 parity preserved; `route_ref_blob` now calls it too). New
  `route_catalog_files` parses each distinct **(object_id, classification, language)**
  exactly once via a `parse_cache`, clones the cached `FileProcessingResult`, and
  re-maps `relative_path` per path. Withheld (secret/LFS/encoding) blobs still run the
  per-path gate and never parse. `build_ref_source_index` now delegates to it.
- Test: `identical_blob_is_parsed_once_across_same_classification_paths` — identical
  bytes at `one.rs`/`two.rs`/`notes.md` share one OID; `parses_performed == 2`
  (the two `.rs` share one parse; `.md` re-derives — L-R14), all 3 paths mapped.
- **Scrutinize the correctness of parse reuse**: is `FileProcessingResult` truly
  path-independent except `relative_path`? Do any symbols/references/diagnostics
  embed the path such that re-mapping only `relative_path` yields a wrong file? Could
  two entries share `(object_id, classification, language)` yet legitimately need
  *different* parses (e.g. classification equal but a path-policy secret decision
  differs)? Contract IDs: **L-R02**, **L-R14**, **L-G03**.

### FIX 5 — was LOW: stale doc comment
- `src/protocol/search_tools.rs` comment now states both `search_knowledge` and
  `review_knowledge` compose across all four scopes. Confirm it matches runtime.

## Known-OPEN — do NOT report as bugs

- Linked **worktrees as separate `ProjectInstance`s** (cross-project dispatcher) —
  **L-G01**, unbuilt.
- **L-R11** second-session protected-worktree membership — unbuilt.
- `remove_ref_source` carries `#[allow(dead_code)]` (tested; reconcile driver unbuilt).
- **Gate M** (health/surface/corpus/embed) and **AAP** blockers SF-AAP-001/002/003.

## Verification commands (repo-pinned; clean up after)

```
C:\Users\rakovnik\.cargo\bin\cargo.EXE fmt --all -- --check
C:\Users\rakovnik\.cargo\bin\cargo.EXE clippy -j1 --all-targets --features server -- -D warnings
C:\Users\rakovnik\.cargo\bin\cargo.EXE test -j1 --lib -- --test-threads=1
```

- Artifacts go to repo-local `target/` on `E:` (gitignored). **`cargo clean` when done.**
- `.rs` files ≥ ~90 KB may be `UnstableDuringRead`-demoted on a cold `LiveIndex::load`
  while another SymForge watcher is live — environment race, not a code bug.

## Output

Findings list (BLOCKER→LOW): claim, failure scenario, `file:line`, contract ID. If a
fix is clean, say so. If nothing is above LOW, say so plainly — do not invent issues.
