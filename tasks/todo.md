# SFB01 - Mitigate Windows libgit2 lockfile flakes in git-heavy tests

## Plan

- [x] Run branch guard and move work to `backlog-implementation`.
- [x] Mark SFB01 in progress.
- [x] Identify git-heavy tests/helpers that create many commits.
- [x] Confirm whether the flaky path is isolated to test setup.
- [x] Add a bounded helper-level mitigation preserving the final git error.
- [x] Add regression or stress coverage documenting the Windows lockfile race mitigation.
- [x] Verify no `#[ignore]` workaround was introduced.
- [x] Run required verification commands.
- [ ] Commit verified implementation work.
- [ ] Mark SFB01 completed and commit goal status.

## Review

- Before: duplicated frecency, persist, co-change, and ranking test fixtures called
  `repo.commit(Some("HEAD"), ...)` directly while creating many commits.
- After: `src/git/test_helpers.rs::commit_head_with_retry` wraps only the test
  `HEAD` ref update in bounded retry/backoff and preserves the final git error.
- Regression coverage: `tests/git_commit_retry.rs` exercises retry success and
  final-error preservation for a Windows-style `.git/refs/heads/*.lock` failure.
- Scope check: production git/query semantics were not changed; rewrites are
  test fixture paths and inline unit-test helpers only.
- Ignore check: SymForge search found only pre-existing ignored tests in
  `tests/live_index_integration.rs` and `tests/coupling_calibration.rs`.
- Verification passed: `cargo fmt --check`, `cargo check`,
  `cargo test --all-targets -- --test-threads=1`, `cargo test --all-targets`,
  `rg "#\\[ignore\\]" tests src`, `git branch --show-current`,
  `git diff --check`, and `cargo build --release`.
