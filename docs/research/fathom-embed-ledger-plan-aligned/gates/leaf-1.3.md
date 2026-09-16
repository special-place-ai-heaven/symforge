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

- [ ] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.3.md
  EXPECT: LINT OK
  EVIDENCE: pending

- [ ] G1: every plan and brief citation this task relies on is re-read on the branch before its next edit, with its status recorded
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

- [ ] G2: a reload hold on one root never holds a reload of another root
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name reload_hold_on_one_root_leaves_other_roots_running -- test -j 8 --no-default-features --features embed --test embed_bound_index -- reload_hold_on_one_root_leaves_other_roots_running --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=reload_hold_on_one_root_leaves_other_roots_running
  RED: guard for plan Task 3 Step 1 ("process-wide, root-scoped test gate"); its teeth are G3.
  EVIDENCE: pending

- [ ] G3: a hold that ignores its root lets the G2 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m0-hold-ignores-root.patch --name reload_hold_on_one_root_leaves_other_roots_running -- test -j 8 --no-default-features --features embed --test embed_bound_index -- reload_hold_on_one_root_leaves_other_roots_running --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m0-hold-ignores-root.patch
  EVIDENCE: pending

- [ ] G4: dropping a handle during its first load, and closing during a refresh reload, each return within one second while the reload is held
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 2 --name drop_while_loading_returns_within_one_second --name close_during_refresh_returns_before_gate_release -- test -j 8 --no-default-features --features embed --test embed_bound_index -- drop_while_loading_returns_within_one_second close_during_refresh_returns_before_gate_release --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=2; names=drop_while_loading_returns_within_one_second,close_during_refresh_returns_before_gate_release
  RED: failing-first; at v11.2.0 close waits for the whole reload.
  EVIDENCE: pending

- [ ] G5: removing the reload, admission-wait and parse cancel checks lets both G4 tests fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m1-no-reload-cancel-checks.patch --name drop_while_loading_returns_within_one_second --name close_during_refresh_returns_before_gate_release -- test -j 8 --no-default-features --features embed --test embed_bound_index -- drop_while_loading_returns_within_one_second close_during_refresh_returns_before_gate_release --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m1-no-reload-cancel-checks.patch
  EVIDENCE: pending

- [ ] G6: a first load cancelled by close publishes nothing and ends Stopped with source_version 0 and no publication identity
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name cancelled_first_load_publishes_nothing -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_first_load_publishes_nothing --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=cancelled_first_load_publishes_nothing
  RED: failing-first.
  EVIDENCE: pending

- [ ] G7: a cancelled reload that still reaches the publish step lets the G6 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m2-cancelled-reload-publishes.patch --name cancelled_first_load_publishes_nothing -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_first_load_publishes_nothing --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m2-cancelled-reload-publishes.patch
  EVIDENCE: pending

- [ ] G8: a refresh cancelled by close keeps the last Current source_version and publication identity
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name cancelled_refresh_keeps_last_current_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_refresh_keeps_last_current_publication --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=cancelled_refresh_keeps_last_current_publication
  RED: failing-first.
  EVIDENCE: pending

- [ ] G9: a cancel path that clears the publication identity lets the G8 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m3-cancel-clears-publication.patch --name cancelled_refresh_keeps_last_current_publication -- test -j 8 --no-default-features --features embed --test embed_bound_index -- cancelled_refresh_keeps_last_current_publication --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m3-cancel-clears-publication.patch
  EVIDENCE: pending

- [ ] G10: a reader polling runtime_view across a close never sees Blocked or Current after Stopping
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name stopping_is_never_overwritten_by_blocked_or_current -- test -j 8 --no-default-features --features embed --test embed_bound_index -- stopping_is_never_overwritten_by_blocked_or_current --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=stopping_is_never_overwritten_by_blocked_or_current
  TEST: force the window between the shutdown flag being set and `Stopping` being written with a hold point, so the test is deterministic.
  RED: failing-first.
  EVIDENCE: pending

- [ ] G11: worker phase writes that ignore shutdown under the state mutex let the G10 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m4-unconditional-phase-writes.patch --name stopping_is_never_overwritten_by_blocked_or_current -- test -j 8 --no-default-features --features embed --test embed_bound_index -- stopping_is_never_overwritten_by_blocked_or_current --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m4-unconditional-phase-writes.patch
  EVIDENCE: pending

- [ ] G12: after a held reload is released into a pending stop, close returns well under one worker poll interval
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name close_after_cancelled_reload_does_not_wait_a_poll -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_after_cancelled_reload_does_not_wait_a_poll --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=close_after_cancelled_reload_does_not_wait_a_poll
  RED: failing-first; at v11.2.0 the notify is lost while the worker is busy.
  EVIDENCE: pending

- [ ] G13: worker waits that do not test the stop condition let the G12 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m5-plain-wait-timeout.patch --name close_after_cancelled_reload_does_not_wait_a_poll -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_after_cancelled_reload_does_not_wait_a_poll --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m5-plain-wait-timeout.patch
  EVIDENCE: pending

- [ ] G14: closing while a scout walk is held part-way returns before the walk completes
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name close_during_scout_returns_before_scout_completes -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_during_scout_returns_before_scout_completes --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=close_during_scout_returns_before_scout_completes
  RED: failing-first; at v11.2.0 the scout takes no cancel input.
  EVIDENCE: pending

- [ ] G15: removing the scout's cancel checks lets the G14 test fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m6-no-scout-cancel-checks.patch --name close_during_scout_returns_before_scout_completes -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_during_scout_returns_before_scout_completes --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m6-no-scout-cancel-checks.patch
  EVIDENCE: pending

- [ ] G16: on the large-repository fixture, at least twenty closes landed during Loading return within one second, an unrelated root opened during those closes also completes within one second, and one full reload takes at least ten seconds
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name close_latency_bound_on_this_checkout --echo CLOSE-LATENCY -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_latency_bound_on_this_checkout --ignored --show-output --test-threads=1
  EXPECT: /CLOSE-LATENCY bound_ms=1000 closes=(?:[2-9]\d|\d{3,}) max_ms=(?:\d{1,3}|1000) median_ms=\d+ unrelated_open_max_ms=(?:\d{1,3}|1000) full_reload_ms=\d{5,}\b[\s\S]*LEDGER-ORACLE TESTS GREEN tests=1;/
  TEST: plan Task 3 Step 5 ("Enforce and measure a one-second close bound against the agreed large-repository fixture. In the same gate, prove that opening an unrelated root completes within that bound while another root closes."). An `#[ignore]` test opens the fixture, measures one uncancelled full reload, then opens and closes at spread points during Loading while timing an open of a small unrelated temporary root started inside each close, and prints exactly one line `CLOSE-LATENCY bound_ms=1000 closes=<k> max_ms=<m> median_ms=<m> unrelated_open_max_ms=<u> full_reload_ms=<f>`. EXPECT re-checks the numbers independently of the test's asserts; a debug build is the conservative measurement. Owner revision 2: the agreed fixture is this checkout (`env!("CARGO_MANIFEST_DIR")`), real Loading, not the 32-file hold. If a debug reload is under 10 s, enlarge the fixture until the floor is real; do not lower this gate. Unrelated-open timing starts after a join-entered signal, not after a sleep.
  EVIDENCE: pending

- [ ] G17: removing the checks between derived-index stages lets the G16 measurement fail
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs mutant --patch docs/research/fathom-embed-ledger-plan-aligned/mutations/task3-m7-no-derived-stage-checks.patch --name close_latency_bound_on_this_checkout -- test -j 8 --no-default-features --features embed --test embed_bound_index -- close_latency_bound_on_this_checkout --ignored --test-threads=1
  EXPECT: LEDGER-ORACLE MUTANT CAUGHT patch=task3-m7-no-derived-stage-checks.patch
  NOTE: timing-based. If the derived-index tail on this checkout is shorter than the bound in a debug build, this mutant survives honestly; then add a deterministic hold between derived stages with its own test, amend this gate with the owner, and do not weaken G16.
  EVIDENCE: pending

- [ ] G18: each failing-first test of this task was observed red before its fix, with command and failure recorded
  RECORD: for the G4, G6, G8, G10, G12 and G14 tests, the commit or working state with the hold points and tests but without the fix, the exact command and each failure line; tasks/todo.md entries count only where they name both; G2 is a declared guard
  EVIDENCE: pending

- [ ] G19: no existing public item, variant, field or re-export changed since v11.2.0 and every addition is a plan-named atom or test-only
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: pending

- [ ] G20: the binding verification cells pass at this task's HEAD
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [ ] G21: the delivered state is committed, with no uncommitted tracked change
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label committed -- git diff --quiet HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=committed;
  EVIDENCE: pending
