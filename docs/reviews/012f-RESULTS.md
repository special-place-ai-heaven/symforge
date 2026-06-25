# Sprint 012f — Results (Culprit-B frontier: the overlay reckoning + the real recall defect)

**Date:** 2026-06-24/25. **Lane:** `E:\project\symforge-012`. **Campaign:** SymForge trust campaign.
**Headline:** the adversarial-review process caught the *campaign itself* about to ship no-op work
(the per-session overlay), forced an honest root-cause reckoning, and redirected effort to the real,
reproduced trust defect (D13 xref recall). Every item adversarially verified; the workflow pattern
repeatedly caught partial fixes, scope lies, and silent degrades before merge.

## What landed (all on `main`, release-please cut v8.8.4 → v8.9.4)

| Item | PR | Release | Outcome |
|------|-----|---------|---------|
| D15 overlay-WRITER | #372 | v8.8.4 | Writer + single-project read landed — then found REDUNDANT (below) |
| Overlay-redundancy decision + fix | #375 | v8.9.0 | Removed the redundant D15 read (staleness shadow); writer kept as a DORMANT SEAM; finding recorded |
| D13 xref recall | #377 | v8.9.1 | **The real #1 defect.** find_references now recalls qualified-call construction sites (Rust + C++) |
| D18 loud not-found | #379 | v8.9.2 | get_symbol on a missing symbol says so, instead of a silent content-anchored window |
| D20 search_files path_prefix | #381 | v8.9.3 | Pre-count path scoping (true sibling mirror) |
| bases-table orphan GC | #383 | v8.9.4 | SC-002-safe sweep on both orphan paths (commit-advance + last-session-close) |
| R3 recall-confidence caveat | (this PR) | pending | Honest, targeted caveat on best-effort find_references usage traces |

## The keystone finding: the overlay was redundant

A D-B0 design skeptic, then a dedicated 5-agent root-cause investigation (refuter + vision + re-derive
→ synthesis → skeptic, `CONFIRM_REDUNDANT`), proved with line-pinned evidence that SymForge's
per-session base+overlay `IndexView` overlay is **redundant in production**:

- Every edit writes through to the SHARED live index synchronously (`reindex_after_write` →
  `index.update_file`). Single-project reads consult that same live index, so they already see the edit.
- Cross-project reads go through `IndexView(entry.base + overlay)`, but B2/D12's
  `refresh_working_set_bases` re-interns `entry.base` from the live index on every advance **and empties
  the overlay** — so the refreshed base already has the edit and the overlay is never the sole source.

The overlay presupposes **session-private edits** (edits land only in the overlay until committed); the
real edit path writes to the shared index immediately, so the overlay has no production consumer. Two
sprints (B2, D15) built session-isolation machinery the architecture does not use.

**Lesson (ledger invariant):** *A read overlay is dead until edits are commit-gated to it; while the
writer also writes the shared base, the overlay can never be the sole fresh source — build the commit
gate first, or the overlay is a no-op (and a staleness shadow).*

**Disposition:** D15 = DORMANT SEAM (writer kept gated/byte-identical + `#[cfg(test)]` coherence
assertion; the redundant + staleness-risky single-project read REMOVED). D-B0 / read-path-migration /
D16-full = honestly PARKED as blocked-on-a-real-overlay-writer (precondition #1: edits land in the
overlay, not the shared base) — capability deferrals with a stated path, not silent drops. Full evidence:
`docs/reviews/overlay-redundancy-decision.md`.

## The real frontier delivered: D13 (reclassified out of Culprit B into its own root)

D13 was a reproduced silent-wrong answer on the most trust-critical operation ("who uses X"):
`find_references(Type)` returned only definition lines and missed construction sites (~29% recall). The
ledger had **misattributed** it as a Culprit-B/overlay win — the skeptic caught that; it is a parser/
lookup defect, fixed with zero overlay/view involvement:

- Root: qualified calls (`Foo::new()`) were keyed under the leaf (`new`), not the type head.
- Fix: `find_references` matches qualified-call sites by their **immediate qualifier** (segment before
  the leaf) at any path depth (`X::method()`, `a::b::X::method()`), Rust + C++.
- The audit's "struct literals never captured" premise was empirically FALSE — the unscoped
  `(type_identifier)` capture already lands them; zero query churn there.

## Honesty wins (loud-not-silent)

- **D18:** missing-symbol get_symbol → loud not-found (auto-classified `NotFound`), not a silent window.
- **R3:** find_references type/value-usage traces carry a targeted recall-confidence caveat (not blanket
  noise, no false precision); mirrors the kind filter so a typo'd kind cannot silently ship an unmarked
  best-effort trace.

## How the process performed (the campaign's own thesis, applied to itself)

The trust campaign exists to kill no-op / fake-success / silent-degrade work. The
workflow+adversarial-verify pattern caught, *before merge*, the campaign itself about to commit exactly
those: the redundant overlay (whole direction), D13's C++ scope misrepresentation, D13's partial
first-segment head-match, D20's overflow-set scoping escape, bases-GC's missed session-close orphan
path, and R3's own unrecognized-kind silent-degrade. Each was an adversarial-review catch, not a
post-merge regret.

## Honest caveats

- Local `cargo test --all-targets` / `build --release` were deferred to CI on several items when the
  shared E: drive ran tight (it hit 100% once mid-sprint — `cargo clean` reclaimed 22 GiB); CI is the
  authoritative full gate, green on every merged commit. Policy adopted: `cargo clean` after each build
  item rather than holding a warm cache, to protect the shared 4-agent disk.
- D15's overlay writer remains a live (gated) dormant seam doing a parse+upsert nothing reads on the
  daemon path — a marked, roadmap-tracked seam (not false success); tighten to inert if it ever costs.
- The `strong_count == 1 == orphan` GC invariant is coupling-fragile (documented at the call site): a
  future `Arc<IndexBase>` holder would need to re-check it.

## Next frontier

The overlay/IndexView track unblocks only when **precondition #1** lands: a real overlay WRITER where
edits land in the per-session overlay and are NOT written through to the shared base (commit-gated
session isolation), which is also the substrate for **D16-full** (`/mcp` per-connection session) and
makes **D-B0** (per-view derived index for non-empty overlays) live-verifiable. Until then, those stay
tracked-large/blocked. See the updated handoff + ledger.
