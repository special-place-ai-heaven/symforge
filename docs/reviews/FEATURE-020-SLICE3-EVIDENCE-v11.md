# Feature 020 Slice 3 evidence (T041–T052)

Living document for the slice; T052 completes it. Every claim here was observed,
not inferred. Where a command is cited, it was run on the named tree.

## T041 + T042 — observed RED (durable record)

The RED observation lives in branch commit `cdb3ff20`, which a squash-merge will
collapse, so the evidence is recorded here as well.

Command, on `cdb3ff20`'s tree (before `claim_provenance.rs` existed):

```
cargo test --test read_gate_authority_v11 --test claim_provenance_v11 --no-run
```

Observed output:

```
error[E0432]: unresolved import `symforge::protocol::format::claim_provenance`
   |                                 ^^^^^^^^^^^^^^^^ could not find `claim_provenance` in `format`
error[E0433]: cannot find `claim_provenance` in `format`   (x4)
error: could not compile `symforge` (test "claim_provenance_v11") due to 5 previous errors
```

Every error names the missing module and nothing else, so the RED was about the
absent types, not a malformed test.

## T043 — GREEN transition and the mutation ledger

After `src/lifecycle_identity.rs`, `src/protocol/claim_provenance.rs`, and the
`#[path]` anchor in `format.rs` landed, the same two files compiled and passed:
initially 22, then 23 after M2 forced a new oracle (below).

**Mutation ledger.** Each guard was flipped in production, the suite run, the
named oracle observed failing ALONE, and the guard restored. A guard whose
mutation survives is not enforced; one did, and the response was a new test, not
a shrug.

| # | Mutation (production change) | Expected catcher | Observed |
|---|---|---|---|
| M0 | `AtomicAuthority::proves_repository_absence` → `true` | `no_local_negative_receipt_can_be_widened_to_repository_absence` | CAUGHT — that test alone failed, message named `DiskObservation` |
| M1 | empty-derivation refusal disabled (`if inputs.is_empty()` → `if false`) | `a_derivation_refuses_an_empty_input_set` | CAUGHT — alone, 11 held |
| M2 | bijection LENGTH check disabled (`false && captured.len() != …`) | — | **SURVIVED. 12 passed.** See below. |
| M2' | same mutant, after the new oracle | `a_selected_aggregate_refuses_an_extra_unselected_generation` | CAUGHT — alone, 12 held |
| M3 | `roots_are_compatible` → always `true` | `a_derivation_across_two_roots_is_refused_rather_than_composed` | CAUGHT — alone, 9 held in its file; other file 13 green |
| M4 | `render_bounded` mints a fresh `ProvenanceIdentity` | `truncated_coverage_never_enters_a_claim_identity` | CAUGHT — alone, 12 held |

Final state after all restores: **23 passed, 0 failed** across both files.

**The M2 survival was a real test gap, not a weak mutant.** The bijection
condition is `len_mismatch || !all_contained`; the mutant disabled only the
length half, and the containment half caught the only fixture the suite had
(missing generation). The length guard alone is what catches an EXTRA captured
generation nobody selected — "Missing, extra, forged, or uncaptured inputs
refuse" (`data-model.md:1893`) — and no test exercised that arm. The new oracle
`a_selected_aggregate_refuses_an_extra_unselected_generation` was written while
the mutant was live, observed catching it, and kept.

## T043 stand-ins that must not be "completed" casually

- **`ObservationLease::completed_render_authority` always returns `Ok`.**
  `OutputCoverage::Truncated` is gated on holding a `CompletedRenderAuthority`;
  in Slice 3 that token is obtainable from any `ObservationLease`, because the
  real strict-lease machinery is Slice 4 (T047/T060). The gate is the TYPE, not
  a runtime check. Do not "complete" this method by adding a fake check that
  pretends to verify lease completion it cannot observe — that is the reporting
  defect this feature exists to prevent. Slice 4 replaces the constructor's
  evidence, not its shape.
- The other lease constructors (`observe_missing_path`, `complete_scope_scan`,
  `admit_generation`) are the same shape: sealed constructors whose *evidence*
  arrives with the real runtime. Their `Result` returns exist so the signatures
  do not change when the evidence does.

## Deliberate decisions in force (recorded before code was written)

- **D3** — `DerivedLimitKind`/`LimitBreach` are the LIVE eight-variant types from
  `live_index::knowledge_bridge`, imported, never transcribed. The frozen six is
  stale; a later corpus amendment may add the two names. Confirmed by the
  compiler: the integration crate imports the live type directly.
- **D9** — where `data-model.md` and `contracts/public-api-v11.json` disagree,
  the ATOMS win (opaque `SourceRefusal` + `SourceRefusalKind` + `RetryAdvice`,
  `Claim::producing_runtime_identity`), because the activation rule is
  machine-enforced and the prose is not. Neither document was amended.
- **One identity counter** — `identity_newtype!` and `NEXT_IDENTITY` moved to
  `src/lifecycle_identity.rs` (`pub(crate)` in `lib.rs`, so the public-API
  census gains no atom); `index_lifecycle/authority.rs` re-exports its six
  identities from there. No `protocol → index_lifecycle` call edge exists, so
  T051's darkness proof is intact.
- **The `#[path]` anchor lives in `format.rs`**, not `protocol/mod.rs`
  (censused; also `read_gate` is `pub(crate)` so the oracles — a separate crate —
  could not see the module through it).

## The traceability catalog caught an invented name (T041)

First run of `node scripts/validate-lifecycle-oracle-traceability.cjs` on the
T043 tree FAILED:

```
ERROR PLANNED_TEST_CASE_MISSING: trace.catalogs.tests.TEST-PROVENANCE:
tests/claim_provenance_v11.rs::operation_contract_cartesian_matrix
```

The frozen catalog pins `TEST-PROVENANCE` to that exact function name
(CMD-PROVENANCE, owner T041, `introduced_slice: 3`), and the pin activates the
moment the FILE exists. The Cartesian test had been written under an invented
name — the Slice 2 failure mode, caught by the machine this time. Renamed to the
pinned name and WIDENED to match it: the pinned name says OPERATION contract, so
the operation kind became an axis — 4 operations x 4 refusal kinds x 3 retry
advices, `seen == 48`. The pinned command was then run verbatim and observed:
`cargo test --test claim_provenance_v11 operation_contract_cartesian_matrix --
--exact` -> `1 passed; 0 failed; 12 filtered out`.

## Embed-gate result, and why it passes by design

Prediction before running: FAIL, because the nine new `lifecycle_identity` items
are consumed only by `claim_provenance`, which sits under the server-gated
`protocol` module. Observed: **PASS, 1332 passed, 0 failed** (up 3 — the new
module's own unit tests run under embed).

The prediction missed `src/lib.rs:4`:
`#![cfg_attr(not(feature = "server"), allow(dead_code))]`, whose comment states
the policy: under embed an embedder uses a subset of the engine API, so
unused-but-public helpers are expected, not dead. `protocol` IS absent under
embed (`lib.rs:67`), and the identities are idle there BY DESIGN. No cfg-gating
of the new items is needed, and none was added.

## Gate results for the T043 chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean, 29s warm |
| embed lib gate (`--no-default-features --features embed --lib`) | 1332 passed, 0 failed, 4 ignored |
| traceability checker | OK (78 requirements, 24 oracles, 13 categories) — after the pinned-name fix above |
| pinned CMD-PROVENANCE, verbatim | 1 passed, 0 failed, `--exact` |
| both oracle files | 23 passed, 0 failed |
| five closure digests re-emitted | byte-identical to the pinned values |
| full `cargo test --all-targets` | — (runs before the PR, not per chunk) |
