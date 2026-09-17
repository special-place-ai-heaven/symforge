# Plan: plan-aligned ledger for the Fathom embed alignment release

Scope: fathom-embed-plan-aligned (in-tree at docs/research/fathom-embed-ledger-plan-aligned/ by the owner's instruction, not under .unlazy/)
Depth: tree 1 (root node plus one leaf per plan task; every task integrates into one release)
Mode: orchestrated, sequential fallback

## Contract

- Governing plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Tasks 1 to 7,
  as written (owner decision, 2026-09-15). Evidence brief:
  docs/research/fathom-embed-change-brief.md. The earlier ledger at
  docs/research/fathom-embed-ledger/ is superseded and must not be used or edited.
- Branch: `feature/fathom-embed-contract`, built on v11.2.0 (b549aa7), worked by
  the plan's own session. Local commits only; the owner performs or authorises
  every push, pull request, merge, tag, release and CI cancellation.
- Interfaces: additive except the owner-accepted `OperationKind` extension below.
  No existing public item, field, signature, re-export or contract atom is
  removed or renamed. Additions are the plan's atoms (`IndexCensus`, `CensusFile`,
  `index_census`, `index_progress`, `EmbeddedSourceHandle::search_knowledge` with
  its knowledge request and result atoms) plus `IndexProgress`,
  `KnowledgeSearchRequest`, `KnowledgeSearchResult` and `KnowledgeMatch`. Fields
  and methods inside an approved new type are approved with it; each addition is
  recorded in the frozen contract JSON, the export delta file, the wrap table and
  the facade `contract` module in the same change, with the delta test green.
  `RetryAdvice`, `SourceRefusalKind`, `SourceRuntimePhase` and `SourceRuntimeView`
  stay frozen. `OperationKind` may gain exactly `IndexCensus` and `SearchKnowledge`
  (claims and refusals for those methods). The original seven variants, their
  order, and their `kind_name` strings stay. `ALL` becomes `[Self; 9]`. That is a
  major; the owner accepted it. Do not add `IndexProgress` as a kind: the method
  is infallible and mints no receipt. Reusing `SearchSymbols` or `SearchText` for
  the new methods is forbidden (it mislabels the receipt). Test-only hooks behind
  `__test-internals` or `cfg(test)` are not public additions. No new dependency.
- Tests: where the plan and its session put them. `tests/activation_cut_v11.rs`
  (server feature only) for CR-0; `tests/embed_bound_index.rs` (embed feature,
  run as its own target) for CR-1, CR-2, CR-3, the shared-factory cost and the
  embed path prefix and embedded knowledge retrieval; lib protocol and daemon
  tests for F-1, F-2, the selector and the seam-versus-server ranking check.
  Every test gate proves the named tests ran under its exact command: exact count
  plus each name reported `ok`.
- Ownership: each leaf's `OWNS:` header. Leaves overlap by design on one branch,
  so they run one at a time and no `--claim` is used.
- Host launch mode: sequential fallback. One cargo target directory, feature sets
  never built concurrently, overlapping files. No dispatch wave.
- Toolchain: rust-toolchain.toml channel 1.96.0; Node 18+ for the oracle;
  Windows, checker shell cmd.exe via ComSpec. Every CHECK starts with `node`.
  Every gate-check invocation runs from the repository root through Terminal
  Commander `run_and_watch`, with `--root . --cwd .` and `--timeout 10800`
  (node 1 uses 43200). Approvals bind timeout, shell and PATH.
- Conventions: failing-first test observed red before its fix; one mutation
  patch per guard under `mutations/`; three baselines under `baselines/` recorded
  at b549aa7; the release's squash subject names every identifier.
- Manual review: the owner reviews every RECORD gate. Evidence standard: exact
  commit or working state, command and output line.

## Current contract inventory

Contract revision: 3 (2026-09-16: owner named already-merged companions so the
release tag on `main` can stay honest. (5) #695 SnapshotStore restore
(`src/index_lifecycle/snapshot.rs`, `src/live_index/persist.rs`) and #701
lock-only version bumps of crates already at v11.2.0 — `reqwest@0.13.5`,
`rmcp@3.3.0`, `rmcp-macros@3.3.0`, `toml_edit@0.25.15+spec-1.1.0`,
`toml_parser@1.1.3+spec-1.1.0` — ride as named C3 companions. Cargo.toml
dependency and patch tables stay unchanged. N5 / leaf-1.2:G13 pass those five
`--allow-lock` rows and no others.

Contract revision: 2 (2026-09-15: owner chose only the stronger options.
(1) `OperationKind` gains `IndexCensus` and `SearchKnowledge`; major accepted.
(2) Close fixture is this checkout, real Loading, G16 as written.
(3) Type names stay; C28/C29 pin meanings and the knowledge result shape.
(4) G8 names Task 3 CI pins and the 200-character companion; nothing unlabeled.

Revision 1 (2026-09-15): in-flight plan as written; CR-2 and CR-3 in scope;
11:02 plan text including embedded knowledge retrieval and the one-second bound.)

| ID | Required outcome or constraint | Owner | Observing gate or manual review | Disposition | Revision |
|---|---|---|---|---|---|
| C1 | Branch built on v11.2.0 (b549aa7), cited worker-backed source at version 11.2.0 | leaf-1.1 | leaf-1.1:G1-G4 | ACTIVE | 1 |
| C2 | Suites measured green at the aligned source (plan Task 1 Step 3) | leaf-1.1 | leaf-1.1:G5-G7 | ACTIVE | 1 |
| C3 | Only named work rides along: plan Tasks 1-7, Task 3 CI pins, the 200-character harness companion, #695 SnapshotStore restore, and #701 lock-only version bumps of already-present crates | leaf-1.1 | leaf-1.1:G8 | ACTIVE | 3 |
| C4 | Second ProcessRuntimeApi on the same root gets SelectionUnavailable | leaf-1.2 | leaf-1.2:G2, G3 | ACTIVE | 1 |
| C5 | Panic after registration leaks nothing; root reopens | leaf-1.2 | leaf-1.2:G4-G6 | ACTIVE | 1 |
| C6 | Identity-checked guards: runtime shutdown and open rollback act only on their own identity | leaf-1.2 | leaf-1.2:G7-G10 | ACTIVE | 1 |
| C7 | Unrelated open completes within the close bound while another root closes (shared-factory cost) | leaf-1.2 | leaf-1.2:G11, G12 | ACTIVE | 1 |
| C8 | No new dependency | leaf-1.2, node-1 | leaf-1.2:G13, node-1:N5 | ACTIVE | 1 |
| C9 | Close returns promptly during first load, refresh and scout; cancelled reloads publish nothing | leaf-1.3 | leaf-1.3:G2-G9, G14, G15 | ACTIVE | 1 |
| C10 | Stopping never overwritten; no lost-notify poll delay | leaf-1.3 | leaf-1.3:G10-G13 | ACTIVE | 1 |
| C11 | One-second close bound enforced and measured on this checkout as the agreed large-repository fixture (real Loading, full reload ≥10 s, twenty timed closes), with an unrelated open inside the same bound | leaf-1.3 | leaf-1.3:G16, G17 | ACTIVE | 2 |
| C12 | Census: one generation, sorted zero-symbol parsed files, matching counts, typed readiness refusal, normal acquisition path | leaf-1.4 | leaf-1.4:G2-G11 | ACTIVE | 1 |
| C13 | Progress: exact documented meanings below, rises only during a real reload, reset before each, kept after cancel, idle scouts untouched | leaf-1.5 | leaf-1.5:G2-G11 | ACTIVE | 2 |
| C14 | Public surface additive except the accepted `OperationKind` extension; new atoms recorded with delta, wrap table, facade and CHANGELOG embed section | leaf-1.2 to leaf-1.6, node-1 | API, atoms, delta, facade and doc gates in each; node-1:N3, N4 | ACTIVE | 2 |
| C15 | F-1 exact excluded-knowledge count pointing to search_knowledge | leaf-1.6 | leaf-1.6:G2-G6 | ACTIVE | 1 |
| C16 | F-2 withheld counts and reasons in the neutral policy wording | leaf-1.6 | leaf-1.6:G7-G10 | ACTIVE | 1 |
| C17 | Ambiguous selector refusal with candidate ids stays (regression guard) | leaf-1.6 | leaf-1.6:G20, G21 | ACTIVE | 1 |
| C18 | Embedded path prefix src/ excludes srcx/ | leaf-1.6 | leaf-1.6:G22, G23 | ACTIVE | 1 |
| C19 | search_text stays code search; no second Markdown indexing; server search_knowledge semantics unchanged and shared with the embed seam | leaf-1.6 | leaf-1.6:G18, G19, G24, cells C4 | ACTIVE | 1 |
| C26 | Embedded search_knowledge returns admitted Markdown, text and config knowledge with provenance, relationship evidence, authority and coverage, and withholding evidence through a typed seam | leaf-1.6 | leaf-1.6:G11-G17 | ACTIVE | 1 |
| C20 | Every plan and brief citation re-read on the branch before editing | each code leaf | G1 of leaves 1.2 to 1.6 | ACTIVE | 1 |
| C21 | Each behavioural gate: red before the fix, green after, a caught mutation | each code leaf | red-receipt gates and mutant gates; node-1:N8 | ACTIVE | 1 |
| C22 | SymForge's binding verification cells, the embed lib cell and the embed integration cell pass | each code leaf, node-1 | gates/cells.md via each leaf; node-1:N7 | ACTIVE | 1 |
| C23 | CR-0 and CR-1 (with CR-2, CR-3, F-1, F-2) ship in one release tag whose notes name each identifier | leaf-1.7, node-1 | leaf-1.7:G3, node-1:N2 | ACTIVE | 1 |
| C24 | CI inventory before CI; tag only after all required gates pass; task ledger records tag, results and the embedder pin move | leaf-1.7 | leaf-1.7:G2, G4, G5 | ACTIVE | 1 |
| C25 | No push, pull request, merge, tag, release or CI cancellation without the owner | owner, driver | leaf-1.7:G1, G2; node-1:N8 | ACTIVE | 1 |
| C27 | `OperationKind` gains exactly `IndexCensus` and `SearchKnowledge`; original seven stay; `ALL` is `[Self; 9]`; major accepted | leaf-1.2 to leaf-1.6 | each leaf's api CHECK `--extend OperationKind=IndexCensus,SearchKnowledge` | ACTIVE | 2 |
| C28 | `IndexProgress` field meanings: `files_discovered` is this reload's executable scout work-set; `files_parsed` is post-fold `Parsed` outcomes that remain parsed; `symbols_found` is symbols extracted from those files. Current progress need not equal census totals | leaf-1.5 | leaf-1.5:G2-G11 | ACTIVE | 2 |
| C29 | Knowledge types keep facade-parallel names. `KnowledgeSearchResult` is not a thin `TextSearchResult` clone: it carries matches, truncation, withheld count and neutral withheld reasons. `KnowledgeMatch` carries path, heading path, preview, content hash, provenance, relationship evidence, and authority/coverage | leaf-1.6 | leaf-1.6:G11-G17 | ACTIVE | 2 |

An owner amendment (for example a further `OperationKind` variant, a different
fixture, or different progress or knowledge type names) bumps the revision and
re-approves every changed gate.

## State vocabulary

Leaf state is exactly one of WAITING, READY, IN-FLIGHT, VERIFIED, ABANDONED.
Branch state is exactly one of OPEN, VERIFIED, ABANDONED.

## Tree

- 1 Fathom embed alignment release .... GATES.md ................. State: OPEN
  - 1.1 Task 1 baseline ............... gates/leaf-1.1.md ........ Needs: - .................... State: IN-FLIGHT
  - 1.2 Task 2 CR-0 ownership ......... gates/leaf-1.2.md ........ Needs: 1.1 .................. State: WAITING
  - 1.3 Task 3 CR-1 cooperative close . gates/leaf-1.3.md ........ Needs: 1.2 .................. State: WAITING
  - 1.4 Task 4 CR-2 census ............ gates/leaf-1.4.md ........ Needs: 1.3 .................. State: WAITING
  - 1.5 Task 5 CR-3 progress .......... gates/leaf-1.5.md ........ Needs: 1.3 .................. State: WAITING
  - 1.6 Task 6 search scope ........... gates/leaf-1.6.md ........ Needs: 1.1 .................. State: WAITING
  - 1.7 Task 7 release handoff ........ gates/leaf-1.7.md ........ Needs: 1.2, 1.3, 1.4, 1.5, 1.6 .. State: WAITING

Helper ledger, not a tree node: gates/cells.md. READY for 1.1 means ready once
the owner has approved the ledgers. Leaf 1.2's G11 and G12 use leaf 1.3's reload
hold, which is already on the branch; order the work as the plan does.

## Status log

`gate-check --log` needs a `.unlazy/<scope>/` pipeline, which this in-tree ledger
is not. Record state changes by editing State and Needs above and in each task's
report; never rewrite an earlier report.
