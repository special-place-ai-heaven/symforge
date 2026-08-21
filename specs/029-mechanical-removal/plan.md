# Implementation Plan: Feature 020 Slice 5 — Mechanical Removal

**Branch**: `029-mechanical-removal` | **Date**: 2026-08-21 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/029-mechanical-removal/spec.md`

## Summary

Delete only code the Slice 4 activation cut already proved unreachable, and
prove the deletion changed nothing observable.

Phase 0 research turned the roster's prose into a closed boundary. The
traceability checker already defines "public behaviour unchanged" as an
equality against a frozen **postactivation** atom set, and that set is defined
as *kept ∪ introduced* — so every public atom that survived the cut is
required by contract to exist. **Slice 5 therefore cannot remove a public atom
at all; its removal surface is strictly non-public code.** That single finding
does more to bound this slice than the whole of Phase 7's task text.

The technical approach is consequently inverted from a normal slice. The
primary deliverable is not the removal but the **neutrality bracket** — a
baseline captured before, re-captured after, compared field by field, and
proven capable of detecting a real change before it is trusted to certify the
absence of one. The removal is whatever survives that bracket's admission
rules, and may legitimately be empty.

## Technical Context

**Language/Version**: Rust, toolchain pinned by `rust-toolchain.toml`; Node and Python for the checker and refreeze tooling

**Primary Dependencies**: none added — this slice only removes

**Storage**: N/A

**Testing**: `cargo test` serial (`--test-threads=1`), the embed feature cell, the Node checkers, the npm suite

**Target Platform**: Linux CI is authority; Windows and macOS cells gate additionally

**Project Type**: single Rust crate exposing an MCP server surface

**Performance Goals**: none — the goal is neutrality, not speed

**Constraints**: public atom set must remain exactly the postactivation set; frozen spec tree byte-identical including checkbox bytes; sealed pins recomputed only by their owning Rust oracle, after `fmt`

**Scale/Scope**: bounded by evidenced-unreachable non-public items; may be empty, and an empty result is a pass

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Assessed against Constitution v1.0.0 (ratified 2026-08-18).

| Principle | Status | How this plan satisfies it |
|---|---|---|
| **I. Reporting Invariant** | PASS | Every baseline field carries the command that produced it (data-model), so no field can be present without provenance. Research R4 deliberately leaves the embed question **unresolved** rather than predicting it — asserting T076 complete would be a success claim for an unobserved operation. |
| **II. RED-First Evidence** | PASS | The bracket is a negative instrument, so it carries its positive control: a deliberate change the comparison must catch, run *before* any removal (research R5, contract C-1). A bracket failing its control is `void` and authorizes nothing. |
| **III. Frozen Contracts Win** | PASS | `specs/020-…` is input only; SC-005 requires zero bytes changed there. Contract identifiers are quoted exactly. Where the roster's prose and the tree disagree — "legacy mode branches" versus the live `LegacyOpen`/`LegacyClosing` bootstrap states — the plan records the divergence as a stated assumption instead of editing the frozen text. |
| **IV. Verification Gates** | PASS | Research R6 fixes the full gate set unchanged, one cargo invocation at a time, long runs through Terminal Commander. The slice adds no gate and relaxes none. |
| **V. Unrepresentable Over Checked** | **PASS, with a stated limit — the weakest of the six** | Pin refresh is genuinely oracle-only and after `fmt`, with a per-pin audit direction. But the `void → armed` prohibition on `NeutralityComparison` is a **document convention drawn in a markdown state diagram, not a type**. Nothing makes the illegal transition unspellable; an executor can write an evidence document that skips it, and C-1 is a human gate. Enforcement is T077's review. Recorded as a limit rather than claimed as full compliance. |
| **VI. Independent Review Before Merge** | PASS | T077 carries an independent adversarial review including the mandatory cfg-lens sweep; findings fixed RED-first or adjudicated with rationale, never dropped. |

**Post-Phase-1 re-check**: PASS, with Principle V carrying the limit above.

**Post-review re-check (2026-08-21)**: PASS. Two independent reviews
(`REVIEW-FINDINGS-grok-4-6-…`, `REVIEW-FINDINGS-composer-…`) returned no
BLOCKER and a verdict of *amend, then implement*. Four MAJORs were confirmed
against source and amended: C-2 narrowed from "the definition of public
behaviour" to one of three owners; C-5's pin arithmetic corrected per-pin;
research observation 1 closed as `postactivation`; and US1's Independent Test
replaced, because as written it closed the story on the null re-run — the
vacuous negative Principle II exists to forbid, in the document that invokes
Principle II. Principle V's PASS was downgraded to a stated limit in the same
pass.

That last one is worth naming plainly: the Constitution Check above originally
claimed PASS on a principle the artifacts only partly satisfy, and no amount of
re-reading my own reasoning surfaced it. An outside reader did, immediately.

**Violations requiring justification**: none. Complexity Tracking is omitted
because it would be empty.

## Project Structure

### Documentation (this feature)

```text
specs/029-mechanical-removal/
├── plan.md                              # This file
├── spec.md                              # Feature specification
├── research.md                          # Phase 0 — R1–R6 + open observations
├── data-model.md                        # Phase 1 — evidence artifacts
├── quickstart.md                        # Phase 1 — run and validation guide
├── contracts/
│   └── neutrality-bracket-v1.md         # Phase 1 — the evidence contract, C-1…C-7
├── checklists/
│   └── requirements.md                  # Spec quality checklist
└── tasks.md                             # Phase 2 — produced by /speckit-tasks
```

### Source Code (repository root)

Removal targets and the gates that bound them:

```text
src/                                     # removal surface — NON-PUBLIC items only
├── embed.rs                             # T076, gated on the allowlist negative suite
├── index_lifecycle/
│   └── activation.rs                    # NOT a target: LegacyOpen/LegacyClosing are live
└── …                                    # candidates enumerated by evidence, not by name

tests/
├── preventive_runtime_dark_v11.rs       # owns FULL_SOURCE_PIN_V1 + EXCLUDED_RUNTIME_SOURCE_PIN_V1
├── activation_cut_v11.rs                # writer-reachability case
└── fixtures/public-api-v11-consumer/    # all-cfg, compile-fail, dependent-positive

scripts/
├── validate-lifecycle-oracle-traceability.cjs   # the C-2 three-state public-API gate
└── slice0-oracle-artifact.cjs                   # fails closed if a control flips

execution/
└── refreeze_v11.py                      # allowlist / API-atom authority

docs/reviews/
├── FEATURE-020-SLICE5-BASELINE-v11.md   # T074 output
└── FEATURE-020-SLICE5-EVIDENCE-v11.md   # T077 output + review record
```

**Structure Decision**: single-project layout, unchanged. This slice creates no
module and moves no file; it deletes within `src/`, refreshes two pins in
`tests/preventive_runtime_dark_v11.rs`, and writes two documents under
`docs/reviews/`. `src/index_lifecycle/activation.rs` is listed explicitly as a
**non**-target so the exclusion is visible in the structure rather than buried
in prose.

## Phase Sequencing

| Phase | Frozen task | Output | Gate to proceed |
|---|---|---|---|
| Observe | — | Lifecycle phase recorded | Checker green |
| Baseline | T074 | `FEATURE-020-SLICE5-BASELINE-v11.md` | Every field has a command |
| Arm | T074 | Control result | `detected(<field>)`, else stop |
| Enumerate | T075 | Candidate list with dispositions | Each removal cites evidence |
| Remove | T075 → T076 | Deletions + pin refresh | T076 only after the allowlist suite speaks |
| Re-run | T077 | `FEATURE-020-SLICE5-EVIDENCE-v11.md` | No unexplained differing field |
| Review | T077 | Review record | Findings fixed or adjudicated |

Unlike Slice 4, no part of this slice is indivisible: candidates may land in
separate changes provided each carries its own bracket result.

## Risks

| Risk | Why it is real here | Mitigation |
|---|---|---|
| A quiet bracket certifies a regression | Neutrality instruments report "nothing changed" whether or not they work | C-1's control, run before any removal; a void bracket authorizes nothing |
| Deleting a frozen V11 seam | Its receipt must bind *this* tree, so deletion destroys evidence, not just code | C-3; seam check precedes every removal |
| Acting on the roster's naive wording | "Legacy mode branches" exactly describes the live bootstrap states | Recorded as a spec assumption; C-4 rules roster wording inadmissible as evidence |
| Inventing a removal to avoid an empty slice | The social pressure runs this way when nothing is removable | C-7 names the empty outcome a pass; C-6 forbids substitution |
| Hand-refreshing a moved pin | Faster than running the oracle, and has silently diverged once already | C-5; counts must move downward by the removed amount |
