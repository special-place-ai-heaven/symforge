# Feature 020 Slice 4 — Activation-Cut Campaign Plan (v11)

Drafted 2026-08-17 at the close of Slice 3 (PR #582 merged, 10.6.1 released),
from a three-lens recon of `specs/020-repository-knowledge-index/tasks.md`
(Slice 4 roster at lines 931–956), the frozen contracts
(`v10-authority-retirement-v11.md`, `public-api-v11.json`,
`lifecycle-oracle-traceability-v11.md`), and the dark machinery under
`src/index_lifecycle/`. This document is campaign planning under
`docs/reviews/`; the spec tree is frozen and is never edited.

> **Execution status (as_of 2026-08-20)**: EXECUTED through T039, awaiting
> the T040 operator gate. The as-executed record (with every deviation
> from this plan) lives in
> `specs/028-preventive-activation-cut/activation-cut-execution-map.md`;
> the evidence and review record is
> `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` (T038
> CLOSED, three rounds, zero unresolved P0/P1/P2). Notable deviations:
> commit groups split beyond the planned sequence (C4a/b/c, C11a/b/c);
> 020:T065's live persist.rs format bump was discharged inside T038
> round 1 rather than a C-numbered group (the whole-snapshot seed gate is
> live; `SnapshotStore` per-entry wiring stays a recorded open residual);
> the embed test cell's frozen count moved 1336 → 1340 with the review
> rounds' new unconditional oracles.

## Mission and governing constraints

Slice 4 (T053–T073) enables the preventive lifecycle everywhere in one cut.
Binding constraints, verbatim from the frozen tasks file:

- "No merge or release may ship a refusal-per-edit full-rebuild phase, a
  legacy fallback, or mixed authority." (tasks.md:933-934)
- "Slice 4 is one enablement unit: no subset is independently shippable."
  (tasks.md:999-1001) — only the RED-test tasks T053–T058 are `[P]`.
- RED → observed failure → minimal GREEN → focused verification; an
  acceptance spec is not reported executed until its production seam exists.
  (tasks.md:836-838)
- The two publication roots must never be simultaneously authoritative, and
  PreventiveV1 has no in-place fallback. (quickstart.md:724-730)
- Execution evidence goes under `docs/reviews/`; the spec tree, including
  checkbox bytes, is immutable post-T012. (tasks.md:858-865)

## What already exists (the dark machinery inventory)

Slices 1–3 left the cut better prepared than the task names suggest. The
darkness invariant ("call edges, not grep hits", `src/index_lifecycle/mod.rs`)
holds via the `#[path]` mount from `src/live_index/mod.rs:25-26`, sealed by
`tests/preventive_runtime_dark_v11.rs`.

| Ready and hardened | Skeleton / dark refusals | Missing entirely |
|---|---|---|
| `authority.rs` five-state writer machine with A20 grants | `runtime.rs` DarkRuntimeFactory with recorded payload simplifications | `supervisor.rs` (T059) |
| `mutation.rs` permit lane (one obligation: `NoSideEffectProof` is a declaration, must move behind the real write lane) | `embedded.rs` handle whose query/refresh ops are honest dark refusals | `candidate.rs` (T060) |
| `physical_root.rs` cap-std beneath-confinement | `public_api.rs` wrap_table — the activation work list, 30 atoms with obligations | `observer.rs` (T061) |
| `transition.rs` Freeze→Drain→Install | `claim_provenance.rs` sealed types, production-unreachable | `verification.rs` (T062, the emptiest — zero existing code) |
| `registry.rs` single-flight admission, tombstones | | `query.rs` (T063 wiring) |
| `capacity.rs` accounting ledger (never blocks; T069 peak accounting pending) | | `activation.rs` (T066 mode machine) |
| `process_runtime.rs` one capacity domain, four doors | | |
| `read_gate.rs` — already LIVE production for every disk/git content fetch | | |

Key pre-plotted flips: `server_api` `pub(crate)`→`pub` (`lib.rs:31-32`); the
`#[path]` mount becomes a private `lib.rs` mod with `embed.rs` re-exports; the
`*_for_test` cfg tightening recorded at `runtime.rs:57-66`. Snapshot version
lives at `src/live_index/persist.rs:31` (`CURRENT_VERSION: u32 = 7`), with
quarantine machinery already in place for the T065 `.symforge/v11/` bump.

`LegacyOpen` exists nowhere in `src/` — it names the implicit always-open V10
state. T066 writes the `LegacyOpen → LegacyClosing → PreventiveV1Open` machine
fresh in `activation.rs`, process-wide and non-configurable.

## Campaign structure

Two waves, honoring both the darkness discipline and "one enablement unit":

**Wave 1 — dark machinery, landable incrementally on `main`.** Exactly the
Slice 1–3 pattern: behavior-neutral additions inside `src/index_lifecycle/`,
each PR extending the whole-source seal and the traceability census. Pairs of
RED tests with the machinery that turns them green (a RED test alone cannot
land — CI must stay green):

1. T053 RED + T059 supervisor + T060 candidate pipeline (the promotion
   matrix, opaque-path identity, capacity-reserved isolated builds, delta
   exact-validation, no-allocation root patch).
2. T054 RED + T061 observer (coalescing accumulator, monotonic cuts,
   gap latches, stable handoff; adapts the live `src/watcher/`).
3. T055 RED + T062 verification (15-minute monotonic deadline, sealed scope
   receipts, 712-second work bound — built from nothing).
4. T056 RED + T063 query leases (sealed types exist; the work is protocol
   and health wiring kept dark behind the factory).
5. T057 RED + T065 snapshot V11 migration (persist.rs is mature; namespace,
   untrusted-seed restore, FR-051 four-state matrix).

**Wave 2 — the indivisible cut, one branch, one PR, Slice-3-style.** Branch
`feature-020-slice-4-activation`, developed with the same evidence discipline
that closed PR 4 (frozen-source seals, TC-receipted gates, immutable
non-closure candidates, fresh adversarial review rounds):

- T064: route every ingress write through `SourceMutationPermit`; every
  read through leases; watcher/sidecar/init/hook/edit/live_index plumbing.
- T066: the activation mode machine plus the INV-SURFACE 11-member decision
  and the 16 source-free-mode dispositions.
- T067: retire all 244 inventory members (13 categories, dispositions frozen
  per category), expose the 64 V11 embed atoms with exact-graph equality
  across 26 configuration cells, flip the pre-plotted keywords.
- T058's four `#[ignore]` stand-ins in `tests/activation_cut_v11.rs` gain
  observing bodies and go live in the same range.
- T068–T071 gates inside the branch: `ObservedRefreshGateV1` benchmark
  (p95 ≤ 2s), capacity peak accounting, delta/full-rebuild equivalence,
  activation campaign; then T072 runs the campaign through the T050 matrix.
- T073 migration docs close the slice.

Residual families the cut must resolve (carried from Slice 3 evidence):
D16 cross-process publication atomicity (the structured activation boundary),
D14 live-observer invalidation proof (T056/T063), the eight-ingress edit
replay authority (T058/T064/T066/T072 — the forbidden shortcut of
reacquiring a permit to return stored text is documented), cancelled
non-abortable `index_folder` outcome (activation epoch or authoritative
ACTIVE re-sync), and the repeat-cache/CCR publication-identity fence.

## Effort map (grounded in the machinery inventory)

| Class | Tasks |
|---|---|
| Small, pre-plotted | T065 snapshot bump; T067's keyword flips proper |
| Medium | T059 (a move of existing loader logic), T063 (wiring over sealed types) |
| Big builds | T060 candidate pipeline, T061 observer, T062 verification, T066 activation machine (modest code, largest blast radius) |
| Big adaptations | T064 (touches watcher/sidecar/init/hook/edit/live_index), T067 (member-by-member across `daemon.rs`, `main.rs`, `sidecar/`, `cli/hook.rs`, `embed.rs`, `lib.rs`, guided by the frozen 244-slot matrix) |

## Risks and standing lessons

- **Platform-blind cfg code**: PR 4 shipped two `cfg(unix)` defects invisible
  to every Windows-side gate and five review passes; Linux CI was the first
  executor. Slice 4 adds substantial cfg-bearing code — every review round
  must include a cfg-lens sweep, and never-executed test bodies are treated
  as unverified claims.
- **Out-of-band recomputations**: the seal-recompute drift incident — only
  the Rust oracle knows; validate every recompute against it on a known
  input before pinning.
- **Squash-merge bodies**: always explicit `--subject`/`--body` (CLAUDE.md,
  measured on #582); the release pipeline silently drops unparseable commits.
- **T069 capacity**: the ledger deliberately never blocks; if the gate needs
  parking/drain barriers, that is new machinery, not a flag flip.
- **External approval**: release closure (T078–T090, outside this slice)
  re-runs refreeze with trusted external approval; approval sequence 4 is an
  unsigned draft that must be re-targeted at the release commit.

## Immediate next actions

1. Wave 1 pair 1: author T053's RED oracles (`index_candidate_lifecycle_v11.rs`
   with the frozen names `closed_candidate_promotion_matrix`,
   `opaque_non_utf8_path_identity_is_lossless`), observe RED, then build
   T059 supervisor + T060 candidate to minimal GREEN, extend the darkness
   seal, land dark.
2. Repeat for pairs 2–5 in any order (they are `[P]`), serializing the
   shared-file edits.
3. Open the Wave 2 branch only when every Wave 1 pair is dark-landed and the
   T058 stand-ins are the only ignored tests left.
