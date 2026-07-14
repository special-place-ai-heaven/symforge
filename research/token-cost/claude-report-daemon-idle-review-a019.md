# Review: daemon idle self-shutdown (uncommitted, src/daemon.rs)

Reviewer: independent Claude code-review checkpoint (A019). Read-only; no product
code, tests, git state, caches, or worktrees were modified. SymForge-indexed
inspection first; `git diff` for the exact diff. No secret values reproduced.

## Scope reviewed

Uncommitted diff on `feat/token-speed-tool-trust`. In-scope change:
`src/daemon.rs` (+105/-5: env parser, `DaemonState` fields, activity stamp,
reaper extension, `run_daemon_until_shutdown` select arms, one test). Unrelated
branch artifacts, excluded from the code verdict: `CLAUDE.md` (doc-hygiene
section), `tasks/lessons.md`, `tasks/todo.md`, `docs/*`, `research/*`
(untracked).

Verified default: **600 seconds** (`DEFAULT_DAEMON_IDLE_SHUTDOWN_SECS = 600`,
src/daemon.rs:110). The handoff's "3600" is stale — the default was lowered
in-session after the handoff was drafted.

## Findings

### Critical (blocking)

1. **[src/daemon.rs:3073-3130 `run_daemon_until_shutdown`] Default-on lifecycle
   change for explicitly managed daemons, documented nowhere operator-facing.**
   Every production daemon — auto-spawned (`spawn_daemon_process`, :2477, runs
   `symforge daemon`) AND an operator's systemd/Task Scheduler `symforge daemon`
   service — flows through the same entry, so a long-lived service now silently
   exits after 10 quiet minutes. Without `Restart=always` that is a dead
   service; with it, restart churn. The env var appears only in a source comment
   and untracked research notes; not in README, docs/, or repo CLAUDE.md.
   Precedent cuts both ways: sibling knobs (`SYMFORGE_SESSION_TTL_SECS`,
   autospawn kill-switch) are also source-only, but those are tuning knobs —
   this changes when the process *exists*, which is exactly the class of
   behavior an operator configures around. Fix: one paragraph in
   README/CLAUDE.md naming the env var, the 600 s default, the 60 s clamp,
   `0` disables — and release-notes visibility (conventional-commit body).

2. **[src/daemon.rs:4820-4854] The only new test proves the parser, not the
   feature.** `daemon_idle_shutdown_env_parsing` covers unset/0/sub-minimum/
   garbage — good, and it passes (re-ran: 1 passed). Nothing proves: the reaper
   fires the notification after the boundary; a heartbeat suppresses it;
   unauthenticated requests do NOT suppress it; graceful path + owner-checked
   cleanup runs. The core claim — "an idle daemon shuts itself down" — has zero
   behavioral evidence. The existing harness already spins real daemons over
   HTTP (e.g. `test_spawn_daemon_serves_project_and_session_endpoints`, :6506),
   so a behavioral test is feasible: backdate `last_activity_at`, wait a reaper
   tick, observe `state.idle_shutdown.notified()` resolving; plus one showing an
   authenticated request resets the window.

### Warnings (required, non-blocking individually)

3. **[src/daemon.rs:3046-3047] Wall-clock epoch for an idle-duration guard.**
   Backward clock steps are safe (`saturating_sub` → 0), but forward steps and —
   the realistic case on a Windows dev box — **sleep/hibernate resume** make
   `idle_ms` jump past the window instantly, shutting down a daemon whose client
   is alive. The 15 s heartbeat races the reaper tick on resume; whichever wins
   decides. Consequence is soft (the client reconnect seam respawns the daemon
   in milliseconds), so warning, not critical — but `Instant`-derived millis
   from a process-start anchor is the correct clock for "duration since last
   event" and a trivial swap. `Instant` on Windows does not advance during
   sleep, which is arguably the desired semantics (a suspended machine isn't
   "idle").

4. **[src/daemon.rs:3121-3128 + :3007-3015] Mid-flight shutdown / lost cleanup
   on long requests.** Activity is stamped at request **start**
   (`authorize_daemon_request`, :554-558). A single authenticated request
   outliving the idle window (large `index_folder` from a direct HTTP client
   with no heartbeat loop) lets the reaper fire mid-flight; axum's graceful
   shutdown would wait for it, but `run_daemon_until_shutdown` only waits 5 s
   for `server_task` then returns → process exits, killing the request AND
   skipping the owner-checked `cleanup_daemon_runtime_files_if_owner` (:3015),
   leaving stale port/pid/token files. Mitigations that make this narrow in
   practice: the stdio proxy heartbeats every 15 s authenticated
   (src/main.rs:267-273 → `POST /v1/sessions/{id}/heartbeat` through
   `authorize_daemon_request`, verified :3601-3608, :2002-2013); hook fallback
   requests have a 500 ms budget (src/cli/hook.rs:991-992); the governor bounds
   tool-call duration. The 5-s-then-abandon exit is shared with the pre-existing
   Ctrl+C path — idle shutdown merely automates its trigger. Cheapest
   hardening: stamp activity at request **completion** too, or drain the
   governor before notifying.

### Suggestions

5. **Nit [src/daemon.rs:4824-4825]:** duplicated comment line
   `// Default: enabled at DEFAULT_DAEMON_IDLE_SHUTDOWN_SECS.`
6. **Consider [src/daemon.rs:2508-2515]:** the narrower design — default-on only
   for auto-spawned daemons — is one line in `spawn_daemon_process`
   (`command.env(DAEMON_IDLE_SHUTDOWN_ENV, "600")`) plus a disabled default. It
   confines the new lifecycle to exactly the orphan class it targets and keeps
   explicit services backward-compatible. If default-on everywhere is intended,
   finding 1's documentation becomes even more load-bearing.
7. **FYI:** garbage env values silently fall back to 600 s (enabled) rather than
   warning. Consistent with `session_ttl_from_env`, so acceptable; a
   `tracing::warn!` would be kinder.

### What is done well

- The `notify_one` choice is correct and race-free: tokio `Notify::notify_one`
  stores a permit when no waiter is registered, so a reaper firing before
  `notified().await` is not lost. The `idle_fired` latch (:3036, re-armed
  :3057-3059) prevents permit spam while keeping the reaper alive for embedders.
- Stamping activity **only inside the constant-time auth success branch**
  (:551-558) is the right shape: unauthenticated probes cannot keep a zombie
  warm; exactly one place records activity.
- Routing idle shutdown through the *same* select as SIGINT/SIGTERM means one
  shutdown path, not a second one to keep correct.

## Mechanism verification (anchors)

- Activity init/update: `DaemonState::with_token` seeds `now_epoch_millis()`
  (:698-699); updated only on auth success (:554-558). Failed auth cannot update
  it. Confirmed.
- Heartbeat: src/main.rs:267-273, 15 s loop, authenticated POST, errors
  swallowed (`let _ =`), task aborted on proxy exit (:325). A live proxy keeps
  the daemon warm; a dead proxy stops heartbeating and the daemon dies
  ≤ ~600 s + one reaper tick later.
- Parser: unset→600 s, `0`→None, `5`→60 s, `"soon"`→600 s. Verified by reading
  and by running the test.
- Reaper: interval tightened to ≥4 sweeps per idle window (:3027-3030),
  `MissedTickBehavior::Delay`; tokio `interval` fires its first tick
  immediately, but `idle_ms` ≈ 0 at startup so no spurious fire.
- Startup modes: production daemons exist only via `symforge daemon` (explicit
  or detached auto-spawn at :2510) → `run_daemon_until_shutdown` → observes the
  notification. Other `spawn_daemon` callers are tests/embedders, which the
  reaper deliberately tolerates (keeps looping after firing). Confirmed.
- Graceful path: reaper abort → oneshot → axum `with_graceful_shutdown` (waits
  for in-flight) → owner-checked cleanup inside `server_task` (:3015) — but only
  if the task finishes within the outer 5 s timeout (finding 4).

## Product-fit judgment (pre-flagged question)

The feature materially mitigates exactly one orphan class: a **daemon** whose
last client (proxy or direct) is gone — previously immortal, now dead within
~10 minutes. It does not and cannot reap leaked **proxy stacks** whose stdin is
still held by a live client: those heartbeat and legitimately look alive to the
daemon; that causal boundary is correctly stated in the handoff, not hidden.
Verdict on belonging: yes — the daemon is SymForge's own detached process,
self-limiting its lifetime is the right layer for this defense, and the
2026-07 leak observations in this repo show the orphan-daemon class is real. It
is a partial fix honestly scoped, which is fine; a partial fix sold as the
whole fix would not be.

## Verdict

**CHANGES_REQUIRED**

**Minimum change set:**
1. Operator documentation: env var, 600 s default, 60 s clamp, `0` disables, and
   that explicitly managed daemons are affected (README and/or repo CLAUDE.md;
   release-notes-visible commit body). Decide explicitly — and record — whether
   default-on for explicit services is intended (finding 6 is the one-line
   alternative).
2. At least one behavioral test: idle boundary fires `idle_shutdown` (backdate
   `last_activity_at`, observe `notified()`), and an authenticated
   request/heartbeat resets the window. Fix the duplicated comment while there.
3. Recommended (may be a ticketed follow-up): monotonic clock (finding 3) and
   completion-time activity stamp (finding 4).

**Verification gate before commit:** `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, full
`cargo test --all-targets -- --test-threads=1` (only the daemon lib subset has
been run; the daemon HTTP integration tests in `tests/` share this file and
have not been exercised against the diff), plus one manual smoke: start
`symforge daemon` with `SYMFORGE_DAEMON_IDLE_SHUTDOWN_SECS=60`, confirm exit
~60-75 s after the last authenticated call and that the daemon runtime files
(port/pid/token) are gone.

**Verification performed for this review:** full `git diff`; targeted
`cargo test --lib daemon::tests::daemon_idle_shutdown_env_parsing` (pass);
SymForge-indexed inspection of `run_daemon_until_shutdown`, the
`spawn_daemon`/reaper block, `spawn_daemon_process`, the
`authorize_daemon_request` stamp, heartbeat route/handler/client,
`try_daemon_fallback`; repo-wide doc search for the env var (hits only in
source + untracked notes). Full suite not run per checkpoint protocol.
