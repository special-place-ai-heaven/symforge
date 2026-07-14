# Independent review handoff: daemon idle self-shutdown (A019)

Review the current uncommitted daemon idle-shutdown change on branch
`feat/token-speed-tool-trust`. Do not modify product code, tests, git state, Cargo
caches, worktrees, retained benchmark traces, or evidence directories. This is a
read-only review checkpoint.

Write the final review to:

`research/token-cost/claude-report-daemon-idle-review-a019.md`

Use SymForge first for Rust code inspection. Direct reads are appropriate for
the exact git diff and repository documentation when needed. Do not print or
copy any credential or secret value; report only variable names and locations.

## Candidate claim to audit

The uncommitted change in `src/daemon.rs` adds daemon idle self-shutdown:

- `DaemonState` records the time of every successfully authenticated request
  and owns an idle-shutdown notification.
- The existing session reaper checks elapsed idle time and requests graceful
  shutdown after `SYMFORGE_DAEMON_IDLE_SHUTDOWN_SECS` (claimed default 3600 s,
  minimum 60 s, `0` disables).
- `run_daemon_until_shutdown` handles that notification through the same path
  as process signals, including owner-checked runtime-file cleanup.
- The stdio proxy is claimed to send an authenticated heartbeat every 15 s, so
  a live proxy should keep the daemon warm.
- Reported verification: `cargo fmt --check`, workspace clippy with warnings
  denied, and all 82 daemon tests passed. The full workspace test suite was not
  run.

The author explicitly states that this does **not** fix duplicate MCP proxy
stacks leaked while Codex remains alive: those proxies retain stdin and keep
heartbeating. Treat that limitation as part of the scope assessment, not as a
hidden fact.

## Required review order

### Phase 1 — independent diff review

Before using the candidate claims or the questions below as conclusions:

1. Inspect the exact uncommitted product-code and test diff.
2. Build your own findings list from the code.
3. Record changed files and distinguish this change from unrelated existing
   branch artifacts.

### Phase 2 — mechanism verification

Verify from source, with exact file/line or symbol anchors:

1. Where activity is initialized and updated, and whether failed or missing
   authentication can update it.
2. The exact heartbeat route, interval, authentication behavior, error handling,
   and proxy shutdown behavior.
3. The idle duration parser for unset, zero, sub-minimum, invalid, and valid
   values.
4. Reaper cadence, clock source, first-tick behavior, one-shot notification
   behavior, and whether notification can be lost.
5. The graceful shutdown sequence, reaper cancellation, server timeout, and
   owner-checked runtime-file cleanup.
6. Whether every daemon startup mode observes the notification, including the
   distinction between detached auto-spawn and an explicitly managed
   `symforge daemon` service.
7. Whether an authenticated request can still be executing when idle shutdown
   fires. Include non-abortable/governed work and direct HTTP clients in the
   analysis; do not assume every caller runs a separate heartbeat loop unless
   source proves it.

### Phase 3 — goal and product-semantics assessment

Judge these independently:

1. Does the change materially mitigate any SymForge-owned orphan process, and
   exactly which one(s)?
2. Does it address the observed duplicate live MCP-stack leak? State the causal
   boundary precisely.
3. Is default-on one-hour shutdown appropriate for both detached demand-cache
   daemons and explicitly managed long-lived services? Consider service-manager
   restart policies and backward compatibility.
4. Would a narrower design—such as default idle shutdown only for auto-spawned
   daemons while explicit service mode stays persistent unless configured—be
   safer and simpler? Treat this as a design question, not a requested answer.
5. Is wall-clock epoch time appropriate for an in-process idle-duration guard,
   or can clock adjustment produce early/late shutdown?

### Phase 4 — test and documentation sufficiency

Inventory the new tests and state what each proves. Specifically determine
whether there is behavioral evidence for:

- notification after the idle boundary;
- authenticated heartbeat preventing notification;
- unauthenticated requests not preventing notification;
- no shutdown while useful work is in flight;
- graceful server termination and runtime-file cleanup;
- disabled mode;
- detached auto-spawn versus explicit service behavior.

Search operator-facing documentation and release notes for the new environment
variable and the default behavior. A source comment is not operator
documentation.

Do not run the full multi-hour suite for this read-only checkpoint. You may run
small, targeted, non-mutating checks only if they materially resolve an
uncertainty. Do not delete the existing SymForge `target/` directory while the
branch is active.

### Phase 5 — verdict

Use exactly one verdict:

- `APPROVE_AS_IS`
- `CHANGES_REQUIRED`
- `REJECT_SCOPE`

Put findings first, ordered by severity, and include concrete code anchors.
Separate blocking findings from non-blocking observations. End with the minimum
change set and verification gate you recommend before the feature is committed.
