# 013 Predictor Calibration — Session Handoff (resume here)

**Repo:** `E:/project/symforge`  ·  **Date:** 2026-06-23  ·  **Status:** complete calibration work done + verified; recovering it onto a fresh PR (a merge is in progress, resolved, not yet committed).

---

## TL;DR — the ONE next action

You are on branch **`feat/013-calibration-complete`** with a **git merge IN PROGRESS** (merging `origin/main` = 8.6.0 + concurrent 012 work into the complete calibration tip). The single conflict (`src/protocol/tools.rs`) is **already resolved** and the tree **compiles** (`cargo check --all-targets` = rc 0). What's left to finish the merge:

```bash
cd /e/project/symforge          # (git bash) or E:\project\symforge
cargo fmt                        # FIX the pending fmt diff (tools.rs ~9967)
cargo fmt --check                # must be clean
# Re-run the full gate to confirm the MERGED tree is correct (both calibration + 012 intact):
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1            # expect all 0 failed
cargo test --test stel_calibration_tuning --test surface_honesty -- --test-threads=1
cargo build --no-default-features --features embed --lib
# If green, COMPLETE the merge:
git add src/protocol/tools.rs tests/surface_honesty.rs   # the resolution + the fmt fix
git commit --no-edit                                     # completes the merge commit
```
Then push + open a NEW PR (see "Release plan" below). **Do NOT merge to main without the user's explicit "go".**

Windows cargo 1.96.0; build cache per repo `.cargo/config.toml` (`target/` on E:). Run `cargo clean` after heavy local gates. If a stale `.cargo-lock` deadlocks, `pkill -f "cargo|rustc"` then retry. Never run two cargo builds on the same target at once (deadlocks the lock).

---

## Big picture: what 013 is, and why this recovery exists

**013 = the STEL predictor actually calibrates** (the user's friction #4: "the predictor never calibrates"). It is the symforge MCP's economics predictor learning to correct itself from observed data.

**What shipped vs what's pending:**
- **`8.6.0` is RELEASED but it is the PARTIAL feature** — PR #347 was merged to `main` at commit `e1c9859` (US1 only) by `special-place-administrator` (concurrent/admin, NOT us) and release-please cut 8.6.0 from it. So 8.6.0 has **US1 durable ledger persistence** but the calibration still reads **`deferred`** (no auto-tune). The user explicitly did **not** want a partial release; this is the thing to correct.
- **The COMPLETE calibration work is on the branch, locally green, NOT released.** It must reach `main` via a fresh PR → release-please → **8.7.0**, which the user will install + dogfood.

**The complete work (commits on `a47bcb0`, all AFTER `e1c9859`):**
- `2010cfe` — US2 auto-tune + **D3-ROOT**: extracted a protocol-free `src/stel_core/` (types + ledger_store + calibration + 4 consts) gated `any(server,embed)` so the durable store reaches the embed facade (FR-001 embed durability DELIVERED). Server `stel` re-exports from it.
- `6eeaf96` — **D8 fix**: the auto-tune now corrects the predictor's REAL output (a single `response_correction_factor`) validated against the REAL `|predicted·f − actual|` residual — NOT the static floor the live byte-grounded path bypasses. Schema/invoke/manual untouched. (D8 was: it validated the wrong quantity.)
- `8963193` — **D15 fix**: record the RAW (pre-factor) prediction in the ledger so re-tunes learn the ABSOLUTE factor (not a delta) and converge. (D15 was: re-tunes under-corrected.)
- `2cacda1` — **D1-ROOT**: one `paths::symforge_db_path(root, bare_name)` helper; fixed a SECOND live doubled-path bug (api-keys, shipping in 8.5/8.6) + migrated all db stores.
- `def21a3` — **D2-ROOT**: the proxy `status` overlay now covers ALL 4 proxy-owned lines (incl. the calibration section) so calibration is VISIBLE on the daemon-backed default (was a permanent worker-blind `deferred`).
- `fac5db5` — **D5**: evidence-based durability decision (lossy only under >5s contention; median-over-24 robust; WARN-observable) replacing a "best-effort" hand-wave.
- `a47bcb0` — the findings ledger.
- (`5c8e513` = an empty "re-trigger CI" nudge commit on the OLD branch `013-stel-predictor-calibration-spec`; **ignore/drop it** — `feat/013-calibration-complete` was branched from `a47bcb0` precisely to exclude it.)

**Findings ledger:** `docs/reviews/013-findings-ledger.md` — every defect (D1–D16) named, fixed or evidence-decided. Read it for the full root-cause history.

---

## The merge in progress (exact state)

- Branch `feat/013-calibration-complete` (from `a47bcb0`). Merging `origin/main`.
- `origin/main` is `8.6.0` and contains: the partial 013 (e1c9859, via #347) + release-please 8.6.0 + the concurrent **012 harness-agnostic-mcp** work (merged via #351).
- **One conflict, resolved:** `src/protocol/tools.rs` in `status_stel_tool`. The 012 work added a `project_root: Option<String>` PARAMETER to `StelStatusContext::from_server` (in `src/stel/status.rs`) + a `let project_root = self.capture_repo_root()...` computation. Our calibration work needs `let mut ctx` (line ~9286 reassigns `ctx` with the durable verdict, T033/FR-009). **Resolution kept BOTH:** 012's `project_root` block + our `let mut ctx`.
- **Cascading fix:** because `from_server` gained a param, our other call sites needed `project_root`/`None` inserted: `tools.rs` proxy-overlay call (~9317, `None` — proxy omits the project line), `tools.rs` test (~9971, `None`), `tests/surface_honesty.rs` calibration-verdict test (~404, `None`). All done; `cargo check --all-targets` = rc 0.
- The merge also staged main's files (8.6.0 release files, `Cargo.toml/lock`, `CHANGELOG`, `.github/*`, **main's `CLAUDE.md`**, the 012 `docs/diagrams/*`). **That is expected** — a merge incorporates main's state. (The "don't touch CLAUDE.md" rule was about US editing it, not the merge bringing main's version.)
- **NOT yet committed.** Pending: `cargo fmt` (a diff at tools.rs ~9967), confirm full gate green, then `git commit --no-edit`.

---

## Release plan (after the merge is committed + green)

1. `git push -u origin feat/013-calibration-complete`.
2. Open a NEW PR (the old #347 is MERGED/closed): `gh pr create --base main --head feat/013-calibration-complete --title "feat(stel): 013 predictor calibration (complete) — US2 auto-tune + systemic fixes" --body "..."` (body: completes 013 on top of the US1 partial in 8.6.0; the working auto-tune; D1/D2/D5/D3 fixes; one tools.rs conflict reconciled keeping both calibration + 012). End body with the Generated-with-Claude-Code line.
3. **Watch the NEW PR's CI — and verify it is on the ACTUAL head commit, not a stale run.** (Last session got fooled: a "green" CI was for `e1c9859`, the pre-calibration state. Check `gh run list --branch feat/013-calibration-complete --json headSha` and confirm the green run's `headSha` == your pushed HEAD. GitHub PR-head metadata can lag/desync; if no run appears for your head, a no-op commit or close/reopen re-triggers it.)
4. **On the user's explicit "go" + real green CI → merge → release-please cuts 8.7.0 → user installs + validates.** Pause for that "go"; do not merge unilaterally.

---

## Working agreements with this user (carry these — they matter)

- **No "best effort" / "honest gap" framing for defects.** A bug/flaw/missing feature is a DEFECT: name it, log it to `docs/reviews/013-findings-ledger.md` on the spot, fix it ASAP, and decide immediately if it's a SYMPTOM of a systemic root → if so, investigate properly + attack the ROOT, not the instance. ("Plugging ever-growing holes won't save the ship.") This discipline already found a 2nd live db-path bug (D7) and the re-tune bug (D15) that green gates hid.
- **No deferrals.** The user wants deferred items properly implemented, not shipped partial. (That's the whole reason for this recovery.)
- **Verify before claiming done — independently.** Re-run the gate yourself; do not trust an agent's green paste or a stale CI. Adversarial code-review (the `code-reviewer` agent) on the calibration math caught D8 and D15 — keep using it for load-bearing changes.
- **A live concurrent session works in this repo** (the 012 worktree, under `special-place-administrator`). It merged #347 + main into 012. Expect main to move; reconcile rather than fight. Don't touch `CLAUDE.md` / the untracked `SELF_UPDATE_PROCEDURE.md` directly; stage only YOUR files (never `git add -A`).
- **Gate (the symforge CI mirror):** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, plus embed: `cargo build/clippy/test --no-default-features --features embed --lib`. `embed-musl` (x86_64-unknown-linux-musl) is CI-ONLY (no musl linker on this Windows box) — never fake it.
- **Ultracode is ON this session** (xhigh + workflows). The user said "continue" repeatedly — they want forward motion to a complete, released feature.

---

## Quick orientation commands for the new session

```bash
cd /e/project/symforge
git status                                  # see the in-progress merge + staged files
cat docs/reviews/013-findings-ledger.md     # full defect ledger (D1-D16, all closed)
git log --oneline e1c9859..a47bcb0          # the complete calibration commits to release
gh pr view 347 --json state,mergeCommit     # confirms #347 already MERGED at the partial
git show origin/main:Cargo.toml | grep version   # main is 8.6.0
```

The calibration math lives in `src/stel_core/calibration.rs` (derive/validate/verdict) + `src/stel/controller.rs` (`estimate_economics_tuned`, `active_tuning_in_force`). Surfacing in `src/stel/status.rs` + `src/protocol/tools.rs` (the proxy overlays). Persistence in `src/stel_core/ledger_store.rs`.
