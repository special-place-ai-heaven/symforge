# Contract: honesty CI gate (US6, FR-018)

**Surface**: a CI check (`.github/workflows/...`) that fails the build on a dishonest
claim. Runs on PR + push, alongside the existing gates.

## Inputs
- `docs/stel-assumptions.md` (assumption IDs + proof states).
- `docs/v8-capability-matrix.md` (feature → assumption ID → surface claim).
- The shipped LLM-facing surfaces / docs that assert capabilities.

## Failure conditions (build FAILS)
1. A shipped surface or doc asserts a capability as **validated/measured** while its
   backing assumption is `OPEN` (FR-018, US6 AC-3).
2. A number/figure has **two divergent definitions** across surfaces (one-source-of-truth
   violation, FR-017).
3. A `VALIDATED` verdict in the register has no `artifact_ref` (FR-004).

## Must NOT fail on (allowed)
- A change that correctly **labels** a still-OPEN assumption as heuristic/observational/
  deferred. The gate keys on "claim of validation ⇒ proof exists", NOT on the presence of
  the word OPEN (spec edge case; relabel ≠ validate).

## Determinism
The gate is a static parse + cross-reference (no network, no flake). It is part of the
per-phase verification (FR-019) once Phase F lands, and guards against future regression of
the honesty work.
