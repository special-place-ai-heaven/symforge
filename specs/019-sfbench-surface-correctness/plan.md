# Implementation Plan: SFBENCH Surface Correctness & Safety

**Branch**: `019-sfbench-surface-correctness` | **Date**: 2026-07-12 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/019-sfbench-surface-correctness/spec.md`

## Summary

Eight independently-verified defects from the SFBENCH-1.0 8.14.0 benchmark
(six from the initial grading + two surfaced by the adversarial review of this
very spec), fixed in verified-severity order, each with a **fail-first
regression test**, landed and gated independently. Two safety/correctness P0
(`batch_rename` unsafe write, `detect_impact` identity), two P1 (selector +
Python relative import; **watcher cold-start staleness — added after the review
refuted my own refutation**), four lower (replay P2, health counter P2, daemon
calibration-reset no-op P2, BOM/estimate P3). Severity order:
US1 → US2 → US3 → US5b → US4 → US5a → US5c → US6. The benchmark's meta P0 is
excluded (verified non-bug).

> [!IMPORTANT]
> **Code is gospel; no document is trusted — including this plan and the
> benchmark report.** Every file:line below is a *verified-at-2026-07-12
> starting anchor*, not scripture. **Step 0 of every item is to re-confirm the
> anchor against live source** (`get_symbol` / `get_symbol_context` /
> `search_text`). If the code moved, the guard already changed, or the behavior
> no longer reproduces, the item is **re-opened** — write the failing test
> first and let the *running test*, not the prose, decide whether a bug exists.
> A benchmark says a thing is broken; only a red test on current source proves
> it, and only a green gate proves it fixed.

## Technical Context

**Language/Version**: Rust 2024 (single crate `symforge`). No new dependencies.

**Primary Dependencies**: tree-sitter (Tier-0 xref, pinned `tree-sitter-python`
0.25.0), `git2`, in-process `LiveIndex`, STEL controller, MCP protocol surface,
`serde_json` (config validation). No new deps introduced.

**Storage**: in-process `LiveIndex` + local `.symforge/` snapshots. No external
store, no schema/persistence change (Constitution I).

**Testing**: `cargo test --all-targets -- --test-threads=1`; per-story
fail-first regression tests colocated with the touched tool's `#[cfg(test)]`
module or under `tests/`; controlled-graph assertions reuse the SFBENCH fixture
shape (`sfbench_entry → sfbench_mid → sfbench_leaf`, `sfbench_duplicate`,
`sfbench_rename_me`) where a local equivalent is cheaper.

**Target Platform**: local dev host (Windows/Linux/macOS); stdio + `serve`.

**Project Type**: single-crate Rust MCP server. Not web/mobile.

**Performance Goals**: no regression. US2 makes `detect_impact` *smaller* and
faster (fewer seed symbols, scoped edges). US1 adds identity checks on an
already-bounded reference set. No path becomes super-linear.

**Constraints**: Constitution I–VIII (below). Frecency-neutral read paths,
deterministic ordering, embed build green, stdio↔serve parity, full gate,
fail-closed mutation safety.

**Scale/Scope**: six surgical fixes. US1/US2 share a small identity concept but
are landed as **separate commits with separate tests**; the rest are disjoint.

## Constitution Check

*GATE: evaluated against constitution v1.0.0, all eight principles. Re-check
after each item lands.*

| # | Principle | Assessment | Verdict |
|---|-----------|------------|---------|
| I | Local-First In-Process Index | All fixes read/act on the existing `LiveIndex`; no second index, no external store. US2 tightens graph projection *within* the one index. | PASS |
| II | MCP-Native Surface | No new tools. Behavior corrections to `batch_rename`, `detect_impact`, `analyze_file_impact`, three search tools, `symforge_edit`, `health`, `validate_file_syntax`. No chat injection, no client-tool shadowing. | PASS |
| III | Trust Envelopes | **US1 strengthens** (label reflects real binding confidence; unsafe writes become disclosed-uncertain). US5 strengthens health honesty (no no-op overcount). US2 blast nodes carry identity. No disclosure removed. | PASS |
| IV | Determinism & Recovery | **US1 is a fail-closed mutation-safety fix** (core of this principle). US2/US3 keep deterministic ordering; tests assert determinism and zero-write on uncertain/replay paths. | PASS |
| V | Frecency Invariant | US2/US3/US6 are discovery/read paths and MUST NOT write frecency; US1/US4 are mutation paths whose frecency behavior is unchanged. Frecency-neutrality assertion on touched read paths. | PASS |
| VI | Embed Isolation | Changes live in parsing/query/graph/protocol/edit paths present in `embed`; no server/network dep added. `cargo check --no-default-features --features embed` gate per item. | PASS |
| VII | Transport Parity | All behaviors flow through shared handlers/formatters both transports call. Any shared formatter/signature change (US1 label, US2 blast node, US5 counter) carries a parity assertion. | PASS (verify per item) |
| VIII | Verification Before Done | Full gate + embed check + fail-first test per story, on current source, before any completion claim. **No item is "done" on a green document — only on a green gate.** | PASS |

**No violations → Complexity Tracking empty. Cleared for Phase 0.**

## The fix loop (applied to every item, in order)

Each item is one pass of a fixed, test-first loop. Do **not** batch items; land
and gate one at a time so a regression is attributable.

```text
0. RE-CONFIRM  Locate the real code by symbol name (not the cached line number).
               Read it. Confirm the defect still reproduces conceptually. If the
               anchor moved or a guard already exists → re-open, adjust, or drop.
1. RED         Write the smallest failing test that encodes the acceptance
               scenario against CURRENT source. Run it. WATCH IT FAIL for the
               right reason (assert the actual wrong output, not just "errors").
2. GREEN       Make the minimal change that binds identity / scopes the seed /
               fixes the ordering. No speculative abstraction; smallest diff that
               turns the test green and holds the acceptance scenario.
3. GATE        cargo fmt --check; cargo check; cargo clippy --all-targets
               -D warnings; cargo test --all-targets -- --test-threads=1;
               cargo check --no-default-features --features embed. (release
               build + npm at end-of-feature.) Green before moving on.
4. NEIGHBORS   Rerun the neighbor cases the change could disturb (listed per
               item). Confirm no adjacent regression.
5. COMMIT      One conventional commit per item on this branch. Then next item.
```

Disk discipline (CLAUDE.md): `target/` is on E:. Run full gates locally, but
`cargo clean` before ending a heavy session; `cargo clean` first if
`target/debug` is already large.

---

## Item order and per-item plan

### Item 1 — US1 `batch_rename` fail-closed identity binding (P0 SAFETY, MVP)

- **Starting anchors (re-confirm first)**: `execute_batch_rename`
  (`src/protocol/edit.rs`, ~L2256–2618): target resolved only for def site;
  `find_references_for_name(&input.name, None, false)` (~L2287, name-only, no
  scope); `code_only` language-class filter (~L2296/L2320); qualified scan
  (~L2334); `dry_run.unwrap_or(false)` apply-default (~L2405); write loop
  (`atomic_write_file`). Trust label `MatchType::Constrained`
  (`src/protocol/edit_tools.rs`, "project-wide constrained references").
  Reference lookup: `src/live_index/query.rs` `find_references_for_name` /
  `collect_refs_for_key` (simple-name reverse index).
- **RED**: fixture with a **shared method name** having 2+ unrelated owners
  (e.g. `Target::run` in file A + unrelated `Widget::run` / Flask handler `run`
  in file B). `batch_rename(name="run", path="A")` **apply**; assert file B
  byte-hash unchanged and B's sites reported uncertain. **Python arm is the
  mandatory gating fixture** (that is where the recorded escape happened).
  Explicitly **forbid the honeytrap shape**: the existing green test
  `test_batch_rename_scopes_common_name_to_target` renames the *unique* struct
  `Target` and asserts an untouched-by-construction `new` — a plausible-looking
  "common name" test that exercises nothing. The RED test must rename a shared
  name with real ambiguity.
- **GREEN (corrected — review B3)**: key writability on **index-wide name
  ambiguity**, NOT on "has resolved identity". Bare-name Tier-0 refs never carry
  identity, so identity-keying would demote *every* rename and regress the green
  tests. Rule: **name with exactly 1 definition → keep all bare-name refs
  writable + "constrained" (unchanged); name with 2+ definitions → demote
  name-only refs to uncertain / non-writable, correct the label; if no ref of an
  ambiguous name can be safely bound → fail closed.** Honest ceiling
  (`research.md`): Tier-0 xref carries no resolved identity for dynamic
  languages, so for an ambiguous name the safe default is uncertain, not "guess".
- **MUST-NOT-REGRESS (named)**: `batch_rename_updates_definition_and_callers`
  (asserts the bare `old_name();` site IS rewritten + "constrained"),
  `batch_rename_bumps_definition_and_call_site`, and the frecency-ranking rename
  path. These pass because those names are unique in their fixtures.
- **NEIGHBORS**: all `batch_rename` language cases; `find_references` (shared
  lookup); `symforge_edit` rename intent; `batch_edit`/`batch_insert` rollback
  behavior (shared write path).
- **DONE WHEN**: SC-001. Zero unrelated writes across Rust/Python/TS + one of
  Go/Java/C++; ambiguity test green; label honest; gate green.

### Item 2 — US2 `detect_impact` changed-symbol delta + scoped edges (P0 CORRECTNESS)

- **Starting anchors (re-confirm first)**: seed loop
  (`src/protocol/tools.rs`, ~L7463–7476, seeds every `file.symbols` of each
  changed path as `SymbolId{path,name,kind}`); `GraphProjection::from_index`
  (`src/live_index/graph.rs`, L66–133, `defs_by_name` keyed on bare name; Pass 2
  links each `Call` to *every* same-name def); `is_entry_point` (graph.rs
  L275–277, matches any `fn main`); Symbols-scope formatter (tools.rs ~L7493,
  drops path/kind); `analyze_file_impact` same-file filter
  (`src/sidecar/handlers.rs`:1326) + bare-name lookup (:1323) **with** existing
  parent-type narrowing (:1305–1321) — do not remove that. Prior evidence:
  `specs/015-cbm-capability-ports/contracts/detect-impact.md:104–111` (the
  291K/54MB explosion + 200-truncation band-aid).
- **RED**: controlled graph; body-only edit to `sfbench_leaf`. Assert exactly
  `{sfbench_leaf}` changed and hop-1 `{sfbench_mid}`. Second test: two `run`
  defs → change one → assert the other never enters the blast and nodes carry
  path/kind. Third: comment-only shift → zero changed symbols.
- **GREEN**: (a) seed only genuinely added/modified/removed symbols by comparing
  body hashes, not all symbols of the file. **Sizing (review M4)**: this is
  **net-new machinery**, not a seed tweak — the live index holds only *current*
  bodies and `SymbolRecord` has no per-symbol body hash (only file-level
  `content_hash`). Source the base bodies by reusing `diff_symbols`' git-blob
  reparse (`tools.rs` ~L10859 `changed_paths_between_refs`); enumerate removed
  symbols from the base parse (they are absent from the current index). The
  "smaller/faster" claim must be re-validated against this added base-parse cost.
  (b) scope edge resolution by **callee-name ambiguity** (refined design, same
  principle as US1): when a `Call` ref's name resolves to **exactly one
  definition** in `defs_by_name`, link it (bare-name is correct and unambiguous —
  this preserves `compute_impact_reaches_across_qualified_module_call`, where
  `call_a` has one def); when it resolves to **multiple** definitions, the call
  cannot be attributed to a specific one from syntax alone, so **do not fan out
  to all of them** — either use the module-qualifier from the call site if
  present to disambiguate, or drop the ambiguous edge rather than invent N wrong
  ones. This is what cuts the duplicate-`main` explosion (`main` is multiply
  defined) without dropping the legitimate single-def module-sibling edge.
  Formatter identity (c) is the backstop for any remaining same-name nodes. (c) formatter emits path/kind so duplicate `main` nodes are
  distinguishable; (d) tighten `analyze_file_impact`'s residual bare-name lookup
  to typed identity while keeping same-file callers and the parent-type
  narrowing.
- **CAVEAT (verified)**: fixing the seed alone is insufficient — hop-1+ still
  over-connects via bare-name edges. Both (a) and (b) are required. The RED test
  MUST include the two-same-name case (SC-002 clause 2); a green on the
  unique-name leaf edit alone does NOT prove (b). The entry-point-collapse clause
  needs a fixture with a reachable duplicate `main` (the SFBENCH fixture has
  none) — add one or mark that clause untested-on-fixture.
- **MUST-NOT-REGRESS (named)**: `compute_impact_reaches_across_qualified_module_call`
  (graph.rs ~L419), the risk-tier tests, and 018's `code_only` data-file
  defaulting.
- **NEIGHBORS**: all `graph.rs` `compute_impact` tests; `diff_symbols`
  (shares the delta concept); `what_changed`; `analyze_file_impact` cases;
  018's `code_only` data-file defaulting (must remain intact).
- **DONE WHEN**: SC-002. Leaf edit → 1 changed + 1 hop-1; comment shift → 0;
  no duplicate/unrelated blast nodes; 015 explosion no longer reproduces without
  relying on truncation; gate green.

### Item 3 — US3 selector consistency + Python relative import (P1 CORRECTNESS)

- **Starting anchors (re-confirm first)**: `search_symbols`
  (`src/protocol/tools.rs`:4986), `search_text` (:5149), `find_references`
  (:7941) call `local_cross_project_refusal` (tools.rs:2787–2804, refuses any
  selector); siblings call `foreign_project_refusal` (tools.rs:6785, allows
  matching-local). `PYTHON_XREF_QUERY` (`src/parsing/xref.rs`:72–121) — no
  `relative_import` capture; consumers `find_dependents_for_file` +
  `matches_target_import`/`matches_target_stem` (`src/live_index/query.rs`:364–392).
- **RED (selector)**: for each of the three tools, a matching-local selector
  currently refuses — assert it *should* succeed and equal the no-selector
  result; a foreign selector asserts typed refusal. Cover both `project` and
  `projects` incl. `"*"`. **RED (import)**: Python pkg with `from .b import f`;
  assert `find_dependents(pkg/b.py)` includes `pkg/a.py` and excludes an
  unrelated same-stem module.
- **GREEN**: centralize selector validation so all selector-bearing tools use
  one guard covering both params and both directions (match-local → proceed;
  foreign/over-broad → typed refuse) — **do not** naively swap the three tools to
  `foreign_project_refusal` (it takes only singular `project` and, per P2-10,
  under-refuses `projects=["*"]`). Add a `relative_import` capture to
  `PYTHON_XREF_QUERY` and resolve the target by counting `import_prefix` dots
  against the importer's package directory.
- **NEIGHBORS**: `get_symbol`/`search_files`/`get_file_context` selector paths
  (must stay correct); daemon vs local path (selector bug only manifests
  local); `find_dependents` TS/Go cyclic/aliased import cases; 017 selector
  ranking work.
- **DONE WHEN**: SC-003 + relative-import dependent test green; both selector
  parameters consistent both directions; gate green.

### Item 4 — US4 `symforge_edit` replay-before-concurrency (P2 CORRECTNESS)

- **Starting anchors (re-confirm first)**: `symforge_edit_stel_handler` local
  apply branch (`src/protocol/tools.rs`:10040–10159) runs `run_pre_apply_gates`
  **before** `begin_mutation_replay`; `if_match` guard in
  `src/stel/edit_apply.rs`:71–149 (~L130–137) rejects with no idempotency
  exemption (`pre_apply_rejects_if_match_mismatch` test :259–281); replay lookup
  `begin_tool_replay` (`src/protocol/idempotency.rs`:439) reached only later.
- **RED**: apply `{key, request, if_match}`; replay identical → assert stored
  result + zero writes (currently rejected 3/3). Plus: same key + changed
  request → conflict; new key + stale `if_match` → concurrency fail.
- **GREEN**: perform a **non-reserving** exact replay probe (read-only, does not
  consume the reservation) before `run_pre_apply_gates`; on an identical
  key+request hit, return the stored result without re-validating the now-stale
  `if_match`. Keep `FirstExecution` vs `Replay` reservation semantics intact.
  Caveat (verified): the probe's request hash must be computed so `if_match`
  presence does not reintroduce the mismatch.
- **NEIGHBORS**: granular edit tools' replay (`replace_symbol_body`,
  `edit_within_symbol`, `delete_symbol`); STEL concurrency tests; idempotency
  conflict tests.
- **DONE WHEN**: SC-004; conflict + concurrency guards still fire; gate green.

### Item 5b — US5b watcher cold-start generation staleness (P1 CORRECTNESS)

> **Scheduling**: this is P1 and should land in the P1 wave (with/after Item 3),
> BEFORE the P2 items — the item numbering keeps the watcher cluster together but
> the severity order is US1, US2, US3, **US5b**, US4, US5a, US5c, US6.

- **Starting anchors (re-confirm first — verified 2026-07-12)**: cold-start
  bootstrap in `src/main.rs`: `tokio::task::spawn_blocking(|| bg_index.reload(
  &bg_root))` is **fire-and-forget** (not awaited; `reload` bumps
  `project_generation` at `src/live_index/store.rs`:800), then the watcher is
  spawned unconditionally via `run_watcher` (~`main.rs`:432) which captures
  `expected_gen` **once** at `src/watcher/mod.rs`:721. Cheap spawn wins the race
  vs the ~220 ms reload → captures pre-reload gen → all later
  `freshen_file_if_stale` / `reconcile_stale_files_with_stop` see
  `GenerationMismatch` → file removed, never re-indexed. (Contrast: daemon
  `reload_with` `daemon.rs`:2761–2772 and stdio `index_folder` tools.rs:6931–6971
  DO restart the watcher — those are fine.)
- **RED**: cold-start a snapshot-less repo with a deterministic seam that lets the
  reload bump the generation before the watcher captures it (or observe
  captured-vs-current gen); modify a tracked file; assert it is **indexed**, not
  `GenerationMismatch`-removed, and `expected_gen == current_project_generation()`.
- **GREEN** (pick one, justify in `research.md`): (i) **await** the cold-start
  reload before spawning the watcher (reorder `main.rs` so the watcher captures
  the post-reload gen — simplest, small latency cost to first-watch); or (ii) have
  `run_watcher_with_stop` **re-read** `current_project_generation()` per committed
  reconcile/watcher batch instead of capturing once; or (iii) subscribe the
  watcher to generation changes. Keep the generation fence for genuine
  concurrency races intact.
- **NEIGHBORS**: daemon/index_folder restart paths (must stay correct); watcher
  generation-fence tests (`lsn_b1f83a91` established this pattern); snapshot-warm
  start; US5a counter (shares the same reconcile path).
- **DONE WHEN**: SC-005a; cold-start edit indexed; fence still rejects real
  in-flight stale mutations; gate green.

### Item 5a — US5a health reconcile-repair counter honesty (P2 OBSERVABILITY)

- **Starting anchors (re-confirm first)**: `reconcile_stale_files_with_stop`
  (`src/watcher/mod.rs`:505) counts every `true` from
  `freshen_file_if_stale_at_generation` (:487), which returns `true` for
  `FreshenResult::GenerationMismatch` (a no-op) as well as real
  `StaleReindexed`/`StaleRemoved`; surfaced by health
  (`src/protocol/format.rs`:1903–1909, 2055–2079).
- **RED**: reconcile pass over generation-mismatched files (no real reindex);
  assert reported repair count is 0 for those; genuine stale files still count;
  rejected attempts surfaced separately.
- **GREEN**: distinguish `GenerationMismatch` from real repairs in the counter;
  count only `StaleReindexed`/`StaleRemoved`; expose rejected attempts as a
  distinct figure (or omit), never as "repairs".
- **NOTE**: causally linked to US5b — fixing the cold-start race drains most of
  the `GenerationMismatch` no-ops. Land US5b first; this counter fix is the
  honest-signal backstop for the residual legitimate-race window.
- **NEIGHBORS**: health full + compact formatters; watcher generation-fence
  tests; any snapshot-restore health test.
- **DONE WHEN**: SC-005b; health repair count excludes no-ops; gate green.

### Item 5c — US5c daemon `status(reset_calibration=true)` durable reset (P2 CORRECTNESS)

- **Starting anchors (re-confirm first — verified via review)**: `status_stel_tool`
  (`src/protocol/tools.rs`:10256) calls `proxy_tool_call("status", &request)`
  (passing `reset_calibration` through), overlays proxy-owned lines (:10268), and
  **returns early at :10272** with `OutcomeClass::Found` — never reaching the
  actual reset in `render_stel_status_body` (:10302–10303). The daemon worker is
  storeless; the proxy owns the durable store but never calls its own
  `reset_calibration()` (`src/protocol/mod.rs`:540–546) in this path.
- **RED**: daemon mode with durable calibration samples; call
  `status(reset_calibration=true)`; assert durable samples/constants clear to
  `deferred`; equivalent local reset behaves identically.
- **GREEN**: in the daemon-proxy `status` path, apply calibration reset to the
  proxy-owned durable store before/after proxying, and emit an honest reset
  receipt (do not report success on state the storeless worker cannot touch).
- **NEIGHBORS**: local (non-daemon) reset (must stay correct); `status` identity
  fields; STEL calibration/ledger tests.
- **DONE WHEN**: SC-005c; daemon + local reset both clear to `deferred`; gate
  green.

### Item 6 — US6 `validate_file_syntax` BOM + estimate (P3 CORRECTNESS)

- **Starting anchors (re-confirm first)**: `normalize_jsonc`
  (`src/parsing/config_extractors/json.rs`, ~L22) → `blank_trailing_commas`
  (~L32), no BOM strip; `JsonExtractor::extract` (json.rs:154,
  `serde_json::from_slice`); handler `validate_file_syntax`
  (`src/protocol/tools.rs`:7838) never reads `input.estimate`
  (`src/protocol/read_tools.rs`:402). JSON routes through the config-extractor
  path (`src/parsing/mod.rs`:45), **not** tree-sitter — the report's stated root
  cause is false; do not touch the tree-sitter dispatch.
- **RED**: valid JSON with vs without leading UTF-8 BOM → both `ok`, identical
  span accounting. Malformed JSON → still fails at oracle location. JSONC
  (trailing commas/comments) → still `ok` (guard against regressing
  `test_jsonc_trailing_commas_now_parse`, `test_tsconfig_jsonc_*`).
- **GREEN**: strip a leading UTF-8 BOM in `normalize_jsonc` before `serde_json`.
  Either wire `estimate` into the handler (tagged, within tolerance) or remove it
  from the input contract — no inert flag.
- **NEIGHBORS**: all JSON/JSONC/TOML/YAML validation tests; config extractor
  round-trips.
- **DONE WHEN**: SC-006; BOM `ok`, JSONC `ok`, malformed fails; `estimate`
  honored or removed; gate green.

---

## End-of-feature verification (before PR)

- Full gate on the whole branch: `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets -- --test-threads=1`, `cargo build --release`,
  `cd npm && npm test`, plus `cargo check --no-default-features --features embed`.
- Confirm every US has a fail-first test that is now green, and that the
  originally-red assertions encode the *actual wrong output* the benchmark
  measured (not a weaker proxy).
- Live re-check of the two P0s through the running MCP surface on a disposable
  clone (drive the tool, observe the real result) — code is gospel, and "the
  test passes" is confirmed by exercising the tool, not only `cargo test`.
- `cargo clean` to clear debug artifacts (disk discipline).
- Merge via `gh pr merge <N> --merge --delete-branch --body "PR #<N>"` (release-
  please double-count guard).

## Project Structure

### Documentation (this feature)

```text
specs/019-sfbench-surface-correctness/
├── spec.md              # Prioritized user stories (verified findings only)
├── plan.md              # This file — the per-item fix loop
├── research.md          # Phase 0 — per-item root cause, verified verdict, honest ceilings
├── contracts/
│   └── tool-behavior.md # Phase 1 — observable contract deltas per tool
└── tasks.md             # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

Single-crate Rust. Items touch mostly-disjoint files:

```text
src/
├── protocol/
│   ├── edit.rs          # US1: execute_batch_rename identity binding + honest label
│   ├── edit_tools.rs    # US1: MatchType label; US4 granular replay neighbors
│   ├── tools.rs         # US2: detect_impact seed + formatter; US3: 3 selector guards; US4: symforge_edit handler; US6: validate handler
│   ├── read_tools.rs    # US6: estimate flag contract
│   ├── format.rs        # US5: health repair-count formatting; parity checks
│   └── idempotency.rs   # US4: replay probe ordering
├── stel/
│   └── edit_apply.rs    # US4: run_pre_apply_gates vs replay ordering
├── live_index/
│   ├── graph.rs         # US2: from_index edge scoping + blast node identity
│   └── query.rs         # US1/US3: find_references_for_name; find_dependents import resolution
├── parsing/
│   ├── xref.rs          # US3: PYTHON_XREF_QUERY relative_import capture
│   └── config_extractors/json.rs  # US6: normalize_jsonc BOM strip
├── sidecar/
│   └── handlers.rs      # US2: analyze_file_impact same-file caller + typed lookup
└── watcher/
    └── mod.rs           # US5: reconcile repair-count semantics

tests/                   # per-item fail-first regression tests (or colocated #[cfg(test)])
```

**Structure Decision**: Single project (Option 1). No new modules. US1 and US2
share the *concept* of resolved identity but are separate commits with separate
tests; all six items land and gate independently in verified-severity order
(US1 → US2 → US3 → US4 → US5 → US6).

## Complexity Tracking

> No Constitution Check violations. Section intentionally empty.

## Notes on trust

This plan is derived from a benchmark report that was **not trusted**: a 7-agent
skeptical source review re-graded every finding, refuted P0-04 entirely,
downgraded P0-03 and P1-05, and corrected the root causes of P1-05 and P1-06.
The plan carries the *verified* set only. Per the same discipline, this plan's
own anchors are provisional until re-confirmed against live source at
implementation time — the red test on current code is the arbiter, not any
document.
