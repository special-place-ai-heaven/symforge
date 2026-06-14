# T019 — Index-Recall Taxonomy Sign-off Packet

**Gate:** Blocks T2.2/T2.3 `src/**` until **GO** + cleanup merged on `main`
**Program:** 8.1 index-recall · [`tasks.md`](../../specs/003-81-index-recall/tasks.md)
**Evidence index:** [`81-index-recall-evidence-index.md`](./81-index-recall-evidence-index.md)

## Decision

| Field | Value |
|-------|-------|
| **Reviewer** | Independent review (T019 gate — not evidence producer) |
| **Evidence producer** | Cloud agent — T2.1 audit slice (PR #314) |
| **Date** | 2026-06-14 |
| **Decision** | **GO** |

## Review conclusion

1. **Taxonomy complete enough** to authorize T2.2/T2.3 planning and implementation branches.
2. **Pre-implementation cleanup (issues #1–#3)** required on PR #314 before merge:
   - **#1 Bench bucketing:** `scripts/a029-t21-rg-inventory.cjs` uses `(^|/)benches?/`; rg-hits JSON `missed_bucket_counts` updated.
   - **#2 FM-CAP narrative:** FM-CAP binding for **tokio** (20/20 cited); django **not** cap-bound at 20; django **TX-02-bound**; per-repo re-measure after TX-01.
   - **#3 Artifact paths:** `specs/003-81-index-recall/tasks.md` aligned to shipped artifact filenames.
3. **Recommended implementation order:** **TX-01 → TX-02 → TX-04 → TX-03**; re-measure per repo after TX-01.
4. **TX-01 expected to affect tokio more than django;** django likely needs **TX-02** (xref/structured refs) for material recall lift.
5. **No `src/**` authorization** until this cleanup commit is **merged and main is green.**

## Required artifacts (T010–T018)

| # | Artifact | Link | Status |
|---|----------|------|--------|
| 1 | T2 task crosswalk | [`A-029-t2-task-crosswalk.md`](./A-029-t2-task-crosswalk.md) | ✓ |
| 2 | rg-hits JSON (4 tasks) | [`rg-hits/`](./rg-hits/) | ✓ (bucket fix) |
| 3 | Tokio spike summary | [`A-029-tokio-recall-spike.md`](./A-029-tokio-recall-spike.md) | ✓ |
| 4 | Django spike summary | [`A-029-django-recall-spike.md`](./A-029-django-recall-spike.md) | ✓ |
| 5 | Gap taxonomy | [`A-029-gap-taxonomy.md`](./A-029-gap-taxonomy.md) | ✓ |
| 6 | Phase 2 baseline | [`a029-t2-results.json`](./a029-t2-results.json) | ✓ (unchanged) |

## Taxonomy acceptance checklist

- [x] Failure-mode table (FM-CAP … FM-POLICY) is evidence-backed
- [x] FM-CAP scoped: tokio cap-bound; django not cap-bound at cited counts
- [x] Taxonomy rows TX-01..TX-06 map to fix surfaces; implementation gated post-merge
- [x] Explain-power ranking acceptable: TX-01 → TX-02 → TX-04 → TX-03
- [x] FM-MARKDOWN limitation documented
- [x] TX-06 (P-T2-only) excluded from implementation scope
- [x] No A-029 PASS or golden/`eligible_h6` change implied
- [x] Scope guard respected

## Next step (post-merge only)

Open first implementation branch for **TX-01 cap** after PR #314 merges and main CI is green. Do **not** start before merge.

## Scope attestation

- [x] Audit slice: zero `src/**` diff
- [x] No golden row edits
- [x] No A-029 PASS claim
- [x] P-T2 baseline unchanged (0/4 PIVOT)

---

**Status:** **GO** — implementation authorized **after** cleanup merge + green `main`.
