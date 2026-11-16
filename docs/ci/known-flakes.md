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

### Budgeted hook and loopback binaries on four-core runners

| Field | Value |
| --- | --- |
| Test | `run_hook_initial_daemon_index_not_ready_fails_open_without_retry`, `run_hook_root_conflict_fails_open_without_stale_sidecar_hint`; any test of the binaries below can show it |
| Binary | `hook_subprocess_integration`, and by the same mechanism `sidecar_integration`, `api_keys_store`, `hook_enrichment_integration`, `hook_root_guard` |
| Where | Any runner with fewer than 8 logical CPUs: the self-hosted Linux slots and GitHub-hosted Linux and Windows. Seen in CI runs 43937599200 (Linux `test`) and 43947628456 (`windows-native (2/3)`). |
| Class | TIMING |
| Issue | None filed; the mitigation landed with this row. |
| Signature | The hook logs `no-sidecar` where the test expects `sidecar-error`, or a second `/v1/projects` discovery appears in the mock daemon's routes. Both nextest attempts fail. |
| What held | The hook's 50 ms `HTTP_TIMEOUT` (`src/cli/hook.rs`) expired before the in-process mock server answered, so the hook correctly took its unreachable path. Product behaviour is as designed. |
| Mitigation | `.config/nextest.toml` gave these binaries `threads-required = "num-cpus"`, which reserves only half of the 8-slot pool on four cores. They now use `num-test-threads`. Test bodies are unchanged. |
| Follow-up | Open. A test-controlled hook timeout would remove the race instead of starving it of competitors. |
| Not this row | A failure with the machine otherwise idle, or `sidecar_port_stale` in the log. |

### `test_load_perf_70_files`

| Field | Value |
| --- | --- |
| Test | `test_load_perf_70_files` |
| Binary | `live_index_integration` (`tests/live_index_integration.rs`) |
| Where | `windows-native (1/3)` on a four-core hosted runner, CI run 43947628456. |
| Class | TIMING |
| Issue | None filed; the mitigation landed with this row. |
| Signature | panic `LIDX-05: 70-file load must complete in <500ms, took 507ms`. Both nextest attempts failed. |
| What held | The count assertion (70 files indexed). Only the wall-clock budget failed, by 7 ms. |
| Mitigation | `.config/nextest.toml` selects `binary(live_index_integration) and test(/^test_load_perf_/)` with `threads-required = "num-test-threads"`. The 500 ms budget is unchanged. |
| Follow-up | None while it stays green alone. A load slower than 500 ms with the machine to itself is a product regression. |
| Not this row | A file count other than 70, or a time far above the budget. |
