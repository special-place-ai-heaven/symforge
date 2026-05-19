---
title: RTK → SymForge Integration — Single Source of Truth for Planning
type: planning-input
status: ready-for-planner
date: 2026-05-19
audience: GPT-5.5 Pro (external planning agent producing /goal tasks)
repo: special-place-administrator/symforge @ main
head_commit: e525ada
current_version: 7.11.1
---

# RTK → SymForge Integration — Single Source of Truth

This document is the complete brief for a planning agent that will produce
`/goal` tasks for the remaining "RTK techniques" adoption work in SymForge. It
is self-contained: it states what RTK is, what was promised, what shipped, what
is open, where each open item lives in code, what constraints apply, and which
shapes a `/goal` task should take.

The reader (GPT-5.5 Pro) does not have access to the repo at task-generation
time. Every code path it must reason about is inlined or pinned by file+line
below.

---

## 1. Project Context

**SymForge** is a Rust MCP server providing symbol-aware code navigation and
editing tools to Claude Code, Codex, and Gemini CLI. Version 7.11.1 on `main`.
Stack: `tokio` async, `rmcp` MCP transport, `tree-sitter` parsing, `git2`,
`rusqlite` (bundled, used for frecency and co-change stores), `rayon` indexing
pool, `notify` watcher, `regex` + `aho-corasick` text search.

**RTK** is a separate sibling tool (CLI-output filtering / token compression
for shell-driven workflows). It is not a SymForge dependency. The "integration"
referred to in this doc is unidirectional adoption: SymForge cherry-picks
proven Rust idioms and architectural patterns from RTK's source. There is no
runtime coupling, no shared crate, no API contract between the two.

Source-of-truth doc inside the project: `wiki/concepts/RTK Techniques for
SymForge.md` (vault path; the SymForge repo references it but does not own it).
That doc enumerates 17 techniques across Tier 1 / Tier 2 / Tier 3. This
planning input restates only the techniques still open plus the decision
context the planner needs.

---

## 2. Roadmap Anchor

The merged sprint is **Wave 3** of `docs/plans/2026-05-15-symforge-post-h-roadmap.md`,
titled "Phase 4 RTK Tier 1 + Tier 2 merged sprint". Wave 3 was closed and
released as v7.10.0 but with **partial RTK scope** — only two implementation
units and two research units shipped. Post-Wave-3, the project shifted focus
to **Wave 3g — Call-Time Capability Resolution** (ADR 0016), which shipped
across v7.10.0 → v7.11.1 and is largely orthogonal to the remaining RTK work.

The planning agent should treat the open RTK items as a **post-Wave-3
follow-up sprint**, not as a re-open of Wave 3 itself. Wave 3 close-out
evidence (`docs/notes/2026-XX-XX-w3-close-out-evidence.md` referenced in the
roadmap) was produced; do not regenerate it.

---

## 3. Status Inventory

All 17 RTK techniques, with definitive status. Tier numbering follows the
wiki concept doc.

### Tier 1 — High Value, Low Overlap

| # | Technique | Status | Landed-as | Evidence |
|---|-----------|--------|-----------|----------|
| 1 | Aggressive release profile (`lto=true`, `codegen-units=1`, `strip=true`) | ✅ Done | `aed8ec8` | `Cargo.toml:84-87` (note: `opt-level` and `panic="abort"` deliberately NOT adopted — see §7) |
| 2 | Build-time asset embedding via `build.rs` | ⊘ N/A | n/a | Per Wave 3 R5: zero `.scm` files exist outside `vendor/` and `target/`. Nothing to embed. Re-evaluate only if a non-vendor `.scm` corpus is introduced. |
| 3 | `automod` for `src/parsing/languages/` | ✅ Done | `1a4fa86` | `automod = "1"` in `Cargo.toml:22`; `src/parsing/languages/mod.rs` is now a single `automod::dir!()` |
| 3' | `automod` for `src/parsing/config_extractors/` | ❌ Pending | — | `src/parsing/config_extractors/mod.rs:1-5` still has 5 manual `pub mod` declarations (env, json, markdown, toml_ext, yaml) |
| 4 | Inline test framework for extractors | ❌ Pending | — | `src/parsing/inline_tests.rs` does not exist |
| 5 | Pre-edit tee snapshots | ✅ Done | `22a5edb` (feat), `b82e77e` (close-out) | `src/edit_safety/{mod.rs, tee.rs}` present; wired in `src/protocol/edit.rs:153` (`atomic_write_file`) via `format_tee_snapshot_suffix` (line 177) and `append_response_suffix_to_first_summary` (line 185). Backed by `tests/edit_safety_tee.rs`. |
| 6 | Trust-gating for `.symforge/` project config | ❌ Pending | — | `src/edit_safety/trust.rs` does not exist; `docs/decisions/0015-rtk-trust-gating-symforge-config.md` does not exist (highest-numbered ADR on disk is `0016-call-time-capability-resolution.md`). Note: recent commits `c0bc663`, `0dcbba4` mention "trust gates" but refer to **Wave 0 trust restoration / search-and-context trust calibration**, NOT this RTK unit. Reconciliation note required (see §7). |

### Tier 2 — Medium Value, Worth Investigating

| # | Technique | Status | Landed-as | Evidence |
|---|-----------|--------|-----------|----------|
| 7 | Three-tier graceful degradation (Full / Degraded / Passthrough) | ❌ Pending (behavior layer) | — | Tiers exist as labels in `health` output, but `get_symbol_context` and `find_references` do not yet fall back from Tier 1 (indexed) to Tier 2 (metadata-only) on per-file parse failure. No `tests/graceful_degradation.rs`. |
| 8 | `RegexSet` for fast multi-pattern matching | ✅ Effectively done (different mechanism) | `aed8ec8` | The release-profile commit also added **Aho-Corasick multi-term search** (`aho-corasick = "1.1"` in `Cargo.toml:18`). `RegexSet`-vs-`aho-corasick` is an implementation choice; the goal — single-automaton multi-term scan — is achieved. Do not re-implement as `RegexSet` unless benchmarks justify swapping. |
| 9 | Shell command lexer | ⊘ Defer / out of scope | — | Tier 2 wiki entry is conditional ("if SymForge ever needs to parse shell"); no current consumer. Do not plan unless a concrete use case appears. |
| 10 | SQLite persistent analytics (`rusqlite` + WAL) | ❌ Pending (standalone product decision) | — | `rusqlite = "0.32"` already vendored (`Cargo.toml:68`) for frecency + co-change stores. T2.6 investigation decoupled this from `match_output` (which doesn't exist). The owner must decide whether T2.2 ships on independent observability merit. |
| 11 | Fire-and-forget telemetry | ⊘ Defer | — | Tier 2 wiki entry; not in the merged sprint scope. No commitment yet. |
| 12 | WAL-mode + 5s busy timeout + GLOB scoping | ✅ Partial (frecency + coupling stores) | various | The pattern is already in use for the two existing SQLite stores. New analytics work in #10 would inherit it. |
| 13 | `OnceLock` audit | ✅ Done (audit only, doc deliverable) | `d3dda0e` | `docs/notes/2026-05-16-rtk-once-lock-audit.md`. Identified 7 candidate sites with recommended migration order. No source patches applied yet — each migration is its own follow-up. |

### Tier 3 — Interesting Ideas, Evaluate Later

| # | Technique | Status | Note |
|---|-----------|--------|------|
| 12' | Output compression ratio CI assertion | ❌ Pending | T2.4 in Wave 3 roadmap (50% threshold for `get_file_context` vs raw read). No `tests/persist_compression_ratio.rs`. |
| 13' | CLI correction learning | ❌ Pending | T2.5 in Wave 3 roadmap. Depends on analytics (#10) for the failure corpus. |
| 14 | `match_output` short-circuit | ⊘ N/A as written | `afc41eb` + `docs/notes/2026-05-16-rtk-match-output-investigation.md`. No real `match_output` symbol exists in SymForge; the abstraction would have to be invented. If revisited, must be a new typed formatter design, not a patch. |
| 15 | Compound command rewriting | ⊘ Defer | Conditional, no current consumer. |
| 16 | Weighted cost-per-token model | ⊘ Defer | Only relevant if SymForge reports token savings; currently does not. |
| 17 | Tee mode configuration (Failures/Always/Never) | ⊘ Optional follow-up to #5 | Current tee is single-mode; mode enum + caps come "for free" if the planner decides to add config. |

### Open Items the Planner Should Generate Tasks For

In dependency order (foundation → derived):

1. **RTK-OPEN-1** — `automod` for `src/parsing/config_extractors/` (Tier 1 #3')
2. **RTK-OPEN-2** — Inline test framework for extractors (Tier 1 #4)
3. **RTK-OPEN-3** — Trust-gating for `.symforge/` + ADR 0015 (Tier 1 #6)
4. **RTK-OPEN-4** — Three-tier graceful degradation behavior layer (Tier 2 #7)
5. **RTK-OPEN-5** — SQLite analytics standalone decision + implementation (Tier 2 #10)
6. **RTK-OPEN-6** — Compression ratio CI assertion (Tier 3 #12', T2.4)
7. **RTK-OPEN-7** — CLI correction learning (Tier 3 #13', T2.5; depends on RTK-OPEN-5)
8. **RTK-OPEN-8** — `OnceLock` migrations (Tier 2 #13 follow-ups; 6 sub-items from audit, ordered)

Each is expanded in §6 with concrete acceptance criteria.

---

## 4. Code Surface Map

Paths and line anchors the planner can hand directly to an executor. All paths
relative to repo root `E:\project\symforge\` (a.k.a. SymForge worktree on
Linux).

### Edit-Safety Module (already exists; trust.rs is sibling target)

- `src/edit_safety/mod.rs` — currently re-exports `tee` only. Will need to add `pub mod trust;`.
- `src/edit_safety/tee.rs` — 19 symbols, including:
  - `TEE_MAX_FILES = 20`, `TEE_MAX_FILE_BYTES = 1_048_576`
  - `TeeRecord { tee_path, original_path }`
  - `enum TeeSnapshot { Saved(TeeRecord), Skipped { reason }, ... }`
  - `Tee::for_repo`, `Tee::for_target`, `Tee::snapshot`
  - Free helpers: `discover_repo_root`, `snapshot_file_name`, `sanitize_file_name`, `enforce_retention`, `display_relative`
- `tests/edit_safety_tee.rs` — coverage pattern to mirror for trust.

### Edit Pipeline Wire-Up Points

- `src/protocol/edit.rs:153-175` — `atomic_write_file` returns `AtomicWriteReport`; tee fires before this writes the new bytes.
- `src/protocol/edit.rs:177-183` — `format_tee_snapshot_suffix` formats the recovery hint for the tool response.
- `src/protocol/edit.rs:185-201` — `append_response_suffix_to_first_summary` is where the trust-gate decision would also surface to the LLM.
- `src/protocol/edit.rs:212-247` — `reindex_after_write` is post-write; trust check must run **before** any disk mutation.

### Parsing Surface (for `automod` and inline tests)

- `src/parsing/languages/mod.rs` — already on `automod::dir!()`. Reference for #3'.
- `src/parsing/config_extractors/mod.rs:1-5` — target of RTK-OPEN-1 (5 manual decls).
- `src/parsing/mod.rs:49-52` — calls `config_extractors::extractor_for`. The `extractor_for` signature must not change in #3'.
- `src/parsing/config_extractors/mod.rs:72-86` — `extractor_for` allocator pattern (boxed stateless ZSTs). Touched by `OnceLock` audit candidate #3.

### Graceful Degradation Surface

- `src/protocol/tools.rs` — `get_symbol_context` and `find_references` handlers (search by name; both are large handlers, line numbers drift between releases).
- `src/live_index/query.rs` — Tier 2 metadata-only lookup helpers will live here (the file already hosts `search_files` glob compilation at lines 1500-1515 per the OnceLock audit).
- Existing tier labels: emitted in `src/protocol/format.rs` `health_report_from_stats` (lines 1236-1368) — names "Tier 1 (indexed)", "Tier 2 (metadata only)", "Tier 3 (hard-skipped)".

### Analytics Surface (new)

- `src/observability/` does not exist. Wave 3 plan reserved `src/observability/analytics.rs`. The planner can confirm by listing the `src/` tree.
- Instrumentation site: each tool handler in `src/protocol/tools.rs` (≥31 tools per `SYMFORGE_TOOL_NAMES` in `src/cli/init.rs`). Wrap each via a helper, not per-call inline writes.

### Verification Commands (Project-Wide)

From `CLAUDE.md`:

```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
```

For npm-only changes (not expected here): `cd npm && npm test`. Mixed: run both.

---

## 5. Architectural Constraints & ADR Anchors

The planner must respect these before proposing implementation shapes. ADRs
live in `docs/decisions/`.

| ADR | Title | Relevance |
|-----|-------|-----------|
| 0001 | Tool-consolidation contract | Adding new tools requires aliases for backward compat (`src/daemon.rs`) and removal from `SYMFORGE_TOOL_NAMES`. Adding new analytics tool would follow this. |
| 0002 | Parasitic hooks, not tool replacement | New observability cannot replace existing tool surfaces. |
| 0010 | Worktree working_directory | Any new edit-path code (trust gate) must thread `working_directory` correctly. |
| 0011 | Frecency bump policy | Establishes "ships inert, opt-in after dogfood" precedent — apply same pattern to trust gate. |
| 0012 | Edit-and-ranker hook architecture | Trust gate hooks into `atomic_write_file`-adjacent surface; must not bypass ranker hook invariants. |
| 0013 | Coupling signal contract | Co-change tests for trust-gated config writes if any analytics ships. |
| 0014 | Watcher subsystem spawn-blocking discipline | Trust gate **does not** spawn watchers; tee module already complies. Cite this when justifying that trust gate is not a stop-block site. |
| 0016 | Call-time capability resolution | The currently shipped capability-resolution work. **Trust-gate decisions should also be resolved at call time**, not stored as boot-time state. This is the most important architectural constraint to land cleanly. |

**Required new ADR:** `docs/decisions/0015-rtk-trust-gating-symforge-config.md`
must land with RTK-OPEN-3 (per Wave 3 Unit 3d.1 and institutional learning #6).
Use 0015 even though 0016 is already taken — the gap is intentional; 0015 was
reserved for this work.

### Naming Collision Warning

Commits `c0bc663` ("stabilize search and context trust gates"), `0dcbba4`
("close trust gate evidence and observability"), and adjacent — these are
**Wave 0 / search-context trust calibration**. They are NOT the RTK trust-gate
unit. The planner must instruct executors to verify they are touching the
right "trust gate" via file path:

- Wave 0 / search-context trust: lives in `src/live_index/` and `src/protocol/tools.rs` discovery paths.
- RTK trust-gate (this work): new module `src/edit_safety/trust.rs` + daemon startup hook.

ADR 0015 should open with a one-paragraph disambiguation between the two.

---

## 6. Open Items — Acceptance Criteria & Goal-Task Shapes

Each item below is shaped so the planner can lift it directly into a
`/goal`-style task. Required fields: **Goal**, **Files**, **Dependencies**,
**Acceptance**, **Risks**, **Verification**.

### RTK-OPEN-1 — `automod` for `src/parsing/config_extractors/`

**Goal:** Replace 5 manual `pub mod` declarations in
`src/parsing/config_extractors/mod.rs:1-5` with `automod::dir!()`, mirroring
the pattern already applied to `src/parsing/languages/`.

**Files:**
- Modify: `src/parsing/config_extractors/mod.rs`

**Dependencies:** None. `automod = "1"` is already in `Cargo.toml:22`.

**Acceptance:**
- The 5 listed modules (env, json, markdown, toml_ext, yaml) remain accessible from `src/parsing/mod.rs:49-52` (`config_extractors::extractor_for`) with no API change.
- `cargo test --lib config_extractors -- --test-threads=1` passes.

**Risks:** Minimal. The visibility migration is identical to the one already proven in `src/parsing/languages/mod.rs`.

**Verification:** `cargo check && cargo test --all-targets -- --test-threads=1`.

**Estimated size:** 1 file, ~10 lines diff. Suitable as a single atomic commit.

---

### RTK-OPEN-2 — Inline Test Framework for Extractors

**Goal:** Establish a co-located test pattern where each language extractor
embeds source snippets + expected symbol extraction results. Ship the
framework + first two example tests (Rust and Python). Remaining 17 languages
become a wiki todo.

**Files:**
- Create: `src/parsing/inline_tests.rs` (framework module with `inline_test!` macro)
- Modify: `src/parsing/mod.rs` (register the inline-tests module)
- Modify: `src/parsing/languages/rust.rs` (add 1 example inline test)
- Modify: `src/parsing/languages/python.rs` (add 1 example inline test)
- Append: `wiki/todos/Todos — SymForge.md` — follow-up to extend remaining languages

**Dependencies:** RTK-OPEN-1 (cleanest after `automod` restructure stabilises the surface; not strictly required, but the planner should sequence it after).

**Acceptance:**
- `#[cfg(test)] inline_test!(name, source = "...", expected_symbols = [...])` macro compiles.
- Macro produces a `#[test]` fn that parses the source via `parse_source` and asserts on extracted symbol kinds + names.
- 1 passing inline test exists in `src/parsing/languages/rust.rs`.
- 1 passing inline test exists in `src/parsing/languages/python.rs`.
- `cargo test --lib parsing::languages::rust -- --test-threads=1` passes including the new test.

**Risks:**
- Tree-sitter parser construction in `parse_source` (`src/parsing/mod.rs:191-250`) takes a fresh `Parser` per call. The macro must call the existing entry point, not introduce a parallel construction path.
- Don't over-engineer the macro: no language-detection logic in the macro; require the caller to specify `LanguageId`.

**Verification:** `cargo test --lib parsing -- --test-threads=1`.

**Estimated size:** ~150-250 lines new + 2 small test additions. Atomic feat commit.

---

### RTK-OPEN-3 — Trust-Gating for `.symforge/` + ADR 0015

**Goal:** SHA-256 hash the `.symforge/` config tree on first daemon load. Persist
the hash to user config (`~/.config/symforge/trust.json` or platform
equivalent via `dirs` crate). On every subsequent load, re-hash and compare.
If changed, surface a prompt via tool-response envelope. Ship inert: default
mode is **LOG_ONLY** (allow daemon to start, log a one-line warning to the
tool envelope). Mode **ENFORCE** (refuse to start on hash mismatch) is opt-in
behind a config flag, to be enabled in a later patch after dogfood. Resolve
the mode at **call time** per ADR 0016, not boot time.

**Files:**
- Create: `src/edit_safety/trust.rs`
- Create: `docs/decisions/0015-rtk-trust-gating-symforge-config.md`
- Modify: `src/edit_safety/mod.rs` (add `pub mod trust;` + re-export)
- Modify: `src/daemon.rs` (wire trust check into daemon-startup tool-envelope surfacing — not into the panic path)
- Test: `tests/edit_safety_trust.rs`

**Dependencies:** RTK-OPEN nothing in the open set. Implicitly depends on existing `src/edit_safety/` module (already shipped via tee unit).

**Acceptance:**
- First daemon launch in a repo with `.symforge/` config: hashes config, writes trust record, no prompt surfaces.
- Subsequent launch, config unchanged: trust check passes silently.
- Subsequent launch, config changed, mode LOG_ONLY (default): daemon starts, next tool response includes a one-line trust-gate warning suffix with the changed-file paths.
- Subsequent launch, config changed, mode ENFORCE: daemon refuses to start with a typed error referencing the trust record.
- `tests/edit_safety_trust.rs` covers all four cases.
- ADR 0015 is committed and references ADR 0016 (call-time resolution) and ADR 0011/0012 (ships-inert precedent).
- ADR 0015 opens with a paragraph disambiguating this work from the Wave 0 / search-and-context "trust gate" naming collision.

**Risks:**
- `~/.config/symforge/trust.json` is new persistent state. Document a migration story in ADR 0015 (versioned schema, missing-file = first-launch behaviour, corrupt-file = reset-and-log).
- Cross-platform path resolution: use `dirs::config_dir()` (already in deps).
- Hash performance: 5-second daemon-start budget is the bar. SHA-256 over a typical `.symforge/` is sub-millisecond, but cap the walk depth (`MAX_DEPTH = 6` is the existing config-extractor convention; reuse).
- DO NOT spawn a watcher on `.symforge/` for live re-hashing — that would create a new ADR 0014 stop-block site. Re-hash only on daemon startup.

**Verification:** `cargo test --test edit_safety_trust -- --test-threads=1` + full `cargo test --all-targets`.

**Estimated size:** ~400-600 lines (trust.rs + tests + ADR). Two commits acceptable: feat + docs.

---

### RTK-OPEN-4 — Three-Tier Graceful Degradation (Behavior Layer)

**Goal:** Extend `get_symbol_context` and `find_references` so that when a
file's Tier-1 parse failed (file is in the partial-parse or hard-skip set),
the tool returns a degraded response labelled with `tier: 2` (metadata-only:
path + size + language) instead of a 404-equivalent. Tier-3 files still return
explicit 404 with `reason`.

**Files:**
- Modify: `src/protocol/tools.rs` — `get_symbol_context`, `find_references` handlers
- Modify: `src/live_index/query.rs` — add Tier 2 metadata-only lookup helpers
- Test: `tests/graceful_degradation.rs`

**Dependencies:** None within the RTK-open set. Conceptually relies on the existing tier labels already present in `health` (`src/protocol/format.rs:1236-1368`).

**Acceptance:**
- Tier-1 indexed file → unchanged Tier-1 response shape.
- Tier-2 (metadata-only) file → degraded response with explicit `tier: 2` label + warning message.
- Tier-3 (hard-skipped) file → typed 404 with `reason` field.
- Tests cover all three branches per tool.

**Risks:**
- Existing callers must not break. Verify by enumerating handler call sites with `find_references` on each handler before patching the response shape.
- The current Tier 2 surface is metadata-only (file path + size + language). Do not invent new metadata; expose what's already captured.

**Verification:** `cargo test --test graceful_degradation && cargo test --all-targets -- --test-threads=1`.

**Estimated size:** ~200-400 lines including tests.

---

### RTK-OPEN-5 — SQLite Analytics (Standalone Decision + Implementation)

**Goal:** Before implementing, the planner should generate a **product
decision task** (a Sub-Task 0) that gathers maintainer assent on whether
analytics ships at all. If green-lit, then implementation.

**Decision-Task Shape (Sub-Task 0):**
- Surface the tradeoff: persistent new SQLite store at
  `~/.config/symforge/analytics.sqlite3`; instrumentation of all ~31 tool
  handlers; data lifecycle (90-day auto-cleanup pattern from RTK); opt-out
  story (env var + config); no telemetry network egress.
- Output: ADR 0017 (analytics-or-not) OR a decision note explicitly closing the item.

**Implementation-Task Shape (if green-lit):**

**Files:**
- Create: `src/observability/mod.rs`, `src/observability/analytics.rs`
- Modify: `src/lib.rs` (`pub mod observability;`)
- Modify: `src/protocol/tools.rs` (wrap each tool handler via a single helper, not inline)
- Test: `tests/observability_analytics.rs`

**Dependencies:** Sub-Task 0 must land first. `rusqlite = "0.32"` already in `Cargo.toml:68` with `features = ["bundled"]`.

**Acceptance:**
- Database opens with WAL mode + 5s busy timeout + GLOB-based project scoping (mirror the existing frecency-store pattern in `src/live_index/frecency.rs`).
- Each MCP tool call writes one row: `tool_name, project_glob, response_bytes, est_tokens, started_at_ns, duration_ns, success_bool`.
- Aggregation query exists and is tested.
- Opt-out: `SYMFORGE_ANALYTICS_DISABLED=1` + `.symforge/analytics.toml: enabled = false` — both honored.
- 90-day rolling cleanup runs at daemon start (lightweight, off-thread).

**Risks:**
- Performance: writing per tool call must be **fire-and-forget** (mpsc channel + background task), not synchronous SQLite inserts on hot path.
- Privacy: do not log raw query text or file paths beyond a configurable project glob.
- Discovery contract from OnceLock audit candidate #2: "discovery-only tools do not create frecency files." Apply the same rule — discovery-only tools must not create an analytics DB if disabled.

**Verification:** Targeted tests + full suite.

**Estimated size:** ~600-1200 lines including instrumentation harness + tests.

---

### RTK-OPEN-6 — Compression Ratio CI Assertion

**Goal:** Add a CI-enforced assertion that `get_file_context` output bytes
are ≤50% of raw file bytes, measured against a corpus of 5 representative
source files (varying language + size).

**Files:**
- Create: `tests/persist_compression_ratio.rs` (corpus + assertion)
- Possibly modify: `.github/workflows/ci.yml` — only if a new explicit gate is needed; otherwise the test runs as part of the existing suite.

**Dependencies:** None.

**Acceptance:**
- Corpus of 5 files (e.g., one Rust, one Python, one TypeScript, one JSON, one Markdown) passes the 50% threshold.
- Small-file edge case (<100 bytes): either documented exemption or expected fail with explicit `#[ignore]` reason — do not silently exempt.
- Test fails loudly if ratio regresses.

**Risks:**
- Corpus must be stable (vendored test fixtures, not arbitrary repo files that drift).
- The 50% threshold is per the wiki concept doc Tier 3 #12'. RTK's threshold is 60%; SymForge's wiki adopted 50%. Stick with 50%.

**Verification:** `cargo test --test persist_compression_ratio`.

**Estimated size:** ~150-300 lines including fixtures.

---

### RTK-OPEN-7 — CLI Correction Learning

**Goal:** On `replace_symbol_body` / `batch_edit` failure (symbol not found,
ambiguous match), fuzzy-match the missing symbol against existing symbols in
the same file and suggest the top 3.

**Files:**
- Modify: `src/protocol/edit.rs` — `resolve_or_error` (currently at
  `src/protocol/edit.rs:254-292`) is the primary failure surface.
- Possibly modify: `src/cli/init.rs` if the suggestion surface widens.

**Dependencies:** RTK-OPEN-5 if the suggestion engine is trained on analytics
data. If implemented as pure same-file fuzzy match against the live index,
no analytics dependency exists — make this explicit in the task.

**Acceptance:**
- Typo `foo` near actual `foo_bar` in same file → response includes `did_you_mean: ["foo_bar", ...]` (top 3).
- No fuzzy match within Levenshtein distance threshold (suggest distance ≤3) → original error returned, no `did_you_mean` field.
- Existing error messages are not breaking-changed; suggestions are additive.

**Risks:**
- `resolve_or_error` already handles existing CSS / Swift / Rust / Cpp resolution paths (per tests `test_resolve_*` in `src/protocol/edit.rs`). The suggestion layer is additive — don't refactor the resolver.
- Avoid suggesting symbols from other files; same-file scope only.

**Verification:** Targeted tests in the existing `mod tests` block of `src/protocol/edit.rs`.

**Estimated size:** ~150-300 lines.

---

### RTK-OPEN-8 — `OnceLock` Migrations (6 follow-ups from audit)

**Goal:** Each of the 6 audit candidates becomes its own implementation task,
in the order recommended by `docs/notes/2026-05-16-rtk-once-lock-audit.md`.
**These are independent tasks — the planner should generate 6 sub-goals, not
one mega-goal.** Two of the audit's 7 sites are listed as deferrals (ast-grep
language wrapper subsumed by #1; tree-sitter parser explicitly NOT a OnceLock
candidate).

Ordered as per audit §Recommended Migration Order:

| Sub-task | Site | Hot? | Shape |
|----------|------|------|-------|
| 8.1 | `search_structural` / `ast_grep::structural_search` pattern compile | Yes (highest priority) | Compile once per `(LanguageId, pattern)` per request first; only promote to process-wide cache after `Pattern: Send + Sync` verified and bounded-growth policy is designed. |
| 8.2 | Frecency read-path store opens (`src/protocol/tools.rs:4549-4582` + `:4694-4710`) | Medium | Add a cached read-only/shared helper in `src/live_index/frecency.rs` reusing the existing `OnceLock<Mutex<HashMap<PathBuf, Arc<FrecencyStore>>>>` pattern. **Preserve no-DB-creation invariant from `open_existing_readonly`.** |
| 8.3 | Config extractor registry cleanup (`src/parsing/config_extractors/mod.rs:72-86`) | Low | Convert boxed stateless ZSTs to `&'static dyn ConfigExtractor` only if microbenchmark or allocation profile shows value. Otherwise close as N/A. |
| 8.4 | Worktree feature flag caching (`src/worktree.rs:339-343`) | Low-medium | Cache only if edit-path telemetry shows cost. Beware: `src/live_index/persist.rs:374-383` deliberately avoids caching `SYMFORGE_FRECENCY` for testability — apply same caution. |
| 8.5 | Tree-sitter parser reuse | Hot but NOT a OnceLock migration | Investigate thread-local parser cache or small parser pool keyed by `LanguageId`; only `OnceLock` the pool container. Defer until benchmarks justify. |
| 8.6 | Dynamic regex/glob/AC matcher caching | Hot but unbounded-input risk | Defer until repeated-query evidence + bounded cache design exists. **Explicitly forbid** unbounded process-wide caching of user-input regex/glob. |

**Common acceptance pattern (apply per sub-task):**
- Before/after benchmark or targeted regression test that proves fewer repeated compiles/opens on the selected path.
- Tests preserving invalid regex/glob/structural-pattern error behavior.
- Cache growth policy documented for any user-input-keyed cache.
- Confirmation that discovery-only tools do not create frecency files.

**Common risks:**
- `lazy_static!` is forbidden — use std `OnceLock` / `LazyLock`.
- Do not cache user-provided regex/glob/pattern inputs in an unbounded global map.
- Do not cache env flags until tests and runtime expectations are audited.

---

## 7. Decisions Already Made (Planner Must Not Re-Open)

These are settled. Do not generate tasks for them.

1. **`opt-level = 3` and `panic = "abort"` are NOT adopted** even though the wiki Tier 1 #1 entry mentions them. The current `Cargo.toml:84-87` profile uses `lto = true`, `codegen-units = 1`, `strip = true` only. The reasoning: `panic = "abort"` would require auditing `catch_unwind` usage and break unwinding-dependent semantics; the cost-benefit was not deemed worth the risk. If revisited, requires its own ADR.
2. **RTK 4.3 (build-time `.scm` embedding) is N/A.** Zero `.scm` files outside vendor/target.
3. **T2.6 (`match_output` short-circuit) is N/A as written.** No such symbol exists. See `docs/notes/2026-05-16-rtk-match-output-investigation.md`. If trivial-response optimization is revisited, it must be a new typed formatter design.
4. **RegexSet vs Aho-Corasick.** Aho-Corasick is the shipped choice (`Cargo.toml:18`). Do not propose `RegexSet` migration without benchmark evidence.
5. **Wave 3 is closed.** v7.10.0 shipped with partial RTK scope. The remaining items are a post-Wave-3 sprint, not a Wave 3 reopening.
6. **Tier 2 #9 (shell lexer), #11 (telemetry), and most of Tier 3 are deferred** until a concrete consumer surfaces.

---

## 8. Recommended Sprint Shape for `/goal` Generation

The planner should produce **8 top-level goals** mapped to RTK-OPEN-1..8.
RTK-OPEN-5 and RTK-OPEN-8 are multi-task containers (Sub-Task 0 decision +
implementation for #5; six sub-tasks for #8). The remaining six are single
atomic goals.

**Recommended ordering:**

```
Wave A (trivial, parallelisable):
  RTK-OPEN-1  (automod for config_extractors)
  RTK-OPEN-6  (compression ratio CI)

Wave B (foundation):
  RTK-OPEN-2  (inline test framework)       depends on Wave A
  RTK-OPEN-3  (trust-gating + ADR 0015)     depends on existing edit_safety/

Wave C (behavior layer):
  RTK-OPEN-4  (graceful degradation tiers)
  RTK-OPEN-8.1 (OnceLock: structural search pattern)
  RTK-OPEN-8.2 (OnceLock: frecency read-path)

Wave D (product decisions):
  RTK-OPEN-5.0 (analytics: ADR 0017 or close)

Wave E (gated on D):
  RTK-OPEN-5.1 (analytics implementation, if green-lit)
  RTK-OPEN-7   (CLI correction learning)

Wave F (audit follow-ups, lowest priority, evidence-gated):
  RTK-OPEN-8.3..8.6
```

Each wave should close with the project verification gate
(`cargo check && cargo test --all-targets -- --test-threads=1 && cargo build
--release`) before the next wave dispatches.

---

## 9. Release Cadence Expectations

SymForge uses `release-please` (see `chore(main): release X.Y.Z` commits). Each
wave's close-out is expected to bump:

- Wave A: patch (e.g., 7.12.0 → 7.12.1) — pure refactor/test additions
- Wave B: minor (new feature surface for trust gate, inline tests)
- Wave C: minor (behavior surface change for degradation tiers)
- Wave D: patch or no-bump (ADR-only)
- Wave E: minor (analytics + correction learning are new surfaces)
- Wave F: patch (perf, no surface change)

The planner does not need to plan releases; release-please handles bumps from
conventional commits.

---

## 10. Quick-Reference Glossary

- **RTK** — Sibling Rust CLI tool whose Rust idioms are being adopted by SymForge. Not a runtime dependency.
- **Tier 1 / 2 / 3** — Both the RTK technique buckets AND the SymForge admission-control tiers (indexed / metadata-only / hard-skipped). Context disambiguates; in the wiki concept doc and this brief, "Tier 1/2/3" refers to RTK technique buckets unless explicitly labelled "admission tier".
- **Wave** — Roadmap-level grouping in `docs/plans/2026-05-15-symforge-post-h-roadmap.md`.
- **Trust gate (RTK sense)** — `.symforge/` config SHA-256 hash check. THIS doc's subject.
- **Trust gate (Wave 0 sense)** — Search-and-context calibration shipped in v7.9.x. NOT this doc's subject; only mentioned to prevent collision.
- **`.symforge/`** — Project-local SymForge config directory at repo root. Currently passive; trust gate is anticipatory protection for future executable behavior.
- **ADR** — Architecture Decision Record in `docs/decisions/`. Highest existing: 0016. RTK trust gate is reserved 0015.

---

## 11. Source Anchors (For Verification)

If the planner wants to spot-check this document against the repo before
generating tasks, these reads are sufficient:

```
Read   docs/plans/2026-05-15-symforge-post-h-roadmap.md           # Wave 3 source
Read   docs/notes/2026-05-16-rtk-once-lock-audit.md               # OnceLock follow-ups
Read   docs/notes/2026-05-16-rtk-match-output-investigation.md    # T2.6 closure rationale
Read   Cargo.toml                                                  # current profile + deps
Read   src/edit_safety/mod.rs + src/edit_safety/tee.rs            # trust.rs sibling reference
Read   src/parsing/config_extractors/mod.rs                       # RTK-OPEN-1 target
Read   src/parsing/languages/mod.rs                               # automod reference
Read   docs/decisions/0016-call-time-capability-resolution.md     # call-time pattern precedent
Read   wiki/concepts/RTK Techniques for SymForge.md               # original source-of-truth (in vault)
```

Repo HEAD at this writing: `e525ada` on `main`.

---

---

## 12. Upstream RTK Audit — Direct Read of `rtk-ai/rtk` @ v0.40.0 (2026-05-19)

The §1–11 sections above are derived from the SymForge wiki snapshot
`wiki/concepts/RTK Techniques for SymForge.md` dated **2026-04-09**. That
snapshot is ~6 weeks old and predates RTK v0.34.3 → v0.40.0 work. This
section is a direct read of the upstream `rtk-ai/rtk` repo at
`pushedAt 2026-05-19T08:29:02Z`, default branch `develop`, latest release
`v0.40.0` (2026-05-13). It catalogues:

1. What upstream RTK now ships that the wiki snapshot missed.
2. Which of those new surfaces make sense for SymForge.
3. Which definitely do not (with reasons).

The planner should fold the **"In Scope for SymForge"** items below into the
existing RTK-OPEN backlog (specifically refining RTK-OPEN-3, RTK-OPEN-5, and
RTK-OPEN-7) and **add one new goal** (RTK-OPEN-9: lints policy).

### 12.1 Upstream Repo Shape

```
rtk-ai/rtk @ develop, v0.40.0, 50k stars
├── src/
│   ├── analytics/      cc_economics.rs ccusage.rs gain.rs session_cmd.rs
│   ├── cmds/           [CLI command registry, per-tool TOML filters]
│   ├── core/           config.rs constants.rs filter.rs runner.rs stream.rs
│   │                   tee.rs telemetry.rs toml_filter.rs tracking.rs utils.rs
│   ├── discover/       lexer.rs provider.rs registry.rs report.rs rules.rs
│   ├── filters/        ~60 .toml filter definitions (ansible, gcc, jq, ...)
│   ├── hooks/          init.rs trust.rs integrity.rs permissions.rs
│   │                   hook_audit_cmd.rs hook_check.rs hook_cmd.rs
│   │                   rewrite_cmd.rs verify_cmd.rs
│   ├── learn/          detector.rs report.rs
│   └── parser/         formatter.rs types.rs
├── openclaw/           [TypeScript plugin for OpenClaw / Claude Code]
├── Formula/            [Homebrew formula]
├── hooks/              [installer assets — claude/, codex/, opencode/]
└── Cargo.toml          [pinned to 0.34.3 in source; releases moved past it]
```

Upstream still uses `lazy_static = "1.4"` and `chrono = "0.4"`, plus
`automod = "1"`, `sha2 = "0.10"`, `rusqlite = "0.31"` (bundled), `dirs = "5"`,
`ureq = "2"`, `flate2 = "1.0"`, `quick-xml = "0.37"`, `which = "8"`,
`getrandom = "0.4"`. Profile: `opt-level = 3, lto = true, codegen-units = 1,
panic = "abort", strip = true`. Lints policy (relevant — see §12.6):

```toml
[lints.rust]
unsafe_code = "deny"
warnings = "deny"
```

### 12.2 New Upstream Surfaces vs Wiki Snapshot

| Upstream Surface | Wiki Snapshot Mention | Reality at v0.40.0 |
|------------------|----------------------|--------------------|
| `src/hooks/trust.rs` | "SHA-256 hash, prompt on change" (1 paragraph) | Full module: 4-state enum (`Trusted`, `Untrusted`, `ContentChanged{expected, actual}`, `EnvOverride`), TOCTOU-safe `trust_filter_with_hash`, canonical-path keying, fail-closed canonicalization, persisted JSON store at `dirs::data_local_dir()`, CLI surface (`run_trust`, list/untrust). |
| `src/hooks/integrity.rs` | Not mentioned | Separate hash-sidecar pattern for installed hook scripts: `<hex>  <filename>\n` (sha256sum-compatible), 0o444 read-only as a speed bump, `IntegrityStatus::{Verified, Tampered{expected,actual}, NoBaseline, OrphanedHash, NotInstalled}`. Cited as remediation of finding F-01 / SA-2025-RTK-001. |
| `src/hooks/permissions.rs` | Not mentioned | Parses Claude Code permission rules from `settings.json` + `settings.local.json`, splits compound commands via `discover::lexer`, evaluates `Allow/Deny/Ask/Default` per segment with **all-segments-must-allow** semantics (cites issue #1213 bypass fix). |
| `src/hooks/init.rs` | Not mentioned | Multi-agent installer: claude, codex, cursor, gemini, hermes, opencode. Templates for `filters.toml` (project + global), embedded `RTK.md`, marker-block `<!-- rtk-instructions ... -->` injection into `CLAUDE.md`/`AGENTS.md`/`GEMINI.md`. |
| `src/core/tracking.rs` + `src/analytics/gain.rs` | "Add SQLite analytics" (1 paragraph) | Production schema: SQLite at `~/.local/share/rtk/tracking.db`, 90-day retention, GLOB project scoping, `TimedExecution` RAII timer, summary/daily/weekly/monthly aggregations, JSON + CSV exports, failures-only view, reset with confirmation. |
| `src/analytics/cc_economics.rs` + `ccusage.rs` | Not mentioned | Claude Code-specific cost analytics. Imports a session-bound cost model. Largely RTK-specific. |
| `src/learn/detector.rs` | "Mine error-then-correction patterns" (1 paragraph) | Concrete algorithm: `ErrorType` regex classifier (UnknownFlag, CommandNotFound, WrongSyntax, WrongPath, MissingArg, PermissionDenied), `is_command_error` filter (rejects user rejections), `command_similarity` (Jaccard), `CORRECTION_WINDOW = 3`, `MIN_CONFIDENCE = 0.6`, `extract_base_command` strips env-prefixes. |
| `src/discover/lexer.rs` | "Shell command lexer (~250 lines)" | Same as snapshot. ~250-line full shell tokenizer. Plus `provider.rs`, `registry.rs`, `rules.rs` for command classification. |
| `src/core/tee.rs` | Wiki #5 — "save before edit" | Upstream variant has `MIN_TEE_SIZE = 500` (skip-small-outputs), `RTK_TEE_DIR` env override, config-driven directory, `TeeMode { Never, Failures, Always }`. SymForge tee already has 1 MB cap, 20-file LRU, but no min-size skip and no mode enum. |
| `src/core/telemetry.rs` | "Fire-and-forget telemetry" | Daily HTTP ping pattern with triple opt-out exists in upstream as described. |
| `src/parser/` (formatter.rs + types.rs) | Not mentioned | Output parser/formatter shared layer. Specific to RTK's CLI-output transformation pipeline. |
| `openclaw/` | Not mentioned | TypeScript plugin (`index.ts`, `openclaw.plugin.json`, `package.json`). RTK as Claude Code plugin via OpenClaw's plugin API. |
| `[lints.rust] unsafe_code = "deny" + warnings = "deny"` | Not mentioned | Strict lints — RTK accepts no `unsafe` and treats warnings as errors. |

### 12.3 In Scope for SymForge — New Items to Add

These are upstream surfaces that the wiki missed and that **do** make sense to
adopt or borrow design from. They refine the existing RTK-OPEN backlog rather
than create competing goals, except where noted.

#### 12.3.a — Refine RTK-OPEN-3 (Trust-Gating) with the upstream four-state model

The wiki's "LOG_ONLY vs ENFORCE mode" framing is coarser than upstream's
four-state `TrustStatus` enum. The planner should update RTK-OPEN-3's
acceptance criteria to match upstream's richer surface:

```rust
pub enum TrustStatus {
    Trusted,
    Untrusted,
    ContentChanged { expected: String, actual: String },
    EnvOverride,
}
```

Additional upstream details the planner must fold in:

- **Fail-secure semantics**: any I/O or parse error on the trust store →
  treat as `Untrusted`, log to stderr, do not refuse to start. SymForge's
  daemon-startup must not crash on missing/corrupt trust store.
- **TOCTOU-safe trust recording**: the trust API should accept a
  **pre-computed** SHA-256 (compute once at check time, persist that exact
  value). Do not re-hash on write — prevents a race where the config changes
  between check and trust.
- **Canonical-path keying** via `std::fs::canonicalize`. Upstream is
  fail-closed on canonicalization failure. **SymForge note**: Windows
  canonicalization quirks (UNC paths, junctions) make pure fail-closed risky.
  Use the existing `dunce = "1"` dep (already in `Cargo.toml:71`) to
  normalize before keying. Add a Windows-specific test.
- **CI override gated by CI detection**: an env var alone is not enough.
  Upstream requires one of `CI`, `GITHUB_ACTIONS`, `GITLAB_CI`,
  `JENKINS_URL`, `BUILDKITE` to be set, otherwise the override is logged
  and ignored. SymForge should adopt this exact list to prevent `.envrc`
  injection attacks. Env var name: `SYMFORGE_TRUST_PROJECT_CONFIG=1`
  (mirror RTK naming convention).
- **Persist at `dirs::data_local_dir()`**, not `dirs::config_dir()`. Upstream
  uses `data_local_dir` for the trust JSON. SymForge already uses `dirs = "6"`
  (`Cargo.toml`); switch the planned path from `~/.config/symforge/trust.json`
  to platform-correct `dirs::data_local_dir()/symforge/trust.json` (which
  resolves to `%LOCALAPPDATA%\symforge\trust.json` on Windows,
  `~/.local/share/symforge/trust.json` on Linux).
- **CLI surface**: ship `symforge trust --list`, `symforge trust`,
  `symforge untrust` (or MCP-tool equivalents — TBD per ADR 0015) so users
  can audit and revoke trust without editing JSON by hand. RTK ships these as
  CLI subcommands; SymForge can expose them as MCP tools or as `symforge`
  binary subcommands depending on which the daemon surface allows.
- **`trusted_at` timestamp** uses `chrono::Utc::now().to_rfc3339()`. Add
  `chrono = "0.4"` to `Cargo.toml` deps. This is a new dep — call it out in
  ADR 0015.

The four-state enum + fail-secure + CI-gated override + canonical-path-with-
Windows-normalization + CLI surface together raise the bar of RTK-OPEN-3
from "ship inert with LOG_ONLY mode" to a production-grade trust gate. The
planner should treat this as **scope expansion of RTK-OPEN-3**, not a new
goal.

#### 12.3.b — Add Hash-Sidecar Pattern (`integrity.rs`)

This is a **new** technique the wiki missed. Upstream uses it for the
installed-hook script that auto-approves rewritten commands. The pattern is
small (~150 lines) and portable:

- File: `<hex_hash>  <filename>\n` (sha256sum-compatible)
- Permissions: `0o444` read-only on Unix (speed bump only; not a security
  boundary — attacker with write access can chmod it)
- Status enum: `IntegrityStatus::{Verified, Tampered{expected, actual},
  NoBaseline, OrphanedHash, NotInstalled}`

**SymForge applicability**: when `.symforge/` config gains executable behavior
later (custom queries, hooks, transforms), the integrity-sidecar pattern
complements trust-gating. Trust-gating asks "is this config trusted by the
user?", integrity asks "has this trusted config been modified since trust
was granted?". The two answer different questions and compose.

**Recommendation**: bundle this into RTK-OPEN-3 as a sub-task. Same module
(`src/edit_safety/integrity.rs`), same ADR (0015), but split into:

- 3.a Trust gate (the existing scope)
- 3.b Integrity sidecar for any **trusted** `.symforge/` file (writes
  `.symforge/<file>.sha256` alongside on trust, verifies on every load)

#### 12.3.c — Refine RTK-OPEN-5 (Analytics) with the upstream schema

The wiki said "add `rusqlite`, design schema." Upstream has a working schema
with hard-won decisions the planner should adopt verbatim:

- **Location**: `dirs::data_local_dir()/symforge/tracking.db` (platform-
  correct, not `~/.config`)
- **Retention**: 90-day rolling cleanup at daemon start
- **Project scoping**: GLOB (not LIKE) for path matching — already cited
  in `docs/notes/2026-05-16-rtk-once-lock-audit.md` as a gotcha
- **Schema** (adapted for SymForge — RTK's columns are CLI-shaped):

  ```sql
  CREATE TABLE IF NOT EXISTS tool_calls (
      id            INTEGER PRIMARY KEY,
      timestamp_utc TEXT NOT NULL,           -- chrono RFC3339
      tool_name     TEXT NOT NULL,           -- e.g. "search_text"
      project_path  TEXT NOT NULL,           -- canonical, GLOB-scoped
      response_bytes INTEGER NOT NULL,
      est_tokens    INTEGER NOT NULL,
      duration_ms   INTEGER NOT NULL,
      success       INTEGER NOT NULL         -- 0/1
  );
  CREATE INDEX IF NOT EXISTS idx_tool_calls_project ON tool_calls(project_path);
  CREATE INDEX IF NOT EXISTS idx_tool_calls_ts ON tool_calls(timestamp_utc);
  ```

- **TimedExecution RAII pattern**: each handler wraps a single timer; on
  Drop, the row is enqueued to a background mpsc channel — never sync
  insert on the hot path.
- **Aggregations**: implement summary / daily / weekly / monthly views.
  Match RTK's `gain` output shape so users familiar with one tool feel at
  home with the other.
- **Failures-only view**: a `--failures` flag (or MCP tool variant) that
  filters `success = 0`. Useful for understanding which tools error most.
- **JSON + CSV exports**: match RTK's `--format json` / `--format csv`.
- **Reset with confirmation**: a `symforge analytics --reset --yes` pattern
  (RTK's `gain --reset --yes`). MCP-side: an explicit `--yes` arg.

**Recommendation**: update RTK-OPEN-5.1's acceptance criteria to require
this schema, retention, and aggregation surface. The Sub-Task 0 product
decision still stands — the schema only matters if the project decides to
ship analytics.

#### 12.3.d — Refine RTK-OPEN-7 (Correction Learning) with upstream algorithm

The wiki was hand-wavy. Upstream `learn/detector.rs` gives concrete
parameters worth adopting (and adapting for symbol-name correction instead
of CLI-command correction):

- **`CORRECTION_WINDOW = 3`** — how many recent failed attempts to consider
  when proposing corrections.
- **`MIN_CONFIDENCE = 0.6`** — Jaccard similarity threshold below which
  no suggestion fires.
- **Same-base-command boost = 0.5** — for SymForge, this maps to
  "same-file" boost. If the user's `replace_symbol_body(name="foo")`
  fails and the file contains `foo_bar`, the same-file boost should
  dominate the similarity score.
- **User-rejection filter**: don't train on errors that look like user
  cancellation. For SymForge: don't train on `working_directory` mismatches
  or worktree-routing errors (the user knew the file existed; the wrong
  workspace was selected). Filter those out at ingest time.

**Recommendation**: update RTK-OPEN-7's algorithm description to cite
these constants and the same-file boost mechanic. Make the dependency on
RTK-OPEN-5 explicit: training data lives in the analytics DB; if analytics
doesn't ship, correction-learning falls back to **stateless** same-file
fuzzy match against the live index (which is the more important MVP shape
anyway — it works without persistent state).

#### 12.3.e — Add RTK-OPEN-9: Strict Lints Policy

New goal. Upstream RTK has:

```toml
[lints.rust]
unsafe_code = "deny"
warnings = "deny"
```

SymForge `Cargo.toml` currently has no `[lints.rust]` block. Adopting these
two lints:

- **`unsafe_code = "deny"`** — SymForge does not currently use `unsafe`
  outside of vendored tree-sitter parsers. Confirming this with a lint
  makes accidental introduction visible.
- **`warnings = "deny"`** — promotes all warnings to errors. Forces fixes
  rather than accumulation.

**Files**:
- Modify: `Cargo.toml` (add `[lints.rust]` block)

**Risks**:
- `warnings = "deny"` may catch latent unused-import / dead-code warnings.
  Run `cargo check --all-targets` first; fix or `#[allow(...)]` justified
  exceptions.
- Vendored code (e.g. `vendor/tree-sitter-scss/`) has its own lints; the
  workspace-level lint applies to the crate, not patched deps.

**Acceptance**:
- `cargo check --all-targets` and `cargo test --all-targets -- --test-threads=1`
  pass cleanly with the new lints.
- Any required `#[allow(...)]` annotations are localized and documented
  with a one-line reason.

**Estimated size**: ~5 lines `Cargo.toml` + variable fix-up depending on
current warning load.

**Sequencing**: Wave A (parallel with RTK-OPEN-1 and RTK-OPEN-6 — pure
hygiene, no dependencies).

### 12.4 Definitively Out of Scope (with reasons)

These upstream surfaces exist but the planner should NOT generate tasks for
them. Each is rejected with a concrete reason.

| Upstream | Reason for SymForge Rejection |
|----------|------------------------------|
| `src/hooks/init.rs` multi-agent installer | SymForge is itself an MCP server; agents talk to it via MCP, not via Bash hooks. SymForge's `src/cli/init.rs` already handles its own install. |
| `src/hooks/permissions.rs` Claude Code permission parser | SymForge tools don't intercept Bash commands; they ARE MCP tools subject to the host's own permission system. |
| `src/hooks/rewrite_cmd.rs` Bash command rewriter | Same — SymForge does not wrap arbitrary commands. |
| `src/filters/*.toml` (~60 CLI tool filters) | SymForge does not filter CLI output. Different product class. |
| `src/discover/lexer.rs` shell tokenizer | No current SymForge consumer needs to parse shell. Tier 2 wiki #9 stays deferred per §7 of this brief. |
| `src/analytics/cc_economics.rs` + `ccusage.rs` | Claude Code session cost analytics tied to RTK's tracking schema. SymForge's analytics (if shipped) tracks tool-call savings, not session-level $$ economics. |
| `src/core/telemetry.rs` HTTP ping | Tier 2 wiki #11. The planner should defer until a maintainer decides anonymous usage telemetry is worth shipping. Not in current RTK-OPEN backlog. |
| `openclaw/` TypeScript plugin | Different distribution channel. SymForge already ships an npm wrapper (`npm/`). The OpenClaw plugin is RTK-specific. |
| `Formula/` Homebrew | SymForge distributes via npm + cargo; brew distribution is a separate future decision, not RTK adoption. |
| `panic = "abort"` in release profile | Settled per §7 — not adopted. Don't re-open. |
| `lazy_static = "1.4"` dep | Settled per OnceLock audit — new init must use std `OnceLock`/`LazyLock`. Don't add `lazy_static`. |

### 12.5 Cargo Dep Diff for the Planner

Adopting §12.3 in full requires adding:

```toml
chrono = "0.4"     # for trust.json `trusted_at` + analytics timestamps
```

Already present and reusable (no add needed):

```toml
sha2 = "0.10"          # trust + integrity hashing
rusqlite = "0.32"      # analytics (bundled)
dirs = "6"             # platform data_local_dir
dunce = "1"            # Windows path normalization for canonical-path keying
serde = "1.0"          # store JSON
serde_json = "1.0"     # store JSON
automod = "1"          # already in use
```

Explicitly NOT to be added:

```toml
lazy_static    # forbidden — see OnceLock audit
ureq           # telemetry deferred
flate2         # CLI output decompression, not applicable
quick-xml      # XML CLI output, not applicable
which          # executable lookup, not applicable
getrandom      # only needed by telemetry salt, not applicable yet
```

### 12.6 Revised Sprint Shape (Incorporating §12.3)

The Wave-shape from §8 holds, with these refinements:

```
Wave A (trivial, parallelisable):
  RTK-OPEN-1  (automod for config_extractors)
  RTK-OPEN-6  (compression ratio CI)
  RTK-OPEN-9  (strict lints policy)            [NEW, §12.3.e]

Wave B (foundation):
  RTK-OPEN-2  (inline test framework)          depends on Wave A
  RTK-OPEN-3.a (trust-gating, expanded scope)  see §12.3.a refinements
  RTK-OPEN-3.b (integrity sidecar)             [NEW sub-task, §12.3.b]

Wave C (behavior layer):
  RTK-OPEN-4  (graceful degradation tiers)
  RTK-OPEN-8.1 (OnceLock: structural search pattern)
  RTK-OPEN-8.2 (OnceLock: frecency read-path)

Wave D (product decisions):
  RTK-OPEN-5.0 (analytics: ADR 0017 or close)

Wave E (gated on D):
  RTK-OPEN-5.1 (analytics impl, upstream schema  per §12.3.c)
  RTK-OPEN-7   (correction learning, RTK algo    per §12.3.d)

Wave F (audit follow-ups, evidence-gated):
  RTK-OPEN-8.3..8.6
```

### 12.7 Citations (For Planner Verification)

If the planner wants to independently verify §12, these are the upstream
files referenced:

```
github.com/rtk-ai/rtk @ develop
  README.md
  Cargo.toml
  src/hooks/trust.rs                    (4-state TrustStatus, CI-gated env override)
  src/hooks/integrity.rs                (hash sidecar pattern)
  src/hooks/permissions.rs              (compound-command allow-must-cover-all)
  src/hooks/init.rs                     (installer, not adopted)
  src/core/tee.rs                       (compare to SymForge tee)
  src/core/tracking.rs                  (analytics schema reference)
  src/analytics/gain.rs                 (analytics UI reference)
  src/learn/detector.rs                 (correction-learning algorithm)
  src/discover/lexer.rs                 (shell lexer, not adopted)
```

Repo HEAD at audit: pushedAt `2026-05-19T08:29:02Z`, default branch
`develop`, latest release `v0.40.0` (2026-05-13). Stargazer count:
~50,253.

---

## End of Brief

Hand this file to GPT-5.5 Pro and ask it to produce `/goal` tasks for items
RTK-OPEN-1 through RTK-OPEN-9, respecting the revised ordering in §12.6 and
the constraints in §5, §7, and §12.4. Each goal should be independently
executable with the file paths, acceptance criteria, and upstream-design
refinements stated in §6 and §12.3.
