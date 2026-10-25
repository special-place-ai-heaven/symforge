# Differential overlap implementation ledger

Status checked 2026-10-25. Project: `symforge`.

The three concrete advantages in the owner's quoted Differential assessment are
implemented and verified at `3565e46bdd2aef42b6aa17f4f7f5504642c3e1ae`, then
landed on main through PR #826 at
`113cc66978f487dabeb1d2bfad00d336bd916555` (2026-10-25 18:07 UTC).
The merge tree exactly matches reviewed branch tip
`276c627034c8dc8b7d6208c08d9547880780e52c`; its product source and Cargo/npm
inputs are unchanged from the verified implementation. The remaining delivery
work is publishing a new version, not implementing these mechanisms again.

Delivery: [PR #826](https://github.com/special-place-ai-heaven/symforge/pull/826).
Verification: [CI run 40207500654, attempt 2](https://github.com/special-place-ai-heaven/symforge/actions/runs/40207500654),
completed successfully on the implementation commit above. The owner subsequently
authorized consolidating branches into main and issuing an installable version
through Release Please. This ledger records that delivery separately from code
completion; a green branch is not a merged or released product.

## Status and execution order

`LANDED` means implemented, reviewed, verified and integrated into main.
`OPEN` means its acceptance criteria have not been demonstrated. Close delivery
items only with actual merge, tag and package evidence.

| ID | Priority | Work item | State | Owner / dependency |
| --- | --- | --- | --- | --- |
| D01 | P1 | Isolate dependency matching by language family | LANDED | Rust graph engineer; shared by both adapters |
| D02 | P1 | Use one immutable Git comparison baseline | LANDED | Rust Git/change engineer; both adapters |
| D03 | P1 | Expose inspectable dependency evidence | LANDED | Rust graph/impact engineer; D01, D02 |
| D04 | P1 | Verify server/embed parity for every fix | LANDED | Independent verifier; D01-D03 |
| D05 | P1 | Consolidate unique branch work into main | CLOSED | Integration lead; D01-D04 |
| D06 | P1 | Release an installable version with Release Please | OPEN | Release lead; D05 |

## D01 — prevent cross-language false dependencies

Problem: an unrelated JavaScript `read_to_string` definition could make a Rust
`std::fs::read_to_string` caller appear affected, or make a real match ambiguous.

Implementation: `graph_language_family` and `(family, name)` definition buckets
in [graph.rs](../src/live_index/graph.rs). Rust and C# remain isolated;
JavaScript/TypeScript, C/C++, and Java/Kotlin use their respective compatible
families. This does not infer FFI relationships or add Kotlin parser work.

Acceptance evidence:

- [x] The reproduced Rust/JavaScript false edge is absent.
- [x] Unrelated definitions do not suppress a valid Rust match.
- [x] Compatible-family controls remain valid.
- [x] Regressions include `rust_std_call_does_not_resolve_to_unrelated_javascript_definition`
  and `same_name_definitions_resolve_within_compatible_language_family`.

Remaining implementation gap for the quoted finding: none established on the
verified branch. This is an accuracy correction, not complete compiler resolution.

## D02 — compare the same pinned endpoints throughout

Problem: changed paths used a merge base while symbol comparison used the base
branch tip, incorrectly including a base-only `beta` edit beside a feature's
`alpha` edit in the same file.

Implementation: `GitComparisonRange`, `comparison_range_between_refs`, and
`resolve_ref_tree_reference` in [git.rs](../src/git.rs), consumed by shared
[change guidance](../src/index_lifecycle/guidance/changes.rs),
[embed changes](../src/index_lifecycle/embed_changes.rs), and
[MCP handlers](../src/protocol/tools.rs). Ref-to-worktree reports read the actual
worktree against one pinned tree; provenance distinguishes tree and commit IDs.

Acceptance evidence:

- [x] Divergent branches report feature `alpha`, excluding base-only `beta`.
- [x] Changed-path selection and blob comparison use the same effective baseline.
- [x] Dirty worktree content is compared instead of substituting HEAD content.
- [x] Requested labels remain visible; actual endpoints are disclosed.
- [x] Commit refs, annotated tags and tree-only refs have explicit provenance.
- [x] `diff_comparison_server_uses_resolved_merge_base` and direct paired
  regressions cover the repaired behavior; the full default golden also passes.

Remaining implementation gap for the quoted finding: none established on the
verified branch. Unresolvable refs are refused; they do not silently change scope.

## D03 — retain the explanation for an affected result

Problem: impact output retained symbol, hop and risk while dropping the call site
and resolution explanation already available during graph construction.

Implementation: `CallEdgeEvidence` and `ResolutionReason` in
[graph.rs](../src/live_index/graph.rs), shared shortest-chain traversal and
256 KiB evidence cap in [impact guidance](../src/index_lifecycle/guidance/impact.rs),
and additive typed results in [the embed parity API](../src/embed/parity/detect_impact.rs).
Contract: [detect-impact addendum](../specs/015-cbm-capability-ports/contracts/detect-impact.md).

Acceptance evidence:

- [x] Returned evidence includes caller/callee identities, source path, raw
  half-open byte spans, zero-based half-open line spans and resolution reason.
- [x] Each available explanation is one deterministic shortest chain from the
  changed symbol, subject to the existing depth bound.
- [x] File scope retains its selected representative's chain.
- [x] Evidence uses the admitted captured publication and contains no source text.
- [x] Bounded output uses explicit `available`, `unavailable`, or `truncated`
  status; truncating evidence preserves impact rows and risk totals.
- [x] Legacy typed payloads deserialize without fabricated evidence.
- [x] `graph_edges_keep_each_call_site_with_resolution_reason` and the paired
  cap/budget regressions cover these behaviors.

The implementation also corrects same-line C# caller attribution using byte
containment. Ambiguous unqualified calls can still be omitted deliberately;
`unavailable` is not proof that no dependency exists. No new resolver project is
justified by the quoted comparison alone.

## D04 — parity and verification gate

- [x] Actual MCP handler output equals public embed output for the changed Git
  and Rust/C#/C++ impact behaviors, in symbol and file scopes.
- [x] Shared-cap output matches exactly with all 150 ordered impact rows.
- [x] Constrained embed budgets disclose truncation without losing rows/totals.
- [x] Linux and Windows explicitly run both `server_and_embed_` tests; empty
  selection fails their gates. These tests are in [tools.rs](../src/protocol/tools.rs):
  `server_and_embed_match_git_and_multilanguage_impact_payloads` and
  `server_and_embed_disclose_evidence_truncation_without_dropping_rows`.
- [x] Final CI: full Linux native suite including 3,749 library tests, benchmark
  smoke, release build and both live tool-correctness fixtures; Windows 4,785
  tests plus 2 paired tests; embed 1,838 library tests plus 25/11/31/32 contract
  tests; downstream consumer, musl, Darwin, npm, format and lint gates.

The existing parser timing-ratio test failed once during concurrent CI and
passed a targeted job retry with its assertion unchanged. A Windows lifecycle
test passed Nextest's configured retry. Scheduled/manual performance-smoke was
skipped by its PR condition; native benchmark smoke ran. These results establish
the repaired behaviors, not universal performance superiority over Differential.

## D05 — land and consolidate

- [x] Reconcile every local/remote branch against fresh main: already merged,
  patch-equivalent, or genuinely unique work; preserve dirty worktrees.
- [x] Land PR #826's reviewed implementation on main using current passing
  evidence and repository merge rules.
- [x] Verify the landed source against the tested implementation; independently
  review any conflict resolution and rerun affected checks if code changes.
- [x] Record landed commit and prune only proven superseded branches/worktrees.

Do not replay an abandoned intermediate implementation solely because its
branch still exists. Preserve unique intended work and record its disposition.

Branch audit: the AAP surface and LLVM-tarball branches are ancestors; clang-cl
and xwin-clang patches are equivalent to integrated work. The old Windows/Linux
release, resource-outcome, AAP WIP, and pre-reset backup branches are superseded
by the current typed outcomes, lease/census behavior, native Windows release
matrix and tested AAP implementation. The README refresh is byte-identical to
the current README; its two remaining contract-claim corrections were ported
to `docs/contracts/embed-parity-v1-coverage.md`. Stale remote tracking refs were
pruned. All 11 old local branch tips were preserved as identical-tree local tags
under `archive/2026-10-25/<original-branch>` before removing their branch refs;
main was the only local branch at that consolidation checkpoint. The merged
remote feature branch was deleted.
The detached `symforge-release-1186` worktree has no unique tracked code but
contains untracked release artifacts, so it is preserved.

The release workflow also now routes Linux jobs and the Linux build matrix
entry to the owner's `fresh` pool while keeping Windows on `windows-latest`.
YAML parsing, semantic before/after comparison and the narrowly refreshed
workflow fingerprint passed; publishing execution verifies the runner path.

## D06 — publish the new version

- [x] Let the repository's Release Please configuration generate the version,
  changelog, Cargo and npm package updates after D05.
- [x] Verify the generated release PR and required checks, then land it.
- [ ] Verify the release tag points to main and contains D01-D04.
- [ ] Verify required platform artifacts and npm packages publish successfully.
- [ ] Record the exact version and a Windows installation command, then verify
  the published executable reports that version. Do not replace or restart the
  owner's active daemon implicitly.

Release Please generated and auto-merged
[PR #827](https://github.com/special-place-ai-heaven/symforge/pull/827), version
`11.10.2`, at main commit `686bcbe907afaa844e47958a8eafe0daded1a530`.
Its seven-file diff is restricted to release metadata, changelog, Cargo and npm
versions. Main ancestry confirms that it contains the landed improvements.
[Publication run 40222515232](https://github.com/special-place-ai-heaven/symforge/actions/runs/40222515232)
is the remaining execution gate; version metadata alone is not proof of
published installable artifacts.

Publication run 40222515232 stopped before tag/package creation because its
embed compilation exhausted runner disk space. Two separate main-CI runners
failed with ENOSPC within the same seconds (2026-10-25 19:15 UTC), supporting
shared backing-storage pressure under concurrent builds. Native tests, benchmark
smoke and the release binary build had already passed. The same release workflow
had passed on the pre-version commit.

The bounded CI mitigation disables incremental artifacts and dev/test debug
symbols, retains debug assertions and the release profile, and adds disk/inode
observations before heavy Cargo work. It does not remove test gates, delete
system caches, reduce the five-runner pool or move Windows off hosted runners.
Publishing remains open until a successful retry/new workflow and registry proof.

## Scope boundaries and separate follow-ups

No further unimplemented Differential advantage among the three quoted findings
was established by the source check. A wider comparison needs new reproductions
and a pinned competitor revision before adding implementation work. General
parser/index superiority and complete semantic resolution remain unproven.
The Malwarebytes detection and upstream standalone SCSS/MSVC build limitation
remain separately tracked issues, not unfinished Differential integrations.

## Historical execution record

The entries below describe the original implementation and interim handoff;
the status table and final CI evidence above supersede their pending-check notes.

Requested 2026-10-25. Starting checkout: `main`, `e09b88ed`, SymForge 11.10.1.
Scope: improve existing change/impact tools; no new tool, dependency, or reviewer UI.

## Acceptance gates

- Impact graph definitions are isolated by compatible language family. An unrelated
  JavaScript definition cannot create a Rust call edge or suppress a valid Rust edge.
  JavaScript/TypeScript and C/C++ compatibility remains covered.
- Symbol comparisons resolve immutable endpoints once. Divergent-branch three-dot
  comparisons include feature changes and exclude base-only changes in the same file.
  Requested refs remain visible; commit provenance names the actual compared endpoints.
- Impact results expose one deterministic shortest supporting call chain per returned
  node, including call-site spans and resolution rationale. Evidence is bounded, follows
  existing admission policy, and does not include source text. Truncation is explicit.
- Server and embed consumers agree. Existing supported configurations compile and
  relevant tests, formatting, lint, and repository verification gates pass.
- Independent review examines correctness, disclosure, compatibility, and scope.

## Ownership

- Lead: scope, contract decisions, integration review, this record.
- `rust_impact`: graph namespace isolation and evidence; impact consumer integration
  after coordinating shared-file ownership with `rust_range`.
- `rust_range`: Git range resolution, symbol diff consumers, directly affected change
  consumer regressions. Shared files are owned by symbol, with sequential handoff.
- `verification`: baseline, RED/GREEN regression execution, supported configuration
  checks, live process verification. One Cargo build/test job at a time.
- Independent reviewer: assigned after implementation is ready.

## Evidence and status

- Initial worktree clean; running MCP reports 11.10.1.
- Baseline: affected default/server library tests 48/48 passed; preventive source
  gates 10/10 passed (Terminal Commander observed exits 0).
- RED: divergent-branch embed regression failed with actual `[alpha, beta]` versus
  expected `[alpha]` (observed exit 100). Range implementation released.
- RED: two cross-language graph regressions failed while compatibility controls
  passed. A focused same-line C# regression subsequently exposed calls attributed
  to the enclosing class; it failed before the byte-containing caller correction.
- GREEN: final focused default/server checks 63/63; previously failing embed impact
  and golden checks 3/3. Full/default/embed/lint/live gates remain in progress.
- Independent review closed tree-versus-commit provenance, ref-movement races,
  line-span conventions, and evidence byte budgeting findings. No new runtime or
  lifecycle access bridge was found; source seal refresh awaits lint-stable bytes.
- The owner subsequently directed remaining platform checks to CI: Linux jobs
  use the five custom runners labeled `fresh`; Windows uses `windows-latest`.
  Delivery proceeds through a review branch and draft PR. No deployment or
  user-daemon restart is included.
- Separate open product issue: Malwarebytes Anti-Ransomware quarantined the installed
  released 11.10.1 binary on 2026-10-25 (rule 392685). Exact triggering operation is
  unestablished. The owner added an exception for local use and explicitly deferred
  this investigation so the original improvements can continue. Agents changed no
  antivirus settings; the exception is not a product resolution.

## Final local evidence and CI handoff

- Actual paired MCP-handler/public-embed checks passed 5/5, including exact
  shared-cap output with all 150 ordered rows; default affected checks 63/63.
- Live isolated MCP calls passed divergent Git range, Rust chain/isolation,
  C# method attribution, and qualified C++ evidence cases.
- Default clippy, embed clippy, formatting, cargo check, and the release build
  passed. The final all-features lint rerun, full suites, and benchmark smoke
  are delegated to CI under the owner's platform instructions.
- The external embed consumer passed using the repository's existing SCSS
  patch. An unpatched standalone Windows consumer hits the documented upstream
  `-Wno-unused-parameter`/MSVC build failure; that is separate from these fixes.
- Source and workflow pins were independently recomputed and reviewed. No
  source roster, excluded path, hash domain, or guard assertion was relaxed.
- Both CI platforms now run the explicit dual-feature paired regression gate.
