# Implementation Plan: v8 Trust Remediation

**Branch**: `010-v8-trust-remediation` | **Date**: 2026-06-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/010-v8-trust-remediation/spec.md`

**Discovery source of truth**: `docs/reviews/v8-trust-remediation-ledger.md` (TR-01..TR-20
+ panel findings N-1..N-7 + code anchors). Triple-confirmed against live 8.0.0 code
(`feat/010` @ 711ee68). Not re-litigated here.

## Summary

Make SymForge trustworthy to the LLMs that call it without rewriting the v8
architecture. The engine is sound (single in-process index, constant-time auth,
path guards, SQLite ledger, candid assumptions register); the trust debt is in the
**presentation layer** — `status`, the economics envelope, error/recovery text, tool
descriptions, public docs — which overstate what the code delivers, plus **two real
bugs**: `status` reads the empty front-end proxy index instead of the warm daemon
index (reports a working index as empty), and `if_match` guarded-apply is structurally
absent from the write path (silent clobber under concurrency with a success receipt).

**Approach**: six independently-shippable phases sequenced truth → recovery → labels →
safety → measure, every LLM-facing string made true or explicitly labeled
heuristic/observational/deferred. Two design forks are locked: (1) `if_match` is
re-verified against the bytes actually written, **in the same critical section as the
splice** (re-check-at-write, not a second pre-flight); (2) economics is **grounded now**
(clarified 2026-06-17) by wiring the existing byte-grounded estimator
(`format.rs:4925-5029`) into the planner, which legitimately reopens the
degrade/bypass branches. The full verification gate (incl. the network-free embed
build) runs after **each** phase, and three named regression tests are added.

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge`.

**Primary Dependencies**: tree-sitter (parsing); `axum` + `rmcp` (server transport,
`server` feature only); `rusqlite`/SQLite (durable ledger, serve mode); `tokio` (async
runtime). No new dependency is introduced by this feature.

**Storage**: in-process LiveIndex (authoritative read path) + local `.symforge/`
snapshots (warm-start/recovery) + SQLite ledger (`serve` mode, durable write metadata).
No new store. The `status` fix **reuses** the existing daemon index via the existing
proxy channel — it does not add a second index (Constitution Principle I).

**Testing**: `cargo test --all-targets -- --test-threads=1`; golden replay suite; three
new named regressions — `status_index_matches_daemon_proxy_after_symforge_serve`
(TR-01), `compact_surface_index_not_loaded_message_never_mentions_blocked_tools`
(TR-02), `symforge_edit_if_match_rejected_after_concurrent_disk_change` (TR-06, via a
deterministic injected interleave point, **not** a timing sleep).

**Target Platform**: local MCP server, Windows/Linux/macOS, stdio + `symforge serve`
`/mcp` transports.

**Project Type**: MCP code-intelligence server (single Rust project).

**Performance Goals**: none added; this is presentation + correctness. No regression to
index-query latency; the economics grounding reuses an O(bytes) estimator already on the
response path.

**Constraints**: Phase A is **zero behavior change** (relabel only). All changes are
`server`-feature-gated; the `embed` build stays network/server-free
(`cargo check --no-default-features --features embed`). Shared protocol formatters keep
stdio↔serve parity. Regression tests use injected interleave points, not sleeps. No
mutation of real operator configs (fixtures only).

**Scale/Scope**: ~12 source files across `src/stel/` and `src/protocol/` + `src/cli/init.rs`
+ `src/main.rs` + `src/daemon.rs`; docs (README/AGENTS/CLAUDE + new capability matrix +
assumptions register); one CI honesty gate. No architectural surface change on the wire
(compact-3 default stays).

## Constitution Check

*GATE: evaluated against `.specify/memory/constitution.md` v1.0.0 (8 principles).*

| # | Principle | Verdict | Note |
|---|-----------|---------|------|
| I | Local-First In-Process Index | **PASS (strengthened)** | `status` is fixed to read the **same** authoritative daemon index that serves queries — it removes a "two truths" reporting split. No second index introduced. |
| II | MCP-Native Surface | **PASS** | `status`/`health` stay MCP tools; the daemon gains a `status` arm reached via the existing proxy (no chat injection, no client-tool shadowing). Compact-3 wire surface unchanged. |
| III | Trust Envelopes | **PASS (core alignment)** | This feature *is* the trust-envelope honesty work: enumerated subsystem states, heuristic-vs-measured labels, "map orients / tools prove" preserved. |
| IV | Determinism & Recovery | **PASS (strengthened)** | `if_match` enforcement implements IV's "mutations reject stale-state edits"; surface-aware recovery hints support resumability. Best-effort backup is **not** relabeled as transactional. |
| V | Frecency Invariant | **PASS** | No frecency writes; `status`/economics/recovery are read-side surfaces. |
| VI | Embed Isolation (G-045) | **PASS** | Every change is `server`-gated (status proxy, daemon arm, economics planner, init wrapper). `cargo check --no-default-features --features embed` is in the per-phase gate (FR-019). |
| VII | Transport Parity | **PASS** | Honesty labels live in shared formatters → apply to stdio and serve identically; parity covered when formatters are touched. |
| VIII | Verification Before Done | **PASS** | Per-phase full gate + golden replay + three named regression tests (FR-019, SC-007). |

**Result**: no violations. **Complexity Tracking**: empty (nothing to justify).

**Re-check after Phase 1 design**: still PASS — the design artifacts (data-model,
contracts, quickstart) introduce no new index, no new wire tool, no new feature flag, no
chat injection, no frecency write. Recorded at the end of this plan.

## Project Structure

### Documentation (this feature)

```text
specs/010-v8-trust-remediation/
├── plan.md              # This file
├── research.md          # Phase 0: decisions + anchors (discovery already done)
├── data-model.md        # Phase 1: trust entities + their honest state machines
├── quickstart.md        # Phase 1: dogfood + regression run guide
├── contracts/           # Phase 1: the LLM-facing surface contracts
│   ├── status-readout.md
│   ├── economics-envelope.md
│   ├── if-match-guard.md
│   ├── recovery-hint.md
│   ├── capability-matrix.md
│   └── honesty-ci-gate.md
├── checklists/requirements.md   # (from /speckit-specify)
└── tasks.md             # Phase 2 (/speckit-tasks)
```

### Source Code (repository root) — touch map by phase

```text
src/stel/
├── status.rs            # A: enumerated states; B: distinguish disabled/unavailable (N-3)
├── planner.rs           # A: rename est_ fields; E: wire real estimator (TR-04)
├── controller.rs        # E: degrade/bypass/mandatory_degrade reachable (TR-04b, N-2)
├── session.rs           # A: session_tokens_served rename (TR-05)
├── envelope.rs          # A: heuristic-vs-measured labels (TR-05, TR-11)
├── types.rs             # A: CalibrationState relabel (N-1); E: economics types
├── executor.rs          # A: reject-path session figure (TR-11)
├── golden_replay.rs     # E: assert-or-remove expected_equiv (TR-13)
└── ledger_store.rs      # A/B: summary() error surface, disabled vs unavailable (N-3, TR-17)

src/protocol/
├── tools.rs             # B: status_stel_tool proxies daemon (TR-01); A: session line (TR-05)
├── format.rs            # D: empty_index_recovery_hint(profile) (TR-02); E: estimator reuse
├── mod.rs               # B: daemon status arm wiring; compact gate (unchanged chokepoint)
├── edit_apply.rs        # C: if_match re-verify at write (TR-06)
├── edit_planner.rs      # C: thread if_match through plan (TR-06)
├── edit_tools.rs        # C: ReplaceSymbolBodyInput.if_match field (TR-06, N-6 note)
├── edit.rs              # C: critical-section write guard (TR-06)
└── handler.rs           # A: bytes/4 "estimated" labeling note (N-4)

src/daemon.rs            # B: daemon-side status arm (TR-01)
src/cli/init.rs          # D: wrapper CWD + proven env (TR-03); A: doc string (TR-07)
src/main.rs              # D: cold-start root discovery (TR-03)

docs/
├── stel-assumptions.md  # A: demote A-009/A-028; single-source A-005/A-016 (TR-12/13/16)
├── v8-capability-matrix.md   # F: NEW — features → assumption IDs → proof state (TR-09)
└── reviews/...          # ledger (reference)

README.md, AGENTS.md, CLAUDE.md   # F: compact-3 default + 32-tool opt-out (TR-07)
.github/workflows/...     # F: surface-honesty + OPEN-assumption CI gate (FR-018)
tests/                    # B/C/D: three named regressions + status/recovery coverage
```

**Structure Decision**: single Rust crate, no new modules. Changes are localized to the
STEL controller/status/economics layer and the protocol edit/format/status handlers, plus
init/main for cold-start and docs/CI for the public record. No new directory beyond the
new `docs/v8-capability-matrix.md` and the spec's own `contracts/`.

## Phase mapping (user stories → phases → anchors)

| Phase | Story | Pri | Findings | Behavior change? |
|-------|-------|-----|----------|------------------|
| **A** Relabel | US1 | P1 | TR-05, TR-10, TR-11, N-1, N-4 + doc demotes TR-07/12/13/16 | **No** (labels only) |
| **B** Status truth | US2 | P1 | TR-01, N-3, TR-17 | Yes (read path) |
| **C** Edit safety | US3 | P1 | TR-06, N-6 | Yes (write guard) |
| **D** Recovery + onboarding | US4 | P2 | TR-02, TR-03, N-5 | Yes (error text + init) |
| **E** Economics grounding | US5 | P2 | TR-04, TR-04b, TR-13, N-2 | Yes (planner; reopens branches) |
| **F** Matrix + CI | US6 | P2 | TR-07/08/09, FR-017/018 | Docs + CI only |

**Sequencing rule (ledger)**: Phase A ships **before** any README "token-efficient"
language. The two real bugs (TR-01 status, TR-06 if_match) + Phase A are the
highest-leverage quick-win. Phases are independently shippable; each runs the full gate
before the next begins (FR-019).

## Complexity Tracking

No constitution violations. Table intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| — | — | — |
