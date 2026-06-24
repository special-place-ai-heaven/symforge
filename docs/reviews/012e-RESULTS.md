# Sprint 012e — Results (cross-project freshness + read-path coherence)

**Date:** 2026-06-24. **Lane:** `E:\project\symforge-012`. **Campaign:** 012 trust campaign (Culprit B).
**Outcome:** B2/D12 **FIXED + merged** (PR #369). D15 **investigated → OPEN-with-plan** (evidence-based, SC-5 escape hatch). One scope item (B2) delivered; the other (D15) honestly found not safely sliceable this sprint.

## What landed

### ITEM 1 — B2 / D12 — FIXED (PR #369, commit `745313d`, on `main` via merge `71e48f5`)
Cross-project reads now stay fresh after ANY watcher-observed change (file edit/add/delete), not only after a git commit.

- **Mechanism (LAZY re-intern at the read path):** `DaemonState::refresh_working_set_bases`, invoked from `call_tool_handler` before the cross-project search. For each targeted project it compares the working-set entry's interned `Arc<LiveIndex>` against the project's CURRENT published index via `Arc::ptr_eq` (every publish swaps a new `Arc`, so a pointer mismatch == the index advanced). On mismatch it force-replaces the interned base (`intern_base_refresh`) and re-attaches the entry with a fresh empty overlay.
- **The decisive correction over the naive design:** a watcher edit keeps `BaseKey = (root, commit)` unchanged, so the plain `intern_base` cache-HITs the STALE base. The fix force-REPLACES the `bases`-table value for that key (SC-002-preserving: one `Arc` per key) rather than reusing the cache hit.
- **Design provenance:** EAGER-vs-LAZY settled by investigation + an independent adversarial skeptic (both chose LAZY; EAGER would force a layering-inverted watcher→working-set callback on the hot publish path).

**Success criteria:**
| SC | Status | Evidence |
|----|--------|----------|
| SC-1 freshness (add) | ✅ | unit test + real-watcher live daemon-HTTP transcript (gamma appears post-edit) |
| SC-2 deletion/move (no ghost) | ✅ | unit test + live transcript (alpha removed → gone) |
| SC-3 no-stale invariant | ✅ | retained `view.rs` StaleOverlay-fence unit test; empty-overlay rebase via `Overlay::fresh` |
| SC-4 single-project parity | ✅ | gated out by shared `resolve_cross_project_targets` (byte-identical); frecency-neutral (no edit-commit hook on the path); code-reviewer confirmed |
| SC-6 full gate | ✅ | CI green on the merged code: `fmt`/`check`/`clippy --all-targets -D warnings`/`test --all-targets`/`build --release`/`embed` |

- **Tests:** `daemon::tests::test_cross_project_read_is_fresh_after_watcher_reindex` (add, delete, mismatch-gate A-unchanged, lone-non-active `Targets::One`, SC-002-after-replace).
- **Review:** wf `wusdyz8fx` — code-reviewer **APPROVE** (7 invariants hold), security-reviewer **APPROVE** (no cross-project leak, full identity chain verified), skeptic **APPROVE_WITH_NITS** (test proven non-vacuous; nits applied: forward-compat overlay comment + the two extra test cases).
- **Scope:** one file (`src/daemon.rs`); no `view.rs`/`embed.rs` change; non-empty overlays (D-B0) remain deferred.

### ITEM 2 — D15 — OPEN-with-plan (no code this sprint, by evidence)
Investigation found D15's stated acceptance (an overlay edit becomes visible in an ordinary read) is **NOT live-verifiable this sprint**: there is NO production single-project overlay-WRITE path (every `overlay.upsert` is test-only; `SymForgeServer` holds only `index: SharedIndex`), so flipping the 64+13 reads to `IndexView` is byte-identical — a parity refactor with **zero observable effect**. The real root is the missing overlay-WRITER, OUT of D15's read-migration scope.
- **Decision (SC-5 escape hatch):** ship the phased migration **plan** (`docs/reviews/D15-readpath-coherence-migration-plan.md`), keep D15 OPEN, name the overlay-WRITER as the tracked root blocker. The parity-only read-flip is NOT landed as a no-op (ponytail; and the future writer may reshape the seam).

## Honest notes / caveats (mock-as-mock, deferred-as-deferred)
- **`Arc::ptr_eq` freshness is not exact:** it over-triggers on mtime-only `touch_mtime` swaps (a fresh `Arc` without a published-state bump). A spurious refresh is SAFE (re-captures an equal-or-fresher snapshot) and rare (cross-project read path only); documented at the call site. Tighter signal (capture `published_state().generation` into `IndexBase`) is the noted upgrade path, not taken (least-code).
- **Local final gate re-run skipped:** the merged code is byte-identical to what CI validated green; a local `--all-targets`/`--release` re-run was skipped to protect a critically-tight disk (see below). CI is the gate of record for SC-6 on the merged code; the real-watcher live-verify was run pre-merge against the built binary (behavior unchanged by the post-review test-only nits).
- **No new product defect found.** One pre-existing **tracked-minor** surfaced by review: the `bases` intern table has no orphan GC (a genuine git-commit advance orphans the old `BaseKey`; unbounded only over a very-long-lived multi-commit session; the common no-commit path force-replaces the same key). NOT a B2 regression; ledgered, owner 012, low priority.

## Environment incident (2026-06-24)
Mid-sprint, the operator's harnesses crashed — root cause **C: at 2% free** (854 GB drive, 19 GB free), Disk at 100% utilization, driven by two ~150 GB `.vhdx` files (Docker data + Ubuntu WSL) plus dev-build churn. Relief applied (all safe, no data loss): `cargo clean` of the 012 target (−9.3 GB on E:), `docker system prune -a` of unused images+cache (−13 GB inside the Docker vhdx; **volumes kept**), and cleared regenerable C: dev caches (cargo/npm/pip, −12.6 GB). Result: **C: 2% → 5.5% (47 GB free), E: 8% → 24% (23.8 GB free)** — out of the danger zone. Unrealized lever (needs admin/Docker-Desktop GUI, not done): compacting the Docker vhdx (~39 GB) and the 151 GB Ubuntu vhdx.

## Next frontier
The cross-project/overlay-WRITE track: **D15 overlay-WRITER** (then the read-path migration becomes live-verifiable) and **D-B0** (per-`IndexView` derived index for non-empty overlays). Independent: **D16-full** (`/mcp` per-connection daemon session; silent-half already contained). See the updated handoff + ledger.
