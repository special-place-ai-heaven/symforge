# SymForge Actionable Implementation Backlog

Last reconciled: 2026-05-22

This file is a closed archive of the 2026-05-20 implementation backlog. SFR00
verified that the SFB completion commits referenced below are ancestors of
current `HEAD`, so the historical items are not an active implementation queue.

Do not generate new implementation tasks from the closed archive unless a new
review or failing test reopens an item with current evidence. Review-driven
active work from `docs/code-review-2026-05-22.md` belongs in the SFR goal chain,
not in these completed SFB items.

## Reconciliation Summary

| Former item | Status in current `HEAD` | Evidence |
|---|---|---|
| 1. Windows libgit2 lockfile flake mitigation | Closed by SFB01 | `completion_commit: e88ed59e0d654209ed843d5c77636cc5e06dbdf3`; `tests/git_commit_retry.rs`; `src/git/test_helpers.rs` |
| 2. Untracked-file search diagnostic | Closed by SFB02 | `completion_commit: 5594c785172d5582bc372b60c5bc6b524e6edd03`; `src/protocol/tools.rs:3330`; assertions at `src/protocol/tools.rs:12826` and `:13598` |
| 3. Sidecar PID/alive state in health output | Closed by SFB03 | `completion_commit: 03bf46fa2515821a040e985dbba16583e923e5c1`; `src/protocol/format.rs:1387`; `test_health_compact_surfaces_dead_sidecar_pid_and_state` |
| 4. NoisePolicy classification for Obsidian internals | Closed by SFB04 | `completion_commit: 2a4577a39a76c38e33889519d689dd458d3a837c` |
| 5. External-evaluator regression coverage | Closed by SFB05 | `completion_commit: 5ac3e3959db88ef837ac9b6bde3178c42303eaaf` |
| 6. Current partial-parse hygiene | Closed by SFB06; SFR13 adds bounded quarantine health evidence | `completion_commit: 37d7918`; `health` distinguishes expected vendor partials from unexpected repo partials; `tests/health_parse_quarantine.rs` covers parse/span quarantine surfacing |
| 7. `search_text(group_by="usage")` doc/comment filter | Closed by SFB07 | `completion_commit: 96b4954e4458dc79f10012e28222c8588916cc9f` |
| 8. `replace_symbol_body` same-line inline doc preservation | Closed by SFB08 | `completion_commit: 691e0a713035309d910b78b3cdf2d540112a4d37`; `src/protocol/edit.rs:5312`; `src/protocol/tools.rs:20009` |
| 9. Machine-readable result status semantics | Closed by SFB09-SFB11 | `src/protocol/result_status.rs`; `src/protocol/tools.rs:14992`, `:15090`, `:15183`, `:15258` |
| 10. Runtime state identity and reset clarity | Closed by SFB12-SFB13 | `completion_commit: 9abdac0095c3a058c1d18ea96d464d9f8298c529`; `completion_commit: 5d6ab488b21ca9b56a2dd377c02c7eb9c07fb5ff` |
| 11. Replayable public-contract conformance suite | Closed by SFB14 | `completion_commit: a247c61faa7c0ef73c0f0b25cdab52ae1419a5c9`; `tests/conformance.rs` |
| 12. Guidance ranking and noise filtering | Closed by SFB15 | `completion_commit: 2c907ce54924d097987736e2ffc1c21f5513921a` |
| 13. Co-change rerank calibration closure | Closed by SFB16 | `completion_commit: d2b5de693632d7a0bfbb47ff01fb20c53b7c1c32` |
| 14. `trace_symbol` compatibility alias cleanup | Closed by SFB17, with daemon compatibility intentionally retained through v7.x and planned for v8.0 removal | `completion_commit: 2e8900f`; `src/cli/init.rs:1671`; `tests/daemon_aliases.rs` |
| 15. Rust raw-reference grammar upgrade | Closed by SFB18 | `completion_commit: 8cdf24e5e2b8c38e61c1c7b0196b9ad2d4f60efe` |
| 16. `validate_file_syntax` deepest-error diagnostics | Closed by SFB19 | `completion_commit: dbb70fd935fdd94919abdb3083cbb3d71f98b2a9`; `src/parsing/mod.rs:147` |
| 17. Unified truncation phrasing | Closed by SFB20 | `completion_commit: a6c1841eccf6b981f35e64567c0fb92cbca16541` |
| 18. Remaining language inline extractor tests | Closed by SFB21-SFB23 | `completion_commit: 82e3d98b38b3a751b28d75cde8cc91d106573ef3`; `3360c5b9635323a235d0f790e1f104d0a7364fc8`; `8eaf08905937aed8ca69da2d741cbbef69605ceb` |
| 19. Local SQLite analytics implementation | Closed by SFB24-SFB26 | `tests/sfb25_analytics_queue.rs`; `tests/sfb26_analytics_cli.rs`; no MCP analytics tool is advertised |
| 20. Non-code repository intelligence expansion | Closed by SFB27-SFB28 | `tests/sfb27_ci_yaml_corpus.rs`; `tests/sfb28_ci_yaml_repository_intelligence.rs` |

## Closed Archive

## 1. Windows libgit2 lockfile flake mitigation

Problem: tests that create many commits can intermittently fail on Windows when
libgit2 cannot rename `.git/refs/heads/*` lockfiles.

Historical implement request:

- Add retry/backoff around affected git-test helpers, or replace the helper's
  libgit2 commit path with process `git commit` where appropriate.
- Keep the fix isolated to tests/helpers unless production code is proven to hit
  the same Windows lockfile race.

Historical acceptance:

- Affected frecency/persist tests pass repeatedly on Windows under parallel and
  serial cargo test runs.
- No `#[ignore]` workaround is needed.

## 2. Untracked-file search diagnostic

Problem: `what_changed` can see untracked files, but `search_files` and
`search_text` do not appear to emit an actionable empty-result diagnostic that
points users to `analyze_file_impact(path, new_file=true)`.

Historical implement request:

- In `search_files` and `search_text`, when a query returns zero hits and a
  matching untracked file exists, append a diagnostic such as:
  `Note: 1 untracked file may match this query. Run analyze_file_impact("<path>", new_file=true) to index it.`
- Do not auto-index untracked files by default.

Historical acceptance:

- A regression test proves the diagnostic appears for a matching untracked file.
- Existing tracked-file search behavior is unchanged.

## 3. Sidecar PID/alive state in health output

Problem: health still reports hook adoption text, but does not surface an
explicit `Sidecar:` line with PID and alive/dead state.

Historical implement request:

- Expose sidecar PID and liveness from the existing sidecar state/port-file path.
- Render this in both `health` and `health_compact`.
- Preserve existing hook-adoption counters.

Historical acceptance:

- Tests assert sidecar status appears in full and compact health output.
- Tests cover a down/dead sidecar state.

## 4. NoisePolicy classification for Obsidian internals

Problem: `.obsidian/` and `wiki/.obsidian/` can still pollute search or
coupling signals.

Historical implement request:

- Extend the path-noise classifier so `.obsidian/` and `wiki/.obsidian/`
  classify as personal tooling.
- Do not exclude normal markdown/wiki content outside `.obsidian`.

Historical acceptance:

- Tests cover `.obsidian/`, `wiki/.obsidian/`, and
  `.obsidian/plugins/dataview/styles.css`.

## 5. External-evaluator regression coverage

Problem: historical external evaluations found bugs that ordinary tests did not
make obvious. Most specific fixes landed, but the test-surface gaps still need
hardening.

Historical implement request:

- Audit fixed evaluator bugs and map each to the test category that should have
  caught it before release.
- Add the top regression tests directly, or turn each into a concrete backlog
  item in this file.

Historical acceptance:

- At least three new regression tests land, or each top gap is represented here
  as a concrete implementation item with file targets and verification.

## 6. Current partial-parse hygiene

Problem: current health no longer shows SymForge Rust source partials; the
remaining partial files are vendored SCSS parser C/header files.

Historical implement request:

- Decide whether vendor partials should be fixed, suppressed as vendor noise, or
  surfaced as expected vendor parse limitations.
- Keep the old Rust `&raw` parser issue closed unless it reappears.

Historical acceptance:

- Health reports zero unexpected partials for the repo, or clearly marks vendor
  partials as expected/noise.

SFR13 verification adds a bounded parse/span quarantine registry to full and
compact health output. It is diagnostic evidence derived from `ParseStatus`,
not a second parser state machine: unexpected repo partials, expected vendor
partials, and failed parses remain visible with bounded `showing`/`omitted`
counts.

## 7. `search_text(group_by="usage")` doc/comment filter

Problem: `group_by="usage"` filters imports and ordinary comments, but doc
comments can still remain usage-visible. The intended product behavior needs to
be pinned in code.

Historical implement request:

- If doc/markdown matches should be suppressed, update the usage filter and add
  regression tests.
- If current behavior is intentional, add tests that pin why doc comments remain
  usage-visible.

Historical acceptance:

- `group_by="usage"` behavior around doc comments and markdown is explicitly
  tested.

## 8. `replace_symbol_body` same-line inline doc preservation

Problem: when a doc comment and symbol signature live on the same source line,
for example `/** @deprecated */ export function legacy() { ... }`,
`replace_symbol_body` can replace from the start of the line and swallow the
inline doc if the replacement body has no docs.

Historical implement request:

- Detect inline doc/comment text between `raw_line_start` and
  `sym.byte_range.0` before replacing.
- Preserve the inline doc prefix or adjust the splice start to begin after the
  inline doc marker when the new body does not provide its own docs.
- Add focused fixtures for TypeScript/JSDoc and one Rust-style inline comment
  case if the parser can represent it.

Historical acceptance:

- `replace_symbol_body` preserves a same-line inline doc when replacing a symbol
  with a docless `new_body`.
- Existing attached-doc and orphan-doc tests still pass.

## 9. Machine-readable result status semantics

Problem: many tool responses are optimized for readable text. Agents still need
stable machine-level outcome semantics for states such as found, not found,
ambiguous selector, invalid request, empty result, and internal failure.

Historical implement request:

- Add a public result-status contract for MCP tool responses where the protocol
  can carry it without breaking existing text output.
- Keep human-readable messages, but expose stable machine truth separately.
- Prioritize `get_symbol`, `get_file_content`, `search_*`, `find_references`,
  `replace_symbol_body`, `batch_edit`, and `batch_insert`.

Historical acceptance:

- Contract tests cover found, not found, ambiguous, invalid request, and empty
  or no-match states.
- Existing human text remains understandable.

## 10. Runtime state identity and reset clarity

Problem: shared daemon/index state is useful, but hidden carry-over state makes
benchmarking, debugging, and reproductions harder.

Historical implement request:

- Surface active project root, index/session identity, and whether the index was
  freshly built or reused in `health`, `health_compact`, or a dedicated status
  surface.
- Add or document a deterministic fresh-index/reset workflow for evaluations.
- Make context/session carry-over visible enough that callers do not infer a
  clean session incorrectly.

Historical acceptance:

- A fresh process, reused daemon session, and explicit `index_folder` reset are
  distinguishable in tool output.
- Evaluation harnesses can assert active project/index identity before running.

## 11. Replayable public-contract conformance suite

Problem: historical evaluations exposed contract-level issues that ordinary
implementation tests did not catch.

Historical implement request:

- Add a versioned conformance corpus for public MCP contracts: canonical JSON
  requests, expected response class/status, expected recovery hint for invalid
  requests, and dry-run behavior for mutating tools.
- Include negative cases for malformed payloads and unsupported forms.
- Record schema/behavior deltas in release notes when public contracts change.

Historical acceptance:

- The conformance suite can replay core read/search/edit/dry-run cases against a
  built binary.
- At least one invalid-request case asserts a specific recovery message instead
  of generic deserialization fallout.

## 12. Guidance ranking and noise filtering

Problem: guidance tools are valuable, but they should avoid low-signal symbols,
doc-only code patterns, and unexplained suggestions.

Historical implement request:

- Audit `investigation_suggest`, `ask`, and `explore` ranking for low-signal
  symbols such as builtins/common names and doc/comment-only pattern hits.
- Prefer project-owned symbols, changed files, loaded-context proximity,
  caller/reference depth, and explicit reason text.
- Keep outputs concise.

Historical acceptance:

- Focused tests prove trivial names are filtered unless strongly contextualized.
- At least one guidance response includes a concise reason for why a suggestion
  was made.

## 13. Co-change rerank calibration closure

Problem: query-level anchor-confidence gating remains provisional. Current code
has a conservative basename-tier floor and hardcoded chore-anchor defaults; the
remaining work is to close the empirical/configuration loop.

Historical implement request:

- Add a query-level calibration or regression corpus for
  `search_files(rank_by="path+cochange", anchor_path=...)` that proves weak
  anchors do not degrade baseline path ordering.
- Promote, adjust, or remove the basename-tier anchor-confidence threshold based
  on measured outcomes.
- Decide whether the chore-anchor denylist should remain hardcoded or become
  workspace-configurable.

Historical acceptance:

- Tests cover the chosen weak-anchor behavior and chore-anchor behavior.
- The chosen constants/config are documented in code comments or test names.

## 14. `trace_symbol` compatibility alias cleanup

Problem: `trace_symbol` was kept as a compatibility alias for one release cycle.
It is still present in client allow-list guidance and daemon compatibility after
many later releases.

Historical implement request:

- Remove `trace_symbol` from generated client allow lists and default tool-name
  guidance.
- Decide whether the daemon compatibility route should remain for one final
  release with an explicit deprecation warning or be removed in the same patch.
- Ensure `find_references` and `get_symbol_context` are the only documented
  paths.

Historical acceptance:

- Source search for `trace_symbol` returns only deliberate historical references
  or none at all, depending on the chosen compatibility policy.
- Client init tests still pass and do not grant the retired tool by default.

Resolution: the daemon compatibility route is retained through v7.x with an
explicit deprecation warning and planned for removal in v8.0. Generated client
allow-lists continue to exclude the retired name.

## 15. Rust raw-reference grammar upgrade

Problem: `Cargo.toml` still pins `tree-sitter-rust = "=0.24.2"`. Rust 2024
`&raw const` / `&raw mut` should parse without partial-parse fallout.

Historical implement request:

- Bump `tree-sitter-rust` after checking compatibility with the pinned
  `tree-sitter` version.
- Add or retain a fixture proving raw-reference expressions parse cleanly.
- Re-check current partial Rust parse examples before and after the bump.

Historical acceptance:

- Raw-reference syntax no longer creates unexpected Rust partial parses.
- Full Rust parsing tests and the repo-wide test suite pass.

## 16. `validate_file_syntax` deepest-error diagnostics

Problem: current tree-sitter diagnostics still use a first-error walk. The
reported syntax location should prefer the deepest useful ERROR or MISSING node.

Historical implement request:

- Replace first-error selection with deepest-error selection that preserves
  stable line/column/byte-span reporting.
- Add malformed-source fixtures where the outer parse error is less useful than
  the nested error.

Historical acceptance:

- `validate_file_syntax` reports the deepest actionable syntax error for the
  targeted fixtures.
- Existing config-language diagnostics remain unchanged unless the new location
  is strictly more specific.

## 17. Unified truncation phrasing

Problem: protocol and sidecar surfaces still use multiple truncation phrases,
which makes automated parsing and user guidance noisier than necessary.

Historical implement request:

- Choose one canonical truncation footer/envelope phrase.
- Apply it consistently to protocol output and sidecar budgeted output.
- Add tests for at least one protocol path and one sidecar path.

Historical acceptance:

- No active output surface emits a second, contradictory truncation phrase.
- Tests pin the canonical phrasing.

## 18. Remaining language inline extractor tests

Problem: the inline extractor-test framework and first Rust/Python examples are
implemented, but the remaining language extractors do not each have a co-located
fixture that asserts expected symbol extraction.

Historical implement request:

- Add focused `inline_test!` cases for the remaining language extractors in
  small batches.
- Keep each fixture minimal: one representative snippet, expected symbol names,
  expected kinds, and no broad parser refactor.

Historical acceptance:

- Every supported language extractor has at least one inline test.
- `cargo test --lib parsing -- --test-threads=1` passes.

## 19. Local SQLite analytics implementation

Problem: local persistent tool-call analytics is accepted as useful, but
production code still has only tracing and session-local counters.

Historical implement request:

- Implement a versioned local SQLite analytics store.
- Preserve disabled-no-footprint behavior: disabled analytics must not create a
  database, especially for discovery-only tools.
- Record only bounded local metadata: tool name, surface, configured scope,
  response bytes, estimated tokens, duration, success, outcome class, and
  capability state where already computed.
- Keep writes off the hot path through a bounded queue and background writer.
- Add CLI status/summary/export/reset surfaces; do not add an MCP analytics tool
  without a separate decision.

Historical acceptance:

- Analytics storage has migration, retention, redaction, disabled-mode, and
  queue-failure tests.
- Enabled analytics records bounded metadata without synchronous SQLite writes
  in handler hot paths.
- Disabled mode creates no analytics database and reports explicit disabled
  status.

## 20. Non-code repository intelligence expansion

Problem: SymForge is strong for source symbols, but many real debugging tasks
depend on operational files that still behave too much like plain text: SQL
migrations, XML/MSBuild, YAML/CI, shell scripts, fixtures, logs, and large docs.

Historical implement request:

- Add one file family at a time; do not attempt a broad parser rewrite.
- Start with SQL/migration facts or CI/YAML facts, whichever has the clearest
  user workflow and test corpus.
- Reuse existing config-extractor and metadata-only degradation patterns before
  inventing new public tools.
- Keep search, outline, explain, and edit behavior consistent with source-code
  files wherever possible.

Historical acceptance:

- The chosen file family becomes searchable, resolvable, and explainable through
  existing SymForge surfaces without raw shell fallback for normal inspection.
- Tests include normal, malformed, large, and empty/edge-case files.
- Any new edit behavior has dry-run or recovery evidence comparable to source
  edits.
