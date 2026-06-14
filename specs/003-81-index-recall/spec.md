# Feature Specification: SymForge 8.1 Index-Recall Program — T2.0/T2.1 Audit Slice

**Feature Branch**: `cursor/81-index-recall-t21-audit-0ef7`

**Created**: 2026-06-14

**Status**: Audit slice — pending independent sign-off (T006, T019)

**Baseline**: Main at **`470826a`** — task plan merged (#313); Phase 2 closed with A-029 **PIVOT** (0/4 T2 equiv).

**Input**: [`tasks.md`](./tasks.md), [`docs/v8-gap-closure-plan.md`](../../docs/v8-gap-closure-plan.md) §6.1 Program T2.

## Scope (this slice)

Produce **docs/evidence only** for T2.0 planning and T2.1 gap audit through **T019 taxonomy sign-off packet**. No `src/**` changes. No recall fixes. No golden-row or `eligible_h6` changes. Do not claim A-029 PASS.

## User Stories

### US1 — Crosswalk program scope

Map Phase 2 handoff, golden T2 rows, A-029 external tasks, and index pipeline touchpoints to §6.1 hypothesis classes.

**Acceptance**: [`docs/research/A-029-t2-task-crosswalk.md`](../../docs/research/A-029-t2-task-crosswalk.md) complete.

### US2 — rg-baseline inventory

For all four A-029 T2 tasks, capture rg baseline file sets, symforge cited paths (measurement only), missed-site buckets, and per-task JSON artifacts.

**Acceptance**: [`docs/research/rg-hits/`](../../docs/research/rg-hits/) populated; repo spike docs summarize tokio/django.

### US3 — Gap taxonomy and T019 gate

Classify failure modes, rank explain-power, propose fix surfaces (for post-T019 implementation only), and prepare independent taxonomy reviewer sign-off.

**Acceptance**: [`docs/research/A-029-gap-taxonomy.md`](../../docs/research/A-029-gap-taxonomy.md) + [`docs/research/81-index-recall-taxonomy-signoff.md`](../../docs/research/81-index-recall-taxonomy-signoff.md).

## Explicit exclusions

B-RESULTS, persistence, EMA→L2, H6–H8 closure, new compact MCP tools, T2.2/T2.3 implementation, T3 program, deploy/admin.

## Exit criteria (this slice)

- T001–T019 artifacts linked from evidence index
- T019 sign-off packet ready for independent reviewer (**GO required before T2.2**)
- Zero `src/**` diff
