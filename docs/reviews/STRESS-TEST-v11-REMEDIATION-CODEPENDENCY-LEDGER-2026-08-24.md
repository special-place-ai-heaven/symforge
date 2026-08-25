# SymForge v11 stress-test remediation codependency ledger

**Date:** 2026-08-24  
**Baseline:** `main` at `05f7e60e`, clean and equal to `origin/main`  
**Installed product used for live rechecks:** official SymForge 11.0.8  
**Inputs:** Grok MCP-surface stress report, blind Codex stress evaluation, and the
2026-08-24 branch/release cleanup ledger

## Executive decision

Do not implement this as one patch and do not preserve either report's original
priority labels verbatim. The reports tested 11.0.5/11.0.6, while `main` now contains
three directly relevant lifecycle fixes (#643, #648, #652). The remediation campaign
must begin with a packaged-11.0.8 readiness reproduction gate, then land independent
trust slices in dependency order.

The optimization order is **maximum end-to-end token reduction subject to hard
correctness and LLM-trust invariants**. A token win that creates a false zero, wrong-
project answer, hidden mutation, or retry is a failed optimization.

Prefer the superior end state over preserving accidental behavior, but change a
frozen contract through its declared authority path. Existing plans, tool shapes, and
compatibility paths are evidence to evaluate; frozen Feature 020 artifacts remain
normative until an approved successor explicitly replaces named clauses. Reuse sound
internals, remove redundant policy branches, and keep compatibility only where it has
measured user value and does not weaken the trust contract.

The first production work should be:

1. make selected-project authority uniform for every repo-scoped tool;
2. make every unsuccessful mutation unambiguously unsuccessful at the MCP host seam;
3. stop `index_folder` from editing tracked `.gitignore` files;
4. fix deterministic test-path and path-punctuation classification;
5. derive and approve a successor exact-text contract, then make search cover every
   admitted non-binary text lane by default;
6. make high-value read responses code-first and progressively disclose knowledge
   provenance;
7. treat compact-default as a product architecture decision: derive and approve the
   successor surface contract, prove equivalent-task parity, and only then change the
   default and full-surface escape hatch atomically across code, tests, and guidance.

Readiness is a release-gate candidate, not a presumed live P0. Curation durability is
an honestly unavailable capability, not permission to invent a new persistence design.

### Frozen-contract handling — derive; do not edit

`specs/020-repository-knowledge-index/` is frozen authority. This ledger does **not**
edit it, silently amend it, or declare it superseded. Its current code/knowledge search
separation remains the shipped contract until the required authority process approves
a successor.

The chosen path is to **derive a new successor specification**, provisionally the next
available Spec Kit feature, for truthful exact-text retrieval. That successor must:

1. cite every Feature 020 clause it proposes to replace;
2. carry an amendment record outside the frozen Feature 020 tree;
3. produce the new manifest, attestation, exact-target review, and trusted external
   approval required by `specs/020-repository-knowledge-index/tasks.md:862-865`;
4. retain the sound Feature 020 invariants: one resident index, `IndexTargets`, secret
   filtering, deterministic publication, and no duplicate document store; and
5. authorize implementation only after approval, never retroactively by merging code.

Compact-default is a separate architecture/surface successor decision, not an issue or
ordinary P1 ticket. It may reuse the same evidence campaign, but it needs its own
accepted contract because `CLAUDE.md` currently pins full-39 as default and compact-3
as `SYMFORGE_SURFACE=compact` opt-in.

## Priority vocabulary

| Priority | Meaning in this ledger |
|---|---|
| **P0** | Wrong project, false success, false absence, or an unavailable product path with no recovery. Blocks the next release if confirmed. |
| **P1** | Common-path correctness, unexpected repository mutation, or default token-negative behavior. Land in the same campaign. |
| **P2** | Bounded quality, parser, ranking, or ergonomics defect. Land after trust invariants. |
| **P3** | Test/CI/release-process debt that does not change normal product answers. |

Status is independent of priority:

- **LIVE** — reproduced on installed 11.0.8 or proved by current source/schema.
- **CONDITIONAL** — historically reproduced, but intervening fixes plausibly close it;
  rerun the exact artifact sequence before opening an implementation ticket.
- **ADJUDICATE** — evidence is real but the intended contract must be chosen first.
- **CONTRACT PROPOSAL** — product/spec-tree decision; do not create an implementation
  ticket until its successor contract is approved.
- **CLOSED/PROTECTED** — fixed, superseded, or intentionally fail-closed; retain tests.

## Current urgency board

| ID | Priority | Status | Finding | Current evidence | Dependency |
|---|---:|---|---|---|---|
| R0 | P0 if red | CONDITIONAL | Compact launch can remain `Loading` after switch/restart | #643/#648/#652 now cover the watcher abort gap, joined-live retirement, and same-root reindex; live 11.0.8 is currently Ready | None; execute before ticketing readiness code |
| A0 | P0 | LIVE | Repo-scoped tools can observe or mutate the wrong project | Current schemas still omit `project` for `conventions`, `inspect_match`, `checkpoint_now`, and `detect_impact` | Root dependency for A1-A6 |
| A1 | P0 | LIVE | A single foreign `project=` can enter the cross-project path, making `find_references(path=...)` refuse valid scope | `execute_cross_project_read` explicitly refuses `find_references` path scope even when `Targets::One` is selected | A0 |
| B0 | P0 | LIVE | Failed structural mutations can be host-visible success | `NotFound` and `Ambiguous` are not errors; live contract relies on `_meta`, which some wrappers drop | None |
| C0 | P0 impact | CONTRACT PROPOSAL | `search_text` silently returns zero for admitted Markdown/RST, but Feature 020 currently requires separate code/knowledge lanes | Reproduced on 11.0.8 with `glob="**/*.md"`; `search_knowledge` finds the same fact | Derive and approve the retrieval successor; then A0 for foreign-project acceptance |
| B1 | P1 | LIVE | Normal indexing edits tracked root `.gitignore` | Both daemon and local `index_folder` pass `ExplicitNormalBinding`, which authorizes the append | None |
| C1 | P1 | LIVE | `include_tests=false` leaks `tests.rs` and `testutil.rs` | Reproduced on 11.0.8; classifier omits exact `test`/`tests` stems | None |
| C2 | P1 | LIVE | `ask` retains terminal punctuation in a path | Reproduced on 11.0.8: `src/lib.rs?` is sent to `search_symbols` | None |
| D0 | P1 | LIVE | Default `get_file_context` can spend its budget on knowledge and omit the outline | Reproduced on 11.0.8; overflow fallback intentionally returns knowledge-only provenance | None |
| D1 | P1 | LIVE | Knowledge discovery is provenance-heavy and ranks changelog history above current guidance | Reproduced on 11.0.8: four `CHANGELOG.md` hits precede `docs/ideation.md` | C0 should settle the search contract first |
| E0 | P1 | LIVE | `batch_rename` confidence is not reference-authoritative | Reports show both confident `exec.Command` false positives and an exact same-file call left uncertain | A0, B0 |
| E1 | P1 | LIVE | Macro-decorated C++ declarations are lost/partial | 124/180 spdlog files partial; ordinary corpora were 953/959 fully parsed | Independent after trust work |
| M0 | P1 | ADJUDICATE | Token savings instrumentation is not equivalent-workflow net accounting | Only paired end-to-end result is 24.8%; health counters omit schema/retry/CCR/host effects | D0/D1 response contracts first |
| S0 | — | CONTRACT PROPOSAL | Full-39 is the CLAUDE.md-pinned default despite a roughly 21.4k-token schema versus roughly 1.2k for compact-3 | Historical live measurement: 85,608 versus 4,768 JSON characters | Product/spec-tree decision, then R0/A0/B0/C0/D0 and parity proof; not an implementation ticket |
| D2 | P2 | LIVE | `analyze_file_impact` can bury summary counts below truncated detail | Reproduced in Grok report; formatter ordering issue | None |
| D3 | P2 | LIVE | `review_knowledge(summary)` returns counts with zero dossiers and remediation can become a hash wall | Current description and output contract disagree | D1 |
| E2 | P2 | LIVE | Cross-project caps can omit an entire selected project | Global cap is applied after merged ranking with no fairness guarantee | A0 |
| E3 | P2 | LIVE | TypeScript barrels/dotted APIs, Go aliases, Java constructors, YAML keys, and C++ recovery have parser/symbol gaps | Cross-language corpus evidence | Separate language-specific slices |
| E4 | P2 | LIVE | `get_symbol` large-item default and `what_changed` filtered-path wording invite misreads | Honest footers exist, but the important limitation is not leading | D0 presentation policy |
| E5 | P2 | LIVE | Dry-run increments worktree-misuse and watcher repair metrics lack root attribution | Observability semantics, no data corruption shown | None |
| K0 | P2 | PROTECTED | `curate_knowledge` apply is unavailable without atomic durability | Health and tool correctly refuse unsafe apply | Existing Feature 020 durability owner only |
| O0 | P3 | LIVE FLAKE | `/health <50 ms` wall-clock assertion flakes on loaded runners | `tests/sidecar_integration.rs:243` asserts a shared-runner wall clock | None |
| O1 | P3 | LIVE/WINDOWS | Proxy evidence test fails on this Windows host, passes Linux CI | `daemon_proxy_success_without_receipt_does_not_reuse_home_evidence` | Diagnose independently; do not weaken invariant |
| O2 | P3 | LIVE | Release subject validation can reach two legacy non-conventional commits | Release workflow ranges from last successful Release, which can drift far back | None |
| O3 | P3 | CLOSED | 11.0.6 publish gap and branch/PR clutter | Main-only, zero PRs; 11.0.7/11.0.8 shipped; backup bundle verified | Retain cleanup receipts |

## Hierarchical codependency graph

```text
GATE R0 — packaged 11.0.8 lifecycle reproduction
  ├── green → close historical readiness ticket; keep regression sequence
  └── red   → P0 readiness repair (before every other release)

TRUST ROOTS
  ├── A0 selected-project authority
  │   ├── A1 single-project dispatch (project=ONE is not cross-project)
  │   ├── A2 conventions + inspect_match schemas/handlers
  │   ├── A3 checkpoint target and ambiguous-omission refusal
  │   ├── A4 detect_impact + diff_symbols selected Git root
  │   └── A5 find_references path scope and honest scope banner
  │
  ├── B0 mutation outcome authority
  │   ├── structural edit false-success repair
  │   ├── batch per-operation status
  │   └── packaged-host JSON-RPC/wrapper smoke
  │
  └── B1 source-mutation authority
      ├── index/open = ObserveOnly
      └── explicit init = ProjectAwareInit (the only default hygiene writer)

DETERMINISTIC INPUTS
  ├── C1 test-path classification
  ├── C2 punctuation normalization
  └── C0 proposed unified exact-text retrieval (derived-spec gate)
      ├── selected-project authority (A0)
      ├── default all admitted non-binary text
      ├── explicit code/knowledge/all narrowing
      └── deterministic code/knowledge quotas and lane labels

TOKEN ECONOMY / RESPONSE COMPOSITION
  ├── D0 code-first file context
  │   └── compact knowledge count + retrieval handle
  ├── D1 compact knowledge rows + authority-aware ranking
  │   └── D3 honest summary/remediation modes
  ├── D2 summary-first impact output
  └── M0 equivalent-task net-token benchmark after formats stabilize

SURFACE ECONOMY — CONTRACT DECISION, NOT A TICKET
  └── S0 derive compact-default successor contract
      ├── packaged readiness (R0)
      ├── selector/mutation/search/context trust (A0/B0/C0/D0)
      ├── equivalent-task correctness + token parity
      └── atomic CLAUDE/runtime/init/test/doc transition if approved

DEEP QUALITY
  ├── E0 reference-authoritative rename (depends A0 + B0)
  ├── E2 fair cross-project caps (depends A0)
  └── E1/E3 independent per-language parser slices

OPERATIONS (parallel, never allowed to weaken product invariants)
  ├── O0 functional health timeout + separate benchmark
  ├── O1 Windows-only proxy-evidence diagnosis
  └── O2 push-local conventional-commit range
```

## Verified root causes and code seams

### A. Selected-project authority

`src/daemon.rs:1913` already contains the correct reusable choke point:
`runtime_for_target(session_id, project)`. It resolves an explicit selector only
among the session's open projects and returns the selected runtime/root/index.

The missing schemas are direct current-source facts:

- `src/protocol/tools.rs:584` — `CheckpointNowInput` has only
  `verify_after_write` and `export_artifact`.
- `src/protocol/tools.rs:698` — `DetectImpactInput` has Git/options fields but no
  selector.
- `src/protocol/read_tools.rs:655` — `InspectMatchInput` has path/line/display fields
  but no selector.
- `src/protocol/tools.rs:10818` — `conventions(&self)` proxies without parameters and
  reads `self.index`.

`src/daemon.rs:5372` is the coupled defect: `execute_cross_project_read` is reached
when the target is not the active project, even for `Targets::One`, then rejects
`find_references` path scope as a cross-project-only limitation. The fix is dispatch,
not another special case in the formatter.

**Implementation rule:** `project=<one>` always resolves through
`runtime_for_target` and executes the normal single-project handler. Only
`projects=[...]` or `projects=["*"]` enters merged cross-project execution.

### B. Mutation truth and repository purity

`src/protocol/result_status.rs:40` treats only `InvalidRequest` and
`InternalFailure` as MCP errors. `src/protocol/edit_tools.rs:259` therefore emits
`isError:false` for `NotFound` and `Ambiguous`, even though no requested mutation
occurred.

Do not globally redefine read-side `NotFound`: an empty discovery result may remain
a successful query. Give mutation responses a stricter terminal predicate:

```text
applied/dry-run-complete       -> isError=false
not-found/ambiguous/invalid/
internal/rollback-incomplete  -> isError=true
```

Keep the typed `_meta` payload and lead the human body with the same bounded status.
That makes the contract survive hosts which retain only `isError` + content.

For `.gitignore`, `src/daemon.rs:1666` and `src/protocol/tools.rs:8076` both pass
`ExplicitNormalBinding`; `src/gitignore_hygiene.rs:81` then writes the tracked file.
The authority model already contains the required non-writing variant:
`ObserveOnly`. Use it for normal indexing/opening. Preserve `ProjectAwareInit` in
`src/cli/init.rs:390` as the explicit setup workflow allowed to append the rule.

### C. Classification and routing

`src/domain/index.rs:394` recognizes test-directory segments and affixes such as
`test_`, `_test`, `.test`, `_spec`, and `.spec`, but not an exact `test`/`tests` stem
or the documented `testutil` family. Fix this once in `FileClassification`; consumers
such as `file_path_is_test` already delegate to it.

`src/protocol/smart_query.rs:80` strips `?` for some conceptual routes but passes
path-like trailing tokens through `clean_symbol_and_optional_path` before a shared
terminal-punctuation normalization. Normalize the extracted path token, not the full
query, so punctuation inside legitimate paths is not destroyed.

For documentation search, frozen Feature 020 says code queries stay code-scoped and
prose must not leak into code results. The stress evidence makes a strong case for a
successor contract: the current exact-text experience can look like false absence and
cost an extra discovery turn. Current `TextSearchOptions::for_current_code_search`
(`src/live_index/search.rs:527`) uses `SearchScope::Code`; Markdown is correctly
`IndexTargets::Knowledge`. This is a proposed public routing/default change, not an
admission defect and not authorization to edit Feature 020.

Proposed successor contract, effective only after the frozen-contract authority gate:

```text
scope omitted / all                           -> every admitted non-binary text lane
scope=code                                    -> code only
scope=knowledge                               -> admitted knowledge text only
scope=all                                     -> same as omitted

bounded mixed result composition
  ├── deterministic code quota
  ├── deterministic knowledge quota
  ├── unused quota spills to the other lane
  └── every hit carries lane=code|knowledge
```

Do not guess intent to exclude a lane: heuristic exclusion recreates false zeros.
Control volume with ranking, quotas, path/glob filters, compact rows, and a strict
total budget. Code may receive the larger default quota for coding ergonomics, but a
matching knowledge lane cannot disappear silently. A docs-only glob naturally spends
its whole budget on knowledge because the unused code quota spills over.

`search_knowledge` remains distinct because it answers a different question:
authority-aware intent/history/decision retrieval. `search_text` is exact lexical
evidence. This division is understandable to an LLM and does not require it to know
which ingestion lane contains a string before searching.

### D. Token economy and knowledge presentation

The current `get_file_context` behavior is intentionally inverted relative to the
product's coding-first goal. `src/protocol/tools.rs:4711` assembles code followed by
knowledge, but `src/protocol/knowledge_model.rs:201` falls back to knowledge-only when
the combined response cannot retain the complete provenance block. The 11.0.8 live
recheck returned five knowledge backlinks and no outline for `src/protocol/tools.rs`.

Reverse the fallback priority:

1. reserve a bounded code outline budget first;
2. append one knowledge trust/count line;
3. append at most one or two backlinks only if budget remains;
4. expose a CCR/review handle for full provenance;
5. make a cache hit include the minimal outline plus handle, rather than requiring an
   extra retrieval merely to recover the primary result.

For `search_knowledge`, keep full provenance in `review_knowledge` and behind an
explicit full mode. Default rows should be path/line, heading, one-line excerpt, and a
compact authority label. Rank `historical_record` below current/intent evidence unless
the caller requests history. Do not lower degraded/uncertain coverage labels.

## Implementation slices

Each slice is intended to be one PR with a red test first. File lists are expected
touch points, not permission to widen scope.

### PR 0 — packaged readiness adjudication (no production code)

**Depends on:** nothing.  
**Goal:** reproduce the exact historical topology on official 11.0.8:

```text
healthy snapshot -> full-surface project A -> project B -> project A
-> clean shutdown -> immediate compact/no-daemon launch
-> poll status -> invoke every compact facade intent
```

Assert a terminal `Ready` or typed refusal within a bounded functional deadline;
capture watcher freshness and snapshot verification. Run against packaged native and
npm artifacts, not only a source test binary.

- Green in repeated cycles: mark R0 closed by #643/#648/#652 and retain the sequence
  as a release golden.
- Red: stop the campaign release path and open one P0 using the captured terminal
  state. Do not revive the older watcher-race hypothesis without new evidence.

### PR 1 — one selected-project dispatch contract

**Depends on:** PR 0 only if R0 is red; otherwise none.  
**Likely modules:** `protocol/{read_tools,tools,conventions}`, `daemon`, schema and
multi-project integration tests.

Changes:

1. add a shared optional `project` field/type to the four missing input schemas;
2. make `conventions` parameterized;
3. route `project=one` through `runtime_for_target` and the selected server/root;
4. reserve merged working-set execution for `projects` only;
5. make selector omission explicit: active project for reads, but
   `checkpoint_now` refuses omission when more than one project is open;
6. include selected project evidence in success and refusal envelopes;
7. verify `diff_symbols` and every Git subprocess uses the selected canonical root.

Critical tests:

- `conventions_honors_project_selector`
- `inspect_match_honors_project_selector`
- `checkpoint_now_writes_selected_project_snapshot`
- `checkpoint_now_refuses_ambiguous_multi_project`
- `detect_impact_git_root_is_selected_project`
- `diff_symbols_git_root_is_selected_project`
- `find_references_single_project_allows_path_scope`
- unknown, unopened, ambiguous-name, path-as-selector, and retired-slot refusals

### PR 2 — mutation terminal semantics

**Depends on:** none; merge before rename work.  
**Likely modules:** `protocol/{result_status,edit_tools,tools}`, conformance, stdio
JSON-RPC and npm/host integration tests.

Changes:

1. introduce a mutation-specific `is_terminal_failure` decision;
2. set MCP `isError:true` for every no-apply/failed mutation terminal status;
3. retain namespaced `_meta`, per-operation status, and bounded leading text;
4. assert rollback-complete versus rollback-incomplete separately;
5. test the real serialized response and at least one consuming host bridge.

Do not make read-side empty/not-found queries into protocol errors.

### PR 3 — indexing is observational; init owns hygiene writes

> **ADJUDICATE (2026-08-25): blocked on a frozen contract, not on code.**
> `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`
> requires the shared `.gitignore` mutation to run after successful explicit
> normal `index_folder` binding (steps 1-6 under "The shared `.gitignore`
> mutation") and pins it as invariant 14 ("Explicit normal `index_folder` and
> project-aware init share a byte-for-byte ... matrix"). Two tests on `main`
> pin the same behavior: `explicit_normal_index_folder_reconciles_existing_root_gitignore`
> and `daemon_index_folder_reconciles_existing_root_gitignore`, both asserting
> `changed=true`. Making indexing observe-only therefore needs a derived
> amendment/successor spec and approval first, exactly as C0 does. `ObserveOnly`
> already exists and is already used by `health`, so the code change is small
> once the contract decision is made; do not land it before then.


**Depends on:** none.  
**Likely modules:** `gitignore_hygiene`, `daemon`, `protocol/tools`, `cli/init`, tests.

Changes:

1. use `ObserveOnly` for daemon open and local/daemon `index_folder`;
2. keep `.symforge/` hard-excluded independently of `.gitignore`;
3. preserve `ProjectAwareInit` as the only default tracked-file writer;
4. return `missing_rule` plus the explicit `symforge init` remediation;
5. regression-test an existing `.gitignore`, a missing `.gitignore`, CRLF, symlink,
   protected, and concurrent-change cases without index-time writes.

### PR 4 — deterministic edge normalization

**Depends on:** none.  
**Likely modules:** `domain/index`, `protocol/smart_query`, focused unit/integration
tests.

Changes:

1. classify exact `test`/`tests` stems and an explicit, documented `testutil` rule;
2. add positive controls so `contest.rs` and `latest.rs` remain source;
3. strip terminal sentence punctuation from extracted path scope;
4. preserve legal punctuation inside a path and quoted query;
5. add live tool-route regressions, not only helper tests.

### Derived-spec gate, then PR 5 — truthful textual search scope

**Depends on:** approved retrieval successor specification, then PR 1 for
foreign-project acceptance.  
**Likely modules:** `domain/index`, `live_index/search`,
`protocol/{search_tools,tools,format}`, schema/conformance and corpus tests.

After approval, implement the `all|code|knowledge` contract above using existing
`IndexTargets`; do not create a second text index or duplicate document bodies.
Omitted scope is `all`. Add deterministic per-lane quotas with spillover and lane
attribution. Add Markdown, RST, plain-text, config dual-target, secret-withheld, glob,
path-prefix, project, and cross-project tests. Search remains frecency-neutral. Before
approval, the only permitted work here is evidence, contract drafting, and a
non-shipping experiment behind an explicit test-only boundary.

### PR 6 — code-first context and summary-first impact

**Depends on:** none; merge before knowledge presentation changes.  
**Likely modules:** `protocol/{tools,format,knowledge_model,session}` and focused
tests.

- Reserve outline tokens before optional knowledge.
- Replace knowledge-only overflow fallback with a compact trust/count line + handle.
- Make cache-hit payload include enough outline to be independently useful.
- Put added/removed/changed counts before `analyze_file_impact` detail; CCR the rest.

### PR 7 — compact, authority-aware knowledge discovery

**Depends on:** PR 5 and PR 6.  
**Likely modules:** `protocol/{knowledge_search,knowledge_model,knowledge_review}`,
authority/ranking tests and benchmark corpus.

- Add compact default rows; retain full provenance on demand.
- Demote `historical_record` in default/current retrieval.
- Make summary mode either return bounded dossiers or explicitly declare counts-only.
- Bound remediation index before CCR.
- Keep coverage/provenance retrievable and secret filtering before formatting/CCR.

### PR 8 — reference-authoritative rename

**Depends on:** PR 1 and PR 2.  
**Likely modules:** `live_index/{query,graph}`, `protocol/edit_tools`, language fixtures.

Use exact parsed definition/reference identity as confident evidence. Qualified
symbols from another owner (`exec.Command`) must not inherit confidence from a
same-spelling target. Comments, strings, ambiguous imports, macro expansions, and
unresolved edges remain uncertain. Before apply, prove no previously exact target
edge is silently left unresolved. Paginate dry-run output.

### PR 9+ — parser and secondary ergonomics, one language/concern per PR

After the trust campaign, split C++ decorated declarations/span quarantine,
TypeScript re-export/dotted resolution, Go type aliases, Java constructor tagging,
and YAML/config symbol filtering. Do not put five parsers in one PR. Each slice needs
a small real-corpus fixture, an expected missing-symbol diagnostic, and no regression
in ordinary source parsing.

Separately land cross-project result fairness, `get_symbol` leading outline for large
items, `what_changed` leading filtered count, dry-run misuse accounting, and watcher
root attribution.

### Surface-contract derivation — not an implementation PR

**Decision dependency:** accepted successor surface contract, then R0 and PRs 1, 2,
5, 6; benchmark after PR 7.  
**Atomic change set if approved:** `CLAUDE.md`, `list_tools_for_profile`, the canonical
tool-name allow-list, `src/cli/init.rs`, every client-init/registration document,
schema snapshots, `test_client_allow_lists_match_registered_tool_surface`, the
FR-311c public-cache warning/contract, and packaged npm/native acceptance harnesses.

The largest guaranteed token saving is avoiding the full tool schema on every
connection. The historical measurement was approximately 21.4k schema tokens for
full-39 versus 1.2k for compact-3. Use the existing `symforge`, `symforge_edit`, and
`status` facade rather than inventing another surface.

Migration gates:

1. every common read/search/context/change intent has a deterministic facade route;
2. project selection and result status survive the facade;
3. compact readiness/recovery passes PR 0's packaged sequence;
4. a blind equivalent-task corpus has equal correctness, fewer retries, and lower
   total tokens including schema/request/response/CCR;
5. unsupported expert operations return one exact full-surface retry instruction;
6. the successor contract explicitly decides the full-surface escape hatch and
   compatibility window;
7. the FR-311c one-surface-per-origin cache assumption is revalidated for the new
   default and documented for mixed deployments;
8. only after all seven pass does one synchronized change update `CLAUDE.md`, runtime
   selection, allow-lists, init output, tests, and every client-init document.

Do not switch the default merely because schema bytes are lower. A facade that saves
schema tokens but causes route retries or hides an unavailable operation loses at the
task level. Do not open a "flip compact default" ticket before the product/spec-tree
decision; otherwise the ticket would falsely imply that the architecture is settled.

Changing the default advertised surface is a public contract transition. Ship it in
an explicitly announced version boundary with migration notes and generated client
inventory; do not hide it in an 11.0.x patch. Internally, compact and full must share
one execution engine—two schema exposures must never become two implementations.

### Operational PRs — independent lane

- **O0:** replace `/health <50 ms` as a functional assertion with a generous timeout;
  retain latency as a repeated benchmark/distribution check.
- **O1:** diagnose the Windows proxy-evidence failure with platform/path/body capture;
  do not weaken the rule that foreign responses cannot reuse home evidence.
- **O2:** validate subjects only in the current push range (`event.before..SHA`) in the
  Release workflow, matching CI. Do not rewrite history or whitelist legacy subjects.

## Test coverage map

```text
CODE PATHS                                      USER/AGENT FLOWS
[R0] lifecycle artifact sequence               [R0] switch -> restart -> compact query
  ├── ready [existing unit controls]              ├── [GAP -> artifact E2E] packaged native
  ├── watcher gap [covered #643]                  └── [GAP -> artifact E2E] packaged npm
  ├── joined-live retirement [covered #648]
  └── same-root handoff [covered #652]

[A0] selector -> runtime_for_target             [A0] agent works in foreign project
  ├── active omission [existing]                  ├── [GAP] conventions/inspect
  ├── exact id/name [existing resolver]            ├── [GAP] checkpoint no wrong write
  ├── unknown/ambiguous [existing resolver]        ├── [GAP] impact/diff correct Git root
  └── single vs merged dispatch [GAP]              └── [GAP] references + path

[B0] edit outcome -> MCP result                 [B0] agent retries/continues after edit
  ├── applied/dry run [covered]                   ├── [GAP] missing symbol isError=true
  ├── not found/ambiguous [wrong today]            ├── [GAP] rollback per-op status
  └── internal/rollback incomplete [partial]       └── [GAP -> host E2E] metadata dropped

[B1] index/open -> gitignore authority          [B1] index a clean clone
  ├── observe only [GAP]                          ├── [GAP] git status remains clean
  ├── explicit init write [covered]               └── [GAP] remediation names init
  └── protected/concurrent/symlink [covered]

[C] classification/routing/search              [C] ask and search naturally
  ├── test/tests/testutil [GAP]                   ├── [GAP] include_tests false
  ├── terminal path punctuation [GAP]             ├── [GAP] src/lib.rs? resolves
  └── unified text + fair lanes [GAP]              └── [GAP] docs glob never false-zero

[D] response composition                       [D] smallest useful context first
  ├── outline budget [wrong today]                ├── [GAP] default + cache have outline
  ├── compact provenance [GAP]                    ├── [GAP] docs fact beats changelog
  └── summary-first impact [GAP]                  └── [GAP] counts visible before detail

[S0] surface contract decision                 [S0] agent connects and completes a task
  ├── full-39 [works, schema-heavy]               ├── [GAP -> E2E/EVAL] facade parity
  ├── compact-3 [small, readiness history]        ├── [GAP -> E2E] recovery instruction
  └── successor decision [GATED]                  └── [GAP -> EVAL] measured net tokens
```

Every `[GAP]` is a required regression test in its owning PR. R0 and the host-visible
mutation flow require end-to-end tests because unit mocks hide the defect class. The
other branches are deterministic unit/integration targets.

## Failure-mode requirements

| Slice | Production failure to prevent | Required behavior |
|---|---|---|
| R0 | Background verification and watcher reconciliation each wait for the other | Bounded terminal state; release gate fails with captured freshness/verification |
| PR 1 | Selector resolves a closed/retired or same-named project | Deterministic refusal with candidates; no fallback to home |
| PR 2 | Host drops `_meta` | `isError` and bounded text still convey failure |
| PR 3 | `.gitignore` missing or unwritable | Index remains usable; receipt says missing/unverifiable; source unchanged |
| PR 4 | Over-broad test/punctuation rule hides normal source or damages path | Positive controls retain `contest.rs` and legal path punctuation |
| PR 5 | Approved `all` scope floods code results or exposes withheld text | Code-first deterministic tiers; safety filter before candidate formatting |
| PR 6 | Knowledge provenance consumes the entire response | Outline survives; provenance becomes count + retrieval handle |
| PR 7 | Compact mode loses auditability | Full provenance remains addressable by stable review/CCR handle |
| PR 8 | Rename changes same-spelling foreign symbol or misses exact edge | Transaction refuses or reports unresolved exact/uncertain sets before apply |

No listed failure may be silent. A path with neither a test nor an explicit refusal is
not complete.

## Parallel worktree strategy

| Lane | Work | Depends on |
|---|---|---|
| A | PR 0 -> PR 1 -> retrieval successor approval -> PR 5 -> PR 7 | Lifecycle adjudication, then selector/search/knowledge chain |
| B | PR 2 -> PR 8 | Mutation truth before rename |
| C | PR 3 | Independent source-purity slice |
| D | PR 4 | Independent deterministic-edge slice |
| E | PR 6 | Independent response-composition slice; merge before PR 7 |
| F | O0, O1, O2 as separate small PRs | Independent operational debt |
| G | Surface-contract evidence and compact parity harness; no default-switch ticket yet | Product/spec-tree approval, then R0, PR 1, PR 2, PR 5, PR 6, and PR 7 benchmark |

Launch PR 0 evidence work plus B/C/D/E/F in parallel only if PR 0 remains green. PR 1
and PR 2 both touch protocol dispatch/result seams, so merge/rebase PR 1 before PR 2
if their diffs overlap. Parser PRs begin only after PR 1/2/5 stabilize the contracts
their acceptance tests consume. Lane G may gather evidence and build a non-shipping
parity harness early, but it cannot create a default-switch implementation ticket
until the successor surface contract is approved and cannot switch defaults until the
trust dependencies merge.

## Release gates

For each Rust PR:

1. focused red/green tests for the exact defect;
2. `cargo fmt --all -- --check`;
3. `cargo check`;
4. `cargo clippy --all-targets -- -D warnings`;
5. `cargo test --lib --bins --tests -- --test-threads=1` (cap heavy Windows work at
   `-j 8` and use the long-running job owner rather than a hard 600-second kill);
6. `cargo test --no-default-features --features embed --lib -- --test-threads=1`;
7. npm tests when package/host integration is touched;
8. packaged artifact smoke for R0, selector persistence, or host-visible status work;
9. rerun the equivalent-workflow token corpus after PR 7, comparing schema + request +
   response + retry + CCR + provenance totals rather than health estimates alone.

### Landing this ledger

This file is a new document under the unsealed `docs/reviews/` tree. Land it alone via
a short-lived docs branch and pull request with a conventional `docs:` subject. Let the
doc-hygiene hook request its owner/metadata rather than bypassing the hook. Do not mix
this planning artifact with a product-code change, version bump, or either successor
specification; those have different review authority.

## What already exists and must be reused

- `DaemonState::runtime_for_target` — canonical session/open-project selector.
- `ProjectEvidence` and result-status `_meta` — retain, do not replace.
- `IndexTargets::{Code, Knowledge, CodeAndKnowledge}` and `SearchScope` — enough to
  implement truthful text lanes without a second index.
- `GitignoreHygieneAuthority::ObserveOnly` and `ProjectAwareInit` — the exact
  authority split required for non-mutating indexing.
- strict lifecycle publication, watcher freshness, snapshot verification, and the
  #643/#648/#652 regression controls.
- CCR/review paths — use them for progressive provenance rather than inventing
  generated summaries or duplicate persistence.
- `FileClassification` as the shared test/noise classifier.
- edit transaction, rollback, idempotency, and parsed reference graph seams.

## NOT in scope

- Rewriting V11 lifecycle architecture; current findings fit existing seams.
- Editing, checking boxes in, or silently weakening the frozen Feature 020 tree.
- Treating either proposed successor contract as implemented or approved because this
  review ledger recommends it.
- Reopening #643/#648/#652 without a failing packaged-11.0.8 sequence.
- Publishing stale 11.0.6 artifacts or changing completed branch/PR cleanup.
- Searching binary, catalog-only, secret-positive, or otherwise withheld bytes.
- Allowing document matches to dominate by volume; unified search uses bounded lane
  quotas, not unrestricted concatenation.
- An LLM synthesizer inside the Rust server for `ask`; describe it as a router unless
  deterministic synthesis is separately specified.
- New database, vector store, embeddings service, or knowledge sidecar.
- Physical document move/delete workflows.
- Weakening secret filtering, degraded coverage labels, snapshot byte exactness,
  frecency neutrality, dry-run safety, or project retirement checks.
- Curation apply fallback without the existing atomic-durability contract.
- Rewriting Git history to cure the two legacy conventional-commit subjects.

## Implementation tasks

- [ ] **T0 (P0 conditional)** — Run the packaged 11.0.8 readiness sequence and close
  or reopen R0 with a terminal-state receipt.
- [ ] **T1 (P0)** — Unify selected-project schemas and dispatch through
  `runtime_for_target`; verify every Git/snapshot/read result against project evidence.
- [ ] **T2 (P0)** — Make unsuccessful mutations MCP errors at the serialized host seam
  while retaining structured status metadata.
- [ ] **T3 (P1)** — Make normal index/open observe `.gitignore`; reserve hygiene writes
  for explicit init.
- [ ] **T4 (P1)** — Fix exact test stems and extracted-path punctuation with positive
  controls.
- [ ] **T5a (contract gate)** — Derive the retrieval successor specification and its
  outside-Feature-020 amendment record; obtain the required manifest, attestation,
  exact-target review, and trusted external approval without editing frozen bytes.
- [ ] **T5b (P0 impact, after T5a)** — Implement explicit textual scopes so admitted
  documentation cannot produce an unexplained code-lane zero.
- [ ] **T6 (P1)** — Make file context outline-first, cache independently useful, and
  impact output summary-first.
- [ ] **T7 (P1)** — Compact and authority-rank knowledge discovery, then rerun net-token
  evaluation.
- [ ] **T8a (architecture decision, not a ticket)** — Derive and approve the successor
  surface contract against `CLAUDE.md`, including compatibility, cache scope, and
  client-init migration.
- [ ] **T8b (only after T8a)** — Prove compact-facade task parity; if the decision is
  compact-default, land the entire atomic change set in one reviewed migration.
- [ ] **T9 (P1)** — Make rename confidence derive from exact parsed identity and refuse
  unresolved exact edges.
- [ ] **T10 (P1/P2)** — Land one parser/secondary ergonomics concern per PR.
- [ ] **T11 (P3)** — Fix the health timing test, diagnose Windows proxy evidence, and
  bound release commit validation to the current push in separate PRs.

## Review verdict

### Architecture

Five issue clusters exist, but none requires a new runtime subsystem. The load-bearing
choice is to reuse the current selector, target, authority, result-status, and CCR
seams. Two proposed behavior changes do require new successor contract artifacts. The
implementation campaign is too coupled for one PR and cleanly decomposes into six
parallel lanes after those gates.

### Code quality

The main debt is semantic duplication at adapters: single-project requests are being
treated as cross-project, mutation failure semantics differ by consumer, and code
versus knowledge budgets are enforced after independent formatting. Centralize each
decision once at its existing choke point.

### Tests

The suite has strong unit controls for lifecycle and safety, but lacks packaged
topology, consuming-host, and default-response composition oracles. The coverage map
above makes those gaps release requirements.

### Performance

Do not optimize indexing or add caches before response composition is corrected.
Current measurements show ordinary code tools around 216-246 ms and knowledge around
1.56 s on the historical corpus; the more urgent cost is extra agent turns and
provenance volume. Measure p50/p95 and total equivalent-task tokens after PR 7.

### Product direction and authorization boundary

The 2026-08-24 user direction prioritizes the superior end state: maximum net token
savings subject to correctness and LLM trust. This ledger recommends unified
exact-text retrieval and records the evidence for considering compact-default, but it
does not override frozen Feature 020 or the current `CLAUDE.md` surface architecture.
Those are unresolved successor-contract decisions. Code begins only after the stated
authority gates; until then the shipped contracts remain authoritative.
