# Differential comparison improvements

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
