# Adversarial implementation review — 033-containment-durability

**Date**: 2026-09-18  
**Against**: branch `033-containment-durability` after story merge and ingest-seam fix.  
**Cfg-lens**: Unix `0o600`, Windows owner+SYSTEM DACL, Windows nested `mklink /J` then `junc/secret`, sidecar Bearer only off-loopback.

Independent reviewers: [Code Reviewer](1063b10e-5764-4937-8184-c7c8af377d80), [Security Reviewer](ba0f395e-c4d8-49ab-b1bf-0c16bbe13ac8), [Converge assessor](ee38a0f8-c37d-457a-9e9a-e03c418de9f9). Spec/plan hostility is in `033-adversarial-spec-plan-2026-09-18.md`.

## Closed (observed in code)

- **F-001 retrieval**: `safe_repo_path_for_observe` canonicalizes existing in-repo files and returns the unresolved join only when the leaf is missing (`symlink_metadata` NotFound) so scout can unpublish. Existing outbound symlink/junction paths stay `Err` — no join-fallback of a live link. `admit_worktree_text` canonicalizes first. Watcher freshen skips when that path is outside. Nested Windows fixture is `junc/secret.txt`.
- **F-001 ingest (review HIGH)**: `read_and_index_with_stable_read_receipt` now calls `canonical_abs_stays_under_relative_root` before scout, so watcher events and `analyze_file_impact` / sidecar `/impact` cannot publish `junc/secret` under the in-repo name. Oracle: `outbound_link_single_path_ingest_skips_nested_secret`. Missing indexed files still unpublish (`a_failed_read_observation_preserves_the_fence_the_observer_moves`).
- **F-002**: `resolve_sidecar_bind`; Bearer layer only on `NonLoopbackAuthenticated`; loopback plus leftover token env is not 401. Empty/whitespace token refuses spawn (`test_sidecar_bind_policy_rejects_empty_or_whitespace_token`).
- **F-003**: `collect_config_dir` skips symlink/reparse, warns, evaluation Untrusted.
- **F-004**: `persist_named_tmp` `sync_all` then rename; `snapshot_sync_all_is_observed_before_rename`.
- **F-005**: `apply_splice` is `Result`; `symbol_diff_pairs` skips illegal/non-char ranges.
- **F-006**: Unix `0o600` at create; Windows DACL after every `fs::write`, including replace. Token file is not DurableFile tmp+rename.
- **F-007 / F-008**: `write_bytes_atomic` for trust store and init client configs.
- **F-009**: parent `index_lifecycle` docs name production callers; darkness phrase remains on `candidate` / `supervisor` / `verification`.
- **Pins**: `FULL_SOURCE_PIN_V1` / `EXCLUDED_RUNTIME_SOURCE_PIN_V1` from the rust oracle only.
- **Rejected R-001…R-007**: untouched. `select_runtime_data_base` unused. No loopback sidecar Bearer. No `bind_policy.rs` / `fs_persist.rs`.

## Remaining (not blocking F-001…F-009)

- **LOW**: no failpoint that `sync_all` `Err` leaves the previous snapshot bytes (production already returns `Err` before rename). Ordering oracle exists.
- **LOW**: `observe_disk_beneath` is still lexical; retrieval callers canonicalize first. Catalog canonicalize `unwrap_or_else(|_| entry.path())` is fail-open on rare canonicalize errors.
- **LOW**: Windows token write has a brief inherited-ACL window before DACL replace; closing it needs `CreateFile` with a security descriptor, not `write_bytes_atomic`.

## Verdict

F-001…F-009 as specified are implemented. Ingest-door HIGH from implementation review is folded. R-001…R-007 stay rejected.
