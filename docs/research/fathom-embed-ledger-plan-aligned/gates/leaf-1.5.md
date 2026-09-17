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

- [x] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.5.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=7f47869b1e9adf5ecfdb52625cf29b168d18bf94b2fb781d48add41c95c0dcdd; output-bytes=444

- [x] G1: every plan and brief citation this task relies on is re-read on the branch before its next edit, with its status recorded
  RECORD: table of the branch commit, each cited file:line or symbol (plan Task 5, brief section CR-3), holds / moved-to / changed
  EVIDENCE: 2026-09-16 re-read on `feature/fathom-embed-wave2` d669f895. Plan Task 5 and brief CR-3. Owner: no `OperationKind` for progress; method stays infallible.
    | brief / plan cite | status on d669f895 |
    |---|---|
    | `SourceRuntimeView` fields (`public_api.rs:392-400`); not `#[non_exhaustive]` | HOLDS fields, line MOVED: `public_api.rs:462-468`. No progress fields added. |
    | proposed `index_progress` / `IndexProgress` | HOLDS: type `public_api.rs:241`; method `embedded.rs:1424` returns `IndexProgress` (infallible). |
    | counters on `EmbeddedBinding` next to `state` (`embedded.rs:157`) | CHANGED: `ReloadProgressSink` atomics live in `store.rs:214`; binding registers via `register_reload_progress` (`store.rs:259`). |
    | reset at start of each reload | HOLDS: `reload_and_publish` calls `self.progress.reset()` at `embedded.rs:545`. |
    | tick scout must not touch counters (`embedded.rs:299-307`) | HOLDS: `note_reload_discovered` is only at real-reload scout finish (`store.rs:5743`). |
    | after cancel, keep last values | HOLDS: cancel path does not call `reset()`. |

- [x] G2: with a reload held after a known number of parses, parsed progress equals that number, and it does not change once the source is Current
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_progress_rises_only_during_a_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_rises_only_during_a_reload --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_progress_rises_only_during_a_reload
  RED: failing-first against a stub that returns zeroed counters.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=4e50ff3604e35c7a966f826f7adabfac1c8ce9760c1eb76e2dd95feb8a3d37b8; output-bytes=83

- [x] G3: a parsed counter that never advances lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m1-parsed-counter-never-advances.patch --name index_progress_rises_only_during_a_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_rises_only_during_a_reload --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m1-parsed-counter-never-advances.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=d25522a7d8a0a24d6c91269f57206111d05acfe6fb49fa4684479bcb9ecddc96; output-bytes=128

- [x] G4: a refresh reload starts its counters from zero rather than from the previous reload's totals
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_progress_resets_before_each_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_resets_before_each_reload --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_progress_resets_before_each_reload
  RED: failing-first against a stub that never resets.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=3e4d7084ac8772d1017fe16e80767bcebedcb5d764b74c0ce50f6e9b17bf9144; output-bytes=82

- [x] G5: counters that are not reset before a reload let the G4 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m2-no-reset-before-reload.patch --name index_progress_resets_before_each_reload -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_resets_before_each_reload --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m2-no-reset-before-reload.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=a5e46348a18dabfaa9f5587785a2749b54067ac5cb733164725eeed0d4a49f32; output-bytes=120

- [x] G6: after a close cancels a held reload, progress still reports the values reached before the cancel
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name index_progress_keeps_last_values_after_cancel -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_keeps_last_values_after_cancel --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=index_progress_keeps_last_values_after_cancel
  RED: failing-first against a stub that zeroes on cancel.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=74de17b6335ccc83ac572c522ec9a161dee6bd1628482e98e18c9968c165a3bb; output-bytes=87

- [x] G7: counters cleared on cancellation let the G6 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m3-cancel-resets-progress.patch --name index_progress_keeps_last_values_after_cancel -- test -j 8 --no-default-features --features embed --test embed_bound_index -- index_progress_keeps_last_values_after_cancel --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m3-cancel-resets-progress.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c9c83bfa801cdc324497faff9fba410dd738b9b2662aac579f13032ec3976e0f; output-bytes=125

- [x] G8: several idle tick scouts on a Current source leave every counter unchanged
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name idle_tick_scout_leaves_progress_untouched -- test -j 8 --no-default-features --features embed --test embed_bound_index -- idle_tick_scout_leaves_progress_untouched --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=idle_tick_scout_leaves_progress_untouched
  RED: guard; its teeth are G9.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=93515335c92a9f15f87fb983b47ca57e6f1f3104995dc6c1965fac65d5daaf41; output-bytes=83

- [x] G9: a tick scout that updates the discovered counter lets the G8 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task5-m4-tick-scout-updates-progress.patch --name idle_tick_scout_leaves_progress_untouched -- test -j 8 --no-default-features --features embed --test embed_bound_index -- idle_tick_scout_leaves_progress_untouched --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task5-m4-tick-scout-updates-progress.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=70c04a7a0d89d881fcca528a2e93853fbdd080e3ea0b190390b0cbf3847249b1; output-bytes=126

- [x] G10: the hand-maintained CHANGELOG embed section names the progress method, its type and each counter
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs doc-has --file CHANGELOG.md --section "## Embedder API" --token index_progress --token IndexProgress --token files_discovered --token files_parsed --token symbols_found
  EXPECT: LEDGER-ORACLE DOC GREEN file=CHANGELOG.md;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=ae69cd82806a9e967d1f6add401fefc8161df1c18445aed5bba1a5bcd83f6dcc; output-bytes=43

- [x] G11: each counter's documented meaning names the exact lifecycle point where it increments, and the code increments it there and nowhere else
  RECORD: for each counter, the rustdoc sentence, the one increment site (file:line on the branch) and whether a staged-bytes hard skip or circuit-breaker fold can still change a counted file (brief CR-3, "Define the counts precisely"); reviewed by the owner
  EVIDENCE: 2026-09-16 on d669f895, increment sites re-read at HEAD. Rustdoc is `public_api.rs:231-238`. Owner accepted C28 meanings.
    | counter | rustdoc | increment site | fold / staged-bytes can still drop the file? |
    |---|---|---|---|
    | `files_discovered` | "this reload's executable scout work-set, stored when that scout finishes" | `store.rs:5794` `note_reload_discovered(root, projection.entries.len())` after scout projection (`:329` stores, does not increment) | No. This is the scout work-set size, not a per-file parse outcome. |
    | `files_parsed` | "increments when a file parse completes, at the same lifecycle point as the reload hold; a later staged-bytes hard skip or circuit-breaker fold can still drop that file" | `store.rs:5213` `note_reload_parsed` immediately after `process_file_with_classification` (`:335`) | Yes. Staged-bytes handoff still follows the increment. |
    | `symbols_found` | "increments from symbols extracted on those parsed files" | `store.rs:5230` `note_reload_symbols(..., indexed.symbols.len())` (`:341`) | Yes. Same handoff follows the increment. |

- [x] G12: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=336cd67d8d6efa261561b22d7c4470d08414a3dd69a15320bc5f11a85fe31c5d; output-bytes=41

- [x] G13: the contract keeps every v11.2.0 atom and records the progress atoms, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::IndexProgress --require-atom symforge::embed::EmbeddedSourceHandle::index_progress
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=955ab01ab9bdb405ec230e26b2e547aec3da7775c86d5ed3220948bb2b007641; output-bytes=51

- [x] G14: the regenerated export delta matches the amended contract atoms and wrap table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name export_delta_matches_frozen_contract_atoms -- test -j 8 --test public_api_delta_v11 -- export_delta_matches_frozen_contract_atoms --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=export_delta_matches_frozen_contract_atoms
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f5a517337df56a1c4bed5b0b43280c969e37ae9a1ff6118cb7a3465200977b42; output-bytes=84

- [x] G15: the embed facade contract tripwire names the progress items and passes in the embed lib cell
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name facade_contract_is_stable -- test -j 8 --no-default-features --features embed --lib -- facade_contract_is_stable --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=facade_contract_is_stable
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=38bbef32fac16dbfbf1d400eda166addf9674c17fb546aa77a2b691a845b4417; output-bytes=67

- [x] G16: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G2, G4 and G6 tests, the stub state, exact command and each failure line; G8 is a declared guard
  EVIDENCE: 2026-09-16. G2/G4/G6 names were added with zeroed / never-reset / zero-on-cancel stubs as RED notes in this ledger; their teeth are G3/G5/G7. G8 is a declared guard (teeth G9). `--approve` observed G2, G4, G6 and G8 GREEN and G3/G5/G7/G9 CAUGHT. G7 needed the test to wait until the cancelled reload reached Stopped; G9's first patch no-op'd because idle ticks have no registered sink and `observe_fingerprint` is shared with real reload.

- [x] G17: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=7f2d7cdbd6284c9dbb1106f833cea3c6e5d99f8b70e1a47e62bfea5426d485e1; output-bytes=11139

- [x] G18: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c0b92c7e8f2232d84908fd862489de264811d67a0cbc66dc91d6db8319210ee4; output-bytes=41
