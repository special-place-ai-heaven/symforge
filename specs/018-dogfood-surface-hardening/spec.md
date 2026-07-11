# Feature Specification: Dogfood Surface Hardening

**Feature Branch**: `018-dogfood-surface-hardening`

**Created**: 2026-07-09

**Status**: Draft

**Input**: User description: four independently-verified defects from the Grok exhaustive dogfood report (`docs/grok_report.md`) that make the 36-tool code-intelligence surface noisy or leaky on real repositories. Each was confirmed against current source (file:line verified). Finding #2 (multi-project retrieval completeness) is explicitly out of scope — tracked as a larger Feature 012 continuation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Trustworthy change/impact on repos that contain data files (Priority: P1)

An agent asks "what changed?" or "what does this change affect?" on a real repository that also contains non-source data files (JSON tool definitions, fixtures, generated payloads). Today `what_changed` (uncommitted mode) returns hundreds of `*.json` data paths as "changes", and `detect_impact` walks the symbol graph from those data-file keys and produces an enormous, useless "high risk" blast radius — the actual source edits are buried. The agent must already know to pass `code_only=true` to get a usable answer, which most callers do not.

**Why this priority**: This is the highest-impact, highest-frequency defect: it degrades the two most obvious "what changed / what breaks" tools on any repository that ships data files, and it silently trains agents to distrust the surface. It is also the clearest defaulting/admission fix — the machinery to exclude data files already exists (`code_only`), so the change is about the sensible default, not new capability.

**Independent Test**: On a repository with a dirty non-source file only under a data directory (e.g. `mcps/.../foo.json`), call `what_changed` (uncommitted) and `detect_impact` with no extra flags and confirm the result is source-focused (data-file paths and data-file-derived symbols do not dominate), while an explicit opt-in still surfaces them. Fully testable on its own.

**Acceptance Scenarios**:

1. **Given** a repo whose only uncommitted change is a non-source data file, **When** a caller runs `what_changed` in uncommitted mode with no extra flags, **Then** the result does not list that data file as a code change (the default is source-focused), and the disclosure still states the mode and any filtering applied.
2. **Given** a real source edit plus many dirty data files, **When** a caller runs `detect_impact` with default flags, **Then** the blast radius is computed from the source change and is not dominated by data-file-derived symbols (JSON keys do not appear as high-risk impacted symbols).
3. **Given** a caller who genuinely wants data files included, **When** they pass the explicit opt-in, **Then** the previous inclusive behavior is available (no capability is removed, only the default changes).
4. **Given** the change to defaults, **When** any existing `code_only=true` / explicit caller is run, **Then** its result is unchanged (the fix moves the default, it does not alter explicit requests).

---

### User Story 2 - Browse mode surfaces important symbols, not generic noise (Priority: P2)

An agent browses a scope to orient — it calls `search_symbols` with no query but a `kind` and/or `path_prefix` filter, expecting the notable symbols in that scope. Today it gets mostly generic short names (`add`, `get`, `len`, `new`, `fmt`) followed by "N more omitted", because an empty query matches every symbol and results are ordered by file path rather than importance.

**Why this priority**: Browse is a common orientation move, and returning trivial names first wastes the result budget and misleads the agent about what matters in a scope. It is a self-contained ranking fix with no dependency on US1.

**Independent Test**: Call `search_symbols` with an empty query plus a scope filter (`kind`/`path_prefix`) and confirm the top results are higher-signal symbols (by usage/importance) rather than generic short names ordered by path. Testable without the other stories.

**Acceptance Scenarios**:

1. **Given** an empty query with a `kind` and/or `path_prefix` filter (browse intent), **When** `search_symbols` runs, **Then** results are ranked by importance (e.g. reference count and/or kind priority) rather than path order, so notable symbols appear before generic short names.
2. **Given** a non-empty query, **When** `search_symbols` runs, **Then** its existing name-match behavior and ordering are unchanged (the fix applies only to the empty-query browse case).
3. **Given** identical repository state, **When** the same browse query runs twice, **Then** the ordering is identical (deterministic tie-breaks).

---

### User Story 3 - Repo map never surfaces paths outside the workspace root (Priority: P3)

A developer or agent requests the full repository outline (`get_repo_map` at full detail) and expects only files under the bound workspace root. Today the outline is built from every indexed file with no root-containment guard, so a path from outside the project root can appear in the map.

**Why this priority**: An information-hygiene / trust hardening fix — the outline should never present out-of-workspace paths as project content. Lower frequency than US1/US2 (and the specific reported example was induced by the reporter's own multi-project test), but the missing guard is a real defect worth closing.

**Independent Test**: Build an index whose file set includes an escaping/out-of-root path, request the full repo map, and confirm no absolute or root-escaping path appears in the outline. Testable without the other stories.

**Acceptance Scenarios**:

1. **Given** an index that contains a path not under the workspace root, **When** `get_repo_map` at full detail runs, **Then** the escaping path is excluded from (or quarantined out of) the outline, and only in-root files are presented.
2. **Given** a normal in-root repository, **When** the full repo map runs, **Then** every expected in-root file still appears (the guard rejects only genuine escapes, not legitimate files).

---

### User Story 4 - Truncated large responses always offer a retrieval handle (Priority: P4)

An agent issues a broad query (e.g. full repo map, wide symbol search) under a tight token budget. The response is truncated, but there is no `symforge_retrieve` footer with a hash, so the agent cannot fetch the full payload — it must re-issue a narrower query and guess. Today the retrieval footer is frequently absent because hard truncation runs before or instead of the content-compression decision.

**Why this priority**: This closes a dead-end in the truncation contract — the whole point of content compression is that a truncated answer remains recoverable. Valuable but lower frequency and independent of the other stories.

**Independent Test**: Issue a large-response query with a deliberately tight token budget and confirm the truncated output includes a usable `symforge_retrieve` handle that, when fetched, returns the full payload. Testable on its own.

**Acceptance Scenarios**:

1. **Given** a large response and a token budget that forces truncation, **When** a big-response tool truncates, **Then** the output includes a `symforge_retrieve` footer with a hash that fetches the complete payload.
2. **Given** a response that fits within budget, **When** the tool runs, **Then** no retrieval footer is added and the full content is returned inline (the footer appears only when truncation actually occurs).
3. **Given** a truncated response with a footer, **When** the caller invokes `symforge_retrieve` with the provided hash, **Then** the full pre-truncation payload is returned.

---

### Edge Cases

- **US1**: A directory that is mostly data but contains a few genuine source files — the source files MUST remain first-class (change/impact/search still see them); only the pure-data content is demoted/excluded. The mechanism MUST NOT reclassify a real source language as data.
- **US1**: `what_changed` in `since`/`git_ref` (committed) modes vs. uncommitted mode — the defaulting change targets the noisy uncommitted/untracked path; committed-diff modes MUST keep their existing meaning.
- **US2**: Empty query with NO scope filter (`kind`/`path_prefix` both absent) — MUST retain today's behavior (not silently reinterpreted as browse) to avoid changing an unscoped call's meaning.
- **US2**: A browse scope where reference counts are unavailable or all equal — MUST fall back to a deterministic secondary order (e.g. kind priority then path), never a random or unstable order.
- **US3**: A legitimately relative in-root path that merely contains unusual characters MUST NOT be falsely rejected; only paths that escape the root (absolute-outside-root, `..`-escaping) are filtered.
- **US4**: A response that truncates to essentially nothing under an extremely tight budget MUST still emit a valid retrieval handle rather than an empty body with no recovery path.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `what_changed` (uncommitted mode) and `detect_impact` MUST default to a source-focused result so that non-source data files (and symbols derived from them) do not dominate the default output, while preserving an explicit opt-in that restores full inclusion.
- **FR-002**: The symbol graph used by `detect_impact` MUST NOT let pure-data-file symbols (e.g. JSON keys) drive blast-radius by default; impact from a real source change MUST be computed over source symbols unless data inclusion is explicitly requested.
- **FR-003**: Any pre-existing explicit caller (e.g. `code_only=true`, or an explicit data-inclusion flag) MUST behave exactly as before — this feature changes defaults and admission classification, not the meaning of explicit requests.
- **FR-004**: `search_symbols` MUST detect browse intent (empty query with a `kind` and/or `path_prefix` scope filter) and rank results by importance (reference count and/or kind priority) rather than path order, so notable symbols surface ahead of generic short names.
- **FR-005**: `search_symbols` with a non-empty query MUST retain its current name-match filtering and ordering unchanged; the browse ranking MUST apply only to the empty-query-with-scope case.
- **FR-006**: `get_repo_map` at full detail MUST apply a workspace-root containment guard at outline collection time so that no path outside the bound project root appears in the outline; in-root files MUST be unaffected.
- **FR-007**: When a large-response tool truncates output under a token budget, it MUST run the content-compression decision on the complete payload before the final hard token cut and MUST emit a `symforge_retrieve` footer whose hash fetches the full pre-truncation payload.
- **FR-008**: A response that fits within budget MUST NOT gain a retrieval footer (footers appear only on actual truncation), and a `symforge_retrieve` hash produced on truncation MUST resolve to the complete payload.
- **FR-009**: All four behaviors MUST remain deterministic for identical repository state (Constitution IV), MUST preserve trust/ranked/truncated disclosure (Constitution III), and MUST stay frecency-neutral — none of these read paths may write frecency back as a side effect (Constitution V).
- **FR-010**: All four behaviors MUST be equivalent across the `stdio` and `serve` transports (Constitution VII) and MUST NOT break the `embed` build (Constitution VI).
- **FR-011**: Each fix MUST ship with a regression test that fails on the current (pre-fix) behavior and passes after the fix.

### Key Entities

- **Change/impact result**: the set of paths (`what_changed`) or impacted symbols (`detect_impact`) returned for the working tree. The feature changes which files/symbols are included by default (source-focused) without removing the inclusive opt-in.
- **Symbol admission tier**: the classification that decides whether a file's contents are extracted as first-class symbols or treated as metadata only. The feature ensures pure-data content does not enter the impact/graph surface as first-class symbols by default.
- **Browse result ordering**: the ranking applied when `search_symbols` is called with no query but a scope filter. The feature replaces path-order with importance-order for this case.
- **Repository outline**: the file list `get_repo_map` presents at full detail. The feature adds a root-containment guard to its collection.
- **Truncation retrieval handle**: the `symforge_retrieve` footer/hash offered when a large response is truncated. The feature guarantees it is present whenever truncation occurs.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a repo whose only uncommitted change is a non-source data file, default `what_changed` (uncommitted) reports zero data-file paths as code changes (currently: it lists them), and default `detect_impact` produces no data-file-derived high-risk symbols.
- **SC-002**: For a real source edit accompanied by many dirty data files, the default `detect_impact` blast radius contains only source-derived impacted symbols; the explicit data-inclusion path still reproduces the old inclusive result.
- **SC-003**: A browse call (`search_symbols`, empty query + scope filter) returns importance-ranked symbols such that no generic trivial name (`add`/`get`/`len`/`new`/`fmt`) occupies a top slot ahead of a materially more-referenced symbol in the same scope.
- **SC-004**: For an index containing an out-of-root path, `get_repo_map` full detail returns zero paths outside the workspace root; for a clean in-root repo, the outline file count is unchanged vs. current behavior.
- **SC-005**: For a large response forced to truncate under a tight token budget, the output contains a `symforge_retrieve` footer whose hash resolves to the full payload (currently: footer frequently absent).
- **SC-006**: The full verification gate (Constitution VIII) passes with the new tests: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, plus `cargo check --no-default-features --features embed`.
- **SC-007**: No regression to existing explicit callers: every current `code_only`/scoped/committed-mode call and every non-empty-query `search_symbols` call returns the same result as before the change.

## Assumptions

- The four user stories are code-disjoint and independently shippable; **US1 is the MVP** and lands/verifies first. US2–US4 follow in priority order and can each ship alone.
- "Source-focused by default" is interpreted as: the change/impact surface excludes non-programming-language / pure-data content by default, reusing the existing `code_only`/language-filter machinery rather than introducing a new classifier — unless a small admission-tier change is separately justified during planning.
- "Importance" for browse ranking reuses signals the index already holds (reference counts, kind weighting); no new persisted data or second index is introduced (Constitution I/VI).
- The workspace root available to `capture_repo_outline_view` is the canonical root already stored on the index at construction; the guard reuses it rather than deriving a new notion of root.
- The content-compression (CCR) store and `symforge_retrieve` mechanism already exist; US4 changes *when* the compression decision runs and *whether* the footer is emitted, not the retrieval backend.
- Finding #2 (multi-project retrieval completeness) is **out of scope** for 018 and is tracked as a separate Feature 012 continuation.
- Work stays on the review branch `018-dogfood-surface-hardening`; no push/merge without explicit user approval.
