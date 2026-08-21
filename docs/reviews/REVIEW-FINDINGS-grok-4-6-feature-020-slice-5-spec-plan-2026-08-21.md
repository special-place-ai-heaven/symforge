# Review findings — Feature 020 V11 Slice 5 spec+plan (pre-implementation)

**Reviewer:** grok-4-6
**Date:** 2026-08-21
**Scope:** `specs/029-mechanical-removal/{spec,plan,research,data-model,quickstart,contracts/neutrality-bracket-v1}.md` against the frozen roster and the current tree. No implementation exists. No `cargo` run.

---

### BLOCKER

None.

---

### MAJOR — R1 is true on this tree and incomplete as the definition of “public behaviour”

- **Where**: `specs/029-mechanical-removal/research.md:9-38`; `contracts/neutrality-bracket-v1.md:30-47` (C-2); `scripts/validate-lifecycle-oracle-traceability.cjs:1998-2070`; `specs/020-repository-knowledge-index/contracts/public-api-v11.json` `introduced_v11_atoms`; `tests/fixtures/public-api-v11-consumer/compile-fail/cases.json`; `execution/refreeze_v11.py:2634-2639`
- **Claim**: The traceability checker’s postactivation set is kept ∪ introduced, depth ≤ 3. On this tree `actual` equals that set (34 atoms), so no *lifecycle* atom is removable. That set is not the public surface. 34 of 64 `introduced_v11_atoms` are depth-4 methods; `deriveLifecyclePublicAtoms` never sees them (`embed` is excluded from the 2044–2049 regex; `directPublicAtoms` drops `split("::").length > 3`). `refreeze_v11.py` still requires the full 64-atom introduced set, and the consumer compile-fail/dependent-positive fixtures pin names the lifecycle checker does not.
- **Why it matters**: C-2 would stay green after deleting `Claim::value` (or any other depth-4 method). That is a public-behaviour change the frozen consumer surface would catch and the Slice 5 contract would not. The 2044–2049 regex also has no `cfg(test)` filter; it scans only `server_api`, so a test-only `pub fn` there would move the lifecycle set without being API.
- **Recommended fix**: State C-2 as one conjunct of public-behaviour neutrality, not the definition. Name the other owners: the 64-atom introduced set, the consumer fixtures, and `verify-internal`. Keep lifecycle-atom immutability. Do not treat a green `ordinaryRetirementLifecycle` as “public behaviour unchanged.”

### MAJOR — Open observation 1 is already answered; the Observe step cannot record it

- **Where**: `research.md:189-193`; `plan.md:128`; `quickstart.md:16-27`; `data-model.md:22,37`; `spec.md:161-163` (FR-007)
- **Claim**: The phase is `postactivation`. `node scripts/validate-lifecycle-oracle-traceability.cjs` prints `OK (78 requirements, 24 acceptance oracles, 13 retirement categories)` and does not print the phase, yet Step 1 tells the executor to “record which frozen set the tree currently matches” from that output. FR-007 still allows either frozen set; C-2 already requires postactivation.
- **Why it matters**: R1’s “cannot remove a public atom” is only true in postactivation. Deferring that at plan time was Principle I. Leaving it deferred after it is a one-second derivation, and pointing the observe step at a command that cannot fill `lifecycle_phase`, is an unobserved field. FR-007’s either-set wording is now the wrong rule for this tree.
- **Recommended fix**: Record `postactivation` in research (observation 1 closed, with the derivation). Change quickstart Step 1 to a command that *emits* the phase (the checker’s `ordinaryRetirementLifecycle` result, or the independent derivation). Tighten FR-007 / SC-004 to “the observed postactivation set.”

### MAJOR — C-5’s audit property is false for T076 and for any in-file deletion

- **Where**: `contracts/neutrality-bracket-v1.md:87-98`; `research.md:49-67`; `tests/preventive_runtime_dark_v11.rs:960-1110`
- **Claim**: C-5 requires both whole-source pins’ file and byte counts to move downward by the removed amount, and treats a flat count across a non-empty removal as a defect. `EXCLUDED_RUNTIME_SOURCE_PIN_V1` is the 20-file set `index_lifecycle/**` + `server_api.rs`. `src/embed.rs` is outside that set. `FULL_SOURCE_PIN_V1` covers all of `src/`. Deleting bytes inside a file does not change file count.
- **Why it matters**: T076’s target is `src/embed.rs`. A real embed deletion moves FULL bytes down, leaves FULL file count flat, and leaves EXCLUDED entirely flat. C-5 as written refuses the correct refresh. An executor who then “fixes” EXCLUDED is corrupting a pin that did not move.
- **Recommended fix**: Per pin: counts must not *rise*. File count may stay flat when no file was deleted. A pin whose path set does not contain the removed bytes must stay identical. Direction is checked only on pins that actually covered the deletion.

### MAJOR — US1’s Independent Test is the vacuous check Principle II forbids

- **Where**: `spec.md:41-45` vs `:56-59`, `:154-155` (FR-003), `:198-199` (SC-002); `research.md:125-144`; `.specify/memory/constitution.md:36-41`
- **Claim**: US1’s Independent Test is: capture, change nothing, re-run, confirm identity. A comparison that never moves would pass that test whether or not it can detect a change. Scenario 3, FR-003, SC-002, and C-1 already require the positive control.
- **Why it matters**: Spec-kit Independent Tests are what an executor uses to close a story alone. Closing US1 on the null re-run skips the only thing that makes the bracket a bracket. That is the `format_search_envelope` shape the research cites.
- **Recommended fix**: Replace the Independent Test with the control: a named field must move, then the control edit is discarded. Keep the null re-run as scenario 2, not as the story’s standalone proof.

---

### MINOR — C-6’s schema has teeth; nothing machine-checks the evidence field

- **Where**: `contracts/neutrality-bracket-v1.md:102-111`; `data-model.md:102-118`
- **Claim**: A `DischargedExpectation` requires `observed`, a command-backed `evidence` citation, and `discharge ∈ {already-removed, never-existed}`, and forbids substitution. No checker reads that document.
- **Why it matters**: C-7’s empty pass is honest only if those records are real observations. A one-line “already gone” with no failing-capable command would still parse as a filled-in record. FR-012 (the T077 review) is the only backstop.
- **Recommended fix**: In the evidence doc template, require the discharging command and its output verbatim, one record per T075/T076 predicted class. That is enough; do not add a new gate.

### MINOR — Principle V is claimed PASS for a prose state machine

- **Where**: `plan.md:59`; `data-model.md:63-71`
- **Claim**: `NeutralityComparison` has “no edge from `void` to `armed`.” That edge is a markdown diagram. Anyone can write an evidence doc that skips it.
- **Why it matters**: Principle V’s force is types that cannot spell the illegal state. Applied to artifacts this is a checklist, and C-1’s “refuses void” is still a human gate. The PASS is not false; it is the weakest of the six.
- **Recommended fix**: Either keep the PASS and say “document convention, enforced by T077 review,” or drop V to “PASS with that limit.” Do not pretend the diagram makes `void → armed` unrepresentable.

### MINOR — Quickstart does not name commands for two baseline fields

- **Where**: `quickstart.md:29-48`; `data-model.md:25-29,33-34`
- **Claim**: `activation_result` and `writer_reachability_verdict` are required baseline fields, each needing a command. Step 2 lists cargo/node gates and then says “also record” those results, with no command.
- **Why it matters**: Data-model rule: a field without a command is not a captured field. T074 will invent the commands under time pressure.
- **Recommended fix**: Name them (the activation-cut writer-reachability case and the activation-mode observation already cited in `plan.md`’s test list).

---

### NIT

None that survive the bar. `plan.md` listing `tasks.md` before `/speckit-tasks` is expected, not a defect.

---

## Adjudications (§5)

### 5.1 Bracket as P1 — discipline, not over-engineering

T074/T077 already mandate a baseline and a re-run. The only added cost is C-1’s one deliberate control, discarded before removal. That is Principle II applied to a negative instrument, and it is cheap. Making the instrument P1 is correct because an empty T075 still needs a working bracket (spec Edge Cases, C-7). What *is* over-engineering is not the P1 call; it is US1’s Independent Test describing the vacuous half (MAJOR above). Fix that test. Keep P1.

### 5.2 C-7 empty removal as a pass — honest scoping, not a pre-authorized excuse

Slice 4 may already have deleted most of T075/T076. Naming the empty outcome a pass is how you stop someone inventing a deletion. C-6 is not a formality on paper: closed `discharge` enum, evidence must be an observation, substitution forbidden. It is a formality in enforcement (MINOR above). That is not a reason to drop C-7. An executor who writes “already gone” with no command fails FR-011/FR-013 and should fail T077, not C-7.

---

## Negatives

- **R1’s three-state check, as far as it goes.** `ordinaryRetirementLifecycle` (cjs:2054-2070) compares source-derived `actual` to preactivation (all migration atoms, depth ≤ 3) and postactivation (kept ∪ introduced, depth ≤ 3). Anything else is `RETIREMENT_LIFECYCLE_PHASE_INVALID`. Python against the manifest: 12 categories, kept `v10-00-crate-root` / `v10-02-embed-module` / `v10-03-engine-info`, 4 kept atoms, 64 introduced. Independent derivation: `actual` length 34, equals postactivation, not preactivation (83). Checker: `lifecycle oracle traceability v11: OK (78 requirements, 24 acceptance oracles, 13 retirement categories)`. `derivePublicApiAtoms` is source-derived (`lib.rs` `pub mod` + `embed.rs` pub items/`pub use`). The 2044–2049 regex currently scans only `server_api` and adds exactly `ServerBootstrapError`, `ServerExit`, `run` — no extras today (`actualOnly` empty).
- **Phase is postactivation**, so R1’s operational conclusion holds: no depth-≤3 lifecycle atom is removable. Plan is not mis-scoped.
- **`LegacyOpen` / `LegacyClosing` are live.** `src/index_lifecycle/activation.rs:7-64` — monotonic `LegacyOpen -> LegacyClosing -> PreventiveV1Open`, process boot path. Excluding them from T075 is correct. Acting on the roster’s “legacy mode branches” wording would delete startup. C-4’s example is the right one.
- **R3 asymmetry is real on this tree.** Ordinary postactivation skips current-tree V10 `src/` member resolution (`validateRetirement` cjs:2687-2691) and instead runs `validatePostactivationRetirement`, which fails if a retired API atom is reachable or a frozen seam lacks a same-tree receipt (`EXPECTED_PRODUCTION_SEAMS`, cjs:160-168, 2073-2088). Materialized/release evidence resolves V10 anchors on `gitRustSourceMap(approved_refreeze_commit)` (cjs:2892-2894). Deleting retired V10 code cannot break the ordinary census; deleting a V11 seam can. FR-005 is the right tripwire.
- **Pin owners match R2.** `FULL_SOURCE_PIN_V1` = all `src/` (196 files, 9_300_142 bytes). `EXCLUDED_RUNTIME_SOURCE_PIN_V1` = 20 listed relative paths (cjs-equivalent list in the test). Refresh is `source_set_fingerprint` over LF-normalized bytes. Counts in research.md match the test constants. The subset relationship is why C-5 as written is wrong (MAJOR), not why the pins are the wrong seals.
- **T074–T077 are covered.** T074 → FR-001, US1, SC-001. T075 → FR-004/005/006/008/009, US2, C-4/C-6. T076 → FR-011, US3, R4. T077 → FR-002/003/012/013, SC-008/010. No roster task is orphaned. T075’s named classes are predictions to perform or discharge, not extra FRs.
- **SC-003–SC-007 are measurable** once the artifacts exist: evidence citations on candidates, checker equality, `git diff` over `specs/020-…`, seam set vs `EXPECTED_PRODUCTION_SEAMS`, tests deleted only with their subject. None is a “zero X” nobody can check.
- **R4 unresolved embed is correct.** `src/embed.rs` is 163 lines of the kept identity plus `pub use` of V11 atoms plus a `#[cfg(test)]` contract module. Predicting “no dead V10 left” from line count would be Principle I. Leave it to the allowlist suite.
- **Known deliberate items (§7) not re-reported:** contract identifiers in the spec; Complexity Tracking omitted; slice not indivisible.
- **No live `internals::` `--exact` footgun** in the 029 docs. Quickstart’s suite filter is `--lib --bins --tests`, not a pre-cut exact name.
- **Constitution I, II, III, IV, VI** as argued in `plan.md` match the artifacts, with I’s observe-step hole called out above. II is carried by C-1, not by US1’s Independent Test.

---

## Verdict

1. **Is the load-bearing claim in §4 true?** **Yes**, on this tree, for the lifecycle atom set the checker actually compares.

   Command output that decided it:

   ```
   python (manifest): categories: 12; kept ids: ['v10-00-crate-root', 'v10-02-embed-module', 'v10-03-engine-info']; kept atoms: 4; introduced: 64
   node scripts/validate-lifecycle-oracle-traceability.cjs: lifecycle oracle traceability v11: OK (78 requirements, 24 acceptance oracles, 13 retirement categories)
   independent derivation of ordinaryRetirementLifecycle: phase=postactivation; actual=34; preactivation=83; postactivation=34; scannedModules=['server_api']; introduced depth {2:1, 3:29, 4:34}; actualOnly=[]; postOnly=[]
   ```

   Incomplete in the sense of the MAJOR: 34 depth-4 introduced methods and the consumer/refreeze gates also constrain public behaviour and are outside C-2.

2. **Should this spec+plan be implemented as written, amended, or rejected?** **Amended.** Do not implement as written. Do not reject. Close observation 1 (`postactivation`), split C-2 from the rest of the public surface, fix C-5’s pin arithmetic, and replace US1’s Independent Test with the control. The two design decisions (bracket as P1, C-7 empty pass) stand.
