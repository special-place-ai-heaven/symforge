# Contract: `if_match` guarded apply (US3)

**Surface**: `symforge_edit` / `replace_symbol_body` apply path.

## Input
- `if_match`: expected pre-edit body (or its hash) the apply is conditioned on. Threaded
  through `ReplaceSymbolBodyInput` (`edit_tools.rs`) → planner (`edit_planner.rs`) → write
  (`edit_apply.rs`/`edit.rs`). MUST NOT be dropped by the planner (TR-06).

## Enforcement (FR-009, D1)
1. The guard is re-verified against the **bytes actually being written**, in the **same
   critical section** as the splice + `atomic_write` (re-check-at-write, single lock).
2. On divergence (on-disk body changed after the agent's read): **reject, no write**; the
   divergent on-disk content is left intact (US3 AC-1).
3. Negative control (no concurrent change): the guarded apply succeeds and matches the
   request (US3 AC-2).

## Response honesty (FR-010)
- The response claims a successful guarded apply **only** when the guard was enforced at
  the write (US3 AC-3).
- The best-effort tee backup is NOT described as transactional rollback.

## Regression
`symforge_edit_if_match_rejected_after_concurrent_disk_change`: apply a guarded edit;
via a **deterministic injected interleave point** (test hook, NOT a sleep) mutate the file
between guard-read and write; assert reject + on-disk change preserved + no false success.

## N-6 boundary
Batch executors carry no `if_match` today; the batch path is marked "no `if_match` (same
TOCTOU if extended)" — never a silent false-safety control. `verify_index_matches_disk`
stays pre-flight-only and is not advertised as a write guard.
