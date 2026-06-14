# Feature Specification: SymForge v8 Phase 2 STEL Controller Maturity

**Feature Branch**: `cursor/v8-phase2-stel-controller` (planned; not started)

**Created**: 2026-06-14

**Status**: Draft — planning only; **no implementation in this feature**

**Baseline**: Phase 1 compact-3 shipped on `main` at **`66742f1`** ([`docs/phase1-stel-checkpoint.md`](../../docs/phase1-stel-checkpoint.md))

**Input**: Extend STEL from truthful compact-3 single-step behavior to Phase 2 controller maturity. Include multi-step L1 planning for the 3 deferred golden rows, hardened L2 admission states, H3/H4 compact-surface battery gates, T2/T3 equivalence spike A-029, and clear boundaries for calibration persistence and B-RESULTS.

## Clarifications

### Session 2026-06-14

- Q: What is the Phase 1 exit state? → A: Compact-3 (`symforge`, `symforge_edit`, `status`) is truthful on `main`; L1 single-step read/edit, L2 economics metadata, L3 P-FF bypass enforcement, L4 in-memory ledger; **29/36** golden rows replay as supported serve or P-FF bypass; **3 multi-hop rows deferred**.
- Q: What gates define Phase 2 exit? → A: Binding docs require **H3, H4, H5** PASS on compact surface battery diff ([`docs/v8-gap-closure-plan.md`](../../docs/v8-gap-closure-plan.md) §7 Phase 2; [`docs/stel-architecture.md`](../../docs/stel-architecture.md) Phase 2 checklist).
- Q: Is calibration persistence in Phase 2? → A: **No** — observational calibration remains in-memory; durable ledger and EMA auto-tuning are Phase 3 boundaries unless explicitly scoped as research-only spikes.
- Q: Is B-RESULTS in Phase 2? → A: **No** — `B-RESULTS` / RESULTS.md §8.7 requires pinned `results-v8-8.0-baseline.json` at **8.0 tag** (A-024); Phase 2 may produce candidate battery artifacts but not claim §8.7 closure.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Close Deferred Multi-Hop Golden Rows (Priority: P1)

As a STEL implementer, I need L1 to emit ordered multi-step plans that replay the three deferred `chain: multi` golden rows, so Phase 1 golden deferrals are closed before claiming Phase 2 trajectory maturity.

**Why this priority**: These are the only remaining golden corpus rows explicitly deferred at Phase 1 exit. Closing them is the smallest product follow-up and unblocks honest H2 trajectory claims for multi-hop tasks.

**Independent Test**: `tests/stel_golden_replay.rs` replays all three rows in `DEFERRED_MULTI_HOP_ROW_IDS` as **SupportedServe** (or documented bypass where golden expects bypass), with `must_call` order preserved and trust envelope + ledger validated.

**Acceptance Scenarios**:

1. **Given** golden row `cfg-if/multi_search_symbol`, **When** `build_plan()` runs with the row query, **Then** the plan contains ordered steps `search_symbols` → `get_symbol` matching `must_call`.
2. **Given** golden row `records/multi_context_refs`, **When** planned and executed on compact surface, **Then** `get_file_context` then `find_references` run in order without forbidden tools.
3. **Given** golden row `is-plain/multi_files_content`, **When** planned and executed, **Then** `search_files` then `get_file_content` run in order.
4. **Given** any multi-hop row, **When** golden replay classifies the row, **Then** it is no longer in `deferred_multi_hop`.

---

### User Story 2 — Harden L2 Admission Controller (Priority: P1)

As a battery reviewer, I need L2 to emit normative `serve | degrade | bypass | cache_hit` decisions with honest economics, so H3/H4 gate computation reflects real controller behavior on compact surface.

**Why this priority**: Phase 1 ships conservative economics and P-FF bypass enforcement but does not claim full admission maturity or H3/H4 PASS.

**Independent Test**: Unit and integration tests prove each admission state; sf-bench rows include required STEL classification fields; compare-results reports H3/H4 on compact battery output.

**Acceptance Scenarios**:

1. **Given** a duplicate target already in session context, **When** L2 evaluates the plan, **Then** decision is `cache_hit` and L3 legacy dispatch is skipped.
2. **Given** predicted net ≤ 0, **When** L2 evaluates a non-P-FF plan, **Then** decision is `bypass` with `StelBypassBody` and no silent serve.
3. **Given** predicted net ≤ margin_low, **When** L2 evaluates, **Then** decision is `degrade` with documented degrade flags (e.g. outline-only, capped tokens).
4. **Given** a P-FF eligible small-file read, **When** L2 detects P-FF, **Then** bypass is enforced at L3 without legacy tool dispatch (Phase 1 behavior preserved).
5. **Given** positive net above margins, **When** L2 evaluates, **Then** decision is `serve` and L3 executes planned step(s).

---

### User Story 3 — Pass H3/H4 Compact-Surface Battery Gates (Priority: P1)

As the release owner, I need a compact-surface sf-bench run that PASSes H3 and H4 in compare-results, so Phase 2 exit criteria from the binding gap-closure plan are met.

**Why this priority**: Phase 2 exit is defined by battery gates, not by feature count.

**Independent Test**: A documented battery artifact (`results-v8-phase2-candidate.json` or equivalent) diffed via compare-results shows H3 PASS (zero sGteM on accepted small-file serve rows per A-012 scope) and H4 PASS (`session_net_accepted ≥ 0`).

**Acceptance Scenarios**:

1. **Given** a compact-surface battery run with STEL fields populated, **When** compare-results computes H3, **Then** no accepted serve row with `*_small` pattern has `sGteM=true` (serve-only H3 scope until two-hop harness lands, per A-012 interim policy).
2. **Given** the same run, **When** H4 is computed, **Then** `session_net_accepted ≥ 0` using accepted-serve rows only (A-026).
3. **Given** golden `chain=single` rows, **When** H5 is computed, **Then** external MCP calls ≤ 1 per task (internal multi-step chain still one MCP call).
4. **Given** Phase 2 is not complete, **When** a reviewer inspects claims, **Then** no document claims H5/H6/H7/H8 PASS unless separately evidenced.

---

### User Story 4 — A-029 T2/T3 Equivalence Spike (Priority: P2)

As a technical reviewer, I need a bounded spike proving T2 reference-task equivalence or a documented bypass-only pivot, so Phase 2 does not silently fail reference-quality rows.

**Why this priority**: Binding plan Phase 2 includes T2 spike start (§6.1); A-029 blocks overstated equivalence claims.

**Independent Test**: Spike artifact in `docs/research/A-029-t2-spike.md` records PASS (≥2/4 equiv on tokio+django T2), PIVOT (P-T2 bypass-only policy registered), or KILL with next research action.

**Acceptance Scenarios**:

1. **Given** sidecar-parity reference fixtures (markdown + benches + imports), **When** spike runs on compact surface, **Then** ≥2/4 T2 tasks achieve equivalence **or** P-T2 pivot is documented with H6 denominator adjustment.
2. **Given** T3 large-file rows, **When** degrade path is exercised, **When** A-014 validation runs, **Then** degrade beats competent-manual window on token cost without equivalence regression (or pivot documented).

---

### User Story 5 — Preserve Phase 3/Post-8.0 Boundaries (Priority: P2)

As a maintainer, I need Phase 2 scope to exclude calibration persistence, auto-tuning, and B-RESULTS closure, so work does not bleed into Phase 3 or premature baseline pinning.

**Why this priority**: Phase 1 explicitly deferred persistence and B-RESULTS; scope creep caused the Phase 1 merge repair cycle.

**Independent Test**: Scope review confirms no SQLite ledger migration, no EMA fudge writing to L2, no `results-v8-8.0-baseline.json` pin, no §8.7 RESULTS closure claims.

**Acceptance Scenarios**:

1. **Given** a proposed Phase 2 task requiring durable ledger storage, **When** scope is checked, **Then** it is rejected or moved to Phase 3 unless research-only (no production path).
2. **Given** a proposed task to auto-tune L2 margins from calibration EMA, **When** scope is checked, **Then** it is deferred to Phase 3 (A-016).
3. **Given** a proposed B-RESULTS / §8.7 update, **When** scope is checked, **Then** it is rejected until 8.0 tag baseline exists (A-024).

### Edge Cases

- Multi-hop plan where step 1 succeeds but step 2 target is missing from index.
- `cache_hit` on partial overlap (same file, different sections requested).
- Degrade path still exceeds small-file H3 budget — must become bypass or further degrade, not silent serve.
- Multi-hop internal chain where intermediate step would have been bypass alone but chain net is positive.
- A-029 spike fails all T2 tasks — must register P-T2 before claiming H6 eligibility unchanged.
- Battery run missing `stel.decision` or `acceptedServe` fields — gate comparator rejects run.
- Attempt to persist ledger across restart during Phase 2 — out of scope unless behind explicit Phase 3 feature flag (not shipped).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Phase 2 MUST extend L1 to produce **ordered multi-step `StelPlan`** values for the three golden rows in `DEFERRED_MULTI_HOP_ROW_IDS`.
- **FR-002**: Multi-step execution MUST remain **one external MCP call** to `symforge` on compact surface (H5 preserved); internal step dispatch is in-process.
- **FR-003**: L2 MUST implement normative admission decisions: `serve`, `degrade`, `bypass`, `cache_hit` per [`stel-schema.md`](../../docs/stel-schema.md) controller algorithm.
- **FR-004**: L2 MUST record `StelDecision` fields required for sf-bench row extension (`decision`, predicted/actual economics, degrade flags, bypass body when applicable).
- **FR-005**: L3 MUST honor L2 decisions: skip legacy dispatch on `bypass` and `cache_hit`; apply degrade caps before dispatch on `degrade`; execute all plan steps on `serve`.
- **FR-006**: Phase 2 MUST replay **36/36** golden rows with honest classification: zero `deferred_multi_hop`, zero undeclared `deferred_planner_mismatch`.
- **FR-007**: Phase 2 MUST produce battery evidence that compare-results evaluates **H3 and H4** on compact surface and documents PASS/FAIL.
- **FR-008**: Phase 2 SHOULD produce battery evidence for **H5** on compact surface (single-hop external call invariant).
- **FR-009**: Phase 2 MUST execute A-029 spike and record PASS, PIVOT (P-T2), or KILL in research artifacts.
- **FR-010**: Phase 2 MUST validate or document A-011 (token estimate ±20%), A-012 (bypass/H3 scope), A-013 (cache_hit savings), A-014 (degrade on T3 large) with linked artifacts.
- **FR-011**: Phase 2 MUST NOT implement durable ledger persistence, calibration EMA → L2 fudge, or SQLite migrations (Phase 3 — S7).
- **FR-012**: Phase 2 MUST NOT pin `results-v8-8.0-baseline.json` or claim B-RESULTS / RESULTS.md §8.7 closure (A-024).
- **FR-013**: Phase 2 MUST NOT expand compact-3 tool surface beyond `symforge`, `symforge_edit`, `status` without explicit L0 pivot evidence.
- **FR-014**: Phase 2 MUST preserve Phase 1 guarded `symforge_edit` apply semantics; multi-file edit apply remains out of scope.
- **FR-015**: Phase 2 MUST update assumption register (`docs/stel-assumptions.md`) for A-008..A-014 and A-029 verdicts.
- **FR-016**: Phase 2 MUST update [`docs/phase1-stel-checkpoint.md`](../../docs/phase1-stel-checkpoint.md) or successor Phase 2 checkpoint doc when exit criteria are met.
- **FR-017**: Phase 2 implementation MUST stay behind a milestone branch until H3/H4 evidence is reviewer-ready; no merge to `main` without CI green and gate artifact links.

### Key Entities

- **MultiStepStelPlan**: Ordered list of plan steps with shared plan id, confidence, and per-step tool/args; consumed by L2 as a unit.
- **StelDecision**: Admission outcome with economics, degrade flags, optional bypass/cache bodies (existing schema — hardened in Phase 2).
- **GoldenReplayClassification**: Per-row category including formerly deferred multi-hop rows.
- **BatteryRowSTEL**: sf-bench row extension with `stel.plan_id`, `stel.decision`, tools called, token economics.
- **Phase2GateReport**: compare-results output focusing on H3, H4, H5 with PASS/FAIL and diagnostics.
- **A029SpikeRecord**: T2/T3 spike methods, repos, equivalence counts, pivot policy if any.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All **3** `DEFERRED_MULTI_HOP_ROW_IDS` replay as supported serve (or golden-expected bypass) in `tests/stel_golden_replay.rs`.
- **SC-002**: Golden corpus classification reports **0** deferred multi-hop rows at Phase 2 exit.
- **SC-003**: L2 unit/integration tests cover all four admission states (`serve`, `degrade`, `bypass`, `cache_hit`) with negative cases.
- **SC-004**: Compact-surface battery candidate shows **H3 PASS** under documented A-012 serve-only scope (or updated policy if two-hop lands).
- **SC-005**: Compact-surface battery candidate shows **H4 PASS** (`session_net_accepted ≥ 0`).
- **SC-006**: Compact-surface battery candidate shows **H5 PASS** for `chain=single` golden rows (external MCP calls ≤ 1).
- **SC-007**: A-029 spike artifact exists with PASS (≥2/4 T2 equiv) or documented P-T2 pivot.
- **SC-008**: Assumption register updated: A-008..A-014 and A-029 each VALIDATED, OPEN with blocker, or INVALIDATED with pivot/kill.
- **SC-009**: Zero production code paths persist ledger or calibration state across process restart in Phase 2 scope.
- **SC-010**: Zero docs claim B-RESULTS / §8.7 closure or 8.0 baseline pin from Phase 2 work alone.
- **SC-011**: Phase 2 checkpoint doc enables independent reviewer to confirm exit in ≤20 minutes from linked artifacts.

## Assumptions

- Phase 1 on `main` at `66742f1` is the implementation baseline; Phase 0 evidence remains valid.
- Binding source for gates: [`docs/v8-gap-closure-plan.md`](../../docs/v8-gap-closure-plan.md) §5.1, §7 Phase 2.
- Normative types and controller algorithm: [`docs/stel-schema.md`](../../docs/stel-schema.md) S5–S6.
- Golden corpus: [`docs/fixtures/routes.golden.jsonl`](../../docs/fixtures/routes.golden.jsonl) (36 rows).
- sf-bench / compare-results may live outside this repo; artifact paths documented in research log.
- H3 interim serve-only scope for bypass rows remains valid until two-hop harness (A-012) ships.
- This feature specifies and plans Phase 2; **implementation is a separate milestone** after spec approval.

## Explicitly Out of Scope (Phase 2)

| Item | Deferred to |
|------|-------------|
| Calibration / ledger persistence (SQLite, snapshot files) | Phase 3 (S7, G-038) |
| EMA auto-tuning → L2 margins (A-016) | Phase 3 |
| B-RESULTS / RESULTS.md §8.7 baseline closure | Post–8.0 tag (A-024) |
| H6/H7/H8 PASS claims | Phase 3–4 per gap plan |
| `symforge serve` / Streamable HTTP (A-020..A-022) | Phase 4 |
| Multi-file `symforge_edit` apply | Future edit scope |
| Admin UI, AAP integration convenience | Phase 4 |

## Recommended Implementation Slices (post-spec)

1. **Slice 1 — Multi-hop L1 + executor chain** (closes SC-001/002)
2. **Slice 2 — L2 admission hardening + cache_hit** (SC-003)
3. **Slice 3 — Battery harness + H3/H4/H5 evidence** (SC-004–006)
4. **Slice 4 — A-029 spike + assumption register updates** (SC-007–008)

Do not start Slice 3 until Slices 1–2 have golden replay green.
