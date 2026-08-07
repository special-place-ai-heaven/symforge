# Composer review — cold-start Ready before rooted

**Branch:** `fix/cold-start-ready-before-rooted` (off `main`)  
**Reviewer:** Cursor Composer (adversarial, code-backed)  
**Date:** 2026-08-07  
**Scope:** Inline diff from `symforge-review.md` + repo inspection

Ground rules applied: comments are not behaviour; existence is not invocation; claims cite `file:line`; confidence labels used throughout.

---

## Executive summary

The fix is **directionally correct**. Gating `index_state()` on `load_source == EmptyBootstrap` (not `indexed_root.is_none()`) matches the measured defect and preserves the local-ref lane. Pairing `(a)` with `(c)` is **mandatory** — without the harness change, CI would go red deterministically.

**Merge recommendation:** Do not block on logic alone. Add one **rooted load + mutation → Ready** regression test before merge; consider fixing `sidecar_contract`’s `build_shared_index` to use a rooted index so the golden documents a genuinely healthy sidecar.

---

## Q1 — Can the fix deadlock the index at `Loading` forever?

**Cannot rule out “Loading forever” from the diff alone, but the catastrophic case — successful reload that never clears `EmptyBootstrap` — is ruled out by code structure.**

| Claim | Confidence | Evidence |
|-------|------------|----------|
| Successful reload replaces placeholder, sets `FreshLoad` | **Proven** | `main.rs:423-430` → `reload_for_state_placement` → `swap_and_publish` (`store.rs:2096-2121`); `apply_reload_data` sets `load_source = FreshLoad`, `indexed_root = Some(...)` (`store.rs:4354-4365`) |
| In-place mutation without updating `load_source` on success | **Proven false** | Reload builds new `LiveIndex` via `from_reload_data`, does not mutate placeholder |
| Failed reload leaves stuck `Loading` after freshen | **Likely** | Placeholder remains; `is_empty == false` + `EmptyBootstrap` → `Loading`; tools refuse via `loading_guard!` |

**To fully rule out production “forever Loading”:** integration trace showing `"background cold-start indexing complete"` (`main.rs:428`) and next `published_state().load_source == FreshLoad`. Diff does not add that test.

**Verdict:** No deadlock on **successful** load. Permanent `Loading` on **failed** load after freshen is a visible degraded state, not silent wrong answers.

---

## Q2 — Is `EmptyBootstrap` sufficient?

**Covers the reported bug path. Not a complete proxy for “populated + rootless”.**

| Route | Guarded? | Confidence | Location |
|-------|----------|------------|----------|
| Cold-start placeholder + targeted freshen | Yes | **Proven** | `empty_live_index()` `4084-4097`; `update_file` `4472` |
| `from_source_files` (local-ref lane) | No (`FreshLoad`, rootless) | **Proven** | `store.rs:4110-4130` |
| Snapshot restore | N/A (`SnapshotRestore` + rooted) | **Proven** | `persist.rs:1752-1764` |
| `remove_file` leaves `is_empty == false` on bootstrap | Yes → `Loading` | **Proven** | `store.rs:4513+` |

Local-ref lane intentionally uses `publish_ref_source`, not `capture_published_manifest` (`store.rs:978`). A populated rootless index with `FreshLoad` can still report `Ready` with unbound manifest if ref-source publish is skipped — **different lane**, not the measured cold-start flake.

**Verdict:** Sufficient for the **measured** defect. Incomplete as a general “never serve rootless populated as Ready” invariant.

---

## Q3 — Is flipping `health.json` legitimate?

**Legitimate given how the test is built. Does not model a healthy production index.**

**Proven:** `test_health_contract_golden` (`sidecar_contract.rs:296-313`) uses `build_shared_index` → `LiveIndex::empty()` + `add_file` (`196-206`). That is the defect scenario: `EmptyBootstrap`, files present, no `indexed_root`. Golden `"Ready"` was false once honesty guard exists.

**Settles (ii) vs (i):**

- **(i) Bootstrap placeholder model** — flip to `"Loading"` is **correct**.
- **(ii) Healthy index model** — test never modeled one. Positive Ready coverage exists in `handlers.rs:2792-2816` (`FreshLoad` manual index → Ready).

**Concern:** External readers may treat `health.json` as normative “healthy = Ready”. After flip: 2 files, 2 symbols, `Loading`. Honest for the test; misleading as exemplar.

**Recommendation:** Change contract test `build_shared_index` to rooted `FreshLoad` and restore golden `"Ready"`.

---

## Q4 — Is the positive case still tested?

**Partially. Gap on “rooted index + mutation → still Ready”.**

**Still covered:**

- `unbound_bootstrap_rebinds_writable_project_without_restart` — `reload()` → `Ready` (`store.rs:6078-6098`)
- Snapshot verify → Ready (`persist.rs:2689-2792`)
- Sidecar handler test → Ready (`handlers.rs:2792-2816`)

**Gap (proven):** Four flipped tests were the only `empty()` → `add_file` → `PublishedIndexStatus::Ready` assertions. No replacement test: `LiveIndex::load(root)` → `add_file` → `Ready`.

A future change pinning everything at `Loading` would **not** be caught by the four flipped tests.

**Recommendation:**

```rust
// store.rs — suggested regression
#[test]
fn rooted_index_mutation_after_load_stays_ready() {
    let tmp = TempDir::new().unwrap();
    write_file(tmp.path(), "a.rs", "fn alpha() {}");
    let shared = LiveIndex::load(tmp.path()).unwrap();
    shared.add_file("b.rs".into(), make_indexed_file_for_mutation("b.rs"));
    assert_eq!(shared.published_state().status, PublishedIndexStatus::Ready);
}
```

---

## Q5 — Does `is_ready()` delegating to `index_state()` change behaviour beyond the fix?

**Yes, only where `EmptyBootstrap` + `is_empty == false` — aligns previously divergent surfaces.**

| Call site | Effect | Confidence |
|-----------|--------|------------|
| `tools.rs:11242` — `StelStatusContext::from_server(..., guard.is_ready(), ...)` | `index_ready` false during bootstrap-with-admitted-file | **Proven** |
| `tools.rs:6674` — `ground_plan_economics` | Skips grounding while not Ready | **Proven** |
| Tool/edit guards (`tools.rs:3794-3804`) | Use `index_state()` directly; unchanged for Empty/Ready | **Proven** |

Truly empty bootstrap (`is_empty == true`): `index_state()` → `Empty`; `is_ready()` → false. **No change.**

**Verdict:** In-scope, desirable alignment. No proven hang loop.

---

## Q6 — Is the 20-second harness ceiling enough?

**Likely yes for current fixtures; not proven under worst CI contention.**

| Fact | Value |
|------|-------|
| `verify-tools` fixture | ~4 src files |
| `verify-tools-real` fixture | 3 modules |
| Old ceiling | 30 × 250 ms ≈ 7.5 s (file-count gate — wrong signal) |
| New ceiling | 80 × 250 ms ≈ 20 s sleep + RPC time |
| Gate condition | `_meta` evidence: `Ready`, `load_source != EmptyBootstrap`, `index_files > 0` |

**Verdict:** **Defensible** for tiny fixtures. If CI flakes at exit 2, bump to 120×250 ms (30 s) or env-configurable ceiling.

---

## Q7 — Is `process.exit(2)` handled correctly?

**Proven: CI fails correctly.**

- `.github/workflows/ci.yml:104-105` — direct `node scripts/verify-tools.cjs`; any non-zero fails job
- Exit 2 = harness abort; exit 1 = regression — useful local distinction
- Abort path: `proc.kill()` then `process.exit(2)` — **likely** fine; **speculative** orphan-child race on kill-before-exit (low severity on CI)

---

## Q8 — Is the `_meta` gate contract-safe?

**Mostly stable, with one format fragility.**

**Stable (proven):**

- Key: `symforge/project_evidence` — `result_status.rs:46`
- Fields: `result_status.rs:58-67`
- Attached via `with_project_evidence_scope` — `mod.rs:1540-1544`
- Harness: `SYMFORGE_NO_DAEMON=1` — local seed path

**Fragility (likely):** Evidence `load_source` uses `format!("{:?}", ...)` → `"EmptyBootstrap"` (`tools.rs:7305`). Status text uses snake_case `"empty_bootstrap"` (`format.rs:468`). Harness checks `!== "EmptyBootstrap"` — works today, breaks if evidence format changes without harness update.

**Recommendation:** Use `index_load_source_label()` in evidence construction, or document Debug-format coupling.

---

## Q9 — Anything else actually wrong

1. **Freshen-before-guard still mutates placeholder on same call** (`tools.rs:4466-4469`). After fix, `get_file_context` returns loading message after admit. Real load `swap_and_publish` discards placeholder — no lasting corruption (**proven**), but unnecessary work.

2. **`validate_file_syntax` skips loading guard** (`tools.rs:8672-8673`) and freshens first (`8668`). Same bootstrap mutation path — **bug survives in rarer form** for that tool during cold start.

3. **`Empty` vs `Loading` on true-empty bootstrap:** Before first freshen, state is `Empty`. First targeted read can freshen; tools that skip guard remain exposed.

4. **Failed background reload → permanent Loading** — operational regression vs silently wrong Ready. Consider surfacing reload failure as Degraded with reason (not in diff).

5. **No mutation-after-load → Ready test** (Q4) — suite cannot fail on broken positive mutation path.

---

## Summary table

| Q | Answer | Confidence |
|---|--------|------------|
| 1 | No deadlock on successful reload; stuck Loading if reload fails after freshen | Proven / Likely |
| 2 | Sufficient for cold-start bug; not for all rootless populated indexes | Likely |
| 3 | Golden flip correct for test setup; test does not model healthy index | Proven |
| 4 | Positive Ready on load exists; gap on mutation-after-load → Ready | Proven |
| 5 | `is_ready()` change is intentional alignment; low external risk | Proven |
| 6 | 20 s defensible for tiny fixtures | Likely |
| 7 | exit(2) fails CI correctly | Proven |
| 8 | Stable with Debug-format coupling risk | Likely |
| 9 | Guard-only leaves freshen ordering + validate_file_syntax hole | Proven |

---

## Pre-merge checklist

- [ ] Add `rooted_index_mutation_after_load_stays_ready` (or equivalent)
- [ ] (Optional) Fix `sidecar_contract` `build_shared_index` to use rooted `FreshLoad` + restore golden `Ready`
- [ ] (Optional) Use `index_load_source_label()` in `ProjectEvidence.load_source`
- [ ] Confirm full gate green: `cargo fmt --check`, clippy, tests, release build, both verify-tools fixtures

---

## Files touched by fix (reference)

| File | Change |
|------|--------|
| `src/live_index/health_view.rs` | `EmptyBootstrap` → `Loading`; `is_ready()` delegates to `index_state()` |
| `src/live_index/store.rs` | `capture_published_manifest` warnings; regression test; four assertion flips |
| `tests/fixtures/sidecar_contract/health.json` | `Ready` → `Loading` |
| `scripts/verify-tools.cjs` | `_meta` evidence gate; stderr inherit; `RUST_LOG=warn`; 80 polls; exit 2 abort |
