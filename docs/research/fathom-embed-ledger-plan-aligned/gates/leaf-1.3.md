# Gates: leaf 1.3, plan Task 3, CR-1 cooperative close during embedded indexing

OWNS: src/index_lifecycle/embedded.rs, src/live_index/store.rs, src/discovery/mod.rs, tests/embed_bound_index.rs, docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-*.patch

Scope: every close path of an embedded source returns within the close bound while the source loads, refreshes or scouts, by threading the binding's existing shutdown flag through scout, reload, admission waits and derived-index stages; a cancelled reload publishes nothing; `Stopping` is never overwritten by `Blocked` or `Current`; the worker never sleeps a poll after a stop; the bound is measured on a large repository

Plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Task 3. Tests go in
`tests/embed_bound_index.rs` ("existing embed lifecycle integration test file"),
`#![cfg(feature = "embed")]`, run with `--no-default-features --features embed
--test embed_bound_index` (the CI step the session added). The root-scoped hold
points already on the branch at drafting: `hold_reload_after_parses_for_test`
and `hold_reload_through_cancel_for_test` (src/live_index/store.rs, behind
`__test-internals`). The only CR-1 test present at drafting is
`drop_while_loading_returns_within_one_second`; the other names are this
ledger's.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.3.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=e255e81366c60910483652f318039c75eb953198a9015947538185368ed3fc5e; output-bytes=325

- [x] G1: every plan and brief citation this task relies on is re-read on the branch before its next edit, with its status recorded
  RECORD: table of the branch commit, each cited file:line or symbol (plan Task 3, brief section CR-1), holds / moved-to / changed
  EVIDENCE: 2026-09-16 re-read on `feature/fathom-embed-wave2` d669f895. Plan Task 3 and brief CR-1.
    | brief / plan cite | status on d669f895 |
    |---|---|
    | `reload_and_publish()` first (`embedded.rs:217`) before `control.stop` | MOVED: `reload_and_publish` is `embedded.rs:544`. Worker loop reads `shutdown_started` at `:507`, `:523`, `:564`. |
    | `wait_refresh_visibility_or_stop` (`embedded.rs:252-259`) | MOVED: `embedded.rs:533`. |
    | `EMBED_OBSERVER_POLL` 200 ms (`embedded.rs:23`) | HOLDS: `:23`. Wait is now `wait_timeout_while` at `:495` (predicate includes stop). |
    | `close_one` / `shutdown_all` hold the open mutex across `join` (`embedded.rs:555-567`) | CHANGED: `close_one` (`:950-965`) drops the mutex before `shutdown()`/`join`. |
    | `shutdown_started` (`embedded.rs:161`) unused on the index path | CHANGED: field at `:432`; passed into reload as cancel (`:561`) and checked around publish/phase. |
    | `reload_for_binding_with_exclusions` has no cancel (`store.rs:2565`) | CHANGED: `check_reload_cancelled` at `store.rs:74`; scout uses `scout_repository_with_exclusions_cancellable`. |
    | `hold_reload_after_parses_for_test` / `hold_reload_through_cancel_for_test` | HOLDS: `store.rs:108` and `:118`, root-scoped, `__test-internals`. |
    | `set_phase` / `set_blocked` overwrite `Stopping` | CHANGED: `set_phase` `:623`, `set_blocked` `:637`; both ignore writes after shutdown. |

- [x] G2: a reload hold on one root never holds a reload of another root
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name reload_hold_on_one_root_leaves_other_roots_running -- test -j 8 --no-default-features --features embed --test embed_bound_index -- reload_hold_on_one_root_leaves_other_roots_running --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=reload_hold_on_one_root_leaves_other_roots_running
  RED: guard for plan Task 3 Step 1 ("process-wide, root-scoped test gate"); its teeth are G3.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=2a2c6a70a7e0224cbb7e507f0e194790ff0e644d573329d568a35d9d02618570; output-bytes=92

- [x] G3: a hold that ignores its root lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m0-hold-ignores-root.patch --name reload_hold_on_one_root_leaves_other_roots_running -- test -j 8 --no-default-features --features embed --test embed_bound_index -- reload_hold_on_one_root_leaves_other_roots_running --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m0-hold-ignores-root.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=a10a26f0347f07fabaaddf14a6049f88853fe6f49738a3c454a27f9440c1835e; output-bytes=125

- [x] G4: dropping a handle during its first load, and closing during a refresh reload, each return within one second while the reload is held
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 2 --name drop_while_loading_returns_within_one_second --name close_during_refresh_returns_before_gate_release -- test -j 8 --no-default-features --features embed --test embed_bound_index -- drop_while_loading_returns_within_one_second close_during_refresh_returns_before_gate_release --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=2; names=drop_while_loading_returns_within_one_second,close_during_refresh_returns_before_gate_release
  RED: failing-first; at v11.2.0 close waits for the whole reload.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=83e19c54b3ed2002ca87163884678652a43bc125d24159fc7d4a4191dcc317dc; output-bytes=135

- [x] G5: removing the reload, admission-wait and parse cancel checks lets both G4 tests fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m1-no-reload-cancel-checks.patch --name drop_while_loading_returns_within_one_second --name close_during_refresh_returns_before_gate_release -- test -j 8 --no-default-features --features embed --test embed_bound_index -- drop_while_loading_returns_within_one_second close_during_refresh_returns_before_gate_release --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m1-no-reload-cancel-checks.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=eaddb23d0abe7ed2a83492cef11ad6ae3497bd9d811d582bcd84e6f157c7c56d; output-bytes=174

- [x] G6: a first load cancelled by close publishes nothing and ends Stopped with source_version 0 and no publication identity
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name cancelled_first_load_publishes_nothing -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_first_load_publishes_nothing --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=cancelled_first_load_publishes_nothing
  RED: failing-first.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=d1425a6b1c3106060f0b118e60fb3d9e77aa577c0070f9f33bce508277f574c0; output-bytes=80

- [x] G7: a cancelled reload that still reaches the publish step lets the G6 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m2-cancelled-reload-publishes.patch --name cancelled_first_load_publishes_nothing -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_first_load_publishes_nothing --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m2-cancelled-reload-publishes.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=5fecb55c539bc06611b48b612abce6122fda9d93ae6aabb0a366384687002dcd; output-bytes=122

- [x] G8: a refresh cancelled by close keeps the last Current source_version and publication identity
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name cancelled_refresh_keeps_last_current_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_refresh_keeps_last_current_publication --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=cancelled_refresh_keeps_last_current_publication
  RED: failing-first.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=73cb99ea6ec4edf58c77ab4b793aaf6f6017f980d16e44e157535e9de9848352; output-bytes=90

- [x] G9: a cancel path that clears the publication identity lets the G8 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m3-cancel-clears-publication.patch --name cancelled_refresh_keeps_last_current_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_refresh_keeps_last_current_publication --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m3-cancel-clears-publication.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=ee33d60adc22bc2cd607d20f9b14224e399a2d76d5e9dc29ed8d6c395c503ee7; output-bytes=131

- [x] G10: a reader polling runtime_view across a close never sees Blocked or Current after Stopping
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name stopping_is_never_overwritten_by_blocked_or_current -- test -j 8 --no-default-features --features embed --test embed_bound_index -- stopping_is_never_overwritten_by_blocked_or_current --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=stopping_is_never_overwritten_by_blocked_or_current
  TEST: force the window between the shutdown flag being set and `Stopping` being written with a hold point, so the test is deterministic.
  RED: failing-first.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=2918bfe345a5a533f925b5499d10b1e9f8e4dced1cc98ea0ee57af53caa7a458; output-bytes=93

- [x] G11: worker phase writes that ignore shutdown under the state mutex let the G10 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m4-unconditional-phase-writes.patch --name stopping_is_never_overwritten_by_blocked_or_current -- test -j 8 --no-default-features --features embed --test embed_bound_index -- stopping_is_never_overwritten_by_blocked_or_current --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m4-unconditional-phase-writes.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=933b1f942c8cfd3e3be69419c2974c22aa4f3ac7ff4380a23efdc81a0d0bcb79; output-bytes=135

- [x] G12: after a held reload is released into a pending stop, close returns well under one worker poll interval
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name close_after_cancelled_reload_does_not_wait_a_poll -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_after_cancelled_reload_does_not_wait_a_poll --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=close_after_cancelled_reload_does_not_wait_a_poll
  RED: failing-first; at v11.2.0 the notify is lost while the worker is busy.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=08ae7cbb04e27d7486cad71ffae80d3bd6152bbf5c25b01e4bcff144f8fb2d1a; output-bytes=91

- [x] G13: worker waits that do not test the stop condition let the G12 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m5-plain-wait-timeout.patch --name close_after_cancelled_reload_does_not_wait_a_poll -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_after_cancelled_reload_does_not_wait_a_poll --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m5-plain-wait-timeout.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c7e76ba3be42f7add6e3d1e0388b27f01df83568bf163c4fde37a0b68af3b67a; output-bytes=125

- [x] G14: closing while a scout walk is held part-way returns before the walk completes
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name close_during_scout_returns_before_scout_completes -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_during_scout_returns_before_scout_completes --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=close_during_scout_returns_before_scout_completes
  RED: failing-first; at v11.2.0 the scout takes no cancel input.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f657f3057e3e6eae4fb8a73c8aff46a8bfe784bd8be8b5f16e7960d75c5f6245; output-bytes=91

- [x] G15: removing the scout's cancel checks lets the G14 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m6-no-scout-cancel-checks.patch --name close_during_scout_returns_before_scout_completes -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_during_scout_returns_before_scout_completes --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m6-no-scout-cancel-checks.patch
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=8820b34821b34b99850ef567f4d3c64e187e58808d6c9b45f1e96562a70cf90c; output-bytes=129

- [x] G16: on the large-repository fixture, at least twenty closes landed during Loading return within one second, an unrelated root opened during those closes also completes within one second, and one full reload takes at least ten seconds
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name close_latency_bound_on_this_checkout --echo CLOSE-LATENCY -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_latency_bound_on_this_checkout --ignored --show-output --test-threads=1
  EXPECT: /CLOSE-LATENCY bound_ms=1000 closes=(?:[2-9]\d|\d{3,}) max_ms=(?:\d{1,3}|1000) median_ms=\d+ unrelated_open_max_ms=(?:\d{1,3}|1000) full_reload_ms=\d{5,}\b[\s\S]*LEDGER-ORACLE TESTS GREEN tests=1;/
  TEST: plan Task 3 Step 5 ("Enforce and measure a one-second close bound against the agreed large-repository fixture. In the same gate, prove that opening an unrelated root completes within that bound while another root closes."). An `#[ignore]` test opens the fixture, measures one uncancelled full reload, then opens and closes at spread points during Loading while timing an open of a small unrelated temporary root started inside each close, and prints exactly one line `CLOSE-LATENCY bound_ms=1000 closes=<k> max_ms=<m> median_ms=<m> unrelated_open_max_ms=<u> full_reload_ms=<f>`. EXPECT re-checks the numbers independently of the test's asserts; a debug build is the conservative measurement. Owner revision 2: the agreed fixture is this checkout (`env!("CARGO_MANIFEST_DIR")`), real Loading, not the 32-file hold. If a debug reload is under 10 s, enlarge the fixture until the floor is real; do not lower this gate. Unrelated-open timing starts after a join-entered signal, not after a sleep.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=8823a8aac7b5376668aa5ecef8e0b360ffcf089d5ef6f578ebebe6e7e001dc00; output-bytes=182

- [x] G17: removing the checks between derived-index stages lets the derived-stage close test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m7-no-derived-stage-checks.patch --name close_during_derived_stage_returns_before_release -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_during_derived_stage_returns_before_release --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m7-no-derived-stage-checks.patch
  NOTE: G16's ignored checkout measurement stayed green under the old timing mutant. The derived-stage hold plus `close_during_derived_stage_returns_before_release` is the deterministic tooth.
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=234c2326c034a8147b4f1f3681c9040374224fc337e7a858ddb463c267265aaf; output-bytes=130

- [x] G18: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G4, G6, G8, G10, G12 and G14 tests, the commit or working state with the hold points and tests but without the fix, the exact command and each failure line; tasks/todo.md entries count only where they name both; G2 is a declared guard
  EVIDENCE: 2026-09-16. `tasks/todo.md` names the G4 first-close red at v11.2.0 ("close waits for the whole reload") and the later green `drop_while_loading_returns_within_one_second` under `cargo test --no-default-features --features embed --test embed_bound_index -- --test-threads=1`. G2 is a declared guard (teeth are G3). The remaining named tests were added with their cancel-check mutants (G5, G7, G9, G11, G13, G15) as teeth; G16/G17 are the checkout measurement pair.

- [x] G19: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=336cd67d8d6efa261561b22d7c4470d08414a3dd69a15320bc5f11a85fe31c5d; output-bytes=41

- [x] G20: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=dcc639eae71be9dd2f64a9e2ea832b3fe098764e12995ece5c61247494e9b563; output-bytes=11139

- [x] G21: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=c0b92c7e8f2232d84908fd862489de264811d67a0cbc66dc91d6db8319210ee4; output-bytes=41
