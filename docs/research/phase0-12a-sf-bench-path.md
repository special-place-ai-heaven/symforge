# Phase 0 §12A — sf-bench workspace resolution

**Resolved:** 2026-06-13  
**Task:** T002

## Candidate paths checked

| Path | Result |
|------|--------|
| `E:\project\sf-bench` | **MISSING** |
| `..\sf-bench` (sibling of symforge repo) | **MISSING** |
| `C:\AI_STUFF\PROGRAMMING\sf-bench` | **MISSING** |
| `C:\AI_STUFF\sf-bench` | **MISSING** |

## Selected workspace

**NONE — NO-GO blocker B-SFBENCH**

## Required artifacts (not found)

When the workspace is restored, confirm these paths exist:

| Artifact | Expected relative path |
|----------|------------------------|
| Gate comparator | `compare-results.js` |
| Golden route corpus | `routes.golden.jsonl` |
| Results spec | `RESULTS.md` |
| Preflight fixture | `fixtures/preflight-minimal.json` (per gap plan progress note) |

## Unblock steps

1. Clone or restore sf-bench at a stable path (canonical: `E:\project\sf-bench` or repo sibling `../sf-bench`).
2. Verify commit `16acb4b` or later (compare-results `--preflight`, golden skeleton).
3. Re-run T010 and all US2/US3 tasks that depend on this path.
4. Update this file with the selected absolute path and re-link evidence index.

## Impact

All measurement ruler tasks (A-001..A-004, G-005, A-028, golden README) are **blocked** until B-SFBENCH clears.
