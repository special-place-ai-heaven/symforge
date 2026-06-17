# Contract: `status` readout (US2, US1)

**Surface**: the `status` MCP tool output (compact + full), shared formatter.

## Inputs
- Active surface profile (compact | full).
- The **daemon** index facts (readiness, symbol/file counts) — via the existing proxy
  channel (TR-01 fix). NOT the front-end `self.index`.
- `ledger_store.summary()` real result (including the open-error, not swallowed — N-3).

## Output guarantees
1. `index_state ∈ {Ready, Empty, Loading, Unavailable}` reflects the index that **serves
   queries**. After any successful query, `index_state == Ready` and counts > 0 (FR-006,
   SC-002).
2. `source == Daemon` in the proxy topology; a `FrontEnd`-sourced empty read while serving
   is a contract violation.
3. On compact, `status` is present and reports real index health — there is no gap where
   "no honest index-health tool exists" (FR-007).
4. Subsystem states are enumerated (E4): `InMemory | Durable | Disabled(reason) |
   Unavailable`. `Disabled(open_failed)` ≠ `Unavailable` (FR-008, TR-17).
5. No unconditional `active`/`pending` literal (TR-10); `calibration: deferred` not
   `pending` (N-1); `empty_index_reason` present when empty (US4 input).
6. `deferred:` list is derived from real state — `ledger_persistence` removed when the
   durable store ships in serve mode (FR-004).

## Regression
`status_index_matches_daemon_proxy_after_symforge_serve`: start serve, run a query that
populates the index, read `status`, assert reported counts == the served index's counts
(non-zero, Ready).

## Non-goals
- Do NOT hide index fields to "fix" the mismatch (ledger Do-Not #6).
- Do NOT widen the compact wire surface beyond the targeted `status` proxy.
