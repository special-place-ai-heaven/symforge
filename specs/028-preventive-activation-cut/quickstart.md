# Quickstart: validating Slice 4 work

Runbook for proving each landing, from a Wave 1 pair to the final cut.
Commands assume the repo root (worktree `symforge-slice4` during development).

## Prerequisites

- Rust stable toolchain; Node (for the validator scripts).
- Terminal Commander daemon running — **mandatory** for any cargo invocation
  that can exceed 10 minutes (the Bash tool's ceiling corrupts `target/` on
  kill). Serial cargo discipline: one cargo at a time, `-j 4`,
  `--test-threads=1`.

## Per Wave-1 pair (repeat for each of the five PRs)

1. **RED first** — author the pair's oracle(s) using the exact frozen names
   (research.md R4), run only that test file, and record the observed failure:

   ```
   cargo test --test <new_test_file> -- --test-threads=1
   ```

   Expected: the frozen-named tests FAIL (or panic via `todo!` seams) — capture
   the output as the RED receipt before building machinery.

2. **Minimal GREEN** — build the pair's machinery inside
   `src/index_lifecycle/`, re-run the same command. Expected: pass, with the
   machinery still unreachable from production entry points.

3. **Darkness seal** — extend `tests/preventive_runtime_dark_v11.rs`
   (constructor-unreachability + census) to cover the new module, then refresh
   `FULL_SOURCE_PIN_V1` **using the Rust oracle only** (run the seal test,
   take the actual-vs-pinned mismatch it prints, update the pin). Never trust
   an out-of-band recompute that was not validated against the oracle on a
   known tree (research.md R7).

4. **Traceability** — `node scripts/validate-lifecycle-oracle-traceability.cjs`
   → must pass.

5. **Full gates** (via Terminal Commander, serially):

   ```
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test --all-targets -- --test-threads=1
   cargo test --no-default-features --features embed --lib -- --test-threads=1
   cargo build --release
   node scripts/verify-tools.cjs --bin target/release/symforge
   cd npm && npm test
   ```

   The embed-build line is the blind-spot gate: `#[cfg(test)]` helpers whose
   only consumer is `server`-gated must be `#[cfg(all(test, feature = "server"))]`.

6. **Review** — one independent code-review pass including the cfg-lens sweep
   (every `cfg(unix)`/`cfg`-gated body is an unverified claim until Linux CI
   executes it). Then PR, CI green, auto-merge (spec FR-015), and
   `cargo clean` if the local session was heavy.

## Wave 2 cut (single PR)

All of the above, plus:

- T058 stand-ins go live: remove the four `#[ignore]` attributes at
  `tests/activation_cut_v11.rs:2120–2154` and give the bodies real
  observations in the same change as the seams they observe.
- The T050 matrix: `cargo test --test activation_cut_v11 -- --test-threads=1`
  must pass `all_ingress_uses_exact_typed_authority_branch` with every ingress
  lane resolving to exactly one typed branch.
- Performance gate:

  ```
  cargo bench --bench observed_refresh_gate_v1
  ```

  against baseline `1521abb0` and the candidate; record p95 ≤ 2 s, max ≤ 5 s,
  p95 ≤ 1.25× baseline in `docs/reviews/OBSERVED-REFRESH-GATE-v1.md`.
- Equivalence: `cargo test --test delta_full_rebuild_equivalence_v11` — every
  advertised edit class matches a clean full rebuild.
- Campaign + evidence: run the T072 activation campaign and close
  `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` with the
  multi-round adversarial review; migration docs in
  `docs/migrations/v11-index-lifecycle.md` (T073).
- **Do not merge without explicit operator approval** (spec FR-015).

## End-to-end acceptance spot-checks (per user story)

- **US1**: make one file unreadable mid-refresh in a scratch project; sibling
  queries stay current; the source's promotion is blocked with a typed cause.
- **US2**: query `health` during induced retries; committed vs attempt
  ledgers are separate fields.
- **US3**: query with one non-Current source selected; response is a typed
  refusal naming it.
- **US4**: burst-edit a scratch repo; first strict lease with the new byte
  identity within gate bounds.
- **US5**: seed `.symforge/` with a digest-mismatched V10 snapshot; restart
  quarantines it under `.symforge/v11/` and rebuilds.

## Frozen-tree guard (every landing)

```
git diff --stat <merge-base> HEAD -- specs/020-repository-knowledge-index/
```

Expected output: empty (spec SC-008).
