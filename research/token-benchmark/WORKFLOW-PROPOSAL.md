# Proposed scheduled workflow (not installed)

This is the workflow proposed for SF-BEN-01. It is **not** under `.github/workflows/`, on purpose: `tests/preventive_runtime_dark_v11.rs` seals `ci.yml` and `release.yml` with `WORKFLOW_FINGERPRINTS`, and its `CARGO_LINES` pins count cargo invocations. Before adding any workflow file, grep `tests/` for the workflow directory and file name; if the seal covers it, the workflow and its fingerprint land in one commit, and the `rust` job is the authority for the new pin. A new file outside the sealed pair may still be covered by a directory-level check; confirm before wiring.

## Shape

- Triggers: weekly (Monday 03:00 UTC) and manual dispatch with the SymForge version and repetition count.
- 24 shard jobs (repo x mode x arm), each about 30 cells, about 2.3 hours at the spec's estimate, under the 6-hour hosted-runner limit. `max-parallel` limits concurrent model traffic.
- Each shard calibrates its repository first and fails if any task is uncalibrated, so no spend happens on a broken oracle.
- The report job runs even when shards fail, aggregates whatever records exist (the report shows the valid fraction, and the rule rejects below 95%), and publishes `report.md` to the job summary and the report, JSON and per-cell file as an artifact.
- Secrets by name only: `ANTHROPIC_API_KEY`. Repository variable: `BENCH_CLAUDE_CODE_VERSION` (pin the client).
- Action references are tags here; pin them to commit SHAs to match the repository's practice before installing.

## YAML

```yaml
name: token-benchmark

on:
  schedule:
    - cron: "0 3 * * 1"
  workflow_dispatch:
    inputs:
      symforge_version:
        description: "npm version of symforge to benchmark"
        default: "latest"
      reps:
        description: "repetitions per cell"
        default: "5"

permissions:
  contents: read

concurrency:
  group: token-benchmark
  cancel-in-progress: false

env:
  BENCH_RUN_ID: bench-${{ github.run_id }}
  BENCH_REPS: ${{ inputs.reps || '5' }}

jobs:
  shard:
    name: ${{ matrix.repo }} / ${{ matrix.mode }} / ${{ matrix.arm }}
    runs-on: ubuntu-latest
    timeout-minutes: 340
    strategy:
      fail-fast: false
      max-parallel: 6
      matrix:
        repo: [flask, cobra, zod, ripgrep]
        mode: ["off", "on"]
        arm: [a, b, c]
    env:
      # The runner points HOME at a per-cell client home, so toolchain homes must be explicit.
      CARGO_TARGET_DIR: ${{ runner.temp }}/cargo-target
      CARGO_HOME: /home/runner/.cargo
      RUSTUP_HOME: /home/runner/.rustup
      GOPATH: /home/runner/go
      GOMODCACHE: /home/runner/go/pkg/mod
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - uses: actions/setup-node@v4
        with:
          node-version: "22"
      - if: matrix.repo == 'cobra'
        uses: actions/setup-go@v5
        with:
          go-version: stable
      - if: matrix.repo == 'ripgrep'
        uses: dtolnay/rust-toolchain@stable
      - name: Install client and SymForge
        run: npm install -g "@anthropic-ai/claude-code@${{ vars.BENCH_CLAUDE_CODE_VERSION }}" "symforge@${{ inputs.symforge_version || 'latest' }}"
      - name: Record versions
        run: |
          claude --version
          symforge --version
      - name: Calibrate oracles
        run: python research/token-benchmark/bench.py calibrate --work "$RUNNER_TEMP/bench" --repo "${{ matrix.repo }}"
      - name: Run shard
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: >
          python research/token-benchmark/bench.py matrix
          --work "$RUNNER_TEMP/bench" --out "$RUNNER_TEMP/out"
          --repo "${{ matrix.repo }}" --mode "${{ matrix.mode }}" --arm "${{ matrix.arm }}"
          --reps "$BENCH_REPS" --run-id "$BENCH_RUN_ID" --seed "${{ github.run_id }}"
      - if: always()
        uses: actions/upload-artifact@v4
        with:
          name: records-${{ matrix.repo }}-${{ matrix.mode }}-${{ matrix.arm }}
          path: ${{ runner.temp }}/out/records
          if-no-files-found: warn
          retention-days: 90

  report:
    needs: shard
    if: always()
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - uses: actions/download-artifact@v4
        with:
          pattern: records-*
          merge-multiple: true
          path: ${{ runner.temp }}/out/records
      - name: Aggregate and apply the decision rule
        run: python research/token-benchmark/bench.py report --out "$RUNNER_TEMP/out" --reps "$BENCH_REPS"
      - name: Publish summary
        run: cat "$RUNNER_TEMP/out/report.md" >> "$GITHUB_STEP_SUMMARY"
      - uses: actions/upload-artifact@v4
        with:
          name: token-benchmark-report-${{ github.run_id }}
          path: |
            ${{ runner.temp }}/out/report.md
            ${{ runner.temp }}/out/report.json
            ${{ runner.temp }}/out/cells.jsonl
          retention-days: 400
```

## Notes

- The `matrix` seed is the workflow run id, so every shard of one run shuffles its own cells reproducibly.
- `calibrate` clones the pins into `$RUNNER_TEMP/bench`; `matrix` reuses those clones.
- The report job's rule outcome is informational in the job summary; it does not fail the workflow. A failed shard shows as a lower valid fraction, which the rule treats as REJECT.
- Cost and wall-time estimates with the arithmetic are in the spec, section "Cost estimate per full run".
