# Tasks: Admission Coverage Honesty

**Feature**: `specs/021-admission-coverage-honesty/`
**Input**: [spec.md](spec.md), [plan.md](plan.md), [research.md](research.md)
**Requirements source (read-only)**: `E:\project\testpilot\.scratch\symforge-dogfood-issues-2026-07-27.md`
**Inherited prerequisite**: `specs/020-repository-knowledge-index/sift/tasks.md` Phase 9, `T062`–`T082`

**Tests**: REQUIRED. `FR-026` — every fix ships with a regression test that fails on the pre-fix
behavior and passes after. **A check that passes either way is not evidence**, so every VERIFY task
below names a runnable command *and* the specific assertion that fails without the fix.

**Task IDs start at T100** so they cannot collide with the sift slice's `T001`–`T082`.

## Story ⇄ workstream ⇄ finding map

| Story | WS | Finding | Priority | Tasks |
|---|---|---|---:|---|
| — | shared | `0.0 MB` rendering (`SF-DOG-002`/`004`/`008`/`009`) | P1 | T107–T109 |
| US1 | `ACH-01` | `SF-DOG-007` — **wrong write** | P1 | T110–T118 |
| US6 | `SIFT-WS5` | `SF-DOG-001`…`005` — **inherited** | P1 | `T062`–`T082` (gated at T119–T121) |
| US2 | `ACH-02` | `SF-DOG-006` | P1 | T122–T131 |
| US3 | `ACH-03` | `SF-DOG-009` | P1 | T132–T142 |
| US4 | `ACH-04` | `SF-DOG-008` | P1 | T143–T154 |
| US5 | `ACH-05` | index identity (`T081`'s symptom) | P1 | T155–T167 |

> **`ACH-01` is independent and must be schedulable immediately.** It is the only finding that causes
> a **wrong write** — an agent following the plan edits an unrelated file. Its fail-closed fix depends
> on nothing: the predicate already exists at `src/protocol/edit_plan.rs:90` and the Tier-2 lookup
> already exists at `src/live_index/query.rs:1243-1256`. It does **not** wait on the admission root
> cause, on the honest reason codes, or on any other phase. If only one workstream ships, ship this
> one. `[P]` markers are valid **within** a phase only.

## Path conventions

Single Rust crate at repository root: `src/…`, `tests/…`. All paths repo-relative.

## Standing rules for every task

- **Disk**: run `df -h /e | tail -1` before and after any `cargo` invocation. **Abort below 6 GB.**
  `cargo clean` after any full gate.
- **Measurement channel**: quote index numbers from the MCP `status` route only, never from the
  PostToolUse hook — that hook is the surface `ACH-05` exists to fix (see [research.md](research.md) §6).
- **PR #479 footprint** (as of 2026-07-28): `.github/workflows/ci.yml`, `Cargo.toml`,
  `src/protocol/edit_tools.rs`, `src/protocol/tools.rs`, `src/server/serve.rs`, `tests/serve_port.rs`,
  `tests/stel_symforge_edit.rs`. Only `ACH-04` needs one of these; it is gated at **T144**.
- **Do not touch** `E:\project\symforge-pr479`, and do not edit the read-only ledger.
- **Frozen security invariant, verbatim**: *a bounded lexical fallback must NEVER read files excluded
  for `SensitivePath` or `SensitiveContent`. Feature 020's security contract governs.*

---

## Phase 1: Setup, investigation, and owner rulings

**Purpose**: settle what is genuinely unknown before any fix is designed around a guess, and pin the
measurement channel. Three causes are **undetermined** and two design questions need an owner.

- [ ] **T100** Confirm the working state: branch checked out; MCP `status` reports
  `project_root` matching this checkout (record the exact value, e.g. `//?/E:/project/symforge`) with
  `index_ready: true` and the file/symbol counts; `df -h /e | tail -1` ≥ 6 GB; `git status --porcelain`
  recorded. Write into this task's receipt the binding rule that **every number quoted anywhere in
  this feature comes from MCP `status`, never from the PostToolUse hook** ([plan.md](plan.md)
  §Constraints).
- [ ] **T101** **Amend the prerequisite.** Correct the elimination list in
  `specs/020-repository-knowledge-index/sift/tasks.md:231` (`T066`): the recorded elimination
  *"secret-pattern content (zero detector literals in `knowledge_search.rs`)"* is **invalid** — the
  `secret.context-assignment` rule (`src/knowledge/mod.rs:89-95`) needs no secret literal, because
  `let token = token.to_lowercase();` at `src/protocol/knowledge_search.rs:1328` matches it. Replace
  that elimination with the measured root cause and evidence pointer
  ([research.md](research.md) §1). Documentation edit only — no source change. **This is the only
  edit 021 makes to the prerequisite.**
- [ ] **T102** [INVESTIGATION] **Measure the demotion blast radius across ALL languages**, not just
  `src/**/*.rs`. `19` of `167` `src/**/*.rs` files are known demoted; the full index is 891 files
  including Markdown, TOML, JSON, YAML, and `tests/`, and the non-Rust count is **unmeasured**
  ([research.md](research.md) §7 `D5`). Record: total demoted, per-`MetadataOnlyReason` breakdown,
  per-rule-id breakdown, and the explicit list. **Output is the SC-015 baseline** — the set that must
  be back at Tier 1 after `T066`.
- [ ] **T103** [INVESTIGATION] Resolve the one ledger disagreement
  ([research.md](research.md) §7 `D6`): `backend/src/modules/testing/services/generator.service.ts`
  scans **CLEAN** under the verbatim detector (22912 bytes today vs "~28 KB" in the 2026-07-27
  ledger), yet the ledger reports it demoted. Either the file changed since the reproduction, or that
  single `SF-DOG-004` entry has a **different cause**. One `get_file_context` against a
  testpilot-bound index settles it. **Record the answer**; do not assume the rule fix covers it.
- [ ] **T104** [DECISION — owner ruling, recorded] **Full-file demotion vs. per-range suppression**
  ([plan.md](plan.md) §D1). Today one finding discards a whole file's symbols and its byte buffer
  (`src/knowledge/mod.rs:318-328`). `FR-023`/`T070` freezes "security dispositions must never be
  lexically read" — does that also forbid indexing the file's *other* symbols? Plan recommends
  **keeping full-file demotion** (measured genuine-secret population in demoted files: **0 of 29
  findings**). `ACH-02`'s and `SIFT-WS5B`'s fallback scope both depend on this answer, so it must be
  recorded before either proceeds.
- [ ] **T105** [DECISION — owner ruling, recorded] **`around_symbol` on a Tier-2 file**
  ([plan.md](plan.md) §D2): explicit structured refusal, or degrade to a text search for the symbol
  name? Plan recommends **explicit refusal** (fail-closed; the substitute silently changes mode
  semantics, which is the defect class `SF-DOG-006` *is*). **T126** tests whichever is ruled.
- [ ] **T106** Baseline test run: `cargo test --all-targets -- --test-threads=1` and record which
  tests pass, so any later failure is attributable to this feature and not pre-existing. Then
  `df -h /e | tail -1` and `cargo clean` if `target/debug` is large.

**Checkpoint**: nothing unknown is being designed around. `T102`'s list is the measurable baseline.

---

## Phase 2: Foundational — one honest size renderer (shared across four findings)

**Goal**: no non-empty file ever reports zero size. `FR-013`.

**Why one task, not four**: `size 0.0 MB` appears in `SF-DOG-002` (ledger line 134),
`SF-DOG-004` (line 288), `SF-DOG-008` (line 497), and `SF-DOG-009` (line 542). It is **one**
precision bug: `src/sidecar/handlers.rs:905` divides a populated `byte_len` by 1 MiB and renders it at
`{:.1}` (`:920` and `:934`), so **every file under ~51 KB prints `0.0 MB`**. Landed early so every
later VERIFY asserts against a correct size.

### RED

- [ ] **T107** Add a failing test in `src/sidecar/handlers.rs` `#[cfg(test)]` (or
  `tests/impact_admission_consistency.rs`) asserting a ~10 KB non-empty file's rendered size is
  non-zero in both `impact_skipped_text` branches (`src/sidecar/handlers.rs:918-923` and `:930-938`).
  **Record red** — today it renders `size 0.0 MB`.

### GREEN

- [ ] **T108** Replace the `size_mb` computation at `src/sidecar/handlers.rs:905` with a scaled
  renderer (B / KB / MB chosen by magnitude, or exact bytes). Reuse the existing `human_size` helper
  that `src/protocol/format.rs:3668` already calls rather than writing a second one; if it is not
  reachable from `handlers.rs`, note that and use the smallest local equivalent. Update both render
  sites (`:920`, `:934`).

### VERIFY

- [ ] **T109** Run `cargo test --lib sidecar -- --test-threads=1`. **Fails without the fix**: `T107`
  asserts the rendered string does **not** contain `0.0 MB` for a 10240-byte fixture; the pre-fix code
  emits exactly that. Then `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings`; fix only what this phase introduced.

**Checkpoint**: sizes are honest. Four findings lose one of their symptoms.

---

## Phase 3: US1 — `ACH-01` fail-closed path-shaped targets (Priority: P1) 🎯 MVP — INDEPENDENT

**Goal**: a path-shaped `edit_plan` target resolves to that path or to a typed miss. It **never**
degrades to fuzzy symbol matching. `FR-001`–`FR-004`.

**Independent test**: a nonexistent `.ts` path while an indexed file holds a symbol named `ts`; plus a
path that exists but is Tier 2; plus a bidirectional guard that `Type.Method` / `Foo::bar` still
resolve.

**Depends on**: nothing. Schedulable immediately, in parallel with Phase 1's investigations if desired.
Outside PR #479's footprint.

### RED

- [ ] **T110** [US1] Extend `tests/edit_plan_literal_path_precedence.rs` (the existing SF-AAP-001
  regression, which covers only the *existing-Tier-1-path* case) with a failing test: a
  **nonexistent** path ending in `.ts` while an indexed file contains a symbol named `ts`. Assert the
  output contains no symbol or mutation recommendation. **Record red** — today
  `src/protocol/edit_plan.rs:107` feeds the full path into the cascade, `strip_qualification`
  (`src/live_index/disambiguation.rs:438-442`) reduces it to `ts`, and the plan recommends
  `edit_within_symbol` / `replace_symbol_body` / `batch_rename` / `delete_symbol` against
  `Constant ts`. Follow the file's existing `#![cfg(feature = "server")]` gating and
  `LiveIndex`+`TempDir` fixture pattern.
- [ ] **T111** [P] [US1] Add a failing test in the same file: a path that **exists but is Tier 2** must
  yield a `metadata_only` disclosure with the real reason, textually distinct from `file_not_found`.
  **Record red** — today `index.all_files()` (`src/live_index/query.rs:1233-1235`) iterates Tier 1
  only, so a valid Tier-2 path misses both `exact` (`edit_plan.rs:84-86`) and `suffix_matches`
  (`:91-99`) and lands on the identical fuzzy path.
- [ ] **T112** [P] [US1] Add the **bidirectional guard** test (must stay green): `Type.Method` (Go
  receiver) and `Foo::bar` MUST still resolve through the symbol cascade with unchanged output. This
  is the regression the veto could cause — `target.contains('.')` is too broad a path test
  ([plan.md](plan.md) §D3).

### GREEN

- [ ] **T113** [US1] Redefine the path-shaped predicate at `src/protocol/edit_plan.rs:90`: key on a
  **path separator or a known-file-extension tail**, not on any dot. Keep it usable for both its
  current purpose (gating `suffix_matches`) and its new purpose (vetoing the cascade), hoisting or
  widening its scope as needed.
- [ ] **T114** [US1] Gate the cascade arm on `!path_shaped` in the
  `match suffix_matches.len()` block at `src/protocol/edit_plan.rs:103-121` — the `0 =>` arm at
  `:107` must not run `collect_selector_hits` with the path as the selector. **Delete or rewrite the
  comment at `:104-106`**, which currently sanctions the fall-through as intent.
- [ ] **T115** [US1] Add the path-shaped miss outcomes at `src/protocol/edit_plan.rs:132-136`:
  `metadata_only` (with the real reason, read from `LiveIndex::metadata_only_skipped_paths()`,
  `src/live_index/query.rs:1243-1256` — already on the same `impl` `plan_edit` borrows, no new API) and
  `file_not_found` / new-file plan pointing at `analyze_file_impact(new_file=true)`. **Must not** emit
  the generic `search_symbols(query="...")` hint (currently unconditional at `:134`) for a path-shaped
  target — that hint is what invites the fuzzy retry. Record the `D3` ruling
  (`file_not_found` vs. a first-class `new_file` plan; the ledger accepts either) in this task's
  receipt.
- [ ] **T116** [US1] Confirm `src/live_index/disambiguation.rs:438-442` (`strip_qualification`) is
  **unchanged** — `FR-004`. It is correct for its intended input; constraining it would break Go/C++
  selector resolution. Add a one-line comment at the new veto naming it, so a future fixer does not
  "fix" the wrong function.

### VERIFY

- [ ] **T117** [US1] Run
  `cargo test --test edit_plan_literal_path_precedence -- --test-threads=1` plus
  `cargo test --test edit_plan_symbol_line -- --test-threads=1`. **Fails without the fix**: `T110`
  asserts the literal string `Constant ts` is **absent** from the response, which the pre-fix build
  emits; `T112` asserts `Type.Method` output is unchanged, which a naive dot-based veto would break.
- [ ] **T118** [US1] Repeat `T110`'s scenario with `.js`, `.rs`, and `.py` paths (ledger `SF-DOG-007`
  acceptance check: "prove extensions are not fuzzy symbol queries"). Then `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings`.

**Checkpoint**: the wrong-write defect is closed. **This is the MVP and it is landable alone.**

---

## Phase 4: Prerequisite gate — `SIFT-WS5` (`T062`–`T082`)

**Purpose**: `ACH-03` and `ACH-04` cannot be truthful while `SkipReason::UnsupportedLanguage` is a
≥11-way catch-all (`src/live_index/store.rs:3360-3366`, `:3376-3380`, `:3673`;
`Display` at `src/domain/index.rs:1424`). No test can assert *which* cause fired, so
`ACH-03` cannot tell a real policy exclusion from `reason: None`, and `ACH-04` cannot make the
index receipt and the search response name the **same** reason.

**This phase adds no implementation.** It gates on the prerequisite and verifies its outcome.
`SF-DOG-004` (rated LOW) is scheduled ahead of two HIGHs for exactly this reason.

- [ ] **T119** GATE: confirm `T062`–`T065` landed — `MetadataOnlyReason::SensitiveContent`,
  `SensitivePath`, `LfsPointer`, `PlatformPathCollision`, `UnsupportedPathEncoding`,
  `PathMetadataTooLarge`, and `UnsupportedTextEncoding` each map to a **distinct** `SkipReason` with
  honest `Display` text; `Unreadable`/`UnstableDuringRead`/`AbortedCircuitBreaker`
  (`src/live_index/store.rs:3376-3380`) and the missing-ingest-plan arm (`:3673`) no longer report
  `UnsupportedLanguage`; and the reverse mapping (`store.rs:3394-3405`, which today collapses
  `UnsupportedLanguage | SizeCeiling | None` → `UnsupportedTextEncoding`) round-trips totally.
  Verify by `cargo test --lib store -- --test-threads=1` and by reading the match arms — `cargo check`
  exhaustiveness is the mechanical proof that no arm was missed.
- [ ] **T120** GATE: confirm `T066` landed **with the amended premise from T101**, and that every path
  in **T102**'s recorded list is back at Tier 1. Prove per file with
  `search_symbols` / `search_text` / `get_symbol` returning a real body — `stable_read_with_retries`
  (`src/live_index/store.rs:445`) is the canonical probe: it is unfindable today purely because its
  file is demoted. **Fails without the fix**: `get_file_context` on
  `src/knowledge/mod.rs` currently returns `Tier 2 (metadata only) — reason: unsupported language,
  size 21 KB`.
- [ ] **T121** GATE: confirm `T070`'s recorded decision honors the frozen invariant **verbatim** — *a
  bounded lexical fallback must NEVER read files excluded for `SensitivePath` or `SensitiveContent`;
  Feature 020's security contract governs.* Record the honest consequence: files that were never
  sensitive stop being classified sensitive, so they become readable by **removing a false positive**,
  not by relaxing the invariant. Files the corrected detector still flags stay metadata-only with
  bytes and hashes discarded. Also confirm `T066` did not trade the over-broad rule for a false
  **negative**: the existing detector unit tests stay green, and the canary fixtures
  (`password={canary}`, `token={canary}`) are recognized as **placeholders** — not merely
  "no longer matched".

**Checkpoint**: exclusion reasons are observable. The 19 withheld files are back. Downstream truth is
now testable.

---

## Phase 5: US2 — `ACH-02` Tier-2 selection honored or refused (Priority: P1)

**Goal**: `around_match`, `around_symbol`, and `chunk_index` on a metadata-only file are honored or
explicitly refused. Nothing reaches the whole-file return while an unserviced selector is present.
`FR-005`–`FR-008`.

**Independent test**: a fixture that is Tier 2 **by deliberate policy** (lockfile / oversized data) —
not one from `T102`'s accidentally-demoted set, which `T066` has now restored and which would make the
test vacuous. Unique literal near EOF.

**Deliberately routed through `src/protocol/format.rs` so no `tools.rs` edit is needed** (PR #479).

### Investigation

- [ ] **T122** [INVESTIGATION] Enumerate **every** caller of `render_file_content_bytes`
  (`src/protocol/format.rs:3176`) before touching its signature. Two hot callers are known —
  `src/protocol/tools.rs:8607` and `src/protocol/format.rs:3099` — but `file_content_view`
  (`format.rs:3110`) and the resources render path are suspected additional callers
  ([research.md](research.md) §7 `D4`). **Adding branches without changing the signature is the safer,
  lazier option**; record which route the caller set permits.

### RED

- [ ] **T123** [US2] Add `tests/file_content_tier2_selection.rs` with a failing test: a fixture that is
  Tier 2 by deliberate policy, holding a unique literal near EOF. `around_match` for that literal must
  return a bounded window containing it **at its real line number**. **Record red** — today
  `render_file_content_bytes` has no `around_match` branch, so execution falls to
  `(None, None) => content.into_owned()` at `src/protocol/format.rs:3212` and returns the file from
  line 1 until `cap_file_content_output` (`:3594`) truncates at `GET_FILE_CONTENT_MAX_BYTES = 60_000`
  (`:3569`).
- [ ] **T124** [P] [US2] Add a failing test: an **absent** literal returns the explicit no-match result,
  not line-1 content. The refusal already exists (`not_found_file_match`, `src/protocol/format.rs:3508`;
  occurrence-not-found at `:3518-3522`) — it is merely unreachable from Tier 2.
- [ ] **T125** [P] [US2] Add a failing test: `chunk_index` on the Tier-2 fixture is honored or
  explicitly refused, and `around_line` / `around_match` / chunk modes can never substitute for one
  another (ledger `SF-DOG-006` acceptance check).
- [ ] **T126** [P] [US2] Add a failing test for `around_symbol` per **T105**'s recorded ruling. It must
  never return a silent full-file read. The ledger measured the cost of the current behavior:
  seven `around_symbol` calls produced ~62.6K tokens of duplicate output (ledger lines 790–798).

### GREEN

- [ ] **T127** [US2] Split the `&IndexedFile` coupling on `render_numbered_around_match_excerpt`
  (`src/protocol/format.rs:3497-3526`): extract a core taking
  `(path: &str, lines: &[&str], around_match, match_occurrence, context_lines)` so the raw-byte path
  can reuse it, keeping the `IndexedFile` wrapper for the existing caller at `:3089`.
- [ ] **T128** [US2] Same split for `render_numbered_chunk_excerpt` (`src/protocol/format.rs:3305`).
- [ ] **T129** [US2] Add the missing selector dispatch to `render_file_content_bytes`
  (`src/protocol/format.rs:3176-3225`) **before** the line-range fall-through. Mirror the dispatch
  order of the indexed reference implementation `file_content_from_indexed_file_with_context`
  (`:3061-3100`: chunk `:3065` → `around_symbol` `:3078` → `around_match` `:3088` → bytes `:3099`) so
  the two branches cannot drift again. **Nothing may reach `(None, None) => content.into_owned()` at
  `:3212` while an unserviced selector is present in the `ContentContext`.** Honor `around_match` and
  `chunk_index` against the raw `lines`; handle `around_symbol` per **T105**.

### VERIFY

- [ ] **T130** [US2] Run `cargo test --test file_content_tier2_selection -- --test-threads=1` and
  `cargo test --lib protocol::format -- --test-threads=1`. **Fails without the fix**: `T123` asserts
  the returned window's first line number is **near the literal**, not `1`; the pre-fix build returns
  line 1. `T124` asserts the no-match text is present; the pre-fix build returns content instead.
- [ ] **T131** [US2] Confirm the mode annotation stops lying with **no edit** to
  `src/protocol/tools.rs` — it is built at `:8532-8538` and prepended at `:8618-8621`, and becomes
  truthful once the renderer honors or refuses. Assert it by reading the annotation and the returned
  first line number in one test (`SC-005`). **If a `tools.rs` edit turns out to be genuinely
  required, FLAG it and sequence it after PR #479 merges — do not land it on a blocked file.** Then
  `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.

**Checkpoint**: the prescribed raw fallback works, or says why it cannot. No silent substitution.

---

## Phase 6: US3 — `ACH-03` one admission oracle, typed races (Priority: P1)

**Goal**: context and impact agree in one generation; a lost publication race is reported as a race
with a retry instruction, not as an admission verdict. `FR-009`–`FR-014`.

**Independent test**: index a fixture, get exact parsed context, mutate it, immediately run impact
against the same session/generation; then repeat under an active watcher event.

**Depends on**: Phase 4 (honest reason codes — otherwise `reason: None` and a real policy exclusion are
indistinguishable) and Phase 2 (the size renderer).

### Investigation

- [ ] **T132** [INVESTIGATION] Pin the unknowns ([research.md](research.md) §7 `Q2`) before fixing:
  (a) which branch of `capture_admission_tier_lookup_view` (`src/live_index/health_view.rs:275-309`)
  produced `Tier 1 — reason: policy` — the manifest branch (`:280-296`) or the `files` branch
  (`:297-306`)? Best reading is the `files` branch, since `Normal` + `None` is exactly what it
  returns and the manifest branch could only produce that string for a `MetadataOnly`/`HardSkip`
  entry, which would render Tier 2/3 instead. (b) which of the ≥8 `ReindexResult::Skipped` sites
  fired. Reproduce under an active watcher event with tracing enabled on the `trace!` at
  `src/watcher/mod.rs:572` and the `warn!` at `:576-578`. **Record the answer** — making `files`
  authoritative (`FR-011`) and adding a typed race variant (`FR-009`) are different fixes and guessing
  ships only one.

### RED

- [ ] **T133** [US3] Add `tests/impact_admission_consistency.rs` with a failing test:
  `view.reason == None` must **never** render as the string `"policy"`
  (`src/sidecar/handlers.rs:901-904`), and a `tier: Normal` view must **never** render under
  "Not indexed" (`:918-923`). **Record red** — the reported ledger string
  `Not indexed: … is Tier 1 — reason: policy, size 0.0 MB` is the byte-for-byte rendering of a
  successfully indexed file.
- [ ] **T134** [P] [US3] Add a failing test: for one path in one generation, `get_file_context` and
  `analyze_file_impact` report the same tier, admission reason, byte size, project, and generation
  (`SC-006`, ledger `SF-DOG-009` acceptance check). **Record red** — today the manifest-first ordering
  lets impact answer from a stale terminal disposition while context answers from the parsed `files`
  record.
- [ ] **T135** [P] [US3] Add a failing test: a publication-generation race produces a **typed
  stale-generation** result with a retry instruction, not an admission refusal.
- [ ] **T136** [P] [US3] Add a failing test: **both** impact branches carry the index generation.
  **Record red** — `src/sidecar/handlers.rs:926` computes `current_project_generation()` but only the
  non-parser branch at `:935` prints it; the `has_code_parser` branch (the one that fired) drops it.

### GREEN

- [ ] **T137** [US3] Add `ReindexResult::StaleGeneration { expected, observed }` and return it from the
  four non-admission sites in `read_and_index_with_stable_read`: `src/watcher/mod.rs:352-359` (stale
  metadata-terminal admission rejected), `:539` (stale hash-skip publication), `:566-573` (stale
  indexed-file publication rejected), `:576-579` (abort after `MAX_PUBLICATION_ATTEMPTS` = 4). Leave
  the genuine admission and scope/eviction returns (`:303-314`) as `Skipped`.
- [ ] **T138** [US3] Handle the new variant separately at **both** impact entry points —
  `src/sidecar/handlers.rs:958` (new-file) and `:1113` (edit) — with an explicit retry instruction.
  Neither may route it into `impact_skipped_text`, which is written for admission refusals.
- [ ] **T139** [US3] Make the live `files` record authoritative in
  `capture_admission_tier_lookup_view` (`src/live_index/health_view.rs:275-309`): check `files`
  (`:297-306`) before `manifest_entries` (`:280-296`). Where both are present and **disagree**, return
  both and flag the disagreement rather than silently preferring either ([plan.md](plan.md) §D5) — a
  disagreement is diagnostic information, and hiding it is how this defect stayed invisible.
- [ ] **T140** [US3] Fix the rendering in `impact_skipped_text` (`src/sidecar/handlers.rs:886-939`):
  drop the `unwrap_or_else(|| "policy")` default at `:901-904` (a `None` reason on a `Normal` view is
  an internal inconsistency, and must be reported as one, not formatted as a policy claim); stop
  rendering a `Normal`/Tier-1 view under the "Not indexed" template at `:918-923`; and print the
  generation on **both** branches.

### VERIFY

- [ ] **T141** [US3] Run
  `cargo test --test impact_admission_consistency -- --test-threads=1`,
  `cargo test --lib health_view -- --test-threads=1`, and
  `cargo test --lib watcher -- --test-threads=1`. **Fails without the fix**: `T133` asserts the
  literal substring `reason: policy` is absent for an indexed file, and `T134` asserts tier equality
  across the two tools — both of which the pre-fix build violates. **Constitution VII ATTENTION**:
  assert **transport parity explicitly** — the stdio path must report the same reason, size, and
  generation as the `serve` path in `handlers.rs`. Do not assume shared formatters cover it.
- [ ] **T142** [US3] Repeat the reproduction **under an active watcher event** (ledger `SF-DOG-009`
  acceptance check: "Repeat under an active watcher event to cover the production race") and confirm
  the stale-generation response succeeds after one refresh. Then `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings`.

**Checkpoint**: one oracle, one truth per generation. A race is recoverable and says so.

---

## Phase 7: US4 — `ACH-04` untracked and excluded truth (Priority: P1) — GATED ON PR #479

**Goal**: eligible untracked source is searchable after one full index, or the exclusion is named
identically by the receipt and the search, and `new_file=true`'s promise matches its implementation.
`FR-015`–`FR-018`.

**Independent test**: untracked `.ts`, `.js`, `.rs`, and `.py` fixtures each holding a unique
identifier; one full index; exact search must find every identifier with no separate file mutation.

**Depends on**: PR #479 landing (fix sites are almost entirely in `src/protocol/tools.rs`), Phase 4
(the mislabel **is** the reason collapse), Phase 6 (the oracle and the race variant), and `T143`.

### Investigation and gate

- [ ] **T143** [INVESTIGATION] Determine which `MetadataOnlyReason` actually demoted the ledger's
  `generated-auth-global-setup.ts` ([research.md](research.md) §7 `Q1`). **Undetermined** — testpilot
  was not indexed. `sensitive_path_rule` (`src/knowledge/mod.rs:250-279`) is narrow and would not match
  that filename, so the candidates are `SensitiveContent` (a generated auth-setup file plausibly
  embeds a bearer token → `secret.authorization-header`, `src/knowledge/mod.rs:76-78`) or
  `UnsupportedTextEncoding`. **Record the answer** — `SC-009` requires the receipt and the search
  response to name the *same* reason, which needs the real reason known.
- [ ] **T144** GATE: confirm PR #479 has merged to `main` and this branch has rebased/merged it, so
  `src/protocol/tools.rs` is editable. **Do not work around the footprint.** Until this gate passes,
  Phase 7 does not start.

### RED

- [ ] **T145** [US4] Add `tests/untracked_admission_truth.rs` with a failing test pinning the
  `new_file=true` contract. **Record red** — `src/sidecar/handlers.rs:800` promises "Reads file from
  disk, parses it, indexes it" while `:892-894` and `:921-922` say "no force-admit", and
  `src/watcher/mod.rs:277-284` shows `admit_and_index_single_path` is literally
  `read_and_index(relative_path, abs_path, shared, None, expected_gen)` — **no override parameter
  exists on the seam**. Assert the two claims cannot both hold.
- [ ] **T146** [P] [US4] Add a failing test: a deliberately Tier-2 metadata-only path must **not** be
  listed as a recoverable untracked candidate. **Record red** — `src/protocol/tools.rs:2184` retains
  paths on `guard.get_file(path).is_none()` alone, which is true for permanently-excluded files that
  live in the manifest rather than in `files`.
- [ ] **T147** [P] [US4] Add a failing test: `untracked_file_diagnostic`
  (`src/protocol/tools.rs:2191-2198`) must not recommend `analyze_file_impact(new_file=true)` for a
  path the admission gate will refuse. **Record red** — it mints that recommendation unconditionally,
  with no admission consultation of any kind.
- [ ] **T148** [P] [US4] Add a failing test: a search whose only matches live in unindexed files
  returns a **qualified** zero naming the excluded count and reason class. **Record red** — search
  already **proves** the match (`untracked_text_matches`, `src/protocol/tools.rs:2336-2365`, runs the
  real regex/term query against the file's bytes) and then discards it:
  `matching_untracked_paths_for_search_text` (`:2410-2421`) keeps only `Vec<String>`, `:3316-3323`
  computes it only when `result.files.is_empty()`, and `:3351` appends it as prose. It never enters
  `result.files`.

### GREEN

- [ ] **T149** [US4] Resolve the `new_file` contract (`FR-017`). Either give
  `admit_and_index_single_path` (`src/watcher/mod.rs:277-284`) a real admission-override parameter, or
  rewrite the description at `src/sidecar/handlers.rs:800` **and** the `analyze_file_impact` tool
  schema to state that it re-runs the gate and cannot admit an excluded file. **The current
  state — promising indexing at `:800` and disclaiming force-admit at `:892-894` — is not an option.**
  Record which was chosen and why. Note that untracked files are **not** excluded by default:
  `SYMFORGE_EXCLUDE_UNTRACKED` is opt-in (`src/discovery/mod.rs:2192-2197`) and
  `SkipReason::Untracked` is documented as never minted on the default path
  (`src/domain/index.rs:1392-1396`), so an untracked file that was refused was refused for some
  *other* reason — which Phase 4 now makes visible.
- [ ] **T150** [US4] Make the untracked-candidate test manifest-aware at
  `src/protocol/tools.rs:2184`: test admission (via the now-authoritative oracle from `T139`), not
  just `files` membership.
- [ ] **T151** [US4] Make `untracked_file_diagnostic` (`src/protocol/tools.rs:2191-2198`)
  admission-aware: consult the gate first; where it would refuse, name the actual exclusion reason
  instead of advertising a recovery that cannot work.
- [ ] **T152** [US4] Carry the **proven** match locations into the result: change
  `untracked_text_matches` (`src/protocol/tools.rs:2336-2365`) and
  `matching_untracked_paths_for_search_text` (`:2410-2421`) to return locations rather than a `bool` /
  `Vec<String>`, and merge them into `result.files` marked unindexed at `:3316-3323`/`:3351` — or, if
  merging changes the result contract more than is warranted, make the zero **explicitly qualified**.
  Record which. **Honor the frozen invariant**: no `SensitivePath`/`SensitiveContent`-excluded file may
  be read for this purpose.

### VERIFY

- [ ] **T153** [US4] Run `cargo test --test untracked_admission_truth -- --test-threads=1` with the
  four-language fixture set (`.ts`, `.js`, `.rs`, `.py`), one full index, no separate file mutation.
  **Fails without the fix**: the test asserts each unique identifier is found; the pre-fix build
  returns an unqualified zero plus a prose notice.
- [ ] **T154** [US4] Confirm the exclusion reason named by the full-index receipt and by the search
  response are the **same string** for any fixture policy genuinely excludes (`SC-009`, ledger
  `SF-DOG-008` acceptance check), and that `analyze_file_impact(new_file=true)` on an eligible fixture
  makes the next exact search succeed — or that its description/schema now says it cannot. Then
  `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.

**Checkpoint**: new code is findable, or the refusal is honest and the recovery advertised actually
works.

---

## Phase 8: US5 — `ACH-05` index identity (Priority: P1)

**Goal**: a response describing an index carries the identity of the index that answered it, and a
sidecar bound to another project cannot serve a caller foreign counts. `FR-019`–`FR-025`.

**Independent test**: the two-curl reproduction from [research.md](research.md) §6, plus a descriptor
census.

**Depends on**: nothing here. A disjoint file set (`src/sidecar/*`, `src/cli/hook.rs`) that no other
phase touches. Placed last for review size only.

### Investigation

- [ ] **T155** [INVESTIGATION] Determine whether the two unregistered local sidecars observed (port
  63032 `Empty`; port 64700 rooted at `E:\project\aap-rooms-019` serving `2265`/`76582`) are **orphaned
  from crashed sessions** or **current sessions that failed to register with the daemon**
  ([research.md](research.md) §7 `Q3`). **Record the answer** — it decides whether descriptor hygiene
  alone suffices (`FR-025`) or registration itself is unreliable, which is a materially larger fix.
  Also confirm whether `write_session_descriptor` is ever called with `project_root: None` in
  practice: all 22 descriptors on disk declare a root, so the permissive check at
  `src/sidecar/port_file.rs:283-289` may be a latent hole rather than the operative one — which
  affects `T158`'s priority, not its correctness.

### RED

- [ ] **T156** [US5] Add `tests/sidecar_identity_guard.rs` with a failing test reproducing the measured
  defect: a sidecar rooted at project A, a caller from project B sending its real `caller_root`.
  `/outline` correctly 409s; `/health` returns `200` with A's `file_count`/`symbol_count`. **Record
  red** — `src/sidecar/handlers.rs:344-346` exempts exactly `/health` and `/stats` from
  `caller_root_guard`, and `src/cli/hook.rs:865-868` (non-symbol Grep) and `:885` (unknown tool) make
  `/health` the hook's busiest endpoint.
- [ ] **T157** [P] [US5] Add a failing test: `HealthResponse` (`src/sidecar/handlers.rs:88-93`) carries
  an identity stamp. **Record red** — it holds only `{ file_count, symbol_count, index_state,
  uptime_secs }`; confirmed against the raw JSON in [research.md](research.md) §6 that there is
  **nothing** for a caller to check.
- [ ] **T158** [P] [US5] Add a failing test: a descriptor with `project_root: None` is **rejected**,
  not accepted by every project (`src/sidecar/port_file.rs:283-289`, with the `Option<String>` field at
  `:161` and `write_session_descriptor`'s `Option<&Path>` at `:206`).
- [ ] **T159** [P] [US5] Add a failing test: the legacy fixed-port fallback
  (`src/sidecar/port_file.rs:495-505`, `read_port_at(&dir)?` at `:501`) applies the same root
  validation as the descriptor path.
- [ ] **T160** [P] [US5] Add a failing test: a **dead** port is never returned as a resolved endpoint
  (`src/sidecar/port_file.rs:496` accepts `selected.status.port` with no liveness probe). Baseline for
  the assertion: 22 descriptors on disk, 7 distinct ports, **all dead**.
- [ ] **T161** [P] [US5] Add a failing test: a `\\?\`-prefixed root (`//?/E:/project/symforge`, what
  the daemon emits) and the plain form (`E:\project\symforge`, what descriptors store) compare **equal**
  in all three sites — `same_root_identity` (`src/sidecar/port_file.rs:337-348`, no canonicalization),
  `roots_match` (`src/sidecar/handlers.rs:375-388`, `dunce` strips the prefix), and
  `normalize_path_for_match` (`src/cli/hook.rs:1083-1091`, fed `std::fs::canonicalize`, which adds it).

### GREEN

- [ ] **T162** [US5] Gate the **index-describing fields** (`file_count`, `symbol_count`,
  `index_state`) on the `caller_root` check at `src/sidecar/handlers.rs:344-346`, keeping liveness
  answerable without identity — or return the mismatch in band. **Do not simply remove the
  exemption**: its comment at `:342-343` records a real requirement ("liveness probes and the hook's
  fail-open target must never 409"), and removing it would break the hook's fail-open path
  ([plan.md](plan.md) §D8).
- [ ] **T163** [US5] Add the identity stamp to `HealthResponse` (`src/sidecar/handlers.rs:88-93`) —
  project root, project id, and index generation — and make the hook and the MCP `status` surface print
  the **same** stamp, so a mismatch is visible without a curl (`FR-020`, `SC-013`).
- [ ] **T164** [US5] Collapse the three root comparators into **one shared comparator** used by all
  three call sites (`src/sidecar/port_file.rs:337-348`, `src/sidecar/handlers.rs:375-388`,
  `src/cli/hook.rs:1083-1091`). **Constitution VI**: place it where it does not pull a server
  dependency into the `embed` build — gated by `T171`.
- [ ] **T165** [US5] Require a declared root in descriptor selection (`src/sidecar/port_file.rs:283-289`,
  `:161`, `:206`) — absent identity is not universal identity. Apply the same root validation to the
  legacy fallback and add a liveness probe before returning a port (`:495-505`, `:496`, `:501`).
- [ ] **T166** [US5] Fix the hook's own identity: resolve the git/project root and **fail loudly**
  instead of `std::env::current_dir().unwrap_or_default()` at `src/cli/hook.rs:255`, so a worktree or a
  subdirectory cannot silently resolve to the wrong project; validate the selected daemon session's
  **live indexed root** (not only the registered `canonical_root`) at `:1041-1058`, so an
  `index_folder` retarget is visible; and add read-path staleness pruning by reusing
  `cleanup_stale_descriptors_at` (`src/sidecar/port_file.rs:234-241`, today only on the update/repair
  path), plus either migrate or explicitly stop honoring the legacy `<control_state>/sessions`
  location the current reader (`:23`, `:62-64`) no longer inspects.

### VERIFY

- [ ] **T167** [US5] Run `cargo test --test sidecar_identity_guard -- --test-threads=1` and
  `cargo test --lib sidecar -- --test-threads=1`, then re-run the **live two-curl reproduction**.
  **Fails without the fix**: `T156` asserts `/health` with a mismatched `caller_root` does **not**
  return the foreign counts — the pre-fix build returns
  `200 {"file_count":2265,"symbol_count":76582,…}` from `E:\project\aap-rooms-019` in the same instant
  `/outline` 409s. Re-run the descriptor census and confirm zero dead-port descriptors are returned as
  resolved endpoints (`SC-014`). Then `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings`.

**Checkpoint**: a session can prove which index answered it.

---

## Phase 9: Polish, cross-cutting, and close-out

- [ ] **T168** Re-run the `SF-DOG-001` reproductions now that the demotion is fixed
  ([research.md](research.md) §7 `D7`). `stable_read_with_retries` (`src/live_index/store.rs:445`) was
  unfindable via `search_symbols`/`search_text` **purely** because its file was demoted, so some
  "search silently misses" reports may collapse into that root cause rather than being separate
  defects. Record which reproductions closed as duplicates and which remain genuinely owned by
  `SIFT-WS5B` (`T068`–`T071`).
- [ ] **T169** Update `CLAUDE.md` **if and only if** this feature falsified a claim in it
  (documentation-hygiene rule). Candidate claims to check: the architecture section's tool-surface
  description, and anything asserting current admission behavior. Do not add a new doc file.
- [ ] **T170** Full verification gate (Constitution VIII): `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`,
  `cargo build --release`. Check `df -h /e | tail -1` **before** starting — abort below 6 GB.
- [ ] **T171** Embed isolation (Constitution VI, ATTENTION): `cargo check --no-default-features
  --features embed`. This is a **hard gate** for `T164`'s shared comparator, which lives adjacent to
  server-side modules.
- [ ] **T172** npm tests: `cd npm && npm test`.
- [ ] **T173** Disk hygiene: `cargo clean`, then `df -h /e | tail -1` and record the reclaimed space
  (repo `CLAUDE.md` Windows disk rule).
- [ ] **T174** **Exit-criterion ledger.** Record, per finding, the receipt that closes it: the test
  name, the command that runs it, and the assertion that would fail without the fix. All nine
  `SF-DOG` findings plus the adopted index-identity defect must have a receipt. Any finding without
  one is **not closed** — say so rather than claiming completion.

---

## Dependencies

```text
Phase 1 (setup + investigations + owner rulings: T100-T106)
  │
  ├─> Phase 2 (shared size renderer: T107-T109) ──────────────┐
  │                                                            │
  ├─> Phase 3  US1 / ACH-01  SF-DOG-007  (T110-T118)           │   INDEPENDENT — the MVP.
  │            no upstream dependency at all; schedulable      │   Only wrong-write defect.
  │            immediately, even beside Phase 1.               │
  │                                                            │
  └─> Phase 4  PREREQUISITE GATE  SIFT-WS5 T062-T082 (T119-T121)
                 │   honest reason codes ──> root cause observable ──> downstream truth testable
                 │
                 ├─> Phase 5  US2 / ACH-02  SF-DOG-006  (T122-T131)
                 │            independent of the gate LOGICALLY, but placed after it so the
                 │            Tier-2 fixture cannot be one T066 restores (test validity).
                 │
                 └─> Phase 6  US3 / ACH-03  SF-DOG-009  (T132-T142)   [needs Phase 2]
                              │
                              └─> Phase 7  US4 / ACH-04  SF-DOG-008  (T143-T154)
                                           ALSO HARD-GATED on PR #479 merging (T144)

Phase 8  US5 / ACH-05  index identity  (T155-T167)
         disjoint file set; independent of every phase above. Last for review size only.

Phase 9  Polish + close-out  (T168-T174)   after everything that shipped.
```

### What unblocks what, and why

1. **Honest reason codes unblock root-causing the demotion, which unblocks downstream truth.**
   `SkipReason::UnsupportedLanguage` is a ≥11-way catch-all
   (`src/live_index/store.rs:3360-3366`, `:3376-3380`, `:3673`), so today it is **impossible** to tell
   why a given file was excluded. That is why the demotion went un-root-caused for a full day, and why
   `SF-DOG-004` (LOW) is scheduled ahead of two HIGHs. Phase 4 must precede Phase 6 (`ACH-03` cannot
   distinguish a real policy exclusion from `reason: None`) and Phase 7 (`ACH-04`'s mislabel **is** the
   collapse; and `SC-009` needs the receipt and the search to name the same reason).
2. **`ACH-03` unblocks `ACH-04`.** `ACH-04`'s honest `new_file=true` refusal must distinguish "the gate
   refused this" from "you lost a publication race" — which only exists once `FR-009`'s typed variant
   lands (`T137`). `ACH-04`'s admission-aware diagnostic (`T150`/`T151`) also reads the single
   authoritative oracle that `T139` establishes.
3. **The size renderer unblocks four VERIFY tasks.** Landed once in Phase 2 rather than re-litigated in
   `SF-DOG-002`, `004`, `008`, and `009`.
4. **`ACH-01` is unblocked by nothing.** Stated explicitly because it is the only wrong-write defect and
   the temptation is to sequence it behind the "root cause" work. It has no relationship to admission
   at all: the predicate exists at `src/protocol/edit_plan.rs:90` and the Tier-2 lookup exists at
   `src/live_index/query.rs:1243-1256`.
5. **PR #479 blocks `ACH-04` only.** `ACH-02` was deliberately routed through
   `src/protocol/format.rs` so that nothing else in this feature waits on a merge.

## Parallel opportunities (within a phase only)

- **Phase 1**: `T102`, `T103` are independent measurements; `T104`, `T105` are independent owner
  questions that can be asked together.
- **Phase 3 RED**: `T111`, `T112` are independent test functions alongside `T110`.
- **Phase 5 RED**: `T124`, `T125`, `T126` are independent test additions after `T123` establishes the
  fixture.
- **Phase 6 RED**: `T134`, `T135`, `T136` are independent alongside `T133`.
- **Phase 7 RED**: `T146`, `T147`, `T148` are independent alongside `T145`.
- **Phase 8 RED**: `T157`–`T161` are independent alongside `T156`.
- **Across phases**: Phase 3 (`ACH-01`) and Phase 8 (`ACH-05`) touch entirely disjoint file sets from
  each other and from Phases 5–7, so they may be implemented by separate agents in parallel with
  strict file ownership (`ACH-01` = `edit_plan.rs`; `ACH-05` = `sidecar/*` + `cli/hook.rs`).

**GREEN tasks within a phase are not parallelizable** — they mutate the same functions in sequence.

## Implementation strategy

**MVP = Phase 3 (`ACH-01` / `SF-DOG-007`).** It alone removes the only defect in the ledger that
causes a **wrong write**. Everything else in this feature converts a false negative into a declared
limitation — real and valuable, but an agent that stops early is cheaper than an agent that edits the
wrong file confidently.

**Incremental delivery**: each phase ends at a checkpoint where the focused test command is green, so
the branch is landable at any checkpoint. Phase 2 + Phase 3 together are a complete, shippable slice.

**Stop conditions**:

- If an investigation task (`T103`, `T132`, `T143`, `T155`) cannot reach a recorded answer, **STOP and
  report**. Do not build a fix on a guessed cause — a plan that names an unknown is better than one
  that pretends.
- If an owner ruling (`T104`, `T105`) is unavailable, implement the recorded recommendation and mark it
  provisional in the task receipt. Do not silently pick the other option.
- If closing a finding would require reading a file excluded for `SensitivePath` or
  `SensitiveContent`, the finding yields — **the frozen security invariant does not**.
- If `ACH-04` remains blocked by PR #479 at close-out, report it as **not closed** and say which
  acceptance checks are outstanding. Do not restate a blocked finding as done.

## Task count summary

| Phase | Story / WS | Finding(s) | Tasks | IDs |
|---|---|---|---:|---|
| 1 Setup + investigation | — | cross-cutting | 7 | T100–T106 |
| 2 Foundational | — shared | `002`/`004`/`008`/`009` size | 3 | T107–T109 |
| 3 | US1 / `ACH-01` | `SF-DOG-007` | 9 | T110–T118 |
| 4 Prerequisite gate | US6 / `SIFT-WS5` | `SF-DOG-001`…`005` | 3 | T119–T121 |
| 5 | US2 / `ACH-02` | `SF-DOG-006` | 10 | T122–T131 |
| 6 | US3 / `ACH-03` | `SF-DOG-009` | 11 | T132–T142 |
| 7 | US4 / `ACH-04` | `SF-DOG-008` | 12 | T143–T154 |
| 8 | US5 / `ACH-05` | index identity | 13 | T155–T167 |
| 9 Polish | — | close-out | 7 | T168–T174 |
| **Total (native to 021)** | | | **75** | T100–T174 |
| *Inherited, not counted* | `SIFT-WS5` | `001`…`005` | *21* | `T062`–`T082` |

## Exit criteria

**All nine ledger findings closed with a receipt, plus the index-identity defect 021 adopted.**

| Finding | Closed by | Receipt |
|---|---|---|
| `SF-DOG-001` | `SIFT-WS5B` (`T068`–`T071`) + `T168` duplicate triage | qualified-negative test; `T168`'s record of which reproductions collapsed into the demotion root cause |
| `SF-DOG-002` | `SIFT-WS5C` (`T072`–`T074`) + `T107`–`T109` (size) | `edit_plan` typed unavailability test; non-zero size assertion |
| `SF-DOG-003` | `SIFT-WS5D` (`T075`–`T077`) | omitted / `true` / `false` `code_only` contract tests |
| `SF-DOG-004` | `SIFT-WS5A` (`T062`–`T066`), amended by `T101`; verified `T119`/`T120`; blast radius `T102`; residual `T103` | distinct-`SkipReason` tests; `T102`'s list back at Tier 1 |
| `SF-DOG-005` | `SIFT-WS5E` (`T078`–`T080`) | compact/full health same-denominator test |
| `SF-DOG-006` | `ACH-02` (`T122`–`T131`) | `tests/file_content_tier2_selection.rs`; first-line-number and no-match assertions |
| `SF-DOG-007` | `ACH-01` (`T110`–`T118`) | `tests/edit_plan_literal_path_precedence.rs`; `Constant ts` absent; `Type.Method` unchanged |
| `SF-DOG-008` | `ACH-04` (`T143`–`T154`) — **gated on PR #479** | `tests/untracked_admission_truth.rs`; four-language fixtures found after one index; same reason string in receipt and search |
| `SF-DOG-009` | `ACH-03` (`T132`–`T142`) + `T107`–`T109` | `tests/impact_admission_consistency.rs`; `reason: policy` unreachable; tier equality across tools; watcher-event repeat |
| index identity | `ACH-05` (`T155`–`T167`) | `tests/sidecar_identity_guard.rs`; two-curl reproduction inverted; descriptor census clean |

Plus: the full gate green (`T170`–`T172`), disk reclaimed (`T173`), and `T174`'s ledger complete.
**A finding without a receipt is not closed** — report it as outstanding.
