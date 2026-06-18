# Ponytail Over-Engineering Audit — symforge

**Date:** 2026-06-17 · **Refreshed:** 2026-06-18 (post 010 v8-trust-remediation merge, HEAD `3e59b2d`)
**Scope:** whole `src/` tree (~138 KLOC, 144 Rust files) + `tests/`.
**Method:** 5 parallel read-only auditors (2026-06-17), one per subsystem
cluster, each hunting only complexity to cut. Refreshed 2026-06-18 with a
single-pass re-verification + a delta scan of the 010 changes. Every finding
cross-checked with a reference count or `file:line`; false positives verified
and **rejected** (see end).

**Refresh result:** every 2026-06-17 finding **re-verified still present and
unapplied** — Tiers 1–3 are not yet cut. The 010 merge was additive in separate
files and introduced **two** new findings (one dead getter, one shrinkable test),
folded into the tiers below and marked `[010]`. No 2026-06-17 finding was
invalidated.

**Mandate:** complexity only. Correctness bugs, security holes, and
performance regressions are out of scope and belong to a normal review pass.

---

## Summary

| Tier | Findings | Lines | Risk |
|---|---|---|---|
| Dead code (zero refs) | 7 | ~128 | none — pure deletion |
| YAGNI (one user) | 6 | ~101 | low — inline at single call site |
| Shrink (same logic, fewer lines) | 2 | ~16 | low |
| **Total safe (dead+yagni+shrink)** | **15** | **~245** | low |
| Structural — xref query table | 1 | ~150 | medium — careful refactor |
| Shrink — honesty_gate parser `[010]` | 1 | ~120 | low, but correctness-bearing test |
| **Total incl. own-change items** | **17** | **~515** | mixed |

**Dependencies cuttable: 0.** No hand-rolled stdlib, no deps duplicating the
platform. The `serde_yml` / `rmcp` / `tree-sitter-dart` version pins all carry
inline `REVIEW`/justification comments and are deliberate — not debt. The 010
honesty gate hand-rolls a small markdown-table parser, but adding a table-parser
dep to save lines is the wrong trade by the audit's own ladder (rung 4) — keep it
hand-rolled, just tighter.

---

## Tier 1 — Dead code (delete, zero references)

Highest confidence. Each verified to have no live caller (tests-only or none).

| Tag | What to cut | Replacement | Location | Lines |
|---|---|---|---|---|
| `delete:` | `spot_verify_sample` — unused pub fn, verification sampling, no callers | nothing | `src/live_index/persist.rs:517-590` | −78 |
| `delete:` | test-only `DAEMON_PORT_FILE` / `DAEMON_PID_FILE` / `DAEMON_START_LOCK_FILE` consts aliasing `LEGACY_*`; production uses the `*_file_name()` fns | point tests at the legacy consts | `src/daemon.rs:50-61` | −12 |
| `delete:` | `open_existing_readonly` — unused pub fn returning readonly DB handle, called only in tests | nothing | `src/live_index/frecency.rs:108-117` | −10 |
| `delete:` | `AapView.indexed_roots` field + `aap_indexed_roots()` — always returns `[aap_root]` or `[]` | hardcode at the one call site | `src/server/admin/api_v1.rs:341-357` | −10 |
| `delete:` | `stat_check_files` — unused pub fn, internal wrapper only | nothing | `src/live_index/persist.rs:439-446` | −7 |
| `delete:` | `exact_lines` — unused pub fn wrapping `for_explicit_path_read` | nothing | `src/live_index/search.rs:623-629` | −7 |
| `delete:` `[010]` | `current_rejected_stale_mutations` — `#[allow(dead_code)]` getter, no caller (the `rejected_stale_mutations` counter is written by `note_rejected_stale_mutation` but never read) | nothing | `src/live_index/store.rs:775-778` | −4 |

Tier 1 subtotal: **−128 lines.**

---

## Tier 2 — YAGNI (abstraction with one user)

Each is a layer, wrapper, or extension point with exactly one (or zero
non-test) callers. Inline at the call site.

| Tag | What to cut | Replacement | Location | Lines |
|---|---|---|---|---|
| `yagni:` | admin view-wrappers `KeyRecordView` / `HarnessEntryView` / `AapPresetsView` — each duplicates a serde struct that already carries the fields; the only added value is one derived field (e.g. `active = revoked_ms.is_none()`) | serde the source type directly, compute the derived field inline | `src/server/admin/api_v1.rs:117-188, 237-251, 361-383` | −45 |
| `yagni:` | `main.rs` micro-wrappers: `checkpoint_interval_from_value`, `checkpoint_interval_from_env`, `local_empty_reason`, `StartupPlan`/`StartupIndexLogView` enums — each used at exactly one call site | inline | `src/main.rs:14-33, 57-89` | −35 |
| `yagni:` | `record_tool_savings` — single caller, duplicates `record_tool_savings_named` | inline at `tools.rs:8965` | `src/protocol/mod.rs:464-474` | −11 |
| `yagni:` | `rank_signals::register()` — `RankSignal` registration extension point called only from tests | drop or `#[cfg(test)]` | `src/live_index/rank_signals.rs:318-321` | −6 |
| `yagni:` | `WalkerConfig.include_symbols` — set in `with_now()`, never read; speculative symbol-level ledger feature | drop the field | `src/live_index/coupling/walker.rs:34` | −4 |
| `yagni:` | `InitPaths::from_home_and_working_dir` — trivial wrapper called once, passes `None` for appdata | inline into the one caller (`init.rs:243`) | `src/cli/init.rs:56-62` | −6 |

Tier 2 subtotal: **−107 lines** (some overlap with the −101 summary figure;
the table is the authoritative count).

---

## Tier 3 — Shrink (same logic, fewer lines)

| Tag | What to cut | Replacement | Location | Lines |
|---|---|---|---|---|
| `shrink:` | 3 copies of path normalization (`replace('\\',"/").trim_start_matches("./")`) in `rank_signals::normalize`, `frecency::normalize_path`, `git_temporal::normalize_git_path` | one shared `normalize_repo_path` helper | rank_signals.rs:122, frecency.rs:353, git_temporal.rs:738 | −12 |
| `shrink:` | `RankCtx::default()` impl exactly duplicates `RankCtx::empty()` | `impl Default` delegates to `empty()` | `src/live_index/rank_signals.rs:89-93` | −4 |

Tier 3 subtotal: **−16 lines.**

---

## Tier 4 — Own-change items (real, but NOT a free cut)

| Tag | What to cut | Replacement | Location | Lines |
|---|---|---|---|---|
| `shrink:` | 21 identical `*_query` getters (each `OnceCell::get_or_init` per language) + a 21-arm language→query dispatch match | a `OnceCell<HashMap<Lang, Query>>` table | `src/parsing/xref.rs:449-545` and `:1000-1082` | ~−150 |
| `shrink:` `[010]` | `honesty_gate.rs` is 753 lines (~290 hand-rolled markdown-table parser + ~300 fixtures/tests + types) to enforce 3 invariants across 2 tables | tighten `parse_register`/`parse_matrix` (one shared row-iterator); keep it hand-rolled (no md-table dep — rung 4) | `tests/honesty_gate.rs` | ~−120 |

**Why honesty_gate is here, not in the free-cut tiers.** It is correctness-
bearing (it is the FR-018 honesty enforcement). Shrinking it is no-behavior but
must keep every T042 fixture failing/passing exactly as now — verify against the
7 existing cases, do not loosen the gate to save lines. Low risk, own change.

**Caveat — why the xref table is flagged, not recommended as-is.** The getters are
mechanically identical in shape, but each binds a *distinct* `static OnceCell`
and a *distinct* query const, and Rust + Python carry extra query passes
(`rust_const_def_query`, `rust_value_ident_query`,
`python_value_type_ident_query`, `python_string_type_query`) that don't fit a
uniform table. Collapsing them is a careful refactor with real regression
surface across 21 languages, not a mechanical sweep. Take it as its own
change with full parser-corpus verification, or skip it. It is the only
finding in this audit with non-trivial risk.

---

## Rejected by verification (auditor false positives — do NOT cut)

Findings surfaced by the auditors (2026-06-17) and the trait scan (2026-06-18)
were investigated against the source and **rejected**. Recorded here so they
don't resurface in a future audit.

- **`StelLedgerStore` / `ApiKeyStore` "single-impl enums"** (`Sqlite | Disabled`).
  Flagged as speculative dual-backend future-proofing. **False.** These are the
  never-panic **graceful-degradation contract**: `StelLedgerStore::open`
  returns `Disabled` on any DB-open failure (logged, never panics — FR-011 is
  cited in the doc comment), `record` no-ops when `Disabled`, `recent` returns
  empty. Collapsing to `Arc<SqliteStelLedgerStore>` would push `Option`/`Result`
  handling to all ~16 call sites and delete the degradation seam. This is a
  runtime-state enum, not a single-impl trait. **Keep.**
  (`src/stel/ledger_store.rs:172-200`, `src/server/api_keys.rs:93-172`)

- **`OnboardingSink` / `AnalyticsWriter` single-impl traits → "inline the one
  impl".** Surfaced by the 2026-06-18 trait scan; **rejected** — both are
  deliberate **test seams**. `OnboardingSink` (`src/cli/onboarding.rs:56`) has
  `StderrSink` (prod) + `RecordingSink` (test), module doc: exists "so tests never
  launch a real browser". `AnalyticsWriter` (`src/analytics/queue.rs:40`) is a
  generic bound letting the queue be tested without a real store. Inlining either
  deletes the seam that keeps those tests off real I/O. **Keep.**

- **010 fixes flagged as new structure → keep.** `PATH_WRITE_LOCKS` /
  `lock_for_path` (`src/protocol/edit.rs:266`, the per-path write-serialization that
  closes the if_match TOCTOU), `ground_plan_economics` (`src/protocol/tools.rs:5445`),
  `empty_index_recovery_hint` (`src/protocol/format.rs:4789`) are load-bearing,
  tested trust-remediation fixes — not bloat. **Keep.**

- **`constant_time_eq` hand-roll → "use `subtle`/`ring`".** Out of scope: this
  is a timing-safe comparison, i.e. a correctness/security concern, not a
  complexity concern. And adding a dependency to save ~20 lines is the wrong
  trade by the audit's own ladder (rung 4 prefers no new dep). **Keep.**
  (`src/server/auth.rs:307-328`)

---

## What came up empty

- **`stdlib:`** — no hand-rolled standard-library reimplementations found. Sorts,
  dedups, and group-bys use `Vec`/`HashMap`/`rayon` directly.
- **`native:`** — no dependency duplicating a platform feature; the admin GUI
  assets (`app.js`, `style.css`) are small and use the browser directly, no
  framework to cut.
- **deps** — every version pin in `Cargo.toml` flagged by a quick scan carries
  an inline justification comment (`serde_yml` libyaml-lineage rationale,
  `rmcp` REVIEW P3-C deferral, `tree-sitter-dart` corpus evidence). No
  unjustified or removable dependency. **Re-confirmed 2026-06-18:** the 010 merge
  added no new dependency; deps cuttable stays **0**.

---

## Recommendation

Apply **Tiers 1–3 as one cleanup commit** (~−245 lines, no behavior change, all
dead-code deletion or single-call-site inlining — incl. the `[010]` dead getter
at `store.rs:775`). Hold the two **Tier 4** own-change items separately: the xref
query table behind full parser-corpus verification (or skip), and the
`honesty_gate.rs` shrink verified against its 7 existing T042 cases (or skip). Do
**not** act on the rejected findings.

Per ponytail-debt convention, if either Tier-4 item is deferred rather than done,
leave a `ponytail:` marker at its site (`xref.rs:449` — 21 hand-written getters,
collapse after corpus verification; `tests/honesty_gate.rs` top — hand-rolled
md-table parser, tighten while keeping the gate exact) so the deferral is tracked
rather than forgotten.

---

## Applied — 2026-06-18 (every item tackled, correctness-first)

Driven via rust-pro under the rule **correctness + superiority, no duplicates /
inferior code / nonworking features**, and the operator's clarification that
**vision-aligned incomplete seams are not dead code — they are preserved/wired,
not cut**. Each finding was re-verified against live code before acting; the
audit itself was **wrong on 5 items** (it over-flagged live code as dead). Gate
green after the pass (3014 tests, fmt/clippy/build/embed).

| Item | Outcome |
|---|---|
| T1 `spot_verify_sample` / `stat_check_files` pub wrappers | **CUT** — redundant public entry points; the verification **seam is LIVE** via `*_from_view` in `background_verify` (`persist.rs:750`, 10% hash spot-check → re-index). Seam preserved, only the dead wrapper removed. |
| T1 `exact_lines` | **CUT** — truly dead (0 refs), no seam behind it. |
| T1 `AapView.indexed_roots` field | **KEPT** — consumed by the admin GUI (`app.js`) + wire test; only the trivial `aap_indexed_roots()` helper inlined. |
| T1 daemon `DAEMON_*` test aliases | **CUT** — repointed tests to the live `LEGACY_*` consts. |
| T1 `open_existing_readonly` `[audit wrong]` | **KEPT** — live prod caller via `search_files` → `ranking_scores_for_paths`. Not test-only. |
| T1 `current_rejected_stale_mutations` `[010, audit wrong]` | **KEPT** — read by the watcher + health tools. Not dead. |
| T2 `record_tool_savings`, `checkpoint_interval_from_*`, `InitPaths::from_home_and_working_dir` | **CUT/INLINED** — genuine duplicates / single-call wrappers. |
| T2 `rank_signals::register()` | **PRESERVED AS SEAM** — public extension point of the live ranking framework (`combine` runs in prod with default signals). NOT gated to tests, NOT cut; marked `ponytail:` as a vision-aligned seam awaiting its first contributor. |
| T2 admin view-wrappers `[audit wrong]` | **KEPT** — perform real enum→string transformation; source types don't derive `Serialize`. Not duplication. |
| T2 `WalkerConfig.include_symbols` `[audit wrong]` | **KEPT** — read at `walker.rs:312` to gate symbol-level ledger emission. Not unused. |
| T2 `local_empty_reason` / `StartupPlan` / `StartupIndexLogView` `[audit wrong]` | **KEPT** — multi-caller / typed-and-tested state seams. |
| T3 path-normalize ×3 | **DEDUPED** — one shared `paths::normalize_repo_path`; the `rank_signals` variant that also trims `./` was deliberately left distinct (real behavioral difference). |
| T3 `RankCtx::default` | **NO-OP** — already delegates to `empty()`. |
| T4 xref query table, honesty_gate shrink | **PENDING** — own gated changes, in progress. |

**Net applied:** the genuine dedup/dead-code cuts (no seam removed); **6 audit
findings corrected as false positives or seams** (kept). This is the seam rule in
practice: the audit's "dead code" tier contained live trust/ranking seams, and
they were preserved, not cut.
