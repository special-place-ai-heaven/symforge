# Implementation Plan: Preventive Lifecycle Activation Cut (Feature 020 Slice 4)

**Branch**: `feature-020-slice-4-candidates` | **Date**: 2026-08-18 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/028-preventive-activation-cut/spec.md`

## Summary

Enable the dark-landed preventive index lifecycle everywhere in one indivisible
activation cut (frozen Feature 020 tasks T053–T073). Delivery is two waves:
**Wave 1** lands five behavior-neutral RED+machinery pairs as five separate PRs
onto `main` (each extending the darkness seal and retirement census, CI green,
one review pass with a mandatory cfg-lens sweep, auto-merged); **Wave 2** ships
the indivisible cut — ingress rerouting, the activation mode machine, the
244-member V10 retirement with V11 API exposure, and the performance/capacity/
equivalence gates — as one PR that merges only on explicit operator approval
after multi-round adversarial review.

## Technical Context

**Language/Version**: Rust, stable toolchain (repo-pinned via CI; edition per `Cargo.toml`)

**Primary Dependencies**: rmcp 3.1.0 (MCP server), tree-sitter grammar set,
cap-std (physical-root confinement), notify (watcher), vendored libgit2,
bundled sqlite. New in this slice: `criterion` dev-dependency (T068,
contract-mandated registration shape — see research.md R6).

**Storage**: in-process live index + on-disk snapshots under `.symforge/`
(V11 namespace `.symforge/v11/`, quarantine under
`.symforge/quarantine/index-snapshots/`); no external database.

**Testing**: `cargo test --all-targets -- --test-threads=1` (serial, via
Terminal Commander for anything that can exceed 10 minutes);
`cargo test --no-default-features --features embed --lib` (the feature-gate
blind-spot gate); `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`;
`node scripts/validate-lifecycle-oracle-traceability.cjs`;
`node scripts/verify-tools.cjs --bin target/release/symforge`.

**Target Platform**: Windows (dev), Linux + macOS (CI executors). Linux CI is
the first executor of every `cfg(unix)` body — treated as unverified until it
runs there.

**Project Type**: Rust MCP server (lib + bin + embed feature), single crate.

**Performance Goals**: `ObservedRefreshGateV1` — completed write burst to
first strict lease carrying that byte identity: p95 ≤ 2 s, max ≤ 5 s,
p95 ≤ 1.25× baseline `1521abb0`; no single-path full rebuild outside
Gap/ScopeDirty.

**Constraints**: darkness invariant until the cut (call edges, not grep hits —
sealed by `tests/preventive_runtime_dark_v11.rs` + `FULL_SOURCE_PIN_V1`);
frozen 020 tree byte-immutable; two publication roots never simultaneously
authoritative; serial cargo discipline; Bash tool 10-minute ceiling → Terminal
Commander for long gates.

**Scale/Scope**: 6 new lifecycle modules, 5 new test suites + 1 bench suite,
244 retirement members across 13 frozen categories, 64 V11 embed atoms,
26 configuration cells, ~8 ingress surfaces rerouted (watcher, sidecar, init,
hook, edit, live_index, daemon, embed).

## Constitution Check

*GATE: `.specify/memory/constitution.md` is an unfilled template — no speckit
constitution exists. The project's de-facto constitution is the binding rule
set in `CLAUDE.md` (repo) and the frozen 020 governance. Gates checked:*

| Gate | Status |
|---|---|
| Reporting invariant — no component reports success it did not observe | PASS (design: every promotion/receipt is observation-backed; RED oracles assert refusal paths) |
| Frozen-tree immutability — no edit under `specs/020-*/` | PASS (spec FR-013, SC-008; all artifacts live in `specs/028-*/` + `docs/reviews/`) |
| RED → observed failure → minimal GREEN | PASS (spec FR-003; wave structure enforces pair-wise RED-first) |
| CI gates unchanged (fmt, clippy -D warnings, serial suite, release build, npm, embed-build) | PASS (no CI edits planned; embed-build gate explicitly in every pair's checklist) |
| Build discipline (TC for >10 min, serial cargo, clean-up) | PASS (recorded in research.md R7; quickstart encodes the commands) |
| Docs hygiene (no hand-written volatile state; evidence under `docs/reviews/`) | PASS (evidence docs generated from receipts; campaign-state script for git facts) |

No violations → Complexity Tracking left empty.

*Re-check after Phase 1 design: PASS — no design artifact introduces a new
project, dependency (beyond contract-mandated criterion), or surface.*

## Project Structure

### Documentation (this feature)

```text
specs/028-preventive-activation-cut/
├── spec.md              # Execution spec (written, clarified)
├── plan.md              # This file
├── research.md          # Phase 0 — verified facts + campaign-doc correction
├── data-model.md        # Phase 1 — lifecycle entities and state machines
├── quickstart.md        # Phase 1 — per-pair and cut validation runbook
├── contracts/           # Phase 1 — binding-by-reference to frozen 020 contracts
│   └── README.md
├── checklists/
│   └── requirements.md  # Spec quality checklist (16/16)
└── tasks.md             # Phase 2 output (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
src/index_lifecycle/
├── supervisor.rs        # NEW (T059) — loader ownership, cancellation, attempt accounting
├── candidate.rs         # NEW (T060) — isolated full/delta candidates, one commit point
├── observer.rs          # NEW (T061) — coalescing accumulator, cuts, latches, handoff
├── verification.rs      # NEW (T062) — rolling verification, 15-min deadline, receipts
├── query.rs             # NEW (T063) — strict leases, selections, render authority
├── activation.rs        # NEW (T066) — LegacyOpen → LegacyClosing → PreventiveV1Open
└── (existing 12 modules adapted, not rebuilt — see research.md R2)

src/                     # Wave 2 ingress rerouting (T064) + retirement (T067)
├── watcher/mod.rs, sidecar/, cli/init.rs, cli/hook.rs,
├── protocol/edit.rs, protocol/edit_hooks.rs, live_index/ (incl. persist.rs V11 bump, T065)
└── daemon.rs, main.rs, embed.rs, lib.rs (T067 retirement/exposure)

tests/
├── index_candidate_lifecycle_v11.rs   # NEW (T053)
├── observer_handoff_v11.rs            # NEW (T054)
├── rolling_verification_v11.rs        # NEW (T055)
├── project_query_lease_v11.rs         # NEW (T056)
├── snapshot_v11_migration.rs          # NEW (T057)
├── delta_full_rebuild_equivalence_v11.rs  # NEW (T071)
├── activation_cut_v11.rs              # EXISTS — T058 stand-ins go live at the cut
├── process_capacity_pool_v11.rs       # EXISTS — T069 adds the conservation oracle
└── fixtures/observed-refresh-v1/      # NEW (T068) — corpora + digests

benches/
└── observed_refresh_gate_v1.rs        # NEW (T068) — criterion group per frozen registration
```

**Structure Decision**: single-crate layout unchanged; all new lifecycle code
stays inside `src/index_lifecycle/` behind the existing `#[path]` mount so the
darkness seal continues to govern reachability until T066/T067 flip the
pre-plotted keywords (`server_api` `pub(crate)`→`pub`, mount → private
`lib.rs` mod with `embed.rs` re-exports).

## Execution structure (waves and pairs)

| Unit | Content | Lands as |
|---|---|---|
| W1-P1 | T053 RED (candidate promotion matrix, opaque path) + T059 supervisor + T060 candidate | PR 1 |
| W1-P2 | T055 RED (rolling verification) + T062 verification | PR 2 |
| W1-P3 | T056 RED (strict query leases, health split) + T063 query wiring (dark) | PR 3 |
| W1-P4 | T054 RED (observer handoff) + T061 observer | PR 4 |
| W1-P5 | T057 RED (snapshot migration) + T065 snapshot V11 (dark) | PR 5 |
| W2 | T058 stand-ins live + T064 ingress rerouting + T066 activation machine + T067 retirement/exposure + T068–T071 gates + T072 campaign + T073 migration docs | one cut PR, operator-approved |

Pairs are `[P]`-parallel in the frozen roster but shared-file edits (mod.rs,
seal pins, census) serialize; the order above matches tasks.md (candidate
pipeline first, then spec priority order: US2 verification, US3 query,
US4 observer, US5 snapshot).

## Complexity Tracking

No constitution violations to justify.
