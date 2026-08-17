# Contracts: binding by reference

This feature deliberately defines **no new contracts**. The activation cut's
interfaces are fully specified by the frozen Feature 020 contract set, which
is immutable and lives at:

- `specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md`
  — the 244-member retirement inventory (13 categories, per-category content
  digests pinned by `RETIREMENT_MEMBER_DIGESTS`).
- `specs/020-repository-knowledge-index/contracts/public-api-v11.json`
  — the 64 attested V11 embed atoms and exact-graph equality across the 26
  configuration cells.
- `specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md`
  — the frozen oracle registry; Slice 4's exact test/bench names are extracted
  into this feature's `research.md` (R4) for convenience, but the registry is
  authoritative.

Duplicating any of these here would create a second source of truth that can
drift; the frozen tree already pins them by digest. The binding chain that
enforces conformance is:

```
frozen contract → validate-lifecycle-oracle-traceability.cjs (retirement_records)
             → REFREEZE-MANIFEST-v11.md (sha)
             → FEATURE-020-REFREEZE-ATTESTATION-v11.md (pin)
             → tests/preventive_runtime_dark_v11.rs (FULL_SOURCE_PIN_V1 + census)
```

Any Slice 4 landing that touches contract-covered surface must reconcile that
whole chain (see quickstart.md); the emitter is
`SYMFORGE_LIFECYCLE_EMIT_CLOSURE=1`, and refreeze runs `verify-internal`.
