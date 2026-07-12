# Phase 0 Research — SFBENCH Surface Correctness

Per-item: the **verified** root cause (source-confirmed, not report-trusted),
the observed benchmark evidence where it exists, the chosen approach, and the
honest ceiling. Anchors are the 2026-07-12 verified state and must be
re-confirmed at implementation time (code is gospel).

## Evidence provenance

Three independent legs were used; a finding is only in-scope if it has at least
the source leg:

1. **Source** — the actual current code exhibits the behavior (read via
   symforge `get_symbol`/`get_file_content`, cited file:line).
2. **Skeptical review** — a rust-pro agent tasked to *refute* the finding could
   not, and graded verdict + severity + fix-soundness (2026-07-12 workflow).
3. **Observed run** — a recorded SFBENCH trial in
   `C:\AI_STUFF\BENCHMARKS\symforge-8.14.0-surface` measured the failure.

The benchmark *report* (a document) was **not** trusted; these three legs were
reconstructed independently. P0-04 failed all three (non-bug) and is excluded.

---

## US1 — `batch_rename` unsafe write (P0)

- **Source**: `execute_batch_rename` (`src/protocol/edit.rs`) resolves the
  target only to locate the definition, then collects refs via
  `find_references_for_name(&input.name, None, false)` — a bare-name reverse
  lookup with no path/identity scope — and writes every match; `code_only`
  filters by language class, not identity; `dry_run` defaults `false`. Trust
  label `MatchType::Constrained` overstates safety.
- **Observed run**: `SF-batch_rename-002` in `runs/formal-broad-v1-20260712.jsonl`
  recorded a `case_error` with `error_type: RunnerError`,
  `message: "source mutation escaped the frozen allowlist"` — the harness's
  before/after source fingerprints caught `batch_rename` writing files outside
  the declared allowlist during an actual run. This is measured, not asserted.
  (The happy `SF-batch_rename-001` PASSes and is the only batch_rename row in the
  happy-v2 adjudicated set — the aggregate "PASS" there is happy-path only and
  does not contradict the adverse escape.)
- **Approach**: bind writes to resolved identity; name-only/dynamic/textual
  matches non-writable by default and reported uncertain; fail closed when exact
  binding is unavailable; honest trust label.
- **Ceiling**: Tier-0 tree-sitter xref carries no resolved identity for dynamic
  languages (Python). A fully sound binding is impossible there from syntax
  alone, so the *safe* default for name-only matches is uncertain/non-writable,
  not "guess and write". This is the correct fail-closed ceiling, not a
  limitation to be engineered away in this feature.

## US2 — `detect_impact` confidently wrong (P0)

- **Source**: seed loop (`src/protocol/tools.rs` ~L7463–7476) pushes every
  symbol of each changed file; `GraphProjection::from_index`
  (`src/live_index/graph.rs` L66–133) links each `Call` to *every* same-bare-name
  definition (doc comment: "the over-approximation the v1 resolver will later
  narrow"); formatter drops path/kind; `is_entry_point` tags any `fn main`.
- **Observed run**: `SF-detect_impact-001` adjudicated **FAIL /
  INVALID_INCORRECT**, direct payload **1278 cl100k tokens** (vs an 82-token
  lower-bound baseline) — matches the report's measured figure.
- **Prior internal evidence**: the project's own contract
  `specs/015-cbm-capability-ports/contracts/detect-impact.md:104–111` records a
  291K-changed-symbol / 54MB "explosion" from this exact path, mitigated only by
  a 200-entry truncation. The root cause was known and deferred.
- **Report correction (verified)**: the finding conflated `detect_impact` (the
  explosion) with `analyze_file_impact` (a separate tool that already has
  parent-type narrowing at `src/sidecar/handlers.rs:1305–1321` and only a
  *residual* bare-name lookup). US2 fixes the former fully and tightens the
  latter's residual lookup without removing its narrowing.
- **Approach**: body-hash `SymbolDelta` seed + owner/qualified-scoped edges +
  identity-bearing blast nodes. Both seed and edge fixes are required — a perfect
  seed still over-connects at hop-1+ through bare-name edges.
- **Ceiling**: where the xref genuinely cannot resolve a call target, do not
  invent an edge (same honest-ceiling discipline as US1) rather than fall back to
  every same-name def.

## US3 — selector refusal + Python relative import (P1)

- **Source**: `search_symbols`/`search_text`/`find_references`
  (`src/protocol/tools.rs`:4986/5149/7941) call `local_cross_project_refusal`
  (refuses any selector) while siblings use `foreign_project_refusal` (allows
  matching-local). `PYTHON_XREF_QUERY` (`src/parsing/xref.rs`:72–121) has no
  `relative_import` capture — verified against pinned `tree-sitter-python`
  0.25.0 `node-types.json` (`import_from_statement.module_name` may be
  `relative_import`, unhandled).
- **Observed run**: `search_symbols`, `search_text`, `find_references`,
  `find_dependents`, `get_file_context` all adjudicated **FAIL** in the summary,
  consistent with the selector + import defects.
- **Approach**: one centralized selector guard covering `project` + `projects`
  and both directions (match-local proceed / foreign+over-broad refuse — do not
  regress the P2-10 `projects=["*"]` under-refusal). Add `relative_import`
  capture and resolve by counting `import_prefix` dots vs the importer's package.
- **Ceiling / dependency**: this is the small, targeted slice of the larger
  "typed identity end-to-end" theme; it does not attempt the full identity
  refactor, only what these three tools + `find_dependents` need.

## US4 — `symforge_edit` replay-before-concurrency (P2)

- **Source**: `symforge_edit_stel_handler` (`src/protocol/tools.rs`:10040–10159)
  runs `run_pre_apply_gates` (`src/stel/edit_apply.rs`:71–149, if_match guard
  ~L130–137, unconditional per test `pre_apply_rejects_if_match_mismatch`) before
  `begin_mutation_replay` (`src/protocol/idempotency.rs`:439).
- **Observed run**: `symforge_edit` adjudicated **FAIL** (6 samples); the report
  attributes 3/3 replay rejection to this ordering.
- **Approach**: non-reserving read-only replay probe before the gates; return
  stored result on identical key+request; keep reservation + genuine concurrency
  semantics.
- **Refuted sub-claim**: insertion whitespace — `build_insert_before` already
  emits the blank separator; the single-`\n` case is intentional doc-comment
  tightening; `insert_symbol` and `batch_insert` already share the layout
  function. Not in scope.

## US5b — watcher cold-start generation staleness (P1) [ADDED BY REVIEW]

- **Source (verified first-hand 2026-07-12)**: cold-start bootstrap in
  `src/main.rs` fires `tokio::task::spawn_blocking(|| bg_index.reload(&bg_root))`
  **fire-and-forget** (not awaited — control returns to
  `index.published_state()` immediately), which bumps `project_generation`
  (`src/live_index/store.rs`:800). The watcher is then spawned unconditionally
  (~`main.rs`:432) and captures `expected_gen` once at
  `src/watcher/mod.rs`:721. The cheap spawn wins the race vs the ~220 ms reload →
  captures pre-reload gen → later `freshen_file_if_stale` /
  `reconcile_stale_files_with_stop` see `GenerationMismatch` → file removed,
  never re-indexed, no self-heal.
- **My earlier refutation was WRONG**: I checked only the daemon `reload_with`
  and stdio `index_folder` paths (which DO restart the watcher) and generalized
  "every reload path restarts the watcher." The cold-start bootstrap does not.
  The adversarial review caught this by reading `main.rs` directly. This is a
  real correctness bug on the common cold-start path, not a P2 counter issue.
- **Approach**: await the cold-start reload before spawning the watcher, OR
  re-read the generation per committed reconcile batch, OR subscribe to
  generation changes. Keep the generation fence intact (it correctly rejects
  genuine in-flight stale mutations — that half of my analysis held).

## US5a — health reconcile-repair counter (P2)

- **Source**: `reconcile_stale_files_with_stop` (`src/watcher/mod.rs`:505) counts
  every `true` from `freshen_file_if_stale_at_generation` (:487), which returns
  `true` for `FreshenResult::GenerationMismatch` (a no-op) as well as real
  repairs; surfaced by health (`src/protocol/format.rs`:1903–1909, 2055–2079).
- **Real (the surviving half)**: the counter over-counts `GenerationMismatch`
  no-ops as repairs. This is genuine and **causally linked to US5b** — the
  cold-start race is what *produces* most of those no-ops; fixing US5b drains
  them, and this counter fix is the honest-signal backstop for the residual
  legitimate-race window.
- **Approach**: count only `StaleReindexed`/`StaleRemoved`; surface rejected
  mismatch attempts separately.

## US5c — daemon `status(reset_calibration=true)` silent no-op (P2) [ADDED BY REVIEW]

- **Source (verified via review)**: `status_stel_tool` (`src/protocol/tools.rs`:10256)
  proxies with `reset_calibration` passed through, overlays proxy-owned lines
  (:10268), and returns early at :10272 with `OutcomeClass::Found` — never
  reaching the actual reset in `render_stel_status_body` (:10302–10303). The
  proxy owns the durable calibration store but never calls its own
  `reset_calibration()` (`src/protocol/mod.rs`:540–546); the daemon worker it
  proxies to is storeless. Durable calibration is untouched; the caller is told
  it succeeded.
- **Wrongly omitted**: the first draft dropped the `status` FAIL into "economics"
  without recording it. It is a trust-envelope state defect, the class 019
  carries. The review restored it as US5c.
- **Approach**: apply calibration reset to the proxy-owned durable store in the
  daemon path; honest receipt on the no-store path.

## US6 — `validate_file_syntax` BOM + estimate (P3)

- **Source**: JSON validation uses `serde_json` via the config-extractor path
  (`src/parsing/mod.rs`:45), **not** tree-sitter (the report's stated root cause
  is false). `normalize_jsonc` (`src/parsing/config_extractors/json.rs` ~L22)
  does not strip a leading UTF-8 BOM, so `serde_json` rejects valid BOM JSON at
  1:1. `estimate` (`src/protocol/read_tools.rs`:402) is never read by the handler
  (`tools.rs`:7838).
- **Refuted sub-claim**: trailing-comma acceptance is deliberate JSONC/tsconfig
  support (`test_jsonc_trailing_commas_now_parse`, `test_tsconfig_jsonc_*`).
  "Fixing" it would regress shipped behavior. Not a defect.
- **Approach**: strip leading BOM in `normalize_jsonc`; honor or remove
  `estimate`. Preserve JSONC tolerance and correct malformed-input rejection.

## Excluded — P0-04 meta (non-bug)

`meta` is a measurement-only A-019 L0 A/B probe (`surface_probe.rs`:244) that
tied then **lost** to `compact-3` (`docs/research/A-019-l0-surface-choice.md`)
and is never advertised to users (CLAUDE.md documents only full + compact). The
surface guard (`tools.rs`:9613–9622) is by-design; the intended probe-relay path
works. The benchmark graded an experimental measurement surface as if shipping —
a benchmark artifact. If ever actioned, the only reasonable move is deleting the
retired variant; it is not a correctness/safety obligation and is not a story.

## Economics themes deferred (real, but not this feature)

The report's strongest *non-correctness* findings — full-surface schema tax
(17,894 vs 1,163 cl100k), compact-as-default, verbose mutation receipts, CCR
mandatory-retrieval cost — are real and measured but are **economics**, not
safety/correctness. They belong to a separate spec so this feature stays a
tight, verifiable correctness/safety slice. Recorded here so they are not lost.

## Contract-tightening deferred (real, but not a wrong answer)

- **`what_changed` drops git status classes + rename mapping**: `src/git.rs`
  `uncommitted_paths` collects only the path string and discards `entry.status()`
  and rename source/dest, so `what_changed` cannot report
  staged/unstaged/added/deleted/renamed. Verified real, but the *paths returned
  are correct* — this is missing-information / contract-tightening (the report's
  own P2-10), not a wrong result. Deferred, recorded here so `what_changed` is
  not later read as verified-clean.

## Adversarial review corrections (2026-07-12) — before → after

This spec was red-teamed by 4 source-grounded reviewers + a judge *after* the
first draft. The review found real defects **in the spec**. What changed:

| # | First draft (WRONG) | Corrected |
|---|---|---|
| B1 | P0-03 refuted; only a P2 counter fix (US5) | US5b added at **P1** — stdio cold-start watcher goes permanently stale (`main.rs` fire-and-forget reload bumps gen after watcher captures it). My "every reload restarts the watcher" generalization was false for cold start. |
| B2 | US2 edge fix: "owner-scoped; don't invent an edge if unresolved" | Would drop the green `compute_impact_reaches_across_qualified_module_call` edge (`SymbolRecord` has no owner). Added explicit **bare-name fallback** when the callee has no resolvable owner. |
| B3 | US1: name-only matches non-writable "by default" / key on resolved identity | Would regress `batch_rename_updates_definition_and_callers` (bare refs never have identity). Re-keyed on **index-wide name ambiguity** (1 def → write; 2+ → demote). |
| M1 | daemon `status(reset_calibration)` no-op dropped into "economics" | Restored as **US5c** (P2) — real trust-envelope state defect. |
| M2 | SC-002 = leaf edit → 1 changed + 1 hop-1 | Fixture names are globally unique → passes with seed fix alone, blind to the edge fix. SC-002 now requires **both** the leaf case AND a two-same-name case. |
| M3 | FR-004 asserts no duplicate-`main` collapse | SFBENCH fixture has **no `main`** → clause untestable on it. Now requires a `main`-bearing fixture or explicit untested-on-fixture marking. |
| M4 | US2 seed = "compare body hashes" (implied small) | Net-new machinery: needs **base-ref reparse** (reuse `diff_symbols` git-blob parse); `SymbolRecord` has only file-level `content_hash`. Sized accordingly; "smaller/faster" claim to be re-validated. |

**Held up under attack (do NOT re-litigate)**: P0-04 meta exclusion (non-shipping
probe), P1-05 JSON trailing-comma downgrade (deliberate JSONC, shipped tests),
US4 insertion-whitespace refutation (layout already shared, single-`\n`
intentional), US3 daemon-path safety (proxy short-circuits before local refusal).
The core scope and the three exclusions survived; the fixes and one downgrade did
not, which is exactly what the review was for.
