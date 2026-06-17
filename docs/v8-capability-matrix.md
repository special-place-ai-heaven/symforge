# v8 capability matrix

**Audience:** an operator or evaluating agent deciding how far to trust SymForge's
shipped surfaces.

**Purpose (FR-017, US6):** map every user-visible capability to an honest proof
state, the assumption that backs it, the claim the shipped surface actually makes,
and — for proven capabilities — the test/artifact that proves it. This is the
public, evidence-led honesty record produced by feature 010 (v8 trust
remediation). It reflects **post-010 reality** (Phases A–E, branch
`feat/010-v8-trust-remediation`), not aspiration.

## How to read this

- **Proof state** is one of:
  - `Implemented` — the code delivers the behavior; an artifact (test/measurement)
    proves it. Evidence is **required**.
  - `Heuristic` — a real computation runs, but the figure is an estimate, never a
    measurement. Honestly labeled on the surface; not "proven", so no artifact is
    required beyond the label test.
  - `Observational` — a wired seam that reports state but whose *premise* (that it
    helps) is not yet proven. A bet under measurement.
  - `Deferred` — an inert seam, present but not active. Honestly labeled; no
    behavior to prove.
- **Assumption ID** points at `docs/stel-assumptions.md`, which is the **single
  source of truth** for every proof verdict. This matrix *references* those IDs;
  it never restates a verdict that contradicts the register. Where a row has no
  register entry (it is a bug-fix or a presentation-layer guarantee, not a
  research assumption), the ID is `n/a` with a note.
- **Surface claim** is what the shipped string an agent reads actually says. By
  the honesty contract (010 US1), a surface claim MUST NOT exceed its proof state.

> **v8 is a bet under test.** The two load-bearing premises — **A-017** (a small
> tool surface improves LLM tool-selection) and **A-011** (the index-based token
> predictor lands within ±20% of actual) — are **OPEN** in the register: cited or
> designed, **not reproduced** on our corpus. Nothing in this matrix presents
> either as a proven win. Relabel ≠ validate: a Phase A label change never
> promoted an OPEN assumption to VALIDATED.

## Matrix

| Feature | Proof state | Assumption ID | Surface claim | Evidence |
|---------|-------------|---------------|---------------|----------|
| Status index health (default deployment) | Implemented | A-019 | `status` reports the index that actually serves queries: `index_ready: true` + matching file count on the default daemon-proxy path | Phase B. `src/daemon.rs::test_status_index_matches_daemon_proxy_after_symforge_serve`; `tests/status_truth.rs::status_index_matches_daemon_after_index_over_http` |
| Guarded edit (`if_match`) | Implemented | n/a (TR-06 data-integrity bug fix; no register assumption) | A guarded `symforge_edit apply` is rejected if the on-disk body diverged before the write; success is reported only when the guard was honored at the write. **Residual:** enforced under a per-path in-process lock — it serializes SymForge's own concurrent writers; a write by an external OS editor between the guard re-read and the rename is the documented out-of-scope window | Phase C. `src/protocol/edit.rs::symforge_edit_concurrent_same_file_apply_never_clobbers` (200-round race, zero double-commits) |
| Honest surfaces / labels | Implemented | n/a (US1 honesty contract; relabel ≠ validate) | No envelope/status field named `saved`/`net`/`active`/`validated`/`pending` presents a constant or gross counter as a measured result; estimates are labeled `est.`/`heuristic`; the session running total is `session_tokens_served`; inert seams read `deferred`/`observational` | Phase A. `tests/surface_honesty.rs` (envelope + status label assertions) |
| Token economics prediction | Heuristic | A-011 | Predicted figures are derived from real bytes for single-file reads (`get_file_context`/`get_file_content`/`get_symbol`) so they vary with file size; every figure is labeled an estimate (`est. chars/4` / `heuristic`). **It is an estimate, never a measurement.** | Phase E. `tests/stel_l2_admission.rs::grounded_predictions_differ_by_real_file_size_end_to_end` (T035); accuracy (±20%) is **A-011 OPEN** — not claimed |
| Adaptive economics gate (degrade / bypass) | Implemented | A-012 (PARTIAL), A-013 (VALIDATED) | A sufficiently small/cheap read reaches a non-serve branch (bypass) on the live planner path, so the gate is no longer parked permanently in `serve` by a constant | Phase E. `tests/stel_l2_admission.rs::grounded_small_file_reaches_bypass_end_to_end` (T036) |
| Recoverable cold start | Implemented | n/a (TR-02/TR-03 recovery + onboarding) | An empty-index error on the default (compact) surface names only callable recovery steps and never a surface-forbidden tool (e.g. never `index_folder`). **Gap:** the OS desktop-launcher CWD/project-discovery path (TR-03) is fixed in code but not yet live-dogfooded against a built Desktop binary | Phase D. `tests/recovery.rs` (compact recovery hint names no blocked tool; full surface may name `index_folder`) |
| Durable ledger restart-survival | Implemented (serve `/mcp`) | n/a (durable-store engineering) | On `symforge serve`, ledger rows are written through to SQLite and survive a process restart (reopen preserves rows) | `src/stel/ledger_store.rs::persist_to_file_and_reopen_preserves_rows`; `tests/stel_l4_ledger.rs::serve_invocation_writes_through_to_durable_store` |
| Durable ledger on the daemon-proxy path | Deferred | n/a (reachability note, Phase B) | The default daemon-proxy path exposes the in-memory ledger only; status labels it `l4_ledger: in_memory`. Durable persistence is a `serve`-only surface — not advertised as available on daemon-proxy | Honestly labeled; `tests/surface_honesty.rs::status_banner_uses_no_blanket_active_or_pending_literal` pins `l4_ledger: in_memory` |
| Compact-3 default surface premise (fewer tools help the LLM) | Observational | A-017 | The default surface is the compact-3 set (`symforge`, `symforge_edit`, `status`); the full 35-tool surface is a documented opt-out (`SYMFORGE_SURFACE=full`). **The premise that this improves LLM tool-selection is a bet under test (A-017 OPEN — cited, not reproduced), never presented as a proven win.** A-019 is VALIDATED only for the narrow L0 A/B (compact-3 beat a meta-tool on our corpus), not for the surface-size premise | A-017 **OPEN**; A-019 **VALIDATED** (L0 choice only) — see register |
| Predictor accuracy (±20%) | Deferred | A-011 | Not claimed on any surface. The predictor is grounded (figures vary with size) but its accuracy band is unmeasured — a bet under test | A-011 **OPEN** in register; no shipped surface asserts accuracy |
| Calibration auto-tune | Deferred | A-016 | `calibration: deferred` (the `CalibrationState` EMA seam is inert — constructed, read nowhere, N-1). Never `pending` (which would imply transient in-flight work) | Phase A. `src/stel/status.rs::DEFERRED_ITEMS` includes `calibration_auto_tune`; `tests/surface_honesty.rs::envelope_calibration_is_deferred_not_pending` |
| Multi-step planner | Deferred | A-009 | Listed in the deferred set. Multi-step plans DO execute in production, but the public `multi_step_planner` capability is deferred; A-009 is **PARTIAL** in the register (no equivalence A/B artifact), not VALIDATED | `src/stel/status.rs::DEFERRED_ITEMS` (`multi_step_planner`); A-009 **PARTIAL** per register (demoted, TR-12) |
| Batch results (`b_results`) | Deferred | n/a | Listed in the deferred set; not advertised as available | `src/stel/status.rs::DEFERRED_ITEMS` (`b_results`) |

## Source-of-truth rule

The register (`docs/stel-assumptions.md`) is authoritative for every proof
verdict. If a row here ever disagrees with the register, the register wins and
this matrix is wrong and must be corrected. Current register verdicts referenced
above: **A-009 PARTIAL**, **A-011 OPEN**, **A-012 PARTIAL**, **A-013 VALIDATED**,
**A-016 OPEN**, **A-017 OPEN**, **A-019 VALIDATED (L0 choice only)**.

## Default surface (for the record)

- **Default:** compact-3 — `symforge`, `symforge_edit`, `status`
  (`src/stel/surface.rs::COMPACT_TOOL_NAMES`, `COMPACT_SURFACE_TOOL_COUNT = 3`).
- **Opt-out:** the full surface of **35** registered tools via
  `SYMFORGE_SURFACE=full` (`src/cli/init.rs::SYMFORGE_TOOL_NAMES` /
  `tests/conformance.rs::EXPECTED_TOOLS`, both 35 entries).
