# REVIEW-FINDINGS — publication-authority invariant (`activation.rs:19-24`)

Reviewer: LINUS
Tree: `main` @ `48d1d0ab` (src/ byte-identical to `fd4de8dc`; compare `fd4de8dc...48d1d0ab` has no `src/` files)
Brief: `docs/reviews/REVIEW-REQUEST-publication-authority-invariant-2026-08-22.md` @ `8f5a39e4`
Status: **on origin.** Parts 1–6. Part 6 after appendix `ac266d12`.

---

## Part 1 — the reading

### What "serves" means here

The sentence does not bind "serves" to a function, a type, or a test.

The same file uses the word in two incompatible ways:

- `ActivationMode::LegacyOpen` is documented as "Legacy authority serves" (`activation.rs:59`). That is a mode label. The only production `ActivationMode` check in `src/` is `activate_surface` (`activation.rs:1044`): if the process machine is still `LegacyOpen`, run the ceremony and flip to `PreventiveV1Open` before the surface attaches. After that, nothing in protocol, daemon, sidecar, or server reads the mode again. `gh search code ActivationMode --repo special-place-ai-heaven/symforge` returns `activation.rs` and tests/docs. Query ingress does not key off the machine.
- `ProjectRuntimeHandle::data_plane` (`activation.rs:984-986`) is what actually answers. It returns `&SharedIndex`. Call sites include `protocol/mod.rs`, `protocol/edit_tools.rs`, `sidecar/handlers.rs`, `server/mod.rs`, `server/admin/api_v1.rs`. The brief's 226-count is consistent with that door; I sampled the production files and did not re-count every hit.
- The same handle's comment says the handle is "the replacement, not a root" and that every `index: SharedIndex` field is a "V10 publication root" (`activation.rs:932-934`).
- `ObservationLane` says the opposite of a completed cut: "The LiveIndex data plane keeps serving admissions itself mid-cut … until C4/C5 make it the root" (`activation.rs:337-339`).
- The frozen retirement inventory's publication_roots assertion is "Only ProjectPublicationRoot is query-visible" (`specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md`, category `publication_roots`, `status: planned_not_executed`). That is the nearest technical meaning of "serves" the tree actually writes down: query-visible. It is still planned.

So: if "serves" means "the activation machine is in this mode", it is a tautology of `ActivationMode`. If "serves" means "answers a query", the tree names `data_plane() → SharedIndex`, not a V11 root. The sentence uses both vocabularies and defines neither.

### Which two roots the invariant names

The sentence names roles, not types: "legacy authority" and "the V11 publication root".

Types in the tree that could be those roles:

| Candidate | Why it might be a "publication root" | Why it might not |
|---|---|---|
| `SharedIndex` / `data_plane()` | Comment at `activation.rs:933` calls every such field a V10 publication root. This is what queries hit. | Same comment says `ProjectRuntimeHandle` is not a root. |
| `ProjectPublicationRoot` (`runtime.rs:186-197`) | Documented "SOLE publication root for a project's runtime state". `load()` is the only method. | No production callers outside `runtime.rs`. `publication_root()` is used inside that file (identity swap / tests). Not query-visible. |
| `ProjectArtifactRoot` (`candidate.rs`; field on `ObservationLane` at `activation.rs:347`) | Carries "root" in the name. | Artifact store, not a publication. |

I cannot determine the pair without guessing. The sentence's own pair is the two *roles*. The types those roles would have to be (`SharedIndex` vs `ProjectPublicationRoot`) are the pair a reader would guess. That guess is not established by construction: both types exist at once, and only `SharedIndex` is reached from ingress.

"Simultaneously authoritative" appears only in this doc comment. No test names it.

### Verdict

**cannot be evaluated as written.**

The predicate is unbound. "Enforced by construction across the cut" is a claim about types. The types do not make dual authority unrepresentable, and they do not make "serves" a compiler fact.

### What would change the verdict

- Bind "serves" to `ActivationMode` occupancy only → **holds** (one `u8`, monotonic, no reverse edge). That would be evaluating a different sentence than the one that says "publication roots".
- Bind "serves" to query-visible via `ProjectPublicationRoot` (the retirement inventory's words) → **does not hold**. See Part 2.
- A single query door, ActivationMode-gated, whose V11 side is `ProjectPublicationRoot` and whose legacy side is uninhabited after `open_preventive` → evaluable, and then a holds/does-not-hold question.

### Confidence

High on cannot-evaluate. Raised only by a definition I missed: a `serve` method, a type alias, or a test that names the two roots and the exclusive predicate. I grepped `ActivationMode` and `ProjectPublicationRoot` on the repo and read the comment cluster at `activation.rs:19-24`, `:337-339`, `:920-986`, and `runtime.rs:184-317`.

---

## Part 2 — if it does not hold

Conditional on the only operational reading the frozen inventory writes down: "serves" = query-visible.

Under that reading: **does not hold.**

**Since when (as far as the tree shows).** On `48d1d0ab` the companion comment, the V10 `data_plane()` accessors, and an unwired `ProjectPublicationRoot` coexist. The retirement inventory still says `publication_roots` is `planned_not_executed`. `activate_surface` (`activation.rs:1037-1064`) already records that "after this PR's compile-time flip there is no legacy traffic left to drain at runtime" and drives the machine to `PreventiveV1Open` before the first request. The process is in the mode where the V11 root is supposed to serve. The V11 root does not.

**Documentation defect, not a runtime dual-authority defect.** One thing answers queries: `SharedIndex` through `data_plane()`. `ProjectPublicationRoot` is not also answering. The wording overclaims exclusive V11-root service. It does not hide two live roots. The special-case smell is the comment: "enforced by construction" papers over a predicate the types do not encode.

**What a reader of the shipped release is entitled to assume that is not true.** That after the cut, only the V11 publication root is query-visible, and legacy authority is not. What they get: the machine is `PreventiveV1Open`; every sampled ingress reads `SharedIndex`; `ProjectPublicationRoot` is dark.

---

## Part 3 — open questions

1. **Is "enforced by construction" true here, and by which construction?**
   True of *mode*: `ActivationMode` is a closed enum behind one process `ActivationCut` (`activation.rs:57-65`, `:177-189`). Transitions require the prior mode (`:202+`, `:252+`). No reverse edge. Unrepresentable-over-checked works for the machine.
   False of *publication exclusivity*. `SharedIndex` and `ProjectPublicationRoot` are both inhabitable. Nothing in the type of a query path mentions `ActivationMode`. Constitution Principle V is not doing the work the comment claims.

2. **Does `ProjectPublicationRoot` having no production callers mean anything, or is it legitimately ahead of its wiring?**
   Both, and they are not in conflict. It is ahead of wiring: inventory status `planned_not_executed`, Slice 4 tasks T066/T067 still listed. It also means the named V11 root does not serve. "Ahead of wiring" is not a license for a comment that says the V11 root already serves in `PreventiveV1Open`.

3. **Does any ingress key off `ActivationMode` rather than data-plane readiness?**
   No production ingress I found. Mode is read in `activate_surface` to run the ceremony, then forgotten. Readiness that *is* checked is admission liveness on `acquire()` (`activation.rs:972-980`): tombstoned slot → `RegistryRefusal`. `data_plane()` (`:984-986`) does not even do that. Queries use `data_plane()`.

4. **Is the invariant falsifiable as written, or is it prose that reads like a guarantee?**
   Prose. You cannot write a failing case until "serves" and "the two publication roots" are bound. Absence of a test named "simultaneously authoritative" is not itself a defect (the brief is right that construction can replace tests). Here construction does not encode the claim, so the missing test is the missing definition.

5. **What did this brief fail to ask about?**
   The dual door: `acquire()` is live-gated, `data_plane()` is not, and ingress uses the ungated one. Whether "legacy authority" is a type (`BindingAuthority` / `SourceRuntime`) or only a mode name. Whether C4/C5 "make it the root" (`activation.rs:339`) is still the live plan after shipping `PreventiveV1Open` as 11.0.0. I did not treat FR-008's two-ArcSwap story as in scope; the brief did not name it.

---

## Part 4 — Outside the questions asked

`ObservationLane` (`activation.rs:337-339`) already admits the mid-cut shape: LiveIndex keeps serving; the authority plane runs beside it until C4/C5. The process machine is nonetheless already `PreventiveV1Open` at first request (`activation.rs:1034-1064`). The companion invariant and the ObservationLane comment cannot both be describing a finished cut.

`ProjectPublicationRoot` itself is a coherent ArcSwap of `ProjectRuntimePublication` with a never-reused `PublicationIdentity` (`runtime.rs:186-217`, used at `:537-558`). That is a publication root. It is not the one queries hit.

I accepted the brief's `data_plane()` file split without a full recount. I verified the return type, the "not a root" comment, and production call sites in protocol/server/sidecar. I verified `src/` did not move: `gh api .../compare/fd4de8dc...48d1d0ab` lists `.github/workflows/ci.yml` and `tests/preventive_runtime_dark_v11.rs` only.

PR #609 not read. `specs/020` read only for the retirement inventory assertion above. No cargo. Appendix opened only for Part 6.

---

## Part 5 — Negatives

- The mode machine is sound as a mode machine: three states, monotonic, process-wide, non-configurable, ceremony serialized on `PROCESS_BOOTSTRAP` (`activation.rs:1010-1070`). Out-of-order transitions refuse. That part I would ship.
- `ProjectRuntimeHandle` is an enumerable census door for V10 fields, as its comment says (`activation.rs:920-923`). Counting `data_plane()` sites is a real construction. It is a construction of *reroute work*, not of exclusive authority.
- `ProjectPublicationRoot::load` / identity rebase inside `runtime.rs` is internally consistent. Dark, not wrong.
- I did not find a second process `ActivationCut`, a reverse edge, or a config/env read that selects a mode.
- I did not find `ProjectPublicationRoot` answering a protocol/daemon/sidecar query. The failure mode is "V11 root does not serve", not "two roots serve".


---

## Part 6 — Delta

Appendix: `docs/reviews/APPENDIX-publication-authority-suspicions-2026-08-22.md` @ `ac266d12`.
Read after Parts 1–5. Nothing in Parts 1–5 was rewritten. The Part 1 verdict survives.

### Already reached

| Suspicion | Mine, independently |
|---|---|
| S1 tension (machine in `PreventiveV1Open`; LiveIndex still serving; `:337-339` vs `:19-24`) | Yes. Part 1 and Part 4. I did not need Holmes. |
| S1.3 C4/C5 ran and did something else | The "until C4/C5" hang I already named (Part 3 Q5, Part 4). The appendix's dates and "THE EXPOSURE FLIP" I did not have. I checked: execution map pins C4a–C4c and C5 as executed 2026-08-19; C5 is the public-census / `pub` flip (`activation-cut-execution-map.md:199-294`, `src/main.rs` C5 comment). Confirmed: those owners ran. They did not make the lane the publication root. |
| S3 unearned guarantee (mode without serving) | Yes, as the *conditional* in Part 2, under query-visible. I did not adopt it as the Part 1 verdict because the sentence is unbound. |
| S4 count is not a verdict; dark wiring is a real discipline | Yes. Part 3 Q2. 226 vs 0 is a fact. It does not decide "serves". |
| S6 binary is the wrong shape; type-level half vs runtime half | **This was the Part 1 verdict.** "cannot be evaluated as written" is S6 said first, without the appendix. Mode construction holds. Publication exclusivity is not a construction. I refused to pick holds/does-not-hold to satisfy the brief. |

### Now think are wrong

**S2 is not a resolution.** I did not reach "serves = write/publication authority" and I do not adopt it now. It is another unbound guess. The same file that would have to host that meaning calls every `SharedIndex` field a "V10 publication root" (`activation.rs:933`) and hands that type to ingress via `data_plane()`. `ProjectPublicationRoot` still has no production callers. Writers retirement is also `planned_not_executed`. If "serves" meant publish, the production publisher is still `SharedIndex`, not the type documented as SOLE. S2's "V10 is a cache/serving layer, nothing is violated" does not survive contact with `:933` or with `swap_and_publish` living on that store. Wording-is-misleading is true either way. "Invariant is sound" is not established.

**S3 as "the unlikely one".** Under any bound operational reading (query-visible, or publish-visible), S3 is the one that matches the artifact: the machine is in the post-cut mode; the named V11 root does not serve. The appendix author's prior (S2 likely, S3 rule-out) is the thing I reject. I still will not promote S3 to the Part 1 verdict, because that would pretend the sentence defined "serves".

### Changed a verdict

None. Part 1 remains **cannot be evaluated as written.** Part 2 remains the conditional: bind query-visible → does not hold, documentation overclaim, not two roots answering.

S6 is the appendix agreeing with Part 1 after the fact. That is not a change.

### S5 — fourth

The class is real. I already had two members in this file: `:337-339` ("until C4/C5 make it the root") and `:19-24` itself (predicts exclusive V11 service; the mode shipped; the service did not).

A sibling on the same tree, not named in Parts 1–5: `activate_surface` asserts "after this PR's compile-time flip there is no legacy traffic left to drain at runtime" (`activation.rs:1037-1039`) and then flips the machine. Ingress still drains `SharedIndex`. Same class: prose that predicted a flip, now written as an observation.

The retirement inventory's "Only ProjectPublicationRoot is query-visible" is *not* a fourth. It still says `planned_not_executed`. That one aged honestly.

### What I did not take from the appendix

I did not take S2 as the likely answer. I did not reclassify this as a release defect or as sound-but-misleading. I did not open #609. I did not run cargo.
