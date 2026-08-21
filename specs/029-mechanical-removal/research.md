# Phase 0 Research — Feature 020 Slice 5 (Mechanical Removal)

Every unknown in the Technical Context is resolved here, or recorded as an
observation the plan's first executed step must make. Nothing below is inferred
from task wording; each decision cites the artifact that establishes it.

---

## R1 — What does "do not change public behavior" mean mechanically?

**Decision**: It means the derived public atom set must continue to equal, byte
for byte, the frozen **postactivation** set — not merely "stay close to it".

**Rationale**: `scripts/validate-lifecycle-oracle-traceability.cjs` already owns
this as a three-state check (`ordinaryRetirementLifecycle`). It derives the
actual public atom set from the source map and compares it against exactly two
frozen sets:

- **preactivation** — every migration atom in the manifest;
- **postactivation** — the atoms of every category whose `decision` is `"keep"`,
  plus `introduced_v11_atoms`.

Anything that is neither fails closed with
`RETIREMENT_LIFECYCLE_PHASE_INVALID: public API is neither frozen preactivation
(N) nor postactivation (M); actual=K`.

This has a consequence sharper than the roster's prose, and it is the single
most useful thing this research produced:

> **The postactivation set is defined as kept ∪ introduced. Every public atom
> that survives the cut is therefore already required to exist by the frozen
> contract. Slice 5 cannot remove a public atom at all** — removing one drops
> the tree out of both accepted states and fails the gate.

So Slice 5's removal surface is **strictly non-public code**: private items,
crate-internal helpers, unreachable branches behind public entry points, and
tests whose subjects go with them. "Do not change public behavior" is not a
caution to be careful; it is a closed set the checker enforces.

**Alternatives considered**: treating the retirement inventory's `disposition`
fields as the removal authority. Rejected — dispositions describe what the
*cut* had to do to each member (route / remove / retire). They are not a
statement about what may be deleted from the tree afterwards, and several
members with retirement dispositions are public atoms the postactivation set
still requires.

---

## R2 — Which sealed values move when source is deleted, and who may recompute them?

**Decision**: Two whole-source pins in `tests/preventive_runtime_dark_v11.rs`
move on any `src/` deletion, and only the owning Rust oracle may recompute
them:

| Pin | Shape | Current recorded value |
|---|---|---|
| `FULL_SOURCE_PIN_V1` | (digest, file count, byte count) | 196 files, 9 300 142 bytes |
| `EXCLUDED_RUNTIME_SOURCE_PIN_V1` | (digest, file count, byte count) | 20 files, 388 720 bytes |

**Rationale**: Constitution Principle V forbids out-of-band recomputation of
sealed values outright — "the Rust oracle that owns the seal is the only
recompute authority, and `rustfmt` runs BEFORE any pin refresh". The rule
exists because a hand-rolled recompute silently diverged from the oracle once
already. The pins carry counts as well as a digest, which is what makes a
refresh auditable: a removal must move the file/byte counts *downward by the
amount removed*, and a refresh whose counts move the wrong way is evidence of
a mistake rather than a formality to be updated.

**Alternatives considered**: excluding removed files from the seal's scope so
the pin does not move. Rejected — that hides the removal from the very seal
whose job is to notice source changes.

---

## R3 — Where does the reachability evidence Slice 5 consumes actually live?

**Decision**: In the executed Slice 4 reachability cases, not in the
preactivation census.

**Rationale**: `contracts/v10-authority-retirement-v11.md` states it directly:
"After activation, the executed Slice 4 reachability cases replace the
preactivation census." The traceability checker's `source_anchor_policy` says
the same from the other side: "Every preactivation V10 `src/` retirement member
resolves on the refreeze tree; release evidence instead covers every frozen V11
production seam with one same-tree source receipt after retirement."

Two consequences the plan depends on:

1. **Deleting retired V10 code cannot break the census.** Its anchors resolve
   against the externally approved refreeze ancestor tree, which deletion on
   the current tree cannot alter.
2. **Deleting a frozen V11 production seam breaks the release evidence**,
   because those require a *same-tree* source receipt. This is the real
   tripwire, and it is why spec FR-005 exists.

**Alternatives considered**: re-deriving reachability from scratch with a
call-graph sweep. Rejected as both redundant and weaker — Slice 4 already
executed the cases under review, and a fresh grep-based sweep would be exactly
the "looks unused" evidence FR-004 forbids.

---

## R4 — Is there dead V10 embed implementation left for T076?

**Decision**: Unresolved by research, and deliberately left so. The plan's
executed step must observe it.

**Rationale**: `src/embed.rs` is 163 lines with a small surface
(`EngineInfo`, `engine_info()`, and a `contract` module whose
`facade_contract_is_stable` test asserts the shape). That is consistent with
Slice 4's T067 having already retired the raw embed update/remove exports, but
consistency is not proof, and the frozen ordering forbids acting before the
allowlist negative suite has spoken.

Recording this as an unresolved observation rather than a conclusion is the
point: spec FR-011 requires a discharged expectation to be evidenced, and an
expectation cannot be discharged by a plan that assumed the answer.

**Alternatives considered**: asserting in the plan that T076 is already
complete. Rejected — that is a success claim for an operation nobody observed,
which Principle I forbids in exactly these words.

---

## R5 — How is the neutrality bracket shown to work before it is trusted?

**Decision**: By a deliberate control edit that the comparison must catch, run
before any real removal, and discarded afterwards.

**Rationale**: Principle II requires every negative test to carry its accepting
positive control in the same test, because "a system that refuses everything
satisfies a lone negative perfectly". A neutrality bracket is a negative
instrument — it reports "nothing changed" — so an always-quiet bracket passes
its own check perfectly while proving nothing. The control is what separates
an enforced comparison from a vacuous one.

The control must move a field the bracket claims to cover, and the bracket must
name that field. A control that trips the build instead of the comparison
proves nothing about the comparison.

**Alternatives considered**: trusting the comparison because it is
mechanically derived. Rejected on precedent — `format_search_envelope` was
mechanically derived too, and collapsed to a confident banner on a string
comparison it never measured.

---

## R6 — What gate set applies, and what is the ordering constraint?

**Decision**: The project's existing gate set, unchanged, run one cargo
invocation at a time with long runs through Terminal Commander.

| Gate | Purpose here |
|---|---|
| `cargo fmt --check` | Must run **before** any pin refresh (Principle V) |
| `cargo clippy --all-targets -- -D warnings` | Removal commonly orphans an import; this is where that surfaces |
| Full serial suite (`--test-threads=1`) | Behaviour |
| `cargo test --no-default-features --features embed --lib` | The cfg cell no default-feature gate can see |
| Release build + `verify-tools.cjs` | Tool correctness |
| `node scripts/validate-lifecycle-oracle-traceability.cjs` | The R1 three-state public-API check |
| `node scripts/slice0-oracle-artifact.cjs` | Fails closed if a control's status flips |
| npm suite | Package surface |

**Rationale**: Principle IV fixes the set and the serial discipline; the
ordering constraint (`fmt` before pin refresh) comes from Principle V. The
slice adds no gate and relaxes none — a removal slice that needed a new gate
would not be a removal slice.

**Alternatives considered**: running only the gates a removal "could plausibly
affect". Rejected — the whole premise under test is that the removal affects
nothing, so narrowing the gates assumes the conclusion.

---

## Resolved Technical Context

| Field | Value |
|---|---|
| Language/Version | Rust (repository `rust-toolchain.toml`), with Node and Python for the checker and refreeze tooling |
| Primary Dependencies | None added; this slice removes only |
| Storage | N/A |
| Testing | `cargo test` serial, plus the Node checkers and the npm suite |
| Target Platform | Linux CI is authority; Windows and macOS cells gate additionally |
| Project Type | Single Rust crate with an MCP server surface |
| Performance Goals | None — neutrality, not speed |
| Constraints | Public atom set must remain exactly the postactivation set (R1); frozen tree unedited; seals refreshed only by their oracle (R2) |
| Scale/Scope | Bounded by evidenced-unreachable non-public items; may legitimately be empty |

## Open observations the executed plan must make

1. Which lifecycle phase the tree reports today — `preactivation` or
   `postactivation`. R1 makes this decisive for whether the atom set may move
   at all, and it is cheap to observe but must not be assumed.
2. Whether any evidenced-unreachable non-public code actually remains.
3. Whether `src/embed.rs` still holds dead V10 implementation (R4).

All three are observations, not predictions. If the answer to 2 and 3 is "no",
the slice closes with the bracket, the baseline, and a recorded discharge —
which spec Edge Cases already admits as a valid outcome.
