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

- [x] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.4.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=21b3ecd28ecd70e37a737c6520f5d31629515e1baea13192a98fd4d26e01bc23; output-bytes=325

- [x] G1: every plan and brief citation this task relies on is re-read on the branch before its next edit, with its status recorded
  RECORD: table of the branch commit, each cited file:line or symbol (plan Task 4, brief section CR-2), holds / moved-to / changed, and the owner's operation-kind decision
  EVIDENCE: 2026-09-16 re-read on `feature/fathom-embed-wave2` d669f895. Plan Task 4 and brief CR-2. Owner revision 2: `OperationKind::IndexCensus` only; `SearchSymbols` reuse forbidden; `ALL` is `[Self; 9]`.
    | brief / plan cite | status on d669f895 |
    |---|---|
    | proposed `IndexCensus` / `CensusFile` | HOLDS: `public_api.rs:217` and `:225`; re-exported from `embed.rs:84`. |
    | proposed `index_census` on the handle | HOLDS: `embedded.rs:1350`; claims use `OperationKind::IndexCensus` (`:1360`, `:1367`, `:1371`, `:1377`). |
    | `OperationKind::ALL` | HOLDS: `lifecycle_identity.rs:151` is `[Self; 9]`; `IndexCensus` is `:144`, after the original seven, before `SearchKnowledge`. |
    | census via runtime acquire, not data-plane bypass | HOLDS: method goes through `current_claim` + `acquire()`, not `data_plane()`. |
    | `SourceRuntimeView` must stay unchanged | HOLDS fields: `public_api.rs:462-468` still the five v11.2.0 fields (line moved from brief `:392`). |

- [x] G2: a Current source's census lists every parsed file sorted by path, including files with zero symbols
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_census_lists_sorted_zero_symbol_parsed_files -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_lists_sorted_zero_symbol_parsed_files --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_census_lists_sorted_zero_symbol_parsed_files
  RED: failing-first against a stub that returns an empty census.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=33e9cc64ef82d6b7130f086a4cf2e8a15e0d40b9058b2013af365fbb0eef99ca; output-bytes=92

- [x] G3: a census that drops zero-symbol files lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m1-drop-zero-symbol-files.patch --name index_census_lists_sorted_zero_symbol_parsed_files -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_lists_sorted_zero_symbol_parsed_files --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m1-drop-zero-symbol-files.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=591145ba5ddf063ed35361b1d354fe3bcd8ddc10bb477df681f9f93f6beedf09; output-bytes=130

- [x] G4: a census in index order rather than path order lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m2-census-unsorted.patch --name index_census_lists_sorted_zero_symbol_parsed_files -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_lists_sorted_zero_symbol_parsed_files --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m2-census-unsorted.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=6023c6d02e3d2d50abbf6db060c8ab39efbfb4f11c79725ac8e5f61855298184; output-bytes=123

- [x] G5: each file's census symbol count equals that path's symbol matches in the same captured publication
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_census_path_counts_match_the_captured_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_path_counts_match_the_captured_publication --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_census_path_counts_match_the_captured_publication
  RED: failing-first against a stub that reports zero symbols per file.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=da67cc994d49f7204c8510c17c34e93f42d6fe38684963501e728231de041f61; output-bytes=97

- [x] G6: a census that reports a wrong per-file symbol count lets the G5 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m3-census-counts-wrong.patch --name index_census_path_counts_match_the_captured_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_path_counts_match_the_captured_publication --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m3-census-counts-wrong.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=9c144fc378c6ecc58d2ff9042f0f9dafe9a1b2cc96ce870ac3ded57055ade29d; output-bytes=132

- [x] G7: a census taken while a new publication lands between its reads still reports one generation's files and counts
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_census_reads_one_generation_across_a_publish -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_reads_one_generation_across_a_publish --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_census_reads_one_generation_across_a_publish
  TEST: needs a test-only hold inside `index_census` after its first read, a publish released during the hold, then a check that files and counts all come from one generation.
  RED: guard for plan Task 4 Step 2 ("Read one publication generation for both live data and outline"); its teeth are G8.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=6c1a88badd2a4f09cf75b102e10569413ef430d77b0169325fd0e25df90c298d; output-bytes=92

- [x] G8: a census that loads live data and outline separately lets the G7 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m4-census-reads-two-generations.patch --name index_census_reads_one_generation_across_a_publish -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_reads_one_generation_across_a_publish --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m4-census-reads-two-generations.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=2d3dd181464b8953251617a093401344acce7c2f04d84e5f20d17d73a56d895d; output-bytes=136

- [x] G9: a census before Current refuses with a typed SourceUnavailable refusal and OnEvent retry, and a census after the source's admission is revoked refuses
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 2 --name index_census_refuses_before_current --name index_census_refuses_after_admission_revocation -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_refuses_before_current index_census_refuses_after_admission_revocation --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=2; names=index_census_refuses_before_current,index_census_refuses_after_admission_revocation
  RED: failing-first against a stub that always answers.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=86206455fe1be31e837f404ab982a2f0674f2af10ffc5f331f75173d3c89470d; output-bytes=125

- [x] G10: a census that skips the readiness claim lets the before-Current test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m5-census-skips-readiness-claim.patch --name index_census_refuses_before_current -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_refuses_before_current --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m5-census-skips-readiness-claim.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=e7fa564849697be8181fde568b5d1a722c987ddbee1b26d9473aff32d8f9b884; output-bytes=121

- [x] G11: a census that reads the raw data plane instead of runtime acquisition lets the revocation test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task4-m6-census-bypasses-acquire.patch --name index_census_refuses_after_admission_revocation -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_census_refuses_after_admission_revocation --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task4-m6-census-bypasses-acquire.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=bc780e84ee1549aa730f36e1bab5cbd59789711e0c2a41358a783781cf254633; output-bytes=128

- [x] G12: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=336cd67d8d6efa261561b22d7c4470d08414a3dd69a15320bc5f11a85fe31c5d; output-bytes=41

- [x] G13: the contract keeps every v11.2.0 atom and records the census atoms, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::IndexCensus --require-atom symforge::embed::CensusFile --require-atom symforge::embed::EmbeddedSourceHandle::index_census
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=955ab01ab9bdb405ec230e26b2e547aec3da7775c86d5ed3220948bb2b007641; output-bytes=51

- [x] G14: the regenerated export delta matches the amended contract atoms and wrap table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name export_delta_matches_frozen_contract_atoms -- test -j 8 --test public_api_delta_v11 -- export_delta_matches_frozen_contract_atoms --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=export_delta_matches_frozen_contract_atoms
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f5a517337df56a1c4bed5b0b43280c969e37ae9a1ff6118cb7a3465200977b42; output-bytes=84

- [x] G15: the embed facade contract tripwire names the census items and passes in the embed lib cell
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name facade_contract_is_stable -- test -j 8 --no-default-features --features embed --lib -- facade_contract_is_stable --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=facade_contract_is_stable
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=38bbef32fac16dbfbf1d400eda166addf9674c17fb546aa77a2b691a845b4417; output-bytes=67

- [x] G16: the hand-maintained CHANGELOG embed section names every census item
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs doc-has --file CHANGELOG.md --section "## Embedder API" --token index_census --token IndexCensus --token CensusFile
  EXPECT: LEDGER-ORACLE DOC GREEN file=CHANGELOG.md;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=ae69cd82806a9e967d1f6add401fefc8161df1c18445aed5bba1a5bcd83f6dcc; output-bytes=43

- [x] G17: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G2, G5 and G9 tests, the stub state, exact command and each failure line; G7 is a declared guard
  EVIDENCE: 2026-09-16. G2/G5/G9 names were added with empty-census / zero-count / always-answer stubs as RED notes in this ledger; their teeth are G3/G6/G10. G7 is a declared guard (teeth G8). `--approve` of this leaf observed G2, G5 and G9 GREEN and G3/G6/G8/G11 CAUGHT.

- [x] G18: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=7f2d7cdbd6284c9dbb1106f833cea3c6e5d99f8b70e1a47e62bfea5426d485e1; output-bytes=11139

- [ ] G19: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: pending
