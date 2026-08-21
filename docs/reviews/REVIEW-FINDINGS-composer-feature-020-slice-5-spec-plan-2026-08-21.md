# Review findings — Feature 020 Slice 5 spec+plan (Composer)

**Model**: Composer  
**Date**: 2026-08-21  
**Scope**: `specs/029-mechanical-removal/{spec,plan,research,data-model,quickstart,contracts/*}` vs frozen Phase 7 roster and checker source

---

### MAJOR — R1 is true for the gate but overstates automatic coverage of the manifest atom list

- **Where**: `specs/029-mechanical-removal/research.md:30-33`, `scripts/validate-lifecycle-oracle-traceability.cjs:2032-2034,2063`
- **Claim**: `directPublicAtoms` drops every manifest atom with more than three `::` segments before building the postactivation set, so the lifecycle equality check covers 34 type/module-level atoms today, not all 64 `introduced_v11_atoms` rows (30 associated-method paths are manifest-only for this gate).
- **Why it matters**: Removing a public associated item (e.g. `symforge::embed::Claim::value`) leaves `symforge::embed::Claim` in the derived set, so `ordinaryRetirementLifecycle` can stay green while real public behaviour shrinks. R1’s “cannot remove a public atom at all” is process-true via `RemovalCandidate.visibility = public` but not checker-true for method-level API.
- **Recommended fix**: Narrow R1/plan/C-2 prose to “the checker freezes the 3-segment lifecycle atom set (currently 34 atoms)”; add `python execution/refreeze_v11.py verify-internal --target-ref HEAD` (or the consumer compile-fail suite) to research R6 / quickstart Step 6 for **every** removal landing, not only the T076 embed path.

---

### MINOR — Baseline requires `lifecycle_phase` but no runbook command emits it

- **Where**: `specs/029-mechanical-removal/data-model.md:22,37`, `specs/029-mechanical-removal/quickstart.md:16-27`, `scripts/validate-lifecycle-oracle-traceability.cjs:3609`
- **Claim**: Step 1 runs the traceability checker, which prints only `OK (…)` and never names `preactivation` or `postactivation`, yet `NeutralityBaseline.lifecycle_phase` is mandatory and must be “recorded as observed”.
- **Why it matters**: Two executors can both paste “postactivation” with only a green checker as justification; one may have inferred it. That violates FR-001/FR-013 provenance for the field that bounds the whole slice.
- **Recommended fix**: Add an explicit sub-second probe to quickstart Step 1 (document the `ordinaryRetirementLifecycle` helper or a 10-line node one-liner) and require its stdout in `commands.lifecycle_phase`.

---

### MINOR — Pin arithmetic prose overstates EXCLUDED pin movement

- **Where**: `specs/029-mechanical-removal/research.md:51-52,65-66`, `specs/029-mechanical-removal/contracts/neutrality-bracket-v1.md:96-98`, `tests/preventive_runtime_dark_v11.rs:958-979,1054-1080`
- **Claim**: `EXCLUDED_RUNTIME_SOURCE_PIN_V1` fingerprints only the 20 paths in `EXCLUDED_RUNTIME_SOURCE_PATHS` (`index_lifecycle/**` + `server_api.rs`), not all of `src/`.
- **Why it matters**: A T075 deletion in e.g. `src/daemon.rs` must move `FULL_SOURCE_PIN_V1` but should leave the excluded pin unchanged. C-5’s “counts stay flat across a non-empty removal is a defect” is only true when the removal intersects that pin’s file set; flat excluded counts after a deletion elsewhere are correct.
- **Recommended fix**: Replace research R2’s “move on any `src/` deletion” with “move when the deletion intersects the pin’s file set”; in quickstart Step 6, say excluded pin differs only when an excluded-path file changed.

---

### NIT — Open observation #1 is already cheaply closed on current `main`

- **Where**: `specs/029-mechanical-removal/research.md:191-193`, `specs/029-mechanical-removal/quickstart.md:16-27`
- **Claim**: Deferring lifecycle-phase observation to execution is correct (Principle I), but the tree is **`postactivation` today** (`actual=34`, `pre=83`, `post=34`; checker `OK`).
- **Why it matters**: None for correctness; it confirms the slice’s bounding case (no public atom removal) applies now, not only after a future cut.
- **Recommended fix**: Optional one-line note in research Open observations: “on a postactivation tree at spec authoring time, R1’s no-public-removal bound is already live.”

---

## §5 adjudication (explicit)

**5.1 Bracket before removal (P1)** — **Discipline, not over-engineering.** T074/T077 already require a baseline re-run; FR-003 adds only the RED-first control the frozen roster implied but did not spell out. Given Slice 4’s “confidently wrong banner” history and Principle II, certifying a negative instrument without a positive control would repeat a known failure class. Cost is one deliberate edit discarded before deletion.

**5.2 C-7 empty removal pass** — **Honest scoping, not a blank check.** C-6 plus `DischargedExpectation.evidence` (command + output, not prose) has teeth if the evidence doc is filled per data-model. The failure mode is social (inventing a deletion), and C-7 names the legitimate outcome so FR-011 is not “satisfied” by scope creep. Weak only if the executor treats discharge as a one-liner — that would violate FR-013, not C-7 itself.

---

## Negatives

Checked and found sound:

- **R1 core logic**: `deriveLifecyclePublicAtoms` is source-derived (`derivePublicApiAtoms(sourceMap)` at line 2038, then regex scan for non-embed introduced modules at 2044–2049). Manifest alone does not drive `actual`.
- **Phase on this tree**: `postactivation` with identical 34-atom `actual` and `post` sets (command probe using checker logic; matches `node scripts/validate-lifecycle-oracle-traceability.cjs` → `OK`).
- **Manifest counts**: python block → 12 categories, 3 kept ids, 4 kept atoms, 64 introduced rows (unchanged from brief).
- **LegacyOpen/LegacyClosing**: `src/index_lifecycle/activation.rs:57-64` — live bootstrap states; plan/spec assumption excluding them from T075’s naive “legacy mode branches” wording is correct.
- **R3 census asymmetry**: `source_anchor_policy` at checker line 209 matches research (V10 anchors on refreeze ancestor; V11 seams need same-tree receipts). `validatePostactivationRetirement` enforces seam resolution post-cut.
- **Frozen roster coverage**: T074→FR-001/002/003; T075→FR-004–010 + US2; T076→FR-011 + US3; T077→FR-002/012/013 + SC-001–010. No orphaned Phase 7 task.
- **Success criteria measurability**: SC-003–SC-007 are zero-count checks with explicit numerators; SC-008 is the enumerated R6 gate list; SC-009 maps to C-6 discharge records.
- **Constitution Check**: No false PASS spotted. Weakest entries (IV, VI) still hold — full gate set unchanged, T077 mandates adversarial review + cfg sweep.
- **Deliberate §7 choices**: Contract-id quoting, R4 unresolved embed, omitted Complexity Tracking, non-indivisible slice — all consistent with constitution and Slice 4 evidence style.
- **refreeze co-gate exists**: `python execution/refreeze_v11.py verify-internal --target-ref HEAD` passes on this tree (~6s); suitable as a supplemental public-surface gate if added to R6 as recommended above.

No BLOCKER findings.

---

## Verdict

1. **Is the load-bearing claim in §4 true?** **Yes, with the MAJOR incompleteness above.** On this checkout the tree is `postactivation`; equality is against a 34-atom 3-segment set (`kept ∪ introduced` after `directPublicAtoms`), not the full 64-row manifest list. Non-public removal cannot change that set; removing a surviving 3-segment public atom would fail the checker; removing associated methods could slip past the checker but is blocked by enumeration rules and should be caught by refreeze if wired into R6.

   Deciding commands:
   ```
   node scripts/validate-lifecycle-oracle-traceability.cjs
   → lifecycle oracle traceability v11: OK (78 requirements, 24 acceptance oracles, 13 retirement categories)

   python -c "…" (brief §4 block)
   → categories: 12; kept atoms: 4; introduced: 64

   lifecycle phase probe (checker logic replicated)
   → phase: postactivation; actual/pre/post counts: 34/83/34
   ```

2. **Implement as written, amended, or rejected?** **Amended, then implement.** No rejection warranted: scope inversion (bracket first, deletion maybe empty) matches Phase 7 and postactivation reality. Land the three prose fixes (R1 narrowing + refreeze in R6, lifecycle_phase probe in quickstart, pin-set wording) before execution; none require frozen-tree edits.
