# Contract: `docs/v8-capability-matrix.md` (US6, NEW)

**Surface**: the published capability record an operator / evaluating agent reads.

## Schema (one row per capability)
| Column | Rule |
|--------|------|
| Feature | A user-visible capability (e.g. "token economics prediction", "if_match guard", "status index health"). |
| Proof state | `Implemented \| Heuristic \| Observational \| Deferred` — what the code delivers. |
| Assumption ID | `A-0NN` from `docs/stel-assumptions.md`; one source of truth (FR-017). |
| Surface claim | What the shipped surface says — MUST NOT exceed Proof state. |
| Evidence | Artifact ref (test, measurement) for `Implemented`; none required for the others (they are honestly labeled, not proven). |

## Guarantees
1. Each capability maps to exactly one assumption ID and one proof state (FR-017, US6 AC-2).
2. The premise is framed as a **bet under test**: A-017 (surface premise) and A-011
   (predictor accuracy) stay OPEN — never presented as a proven win.
3. No row's Surface claim asserts validation while its Assumption is OPEN (feeds FR-018 /
   the CI gate).

## Source-of-truth rule
The matrix references assumption IDs; it does NOT restate proof states that could drift
from `stel-assumptions.md`. The register is authoritative; the matrix is the human-readable
projection.
