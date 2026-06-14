# Quickstart: SymForge v8 Phase 2 STEL Controller Maturity

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

This quickstart is for reviewers and implementers **after spec approval**. Phase 2 is **not implemented** at spec time.

## Prerequisites

- `main` at Phase 1 repair tip or later ([`docs/phase1-stel-checkpoint.md`](../../docs/phase1-stel-checkpoint.md))
- Rust toolchain from `rust-toolchain.toml`
- `SYMFORGE_SURFACE=compact` for STEL tests
- sf-bench workspace configured (operator path; see `SYMFORGE_CALIBRATION_REPOS` pattern in CI docs)

## 1. Verify Phase 1 baseline (before any Phase 2 code)

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -- -D warnings
cargo test --all-targets -- --test-threads=1
```

Targeted STEL suites:

```bash
cargo test --test stel_golden_replay --test stel_symforge_edit --test stel_status \
  --test stel_l3_enforcement --test stel_l4_ledger -- --test-threads=1
cargo test stel:: -- --test-threads=1
```

Confirm golden classification shows **3 deferred multi-hop** rows (Phase 2 entry state):

```bash
cargo test --test stel_golden_replay golden_corpus_classification_lists_deferred_rows_explicitly -- --test-threads=1
```

## 2. Implementation branch (when approved)

```bash
git checkout main && git pull
git checkout -b cursor/v8-phase2-stel-controller
```

Follow slice order in [plan.md](./plan.md): P2-S1 multi-hop → P2-S2 executor → P2-S3 L2 → P2-S4 battery → P2-S5 A-029.

## 3. Golden replay exit check (Slice P2-S1)

After multi-hop planner lands:

```bash
cargo test --test stel_golden_replay -- --test-threads=1
```

Expected: **36 rows classified**; **0** in `deferred_multi_hop`; all three rows replay:

- `cfg-if/multi_search_symbol`
- `records/multi_context_refs`
- `is-plain/multi_files_content`

## 4. L2 admission tests (Slice P2-S3)

```bash
cargo test stel::controller -- --test-threads=1
# plus any new tests/stel_l2_admission.rs when added
```

Verify unit coverage for: `serve`, `degrade`, `bypass`, `cache_hit`.

## 5. Compact-surface battery (Slice P2-S4)

Run sf-bench compact surface battery (exact command depends on sf-bench workspace):

```bash
# Example — adjust paths to local sf-bench checkout
# node compare-results.js baseline.json candidate.json --surface compact
```

Record output in `docs/research/phase2-gate-report.md` per [contracts/phase2-gate-evidence-contract.md](./contracts/phase2-gate-evidence-contract.md).

Required PASS for Phase 2 exit: **H3**, **H4**; **H5** strongly recommended.

## 6. A-029 spike (Slice P2-S5)

Document in `docs/research/A-029-t2-spike.md`:

- Repos: tokio + django T2 reference tasks (minimum)
- Equivalence count / 4
- PASS, P-T2 pivot, or KILL

Update `docs/stel-assumptions.md` A-029 verdict.

## 7. Scope stop conditions (do not proceed if violated)

| Action | Verdict |
|--------|---------|
| Add SQLite / ledger persistence | **Stop** — Phase 3 |
| Wire calibration EMA → L2 margins | **Stop** — Phase 3 (A-016) |
| Pin `results-v8-8.0-baseline.json` | **Stop** — 8.0 tag (A-024) |
| Publish B-RESULTS / §8.7 closure | **Stop** — post-8.0 |
| Add 4th compact MCP tool | **Stop** — needs L0 pivot evidence |

## 8. Phase 2 exit doc

When gates PASS, add `docs/phase2-stel-checkpoint.md` (or update phase checkpoint index) linking:

- Main merge commit
- Gate report path
- A-029 artifact
- Assumption register deltas

## Spec Kit paths

```text
specs/002-v8-phase2-stel-controller/
├── spec.md
├── plan.md
├── tasks.md          ← implementation tasks (generated next)
└── contracts/phase2-gate-evidence-contract.md
```

When `/speckit-tasks` runs, wire concrete task IDs to the verification commands above.
