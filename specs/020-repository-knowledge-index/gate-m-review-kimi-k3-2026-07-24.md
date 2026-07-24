# Gate M — Kimi K3 Adversarial Review (commit 315c2b9)

**Reviewer**: Kimi K3 (cross-model, independent)
**Date**: 2026-07-24
**Branch**: `feat/repository-knowledge-index` (HEAD `315c2b9`, NOT pushed)
**Why**: This is the FINAL gate of Feature 020 before merge-to-main, which triggers a
`release-please` public release. The prior Codex reviewer's sandbox denied process
creation and could not inspect the diff — so this diff currently has **zero independent
eyes**. Be skeptical; hunt for correctness regressions.

> **INSTRUCTION TO REVIEWER (Kimi K3):** Write each finding into the "Findings" section
> below **the moment you confirm it** — do NOT buffer everything for a final flush. Append
> incrementally so that if the session ends early, the partial report survives. When done,
> fill in the single-line VERDICT at the very bottom. If you find nothing real, say so
> plainly — do not invent findings to look thorough.

---

## Target

Commit `315c2b9` — `fix(gate-m): close M-012 embed gate + resolve Cursor AAP review findings`.
Inspect with `git show 315c2b9`, then read surrounding code as needed.

**7 code files in the diff** (+2 doc files, ignore those):

| File | What changed |
|---|---|
| `src/live_index/persist.rs` | `background_verify_with_hook`: watcher-dependent reparse path gated behind `#[cfg(feature="server")]`; 2 watcher-using tests gated |
| `src/live_index/local_ref_scout.rs` | 4 tests using `crate::protocol` gated behind `#[cfg(feature="server")]` |
| `src/protocol/edit_plan.rs` | AAP-001: exact path==target → file; path-shaped suffix → sorted-first + ambiguity note; bare identifier → symbol cascade |
| `src/sidecar/handlers.rs` | AAP-002: `LanguageId::from_path` (not `from_extension`) at both sites |
| `src/protocol/format/tests.rs` | M-002: dropped a vacuous `assert_ne`; added `UserLocal` `ProjectLocalUnavailable` → normal |
| `tests/edit_plan_literal_path_precedence.rs` | +2 tests; assert-not-`continue` |
| `tests/get_file_content_concurrency.rs` | assert content substring, not just non-hang |

## Context: two feature modes

- **`server`** (default): has a filesystem watcher + protocol layer.
- **`embed`** (`--no-default-features --features embed`): **no watcher**.

The embed `--lib` build was RED (13 compile errors). This commit makes it GREEN by gating
watcher/protocol-dependent code behind `#[cfg(feature="server")]`.

## Verification already performed this session (real exit codes, repo-pinned cargo, -j1 --test-threads=1)

- `cargo test --no-default-features --features embed --lib` → **1282 passed / 0 failed**
- `cargo test --lib --features server` → **3012 passed / 0 failed**
- `cargo fmt --all -- --check` → clean
- (Earlier, HEAD 40d6250) `clippy --all-targets --features server -- -D warnings` → clean;
  full all-targets suite 113 binaries / 0 failed.

Tests pass — but "green" is not "correct". The risks below are semantic and may not be
covered by tests.

---

## Specific risks to verify (with file:line evidence)

1. **PARITY / silent no-op** (`persist.rs` `background_verify_with_hook`, `local_ref_scout.rs`):
   Is the `#[cfg(feature="server")]` gating a **principled** skip — under embed there is
   genuinely no watcher, so the gated reparse path is unreachable/irrelevant — and NOT a
   silent correctness hole where embed now no-ops work it should still do? Is the **server**
   build behaviorally **byte-identical** to before (the gate only ADDS an embed path, never
   alters server flow)? Distinguish gated *tests* (fine) from gated *production logic*
   (scrutinize). Any production embed path that now silently does nothing?

2. **DETERMINISM** (`edit_plan.rs`, AAP-001): the "sorted-first" tie-break for a path-shaped
   suffix with multiple matches — is it a **total, stable** order (not `HashMap` iteration
   order, not platform-dependent)? Could two runs on identical input pick different winners?

3. **`from_path` PARITY** (`sidecar/handlers.rs`, AAP-002): `from_extension` → `from_path` at
   both sites. Does `from_path` cover **every** case `from_extension` did? Any file whose
   language now resolves differently, or returns `None` where it used to resolve?

4. **TEST INTEGRITY** (`format/tests.rs`, the two `tests/` files): did any assertion get
   **weakened** or made vacuous? Changes claim to STRENGTHEN. Confirm each new assertion
   actually fails if the underlying logic breaks.

5. **FEATURE-FLAG MATRIX**: any combination (embed alone, server alone, both, neither) that
   fails to compile or misbehaves; any new `panic!`/`unwrap`/`expect` path.

---

## Findings

<!-- Kimi K3: append each confirmed finding here as you go. One block per finding. -->

<!--
### [SEVERITY: BLOCKER|HIGH|MEDIUM|LOW] short title
- **File:line**:
- **Failure scenario** (concrete input/state → wrong output/crash):
- **Recommendation**:
-->

### [SEVERITY: LOW] Commit-message verification scope omits the two integration binaries it modifies
- **File:line**: commit `315c2b9` message ("Verified ... embed --lib 1282/0, server --lib 3012/0"); `tests/edit_plan_literal_path_precedence.rs`, `tests/get_file_content_concurrency.rs`
- **Failure scenario**: Not a code defect. The commit rewrites assertions in two `tests/` integration binaries, but the recorded verification only ran `--lib` suites, which do not compile or run `tests/` targets; the cited full all-targets run was at ancestor `40d6250`, before these test changes existed. A release cut from this evidence would have shipped unverified test changes.
- **Recommendation**: I closed the gap during this review — at `315c2b9`, `cargo test --features server --test edit_plan_literal_path_precedence` → 3/3 ok and `--test get_file_content_concurrency` → 2/2 ok. No action needed for the code; note for release process: all-targets evidence should be produced at the release commit, not inherited from an ancestor.

_(none recorded yet)_

### [SEVERITY: HIGH] Embed `background_verify` reports freshness `Current` while leaving stat-detected changed/new files stale
- **File:line**: `src/live_index/persist.rs:2150-2176` (gated reparse), `:2208` (`mark_snapshot_verify_completed_at_fence(commit_fence, spot_mismatches)`)
- **Failure scenario** (concrete input/state → wrong output/crash): An embedder (AAP) restores a snapshot via `LiveIndex::load` and calls the documented post-restore step `live_index::persist::background_verify` (pub, reachable via the `symforge::embed::live_index` re-export in `src/embed.rs`). Files changed on disk since the snapshot are detected by the stat check (`stat_result.changed`, counted in the log) but, under `embed`, the `#[cfg(feature = "server")]` gate skips the reparse AND the changed/new paths are never passed to `mark_snapshot_verify_completed_at_fence` — only the 10% `spot_mismatches` are. So ~90% of genuinely-stale files escape the mismatch report, `store.rs:2789` freshness resolves to `FreshnessStatus::Current`, and the embedder serves stale content labeled fresh. The new comment at `:2189-2190` ("embed reports detected mismatches") is only true for spot-check mismatches; stat-detected changes are silently dropped. Under `server` this is correct (changed files are reparsed, hence not mismatches); under `embed` it is a silent correctness no-op, not a principled skip of irrelevant work.
- **Recommendation**: Under `embed`, either (a) include `stat_result.changed` + `new_files` in the `mismatched_paths` passed to `mark_snapshot_verify_completed_at_fence` so freshness degrades honestly, or (b) reparse through a watcher-free single-file path (cold-load already parses files without the watcher), or (c) gate `background_verify` itself `#[cfg(feature = "server")]` and document that embed must cold-index. Option (a) is the smallest honest fix.

---

## VERDICT

<!-- Kimi K3: replace this line with exactly one of: SHIP  |  FIX-FIRST (n blockers) -->

FIX-FIRST (1 blocker) — the embed `background_verify` freshness mislabel (HIGH, finding 1) is a ~5-line fix (pass stat-detected changed/new paths into `mark_snapshot_verify_completed_at_fence` under `embed`); everything else is ship-clean: server flow byte-identical, AAP-001 sort-based tie-break is a total deterministic order, `from_path` parity holds at both `.is_some()` call sites, no test was weakened (m002's dropped `assert_ne` was vacuous against an exhaustive match; both integration binaries re-run green at this commit: 3/3 and 2/2), and the full feature matrix (neither / embed / server / both + embed clippy `-D warnings`) compiles warning-free.

---

## Resolution (2026-07-24)

**Blocker FIXED.** `src/live_index/persist.rs::background_verify_with_hook` now, under
`#[cfg(not(feature = "server"))]`, folds stat-detected `changed`+`new_files` into the mismatch
set (sorted+deduped) before `mark_snapshot_verify_completed_at_fence` — so an embed snapshot with
unreconciled on-disk changes resolves to `Degraded` (`SnapshotVerificationFailed`) instead of a
false `Current`. Server path byte-identical (block compiled out under `server`).

**Regression guard added.** `test_background_verify_embed_folds_stat_changed_into_mismatches`
(embed-only): a stat-changed / spot-clean file must be folded into the mismatch report and degrade
freshness. Proven a real guard — FAILS ("got []") with the fix disabled, PASSES with it enabled.

**Verified** (repo-pinned cargo, `-j1 --test-threads=1`): embed `--lib` 1283/0, server `--lib`
3012/0, integration `edit_plan_literal_path_precedence` 3/3 + `get_file_content_concurrency` 2/2,
clippy `--all-targets --features server` clean, clippy `--no-default-features --features embed
--lib` clean, fmt clean.

**Known pre-existing limitation (out of scope, not a regression):** `clippy
--no-default-features --features embed --all-targets` is RED because ~65 `tests/*.rs` integration
binaries unconditionally reference server-only modules (`sidecar`/`watcher`/`daemon`/`protocol`);
reproduces with this change stashed. Embed is exercised via `--lib` (the defined M-012 gate). A
file-level `#![cfg(feature = "server")]` guard on those test binaries would close it — tracked,
not done here.
