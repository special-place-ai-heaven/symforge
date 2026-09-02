# Implementation Plan: Repeat-Call Notice and Ledger Retry Collapse

**Branch**: `032-repeat-call-breaker` | **Date**: 2026-09-01 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/032-repeat-call-breaker/spec.md`

**Grounding**: Every code claim in this plan was verified 2026-09-01 by a four-agent read-only survey of the live index (694k tokens, anchors recorded in [research.md](research.md)). Line numbers are as-of that date; re-verify anchors with `search_symbols`/`get_symbol_context` before editing, not by trusting this document.

## Summary

Two deliverables, one theme (the server being honest about repetition):

1. **Repeat-call notice (US1, P1)** — a session-scoped tracker at the `SymForgeServer::call_tool` seam (`src/protocol/mod.rs:2130-2198`, the single choke point both stdio and HTTP `/mcp` dispatch through) fingerprints each eligible read call with the existing generic `RequestHash::for_tool_request` (`src/idempotency.rs:42-57`), keyed by (observed session discriminator, tool, request hash), and counts consecutive-in-kind serves. When the same fingerprint is served a 3rd time in one session and the response-attached `ProjectEvidence` (`src/protocol/result_status.rs:58-68`) is **observed equal** across all serves of the run, the seam appends a short notice to the response text and a `symforge/repeat_notice` `_meta` entry. The claim "the result cannot differ" is constructible only from an evidence-equality witness (Constitution P1 + P5); when evidence is not positively observed (unavailable marker, unbound project, unequal, or no observable session identity on the lane), the notice is structurally impossible, not just skipped. Eligibility is a **5-tool** allow-list (R4 post-adversarial: `get_symbol` and `search_files` were refuted out — their repeat output varies on an unchanged index).

2. **Ledger retry collapse (US2, P2)** — one generic run-length collapse (`collapse_runs(items, identity_key)` in `src/stel/ledger.rs`) with **two lane identities** (data-model.md): the in-memory `StelLedgerEvent` lane (exhaustive destructure; feeds the status `detail:full` last-ledger lines, which gain a `×N` suffix on the `last_ledger_decision:` line when the trailing run has N≥2) and the durable `StoredLedgerRecord` lane (feeds a new `recent_runs` section on the admin `LedgerSummaryView`; `StelLedgerStore::recent()` gains its first production caller, with its SELECT and `StoredLedgerRecord` widened additively by `pff_bypass`/`cache_hit`/`degrade_flags_json` so identity does not silently merge rows that differ in them; rows are fetched newest-first and reversed to chronological before collapse; window clipping is labeled, never silent). Stored events are never rewritten (FR-009).

**Explicitly NOT ledger-driven detection**: the survey proved granular tools never append ledger events (only the compact-facade STEL handlers do, `src/protocol/tools.rs:11295/11527/11744/11823`), so US1 detection is serve-path state, and US2 is a rendering feature over whatever events exist.

## Technical Context

**Language/Version**: Rust, 2021 edition, toolchain as pinned by repo CI (`rust` job)

**Primary Dependencies**: rmcp 3.1.4 as resolved by `Cargo.lock` (T001 2026-09-02; the survey's "3.1.0" was the stale patch level) (MCP server, `CallToolResult`/`_meta`), serde/serde_json (canonical fingerprint input), rusqlite bundled (existing durable STEL ledger, read-only reuse), axum (existing admin API). **No new dependencies.**

**Storage**: New in-memory bounded map on `SymForgeServer` (repeat tracker; cap + clear-on-overflow). Existing stores reused read-only: in-memory `SessionLedger` (`src/stel/ledger.rs:16-48`), durable `stel_ledger_events` SQLite (`src/stel_core/ledger_store.rs`, retention cap 2000). No schema changes; `recent()`'s SELECT and `StoredLedgerRecord` widen additively to read back three columns the table already has (`pff_bypass`, `cache_hit`, `degrade_flags_json`).

**Testing**: `cargo test --lib --bins --tests -- --test-threads=1` (serial, per repo CLAUDE.md); embed cell `cargo test --no-default-features --features embed --lib -- --test-threads=1`; `verify-tools.cjs` against release binary; npm suite untouched (no npm-side changes). Long runs via Terminal Commander only.

**Target Platform**: Windows/Linux/macOS server builds. **The embed cell cannot observe one line of this feature**: `src/protocol/` and `src/stel/` are `#[cfg(feature = "server")]` at the crate root (`src/internals.rs:97-109`, `src/lib.rs:53-62`), so every new US1/US2 file is server-only by location; only `src/stel_core/ledger_store.rs` (the additive SELECT widening) compiles under embed. The embed gate stays in the battery as a regression check for that shared module — not as a proof about the feature (R12 corrected the earlier inverted claim).

**Project Type**: Single Rust crate (existing layout; no new crates, no new MCP tools, no CI workflow changes).

**Performance Goals**: Seam overhead per eligible call ≤ one canonical-JSON serialization + one FNV-domain hash + one mutex'd HashMap op (micro-seconds against handlers that do index queries); collapse is O(n) over ≤2000 durable rows / session ledger length, computed only when a view renders.

**Constraints**: Zero false claims (SC-002) dominates every trade-off — the design prefers withholding a true notice over any path that could emit a false one. No new MCP tool names (the 39/40 advertised-vs-registered pins and `SYMFORGE_TOOL_NAMES` stay untouched). No `.github/workflows` edits (the `WORKFLOW_FINGERPRINTS` seal stays untouched). No README/AGENTS.md phrase changes needed.

**Scale/Scope**: ~9 production files touched (`src/protocol/mod.rs`, `src/protocol/result_status.rs`, new `src/protocol/repeat.rs`, `src/idempotency.rs` — one-word `Hash` derive on `RequestHash`, `src/stel/ledger.rs`, `src/stel/status.rs` — incl. `StelStatusContext` field additions, `src/stel_core/ledger_store.rs` — additive SELECT widening, `src/server/admin/api_v1.rs` + `assets/app.js`, `scripts/verify-tools.cjs` — readiness-probe fingerprint made distinct so it cannot trip the notice into a release-gate snapshot), ~12 test files touched/added. Estimated ≤1100 net new lines including tests — **measured on 2026-09-02 at the first review checkpoint: ~3,400 net (≈900 production, ≈1,100 in-file unit tests, ≈1,300 integration tests, the rest docs)**, and larger again after two review rounds of fixes; the RED-first oracle set, the two-lane collapse tests, and the subprocess harness account for the difference. Re-derive rather than trusting the number: `git diff origin/main...HEAD --numstat` for the net total, and intersect the added hunks with each file's `#[cfg(test)]` boundary for the production/test split. Delivery stays one behavior-changing PR per quickstart.md.

## Constitution Check

*GATE: evaluated against SymForge Constitution v1.0.0 before Phase 0; re-evaluated after Phase 1 design — both passes recorded here.*

| Principle | Compliance in this design | Status |
|---|---|---|
| **I. Reporting Invariant** | The notice's claim is backed by an equality check over `ProjectEvidence` values actually attached to the run's responses; when the observation fails (evidence absent, unequal, or index not current) the notice is **withheld entirely** — the answer to "what does it emit when the observation fails" is *nothing*, by construction. The `×N` annotation is computed from the rows being rendered, never asserted. | PASS |
| **II. RED-First Evidence** | Every acceptance behavior in [quickstart.md](quickstart.md) has a named oracle that MUST be observed failing before its machinery lands (receipts required). Every negative oracle (notice absent after index change; no collapse across session boundary) carries its accepting positive control in the same test. | PASS (obligation carried into tasks) |
| **III. Frozen Contracts Win** | No frozen tree touched. New contract identifiers (`symforge/repeat_notice`, `recent_runs`) are introduced, not renamed. | PASS |
| **IV. Verification Gates** | Full gate list in quickstart.md, serial cargo, `-j 4`, Terminal Commander for long runs. The embed cell runs as a regression gate for the one shared file touched (`src/stel_core/ledger_store.rs`); it cannot observe the server-gated feature code — corrected per R12, the earlier claim that seam code compiles under embed was inverted (research.md R10). | PASS (obligation) |
| **V. Unrepresentable Over Checked** | `RepeatNotice` is constructible only via a witness function consuming both evidence values and returning `Some` only on observed equality — mirroring the `SourceAuthority::from_freshness` repair of the prior string-equality forgery (`src/protocol/search_format.rs:3-16`, the same defect class this feature could otherwise reintroduce). Run identity for collapse uses exhaustive struct destructuring so a future `StelLedgerEvent` field cannot silently join or skip identity. | PASS |
| **VI. Independent Review Before Merge** | The planning package ran a 4-lens independent adversarial round (anchors / false-claim red-team / test-feasibility / cross-artifact consistency) on 2026-09-01; every finding — 4 BLOCKER-grade, 4 HIGH, 8 MEDIUM, 4 LOW — is fixed in the artifacts or adjudicated with rationale in research.md R12, which records the actual findings, not a placeholder. Implementation still ships as one behavior-changing PR requiring its own independent adversarial review incl. cfg-lens sweep before merge. | PASS (plan-stage review recorded; implementation review remains an obligation) |

**Post-Phase-1 re-check (2026-09-01)**: design artifacts introduce no violation; Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/032-repeat-call-breaker/
├── spec.md              # Feature specification (validated)
├── plan.md              # This file
├── research.md          # Phase 0: decisions, rationale, alternatives, verified anchors, adversarial findings
├── data-model.md        # Phase 1: RepeatKey/RepeatRun/RepeatWitness/LedgerRun + identity table
├── quickstart.md        # Phase 1: gate list + runnable validation scenarios
├── contracts/
│   ├── repeat-notice.md # _meta key + appended text line contract
│   └── admin-recent-runs.md # LedgerSummaryView.recent_runs JSON contract
├── checklists/requirements.md
└── tasks.md             # Phase 2 (/speckit-tasks output)
```

### Source Code (repository root)

```text
src/
├── idempotency.rs       # one-word change: add Hash to RequestHash's derive
├── protocol/            # (server-gated at the crate root — none of this compiles in the embed cell)
│   ├── repeat.rs        # NEW — RepeatTracker, RepeatKey (+session discriminator), RepeatWitness,
│   │                    #        RepeatNotice, REPEAT_ELIGIBLE_TOOLS (5), NOTICE_THRESHOLD
│   ├── mod.rs           # call_tool seam: clone (name, args) pre-dispatch; post-dispatch tracker
│   │                    #   update + notice attach (text append + _meta), after evidence attach (2173)
│   └── result_status.rs # REPEAT_NOTICE_META_KEY constant + serde view (single-writer, like FR-319 seam)
├── stel/                # (server-gated at the crate root)
│   ├── ledger.rs        # generic collapse_runs + Run<T> + two lane identity fns
│   └── status.rs        # StelStatusContext.trailing_run field + from_server computation +
│                        #   ×N suffix on the last_ledger_decision: line (N==1 renders byte-identically)
├── stel_core/
│   └── ledger_store.rs  # recent() SELECT + StoredLedgerRecord widened additively (pff_bypass,
│                        #   cache_hit, degrade_flags_json); first production caller (compiles in embed)
└── server/admin/
    ├── api_v1.rs        # LedgerSummaryView.recent_runs + recent_runs_window (additive serde fields)
    └── assets/app.js    # minimal recent-runs list rendering

scripts/
└── verify-tools.cjs     # readiness probe gets a fingerprint-distinct argument (release-gate immunity)

tests/ (touched or added)
├── repeat_notice.rs     # NEW — US1 acceptance oracles (RED-first, positive controls inline)
├── stel_status.rs       # reconcile pins if any seed produces a run ≥2; add ×N oracles
├── surface_honesty.rs   # reconcile durable_ledger/calibration pins ONLY if wording changes (none planned)
├── admin_api_v1.rs      # recent_runs + window-edge oracles
├── admin_render.rs      # app.js field-name pin extension
└── stel_l4_ledger.rs    # additive impact from StoredLedgerRecord widening (verify, don't assume)
```

**Structure Decision**: single-crate layout, one new module (`src/protocol/repeat.rs`) so the tracker is unit-testable without the 12k-line `tools.rs`; all other changes land inside the files that own the touched behavior today. No new binaries, tools, endpoints beyond the additive admin field.

## Design (normative for /speckit-tasks)

### US1 — Repeat-call notice

1. **Seam placement** (`src/protocol/mod.rs:2130-2198`): before `ToolCallContext::new` consumes the request (2157), clone `(tool_name, canonical args Value)` — the code at 2142-2155 already deserializes arguments here. After the router returns and after `attach_project_evidence_meta` (2173) and error reclassification (2181-2190), run the tracker update. Order matters: the tracker reads the evidence the response actually carries.
2. **Eligibility**: `REPEAT_ELIGIBLE_TOOLS` allow-list, pinned by a test, membership exactly **5 tools**: `search_symbols`, `search_text`, `get_repo_map`, `find_references`, `find_dependents`. The adversarial round refuted the original 7 (research.md R4/R12): `get_symbol` repeats return the session cache-hit body embedding wall-clock `session_age_secs` (`src/protocol/format.rs:5833-5865`) and the `force_refresh` lane appends an elapsed-seconds dedup footer — output varies on an unchanged index; `search_files` with `rank_by="frecency"` reorders by a `SystemTime::now`-decayed, cross-process-mutable frecency store bumped by interleaved reads (`src/protocol/tools.rs:6650-6710`). The full exclusion table is R4 — widening the list later requires a per-tool proof that every input to its rendered output is fenced by the compared evidence.
3. **Fingerprint**: `RequestHash::for_tool_request(tool_name, &args)` — generic, domain-separated, already crate-exported (`src/internals.rs:46-47`). Key = `(session discriminator, tool_name, RequestHash)`. Nothing stripped (read tools carry no idempotency_key). `RequestHash` needs a one-word `Hash` derive added in `src/idempotency.rs:37-39` (it derives Eq but not Hash today).
4. **Grounding**: deserialize the `symforge/project_evidence` `_meta` value from the outgoing response into TYPED `ProjectEvidence` (the seam wrote it at 2173; single-writer rule means it is authoritative). The key is ALWAYS present — "unobserved" means the value is the `{"bound": false}` unavailable marker, fails deserialization, or carries the `"unbound"` placeholder `project_id` (`result_status.rs:149-167`); any of those clears the run (cannot observe ⇒ cannot accumulate ⇒ cannot claim). **The single outcome rule lives in data-model.md's state machine**: a serve with equal observed evidence advances the run regardless of outcome class (NotFound/EmptyResult/InvalidRequest retry loops are the motivating case), EXCEPT an observed `InternalFailure` clears — and on lanes where `OutcomeClass` is unobservable (daemon-proxied plain-String bodies carry no `symforge/result_status`), `isError == true` clears conservatively.
5. **Witness type**: `RepeatWitness::observe(prior: &ProjectEvidence, current: &ProjectEvidence) -> Option<RepeatWitness>` — private field, `Some` only on full equality; `RepeatNotice::new(witness: RepeatWitness, count: u32, ...)` is the only constructor. A forged notice is unspellable.
6. **State & session scoping**: `repeat_tracker: Mutex<RepeatTracker>` on `SymForgeServer` (`src/protocol/mod.rs` struct at 280, alongside `stel_ledger` at 255). `RepeatTracker` = `HashMap<RepeatKey, RepeatRun>`; hard cap `REPEAT_TRACKER_MAX_ENTRIES = 512`; on overflow, `clear()` (loses only true notices — false negatives are free, SC-002 is untouchable). **Session isolation is a spec MUST (Scenario 5) and is satisfied by construction, not by precedent**: the `RepeatKey` carries a session discriminator observed at the seam — on stdio (one client per process) a process-constant value is the observed session; on the shared HTTP `/mcp` lane (ONE `Arc<SymForgeServer>` for all clients, `src/server/serve.rs:488-502`) the discriminator must come from the per-session identity rmcp exposes in `RequestContext` — T001 verifies exactly what is observable there; **if no per-session identity is observable on a shared lane, that lane never accumulates** (spec FR-002). The plan's earlier "process-global on HTTP serve — documented" position was refuted (R12) and is withdrawn. **T001 resolved this on 2026-09-02 (research.md R9)**: the HTTP `/mcp` lane is stateless by construction (`mcp_http.rs:125` `with_legacy_session_mode(false)`; rmcp 3.1.4 never creates a session, never issues `Mcp-Session-Id`, clones the shared server and builds a fresh `Peer` per request), so NO per-session identity is observable there. `SessionDiscriminator::observe(&context)` yields `HttpInert` when rmcp's `http::request::Parts` is present in `context.extensions` (only the streamable-HTTP transport inserts it) and `Stdio` otherwise; the tracker never interacts on `HttpInert`. Oracle 8 is the lane-inert pin with the stdio positive control, plus a `legacy_session_mode == false` config pin in `mcp_http.rs`.
7. **Notice delivery** at count ≥ `NOTICE_THRESHOLD = 3`: (a) append the byte-canonical text from [contracts/repeat-notice.md](contracts/repeat-notice.md) (with `\n\n` separator) to the response's final text content block; (b) insert `_meta["symforge/repeat_notice"] = {contract_version: 1, repeat_count, tool, request_hash, evidence_generation}` (same contract — it is the single normative source for both carriers). `isError` untouched. The B5 tip-saturation counter-precedent (`src/protocol/tools.rs:4335-4336`) is satisfied: the notice is threshold-gated, never always-on.
8. **Honest wording**: the notice says "no index change **published**" — never "the files have not changed" — because that is what evidence equality observes (the watcher pipeline is asynchronous; see research.md R6).
9. **Release-gate immunity**: `scripts/verify-tools.cjs`'s readiness loop deliberately repeats the first `search_symbols` snapshot case's exact query in-session (`verify-tools.cjs:334-352, 465-467`), so ≥2 probe iterations would make the snapshot case the 3rd identical serve and append the notice into a release-gate snapshot nondeterministically. The harness's probe gets a fingerprint-distinct argument as part of this feature (R12 adjudication; the alternative — stripping the notice in comparison — hides the surface instead of respecting it).

### US2 — Ledger retry collapse

1. **One generic algorithm, two lane identities**: `collapse_runs(items, identity_key) -> Vec<Run<T>>` in `src/stel/ledger.rs` (the "one pure function over `StelLedgerEvent`" framing was refuted — `recent()` returns `StoredLedgerRecord`, a different string-typed row). Lane identities per [data-model.md](data-model.md): the event lane via exhaustive destructure; the durable lane over stored columns with `session_id` in identity and `accepted`/`eligible_h6` deliberately excluded. Property oracles per lane: counts sum to input length; flattening runs reproduces the identity sequence; N==1 everywhere ⇒ output order/content equals input.
2. **Status view** — a context-shape change, not a formatter tweak (refuted as formatter-local, R12): `StelStatusContext` gains `trailing_run: Option<(count, first_ts_ms, last_ts_ms)>`, computed in `from_server` from `ledger.events()` (`src/stel/status.rs:117-153`); `format_last_ledger_lines` (`status.rs:226-240`) renders byte-identically when the trailing run has length 1 and appends ` ×{N} (first={first_ts_ms}, last={last_ts_ms})` to the **`last_ledger_decision:` line only** when N≥2. All context construction sites absorb the field (`src/protocol/tools.rs:12064, 12130, 13124` + the `sample_context` fixture at `status.rs:421-434`). The proxy overlay reuses the same formatter (`status.rs:282-291`) so worker and proxy cannot drift, and its `starts_with("last_ledger_decision:")` line replacement (`tools.rs:3826`) tolerates a suffix — verified during implementation. Aggregate lines (`ledger_events:`, `durable_ledger:`, calibration) are untouched — computed from uncollapsed storage, which is what makes FR-008 hold by construction.
3. **Admin view** (`src/server/admin/api_v1.rs:65-79`): `LedgerSummaryView` gains `recent_runs` + `recent_runs_window` per [contracts/admin-recent-runs.md](contracts/admin-recent-runs.md) (additive; `[]` when store unavailable — never fake rows), built from the **widened** `StelLedgerStore::recent(50)` (SELECT + `StoredLedgerRecord` gain `pff_bypass`/`cache_hit`/`degrade_flags_json` — columns the table already has), **reversed to chronological before collapse** (`recent()` returns `ORDER BY id DESC`, newest-first — the earlier "newest-last preserved" claim was wrong), with the window-edge run labeled `window_clipped`. `app.js` renders a minimal list (`tests/admin_render.rs` pin extended deliberately).
4. **Blast-radius pins** (verify each, reconcile deliberately, never loosen): `tests/stel_status.rs:141,166-168`; `src/stel/status.rs:482,559,576,597` (in-file tests); `tests/surface_honesty.rs:268,293,314,340-348,420-442,542-563`; `src/protocol/tools.rs:13146-13167,13272` (overlay pins); `tests/admin_api_v1.rs:160-164,210-214` (NOTE: its seeded 3 events are identity-identical — they will collapse ×3; reuse that seed as the FR-008 control); `tests/admin_render.rs:172-186`; `tests/stel_l4_ledger.rs:125-182,241` (additive `StoredLedgerRecord` fields); `tests/stel_ledger_persistence.rs`; `tests/stel_symforge_edit.rs:80-82`; `src/stel/golden_replay.rs:239-240,302-303`; **`scripts/verify-tools.cjs` snapshot fixtures (the missed consumer the adversarial round found — see US1 item 9)**. Survey expectation: only tests that seed ≥2 *identical consecutive* events see changed rendering; everything else is byte-stable — verified during implementation, not assumed.

## Complexity Tracking

> No Constitution Check violations — table intentionally empty.
