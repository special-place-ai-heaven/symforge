/goal Resolve all 11 SF issues from the 2026-06-03 SymForge MCP audit per the
verified implementation plan — fix the 8 real code defects (SF-002..SF-008, SF-010,
SF-011), add tests + ops guard for the already-fixed SF-001, and add untracked-file
surfacing for the refuted-mechanism SF-009 — until every per-issue acceptance
criterion + regression test is met, the full verification suite is green, each
live-verifiable fix is proven against the REAL repos on a freshly-restarted daemon,
and the work is integrated to a review branch — without changing the find_dependents
matching algorithm (SF-001), without changing index admission defaults (SF-009),
without touching the working-tree gsd-* -> local-agent-* rename, and without any
push/merge to main (commit to a review branch and STOP for human review).

Context:
  - Repo: C:\AI_STUFF\PROGRAMMING\symforge  (Rust MCP server — symbol-aware code
    navigation/editing tools). Branch: main. Cargo version 7.18.1 (HEAD is PAST the
    v7.18.0 tag — this matters for SF-001 below). Project rules: see
    C:\AI_STUFF\PROGRAMMING\symforge\CLAUDE.md (verification commands, CI gates,
    Tool Consolidation Pattern, key source-file map).
  - SOURCE OF TRUTH (read both BEFORE doing anything; do NOT re-derive the analysis):
      * Actionable plan (repair order, per-issue fix location/approach/regression
        test/risk, ALL verifier corrections baked in):
        C:\AI_STUFF\PROGRAMMING\symforge\docs\SYMFORGE_BUGFIX_IMPLEMENTATION_PLAN.md
      * Per-issue verified verdicts + corrections:
        C:\AI_STUFF\PROGRAMMING\symforge\docs\SYMFORGE_MCP_BUG_REPORT_2026-06-03_VERDICTS.md
      * Original audit (reference only — superseded where it conflicts with the two above):
        C:\AI_STUFF\PROGRAMMING\symforge\docs\SYMFORGE_MCP_BUG_REPORT_2026-06-03.md
    Provenance: produced by 11 root-cause investigators + 11 adversarial verifiers
    + 7 live-reproduction agents against the REAL Agent_Army_Professionals (AAP),
    testpilot, and symforge repos. Treat these docs as gospel; this goal POINTS to
    them — open the per-issue spec section before implementing each issue.
  - Real repos on disk (confirmed present), used for live verify-as-user:
      * C:\AI_STUFF\PROGRAMMING\Agent_Army_Professionals  (Rust — co-change, dependents)
      * C:\AI_STUFF\PROGRAMMING\testpilot  (TypeScript / Angular — TS/HTML parsing, xref)
  - Verified ground truth (do not re-litigate): tree-sitter-typescript = 0.23.2 and
    tree-sitter-html = 0.23.2 are the LATEST published crates (no grammar-bump remedy);
    .ts/.tsx share LANGUAGE_TYPESCRIPT (one fix covers both); SF-001 collision-filter
    commit 251d7f0 is a VERIFIED ancestor of v7.18.0 (git merge-base --is-ancestor
    returns true), so the fix is already in this HEAD.
  - SCOPE COUNTS: 8 confirmed code fixes (SF-002, SF-003, SF-004, SF-005, SF-006,
    SF-007, SF-008, SF-010 [half only], SF-011); SF-001 = tests + optional ops guard
    ONLY (already fixed); SF-009 = surfacing ONLY (mechanism refuted).
  - Runner: this is a self-orchestrating overnight-capable campaign. The runner is the
    ORCHESTRATOR — it dispatches specialists per issue, gates each fix, integrates via
    git-master to a review branch, then STOPS. Always use `git -C C:\AI_STUFF\PROGRAMMING\symforge`
    (the runner may be in HOME). This goal file is throwaway: self-delete it when the
    campaign is complete and the results doc is written.

Success criteria (MEASURABLE — "done" = a concrete, checkable outcome, NOT
  "it compiles" / "tests pass alone" / "it works"):
  [1] All 11 SF issues are resolved exactly per their per-issue spec in
      SYMFORGE_BUGFIX_IMPLEMENTATION_PLAN.md, including the verifier corrections, and
      each issue's named regression test(s) exist and pass. Specifically:
        - SF-001: a REAL-PARSER end-to-end test in tests/find_dependents_pass2.rs that
          parses AAP-shaped work_item.rs + actor.rs via symforge::parsing::process_file
          (NOT hand-built ReferenceRecords) asserts find_dependents_for_file(work_item.rs)
          yields ZERO refs whose path contains actor.rs. NO change to the matching algorithm.
        - SF-002: controller method callers.total_count == 0 for the testpilot
          same-name delegation fixture; controller is NOT its own callee (appears in a
          new unresolved_same_name_member_call section); a true bare top-level foo() is
          STILL counted; fix uses enclosing_symbol_index (NOT a byte heuristic).
        - SF-003: the import-type+[] fixture classifies as ok/parsed with symbols still
          extracted; the negative-control broken file ("import('rxjs').Subscription[] = [ ; foo bar")
          STAYS partial/failed (detector is SOUND — validates the whole construct);
          checkpoint serde back-compat is preserved.
        - SF-004: an Angular @if/@for .html lands under [expected_framework_partial]
          (not [unexpected_partial]) in health AND get_file_context no longer labels it
          a bare "partial".
        - SF-005: ask("Where is TestingController defined and what module imports it?")
          classifies as FindSymbol{name:"TestingController"} with the truncation flag set
          (confidence downgraded to Inferred + a suggested_next_step naming
          search_symbols -> find_references); "where is file tools.rs" still routes to FindFile.
        - SF-006: a stem query that names the anchor (query="work_item",
          anchor=".../work_item.rs", rank_by="path+cochange") applies co-change (NOT
          "fallback used") with the partner under the co-change tier; a true-prefix query
          ("work") still falls back BUT with the precise reason (one of the four distinct
          reasons incl. chore-anchor). The maintainer's >=3-char stem policy decision is
          recorded in the results doc.
        - SF-007: a daemon round-trip checkpoint_now succeeds (body contains "Checkpoint
          complete", NOT "unavailable in daemon-proxy mode", NOT "unknown tool"; snapshot
          exists), via proxy-first forwarding + a daemon dispatch arm.
        - SF-008: on a Windows build, format_shadow_warning ForeignPrefix arm emits
          PowerShell-native guidance (contains "Get-Command symforge -All", names both bin
          dirs, is_ascii(); does NOT contain "~/.profile" or "export PATH"); POSIX text
          retained for non-Windows. classify_shadow/ShadowKind unchanged.
        - SF-009: a non-dotfile untracked recognized-ext file surfaces as
          untracked_indexed == 1 and the health line contains "indexed untracked files: 1";
          feature FAILS OPEN (no git / no index -> off, count 0); admission defaults UNCHANGED.
        - SF-010: ask("what tools can I use for impact analysis?") classifies as
          ToolHelp and returns a tool list containing find_references / find_dependents /
          get_symbol_context / analyze_file_impact / what_changed / diff_symbols; "how does
          the Tool registry work" still routes to Understand/Explore (NOT ToolHelp); every
          tool_catalog_groups() name exists in SYMFORGE_TOOL_NAMES. Lazy-exposure half is
          documented as harness behavior (out of scope).
        - SF-011: a TS-majority fixture yields conventions whose error_handling does NOT
          say "Result-based", mentions exceptions, naming mentions camelCase, test count is
          nonzero, and a "language: TypeScript" header is present; a Rust-majority fixture
          still yields "Result-based"/% snake_case. Config files excluded by LanguageId/is_config
          (NOT FileClass); per-file scan gated by language.
  [2] The full project verification suite is GREEN on the final review branch:
      cargo fmt --check, cargo check, cargo clippy --all-targets -- -D warnings,
      cargo test --all-targets -- --test-threads=1, cargo build --release. If npm/ was
      touched: cd npm && npm test. (No npm/ change is expected for these issues.)
  [3] LIVE verify-as-user PROOF (compiling + green tests is necessary but NOT
      sufficient): for SF-002, SF-003, SF-004, SF-005, SF-006, SF-007, SF-008, SF-010,
      SF-011, the relevant MCP tool was re-run against the REAL repo (testpilot for
      TS/Angular: SF-002/003/004; AAP for Rust co-change/dependents: SF-006/SF-001
      cross-check; symforge-self or either for routing/ops: SF-005/007/008/010/011) on a
      daemon FRESHLY built+restarted from the fixed branch, and the corrected output was
      observed and captured (exact tool, args, before/after snippet) in the results doc.
  [4] All work is integrated by git-master onto ONE review branch
      (e.g. fix/symforge-mcp-audit-2026-06-03), committed in scoped commits (per-issue or
      per-phase, cleanup never mixed with feature/refactor). NOTHING is pushed, no PR is
      opened, no merge to main — the campaign PAUSES at this gate for human review.
  [5] ONE honest results doc is written (incrementally, to survive a crash) at
      C:\AI_STUFF\PROGRAMMING\symforge\docs\SYMFORGE_BUGFIX_RESULTS_2026-06-03.md
      recording per-issue: FIXED / VERIFIED-LIVE / TEST-ONLY / DEFERRED, the verification
      evidence, the live-verify before/after, any open maintainer decision (SF-006 length
      policy; SF-002 output-shape; SF-003 shape A-vs-B), and any remaining gap. Mock is
      labeled mock, unverified is labeled unverified. No premature "all works".

Constraints:
  [SF-001-frozen]   Do NOT change the find_dependents matching algorithm
                    (matches_exact_symbol_qualified_name / matches_exact_symbol_reference /
                    can_match_type_dependents). Changing it regresses legit cross-module
                    dependents pinned by real_qualified_call_dependent_still_reported.
                    SF-001 work = real-parser regression test + (optional, high-leverage)
                    daemon binary-staleness health guard ONLY.
  [SF-009-frozen]   Do NOT change index admission defaults / the ignore-crate hidden
                    filter / language tiering. SF-009 work = surface "indexed untracked
                    files: N" ONLY, and it MUST fail open (no git index -> feature off).
  [SF-010-scope]    Only the ask() ToolHelp intent + symforge://tools/catalog half is in
                    scope. The "lazy exposure" two-pass discovery is HARNESS behavior —
                    document it out of scope, do NOT attempt a server-side fix.
  [verify-as-user]  "Verified" means a live, real-flow MCP tool call against the real repo
                    on a freshly-restarted daemon — NEVER code-reading, NEVER "tests
                    passed" alone. Capture exact tool+args+output.
  [kill-stale-daemon] Before EVERY live verification: kill stray symforge daemons, rebuild
                    from the fixed branch, confirm the SERVED binary is the new one (PID +
                    version), bypass any reused session/cache. SF-001 is the cautionary
                    tale — a shadowed/reused daemon serves stale behavior and produced the
                    audit's two biggest misses.
  [code-is-gospel]  Trust the file:line citations in the plan, but RE-READ each cited
                    symbol/file immediately before editing (context may be stale); verify
                    claims against the running code, treat comments/docs as stale-until-confirmed.
  [exhaustive-match] SF-003 (new FileOutcome/ParseStatus variant) and SF-010 (new
                    QueryIntent variant) intentionally break every match at compile time —
                    that is the safety net; fix EVERY arm including the test-only
                    dispatch_tool_for_tests / decode paths. SF-003 ADDITIONALLY changes the
                    PERSISTED checkpoint format — confirm serde back-compat (a snapshot
                    written by the new binary must still load, and vice versa) before shipping.
  [public-contracts] SF-002 changes find_references/edit_plan caller counts (TS/JS/Py/C#/Go);
                    SF-006 changes search_files ordering for stem queries. These are
                    user-visible — update golden/snapshot tests and treat as DELIBERATE
                    behavior changes, never silent. VERIFY asserted output substrings
                    against the live format.rs before writing test assertions (the plan flags
                    several proposed substrings as unverified guesses).
  [tool-consolidation] When adding/removing any tool surface (SF-010 catalog/resource,
                    SF-007 dispatch), follow the Tool Consolidation Pattern in CLAUDE.md
                    (input struct + handler branch + daemon.rs dispatch + init.rs tool-name
                    list + cross-ref descriptions + tests).
  [scope-phases]    <=5 files per phase (CLAUDE.md). Serialize phases/issues that share a
                    file: SF-005 & SF-010 both touch smart_query.rs (serialize D1->D2);
                    SF-003 & SF-004 share format.rs/health_view.rs/store.rs (serialize
                    C1->C2). Do NOT batch a multi-file refactor in one response.
  [do-not-touch]    Leave the uncommitted working-tree gsd-* -> local-agent-* rename alone
                    (another agent's WIP, unrelated). Do not stage/revert/commit it.
  [cargo-cache]     Keep target/ WARM across the whole campaign for incremental speed. Run
                    cargo clean ONLY at the very end (campaign boundary), never between
                    phases/agents.
  [resource-throttle] Max 2 truly-parallel heavy cargo/check/clippy/test agents; batch
                    larger fan-outs. Read-only investigation can fan out wider. Serialize
                    same-file edits.
  [safety-gate]     NO push / NO PR / NO merge to main / NO force / NO destructive git.
                    git-master commits to a REVIEW BRANCH and STOPS. Pause for explicit
                    human approval before any of the above. Especially binding for an
                    unattended run.

Agent routing (dispatch the right specialist per issue; respect the 2-heavy-parallel cap):
  - rust-pro (Matsakis) — all the Rust code fixes: SF-002, SF-003, SF-004, SF-005,
    SF-006, SF-007, SF-008, SF-010, SF-011, plus the SF-001 test + SF-009 surfacing.
  - code-reviewer (Linus, read-only) — pre-merge review gate on every issue, with
    special attention to [SF-001-frozen]/[SF-009-frozen]/[public-contracts]/[exhaustive-match].
  - test-runner (Kent Beck) — run + repair the per-issue regression tests and the full
    suite; confirm golden/snapshot updates are deliberate.
  - debugger (House) — if the SF-003 sound-detector or SF-007 daemon round-trip misbehaves
    at runtime, root-cause it.
  - tech-researcher (Sherlock) — only if an open decision needs primary-source resolution
    (e.g. checkpoint serde back-compat strategy, SF-006 length policy) beyond what the plan states.
  - git-master (Junio Hamano) — Phase F integration: collect all issue work onto the review
    branch, scoped commits, resolve conflicts. HARD-GATED: commit + STOP, no push/merge.
  - browser-tester / charlotte — NOT needed (these are MCP-tool flows, not browser UI);
    live verify-as-user is done by invoking the MCP tools directly against a fresh daemon.

Checklist: (this file is the running log — tick items as completed; serialize where flagged)

PHASE 0 — BASELINE
  [ ] git -C <repo>: confirm branch main, clean except the known docs + working-tree
      gsd-* rename; create the review branch fix/symforge-mcp-audit-2026-06-03 from main.
  [ ] Read SYMFORGE_BUGFIX_IMPLEMENTATION_PLAN.md and VERDICTS.md fully (gospel).
  [ ] Establish a green baseline: cargo fmt --check, cargo check, cargo clippy
      --all-targets -- -D warnings, cargo test --all-targets -- --test-threads=1. Record
      the baseline state. Kill stray symforge daemons; note current served binary/PID.
  [ ] Build a release daemon from the baseline branch and confirm it serves (version + PID)
      so the "freshly restarted daemon" procedure is established for later live verifies.
  [ ] Create the results doc skeleton (one row per SF issue) and write to it incrementally.

PHASE A — High value, isolated (SF-007, SF-008) — can run as 2 parallel rust-pro agents
  [ ] SF-007 checkpoint_now forwarding — tools.rs (proxy-first) + daemon.rs (dispatch arm;
      import CheckpointNowInput; add dispatch_tool_for_tests arm if a parity test routes there).
      Gate: code -> code-reviewer -> test-runner (daemon round-trip test) -> LIVE verify
      (fresh daemon, checkpoint_now over the proxy returns "Checkpoint complete").
  [ ] SF-008 Windows PATH remediation — path_shadow.rs ForeignPrefix arm via cfg!(windows);
      keep ShadowKind/classify_shadow unchanged; ASCII-only; assert backslash form on Windows.
      Gate: code -> code-reviewer -> test-runner (#[cfg(windows)] test) -> LIVE verify
      (trigger/inspect the shadow warning on this Windows box; confirm PowerShell-native text).

PHASE B — Resolution/ranking correctness (SF-002, SF-006) — serialize if file overlap; else 2 parallel
  [ ] SF-002 TS same-name method — use enclosing_symbol_index in the same-file caller branch
      (query.rs), add unresolved_same_name_member_call section (context_bundle.rs +
      ContextBundleFoundView), render it (format.rs); callee-side self-call guard. Update
      find_references/edit_plan golden tests deliberately. Use a ref builder that decouples
      byte_range from line_range in the regression test (the plan's make_ref recipe is broken).
      Gate: code -> code-reviewer (public-contract focus) -> test-runner -> LIVE verify
      (testpilot get_symbol_context on TestingController.startExploration: total_count==0, not self-callee).
  [ ] SF-006 co-change stem gate + reasons — rank_signals.rs (stem-equality in PathMatchSignal::score;
      resolve the >=3-char policy and RECORD the decision), tools.rs (split the catch-all fallback
      into the FOUR distinct reasons incl. chore-anchor; compute anchor-confidence reason once),
      calibration tests. Verify asserted format substrings against format.rs FIRST. Keep
      weak_prefix_anchor_keeps_baseline_path_order... green.
      Gate: code -> code-reviewer -> test-runner -> LIVE verify (AAP search_files query="work_item"
      anchor work_item.rs rank_by=path+cochange debug_ranking=true: applies co-change, partner under tier).

PHASE C — Parse classification (SERIALIZE C1 -> C2; shared format.rs/health_view.rs/store.rs)
  [ ] C1 SF-003 import-type+[] -> grammar-limitation classification. SOUND detector (validate the
      WHOLE construct; negative control MUST stay partial). New FileOutcome::ExpectedGrammarLimitation
      -> ParseStatus::ParsedWithGrammarLimitation. CONFIRM checkpoint serde back-compat before
      choosing shape B (else shape A). Fix EVERY broken match arm. Touches parsing/mod.rs,
      domain/index.rs|store.rs, format.rs, tools.rs, health_view.rs.
      Gate: code -> code-reviewer (exhaustive-match + serde back-compat) -> test-runner (positive +
      type-alias positive + REQUIRED negative control) -> LIVE verify (testpilot validate_file_syntax /
      get_file_context on workflow-builder.component.ts: Status ok/parsed, not partial; symbols present).
  [ ] C2 SF-004 Angular @if/@for -> expected_framework_partial bucket. health_view.rs (detector +
      third branch + HealthStats fields), store.rs (PublishedIndexState fields — NO serde, no default),
      format.rs (ParseQuarantineKind::ExpectedFrameworkPartial + wiring), sidecar/handlers.rs
      (get_file_context parse_state label). Update health_parse_quarantine.rs hard-coded registry strings.
      Gate: code -> code-reviewer -> test-runner -> LIVE verify (testpilot app.html: under
      [expected_framework_partial], get_file_context no longer bare "partial").

PHASE D — Routing/UX (SERIALIZE D1 -> D2; both touch smart_query.rs)
  [ ] D1 SF-005 ask compound query — reorder "where is file " before bare "where is "; first-token
      extraction (drop the stop-word list); clean_symbol_name casing; downgrade to Inferred +
      suggested_next_step on truncation. Tests assert BOTH name and the truncated flag; add
      interior-article + FindFile-guard fixtures + tools.rs ask E2E.
      Gate: code -> code-reviewer -> test-runner -> LIVE verify (ask the compound question on a fresh
      daemon: Chosen tool search_symbols, invocation has "TestingController" not "imports it", Suggested next step present).
  [ ] D2 SF-010 ask ToolHelp + catalog — add QueryIntent::ToolHelp{topic} detected EARLY (scoped to
      tool(s) + recommendation verb; NOT in the Understand|Explore upgrade guard); tool_catalog_groups();
      ask arm; symforge://tools/catalog resource (resources.rs); init.rs/Tool Consolidation Pattern;
      drift guard test (every catalog name in SYMFORGE_TOOL_NAMES). Fix every QueryIntent match arm.
      Gate: code -> code-reviewer (exhaustive-match) -> test-runner -> LIVE verify (ask "what tools for
      impact analysis?" returns the impact tools; "how does the Tool registry work" is NOT ToolHelp).

PHASE E — Advisory + ops/tests (SF-011, SF-001, SF-009) — SF-011/SF-001/SF-009 are file-disjoint enough to parallel ~2 at a time
  [ ] SF-011 language-aware conventions — conventions.rs: dominant-lang vote excluding config by
      LanguageId/is_config (NOT FileClass); fold JS+TS; gate per-file scan by file.language; TS/JS
      branch (try/catch, throw new, HttpException, RxJS catchError, camelCase/PascalCase, *.spec.ts +
      describe/it scan, decorator content-scan); language header; generic default branch. Add the
      #[cfg(test)] mod tests (none today) — wire file.language explicitly on fixtures.
      Gate: code -> code-reviewer -> test-runner -> LIVE verify (testpilot conventions: no "Result-based",
      mentions exceptions, camelCase, language: TypeScript; AAP conventions still Rust-flavored).
  [ ] SF-001 test + ops guard — tests/find_dependents_pass2.rs real-parser end-to-end test (NO algorithm
      change); optional but HIGH-LEVERAGE daemon/health served-binary-staleness guard (overlaps SF-008;
      product decision warn-vs-restart — record it). This guard is the durable fix for the audit's two misses.
      Gate: code -> code-reviewer ([SF-001-frozen] check) -> test-runner -> (ops guard) LIVE verify a
      reused/older daemon triggers the warning.
  [ ] SF-009 untracked-file surfacing — git ls-files tracked set (NOT via ignore crate); count Tier-1
      not-tracked & not-ignored; untracked_indexed on HealthStats; "indexed untracked files: N" in the
      health line (format.rs); FAIL OPEN when no git index; optional SYMFORGE_EXCLUDE_UNTRACKED (default
      off). Account for the two discovery paths (discover_all_files vs build_reload_data/discover_files).
      Gate: code -> code-reviewer ([SF-009-frozen] check) -> test-runner -> LIVE verify (health line shows
      the untracked count on a repo with an untracked recognized-ext file; off on a non-git tempdir).

PHASE F — INTEGRATE (git-master) — HARD-GATED
  [ ] git-master collects all issue work onto fix/symforge-mcp-audit-2026-06-03 in scoped commits
      (per-issue/per-phase; cleanup never mixed with feature). Resolve any conflicts. Do NOT touch the
      gsd-* working-tree rename. Re-run the FULL suite on the integrated branch (criterion [2]).
  [ ] STOP. No push, no PR, no merge. Leave the branch for human review.

PHASE G — VERIFY END-TO-END + REPORT
  [ ] On a daemon freshly built+restarted from the integrated review branch (kill-stale-daemon first,
      confirm version+PID), re-run the live verify-as-user matrix for SF-002/003/004/005/006/007/008/010/011
      against the real repos and capture before/after evidence (criterion [3]).
  [ ] Finalize the honest results doc (criterion [5]): per-issue status, evidence, live before/after,
      open maintainer decisions, remaining gaps. Mock-as-mock, unverified-as-unverified.
  [ ] cargo clean (campaign boundary — reclaim disk).
  [ ] Self-delete this goal file. Report: review branch name, results-doc path, the human-review gate,
      and any open decision the maintainer must make.
