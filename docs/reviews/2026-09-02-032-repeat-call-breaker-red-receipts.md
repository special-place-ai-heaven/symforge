# Feature 032 — RED-first receipts and reporting-invariant answers

Evidence record for `specs/032-repeat-call-breaker/` (Constitution II and I). Each oracle below was observed failing before its machinery existed — first as a compile refusal where the test names new items, then at runtime against a degenerate stub — and then observed green. Receipts are verbatim excerpts of the test output captured by the implementers on 2026-09-02 (Windows, `-j 4`, `--test-threads=1`). The full battery (Constitution IV) is run separately by the orchestrator and recorded in the PR.

## US1 — Repeat-call notice

Implementation: `src/protocol/repeat.rs` (new), `src/protocol/mod.rs` seam, `src/protocol/result_status.rs`, `src/idempotency.rs` (`Hash` derive), `src/server/mcp_http.rs` (config pin), `scripts/verify-tools.cjs` (probe fingerprint), `tests/repeat_notice.rs` (new).

### Oracle map

| Test | Quickstart oracle |
|---|---|
| `tests/repeat_notice.rs::third_identical_eligible_call_carries_notice_and_first_two_do_not` | 1 (SC-001, SC-004 byte-stability, `isError` untouched) |
| `tests/repeat_notice.rs::index_change_between_repeats_resets_run` | 2 (watcher republish observed; notice returns at the new run's count 3) |
| `tests/repeat_notice.rs::ineligible_tools_never_notice` | 4 (`status`, `what_changed`, `get_symbol`; eligible control) |
| `tests/repeat_notice.rs::sessions_never_share_runs` | 8 (lane-inertness pin over in-process HTTP `/mcp` with bound, equal evidence; stdio positive control) |
| `tests/repeat_notice.rs::projects_argument_never_accumulates` | 9 (evidence is the `bound:false` marker; single-project control) |
| `tests/repeat_notice.rs::every_eligible_tool_is_byte_stable_and_notices_on_third_serve` | R4 eligibility pin across all five tools (property pin; no reachable RED on this code) |
| `src/protocol/repeat.rs::tests::eligible_list_is_pinned` | 5 |
| `src/protocol/repeat.rs::tests::unobserved_evidence_clears_run` | 3 |
| `src/protocol/repeat.rs::tests::internal_failure_clears_run` | 6 (+ InvalidRequest/NotFound/EmptyResult advance; no-status + `isError` clears) |
| `src/protocol/repeat.rs::tests::tracker_cap_clears_without_false_claim` | 7 |
| `src/protocol/repeat.rs::tests::witness_requires_full_equality`, `notice_threshold_is_three_and_count_saturates` | 10 |
| `src/protocol/repeat.rs::tests::http_inert_lane_and_projects_fan_out_never_key`, `notice_attaches_to_final_text_block_and_meta` | structural non-keys; delivery shape |
| `src/protocol/result_status.rs::tests::observed_outcome_class_reads_only_outcome_class_and_tolerates_unknown_fields` | research.md R11 seam note (f) |
| `src/server/mcp_http.rs::tests::mcp_service_config_pins_the_sessionless_lane` | regression guard for the T001 lane decision (no reachable RED) |

### Receipts

Integration oracles, RED against pre-feature code (`cargo test --test repeat_notice`, exit 101):

```
test index_change_between_repeats_resets_run ... FAILED
test ineligible_tools_never_notice ... FAILED
test projects_argument_never_accumulates ... FAILED
test sessions_never_share_runs ... FAILED
test third_identical_eligible_call_carries_notice_and_first_two_do_not ... FAILED
thread 'third_identical_eligible_call_carries_notice_and_first_two_do_not' panicked at tests\repeat_notice.rs:271:28:
serve 3: must carry symforge/repeat_notice: {"_meta":{"symforge/project_evidence":{...,"generation":6,...},"symforge/result_status":{...}},"content":[{"text":"Trust: heuristic (substring tier) | current index | ...
test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.86s
```

Unit oracles, RED as a compile refusal (`cargo test --lib ... repeat`, exit 101):

```
error[E0432]: unresolved import `crate::protocol::result_status::REPEAT_NOTICE_META_KEY`
error[E0425]: cannot find type `RepeatKey` in this scope
error[E0425]: cannot find type `ServeObservation` in this scope
error[E0425]: cannot find type `RepeatTracker` in this scope
error[E0425]: cannot find type `RepeatNotice` in this scope
error[E0425]: cannot find value `REPEAT_ELIGIBLE_TOOLS` in this scope
error[E0425]: cannot find value `NOTICE_THRESHOLD` in this scope
error[E0425]: cannot find value `REPEAT_TRACKER_MAX_ENTRIES` in this scope
```

Unit oracles, RED at runtime against the degenerate stub (witness never observes, notice never constructs, tracker stateless, reader returns `None`; exit 101):

```
test internals::protocol::repeat::tests::eligible_list_is_pinned ... ok
test internals::protocol::repeat::tests::http_inert_lane_and_projects_fan_out_never_key ... ok
test internals::protocol::repeat::tests::internal_failure_clears_run ... FAILED
test internals::protocol::repeat::tests::notice_attaches_to_final_text_block_and_meta ... FAILED
test internals::protocol::repeat::tests::notice_threshold_is_three_and_count_saturates ... FAILED
test internals::protocol::repeat::tests::tracker_cap_clears_without_false_claim ... FAILED
test internals::protocol::repeat::tests::unobserved_evidence_clears_run ... FAILED
test internals::protocol::repeat::tests::witness_requires_full_equality ... FAILED
test internals::protocol::result_status::tests::observed_outcome_class_reads_only_outcome_class_and_tolerates_unknown_fields ... FAILED
test internals::server::mcp_http::tests::mcp_service_config_pins_the_sessionless_lane ... ok
panicked at src\protocol\repeat.rs:321:9: equal evidence must be witnessable
panicked at src\protocol\repeat.rs:550:9: assertion `left == right` failed  left: None  right: Some(2)
panicked at src\protocol\result_status.rs:298:9: assertion `left == right` failed  left: None  right: Some(EmptyResult)
test result: FAILED. 13 passed; 7 failed; 0 ignored; 0 measured; 3296 filtered out; finished in 0.27s
```

Integration oracles, RED against the stub seam (tracker wired, never notices; exit 101) — every negative control passed, every positive control failed:

```
test index_change_between_repeats_resets_run ... FAILED
test ineligible_tools_never_notice ... FAILED              (control serve 3: must carry symforge/repeat_notice)
test projects_argument_never_accumulates ... FAILED        (control serve 3: must carry symforge/repeat_notice)
test sessions_never_share_runs ... FAILED                  (stdio control serve 3: must carry symforge/repeat_notice)
test third_identical_eligible_call_carries_notice_and_first_two_do_not ... FAILED (serve 3: must carry symforge/repeat_notice)
test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.62s
```

GREEN, unit (`cargo test --lib -j 4 -- --test-threads=1 repeat observed_outcome_class mcp_service_config`):

```
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 3296 filtered out; finished in 0.26s
```

GREEN, integration (`cargo test --test repeat_notice -j 4 -- --test-threads=1`; first the five original oracles over three consecutive runs, then the binary after the five-tool byte-stability pin was added; the index change in oracle 2 was observed through the real watcher every time):

```
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.45s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.97s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.98s
test every_eligible_tool_is_byte_stable_and_notices_on_third_serve ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.85s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.82s
```

Neighbours: `rmcp3_protocol` 15/15, `rmcp3_roots_interop` 1/1, `idempotency` 5/5.

### Reporting-invariant answers (US1)

- `_meta["symforge/repeat_notice"]` and the appended text observe a `RepeatWitness` (full struct equality between the typed `ProjectEvidence` stored at the run's first serve and the typed evidence deserialized from this response's own `_meta`), a count that only equal-witnessed serves can raise to 3, and an observed `Stdio` lane. When the observation fails, nothing is emitted: a `RepeatNotice` is unconstructible without a witness, and every unobserved path removes the run.
- `SessionDiscriminator::observe` observes the presence of rmcp's `http::request::Parts` in `RequestContext::extensions`. Presence is the inert lane (no interaction); absence is stdio. `mcp_http.rs` pins the sessionless posture of the only shared lane.
- `ServeObservation::from_result` observes this response's own evidence value and `outcome_class`; anything it cannot read yields `Unobserved`, never a default; an unreadable `outcome_class` with `isError: true` clears conservatively.
- `observed_outcome_class` reads only the `outcome_class` field under `symforge/result_status` in the enum's serde spelling and returns `None` for anything else (pinned).
- The tracker's cap clear loses only in-flight true notices; the next serve of any old key is count 1 (pinned).

### Deviations recorded by the implementer

- `eligible_list_is_pinned` lives in `src/protocol/repeat.rs` (no `internals.rs` export was needed).
- `RepeatNotice::new(witness, count, &RepeatKey)` takes the key rather than `(tool, hash)` separately.
- The stdio harness gates on `health` reporting `Watcher: active` plus a short settle before any 3× sequence, because the fresh-instance reconciliation republishes once shortly after startup (observed as generation 6→7); `capture_local_response_generation` fences the trust banner and the evidence with one `PublishedGeneration`, so this is harness timing, not a claim-safety gap.
- T009's premise did not hold as-is: `scripts/verify-tools.cjs` runs under `SYMFORGE_SURFACE=compact` and relays `search_symbols` through the `symforge` facade (ineligible), so the notice could not enter a snapshot today. The probe was still made fingerprint-distinct (`limit: 50`, the tool's default) as future-proofing; no case or snapshot changed.

## US2 — Ledger retry collapse

Implementation: `src/stel/ledger.rs`, `src/stel/status.rs`, `src/stel_core/ledger_store.rs`, `src/server/admin/api_v1.rs`, `src/server/admin/assets/app.js`, `tests/stel_status.rs`, `tests/admin_api_v1.rs`, `tests/admin_render.rs`.

### Oracle map

| Test | Oracle |
|---|---|
| `src/stel/ledger.rs::collapse_tests::collapse_runs_merges_only_strictly_consecutive_runs` | US2-1 (A,A,B,A → 2,1,1; both lanes) |
| `…::collapse_runs_counts_sum_to_input_length` | invariant 1 |
| `…::collapse_runs_flattening_reproduces_identity_sequence` | invariant 2 |
| `…::collapse_runs_all_distinct_input_keeps_every_count_one` | invariant 3 |
| `…::collapse_runs_ten_thousand_identical_events_collapse_to_one_run` | SC-003 status lane |
| `…::event_identity_ignores_exactly_the_six_measurement_fields` | data-model Lane A |
| `…::stored_record_identity_scopes_by_session_and_reads_widened_columns` | data-model Lane B |
| `src/stel_core/ledger_store.rs::tests::recent_and_samples_read_back_the_widened_columns` | T015 (both readers) |
| `tests/stel_status.rs::status_full_annotates_trailing_run` | US2-2 (×3 on the decision line only; single-event control byte-identical; ×10000) |
| `tests/admin_api_v1.rs::admin_recent_runs_collapses_and_scopes_by_session` | US2-3 (Disabled ⇒ `[]` + no window; FR-008 control; two sessions never merge) |
| `tests/admin_api_v1.rs::admin_window_edge_is_labeled` | US2-4 (60 → count 50 clipped; only the oldest run clipped; 5 rows → nothing clipped) |
| `src/server/admin/api_v1.rs::tests::ledger_run_view_withholds_unparseable_stored_arrays` | zero-false-claims on unparseable stored JSON |
| `tests/admin_render.rs::admin_page_references_endpoints_and_summary_has_real_values` (extended) | field-name pin + FR-008 render-side control |

### Receipts

T011 + T015 RED (compile refusal):

```
error[E0432]: unresolved imports `super::Run`, `super::collapse_runs`, `super::ledger_event_identity`, `super::stored_record_identity`
   --> src\stel\ledger.rs:372:17
error[E0609]: no field `degrade_flags_json` on type `&ledger_store::StoredLedgerRecord`
    --> src\stel_core\ledger_store.rs:1372:32
error[E0560]: struct `ledger_store::StoredLedgerRecord` has no field named `pff_bypass`
   --> src\stel\ledger.rs:415:13
error: could not compile `symforge` (lib test) due to 15 previous errors
```

T011 + T015 RED against the stub (`collapse_runs` = one run per item; mapper filled the three fields with constants):

```
test ...collapse_tests::collapse_runs_all_distinct_input_keeps_every_count_one ... ok
test ...collapse_tests::collapse_runs_counts_sum_to_input_length ... ok
test ...collapse_tests::collapse_runs_flattening_reproduces_identity_sequence ... ok
test ...collapse_tests::collapse_runs_merges_only_strictly_consecutive_runs ... FAILED
test ...collapse_tests::collapse_runs_ten_thousand_identical_events_collapse_to_one_run ... FAILED
test ...collapse_tests::event_identity_ignores_exactly_the_six_measurement_fields ... FAILED
test ...collapse_tests::stored_record_identity_scopes_by_session_and_reads_widened_columns ... FAILED
test ...ledger_store::tests::recent_and_samples_read_back_the_widened_columns ... FAILED
assertion `left == right` failed: event lane counts:
  left: [1, 1, 1, 1]
 right: [2, 1, 1]
assertion `left == right` failed: one run expected, got 10000
test result: FAILED. 3 passed; 5 failed; 0 ignored; 0 measured; 3306 filtered out
```

T011 + T015 GREEN:

```
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 3292 filtered out; finished in 0.04s
```

T012 RED (pre-feature runtime; the single-event control passed before the failing positive):

```
test status_full_annotates_trailing_run ... FAILED
panicked at tests\stel_status.rs:253:5:
assertion `left == right` failed: trailing run of 3 must be annotated on the decision line:
  left: "last_ledger_decision: serve"
 right: "last_ledger_decision: serve ×3 (first=1000, last=1002)"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out
```

T012 RED against the stub (field and formatter landed, `from_server` observing `None`): identical failure at `tests\stel_status.rs:253:5`.

T013 RED (pre-feature runtime):

```
test admin_recent_runs_collapses_and_scopes_by_session ... FAILED
test admin_window_edge_is_labeled ... FAILED
panicked at tests\admin_api_v1.rs:367:5:  left: Null  right: 50
panicked at tests\admin_api_v1.rs:451:5:  left: Null  right: 50
test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 6 filtered out
```

T013 RED against the stub (view fields landed with an always-empty builder; the Disabled negative control passed first):

```
test admin_recent_runs_collapses_and_scopes_by_session ... FAILED   (panicked at tests\admin_api_v1.rs:381:5: left: Null right: 50)
test admin_window_edge_is_labeled ... FAILED                        (panicked at tests\admin_api_v1.rs:478:5: left: Null right: 50)
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 7 filtered out
```

T012 / T013 / T018 GREEN (`cargo test --no-fail-fast --test stel_status --test admin_api_v1 --test admin_render -j 4 -- --test-threads=1`):

```
Running tests\admin_api_v1.rs   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
Running tests\admin_render.rs   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
Running tests\stel_status.rs    test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

Blast-radius sweep (research.md R11 inventory), all green with no assertion changed: `stel_l4_ledger` 5/5, `stel_ledger_persistence` 18/18, `stel_symforge_edit` 17/17, `surface_honesty` 14/14, `admin_render` 2/2, lib filter `stel golden_replay protocol::tools` 679/679 (the two-identical-push overlay test now renders ` ×2 (first=…, last=…)` and its contains-checks pass untouched). `cargo clippy --lib --bins --test stel_status --test admin_api_v1 --test admin_render -j 4 -- -D warnings` clean; `cargo fmt --all -- --check` clean.

### Reporting-invariant answers (US2)

- The ` ×N (first=…, last=…)` suffix observes `collapse_runs` over the same `ledger.events()` snapshot the decision and route lines are read from, last run, count ≥ 2. Otherwise the line is byte-identical to before. `from_server` is the only producer; the proxy overlay renders through the same formatter.
- `recent_runs` observes `store.recent(50)` reversed to chronological and collapsed with `session_id` in the identity. When the store is disabled or absent, or `recent()` errs, it is `[]` with no `recent_runs_window` (and a warning on the error path). Never fabricated rows.
- `recent_runs_window` is emitted only when rows were actually read, so `[]` with a window means "read zero rows" and `[]` without one means "not read".
- `window_clipped` observes that the run holds the chronologically oldest fetched row and that the fetch filled the window; otherwise `false`.
- `tools_called` / `degrade_flags` observe that the stored JSON parses as a string array; otherwise `null`, never `[]`.
- The widened `StoredLedgerRecord` fields are read through one shared mapper by both `recent()` and `samples_for_estimator()`, pinned with a plain-row control.

### Deviations recorded by the implementer

- `tools_called` / `degrade_flags` are `Option<Vec<String>>` (contract updated to match).
- `window_clipped` only when the fetch filled the window (contract updated to match).
- One shared `STORED_RECORD_COLUMNS` constant builds both SELECTs so positional mapping cannot drift.
- The stale module doc on `src/stel_core/ledger_store.rs` (claimed `server`-gated; the module compiles under `any(server, embed)`) was corrected.

## Review round 1 (2026-09-02) — accepted findings, fixed RED-first

Five of seven review lenses completed before a usage limit interrupted the round (the false-claim and cfg lenses, most refuters, and the critic re-ran in round 2). Twenty findings were raised; dispositions:

| Finding | Disposition |
|---|---|
| state-machine-1 / security-2 (BLOCKER): a run survives a daemon replacement or local-fallback transition, and `ProjectEvidence.generation` is a per-process counter, so a replacement instance's evidence can coincide with the dead one's | FIXED: `RepeatTracker::clear()` at every observed incarnation transition (reconnect success, degraded/healthy flips, local index reload); contract gained the guarantee |
| security-1 (BLOCKER): `search_text` renders a query-time untracked-file diagnostic that evidence does not fence, so a third serve can carry a different body under equal evidence | FIXED: the witness observes the result — a length-framed SHA-256 over the rendered text blocks (taken before the notice is appended) must equal the run's first serve; a differing body restarts the run at 1; contract gained the guarantee |
| collapse-honesty-1 (MEDIUM): `window_clipped` was inferred from a full window rather than observed | FIXED: the view fetches one sentinel row beyond the window and labels the oldest run clipped only when the sentinel shares its identity |
| collapse-honesty-2 (LOW): run span was positional, so insert-order skew could invert it | FIXED: min/max `ts_ms` over the run |
| collapse-honesty-3 (LOW): totals and rows are two separate reads | DOCUMENTED on the field and in the contract; no code change |
| collapse-honesty-4 (LOW): `from_run` destructured with `..` | FIXED: exhaustive destructure |
| state-machine-2 (LOW): load-bearing calls spellable by any in-crate caller | FIXED: `RepeatWitness::observe`, `RepeatNotice::new`, `BodyDigest` private; `ServeObservation` built only by `from_result`; `RepeatKey::observe` requires a `LaneWitness` only `SessionDiscriminator::observe` can mint; cfg(test) doors for the oracles |
| security-3 (LOW): admin summary now carries per-session rows on an unauthenticated loopback bind | ADJUDICATED: pre-existing admin-API posture, operator's own data, no secrets or source content; out of scope for this feature |
| simplicity-1 (MEDIUM, refuted by both refuters): diff size vs the plan's estimate | plan.md records the measured size; one PR per quickstart |
| simplicity-2 (refuted): watcher-nudge fallback in oracle 2 | kept; nudge threshold lowered to 10 s and the header states both mechanisms |
| simplicity-3 (LOW): verify-tools comment narrated an unreachable scenario | FIXED: comment states the true situation (compact facade relay, insurance only) |
| simplicity-4 (LOW): app.js timestamp idiom and dead class | FIXED: existing `toLocaleString()` idiom, `harness-table` |
| simplicity-5, simplicity-7 (refuted) | no change |
| simplicity-6 (LOW): embedded spaces in an assert literal | FIXED |
| test-quality-1 (LOW): receipts GREEN block predated the sixth test | FIXED above |
| test-quality-2 (LOW): battery unreceipted | the full battery is recorded in the PR |
| test-quality-3 (LOW): FR-002 interleaving pinned only at unit level | FIXED: oracle 1 now interleaves a different eligible call and `status` between serves 2 and 3 |
| test-quality-4 (LOW): CI environment dependence of the watcher waits | no code change beyond the 10 s nudge; watch the first Linux runs |

### US1 receipts (round 1 fixes)

F1 unit oracle `body_change_with_equal_evidence_replaces_run`, RED on the pre-fix code:

```
test internals::protocol::repeat::tests::body_change_with_equal_evidence_replaces_run ... FAILED
panicked at src\protocol\repeat.rs:868:9:
a changed body under equal evidence must never notice
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3316 filtered out; finished in 0.00s
```

F1 integration oracle `untracked_file_diagnostic_never_earns_a_notice`, RED on the pre-fix code (the reviewer's scenario reproduced end to end: serves 1-2 clean, serve 3 rendered the untracked-file diagnostic under unchanged evidence generation 8 and still carried the notice):

```
test untracked_file_diagnostic_never_earns_a_notice ... FAILED
panicked at tests\repeat_notice.rs:258:5:
serve 3 (body changed under equal evidence): must not carry the symforge/repeat_notice _meta carrier: {"_meta":{"symforge/project_evidence":{...,"generation":8,...},"symforge/repeat_notice":{"contract_version":1,"evidence_generation":8,"repeat_count":3,"request_hash":"223495c6...","tool":"search_text"},...
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 1.11s
```

F1 GREEN:

```
test internals::protocol::repeat::tests::body_change_with_equal_evidence_replaces_run ... ok
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 3296 filtered out; finished in 0.25s
test untracked_file_diagnostic_never_earns_a_notice ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.91s
```

F2, RED as a compile refusal, then RED at runtime with `clear()` present but unwired:

```
error[E0599]: no method named `clear` found for struct `protocol::repeat::RepeatTracker` in the current scope
   --> src\protocol\repeat.rs:853:17
```

```
test internals::protocol::repeat::tests::clear_restarts_runs_at_one ... ok
test internals::protocol::tests::daemon_degraded_clears_on_next_success ... FAILED
test internals::protocol::tests::daemon_failure_transition_clears_repeat_runs ... FAILED
test internals::protocol::tests::ensure_local_index_does_not_reload_on_repeated_same_root_calls ... ok
test internals::protocol::tests::ensure_local_index_reloads_on_root_mismatch_without_reset ... FAILED
panicked at src\protocol\mod.rs:3528:9: the degraded -> daemon-served transition must clear every repeat run
panicked at src\protocol\mod.rs:3559:9: the daemon-served -> locally-served transition must clear every repeat run
panicked at src\protocol\mod.rs:4623:9: a local index reload is a new incarnation: every repeat run must be cleared
test result: FAILED. 2 passed; 3 failed; 0 ignored; 0 measured; 3314 filtered out; finished in 2.11s
```

F2 GREEN, and the final US1 verification after the last fix commit:

```
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 3292 filtered out; finished in 6.39s
cargo test --test repeat_notice -j 4 -- --test-threads=1   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.98s
cargo test --test repeat_notice -j 4 -- --test-threads=1   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.88s
rmcp3_protocol 15/15, rmcp3_roots_interop 1/1, idempotency 5/5; cargo fmt --all -- --check clean; cargo clippy --lib --bins --test repeat_notice -j 4 -- -D warnings clean
```

The reconnect-success clear site (`proxy_tool_call`, `Ok(new_client)`) is verified by reading only: no cheap harness reconnects to a live replacement daemon. The transitions into and out of the degraded state and the local reload are exercised by tests, and the reconnect path passes through the healthy-to-degraded transition first.

### US2 receipts (round 1 fixes)

collapse-honesty-1 `admin_window_clipped_is_observed_from_the_sentinel_row`, RED then GREEN:

```
test admin_window_clipped_is_observed_from_the_sentinel_row ... FAILED
panicked at tests\admin_api_v1.rs:577:5:
  left: [(50, "admin-sentinel", "find_references", 1001, 1050, true)]
 right: [(50, "admin-sentinel", "find_references", 1001, 1050, false)]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out
```

```
test admin_window_clipped_is_observed_from_the_sentinel_row ... ok
test admin_window_edge_is_labeled ... ok
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
```

collapse-honesty-2 `collapse_runs_span_is_min_max_over_skewed_timestamps`, RED then GREEN:

```
test internals::stel::ledger::collapse_tests::collapse_runs_span_is_min_max_over_skewed_timestamps ... FAILED
panicked at src\stel\ledger.rs:918:9:
  left: (1002, 1005)
 right: (1000, 1005)
```

```
test internals::stel::ledger::collapse_tests::collapse_runs_span_is_min_max_over_skewed_timestamps ... ok
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 3293 filtered out; finished in 0.04s
```

Final US2 sweep after the fix commits: lib filters 32/32; `stel_status` 4/4, `admin_api_v1` 10/10, `admin_render` 2/2, `surface_honesty` 14/14, `stel_l4_ledger` 5/5, `stel_ledger_persistence` 18/18, `stel_symforge_edit` 17/17; clippy on lib, bins and the three test targets clean; `cargo fmt --all -- --check` clean.
