# Phase 1 Data Model: Repeat-Call Notice and Ledger Retry Collapse

All types are in-memory only; nothing here is persisted or serialized except the two wire views defined in `contracts/`. Revised 2026-09-01 after the adversarial round (research.md R12) — the eligible list shrank to 5, the tracker key gained a session discriminator, evidence handling is typed-deserialization-based, and collapse is generic over two lane identities.

## US1 — Repeat tracking (`src/protocol/repeat.rs`, new; server-only by location — `src/protocol/` is `#[cfg(feature = "server")]` at the crate root)

### RepeatKey

| Field | Type | Notes |
|---|---|---|
| `session` | `SessionDiscriminator` | Observed session identity: on a single-client transport (stdio) a process-constant value; on a shared transport, the per-session identity observable at the seam. When NO session identity is observable for a request, the tracker does not accumulate for it at all (spec FR-002 / Scenario 5). |
| `tool` | `String` | MCP tool name as dispatched |
| `request_hash` | `RequestHash` | `RequestHash::for_tool_request(tool, &args)` over canonical JSON args (`src/idempotency.rs:42-57`). NOTE: `RequestHash` currently derives `PartialEq, Eq` but NOT `std::hash::Hash` (`src/idempotency.rs:37-39`) — add `Hash` to its derive (the inner String hashes canonically); `src/idempotency.rs` is in the touched-file set. |

Equality/Hash derive from all three fields. Two calls are "identical" iff `tool` + `request_hash` match; `session` scopes WHERE a run may accumulate (spec FR-001/FR-002; canonical-JSON strictness accepted — false negatives only).

### RepeatRun

| Field | Type | Notes |
|---|---|---|
| `count` | `u32` | Serves of this key with continuously-equal evidence; `saturating_add` |
| `evidence` | `ProjectEvidence` | TYPED evidence deserialized from the run's FIRST serve (`src/protocol/result_status.rs:58-68`). The tracker never stores raw `_meta` JSON — the `{"bound": false}` unavailable marker and any non-deserializable value are "unobserved", full stop. |

State transitions (per eligible-tool response at the seam). "Evidence observed" means: the `symforge/project_evidence` value on the outgoing response deserializes as a full `ProjectEvidence` AND its `project_id` is not the `"unbound"` placeholder (`result_status.rs:149-150` — an unbound adapter has no project to be current about):

```
no entry                --serve, evidence observed-->        count=1, store typed evidence
entry, evidence ==      --serve-->                           count+=1   (notice when count >= 3)
entry, evidence !=      --serve-->                           replace: count=1, new evidence
entry, evidence NOT observed (marker / non-deserializable
        / project_id "unbound") --serve-->                   remove entry (cannot observe)
entry, ResultStatus observed as InternalFailure --serve-->   remove entry (R5)
entry, OutcomeClass unobservable on this lane AND
        result.is_error == true --serve-->                   remove entry (conservative; R5)
any state               --tracker at cap on insert-->        clear() all, then insert fresh
no observable session identity for the request  -->          no tracker interaction at all
```

Outcome classes other than the two clearing rows — including `NotFound`, `EmptyResult`, `InvalidRequest` — ADVANCE the run like any serve (the A019 motivating loop is agents re-issuing failing identical calls). This is the single normative rule; plan.md and research.md R5 defer to it.

Ineligible tools never touch the tracker. Interleaved *different* calls never reset a run (spec FR-002); evidence drift is how "index change" is observed (R6).

### RepeatWitness (unrepresentable-claim guard, Constitution P5)

```
RepeatWitness::observe(prior: &ProjectEvidence, current: &ProjectEvidence) -> Option<RepeatWitness>
```

Private field; `Some` **only** on full struct equality of the two TYPED evidence values. No other constructor. Mirrors the `SourceAuthority::from_freshness` repair (`src/protocol/search_format.rs:3-26`).

### RepeatNotice

| Field | Type | Notes |
|---|---|---|
| `witness` | `RepeatWitness` | Required by the only constructor — a notice without an observed equality is unspellable |
| `repeat_count` | `u32` | ≥ `NOTICE_THRESHOLD` (3) |
| `tool` | `String` | |
| `request_hash` | `RequestHash` | |
| `evidence_generation` | `u64` | From the witnessed evidence, for the wire view |

Renders to: appended text paragraph + `_meta["symforge/repeat_notice"]` — byte-canonical text lives in `contracts/repeat-notice.md` (the contract is the single normative source for the string).

### Constants (all pinned by tests)

| Constant | Value | Rationale |
|---|---|---|
| `NOTICE_THRESHOLD` | `3` | First retry legitimate; second identical retry is the loop signal (spec Assumptions; B5 tip-saturation precedent) |
| `REPEAT_TRACKER_MAX_ENTRIES` | `512` | Overflow → `clear()`; loses only true notices, never creates a false one (R9) |
| `REPEAT_ELIGIBLE_TOOLS` | **5 tools** (R4, post-adversarial) | `search_symbols`, `search_text`, `get_repo_map`, `find_references`, `find_dependents`. `get_symbol` EXCLUDED (repeat serves return the session cache-hit body embedding wall-clock `session_age_secs`, `src/protocol/format.rs:5833-5865`, plus the `force_refresh` dedup footer with elapsed seconds — output varies on an unchanged index). `search_files` EXCLUDED (`rank_by="frecency"` reorders by a `SystemTime::now`-decayed, cross-process-mutable frecency store bumped by interleaved reads, `src/protocol/tools.rs:6650-6710`, `src/live_index/frecency.rs`). |

## US2 — Run-length collapse (`src/stel/ledger.rs`; server-only by location)

`collapse_runs` is **generic over an identity key**: `collapse_runs(items: &[T], key: impl Fn(&T) -> K) -> Vec<Run<T>>` (or two thin typed wrappers) — one algorithm, two lane identities, because the two lanes carry different row types and the "one pure function over `StelLedgerEvent`" framing was refuted (R12: `recent()` returns `StoredLedgerRecord`, not events).

### Lane A identity — in-memory `StelLedgerEvent` (`src/stel_core/types.rs:391-411`), feeds the status view

| Field | In identity? | Why |
|---|---|---|
| `ts_ms` | NO | Wall clock |
| `plan_id` | NO | Embeds wall-clock millis (`src/stel/planner.rs:1230-1236`) |
| `surface` | YES | |
| `intent` | YES | |
| `decision` | YES | |
| `tools_called` | YES | |
| `predicted_response_tokens` | NO | Per-call measurement (spec FR-007) |
| `actual_response_tokens` | NO | Per-call measurement |
| `manual_baseline_tokens` | NO | Per-call measurement |
| `net_vs_manual` | NO | Derived measurement |
| `equivalence` | YES | Always `None` in production today; identity-relevant if it ever isn't |
| `route_confidence` | YES | |
| `pff_bypass` | YES | |
| `cache_hit` | YES | |
| `degrade_flags` | YES | |

The identity comparator **exhaustively destructures** the struct (every field named, ignored ones bound explicitly) so adding a field to `StelLedgerEvent` fails compilation here and forces an identity decision (Constitution P5).

### Lane B identity — durable `StoredLedgerRecord` (`src/stel_core/ledger_store.rs:211-226`), feeds the admin view

`recent()` today SELECTs 13 columns and omits `pff_bypass`, `cache_hit`, `degrade_flags_json`, `accepted`, `eligible_h6`; `equivalence` has no column at all. **Design decision (R12 adjudication): widen `recent()`'s SELECT and `StoredLedgerRecord` additively with `pff_bypass`, `cache_hit`, `degrade_flags_json`** so the durable identity does not silently merge rows that differ in them.

| Field | In identity? | Why |
|---|---|---|
| `id` | NO | Autoincrement |
| `ts_ms` | NO | Wall clock |
| `session_id` | YES | Runs never span sessions (spec Assumptions) |
| `plan_id` | NO | Clock-bearing |
| `surface`, `intent`, `decision`, `tools_called_json`, `route_confidence` | YES | Stored string forms compared verbatim |
| four token columns | NO | Per-call measurements |
| `pff_bypass`, `cache_hit`, `degrade_flags_json` | YES | Newly read back (see decision above) |
| `accepted`, `eligible_h6` | NO — excluded, deliberately | Derived admission/estimator-lane flags (`accepted` derives from `decision`, already in identity; `eligible_h6` is tuning bookkeeping). Recorded here so the exclusion is a decision, not an oversight. |
| `equivalence` | N/A | Not persisted; durable lane cannot compare it — documented coarseness (spec Assumptions) |

Ordering: `recent()` returns `ORDER BY id DESC` (newest first, pinned by `recent_with_limit_caps_result_set`). The admin lane **reverses to chronological (oldest→newest) before collapsing**, so `first_ts_ms`/`last_ts_ms` and "canonical = first event of the run" keep their natural meaning; `recent_runs` renders chronological.

### Run&lt;T&gt; (LedgerRun)

| Field | Type | Notes |
|---|---|---|
| `canonical` | `T` | Chronologically first row of the run (clone) |
| `count` | `u64` | Run length ≥ 1 |
| `first_ts_ms` | `u64` | |
| `last_ts_ms` | `u64` | |

Invariants (property oracles, per lane):
1. `runs.iter().map(|r| r.count).sum() == items.len()`
2. Flattening runs by identity reproduces the input identity sequence in order
3. All-distinct input ⇒ every `count == 1` and rendering is byte-identical to today
4. Admin lane: a run containing the chronologically-oldest fetched row is marked window-clipped (its true start may lie outside the fetch window) — see `contracts/admin-recent-runs.md`

### Status-view plumbing (refuted as a formatter-local tweak — it is a context-shape change)

`format_last_ledger_lines(ctx)` renders from two pre-extracted `Option<String>`s and never sees events (`src/stel/status.rs:75-153, 226-240`). The trailing run is therefore computed in `StelStatusContext::from_server` (from `ledger.events()`) and carried in **new context fields** `trailing_run: Option<(u64 /*count*/, u64 /*first_ts_ms*/, u64 /*last_ts_ms*/)>`; every context construction site absorbs the field (`src/protocol/tools.rs:12064, 12130, 13124` and the in-file `sample_context` fixture at `src/stel/status.rs:421-434`). The `×N` suffix goes on the `last_ledger_decision:` line ONLY (single-line placement, byte-exact in the oracle); the proxy overlay's `starts_with("last_ledger_decision:")` line replacement (`src/protocol/tools.rs:3826`) tolerates a suffix — verified during implementation, not assumed.

### LedgerRunView (admin wire type)

See `contracts/admin-recent-runs.md`. Additive field on `LedgerSummaryView` (`src/server/admin/api_v1.rs:65-79`); `[]` when the store is unavailable (never fabricated rows — matches the existing `available:false` / nulls honesty).
