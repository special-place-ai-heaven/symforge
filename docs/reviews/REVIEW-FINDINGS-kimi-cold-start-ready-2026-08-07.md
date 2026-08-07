# Review findings — SymForge "index reports Ready before it is rooted"

**Date:** 2026-08-07
**Reviewer:** kimi-code/k3 (OMP session, `E:/project/symforge`)
**Subject:** Proposed fix on `fix/cold-start-ready-before-rooted` (diff packet in
`~/Downloads/symforge-review.md`). Review performed **against the repository**,
pre-fix working tree — every claim in the packet was verified against code.
**Verdict:** Correct; ship (a)+(c) together as planned. Two non-blocking additions
recommended (Q4 test, Q3 golden setup).

Confidence labels: **proven / likely / speculative**. All citations `file:line`
against the pre-fix tree.

---

## Q1 — Deadlock at `Loading`? Ruled out on the success path (proven); one caveat on the failure path

The claim "the real load replaces the index via a path that sets `FreshLoad`" is
verifiable and holds, with a refinement: the real load does not mutate the
placeholder in place — it **replaces** it:

- `src/main.rs:419-433` — cold start publishes `LiveIndex::empty()` and spawns
  `bg_index.reload_for_state_placement(&bg_root, …)`.
- `src/live_index/store.rs:2090-2121` (`reload_for_binding_with_exclusions`) —
  builds a **new** `LiveIndex` via `from_reload_data`, then `swap_and_publish(live)`.
- `src/live_index/store.rs:4348-4367` (`apply_reload_data`) — sets
  `load_source = IndexLoadSource::FreshLoad` (4356) **and**
  `indexed_root = Some(data.indexed_root)` (4366), unconditionally, in the only
  reload-application path.

The escape from `Loading` is guaranteed by the same swap that binds the root. No
path mutates the placeholder in place without updating `load_source`.
**Deadlock ruled out — proven.**

Caveat (failure path, **likely**): if `build_reload_data_for_binding_with_exclusions`
returns `Err` *after* a freshen already admitted a file, `main.rs:429` logs and
nothing retries. The placeholder then sits at `is_empty=false,
load_source=EmptyBootstrap` → `Loading` forever, every tool refusing. Recovery
requires explicit re-index (`index_folder`/reset), which does reach
`apply_reload_data`. A real liveness tradeoff the diff doesn't mention — but
strictly more honest than the old behavior (same failure previously reported
`Ready` with `source=unknown`), diagnosable via the new `warn!` plus health lines,
and the no-admission failure case reports `Empty` (the `is_empty` check precedes
the new guard), not `Loading`. Related static case: `src/daemon.rs:3359-3367`
deliberately keeps an `EmptyBootstrap` placeholder when cold load is refused by
catalog capacity — if that placeholder ever admits a file it too pins at `Loading`
with no automatic retry. Acceptable, but should be a conscious decision.

## Q2 — `EmptyBootstrap` sufficiency: covers every current route (proven); gap is future constructors (speculative)

Every route to a populated index:

| Route | `load_source` | `indexed_root` | Guard verdict |
|---|---|---|---|
| Placeholder + freshen mutation (the bug) | `EmptyBootstrap` | `None` | caught |
| `apply_reload_data` / `load_with_project_state` | `FreshLoad` | `Some` (store.rs:4366, store.rs:4054) | correctly Ready |
| Snapshot restore | `SnapshotRestore` | `Some` (persist.rs:1764; wire format carries no root, caller always supplies it — persist.rs:1706) | n/a |
| `from_source_files` (P1 local-ref lane) | `FreshLoad` | `None` (store.rs:4132) | **deliberately not caught** — correct |
| `reset_to_empty` | `EmptyBootstrap` | `None` | caught |

The local-ref lane's legitimacy claim checks out: generations publish via
`publish_ref_source`, which `expect`s a source identity built from the git ref
(store.rs:1521-1525); `build_ref_source_generation` never consults
`indexed_root`. A root-based guard would pin it at `Loading` forever. Also: the
local-ref lane does **not** pass through `capture_published_manifest` (only
callers: store.rs:1247, 1982, 2853, 2965 — all P0-lane publications), so the new
`warn!` does not false-positive on the legitimate rootless lane. **Proven.**

No code path clears `indexed_root` on a populated index — assignments are
constructor/reload-only. The `remove_file`-never-restores-`is_empty` hint resolves
consistently: placeholder that admitted then removed a file stays `Loading`, the
honest state for a still-unbound index.

Residual risk: the guard keys on *provenance* (`load_source`), not on the defect
(`root None` ∧ `no independently published identity`). A **future** constructor
building populated + rootless + non-`EmptyBootstrap` without its own identity
slips through. None exists today. Belt-and-braces alternative: "non-empty ∧
`capture_published_manifest` returns None ⇒ not Ready", but that couples state to
manifest capture. The provenance guard is the pragmatic 95% fix — ship it.

## Q3 — `health.json` flip: legitimate, option (i), but the test setup deserves a look (proven)

The test builds its index at `tests/sidecar_contract.rs:196-199`:
`LiveIndex::empty()` plus write-guard `add_file`s — the fixture **always modeled
an EmptyBootstrap placeholder that admitted two files**, never a healthy rooted
index. `/health` renders `published.status_label()` (`src/sidecar/handlers.rs:410`),
post-fix `Loading` for exactly that state. The flip records the honest output of
what the test actually constructs. **Not a hidden regression.**

Caveats:

1. **Intent mismatch.** A file named `health.json` in a contract suite now
   documents `Loading` with `file_count: 2`, and the suite no longer contains any
   golden showing a healthy index saying `Ready`. Better: make
   `build_shared_index` produce a rooted index (reload from temp dir) so this
   golden keeps modeling "healthy ⇒ `Ready`", and add a second golden for the
   placeholder state.
2. Neighboring assertion `src/sidecar/handlers.rs:2815`
   (`index_state.contains("Ready")`) builds a `FreshLoad` index
   (handlers.rs:2718) — survives the fix. No unflipped casualty.

## Q4 — Positive-case coverage: the specific hole exists, but the suite can still fail positively (proven)

The four flipped assertions (store.rs:5677, 5688, 5852, 5863 — all
`LiveIndex::empty()` + mutate) were the **only** mutation→status assertions in
the crate. After the flip, **no test asserts that mutating a rooted, loaded index
publishes `Ready`**. Remaining positive coverage:

- store.rs:6096-6098 — `reload` → `is_ready()` ∧ `index_state() == Ready` ∧
  `load_source == FreshLoad` (load path, not mutation);
- persist.rs:2792 — snapshot verify completion → `is_ready()`;
- daemon/tool health integration tests (tools.rs:16226, 19132, …) asserting
  `index_state=fresh_process` after real `index_folder` operations.

A future change pinning everything at `Loading` **would** fail 6096-6098 and the
health tests — the suite is not blind in the positive direction. But the exact
contract "mutation on a rooted index stays `Ready`" is untested — precisely the
contract the flips vacated. Recommended in this PR: reload from tempdir,
`add_file`, assert `published_state().status == Ready`. Cheap.

## Q5 — `is_ready()` delegation: two production consumers, both safe (proven)

Non-test callers, exactly two:

1. `src/protocol/tools.rs:6673-6675` (`ground_plan_economics`) — newly returns
   early during the bootstrap-admitted window; grounding skipped, steps keep the
   plan-only floor. Temporary, self-correcting on load completion, conservative
   direction for the economics layer. Benign.
2. `src/protocol/tools.rs:11242` → `src/stel/status.rs:339` — status body
   `index_ready:` line now reads `false` during the window. An externally visible
   contract change beyond the guard's core purpose — the intended one. Explains
   the harness comment "do NOT gate on `index_ready`": on a pre-fix binary that
   line is `true` while unbound, and in a proxied topology the front-end renders
   the *worker's* index lines, possibly from an older daemon. Gating on `_meta`
   evidence is right.

No startup gate, poll loop, or health endpoint consumes `is_ready()` in a way
that could hang; sidecar `/health` uses `status_label()` (handlers.rs:410).

## Q6 — 20 s ceiling: defensible (proven)

The "10 s cold load" number is a red herring: the harness spawns with
`cwd = tests/fixtures/verify-tools` and pins `SYMFORGE_WORKSPACE_ROOT` to it.
That fixture is **14 files / 47 KB**; `verify-tools-real` is 13 files / 61 KB.
Sub-second-to-few-seconds even on a contended runner. 80×250 ms is ~an order of
magnitude of headroom, and the abort path dumps the last evidence. Do not raise.
Revisit only if the harness is ever pointed at a real-repo fixture.

## Q7 — `process.exit(2)`: correctly wired; actually fixes a process leak (proven)

Invoked directly — `.github/workflows/ci.yml:104-105`,
`.github/workflows/release.yml:94-95` run `node scripts/verify-tools.cjs --bin …`
as bare `run:` steps; any non-zero fails the step, no wrapper swallows it. The
abort branch `proc.kill()`s before `process.exit(2)` — child not orphaned. This
**improves** on the old path: pre-fix `throw` inside `startSession` before the
session handle existed, leaving the spawned child to be reaped only by node exit.
Exit-code semantics (2 = harness couldn't run, 1 = real regression) now
distinguishable in CI logs. No dangling-child path in the new code.

## Q8 — `_meta` gate: holds for this topology, fails safe, one fragility (proven/likely)

Chain is complete: every `tools/call` dispatch is wrapped in
`with_project_evidence_scope(self.local_project_evidence(), …)`
(`src/protocol/mod.rs:1539-1543`); `local_project_evidence` always returns `Some`
with all three fields (tools.rs:7300-7321); `status` returns via
`statused_tool_result` → `ResultStatus::into_call_tool_result`, which attaches
`_meta["symforge/project_evidence"]` in scope (result_status.rs:129-134). With
`SYMFORGE_NO_DAEMON=1` the local seed is attached. `index_state` renders
`status_label()` → `"Ready"`/`"Loading"` (store.rs:3142-3144), matching the
harness compare. **Proven present and correctly shaped.**

Fragilities, both failing **safe** (abort, not false-pass):

- `load_source` is `format!("{:?}")` of the Rust enum (tools.rs:7305) — renaming
  `EmptyBootstrap` silently breaks the gate into permanent abort. Visible in CI
  immediately, but a named-serde string would be sturdier.
- "Absent evidence vs still loading": in-process evidence is effectively never
  absent for a statused result, so "absent" in practice means version skew or a
  non-statused render path — a topology/version signal, not a state signal.

Contract stability: the evidence key has no `contract_version` (unlike
`RESULT_STATUS_META_KEY`), but harness and binary ship in the same repo/PR, so
drift is caught atomically. Acceptable.

## Q9 — Anything else actually wrong

Nothing blocking. Two items:

1. **`warn!` repetition (low).** `capture_published_manifest` runs on **every**
   publication (store.rs:1982, 2853, 2965). While the placeholder-admitted state
   persists, every watcher/freshen mutation republishes and re-warns — dozens of
   identical warnings possible during a cold start with an active watcher.
   Bounded by mutation rate, self-limiting once the real load lands, arguably the
   evidence you want — but if CI logs spam, gate to once per generation or
   downgrade repeats to `debug!`.
2. **Unbound-state consumers during the window.** Between admission and load
   completion, tools are refused — correct — but anything reading
   `published_state()` directly (status bodies, evidence `index_files > 0`, repo
   outline) still reports placeholder contents. External consumers polling
   `status` during cold start will see `index_ready: false` with
   `index_files: 1` — odd but honest. No action; don't be surprised in support
   questions.

---

## Verdict

The fix is correct; every comment-claim sampled verified against the code. Ship
(a)+(c) together as planned. Recommended additions, neither blocking:

1. Mutation-on-rooted-index → `Ready` test (Q4).
2. Flip `build_shared_index` to a rooted index so `health.json` keeps a
   healthy-`Ready` exemplar, plus a second golden for the placeholder state (Q3).
