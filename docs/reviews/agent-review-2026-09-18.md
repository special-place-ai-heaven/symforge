# Agent full-repo code review — 2026-09-18

- **HEAD:** `7ecaec876cd4bcae2be889c39765821a5e77b17a` (`docs: record the Fathom embed v11.3.0 handoff (#707)`)
- **Tree:** `git status --short` clean at review start (untracked `.tmp/review/` is gitignored via `/.*/`).
- **Scope:** production `src/` (empty skill argument = whole tree). Skipped `tests/`, `benches/`, `docs/`, `specs/`, `target/`, `node_modules/`. `scripts/` glue scanned, not line-audited.
- **Modules covered:** 36 mapped modules (see `.tmp/review/module-map.md`). Production `.rs` count ~194 (crate-root 18, `parsing` 33, `protocol` 30, `live_index` 27, `index_lifecycle` 19, `cli` 17, `stel` 16, plus remaining dirs).
- **Coverage of the module map:** 100% of modules visited. Trust surfaces (`paths`, `edit_safety`, `read_gate`, `server/auth`, `sidecar`, `discovery` walk, `physical_root`, `process_util`) were read in full. Files over ~2k lines (`protocol/tools.rs`, `daemon.rs`, `live_index/{store,persist}.rs`, `discovery/mod.rs`, `watcher/mod.rs`, `sidecar/handlers.rs`, `protocol/format.rs`) were reviewed by complete reads of every production function implicated by path/read/write/auth greps, not by a linear pass through their `#[cfg(test)]` modules (tests skipped per skill). Language extractors: production panic/`unwrap` census only.
- **Findings after Phase-2 re-read:** 9 kept, 7 rejected as misreads (plus one comment that was the F-001 root cause, not a separate defect). Checkpoints: `.tmp/review/{findings,rejected,progress}.jsonl|md`.

Review-only: no production source was modified.

---

## Findings

Ordered security → bug·high → bug·medium → bug·low → performance → maintainability/docs.

### F-001 — security · high — symlink follow + freshen fallback undoes containment

**Where:** `src/protocol/tools.rs:1231`, `src/sidecar/handlers.rs:240`, `src/protocol/read_gate.rs:70` and `:121`, `src/live_index/store.rs:718`, `src/watcher/mod.rs:234`

**What:** `safe_repo_path` is the crate’s canonicalize-and-`starts_with` containment check. Both freshen helpers call it, then on **any** `Err` — including “path is outside the repository” after following a symlink/junction — return `Ok(canon_root.join(relative))`:

```1229:1236:src/protocol/tools.rs
    match edit::safe_repo_path(repo_root, relative_path) {
        Ok(path) => Ok(path),
        Err(_) => {
            let canon_root = repo_root
                .canonicalize()
                .map_err(|e| format!("cannot resolve repo root: {e}"))?;
            Ok(canon_root.join(relative))
        }
    }
```

(`safe_sidecar_path_for_freshen` is the same shape at `sidecar/handlers.rs:240-247`.)

`get_file_content` calls `freshen_exact_path_for_targeted_retrieval` **before** the index lookup (`tools.rs:9055`). Freshen stats and reindexes via `std::fs::metadata` / `File::open` (`watcher/mod.rs:234`, `store.rs:718`), which follow links. Outside-repo bytes are published under the in-repo relative name. The later disk arm of `get_file_content` uses `safe_repo_path` and would refuse; it never runs if the index now holds the file.

The same follow occurs on:

- `admit_worktree_text` (`read_gate.rs:121-128`): `Path::is_file()` + `admit_disk_read` → `std::fs::read`. Used by untracked `search_text` (`tools.rs:2618`) and `diff_symbols` / `detect_impact` worktree seeding (`tools.rs:8753`).
- `observe_disk_beneath` (`read_gate.rs:67-93`): lexical `..` / prefix only. The ponytail comment at `:70-73` says a symlink-out is “governed by the crate’s existing never-follow walk policy”. Bulk catalog does skip non-files and drops canonicalize-outside entries (`discovery/mod.rs:578-585`). Watcher, freshen, and worktree reads do not use that walk.

**Why it matters:** A cloned repository that contains a file symlink (or a Windows junction the watcher observes) to `/etc/passwd`, `~/.ssh`, or sibling-project secrets is disclosed to MCP and sidecar clients — including agents that are not running as a filesystem explorer. This is a guaranteed user-visible confidentiality failure given a malicious or careless tree, not a theoretical TOCTOU.

**Fix:** Delete the `Err(_) => join` fallback; a `safe_repo_path` failure is a refusal. Open with no-follow (`O_NOFOLLOW` / reject `symlink_metadata` and `paths::metadata_is_reparse_point`) in `admit_disk_read`, `FilesystemStableReadAccess::read_pass`, and watcher reindex. Do not treat `is_file()` on a followed link as in-repo.

**Confidence:** certain.

### F-002 — security · medium — sidecar bind override has no loopback refuse and no auth

**Where:** `src/sidecar/server.rs:42`, `src/sidecar/router.rs:22`

**What:** `spawn_sidecar` binds `SYMFORGE_SIDECAR_BIND` (else `bind_host`) at `{host}:0` with no loopback check. The router exposes `/outline`, `/impact`, `/symbol-context`, `/repo-map`, `/workflows/source-read`, and related GETs. The only middleware is `caller_root` 409. No Bearer, no Origin.

Contrast: daemon refuses non-loopback unless `SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK=1` (`daemon.rs:630-651`). `symforge serve` refuses to start on a routable bind without a key (`server/auth.rs:59-63`, `server/serve.rs:557-558`).

**Why it matters:** Default bind is still `127.0.0.1`. Setting the env to `0.0.0.0` (or a LAN address) publishes an unauthenticated source-intelligence HTTP API. That is an operator footgun the other two HTTP surfaces already close.

**Fix:** Reuse `resolve_daemon_bind_host` (or the same predicate). Off-loopback requires an explicit opt-in and should not start without a token.

**Confidence:** certain.

### F-003 — security · medium — trust hasher walks Windows junctions as directories

**Where:** `src/edit_safety/trust.rs:469-476`

**What:** `collect_config_dir` uses `symlink_metadata` then `file_type.is_dir()` to recurse. Windows junctions are typically `is_dir && !is_symlink`. The crate already treats reparse points as unsafe for state directories (`paths.rs:6-37`, `state_directory_metadata_is_unsafe`) and does not apply that helper here.

**Why it matters:** `symforge trust project-config accept` can hash a tree that is not the `.symforge/config` the operator reviewed. The resulting trust record is the wrong predicate for later edit-safety checks.

**Fix:** Refuse `is_symlink` and `metadata_is_reparse_point` in the hasher, matching state-dir policy. Test a junction under `.symforge/config` pointing outside the project.

**Confidence:** likely (junction `file_type` is platform-specific; the missing reparse check is certain).

### F-004 — bug · medium — snapshot / team-artifact persist without fsync

**Where:** `src/live_index/persist.rs:1618`, `:1020`, `:1036`

**What:** `write_snapshot` is tmp `std::fs::write` then `rename`, no `sync_all`. `export_artifact` does the same for `index.bin.zst`, then in-place `std::fs::write` of `artifact.json`. `checkpoint_now(verify_after_write=true)` re-reads through the OS cache. Elsewhere the crate already fsyncs: `index_lifecycle/physical_root.rs:565`, `idempotency.rs:537`, `protocol/knowledge_curation.rs`, `protocol/edit.rs:251`.

**Why it matters:** A crash after “checkpoint succeeded” can lose the snapshot the operator asked to make durable. Rename without a data sync is not a persistence boundary, which this product otherwise treats as a reporting invariant.

**Fix:** write + `sync_all` + rename; `fsync` the parent dir on Unix. Same sequence for artifact metadata.

**Confidence:** certain (missing fsync). Durability impact is crash/power-loss, not a green-path functional bug.

### F-005 — bug · medium — unguarded byte-range slices on edit and diff paths

**Where:** `src/protocol/edit.rs:51`, `src/parsing/mod.rs:1135`

**What:** `apply_splice` does `content.len() - (end - start)` (wraps if `end < start`) then `&content[..start]` / `&content[end..]` with no bounds check. Some batch sites check (`edit.rs:2049`, `:3133`); `edit_within_symbol` (`:1351-1352`) does not. `extract_symbols_for_diff` indexes `&source[start as usize..end as usize]` after `catch_unwind` only around `parse_source`; a non-char-boundary tree-sitter range panics.

**Why it matters:** A stale or corrupt span turns an MCP edit/diff into a panic instead of a structured error. Not the happy path (index ranges usually match the buffer they were parsed from).

**Fix:** Return `Result`; reject `start > end`, `end > len`, and non-char boundaries on `&str`.

**Confidence:** likely.

### F-006 — security · medium — Windows daemon token file uses default ACLs

**Where:** `src/daemon.rs:600`

**What:** Comments pin confidentiality on a `0o600` token file (`:514-516`, `:561-563`). Unix creates with `.mode(0o600)` (`:569-595`). Windows is `std::fs::write` (`:598-606`).

**Why it matters:** On a default per-user `%LOCALAPPDATA%` directory this is usually owner-only by inheritance. If control-state is ever on a shared ACL, any local reader of the token drives the daemon. The Unix side already decided that was the barrier.

**Fix:** Set a user-only DACL (or write only under a directory whose ACL is asserted).

**Confidence:** likely.

### F-007 — bug · medium — trust-store writes are in-place `fs::write`

**Where:** `src/edit_safety/trust.rs:322`, `src/cli/trust.rs:165`

**What:** `write_store` / `write_cli_store` overwrite `trust.json` in place. Load of a torn file is fail-closed (`Untrusted`), so edits stop until re-accept. Idempotency already has tmp+sync+rename (`idempotency.rs:533-539`).

**Why it matters:** Crash during accept/revoke is user-visible (all structural edits refuse) even though it does not silently trust.

**Fix:** Atomic persist. Same helper as the replay ledger.

**Confidence:** certain.

### F-008 — bug · medium — `symforge init` tears editor MCP JSON on crash

**Where:** `src/cli/init.rs:470` (and `:789`, `:883`, `:980`, `:1024`, `:1105`, `:1274`, `:1318`, `:1378`, `:1400`)

**What:** Client config writers use in-place `std::fs::write`. `cli/harness_apply.rs` and `cli/operator_profile.rs` already tmp+`sync_all`+rename for the same class of file.

**Why it matters:** Killing init mid-write can leave Claude/Codex/Cursor settings unparseable — guaranteed user-visible failure of the editor, not just of SymForge.

**Fix:** Reuse the existing atomic write helper.

**Confidence:** certain.

### F-009 — documentation · low — `index_lifecycle` still claims production darkness

**Where:** `src/index_lifecycle/mod.rs:8`

**What:** The module docs say **nothing in production calls this module**. `protocol/edit.rs:212` and freshen (`tools.rs:1204`, `sidecar/handlers.rs:267`) call `activation::project_source_authority`; source writes go through the confined lease.

**Why it matters:** A reviewer who trusts the crate comment will skip the live mutation/trust surface. That is the reporting-invariant failure mode this directory exists to prevent.

**Fix:** Name the real call edges, or keep darkness language only for submodules that still have none.

**Confidence:** certain.

---

## Rejected after Phase 2 (not defects)

| ID | Claim | Why it died |
|---|---|---|
| R-001 | `constant_time_eq` length early-return leaks token length | Daemon hashes both sides to 32-byte SHA-256 first (`daemon.rs:717-724`). |
| R-002 | `file_from_workdir` still discloses demoted files | No production callers; protocol uses `admit_worktree_text`. |
| R-003 | `select_runtime_data_base` cwd fallback | Unused in production (tests/specs only). |
| R-004 | Catalog indexes symlink targets | Walker skips non-files; canonicalize-outside is `continue`. Leak is F-001’s read paths. |
| R-005 | Admin static assets skip Bearer | Documented P2-1; `/api/v1` and `/mcp` stay gated. |
| R-006 | Raw `Command::new` in daemon | Allowlisted `spawn_daemon_process` with `DETACHED_PROCESS\|CREATE_NO_WINDOW`. |
| R-007 | Hook marker `let _ = fs::write` | Fail-open by contract; repeats a stderr hint. |

---

## What was solid (not findings)

- `safe_repo_path` itself (lexical `..` + canonicalize `starts_with`) on the **non-freshen** `get_file_content` disk arm.
- Serve: Bearer + Origin + refuse-to-start on non-loopback without a key.
- Daemon: loopback refuse + hashed constant-time compare + Unix `0o600` token create.
- Secret detector fail-closed (`INDETERMINATE_RULE_ID`, scan-limit withhold).
- `is_sensitive_path` / discovery `is_forbidden_root`.
- `physical_root` cap-std no-follow writes + `sync_all`.
- `process_util` tripwire against new console-flashing spawns.
- Read gate owning the fetch (classify the buffer just read) for git objects and (when containment holds) disk.

---

## Fix plan

Each item is independently implementable. Do not batch F-001 with API changes to sidecar bind.

### P0 — F-001

1. Remove `Err(_) => Ok(join)` from both freshen helpers; propagate `safe_repo_path` errors as freshen no-ops / refusals (do not reindex).
2. In `admit_disk_read` / stable_read / watcher reindex: `symlink_metadata` (or no-follow open); if symlink or reparse point, skip/refuse, do not `fs::read`.
3. **Acceptance:** a test repo whose only “secret” is outside the root, with an in-repo file symlink (and, on Windows, a junction) to that file:
   - `get_file_content` on the link path does not return the secret and does not insert it into the live index.
   - untracked `search_text` with an anchored regex over the secret does not hit.
   - sidecar `/workflows/source-read` after freshen does not contain the secret.
   - Command: `cargo test --lib --bins --tests -- --test-threads=1` focused on the new tests (plus existing read_gate / tools path tests).

### P1

- **F-002:** sidecar bind uses the daemon loopback predicate. **Acceptance:** `SYMFORGE_SIDECAR_BIND=0.0.0.0` without opt-in fails spawn; with opt-in, warn. Unit next to daemon bind tests.
- **F-003:** hasher skips reparse points. **Acceptance:** Windows-only test with a junction under `.symforge/config`; trust hash does not include the target tree (or evaluation is Untrusted with a warning).
- **F-004:** `sync_all` before snapshot/artifact rename. **Acceptance:** helper-level test or a `File` wrapper spy; `checkpoint_now(verify_after_write=true)` still round-trips in existing persist tests.
- **F-005:** `apply_splice` / `extract_symbols_for_diff` return `Result`. **Acceptance:** unit tests for `end > len`, `end < start`, and a non-char-boundary `start` on a multibyte `&str` — all `Err`, no panic.
- **F-006:** Windows owner-only ACL on the token file. **Acceptance:** after `write_daemon_token_file_at`, the DACL has no Everyone/Users read ACE (Windows CI or a documented cfg-gated test).
- **F-007 / F-008:** atomic writes for trust.json and init client configs. **Acceptance:** existing harness_apply-style tests reused; crash-mid-write leaves the previous file intact (tempfile + replace the write helper with a failing spy if a full crash test is too heavy).

### P2

- **F-009:** rewrite the `index_lifecycle` darkness paragraph to list `project_source_authority` / confined write call edges. **Acceptance:** `rg "Nothing in production calls this module" src/index_lifecycle` is empty; T051 darkness test still matches whatever the campaign still requires.

---

## Notes for a fix pass

- Do not “fix” `select_runtime_data_base` unless someone wires it; it is dead in production.
- Do not add Bearer to loopback sidecar without a hook-compat plan; F-002 is the **non-loopback** refuse, not same-user loopback.
- After F-001, re-run `cargo test --no-default-features --features embed --lib -- --test-threads=1` if the no-follow helper is not `cfg(test)`-gated (embed unused-import trap, `CLAUDE.md`).
