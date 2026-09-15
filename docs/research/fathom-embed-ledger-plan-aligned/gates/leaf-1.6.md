# Gates: leaf 1.6, plan Task 6, honest and complete code and embedded knowledge retrieval (F-1, F-2, selector, path prefix)

OWNS: src/protocol/format.rs, src/protocol/format/tests.rs, src/protocol/search_tools.rs, src/protocol/knowledge_search.rs, src/knowledge/**, src/live_index/**, src/daemon.rs, src/index_lifecycle/embedded.rs, src/index_lifecycle/public_api.rs, src/embed.rs, src/lifecycle_identity.rs, specs/020-repository-knowledge-index/contracts/public-api-v11.json, docs/reviews/FEATURE-020-EXPORT-DELTA-v11.json, tests/public_api_delta_v11.rs, tests/embed_bound_index.rs, CHANGELOG.md, docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-*.patch

Scope: a code-search zero hit reports the exact count of in-scope knowledge-only files it did not scan and points to `search_knowledge`; a partial result reports in-scope admission-withheld files and their reason classes in the neutral wording; `EmbeddedSourceHandle::search_knowledge` returns admitted Markdown, text and config knowledge with provenance, relationship evidence, authority and coverage, and withholding evidence, ranked exactly as the server tool ranks it, through a typed seam; an ambiguous project name keeps refusing with candidate ids; an embedded `path_prefix` of `src/` no longer matches `srcx/`; every public addition is a recorded contract atom

Plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Task 6 as of its 11:02
revision on 2026-09-15. Grounded by the ledger author at v11.2.0 (verify on the
branch, G1):
- `search_text_options_from_input` sets `search::SearchScope::Code`
  (src/protocol/search_tools.rs:635); `SearchScope::allows_targets` keeps only
  `targets.includes_code()` (src/live_index/search.rs:92-105); `IndexTargets::for_path`
  classes Markdown as `Knowledge` only (src/domain/index.rs:824-837). Single-project
  zero-hit text: src/protocol/format.rs:1233-1262. Cross-project zero-hit text:
  `format_cross_project_text` answers only "No text matches in the targeted
  project(s)." (src/daemon.rs:5878-5886).
- Withholding: `MetadataOnlyReason::SensitivePath | SensitiveContent` project to the
  single neutral `SkipReason::PolicyWithheld` (src/live_index/store.rs:4245-4246);
  its doc comment says path rule versus content rule is deliberately not
  disclosed. Size reasons are metadata facts: `METADATA_ONLY_BYTES` 1 MiB,
  `METADATA_ONLY_CODE_BYTES` 4 MiB (src/domain/index.rs:1602, 1609).
- Server knowledge retrieval: `search_scoped` (src/protocol/knowledge_search.rs:429)
  builds `KnowledgeHit` values (:178) carrying `heading_path`, `content_hash`,
  finding and provenance ids and `bridge_previews` (relationship evidence), and
  counts `withheld_sensitive`. The module is server-only (`protocol`); the plan's
  typed seam must compile without the server feature.
- Selector: `runtime_for_target` (src/daemon.rs:2013) refuses an ambiguous display
  name naming candidate ids, pinned by `test_runtime_for_target_resolution_contract`
  (src/daemon.rs:8077). Already true at v11.2.0, so a regression guard.
- Path prefix: the embedded handle trims the trailing `/` and uses a plain
  `starts_with` in `search_symbols` (src/index_lifecycle/embedded.rs:772-778) and
  `search_text` (:882-889); the server's `PathScope::matches`
  (src/live_index/search.rs:54-66) already requires a `/` boundary.

Tests go in the nearest existing files: protocol formatter and daemon tests (lib,
default server cell) and `tests/embed_bound_index.rs` (embed integration cell).
Running the embedded knowledge tests with `--no-default-features --features embed`
is itself the proof that embed callers do not receive stringified server tool
output: no server module is compiled in that cell. The plan does not name the
knowledge request and result types; this ledger assumes the facade's existing
pattern (`KnowledgeSearchRequest`, `KnowledgeSearchResult`, `KnowledgeMatch`).
Owner revision 2 keeps those names and pins C29: the result is not a thin
`TextSearchResult` clone. `search_knowledge` claims and refusals use
`OperationKind::SearchKnowledge`. Test names are this ledger's, except the
existing selector test.

- [ ] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.6.md
  EXPECT: LINT OK
  EVIDENCE: pending

- [ ] G1: the search scope, withholding, knowledge retrieval, selector and path-prefix facts above are re-read on the branch, with their status recorded
  RECORD: table of the branch commit, each cited symbol and file:line, holds / moved-to / changed, and the owner's confirmation of the knowledge type names
  EVIDENCE: pending

- [ ] G2: a zero-hit search_text answer, single-project and cross-project, states the exact count of in-scope knowledge-only files not scanned and names search_knowledge
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 2 --name search_text_zero_hit_reports_exact_excluded_knowledge_count --name cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count -- test -j 8 --lib -- search_text_zero_hit_reports_exact_excluded_knowledge_count cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=2; names=search_text_zero_hit_reports_exact_excluded_knowledge_count,cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count
  TEST: the fixture holds knowledge-only files both inside and outside the query's scope, so only the in-scope count is correct.
  RED: failing-first; at v11.2.0 neither answer mentions excluded files.
  EVIDENCE: pending

- [ ] G3: dropping the excluded-knowledge note lets both G2 tests fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m1-no-excluded-knowledge-note.patch --name search_text_zero_hit_reports_exact_excluded_knowledge_count --name cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count -- test -j 8 --lib -- search_text_zero_hit_reports_exact_excluded_knowledge_count cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m1-no-excluded-knowledge-note.patch
  EVIDENCE: pending

- [ ] G4: counting knowledge-only files without applying the query's scope lets the single-project G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m2-excluded-count-ignores-scope.patch --name search_text_zero_hit_reports_exact_excluded_knowledge_count -- test -j 8 --lib -- search_text_zero_hit_reports_exact_excluded_knowledge_count --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m2-excluded-count-ignores-scope.patch
  EVIDENCE: pending

- [ ] G5: a zero-hit answer whose scope holds no knowledge-only file carries no excluded-knowledge note
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name search_text_zero_hit_without_excluded_knowledge_adds_no_note -- test -j 8 --lib -- search_text_zero_hit_without_excluded_knowledge_adds_no_note --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=search_text_zero_hit_without_excluded_knowledge_adds_no_note
  RED: negative control for G2; its teeth are G6.
  EVIDENCE: pending

- [ ] G6: a note printed unconditionally lets the G5 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m3-note-always-printed.patch --name search_text_zero_hit_without_excluded_knowledge_adds_no_note -- test -j 8 --lib -- search_text_zero_hit_without_excluded_knowledge_adds_no_note --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m3-note-always-printed.patch
  EVIDENCE: pending

- [ ] G7: a partial search_text result reports how many in-scope files admission withheld and their reason classes
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name search_text_partial_result_reports_policy_withheld_files_and_reasons -- test -j 8 --lib -- search_text_partial_result_reports_policy_withheld_files_and_reasons --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=search_text_partial_result_reports_policy_withheld_files_and_reasons
  TEST: the fixture has at least one policy-withheld file and one over the size threshold in scope; the answer names the policy class with the neutral wording and the size class separately.
  RED: failing-first; at v11.2.0 no answer counts admission-withheld files.
  EVIDENCE: pending

- [ ] G8: removing the withheld report lets the G7 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m4-no-withheld-report.patch --name search_text_partial_result_reports_policy_withheld_files_and_reasons -- test -j 8 --lib -- search_text_partial_result_reports_policy_withheld_files_and_reasons --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m4-no-withheld-report.patch
  EVIDENCE: pending

- [ ] G9: the withheld report reads byte-identically whether a path rule or a content rule withheld the files
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name withheld_report_wording_is_identical_for_path_rule_and_content_rule -- test -j 8 --lib -- withheld_report_wording_is_identical_for_path_rule_and_content_rule --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=withheld_report_wording_is_identical_for_path_rule_and_content_rule
  RED: guard for the documented neutrality of `PolicyWithheld`; its teeth are G10.
  EVIDENCE: pending

- [ ] G10: a report that names the rule class lets the G9 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m5-report-names-rule-class.patch --name withheld_report_wording_is_identical_for_path_rule_and_content_rule -- test -j 8 --lib -- withheld_report_wording_is_identical_for_path_rule_and_content_rule --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m5-report-names-rule-class.patch
  EVIDENCE: pending

- [ ] G11: an embedded search_knowledge query on a Current source returns matching Markdown, plain-text and config knowledge, each hit with its path, heading path, content hash and provenance ids
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embedded_search_knowledge_returns_markdown_text_and_config_with_provenance -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_returns_markdown_text_and_config_with_provenance --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embedded_search_knowledge_returns_markdown_text_and_config_with_provenance
  RED: failing-first against a stub that returns no hits.
  EVIDENCE: pending

- [ ] G12: a seam that returns only Markdown hits lets the G11 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m8-knowledge-markdown-only.patch --name embedded_search_knowledge_returns_markdown_text_and_config_with_provenance -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_returns_markdown_text_and_config_with_provenance --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m8-knowledge-markdown-only.patch
  EVIDENCE: pending

- [ ] G13: hits returned without provenance ids let the G11 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m9-knowledge-hits-without-provenance.patch --name embedded_search_knowledge_returns_markdown_text_and_config_with_provenance -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_returns_markdown_text_and_config_with_provenance --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m9-knowledge-hits-without-provenance.patch
  EVIDENCE: pending

- [ ] G14: an embedded knowledge result carries relationship evidence linking a document to the code it names, plus the source's authority and coverage
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embedded_search_knowledge_reports_relationships_authority_and_coverage -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_relationships_authority_and_coverage --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embedded_search_knowledge_reports_relationships_authority_and_coverage
  RED: failing-first against a stub without relationship data.
  EVIDENCE: pending

- [ ] G15: a result that drops relationship evidence lets the G14 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m10-knowledge-drops-relationships.patch --name embedded_search_knowledge_reports_relationships_authority_and_coverage -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_relationships_authority_and_coverage --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m10-knowledge-drops-relationships.patch
  EVIDENCE: pending

- [ ] G16: an embedded knowledge query whose scope holds withheld files reports how many were withheld, in the neutral wording
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embedded_search_knowledge_reports_withheld_evidence -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_withheld_evidence --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embedded_search_knowledge_reports_withheld_evidence
  RED: failing-first against a stub that reports no withholding.
  EVIDENCE: pending

- [ ] G17: a result that drops withheld evidence lets the G16 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m11-knowledge-drops-withheld-evidence.patch --name embedded_search_knowledge_reports_withheld_evidence -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_withheld_evidence --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m11-knowledge-drops-withheld-evidence.patch
  EVIDENCE: pending

- [ ] G18: for the same fixture and query, the typed seam returns the same hits in the same order as the server search_knowledge tool
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name typed_knowledge_seam_matches_server_search_knowledge_hits -- test -j 8 --lib -- typed_knowledge_seam_matches_server_search_knowledge_hits --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=typed_knowledge_seam_matches_server_search_knowledge_hits
  RED: guard for plan Task 6 Step 2 ("do not alter `search_knowledge` retrieval semantics"); its teeth are G19. Runs in the default server cell because the server tool exists only there.
  EVIDENCE: pending

- [ ] G19: a seam that re-ranks hits lets the G18 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m12-seam-reranks-hits.patch --name typed_knowledge_seam_matches_server_search_knowledge_hits -- test -j 8 --lib -- typed_knowledge_seam_matches_server_search_knowledge_hits --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m12-seam-reranks-hits.patch
  EVIDENCE: pending

- [ ] G20: an ambiguous project display name is refused with the candidate ids, and an unknown one with the open projects
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name test_runtime_for_target_resolution_contract -- test -j 8 --lib -- test_runtime_for_target_resolution_contract --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=test_runtime_for_target_resolution_contract
  RED: regression guard; already true at v11.2.0; its teeth are G21.
  EVIDENCE: pending

- [ ] G21: a resolver that takes the first matching display name lets the G20 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m6-ambiguous-name-picks-first-match.patch --name test_runtime_for_target_resolution_contract -- test -j 8 --lib -- test_runtime_for_target_resolution_contract --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m6-ambiguous-name-picks-first-match.patch
  EVIDENCE: pending

- [ ] G22: an embedded search_symbols and search_text with path_prefix src/ return matches under src/ and none under srcx/
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embed_path_prefix_src_excludes_srcx -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embed_path_prefix_src_excludes_srcx --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embed_path_prefix_src_excludes_srcx
  RED: failing-first; at v11.2.0 the handle's plain `starts_with` matches `srcx/`.
  EVIDENCE: pending

- [ ] G23: restoring the plain prefix match lets the G22 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m7-plain-prefix-match.patch --name embed_path_prefix_src_excludes_srcx -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embed_path_prefix_src_excludes_srcx --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m7-plain-prefix-match.patch
  EVIDENCE: pending

- [ ] G24: the excluded count and the embedded knowledge retrieval read the published source set without indexing Markdown bodies a second time
  RECORD: the code paths (symbol and file:line) showing the count reads manifest or coverage data only and the seam reads the existing published knowledge units, with no new body read or second index; reviewed by the owner (plan Task 6 Step 2)
  EVIDENCE: pending

- [ ] G25: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: pending

- [ ] G26: the contract keeps every v11.2.0 atom and records the knowledge retrieval atoms, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::KnowledgeSearchRequest --require-atom symforge::embed::KnowledgeSearchResult --require-atom symforge::embed::EmbeddedSourceHandle::search_knowledge
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: pending

- [ ] G27: the regenerated export delta matches the amended contract atoms and wrap table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name export_delta_matches_frozen_contract_atoms -- test -j 8 --test public_api_delta_v11 -- export_delta_matches_frozen_contract_atoms --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=export_delta_matches_frozen_contract_atoms
  EVIDENCE: pending

- [ ] G28: the embed facade contract tripwire names the knowledge items and passes in the embed lib cell
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name facade_contract_is_stable -- test -j 8 --no-default-features --features embed --lib -- facade_contract_is_stable --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=facade_contract_is_stable
  EVIDENCE: pending

- [ ] G29: the hand-maintained CHANGELOG embed section names the knowledge retrieval method and types
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs doc-has --file CHANGELOG.md --section "## Embedder API" --token search_knowledge --token KnowledgeSearchRequest --token KnowledgeSearchResult
  EXPECT: LEDGER-ORACLE DOC GREEN file=CHANGELOG.md;
  EVIDENCE: pending

- [ ] G30: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G2, G7, G11, G14, G16 and G22 tests, the working or stub state, exact command and each failure line; G5, G9, G18 and G20 are declared guards
  EVIDENCE: pending

- [ ] G31: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [ ] G32: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: pending
