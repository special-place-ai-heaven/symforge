# Implementation Plan: Containment and Durability Hardening

**Branch**: `033-containment-durability` | **Date**: 2026-09-18 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/033-containment-durability/spec.md`

## Summary

Close F-001…F-009 from `docs/reviews/agent-review-2026-09-18.md`. Shared helpers first (`paths` no-follow, `pub(crate)` loopback predicate, `write_bytes_atomic` in idempotency), then independently testable stories in disjoint worktrees. RED-first oracles. T051 pin refresh only on the integration tree after `cargo fmt`. No loopback sidecar Bearer. No catalog-walker or `select_runtime_data_base` work. Do not mix US1 and US2 in one commit.

## Technical Context

**Language/Version**: Rust, workspace `symforge` crate (edition as in root `Cargo.toml`)

**Primary Dependencies**: existing `tempfile`, `windows` 0.62 (add Security features for F-006 only), `axum` sidecar, `cap-std` already on physical-root writes (not for protocol reads)

**Storage**: local `.symforge/` snapshots, control-state token/trust JSON, editor MCP JSON — no new database

**Testing**: `cargo test --lib --bins --tests -- --test-threads=1`; Windows cfg-gated junction/ACL tests; embed cell `cargo test --no-default-features --features embed --lib -- --test-threads=1`

**Target Platform**: Windows (this machine + CI) and Unix CI; junction tests Windows-only

**Project Type**: Rust MCP server + CLI + sidecar HTTP

**Performance Goals**: no regression mandate; containment checks are one `symlink_metadata` per read

**Constraints**: Constitution I–VI; serial cargo; CHECK parallelism is `-j 4`; Terminal Commander for >10 min; do not interleave feature sets in `target/`. CLAUDE.md’s host `-j 8` note is not the campaign CHECK.

**Scale/Scope**: nine review findings; ~10 production files plus tests; no new MCP tools

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | How this plan satisfies it |
|-----------|--------|----------------------------|
| I. Reporting invariant | PASS | Checkpoint/init/trust success only after sync+rename; freshen does not report an indexed file it refused |
| II. RED-first | PASS | Each story’s oracle fails on current code before machinery (`quickstart.md`) |
| III. Frozen contracts | PASS | No edits to `specs/020-repository-knowledge-index/` checkbox bytes; T051 rules unchanged, pin recomputed by oracle |
| IV. Verification gates | PASS | fmt, clippy -D warnings, serial suite, embed cell, `cargo build --release` + `scripts/verify-tools.cjs`, `cd npm && npm test` (T061–T064, T067, T068); long runs via Terminal Commander |
| V. Unrepresentable over checked | PASS | Freshen join-fallback deleted (cannot spell “containment failed but Ok”); `apply_splice` returns Result; bind matrix has no Ok for refused non-loopback |
| VI. Independent review | PASS | Adversarial review after tasks and after each worktree; cfg-lens on Unix `0o600` / Windows DACL / `cfg(windows)` junction tests |

Post-design re-check: same PASS. Complexity tracking empty.

## Project Structure

### Documentation (this feature)

```text
specs/033-containment-durability/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── GATES.md
├── contracts/
│   ├── no-follow-read.md
│   ├── durable-persist.md
│   └── sidecar-bind.md
├── checklists/
│   └── requirements.md
└── tasks.md                 # /speckit-tasks — not this command
```

### Source Code (repository root)

```text
src/paths.rs                         # no-follow helper (foundational)
src/idempotency.rs                   # write_bytes_atomic (foundational)
src/daemon.rs                        # pub(crate) loopback predicate (foundational); token DACL (US6)
src/protocol/read_gate.rs            # US1
src/protocol/tools.rs                # US1 freshen helper only
src/sidecar/handlers.rs              # US1 freshen helper only
src/sidecar/server.rs                # US2
src/sidecar/router.rs                # US2 Bearer layer when non-loopback
src/watcher/mod.rs                   # US1 freshen stat
src/live_index/store.rs              # US1 read_pass + path_after
src/discovery/mod.rs                 # US1 scout_single_path metadata only
src/live_index/persist.rs            # US4
src/edit_safety/trust.rs             # US3 hasher + US7 write_store
src/cli/trust.rs                     # US7
src/cli/init.rs                      # US8 production writers
src/protocol/edit.rs                 # US5 apply_splice + call sites
src/protocol/edit_tools.rs           # US5 apply_splice call sites
src/parsing/mod.rs                   # US5 extract_symbols_for_diff
src/index_lifecycle/mod.rs           # US9 docs (plus stale observer.rs / query.rs)
tests/preventive_runtime_dark_v11.rs # pin refresh at integration only
```

**Structure Decision**: Single existing crate. Helpers land in existing modules so worktrees do not fight `internals.rs` / Feature 020 T051 module pins. No new crates. Land 033 as one PR after integration gates.

## Complexity Tracking

> None. No constitution violations.

## Phase 0 / Phase 1 artifacts

- [research.md](./research.md) — decisions 1–8
- [data-model.md](./data-model.md)
- [contracts/](./contracts/)
- [quickstart.md](./quickstart.md)

## Agent context

After this plan: `.specify/extensions/agent-context/scripts/powershell/update-agent-context.ps1` with this plan path (optional `after_plan` hook).
