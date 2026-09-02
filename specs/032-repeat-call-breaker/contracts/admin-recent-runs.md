# Contract: Admin `recent_runs`

Additive fields on `LedgerSummaryView` (`GET /api/v1/summary`, `src/server/admin/api_v1.rs:65-79`). Existing fields unchanged.

**Precondition (R12 adjudication)**: rows come from `StelLedgerStore::recent(limit)`, which returns `StoredLedgerRecord` — NOT `StelLedgerEvent` — ordered `ORDER BY id DESC` (newest first). This feature **widens `recent()`'s SELECT and `StoredLedgerRecord` additively** with `pff_bypass`, `cache_hit`, `degrade_flags_json` (columns that exist in the table but were not read back, `src/stel_core/ledger_store.rs:80-99, 724-748`) so the collapse identity does not silently merge rows that differ in them. `equivalence` has no column and is NOT part of the durable lane (documented coarseness); `accepted`/`eligible_h6` are deliberately excluded from identity and from this view (data-model.md Lane B).

```json
{
  "available": true,
  "total_events": 12,
  "total_net_vs_manual": 2520,
  "accepted_count": 10,
  "session_count": 2,
  "compression_heuristic": { "...": "unchanged" },
  "recent_runs_window": 50,
  "recent_runs": [
    {
      "count": 4,
      "first_ts_ms": 1788261021000,
      "last_ts_ms": 1788261029000,
      "window_clipped": false,
      "session_id": "stdio-12345",
      "surface": "symforge",
      "intent": "<stored string form, verbatim>",
      "decision": "<stored string form, verbatim>",
      "tools_called": ["find_references"],
      "route_confidence": "<stored string form, verbatim>",
      "pff_bypass": null,
      "cache_hit": null,
      "degrade_flags": []
    }
  ]
}
```

Rules:
- Fetch `recent(50)` (newest-first), **reverse to chronological (oldest→newest)**, then collapse with `session_id` in the identity (runs never span sessions). `recent_runs` renders chronological; `canonical`/`first_ts_ms` refer to the chronologically first row of each run.
- `recent_runs_window`: the raw-row fetch limit (50), always present when `recent_runs` is — the reader can see the reach of the view.
- `window_clipped: true` on the run containing the chronologically-oldest fetched row, **and only when the fetch actually filled the window** (rows fetched == `recent_runs_window`) — that is the only case in which the run's true extent may continue beyond the window. When the store holds fewer rows than the window, the oldest run was counted in full and is NOT labeled (labeling it would itself be an unobserved claim; refinement recorded 2026-09-02 during implementation). This is the spec's "never truncate silently" edge (SC-003): a clipped ×N is labeled, never passed off as a total.
- `recent_runs: []` whenever `available: false` or the store errs — never fabricated rows, never fake zeros (matches the existing view honesty). `recent_runs_window` may be omitted in that case.
- Per-run scalar fields (`session_id`, `surface`, `intent`, `decision`, `route_confidence`) are the stored string forms verbatim; `tools_called` is the parsed `tools_called_json` array; `pff_bypass`/`cache_hit` are the stored nullable booleans; `degrade_flags` the parsed `degrade_flags_json` array. `null`/`[]` here mean "stored as absent/empty", which is faithful — the widened SELECT reads the real columns; nothing is fabricated. **Refinement recorded 2026-09-02 during implementation**: `record()` bounds `tools_called_json` to 1024 bytes by truncation, so an unparseable stored form is representable; when the stored JSON does not parse as a string array, `tools_called` / `degrade_flags` render as `null` (withheld), never as `[]` — an empty list would claim "no tools called", which was not observed. Pinned by an in-file test.
- Excluded per-run: `id`, `ts_ms`, `plan_id` (replaced by `first_ts_ms`/`last_ts_ms` + `count`) and the four token measurement columns (they vary within a run; aggregates remain available in the untouched summary fields).
- Widening `StoredLedgerRecord` is additive; the existing row-level consumers (`tests/stel_l4_ledger.rs:241-245`, `tests/stel_ledger_persistence.rs`) are field-access asserts and gain fields without breakage — verified during implementation, not assumed.
