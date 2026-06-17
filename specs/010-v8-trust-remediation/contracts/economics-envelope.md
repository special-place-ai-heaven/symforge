# Contract: economics envelope (US1, US5)

**Surface**: the token-economics envelope emitted with tool responses (planner +
envelope formatter).

## Field labeling (US1, Phase A — zero behavior)
| Field | Proof state | Rule |
|-------|-------------|------|
| predicted figures | `Heuristic` until grounded | Labeled `est_`/`heuristic`; a `400/800` constant is never presented as measured (FR-001, TR-04). |
| session running total | n/a (gross) | Named `session_tokens_served`; never `session_net_vs_manual`; no `+net` implying savings (FR-002, TR-05/TR-11). |
| any chars/4 value | approximation | Labeled "estimated tokens (chars/4)" (N-4). |
| `calibration` | `Deferred` | `CalibrationState` relabeled, seam kept (N-1, Do-Not #7). |

## Grounding (US5, Phase E — ground now)
1. Predicted figures are derived from real request/result size via the existing estimator
   `format.rs:4925-5029` (`competent_manual_baseline_chars` /
   `saved_tokens_vs_competent_manual`), wired into `planner.rs:44-55` (FR-014, D2).
2. Two materially different file sizes ⟹ two different predictions (SC-005, US5 AC-1).
3. At least one non-serve economics branch (`degrade`/`bypass`/`mandatory_degrade`) is
   reachable for an appropriately small/cheap request (US5 AC-2, TR-04b, N-2).
4. `expected_equiv` golden data is asserted by a test or removed (FR-015, TR-13).

## Invariant
No envelope field presents a constant or a gross counter as a measured saving. A figure
is `Measured` only if computed from real bytes at call time; otherwise it is explicitly
`Heuristic` (SC-001).
