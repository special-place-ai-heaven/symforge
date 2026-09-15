# Gates: leaf 1.5, plan Task 5, CR-3 monotonic live indexing progress

OWNS: src/index_lifecycle/embedded.rs, src/index_lifecycle/public_api.rs, src/embed.rs, src/lifecycle_identity.rs, src/live_index/store.rs, specs/020-repository-knowledge-index/contracts/public-api-v11.json, docs/reviews/FEATURE-020-EXPORT-DELTA-v11.json, tests/public_api_delta_v11.rs, tests/embed_bound_index.rs, CHANGELOG.md, docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-*.patch

Scope: an infallible `EmbeddedSourceHandle::index_progress` returns binding-owned counters with exact documented meanings that reset before each real reload, rise only during it, keep their last values after a cancelled reload, and never move on idle tick scouts, added as recorded contract atoms without adding fields to `SourceRuntimeView`

Plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Task 5. Owner revision 2
keeps `IndexProgress { files_discovered, files_parsed, symbols_found }` and pins
meanings (C28): `files_discovered` is this reload's executable scout work-set;
`files_parsed` is post-fold `Parsed` outcomes that remain parsed; `symbols_found`
is symbols extracted from those files. Current progress need not equal census
totals. The method stays infallible and does not add an `OperationKind` variant.
Fields declared inside the new type are approved with it. Tests go in
`tests/embed_bound_index.rs` and reuse leaf 1.3's root-scoped reload hold. Test
names are this ledger's.

- [ ] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.5.md
  EXPECT: LINT OK
  EVIDENCE: pending

- [ ] G1: every plan and brief citation this task relies on is re-read on the branch before its next edit, with its status recorded
  RECORD: table of the branch commit, each cited file:line or symbol (plan Task 5, brief section CR-3), holds / moved-to / changed
  EVIDENCE: pending

- [ ] G2: with a reload held after a known number of parses, parsed progress equals that number, and it does not change once the source is Current
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_progress_rises_only_during_a_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_rises_only_during_a_reload --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_progress_rises_only_during_a_reload
  RED: failing-first against a stub that returns zeroed counters.
  EVIDENCE: pending

- [ ] G3: a parsed counter that never advances lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m1-parsed-counter-never-advances.patch --name index_progress_rises_only_during_a_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_rises_only_during_a_reload --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m1-parsed-counter-never-advances.patch
  EVIDENCE: pending

- [ ] G4: a refresh reload starts its counters from zero rather than from the previous reload's totals
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_progress_resets_before_each_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_resets_before_each_reload --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_progress_resets_before_each_reload
  RED: failing-first against a stub that never resets.
  EVIDENCE: pending

- [ ] G5: counters that are not reset before a reload let the G4 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m2-no-reset-before-reload.patch --name index_progress_resets_before_each_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_resets_before_each_reload --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m2-no-reset-before-reload.patch
  EVIDENCE: pending

- [ ] G6: after a close cancels a held reload, progress still reports the values reached before the cancel
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_progress_keeps_last_values_after_cancel -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_keeps_last_values_after_cancel --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_progress_keeps_last_values_after_cancel
  RED: failing-first against a stub that zeroes on cancel.
  EVIDENCE: pending

- [ ] G7: counters cleared on cancellation let the G6 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m3-cancel-resets-progress.patch --name index_progress_keeps_last_values_after_cancel -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_keeps_last_values_after_cancel --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m3-cancel-resets-progress.patch
  EVIDENCE: pending

- [ ] G8: several idle tick scouts on a Current source leave every counter unchanged
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name idle_tick_scout_leaves_progress_untouched -- test -j 8 --no-default-features --features embed --test embed_bound_index -- idle_tick_scout_leaves_progress_untouched --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=idle_tick_scout_leaves_progress_untouched
  RED: guard; its teeth are G9.
  EVIDENCE: pending

- [ ] G9: a tick scout that updates the discovered counter lets the G8 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m4-tick-scout-updates-progress.patch --name idle_tick_scout_leaves_progress_untouched -- test -j 8 --no-default-features --features embed --test embed_bound_index -- idle_tick_scout_leaves_progress_untouched --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m4-tick-scout-updates-progress.patch
  EVIDENCE: pending

- [ ] G10: the hand-maintained CHANGELOG embed section names the progress method, its type and each counter
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs doc-has --file CHANGELOG.md --section "## Embedder API" --token index_progress --token IndexProgress --token files_discovered --token files_parsed --token symbols_found
  EXPECT: LEDGER-ORACLE DOC GREEN file=CHANGELOG.md;
  EVIDENCE: pending

- [ ] G11: each counter's documented meaning names the exact lifecycle point where it increments, and the code increments it there and nowhere else
  RECORD: for each counter, the rustdoc sentence, the one increment site (file:line on the branch) and whether a staged-bytes hard skip or circuit-breaker fold can still change a counted file (brief CR-3, "Define the counts precisely"); reviewed by the owner
  EVIDENCE: pending

- [ ] G12: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: pending

- [ ] G13: the contract keeps every v11.2.0 atom and records the progress atoms, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::IndexProgress --require-atom symforge::embed::EmbeddedSourceHandle::index_progress
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: pending

- [ ] G14: the regenerated export delta matches the amended contract atoms and wrap table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name export_delta_matches_frozen_contract_atoms -- test -j 8 --test public_api_delta_v11 -- export_delta_matches_frozen_contract_atoms --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=export_delta_matches_frozen_contract_atoms
  EVIDENCE: pending

- [ ] G15: the embed facade contract tripwire names the progress items and passes in the embed lib cell
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name facade_contract_is_stable -- test -j 8 --no-default-features --features embed --lib -- facade_contract_is_stable --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=facade_contract_is_stable
  EVIDENCE: pending

- [ ] G16: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G2, G4 and G6 tests, the stub state, exact command and each failure line; G8 is a declared guard
  EVIDENCE: pending

- [ ] G17: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [ ] G18: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: pending
