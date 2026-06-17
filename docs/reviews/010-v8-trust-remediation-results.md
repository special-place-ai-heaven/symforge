# 010 v8 Trust Remediation — Results (honest)

**Branch:** `feat/010-v8-trust-remediation` (6 phase commits on main@3df5210 / 8.0.0)
**Status:** all 6 user stories implemented, reviewed, gate-green, committed. **Awaiting
human approval to merge/push — nothing pushed.**
**Keystone (SC-008):** an LLM that trusts SymForge's self-reported numbers and status is
no longer misled — every audited surface is true or explicitly labeled.

## What shipped (phase → commit → proof)

| Phase | Story | Commit | Real bug? | Proof |
|-------|-------|--------|-----------|-------|
| A | US1 honest labels (relabel, zero-behavior) | `6c0fa14` | — | `surface_honesty` 7/7; golden replay unchanged |
| B | US2 status truth | `4e6629d` | **yes (TR-01)** | `status_index_matches_daemon_proxy_after_symforge_serve` (spawns real daemon; front-end index stays empty) |
| C | US3 if_match guarded apply | `4661f6c` | **yes (TR-06)** | `symforge_edit_concurrent_same_file_apply_never_clobbers` (200-round real two-thread race) + injected-interleave T022 |
| D | US4 recoverable cold start | `d1b49ad` | — | `compact_surface_index_not_loaded_message_never_mentions_blocked_tools`; workspace-root override tests |
| E | US5 economics grounding | `3ffc6a5` | — | predictions-vary-by-size + bypass/mandatory_degrade reachable (e2e over real corpora) |
| F | US6 capability matrix + honesty gate | `dd12f25` | — | `honesty_gate` 7/7; real matrix+register PASS the gate |

## The two real bugs (closed)

- **TR-01 status lied about the index.** In the daemon-proxy topology the front-end
  `self.index` is empty; `status_stel_tool` read it directly and the daemon had no
  `status` arm, so a working index reported empty. Fixed: `status` proxies to the daemon's
  served index (+ daemon `status` arm); regression spawns a real daemon and asserts the
  front-end index stays empty while status reports the daemon's counts.
- **TR-06 if_match guard never enforced + didn't serialize.** The guard was plumbed at L0
  and checked in pre-flight but dropped before the write; worse, the first fix didn't
  serialize concurrent in-process writers (`symforge_edit` is governor-Light). Fixed:
  `if_match` threaded to the write; re-verified against on-disk bytes under a process-global
  **per-path mutex** spanning re-read→rename; 200-round two-thread test proves never-two-
  commits / no torn file (fails without the lock).

## Honesty sweep (true or labeled)

- envelope: `session_net_vs_manual` → `session_tokens_served` (gross counter, named for what
  it is); figures `est.`/`(heuristic)`; reject → `n/a (rejected)`.
- status: `l*: active` → honest static `wired` / `l4_ledger: in_memory`; `calibration:
  pending` → `deferred` (the `CalibrationState` seam kept + documented as inert); stale
  `ledger_persistence` dropped from deferred; durable ledger `Disabled{reason}` vs
  `Unavailable` distinct (N-3).
- economics: grounded in real bytes for the single-file read family (predictions now vary;
  bypass + mandatory_degrade reachable); trace/search honestly stay on the labeled floor;
  every figure still labeled estimate, never measured.
- docs: A-005 single-sourced (VALIDATED, draft-shapes caveat); A-009 → PARTIAL; A-028 → OPEN;
  `expected_equiv` write-only dead data removed. README/AGENTS/CLAUDE: compact-3 default +
  the corrected **35-tool** (was a stale "32") full opt-out.
- capability matrix published; A-017 (surface premise) and A-011 (predictor) stay **OPEN —
  bet under test**, never proven. A `cargo test` honesty gate fails any future `Implemented`
  claim on an OPEN assumption (relabel ≠ validate, enforced).

## Verification

- Full per-phase gate green after **every** phase (not just at the end): `cargo fmt --check`,
  `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, and
  `cargo check --no-default-features --features embed` (Constitution VI). Final cumulative
  tree green at `dd12f25` (3000+ tests).
- Each real-bug/behavior phase passed an independent **code-reviewer**; Phase C also passed an
  adversarial **security-reviewer** that *found* the missing-serialization BLOCKER (fixed
  before commit). Every named regression was proven to FAIL against pre-fix code.
- Built binary confirmed: `symforge 8.0.0` (`target/release/symforge.exe`).

## Honest gaps / residuals (named, not hidden)

1. **Human-harness live dogfood is yours to run.** The production daemon-proxy topology is
   proven by executable end-to-end tests (real daemon + HTTP), not code-reading — but a
   click-through on a live Claude Desktop harness pointed at the built 8.0.0 binary was NOT
   performed: the dev harness is intentionally downgraded to 7.27.0. To dogfood as a user:
   point a compact MCP client at `target/release/symforge.exe` from a project root, then
   confirm `status` reports the served index, a `symforge` orient query succeeds, `status`
   full counts match the served query, and `symforge_edit` preview is honest.
2. **TR-03 live cold start** (Claude Desktop launching with CWD=System32) can't be reproduced
   in `cargo`; the fix (workspace-root env + wrapper CWD) is unit-proven, live-Desktop
   pending (same dogfood as #1).
3. **if_match residual:** the per-path mutex serializes SymForge's own in-process writers; a
   truly external OS editor is outside it. Windows `MoveFileExW` can transiently deny a
   back-to-back same-target rename — surfaces as a benign "wrote nothing" error (safe to
   retry), never a clobber.
4. **Economics grounding is partial by design:** grounded for single-file reads; trace/search
   keep the labeled heuristic floor (a single file's size would under-state a grep-across-
   many-files baseline). Honestly labeled; not a hidden cliff.
5. **Honesty gate scope:** enforces the structured records (matrix ↔ register ↔ artifact),
   not an NLP scan of arbitrary prose/code — documented in the gate.

## Next (requires you)

- Review the branch. On approval: merge to main (FF push `git push origin HEAD:main`, never
  force) — release-please will cut the version. **No push/merge performed without your word.**
- Optional: re-point a harness at the built binary for the #1 dogfood, or upgrade after merge.
