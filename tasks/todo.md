# RTK Integration Readiness Sweep

## Plan

- [x] Establish baseline evidence:
  - [x] Confirm SymForge MCP health/index state for `E:\project\symforge`.
  - [x] Review current git status, including ignored copied goal artifacts.
  - [x] Review `tasks/lessons.md` if present.
- [x] Inventory RTK source artifacts:
  - [x] Summarize `.agent/goals/rtk-symforge-integration/INDEX.md`.
  - [x] Extract every RTK task name, claimed goal, readiness status, and expected code touch points.
  - [x] Summarize `docs/plans/2026-05-19-rtk-integration-state-for-planning.md`.
- [x] Measure overlap against actual code:
  - [x] Use SymForge to find current implementations for config extractors, token savings/compression, lints, inline extraction tests, trust, sidecar integrity, Tier 2 metadata, graceful degradation, structural pattern cache, frecency, analytics, correction suggestions, parser reuse, and regex/glob caching.
  - [x] Classify each RTK item as already covered, ready now, needs redesign, defer, or reject.
  - [x] Identify the smallest integration path that makes sense for SymForge rather than blindly importing RTK patterns.
- [x] Produce readiness output:
  - [x] Update or create a concise readiness matrix with code evidence and recommendations.
  - [x] Add a review section here with commands run, results, and remaining risks.
- [x] Commit requested copied/planning material:
  - [x] Stage the copied RTK goal folder even if `.agent/` is ignored.
  - [x] Stage the latest report and readiness task files.
  - [x] Verify staged diff is limited to intended artifacts.
  - [x] Commit with a focused message.

## Review

Completed sweep output:
- Created `docs/plans/2026-05-19-rtk-task-readiness-code-overlap.md`.
- Classified RTK01, RTK02, RTK03, RTK04, RTK05, RTK09, RTK10, and RTK16 as useful near-term work.
- Classified RTK06-RTK08 as trust work blocked by ADR 0015 decisions.
- Classified RTK13-RTK15 and RTK17 as analytics-gated.
- Classified RTK11, RTK12, and RTK18-RTK21 as evidence-gated perf/cleanup investigations.

Evidence and verification:
- SymForge MCP health initially had an empty index; indexed `E:\project\symforge`, then verified 403 indexed files, 399 parsed, 4 partial, 0 failed.
- agentmemory recall found no prior RTK integration memories.
- remindb-vault found the vault concept `wiki/concepts/RTK Techniques for SymForge.md`.
- Two read-only subagents independently inventoried the RTK artifacts and code overlap.
- Current branch is `main`; copied goal files themselves target `rtk-symforge-integration` for future execution.
- `.agent/` is ignored, so `.agent/goals/rtk-symforge-integration/` must be staged with `git add -f`.
- Staged diff was limited to 25 intended planning/task files and passed `git diff --cached --check`.
- Commit created with message `docs: add RTK integration readiness plan`.

No source code was changed.
