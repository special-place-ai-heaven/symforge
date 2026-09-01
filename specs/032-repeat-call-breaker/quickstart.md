# Quickstart: validating Repeat-Call Notice and Ledger Retry Collapse

Validation guide only — implementation detail lives in plan.md/tasks.md. Revised 2026-09-01 after the adversarial round (research.md R12).

## Prerequisites

- Repo checkout on branch `032-repeat-call-breaker` (or its PR branch), Windows or Linux.
- Serial cargo discipline (Constitution IV): one cargo process at a time, `-j 4`, `--test-threads=1`; anything that can exceed 10 minutes goes through Terminal Commander (`run_and_watch`), never the raw Bash tool.

## RED-first obligation (before machinery lands)

Every oracle below MUST be observed failing against the pre-feature code, with the failure receipted (test output or job id) — Constitution II. Negative oracles carry their positive control in the same test.

## US1 — Repeat notice

Test harness (per R12 finding 13 — there is no in-process `call_tool` harness): use the subprocess stdio client pattern with FULL JSON result access from `tests/rmcp3_roots_interop.rs:87` (`call_tool_result`; `SYMFORGE_NO_DAEMON=1`, `SYMFORGE_SURFACE=full`), or the in-process HTTP `/mcp` harness from `tests/rmcp3_protocol.rs` where two distinct sessions are needed. The `graceful_degradation.rs` helper is text-only and panics on `isError` — unusable for `_meta`/`isError` oracles.

Acceptance oracles (new `tests/repeat_notice.rs`):

1. `third_identical_eligible_call_carries_notice_and_first_two_do_not` — 3× identical `search_symbols` against an unchanged index: serves 1-2 clean, serve 3 has BOTH the appended contract text and `_meta["symforge/repeat_notice"]` with `repeat_count: 3` (positive control inside the same test: content of serve 3 minus the notice == content of serve 1 — spec SC-004 byte-stability; `isError` untouched).
2. `index_change_between_repeats_resets_run` — 2× identical call, mutate + republish a file (evidence generation moves), 3rd call: NO notice; then 3 more identical calls: notice returns (positive control).
3. `unobserved_evidence_clears_run` (unit-level in `src/protocol/repeat.rs`, per R12 finding 9 — the `_meta` key is ALWAYS present on the wire): the `{"bound": false}` marker, a non-deserializable value, and a full evidence with `project_id == "unbound"` each clear the run and never accumulate; evidence-present control in the same test. Integration control: oracle 1.
4. `ineligible_tools_never_notice` — 3× identical `status`, `what_changed`, AND `get_symbol` (excluded per R12: its cache-hit body is time-varying): no notice on any; eligible control in the same test.
5. `eligible_list_is_pinned` — exact membership of `REPEAT_ELIGIBLE_TOOLS` (the R4 **five**: search_symbols, search_text, get_repo_map, find_references, find_dependents), so widening is a reviewed decision.
6. `internal_failure_clears_run` — per the data-model state machine (single normative rule); include the InvalidRequest-advances control in the same test.
7. `tracker_cap_clears_without_false_claim` — fill past `REPEAT_TRACKER_MAX_ENTRIES`, confirm cleared state produces no notice on the next serve and re-accumulates honestly.
8. `sessions_never_share_runs` — spec Scenario 5, on the shared HTTP lane (two `/mcp` sessions against ONE server): 2 serves on session A + 1 on session B ⇒ no notice on B; positive control: 3 on A ⇒ notice on A. If T001 finds no observable per-session identity on that lane, the oracle instead pins that the lane NEVER notices (with the stdio positive control).
9. `projects_argument_never_accumulates` — a set-valued `projects` call repeated 3× gets no notice (evidence is structurally the unavailable marker on that lane — R6 lane caveats); single-project positive control in the same test.
10. Unit properties in `src/protocol/repeat.rs`: witness equality (unequal evidence ⇒ `None`), threshold boundary (2 vs 3), saturating count.

Manual smoke (optional, after tests): run stdio server, issue the same `search_symbols` three times from a client, observe the notice on the third response; touch an indexed file, repeat, observe the reset.

## US2 — Ledger retry collapse

1. `collapse_runs` property oracles (unit, `src/stel/ledger.rs` tests), per lane identity: counts sum to input length; order-preserving strict consecutiveness (A,A,B,A → 2,1,1); all-distinct input ⇒ all counts 1.
2. `status_full_annotates_trailing_run` — seed ≥2 identical trailing events via `SessionLedger::push` (public; events literal-constructible — see `tests/admin_api_v1.rs:39-45`), `status detail:full` shows ` ×N (first=…, last=…)` on the **`last_ledger_decision:` line only**; single-event control renders byte-identically to today's format in the same test. Include one large-N control (seed 10,000 — spec SC-003, status lane).
3. `admin_recent_runs_collapses_and_scopes_by_session` — seed runs across two session_ids in a file-backed durable store (two `StelLedgerStore::open` handles on one temp path — `open_in_memory` binds one session per private DB); `GET /api/v1/summary` returns collapsed `recent_runs` (chronological) that never merge across sessions; totals fields unchanged vs uncollapsed (FR-008 control — the existing 3-identical-event seed collapses ×3); `available:false` ⇒ `recent_runs: []`.
4. `admin_window_edge_is_labeled` — seed a run longer than the fetch window; the run containing the chronologically-oldest fetched row carries `window_clipped: true` and `recent_runs_window` is present (spec SC-003: never silent truncation).
5. Blast-radius sweep: run the full pin inventory from research.md R11 — including `scripts/verify-tools.cjs` (both fixtures) — and reconcile deliberately (a changed pin means a seed produced a ≥2 run — verify that's true before updating it).

## Full gate list (Constitution IV — ALL observed green before any success claim)

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings          # via Terminal Commander (long)
cargo test --lib --bins --tests -- --test-threads=1  # via Terminal Commander (long); -j 4
cargo test --no-default-features --features embed --lib -- --test-threads=1   # regression gate for the stel_core SELECT widening ONLY — it cannot observe the server-gated feature code (R10)
cargo bench --bench observed_refresh_gate_v1 -- --test
cargo build --release                               # via Terminal Commander (long)
node scripts/verify-tools.cjs --bin target/release/symforge   # tool-correctness harness (verify exact flags against ci.yml); its readiness probe must already be fingerprint-distinct (plan US1 §9)
cd npm; npm test                                    # unchanged surface, still must be green
```

Then `cargo clean` (repo discipline for heavy local sessions).

## Delivery

One behavior-changing PR (Constitution VI): independent adversarial review including a cfg-lens sweep; squash-merge with explicit conventional `--subject` and a safe one-paragraph `--body` (no parentheses, no colon-bearing prose lines).
