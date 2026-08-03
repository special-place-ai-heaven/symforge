# HANDOVER — raw-read admission gate + detector precision — 2026-07-29

**Read this before touching anything.** It is the complete binding context for an
uncommitted, three-round body of work. Nothing here is optional background: every ruling
below was made explicitly by the owner and is not to be re-litigated.

---

## START HERE — you have no context; this is your orientation

You are picking up mid-campaign. The work is **real, large, uncommitted, and security-
sensitive**. Do not start by exploring the codebase — start by reading this file end to end,
then verify the tree matches §0's fingerprint.

**First five actions, in order:**

1. Read this file completely. §2 (the ruling corpus) is binding and is the part you cannot
   reconstruct by reading code.
2. Verify the worktree fingerprint in §0. If `diff_sha` differs, something changed since
   handover — **stop and report** rather than proceeding on a shifted base.
3. Confirm `git -C E:/project/symforge status` shows `feat/knowledge-llm-sift` clean. That is
   a *different* branch with someone else's unpushed work. Never write there.
4. Read §4 (open items) and §5 (the approved sequence). Do not skip to implementation — the
   next step is finishing verification, not writing code.
5. Ask the owner for the H1/H2 ruling if it has not been given. Two decisions are theirs.

### Memory stores — what actually has content for this work

| Store | Has anything on THIS work? | How to reach it |
|---|---|---|
| **agentmemory** | **YES — two entries, both auto-injected at session start** | `memory_recall` / `memory_smart_search`; scope `project: "symforge"` |
| **Obsidian vault** | **No** — nothing was written about this work | MCP only (`mcp__obsidian__*`), never read/edit vault files from disk |
| **remindb** | **No** — it indexes the vault, which has nothing here | `MemoryTree` → `MemorySearch` → `MemoryFetch` |

The two agentmemory entries are:

- `mem_ms61f6mb_292e2dd99f17` — state handoff: where the work lives, what landed, the
  remaining sequence, open items, follow-ups. Overlaps this file; this file is more complete
  and is authoritative where they differ.
- `mem_ms61ivye_35e5950d8b49` — the Terminal Commander usage lesson (§7 below), with the
  measured failures behind it.

A session-start hook injects curated `[symforge]` memories automatically — **read them, they
are past decisions, not suggestions.** Query `memory_recall` with symptom-shaped phrases
("raw read leaks demoted file", "admission gate uncommitted") for more. Obsidian and remindb
are worth a query only if you need *broader* project history than this campaign.

### Other artifacts on disk

- `specs/021-admission-coverage-honesty/` — the parked Feature 021 re-draft, plus three
  locked independent reviewer reports and a Codex consultation. Relevant background for the
  detector work; **021 itself stays parked** (see §6).
- Workflow scripts from this campaign live under
  `~/.claude/projects/E--project-symforge/<session>/workflows/scripts/`. Their transcripts
  are in the sibling `subagents/workflows/` directory if you need to audit a prior finding.
- `~/.claude.json.bak-20260729-140917` — config backup taken when `TC_SURFACE` was flipped to
  `full`. Keep until full-surface TC is confirmed working.

### Global rules that apply and are easy to miss

- **No secrets in chat, ever** — reference by name and location, never value. This campaign
  is *about* a secret detector, so you will handle credential-shaped fixtures constantly:
  describe shapes abstractly, never reproduce them.
- **Synthetic fixtures only**, assembled at runtime from non-secret fragments. Committing a
  credential-shaped literal is the exact defect Ruling 1 removed from this tree.
- **C: was below the 50 GB floor** at handover (~45 GB). Not blocking, but check before any
  large build, and see the hygiene rules on WSL `ext4.vhdx` growth.

---

## 0. Where the work is

| | |
|---|---|
| Worktree | `E:/project/symforge-rawread` — git worktree, branch `fix/raw-read-admission-gate` |
| Base | `origin/main` @ `21799e6` — **STALE**, 8.16.7 has since shipped |
| State | **UNCOMMITTED**: 18 files, +2127/−119, plus untracked `src/protocol/read_gate.rs` |
| Fingerprint | `diff_sha 59fb88d08572f666` · `read_gate.rs 9d2a39ba93cdb0e1` |
| Main tree | `E:/project/symforge` on `feat/knowledge-llm-sift` @ `9cbe6a9` — **DO NOT DISTURB**, carries 7 unpushed WS2 commits |

**Two checkouts, different branches.** Never read or write the main tree for this work. A
SymForge MCP index points at the *main* tree, so its answers are wrong for this worktree —
use Read/Grep against the worktree path.

**Do NOT resume workflow `wf_5b03af41-7b8`.** Its fix agent completed its work to disk but
never journaled a result, so a resume re-runs it from scratch against an already-fixed
tree and risks double-applying edits. Verify the diff directly instead.

**Do NOT `cargo clean`.** The work is uncommitted; reclamation is gated on commit/merge/push.
`target/` is ~8.3 GB and a cold rebuild costs ~20 minutes.

---

## 1. The defect this fixes

Raw-read lanes in `src/protocol/tools.rs` performed `std::fs::read` with only a
repo-containment guard (`edit::safe_repo_path`, traversal + `starts_with`) and **no
admission check**. A file demoted for security is *absent* from the Tier-1 map — `store.rs`
calls `live.remove_file(path)` when publishing a non-content-retaining disposition — so it
fell through the "not in index" fallback and was served raw.

| Lane | Site | Was |
|---|---|---|
| 1 | `get_file_content` `None` arm | **live breach** — full raw content of a `SensitiveContent`/`SensitivePath` file |
| 2 | `validate_file_syntax` | **live breach** — read + parsed, structure disclosed. Reads disk *unconditionally*; never consults the index |
| 3 | `tier2_reference_disclosure` | safe **by accident only** — filtered on `SkipReason::SizeThreshold` while SF-DOG-004 mislabels security demotions as `UnsupportedLanguage` |

`tools.rs` had **zero** references to `SensitivePath`/`SensitiveContent` in non-test code.

**The trap:** `capture_admission_tier_lookup_view` (`live_index/health_view.rs:275`) looks
like the right lookup and is not — it routes through `compatibility_admission_decision`,
whose `.reason` is the lossy SF-DOG-004 collapse. A gate built on it inherits the bug.
**The gate must read the typed `FileDisposition` off the manifest entry.**

---

## 2. THE RULING CORPUS — binding, do not re-open

### Gate rounds (R-series)

- **R1/E1 — the estimate branch is metadata-only and is NOT a gate site.** Tier-2 aggregate
  counts and safe identifiers are an explicit Feature 020 contract. Estimation must still
  SUCCEED for a security-demoted file. It preserves exactly the fields Feature 020 already
  authorizes — do not strip an existing documented aggregate merely because it derives from
  bytes. Hard limits: selectors must not influence estimates; no positions, excerpts, symbol
  names, or match metadata. Pinned by the mandatory pairing test (estimation succeeds while
  every content selector on the same file refuses). Repeated-query side-channel arguments
  are OUT OF SCOPE absent a demonstrated end-to-end content-recovery exploit.
- **R2 — manifest/current-bytes disagreement: MOST RESTRICTIVE WINS.** Deny when *either*
  the manifest records a security disposition *or* current path/bytes classify sensitive or
  indeterminate. A clean manifest cannot authorize changed bytes; clean current bytes cannot
  override a stale security disposition. Reindexing is the explicit recovery path.
- **R2a/D1 — SINGLE-READ OWNERSHIP.** The gate owns the read and returns the admitted
  `Vec<u8>`. No token, capability, or carrier type unless an equivalent already exists. Read
  once, classify *that exact buffer*, return it only on permit. Every gated caller consumes
  only that buffer. No classify-then-reopen — that reopen is the TOCTOU hole.
- **D2 — no global compile-time prevention of `fs::read`.** Structural single-read ownership
  plus behavioral tests is the standard. **No production instrumentation** (read counters,
  probes, telemetry) to detect hypothetical reopens; behavioral tests plus an explicit
  per-lane source trace are sufficient.
- **D3 — the gate is skipped ONLY for content actually served from the in-memory index.**
  This is about *where the bytes come from*, not what the manifest says. Any path that
  reopens disk is gated **even when the manifest says `Indexed`** — including
  `validate_file_syntax` and any refresh/re-read behaviour.
- **D4 — operation count is the whole performance deliverable.** No benchmarks, criterion,
  timing, or milliseconds. In-memory-served: no added work, gate not reached. Refusal by path
  rule or manifest disposition: **zero content reads**. Fallback needing current-byte
  inspection: exactly one read and one classification.
- **D5 — `capture_admission_tier_lookup_view` is rejected for this gate.** Typed disposition
  plus current-byte classification. Lane 3 eligibility must be independent of the lossy
  `SkipReason`.
- **D6 — verification is READ-ONLY.** No verifier may mutate the worktree — no stash, no
  revert experiment, no edits. RED evidence is established *before* implementation by a
  dedicated phase, never reconstructed afterwards while other reviewers read concurrently.
- **R8 — evidence honesty.** Prior RED evidence is recorded as **model-based, not
  git-verifiable** (nothing was committed, so no baseline blob exists). **Do not manufacture
  history.** The falsifiable proof is the gate-neutralization mutation check, run serially by
  the orchestrator, comparing `(exit_code, passed+failed)` — if `passed+failed` moves, the
  mutation perturbed the *measurement*, and the result is unverifiable, not a pass.

### Detector rounds

- **Ruling 1 — fixtures, not detector carve-outs.** Do NOT exempt `#[cfg(test)]`; do NOT
  accept `tools.rs`/`daemon.rs` remaining refused. **Preserve row S1.** The detector is
  right; the repository's own test code was wrong. Fix by constructing synthetic sensitive
  samples **at runtime** (concatenate non-secret fragments) or isolating them in fixtures
  *expected* to be demoted with that expectation asserted. **Test code receives the same
  detector policy as production code** — no region class, no path class, no test-only
  carve-out anywhere in the detector.
- **Ruling 2 — bounded multiline RHS.** Scan balanced multiline right-hand-side expressions
  within a fixed bound and require full consumption. Unbalanced or over-bound input remains
  SENSITIVE (fail closed).
- **Ruling 3 — embedded literals stay sensitive.** Credentials in URLs and connection strings
  remain SENSITIVE. **Placeholder carve-outs must not hide literal credentials elsewhere in
  the same string** — scope the carve-out to the matched run, not the enclosing string.
- **Ruling 4 — full-buffer encoding validation before Tier-1 publication.** Not complete
  until the *entire* byte buffer is encoding-validated before publication, across **every
  production mutation capable of publishing Tier-1 bytes** — not merely `update_file`
  callers. That includes direct inserts (`upsert_manifest_entry`, direct file-map mutation),
  constructors and load paths, and snapshot/recovery publication seams (`persist.rs` and what
  it feeds). **Preserve the indexed fast path — no read-time rescan.**
- **Ruling 5 — all three `DetectorFailure` variants get honest messaging.** No remaining
  misleading "reindex the repository" advice.
- **Ruling 6 — FU-2 deferred.** Do NOT decouple `SECRET_SCAN_MAX_BYTES` from
  `METADATA_ONLY_CODE_BYTES` without first establishing the new bound and its cost. Continue
  fail-closed for oversized code.
- **Ruling 7 — FU-1 out of scope.** Do NOT touch `src/git.rs`,
  `matching_untracked_paths_for_search_text`, `diff_symbols_result_view`, or `detect_impact`.

### Process rulings

- **No merge** until final gates, fixture-demotion assertions, and the per-lane source trace
  all pass — **and** FU-1 is stacked and green.
- **Commit shape:** split into two commits **only** if both are mechanically producible,
  independently green, and require **no hunk surgery or fixture-history rewriting**.
  Otherwise **one atomic commit**. Fixture evolution alone does not invalidate a split — each
  commit need only prove its own contract at its own point in history. Note a split doubles
  the rebase conflict surface.
- **Before committing:** account for every changed file against a ruling; remove any hunk
  lacking a ruling *and* a guarding assertion. Plausible causality is insufficient.
- **Reporting discipline:** bounded results only — counts, test names, the assertion that
  moved. No unrestricted log tails. Never reproduce fixture contents, matched text, or any
  secret-shaped value. Describe detector shapes abstractly with `file:line`.
- **Green baselines are typed.** GREEN-CONTROL = proves we did not over-refuse (admitted
  non-indexed read; estimate success). GREEN-GUARD = a safety property that must now hold for
  a NEW reason (lane 3's security exclusion, which is currently green *by accident* via the
  SF-DOG-004 mislabel and must survive the reason-code fix).

---

## 3. What landed

**The gate — `read_gate::admit_disk_read`** (`src/protocol/read_gate.rs`, new). Three steps,
most-restrictive-wins: (a) `sensitive_path_rule` → deny, no read; (b) typed
`capture_file_disposition` → security variants deny, no read; (c) the **one** `fs::read`,
then `classify_stable_content` on **that exact buffer** → deny on sensitive; else return the
buffer. Three production call sites, each consuming only the returned buffer. Estimate branch
untouched (bucket M).

**Detector precision** — bounded balanced multiline walk, embedded-literal tightening,
code-language-gated exemptions, all rule-id-scoped so the four sibling rules are unaffected.

**Ruling 1 fixture rewrite** — 14 sites across `src/` and `tests/` now assemble
credential-shaped values at runtime. **Proof this was not a weakening:** the *same* detector
finds **0 files / 0 findings** over current `src/`+`tests/`, and **12 files / 18 findings**
over the HEAD blobs of those same trees. The shapes left the tree; the rule did not move.

**Blocker fixes (verified on disk, compile-verified only):**
- **B1** — apostrophe removed from the generic unbounded quote-skip. Two mis-paired
  apostrophes (an English contraction in a comment, or a pair of Rust lifetime sigils)
  bracketing a credential made the walk skip *over* it and grant the exemption. Fail-open,
  introduced by the round-2 fix. Controls measured: 0 apostrophes → SENSITIVE, 1 → SENSITIVE,
  2 → **CLEAN**.
- **H3** — `reindex_after_write` (`src/protocol/edit.rs`) now calls
  `classify_stable_content(relative_path, targets, &on_disk)` before `update_file`, at the
  shared boundary so all ten call sites inherit it.

`cargo check --all-targets` **green** on this tree (2026-07-29, via TC).

---

## 4. OPEN — needs an owner ruling before proceeding

Both are pre-existing, but "pre-existing" does not permit deferral because a ruling or the G8
oracle row relies on the behaviour, and **G8 cannot bank on a false strictness claim**.
The owner requires the **exact reproducer** and the **violated contract quoted with
`file:line`** before ruling. That evidence package was being gathered when the session ended.

- **H1** — `is_placeholder`'s `${…}`/`{{…}}` branches (`knowledge/mod.rs` ~:179) test
  `starts_with("${") && ends_with('}')` — ends only, whole capture ignored — and run *before*
  the `is_code_language` gate and before `capture_is_single_interpolation`. A hardcoded
  literal **bracketed by placeholders** is admitted on **every** path class, config included.
  Reported CLEAN: placeholder-both-ends with literal payload between (`.rs`); shell
  default-expansion `${VAR:-<payload>}` on `.rs`/`.env`/`.yaml`/`.properties`; mustache
  default-filter (`.yaml`). Control with `{a}` instead of `${a}` → SENSITIVE, proving
  S14/S15 pin only `capture_is_single_interpolation`. Byte-identical to HEAD — **but this
  round re-based G8 onto this branch, making it load-bearing.** Candidate fix: give the two
  branches the same whole-capture discipline `capture_is_single_interpolation` has.
- **H2** — the `b'\n' if depth == 0 => return false` arm (`knowledge/mod.rs` ~:352) means
  formatter continuations at depth 0 are never entered. Reported CLEAN: method-chain (`.rs`),
  `??` fallback (`.ts`), `+` concat (`.java`), backslash continuation (`.py`); single-line
  control SENSITIVE. Not a regression, and Ruling 2's *named* row C10 is genuinely fixed —
  but Ruling 2's rationale ("rustfmt and black PRODUCE that shape … credentials are long")
  applies verbatim, and no row states it while the docstring frames the newline rule as an
  invariant.

Also open from the round-3 assessment: **M1** two unpinned withdrawal arms · **M2** the
`repository_source_is_clean_under_its_own_detector` tripwire walks only `["src","tests"]`
while 4 files / 5 findings sit outside it · **M3** one newly-demoted file
(`research/full-surface-benchmark/claude_task_runner.py:1653`) unstated.

---

## 5. THE APPROVED SEQUENCE — do these in order

1. **Finish verifying B1 and H3.** B1 needs the two-apostrophe regression plus zero- and
   one-apostrophe controls, **all SENSITIVE**. Bounded char-literal handling only if a
   required clean oracle proves it necessary — and name the row that forced it. H3 needs a
   test proving post-edit sensitive content cannot become Tier-1, failing without the fix.
   Also confirm removing the apostrophe skip did not open a new hole: char literals like
   `'('` / `'{'` now advance one byte and count as delimiters — verify that yields
   *unbalanced → SENSITIVE* (fail-closed), not balanced.
2. **Deliver the H1/H2 evidence package** and get the owner's ruling.
3. **Account for every changed file** against a ruling; remove any hunk without a
   ruling + guarding assertion. Scrutinise `live_index/knowledge_bridge.rs`,
   `live_index/local_ref_scout.rs`, `live_index/store.rs`.
4. **Decide commit shape**, then commit.
5. **Rebase onto current `origin/main`** (base is stale; 8.16.7 shipped).
6. **On the rebased tree only**, run the three serial proofs:
   gate-neutralization mutation check on `admit_disk_read` (tuple comparison) → isolated
   rerun of `tests/hook_subprocess_integration.rs:334` (known ~6.87 s against a 5 s deadline)
   → **one** clean full suite.
7. **Hold merge** until FU-1 is stacked and green.

---

## 6. Follow-ups

- **FU-1 — blocks release AND blocks resuming Feature 021.** Three ungated disclosure lanes
  sharing `src/git.rs:290 file_from_workdir`: `search_text` untracked sweep (arbitrary
  anchored regex = character recovery), `diff_symbols` uncommitted mode, `detect_impact`
  `since="WORKTREE"` (both leak symbol names/signatures). One gate at that function's callers
  closes all three. It should stack on this branch to reuse `admit_disk_read`.
- **FU-2 — deferred, stays fail-closed.** `SECRET_SCAN_MAX_BYTES` is *defined as*
  `METADATA_ONLY_CODE_BYTES` (`knowledge/mod.rs:32`) — the size-demotion threshold and the
  unscannable threshold are the same 4 MiB constant, so every size-demoted **code** file
  falls out of lane 3's sweep, the exact population it serves. Latent here (0 of 974 tracked
  files exceed 4 MiB).
- **Feature 021 re-draft — parked.** Its `specs/021-admission-coverage-honesty/` holds three
  locked reviewer reports and a Codex consultation. Note the detector precision landing here
  changes 021's T066 premise.

---

## 7. Tooling rules learned the expensive way

- **Every build/test goes through Terminal Commander `run_and_watch`** (argv + cwd +
  `wait_ms`, resume with `wait` + `bucket_id` + `cursor`). `TC_SURFACE=full` is now set.
  Five global rules active: `cargo-lock-block`, `cargo-compile-error`, `cargo-test-failed`,
  `cargo-test-result`, `cargo-panic`.
- **Never `sleep`-poll a background job.** A fix agent burned **207.5k tokens / 88 tool
  calls** looping `sleep 580; tail -5` against a dead job. Its deliverable was already on
  disk. A staleness watchdog cannot see this — the sleep returns on schedule so the
  transcript looks fresh. Detect by *repetition*, not idleness.
- **Never `| tail -N` a test run.** It discarded the `3050 passed` line and forced a full
  ~19-minute re-run. And `$?` after a pipeline measures the *last* command — it reported
  success for an unverified run.
- **TC rule regexes:** Rust's `regex` crate expands `\w`/`\d` to full Unicode classes;
  several in one pattern exceed the 64 KiB compile limit. Use `[a-zA-Z]`, `[0-9]`.
- A full `cargo test --all-targets -- --test-threads=1` here is **~19 minutes** / 3819 tests
  across 114 binaries. Budget for exactly one, after the rebase.
