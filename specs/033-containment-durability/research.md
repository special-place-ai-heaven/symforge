# Research: 033-containment-durability

Phase 0 decisions. Every NEEDS CLARIFICATION from planning is resolved here. Alternatives that lost are recorded so implementers do not reopen them.

## 1. Shared no-follow helper (F-001, F-003)

**Decision:** Add `path_must_be_regular_non_link(path) -> io::Result<Metadata>` in `src/paths.rs` next to `metadata_is_reparse_point`. It `symlink_metadata`s, then refuses if `file_type().is_symlink()` **or** `metadata_is_reparse_point`, and refuses if the entry is not a regular file (for read lanes) or, for the hasher, if a directory entry is a link/reparse (do not recurse).

**Rationale:** The crate already owns the Windows junction predicate (`paths.rs:7-37`) and uses it for state directories. Physical-root writes already refuse link components via cap-std `symlink_metadata` (`physical_root.rs:320-351`). Discovery catalog already skips non-files and canonicalize-outside (`discovery/mod.rs`). The leak is the *other* readers: `std::fs::read` in `admit_disk_read` (`read_gate.rs:274`), `Path::is_file()` in `admit_worktree_text` (`read_gate.rs:122`, follows), `observe_disk_beneath` lexical-only (`read_gate.rs:70-93`), `std::fs::metadata` in watcher freshen (`watcher/mod.rs:234`), `File::open` in `FilesystemStableReadAccess::read_pass` (`store.rs:718`), and `file_type().is_dir()` recursion in `collect_config_dir` (`trust.rs:469-476`) which treats junctions as directories.

**Call sites that must switch (US1):** `admit_disk_read`, `admit_worktree_text`, watcher freshen stat/reindex, `FilesystemStableReadAccess::read_pass` (including `path_after` stamp — not following `fs::metadata`), `scout_single_path` in `discovery/mod.rs` (today `|path| std::fs::metadata(path)`). `observe_disk_beneath` is covered once `admit_disk_read` refuses links. Freshen path helpers delete `Err(_) => Ok(canon_root.join(relative))` (`tools.rs` `safe_repo_path_for_freshen`, `sidecar/handlers.rs` `safe_sidecar_path_for_freshen`) and propagate `safe_repo_path` errors as no-op/refuse.

**Not in this helper:** catalog walker (R-004, already skip). `select_runtime_data_base` (unused). Physical-root writes (already confined).

**Alternatives considered:**
- Unix `O_NOFOLLOW` + Windows `FILE_FLAG_OPEN_REPARSE_POINT` on the open itself: stronger TOCTOU story; more `OpenOptionsExt` cfg. Upgrade path if a test ever shows a swap between metadata and open. Ponytail ceiling: metadata check first; open-nofollow later.
- Canonicalize-and-`starts_with` on every read: `safe_repo_path` already does this when the path exists, but freshen *undoes* it on `Err`. Fix the fallback; still refuse links because a link *inside* the repo to a file also *inside* the repo is a follow we do not want for secrets-via-indirection, and junctions to outside fail canonicalize containment only when the target exists.
- cap-std for protocol reads: too large; physical_root already has that for writes.

## 2. Sidecar bind policy (F-002)

**Decision:** Do **not** add `src/bind_policy.rs` (crate-root modules require `internals.rs` + Feature 020 T051 pin churn). `is_loopback_bind_host` is `pub(crate)` in `src/paths.rs`; daemon delegates. Sidecar calls `crate::paths::is_loopback_bind_host`. Daemon keeps `SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK`. Sidecar uses sibling `SYMFORGE_SIDECAR_ALLOW_NON_LOOPBACK`. Do **not** share the daemon opt-in env.

Non-loopback without opt-in: spawn fails (copy daemon tests at `daemon.rs:7348-7373`). Non-loopback with opt-in but no `SYMFORGE_SIDECAR_TOKEN`: spawn fails. Non-loopback with opt-in and token: warn, start, attach Bearer middleware **only** on that listener. Loopback: no Bearer (hook-compat). Default bind stays `127.0.0.1`.

**Rationale:** Sidecar today reads `SYMFORGE_SIDECAR_BIND` and binds with no loopback check (`server.rs:42`). Router middleware is only `caller_root_guard` (`router.rs:54-57`). `token_stats` is a savings counter, not auth. Daemon and `symforge serve` (`server/auth.rs`) already close this footgun.

**Alternatives considered:**
- Reuse `SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK` for sidecar: couples two processes; rejected.
- Warn-only on non-loopback: does not meet FR-004.
- Bearer on loopback too: out of spec; breaks hooks.
- Copy-paste the 8-line predicate into sidecar: ponytail-legal, but daemon tests already pin the names; one predicate file avoids drift.

## 3. Durable persist helper (F-004, F-007, F-008)

**Decision:** Do **not** add `src/fs_persist.rs`. Add `pub(crate) fn write_bytes_atomic` in `src/idempotency.rs` next to `write_record_atomic`: `NamedTempFile::new_in(parent)`, write, flush, `sync_all`, `persist`, then Unix `File::open(parent)?.sync_all()`. Identical to `idempotency.rs:523-539` and `cli/harness_apply.rs:239-254`. Knowledge curation already adds parent-dir sync (`knowledge_curation.rs:874`).

**F-004 snapshot/artifact:** Do **not** route `index.bin` through this helper if the snapshot code already has a named tmp + lock (`write_snapshot` in `persist.rs`). Add `sync_all` on the tmp `File` before rename, and Unix parent sync. Team-artifact: same for `index.bin.zst`; replace in-place `artifact.json` write with `write_bytes_atomic`.

**F-007 / F-008:** `write_store`, `write_cli_store`, and production `cli/init.rs` `fs::write` sites (not `#[cfg(test)]` fixtures) call `write_bytes_atomic`.

**Do not use** `protocol/edit.rs::atomic_write_file` — that spends a source-mutation lease.

**Alternatives considered:**
- Three more copy-pasted `atomic_write` functions in trust/init/persist: works, violates “reuse the helper”.
- fsync-only without rename for trust.json: still tears on crash mid-write.

## 4. Panic-free splice/diff (F-005)

**Decision:** `apply_splice` returns `Result<Vec<u8>, String>`. Reject `start > end` or `end > len`. Call sites (`edit.rs` and `edit_tools.rs`) propagate. `extract_symbols_for_diff`: before `&source[start..end]`, require `end <= len`, `start <= end`, and `is_char_boundary` on both; skip that symbol (do not panic; do not abort the whole file unless every range is bad). Valid ranges still hash.

**Rationale:** `catch_unwind` only wraps `parse_source` (`parsing/mod.rs:1132`). Capacity `len - (end - start)` wraps if `end < start`.

**Alternatives considered:** `saturating_sub` without a Result: still panics on `&content[end..]` when `end > len`.

## 5. Windows token DACL (F-006)

**Decision:** Keep Unix `0o600` create. On Windows, after (or while) writing the token file, set an owner-only DACL using the already-depended `windows` crate (`Cargo.toml:159`). Add features `Win32_Security` and `Win32_Security_Authorization` as required by the chosen APIs. DACL: current user + SYSTEM, no Everyone/Users read. Test cfg(windows) asserts those ACEs are absent.

**Rationale:** No ACL helper exists in-tree. `std::fs::write` (`daemon.rs:600`) inherits the directory ACL.

**Alternatives considered:**
- New `windows-acl` crate: extra dep for a one-shot DACL.
- Document that `%LOCALAPPDATA%` is owner-only: not true if control-state is relocated; Unix already decided the file is the barrier.

## 6. Honest lifecycle docs vs T051 (F-009)

**Decision:** Rewrite `src/index_lifecycle/mod.rs` darkness paragraph to name live `project_source_authority` / confined-write call edges. Keep “Nothing in production calls this module” only on submodules that still have no production callers. Verify at implement time: `candidate`, `supervisor`, `verification` were still dark; `observer.rs` (`ObserverId` in watcher/persist) and `query.rs` (`HealthProjection` in `health_view.rs`) were stale. Do **not** change Feature 020 T051’s call-edge rules.

**Pin refresh is mandatory:** Feature 020 darkness test T051 seals `EXCLUDED_RUNTIME_SOURCE_PIN_V1` over `index_lifecycle/mod.rs` bytes (`tests/preventive_runtime_dark_v11.rs`). A doc edit changes that hash **and** `FULL_SOURCE_PIN_V1`. Recompute via the rust oracle after `cargo fmt`, never by hand (033 task T060). Any other `src/` edit in this campaign also moves `FULL_SOURCE_PIN_V1`.

**Worktree implication:** Do not merge pin-file conflict sides. Recompute once on the integration tree after all stories land.

## 7. Foundational vs parallel worktrees

**Decision:** One foundational commit on `033-containment-durability` adds helpers in existing modules (`paths.rs`, `idempotency.rs`, `pub(crate)` loopback predicate in `daemon.rs`). Then fan-out worktrees from that commit with disjoint `OWNS`:

| Worktree | Stories | OWNS |
|----------|---------|------|
| wt-f001 | US1 | `read_gate.rs`, `tools.rs` freshen helper only, `sidecar/handlers.rs` freshen helper only, `watcher/mod.rs` freshen stat, `store.rs` `read_pass`+`path_after`, `discovery/mod.rs` `scout_single_path` |
| wt-f002 | US2 | `sidecar/server.rs`, `sidecar/router.rs`, sidecar tests |
| wt-f003-007 | US3+US7 | `edit_safety/trust.rs`, `cli/trust.rs` |
| wt-f004 | US4 | `live_index/persist.rs` snapshot/artifact write |
| wt-f005 | US5 | `protocol/edit.rs` `apply_splice` + call sites, `parsing/mod.rs` `extract_symbols_for_diff` |
| wt-f006 | US6 | `daemon.rs` `write_daemon_token_file_at` only |
| wt-f008 | US8 | `cli/init.rs` production writers |
| wt-f009 | US9 | `index_lifecycle/mod.rs` docs plus stale submodule darkness (`observer.rs`, `query.rs`); pin refresh deferred to integration |

`tools.rs` is huge; US1 only edits the freshen helper (~1216-1238) plus tests. US5 must not rewrite unrelated `edit.rs` regions if US1 does not touch them — `edit.rs` is US5-only except `safe_repo_path` stays as-is.

Do **not** mix US1 and US2 in one commit (review constraint).

## 8. RED-first

Every story lands a failing test against current code, receipts the failure, then machinery. Embed gate after any new non-`cfg(test)` helper: `cargo test --no-default-features --features embed --lib -- --test-threads=1`.
