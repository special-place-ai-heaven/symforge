# Recovered Claude Findings — Local Validation

> Validated locally against the current working tree without Claude, Fable, workflows, or delegated agents. The original 300 KB workflow output is preserved as `docs/reviews/wplq7sl80.output`.

## Verdict

**Not merge-ready.** The recovered workflow produced real value: 21 findings were recovered; 18 are confirmed or high-confidence current issues, one duplicates another contract-drift finding, one Grok finding is stale because it was subsequently fixed, and one is a low-impact blocking-IO concern with no current production caller.

## Finding matrix

| # | Recovered finding | Local verdict | Merge impact |
|---|---|---|---|
| 1 | `detect_impact` source filtering has no exclusion disclosure | **Confirmed** | Important |
| 2 | Empty filtered `what_changed` response hides default filtering | **Confirmed** | Important |
| 3 | `code_only` drops unknown-extension source files (.sql/.sh/.ps1/.proto/.tf/Dockerfile/Makefile) | **Confirmed** | Important |
| 4 | Feature 018 browse/detect-impact contract docs are stale | **Confirmed** | Documentation |
| 5 | Browse intent differs across handler/options/engine for prefixes normalizing to Any | **High confidence** | Minor |
| 6 | Overlay browse path lacks primary-engine `(name, kind)` deduplication | **Confirmed, currently latent** | Minor |
| 7 | Compact repo map accepts parent-relative, UNC, and backslash-rooted foreign paths | **Confirmed** | Important |
| 8 | Duplicate CCR inserts double-count `total_bytes` and economics bytes | **Confirmed** | Important |
| 9 | CCR footer can make the returned payload exceed its byte budget | **Confirmed** | Minor |
| 10 | Snapshot quarantine deletes `index.bin` without the per-path snapshot lock | **Confirmed** | Important |
| 11 | Snapshot publish does not fsync the temp file or containing directory | **Confirmed** | Minor durability gap |
| 12 | PID/counter snapshot temps survive crashes until explicit reset | **Confirmed** | Minor cleanup gap |
| 13 | `reset_snapshot_state` performs blocking lock/IO work | **Confirmed but no current production caller found** | Non-blocking |
| 14 | Retarget can retain and reuse a stale per-session server for a previously evicted project | **Confirmed on current implementation** | Critical |
| 15 | Project-slot insertion can race cleanup and reinsert a removed slot | **Confirmed** | Important |
| 16 | Additive open can attach a project after the session closes, leaving orphan membership | **Confirmed** | Minor lifecycle race |
| 17 | Reload preserves session-scoped caches that previously reset with the server | **High confidence** | Minor compatibility/cache risk |
| 18 | Contract says browse ranking only, omitting dedup behavior | **Duplicate of #4** | Documentation |
| 19 | Grok inline-table merge destroys user values | **Stale/refuted on current tree** | Fixed with TableLike merge and regression |
| 20 | Proxy-failure refusal test assumes TCP port 1 is unused | **Confirmed test-quality defect** | Important verification reliability |
| 21 | Immutable-home slice tests only successful checkpoint writes, not degraded checkpoint failure | **Confirmed coverage/spec gap** | Required before completion |

## Evidence highlights

### Source filtering

Current `filter_paths_by_prefix_and_language` returns false whenever `LanguageId::from_extension` returns `None`. This treats unrecognized source formats as data. The uncommitted `what_changed` path defaults `code_only` to true, then returns a bare “matched requested filters” message when filtering removes everything. `detect_impact` similarly filters before totals/formatting and emits no exclusion note.

### Containment

`src/sidecar/handlers.rs::is_intra_workspace_path` only rejects paths containing `:` or starting with `/`. It accepts `../evil.rs`, UNC/backslash-rooted paths, and therefore does not match the stronger full/tree containment guard.

### CCR

`CcrStore::insert` always adds the new byte length before replacing the content-addressed HashMap entry; it never subtracts an existing blob. `apply_ccr_overflow` appends the CCR footer after summary truncation without reapplying the output budget.

### Snapshots

`write_snapshot` and `reset_snapshot_state` share a per-path mutex, but `quarantine_bad_snapshot` writes quarantine artifacts and removes the active snapshot without that lock. Snapshot publication uses write+rename but no `sync_all`. Unique temp cleanup exists only on explicit reset.

### Daemon

The current implementation still destructively retargets `active_project_id`. It keeps session servers in a map and uses `entry(...).or_insert(server)`, which preserves an older server for a project revisited after eviction. Project insertion and session attachment occur across separately locked phases, leaving cleanup/close races. The dirty immutable-home tests describe the intended replacement contract but production implementation is not present yet.

### Dirty tree

The Grok inline-table finding was valid against Claude’s frozen snapshot but has since been fixed using `TableLike` mutation with comment/idempotency regression coverage. The daemon and watcher dirty slices remain red-test-only/incomplete.

## Merge blockers

1. Complete immutable-home/additive daemon implementation; eliminate stale server reuse and project/session lifecycle races.
2. Complete watcher generated-output parity implementation.
3. Fix source-filter classification and truthful empty-result disclosure.
4. Apply one containment guard consistently to compact/full/tree repo maps.
5. Correct CCR replacement accounting.
6. Serialize quarantine with snapshot publication/reset.
7. Make proxy-failure test hermetic and add checkpoint-failure coverage.
8. Reconcile Feature 018 contract documentation.
9. Run full headless verification after implementation.

## Non-blocking follow-ups

- fsync semantics for crash-level durability;
- startup cleanup for unique snapshot temp files;
- overlay browse parity before that route becomes production-reachable;
- explicit cache invalidation contract on project reload.

## Artifact provenance

- Raw recovered workflow: `docs/reviews/wplq7sl80.output`
- Recovery index: `docs/reviews/2026-07-10-claude-workflow-recovered.md`
- Original synthesis: not produced because quota was exhausted.
