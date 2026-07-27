# Tasks: Knowledge LLM Sift

**Input**: Design documents from `specs/020-repository-knowledge-index/sift/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [quickstart.md](quickstart.md)

**Tests**: REQUIRED. The user requested RED/GREEN/VERIFY, and the frozen contract makes one test a
*legal precondition* (WS4 diversity is permitted only after a failing corpus fixture proves flooding).

**Organization**: one phase per user story. Story ⇄ workstream mapping is 1:1:

| Story | Workstream | Priority |
|---|---|---|
| US1 | `SIFT-WS0` — global compose/rank/limit | P1 |
| US2 | `SIFT-WS1` — answer-first formatter | P1 |
| US3 | `SIFT-WS2` — truthful authority | P2 |
| US4 | `SIFT-WS3` — prose routing | P2 |
| US5 | `SIFT-WS4` — soft diversity | P3 |

> **Phase order is MANDATORY and NOT negotiable.** Unlike a normal SpecKit feature, these stories are
> **not** independently deliverable: US2 formats US1's output, US3 fills a field US2 renders, US4
> routes into US2's output, and US5 reorders US1's ranked list. Do not parallelize across phases.
> `[P]` markers below are valid **only within** a phase.

## Path conventions

Single Rust crate at repository root: `src/…`, `tests/…`. All paths below are repo-relative.

---

## Phase 1: Setup

- [ ] T001 Confirm branch `feat/knowledge-llm-sift` is checked out and that the pre-existing uncommitted change in `src/protocol/tools.rs` (foreign-project refusal) plus untracked `tests/zz_repro_foreign_project.rs` are recorded as OUT of this slice and never staged with it
- [ ] T002 Capture the pre-slice baseline per [quickstart.md](quickstart.md) §1 — response bytes, first-excerpt line number, `max_tokens=300`/`120` shapes for the release-please and persistence-boundary probes — and write them into `specs/020-repository-knowledge-index/sift/quickstart.md` §1 so SC-001/SC-002 are provable
- [ ] T003 Run `cargo test --test search_knowledge -- --test-threads=1` and record which tests pass, so any later failure is attributable to this slice and not pre-existing

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: shared scaffolding every later phase depends on. No user-visible behavior changes here.

- [ ] T004 Add `CCR_FOOTER_RESERVE_BYTES` to `src/protocol/ccr.rs`, adjacent to the footer `format!` in `apply_ccr_overflow`, sized for the LONGER compact-facade rewrite produced by `rewrite_footer_for_symforge_facade` (the replacement substring is 11 bytes longer than the original), with a unit test in the same file asserting the constant is ≥ the byte length of a rendered footer using a 12-hex handle in BOTH forms
- [ ] T005 [P] Extract the `SymbolId`-debug-free code-anchor formatter out of `code_anchor_label` in `src/protocol/knowledge_search.rs:1014-1021` into one shared `pub(crate)` function rendering `file:<path>` and `symbol:<path>#<name>:<line>`, and add a unit test pinning both forms and asserting the output contains no `{` or `SymbolId`
- [ ] T006 Point `src/protocol/knowledge_review.rs` at the shared formatter from T005 so search and review render code anchors identically, and add a parity unit test asserting both call sites produce byte-identical text for one anchor

**Checkpoint**: `cargo check` green; no behavior change observable through the MCP surface yet.

---

## Phase 3: US1 — `SIFT-WS0` Global source composition (Priority: P1) 🎯 MVP

**Goal**: `limit`, ranking, overflow, withheld, filtered counts, and coverage apply ONCE across all
selected sources — not once per source.

**Independent test**: a two-source fixture whose per-source behavior differs observably from global
behavior (see T007).

### RED

- [ ] T007 [US1] Add a failing test `global_limit_and_counts_apply_once_across_sources` to `tests/search_knowledge.rs`: build two sources each holding ≥8 matching units, query `source_scope=all` with `limit=10`, and assert exactly 10 hits and exactly ONE `overflow=` occurrence in the whole response. Run it and RECORD the red output (today it returns ~16 hits and two count blocks)
- [ ] T008 [P] [US1] Add a failing test `global_ranking_interleaves_sources_by_frozen_tuple` to `tests/search_knowledge.rs`: place a phrase-exact hit in the SECOND source and a term-only hit in the first, then assert the phrase-exact hit outranks the term-only hit despite lower source precedence. Record red
- [ ] T009 [P] [US1] Add a failing test `hits_carry_real_source_labels_not_hardcoded_current` to `tests/search_knowledge.rs` asserting a worktree-source hit renders `worktree:<id>` (and a ref-source hit renders `ref:<name>`) in its hit line — today `knowledge_search.rs:781` emits the literal `source=current`. Record red

### GREEN

- [ ] T010 [US1] Add the private `SourceHits` struct to `src/protocol/knowledge_search.rs` per [data-model.md](data-model.md), and add `source_label` / `source_precedence` / `full_coverage` to `KnowledgeHit`
- [ ] T011 [US1] Split extraction from formatting in `src/protocol/knowledge_search.rs`: convert the body of `search_current` (lines ~293-483) into a private `extract_source(generation, precedence, &query) -> SourceHits` that returns UNTRUNCATED hits and per-source counts, and that maps readiness/withheld-envelope cases to `SourceHits::readiness` instead of an early-return `String`
- [ ] T012 [US1] Move the ranking comparator out of `search_current` (lines ~440-449) into a free function over `(&KnowledgeHit)` keyed on the frozen tuple extended with `source_precedence` in position 4 (phrase → heading → distinct-term → source precedence → path → line → unit_start), in `src/protocol/knowledge_search.rs`
- [ ] T013 [US1] Add `compose_and_render(sources: Vec<SourceHits>, …) -> String` to `src/protocol/knowledge_search.rs` that concatenates all hits, sorts once with T012's comparator, applies `limit` ONCE, sums `withheld_sensitive` and `FilteredCounts` field-wise, computes `overflow` from the pre-truncation total, and takes overall coverage from `worst_source_coverage`
- [ ] T014 [US1] Rewrite `search_scoped` in `src/protocol/knowledge_search.rs:255-291` to route BOTH the `current` fast path and the multi-source path through `select_scoped_sources` → `extract_source` → `compose_and_render`, deleting the render-then-`join` path entirely (research.md Decision 5 — no second composition path)
- [ ] T015 [US1] Derive each source's label once in `extract_source` from `generation.source.location` (`current` / `worktree:<id>` / `ref:<name>`) and thread it into the scope line, the per-source identity line, and each hit — replacing the hardcoded `source=current` at `knowledge_search.rs:781`
- [ ] T016 [US1] Preserve every top-level MUST-include field from `contracts/search-knowledge.md` §Successful response in the composed renderer: per-source identity list with captured source version incl. working-tree state, publication/content generations, freshness, coverage, manifest digest; secret-policy version; worst overall coverage; source scope; overflow; withheld; authority-filtered counts; derived coverage/versions; and the `\nNo match:` seam in its exact prefix and position

### VERIFY

- [ ] T017 [US1] Run `cargo test --test search_knowledge -- --test-threads=1` and confirm T007/T008/T009 now pass and frozen contract tests 9 (current ranks ahead of divergent ref without hiding it), 11 (`source_scope=all` per-source envelopes + worst overall), and 16 (distinct scope sets, `evidence_noncurrent`) are green
- [ ] T018 [US1] Run `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`; fix only what this phase introduced

**Checkpoint**: multi-source correctness restored. This is the MVP — the contract violation is gone
even if no later phase lands.

---

## Phase 4: US2 — `SIFT-WS1` Answer-first formatter (Priority: P1)

**Goal**: the answering excerpt is readable within the first few lines, at ≤60% of baseline bytes,
and tight budgets return whole blocks only.

**Independent test**: excerpt line position + byte ratio vs the T002 baseline; block completeness at
`max_tokens=300`/`120`.

### RED

- [ ] T019 [US2] Demonstrate the vacuity of `ccr_truncation_withholds_partial_hits_and_round_trips_full_safe_output` in `tests/search_knowledge.rs:558-608`: its per-line `line.contains("docs/")` + `assert!(line.contains("authority:"))` filter passes only because every hit is one line. Replace it with a block-completeness assertion (every emitted hit block carries ALL its sub-lines; no block is partial) and record it RED against the current mega-line format
- [ ] T020 [P] [US2] Add failing excerpt unit tests in `src/protocol/knowledge_search.rs` `#[cfg(test)]`: a >1 KB table row must bound to ~240 chars; a heading-line match must fall through to the next substantive line instead of duplicating the breadcrumb; a mid-sentence match must snap to whitespace; CJK, emoji, and combining-mark content must cut on CHARACTER boundaries and never panic
- [ ] T021 [P] [US2] Add a failing test `forced_digest_prefix_collision_extends_until_unique` in `src/protocol/knowledge_search.rs` `#[cfg(test)]` with two digests sharing a 12-hex prefix, asserting BOTH render at an extended, equal, unique length — and a sibling test asserting a semantic ID (`authority-history-v1`) renders verbatim
- [ ] T022 [P] [US2] Add a failing test to `tests/search_knowledge.rs` asserting `max_tokens=300` yields header + ≥1 COMPLETE hit block + CCR handle, and `max_tokens=120` yields provenance + handle with ZERO partial hit blocks
- [ ] T023 [P] [US2] Add a failing test to `tests/search_knowledge.rs` pinning the no-match line as exactly `\nNo match: <class>` so `classify_search_knowledge_output` (`tools.rs:277-284`) cannot silently misclassify after the reformat

### GREEN

- [ ] T024 [US2] Implement Unicode-safe excerpt windowing in `match_unit` / a new `window_excerpt` in `src/protocol/knowledge_search.rs` (replacing the raw `line.to_string()` at ~:586): ~240 Unicode chars, match kept inside the window, cuts on `char_indices` boundaries of the ORIGINAL line (never byte offsets derived from lowercased text), whitespace snapping, heading-duplicate fallthrough, and skip blank / heading-only / fence-marker / table-separator fallbacks while keeping list markers and code. No Markdown parser
- [ ] T025 [US2] Add the `DisplayIds` abbreviation pass to `src/protocol/knowledge_search.rs` per [data-model.md](data-model.md): classify hex-only IDs of length ≥12 as digests, compute ONE response-wide prefix length starting at 12 and extending until all digests are unique, render semantic IDs verbatim
- [ ] T026 [US2] Replace the flat `bridge_previews` with class-partitioned `BridgePreviews` in `src/protocol/knowledge_search.rs:667-696`: partition exact / declared-set / ambiguous / missing, reserve ≥1 slot per PRESENT class, fill the remaining global cap in class order, and emit per-class omitted counts. Drop no class (frozen contract test 18)
- [ ] T027 [US2] Replace the per-hit mega-line at `src/protocol/knowledge_search.rs:779-808` with the indivisible answer-first block from [data-model.md](data-model.md) §Rendered shape, and compact the envelope to Trust / Secret policy / Scope+counts / `Source[n]` per source / Derived
- [ ] T028 [US2] Add the private `SearchKnowledgeOutput { rendered, budget_rendered }` to `src/protocol/knowledge_search.rs` (mirroring `ReviewKnowledgeOutput`), packing `budget_rendered` from WHOLE hit blocks only, up to `max_bytes - CCR_FOOTER_RESERVE_BYTES` (T004), and change `search_scoped` to return it
- [ ] T029 [US2] Switch `src/protocol/tools.rs:5461` from `apply_ccr_budget` to `apply_ccr_budget_with_summary`, passing `rendered` as the full output and `budget_rendered` as the summary — so CCR stores the FULL pre-truncation safe output (never the summary) per `ccr.rs:232-255`

### VERIFY

- [ ] T030 [US2] Run `cargo test --test search_knowledge -- --test-threads=1`; confirm T019-T023 pass and frozen contract tests 3, 7, and 18 are green
- [ ] T031 [US2] Measure against the T002 baseline: first-excerpt line number ≤ ~8 and default 10-hit bytes ≤ 60% of baseline (SC-001, SC-002). Record both numbers in [quickstart.md](quickstart.md) §5
- [ ] T032 [US2] Verify the CCR round-trip returns the full pre-truncation output byte-for-byte (SC-004), and run `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`

**Checkpoint**: the lane is readable. WS0+WS1 together already deliver the product outcome.

---

## Phase 5: US3 — `SIFT-WS2` Truthful authority labels (Priority: P2)

**Goal**: a lifecycle claim cites evidence or says `Unknown`; common repository conventions carry a
real domain; nothing becomes `Suppressed`.

**Independent test**: hand-labeled path fixtures + a zero-delta `suppressed` count.

### RED

- [ ] T033 [US3] Add a failing unit test in `src/live_index/knowledge_authority.rs` `#[cfg(test)]` asserting a unit with NO lifecycle evidence derives `KnowledgeLifecycle::Unknown` — today `derive_native_lifecycle` falls through to `(Active, LifecycleEvidence::None)` at line 1098. Record red
- [ ] T034 [P] [US3] Add failing overmatch guards in `src/live_index/knowledge_authority.rs` `#[cfg(test)]`: `docs/special/report.md`, `docs/redesign/x.md`, and `docs/inspection/x.md` must NOT be classified by the `/spec` and `/design` substring rules at lines 1121-1122. Record red
- [ ] T035 [P] [US3] Add a failing unit test asserting heading evidence takes precedence over a conflicting path convention in `src/live_index/knowledge_authority.rs`
- [ ] T036 [P] [US3] Add failing per-rule tests in `src/live_index/knowledge_authority.rs` for each row of the WS2 table (root `AGENTS.md`/`CLAUDE.md`/`GEMINI.md` + `.agent/` → Operations; `docs/solutions/` → Decision; `docs/reviews/`, `docs/dogfood/`, `research/` → HistoricalRecord; plan/plans/roadmap → NormativeIntent; tasks/handoff/handover → Operations; archive(d) → existing HistoricalRecord)

### GREEN

- [ ] T037 [US3] Change `derive_native_lifecycle` in `src/live_index/knowledge_authority.rs:1068-1098` to return `(KnowledgeLifecycle::Unknown, LifecycleEvidence::None)` on the no-evidence fallthrough, and fix the two in-file tests that assert `Active` (lines ~2049, ~2220) ONLY where they relied on the unevidenced default — do not weaken a test that asserts an evidenced `Active`
- [ ] T038 [US3] Reorder `derive_native_authority_domain` in `src/live_index/knowledge_authority.rs:1101+` so heading evidence is evaluated BEFORE path conventions
- [ ] T039 [US3] Replace the substring path matching at `src/live_index/knowledge_authority.rs:1121-1140` with component tokenization mirroring `path_convention_roles` (`src/live_index/knowledge_bridge.rs:743-785`) — split on `/`, then on non-ASCII-alphanumeric, lowercase, exact token match — and implement the WS2 table with new stable rule IDs
- [ ] T040 [US3] Assert by construction that NO new path rule emits `AuthorityDomain::CurrentImplementation` (its `DeterministicConflict → Suppressed` path at `knowledge_authority.rs:234`/`:1394` would hide units from default scope), and add a test that a `CLAUDE.md`-class unit under a simulated `DeterministicConflict` stays visible at `authority_scope=default`

### VERIFY

- [ ] T041 [US3] Run `cargo test --test search_knowledge -- --test-threads=1` plus `cargo test knowledge_authority`; confirm T033-T036 pass and frozen contract test 16 stays green
- [ ] T042 [US3] Capture `review_knowledge(mode=summary)` after the change and assert `suppressed` delta versus the T002 baseline is EXACTLY zero (SC-006); record the `domain=unknown` / `voice=unknown` movement as a DIAGNOSTIC, not a gate
- [ ] T043 [US3] Prove an active `research/` or `docs/dogfood/` measurement document is still returned under `authority_scope=default`

**Checkpoint**: labels are true. No unit was hidden.

---

## Phase 6: US4 — `SIFT-WS3` Prose routing (Priority: P2)

**Goal**: noun-anchored prose questions reach `search_knowledge`; code intent does not move.

**Independent test**: the cue table routes to knowledge; the false-positive table stays code-routed.

### RED

- [ ] T044 [US4] Extend `ask_routes_explicit_knowledge_intent_without_stealing_code_intent` in `tests/search_knowledge.rs:611-642` with the seven new cues (`what is our policy on `, `where is our policy on `, `what is the process for `, `where is the runbook for `, `what does the spec say about `, `how does our policy on `, `how does the process for … work`) and record them RED
- [ ] T045 [P] [US4] In the same test, add the false-positive table — `find references to X`, `where is search_knowledge defined`, `retry policy in the client code` — asserting they stay code-routed, plus a generic `how does X work` asserting the existing Understand/Explore route is UNCHANGED

### GREEN

- [ ] T046 [US4] Add the seven noun-anchored prefixes to the intent classifier in `src/protocol/smart_query.rs` (~L110 cue region), anchored on doc nouns only — do NOT add a bare `how does ` prefix, which would steal the generic Understand lane
- [ ] T047 [US4] Rewrite the `search_knowledge` tool description at `src/protocol/tools.rs:5413` to lead with answer-oriented routing intent ("Search the repository's prose knowledge — docs, specs, plans, decisions, process, runbooks — when you don't know the path. Prefer this for 'what is our policy/design/process for X'. Use search_text/search_symbols for code."), keeping the existing provenance/scope sentence after it

### VERIFY

- [ ] T048 [US4] Run `cargo test --test search_knowledge -- --test-threads=1`; confirm T044/T045 pass and no other routing test regressed
- [ ] T049 [US4] Confirm the description change did not alter the tool schema (contract test 1 — still exactly eight input fields) and that the compact surface count is still three (contract test 10)

**Checkpoint**: prose questions land in the prose lane.

---

## Phase 7: US5 — `SIFT-WS4` Two-pass soft diversity (Priority: P3)

**Goal**: one file cannot flood the answer, without starving legitimate single-file corpora.

**Independent test**: the flooding fixture must be observed RED before any diversity code exists —
this is a contract precondition, not a preference.

### RED

- [ ] T050 [US5] Add the real-corpus flooding fixture `same_file_flooding_defers_to_distinct_file_canonical_hit` to `tests/search_knowledge.rs`: a corpus where >2 hits from one file outrank a distinct-file canonical hit. Run it and RECORD the red output — `contracts/search-knowledge.md` §Query interpretation step 5 permits diversity ONLY after this proof exists
- [ ] T051 [P] [US5] Add a failing guard test `single_file_corpus_is_not_underfilled` to `tests/search_knowledge.rs`: a corpus whose only matches are many sections of one legitimate policy file must still fill `limit`
- [ ] T052 [P] [US5] Add a failing guard test `one_term_noise_never_promoted_over_full_coverage` to `tests/search_knowledge.rs`

### GREEN

- [ ] T053 [US5] Add the pure function `apply_diversity(hits: Vec<KnowledgeHit>, limit: usize) -> Vec<KnowledgeHit>` to `src/protocol/knowledge_search.rs`: pass 1 walks the globally-ranked list in base order admitting a hit only when its path has fewer than 2 admitted `full_coverage` hits, deferring the rest; pass 2 appends deferred hits in ORIGINAL base order until `limit`. Drop nothing; introduce no score weight or tunable
- [ ] T054 [US5] Call `apply_diversity` in `compose_and_render` AFTER the global sort and BEFORE truncation, in `src/protocol/knowledge_search.rs`

### VERIFY

- [ ] T055 [US5] Run `cargo test --test search_knowledge -- --test-threads=1`; confirm T050 flips red→green, T051/T052 pass, and frozen contract test 8 (byte-for-byte deterministic ranking) is STILL green — diversity must be a deterministic stable permutation

**Checkpoint**: flooding reduced without regression.

---

## Phase 8: Polish & cross-cutting

- [ ] T056 Run the full frozen contract set and confirm tests 3, 7, 8, 9, 11, 16, 18 are green in one run of `cargo test --test search_knowledge -- --test-threads=1`
- [ ] T057 Run the complete verification gate: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`
- [ ] T058 Run `cargo check --no-default-features --features embed` for Constitution VI (embed isolation)
- [ ] T059 Manual dogfood per [quickstart.md](quickstart.md) §5: re-run the release-please and persistence-boundary probes plus `max_tokens=120`/`300`, and record BEFORE/AFTER bytes and first-excerpt line position (measured, not estimated)
- [ ] T060 Update `CLAUDE.md` if and only if this slice falsified a claim in it (documentation-hygiene rule); do not add a new doc file
- [ ] T061 Run `cargo clean` (CLAUDE.md Windows disk rule) and confirm `src/protocol/tools.rs`'s pre-existing foreign-project change and `tests/zz_repro_foreign_project.rs` were never staged with this slice

---

## Phase 9: US6 — `SIFT-WS5` Admission coverage honesty (Priority: P1)

**Goal**: no code-intelligence tool returns an unqualified negative when its scope contained files it
never searched, and every reported exclusion reason is the actual cause.

**Source**: `.scratch/symforge-dogfood-issues-2026-07-27.md` (SF-DOG-001…005), independently
reproduced on `E:\project\symforge`.

> **Ordering note**: SF-DOG-004 (`SIFT-WS5A`) comes FIRST even though it is rated LOW. It is the
> diagnostic that makes SF-DOG-001's true cause observable — `SkipReason::UnsupportedLanguage` is
> currently a catch-all for ≥11 dispositions, so today it is impossible to tell *why* a given file was
> excluded. Fix the reason codes, then root-cause the demotion.

### WS5A — SF-DOG-004: distinct admission reasons (unblocks the rest)

- [ ] T062 [US6] Add a failing test in `src/live_index/store.rs` `#[cfg(test)]` asserting that `MetadataOnlyReason::SensitiveContent`, `SensitivePath`, `LfsPointer`, `PlatformPathCollision`, `UnsupportedPathEncoding`, `PathMetadataTooLarge`, and `UnsupportedTextEncoding` each map to a DISTINCT `SkipReason` — today all seven collapse to `UnsupportedLanguage` at `store.rs:3360-3366`. Record red
- [ ] T063 [US6] Add a failing test asserting `FileDisposition::Unreadable`/`UnstableDuringRead`/`AbortedCircuitBreaker` (`store.rs:3379`) and the missing-ingest-plan path (`store.rs:3673`) do NOT report `UnsupportedLanguage`. Record red
- [ ] T064 [US6] Extend `SkipReason` in `src/domain/index.rs:1378-1431` with the missing variants and honest `Display` text, keeping `UnsupportedLanguage` meaning ONLY "extension maps to no supported grammar", and update the reverse mapping at `store.rs:3394-3402` so the round trip stays total
- [ ] T065 [US6] Fix every `SkipReason::UnsupportedLanguage` catch-all arm in `src/live_index/store.rs` to emit its real reason; verify `cargo check` catches all match arms via exhaustiveness
- [ ] T066 [US6] **Root-cause the actual demotion**: with honest reasons in place, re-run `get_file_context` on `src/protocol/knowledge_search.rs` (48 KB Rust) and `src/knowledge/mod.rs` (21 KB Rust) and RECORD the true reason. Ruled out already: size (87 KB `knowledge_authority.rs` indexes fine), parse failure (not in the quarantine registry), CRLF (universal in this repo), and secret-pattern content (zero detector literals in `knowledge_search.rs`). Fix the underlying cause so both files return to Tier 1 — supported languages belong in code intelligence (FR-022)
- [ ] T067 [US6] VERIFY: `search_symbols("search_scoped")` and `search_text("search_scoped")` return the real hits in `src/protocol/knowledge_search.rs`; `cargo test --lib store` and `discovery` tests green

### WS5B — SF-DOG-001: no unqualified negatives

- [ ] T068 [US6] Add a failing test asserting a zero-hit `search_text` whose scope contained excluded files reports the excluded count and reason class rather than a bare "No matches" (`src/protocol/tools.rs` search_text zero-result path)
- [ ] T069 [US6] Thread admission-coverage state into the search result envelope so `search_text` and `search_symbols` can state incomplete coverage, reusing the existing trust-envelope mechanism (Constitution III) rather than inventing a second one
- [ ] T070 [US6] Decide and record the fallback question: implement a bounded lexical fallback ONLY for non-security dispositions (FR-023), or ship the explicit-coverage result alone. Security dispositions (`SensitivePath`, `SensitiveContent`) must never be lexically read — frozen Feature 020 security contract
- [ ] T071 [US6] VERIFY with a fixture whose only match lives in an excluded file: the result is never an unqualified negative, and a scoped search does not scan unrelated roots

### WS5C — SF-DOG-002: planner must not dead-end

- [ ] T072 [US6] Add a failing test asserting `edit_plan` on a symbol inside a metadata-only file returns a typed `oversized_file`/`metadata_only` unavailability — NOT a generic "target not found" — and never recommends `search_symbols` as the sole recovery when that index cannot see the file
- [ ] T073 [US6] Make the planner admission-aware in `src/protocol/edit_plan.rs` (and the `edit_plan` handler in `src/protocol/tools.rs`), preserving the known Tier-2 state through to the response and emitting a recovery action that actually works
- [ ] T074 [US6] VERIFY: every recovery tool named in the response can operate on the target

### WS5D — SF-DOG-003: `what_changed` default agrees with its schema

- [ ] T075 [US6] Add failing contract tests pinning omitted / `true` / `false` `code_only` behavior for `what_changed` uncommitted mode — today the runtime default is `true` (`src/protocol/tools.rs:3155-3156`) while the description at `:7725` implies `false`
- [ ] T076 [US6] Resolve the mismatch. Report recommends making omitted behave as `false` (safer for handovers: specs, task ledgers, migrations travel with code). Confirm against the existing note at `tools.rs:7878-7886` before choosing, then align schema text, runtime default, and result messaging
- [ ] T077 [US6] VERIFY with a fixture worktree holding one `.ts` and one `.md` change

### WS5E — SF-DOG-005: unambiguous compact health

- [ ] T078 [US6] Add a failing test asserting compact health states the discovered denominator and that tier counts sum to it — today compact prints `Files: 887 indexed` and `Admission tiers: 887/41/0` with no 928 denominator, while full health correctly reports `928 discovered`
- [ ] T079 [US6] Fix the compact rendering in `src/live_index/health_view.rs` / its formatter to the unambiguous form, e.g. `Files: 928 discovered; Tier 1: 887 indexed; Tier 2: 41 metadata; Tier 3: 0 skipped`
- [ ] T080 [US6] VERIFY compact and full health expose the same denominator

### WS5 close-out

- [ ] T081 [US6] Investigate the index-binding discrepancy observed during this session: MCP tools reported 887 files / 25005 symbols for symforge while the PostToolUse hooks simultaneously reported 713 files / 15906 symbols (testpilot's exact figures). Determine whether the hook path resolves a different project than the MCP path; file or fix accordingly — a hook reporting another repository's index would mislead every session
- [ ] T082 [US6] Re-run the full verification gate (T057) after WS5, since WS5 touches admission — the widest-blast-radius subsystem in this slice

---

## Dependencies

```text
Phase 1 (Setup)
  └─> Phase 2 (Foundational: T004-T006)
        └─> Phase 3 US1/WS0  ── MVP boundary ──
              └─> Phase 4 US2/WS1
                    └─> Phase 5 US3/WS2
                          └─> Phase 6 US4/WS3
                                └─> Phase 7 US5/WS4
                                      └─> Phase 8 (Polish)

Phase 9 US6/WS5 (admission honesty) — INDEPENDENT of WS0-WS4:
  WS5A (reason codes) ──> WS5B (no false negatives)
                     └──> WS5C (planner)
  WS5D, WS5E ── independent of everything
```

**WS5 is a disjoint file set** (`domain/index.rs`, `live_index/store.rs`, `live_index/health_view.rs`,
`protocol/edit_plan.rs`) apart from small touches in `protocol/tools.rs`. It could run before, after,
or beside WS0-WS4. It is placed last so the contract-gated knowledge work is not destabilized by
changes to admission — the widest-blast-radius subsystem here. **WS5A must precede WS5B/WS5C**: the
honest reason codes are what make SF-DOG-001's true cause observable.

**Strictly serial across phases.** Every phase edits `src/protocol/knowledge_search.rs` and/or
`tests/search_knowledge.rs`, so concurrent phases would conflict on the same files even if their
logic were independent — which it is not (see the plan's ordering rationale).

## Parallel opportunities (within a phase only)

- **Phase 2**: T005 and T006 touch different files after T004 lands (`knowledge_search.rs` vs `knowledge_review.rs`).
- **Phase 3 RED**: T008, T009 are independent new test functions alongside T007.
- **Phase 4 RED**: T020, T021, T022, T023 are independent test additions.
- **Phase 5 RED**: T034, T035, T036 are independent unit tests.
- **Phase 7 RED**: T051, T052 are independent guard tests alongside T050.

GREEN tasks within a phase are NOT parallelizable — they mutate the same functions in sequence.

## Implementation strategy

**MVP = Phase 3 (US1 / `SIFT-WS0`).** It alone removes the frozen-contract violation. Everything after
it is usability, truthfulness, routing, and ordering — real product value, but not correctness.

**Incremental delivery**: each phase ends at a checkpoint where `cargo test --test search_knowledge`
is green, so the branch is landable at any checkpoint.

**Stop conditions**: if a frozen contract appears to conflict with the Cursor plan, STOP and report —
do not resolve it locally. If a MUST-include field would have to be dropped to hit a byte target, the
byte target yields, not the contract.

## Task count summary

| Phase | Story / WS | Tasks |
|---|---|---:|
| 1 Setup | — | 3 |
| 2 Foundational | — | 3 |
| 3 | US1 / WS0 | 12 |
| 4 | US2 / WS1 | 14 |
| 5 | US3 / WS2 | 11 |
| 6 | US4 / WS3 | 6 |
| 7 | US5 / WS4 | 6 |
| 8 Polish | — | 6 |
| 9 | US6 / WS5 | 21 |
| **Total** | | **82** |

## Scope note

Phases 1–8 are the Knowledge LLM Sift slice as briefed. **Phase 9 (WS5) was added mid-flight** at the
owner's request, from a second agent's dogfood defect ledger. It roughly doubles the slice and targets
a different subsystem (admission/coverage) than WS0–WS4 (knowledge retrieval). Splitting it into its
own branch and PR is entirely reasonable and would keep both diffs reviewable; it is kept here because
the owner asked for these to be resolved together and both serve the same goal — an LLM orienting in
an unknown repository without wasting tokens or drawing false conclusions.
