# Gates: leaf 1.4, plan Task 4, CR-2 generation-consistent index census

OWNS: src/index_lifecycle/embedded.rs, src/index_lifecycle/public_api.rs, src/embed.rs, src/lifecycle_identity.rs, src/live_index/store.rs, src/live_index/query.rs, specs/020-repository-knowledge-index/contracts/public-api-v11.json, docs/reviews/FEATURE-020-EXPORT-DELTA-v11.json, tests/public_api_delta_v11.rs, tests/embed_bound_index.rs, CHANGELOG.md, docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-*.patch

Scope: `EmbeddedSourceHandle::index_census` returns, from one publication generation and through the normal runtime acquisition path, every parsed file sorted by path with its symbol count including zero-symbol files, refuses with a typed readiness refusal before `Current`, and is added as recorded contract atoms with nothing existing changed

Plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Task 4. Tests: the
"nearest embed lifecycle" file is `tests/embed_bound_index.rs` (embed integration
cell); the contract test is `tests/public_api_delta_v11.rs`
(`#![cfg(feature = "server")]`, default server cell). Test names are this
ledger's. Names follow the plan (`IndexCensus`, `CensusFile`, `index_census`);
fields and methods declared inside those new types are approved with them by
the additive gate, and no field may be added to an existing public type.

Owner revision 2: `index_census` claims and refusals use `OperationKind::IndexCensus`.
Reusing `SearchSymbols` is forbidden (it mislabels the receipt). That is a major.
`ALL` becomes `[Self; 9]`. The original seven variants stay, in order.

- [ ] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.4.md
  EXPECT: LINT OK
  EVIDENCE: pending

- [ ] G1: every plan and brief citation this task relies on is re-read on the branch before its next edit, with its status recorded
  RECORD: table of the branch commit, each cited file:line or symbol (plan Task 4, brief section CR-2), holds / moved-to / changed, and the owner's operation-kind decision
  EVIDENCE: 2026-09-16 re-read on `feature/fathom-embed-wave2` d669f895. Plan Task 4 and brief CR-2. Owner revision 2: `OperationKind::IndexCensus` only; `SearchSymbols` reuse forbidden; `ALL` is `[Self; 9]`.
    | brief / plan cite | status on d669f895 |
    |---|---|
    | proposed `IndexCensus` / `CensusFile` | HOLDS: `public_api.rs:217` and `:225`; re-exported from `embed.rs:84`. |
    | proposed `index_census` on the handle | HOLDS: `embedded.rs:1350`; claims use `OperationKind::IndexCensus` (`:1360`, `:1367`, `:1371`, `:1377`). |
    | `OperationKind::ALL` | HOLDS: `lifecycle_identity.rs:151` is `[Self; 9]`; `IndexCensus` is `:144`, after the original seven, before `SearchKnowledge`. |
    | census via runtime acquire, not data-plane bypass | HOLDS: method goes through `current_claim` + `acquire()`, not `data_plane()`. |
    | `SourceRuntimeView` must stay unchanged | HOLDS fields: `public_api.rs:462-468` still the five v11.2.0 fields (line moved from brief `:392`). |

- [ ] G2: a Current source's census lists every parsed file sorted by path, including files with zero symbols
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_census_lists_sorted_zero_symbol_parsed_files -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_lists_sorted_zero_symbol_parsed_files --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_census_lists_sorted_zero_symbol_parsed_files
  RED: failing-first against a stub that returns an empty census.
  EVIDENCE: pending

- [ ] G3: a census that drops zero-symbol files lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m1-drop-zero-symbol-files.patch --name index_census_lists_sorted_zero_symbol_parsed_files -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_lists_sorted_zero_symbol_parsed_files --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m1-drop-zero-symbol-files.patch
  EVIDENCE: pending

- [ ] G4: a census in index order rather than path order lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m2-census-unsorted.patch --name index_census_lists_sorted_zero_symbol_parsed_files -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_lists_sorted_zero_symbol_parsed_files --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m2-census-unsorted.patch
  EVIDENCE: pending

- [ ] G5: each file's census symbol count equals that path's symbol matches in the same captured publication
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_census_path_counts_match_the_captured_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_path_counts_match_the_captured_publication --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_census_path_counts_match_the_captured_publication
  RED: failing-first against a stub that reports zero symbols per file.
  EVIDENCE: pending

- [ ] G6: a census that reports a wrong per-file symbol count lets the G5 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m3-census-counts-wrong.patch --name index_census_path_counts_match_the_captured_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_path_counts_match_the_captured_publication --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m3-census-counts-wrong.patch
  EVIDENCE: pending

- [ ] G7: a census taken while a new publication lands between its reads still reports one generation's files and counts
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_census_reads_one_generation_across_a_publish -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_reads_one_generation_across_a_publish --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_census_reads_one_generation_across_a_publish
  TEST: needs a test-only hold inside `index_census` after its first read, a publish released during the hold, then a check that files and counts all come from one generation.
  RED: guard for plan Task 4 Step 2 ("Read one publication generation for both live data and outline"); its teeth are G8.
  EVIDENCE: pending

- [ ] G8: a census that loads live data and outline separately lets the G7 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m4-census-reads-two-generations.patch --name index_census_reads_one_generation_across_a_publish -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_reads_one_generation_across_a_publish --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m4-census-reads-two-generations.patch
  EVIDENCE: pending

- [ ] G9: a census before Current refuses with a typed SourceUnavailable refusal and OnEvent retry, and a census after the source's admission is revoked refuses
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 2 --name index_census_refuses_before_current --name index_census_refuses_after_admission_revocation -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_refuses_before_current index_census_refuses_after_admission_revocation --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=2; names=index_census_refuses_before_current,index_census_refuses_after_admission_revocation
  RED: failing-first against a stub that always answers.
  EVIDENCE: pending

- [ ] G10: a census that skips the readiness claim lets the before-Current test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m5-census-skips-readiness-claim.patch --name index_census_refuses_before_current -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_refuses_before_current --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m5-census-skips-readiness-claim.patch
  EVIDENCE: pending

- [ ] G11: a census that reads the raw data plane instead of runtime acquisition lets the revocation test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m6-census-bypasses-acquire.patch --name index_census_refuses_after_admission_revocation -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_refuses_after_admission_revocation --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m6-census-bypasses-acquire.patch
  EVIDENCE: pending

- [ ] G12: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: pending

- [ ] G13: the contract keeps every v11.2.0 atom and records the census atoms, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::IndexCensus --require-atom symforge::embed::CensusFile --require-atom symforge::embed::EmbeddedSourceHandle::index_census
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: pending

- [ ] G14: the regenerated export delta matches the amended contract atoms and wrap table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name export_delta_matches_frozen_contract_atoms -- test -j 8 --test public_api_delta_v11 -- export_delta_matches_frozen_contract_atoms --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=export_delta_matches_frozen_contract_atoms
  EVIDENCE: pending

- [ ] G15: the embed facade contract tripwire names the census items and passes in the embed lib cell
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name facade_contract_is_stable -- test -j 8 --no-default-features --features embed --lib -- facade_contract_is_stable --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=facade_contract_is_stable
  EVIDENCE: pending

- [ ] G16: the hand-maintained CHANGELOG embed section names every census item
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs doc-has --file CHANGELOG.md --section "## Embedder API" --token index_census --token IndexCensus --token CensusFile
  EXPECT: LEDGER-ORACLE DOC GREEN file=CHANGELOG.md;
  EVIDENCE: pending

- [ ] G17: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G2, G5 and G9 tests, the stub state, exact command and each failure line; G7 is a declared guard
  EVIDENCE: pending

- [ ] G18: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [ ] G19: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: pending
