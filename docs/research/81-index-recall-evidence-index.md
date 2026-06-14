# 81 Index-Recall — Evidence Index

**Program:** 8.1 index-recall (gap plan §6.1 Program T2)
**Task plan:** [`specs/003-81-index-recall/tasks.md`](../../specs/003-81-index-recall/tasks.md) (merged #313 @ `470826a`)
**Audit branch:** `cursor/81-index-recall-t21-audit-0ef7`
**Slice:** T2.0 + T2.1 only (through T019 sign-off packet)

## Spec Kit inputs

| Artifact | Path | Status |
|----------|------|--------|
| Tasks | [`specs/003-81-index-recall/tasks.md`](../../specs/003-81-index-recall/tasks.md) | **MERGED** (#313) |
| Spec (audit slice) | [`specs/003-81-index-recall/spec.md`](../../specs/003-81-index-recall/spec.md) | **DRAFT** (T2.0/T2.1) |
| Plan / research / data-model | `specs/003-81-index-recall/{plan,research,data-model}.md` | **DEFERRED** (post-audit) |

## Phase 2 handoff

| Artifact | Path |
|----------|------|
| Phase 2 exit | [`docs/phase2-stel-checkpoint.md`](../phase2-stel-checkpoint.md) |
| A-029 spike (PIVOT) | [`docs/research/A-029-t2-spike.md`](./A-029-t2-spike.md) |
| A-029 results JSON | [`docs/research/a029-t2-results.json`](./a029-t2-results.json) |
| Gap plan §6.1 | [`docs/v8-gap-closure-plan.md`](../v8-gap-closure-plan.md) |

**Handoff state:** A-029 **PIVOT** 0/4; **P-T2** bypass-only; `eligible_h6=false` when policy lands. Recall remediation deferred to this program.

## §6.1 / G-029 traceability

| Step | Work (gap plan) | Done when | Audit artifact |
|------|-----------------|-----------|----------------|
| T2.1 | Audit missing sites vs `find_references` + sidecar proxy | Gap taxonomy doc | [`A-029-gap-taxonomy.md`](./A-029-gap-taxonomy.md) |
| T2.2 | Implement missing source classes | tokio T2 equiv | **BLOCKED** (T019) |
| T2.3 | Repeat django T2 | django T2 equiv | **BLOCKED** (T019) |
| T2.4 | Battery T2 all repos | ≥2/4 or P-T2 | **DEFERRED** |
| G-029 | T2 0/4 equiv program | ≥2/4 tokio+django or P-T2 | Baseline 0/4; P-T2 retained |

## T2.0 / T2.1 artifacts (this slice)

| Task | Artifact | Status |
|------|----------|--------|
| T001–T004 | This index | **COMPLETE** |
| T005–T006 | [`81-index-recall-review-signoff.md`](./81-index-recall-review-signoff.md) | **PENDING** reviewer |
| T010–T012 | [`A-029-t2-task-crosswalk.md`](./A-029-t2-task-crosswalk.md) | **COMPLETE** |
| T013–T016 | [`rg-hits/`](./rg-hits/) + spike summaries below | **COMPLETE** |
| T017–T018 | [`A-029-gap-taxonomy.md`](./A-029-gap-taxonomy.md) | **COMPLETE** |
| T019 | [`81-index-recall-taxonomy-signoff.md`](./81-index-recall-taxonomy-signoff.md) | **GO** (cleanup pre-merge) |

| Repo spike summary | Path |
|--------------------|------|
| Tokio (2 tasks) | [`A-029-tokio-recall-spike.md`](./A-029-tokio-recall-spike.md) |
| Django (2 tasks) | [`A-029-django-recall-spike.md`](./A-029-django-recall-spike.md) |

### rg-hits JSON (per task)

| Task ID | Artifact |
|---------|----------|
| `tokio/t2_spawn` | [`rg-hits/tokio/t2_spawn.json`](./rg-hits/tokio/t2_spawn.json) |
| `tokio/t2_block_on` | [`rg-hits/tokio/t2_block_on.json`](./rg-hits/tokio/t2_block_on.json) |
| `django/t2_queryset` | [`rg-hits/django/t2_queryset.json`](./rg-hits/django/t2_queryset.json) |
| `django/t2_model` | [`rg-hits/django/t2_model.json`](./rg-hits/django/t2_model.json) |
| Summary | [`rg-hits/summary.json`](./rg-hits/summary.json) |

**Measurement commit:** `470826a` · **Measured:** 2026-06-14 (see JSON `measuredAt`)

## Scope guard (binding)

**Forbidden until T019 taxonomy GO:**

- Any `src/**` runtime change (index, parser, formatter, STEL, MCP tools)
- Golden row / `eligible_h6` edits
- A-029 PASS claims

**Out of program scope (do not add without spec amendment):**

- B-RESULTS / §8.7
- Persistence / SQLite / EMA→L2
- H6–H8 gate closure
- New compact MCP tools
- T3 outline program (§6.2)
- Deploy / admin (Phase 4)

## P-T2 baseline (unchanged)

> P-T2 bypass-only for reference tasks (grep envelope; `eligible_h6=false`)

Source: [`a029-t2-results.json`](./a029-t2-results.json), Phase 2 #311.

## Review gates

| Gate | Document | Required for |
|------|----------|--------------|
| T006 | [`81-index-recall-review-signoff.md`](./81-index-recall-review-signoff.md) | T2.1 start (spec GO) |
| T019 | [`81-index-recall-taxonomy-signoff.md`](./81-index-recall-taxonomy-signoff.md) | T2.2/T2.3 implementation |

## Operator commands (reproduce inventory)

```bash
git clone --depth 1 https://github.com/tokio-rs/tokio.git tests/fixtures/a029-t2/tokio
git clone --depth 1 https://github.com/django/django.git tests/fixtures/a029-t2/django
cargo build -p symforge
node scripts/a029-t21-rg-inventory.cjs "$(pwd)/target/debug/symforge"
```

Corpus SHAs: see `corpus_sha` in each rg-hits JSON.
