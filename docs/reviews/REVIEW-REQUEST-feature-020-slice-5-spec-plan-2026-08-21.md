# Review request — Feature 020 V11, Slice 5 (mechanical removal): SPEC AND PLAN, pre-implementation

## Instructions to the reviewing model

**This is a READ-ONLY review. Write no code. Change no file except your own
findings file. Do not run `cargo build`, `cargo test`, or anything that takes
minutes — every verification you need is a `grep`, a `sed -n`, or a sub-second
node/python invocation, and the exact commands are given below.**

**Keep verbosity low. Do not explain the codebase back to me. Produce one file
and nothing else.**

Write your findings to:

```
docs/reviews/REVIEW-FINDINGS-<your-model-name>-feature-020-slice-5-spec-plan-2026-08-21.md
```

Use your own model name in the filename (e.g. `gpt-5-2`, `grok-4-6`, `kimi-k3`,
`composer`) so several independent reviews can sit side by side and be diffed.

Every finding uses exactly this shape:

```
### <BLOCKER|MAJOR|MINOR|NIT> — <one-line title>
- **Where**: path:line
- **Claim**: one sentence, falsifiable.
- **Why it matters**: one or two sentences.
- **Recommended fix**: concrete, minimal.
```

End the file with a `## Negatives` section listing, explicitly, the things you
checked and found sound. A silent omission is indistinguishable from not having
looked, which is why this section is mandatory.

If you find nothing at a severity, say so. Do not pad.

Additionally, end with a `## Verdict` section answering exactly two questions:

1. **Is the load-bearing claim in §4 true?** (yes / no / cannot determine, plus
   the command output that decided it.)
2. **Should this spec+plan be implemented as written, amended, or rejected?**

---

## 1. What you are reviewing, and what you are NOT

You are reviewing **two documents describing work that has not started**:

- `specs/029-mechanical-removal/spec.md`
- `specs/029-mechanical-removal/plan.md`

plus their Phase 0/1 supporting artifacts (listed in §3).

You are **not** reviewing an implementation. No code has been removed. No
baseline has been captured. The value of this review is catching a mis-scoped
or wrongly-reasoned plan **before** anyone spends a day executing it.

The most useful thing you can do is try to **falsify the central claim in §4**.
If that claim is wrong, the plan is mis-scoped and most of the rest of the
review is moot — say so early and loudly.

---

## 2. Context you need (the campaign, in six sentences)

SymForge is a Rust MCP server. Feature 020 ("repository knowledge index") is a
long campaign delivered in slices against a **frozen, immutable spec tree** at
`specs/020-repository-knowledge-index/`, which must never be edited — including
its checkbox bytes.

Slice 4 was the "activation cut": it switched the preventive index lifecycle on
everywhere in one indivisible change, and shipped as the breaking **11.0.0**
release. **Slice 5 is the follow-up: delete only the code Slice 4 proved
unreachable, and change nothing observable.** Its frozen goal text is:

> "Delete only code already proven unreachable in Slice 4; do not change
> runtime authority, public behavior, writer reachability, or activation mode."
> — `specs/020-repository-knowledge-index/tasks.md:960-961`

The project's governing rules live in `.specify/memory/constitution.md`
(v1.0.0, six principles) and `CLAUDE.md`. Two you should hold the documents to
hardest:

- **Principle I, the reporting invariant**: a component may not report success
  for an operation whose completion it did not observe. This applies to
  documents too — a spec that asserts an unobserved fact is the same defect.
- **Principle II, RED-first evidence**: every negative test must carry its
  accepting positive control in the same test, because "a system that refuses
  everything satisfies a lone negative perfectly."

---

## 3. Read these, in this order

| # | Path | Why |
|---|---|---|
| 1 | `specs/029-mechanical-removal/spec.md` | The spec under review |
| 2 | `specs/029-mechanical-removal/plan.md` | The plan under review |
| 3 | `specs/029-mechanical-removal/research.md` | Phase 0. **R1 carries the load-bearing claim** |
| 4 | `specs/029-mechanical-removal/contracts/neutrality-bracket-v1.md` | The evidence contract, C-1…C-7 |
| 5 | `specs/029-mechanical-removal/data-model.md` | Artifact shapes and state transitions |
| 6 | `specs/029-mechanical-removal/quickstart.md` | The run guide |
| 7 | `.specify/memory/constitution.md` | The six principles the plan claims to satisfy |
| 8 | `specs/020-repository-knowledge-index/tasks.md` lines **958–966** | Phase 7, the frozen roster (T074–T077). **Read-only — never edit this tree** |
| 9 | `docs/reviews/FEATURE-020-POST-V11-LEDGER.md` | What Feature 020 still owes overall; Slice 5 is "Track C" |
| 10 | `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` §5, §6 | What Slice 4 did **not** discharge, and its six recorded residuals |

Supporting source you may need to check claims against:

- `scripts/validate-lifecycle-oracle-traceability.cjs` — `ordinaryRetirementLifecycle` (~line 2054)
- `specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md`
- `specs/020-repository-knowledge-index/contracts/public-api-v11.json`
- `src/index_lifecycle/activation.rs` lines 1–110
- `src/embed.rs` (163 lines total)
- `tests/preventive_runtime_dark_v11.rs` lines 981–992 (the two source pins)

---

## 4. The load-bearing claim — attack this first

The plan's entire scope rests on **research.md R1**, which claims:

> The traceability checker defines "public behaviour unchanged" as equality
> against a frozen **postactivation** atom set. That set is defined as
> *kept ∪ introduced*. Therefore every public atom that survived the cut is
> already required by contract to exist, and **Slice 5 cannot remove a public
> atom at all** — its removal surface is strictly non-public code.

If this is true, the slice is tightly bounded and the plan is right to invert
its priorities. If it is false — if public atoms *can* legitimately leave the
set, or if the tree is currently in the `preactivation` phase rather than
`postactivation`, or if `deriveLifecyclePublicAtoms` does not mean what R1
assumes — then the plan is mis-scoped and should be rejected or heavily
amended.

Verify it yourself. These are all sub-second:

```bash
# The three-state check the claim rests on
sed -n '2054,2072p' scripts/validate-lifecycle-oracle-traceability.cjs

# What "kept" and "introduced" actually resolve to
python -c "
import json
d=json.load(open('specs/020-repository-knowledge-index/contracts/public-api-v11.json'))
m=d.get('migration_v10',{}); cats=m.get('categories',[])
kept=[c for c in cats if c.get('decision')=='keep']
print('categories:',len(cats))
print('kept ids:',[c['id'] for c in kept])
print('kept atoms:',sum(len(c.get('atoms',[])) for c in kept))
print('introduced:',len(m.get('introduced_v11_atoms',[])))
"

# Where the atoms are derived from — source map, or manifest?
sed -n '2037,2052p' scripts/validate-lifecycle-oracle-traceability.cjs

# Does the checker pass today, and what does it report?
node scripts/validate-lifecycle-oracle-traceability.cjs
```

Expected output of the python block at time of writing: 12 categories, 3 kept
(`v10-00-crate-root`, `v10-02-embed-module`, `v10-03-engine-info`) contributing
4 atoms, plus 64 introduced. If your numbers differ, the tree has moved and you
should say so — that alone would be a finding.

**Specific things to try to break:**

- Does `deriveLifecyclePublicAtoms` derive from *source* or from the *manifest*?
  R1's conclusion only follows if it derives from source. My own reading says
  line 2038 calls `derivePublicApiAtoms(sourceMap)`, and lines 2044–2049 then
  scan module source for additional `pub` items — i.e. source-derived, so R1's
  premise holds. **Check this independently; do not take my word for it.**
- Note lines 2044–2049 add atoms by *regex over source* for introduced modules,
  excluding `embed`. Does that regex path create a way for a non-public-API
  edit to move the set, or for a genuinely removable item to look public? This
  is the most likely place R1 is subtly wrong.
- Is the tree currently `preactivation` or `postactivation`? The plan admits it
  does **not** know and defers this to an executed observation
  (research.md "Open observations", item 1). Is deferring it correct, or is it
  cheaply knowable now and therefore a gap the plan should have closed?
- Does any other gate constrain the public surface in a way R1 missed
  (`execution/refreeze_v11.py`, the `tests/fixtures/public-api-v11-consumer/`
  compile-fail cases)? R1 may be *right but incomplete*, which is a MAJOR, not
  a BLOCKER.

---

## 5. Two design decisions I want explicitly adjudicated

Both are deliberate. Both may be wrong. **Say so plainly if they are** — I
would rather hear it now than after implementation. Do not soften a real
objection into a NIT, and do not manufacture an objection to seem rigorous: if
a decision is sound, put it in `## Negatives` and say why.

### 5.1 The bracket, not the removal, is the P1 deliverable

The spec makes the **neutrality bracket** (User Story 1) the P1 slice, ahead of
any actual deletion (US2). The argument: a bracket is a negative instrument —
it reports "nothing changed" whether or not it works — so it must be shown
detecting a deliberate control change *before* it may certify the absence of
one (spec FR-003, SC-002, contract C-1).

**Adjudicate**: Is making the measurement instrument the primary deliverable of
a *removal* slice correct discipline, or is it over-engineering that inflates a
mechanical cleanup into a methodology exercise? Consider that the frozen roster
(T074/T077) already mandates a baseline and a re-run — does the added control
requirement earn its cost, or is it ceremony bolted onto an existing gate?

### 5.2 C-7 declares an empty removal a passing outcome

Contract clause C-7 states that removing **nothing** is conforming, provided
the bracket is armed and every roster prediction is discharged with evidence.
The stated rationale is that a slice which finds nothing to delete feels like a
failure, and the cheapest way to make it feel successful is to delete something
unevidenced.

**Adjudicate**: Is this honest scoping, or is it a pre-authorized excuse to do
nothing and still claim the slice closed? Specifically — does C-6 (a predicted
removal that does not happen must leave a `DischargedExpectation` with
evidence) actually have teeth, or is it a formality that a lazy executor could
satisfy with a one-line "already gone" and no observation?

---

## 6. Other properties worth attacking

- **The `LegacyOpen` finding.** The plan asserts that T075's "remove legacy mode
  branches" must NOT mean `ActivationMode::LegacyOpen`/`LegacyClosing`, because
  those are the live bootstrap states of the machine the cut installed. Verify
  against `src/index_lifecycle/activation.rs:1-110`. If the plan is wrong here,
  it is a BLOCKER in the opposite direction — the slice would be excluding a
  legitimate target.
- **The census claim.** research.md R3 asserts that deleting retired V10 code
  cannot break the retirement census, because preactivation anchors resolve
  against the approved refreeze *ancestor* tree. Check
  `scripts/validate-lifecycle-oracle-traceability.cjs` around line 209
  (`source_anchor_policy`) and the retirement contract. Is the asymmetry with
  V11 seams (R3 consequence 2) real?
- **Pin arithmetic.** Contract C-5 requires the two whole-source pins' file and
  byte counts to move *downward* by the removed amount. Is that actually true
  of how those pins are computed — e.g. does `EXCLUDED_RUNTIME_SOURCE_PIN_V1`
  cover a disjoint file set such that a removal could move one pin and not the
  other, or move one in an unexpected direction?
- **Unfalsifiable success criteria.** Several criteria are "zero X" (SC-003
  through SC-007). Is each actually *measurable*, or is any of them a claim
  nobody can check after the fact?
- **Gaps.** Is anything in the frozen T074–T077 roster **not** covered by the
  spec's requirements? Map each frozen task to the FRs claiming to cover it and
  report anything orphaned.
- **The Constitution Check.** `plan.md` claims PASS on all six principles. Pick
  the two you find least convincing and argue them.

---

## 7. Known and deliberate — challenge these only if you disagree

Listed so you do not spend effort re-reporting them as discoveries. Each is a
conscious decision with a stated reason; challenging one is welcome, restating
it as a finding is not.

- The spec quotes contract identifiers and file paths despite the spec template
  saying "written for non-technical stakeholders". Reason: Constitution III
  requires exact quoting; paraphrasing a contract identifier into friendlier
  wording makes it wrong. The qualification is written in
  `specs/029-mechanical-removal/checklists/requirements.md` Notes.
- `research.md` R4 deliberately leaves "is there dead V10 embed code left?"
  **unresolved**, rather than predicting it from `src/embed.rs` being 163
  lines. Reason: Principle I — predicting it would be a success claim for an
  unobserved operation.
- Complexity Tracking is omitted from `plan.md` rather than included empty,
  because the Constitution Check found no violations.
- The slice is explicitly **not** indivisible (unlike Slice 4); candidates may
  land in separate changes.

---

## 8. Repository facts you will need

- Do **not** trust any SHA, branch name, version, or PR number written in any
  document. Generate current state with:
  ```
  pwsh scripts/campaign-state.ps1
  ```
- `main` is branch-protected: PRs required, no direct pushes. You are not
  merging anything; this is read-only.
- The frozen tree `specs/020-repository-knowledge-index/` must be byte-identical
  when you finish. If you open a file there, open it read-only.
- Lib unit-test paths carry an `internals::` prefix since the V11 cut mounted
  the server modules under `src/internals.rs`. Any pre-cut `cargo test --lib
  … -- --exact` filter you find in a document silently selects nothing — that
  is a real bug class, and worth reporting if you find a live instance.

---

## 9. What a good review looks like here

The best possible outcome of this review is **one of**:

- "R1 is false, here is the command output that shows it, the plan is
  mis-scoped" — highest value, saves a day of misdirected work;
- "R1 is true but incomplete: gate X also constrains the surface" — high value;
- "R1 is true, the plan is sound, and here are the two places the reasoning is
  thinner than it reads" — normal good outcome;
- "R1 is true and I could not break anything; here is what I checked" — a
  legitimate result, provided `## Negatives` is specific enough that I can tell
  you actually looked.

The worst outcome is a review that agrees pleasantly without having run a
single verification command. If you did not check something, do not list it
under Negatives.
