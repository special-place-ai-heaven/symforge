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

- [x] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.6.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=e0c264c1f99a83aff2ea33d7ba7cf43b788b9a21e04d218ca3693641dde698dd; output-bytes=444

- [x] G1: the search scope, withholding, knowledge retrieval, selector and path-prefix facts above are re-read on the branch, with their status recorded
  RECORD: table of the branch commit, each cited symbol and file:line, holds / moved-to / changed, and the owner's confirmation of the knowledge type names
  EVIDENCE: 2026-09-16 re-read on `feature/fathom-embed-wave2` d669f895. Owner revision 2 keeps `KnowledgeSearchRequest` / `KnowledgeSearchResult` / `KnowledgeMatch` and `OperationKind::SearchKnowledge`; result is not a `TextSearchResult` clone (C29).
    | leaf / brief cite | status on d669f895 |
    |---|---|
    | `search_text_options_from_input` sets `SearchScope::Code` (`search_tools.rs:635`) | HOLDS: `:635`. |
    | `SearchScope::allows_targets` keeps `targets.includes_code()` (`search.rs:92-105`) | HOLDS: `:94-105`. |
    | `IndexTargets::for_path` classes Markdown as Knowledge (`domain/index.rs:824-837`) | HOLDS: `:824`. |
    | single-project zero-hit (`format.rs:1233-1262`) | CHANGED: empty-files branch is now `:1272-1308`; notes via `append_excluded_knowledge_note` `:1171` and `append_withheld_admission_note` `:1181`. |
    | cross-project "No text matches…" (`daemon.rs:5878-5886`) | CHANGED: still `:5878`; now sums `excluded_knowledge_files` and appends the note (`:5885-5892`). |
    | `SensitivePath \| SensitiveContent` → `PolicyWithheld` (`store.rs:4245-4246`) | MOVED: `store.rs:4629-4630`. |
    | `METADATA_ONLY_BYTES` 1 MiB / `METADATA_ONLY_CODE_BYTES` 4 MiB (`domain/index.rs:1602, 1609`) | HOLDS: `:1602` and `:1609`. |
    | `search_scoped` (`knowledge_search.rs:429`) / `KnowledgeHit` (`:178`) | CHANGED: `search_scoped` is `:261` and formats only. Extraction is `retrieve_knowledge` (`knowledge_retrieve.rs:279`); hits are `KnowledgeRetrieveHit`. |
    | `runtime_for_target` (`daemon.rs:2013`); test `:8077` | HOLDS fn `:2013`; test MOVED to `:8084`. |
    | `PathScope::matches` boundary (`search.rs:54-66`) | HOLDS: `:56-67`. |
    | embed `starts_with` after trim (`embedded.rs:772-778`, `:882-889`) | CHANGED: `path_matches_prefix` `:764-768` uses `PathScope::prefix(...).matches`. |

- [x] G2: a zero-hit search_text answer, single-project and cross-project, states the exact count of in-scope knowledge-only files not scanned and names search_knowledge
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 2 --name search_text_zero_hit_reports_exact_excluded_knowledge_count --name cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count -- test -j 8 --lib -- search_text_zero_hit_reports_exact_excluded_knowledge_count cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=2; names=search_text_zero_hit_reports_exact_excluded_knowledge_count,cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count
  TEST: the fixture holds knowledge-only files both inside and outside the query's scope, so only the in-scope count is correct.
  RED: failing-first; at v11.2.0 neither answer mentions excluded files.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=6cb060c16d904012f70fa8f5693809d8a6f6cfc3319f713a677fbedc4c5c1eec; output-bytes=175

- [x] G3: dropping the excluded-knowledge note lets both G2 tests fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m1-no-excluded-knowledge-note.patch --name search_text_zero_hit_reports_exact_excluded_knowledge_count --name cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count -- test -j 8 --lib -- search_text_zero_hit_reports_exact_excluded_knowledge_count cross_project_search_text_zero_hit_reports_exact_excluded_knowledge_count --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m1-no-excluded-knowledge-note.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=40c09efa054f9b6df9eb83de05a2931aed5e35263d682b8721ed1c256e012fb7; output-bytes=217

- [x] G4: counting knowledge-only files without applying the query's scope lets the single-project G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m2-excluded-count-ignores-scope.patch --name search_text_zero_hit_reports_exact_excluded_knowledge_count -- test -j 8 --lib -- search_text_zero_hit_reports_exact_excluded_knowledge_count --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m2-excluded-count-ignores-scope.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=d3ca940689435079cff930c2bf5313aa66c816016adfc906f52b4ef3e6dda56f; output-bytes=145

- [x] G5: a zero-hit answer whose scope holds no knowledge-only file carries no excluded-knowledge note
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name search_text_zero_hit_without_excluded_knowledge_adds_no_note -- test -j 8 --lib -- search_text_zero_hit_without_excluded_knowledge_adds_no_note --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=search_text_zero_hit_without_excluded_knowledge_adds_no_note
  RED: negative control for G2; its teeth are G6.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=3c5c5b35c4afee10379d4c3defc725a3c422cb6e2e30e29650266d82836cc56d; output-bytes=102

- [x] G6: a note printed unconditionally lets the G5 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m3-note-always-printed.patch --name search_text_zero_hit_without_excluded_knowledge_adds_no_note -- test -j 8 --lib -- search_text_zero_hit_without_excluded_knowledge_adds_no_note --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m3-note-always-printed.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=6c37a2fdb04cf104a359b9d84cae3bf7c99f27551982175b1b59c28581093dff; output-bytes=137

- [x] G7: a partial search_text result reports how many in-scope files admission withheld and their reason classes
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name search_text_partial_result_reports_policy_withheld_files_and_reasons -- test -j 8 --lib -- search_text_partial_result_reports_policy_withheld_files_and_reasons --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=search_text_partial_result_reports_policy_withheld_files_and_reasons
  TEST: the fixture has at least one policy-withheld file and one over the size threshold in scope; the answer names the policy class with the neutral wording and the size class separately.
  RED: failing-first; at v11.2.0 no answer counts admission-withheld files.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=d586be8ea2c04bff1ab88bb5abdce659d0549a5b9579e04f5cc263261cb80096; output-bytes=110

- [x] G8: removing the withheld report lets the G7 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m4-no-withheld-report.patch --name search_text_partial_result_reports_policy_withheld_files_and_reasons -- test -j 8 --lib -- search_text_partial_result_reports_policy_withheld_files_and_reasons --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m4-no-withheld-report.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=b7a259544b6e001b65e17103baf0cc659713d069695d8f900f7ea7e9d93bc35b; output-bytes=144

- [x] G9: the withheld report reads byte-identically whether a path rule or a content rule withheld the files
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name withheld_report_wording_is_identical_for_path_rule_and_content_rule -- test -j 8 --lib -- withheld_report_wording_is_identical_for_path_rule_and_content_rule --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=withheld_report_wording_is_identical_for_path_rule_and_content_rule
  RED: guard for the documented neutrality of `PolicyWithheld`; its teeth are G10.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=76e0c70736b2fa0d26c83d33ce90bf659b64ec25149c7cdd54c2105d1862fbc9; output-bytes=109

- [x] G10: a report that names the rule class lets the G9 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m5-report-names-rule-class.patch --name withheld_report_wording_is_identical_for_path_rule_and_content_rule -- test -j 8 --lib -- withheld_report_wording_is_identical_for_path_rule_and_content_rule --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m5-report-names-rule-class.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=1407e6f6bd3410f0735f2ddbc7363bad2329885174366c6c538690a39fb565e4; output-bytes=148

- [x] G11: an embedded search_knowledge query on a Current source returns matching Markdown, plain-text and config knowledge, each hit with its path, heading path, content hash and provenance ids
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embedded_search_knowledge_returns_markdown_text_and_config_with_provenance -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_returns_markdown_text_and_config_with_provenance --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embedded_search_knowledge_returns_markdown_text_and_config_with_provenance
  RED: failing-first against a stub that returns no hits.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=dddc3c1607600ec69ae88939f08f65d770005847265a82cac67dd10a0242e27d; output-bytes=116

- [x] G12: a seam that returns only Markdown hits lets the G11 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m8-knowledge-markdown-only.patch --name embedded_search_knowledge_returns_markdown_text_and_config_with_provenance -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_returns_markdown_text_and_config_with_provenance --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m8-knowledge-markdown-only.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=903f6d6cb9a4e4b1d3229643f6c39a1f136f6bd23a2a4deb634a18f294650d20; output-bytes=155

- [x] G13: hits returned without provenance ids let the G11 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m9-knowledge-hits-without-provenance.patch --name embedded_search_knowledge_returns_markdown_text_and_config_with_provenance -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_returns_markdown_text_and_config_with_provenance --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m9-knowledge-hits-without-provenance.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=9d73c40dd936acc8f7d922720718669974ac07d333378f6785576a92fc81d2a0; output-bytes=165

- [x] G14: an embedded knowledge result carries relationship evidence linking a document to the code it names, plus the source's authority and coverage
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embedded_search_knowledge_reports_relationships_authority_and_coverage -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_relationships_authority_and_coverage --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embedded_search_knowledge_reports_relationships_authority_and_coverage
  RED: failing-first against a stub without relationship data.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=85f0c42449a08c08b1e23e6fa5691ed8b1ddc569c086a552bcf584040463e6e9; output-bytes=112

- [x] G15: a result that drops relationship evidence lets the G14 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m10-knowledge-drops-relationships.patch --name embedded_search_knowledge_reports_relationships_authority_and_coverage -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_relationships_authority_and_coverage --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m10-knowledge-drops-relationships.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=3cd4e86ebe93bd7959755ea25b328d533d8087fc90a779045bc99fbe7966e772; output-bytes=158

- [x] G16: an embedded knowledge query whose scope holds withheld files reports how many were withheld, in the neutral wording
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embedded_search_knowledge_reports_withheld_evidence -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_withheld_evidence --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embedded_search_knowledge_reports_withheld_evidence
  RED: failing-first against a stub that reports no withholding.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=1e02cf74098663ba43127f70b39e73b5ab201f1f100a3cc682b7363209c3aa5a; output-bytes=93

- [x] G17: a result that drops withheld evidence lets the G16 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m11-knowledge-drops-withheld-evidence.patch --name embedded_search_knowledge_reports_withheld_evidence -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embedded_search_knowledge_reports_withheld_evidence --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m11-knowledge-drops-withheld-evidence.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c1bf61e495386495164ec397b5cf3fc72a8784e127418bc2d79dffa662588d2d; output-bytes=143

- [x] G18: for the same fixture and query, the typed seam returns the same hits in the same order as the server search_knowledge tool
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name typed_knowledge_seam_matches_server_search_knowledge_hits -- test -j 8 --lib -- typed_knowledge_seam_matches_server_search_knowledge_hits --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=typed_knowledge_seam_matches_server_search_knowledge_hits
  RED: guard for plan Task 6 Step 2 ("do not alter `search_knowledge` retrieval semantics"); its teeth are G19. Runs in the default server cell because the server tool exists only there.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=97bcf76c91dcc624e05cdf104cbb8b4fb40875ee891288cabc5c0993aa7c8ccf; output-bytes=99

- [x] G19: a seam that re-ranks hits lets the G18 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m12-seam-reranks-hits.patch --name typed_knowledge_seam_matches_server_search_knowledge_hits -- test -j 8 --lib -- typed_knowledge_seam_matches_server_search_knowledge_hits --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m12-seam-reranks-hits.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=11f558d6bcc43e4f62030d1414814458d62f880517deb531052a82dc1cd0ef04; output-bytes=133

- [x] G20: an ambiguous project display name is refused with the candidate ids, and an unknown one with the open projects
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name test_runtime_for_target_resolution_contract -- test -j 8 --lib -- test_runtime_for_target_resolution_contract --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=test_runtime_for_target_resolution_contract
  RED: regression guard; already true at v11.2.0; its teeth are G21.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=4ab185e5c15996b081c6434725f75955ecdcd472d9eeb23ad55796c34d7478ca; output-bytes=85

- [x] G21: a resolver that takes the first matching display name lets the G20 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m6-ambiguous-name-picks-first-match.patch --name test_runtime_for_target_resolution_contract -- test -j 8 --lib -- test_runtime_for_target_resolution_contract --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m6-ambiguous-name-picks-first-match.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=995c3e209c68784eda43622dde24e672bc434a8d67702edef1b047003ef21fbc; output-bytes=133

- [x] G22: an embedded search_symbols and search_text with path_prefix src/ return matches under src/ and none under srcx/
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embed_path_prefix_src_excludes_srcx -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embed_path_prefix_src_excludes_srcx --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embed_path_prefix_src_excludes_srcx
  RED: failing-first; at v11.2.0 the handle's plain `starts_with` matches `srcx/`.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=130edcb257d3713d62c496157e70ad3e74f122b8a88dadaa562622f8544f5ec2; output-bytes=77

- [x] G23: restoring the plain prefix match lets the G22 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task6-m7-plain-prefix-match.patch --name embed_path_prefix_src_excludes_srcx -- test -j 8 --no-default-features --features embed --test embed_bound_index -- embed_path_prefix_src_excludes_srcx --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task6-m7-plain-prefix-match.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=1351105c8b75290b5098e24c7a94f681452bb3644d6ed06f74116a4978c8717f; output-bytes=111

- [x] G24: the excluded count and the embedded knowledge retrieval read the published source set without indexing Markdown bodies a second time
  RECORD: the code paths (symbol and file:line) showing the count reads manifest or coverage data only and the seam reads the existing published knowledge units, with no new body read or second index; reviewed by the owner (plan Task 6 Step 2)
  EVIDENCE: 2026-09-16 on d669f895. Owner accepted C29 and the published-generation seam: no second Markdown index.
    - Excluded count: `count_in_scope_knowledge_only_files` (`search.rs:132-156`) walks `index.manifest_entries` only. It filters `FileDisposition::Indexed { targets: IndexTargets::Knowledge }` plus `path_scope.matches`. No file body is opened. Wired through `with_excluded_knowledge_count` (`search.rs:186-197`).
    - Withheld counts: `count_in_scope_withheld_files` (`search.rs:158`) also walks `manifest_entries` dispositions only (`SensitivePath`/`SensitiveContent` vs size).
    - Embed retrieve: `EmbeddedSourceHandle::search_knowledge` (`embedded.rs:1438`) calls `retrieve_knowledge` (`knowledge_retrieve.rs:279`). `extract_lane` iterates `generation.authority.records` and slices `generation.live.files` already in the published generation (`:503-516`). No disk reopen, no second index. `knowledge_search.rs` formats that result only (`:261`).

- [x] G25: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=336cd67d8d6efa261561b22d7c4470d08414a3dd69a15320bc5f11a85fe31c5d; output-bytes=41

- [x] G26: the contract keeps every v11.2.0 atom and records the knowledge retrieval atoms, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::KnowledgeSearchRequest --require-atom symforge::embed::KnowledgeSearchResult --require-atom symforge::embed::EmbeddedSourceHandle::search_knowledge
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=955ab01ab9bdb405ec230e26b2e547aec3da7775c86d5ed3220948bb2b007641; output-bytes=51

- [x] G27: the regenerated export delta matches the amended contract atoms and wrap table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name export_delta_matches_frozen_contract_atoms -- test -j 8 --test public_api_delta_v11 -- export_delta_matches_frozen_contract_atoms --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=export_delta_matches_frozen_contract_atoms
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f5a517337df56a1c4bed5b0b43280c969e37ae9a1ff6118cb7a3465200977b42; output-bytes=84

- [x] G28: the embed facade contract tripwire names the knowledge items and passes in the embed lib cell
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name facade_contract_is_stable -- test -j 8 --no-default-features --features embed --lib -- facade_contract_is_stable --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=facade_contract_is_stable
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=38bbef32fac16dbfbf1d400eda166addf9674c17fb546aa77a2b691a845b4417; output-bytes=67

- [x] G29: the hand-maintained CHANGELOG embed section names the knowledge retrieval method and types
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs doc-has --file CHANGELOG.md --section "## Embedder API" --token search_knowledge --token KnowledgeSearchRequest --token KnowledgeSearchResult
  EXPECT: LEDGER-ORACLE DOC GREEN file=CHANGELOG.md;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=ae69cd82806a9e967d1f6add401fefc8161df1c18445aed5bba1a5bcd83f6dcc; output-bytes=43

- [x] G30: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G2, G7, G11, G14, G16 and G22 tests, the working or stub state, exact command and each failure line; G5, G9, G18 and G20 are declared guards
  EVIDENCE: 2026-09-16. G2/G7/G11/G14/G16/G22 names were added with the RED notes in this ledger (v11.2.0 silence / empty stubs / plain `starts_with`); their teeth are G3/G4, G8, G12/G13, G15, G17 and G23. G5, G9, G18 and G20 are declared guards. `--approve` observed those tests GREEN and the named mutants CAUGHT. G4 and G23 first patches were compile errors (`unused path` / `String: Pattern`); they were rewritten so the tests FAIL.

- [x] G31: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=7f2d7cdbd6284c9dbb1106f833cea3c6e5d99f8b70e1a47e62bfea5426d485e1; output-bytes=11139

- [x] G32: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c0b92c7e8f2232d84908fd862489de264811d67a0cbc66dc91d6db8319210ee4; output-bytes=41
