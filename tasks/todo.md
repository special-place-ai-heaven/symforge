# SFB03 - Surface sidecar PID and alive state in health output

## Plan

- [x] Run branch guard and move work to `backlog-implementation`.
- [x] Copy the SFB03 goal file into the worktree.
- [x] Mark SFB03 in progress.
- [x] Locate current health and health_compact formatting.
- [x] Locate existing sidecar port/session state parsing and cleanup tests.
- [x] Write failing tests for full, compact, and stale/dead sidecar output.
- [x] Implement sidecar PID/liveness formatting without changing startup behavior.
- [x] Verify existing port-file roundtrip and cleanup tests still pass.
- [x] Run the goal-specific verification command.
- [x] Run default verification if task-specific verification passes and time permits.
- [x] Commit verified implementation work.
- [x] Mark SFB03 completed and commit goal status.

## Review

- Branch guard passed on `backlog-implementation`.
- Verified work commit: `03bf46fa2515821a040e985dbba16583e923e5c1`.
- Added non-mutating `.symforge/sidecar.*` status reporting with `alive`, `dead`,
  `unknown`, and `none` states.
- Full health sample lines covered by tests:
  `Sidecar: pid=4242 port=<port> state=alive`,
  `Sidecar: pid=4242 port=unknown state=unknown`, and `Sidecar: none`.
- Compact health sample line covered by tests:
  `Sidecar: dead pid=4242 port=0`.
- Existing port-file roundtrip and cleanup tests passed via `cargo test port_file -- --test-threads=1`.
- Goal verification passed: `cargo fmt --check`, `cargo check`,
  `cargo test --all-targets -- --test-threads=1`, and `rg "Sidecar:" src tests`.
- Default verification passed: `git branch --show-current`, `git diff --check`,
  `cargo fmt --check`, `cargo check`, `cargo test --all-targets -- --test-threads=1`,
  and `cargo build --release`.
