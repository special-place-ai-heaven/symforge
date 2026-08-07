# Independent review findings — cold-start Ready before rooted

**Reviewer:** Cursor Grok (independent adversarial pass)  
**Date:** 2026-08-07  
**Request:** `C:\Users\rakovnik\Downloads\symforge-review.md`  
**Branch:** `fix/cold-start-ready-before-rooted`  
**Code under review:** `904b2c2` (`fix(live-index): refuse Ready on a bootstrap placeholder with no bound root`)  
**Worktree:** `E:/project/symforge-policy`  
**Base:** `main` (diff: 4 files, +134/−33)  
**Full serial suite:** not run. Claims tagged **proven** / **likely** / **speculative** from code reading only.

**Intent under review:** Stop an EmptyBootstrap placeholder that admitted a file from reporting `Ready` (and unbound knowledge / `source=unknown`) before the detached initial load binds a root; make harness readiness gate on source-bound evidence.

Ground rules applied: comments are claims, not evidence; existence ≠ invocation; cite `file:line`.

---

## Lead judgment

Core guard is sound and does **not** deadlock on a successful cold start. Merge-blocking only if you require (a) a rooted Ready `/health` golden, or (b) an explicit mutation→Ready positive test, before land. Do not treat the `health.json` flip as proof the sidecar happy path was re-validated — the contract helper still builds EmptyBootstrap.

---

## Q1 — Can the fix deadlock the index at `Loading` forever?

**Verdict: No for successful cold start. Yes if background load fails after a file was admitted.**

**Confidence:** proven (success) / likely (failure)

### What must be true to escape Loading

`index_state()` returns `Loading` when `load_source == EmptyBootstrap` (`health_view.rs:347–348`). Escape requires something to set `load_source` to another variant.

### Success path — ruled out from code (not from comments)

Cold start (`main.rs:422–433`):

1. Publishes `LiveIndex::empty()` (EmptyBootstrap).
2. `spawn_blocking` → `reload_for_state_placement`.
3. That builds a **new** index via `from_reload_data` → `apply_reload_data` (`store.rs:4564–4575`), which sets:
   - `load_source = IndexLoadSource::FreshLoad`
   - `indexed_root = Some(data.indexed_root)`
4. Then `swap_and_publish(live)` (`store.rs:2203–2218`).

It does **not** mutate the placeholder in place without updating `load_source`. The unverified claim in the review packet (“real load replaces via FreshLoad”) is **proven**.

### Failure path — residual hang-shaped availability loss

If background reload errors (`main.rs:428–429`), an EmptyBootstrap generation that already admitted a file (`is_empty == false`) stays `Loading` until a successful reload (`index_folder` / restart). Before the fix that state lied as `Ready`; after, tools refuse. Not a process deadlock; permanent tool refusal until recovery.

**Diff alone is insufficient; worktree code rules out the success-path deadlock.**

---

## Q2 — Is `EmptyBootstrap` sufficient to cover the bug?

**Verdict: Yes for the reported cold-start race. No for the broader class “populated + no `indexed_root`”.**

**Confidence:** proven

### Covered

- Only `empty_live_index` / `reset_to_empty` set `EmptyBootstrap` (`store.rs:4286`, `2250`).
- `update_file` / `add_file` set `is_empty = false` and never touch `load_source` (`store.rs:4682`) — the race this fix targets.
- `remove_file` never restoring `is_empty` does **not** evade the guard: load_source stays EmptyBootstrap → still Loading.

### Not covered by the guard

- `LiveIndex::from_source_files` → `FreshLoad` + `indexed_root: None` (`store.rs:4312–4332`). Intentionally excluded; local-ref publishes identity on a separate path (`store.rs:1513–1575`).
- Default `swap_and_publish` of a FreshLoad/rootless index still hits `capture_published_manifest` early-return (`store.rs:1010–1021`, now `warn!`) while `index_state()` is **Ready**. Sidecar handlers’ test helper builds exactly that shape (`handlers.rs:2769–2787`) and still expects Ready (`handlers.rs:2871–2873`).

No other production writer of `load_source = FreshLoad` besides `apply_reload_data` / constructors; snapshot restore always binds a root (`persist.rs:1991–2003`).

---

## Q3 — Is flipping `tests/fixtures/sidecar_contract/health.json` legitimate?

**Verdict: (i) for what the contract test builds — not (ii).**

**Confidence:** proven

`tests/sidecar_contract.rs:196–205` builds via `LiveIndex::empty()` + `add_file`. That is EmptyBootstrap-with-files — the buggy shape. Previously the golden advertised `Ready` for that lie; `Loading` matches the new honesty rule.

It does **not** model a normal rooted healthy index. Contrast: `handlers.rs` builds FreshLoad fixtures and still asserts Ready (`handlers.rs:2871–2873`).

Nobody updated the contract helper to `from_source_files` / a real load, so the golden now documents “2 files + Loading” as the contract fixture’s answer — correct for the helper, misleading as “healthy sidecar” documentation.

**What would settle it:** change `build_shared_index` in `sidecar_contract.rs` to a FreshLoad (or loaded) index and keep `Ready`, **or** keep the flip and add a second golden for a rooted Ready health response.

---

## Q4 — Is the positive case still tested?

**Verdict: Partially. Reload → Ready yes; mutation → Ready on a rooted index no.**

**Confidence:** proven

Still asserts Ready:

- `unbound_bootstrap_rebinds_writable_project_without_restart` — empty → `reload` → `Ready` + `FreshLoad` (`store.rs:6327–6329`)
- `test_is_ready_true_when_not_tripped` / `test_index_state_ready` — FreshLoad constructors (`query.rs:5634–5676`)
- Snapshot verify → `is_ready()` (`persist.rs:3109`)

The four flipped assertions were never a true positive: they asserted Ready on EmptyBootstrap mutations. A change that pins **everything** at Loading would still fail the reload/FreshLoad tests. A change that makes **publication after mutation** always Loading on a previously Ready index would not be caught by those four.

Hole is real but narrower than “suite can’t fail positive.”

---

## Q5 — Does `is_ready()` delegating to `index_state()` change behaviour beyond the fix?

**Verdict: Yes, one intentional external change; no hang found in-repo.**

**Confidence:** proven (behaviour) / likely (no hang)

Previously `is_ready()` ignored EmptyBootstrap and returned true once `is_empty == false`. Now it is false for that state — same as `index_state() == Ready` (`health_view.rs:327–328`).

Call sites:

- Tool `loading_guard!` uses `index_state()`, not `is_ready()` (`tools.rs:3885–3895`) — already covered by (a).
- `status` / STEL: `guard.is_ready()` → `index_ready` (`tools.rs:11376`, `stel/status.rs:352`). After premature freshen, `index_ready` flips from true → false. That is the honesty fix on the status surface.
- `ground_plan_economics` early-returns when not ready (`tools.rs:6759`) — skips grounding during bootstrap; no loop.

No in-repo startup loop waits on `is_ready()`. External clients polling `index_ready` will wait longer / see false during bootstrap — correct, not a deadlock.

---

## Q6 — Is the 20-second harness ceiling enough?

**Verdict: Yes for these fixtures; 20s is defensible. Prefer ~40s if CI flakes.**

**Confidence:** likely

Harness fixtures are tiny (`verify-tools` ~34 files, `verify-tools-real` ~13). The ~10s full-repo cold-load figure does not apply here. Ceiling is sleep budget (80×250ms); work can finish earlier. Risk is only a heavily contended runner where even a small fixture’s load exceeds 20s — then exit 2 aborts every run.

---

## Q7 — Is `process.exit(2)` handled correctly?

**Verdict: Yes for CI failure. Child cleanup is good enough.**

**Confidence:** proven (CI) / likely (orphan)

Invoked directly in CI:

```yaml
node scripts/verify-tools.cjs --bin target/release/symforge
```

(`.github/workflows/ci.yml:127–128`, same in `release.yml`). Any non-zero fails the step. Exit 2 matches “missing binary” (`verify-tools.cjs:334`).

Abort path: `proc.kill()` then `process.exit(2)` (`verify-tools.cjs:244–245`) — does not return into the case loop, so no half-loaded snapshot compare. Residual: if `kill` fails, parent still exits and the job tears down; brief orphan possible on Windows, not a silent green.

---

## Q8 — Is the `_meta` gate contract-safe?

**Verdict: Stable enough to gate on; abort text overclaims “absent.”**

**Confidence:** proven

`ProjectEvidence` is a documented meta surface (`result_status.rs:45–67`) with tests (`tools.rs:20841–20866`). Harness fields exist. `load_source` is `format!("{:?}", ...)` → `"EmptyBootstrap"` (`tools.rs:7390`), matching the JS check — **not** the snake_case health label `empty_bootstrap` (`format.rs:468`). Renaming Debug or switching evidence to the label form would break the gate until timeout.

Attachment always writes the key: unbound → `{"bound": false}` (`result_status.rs:120–126`), not omission. Abort copy saying `null/absent evidence` is wrong for that case; you’d see `{"bound":false}`. Distinction “still loading” vs “contract changed” still works if you read `index_state` / `load_source` vs `bound:false`.

---

## Q9 — Anything else that is actually wrong

| # | Finding | Confidence |
|---|---------|------------|
| 1 | Contract helper left on buggy construction (`sidecar_contract.rs:196–205`): golden flip is honest, but suite never goldens a rooted Ready `/health`. Prefer fix the helper, not only the golden. | proven |
| 2 | Failed cold-start after admission → permanent Loading until reload (Q1). Worse availability than the old lie; `index_folder` is the recovery path (not loading-guarded at entry — `tools.rs:7538+`). | likely |
| 3 | Harness couples to Debug enum spelling of `load_source` (Q8). | proven |
| 4 | No test: rooted Ready → mutate → still Ready (Q4). | proven |
| 5 | Freshen-before-guard ordering still unfixed (acknowledged in request). Guard is the choke point — acceptable if the state machine stays honest. Not a new defect in this diff. | — |

Not findings (per request §5): verbose comments; `RUST_LOG=warn` + inherited stderr noise; unreleased status.

---

## Files reviewed

| Path | Role |
|------|------|
| `src/live_index/health_view.rs` | `is_ready` / `index_state` guard |
| `src/live_index/store.rs` | manifest capture warns; reload/FreshLoad; tests |
| `src/main.rs` | cold-start empty + background reload |
| `scripts/verify-tools.cjs` | harness gate + exit 2 |
| `tests/fixtures/sidecar_contract/health.json` | golden flip |
| `tests/sidecar_contract.rs` | EmptyBootstrap helper (not in diff) |
| `src/sidecar/handlers.rs` | FreshLoad Ready health unit test (not in diff) |
| `src/protocol/result_status.rs` | `_meta` evidence contract |
| `.github/workflows/ci.yml` | direct `node` invocation |

---

## Recommended follow-ups (non-blocking unless noted)

1. **(optional block)** Fix `sidecar_contract::build_shared_index` to FreshLoad/rooted and restore a Ready golden, *or* add a second Ready golden.
2. Add `rooted_index_stays_ready_after_mutation` next to the new EmptyBootstrap regression.
3. Soften harness abort text: distinguish `{bound:false}` from missing key; optionally accept both Debug and label forms of `load_source`, or document Debug as the contract.
4. If cold-start reload failure + admitted files becomes operator-visible: surface it in health/status beyond perpetual Loading.
