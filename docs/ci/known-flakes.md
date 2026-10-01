# Known flakes

as_of 2026-10-01. Registry of tests that fail for scheduling or environment
reasons while the product invariant under test still holds. A row is not a
pass. The job stays red until a run is actually green.

## How to classify

Read the panic and the counts before assigning a class.

**TIMING.** A progress or deadline floor failed, and the invariant the test
exists to pin did not. For the ArcSwap stress test that means
`inconsistencies` stayed 0 and readers completed, while `swaps` hit the
`swaps > 0` floor inside the fixed window. The usual shape is suite
contention: other test processes, or the test's own busy readers, keep the
writer off the CPU until the window closes.

**PRODUCT.** The invariant failed, or the same signature fails with the
machine otherwise idle. `inconsistencies > 0`, a torn snapshot, or a panic
inside `update_file` / ArcSwap is PRODUCT. Product failures do not get a
mitigation row. They get a fix.

Unsure means PRODUCT, and Larry looks. TIMING is a claim that the product
invariant held.

## Ownership

| Who | Owns |
| --- | --- |
| JEZ | This registry and nextest / harness policy. |
| Kent | Assertion and stress-window hardening. |
| Larry | Product code, and only after a PRODUCT classification. |
| Grove | Read this file before `gh run rerun --failed`. A listed flake is not a reason to rerun. Rerunning does not land the mitigation or the assertion fix. |

## Rules

- Never fake green. Do not delete an assert, lower a floor, or skip a test to clear a red job.
- No `#[ignore]` unless a tracked issue and a row in this file both exist.
- Do not raise `[profile.default]` `retries` to absorb a stress flake. `retries = 1` is for the Windows loopback 10054 class. Nextest already marks a retried test flaky. A stress failure that fails both attempts stays red.
- A mitigation may isolate the test. It does not change what the test asserts.

## Registry

### `test_arcswap_concurrent_reads_under_writer_pressure`

| Field | Value |
| --- | --- |
| Test | `test_arcswap_concurrent_reads_under_writer_pressure` |
| Binary | `live_index_integration` (`tests/live_index_integration.rs`) |
| Where | `windows-native` (`cargo nextest run`). The Linux `rust` job runs `cargo test --test-threads=1` and does not use this profile. |
| Class | TIMING |
| Issue | https://github.com/special-place-ai-heaven/symforge/issues/752 |
| Signature | panic `writer did not complete any swaps during the stress window (reads=` with `swaps == 0` and a non-zero read count. Both nextest attempts failed this way. |
| What held | The failure is the `swaps > 0` floor. It is not the `inconsistencies == 0` snapshot check. |
| Mitigation | `.config/nextest.toml` selects this test with `binary(live_index_integration) and test(=test_arcswap_concurrent_reads_under_writer_pressure)` and sets `threads-required = "num-test-threads"`, so it reserves the whole nextest pool. `num-cpus` does not, because `[profile.default]` `test-threads` is 8 and a host with fewer logical CPUs leaves the rest of the pool free. Global `retries` stays 1. The test body is unchanged. |
| Follow-up | Open. Kent hardens the writer-progress contract (window, startup barrier, or non-busy readers). Larry changes product code only if that work shows a real `update_file` / ArcSwap regression. |
| Not this row | `inconsistencies > 0`, a panic inside the writer, or zero reader throughput. Those are a different failure. |
