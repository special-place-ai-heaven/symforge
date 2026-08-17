# Data Model: Preventive Lifecycle Activation Cut

Derived from the spec's Key Entities and the frozen 020 contracts. Types named
here are contract vocabulary; exact field shapes are settled at implementation
against the frozen contracts, not invented here.

## Candidate (T060)

An isolated, capacity-reserved build of a source or whole project root.

- **Kinds**: full, delta (delta exact-validates only its changed source token
  and no-allocation-patches the latest whole project root).
- **Attributes**: reserved capacity, artifact certificates (complete set
  required), `CatalogPath` identity (native/opaque preserved end-to-end —
  scout → candidate → manifest → promotion, never lossily reconstructed),
  numeric epoch (diagnostic only — **never** authorizes publication).
- **Lifecycle**:
  `Building → {Promoted | Discarded}` with terminal discard causes forming the
  closed promotion matrix: `Unreadable`, `UnstableDuringRead`,
  `AbortedCircuitBreaker`, `ParseStatus::Failed`, unknown ordering, truncated
  required derivations, `PartialParse`, failed/panicked build, same-source
  drift (retry or abort).
- **Invariants**: exactly one runtime-store commit point; no capability
  certificate authorizes partial promotion; publish-before-prune; retry
  supersession (a newer attempt supersedes, never interleaves).

## Supervisor attempt record (T059)

Per-source ownership of loading: cancellation, attempt accounting, classified
failure, retry triggers. Attempt history is diagnostic data, structurally
separate from committed dispositions (feeds the US2 health split).

## Observer cut (T061)

- **Attributes**: stable token, monotonic cut boundary, scope-dirty latch,
  gap latch, coalesced accumulation window.
- **Lifecycle**: `Accumulating → Cut → HandedOff`; gap/scope-dirty latch →
  full successor baseline; exhausted capacity → safety transition (latched,
  never a silent drop). Predecessor drains before successor baseline.
- **Invariant**: ingress unwind retains observations (nothing observed is
  lost on unwind).

## Strict lease (T063)

- **Attributes**: exact selected-source set (bijection with the request),
  atomic multi-source capture, separate ranking snapshot, sealed
  completed-lease render authority.
- **States**: `Acquired → Completed | Refused(SourceRefusal)`.
- **Invariants**: success and no-match require every selected source
  `Current`; empty/missing/extra/mismatched `SelectedAggregate` → typed
  rejection; post-lease rendering may add `OutputCoverage::Truncated` only
  after completion and cannot change source-truth/candidate/cache/CCR
  identity; retarget races refuse.

## SourceMutationPermit (T064)

- **Holder**: only SymForge-owned structural edit/curation and
  init/root-ignore/`.gitattributes`/hygiene source-byte writes.
- **Protocol**: acquire fresh → publish non-Current → write → return through
  the isolated candidate pipeline. External observations never hold one.
  Cold/restart recovery cannot mint one and stays read-only until `Current`.
- **The forbidden shortcut** (documented residual): reacquiring a permit to
  return stored text — replay writes must reuse original authority, never
  mint new.

## Verification objects (T062)

- **VerificationScopeReceipt**: sealed scope; no pass may silently narrow it.
- **VerificationWorkBound**: computed seconds ≤ the reachable 712-second
  default.
- **VerificationFeasibilityReceipt**: required for promotion; a lost
  reservation forces non-Current rather than extending the deadline.
- **Deadline predicate (frozen FR-049)**: fixed 15-minute monotonic deadline;
  just-before eligible; at/after atomically latches
  `VerificationOverdueLatched` before any strict acquisition; only a complete
  exact-identity whole-scope `VerificationRecord` bound to its sealed receipt
  advances it; partial/cancelled/resumed work never extends it;
  policy-version mismatch → non-Current authoritative re-scout.

## Activation mode machine (T066)

- **States**: `LegacyOpen → LegacyClosing → PreventiveV1Open` (monotonic, no
  reverse edges, process-wide, non-configurable).
- **Registered lanes**: every tool/resource/prompt query, cache/CCR/retrieval,
  sidecar/hook, and finalization lane.
- **Companion invariant**: the two publication roots are never simultaneously
  authoritative; `LegacyClosing` is the drain window (legacy gate drains,
  cache/CCR invalidate, responses finalize).

## Typed authority branches (T050 matrix, the cut's ingress contract)

Closed set: `GenerationLeased`, `DiskObserved`, `WorktreeScopeObserved`,
`GitObserved`, `RuntimeHealthObserved`, `MutationPermitted`,
`StateWriteAuthorized`, `Refused`. Every ingress lane in the frozen inventory
resolves to exactly one.

## Untrusted seed (T065)

Any pre-existing V10 snapshot/cache/CCR byte at restart.

- **Lifecycle**: `Seed → {Verified(complete current-process proof) → usable |
  Quarantined(.symforge/v11/ namespace) → rebuild fallback}`; rollback
  preserved; pre-decode capacity enforced; root/digest mismatch quarantines.
- **Invariants**: runtime secret-canary bytes never enter snapshots,
  quarantine metadata, receipts, or diagnostics; excluded team-artifact bytes
  are `ProjectStateDir`-only with zero source-mutation authority; the frozen
  FR-051 four-state receipt/refusal matrix (`already_tracked`,
  `untracked_visible`, `ignored_force_add_required`,
  `git_visibility_unavailable`) is exact.

## Health projection (T063)

Two disjoint ledgers surfaced by health/health_compact/status/resources:
**committed generations** (observed promotions) vs **bounded attempts**
(supervisor diagnostics). Neither may be presented as the other (US2).
