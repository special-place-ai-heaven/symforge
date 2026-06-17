# Ponytail Over-Engineering Audit — symforge

**Date:** 2026-06-17
**Scope:** whole `src/` tree (136,161 LOC, 144 Rust files), post-v8 release (HEAD `ecdd8f7`)
**Method:** 5 parallel read-only auditors, one per subsystem cluster, each
hunting only complexity to cut (not correctness, security, or performance).
Every finding cross-checked with a reference count or `file:line` before
inclusion; two auditor findings were verified and **rejected** as false
positives (see end).

**Mandate:** complexity only. Correctness bugs, security holes, and
performance regressions are out of scope and belong to a normal review pass.

---

## Summary

| Tier | Findings | Lines | Risk |
|---|---|---|---|
| Dead code (zero refs) | 6 | ~124 | none — pure deletion |
| YAGNI (one user) | 6 | ~101 | low — inline at single call site |
| Shrink (same logic, fewer lines) | 2 | ~16 | low |
| Structural (real, not free) | 1 | ~150 | medium — careful refactor |
| **Total safe (dead+yagni+shrink)** | **14** | **~241** | low |
| **Total incl. structural** | **15** | **~391** | mixed |

**Dependencies cuttable: 0.** No hand-rolled stdlib, no deps duplicating the
platform. The `serde_yml` / `rmcp` / `tree-sitter-dart` version pins all carry
inline `REVIEW`/justification comments and are deliberate — not debt.

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

Tier 1 subtotal: **−124 lines.**

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

## Tier 4 — Structural (real, but NOT a free cut)

| Tag | What to cut | Replacement | Location | Lines |
|---|---|---|---|---|
| `shrink:` | 21 identical `*_query` getters (each `OnceCell::get_or_init` per language) + a 21-arm language→query dispatch match | a `OnceCell<HashMap<Lang, Query>>` table | `src/parsing/xref.rs:449-545` and `:1000-1082` | ~−150 |

**Caveat — why this is flagged, not recommended as-is.** The getters are
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

Two findings surfaced by the auditors were investigated against the source and
**rejected**. Recorded here so they don't resurface in a future audit.

- **`StelLedgerStore` / `ApiKeyStore` "single-impl enums"** (`Sqlite | Disabled`).
  Flagged as speculative dual-backend future-proofing. **False.** These are the
  never-panic **graceful-degradation contract**: `StelLedgerStore::open`
  returns `Disabled` on any DB-open failure (logged, never panics — FR-011 is
  cited in the doc comment), `record` no-ops when `Disabled`, `recent` returns
  empty. Collapsing to `Arc<SqliteStelLedgerStore>` would push `Option`/`Result`
  handling to all ~16 call sites and delete the degradation seam. This is a
  runtime-state enum, not a single-impl trait. **Keep.**
  (`src/stel/ledger_store.rs:172-200`, `src/server/api_keys.rs:93-172`)

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
  unjustified or removable dependency.

---

## Recommendation

Apply **Tiers 1–3 as one cleanup commit** (~−241 lines, no behavior change,
all dead-code deletion or single-call-site inlining). Hold **Tier 4** as its
own change behind full parser-corpus verification, or skip it. Do **not** act
on the rejected findings.

Per ponytail-debt convention, if any Tier-4 work is deferred rather than done,
leave a `ponytail:` marker at `xref.rs:449` naming the ceiling (21 hand-written
getters) and the upgrade trigger (table collapse after corpus verification) so
the deferral is tracked rather than forgotten.
