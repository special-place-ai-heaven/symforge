# Gate L Validation Review — Kimi Report (2026-07-24)

Review of `specs/020-repository-knowledge-index/GATE-L-REVIEW.md` scope, branch
`feat/repository-knowledge-index` (HEAD `6973af2`). READ-ONLY: no code edits made;
every fix below is a proposal for the maintainer's engineer.

Methodology applied: full-body reads of `worktree_topology.rs` (182 ln),
`local_ref_scout.rs` (2420 ln incl. all tests), the Gate L symbols in
`store.rs`/`daemon.rs`, `search_scoped`/`review_scoped`, plus the shared helpers they
delegate to (`classify_admission`, `classify_stable_content`, `sensitive_path_rule`,
`process_file_with_classification`, `capture_repository_source`,
`runtime_for_target`). Parity diff, adversarial inputs, interleavings, and
test-attacks below.

**Validation verdicts:** B, C, D, H — closed, fixes hold under re-attack. A — the
sensitive-path rule itself is closed, but the "byte-identical to filesystem scout"
claim is **not**: the ref path skips the whole `classify_admission` tier (Defect 2).
E — fail-closed arms exist and are tested, but two residual fail-opens on the same
invariant remain (Defects 1 and 3). F — closed with one stale-rationale nuance
(Suggestion 4).

---

## Part 1 — Defects

### HIGH 1 — Reconcile run from a linked-worktree instance publishes the MAIN worktree's checked-out branch as a P1 lane (L-G01 violation)

**Claim:** the "main repo HEAD" exclusion arm reads `repository.head()`, but when
the daemon opened the project at a *linked worktree*, that is the *worktree's* HEAD
— the main worktree's checked-out branch is never added to `checked_out_refs` and
is misclassified as bare.

**Concrete failure scenario:** repo with main worktree on `main`;
`git worktree add ../wt feature`. User opens `../wt` as a project (an explicitly
supported topology — `data-model.md:1260-1263`: "checked-out linked worktrees
remain separate existing `ProjectInstance`s"). With `SYMFORGE_LOCAL_REF_LANES=1`,
`spawn_local_ref_reconcile` runs `git2::Repository::open(&canonical)` where
`canonical = ../wt` (`daemon.rs:3216`). In `reconcile_local_ref_topology`:

- `checked_out_worktrees(repository)` (`local_ref_scout.rs:545`) enumerates only
  *linked* worktrees from the commondir admin dir — it never includes the main
  worktree.
- The main-HEAD arm (`local_ref_scout.rs:572`) reads `repository.head()` =
  `refs/heads/feature` (the worktree's own HEAD), not `refs/heads/main`.
- Result: `refs/heads/main` is not in `checked_out_refs` → published as P1 lane
  `symforge:git-ref:<repo>:refs/heads/main` — a branch that is the main instance's
  P0 lane, violating "checked-out branches are never P1" and duplicating another
  instance's P0 content as a P1 lane. The deletion pass keeps it (self-consistently
  wrong) on every subsequent pass.

`file:line`: `src/live_index/local_ref_scout.rs:572` (set construction at 545-549);
trigger site `src/daemon.rs:3216`. Contract: L-G01 (`tasks.md:697`),
`data-model.md:1258-1263`. **No test covers a worktree-opened repository** — every
reconcile test does `Repository::open(root)` at the main root.

**Proposed fix:** in `reconcile_local_ref_topology`, when
`repository.is_worktree()`, also open the main repository and union its HEAD into
`checked_out_refs` with the identical fail-closed arms:

```rust
// after the existing repository.head() arm (~local_ref_scout.rs:590)
if repository.is_worktree() {
    let main = Repository::open(repository.commondir())
        .map_err(|e| format!("Error: the main repository could not be opened ({e}); \
                              local-ref reconcile fails closed."))?;
    match main.head() { /* same three arms as the existing main-HEAD block:
                           branch+name -> insert; branch+undecodable name -> Err;
                           detached -> {}; unborn -> {}; other error -> Err */ }
}
```

Cleaner variant: move main-HEAD resolution into `checked_out_worktrees` (return the
main worktree as a first-class entry), so the classifier owns the whole topology
and the worktree-instance case is impossible to forget. Add a test: open the repo
at the linked worktree (`Repository::open(wt_path)`), reconcile, assert
`refs/heads/main` is *not* published.

---

### HIGH 2 — Ref-blob admission skips the entire `classify_admission` tier: lockfiles, denylisted extensions, size thresholds, and binary sniff diverge from filesystem ingestion (L-R10 parity incomplete — finding A's fix is partial)

**Claim:** `classify_ref_blob` applies `sensitive_path_rule` +
`classify_stable_content` only. The filesystem scout additionally routes every entry
through `classify_admission` (hard-skip >100 MB, lockfile basename, extension
denylist, 1 MB-data/4 MB-code metadata-only threshold, 8 KB binary sniff) — none of
which exists on the ref path.

**Parity diff (side by side):**

| Step | Filesystem (`discovery/mod.rs:838-862` → `scout_decision_for_discovered:1141` → `classify_admission:2095`) | Ref (`local_ref_scout.rs:287-315`) |
|---|---|---|
| path metadata | `path_reason` (encoding/length) | n/a (UTF-8 enforced → degraded) |
| sensitive path | `sensitive_path_rule` → SensitivePath | ✓ identical (`:297`) |
| lockfile basename | MetadataOnly `Lockfile` | **missing** |
| extension denylist | MetadataOnly `GeneratedOrVendor` | **missing** |
| >1 MB data / >4 MB code | MetadataOnly `OversizedData` | **missing** (only the 8 MiB blob budget) |
| binary sniff (8 KB) | MetadataOnly `Binary` | **missing** (UTF-8 check only when `targets.includes_knowledge()`) |
| content policy | `classify_stable_content` at ingest (`store.rs:3769`) | ✓ identical (`:303`) |

**Concrete adversarial inputs (committed to a bare branch, ref lane published):**

- `package-lock.json` (clean, 50 KB): disk → `MetadataOnlyReason::Lockfile`, no
  symbols; ref → **admitted and parsed**, thousands of dependency symbols in the
  ref lane's bridge/authority/conventions stats. L-R10 says "the same … config blob
  produces lifecycle/extraction … results identical to filesystem ingestion" —
  violated by construction in *every* JS/Rust repo (Cargo.lock, yarn.lock, … are
  near-universal), so every ref lane systematically carries pollution the disk lane
  refuses.
- `data.csv` (2 MB, clean): disk → `OversizedData` (>1 MB data threshold); ref →
  ingested and parsed as text (under the 8 MiB budget).
- `assets/logo.png` (binary): disk → `DenylistedExtension`/`Binary` metadata-only;
  ref → targets exclude knowledge → the UTF-8 decode guard is skipped → admitted →
  parsed as `LanguageId::Text` → `FileOutcome::Failed { "unsupported text
  encoding" }` file *inside* the ref index (pollutes health/diagnostics and the
  files map) instead of a clean metadata-only withhold.

`file:line`: `src/live_index/local_ref_scout.rs:287-315` vs
`src/discovery/mod.rs:2095-2140`. Contract: L-R10 (`tasks.md:677-680`), L-G04
("route blobs through the shared scout … adapters" — the admission tier is part of
the scout adapter).

**Proposed fix:** extract the path+size tiers of `classify_admission` into one
shared helper and call it from both scouts, so parity is structural rather than
copied:

```rust
// in discovery (or knowledge): shared by both admission paths
pub fn path_admission_reason(relative_path: &str, size: u64) -> Option<MetadataOnlyReason> {
    if is_dependency_lockfile(Path::new(relative_path)) { return Some(MetadataOnlyReason::Lockfile); }
    if denylisted ext { return Some(MetadataOnlyReason::GeneratedOrVendor); }
    if size > size_threshold_for(relative_path) { return Some(MetadataOnlyReason::OversizedData); }
    None
}

// in classify_ref_blob, after the sensitive_path_rule check:
if let Some(reason) = path_admission_reason(&entry.relative_path, entry.size) {
    return RefBlobRoute::Withheld(reason);       // needs no bytes — works pre-materialization
}
if is_binary_content(bytes) {                  // ref already holds the bytes for ingest-decision blobs
    return RefBlobRoute::Withheld(MetadataOnlyReason::Binary);
}
```

Size tiering by `entry.size` (ODB header, already read) also lets oversized blobs
be withheld *before* materialization. Add tests mirroring
`sensitive_path_blob_is_withheld…`: committed `package-lock.json` → `Lockfile`;
2 MB `.csv` → `OversizedData`; small binary blob → `Binary`.

---

### MEDIUM 3 — Worktree classifier fail-open: listed-but-unclassifiable worktree entries are silently skipped, so their checked-out branch becomes P1-eligible (L-G01)

**Claim:** `checked_out_worktrees` skips three unclassifiable cases instead of
treating them as unprovable: (a) errored/non-UTF-8 name entries
(`worktree_topology.rs:50-53`), (b) `find_worktree` failure (`:55-58`), (c)
`validate()` failure (`:60-62`). The same file deliberately fails *closed* for an
unreadable HEAD (`head_resolved = false`) — an inconsistent policy on the same
invariant.

**Concrete failure scenario:** user moves a worktree directory without
`git worktree repair` (or the worktree's gitdir back-link breaks). The admin entry
`.git/worktrees/<name>/` still exists and git's own branch-deletion protection
still reads `.git/worktrees/<name>/HEAD` — i.e., git still considers the branch
checked out [INFERENCE: git protects via admin-dir HEAD regardless of worktree-dir
validity]. `validate()` fails → the entry is skipped → its branch is classified
bare → published as a P1 lane, while a live `ProjectInstance` candidate still holds
it checked out. Compare: a *readable* but *unresolvable* HEAD aborts the whole
pass; an *unclassifiable worktree* silently proceeds — the weaker trigger gets the
weaker treatment.

`file:line`: `src/live_index/worktree_topology.rs:50-62`. Contract: L-G01; mirrors
finding E's own rationale.

**Proposed fix:** on `validate()`/`find_worktree` failure, fall back to reading the
admin HEAD file directly (this is exactly the source git's checkout protection
uses), and only fail closed if that too is unreadable:

```rust
// in checked_out_worktrees, replacing the `continue` on validate() failure:
if worktree.validate().is_err() {
    let admin_head = repository.path().join("worktrees").join(name).join("HEAD");
    match std::fs::read_to_string(&admin_head)
        .ok()
        .map(|s| s.trim().strip_prefix("ref: ").map(str::to_string))
    {
        Ok(Some(refname)) => { out.push(CheckedOutWorktree { name: name.into(), path,
                                      head_ref: Some(refname), head_resolved: true }); continue; }
        _ => { out.push(CheckedOutWorktree { name: name.into(), path,
                       head_ref: None, head_resolved: false }); continue; } // reconcile fails closed
    }
}
```

This protects the branch without wedging reconcile on genuinely pruned entries
whose admin data lingers (a deleted-dir worktree whose admin HEAD still parses
protects its branch — matching git semantics — until `git worktree prune` removes
the admin entry). A prune-aware refinement (skip only when the admin dir is gone
entirely) can sit on top.

---

### LOW 4 — Tree entries are uncounted against the scout budget: a pathological tree shape does unbounded DFS work

**Claim:** the entry budget is checked only in the `Blob` arm
(`local_ref_scout.rs:167-171`); `Tree` entries (`:162-166`) are pushed and read
without any count or depth bound. A branch containing millions of nested/empty
directories forces that many `find_tree` calls in the detached blocking task. P0 is
unaffected (detached `spawn_blocking`, L-V04 holds), but a blocking thread burns CPU
for the lifetime of the pass and the catalog reports `Complete` for what was
effectively an unbounded walk.

**Concrete trigger:** committed tree with 500k empty nested dirs;
`budget.max_entries` never trips because no blobs are ever reached.

`file:line`: `src/live_index/local_ref_scout.rs:161-172`. Contract: L-R07
("entry/blob … budgets cannot block" — honored for P0, not for the pass itself).

**Proposed fix:** count *every* visited tree entry (not just blobs) against
`max_entries`, or add a separate `max_tree_entries`/depth bound that degrades
coverage identically:

```rust
for entry in tree.iter() {
    visited += 1;
    if visited > budget.max_entries { coverage = Degraded; break 'walk; } // covers trees AND blobs
    ...
}
```

---

## Part 2 — Suggestions

### S1 (high value) — Skip re-publish when the branch tip is unchanged: kills per-reload re-parse cost and registry churn

Every reconcile pass republishes *every* bare branch: full blob materialization +
parse per branch on every open/reload, even when nothing moved.
`build_ref_source_generation` already preserves `content_generation` for same-tip
republish (`store.rs:1392-1406`), so the work is provably redundant.
`registry_generation` consumers are tests-only (verified by grep), so the churn is
cosmetic — but the parse cost is real and scales with (branches × files) per
reload. Cross-branch tip sharing (a freshly cut feature branch == main's tip) pays
the same parse twice.

`file:line`: `src/live_index/local_ref_scout.rs:600-620` (publish loop),
`store.rs:1392`.

**Proposed change:** before scouting a branch in the publish loop, resolve the
branch tip OID (cheap) and compare with the existing lane's
`source_version.commit`; if equal, skip scout+ingest entirely:

```rust
let tip = repository.find_reference(ref_name).ok().and_then(|r| r.target());
let existing_tip = handle.published_source_set().sources
    .get(&SourceId::new(format!("symforge:git-ref:{}:{ref_name}", repository_id.as_str())))
    .and_then(|g| g.source_version.as_ref()?.commit.clone());
if tip.is_some() && tip.map(|t| t.to_string()) == existing_tip { continue; } // lane already current
```

### S2 (medium value) — Add the missing test cells that would have caught Defects 1–3

- Reconcile with `Repository::open(wt_path)` (worktree-opened repo) asserting
  `refs/heads/main` is never published (Defect 1).
- Lockfile/oversized/binary ref blobs withheld with the filesystem-identical
  `MetadataOnlyReason` (Defect 2).
- Moved-worktree (validate-fails, admin HEAD intact) classified as checked-out, not
  skipped (Defect 3).
- A *real* two-thread single-flight test: two `std::thread::spawn` reconciles on
  one handle, assert exactly one ran (`skipped == false`) and the other's
  `skipped == true`, total lane set correct. The current
  `concurrent_reconcile_is_single_flighted_and_skips` holds the guard manually — it
  proves the `try_lock` branch, not the interleaving.

### S3 (medium value) — `route_ref_blob` is now test-only; narrow it or use it in production

`route_ref_blob` (public, `local_ref_scout.rs:325`) duplicates
`route_catalog_files`' per-entry logic minus the parse cache, and its only callers
are tests (`route_single`). Two entry points for the same routing invites drift
(exactly how Defect 2-class gaps form). **Proposed change:** make it
`#[cfg(test)]`/`pub(crate)`, or better, have it delegate to the same single-blob
path `route_catalog_files` uses (extract `route_one(entry, bytes, &mut
parse_cache)`), so tests exercise the production code path.

### S4 (low value) — Single-flight skip drops the fresher trigger; the guard comment overclaims

`local_ref_scout.rs:530-533` justifies skipping with "the running pass already
reflects the newest refs." Interleaving that falsifies it: pass A reads topology →
branch X created → reload triggers pass B → B skips → A's snapshot predates X → X
has no lane until a *third* reload. Within the documented open/reload-only ceiling,
but the rationale is wrong as stated. **Proposed change:** either fix the comment
("a skipped pass may be one trigger stale; the next reload reconciles"), or set a
dirty flag on skip that the running pass checks at completion to immediately re-run
once.

### S5 (low value) — `is_safe_template_basename` template bypass also applies to ref paths — verified safe, worth a comment

`.env.example` committed to a branch is admitted on both paths (template allowlist
in `sensitive_path_rule`, `knowledge/mod.rs:240-247`) — parity holds. No change
needed; noting it because the A-fix review asked about paths the rule does *not*
cover, and this is the one intentional bypass, already mirrored byte-identically.

### S6 (low value) — Duplicated byte copies for multiply-mapped blobs

`route_catalog_files` does `bytes.to_vec()` per path (`local_ref_scout.rs:436-440`),
so a blob mapped at N paths stores N copies in the final `files` map despite
L-G03's dedup-by-object-ID at the materialization layer. Bounded by the 8 MiB
budget × path count; fine today. **Proposed change (only if profiles show it):**
let `IndexedFile::from_parse_result` accept `Arc<[u8]>`.

---

## What I verified clean (no findings)

- **P0/P1 race (attack item 1):** `write_mutex` is `parking_lot::Mutex` (no
  poisoning); every P0 swap path (`store.rs:1855-1970`, `:2905-2915`) and both P1
  mutations hold it across load→copy→mutate→store; `publish_ref_source` re-reads
  `current_source_id` from the in-lock snapshot so it can't resurrect a stale
  current; `next_after_current_publish` only drops the *old current* (never a
  `git-ref` lane); the `content_generation` TOCTOU in
  `build_ref_source_generation` is genuinely closed by single-flight (only
  reconcile publishes ref lanes). A reconcile panic cannot stall P0 (detached
  `spawn_blocking`, no poisoning).
- **L-V02 default path:** gate OFF returns `None` before canonicalize/git-open
  (`daemon.rs:3201-3203`); only cost is one `env::var` read per reload (already
  documented as Cursor LOW #8).
- **Lane-id parsing:** `RepositoryId` is a hex digest of the commondir
  (`persist.rs:437-440`); git refnames cannot contain `:` → `strip_prefix` parse is
  collision-safe; deletion pass only touches lanes with this instance's exact
  `symforge:git-ref:<repo>:` prefix.
- **Determinism (L-R09):** `PublishedSourceSet.sources` is a `BTreeMap`
  (`store.rs:888`) → `All`-scope lane order and `combined_result_hash` pair order
  are deterministic.
- **L-R11 chokepoint:** `runtime_for_target` builds `project_indexes` exclusively
  from `session.servers` (`daemon.rs:1872-1877`); alias resolution matches only
  among the session's own open ids; both dispatch-level tests exercise the real
  `execute_tool_call` path with wildcard and explicit-id vectors.
- **B byte-identity:** `search_scoped`/`review_scoped` delegate `Current` to
  `search_current`/`review_current` on the captured current generation; double
  `validate_input` is output-identical.
- **Parse-cache key (L-R02/L-R14):** `process_file_with_classification` uses
  `relative_path` only for the label, `is_tsx`, and the `.h` C/C++ disambiguation
  gate (verified all 20 uses in `parsing/mod.rs`) — all three covered by the cache
  key; C-header disambiguation's content-driven C→C++ flip is key-safe.



FULL TRANSCRIPT FROM THE HARNESS, before I ran out of tokens:

 Key observations so far:

 1. scout_local_ref: classify_ref_blob uses IndexTargets::for_path + classify_stable_content +
 sensitive_path_rule. But note in classify_ref_blob there's no parity check re: filesystem scout — I
 need to compare with the filesystem scout admission path. The filesystem scout applies
 sensitive_path_rule — check what it covers (.env, .ssh/id_*, *.tfstate?). Need to look at
 crate::knowledge::sensitive_path_rule and the filesystem scout in src/discovery/mod.rs for parity
 diff.
 2. Reconcile deletion pass: lane_prefix = symforge:git-ref:<repository_id>:. Ref names contain /.
 RepositoryId as_str — what characters? If repo id could contain : this could collide; probably hex.
 Also a source id like symforge:git-ref:repoX:refs/heads/foo. Strip prefix gives refs/heads/foo.
 Collision-safe if repository_id is fixed format. Check.
 3. Deletion pass reads local_branch_refs snapshot — single-flight prevents concurrent reconcile
 cross-delete. But remove_ref_source checks prefix "symforge:git-ref:" generally then lane_prefix for
 ownership. OK.
 4. Potential race: reconcile publishes lanes and removes lanes while P0 publish happens
 concurrently. Need to read publish_ref_source / remove_ref_source / next_after_current_publish in
 store.rs.
 5. Idempotent re-publish churn: every reconcile re-publishes every bare branch, bumping
 registry_generation even when tip unchanged. Hmm — does publish_ref_source check whether anything
 changed? If tip unchanged, re-ingesting and re-publishing is churn. The review surface item 3 asks
 about this. Check build_ref_source_generation and publish_ref_source for change detection.
 6. route_catalog_files: parse cache keyed by (object_id, classification, language, is_tsx,
 is_c_header). But classify_ref_blob runs per path — two paths with same bytes but different
 IndexTargets (e.g., one admitted to search only vs full index)? The cache key includes
 classification + language, but not targets. IndexedFile::from_parse_result(result, bytes) — targets
 dropped from the cache key; the parse result presumably doesn't depend on targets, targets only
 decide admission lanes. But then in route_catalog_files, the Indexed { targets, file } — wait, in
 route_catalog_files they ignore targets from classify_ref_blob (they use ..). Then files inserted
 regardless of targets. Compare with route_ref_blob which preserves targets. But
 build_ref_source_index uses route_catalog_files which discards targets. Hmm, but does
 LiveIndex::from_source_files care about targets? For the filesystem path, targets decide which lanes
 (search/card/etc.) a file joins. If ref path ignores targets... potential parity gap. Check
 IndexedFile::from_parse_result and LiveIndex::from_source_files. Also whether the cache key needs
 targets — parse result is path-independent except relative_path, fine.

 But wait — is the parse really path-independent? process_file_with_classification(path, bytes,
 language, classification) — presumably uses path only for labeling and grammar flavor (tsx,
 c-header). They key on those two booleans plus language and classification. If a third
 path-dependent grammar branch exists, the ponytail comment acknowledges. OK.

 7. Sensitive path rule parity (finding A attack): "a sensitive path the rule does NOT cover; a path
 the filesystem scout withholds that the ref path still admits." Need to compare filesystem scout:
 does it withhold by path for e.g. .env via sensitive_path_rule, or via other mechanisms (e.g.,
 .gitignored files are not discovered at all; hidden files?). On filesystem, .env might be excluded
 by discovery (dotfiles excluded?) rather than sensitive_path_rule. The ref scout enumerates ALL
 blobs in the tree — including dotfiles that filesystem discovery might skip for other reasons. So
 ref ingestion could index files the filesystem scout would never even enumerate (e.g., dotfiles
 excluded by discovery limits). Is that a parity gap? Contract L-R10 says "same blob produces
 identical lifecycle/extraction/secret/bridge/authority results". If filesystem discovery would never
 admit .env.example... hmm. Check discovery filters vs sensitive_path_rule coverage.
 8. Worktree classifier: skipped unresolvable/stale worktrees — fail-open? A worktree listed but
 find_worktree fails or validate() fails is skipped; if it's actually live with a checked-out branch,
 reconcile would publish that branch as P1. But validate() failing means git considers it
 stale/prunable — working dir removed. Then its HEAD branch... could still be "checked out" in git's
 view? Actually git branch refuses to delete a branch checked out in a worktree that still validates.
 If validate fails, the worktree is prunable, so the branch is effectively free. But find_worktree
 failing while worktrees() lists it — transient error; skipping = fail-open toward P1 publish.
 Contrast with head_resolved=false which fails closed. Inconsistent: entry unreadable → skip
 (fail-open), HEAD unreadable → fail closed. Possible LOW/MEDIUM finding.
 9. checked_out_worktrees — the main repo's own worktree is not in worktrees() list (git worktree
 list includes main). libgit2 Repository::worktrees() lists only linked worktrees; main HEAD handled
 separately in reconcile. OK.
 10. Deletion pass: handle.published_source_set() snapshot read AFTER publishing. Then
 remove_ref_source(source_id) — but between snapshot and removal, could another reconcile...
 single-flight prevents concurrent reconcile for same handle. OK.
 11. Reconcile failure path: ingest_and_publish_local_ref clones repository_id per branch.
 repository_id.clone().

 Now read store.rs symbols and daemon.rs sections. Also the tests in local_ref_scout (rest of file,
 lines 720-2420, mostly tests). I should read the tests to attack them, particularly:
 source_isolation tests, sensitive path test, deletion-pass test, generation test, fail-closed tests,
 single-flight test.

 Let me first read the store.rs symbols via grep to find line numbers.

 Solid understanding of the scout/reconcile. Now the store symbols and daemon wiring.

 🔍 Grep: fn (next_after_current_publish|publish_ref_source|remove_ref_source|build_ref_source_genera
 ├─  *907│    fn next_after_current_publish(
 │  *1344│    pub(crate) fn try_lock_ref_reconcile(&self) -> Option<MutexGuard<'_, ()>> {
 │  *1364│    pub(crate) fn build_ref_source_generation(
 │  *1503│    pub(crate) fn publish_ref_source(&self, generation: Arc<PublishedGeneration>) {
 │  *1530│    pub(crate) fn remove_ref_source(&self, source_id: &crate::domain::index::SourceId) -> b
 └─ … 3 more matches

 🔍 Grep: local_ref_lanes_enabled|LOCAL_REF_LANES_ENV|spawn_local_ref_reconcile|l_r11_|fn reload_with
 ├─  *133│const LOCAL_REF_LANES_ENV: &str = "SYMFORGE_LOCAL_REF_LANES";
 │   *135│fn local_ref_lanes_enabled() -> bool {
 │   *136│    std::env::var(LOCAL_REF_LANES_ENV)
 │  *3126│    fn reload_with<F>(
 │  *3173│        spawn_local_ref_reconcile(Arc::clone(&index), canonical_root.to_path_buf());
 └─ … 18 more matches

 ● Read E:\project\symforge\src\live_index\store.rs:900-1000:raw,1330-1600:raw

 Notable observations:

 1. build_ref_source_generation content_generation TOCTOU: reads published_source_set outside lock,
 then publish_ref_source takes the lock later. Single-flight means reconcile passes don't overlap;
 but can anything else republish the same ref lane concurrently? Only the reconcile driver publishes
 ref lanes (gated). So no concurrent republish of same lane. But wait — a P0 publish could
 concurrently swap the set; content_generation reads previous lane data which is unaffected by P0
 swap. OK.
 2. Idempotent re-publish churn: each reconcile republishes every bare branch even when tip unchanged
 — new bundle built (full index rebuild: materialize blobs, parse) and registry_generation bumped.
 Content generation preserved. This is potentially heavy: every open/reload re-ingests all bare
 branches. That's a perf note (suggestion), not correctness. Also parse_cache is per-pass. Also note:
 with tip unchanged, the entire index is rebuilt and re-parsed — expensive for big repos on every
 reload. Worth a MEDIUM suggestion: skip republish when catalog.tip_object_id equals previous lane's
 recorded commit.
 3. publish_ref_source and remove_ref_source and P0 publish all bump registry_generation — monotonic.
 Race between P0 publish (next_after_current_publish) and P1 publish: both take write_mutex, copy
 map, insert own entry, swap. Interleavings are serialized by the mutex; both updates retained
 (copy-under-lock). One subtlety: next_after_current_publish drops the prior current lane when
 current_source_id changed — a P1 lane publish between the P0's snapshot... no, the whole
 next_after_current_publish + store happens under one lock? Need to verify the P0 swap path holds
 write_mutex across load_full → next_after_current_publish → store. Let me check swap_and_publish.

 Also: when P0's current source id changes (branch switch at filesystem level... actually P0 is
 filesystem lane; its source id changes when repo identity/branch changes),
 next_after_current_publish removes old current lane. If that old lane id coincides with a P1 ref
 lane id? P0 source id format vs symforge:git-ref: — P0 current is symforge:worktree: or similar? The
 old current lane removed is the P0 lane — not a git-ref lane, so no P1 lane lost. OK.

 4. FreshnessStatus::Verifying for empty ref index — fine.
 5. Note build_ref_source_generation uses capture_history_coverage(&index, &temporal.state) — pending
 temporal → degraded history coverage presumably. OK.
 6. registry_generation bump on every lane publish — reconcile with N bare branches bumps registry N
 times per pass even when nothing changed. Churn. Consumers of registry_generation? Review said:
 check consumers before assigning severity. Let me grep registry_generation consumers.

 Now read daemon.rs spawn_local_ref_reconcile fully (3185-3260), reload_with (3126-3185), and the
 L-R11 tests (7242-7420, 7459-7560, 7657+). Also read search_scoped/review_scoped composition
 (finding B/H) — where? Probably in store.rs or search module. Let me grep.

 ● Read E:\project\symforge\src\daemon.rs:3126-3260:raw

 Notes:
 - local_ref_lanes_enabled() reads env var — one std::env::var per reload even when OFF (known LOW
 #8, documented). But note: the env read happens on every reload_with, including watcher-triggered
 reloads? reload_with is daemon-level reload (explicit). Also, on the default path,
 spawn_local_ref_reconcile returns before canonicalize/git open — inert. OK.
 - spawn_local_ref_reconcile uses capture_repository_source(&canonical, &project_id) — the repository
 id is derived from... need to check capture_repository_source — does repository_id differ between
 worktree instances of the same repo? A linked worktree's ProjectInstance would derive repository_id
 from its own path. Two instances (main + linked worktree) of same repo would compute different
 repository ids if based on path → reconcile in the worktree instance would publish lanes prefixed
 with a different repository id, and the deletion pass only touches lanes with its own prefix. Fine.

 Now check capture_repository_source repository id derivation quickly. Also RepositoryId::as_str
 format — collision safety of symforge:git-ref:<repo>: prefix with refnames containing / and :. Git
 refnames cannot contain : (refname rules disallow :, ?, *, [, \, space, ~, ^, control chars). So
 <repo>:refs/heads/x parse is safe: strip_prefix removes repo + colon, remainder is full refname. And
 repository_id as_str — need to check its format. If repository_id contains :... check
 RepositoryId::new sanitization.

 Also deletion pass only considers sources starting with "symforge:git-ref:" — future worktree lanes
 (known-open, not built) — fine.

 Now read the L-R11 test and the scoped search composition. Let me look at
 search_scoped/review_scoped. Grep.

 🔍 Grep: fn (search_scoped|review_scoped|execute_cross_project_knowledge|execute_cross_project_revie
 ├─ # src/
 │  ## daemon.rs
 │  *4734│fn execute_cross_project_knowledge(
 │  *4806│fn execute_cross_project_review(
 ├─ ## live_index/
 └─ … 4 more matches

 ● Read E:\project\symforge\src\daemon.rs:4700-4920:raw

 Finding B attack: "default-scope output byte-identical?" — comment claims search_scoped with
 None/current delegates to search_current on captured current lane. Need to verify search_scoped
 code. Read knowledge_search.rs:200-360.

 ● Read E:\project\symforge\src\protocol\knowledge_search.rs:150-360:raw

 search_scoped with Current delegates to search_current(&source_set.current_generation(), input) —
 need to verify current_generation() is same as what the old path used. Default path before was
 presumably search_current(&handle.published_generation(), input) where published_generation is the
 current lane. source_set.current_generation() = sources.get(current_source_id). Byte-identical claim
 depends on that — likely fine.

 One subtlety: validate_input runs twice on Current path (search_scoped calls validate_input then
 search_current calls it again) — same output, no issue.

 Now review_scoped:

 ● Read E:\project\symforge\src\protocol\knowledge_review.rs:230-470:raw

 B looks closed structurally. Now: the tests in local_ref_scout.rs (lines 720-2420). Attack the
 tests. Read the relevant test bodies:
 - source_isolation tests
 - sensitive path test
 - deletion pass test
 - generation test
 - fail-closed tests
 - single-flight test

 Read lines 720-1500 and 1500-2420. That's a lot; but needed for test-attack. Let me read in two
 chunks, focusing on tests. Actually let me first read the sensitive_path_rule and filesystem scout
 admission for parity diff. Also classify_stable_content and IndexTargets::for_path vs what
 filesystem ingestion applies. Where's the filesystem scout? src/discovery/mod.rs. Grep for
 sensitive_path_rule usage.

 🔍 Grep: sensitive_path_rule|fn sensitive_path|SENSITIVE_PATH 12 matches · 3 files · in src
 ├─ # src/
 ├─ ## discovery/
 │  ### mod.rs
 │   *842│        } else if let Some(rule_id) = crate::knowledge::sensitive_path_rule(&entry.relative
 │  *3953│        fn sensitive_path_is_terminal_before_any_content_probe() {
 └─ … 10 more matches

 ● Read E:\project\symforge\src\discovery\mod.rs:790-960:raw

 Parity diff (filesystem scout vs ref scout):

 Filesystem admission order per entry:
 1. path_reason (catalog_path_projection: UnsupportedPathEncoding / PathMetadataTooLarge) →
 MetadataOnly
 2. sensitive_path_rule → MetadataOnly SensitivePath
 3. scout_decision_for_discovered(&entry, None) — path-only decision (IndexTargets::for_path based?)
 → Ingest or terminal
 4. binary sniff probe (BINARY_SNIFF_BYTES) → scout_decision_for_discovered with sample

 Ref scout admission (classify_ref_blob):
 1. sensitive_path_rule
 2. IndexTargets::for_path
 3. classify_stable_content(path, targets, bytes) — content scan (full bytes, not just sniff sample?)

 Key parity questions:
 - What does scout_decision_for_discovered do that classify_stable_content doesn't? E.g., size
 limits? Filesystem has file_size from metadata; oversized file decision? Binary sniff: filesystem
 reads BINARY_SNIFF_BYTES sample and decides binary → metadata only. classify_stable_content — does
 it check binary? It takes full bytes. Need to compare. If classify_stable_content is the shared
 adapter used by filesystem ingestion after scouting (i.e., during parse/ingest stage), then parity
 holds at ingest stage. But the filesystem ingest stage — where does classify_stable_content get
 called in the filesystem path? Grep.

 Also path_reason: ref paths come from git tree entries — UTF-8 enforced, so UnsupportedPathEncoding
 handled via degraded coverage. PathMetadataTooLarge (MAX_CATALOG_SAFE_PATH_BYTES) — ref scout
 doesn't check path length. A committed file with path > MAX_CATALOG_SAFE_PATH_BYTES would be
 metadata-only on disk but ingested from ref. Edge parity gap — LOW.

 Also: filesystem discovery excludes things before scouting: .gitignore rules, hidden files?, vendor
 dirs, DiscoveryLimits.max_files, max depth? The ref scout enumerates everything in the tree (no
 .gitignore respect — a committed but ignored file still appears in the tree; on disk it's also
 present though; discovery uses gitignore? If a file is committed it's not ignored). Depth limits? If
 discovery has max depth, ref paths have none. Minor.

 More important: scout_decision_for_discovered — does it include a size-based decision (e.g., >
 max_file_bytes → metadata only / catalog-only)? The ref path has its own 8 MiB blob materialization
 budget — CatalogOnly, meaning no bytes read and NO manifest/catalog entry contribution at all (files
 only; catalog-only contributes nothing — finding G accepted). On filesystem, a >8MiB (or whatever
 limit) file still gets a catalog entry (metadata). Accepted as finding G.

 The critical parity item is binary sniff: filesystem sniffs first BINARY_SNIFF_BYTES and marks
 binary as metadata-only. Ref path: classify_stable_content(path, targets, bytes) — does it also
 binary-check? Let me read classify_stable_content and scout_decision_for_discovered.

 🔍 Grep: fn (classify_stable_content|scout_decision_for_discovered) 3 matches · 2 files · in src
 ├─ # src/
 ├─ ## discovery/
 │  ### mod.rs
 │  *1141│fn scout_decision_for_discovered(
 ├─ ## knowledge/
 └─ … 2 more matches

 • Read (2)
   ├─ E:\project\symforge\src\discovery\mod.rs:1141-1240:raw
   └─ E:\project\symforge\src\knowledge\mod.rs:240-400:raw

 PARITY DIFF — filesystem vs ref admission:

 Filesystem (scout_decision_for_discovered via classify_admission):
 - DenylistedExtension, Untracked, GeneratedOutput → GeneratedOrVendor metadata-only
 - DependencyLockfile → Lockfile
 - BinaryContent (sniff) → Binary
 - SizeThreshold → OversizedData
 - SizeCeiling → HardSkip PerFileCeiling
 - Then sensitive_path_rule (checked BEFORE scout_decision in the scout loop — actually order in the
 loop: path_reason → sensitive_path_rule → scout_decision_for_discovered). Good.

 Ref (classify_ref_blob):
 - sensitive_path_rule ✓
 - IndexTargets::for_path
 - classify_stable_content: LFS pointer, text-encoding (only if targets.includes_knowledge()), secret
 scan (full bytes).

 GAPS (ref path does NOT apply):
 1. DenylistedExtension / GeneratedOutput / Lockfile / Untracked — classify_admission path-based
 tiers are NOT applied on the ref path. A committed package-lock.json (Lockfile) or a .min.js
 (generated) or a denylisted extension (e.g., .map, .lock, binary extensions like .png?) — filesystem
 marks metadata-only; ref path would try to ingest. What saves it? IndexTargets::for_path — for a
 .png, targets likely empty (no lanes) — but classify_ref_blob doesn't check whether targets are
 empty! It then calls classify_stable_content which, if targets doesn't include knowledge, skips text
 decode check, then runs secret scan, admits, and then... process_file_with_classification with
 language from LanguageId::from_path (None → Text). A committed PNG's bytes would be parsed as Text??
 On filesystem, classify_admission would mark BinaryContent or DenylistedExtension → metadata-only.
 On ref path: binary blob ingested as "text" parse. That's a parity gap — but is it a security hole?
 The secret scan still runs. The content would be indexed as garbage text (search pollution), and —
 more concerning — if the binary blob contains something, scan_secret_bytes runs so secrets withheld.
 But wait: decode_searchable_text check only applies when targets.includes_knowledge(). For PNG,
 targets probably excludes knowledge, so a binary blob is admitted and parsed as text with
 classification for the path... FileClassification::for_indexed_path. Hmm, but does the ref lane's
 IndexedFile for a binary blob violate any contract? L-R10 says "The same Markdown/text/config blob
 produces ... identical results". The contract scope is Markdown/text/config. Binary parity is
 arguably out of scope — but "fail-open vs fail-closed audit": binary bytes being served in search
 results? The search_current has source_envelope_is_safe and guard checks. Still, binary garbage in
 the index is a real divergence.
    Actually wait — does the filesystem ingest path also route through classify_stable_content at
 ingest stage? The scout decision (Ingest targets) happens at discovery; at parse/ingest, does the
 filesystem path call classify_stable_content too? If yes, then binary files that passed scout
 (Normal tier) would still get checked at ingest. Let me check where classify_stable_content is
 called in the filesystem ingest path.
 2. Size: filesystem: SizeCeiling → HardSkip, SizeThreshold → OversizedData. Ref: > 8MiB →
 CatalogOnly (never read). Under 8 MiB, ref ingests regardless of classify_admission's size
 thresholds. If filesystem SizeThreshold is lower (e.g., 1 MiB data files), ref path would ingest a 2
 MiB data blob that disk marks OversizedData. Need classify_admission thresholds.
 3. Untracked — classify_admission has Untracked skip reason; on disk this relates to
 gitignore/untracked files. In ref land everything is committed, fine.

 Let me look at classify_admission and where filesystem ingest uses classify_stable_content.

 🔍 Grep: fn classify_admission|classify_stable_content\( 5 matches · 5 files · in src
 ├─ # src/
 ├─ ## discovery/
 │  ### mod.rs
 │  *2095│pub fn classify_admission(
 ├─ ## knowledge/
 └─ … 4 more matches

 ● Read E:\project\symforge\src\discovery\mod.rs:2020-2140:raw

 Confirmed parity divergence. Filesystem admission tiers:
 1. 100MB hard skip
 2. lockfile basename → metadata-only
 3. extension denylist → metadata-only
 4. 1MB data / >4MB code → metadata-only
 5. binary sniff → metadata-only

 Ref path (classify_ref_blob): sensitive_path_rule → IndexTargets::for_path → classify_stable_content
 (LFS pointer, UTF-8 decode when knowledge-targeted, secret scan). None of
 lockfile/denylist/size-threshold/binary-sniff.

 Concrete adversarial inputs:
 - Committed package-lock.json (2 MB, clean content): filesystem → MetadataOnlyReason::Lockfile. Ref
 → admitted, parsed as JSON/Text, thousands of symbols → symbol pollution. Divergence of lifecycle
 result. L-R10 says "same Markdown/text/config blob produces
 lifecycle/extraction/secret/bridge/authority results identical to filesystem ingestion".
 package-lock.json is a config/JSON blob → violates L-R10 parity literally.
 - Committed .min.js or denylisted ext: filesystem GeneratedOrVendor; ref ingested.
 - Committed 2MB CSV (data.csv): filesystem OversizedData; ref parsed (as Text?) — data blob
 ingested.
 - Binary blob with clean bytes but NULs (e.g., committed .dat or an extension that's recognized
 language but binary content): filesystem Binary → metadata-only. Ref: if targets doesn't include
 knowledge, no UTF-8 check → admitted → parsed as text garbage. Even for knowledge targets,
 decode_searchable_text error → withheld. But e.g. a .png — LanguageId::from_path → None → targets
 likely empty... but then it's still admitted and parsed as Text classification. Actually does it
 matter if targets are empty? The file is still inserted into files map in route_catalog_files — an
 IndexedFile with binary garbage content. It lands in the ref LiveIndex's files, trigram index, etc.
 Search over the ref lane would return binary garbage? search_current works from authority records; a
 Text-classified binary might produce units. This is a real parity gap (MEDIUM).

 Is it a defect or a suggestion? L-R10 scope: "The same Markdown/text/config blob" — a PNG isn't in
 scope. But a committed package-lock.json (text/config) IS in scope and diverges (Lockfile on disk vs
 indexed on ref). Also a 2MB .sql data file (text) — disk: OversizedData metadata-only; ref:
 ingested. These are text blobs with divergent lifecycle. So HIGH/MEDIUM defect against L-R10.

 Wait — but also check store.rs:3769 — filesystem ingest at store level also applies
 classify_stable_content after scout. So filesystem has BOTH classify_admission (scout) and
 classify_stable_content (ingest). Ref has only classify_stable_content + sensitive rule. So ref is
 missing the classify_admission tier entirely. That's the parity fix: apply
 classify_admission-equivalent for ref blobs using entry.size + path (+ first sniff bytes? ref has
 full bytes; can sniff). Concrete fix: in classify_ref_blob, after sensitive_path_rule, apply the
 same tier logic: is_dependency_lockfile, is_denylisted_extension, size threshold (code 4MB/data 1MB)
 using entry.size, and is_binary_content sniff on the bytes. But note ordering subtlety: filesystem
 checks size BEFORE reading content; ref reads header already. For >8MiB ref blobs → CatalogOnly. For
 blobs under that, size checks possible. Binary sniff needs bytes — ref only reads bytes for
 Ingest-decision blobs, fine.

 But careful: ref size budget (8 MiB) vs filesystem thresholds (1/4 MiB). A 2 MB code file:
 filesystem ingests (under 4MB code threshold); ref ingests (under 8 MiB). Parity. A 2 MB .json data
 file: filesystem metadata-only (1MB data threshold); ref ingests + parses. Divergence.

 Severity: MEDIUM (pollution + contract violation on text/config parity; not a secret leak since
 secret scan still runs; binary content could be withheld only via encoding check when
 knowledge-targeted). Actually there's a worse angle: IndexTargets::for_path for unrecognized
 extension gives targets that exclude knowledge → decode_searchable_text skipped → binary blob with
 high-entropy token-like strings → scan_secret_bytes may flag Sensitive → withheld. OK secrets still
 scanned on raw bytes. So no secret leak. Main harm: index pollution, garbage units, catalog
 divergence. MEDIUM.

 Also missing: Untracked/gitignore — N/A for refs.

 Now, next: parse cache key issue. route_catalog_files cache key = (object_id, classification,
 language, is_tsx, is_c_header). But process_file_with_classification also receives the path — used
 for relative_path label (remapped after) and grammar flavors (covered). Anything else path-dependent
 inside? E.g., extraction could depend on path for module name derivation (e.g., Python module path →
 package name). If process_file_with_classification derives module names from path, two same-language
 paths sharing a blob would get the first path's derivation, then relabeled only in relative_path.
 Need to check what process_file_with_classification does with the path. Let me check quickly.

 Also IndexedFile::from_parse_result(result, bytes) — bytes.to_vec() copy per path: identical blob at
 N paths stores bytes N times in the files map (Arc<IndexedFile> each owning its own copy). L-G03
 dedups raw bytes at materialization but the final files map duplicates content per path. Memory:
 fine (bounded by budget), but "shares raw bytes" claim of L-R14? L-R14 says "shares raw bytes but
 re-derives classification-specific units". from_parse_result(result, bytes.to_vec()) — per-path
 copy. Minor memory note, suggestion-level.

 Wait, also a subtle bug in route_catalog_files: let Some(bytes) = blobs.get(&entry.object_id) else {
 continue; }; — fine.

 But: classify_ref_blob called per path even for same object at different paths — path-dependent
 admission runs per path (correct, e.g., .env at one path withheld, same bytes at safe path
 admitted... wait! Same object id at path config/.env (sensitive → withheld) and path
 templates/env.sample (clean → admitted and parsed). The parse cache key doesn't include sensitivity
 because withheld paths never reach cache. Fine.

 BUT here's a sharper adversarial case: secret CONTENT scan. classify_stable_content scans content
 per path — same bytes, same result. OK.

 What about L-R02's claim "identical blob parsed once with multiple source mappings" —
 parses_performed witness. Fine.

 Next: check process_file_with_classification path-dependence.

 🔍 Grep: fn process_file_with_classification|fn process_file 2 matches · 1 file · in src/parsing
 └─ # src/parsing/
    ## mod.rs
    *32│pub fn process_file(
    *45│pub fn process_file_with_classification(

 ● Read E:\project\symforge\src\parsing\mod.rs:1-140:raw

 Path-dependence in parse: is_tsx (covered by key), C/C++ header disambiguation — is it keyed by
 path? LanguageId::is_c_header_path(relative_path) presumably gates the disambiguation branch —
 covered by is_c_header in key. Beyond that, does the parse use relative_path for anything else
 (module names, alias map)? The alias_map / references — for Rust resolver maybe path-based module
 names. Let me check further into the function to see where relative_path flows after this point.
 Grep within parsing/mod.rs for relative_path uses below line 140.

 🔍 Grep: relative_path 14 matches · 1 file · in src/parsing/mod.rs
 ├─   *33│    relative_path: &str,
 │    *38│        relative_path,
 │    *41│        FileClassification::for_code_path(relative_path),
 │    *46│    relative_path: &str,
 │    *77│            relative_path: relative_path.to_string(),
 └─ … 9 more matches

 Good — relative_path is only used for labeling, is_tsx, is_c_header. Cache key covers all three
 (language and classification too). Note subtlety: C-header disambiguation can CHANGE the language
 (C→C++ verdict "grammar AND reported language"). The cache key uses the pre-disambiguation language.
 Two .h paths with the same blob: same key, one parse, disambiguation result shared — correct since
 disambiguation depends only on content. OK.

 But one more: the parse result for a .h file disambiguated to C++ reports language C++ in
 FileProcessingResult. Cached under key with language=C. Same bytes at a .h path → same result. Fine.

 Edge: same object at foo.h (C header path → disambiguation) and foo.c? is_c_header_path("foo.c")
 false → different key (is_c_header differs). Good.

 Now the L-R14 test and other tests. Let me read the tests in local_ref_scout.rs — particularly the
 ones cited for A/C/D/E/F and L-R08. Read 720-1500.

 Parse is path-independent beyond the keyed flavors — cache key is sound. Now attacking the tests.

 ● Read
E:\project\symforge\src\live_index\local_ref_scout.rs:720-1150:raw,1150-1600:raw,…,2051-2420:raw

 Tests read. Test-attack notes:

 1. concurrent_reconcile_is_single_flighted_and_skips — tests the guard by manually holding it, then
 calling reconcile in the same thread. It never exercises two actual concurrent reconciles (e.g.,
 spawn_blocking x2). It's a decent proxy: the try_lock path IS the production path. Acceptable but a
 real concurrent test would be stronger (medium-value suggestion).
 2. source_isolation_* tests: strong — they search cross-terms. Good. But note: the isolation tests
 construct lanes and check no path leaks; they don't check the bridge cross-source LINK resolution —
 "does the ref lane's bridge/authority ever resolve an anchor to the current lane's code/doc?" They
 check every card's anchor.source equals own source. That covers it.
 3. unresolved_worktree_head_fails_reconcile_closed...: good. But note the symmetric main-HEAD
 fail-closed path (branch HEAD whose name can't be decoded) is untested — comment acknowledges
 ("symmetric with the tested worktree case"). A name that can't be decoded is hard to construct
 (non-UTF8 branch name). Hmm, head.name() returns Err when the refname isn't valid UTF-8. Git allows
 non-UTF8 refnames? Branch names with invalid UTF-8 bytes are possible on disk (refs are bytes).
 Creating one via git2: repository.reference with raw bytes? git2 API takes &str, so hard. Via
 filesystem: write .git/refs/heads/\xff directly. Possible. Test gap, low value.
 4. empty_repo_unborn_head... — good.
 5. The single-flight guard: try_lock on a std::sync::Mutex — note try_lock_ref_reconcile returns
 MutexGuard. Two reconcile calls from two different spawn_blocking threads for DIFFERENT handles
 would not contend — per-handle lock. Correct scope (lanes are per-instance).

 Race analysis (highest-value item 1): P0 publish vs P1 publish/remove.
 - Both take write_mutex; each does load_full → clone map → mutate → store. Serialized → no lost
 updates between P0/P1 map mutations. registry_generation increments under lock by 1 each —
 monotonic.
 - Interleaving: T1 P0 publish (next_after_current_publish with new current id; removes OLD current
 id from map). T2 reconcile deletion pass snapshot taken BEFORE T1 → contains lanes; removal only
 touches git-ref lanes → no interference. publish_ref_source preserves current_source_id from ITS
 snapshot — wait! publish_ref_source copies current.current_source_id from the snapshot taken under
 the same lock as the swap — so it can't resurrect an old current id. Good: current_source_id is
 always read from the freshly-loaded set inside the lock.
 - What about reconcile's DELETION pass snapshot: handle.published_source_set() read OUTSIDE any lock
 (line in reconcile: let current = handle.published_source_set();). Then it iterates and calls
 remove_ref_source per lane, which re-locks and re-loads. Between snapshot and removal, a concurrent
 P1 publish by... only reconcile publishes P1 lanes, and reconcile is single-flighted. So the
 snapshot is stable w.r.t. P1 lanes. P0 publish doesn't add/remove git-ref lanes... EXCEPT:
 next_after_current_publish drops the OLD current lane when current id changes. Old current lane id
 is a P0 lane id (worktree), not git-ref. OK.
   Hmm wait — actually could the P0 current source id itself be a symforge:git-ref: id?
 build_ref_source_generation creates source ids symforge:git-ref:<repo>:<ref>. The current lane comes
 from LiveIndex::load / capture_repository_source — SourceLocation::WorkingTree, source id format
 presumably symforge:worktree:... or similar. Distinct. And remove_ref_source explicitly refuses to
 remove current_source_id. Double-guarded.
 - One more: reconcile reads local_branch_refs and then publishes each — between reading the branch
 list and scouting a branch, the branch could be deleted or checked out by the user (external git
 op). Scout of a deleted ref fails → collected in failed → fine. Branch checked out in a NEW worktree
 created mid-pass → published as P1 lane although now checked out → stale until next reconcile
 (reconcile runs on reload only; known ponytail ceiling). Also the deletion pass of THIS pass
 computes still_bare from the same stale snapshot — won't remove it. This is a benign staleness
 window consistent with the documented "reconcile runs on open/reload only" ceiling. Worth a LOW note
 at most; the refs watcher is the documented upgrade.
 - ingest_and_publish_local_ref failure mode: publish of lane for branch that fails mid-way —
 build_ref_source_generation builds bundle (no mutation), publish under lock. No partial state.
 Failed branch leaves its PREVIOUS lane in place (stale tip) — acceptable? If a branch's tip moved
 and re-ingest now fails (e.g., ODB unreadable), the old lane stays with the old tip — stale but
 typed (generations not advanced). Fail-safe-ish. Fine.
 - build_ref_source_generation content_generation TOCTOU: comment says reconcile single-flighted so
 no concurrent republish of same lane. But wait — the reconcile deletion pass could REMOVE a lane
 while... no, single flight. What about daemon reload spawning a new reconcile while an older one
 from previous reload still runs? Same handle → single-flight → second skips. Good. But subtle:
 reload can also REPLACE the handle? reload_with reuses project.index (Arc::clone) — same handle. If
 the project is closed and reopened, a NEW handle is created — old reconcile still running on the old
 handle mutates only the old (dropped) set. Harmless.

 6. L-R11: check project_indexes chokepoint. Read the L-R11 test in daemon.rs and check addressing
 paths. Let me read daemon.rs:7242-7420 and 7657-7800. Also grep for other dispatch paths that
 resolve project ids (aliases, "*"). The test asserts "project not open" for a second session.
 Attack: is there any path where a session can reach a protected project without opening it — e.g.,
 CCR retrieval cache keyed globally? ccr_server.apply_ccr_budget — CCR storage: does a response get
 cached and retrievable across sessions (mcp__symforge_retrieve hash)? If CCR blobs are shared across
 sessions/projects, a second session could retrieve a hash minted in a protected session. CCR hashes
 are only known if you've seen the response though — the hash is derived from content, so you'd need
 the content to know the hash... unless enumerable. Probably out of scope; the review surface asks to
 find any path. Let me read the L-R11 tests.

 Tests are strong on isolation. Now the L-R11 daemon tests and the tool-dispatch chokepoint.

 ● Read E:\project\symforge\src\daemon.rs:7242-7420:raw

 L-R11 chokepoint: runtime_for_target builds project_indexes from session.servers. Test proves
 wildcard/subset/One blocked. Let me quickly check runtime_for_target to confirm project_indexes
 comes from session only, and look for other addressing paths (aliases). Also check the tool-dispatch
 test at 7657.

 ● Read E:\project\symforge\src\daemon.rs:7657-7810:raw

 🔍 Grep: fn runtime_for_target|project_indexes:|fn resolve_targets 7 matches · 1 file · in src/daemo
 ├─  *719│    project_indexes: HashMap<String, SharedIndex>,
 │  *1805│    fn runtime_for_target(
 │  *4204│fn resolve_targets(
 │  *4737│    project_indexes: &HashMap<String, SharedIndex>,
 │  *4809│    project_indexes: &HashMap<String, SharedIndex>,
 └─ … 2 more matches

 ● Read E:\project\symforge\src\daemon.rs:1805-1900:raw

 project_indexes built from session.servers — session-only. Chokepoint holds. The alias resolution
 only matches among open_ids (session's own working set). Good.

 Now let me check the contracts briefly to tie findings to contract IDs, and then verify the
 remaining claims:

 1. Verify RepositoryId::as_str format — collision-safety of lane prefix. Check domain/index.
 2. capture_repository_source — does it derive repository_id from remote URL or path? Important: two
 clones of the same repo at different paths (main + linked worktree opened as separate instances) —
 if repository_id derives from the worktree path, then reconcile run from a linked worktree instance
 would compute a DIFFERENT repository_id than the main instance and publish duplicate lanes per
 instance. That's fine (per-instance sets). But if repository_id derives from remote URL, two
 different local clones share the id — again per-instance sets, so lanes never collide across
 handles. The prefix collision concern is within one handle only: lanes for THIS repository id. OK.
 But what does capture_repository_source do for a repo with no remote? Probably hashes canonical
 path. Let me check.
 3. L-V02: default path — one env var read per reload. Known/documented.
 4. Remaining attack on item B: "a scoped multi-project call that still drops a lane" — search_scoped
 selects lanes from the captured set per project. select_scoped_sources filters is_git_ref for
 LocalRefs — lane generations have source.location GitRef ✓. For All: current + all others ✓.

 Wait — one thing in select_scoped_sources LocalRefs filter: *id != current_id &&
 is_git_ref(generation). If somehow the current lane's generation has GitRef location (never per
 above), fine.

 But check All with HashMap iteration order — non-deterministic ordering of ref lanes across calls!
 source_set.sources — what map type? PublishedSourceSet.sources — BTreeMap or HashMap? If HashMap,
 All scope ordering of lanes is nondeterministic → "compose deterministically" (L-R09: "compose
 deterministically") — the response order of sources could vary between calls, breaking
 byte-identical determinism claims (L-R06 envelope / combined_result_hash pairs order). Check the
 type of sources.

 Also in review_scoped, pairs order follows selected order → combined_result_hash(&pairs) — if
 selected order is nondeterministic, result_hash varies. Check PublishedSourceSet.sources type.

 🔍 Grep: struct PublishedSourceSet|sources: 2 matches · 1 file · in src/live_index/store.rs
 └─ *885│pub struct PublishedSourceSet {
    *888│    pub sources: BTreeMap<SourceId, Arc<PublishedGeneration>>,

 BTreeMap — deterministic by SourceId ordering. Good, L-R09 determinism holds.

 Now check RepositoryId and capture_repository_source for id derivation and prefix-collision safety.

 🔍 Grep: struct RepositoryId|impl RepositoryId|fn capture_repository_source 3 matches · 2 files · in
 ├─ # src/
 ├─ ## domain/
 │  ### index.rs
 │  *419│pub struct RepositoryId(String);
 │  *421│impl RepositoryId {
 └─ … 1 more match

 ● Read E:\project\symforge\src\live_index\persist.rs:415-500:raw

 RepositoryId is a hex digest of the common dir path. So:

 - Main instance and linked-worktree instance share the SAME repository_id (commondir is the main
 repo's .git). Each instance has its own handle and its own lanes — fine.
 - Lane prefix symforge:git-ref:<hex-digest>: — hex can't contain :; refnames can't contain : (git
 refname rules) → strip_prefix parse is unambiguous. Collision-safe. ✓
 - Interesting: reconcile in the daemon runs capture_repository_source(&canonical, ...) where
 canonical is the project root. For a linked-worktree project instance,
 git2::Repository::open(&canonical) — opening a worktree path: repository.commondir() resolves to
 main repo's .git → same repository_id. Then reconcile_local_ref_topology(&index, &repository, ...)
 with repository opened at the WORKTREE — repository.branches() lists the shared repo's branches;
 checked_out_worktrees(repository) — repository.worktrees() — works from a worktree-opened repo? git2
 Repository::open on a worktree path gives a worktree-backed repository whose worktrees() should
 still list all linked worktrees. And repository.head() is the worktree's HEAD. Hmm — actually
 Repository::open(path) where path is a linked worktree: git2 opens it as the worktree (via .git
 file). .head() returns the worktree's HEAD ✓. So reconcile from a worktree instance also publishes
 lanes for bare branches into ITS handle — with the same repository_id prefix. Per-instance, fine.

 One more subtle issue in reconcile: repository.head() for the MAIN HEAD exclusion. When reconcile
 runs from a linked worktree instance, repository.head() is the worktree's HEAD (also its P0).
 checked_out_worktrees lists linked worktrees including possibly... does worktrees() include the main
 worktree? No — libgit2 lists only linked worktrees, not the main one. So the set = linked HEADs ∪
 this repo's HEAD. If reconcile runs in the main instance: main HEAD covered by repository.head(),
 linked worktrees by enumeration. Complete. If reconcile runs in a worktree instance: worktree HEAD
 via repository.head(), other linked worktrees via enumeration, but the MAIN worktree's HEAD branch
 is NOT covered! Scenario: user opens the linked worktree (on branch feature) as a project. Main
 worktree has main checked out. Reconcile from the worktree instance: checked_out_refs = {feature,
 other-linked...} but NOT main (main repo's own HEAD). repository.head() here is the worktree's head
 (feature), not main's. So branch main — checked out at the main worktree — is published as a P1 lane
 in the worktree instance! Violation of "checked-out branches are never P1" (L-G01,
 data-model.md:1258-1263).

 Is that reachable? checked_out_worktrees(repository) — the git2 worktrees() call on a
 worktree-opened repo — does libgit2 enumerate worktrees relative to the commondir? The worktree list
 lives in <commondir>/worktrees/. Repository::worktrees() reads from the repo's commondir, so it
 lists ALL linked worktrees regardless of whether the repo was opened at main or at a worktree. But
 it never includes the MAIN worktree itself (the main worktree is not in the worktrees list; git
 worktree list shows it, but that's porcelain synthesizing it). So yes: running reconcile in a
 linked-worktree instance misses the main worktree's checked-out branch → publishes it as P1.

 How is this triggered? spawn_local_ref_reconcile(index, canonical_root) runs for ANY opened project
 when the gate is ON — including a linked worktree opened as its own project instance. L-G01's
 invariant "checked-out worktrees are never P1" is then violated in that instance.

 Severity: HIGH? The invariant is explicit in data-model.md:1258-1263 and the reconcile doc comment
 claims "the main repo's own current HEAD" is excluded — the code only excludes repository.head(),
 which is main's HEAD ONLY when the repo was opened at the main worktree. The test
 reconcile_publishes_bare_branch_and_excludes_checked_out_worktree_branch opens the repo at MAIN
 (Repository::open(root)), so the gap is untested.

 Wait — double-check: in that test, main HEAD = master, excluded via repository.head() ✓; checked-wt
 excluded via enumeration ✓. Now if the instance root is the linked worktree:
 Repository::open(wt_path). Its commondir → main .git. worktrees() → lists "checked-wt" (and the
 main? no). head() → feature. main (checked out at main worktree) not in checked_out_refs → published
 as P1. Confirmed by construction. To be careful: is there anything else that would exclude main? The
 publish loop publishes every local branch not in checked_out_refs. Yes, refs/heads/main would be
 published. HIGH finding: L-G01 violation when the daemon opens a linked worktree as a project (an
 explicitly supported topology — worktrees are separate ProjectInstances per the contract).

 Fix: in reconcile_local_ref_topology, the checked-out set must include the MAIN worktree's HEAD
 branch even when repository itself is worktree-opened. Concretely: open the main/common-dir
 repository (Repository::open(repository.commondir())) and read ITS head() too — union both heads.
 Or: checked_out_worktrees plus both repository.head() and main-repo head. Simplest: in reconcile,
 after computing checked_out_refs, if repository.is_worktree() (git2 has Repository::is_worktree),
 also open the commondir repo and add its head branch (fail-closed on unreadable name, same as
 main-HEAD arm).

 Let me verify git2 API: Repository::is_worktree() exists ✓. And opening the commondir path with
 Repository::open gives the main repository whose head() is the main worktree HEAD ✓.

 Also related: when reconcile runs in the MAIN instance, a branch checked out in the main worktree is
 excluded ✓. When it runs in worktree instance W1, branch checked out in W2 is covered by enumeration
 ✓, main covered by nothing ✗.

 Second issue — fail-open in checked_out_worktrees: listed-but-unresolvable worktree entries are
 skipped (continue), and validate() failures are skipped. For validate(): a stale worktree whose
 directory was deleted — its branch is prunable by git, so treating it as not-checked-out matches
 git's own semantics. But find_worktree failing or the entry being non-UTF8/errored → skipped
 silently → that worktree's branch could be published as P1. Contrast with the fail-closed treatment
 of unreadable HEAD. This is an inconsistent fail-open on the same invariant. Concrete trigger:
 .git/worktrees/<name> metadata corrupted such that find_worktree errors but validate would have...
 actually find_worktree error vs validate error overlap. A worktree whose gitdir file points
 somewhere unreadable: validate() fails → skipped → its checked-out branch becomes P1-eligible. Is
 that realistic? validate() checks the gitdir linkage; a worktree directory that was MOVED (common!)
 fails validation. User moves a worktree directory without git worktree repair: validate fails, git
 still refuses to delete the branch? Actually git branch -d checks worktrees via... if the worktree
 is stale (missing), git allows branch deletion ("error: Cannot delete branch ... checked out at"
 only if HEAD resolvable?). For a moved worktree, git branch -d may refuse or allow depending. The
 branch may still be "checked out" at the moved location (the moved worktree still functions — its
 .git file points back to the admin dir, and the admin dir's gitdir points to the old location →
 broken bidirectional link). The moved worktree still has the branch checked out (its own HEAD file
 works fine). Git's branch -d would... use is_branch_checked_out which iterates worktrees — for a
 broken-link worktree it can't determine → I believe git errs toward refusing or ignoring. Uncertain.
 But SymForge's classifier skipping it means P1 publish of a possibly-checked-out branch = fail-open.
 The fail-closed fix: treat a listed worktree that fails find_worktree/validate as "unprovable" → set
 a flag causing reconcile to fail closed, same as head_resolved=false. At minimum MEDIUM (same
 contract invariant as E, weaker trigger likelihood). The maintainers deliberately chose skip for
 "stale/pruned" — validate() failure IS the stale signal in the normal case. Hmm. git2's
 Worktree::validate — validates the worktree's gitdir linkage; a pruned worktree fails. The doc
 comment says "A stale/pruned worktree carries no live P0 lane; skip it". For a genuinely pruned
 worktree that's right. For a MOVED worktree (directory still live, branch still checked out there,
 links broken) it's wrong. git itself considers the branch checked-out... let me think: git worktree
 list shows moved worktrees as "prunable". git branch -d branch when the worktree is prunable → git
 allows deletion? Test knowledge: git refuses to delete a branch checked out in any worktree it can
 resolve; for a prunable worktree, git branch -d succeeds (that's the point of prune semantics). Hmm,
 actually no — git checks HEAD of all worktrees including broken ones? I believe for a moved worktree
 git branch -d reports "Cannot delete branch 'x' checked out at '<old path>'" — no wait, that's for
 valid worktrees. For broken-link ones, git 2.30+ might still find the HEAD via the admin dir's HEAD
 file at .git/worktrees/<name>/HEAD — which still exists and points at the branch! Actually the admin
 dir .git/worktrees/<name>/HEAD persists even when the worktree directory moved. git's branch
 checkout protection reads .git/worktrees/*/HEAD — it doesn't need the worktree dir to exist. So git
 WOULD still refuse to delete the branch. And git2's validate() fails → we skip → publish as P1 while
 git still considers it checked out. That's a real divergence: MEDIUM fail-open.

 Hmm wait — does open_from_worktree succeed for such a worktree? git2's validate is stricter. The
 conservative fix stands: any listed worktree we cannot fully classify should fail closed, not be
 skipped.

 Third: scout_local_ref uses revparse_single(ref_name) — reconcile passes full refnames refs/heads/x
 so no ambiguity. ingest_and_publish_local_ref public API allows short names/tags — tests use "HEAD".
 Fine.

 Fourth: L-R04 — giant blob catalog-only ✓ tested.

 Fifth: parity gap found earlier (classify_admission tiers missing on ref path) — MEDIUM/HIGH defect
 vs L-R10. Concrete adversarial inputs:
 - committed package-lock.json (clean, 50KB): disk → MetadataOnlyReason::Lockfile; ref → parsed,
 thousands of symbols. L-R10 violation (text/config blob).
 - committed data.csv 2MB (clean): disk → OversizedData metadata-only; ref → ingested (under 8MiB)
 parsed as text → bloat.
 - committed .min.js (denylisted? need to check is_denylisted_extension list) — check the denylist.
 - committed binary blob assets/logo.png: disk → DenylistedExtension or Binary; ref → targets
 empty... let me check IndexTargets::for_path for unknown ext: probably empty targets → then
 classify_ref_blob: targets.includes_knowledge() false → skips UTF-8 check → secret scan on bytes
 (likely clean) → Admitted → parsed as Text with classification for_indexed_path...
 FileClassification for unknown → maybe metadata-ish? Then process_file_with_classification with
 language Text → decode_searchable_text fails (binary) → FileOutcome::Failed with "unsupported text
 encoding"! So the binary blob lands in the index as a FAILED parse file (not withheld as
 metadata-only Binary). Divergence in lifecycle label + pollution. Also decode_searchable_text — what
 does it accept? If it accepts lossy Latin-1, binary bytes might "decode" and get indexed as garbage
 text.

 So the ref path needs the classify_admission tier (path-based + size + binary sniff) BEFORE
 classify_stable_content, mirroring the disk scout's order: path_reason(N/A) → sensitive_path_rule →
 classify_admission-equivalent → classify_stable_content. Actually disk order is: sensitive rule THEN
 scout_decision (admission). And at ingest time, store.rs:3769 applies classify_stable_content. So
 ref should be: sensitive_path_rule → classify_admission-equivalent (lockfile/denylist/size/binary) →
 classify_stable_content. That's exactly route parity.

 Severity: I'd say HIGH-ish? It violates L-R10 literally for text/config blobs (lockfile = config).
 But no secret leak. MEDIUM is fair; call it MEDIUM with strong contract citation... The contract
 L-R10: "The same Markdown/text/config blob produces lifecycle/extraction/secret/bridge/authority
 results identical to filesystem ingestion". package-lock.json is config/JSON → lifecycle differs
 (Lockfile metadata-only vs Indexed). That's a contract violation → HIGH? Given review severity
 conventions: a contract violation with a concrete trigger but limited blast radius (pollution, no
 leak) → I'd report HIGH on contract-violation grounds, or MEDIUM. I'll call it HIGH since the frozen
 contract is the authority and the trigger is trivial (any repo with a committed lockfile — i.e.,
 nearly every repo, so every ref lane of every JS/Rust project ingests Cargo.lock/package-lock.json
 that the disk lane refuses — symbol pollution at scale, plus bridge/authority divergence).

 Wait — is Cargo.lock in the lockfile list? is_dependency_lockfile — likely includes Cargo.lock,
 package-lock.json, yarn.lock, pnpm-lock, poetry.lock, Gemfile.lock, composer.lock. Very common. And
 denylisted extensions — check quickly.

 Sixth: route_ref_blob (public, used by route_single in tests) vs route_catalog_files — both call
 classify_ref_blob, consistent.

 Seventh: build_ref_source_generation — project_generation: 0 for ref lanes. Fine (P0-only concept).

 Eighth: reconcile publishes for EVERY bare branch on EVERY reload — full re-ingest each time. Perf:
 for a repo with 50 branches × 5k files, every reload re-parses all branches in a detached
 spawn_blocking. Known design; suggestion: skip republish when tip unchanged (compare
 catalog.tip_object_id with existing lane's source_version.commit before building the index — cheap:
 scout is cheap vs parse; even cheaper: compare branch tip oid before scouting). Value: high for
 large repos. Also registry_generation churn: N bumps per reconcile even with zero changes —
 consumers of registry_generation? Grep quickly.

 Ninth: spawn_local_ref_reconcile swallows ALL reconcile errors at debug level — including the
 fail-closed abort (E). Intended (best-effort P1). Fine, but note: the fail-closed abort now silently
 disables P1 lanes forever until reload — acceptable.

 Tenth: deletion pass snapshot read outside the single-flight... it IS inside (guard held for whole
 function). ✓.

 Eleventh: next_after_current_publish drops old current lane if current_source_id changed. Where is
 it called — under write_mutex? Verify the P0 swap path holds the lock across load→compute→store.
 Grep for next_after_current_publish callsite.

 Twelfth: L-R12: P0 fences on per-lane generations. P1 publish bumps registry_generation — does any
 P0 fence read registry_generation? PublicationFence has publication/content/project generations —
 P0's own. P1 swaps don't touch those. ✓. Who consumes registry_generation? Tests + maybe envelope.
 Grep.

 Now checking the lockfile/extension denylist (parity gap evidence), the P0 swap lock discipline, and
 registry_generation consumers.

 🔍 Grep: fn is_dependency_lockfile|fn is_denylisted_extension|LOCKFILE|DENYLIST 8 matches · 1 file ·
 ├─ # src/domain/
 │  ## index.rs
 │  *1390│    /// metadata; only symbol extraction is skipped. See `LOCKFILE_BASENAMES`.
 │  *1485│const DENYLISTED_EXTENSIONS: &[&str] = &[
 │  *1539│pub fn is_denylisted_extension(ext: &str) -> bool {
 └─ … 5 more matches

 🔍 Grep: next_after_current_publish|registry_generation 34 matches · 3 files · in src
 ├─ # src/
 │  ## daemon.rs
 │  *5776│        let before = index.published_source_set().registry_generation;
 │  *5791│            set.registry_generation, before,
 ├─ ## live_index/
 └─ … 32 more matches

 registry_generation consumers outside store/tests: only tests. Churn is cosmetic (per methodology
 item 6) — downgrade to LOW/suggestion.

 Check the P0 swap at store.rs:1894/1959/2909 — is write_mutex held? Read around 1860-1970 and
 2880-2915.

 ● Read E:\project\symforge\src\live_index\store.rs:1855-1975:raw

 P0 swap paths hold write_mutex across load→compute→store ✓. Both P0 and P1 mutations are serialized;
 no lost update. Race item 1 is clean.

 One remaining subtle race: build_ref_source_generation computes content_generation by reading
 published_source_set OUTSIDE the lock (documented ponytail). Reconcile single-flight means only one
 reconcile per handle. But: reconcile pass P1 publishes lane X (content_gen 2). Meanwhile, could
 remove_ref_source run concurrently? Only from reconcile (single-flighted). Could P0 drop lane X?
 next_after_current_publish only drops the OLD CURRENT id, never a git-ref lane. So the TOCTOU is
 genuinely closed by single-flight. OK.

 But one more interleaving for the content_generation read: T1 reconcile builds generation for lane X
 reading previous tip T0 (content 1, decides same-tip → keep 1). Before publish_ref_source locks,
 T2... nothing else publishes lane X. Fine.

 What about next_generation dispenser shared with P0 — Relaxed fetch_add; monotonic across lanes ✓.

 Now let me double-check the All-scope select_scoped_sources — current first, then BTreeMap order —
 deterministic ✓.

 Now, the env gate read: local_ref_lanes_enabled per reload — known LOW.

 Also L-V02 attack: is the default path byte-unchanged? spawn_local_ref_reconcile returns None before
 canonicalize/git2 open ✓. One env::var read per reload_with — negligible but paid on default path
 (documented as Cursor LOW #8, known).

 Now the HIGH finding (main-HEAD exclusion gap when instance root is a linked worktree). Let me
 double-check Repository::worktrees() semantics for a worktree-opened repo: git2's
 Repository::worktrees() lists worktrees from the commondir's worktrees/ admin dir — same regardless
 of open path. And it never includes the main worktree (libgit2 only tracks linked ones). Yes.

 And does the daemon ever open a linked worktree as a project? Gate L's whole premise: "a checked-out
 linked worktree remains a SEPARATE ProjectInstance (its own P0 lane)" — so yes, users open worktrees
 as projects. With gate ON, reload on the worktree instance spawns reconcile with repository opened
 at the worktree → main's branch (e.g. main) published as P1 → L-G01 violation: "checked-out
 worktrees are never P1".

 Wait — but is the main worktree's checked-out branch "checked out in a linked worktree"? No — it's
 checked out in the MAIN worktree. The contract says: "checked-out set = linked-worktree HEADs ∪ main
 repo HEAD (each a P0 lane, never P1)". The main worktree IS a ProjectInstance's P0 lane (the main
 instance). So publishing main as P1 in the worktree instance violates the same invariant (a branch
 that is some instance's P0 becomes P1 elsewhere). Also concretely: the user has the main repo open
 as instance A (P0 = main worktree, branch main) and worktree W open as instance B (P0 = feature).
 Instance B's reconcile publishes P1 lane for refs/heads/main — a duplicate of instance A's P0 lane —
 violating "never P1" and source isolation semantics (same content addressable twice; L-R01 "current
 worktree outranks divergent ref" gets confusing; L-R08 isolation is per-handle so no leak, but the
 contract's topology invariant is broken).

 Trigger requires: gate ON + linked worktree opened as a project + main worktree on some branch. Very
 realistic.

 Fix sketch: in reconcile_local_ref_topology, when repository.is_worktree(), also open the main
 repository (e.g. Repository::open(repository.commondir()) — wait, commondir for a worktree-opened
 repo is the main .git dir; opening that gives main repo) and read its head() with the same
 fail-closed arms, inserting into checked_out_refs. Alternatively move main-HEAD resolution into
 checked_out_worktrees by returning a struct that includes the main worktree as an entry (path =
 workdir of main repo) — cleaner: make the classifier return all worktrees INCLUDING main, and
 reconcile unions all heads.

 Also should double check: for a worktree-opened repo, repository.head() — returns the worktree's
 HEAD ✓ (git2 resolves HEAD in the worktree's admin dir). And Repository::open(worktree_path) — git2
 supports opening a linked worktree path (it reads the .git file). In spawn_local_ref_reconcile,
 git2::Repository::open(&canonical) where canonical is the worktree root → worktree-opened repo. Yes.

 Also: capture_repository_source computes worktree_id from canonical root and repository_id from
 commondir — consistent with the P0 lane identity of the worktree instance.

 Second fail-open (skipped unresolvable/invalid worktree entries) — MEDIUM. The classifier skips (a)
 non-UTF8/errored name entries, (b) find_worktree failures, (c) validate() failures. (c) is the
 designed stale-skip; but a MOVED worktree (dir renamed without git worktree repair) also fails
 validate while git still considers its branch checked out (admin HEAD file
 .git/worktrees/<name>/HEAD still names the branch — git branch -d refuses to delete it... actually I
 should verify this claim. git's branch checkout protection: git branch -d uses
 replace_each_worktree_head_symref/find_shared_symref which reads .git/worktrees/*/HEAD — yes, from
 the admin dir, regardless of whether the worktree directory exists. A prunable worktree STILL blocks
 branch deletion until git worktree prune. Hmm, actually no: git worktree prune removes entries whose
 directories are missing; until pruned, git branch -d errors "Cannot delete branch 'x' checked out at
 '...'". I believe that's right — the protection reads the admin HEAD, not the worktree's validity.
 git2's Branch::delete? git2 doesn't enforce checkout protection at all maybe. Anyway, from the
 CONTRACT's perspective: the worktree is a live P0 lane somewhere (user still works in the moved dir
 — the moved worktree still functions for git operations since its .git file points at the admin dir;
 only the back-link is broken). SymForge would publish its branch as P1 → duplicate lane. Fail-open.

 Fix: change checked_out_worktrees to return entries with head_resolved=false for find_worktree
 failures, and for validate() failures... validate failure is the prune signal; distinguish "prunable
 because directory gone" vs "unreadable". git2's validate checks: the worktree's gitdir file exists
 and links back. A moved worktree fails because the admin gitdir file points to a nonexistent path.
 Both "user deleted dir" and "user moved dir" fail identically. For "deleted dir", the branch is
 arguably still checked out too (git refuses deletion until prune). So the contract-safe move is:
 treat ANY listed worktree that cannot be fully classified (find/validate/head failure) as unprovable
 → fail closed. That's stricter than current. The cost: a stale never-pruned worktree (common! people
 delete worktree dirs all the time and don't prune... actually modern git auto-prunes on some ops,
 but not reliably) would wedge reconcile permanently (fail-closed every pass → no P1 lanes at all,
 silently). That's a real availability tradeoff. Alternative middle ground: for a validate()-failed
 worktree, read the admin HEAD file directly from .git/worktrees/<name>/HEAD — if it names a branch,
 treat as checked-out (protect the branch, don't publish); if unreadable, fail closed. That mirrors
 git's own protection semantics exactly and avoids the availability wedge. That's the concrete fix to
 propose: in the classifier, on validate() failure, fall back to reading
 repository.path().join("worktrees").join(name).join("HEAD") and parsing ref: refs/heads/...; mark
 head_resolved accordingly (false if unreadable). Good suggestion/defect fix.

 Severity on this one: MEDIUM (fail-open on L-G01 invariant, trigger requires
 moved/deleted-without-prune worktree — moderately common).

 Third: parity gap classify_admission tiers — HIGH (contract L-R10 violation, trivially triggered by
 committed lockfiles — near-universal).

 Wait, also think: does the ref path need to match the size thresholds exactly? The ref budget (8MiB)
 already catalog-only's >8MiB blobs. Filesystem: >100MB hard skip, >4MB code / >1MB data
 metadata-only. So a 2MB data blob: disk metadata-only (path searchable), ref: indexed full text. A
 5MB code file: disk metadata-only, ref: ingested+parsed (under 8MiB). Divergent. And 20MB: disk
 metadata-only; ref: catalog-only (never read) — both "not indexed", close enough in outcome though
 different label. The fix should apply the same size thresholds to ref blobs (using entry.size —
 available from header, no materialization needed) and lockfile/denylist basename checks (path-only),
 plus binary sniff on materialized bytes. The path-based ones are cheap and close most of the gap.

 Fourth: tests to attack more:
 - ref_tip_move_advances_lane_generations... — solid (tests same-tip keeps content, moved-tip
 advances, P0 untouched).
 - search_scoped_composes... asserts scope headers. Fine.
 - Test-attack on F (concurrent_reconcile_is_single_flighted_and_skips): holds the guard manually —
 never two REAL concurrent passes. The production interleaving it protects: reload A spawns
 reconcile; reload B spawns another before A finishes → B skips ✓ (same handle, try_lock). But:
 reload replaces watcher... the handle is the same Arc. OK. But there IS one interleaving worth
 flagging: pass A running (holds guard), pass B skips. B's skip is fine because A "reflects the
 newest refs" — TRUE only if A STARTED after B's trigger. Interleaving: A starts, reads topology;
 branch X created; reload triggers B; B skips; A's snapshot predates X → X's lane missing until next
 reload. Since reconcile only runs on reload anyway (ponytail ceiling), and the NEXT reload will
 catch it — same ceiling as documented. But note: B skipping means B's fresh trigger is dropped
 entirely — a skipped pass does not reschedule. The comment claims "the running pass already reflects
 the newest refs" — not guaranteed as shown. The outcome: a branch created between A's topology read
 and B's trigger has no lane until a THIRD reload. This is within the documented open/reload-only
 ceiling but the skip rationale is slightly wrong. LOW / suggestion: on skip, optionally mark "dirty"
 so completion re-runs once. Value: low-medium.
 - Test-attack on C (reconcile_deletion_pass_runs_despite_a_branch_publish_failure): constructs
 refs/heads/broken pointing at a blob — good adversarial trigger. Solid.
 - Test-attack on A (sensitive_path_blob_is_withheld...): good; covers .env + id_ed25519 +
 benign-path twin. Doesn't cover the classify_admission gap (my HIGH finding) — e.g., no test that a
 committed package-lock.json matches disk behavior.
 - Test-attack on H: strong (cross-term search both directions + review). Good.
 - B: "default-scope output byte-identical?" — is there a test asserting byte-identical default
 output pre/post change? The comment in execute_cross_project_knowledge claims it.
 search_scoped(Current) → search_current(&source_set.current_generation(), input). What did the OLD
 code call? Probably search_current(&handle.published_generation(), input). current_generation() =
 sources[current_source_id]. published_generation presumably same. One subtle difference:
 search_scoped calls validate_input(input) FIRST and returns error string, then search_current
 validates again — same error string, identical output. Byte-identical plausibly holds. There are
 tests cross_project_*_source_scope_composes… (daemon.rs:7459, 7560) — let me check one to see if it
 asserts default byte-identity. Read daemon.rs:7420-7560.

 Also item: execute_cross_project_knowledge sets input.project = None; input.projects = None; then
 calls search_scoped per project — validate_input(input) inside search_scoped — does validate_input
 reject when both project and projects are None? It didn't before either (single-project path also
 has None?). Fine.

 Let me read the two cross-project composition tests quickly (7420-7560).

 Race and lock discipline check out. Reading the two cross-project scope composition tests (B's
 byte-identity claim).

 ● Read E:\project\symforge\src\daemon.rs:7420-7560:raw

 B looks closed with real composition tests.

 Now verify the HIGH finding (worktree-opened repo misses main HEAD) — confirm git2 semantics by
 checking how the reconcile test opens the repo, and check if there's any protection I missed. In
 reconcile_local_ref_topology, the "main repo's own current HEAD" arm reads repository.head(). If
 repository is worktree-opened, head() = worktree HEAD. The checked_out_worktrees(repository)
 enumerates linked worktrees — does it include the main? No. So main's branch is unprotected.

 Double-check Repository::open on a linked worktree root: git2 docs — Repository::open opens a
 repository at path, walking up. A linked worktree has a .git FILE pointing to the admin dir; git2
 handles this and the repo's workdir is the worktree, commondir is main's .git. repo.worktrees() —
 libgit2 git_repository_worktrees? Hmm, does git2's Repository have worktrees()? The code calls
 repository.worktrees() and find_worktree — yes git2-rs has these. For a worktree-opened repo,
 listing worktrees works off the commondir. I'm confident.

 Edge: is_worktree() in git2 — Repository::is_worktree() exists. Fix sketch is valid.

 One more check on the HIGH: does the DAEMON ever open a linked worktree as a project in practice?
 index_folder(path=<worktree>) — sure. And capture_repository_source supports it (worktree_id
 derivation). The worktree classifier comment says a checked-out linked worktree "remains a SEPARATE
 ProjectInstance (its own P0 lane)" — implying users open them. Confirmed realistic.

 Severity: HIGH — direct violation of L-G01's "checked-out worktrees are never P1" (the branch
 checked out at the MAIN worktree becomes a P1 lane in any linked-worktree instance with the gate
 on). Contract: data-model.md:1258-1263. Wait, but is it "never P1" per-instance or globally? The
 invariant as documented in reconcile: "A P1 ref lane is published for every bare local branch — a
 local branch that is NOT checked out in any linked worktree AND is NOT the main repo's own current
 HEAD." The intent: main's checked-out branch is not bare. In a worktree instance, the code
 misclassifies it as bare. HIGH.

 Let me also double check the test for the daemon reconcile spawn — l_r11 uses System32 fixture etc.
 Fine.

 Now check the spec contract lines for exact citations: data-model.md:1258-1263, and maybe the
 checked-out set definition. Also verify claim about git2 validate() semantics — I'm inferring
 libgit2 behavior; mark [INFERENCE] where appropriate. The core HIGH (main HEAD missed in
 worktree-opened instance) needs no inference — pure code reasoning.

 Also, second look at checked_out_worktrees sort: sorted by name — deterministic ✓.

 Another possible issue: reconcile_local_ref_topology — branch list read AFTER publish? No: branches
 read, publish loop, deletion pass uses same local_branch_refs. Deletion pass recomputes still_bare
 from the same snapshot — consistent within pass ✓.

 Also local_branch_refs includes branch.get().name() — full refname. For a branch with a name that
 fails UTF-8 → skipped from the list (if let Ok(name)) — a non-UTF8 branch never published AND
 never... deletion pass: local_branch_refs lacks it, checked_out_refs lacks it; existing lane for it
 would be removed (fine); it just never gets a lane. OK.

 Now the deletion pass starts_with("symforge:git-ref:") filter then strip_prefix(lane_prefix) — a
 lane symforge:git-ref:<other-repo>:... is skipped ✓. Foreign repo lanes preserved ✓.

 Env gate: local_ref_lanes_enabled matches "1","true","on" case-insensitive with trim? Saw test: " on
 " is ON (trimmed). "2" OFF. Fine.

 Another item from the attack list: "Is the per-pass idempotent re-publish (re-bumping
 registry_generation for unchanged lanes) acceptable churn?" — registry_generation consumers: only
 tests (grep showed store.rs + tests + daemon test). So churn is cosmetic → LOW suggestion: skip
 republish when tip unchanged (perf) which also kills the churn.

 Perf detail: reconcile republishes ALL bare branches every reload — full blob materialization +
 parse per branch. For a repo with many branches this is heavy on every open/reload. Suggestion:
 compare branch tip OID to the existing lane's recorded commit BEFORE scouting/parsing; skip
 unchanged. The existing lane tip is in sources map (source_version.commit). Cheap lookup. HIGH-value
 suggestion.

 Also L-V04: "Can a failed/aborted reconcile ever touch or stall the P0 lane?" — reconcile never
 calls P0 paths; it only insert/removes non-current lanes. A panic inside spawn_blocking? If
 reconcile panics, the JoinHandle errors; spawn_blocking panic doesn't propagate. P0 unaffected ✓.
 Also write_mutex poisoning: self.write_mutex.lock() — is this parking_lot or std Mutex? If std and a
 panic occurred while held, poisoning could stall P0 publishes! Check the type of write_mutex.
 parking_lot::Mutex doesn't poison. Check.

 Two more checks: write_mutex type (poisoning risk on panic) and the exact contract text at
 data-model.md:1258.

 🔍 Grep: write_mutex: |ref_reconcile_lock: |use parking_lot|use std::sync::Mutex 6 matches · 1 file
 └─    *8│use parking_lot::{Mutex, MutexGuard};
    *1176│    write_mutex: Mutex<()>,
    *1205│    ref_reconcile_lock: Mutex<()>,
    *1325│            write_mutex: Mutex::new(()),
    *1335│            ref_reconcile_lock: Mutex::new(()),
    *4671│    use std::sync::Mutex as StdMutex;

 ● Read E:\project\symforge\specs\020-repository-knowledge-index\data-model.md:1240-1290:raw

 parking_lot::Mutex — no poisoning ✓. Panic in reconcile can't wedge P0.

 Contract confirms: "Checked-out linked worktrees remain separate existing ProjectInstances" — so
 opening worktrees as projects is intended, and my HIGH finding (worktree instance misses main HEAD)
 is a real topology violation.

 One more validation on the HIGH: Repository::open at a linked worktree — is repository.head() the
 worktree's HEAD? Yes. And checked_out_worktrees enumerates only linked worktrees from the commondir
 admin — includes the CURRENT worktree (the one the instance is opened at) and all others, but never
 main. So checked_out_refs = {all linked heads} ∪ {this worktree's head (redundant)}. Missing: main
 worktree's head. Confirmed.

 Also note: worktree_topology tests — I didn't read them (lines 91-181). Let me check what they cover
 quickly, and verify there's no existing test opening reconcile from a worktree root. The reconcile
 tests all use Repository::open(root) where root is the main repo. Confirmed from what I read.

 Let me read the worktree_topology tests quickly.

 ● Read E:\project\symforge\src\live_index\worktree_topology.rs:91-182:raw

 Confirmed: no test opens the repo from a worktree root. The HIGH finding stands untested.

 Now let me assemble findings. Also do one last check on a couple of secondary items:

 1. route_catalog_files ignores targets from classify_ref_blob — does LiveIndex::from_source_files or
 anything downstream need per-file targets? In filesystem path, targets are used at ingest
 (store.rs:3769 context) to decide admission lanes; IndexedFile doesn't carry targets. Files map is
 shared for all lanes; search vs code lanes query the same files with their own logic. So dropping
 targets in route_catalog_files is consistent with how LiveIndex works (files-only). Actually wait —
 route_ref_blob returns targets but build_ref_source_index uses route_catalog_files which discards
 them; nothing downstream consumes per-file targets from a LiveIndex. OK, not a finding. But then
 IndexTargets::for_path computed per entry is only used inside classify_ref_blob for the
 includes_knowledge() check and... FileClassification::for_indexed_path(path, targets). OK used.
 2. scout_local_ref submodule skip — documented known-open.
 3. Symlink blobs — known-open (index as link text).
 4. The spawn_local_ref_reconcile fire-and-forget on reload: multiple rapid reloads spawn multiple
 reconciles on the same handle — single-flight skips extras ✓. But note the skip drops the LATER
 trigger (staleness note, LOW).
 5. route_ref_blob — public API used only by tests now (route_single)? Check callers: grep
 route_ref_blob usage. It's pub fn. Production path uses route_catalog_files. Dead-ish public API —
 could be pub(crate) or test-only. Suggestion low.
 6. RefBlobBytes.get clones per path via to_vec() in route_catalog_files (bytes.to_vec()) — memory
 duplication per path mapping for shared blobs. Suggestion: IndexedFile::from_parse_result could take
 Arc<[u8]>? It takes Vec<u8> presumably. Minor.
 7. local_ref_lanes_enabled — env read per reload even OFF: known.
 8. Finding G (manifest omits catalog-only) — known-open, don't report.
 9. Deletion pass and handle.published_source_set() snapshot AFTER publish pass — a branch whose
 publish FAILED this pass (in failed) but which already had a lane from a previous pass: still_bare →
 lane kept with stale tip. Right behavior (transient failure shouldn't delete data). ✓
 10. One more possible race: reconcile's deletion pass vs a concurrent P0 publish that CHANGES
 current_source_id — next_after_current_publish drops old current lane. If old current lane id were a
 git-ref lane... never (P0 is WorkingTree). ✓
 11. build_ref_source_generation — manifest RepositoryManifest::new(...).ok() — if manifest
 construction fails, silently None → coverage defaults to Degraded in source_coverage ✓ fail-closed.
 12. Env gate + tests use EnvVarGuard with env_lock — fine.
 13. scout_local_ref revparse_single("HEAD") in tests publishes a lane with ref_name "HEAD" → source
 id symforge:git-ref:<repo>:HEAD. Reconcile never does this (only refs/heads/*). A lane named HEAD
 published manually would be removed by the next reconcile deletion pass (not in local_branch_refs) —
 self-healing ✓.
 14. Check IndexTargets::for_path for .env — irrelevant, sensitive rule fires first ✓. For a
 denylisted ext like .safetensors: targets probably Code? No — unknown ext → maybe empty. Then ref
 path: includes_knowledge false → admitted → parsed as Text → binary decode fails →
 FileOutcome::Failed indexed file. Divergence confirmed.

 Also double check decode_searchable_text semantics for binary: if it rejects, outcome Failed.
 Filesystem: Binary → MetadataOnlyReason::Binary. So a committed PNG ends up in the ref lane as a
 FAILED-parse file (visible in health/diagnostics) vs disk as metadata-only Binary. Contract
 violation-ish (lifecycle label differs; and the failed-parse file IS in files map — does a failed
 file contribute to search? Probably not to authority, but it sits in the index and its health
 stats). Solid evidence for the MEDIUM/HIGH parity finding.

 Severity call on parity gap: The contract L-R10 explicitly scopes to "Markdown/text/config blob".
 package-lock.json (config) and oversized .sql/.csv (text/data) are in-scope violations; binary is
 adjacent. No secret leak. Pollution + lifecycle divergence. I'll rate HIGH because lockfiles are
 near-universal and the divergence is systematic (every ref lane of every JS/Rust repo ingests
 lockfiles the disk lane refuses → symbol/bridge pollution, and the "identical lifecycle" contract is
 broken by construction, not by edge case). Actually — one caveat: does the filesystem ingest ALSO
 ingest the lockfile at the store stage? Disk scout marks it MetadataOnly{Lockfile} → catalog entry,
 no ingest. Ref: no lockfile rule → ingested. Yes, systematic divergence.

 Fix sketch for the parity gap: in classify_ref_blob, after the sensitive_path_rule check and
 before/along classify_stable_content, apply the path+size tiers mirroring classify_admission:...
 Note: for blobs >8MiB ref never reads bytes (catalog-only) — fine, they're already excluded; but the
 size threshold (1/4MB) is BELOW 8MiB, so apply by entry.size before materialization ideally (in
 scout or route). Also refactor: expose a shared path_tier_reason(relative_path, size) ->
 Option<MetadataOnlyReason> used by both disk scout and ref classifier so parity is structural, not
 copy-paste.

 Also note ordering: disk applies sensitive_path_rule BEFORE classify_admission. Ref does sensitive
 first too ✓.

 15. Now the "fail-open skipped worktree entries" — finding. And suggest the admin-HEAD fallback.
 16. Suggestion: reconcile skip-unchanged-tip optimization (perf + registry churn).
 17. Suggestion: single-flight skip loses the fresher trigger — document or requeue (LOW).
 18. Suggestion: test gaps — (a) reconcile from a worktree-opened repo (would catch HIGH #1), (b)
 lockfile/denylist/size parity tests, (c) real two-thread concurrency test for single-flight.
 19. Also: ingest_and_publish_local_ref full re-parse per branch per reload even for identical tips
 across branches (two branches at same commit → same blobs parsed twice — parse cache is per-catalog,
 not cross-branch). Suggestion: cross-lane parse cache keyed by object id? The L-R02 cache is
 per-catalog. Two branches sharing the same tree (common: main and a feature branch just created) →
 full duplicate parse. Combined with #16 (skip unchanged tip per lane), cross-branch same-tip dedup
 matters less but still. Mention in suggestion #16.
 20. ReconcileOutcome.checked_out returned for "later daemon layer" — spawn_local_ref_reconcile only
 logs counts; the known-open auto-open is out of scope. ✓ don't report.
 21. L-V02 check: default path — reload_with calls spawn_local_ref_reconcile unconditionally; gate
 OFF → env::var read → None. Byte-unchanged claim holds except the env read (documented known LOW). ✓
 22. One more possible defect: scout_local_ref degraded on non-UTF8 name — records degraded but skips
 the entry silently; coverage Degraded propagates to manifest ✓ (L-R07 honored).
 23. entry_budget — checked only for Blob entries (entries.len() >= max_entries); trees still pushed;
 a tree bomb (millions of empty dirs) → unbounded stack/memory? Budget bounds BLOB entries only;
 trees aren't counted. A malicious/odd repo with 1M nested trees: DFS pushes subtree per entry —
 unbounded work. Filesystem discovery has max_files/max_depth presumably. Ref DFS: no depth limit, no
 tree-entry count limit. Realistic? Bounded by repo reality, but a pathological tree could stall the
 spawn_blocking worker (CPU) — it's detached so P0 unaffected (L-V04 holds), but a rayon/blocking
 thread burns. LOW finding/suggestion: count tree entries against the budget too (entries.len()
 including trees, or a separate max_tree_entries). Worth a LOW.

 Also note break 'walk on budget — stops mid-tree, marks degraded ✓.

 24. total_distinct_blob_bytes saturating_add ✓.

 Now check contract search-knowledge.md or source-binding for the L-R10 exact wording? Already from
 tasks.md:677. Good enough.

 I think I have enough for a thorough report. Two real defects (HIGH main-HEAD-in-worktree-instance,
 HIGH/MEDIUM admission parity), one MEDIUM fail-open (skipped worktree entries), one LOW (tree-entry
 budget), plus suggestions. Let me also quickly verify finding #1 fix feasibility:
 Repository::open(repository.commondir()) — commondir returns path to main .git; Repository::open on
 the .git dir opens the main repo ✓ (bare-ish open; head() works).

 Also double-check is_worktree method name in git2-rs: Repository::is_worktree() — yes exists.

 One more sanity check on HIGH #1: could repository.head() on a worktree-opened repo return the MAIN
 head? No — git2 resolves HEAD from the worktree's own admin dir (.git/worktrees/<name>/HEAD).
 Confirmed by design.

 And is there anything in spawn_local_ref_reconcile that canonicalizes to the main repo?
 dunce::canonicalize(&canonical_root) — the project root as opened (worktree path).
 git2::Repository::open(&canonical) — worktree-opened. Confirmed.

 Also for the deletion pass in the worktree-instance scenario: lane for refs/heads/main gets
 published; on next reconcile it stays (still "bare" per the broken check) — persistent violation,
 self-consistent wrongness.

 Also check route_ref_blob's IndexedFile::from_parse_result(result, bytes) vs route_catalog_files
 bytes.to_vec() — same.

 Write the report. Structure: Part 1 defects (2 HIGH, 1 MEDIUM, 1 LOW), Part 2 suggestions. Also
 confirm A–H closure status.

 Validation verdicts:
 - A (sensitive path rule): CLOSED for the rule itself, but the parity fix is INCOMPLETE — the ref
 path applies only sensitive_path_rule + classify_stable_content; the filesystem scout ALSO applies
 classify_admission tiers (lockfile/denylist/size/binary). So "byte-identical to filesystem scout"
 fails for those classes. → Defect 2.
 - B: CLOSED (composition tests are real, default path delegates to search_current/review_current;
 BTreeMap deterministic).
 - C: CLOSED (collected failures + unconditional deletion pass + real adversarial fixture).
 - D: CLOSED (per-lane generations; same-tip stable content gen; P0 untouched; TOCTOU closed by
 single-flight).
 - E: PARTIALLY CLOSED — worktree HEAD + main-HEAD fail-closed arms exist and are tested, BUT two
 residual fail-opens: (1) reconcile run from a worktree-opened repo never protects the MAIN
 worktree's branch (Defect 1); (2) listed-but-unclassifiable worktree entries are skipped (fail-open)
 instead of protected (Defect 3).
 - F: CLOSED w/ note (skip-drops-newer-trigger nuance; single-flight verified; test is manual-guard,
 not true concurrency).
 - H: CLOSED (composition-level isolation test attacks both directions).

 Let me write it. File:line citations:
 - Defect 1: src/live_index/local_ref_scout.rs:563-590 (main-HEAD arm, repository.head()),
 daemon.rs:3216-3219 (Repository::open(&canonical)); contract data-model.md:1258-1263, L-G01.
 - Defect 2: src/live_index/local_ref_scout.rs:287-315 (classify_ref_blob) vs
 src/discovery/mod.rs:838-862 + 2095-2140 (classify_admission); L-R10.
 - Defect 3: src/live_index/worktree_topology.rs:50-61 (skip paths); L-G01.
 - Defect 4 (LOW): local_ref_scout.rs:162-172 (tree entries uncounted).

 Line numbers — let me get precise ones for classify_ref_blob: lines ~287-315 (I read 296-315 region:
 sensitive check at 297, targets 302, classify 303). Reconcile main-HEAD arm: I read it at "match
 repository.head()" — in the raw output it was around line 563-595 in the earlier chunk (the chunk
 321-720 showed it; the match was ~line 563+). Let me compute: the chunk started at 321. The "match
 repository.head()" appeared roughly 60% through... I can grep for exact lines.

 Getting exact line citations for the findings.

 🔍 Grep: match repository.head\(\)|Some\(ObjectType::Tree\) =>|let Some\(_reconcile_guard\)|if let S
 └─ *162│                Some(ObjectType::Tree) => {
    *297│    if let Some(rule_id) = crate::knowledge::sensitive_path_rule(&entry.relative_path) {
    *533│    let Some(_reconcile_guard) = handle.try_lock_ref_reconcile() else {
    *572│    match repository.head() {

 All evidence gathered. Write the report now. Mark todos.

╭─── ☑ Todo 5 tasks ─────────────────────────────────────────────────────────────────────────────────╮
│ I. Contracts  1/1                                                                                  │
│ II. Code  2/2                                                                                      │
│ III. Attack                                                                                        │
│   └─ ☑ Attack A-H validation fixes + races + parity + tests                                        │
│ IV. Report                                                                                         │
│   └─ ☐ Write defects + suggestions report                                                          │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯

 Gate L Validation Review — Kimi Report (READ-ONLY, no edits made)

 Methodology applied: full-body reads of worktree_topology.rs (182 ln), local_ref_scout.rs (2420 ln
 incl. all tests), the Gate L symbols in store.rs/daemon.rs, search_scoped/review_scoped, plus the
 shared helpers they delegate to (classify_admission, classify_stable_content, sensitive_path_rule,
 process_file_with_classification, capture_repository_source, runtime_for_target). Parity diff,
 adversarial inputs, interleavings, and test-attacks below.

 Validation verdicts: B, C, D, H — closed, fixes hold under re-attack. A — the sensitive-path rule
 itself is closed, but the "byte-identical to filesystem scout" claim is not: the ref path skips the
 whole classify_admission tier (Defect 2). E — fail-closed arms exist and are tested, but two
 residual fail-opens on the same invariant remain (Defects 1 and 3). F — closed with one
 stale-rationale nuance (Suggestion 4).

 ────────────────────────────────────────────────────────────────────────────────

 Part 1 — Defects

 ### HIGH 1 — Reconcile run from a linked-worktree instance publishes the MAIN worktree's checked-out
 branch as a P1 lane (L-G01 violation)

 Claim: the "main repo HEAD" exclusion arm reads repository.head(), but when the daemon opened the
 project at a linked worktree, that is the worktree's HEAD — the main worktree's checked-out branch
 is never added to checked_out_refs and is misclassified as bare.

 Concrete failure scenario: repo with main worktree on main; git worktree add ../wt feature. User
 opens ../wt as a project (an explicitly supported topology — data-model.md:1260-1263: "checked-out
 linked worktrees remain separate existing ProjectInstances"). With SYMFORGE_LOCAL_REF_LANES=1,
 spawn_local_ref_reconcile runs git2::Repository::open(&canonical) where canonical = ../wt
 (daemon.rs:3216). In reconcile_local_ref_topology:
 - checked_out_worktrees(repository) (local_ref_scout.rs:545) enumerates only linked worktrees from
 the commondir admin dir — it never includes the main worktree.
 - The main-HEAD arm (local_ref_scout.rs:572) reads repository.head() = refs/heads/feature (the
 worktree's own HEAD), not refs/heads/main.
 - Result: refs/heads/main is not in checked_out_refs → published as P1 lane
 symforge:git-ref:<repo>:refs/heads/main — a branch that is the main instance's P0 lane, violating
 "checked-out branches are never P1" and duplicating another instance's P0 content as a P1 lane. The
 deletion pass keeps it (self-consistently wrong) on every subsequent pass.

 file:line: src/live_index/local_ref_scout.rs:572 (and the set construction at 545-549); trigger site
 src/daemon.rs:3216. Contract: L-G01 (tasks.md:697), data-model.md:1258-1263. No test covers a
 worktree-opened repository — every reconcile test does Repository::open(root) at the main root.

 Proposed fix: in reconcile_local_ref_topology, when repository.is_worktree(), also open the main
 repository and union its HEAD into checked_out_refs with the identical fail-closed arms:

 ```rust
   // after the existing repository.head() arm (~local_ref_scout.rs:590)
   if repository.is_worktree() {
       let main = Repository::open(repository.commondir())
           .map_err(|e| format!("Error: the main repository could not be opened ({e}); \
                                 local-ref reconcile fails closed."))?;
       match main.head() { /* same three arms as the existing main-HEAD block:
                              branch+name → insert; branch+undecodable name → Err;
                              detached → {}; unborn → {}; other error → Err */ }
   }
 ```

 Cleaner variant: move main-HEAD resolution into checked_out_worktrees (return the main worktree as a
 first-class entry), so the classifier owns the whole topology and the worktree-instance case is
 impossible to forget. Add a test: open the repo at the linked worktree (Repository::open(wt_path)),
 reconcile, assert refs/heads/main is not published.

 ────────────────────────────────────────────────────────────────────────────────

 ### HIGH 2 — Ref-blob admission skips the entire classify_admission tier: lockfiles, denylisted
 extensions, size thresholds, and binary sniff diverge from filesystem ingestion (L-R10 parity
 incomplete — finding A's fix is partial)

 Claim: classify_ref_blob applies sensitive_path_rule + classify_stable_content only. The filesystem
 scout additionally routes every entry through classify_admission (hard-skip >100 MB, lockfile
 basename, extension denylist, 1 MB-data/4 MB-code metadata-only threshold, 8 KB binary sniff) — none
 of which exists on the ref path.

 Parity diff (side by side):

 ┌──────────────┬──────────────────────────────────────────────┬────────────────────────────────────┐
 │ Step         │ Filesystem (discovery/mod.rs:838-862 →       │ Ref (local_ref_scout.rs:287-315)   │
 │              │ scout_decision_for_discovered:1141 →         │                                    │
 │              │ classify_admission:2095)                     │                                    │
 ├──────────────┼──────────────────────────────────────────────┼────────────────────────────────────┤
 │ path         │ path_reason (encoding/length)                │ n/a (UTF-8 enforced → degraded)    │
 │ metadata     │                                              │                                    │
 ├──────────────┼──────────────────────────────────────────────┼────────────────────────────────────┤
 │ sensitive    │ sensitive_path_rule → SensitivePath          │ ✓ identical (:297)                 │
 │ path         │                                              │                                    │
 ├──────────────┼──────────────────────────────────────────────┼────────────────────────────────────┤
 │ lockfile     │ MetadataOnly Lockfile                        │ missing                            │
 │ basename     │                                              │                                    │
 ├──────────────┼──────────────────────────────────────────────┼────────────────────────────────────┤
 │ extension    │ MetadataOnly GeneratedOrVendor               │ missing                            │
 │ denylist     │                                              │                                    │
 ├──────────────┼──────────────────────────────────────────────┼────────────────────────────────────┤
 │ >1 MB data / │ MetadataOnly OversizedData                   │ missing (only the 8 MiB blob       │
 │ >4 MB code   │                                              │ budget)                            │
 ├──────────────┼──────────────────────────────────────────────┼────────────────────────────────────┤
 │ binary sniff │ MetadataOnly Binary                          │ missing (UTF-8 check only when     │
 │ (8 KB)       │                                              │ targets.includes_knowledge())      │
 ├──────────────┼──────────────────────────────────────────────┼────────────────────────────────────┤
 │ content      │ classify_stable_content at ingest            │ ✓ identical (:303)                 │
 │ policy       │ (store.rs:3769)                              │                                    │
 └──────────────┴──────────────────────────────────────────────┴────────────────────────────────────┘

 Concrete adversarial inputs (committed to a bare branch, ref lane published):
 - package-lock.json (clean, 50 KB): disk → MetadataOnlyReason::Lockfile, no symbols; ref → admitted
 and parsed, thousands of dependency symbols in the ref lane's bridge/authority/conventions stats.
 L-R10 says "the same … config blob produces lifecycle/extraction … results identical to filesystem
 ingestion" — violated by construction in every JS/Rust repo (Cargo.lock, yarn.lock, … are
 near-universal), so every ref lane systematically carries pollution the disk lane refuses.
 - data.csv (2 MB, clean): disk → OversizedData (>1 MB data threshold); ref → ingested and parsed as
 text (under the 8 MiB budget).
 - assets/logo.png (binary): disk → DenylistedExtension/Binary metadata-only; ref → targets exclude
 knowledge → the UTF-8 decode guard is skipped → admitted → parsed as LanguageId::Text →
 FileOutcome::Failed { "unsupported text encoding" } file inside the ref index (pollutes
 health/diagnostics and the files map) instead of a clean metadata-only withhold.

 file:line: src/live_index/local_ref_scout.rs:287-315 vs src/discovery/mod.rs:2095-2140. Contract:
 L-R10 (tasks.md:677-680), L-G04 ("route blobs through the shared scout … adapters" — the admission
 tier is part of the scout adapter).

 Proposed fix: extract the path+size tiers of classify_admission into one shared helper and call it
 from both scouts, so parity is structural rather than copied:

 ```rust
   // in discovery (or knowledge): shared by both admission paths
   pub fn path_admission_reason(relative_path: &str, size: u64) -> Option<MetadataOnlyReason> {
       if is_dependency_lockfile(Path::new(relative_path)) { return
 Some(MetadataOnlyReason::Lockfile); }
       if denylisted ext { return Some(MetadataOnlyReason::GeneratedOrVendor); }
       if size > size_threshold_for(relative_path) { return Some(MetadataOnlyReason::OversizedData);
 }
       None
   }

   // in classify_ref_blob, after the sensitive_path_rule check:
   if let Some(reason) = path_admission_reason(&entry.relative_path, entry.size) {
       return RefBlobRoute::Withheld(reason);       // needs no bytes — works pre-materialization
   }
   if is_binary_content(bytes) {                  // ref already holds the bytes for ingest-decision
 blobs
       return RefBlobRoute::Withheld(MetadataOnlyReason::Binary);
   }
 ```

 Size tiering by entry.size (ODB header, already read) also lets oversized blobs be withheld before
 materialization. Add tests mirroring sensitive_path_blob_is_withheld…: committed package-lock.json →
 Lockfile; 2 MB .csv → OversizedData; small binary blob → Binary.

 ────────────────────────────────────────────────────────────────────────────────

 ### MEDIUM 3 — Worktree classifier fail-open: listed-but-unclassifiable worktree entries are
 silently skipped, so their checked-out branch becomes P1-eligible (L-G01)

 Claim: checked_out_worktrees skips three unclassifiable cases instead of treating them as
 unprovable: (a) errored/non-UTF-8 name entries (worktree_topology.rs:50-53), (b) find_worktree
 failure (:55-58), (c) validate() failure (:60-62). The same file deliberately fails closed for an
 unreadable HEAD (head_resolved = false) — an inconsistent policy on the same invariant.

 Concrete failure scenario: user moves a worktree directory without git worktree repair (or the
 worktree's gitdir back-link breaks). The admin entry .git/worktrees/<name>/ still exists and git's
 own branch-deletion protection still reads .git/worktrees/<name>/HEAD — i.e., git still considers
 the branch checked out [INFERENCE: git protects via admin-dir HEAD regardless of worktree-dir
 validity]. validate() fails → the entry is skipped → its branch is classified bare → published as a
 P1 lane, while a live ProjectInstance candidate still holds it checked out. Compare: a readable but
 unresolvable HEAD aborts the whole pass; an unclassifiable worktree silently proceeds — the weaker
 trigger gets the weaker treatment.

 file:line: src/live_index/worktree_topology.rs:50-62. Contract: L-G01; mirrors finding E's own
 rationale.

 Proposed fix: on validate()/find_worktree failure, fall back to reading the admin HEAD file directly
 (this is exactly the source git's checkout protection uses), and only fail closed if that too is
 unreadable:

 ```rust
   // in checked_out_worktrees, replacing the `continue` on validate() failure:
   if worktree.validate().is_err() {
       let admin_head = repository.path().join("worktrees").join(name).join("HEAD");
       match std::fs::read_to_string(&admin_head)
           .ok()
           .map(|s| s.trim().strip_prefix("ref: ").map(str::to_string))
       {
           Ok(Some(refname)) => { out.push(CheckedOutWorktree { name: name.into(), path,
                                         head_ref: Some(refname), head_resolved: true }); continue; }
           _ => { out.push(CheckedOutWorktree { name: name.into(), path,
                          head_ref: None, head_resolved: false }); continue; } // reconcile fails
 closed
       }
   }
 ```

 This protects the branch without wedging reconcile on genuinely pruned entries whose admin data
 lingers (a deleted-dir worktree whose admin HEAD still parses protects its branch — matching git
 semantics — until git worktree prune removes the admin entry). A prune-aware refinement (skip only
 when the admin dir is gone entirely) can sit on top.

 ────────────────────────────────────────────────────────────────────────────────

 ### LOW 4 — Tree entries are uncounted against the scout budget: a pathological tree shape does
 unbounded DFS work

 Claim: the entry budget is checked only in the Blob arm (local_ref_scout.rs:167-171); Tree entries
 (:162-166) are pushed and read without any count or depth bound. A branch containing millions of
 nested/empty directories forces that many find_tree calls in the detached blocking task. P0 is
 unaffected (detached spawn_blocking, L-V04 holds), but a blocking thread burns CPU for the lifetime
 of the pass and the catalog reports Complete for what was effectively an unbounded walk.

 Concrete trigger: committed tree with 500k empty nested dirs; budget.max_entries never trips because
 no blobs are ever reached.

 file:line: src/live_index/local_ref_scout.rs:161-172. Contract: L-R07 ("entry/blob … budgets cannot
 block" — honored for P0, not for the pass itself).

 Proposed fix: count every visited tree entry (not just blobs) against max_entries, or add a separate
 max_tree_entries/depth bound that degrades coverage identically:

 ```rust
   for entry in tree.iter() {
       visited += 1;
       if visited > budget.max_entries { coverage = Degraded; break 'walk; } // covers trees AND
 blobs
       ...
   }
 ```

 ────────────────────────────────────────────────────────────────────────────────

 Part 2 — Suggestions

 ### S1 (high value) — Skip re-publish when the branch tip is unchanged: kills per-reload re-parse
 cost and registry churn

 Every reconcile pass republishes every bare branch: full blob materialization + parse per branch on
 every open/reload, even when nothing moved. build_ref_source_generation already preserves
 content_generation for same-tip republish (store.rs:1392-1406), so the work is provably redundant.
 registry_generation consumers are tests-only (verified by grep), so the churn is cosmetic — but the
 parse cost is real and scales with (branches × files) per reload. Cross-branch tip sharing (a
 freshly cut feature branch == main's tip) pays the same parse twice.

 file:line: src/live_index/local_ref_scout.rs:600-620 (publish loop), store.rs:1392.

 Proposed change: before scouting a branch in the publish loop, resolve the branch tip OID (cheap)
 and compare with the existing lane's source_version.commit; if equal, skip scout+ingest entirely:

 ```rust
   let tip = repository.find_reference(ref_name).ok().and_then(|r| r.target());
   let existing_tip = handle.published_source_set().sources
       .get(&SourceId::new(format!("symforge:git-ref:{}:{ref_name}", repository_id.as_str())))
       .and_then(|g| g.source_version.as_ref()?.commit.clone());
   if tip.is_some() && tip.map(|t| t.to_string()) == existing_tip { continue; } // lane already
 current
 ```

 ### S2 (medium value) — Add the missing test cells that would have caught Defects 1–3

 - Reconcile with Repository::open(wt_path) (worktree-opened repo) asserting refs/heads/main is never
 published (Defect 1).
 - Lockfile/oversized/binary ref blobs withheld with the filesystem-identical MetadataOnlyReason
 (Defect 2).
 - Moved-worktree (validate-fails, admin HEAD intact) classified as checked-out, not skipped (Defect
 3).
 - A real two-thread single-flight test: two std::thread::spawn reconciles on one handle, assert
 exactly one ran (skipped == false) and the other's skipped == true, total lane set correct. The
 current concurrent_reconcile_is_single_flighted_and_skips holds the guard manually — it proves the
 try_lock branch, not the interleaving.

 ### S3 (medium value) — route_ref_blob is now test-only; narrow it or use it in production

 route_ref_blob (public, local_ref_scout.rs:325) duplicates route_catalog_files' per-entry logic
 minus the parse cache, and its only callers are tests (route_single). Two entry points for the same
 routing invites drift (exactly how Defect 2-class gaps form). Proposed change: make it
 #[cfg(test)]/pub(crate), or better, have it delegate to the same single-blob path
 route_catalog_files uses (extract route_one(entry, bytes, &mut parse_cache)), so tests exercise the
 production code path.

 ### S4 (low value) — Single-flight skip drops the fresher trigger; the guard comment overclaims

 local_ref_scout.rs:530-533 justifies skipping with "the running pass already reflects the newest
 refs." Interleaving that falsifies it: pass A reads topology → branch X created → reload triggers
 pass B → B skips → A's snapshot predates X → X has no lane until a third reload. Within the
 documented open/reload-only ceiling, but the rationale is wrong as stated. Proposed change: either
 fix the comment ("a skipped pass may be one trigger stale; the next reload reconciles"), or set a
 dirty flag on skip that the running pass checks at completion to immediately re-run once.

 ### S5 (low value) — is_safe_template_basename template bypass also applies to ref paths — verified
 safe, worth a comment

 .env.example committed to a branch is admitted on both paths (template allowlist in
 sensitive_path_rule, knowledge/mod.rs:240-247) — parity holds. No change needed; noting it because
 the A-fix review asked about paths the rule does not cover, and this is the one intentional bypass,
 already mirrored byte-identically.

 ### S6 (low value) — Duplicated byte copies for multiply-mapped blobs

 route_catalog_files does bytes.to_vec() per path (local_ref_scout.rs:436-440), so a blob mapped at N
 paths stores N copies in the final files map despite L-G03's dedup-by-object-ID at the
 materialization layer. Bounded by the 8 MiB budget × path count; fine today. Proposed change (only
 if profiles show it): let IndexedFile::from_parse_result accept Arc<[u8]>.

 ────────────────────────────────────────────────────────────────────────────────

 What I verified clean (no findings)

 - P0/P1 race (item 1): write_mutex is parking_lot::Mutex (no poisoning); every P0 swap path
 (store.rs:1855-1970, :2905-2915) and both P1 mutations hold it across load→copy→mutate→store;
 publish_ref_source re-reads current_source_id from the in-lock snapshot so it can't resurrect a
 stale current; next_after_current_publish only drops the old current (never a git-ref lane); the
 content_generation TOCTOU in build_ref_source_generation is genuinely closed by single-flight (only
 reconcile publishes ref lanes). A reconcile panic cannot stall P0 (detached spawn_blocking, no
 poisoning).
 - L-V02 default path: gate OFF returns None before canonicalize/git-open (daemon.rs:3201-3203); only
 cost is one env::var read per reload (already documented as Cursor LOW #8).
 - Lane-id parsing: RepositoryId is a hex digest of the commondir (persist.rs:437-440); git refnames
 cannot contain : → strip_prefix parse is collision-safe; deletion pass only touches lanes with this
 instance's exact symforge:git-ref:<repo>: prefix.
 - Determinism (L-R09): PublishedSourceSet.sources is a BTreeMap (store.rs:888) → All-scope lane
 order and combined_result_hash pair order are deterministic.
 - L-R11 chokepoint: runtime_for_target builds project_indexes exclusively from session.servers
 (daemon.rs:1872-1877); alias resolution matches only among the session's own open ids; both
 dispatch-level tests exercise the real execute_tool_call path with wildcard and explicit-id vectors.
 - B byte-identity: search_scoped/review_scoped delegate Current to search_current/review_current on
 the captured current generation; double validate_input is output-identical.
 - Parse-cache key (L-R02/L-R14): process_file_with_classification uses relative_path only for the
 label, is_tsx, and the .h C/C++ disambiguation gate (verified all 20 uses in parsing/mod.rs) — all
 three covered by the cache key; C-header disambiguation's content-driven C→C++ flip is key-safe.


 path to file?


 The user asks "path to file?" — they probably want the report written to a file, or they're asking
 where the report file is. The review said "A written report (no file edits)". I output the report in
 chat. The user asks for a path to a file — likely wants the report saved as a file. Ambiguous: "path
 to file?" might mean "what file should I look at?" or "give me the report as a file path".

 The GATE-L-REVIEW.md instructed output as a written report, read-only (no code edits). Writing a
 report file is not a code edit. The natural location: specs/020-repository-knowledge-index/. There's
 likely a convention — maybe previous review files exist. Check.

 🔍 Glob: E:\project\symforge\specs\020-repository-knowledge-index 29 files · in specs/020-repository
 ├─ 📝 specs/020-repository-knowledge-index/GATE-L-REVIEW.md
 ├─ 📝 specs/020-repository-knowledge-index/tasks.md
 ├─ 📝 specs/020-repository-knowledge-index/CURSOR-REVIEW-PROMPT-2.md
 ├─ 📝 specs/020-repository-knowledge-index/CURSOR-REVIEW-PROMPT.md
 ├─ 📝 specs/020-repository-knowledge-index/HANDOVER-2026-07-22.md
 ├─ 📝 specs/020-repository-knowledge-index/fable-gate-d-review-request-2026-07-21.md
 ├─ 📝 specs/020-repository-knowledge-index/fable-focused-rereview-2026-07-17.md
 ├─ 📝 specs/020-repository-knowledge-index/GOAL.md
 └─ … 21 more files

 Convention: review reports live alongside the request, e.g. adversarial-review-2026-07-16.md for
 review-request-fable.md, fable-gate-d-review-2026-07-21.md for its request. Gate L review →
 gate-l-review-kimi-2026-07-24.md fits naming style. Write the report there.

 The review was output in chat only — no file written yet. 