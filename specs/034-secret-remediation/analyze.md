# Analyze: Agent-Driven Secret Remediation (034)

**Feature**: 034 · **Date**: 2026-10-01 · **Mode**: Read-only cross-artifact review (spec track)

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 1 |

**Verdict**: **PASS** — spec track ready for independent review. Implementation MUST NOT start from this PR's docs alone without the follow-on implement PR and RED oracles.

## Coverage matrix

| User Story | spec.md | plan.md | tasks.md | contracts | quickstart |
|------------|---------|---------|----------|-----------|------------|
| US1 Actionable meta | ✓ | ✓ | Phase 3 | withheld-meta.md | oracles 1–3 |
| US2 Externalize | ✓ | ✓ | Phase 4 | secret-remediate.md | oracles 4–6 |
| US3 Encrypt | ✓ | ✓ | Phase 5 | secret-remediate.md | oracles 7–9 |
| US4 Dismiss | ✓ | ✓ | Phase 6 | secret-remediate.md + data-model | oracles 10–13 |

## Constitution alignment

All six principles evaluated in [plan.md § Constitution Check](./plan.md). No CRITICAL conflicts. Reporting Invariant and RED-First obligations are carried into quickstart/tasks rather than satisfied by docs alone (expected).

## Findings

### MEDIUM-001 — Dismissal file path/schema deferred

**Artifacts**: data-model.md DismissalRecord; research.md open items; tasks T025  
**Issue**: Exact on-disk path and schema are intentionally left to implement.  
**Recommendation**: Accept for spec PR; block implementation merge until contract amendment or task lands a pinned path + schema with size/symlink tests. **Accepted for spec track.**

### MEDIUM-002 — Harness write-permission oracle is environmental

**Artifacts**: spec SC-006; quickstart oracle 14; FR-006  
**Issue**: MCP client approval cannot be fully simulated in-process without a harness double.  
**Recommendation**: Document the assumed harness behavior and add an in-process guard that apply is classified as a write/mutating tool (so clients that honor annotations must prompt). **Accepted with task T028.**

### LOW-001 — Working tool name may change for registry pins

**Artifacts**: spec Assumptions; contracts/secret-remediate.md; tasks T019  
**Issue**: `secret_remediate` might need a prefix/rename for `verify-tools.cjs` conventions.  
**Recommendation**: Rename is cosmetic if contracts/tests update atomically — not a spec blocker.

## Duplication check

- HANDOVER §3 design is restated in spec/plan by design (HANDOVER remains owner brief; spec is normative for implement).
- Five allowlist failure modes appear in HANDOVER, spec US4, and data-model — consistent, not contradictory.
- Refusal renderer anchors appear in HANDOVER and research — aligned.

## Ambiguity check

- No `NEEDS CLARIFICATION` markers in spec.md.
- Path-rule / unscanned remediation: explicitly "honest unavailable" rather than underspecified.
- Whole-repo scope: allowed but constrained by `resolve_repo_path` + credential-root refusals.

## D1 prerequisite

Deliverable 1 (#738 / `d8ee3d0c`) is on `origin/main` and is an ancestor of this branch after rebase. Spec correctly scopes it out of 034 implementation work.

## Recommended next command

Independent review of this docs PR (LINUS). After merge (Grove process), `/speckit-implement` starting at Phase 1 tasks — **not** in this PR.

No remediation edits required before opening the spec PR.
