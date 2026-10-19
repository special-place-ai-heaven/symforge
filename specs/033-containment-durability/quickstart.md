# Quickstart: 033-containment-durability

Validation guide after implementation. Commands assume repo root, serial cargo (constitution `-j 4`; one cargo invocation at a time).

## Prerequisites

- Feature branch `033-containment-durability`
- Spec, plan, tasks, contracts under `specs/033-containment-durability/`

## RED-first (per story, before machinery)

```text
cargo test --lib --bins --tests -- --test-threads=1 <oracle_filter>
```

Receipt the failure (the new test must fail on current mainline behavior). Then land machinery. Re-run until green.

Suggested filters (must match tasks.md and GATES.md):

| Story | Filter |
|-------|--------|
| US1 | `outbound_link` |
| US2 | `sidecar_bind` |
| US3 | `trust_config_reparse` |
| US4 | `snapshot_sync_all` |
| US5 | `splice_bounds` |
| US6 | `daemon_token_acl` |
| US7 | `trust_store_atomic` |
| US8 | `init_atomic_write` |
| US9 | Feature 020 T051 + parent-doc grep (see GATES.md G-F009) |

## Positive controls

Every negative test file includes one ordinary in-repo file / loopback spawn / valid splice / valid trust hash / Unix 0o600 as applicable.

## Embed trap

If a new helper is not `cfg(test)`:

```text
cargo test --no-default-features --features embed --lib -- --test-threads=1
```

## Integration (after worktrees merge)

Recompute Feature 020 T051 pins with the rust oracle after `cargo fmt`. Then:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib --bins --tests -- --test-threads=1
cargo test --no-default-features --features embed --lib -- --test-threads=1
cargo build --release -j 4
node scripts/verify-tools.cjs --bin target/release/symforge
npm test --prefix npm
```

Long runs: Terminal Commander, constitution `-j 4`, never interleave feature sets in one `target/`.

## Manual symlink smoke (optional)

Create a temp repo, a secret file outside it, `mklink` (or `ln -s`) from an in-repo path to the secret, index, `get_file_content` the link path, confirm the secret is absent. Delete the temp repo.
