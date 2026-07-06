# Analyze: Perl Parser Hardening

**Feature**: 016 · **Date**: 2026-07-06 · **Mode**: Read-only cross-artifact review

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 1 |

**Verdict**: **PASS** — ready for PROG implement after operator push of branch.

## Coverage matrix

| User Story | spec.md | plan.md | tasks.md | contracts | quickstart |
|------------|---------|---------|----------|-----------|------------|
| US0 Retrofit | ✓ | ✓ | S0 tasks | node-shapes | S0 gate |
| US1 Corpus | ✓ | ✓ | S1 tasks | — | S1 gate |
| US2 Recall | ✓ | ✓ | S2 tasks | xref-recall | S2 gate |
| US3 Bump | ✓ | ✓ | S3 tasks | node-shapes | bump § |
| US4 Doc | ✓ | ✓ | S3 tasks | — | S3 gate |

## Constitution alignment

All eight principles evaluated in [plan.md § Constitution Check](./plan.md#constitution-check).
No CRITICAL conflicts.

## Findings

### MEDIUM-001 — SC-002 threshold may fail on small corpus

**Artifacts**: spec SC-002 (≥90% clean parse), research § #341 benchmark  
**Issue**: 20-fixture corpus may not represent CPAN diversity; 90% target could fail on intentional edge fixtures.  
**Recommendation**: Investigation doc must explain shortfall by failure bucket — already in FR-003/spec edge cases. **Accepted**.

### MEDIUM-002 — S2 qualified_call scope depends on S1 taxonomy

**Artifacts**: tasks P-S1-006, C-S2-003  
**Issue**: Circular risk if taxonomy deferred.  
**Recommendation**: Planning Gate on sprint-1 blocks S2 — **already gated**.

### LOW-001 — tasks.md estimate vs actual

**Artifacts**: execution-model task counts (~77) vs tasks.md header (~77)  
**Issue**: Count is estimate until implement tracks completion.  
**Recommendation**: Update task-index during implement (optional file).

## Duplication check

- Node shapes appear in research.md and contracts/perl-node-shapes.md — intentional (research = narrative, contract = normative).
- No conflicting recall targets between spec US2 table and contracts/perl-xref-recall.md.

## Ambiguity check

- "Accepted loss" defined in data-model FailureBucket — clear.
- Grammar bump protocol in quickstart — ordered steps, no ambiguity.

## Recommended next command

`/speckit-implement` starting with **Phase PROG** tasks only.

No remediation edits required before implement.
