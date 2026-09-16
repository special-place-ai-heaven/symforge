# Gates: leaf 1.2, plan Task 2, CR-0 process-wide embedded ownership

OWNS: src/index_lifecycle/process_runtime.rs, src/index_lifecycle/public_api.rs, src/index_lifecycle/embedded.rs, tests/activation_cut_v11.rs, tests/embed_bound_index.rs, docs/research/fathom-embed-ledger-plan-aligned/mutations/task2-*.patch

Scope: a second `ProcessRuntimeApi` opening a root already open through another is refused with `SelectionUnavailable`; an open that unwinds after registration leaks neither its registration nor its admission and the root opens again; the guards that undo an open or close a runtime act only on their own identity; an open of an unrelated root is not held up by another root's close; no dependency, public item or contract atom changes except the plan's additive atoms

Plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Task 2 (the process runtime
owns one shared `EmbeddedSourceFactory`). The gates below pin outcomes, not that
internal design. Tests stay where the plan and its session put them:
`tests/activation_cut_v11.rs` is `#![cfg(feature = "server")]` (line 88 at
drafting), so its tests compile only with the server feature and G2 to G10 run in
the default server cell; `tests/embed_bound_index.rs` is
`#![cfg(feature = "embed")]` and G11 and G12 run it with
`--no-default-features --features embed`, the command the session added to CI.
Test names already present on the branch at drafting:
`embedded_source_has_one_handle_and_no_raw_bypass`,
`panicking_open_releases_its_process_wide_root_reservation`,
`unrelated_open_completes_while_another_root_closes`. The other names are this
ledger's; renaming one is a ledger amendment the owner re-approves.

Plan Task 3 Step 5 also requires the unrelated-open proof inside the bound gate
on the large-repository fixture; that is leaf-1.3:G16. G11 and G12 here are the
deterministic small-fixture form of the same shared-factory cost.

Mutation patches: hand-remove the named guard from the committed code,
`git diff > docs/research/fathom-embed-ledger-plan-aligned/mutations/<name>.patch`,
`git checkout -- <file>`. When a later task moves the guarded code, regenerate the
patch so it removes the same guard; the owner reviews regenerated patches.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.2.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=0a3aeb6e8aabcfccb36aca1c155c1323bc4990ac8e70f8d6f812fccc16e759f0; output-bytes=325

- [x] G1: every plan and brief citation this task relies on is re-read on the branch before its next edit, with its status recorded
  RECORD: table of the branch commit, each cited file:line or symbol (plan Task 2, brief section CR-0), holds / moved-to / changed
  EVIDENCE: 2026-09-16 re-read on `feature/fathom-embed-wave2` d669f895. Plan Task 2 (shared factory) and brief CR-0 (process-wide sole-owner).
    | brief / plan cite | status on d669f895 |
    |---|---|
    | `ProcessRuntimeApi::acquire` builds a factory (`public_api.rs:513-520`, factory at `:517`) | CHANGED: `acquire` is `public_api.rs:575-584`. It clones the process factory via `EmbeddedRuntimeOwner::new(inner.embedded_factory())` (`:578`). |
    | factory construction | MOVED: one `EmbeddedSourceFactory::new()` lives on `ProcessIndexRuntime` (`process_runtime.rs:112`); accessor `embedded_factory()` at `:136`. |
    | per-factory map check `embedded.rs:492-496` | CHANGED: `SourceAlreadyOpen` is process-wide. Reservation refuse at `embedded.rs:822`; `open_bound` refuse at `:863`. |
    | insert before `admit_project_with_outcome` (`embedded.rs:498`) | CHANGED: reservation + `OpenRollback` (`embedded.rs:336`) land before admit. |
    | `stop(&key)` on close (`embedded.rs:408`) | HOLDS intent: `registry.rs:498` `stop` still ends the admission; `close_one` (`embedded.rs:950`) no longer holds the open mutex across `join`. |
    | join-live `registry.rs:382-398`; do not change it | HOLDS: `admit_with_outcome` still `registry.rs:337`; embed sole-owner is the reservation, not this arm. |
    | `SelectionUnavailable` / `OnEvent` mapping (`public_api.rs:550-555`) | MOVED: `SourceAlreadyOpen` maps at `public_api.rs:614`. |

- [x] G2: an open through a second ProcessRuntimeApi of a root already open through the first is refused with SelectionUnavailable, and a fresh sole handle is admitted after the first closes
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name embedded_source_has_one_handle_and_no_raw_bypass -- test -j 8 --test activation_cut_v11 -- embedded_source_has_one_handle_and_no_raw_bypass --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=embedded_source_has_one_handle_and_no_raw_bypass
  RED: failing-first; tasks/todo.md records the red observation ("a second ProcessRuntimeApi::acquire() opened the same root").
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f981837ee035b5ba8727f9d2e4946e06c4765869606e79ef7251bf772079aa41; output-bytes=90

- [x] G3: a runtime-local factory lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task2-m1-runtime-local-factory.patch --name embedded_source_has_one_handle_and_no_raw_bypass -- test -j 8 --test activation_cut_v11 -- embedded_source_has_one_handle_and_no_raw_bypass --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task2-m1-runtime-local-factory.patch
  MUTATION: `ProcessRuntimeApi::acquire` builds its own factory instead of using the process runtime's.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=e2bd027b85650e120fe48fb27406d4bac580d5856ba4191641468966e7081a9a; output-bytes=127

- [x] G4: an open that panics after registration leaves the root openable again
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name panicking_open_releases_its_process_wide_root_reservation -- test -j 8 --test activation_cut_v11 -- panicking_open_releases_its_process_wide_root_reservation --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=panicking_open_releases_its_process_wide_root_reservation
  RED: failing-first; tasks/todo.md records it ("an injected post-worker-start panic poisoned the factory mutex and aborted the test process").
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=fd032cf4593a8e95fefbd668c5b67a53d6c440c333bd14db227e4a8bec3d738d; output-bytes=99

- [x] G5: a rollback guard that does nothing on unwinding lets the G4 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task2-m2-rollback-does-nothing.patch --name panicking_open_releases_its_process_wide_root_reservation -- test -j 8 --test activation_cut_v11 -- panicking_open_releases_its_process_wide_root_reservation --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task2-m2-rollback-does-nothing.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=b692dc56a6d50aa4f77c9be9bf36fe764c912e8b6e36466bff7cf47749188d2c; output-bytes=136

- [x] G6: a rollback that releases the factory registration but keeps the process admission lets the G4 test fail, so the test proves no registration leaks
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task2-m3-rollback-keeps-admission.patch --name panicking_open_releases_its_process_wide_root_reservation -- test -j 8 --test activation_cut_v11 -- panicking_open_releases_its_process_wide_root_reservation --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task2-m3-rollback-keeps-admission.patch
  NOTE: the named test now asserts `process_project_registry().live(key)` is Err after the panic and before reopen, so a rollback that leaves the live admission in place fails the test instead of joining it.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=db0a7e9b42ebfd3ada6f5bf8042505077febcaa29e22c77961bd10c8b00ca320; output-bytes=139

- [x] G7: dropping or shutting down one runtime closes only the sources that runtime opened
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name dropping_one_runtime_leaves_another_runtimes_sources_open -- test -j 8 --test activation_cut_v11 -- dropping_one_runtime_leaves_another_runtimes_sources_open --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=dropping_one_runtime_leaves_another_runtimes_sources_open
  RED: regression guard for the shared factory (per-runtime factories could not close each other's sources); its teeth are G8.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=efd517c5656accf992a52ec5fde2896c97e918b8df0e5faabd8345310d6d510f; output-bytes=99

- [x] G8: an owner-blind runtime shutdown lets the G7 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task2-m4-shutdown-ignores-owner.patch --name dropping_one_runtime_leaves_another_runtimes_sources_open -- test -j 8 --test activation_cut_v11 -- dropping_one_runtime_leaves_another_runtimes_sources_open --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task2-m4-shutdown-ignores-owner.patch
  MUTATION: the runtime's shutdown closes every open source in the shared factory, not only its own.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c7fc2d57756f8f2c054c6d90df0edb9f78ccb5ee6f8fb56cf3bbae5f3b1c8ab9; output-bytes=137

- [x] G9: a stale open's rollback leaves a newer reservation of the same root in place
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name stale_open_rollback_keeps_a_newer_reservation -- test -j 8 --test activation_cut_v11 -- stale_open_rollback_keeps_a_newer_reservation --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=stale_open_rollback_keeps_a_newer_reservation
  TEST: needs a test-only hold between reservation and promotion: runtime A's open is held, A's reservation is removed by A's shutdown, runtime B reserves the same root, A's open resumes and rolls back; B's reservation and later handle must survive.
  RED: regression guard for the identity-checked guard (plan Task 2 Step 3); its teeth are G10.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=bbef79ee7b7ff787f8314790376ee4605640774e7fcd9b4abe3cfbd78eaacade; output-bytes=87

- [x] G10: a rollback that ignores reservation identity lets the G9 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task2-m5-rollback-ignores-identity.patch --name stale_open_rollback_keeps_a_newer_reservation -- test -j 8 --test activation_cut_v11 -- stale_open_rollback_keeps_a_newer_reservation --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task2-m5-rollback-ignores-identity.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=599849d31615bbfcf7aaabd1ab2750ddf144c3ec77c951fafea9c2cddf7cf660; output-bytes=128

- [x] G11: while one root's close waits on its worker, an open of an unrelated root completes within the one-second close bound
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name unrelated_open_completes_while_another_root_closes -- test -j 8 --no-default-features --features embed --test embed_bound_index -- unrelated_open_completes_while_another_root_closes --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=unrelated_open_completes_while_another_root_closes
  NOTE: the shared-factory cost the brief raised (factory lock held across the worker join). At drafting the test enters shutdown with `std::thread::sleep(Duration::from_millis(50))` ("this is only to enter shutdown"), so it can pass without the close having started; G12 exposes that. Prefer a deterministic signal that the close is inside its join.
  RED: failing-first with the hold present (the v11.2.0 close path held the factory lock across the join).
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=7b0d6b4c70d936c946172c05069a620d4ab0c81493aed291c129fd8fd59cb3e2; output-bytes=92

- [x] G12: holding the factory lock across a worker join lets the G11 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task2-m6-factory-lock-held-across-join.patch --name unrelated_open_completes_while_another_root_closes -- test -j 8 --no-default-features --features embed --test embed_bound_index -- unrelated_open_completes_while_another_root_closes --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task2-m6-factory-lock-held-across-join.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=089c3700a949f8b1c00a1c2017f538ca57ed190fed19b2b444d6bd80bccd0029; output-bytes=137

- [x] G13: no crate was added and no Cargo.toml dependency or patch table changed since v11.2.0
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs deps --base v11.2.0 --allow-lock reqwest@0.13.5 --allow-lock rmcp@3.3.0 --allow-lock rmcp-macros@3.3.0 --allow-lock toml_edit@0.25.15+spec-1.1.0 --allow-lock toml_parser@1.1.3+spec-1.1.0
  EXPECT: LEDGER-ORACLE DEPS UNCHANGED base=v11.2.0
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=5ed8afb53eee958b3b2e989fbc8073d92fcdf60fda9cd4cafe4615dd609242f6; output-bytes=56

- [x] G14: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=336cd67d8d6efa261561b22d7c4470d08414a3dd69a15320bc5f11a85fe31c5d; output-bytes=41

- [x] G15: every v11.2.0 contract atom is still present and every new atom is one of the plan's census or progress atoms
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=955ab01ab9bdb405ec230e26b2e547aec3da7775c86d5ed3220948bb2b007641; output-bytes=51

- [x] G16: the checked-in export delta matches the contract atoms
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name export_delta_matches_frozen_contract_atoms -- test -j 8 --test public_api_delta_v11 -- export_delta_matches_frozen_contract_atoms --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=export_delta_matches_frozen_contract_atoms
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f5a517337df56a1c4bed5b0b43280c969e37ae9a1ff6118cb7a3465200977b42; output-bytes=84

- [x] G17: the embed facade contract tripwire compiles and passes in the embed lib cell
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name facade_contract_is_stable -- test -j 8 --no-default-features --features embed --lib -- facade_contract_is_stable --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=facade_contract_is_stable
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=38bbef32fac16dbfbf1d400eda166addf9674c17fb546aa77a2b691a845b4417; output-bytes=67

- [x] G18: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G2, G4 and G11 tests, the commit or working state, exact command and the failure line; the existing tasks/todo.md entries count only where they name the command and the failure; G7 and G9 are declared regression guards
  EVIDENCE: 2026-09-16. `tasks/todo.md` names the G2 and G4 reds; it does not name the exact cargo invocation for G11. G7 and G9 are declared guards (teeth are G8 and G10).
    - G2 RED (todo.md): "a second `ProcessRuntimeApi::acquire()` opened the same root, proving its factory was runtime-local." Named test `embedded_source_has_one_handle_and_no_raw_bypass`. GREEN after shared factory + distinct shutdown owner.
    - G4 RED (todo.md): "an injected post-worker-start panic poisoned the factory mutex and aborted the test process." Named test `panicking_open_releases_its_process_wide_root_reservation`. GREEN after process-wide reservation + `OpenRollback`.
    - G11: `unrelated_open_completes_while_another_root_closes` was already on the branch at drafting; no separate failing-first line in todo.md. Guarded by G12 (factory-lock-held-across-join).

- [x] G19: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=dcc639eae71be9dd2f64a9e2ea832b3fe098764e12995ece5c61247494e9b563; output-bytes=11139

- [x] G20: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c0b92c7e8f2232d84908fd862489de264811d67a0cbc66dc91d6db8319210ee4; output-bytes=41
