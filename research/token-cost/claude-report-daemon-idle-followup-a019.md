# Follow-up review (A019): corrected daemon idle-shutdown patch

Reviewer: independent Claude checkpoint (same reviewer as
`claude-report-daemon-idle-review-a019.md`, resumed with full first-review
context). Read-only; no product code, tests, docs, git state, or caches
modified. SymForge-indexed inspection first; `git diff` for exact diffs. No
secret values reproduced.

## Phase 1 — blind diff review

Changed in scope: `src/daemon.rs` (+152/-5), `Cargo.toml` (tokio `test-util`
feature), `README.md` (one env-table row). Unrelated branch artifacts unchanged
from the last checkpoint (`CLAUDE.md` doc-hygiene section, `tasks/*`, untracked
`docs/*`, `research/*`) — excluded from the verdict.

The blind pass found no new defects in the product code; the two items noted
independently (test ordering race window, `test-util` in `[dependencies]`) are
detailed below as non-blocking.

## Phase 2 — blocker closure, with anchors

1. **Unset ⇒ persistent explicit daemon — CLOSED.**
   `daemon_idle_shutdown_from_env` (src/daemon.rs:111-118) now begins
   `std::env::var(...).ok()?` — unset returns `None`, the reaper's idle branch
   (guarded by `if let Some(idle)`) never runs, and
   `run_daemon_until_shutdown`'s `notified()` arm never fires. An explicit
   `symforge daemon` is persistent by default.
2. **Auto-spawn injection preserving inherited values — CLOSED.**
   `spawn_daemon_process` (src/daemon.rs:2509-2514) injects `"600"` only when
   `var_os(DAEMON_IDLE_SHUTDOWN_ENV).is_none()`. An inherited `"0"` is `Some`,
   so it is passed through and disables shutdown in the child. Edge: an
   inherited *empty string* is also `Some` → child parses garbage → falls back
   to 600 enabled; harmless and consistent with the garbage-fallback policy.
3. **Parser** (src/daemon.rs:111-118): unset→`None`, `0`→`None`, `5`→60 s,
   `"soon"`→600 s enabled, valid→value. Test
   `daemon_idle_shutdown_env_parsing` (:4826-4855) covers all five; the
   duplicate comment was removed. Closed.
4. **README** (README.md:538): mode distinction, 600 s auto-spawn default,
   persistent-when-unset explicit mode, 60 s clamp, `0` disables — all four
   claims match the code exactly. Closed. (The lifecycle change should still be
   release-note-visible via the commit message — see verdict.)
5. **Behavior test reaches REAL machinery — CLOSED.**
   `daemon_idle_shutdown_waits_for_authenticated_activity_then_notifies`
   (src/daemon.rs:4857-4900) calls the real `spawn_daemon` (real reaper task,
   real axum listener) and drives a real `GET /v1/projects` through
   `authed_client` → router → `authorize_daemon_request` (:551-561) — the
   actual activity-stamp path, not reimplemented logic. It proves both halves:
   an authenticated request defers the sweep; a stale interval produces the
   notification. This is exactly the behavioral evidence the first review
   demanded.
6. **Determinism of paused time + wall-clock backdating.** The mechanism is
   sound: `last_activity_at` is wall-clock, but the test never *waits*
   wall-clock — it backdates the atomic by 60 001 ms directly and uses
   `tokio::time::advance(15s)` to fire the reaper interval (period =
   min(ttl/4 clamp, idle/4 = 15 s) = 15 s). The 1 ms `timeout` around
   `notified()` resolves via tokio auto-advance. Platform-independent. One
   residual race, non-blocking: in phase one the backdate is stored *before*
   the HTTP request; while the request awaits socket readiness the
   current-thread runtime can go idle, and paused-time auto-advance may jump to
   the reaper's next tick *while the stamp is still backdated*, firing a stored
   `Notify` permit that would fail the "must defer" assertion. Receipts (full
   suite green, smoke green) say it does not bite in practice on loopback, but
   it is a latent flake. One-line hardening if it ever flakes: drop the
   phase-one backdate (the fresh spawn stamp already proves deferral) or stamp
   *after* the request returns.
7. **Test isolation — acceptable.** `env_lock()` serializes env mutation;
   `EnvVarGuard` restores on drop; `SYMFORGE_HOME` points at a `TempDir`, so
   daemon port/pid/token runtime files land in the temp home, not the shared
   one. Teardown aborts the reaper, sends `shutdown_tx`, and awaits
   `server_task` (which runs the owner-checked cleanup, :3021). A mid-test
   panic would leak the daemon task only within the test binary — the same
   exposure as every sibling daemon test. No new cross-test leak vector.
8. **`test-util` proportionality — acceptable, with a nit.** No new dependency;
   the feature only adds the pause/advance API and is inert unless
   `start_paused` is used, so production behavior is unchanged. It does compile
   into release builds because it sits in `[dependencies]` (Cargo.toml:74). The
   marginally cleaner form is a `[dev-dependencies]` tokio entry carrying
   `test-util` (feature unification applies it to test builds only). Cosmetic;
   not worth blocking.

## Phase 3 — scope / residual-risk judgment

The selected policy lands well. The deferred first-review warnings are now
soundly bounded by the mode split:

- **Wall clock (sleep/resume jump):** can only kill an *auto-spawned* daemon
  early; its client respawns it in milliseconds on the next call (reconnect
  seam), and explicit services are unaffected unless they opted in. Cost of
  failure ≈ one cheap respawn. Deferral justified; no monotonic-clock
  abstraction needed absent a concrete failure.
- **Long direct request mid-flight:** auto-spawn's stdio proxies heartbeat
  every 15 s (src/main.rs:267-273) throughout long work; direct hook requests
  carry a 500 ms budget (src/cli/hook.rs:991-992); explicit daemons that might
  host exotic direct clients are persistent by default. Deferral justified.

Product fit stands as judged previously: kills the SymForge-owned
orphan-daemon class; correctly does not claim to reap leaked live proxy stacks
(client-owned; they keep heartbeating and legitimately look alive).

## Phase 4 — verification

Receipts trusted (TDD red→green on the parser expectation, fmt clean, clippy
`-D warnings` clean, full `cargo test --all-targets -- --test-threads=1`
exit 0 in 1 108 s, `git diff --check` clean with only pre-existing CRLF
warnings, isolated 60 s smoke: daemon exited after 75.0 s with runtime files
absent) — nothing in the diff contradicts them. Independently this session:
full `git diff` of the three files; SymForge-verified surrounding code
(`authorize_daemon_request`, reaper block, `spawn_daemon_process`, heartbeat
route, hook fallback); parser test previously run (pass). Full suite not rerun
— no diff-driven reason.

## Findings summary

**Blocking: none.**

Non-blocking:
- [src/daemon.rs:4869-4880] Latent paused-time auto-advance race in the
  behavior test's first assertion (detail in Phase 2 item 6). Fix only if it
  ever flakes.
- [Cargo.toml:74] `test-util` in `[dependencies]` reaches release builds; a
  dev-dependencies tokio entry would confine it. Cosmetic.
- FYI: an inherited empty-string env value enables 600 s in the child rather
  than being treated as unset — consistent with the garbage-fallback policy.

Done well: the mode split is implemented at exactly the right seam — one
`var_os().is_none()` guard at the spawn site plus one `?` in the parser, zero
new configuration surface; and the behavior test exercises the genuine reaper
and genuine authenticated route instead of a reimplementation, which is the
difference between testing the feature and testing a mirror of it.

## Verdict

**APPROVE_COMMIT**

All three first-review blockers closed: (1) default-on-for-explicit-services —
resolved by policy + code; (2) operator documentation — README row accurate;
(3) behavioral test evidence — real-daemon paused-time test proves defer +
fire.

Recommended conventional commit (release-note-visible lifecycle change):

```
feat(daemon): idle self-shutdown for auto-spawned daemons

Auto-spawned (detached) daemons now exit after 600s without any
authenticated request; clients transparently respawn on the next call.
Explicitly started `symforge daemon` processes remain persistent unless
SYMFORGE_DAEMON_IDLE_SHUTDOWN_SECS is set (nonzero clamps to >=60s,
0 disables). Idle shutdown flows through the same graceful path as
SIGINT/SIGTERM, including owner-checked runtime-file cleanup. Only
authenticated traffic counts as activity, so unauthenticated probes
cannot keep a zombie daemon alive. Known boundary: this does not reap
leaked MCP proxy stacks whose client still holds stdin — those keep
heartbeating and legitimately look alive.
```

Commit scope: `src/daemon.rs`, `Cargo.toml`, `README.md`. Exclude the
unrelated `tasks/*`, the `CLAUDE.md` doc-hygiene hunk, and untracked
`research/`/`docs/` artifacts unless intentionally bundled.
