# SymForge MCP Bug-Fix Implementation Plan

Date: 2026-06-03
Source audit: `docs/SYMFORGE_MCP_BUG_REPORT_2026-06-03.md` (Codex cross-repo audit, SF-001..SF-011)
Verification: 11 root-cause investigators + 11 adversarial verifiers + 7 live reproduction agents
(against the real `Agent_Army_Professionals`, `testpilot`, and `symforge` repos on disk).

This plan is the actionable hand-off for the coding agent. Every claim below was either
reproduced against the real repos, proved against the live source, or both. Where the original
report was WRONG, this plan says so and gives the corrected work. Read this, not the raw report,
when implementing.

---

## TL;DR — what actually has to be built

| ID | Original sev | VERIFIED verdict | Build? | One-line action |
|---|---|---|---|---|
| SF-001 | Critical | **ALREADY FIXED** (not reproducible >=7.10.0) | Tests + ops guard only | Add real-parser regression test; daemon binary-staleness warning. Do NOT touch the matching algorithm. |
| SF-002 | High | **CONFIRMED defect** | YES | Kill self-caller/callee via existing `enclosing_symbol_index`; surface `unresolved_same_name_member_call`. |
| SF-003 | High | **CONFIRMED defect** | YES | Classify import-type+`[]` parse error as parser limitation (Status: ok), not repo syntax error. Sound detector required. |
| SF-004 | Medium | **CONFIRMED limitation** | YES | Add `expected_framework_partial` bucket for Angular `@if/@for` in `.html`. |
| SF-005 | Medium | **CONFIRMED defect** | YES | `ask` "where is X defined and ..." must extract leading symbol token, not pass whole sentence. |
| SF-006 | Medium | **CONFIRMED defect** | YES | Stem-equals-basename co-change gate fix + precise fallback reason (incl. chore-anchor). |
| SF-007 | Medium | **CONFIRMED defect** | YES | Forward `checkpoint_now` to the daemon; add daemon dispatch arm. |
| SF-008 | Medium | **CONFIRMED defect** | YES | Windows-native remediation in `format_shadow_warning` ForeignPrefix arm. |
| SF-009 | Medium | **MECHANISM REFUTED** | Surfacing only | Scratch dotfiles already filtered; `.txt` adds 0 symbols. Add "indexed untracked files: N"; do NOT change admission. |
| SF-010 | Low | **PARTIAL** (lazy-exposure = harness) | YES (half) | `ask` ToolHelp intent + `symforge://tools/catalog`. Document lazy-exposure as harness behavior. |
| SF-011 | Low | **CONFIRMED defect** | YES | Language-aware `detect_conventions` (TS/NestJS/Angular branch). |

Net: **8 real code fixes (SF-002..SF-008, SF-010, SF-011), 1 already-fixed (SF-001: tests+ops), 1 refuted-mechanism (SF-009: surfacing only).**

---

## Corrections to the original report (read before trusting it)

These are the places the Codex report is factually wrong or misleading. The coding agent MUST
follow the corrected version or it will write the wrong fix / a vacuous test.

1. **SF-001 is not a live defect.** The collision filter that rejects the exact `new`/`get`/`state`
   bare-name collision shipped in commit `251d7f0` ("tighten Pass 2 collision filter for H.4"),
   which is an ancestor of the `v7.18.0` tag (VERIFIED: `git merge-base --is-ancestor 251d7f0 v7.18.0`
   returns true). Current source structurally cannot emit a dependent edge for a file with ZERO
   textual reference to the target (it requires either a `::`-boundary qualified-name suffix match
   OR a same-name import). The audit observation is a **stale daemon binary** artifact: it ran in
   `daemon_reused_session` mode with a PATH-shadowing `C:\Program Files\nodejs\symforge` of
   "version unknown" (the same shadow SF-008 flags). Do NOT change the matching algorithm — doing so
   would regress legitimate cross-module dependents pinned by `real_qualified_call_dependent_still_reported`.

2. **SF-002 has a cleaner fix than the report implies.** The index already stores
   `enclosing_symbol_index` on every reference (VERIFIED present in `domain/index.rs`, `store.rs`,
   `context_bundle.rs`, + 13 more files). The self-caller symptom is killable by checking
   `reference.enclosing_symbol_index != Some(target_index)` for same-file refs — precise, language-agnostic,
   and it does NOT suppress legitimate intra-class `this.foo()` callers. The report's implied
   byte-before-`.` heuristic is strictly more invasive (it would turn a false-positive into a
   false-negative for the canonical NestJS delegation pattern). Use `enclosing_symbol_index`.

3. **SF-003 trigger is narrower than reported, and the obvious detector is unsound.** Only the `[]`
   array suffix on an import-type breaks (`import('rxjs').Subscription` scalar parses CLEAN; only
   `import('rxjs').Subscription[]` errors). A grammar bump is NOT available — `tree-sitter-typescript 0.23.2`
   is the latest published crate. CRITICAL: a detector that gates on "error-node text starts with `import(`
   and next char is `[`" is UNSOUND — a genuinely broken file `import('rxjs').Subscription[] = [ ; foo bar`
   produces the same error-node prefix and would be wrongly marked OK. The detector must validate the
   WHOLE construct (no trailing ERROR/MISSING after the array suffix), e.g. re-parse the isolated type
   annotation in a synthetic well-formed wrapper, or confirm no further error node follows.

4. **SF-006 is NOT a path-normalization bug** (the reproduction's leading hypothesis was wrong; the
   investigation proved the map lookup succeeds). The real cause: the anchor-confidence gate. Query
   `work_item` (stem, no extension) scores only PREFIX tier (50.0) against `work_item.rs` (basename
   incl. extension), below the `CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR = 100.0`, so fusion never applies.
   Fusion DOES work for the with-extension shape today (`query="work_item.rs"`). Also: there are
   **four** distinct fallback reasons to disambiguate (anchor-confidence, below-shared-commit-floor,
   no-neighbor-in-candidate-set, AND chore-anchor) — the report/investigation listed three.

5. **SF-009's stated mechanism is FALSE.** The named scratch files are all dotfiles; the `ignore`
   crate's default `hidden:true` filters them before admission ever runs. Even a non-dotfile `.txt`
   maps to `LanguageId::None` -> Tier-2 metadata-only -> **0 symbols, 0 Tier-1 count**. The reported
   +1450 symbols cannot come from text scratch files (arithmetically impossible); it was the
   concurrent agent's real source edits. The report's own proposed regression test (assert
   `.probe.txt` not indexed) would **pass today with zero code change** — a vacuous test. Only fix:
   surface "indexed untracked files: N"; do NOT change admission defaults (breaks non-git/tempdir workflows).

6. **SF-010's "lazy exposure" is harness behavior, not a SymForge defect.** SymForge eagerly returns
   all 32 tools in one `tools/list` (rmcp `#[tool_handler]` macro). The two-pass discovery is the
   client ToolSearch/deferred-loading layer. SymForge can only fix the `ask()` tool-meta-question half
   and add a tool catalog. Document the other half as out of scope.

---

## Verification environment (ground truth)

- `tree-sitter-typescript = 0.23.2` (LATEST published; no bump remedy) — `Cargo.toml:72`, `Cargo.lock` checksum `6c5f76ed...`
- `tree-sitter-html = 0.23.2` (vanilla upstream, zero Angular rules) — `Cargo.toml:85`
- `tree-sitter = 0.26.8` core
- `.ts`/`.tsx` both -> `LanguageId::TypeScript` -> `LANGUAGE_TYPESCRIPT` (no separate TSX grammar; `.tsx` is covered by the same fix)
- Real repos on disk: `C:\AI_STUFF\PROGRAMMING\Agent_Army_Professionals`, `...\testpilot` (the audit originals)
- SF-001 fix commit `251d7f0` confirmed ancestor of `v7.18.0`.

---

## Repair order (gated phases, <=5 files/phase per CLAUDE.md)

Ship in this order. Each issue is independently landable; group by risk and file overlap.
Two issues touch `smart_query.rs` (SF-005, SF-010) — serialize them. Three touch the
parse-classification path (SF-003, SF-004) and `health_view.rs`/`format.rs` — serialize them.

**Phase A — High value, low risk, isolated:**
- SF-007 (`checkpoint_now` forwarding) — `tools.rs` + `daemon.rs`, fully isolated.
- SF-008 (Windows PATH remediation) — `path_shadow.rs` only.

**Phase B — Correctness defects in resolution/ranking:**
- SF-002 (TS same-name method) — `disambiguation.rs`, `context_bundle.rs`, `format.rs` (+ new output section).
- SF-006 (co-change gate + reasons) — `rank_signals.rs`, `tools.rs`, calibration tests.

**Phase C — Parse classification (serialize C1 then C2; shared files):**
- SF-003 (import-type+array) — `parsing/mod.rs`, `domain/index.rs` or `store.rs`, `format.rs`, `tools.rs`, `health_view.rs`.
- SF-004 (Angular template) — `health_view.rs`, `store.rs`, `format.rs` (+ `sidecar/handlers.rs` for get_file_context scope).

**Phase D — Routing / UX (serialize D1 then D2; both touch smart_query.rs):**
- SF-005 (`ask` compound query) — `smart_query.rs`.
- SF-010 (`ask` ToolHelp + catalog) — `smart_query.rs`, `tools.rs`, `resources.rs`.

**Phase E — Advisory quality + ops/tests:**
- SF-011 (language-aware conventions) — `conventions.rs`.
- SF-001 (real-parser regression test + daemon staleness guard) — `tests/find_dependents_pass2.rs`, `daemon.rs`/health.
- SF-009 (untracked-file surfacing) — `health_view.rs`, `format.rs`, `store.rs`, `discovery/mod.rs`.

Per-issue gates (every issue): `cargo fmt --check` -> `cargo check` ->
`cargo clippy --all-targets -- -D warnings` -> `cargo test --all-targets -- --test-threads=1`
-> for the live-verifiable ones, re-run the MCP tool against the real repo and confirm the
corrected output. NO push/merge without human approval (commit to a review branch and stop).

---

## Per-issue implementation specs

### SF-002 — TypeScript same-name method conflation (HIGH, CONFIRMED)

**Symptom:** `get_symbol_context` lists `TestingController.startExploration` as its own caller AND
callee because the body calls `this.testingService.startExploration(...)`. Ground truth verified:
3 distinct `startExploration` defs in testpilot (controller:44, service:415, frontend ApiService:378).

**Root cause:** `matches_exact_symbol_reference` (`src/live_index/disambiguation.rs:51-95`) accepts a
ref as a caller when `reference.name == target_name` (81-83). The only receiver guard
(`is_rust_receiver_method_call`, :89-92 -> :97-113) is gated `if target_kind == SymbolKind::Function
&& *target_language == LanguageId::Rust`. TS methods are `SymbolKind::Method`/`LanguageId::TypeScript`,
so the guard is skipped; the bare-name member call matches. The TS xref query (`xref.rs:110`) captures
only the property name, discarding the receiver, so qualified_name is None. Callee half: `callees_for_symbol`
(`context_bundle.rs:295-329`) has no name-vs-self check.

**Fix (use the verifier's correction — `enclosing_symbol_index`, NOT byte heuristic):**
1. Caller side: in the same-file caller branch of `collect_exact_symbol_references`
   (`query.rs:1558-1573`), reject a Call ref whose `enclosing_symbol_index == Some(target_symbol_index)`
   — that is the symbol calling a same-named member from inside its own body. This kills the
   self-caller precisely, language-agnostically, without suppressing legitimate intra-class
   `this.foo()` callers from OTHER methods.
2. For cross-object same-name calls where the receiver type is unresolved, surface them as a new
   `unresolved_same_name_member_call` section on `ContextBundleFoundView` rather than as exact callers
   (per the report's acceptance criterion). Render under a clearly-labeled line in the
   `get_symbol_context` formatter (`src/protocol/format.rs`).
3. Callee side: in `callees_for_symbol`/`capture_callee_section`, when a callee's bare name equals the
   target's own name AND it is a receiver method call, label it unresolved rather than rendering the
   method as calling itself.

**DO NOT:** broaden the byte-before-`.` guard globally — the verifier proved it reclassifies the
canonical legitimate `this.target()` sibling/recursion caller (receiver IS resolvable there) into a
false-negative. Also note: `find_references` and `edit_plan` are PUBLIC tools consuming this path; any
change to caller counts is a user-visible contract change across TS/JS/Python/C#/Go — gate the new
output-section change deliberately and update golden/snapshot tests.

**Regression test:** `src/live_index/query.rs` tests, modeled on
`test_capture_context_bundle_view_uses_symbol_line_and_exact_callers` (query.rs:3551). TS fixture:
`class TestingService { startExploration() {} } class TestingController { startExploration() { return this.testingService.startExploration(); } }`.
Assert (1) controller method `callers.total_count == 0`; (2) controller method is not its own callee
(appears in the new unresolved section). Add a C#/Java parallel and a guard test that a true bare
top-level `foo()` (no preceding `.`) IS still counted as a caller.
**Verifier caveat on the test:** the `make_ref` helper (`query.rs:2593-2607`) hardcodes
`line_range = (byte_start/100, byte_start/100)`; the naive `content.find("startExploration();")` offset
yields line 0, which won't land inside the controller method's `line_range` (so `callees_for_symbol`'s
line-containment filter won't fire). Use a ref builder that decouples byte_range from line_range, or
position content so `byte_start/100` maps to the controller method's line. The report's recipe is broken as written.

**Open decisions:** output shape (new field vs sibling Vec on `ContextBundleFoundView`); whether to
also guard Rust `SymbolKind::Method` (`self.foo()`); whether `edit_plan` subtracts unresolved hits or
reports them separately. Ship the surgical guard+labeling; DEFER the `xref` receiver-capture
(qualified_name) enhancement (index-format/blast risk).

---

### SF-003 — TS import-type `[]` reported as syntax error (HIGH, CONFIRMED)

**Symptom:** `private subs: import('rxjs').Subscription[] = [];` (valid TS, testpilot
workflow-builder.component.ts:462) -> `Status: partial` in `validate_file_syntax`/`get_file_context`/`health`.

**Root cause (two parts):**
- Grammar: `tree-sitter-typescript 0.23.2` mis-parses an import-type immediately followed by `[]`.
  Empirically isolated (dogfooded against the real pinned grammar): scalar `import('rxjs').Subscription`
  parses CLEAN in every position; only the `[]`/tuple suffix breaks (`type T = import('x').y[]`,
  class field, fn return, multi-dim `[][]`, generic arg all error). 0.23.2 is the latest crate -> no bump.
- Classification: `parsing/mod.rs:273` `let has_error = root.has_error();` -> `mod.rs:95-102` binary
  `if has_error { PartialParse } else { Processed }`. No "known grammar limitation" concept. Flows to
  `ParseStatus::PartialParse` -> `format.rs:2127` "Status: partial", `tools.rs:746-752` parse_state
  "partial", `health_view.rs:174-182` counted as UNEXPECTED partial. The only existing
  expected-vs-unexpected discriminator (`is_expected_vendor_partial_parse_noise`, `health_view.rs:55-71`)
  is path-based vendor-only.

**Fix — diagnostic-content-based known-limitation classifier (grammar bump impossible). Recommended shape (B):**
1. In `parse_source` (after `collect_deepest_error_node`), add a SOUND TypeScript-gated detector for
   the import-type+array pattern. **Must validate the whole construct** (per verifier): confirm the
   error region is exactly an `import(...).Member` followed by `[]`/tuple AND there is no further
   ERROR/MISSING node after the array suffix (e.g. re-parse the isolated annotation in a synthetic
   well-formed wrapper and confirm clean). Do NOT gate merely on "error-node text starts with `import(`
   + next char `[`" — that false-classifies genuinely broken files.
2. Add `FileOutcome::ExpectedGrammarLimitation { note }` (`domain/index.rs`) -> `ParseStatus::ParsedWithGrammarLimitation { note }` (`store.rs`).
3. Render: `validate_file_syntax_result` (`format.rs:2117-2153`) -> `Status: ok` (optionally
   `+ note: parser limitation`); `parse_state_for_file` (`tools.rs:746-752`) -> "parsed";
   `health_view.rs` counts under expected, never unexpected.

**CRITICAL serialization hazard (verifier):** `ParseStatus` derives `Serialize/Deserialize` and is
persisted in the checkpoint snapshot (`persist.rs:54`, 14 occurrences). A new variant changes the
on-disk format -> needs `#[serde(default)]`/untagged strategy or a snapshot written by a new binary
fails to load on an older one. Confirm checkpoint back-compat before shipping shape (B). Shape (A)
(extend the expected-partial predicate + thread `IndexedFile` into the two renderers —
`validate_file_syntax_result` 2 call sites tools.rs:5800/5842, `parse_state_for_file` 1 caller
tools.rs:3369) avoids the persisted-format hazard at the cost of renderer signature churn.

**Note:** `.tsx` is already covered (same grammar) — no separate TSX handling needed (verifier corrected
the investigation's open question; there is no `LANGUAGE_TSX` in this codebase).

**Regression test:** `src/parsing/mod.rs` tests. Pin the user-visible outcome, not grammar internals:
- Positive: class-field `import('rxjs').Subscription[] = []` -> outcome is `Processed | ExpectedGrammarLimitation`; symbols still extracted (`C` present).
- Positive: type-alias `type S = import('rxjs').Subscription[];` (the error-node-starts-with-`[` variant the naive detector misses).
- **Negative control (REQUIRED):** genuinely broken `import('rxjs').Subscription[] = [ ; foo bar baz` MUST stay partial/failed — proves the detector didn't over-broaden and mask real errors.
- Negative control: `class C { private x: = ; }` stays partial.
- Format layer: `validate_file_syntax_result` for the valid fixture contains "Status: ok" not "Status: partial".
- Health: the file does not appear in `unexpected_partial_parse_files`.

---

### SF-004 — Angular template control-flow reported as partial HTML parse (MEDIUM, CONFIRMED limitation)

**Symptom:** `@if (items.length > 0) {` / `@for (...)` in `.html` (testpilot app.html) -> `Parse status: partial`,
`syntax error near '0) {' (line 14, col 38)`. Byte-exact reproduced against the real grammar.

**Root cause:** `tree-sitter-html 0.23.2` has zero Angular rules; the `>` in the Angular expression is
lexed as a tag close, leaving `0) {` unparseable -> ERROR node. `@if (cond) {` (no `>`) parses clean;
the `>` relational operator is the trigger. SymForge's own source already comments it text-scans Angular
(`html.rs:50-54`, `:148-151`). Same classification gap as SF-003: only the vendor-SCSS expected bucket exists.

**Fix — add a third partial bucket `expected_framework_partial`:**
1. `health_view.rs`: add `is_expected_framework_partial_parse(path, file)` parallel to the vendor one
   (:55-71). Signal: `LanguageId::Html` + PartialParse + at least one extracted symbol named
   `@if/@for/@switch/@defer/@let` (these only come from `scan_angular_text`, so precise). Add a third
   branch in `health_stats` (:171-189) and the `expected_framework_partial_*` fields on `HealthStats`.
2. `store.rs`: add `expected_framework_partial_*` to `PublishedIndexState` (near :543) + stats->published
   mapping (~:1161) so daemon-proxy mode carries the bucket. **Verifier-confirmed:** `PublishedIndexState`
   derives only Clone/Debug/PartialEq/Eq — NO serde — so NO `#[serde(default)]` needed (it's an
   in-memory cross-process capture, not a disk snapshot). The investigation's serde worry was moot.
3. `format.rs`: add `ParseQuarantineKind::ExpectedFrameworkPartial` (label `expected_framework_partial`,
   reason ~"expected framework: Angular template control-flow not supported by tree-sitter-html; symbols
   extracted best-effort"); wire `from_stats`/`from_published`/`full_section`/`compact_line`.
4. **Required scope (verifier upgraded this from optional):** `get_file_context` is the report's actual
   repro tool but `parse_state_label` (`sidecar/handlers.rs:132-138`) hard-maps PartialParse -> "partial"
   with no framework awareness. Surface the framework classification in the file-context envelope too,
   or the exact line the report quotes stays unchanged.

**Heuristic caveat (verifier):** a genuinely malformed `.html` that also contains a valid `@if` would be
re-bucketed as framework-partial, masking a real defect. Don't over-promise in the reason string; ideally
correlate the stored `parse_diagnostic` line/span (`store.rs:245`) with an Angular control-flow line.

**Regression test:** (1) parser-level: `@if (items.length > 0) {...}` yields `has_error=true` AND an
`@if` symbol is extracted (no existing test asserts has_error for the `>` case — write fresh). (2)
classification-level in `tests/health_parse_quarantine.rs`: app.html lands under
`[expected_framework_partial]`, registry line includes `expected_framework_partial=1`, file NOT under
`[unexpected_partial]`. Existing `tests/health_parse_quarantine.rs` hard-codes the registry summary
string — update those assertions in the same change.

---

### SF-005 — `ask` fails compound lookup queries (MEDIUM, CONFIRMED defect)

**Symptom:** `ask("Where is TestingController defined and what module imports it?")` routes the WHOLE
sentence to `search_symbols(query="TestingController defined and what module imports it?")` -> no match,
reported as `RouteConfidence::Exact` with no suggested next step (confident false negative).

**Root cause:** the "where is " FindSymbol branch (`smart_query.rs:135-167`) captures the entire remainder
after the prefix and only strips two trailing SUFFIXES (`.trim_end_matches(" defined").trim_end_matches(" declaration")`,
:156-158). "defined" is mid-sentence here, so nothing is stripped; the full sentence becomes the symbol name.

**Fix:**
1. **Reorder prefixes (verifier's structural correction):** check `where is file ` (FindFile, :175)
   BEFORE bare `where is ` (FindSymbol, :138). Currently `where is file tools.rs` wrongly routes to
   FindSymbol with name "file tools.rs"; the new first-token logic would make that strictly worse
   ("file"). Reordering removes the special case entirely.
2. After prefix+kind stripping, extract only the FIRST whitespace-delimited token as the symbol
   candidate (a symbol is one token). **Drop the hardcoded stop-word list** (verifier: fragile, buys
   nothing over first-token extraction). Run the single token through existing `clean_symbol_name`
   casing recovery (`smart_query.rs:551-558`) so `testingcontroller` -> `TestingController`. Verify
   `optimize_deterministic` (snake_case, one token) survives intact.
3. When truncation happened, set the matched/confidence signal so `assess_route` returns `Inferred`
   (not `Exact`) WITH a `suggested_next_step` naming the chained sequence
   (`search_symbols -> find_references`), per the acceptance criterion.

**DO NOT** build a full compound-query decomposition engine (gold-plating for MEDIUM). Leading-token +
chained-hint is sufficient.

**Regression test:** `smart_query.rs` tests:
- `classify_intent("Where is TestingController defined and what module imports it?")` -> `FindSymbol { name: "TestingController", .. }` AND `classify_intent_with_match` second element is false (truncated) — assert BOTH (verifier: name-only assertion leaves the confidence-downgrade untested).
- `classify_intent("where is optimize_deterministic defined")` still -> "optimize_deterministic".
- Add `where is the User class defined` fixture (interior article — currently also wrong; cover it).
- Guard: `where is file tools.rs` routes to FindFile after reorder.
- E2E in `tools.rs` ask tests (next to `test_ask_reports_inferred_route_confidence` :19773): index `class TestingController {}`, assert output has "Chosen tool: search_symbols", invocation contains "TestingController" not "imports it", and "Suggested next step:" present. Confirm `route_invocation` (smart_query.rs:448-490) echoes the name before relying on that assertion.

**Note:** the "how does X work" branch (:217-220) shares the same trailing-only-trim weakness — track as
a separate follow-up using the same helper.

---

### SF-006 — Co-change ranking fallback even with partner in scope (MEDIUM, CONFIRMED defect)

**Symptom:** `work_item_store.rs` is a real co-change partner (VERIFIED: 3/3 shared commits with
`work_item.rs`) and is returned by `search_files`, yet `rank_by="path+cochange"` reports
"co-change ranking fallback used ... none matched returned candidates or passed rank gates".

**Root cause (NOT path-normalization — repro hypothesis refuted by investigation):** the map lookup
`neighbors.get(path)` (`query.rs:677`) SUCCEEDS. The rejection is the anchor-confidence gate
(`rank_signals.rs:203`): `if PathMatchSignal.score(anchor, ctx) < CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR (=100.0) { return 0.0 }`.
Query `work_item` (stem) vs `work_item.rs` (basename incl. ext) gives `has_basename_match = false`
(`"work_item.rs" != "work_item"`, :159) -> prefix branch -> `PREFIX_SCORE = 50.0 < 100.0` -> gate
returns 0.0 for every candidate -> no CoChange tier -> `applied_hits == 0` -> misleading fallback
(`tools.rs:4598-4608`). The message is wrong twice: the partner DID match a candidate, and rejection
was the anchor gate (anchor+query property), not "none matched".

**Fix (two coordinated changes):**
- (A) In `PathMatchSignal::score` (`rank_signals.rs:159`) add stem-equality:
  `has_basename_match = !basename_token.is_empty() && (file_basename == basename_token || file_stem == basename_token)`.
  This promotes a stem-only query that names the anchor to BASENAME tier (100.0), clearing the floor.
  Genuine prefixes (`work` vs `work_item`) must STAY prefix-tier (keep failing) — the existing
  `weak_prefix_anchor_keeps_baseline_path_order_for_path_cochange` test uses `rou` vs `routes` and must stay green.
  **Verifier tension to RESOLVE:** the existing prefix path guards `basename_token.len() >= 3`
  (rank_signals.rs:171, query.rs:1236); a raw `file_stem == basename_token` has no length guard, so
  a 1-2 char stem (`a` vs `a.ts`, `io` vs `io.rs`) would jump to basename tier in DEFAULT ranking. But
  the report's own acceptance fixture uses 1-char `a` vs `a.test.ts`. Maintainer must resolve: honor
  the >=3 policy (and amend the report's example) OR allow short stem-equality. Surface this explicitly.
- (B) Surface the EXACT rejection reason (the acceptance criterion). Split the catch-all fallback into
  FOUR distinct reasons (verifier added chore-anchor): (1) anchor reached only prefix-tier
  (`score s < floor`; suggest `query="<anchor basename>"`); (2) partners present but below shared-commit
  floor; (3) no neighbor key in candidate set (the genuine path-mismatch case); (4) chore-anchor excluded
  (`is_chore_anchor_path`, `rank_signals.rs:200`, fires BEFORE the confidence gate). Compute the
  anchor-confidence reason ONCE at anchor level (not per-candidate). Thread into
  `search_files_ranking_explanation` (`tools.rs:1313-1337`) for `debug_ranking=true`.

**Regression test:** `tests/search_files_path_cochange_calibration.rs`:
- `stem_query_anchor_applies_cochange_for_basename_tier`: files work_item.rs / tests/work_item_store.rs /
  stores/mod.rs + ready coupling row (shared_commits >= 2). `search_files(query="work_item",
  rank_by="path+cochange", anchor_path=".../work_item.rs", debug_ranking=true)` -> NOT "fallback used";
  contains Applied co-change line; `work_item_store.rs` under co-change tier. **Verify the asserted
  output strings against `format.rs` first** (verifier: the proposed "Co-changed files"/"applied evidence"
  substrings are unverified guesses).
- `partial_token_anchor_still_falls_back`: query `work` (true prefix) -> fallback now contains precise
  reason ("reached only prefix-tier path confidence" + basename hint). Update the existing calibration
  tests' asserted substring (and the chore-anchor test `hardcoded_changelog_chore_anchor...`) to the new reasons.

---

### SF-007 — `checkpoint_now` unavailable in daemon-proxy mode (MEDIUM, CONFIRMED defect)

**Symptom:** `checkpoint_now` hard-fails with "unavailable in daemon-proxy mode" — the default runtime.

**Root cause:** it's the only index-bearing tool not wired into the proxy path. (1) `tools.rs:5136-5138`
returns the hard-coded string the instant `self.daemon_client.is_some()`, BEFORE attempting
`proxy_tool_call` (contrast `index_folder`/`health_compact` which proxy first). (2) `daemon.rs`
`execute_tool_call` (2293-2421) has no `"checkpoint_now"` arm -> `unknown tool`. Forwarding IS correct:
the daemon-side server has `daemon_client: None` + real `repo_root`, so the guard passes and the real
checkpoint runs against the daemon's authoritative live index (the proxy-side index is `LiveIndex::empty()`).

**Fix (option b — forward to daemon):**
1. `tools.rs` `checkpoint_now`: replace the early hard-fail with the proxy-first pattern:
   `if let Some(result) = self.proxy_tool_call("checkpoint_now", &params.0).await { return result; }`
   at the top, then the existing local logic (5139-5168) is the fallback. `CheckpointNowInput` already
   derives `Serialize` (verifier-confirmed, tools.rs:254) — use `&params.0` directly.
2. `daemon.rs` `execute_tool_call`: add an arm before `other =>`:
   `"checkpoint_now" => Ok(server.checkpoint_now(Parameters(decode_params::<CheckpointNowInput>(params)?)).await),`
   (import `CheckpointNowInput`).
3. Drop/neutralize the now-misleading guard string for proxy mode.

**Verifier notes:**
- A second test-only dispatcher `dispatch_tool_for_tests` (`mod.rs:498-542`) has no checkpoint_now arm
  (falls to "unknown tool"). Not a correctness blocker, but add the arm if any parity test routes through it.
- Timeout asymmetry: first proxy attempt is 10s (`mod.rs:325`), retry 30s (:386). A very large index could
  spuriously time out the first attempt -> reconnect -> 30s retry (still hits daemon). Consider a longer
  checkpoint timeout. Local-fallback in proxy mode reloads the index via `ensure_local_index`
  (`mod.rs:437-496`) before returning None, so it does NOT checkpoint an empty index in the common case.

**Regression test:** daemon round-trip in `daemon.rs` test module (mirror
`test_daemon_executes_session_scoped_tool_calls` :3537): open session, POST
`/v1/sessions/{id}/tools/checkpoint_now` body `{"verify_after_write": true}`, assert success + body
contains "Checkpoint complete", NOT "unavailable in daemon-proxy mode", NOT "unknown tool", and
`<project>/.symforge/index.bin` exists (or `load_snapshot(...).is_some()`). Keep existing local-mode
tests (tools.rs:12382, 12403).

---

### SF-008 — PATH shadow remediation not Windows-native (MEDIUM, CONFIRMED defect)

**Symptom:** on Windows PowerShell the shadow warning emits POSIX `add to ~/.profile: export PATH="...:$PATH"`.
VERIFIED on this box: `Get-Command symforge -All` shows `C:\Program Files\nodejs\symforge.*` shadowing the nvm install.

**Root cause:** `format_shadow_warning` (`path_shadow.rs:292-347`) picks remediation purely from
`ShadowReport.kind`, computed only from the binary's location, never the host OS. On native Windows any
`C:\` shadow classifies as `ForeignPrefix` (`is_system_prefix` is POSIX-only :203-207; `running_under_wsl`
is the `#[cfg(not(unix))]` `false` stub :230-232), and the ForeignPrefix arm (:337-344) emits the POSIX
line unconditionally. (`$PATH` is also a non-existent var in PowerShell — doubly broken.)

**Fix:** make ONLY the ForeignPrefix arm host-OS-aware via `cfg!(windows)` (project precedent: `daemon.rs:1625`).
On Windows emit PowerShell-native guidance: verification `Get-Command symforge -All`; explain `{shadow_dir}`
precedes `{our_dir}` on PATH; remediation = reorder user PATH so `{our_dir}` precedes (`[Environment]::SetEnvironmentVariable('Path', ..., 'User')` or System Properties), with a brief note that for nvm-for-windows the active node prefix bin wins. Keep POSIX text on `#[cfg(not(windows))]`. Keep output plain ASCII (existing `is_ascii()` asserts). Do NOT change `classify_shadow`/`ShadowKind`.

**Verifier notes:** key off `cfg!(windows)` (build host) — accept that Git-Bash-on-Windows users get
PowerShell wording; include a one-line cmd/POSIX-on-Windows note to soften. Lead with the simple
always-correct statement (our_dir must precede shadow_dir; verify with `Get-Command -All`); keep
setx/nvm as SECONDARY notes (don't over-engineer). The npm-side `launcher.js`/`resolve-binary.js`
POSIX emitters are gated to non-Windows-native paths — out of scope for this Rust fix.

**Regression test:** `#[cfg(windows)] fn format_warning_foreign_prefix_is_powershell_native_on_windows()`
in `path_shadow.rs` tests. Build a `ForeignPrefix` `ShadowReport` with Windows paths; assert
`!contains("~/.profile")`, `!contains("export PATH")`, `contains("Get-Command symforge -All")`, mentions
both bin dirs, `is_ascii()`. **Verifier:** on a real Windows build `Display` renders backslashes and the
ForeignPrefix arm does NOT normalize `\`->`/`, so assert against backslash form or normalize first. Keep
the Unix test gated `#[cfg(not(windows))]`.

---

### SF-011 — `conventions` Rust-biased for TS projects (LOW, CONFIRMED defect)

**Symptom:** TS/NestJS/Angular project gets "Result-based: 3 files return Result", "0% snake_case (0/1237)",
Rust test-module counts.

**Root cause:** `detect_conventions` (`conventions.rs:18-217`) runs one Rust-flavored pass, never reads
`IndexedFile.language` (`store.rs:238`). `Result<`/`-> Result` substring match (conventions.rs:73) fires
on any TS type named `Result`; naming is always framed `% snake_case`/`% CamelCase`; tests count only Rust
`tests` module + `test_`-prefixed fns.

**Fix:** branch on the project's dominant code language computed inside `detect_conventions`:
1. Tally `IndexedFile.language` over code files; pick `primary_lang`. **Verifier correction:** exclude
   by `LanguageId` (drop Json/Toml/Yaml/Markdown/Env/Html/Css/Scss) or by `!is_config`, NOT by `FileClass`
   — `FileClassification::for_code_path` marks EVERY path `FileClass::Code` (config only via `is_config`),
   so a `FileClass`-based filter lets `.json` fixtures win the vote. Fold JS+TS into one bucket
   (`.js->JavaScript`, `.ts->TypeScript` would otherwise split the vote).
2. Gate existing Rust heuristics behind `primary_lang == Rust`. **Gate the per-file SCAN by `file.language`,
   not just the summary branch** (verifier) — else `anyhow`/`Result` counts stay polluted by non-Rust files
   in a Rust-majority mixed repo.
3. TS/JS branch: error handling = `try/catch`, `throw new`, NestJS `HttpException`, RxJS `catchError`
   (NOT the word "Result" unless Rust); naming = `% camelCase fns, % PascalCase types`; tests = count
   `*.spec.ts`/`*.test.ts` (`is_test` already covers these — `test_file_count` is ALREADY
   language-agnostic, verifier; only inline-module/test-fn counts are Rust-only) + `describe(`/`it(`/`test(`
   scan + framework name. Decorators/DTO/signals via content scan (`@Controller`/`@Injectable`/`@Module`/
   `@Component`/`@IsString`/`signal(`/`inject(`) — `SymbolRecord` does NOT store decorators (`index.rs:308-318`).
4. Add a `language` header line to `ProjectConventions`/`format_conventions`. Keep an unknown/default
   generic branch (no Rust-specific wording).

**No internal snapshot risk:** verifier confirmed zero tests grep "Result-based"/"snake_case"/`detect_conventions`,
so rewording the Rust branch carries no internal regression risk.

**Regression test:** add `#[cfg(test)] mod tests` to `conventions.rs` (none today). (a) Rust-majority
fixture -> still "Result-based"/"% snake_case"/nonzero test modules. (b) TS-majority fixture (NestJS
`@Controller`/`@Injectable`, DTO `@IsString`, `*.spec.ts` with `describe(`/`it(`, `throw new HttpException`,
AND a type named `Result<T>`) -> error_handling does NOT contain "Result-based", mentions exceptions,
naming mentions camelCase, test count nonzero, header names TypeScript. **Verifier:** wire `file.language`
explicitly on the fixture `IndexedFile` (don't leave it defaulted) or the test passes for the wrong reason.

**Open decision:** mixed Rust+TS repo (this very repo has `npm/`) — single primary_lang vs per-language
section. Recommend single most-common + a one-line note when a second language has >25% share.

---

### SF-010 — Tool discoverability (LOW, PARTIAL — fix the SymForge half only)

**Symptom:** important tools missed on first discovery; `ask("what tools can I use for impact analysis?")`
returns a code search, not a tool list.

**Root cause:** (1) "lazy exposure" = HARNESS (rmcp `#[tool_handler]` returns all 32 tools eagerly;
two-pass discovery is the client ToolSearch layer) — NOT fixable server-side, DOCUMENT as out of scope.
(2) SymForge-owned: no tool-catalog/workflow-group concept; `ask` has no meta/tool `QueryIntent`, so the
query falls through to `Explore` -> `explore(...)` code search.

**Fix (additive, server-side):**
1. Static workflow-grouped catalog `tool_catalog_groups()` (orientation/search/symbol-context/
   impact-analysis/dry-run-edits/project-switching/diagnostics) + a full and a topic-filtered renderer.
2. `smart_query.rs`: add `QueryIntent::ToolHelp { topic: Option<String> }`, detect EARLY (before
   Understand/Explore fallbacks). **Verifier corrections:** (a) do NOT add ToolHelp to the
   Understand|Explore upgrade guard at `tools.rs:7315-7318` or a topic word matching an indexed symbol
   could clobber it into UnderstandSymbol; (b) scope detection to `tool(s)` + a RECOMMENDATION verb
   (which/recommend/should I use), not any interrogative, to avoid hijacking code queries in repos with
   a `Tool` type. Add arms to `assess_route`/`route_invocation`/`route_tool_name` (exhaustive match).
3. `tools.rs` `ask`: ToolHelp arm returns the topic-filtered catalog (full when topic None/unknown).
4. (Recommended) `resources.rs`: `symforge://tools/catalog` resource mirroring `repo_health_resource`.

**Regression test:** `smart_query.rs`: `classify_intent("what tools can I use for impact analysis?")` ->
`ToolHelp { topic ~ "impact" }`; `which tool should I use?` -> `ToolHelp { None }`; `how does the Tool
registry work` -> Understand/Explore (NOT ToolHelp). `tools.rs` ask E2E: output contains all of
find_references/find_dependents/get_symbol_context/analyze_file_impact/what_changed/diff_symbols. Drift
guard: every `tool_catalog_groups()` name exists in `SYMFORGE_TOOL_NAMES`.

---

### SF-001 — find_dependents false positives (CRITICAL as reported; ALREADY FIXED in source)

**Verdict: not reproducible against >=7.10.0 / HEAD.** Collision filter shipped in `251d7f0`
(ancestor of `v7.18.0`, VERIFIED). `matches_exact_symbol_qualified_name` (`disambiguation.rs:16-49`)
requires a `::`-boundary suffix match; `can_match_type_dependents` is false for Rust (`query.rs:279`),
so name-only Pass-2 edges need a real import. A zero-textual-reference file appearing as a dependent is
structurally impossible. Independent re-run via `LiveIndex::load` (the real daemon pipeline) produced 0
edges. The audit observation is a stale daemon binary (`daemon_reused_session` + PATH-shadowed
"version unknown" binary, SF-008).

**Actions (NO algorithm change — changing it would regress legit dependents):**
1. Close SF-001 as already-fixed; note the stale-binary cause.
2. **Ops guard** (durable fix for the symptom class, overlaps SF-008): `health`/`health_compact` +
   daemon proxy assert the SERVED binary version matches the installed package; warn loudly when a
   reused daemon's binary is older. (Product decision: warn vs auto-restart.)
3. **Test hardening:** add a real-parser end-to-end case to `tests/find_dependents_pass2.rs` (parse
   AAP-shaped `work_item.rs` + `actor.rs` via `symforge::parsing::process_file`, not hand-built
   `ReferenceRecord`s) asserting `find_dependents_for_file(work_item.rs)` yields ZERO refs whose path
   contains `actor.rs`. This closes the one coverage gap (all 4 existing tests hand-build refs).

**If false positives STILL reproduce on a freshly-restarted >=7.18.0 daemon against a specific AAP file:**
capture the exact offending `ReferenceRecord` (name/qualified_name/kind) via the live index and re-open —
only a real `stores::work_item::new`-style qualified call from an unexpected place would indicate a residual gap.

---

### SF-009 — Untracked scratch files in index (MEDIUM as reported; MECHANISM REFUTED)

**Verdict: the stated mechanism does not reproduce.** Named scratch files are dotfiles -> filtered by
`ignore` crate default `hidden:true` before admission. Non-dotfile `.txt` -> `LanguageId::None` ->
Tier-2 metadata-only -> 0 symbols, 0 Tier-1 count. The reported +1450 symbols cannot come from text
files (arithmetically impossible) — it was the concurrent agent's real source edits. Empirically
confirmed: `discover_all_files` on a tempdir with `.git/`, `src_main.rs`, `.probe.txt`, `.verify.txt`,
`notes.txt`, `scratch_probe.json` returns `[notes.txt, scratch_probe.json, src_main.rs]` (dotfiles
filtered; only non-dotfile recognized-ext `.json` is Tier-1). The report's own proposed regression test
(assert `.probe.txt` not indexed) PASSES TODAY with zero code change — do NOT write it as the acceptance test.

**Actions (surfacing only — do NOT change admission defaults):**
1. Compute a git-tracked path set once per load via `git ls-files` (NOT via the `ignore` crate — it has
   no tracked-files concept, verifier-confirmed). **Must fail-open:** no git / no index -> feature off,
   `untracked_indexed = 0` (else every file in a non-git tempdir counts as untracked).
2. Count Tier-1 files NOT git-tracked AND NOT gitignored; add `untracked_indexed` to `HealthStats`;
   render "indexed untracked files: N" in the health line (`format.rs:1620`).
3. Optional opt-in `SYMFORGE_EXCLUDE_UNTRACKED` (default off) demoting untracked recognized-ext files to Tier-2.

**Two discovery paths (verifier):** `LiveIndex::load` uses `discover_all_files` (unknown-ext -> Tier-2);
the watcher incremental path `build_reload_data` uses `discover_files` which HARD-filters on
`from_extension` BEFORE tiering (`discovery/mod.rs:240`). They produce different Tier-2 populations — any
surfacing fix touching both must account for the divergence.

**Regression test:** tempdir with `.git`, `src/main.rs`, root `notes.txt` (unknown ext), root `.probe.txt`.
Assert: `.probe.txt` NOT in `discover_all_files` (documents the report bug doesn't reproduce);
`notes.txt` is Tier-2 metadata-only, 0 symbols; after the fix, a non-dotfile untracked recognized-ext
file -> `untracked_indexed == 1` and health line contains "indexed untracked files: 1". Define behavior
when no git index exists (feature off).

---

## Cross-cutting notes for the coding agent

- **Verify-as-user:** for SF-002/003/004/005/006/007/008/010/011, after the fix, re-run the relevant
  MCP tool against the REAL repo (testpilot for TS/Angular, AAP for Rust co-change/dependents) on a
  FRESHLY restarted daemon built from the fixed source, and confirm the corrected output. Code compiling
  and tests passing is necessary but not sufficient.
- **Kill stale daemon first.** SF-001 is the cautionary tale: a reused/shadowed daemon serves stale
  behavior. Before any live verification, ensure the daemon is the one built from your branch (this also
  validates the SF-001 ops-guard work).
- **Exhaustive-match safety:** SF-003 (new `FileOutcome`/`ParseStatus` variant) and SF-010 (new
  `QueryIntent` variant) intentionally break every `match` at compile time — that is the safety net; fix
  every arm. SF-003's variant additionally touches the PERSISTED checkpoint format — handle serde back-compat.
- **Public-tool contracts:** SF-002 changes `find_references`/`edit_plan` counts; SF-006 changes
  `search_files` ordering for stem queries. Both are user-visible — update golden/snapshot tests and
  treat as deliberate behavior changes, not silent.
- **Do not touch** the uncommitted `gsd-*` -> `local-agent-*` rename in the working tree (another agent's
  WIP, unrelated to these issues).
- Follow CLAUDE.md: structural edits via SymForge edit tools where possible; `cargo clean` only at
  campaign end; no push/merge without human approval (commit to a review branch and STOP).

## Confidence

Every "CONFIRMED"/"REFUTED"/"ALREADY FIXED" verdict above is backed by BOTH an independent root-cause
investigation AND an adversarial verifier that re-checked the cited lines, PLUS (for SF-001..SF-009) a
live reproduction against the real repos. The two highest-impact corrections (SF-001 already-fixed,
SF-002 enclosing_symbol_index) were additionally re-verified directly:
`git merge-base --is-ancestor 251d7f0 v7.18.0` (true) and `enclosing_symbol_index` present across 16 files.
