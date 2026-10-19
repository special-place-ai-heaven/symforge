# SymForge — OCR Full-Review Deliverable (2026-09-18)

**Source**: OpenCodeReview (GLM-5.3-flash via OpenRouter) scan of `src/` (190/436 files reviewed before the 400k token budget stopped it — 13m33s, 549k tokens; budget guard fired cleanly, see "Coverage" below). Cross-referenced with SymForge's own index (36,190 symbols) for blast-radius ranking.

**Scope note**: findings are real LLM review output, deduplicated and severity-ordered. The high-severity item is ranked first because SymForge confirms it sits on a crate-boundary type used across 6+ modules.

---

## P0 — Fix first

### 1. Fail-open defaults in `CapabilityEvidence::new()` — `src/capability/state.rs:118-122` — **[bug · high]**

`new()` defaults `safety` to `ReadOnly` and `cost` to `Low`, so any producer that forgets `with_safety`/`with_cost` publishes an optimistic (safe+cheap) capability state by accident. For a tool whose whole job is *honest capability reporting*, silently-optimistic evidence is a trust defect.

**Blast radius (SymForge, 25 refs / 6 files)**: `src/protocol/tools.rs` constructs evidence at 5 sites (`cochange_ranking_evidence:1454`, `cochange_lazy_prepare_evidence:1523`, `cochange_stale_prepare_evidence:1561`, `frecency_ranking_evidence:1803`, `ranking_diagnostics_evidence:1835`); `src/analytics/store.rs:132,160` persists it; `src/protocol/format.rs:685` renders it. Every capability line an MCP client sees flows through this constructor.

**Action**: make `new()` require explicit safety+cost (return `Result` or take all four fields), or rename the default path to `new_optimistic()` and migrate the 5 `tools.rs` call sites to explicit values. Add a test asserting no `CapabilityEvidence` is constructed with default safety outside the explicit path.

### 2. Schema version stamping is decorative — `src/analytics/schema.rs:3-15`, `src/analytics/store.rs:311-320` — **[bug · medium]**

`migrate()` unconditionally runs `SCHEMA_V1` and `INSERT OR REPLACE`s `CURRENT_SCHEMA_VERSION` without ever *reading* the stored value. `CREATE TABLE IF NOT EXISTS` is a no-op on a divergent older table, so a legacy DB gets silently stamped "current" while missing constraints. Drift becomes undetectable corruption.

**Action**: `migrate()` must `SELECT` the stored version first, branch on it, and refuse (or upgrade) on mismatch. Pin with a test: create a v0-shaped DB → open → expect a visible error, not silent success.

## P1 — Drift-prone duplication (same root cause)

### 3. `OutcomeClass` exists in three unsynchronized spellings — `src/analytics/schema.rs:22` vs `src/protocol/result_status.rs` — **[maintainability · medium]**
SQL `CHECK` literal list is the third copy (after serde rename and `OutcomeClass::as_str()`). Drift = wire-valid rows that fail inserts.
**Action**: add a unit test that round-trips every `OutcomeClass` variant through `as_str()` against the CHECK list parsed from the schema DDL; generate the CHECK list from the enum if feasible.

### 4. Byte-limit constants duplicated as SQL literals — `src/analytics/schema.rs:15` vs `src/analytics/store.rs:17-21` — **[maintainability · low]**
128/64/128/4096 re-encode `MAX_TOOL_NAME_BYTES` etc. with no linkage. Same drift class as #3; same fix shape (format the DDL from the constants).

### 5. `AUTOINCREMENT` on an append-only analytics table — `src/analytics/schema.rs:14` — **[performance · low]**
Maintains `sqlite_sequence` per insert; plain `INTEGER PRIMARY KEY` (rowid alias) is sufficient since retention deletes by age, not max-id gaps.

## P2 — Hygiene (batch into one cleanup PR)

- **`src/analytics/mod.rs:1`** — facade asymmetry: `queue` private while `schema`/`store` are `pub`; the `queue` re-export checklist concern is already closed per evidence in the comment. Drop or align.
- **`src/capability/policy.rs:3`** — crate-boundary enums (`CapabilitySafety`, `CapabilityCost`, …) have zero doc comments despite encoding domain semantics. They are re-exported through `capability/mod.rs`, so this is public API.
- **`src/capability/policy.rs:31`** — `Display` strings ('lazy on request', 'call-time explain') are the only textual form; no `FromStr`/serde counterpart. Add round-trip impls or assert the asymmetry is intentional.
- **`src/capability/policy.rs:81`** — all-Copy fields → make the constructor `pub const fn`.
- **`src/capability/state.rs:50`** — `Stale` exists in both `CapabilityStatus` and `CapabilityFreshness`; contradictory pairs (`Applied` + `Stale`) are representable. Consider a validation invariant or rename one.
- **`src/capability/state.rs:107`** — derived `Eq` includes free-form `detail: Option<String>`; equality should exclude it or hashing/ comparisons will spuriously differ.

---

## Coverage & caveats

- **Budget stopped the scan** after `src/analytics/*` and `src/capability/*` (warnings list the files cut off mid-review). `src/cli/*`, `src/protocol/*` (partially covered via cross-refs), `src/server/*`, `src/live_index/*` and the rest of the 436-file surface are **not yet reviewed**. Resume: `ocr scan --path src --resume <session>` with a larger budget (see raw session in `ocr-review.json`).
- Two subtask LLM errors (context deadline) on `store.rs`/`queue.rs` — retryable, not data loss.
- Raw machine output: `ocr-review.json` (session `d3ba…`, see `session_id` inside). Progress log: `ocr-review.log`.

## Suggested execution order for a coding agent

1. PR-1 (P0 #1): explicit-capability constructor + migrate the 5 `tools.rs` sites + test.
2. PR-2 (P0 #2): real schema-version check + v0-DB test.
3. PR-3 (P1 #3+#4): single-source DDL from Rust constants + round-trip tests.
4. PR-4 (P2 batch): const fn, docs, FromStr round-trips, Eq field fix, facade cleanup.
