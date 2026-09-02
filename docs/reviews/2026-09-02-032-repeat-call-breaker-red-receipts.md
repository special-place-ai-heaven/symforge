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
- `recent_runs` observes the durable rows read back through `StelLedgerStore::recent`, reversed to insert order and collapsed with `session_id` in the identity. When the store is disabled or absent, or the read errs, it is `[]` with no `recent_runs_window` (and a warning on the error path). Never fabricated rows. (Round 1 changed the fetch to the window plus one sentinel row; the honesty properties are unchanged.)
- `recent_runs_window` is emitted only when rows were actually read, so `[]` with a window means "read zero rows" and `[]` without one means "not read".
- `window_clipped` observes that the run holds the chronologically oldest fetched row and that the fetch filled the window; otherwise `false`. **SUPERSEDED by review round 1** (see the round-1 section below): inferring the label from a full window is a hedge, not an observation, and in steady state it labelled a run that had in fact been counted in full. The shipped rule fetches one sentinel row beyond the window and sets the label only when the sentinel shares the oldest run's identity.
- `tools_called` / `degrade_flags` observe that the stored JSON parses as a string array; otherwise `null`, never `[]`.
- The widened `StoredLedgerRecord` fields are read through one shared mapper by both `recent()` and `samples_for_estimator()`, pinned with a plain-row control.

### Deviations recorded by the implementer

- `tools_called` / `degrade_flags` are `Option<Vec<String>>` (contract updated to match).
- `window_clipped` only when the fetch filled the window (contract updated to match). **SUPERSEDED by review round 1**: replaced with the sentinel-row observation described below.
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

## Review round 2 (2026-09-02) — the forward claim, and what closed it

Round 2 ran all seven lenses (the false-claim and cfg-sweep lenses that round 1 lost to a usage limit ran here for the first time). Thirty-one findings; the substantive result was one defect reported independently by three lenses and reproduced end to end by two verifiers.

### The blocker: the notice's forward sentence was unfenced

The notice promises "The result cannot differ until the index changes". Round 1's body digest witnesses that the serves that already happened were byte-identical; it cannot make that forward promise true, because two eligible render paths read inputs the index never publishes:

- `search_text` on a zero-hit result runs an untracked-file sweep over live `git status` plus raw worktree bytes, and appends an "untracked file may match" diagnostic.
- `find_references` with a `path` the index does not hold falls back to an on-disk admission view that reads file metadata and the first bytes, and renders the on-disk size.

Either can render a different answer on the next serve with no publication in between, so a notice emitted on serve 3 is falsified by serve 4. Spec FR-006 already forbids this: a read operation whose answer can vary on an unchanged index must be excluded.

**Fix, per the Reporting Invariant: the component that knows is the component that reports.** The per-dispatch scope that already carries the project evidence gained a latch. The two reading sites set it as their first act, whenever they execute, regardless of what the read returned this time. The seam reads it back and records the serve as unobserved, which removes the run — so the notice is not merely skipped, it is unconstructible there. No string matching of response bodies was used; that is the defect class this feature exists to avoid.

The cost is recorded in `contracts/repeat-notice.md`: a zero-hit `search_text` and a path-qualified `find_references` outside the index no longer earn a notice. Four of the five eligible tools still notice on empty results, as does `search_text` when it has hits. Rewording the notice to a purely backward-looking claim was the alternative and was rejected: the text is byte-canonical and the standing rule is to withhold rather than weaken.

RED for the first lane, on the pre-fix code — the false claim, reproduced:

```
test zero_hit_search_text_never_claims_cannot_differ ... FAILED
panicked at tests\repeat_notice.rs:1006:5:
assertion `left == right` failed: zero-hit serve 3 must be byte-identical
  left: ... "symforge/repeat_notice": Object {"contract_version": Number(1),
        "evidence_generation": Number(8), "repeat_count": Number(3),
        "request_hash": String("223495c6..."), "tool": String("search_text")} ...
        "No matches for 'needle-zz'. ...\n\nRepeat notice: identical request served 3x
        with no index change published in between (project evidence unchanged).
        The result cannot differ until the index changes - change the request instead of retrying."
 right: ... "No matches for 'needle-zz'. ..." (no notice)
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out
```

RED for the second lane, on the pre-fix code:

```
test find_references_disk_fallback_never_claims_cannot_differ ... FAILED
panicked at tests\repeat_notice.rs:1117:5:
assertion `left == right` failed: disk-fallback serve 3 must be byte-identical
  left: ... "symforge/repeat_notice": Object {..., "repeat_count": Number(3),
        "tool": String("find_references")} ...
        "degraded result (Tier 2 metadata-only) ...\nSize: 1026 bytes\n ...
         \n\nRepeat notice: identical request served 3x ..."
 right: (same body, no notice)
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out
```

RED as a compile refusal for the unit oracle, then GREEN for all three:

```
error[E0425]: cannot find function `note_unfenced_input` in module `result_status`
error[E0425]: cannot find function `unfenced_input_consulted` in module `result_status`
error[E0599]: no variant, associated function, or constant named `UnfencedInput` found for
              enum `protocol::repeat::UnobservedReason` in the current scope
```

```
test internals::protocol::repeat::tests::unfenced_input_removes_the_run_and_never_notices ... ok
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 3308 filtered out; finished in 2.29s
test find_references_disk_fallback_never_claims_cannot_differ ... ok
test zero_hit_search_text_never_claims_cannot_differ ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 2.20s
```

**Why the latch cannot leak between calls**, which the review asked to be shown rather than asserted: it is a field of the per-dispatch observation struct, bound only by the scope guard that wraps each `call_tool`, constructed fresh per call and dropped with the future — there is no cross-call storage to leak from. A unit oracle runs two sequential scopes in one task and observes the second entering false. End to end, both integration oracles serve four latched calls and then a control that DOES notice in the same session and process; a leaking latch would have withheld that control.

### The other findings fixed in this round

| Finding | Disposition |
|---|---|
| An in-flight serve could bridge an incarnation change: a serve whose evidence was captured against the old daemon could be recorded after a concurrent request's clear, anchoring a run that spans the replacement | FIXED: the tracker carries an incarnation epoch, bumped on every clear; the seam stamps the key with the epoch it read before dispatch, and a serve whose epoch no longer matches is refused rather than inserted. The capacity eviction deliberately does not bump the epoch — memory pressure is not a change of the index that served those keys |
| The accumulating lane was inferred from the ABSENCE of the HTTP marker, so a future transport that inserts nothing would silently accumulate across clients | FIXED: `Stdio` now requires a positive declaration from the entry point AND the absence of the marker; everything else is inert. Absence of a declaration is not evidence |
| The witness compared two named fields, so a future field on the observation would be silently excluded | FIXED: whole-value comparison. RED was produced by temporarily adding a field and observing the named-field comparison still witness equality |
| The daemon-proxy overlay's `×N` annotation was pinned by a contains-check that passed with or without it | FIXED: exact whole-line assertion plus its negative. The RED receipt first shows the OLD assertion still passing under a mutation that deletes the annotation, which is the blindness itself |
| Contract and data-model said `canonical` was the chronologically first row; the code uses the positional first | FIXED in the docs to match the code, which is the honest reading: the positional first row is the one that really opened the run, and the span is the min/max so skew cannot invert it |
| The receipts' US2 invariant bullets still described the pre-round-1 window rule | FIXED: marked superseded, pointing at the shipped sentinel rule |
| Data model still described the evidence-only witness | FIXED: state machine and witness rewritten to the shipped behavior, including both rounds' new transitions |
| Several LOW notes: unused derives, a boolean that a match subsumes, a defensive branch the server cannot produce, comment volume at the clear sites | ADJUDICATED, no change. Each was refuted by at least one verifier as violating no governing sentence, and the code is clearer with the explicit forms |
| Lenses noted that the embed cell, the release build, verify-tools and npm had not been executed by the implementers | Resolved by the full battery below, which is the observing authority |

Round-2 verification after the fixes, on the merged branch:

```
cargo test --lib -j 4 -- --test-threads=1 repeat result_status mcp_http daemon_degraded ensure_local_index degraded_dead_endpoint
  test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 3295 filtered out; finished in 6.55s
cargo test --lib -j 4 protocol::tools -- --test-threads=1
  test result: ok. 501 passed; 0 failed; 0 ignored; 0 measured; 2832 filtered out; finished in 31.14s
cargo test --test repeat_notice -j 4 -- --test-threads=1   (twice)
  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.04s
  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.95s
cargo test --test surface_honesty --test graceful_degradation -j 4 -- --test-threads=1
  graceful_degradation: ok. 1 passed; 0 failed        surface_honesty: ok. 14 passed; 0 failed
cargo test --test rmcp3_protocol --test rmcp3_roots_interop --test idempotency -j 4 -- --test-threads=1
  ok. 15 passed / ok. 1 passed / ok. 5 passed
cargo test --no-fail-fast --test stel_status --test admin_api_v1 --test admin_render --test surface_honesty --test stel_symforge_edit -j 4 -- --test-threads=1
  47 passed across 5 binaries, 0 failed
clippy on lib, bins and the touched test targets: clean.   cargo fmt --check: clean.
```

The latch sits in a hot render path and changed no existing rendering: the 501 in-file protocol tests, the degraded-tier oracle, and the surface-honesty suite are all green unchanged.

### Carried forward, not defects

- The latch is a no-op outside a dispatch scope, so a future renderer that moves one of these reads onto another task would lose the fence. Both production callers run inline on the dispatch task today, and the two integration oracles would fail if that changed. A scope-required token threaded to the reading sites would make it structural; that is a larger refactor than this round warranted.
- A sweep of every disk and git read reachable from the five eligible tools found no unfenced input beyond the two fixed. That was a grep-and-map exercise, not a proof; `get_repo_map` and `find_dependents` were confirmed by absence rather than by reading their render paths line by line.
- The proxy overlay matches the annotated line by prefix, so the annotation must stay a suffix. Nothing pins that ordering inside the formatter.

## Full gate battery (Constitution IV), 2026-09-02, Windows

Run with `-j 4`, tests `--test-threads=1`, one cargo invocation at a time. The embed cell ran in a separate worktree so the two feature sets never shared a target directory, which is the documented way to avoid the interleaving corruption.

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean, exit 0 |
| `cargo build --no-default-features --features embed` | clean |
| `cargo clippy --no-default-features --features embed,__test-internals --lib -- -D warnings` | clean, exit 0 |
| `cargo test --no-default-features --features embed --lib -- --test-threads=1` | ok. 1344 passed; 0 failed; 4 ignored |
| `cargo bench --bench observed_refresh_gate_v1 -- --test` | exit 0 |
| `cargo build --release` | Finished in 5m 23s |
| `node scripts/verify-tools.cjs --bin target/release/symforge.exe` | 7 PASS, 1 REVIEW, 0 FAIL — every snapshot byte-identical |
| `node scripts/verify-tools.cjs --fixture verify-tools-real --bin target/release/symforge.exe` | 10 PASS, 1 REVIEW, 0 FAIL — every snapshot byte-identical |
| `npm test` (in `npm/`) | 0 failed |
| `cargo test --lib --bins --tests -- --test-threads=1` | see below |

The two REVIEW rows are the harness's documented grep-over-match cases, which it does not count as failures; both invocations exited 0. That the snapshots are byte-identical is the load-bearing result here: the repeat notice did not leak into a release-gate snapshot.

### The full serial suite, stated precisely

One real failure surfaced and was fixed: `full_source_set_matches_reviewed_darkness_baseline`, the broad in-tree source seal, which fired on exactly one added file (197 vs the pinned 196) and its bytes. Before refreshing the pin I verified what the seal exists to protect: the excluded runtime source set is unchanged and its own seal still passes, no file in that set was touched, and the diff adds no reference to it. The pin was then set to the value the oracle itself computed, on an already formatted tree, per the rule that the oracle is the only recompute authority.

Beyond that, no test failed for a reason in this diff. Across four runs of the suite, every test binary has been observed passing, and the only other failures were three separate loopback-socket transients, each in a different pre-existing test that binds a local HTTP server, each passing repeatedly when rerun in isolation:

| Test | Failure | In isolation |
|---|---|---|
| `hook_enrichment_integration::loading_sidecar_refuses_all_content_routes_without_mutation` | health body not JSON, while a release build was compiling concurrently (my own doing) | 4 runs, 13/13 each |
| `serve_auth::missing_bearer_with_key_is_unauthorized` | the HTTP send itself failed, not an auth assertion | 3 runs, 5/5 each |
| `hook_enrichment_integration::test_edit_hook_impact_diff` | `GET /impact must succeed: An existing connection was forcibly closed by the remote host. (os error 10054)` | passes |

`os error 10054` is a Windows TCP reset on loopback. None of these tests reference the dispatch scope or any other code this feature changed, which was checked rather than assumed; the one HTTP-module change in the diff is confined to a hunk inside `mod tests`. A single no-fail-fast pass covered every target and reported exactly one failing target, the second row above.

The honest summary is therefore: every gate is green, the suite has no failure attributable to this change, and this Windows host intermittently resets loopback connections during a ten-minute serial run. The Linux `rust` job in CI is the authoritative observation of that gate and runs the same command.

## Round 3 (2026-09-02) — the withholding rule is itself the defect

Round 2's fix was accepted and shipped in this branch. Re-reading it against the
feature's purpose showed it had traded away the thing the feature exists to do.

### The finding

The notice's version-1 text ended with "The result cannot differ until the index
changes." Round 2 correctly established that no equality of past serves licenses that
sentence when the renderer reads state the index never publishes, and withheld the
notice on those lanes. But the most common such lane is a zero-hit `search_text` —
which is precisely the shape a looping agent produces. The guard fired on the primary
case.

It was also incoherent as observed behavior. The untracked sweep runs only when
`result.files.is_empty() && result.suppressed_by_noise == 0`
(`src/protocol/tools.rs`), so of two zero-hit searches, one could notice and the other
not, decided by a counter no caller can see or predict.

### The adjudication

Four alternatives were considered; three were refuted on evidence.

| Option | Verdict |
|---|---|
| Keep withholding (shipped round 2) | Refuted — discards a valid observation to protect one clause, and does so unpredictably |
| Widen the claim to "no file changed" | Refuted — `git add` moves a path out of untracked without changing a byte; so do ignore rules, repo config, path existence, permissions, and symlink resolution |
| Fence the inputs by digesting what was read | Refuted — a digest witnesses what one serve observed; it does not freeze the next. A real fence would have to snapshot git classification, the path set, metadata, bytes, failures, and filesystem identity |
| Emit the observed facts and drop the prediction | **Adopted** |

A conditional variant (emit the strong sentence only when nothing unfenced was read)
was also rejected: "nothing unfenced was read" is not positive proof when the report is
opt-in — one future renderer that forgets to latch silently restores the false claim —
and the latch was never literally that fact anyway. It fired at the top of
`matching_untracked_paths_for_search_text`, ahead of the structural, missing-repository,
and failed-git-open early returns, so it meant "entered a path that may depend on live
state."

### What the notice says now

```
Repeat notice: identical request served {N}x. Across these serves, no index change was published and the response text before this notice was unchanged. Do not retry unchanged; change the request or relevant project state first.
```

Every factual clause is something the tracker had to observe to reach the threshold:
the serve count, `ProjectEvidence` equality across the run, and `BodyDigest` equality of
the text rendered before the notice. The closing sentence is advice, not a prediction.
`contract_version` goes 1 → 2: the JSON shape is unchanged, but its meaning is not, and
a client keyed to version 1's promise would otherwise silently mis-read version 2.

The scoping of the body clause is deliberate. The delivered response necessarily differs
between serves because `{N}` increments, so the claim names the text *before* the notice
— which is exactly the bytes the digest compares.

### RED receipt

`zero_hit_search_text_notices_on_third_identical_serve` — three byte-identical zero-hit
serves under unchanged evidence, nothing planted between them:

```
test zero_hit_search_text_notices_on_third_identical_serve ... FAILED
thread panicked at tests\repeat_notice.rs:283:47:
zero-hit serve 3: must carry symforge/repeat_notice: {"_meta":{"symforge/project_evidence":{...}}}
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out
```

### GREEN, and why the honesty guarantee survives

```
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The load-bearing result is not that the new test passes; it is which old test does.
`untracked_file_diagnostic_never_earns_a_notice` was written in round 1, plants an
untracked file between serves so the body moves under unchanged evidence, and asserts
no notice. **It passes unchanged.** The body digest — not the latch — was always what
caught the real hazard; round 2 added a second mechanism in front of one that already
worked, and only that second mechanism cost the feature its primary case.

`find_references_disk_fallback_never_claims_cannot_differ` was inverted rather than
deleted, as `find_references_disk_fallback_notices_then_resets_when_the_body_moves`:
three identical serves on the disk-fallback lane now notice, and the half where the
gitignored log is appended still asserts no notice, because the rendered `Size:` line
moves and the digest restarts the run. The lane keeps its coverage and demonstrates the
mechanism.

### Removed

The unfenced-input latch is deleted, not left dormant: `note_unfenced_input`,
`unfenced_input_consulted`, `UnobservedReason::UnfencedInput`, the `DispatchObservations`
struct (whose second slot was the latch — the task-local returns to the single-slot
`PROJECT_EVIDENCE` shape it had before round 2), and both call sites. Dead code behind a
`server` cfg is invisible to the embed cell (CLAUDE.md records why), so leaving it would
have been leaving a trap.

Net `src/`: 57 insertions, 219 deletions. `FULL_SOURCE_PIN_V1` moved on bytes only —
197 files before and after, no file added or removed. Before refreshing it,
`excluded_runtime_source_set_matches_reviewed_baseline` and
`dark_call_edges_appear_only_in_the_wired_roster` were both observed passing, so the set
the seal protects is unchanged and the diff introduces no new bridge.
