# Independent follow-up review: corrected daemon idle shutdown (A019)

Review the current uncommitted daemon idle-shutdown correction on branch
`feat/token-speed-tool-trust`. This is a read-only checkpoint. Do not modify
product code, tests, documentation, git state, Cargo caches, worktrees, retained
benchmark traces, or evidence directories.

Write the final report to:

`research/token-cost/claude-report-daemon-idle-followup-a019.md`

Use SymForge first for Rust code inspection. Direct reads are appropriate for
the exact git diff and documentation. Never print or copy secret values; report
only variable names and locations.

## Context

The first independent report is:

`research/token-cost/claude-report-daemon-idle-review-a019.md`

It returned `CHANGES_REQUIRED` because:

1. idle shutdown defaulted on for both detached auto-spawn and explicitly
   managed daemons, without operator documentation; and
2. the only new test covered environment parsing rather than the reaper,
   authenticated activity, and notification behavior.

The user then explicitly selected this policy:

- detached auto-spawn defaults to 600 seconds;
- an explicit `symforge daemon` remains persistent when the variable is unset;
- an explicit environment value still configures both modes;
- nonzero values clamp to 60 seconds minimum; `0` disables.

## Candidate correction to audit

The primary agent claims the minimum correction is now present:

- `daemon_idle_shutdown_from_env` returns `None` when the environment
  variable is unset.
- `spawn_daemon_process` injects the 600-second value only when the parent
  environment did not define `SYMFORGE_DAEMON_IDLE_SHUTDOWN_SECS`, preserving
  explicit operator values including `0`.
- README's Environment table documents the startup-mode distinction, default,
  clamp, and disable value.
- A paused-time real-daemon test exercises the reaper task and sends an
  authenticated HTTP request. It proves the authenticated request defers the
  first idle sweep and a subsequent stale interval produces the
  `idle_shutdown` notification.
- The duplicate test comment was removed.
- Tokio's already-installed dependency enables its `test-util` feature solely
  to keep the behavior test deterministic and fast; no dependency was added.

The prior wall-clock and long direct-request findings were deliberately not
expanded in this patch. Under the selected default policy, detached auto-spawn
has a live stdio proxy heartbeat during useful work, while explicit/direct
service mode has no idle shutdown unless the operator opts in. Judge whether
that is a sound bounded deferral; do not assume it is.

## Required review order

### Phase 1 — blind current-diff review

Before reading the first report's conclusions in detail:

1. Inspect the exact uncommitted diff for `src/daemon.rs`, `Cargo.toml`, and
   `README.md`.
2. Build your own findings list.
3. Separate this correction from unrelated branch artifacts, including the
   existing token-surface harness and research/task files.

### Phase 2 — previous blocker closure

Read the first report and verify, with exact anchors:

1. Unset behavior for an explicitly launched daemon.
2. Auto-spawn default injection and preservation of an inherited explicit
   value, especially `0`.
3. Parser behavior for unset, zero, sub-minimum, invalid, and valid values.
4. Operator documentation accuracy.
5. Whether the behavior test reaches the real reaper task and real authenticated
   route, rather than merely duplicating implementation logic.
6. Whether paused Tokio time plus wall-clock backdating makes the test
   deterministic on Windows and Unix.
7. Whether task/server cleanup in the test can leak a daemon, reaper, listener,
   runtime file, or environment mutation into sibling tests.
8. Whether enabling Tokio `test-util` is proportionate or creates an
   unnecessary production/runtime consequence.

### Phase 3 — scope and residual-risk judgment

Determine whether the corrected feature:

- bounds an abandoned SymForge-owned detached daemon;
- preserves explicit service compatibility by default;
- honestly does not solve Codex-owned live MCP proxy leakage;
- preserves the existing authenticated-heartbeat behavior;
- leaves the prior wall-clock and in-flight-request warnings acceptably bounded,
  or still needs a blocking fix before commit.

Apply Ponytail/YAGNI discipline: do not request monotonic-clock abstractions,
governor draining, new lifecycle types, or additional configuration unless a
concrete current-path failure makes them necessary.

### Phase 4 — verification receipts

Candidate receipts from the primary session:

- TDD red: focused filter ran two tests; parser expectation failed exactly with
  left `Some(600s)`, right `None`; the new reaper/auth test passed.
- TDD green: focused filter passed 2/2 in 0.04 seconds.
- `cargo fmt --check`: exit 0.
- `cargo clippy --all-targets -- -D warnings`: exit 0, 146.030 seconds.
- `cargo test --all-targets -- --test-threads=1`: exit 0, 1,108.216
  seconds; Terminal Commander emitted no test-failure event.
- `git diff --check`: exit 0; only existing LF→CRLF working-copy warnings.
- Isolated debug-binary smoke with
  `SYMFORGE_DAEMON_IDLE_SHUTDOWN_SECS=60`: authenticated request succeeded,
  daemon exited after 75.0 seconds, and port/pid/token runtime files were
  absent. The ephemeral smoke script and isolated temp directory were removed.

You may rerun the two focused daemon tests, fmt, or another small read-only
check if needed. Do not rerun the full all-targets suite unless the diff itself
gives a concrete reason to distrust the receipt. Do not delete the active
SymForge `target/` directory.

### Phase 5 — verdict

Use exactly one verdict:

- `APPROVE_COMMIT`
- `CHANGES_REQUIRED`
- `REJECT_SCOPE`

Put findings first, ordered by severity, with exact code anchors. State whether
each first-review blocker is closed. Separate blocking findings from optional
follow-ups. If approving, recommend a conventional commit subject/body that
makes the lifecycle behavior release-note-visible.
