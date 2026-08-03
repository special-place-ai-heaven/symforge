# Feature Specification: Admission Coverage Honesty

**Feature Branch**: `feat/knowledge-llm-sift` (drafted on) → implement on a dedicated branch<br>
**Created**: 2026-07-28<br>
**Status**: Draft — ready for `/speckit-plan` sign-off<br>
**Slice IDs**: `ACH-01` … `ACH-05` (plus inherited `SIFT-WS5`)

**Requirements source (read-only, another project)**:
`E:\project\testpilot\.scratch\symforge-dogfood-issues-2026-07-27.md` — nine findings
`SF-DOG-001`…`SF-DOG-009` with reproductions, expected behavior, and acceptance checks. That ledger
is the authority for *what* must change; this spec is the authority for *scope and ownership*.

**Inherited prerequisite (referenced by ID, NOT restated here)**:
`specs/020-repository-knowledge-index/sift/spec.md` **User Story 6 (`SIFT-WS5`)** and
`specs/020-repository-knowledge-index/sift/tasks.md` **Phase 9, tasks T062–T082**, which already own
`SF-DOG-001`…`SF-DOG-005` with real `file:line` targets and a correct ordering note.

**Precedent for turning a dogfood ledger into a feature**: `specs/018-dogfood-surface-hardening/`.

---

## Why this feature exists separately from 020

Feature 020 is the **repository-knowledge** lane. These nine defects are about **code-intelligence
admission truthfulness**: whether a file's symbols exist, whether a tool tells you when it did not
look, and whether the reason it gives is true. An operator asking "why is my file metadata-only?" or
"why did the planner point at the wrong file?" must not have to find the answer inside a
knowledge-index sub-slice.

**021 is therefore the umbrella that owns all nine.** It covers `SF-DOG-006`, `SF-DOG-007`,
`SF-DOG-008`, `SF-DOG-009`, and the index-identity defect (`SIFT-WS5` close-out `T081`'s symptom)
**natively, with its own tasks**. It treats `SIFT-WS5` (`T062`–`T082`) as a **prerequisite phase it
references by ID**, because restating twenty-one already-targeted tasks would create two ledgers that
drift.

**Exit criterion**: all nine ledger findings closed with receipts, plus the index-identity defect 021
adopted. See [tasks.md](tasks.md) §Exit criteria.

## The one thing that makes everything else observable

`SkipReason::UnsupportedLanguage` (`src/domain/index.rs:1404`, `Display` at `:1424`) is a catch-all
for at least eleven unrelated dispositions. Verified today:

- `src/live_index/store.rs:3360-3366` collapses seven `MetadataOnlyReason` variants
  (`SensitivePath`, `SensitiveContent`, `LfsPointer`, `PlatformPathCollision`,
  `UnsupportedPathEncoding`, `PathMetadataTooLarge`, `UnsupportedTextEncoding`) into it;
- `src/live_index/store.rs:3376-3380` collapses `Unreadable`, `UnstableDuringRead`, and
  `AbortedCircuitBreaker` into it;
- `src/live_index/store.rs:3673` hardcodes it on the missing-ingest-plan arm.

Every downstream surface then repeats the lie: `src/protocol/format.rs:3676-3679` renders
`"reason: unsupported language, size {n} KB"`, and `src/sidecar/handlers.rs:918-923` renders
`"reason: {reason}, size {size_mb:.1} MB"`.

So **`SF-DOG-004` (rated LOW) is the unblocker for `SF-DOG-001` and `SF-DOG-008`** — while every
cause reports as one string, no operator and no test can tell which cause fired. That is why the
demotion of nineteen ordinary Rust files in this repository went un-root-caused for a full day, and
why `SF-DOG-008`'s "unsupported language on an ordinary `.ts` file" is not a separate bug but the
same collapse seen from a different tool. `SIFT-WS5A` (`T062`–`T065`) owns that split; 021 gates on
it. See [research.md](research.md) §1 for the measured root cause.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 — A plan never points at the wrong file (Priority: P1) — `ACH-01`

An agent asks `edit_plan` to plan a change to a repository-relative path. The path either does not
exist yet, or exists but is metadata-only. In both cases the agent is told exactly that. It is
**never** handed a confident mutation menu aimed at an unrelated symbol in an unrelated file.

**Why this priority — and why it is independent**: this is the only finding in the ledger that causes
a **wrong write** rather than a false negative. Every other defect makes an agent conclude too
little; this one makes it edit the wrong file. Reproduced repeatedly (`SF-DOG-007`, and ledger lines
764–771, 779, 785–789, 826–830, 831–838): the path
`backend/src/modules/testing/services/generated-auth-global-setup.ts` resolves to
`Constant ts` in `generator.service.spec.ts`, and `edit_plan` then recommends `edit_within_symbol`,
`replace_symbol_body`, `batch_rename`, and `delete_symbol` against it.

Its fix is a **fail-closed guard** in `src/protocol/edit_plan.rs` (path-shaped input never degrades
to fuzzy symbol matching). It **does not depend on the admission root cause, on the honest reason
codes, or on any other story in this feature**, because the predicate it needs already exists at
`edit_plan.rs:90` and the Tier-2 lookup it needs already exists at
`src/live_index/query.rs:1243-1256`. **It must be schedulable immediately** — before, beside, or
independently of the prerequisite phase.

**Independent Test**: a nonexistent path ending in `.ts` while an indexed file contains a symbol
named `ts`; plus a path that exists but is Tier 2. Neither may produce a symbol plan. A
bidirectional guard asserts `Type.Method` and `Foo::bar` still reach the symbol cascade.

**Acceptance Scenarios**:

1. **Given** a nonexistent path ending in `.ts` and an indexed `Constant ts` elsewhere, **When**
   `edit_plan` targets that path, **Then** the result is `file_not_found` / a new-file plan naming
   the requested path — and no unrelated symbol or mutation recommendation appears anywhere in it.
2. **Given** a path that exists but is metadata-only, **When** `edit_plan` targets it, **Then** the
   result discloses `metadata_only` with the real reason, distinct from `file_not_found`.
3. **Given** `.js`, `.rs`, and `.py` variants of scenario 1, **When** each is planned, **Then** none
   degrades to an extension-named symbol.
4. **Given** the legitimate symbol selectors `Type.Method` and `Foo::bar`, **When** `edit_plan`
   targets them, **Then** they still resolve through the symbol cascade unchanged.
5. **Given** any path-shaped target that resolved to nothing, **When** the response renders, **Then**
   it does not emit the generic `search_symbols(query=…)` hint — that hint is what invites the fuzzy
   retry.

---

### User Story 2 — An explicit selection is honored or refused, never substituted (Priority: P1) — `ACH-02`

An agent reading a metadata-only file with `around_match`, `around_symbol`, or `chunk_index` gets a
bounded window centered on what it asked for, or an explicit structured refusal. It never gets the
head of the file under a header that claims the requested mode succeeded.

**Why this priority**: `get_file_context` prescribes raw `get_file_content` as *the* fallback for a
metadata-only file, and that fallback silently ignores three of its four selectors. The response
header still asserts `── mode: match (explicit) ──`, so the operator cannot distinguish "matched near
line 1" from "request ignored". Measured cost in the ledger: ~62.6K tokens of duplicate output from
seven `around_symbol` calls, plus ~20K more from twelve `around_match` probes (ledger lines 790–798).

**Independent Test**: a fixture that is Tier 2 **by deliberate policy** (a lockfile or oversized data
file), not one that is accidentally demoted — otherwise the prerequisite's root-cause fix makes the
test vacuous. Put a unique literal near EOF.

**Acceptance Scenarios**:

1. **Given** a Tier-2 fixture with a unique literal near EOF, **When** `around_match` requests it,
   **Then** a bounded window containing that literal and its real line number returns.
2. **Given** a literal that is absent, **When** `around_match` requests it, **Then** an explicit
   no-match result returns — not line-1 content.
3. **Given** `chunk_index` on a Tier-2 file, **When** it is requested, **Then** it is honored or
   explicitly refused; `around_line`, `around_match`, and chunk modes can never substitute for one
   another.
4. **Given** `around_symbol` on a Tier-2 file, **When** it is requested, **Then** the behavior is
   whatever the recorded owner ruling in [plan.md](plan.md) §D2 specifies — and it is never a silent
   full-file read.
5. **Given** any response whose selector could not be serviced, **When** it renders, **Then** the
   mode annotation does not claim the requested mode succeeded.

---

### User Story 3 — One admission oracle; a race is a race, not a verdict (Priority: P1) — `ACH-03`

An agent that just received exact parsed Tier-1 context for a file, then calls
`analyze_file_impact` on it, gets either an impact report or an explicit stale-generation result with
a retry instruction. It never gets a refusal that rewrites the file's admission class.

**Why this priority**: the reported string is internally impossible — `Not indexed: … is Tier 1 —
reason: policy, size 0.0 MB` simultaneously claims Tier 1 and not-indexed, invents a policy
exclusion, and reports zero size for a non-empty file. It is the byte-for-byte rendering of a
**successfully indexed** file that lost an optimistic-concurrency round
(`src/live_index/health_view.rs:297-306` returns exactly `tier: Normal, reason: None`;
`src/sidecar/handlers.rs:901-904` then prints `None` as the literal string `"policy"`). Post-edit
verification is the step that certifies work as done, so a false refusal there is expensive.

**Independent Test**: index a fixture, get exact parsed context, mutate it, immediately run impact
against the same session/generation; repeat under an active watcher event to cover the production
race.

**Acceptance Scenarios**:

1. **Given** a path present in the live `files` record, **When** context and impact both report on it
   in one generation, **Then** they expose the same tier, admission reason, byte size, project, and
   index generation.
2. **Given** a publication-generation race (`src/watcher/mod.rs:352-359`, `:539`, `:566-573`,
   `:576-579`), **When** impact loses that round, **Then** the response is a typed stale-generation
   result with a retry instruction — not an admission refusal.
3. **Given** an `AdmissionTierLookupView` with `tier: Normal` and `reason: None`, **When** it renders,
   **Then** it is never presented under "Not indexed", and `None` never renders as `"policy"`.
4. **Given** either impact branch, **When** it responds, **Then** the index generation is present
   (today `src/sidecar/handlers.rs:926` computes it and only the non-parser branch at `:935` prints
   it).

---

### User Story 4 — Untracked and excluded code is admitted, or honestly refused with a working recovery (Priority: P1) — `ACH-04`

An agent that creates a new source file, runs a full index, and searches for an identifier in it
either finds it, or is told the index is incomplete **and** given a recovery action that actually
works. It is never handed a nominally-successful `new_file=true` call whose payload says the file is
still unavailable.

**Why this priority**: the advertised recovery is architecturally incapable of recovering.
`src/sidecar/handlers.rs:800` promises `new_file=true` "Reads file from disk, parses it, indexes it";
`:892-894` and `:921-922` state "no force-admit"; and `src/watcher/mod.rs:277-284` shows
`admit_and_index_single_path` is literally `read_and_index(relative_path, abs_path, shared, None,
expected_gen)` — no override parameter exists on the seam. Meanwhile search **already proves the
match** (`src/protocol/tools.rs:2336-2365` runs the real query against the untracked file's bytes) and
then discards it to prose. Untracked files are **not** policy-excluded by default:
`SYMFORGE_EXCLUDE_UNTRACKED` is opt-in (`src/discovery/mod.rs:2192-2197`) and
`SkipReason::Untracked` is documented as never minted on the default path
(`src/domain/index.rs:1392-1396`).

**Blocked on**: `src/protocol/tools.rs` is inside PR #479's footprint. This story's fix sites are
almost entirely in that file, so it cannot start until #479 lands. Flagged, not worked around.

**Independent Test**: untracked TypeScript, JavaScript, Rust, and Python fixtures each holding a
unique identifier; one full index; exact search must find every identifier with no separate file
mutation.

**Acceptance Scenarios**:

1. **Given** untracked source fixtures in four supported languages, **When** one full index runs,
   **Then** exact search finds every unique identifier.
2. **Given** a fixture that policy genuinely excludes, **When** the full-index receipt and the search
   response both name it, **Then** they name the **same** exclusion reason.
3. **Given** an eligible fixture, **When** `analyze_file_impact(new_file=true)` runs once, **Then**
   the next exact search succeeds — or the tool's own description and schema say plainly that it
   re-runs the gate and cannot admit an excluded file.
4. **Given** a deliberately Tier-2 metadata-only path, **When** the untracked diagnostic lists
   recoverable candidates, **Then** that path is not listed — today `src/protocol/tools.rs:2184`
   tests only `guard.get_file(path).is_none()`, which is true for permanently-excluded files.
5. **Given** a search whose only matches live in unindexed files, **When** it returns zero indexed
   hits, **Then** the zero is qualified with the excluded count and reason class — never unqualified.

---

### User Story 5 — A session never reads another project's index (Priority: P1) — `ACH-05`

An agent working in one repository sees index counts, symbol counts, and file contents from **that**
repository. If the resolved sidecar belongs to another project, the agent is told — in band — rather
than served foreign numbers.

**Why this priority**: reproduced live during the investigation with two curl calls against one
sidecar rooted at `E:\project\aap-rooms-019`, both carrying `caller_root=E:/project/symforge`:
`/outline` correctly returned **409** naming the mismatch, and in the same instant `/health` returned
**200** with `{"file_count":2265,"symbol_count":76582}`. `src/sidecar/handlers.rs:344-346` exempts
exactly `/health` and `/stats` from `caller_root_guard`, and `/health` is the hook's highest-traffic
endpoint — `src/cli/hook.rs:865-868` routes any non-symbol Grep there and `:885` routes every unknown
tool there. `HealthResponse` (`src/sidecar/handlers.rs:88-93`) carries no `project_root`, no
`project_id`, and no generation, so the caller cannot detect the substitution either. Three
defenses, all absent on the one endpoint that matters. A hook that injects another repository's
counts corrupts every measurement taken in a session, including this feature's own before/after
numbers.

**Independent Test**: the two-curl reproduction, plus a descriptor census. During the investigation
`E:/project/symforge/.symforge/sessions/` held **22** descriptors (2026-07-13 → 07-27), 13 sharing
port 56828 under 13 different `session_id`s; all 7 distinct ports probed dead.

**Acceptance Scenarios**:

1. **Given** a sidecar rooted at project A, **When** a caller from project B requests `/health` with
   its real `caller_root`, **Then** the response either withholds the index-describing fields or
   discloses the mismatch in band — liveness stays answerable without identity.
2. **Given** any health response, **When** it renders, **Then** it carries an identity stamp
   (project root / id / generation), and the MCP `status` surface prints the same stamp so a mismatch
   is visible without a curl.
3. **Given** a session descriptor that omits `project_root`, **When** endpoint resolution runs,
   **Then** the descriptor is rejected — absent identity is not universal identity
   (`src/sidecar/port_file.rs:283-289`, `:161`, `:206`).
4. **Given** no descriptor matches, **When** resolution falls back to the legacy fixed port
   (`src/sidecar/port_file.rs:495-505`), **Then** the same root validation applies and a dead port is
   never returned as resolved.
5. **Given** a `\\?\`-prefixed root from the daemon (`//?/E:/project/symforge`) and the plain form
   stored in descriptors, **When** any of the three comparison sites runs
   (`src/sidecar/port_file.rs:337-348`, `src/sidecar/handlers.rs:375-388`,
   `src/cli/hook.rs:1083-1091`), **Then** all three agree — one shared comparator, not three
   semantics.
6. **Given** the hook's project resolution, **When** `current_dir()` fails, **Then** it fails loudly
   rather than defaulting to an empty path (`src/cli/hook.rs:255`).

---

### User Story 6 — Coverage honesty for search, planning, and health (Priority: P1) — `SIFT-WS5` *(INHERITED — not restated)*

`SF-DOG-001`…`SF-DOG-005` are owned by
`specs/020-repository-knowledge-index/sift/spec.md` **User Story 6** and implemented by
`specs/020-repository-knowledge-index/sift/tasks.md` **Phase 9, `T062`–`T082`**, with the mandatory
internal ordering `WS5A` → `WS5B`/`WS5C`.

021 references that work rather than duplicating it, and adds exactly one amendment (see
[research.md](research.md) §1 and task **T101**): **`T066`'s elimination list is wrong.** It records
"secret-pattern content (zero detector literals in `knowledge_search.rs`)" as a ruled-out cause. The
rule needs no secret literal — `let token = token.to_lowercase();` at
`src/protocol/knowledge_search.rs:1328` is sufficient to match it. Whoever wrote that elimination
checked for secrets, not for the rule's actual match surface.

### Edge Cases

- **ACH-01**: `Type.Method` (a legitimate Go receiver selector) contains a dot and MUST still reach
  the symbol cascade. A guard keyed on "any dot" would regress Go/C++ selector resolution — the
  predicate must key on `/` or a known-file-extension tail, and MUST be tested in both directions.
- **ACH-01**: `strip_qualification` (`src/live_index/disambiguation.rs:438-442`) is **correct for its
  purpose** (`Type.Method`, `Foo::bar`). Its `rsplit_once('.')` → `ts` is the proximate cause of the
  wrong match, but constraining it would break legitimate resolution. The defect is upstream: a
  path-shaped string must never be handed to it. **Do not change it.**
- **ACH-02**: the Tier-2 fixture MUST be Tier 2 by deliberate policy. If it is a file that the
  prerequisite's root-cause fix restores to Tier 1, the test silently stops exercising the Tier-2
  path.
- **ACH-03**: an `AdmissionTierLookupView` with `tier: Normal` and `reason: None` is **proof the file
  is indexed**. That combination reaching the not-indexed renderer is an internal inconsistency, not
  a state to format prettily.
- **ACH-04**: a file over `SECRET_SCAN_MAX_BYTES` (4 MiB, `src/knowledge/mod.rs:32`) returns
  `Indeterminate { ResourceLimit }` (`:163-167`) and therefore also reports "unsupported language".
  Currently latent on this repository (all measured files are well under 4 MiB) but in the same class.
- **ACH-05**: a git worktree's CWD fails `same_root_identity` against the main checkout's descriptor
  and then falls through to the identity-unchecked legacy port at `src/sidecar/port_file.rs:501`.
  Mechanically plausible, **unreproduced** (the worktree in question is owned by another agent).
- Any lexical fallback and any raw read added by this feature MUST respect the frozen security
  invariant below.

## Requirements *(mandatory)*

### Security invariant — frozen, carried forward verbatim

> **A bounded lexical fallback must NEVER read files excluded for `SensitivePath` or
> `SensitiveContent`. Feature 020's security contract governs.**

This is `FR-023` of the sift slice and it is not reopened here. Note the honest consequence of the
measured root cause: correcting the over-broad `secret.context-assignment` rule means files that were
**never** sensitive stop being classified as sensitive, so they become lexically readable
legitimately — by removing a false positive, not by weakening the invariant. Files the corrected
detector still flags remain metadata-only with their bytes and hashes discarded.

### Functional Requirements

- **FR-001**: A path-shaped `edit_plan` target that matches no indexed file MUST NOT be passed to the
  symbol cascade as a symbol selector. The existing predicate at `src/protocol/edit_plan.rs:90` MUST
  veto cascade entry at `:103-121`, not merely gate suffix-match construction.
- **FR-002**: The path-shaped predicate MUST distinguish `dir/file.ts` from `Type.Method`. It MUST
  key on a path separator or a known-file-extension tail, never on the presence of any dot, and MUST
  carry tests in both directions.
- **FR-003**: A path-shaped miss MUST resolve to a typed outcome: `metadata_only` (with the real
  reason, sourced from `LiveIndex::metadata_only_skipped_paths()`,
  `src/live_index/query.rs:1243-1256`) or `file_not_found` / new-file plan pointing at
  `analyze_file_impact(new_file=true)`. It MUST NOT emit the generic `search_symbols(query=…)` hint.
- **FR-004**: `src/live_index/disambiguation.rs:438-442` (`strip_qualification`) MUST NOT be changed.
- **FR-005**: `render_file_content_bytes` (`src/protocol/format.rs:3176-3225`) MUST NOT reach its
  whole-file return at `:3212` while an unserviced selector (`around_match`, `around_symbol`,
  `chunk_index`) is present in the `ContentContext` it was handed. Each selector MUST be honored or
  explicitly refused.
- **FR-006**: The raw-byte path MUST reuse the existing structured refusals
  (`not_found_file_match`, `src/protocol/format.rs:3508`; the occurrence-not-found message at
  `:3518-3522`) rather than inventing parallel ones. Those refusals are today gated behind
  `file: &IndexedFile` and MUST be split so raw bytes can reach them.
- **FR-007**: The raw-byte and indexed selector dispatch orders MUST NOT be able to drift; the
  indexed reference implementation is `src/protocol/format.rs:3061-3100`.
- **FR-008**: A response MUST NOT assert a selection mode it did not honor. The annotation built at
  `src/protocol/tools.rs:8532-8538` and prepended at `:8618-8621` MUST become truthful — preferably
  with **no edit** to `tools.rs` (PR #479 footprint), by making the renderer honor or refuse.
- **FR-009**: `ReindexResult::Skipped` MUST distinguish an admission decision from a lost
  optimistic-concurrency round. The publication-race and attempt-exhaustion returns
  (`src/watcher/mod.rs:352-359`, `:539`, `:566-573`, `:576-579`) MUST carry their own variant.
- **FR-010**: Both `analyze_file_impact` branches (`src/sidecar/handlers.rs:958`, `:1113`) MUST route
  a stale-generation outcome to an explicit retry instruction, never to the admission renderer.
- **FR-011**: `capture_admission_tier_lookup_view` (`src/live_index/health_view.rs:275-309`) MUST NOT
  let a stale terminal manifest disposition override the live `files` record. Context and impact MUST
  answer from one authority in one generation, or return both and flag the disagreement.
- **FR-012**: `reason: None` MUST NOT render as the string `"policy"`
  (`src/sidecar/handlers.rs:901-904`), and a `Normal`/Tier-1 view MUST NOT render under
  "Not indexed" (`:918-923`).
- **FR-013**: A non-empty file MUST NEVER report zero size. The `size / 1 MiB` at
  `{:.1} MB` precision (`src/sidecar/handlers.rs:905`, rendered at `:920` and `:934`) rounds every
  file under ~51 KB to `0.0 MB`. **One shared fix, one task** — this bug appears in `SF-DOG-002`,
  `SF-DOG-004`, `SF-DOG-008`, and `SF-DOG-009`.
- **FR-014**: Both impact branches MUST report the index generation
  (`src/sidecar/handlers.rs:926` computes it; only `:935` prints it).
- **FR-015**: The untracked-recoverable test MUST consult admission, not only `files` membership
  (`src/protocol/tools.rs:2184`). A permanently-excluded Tier-2 path MUST NOT be advertised as
  recoverable.
- **FR-016**: `untracked_file_diagnostic` (`src/protocol/tools.rs:2191-2198`) MUST NOT recommend a
  recovery the admission gate will refuse. Where the gate would refuse, it MUST name the real
  exclusion reason instead.
- **FR-017**: `new_file=true`'s documented promise and implemented contract MUST agree. Either
  `admit_and_index_single_path` (`src/watcher/mod.rs:277-284`) gains a real admission override, or the
  description at `src/sidecar/handlers.rs:800` and the `analyze_file_impact` tool schema state plainly
  that it re-runs the gate and cannot admit an excluded file. **The current state — promising
  indexing at `:800` and disclaiming force-admit at `:892-894` — is not an option.**
- **FR-018**: `search_text` MUST NOT discard a match it already proved. `untracked_text_matches`
  (`src/protocol/tools.rs:2336-2365`) runs the real query against the file's bytes;
  `matching_untracked_paths_for_search_text` (`:2410-2421`) reduces it to `Vec<String>` and `:3351`
  appends it as prose. The proven locations MUST reach the result set (marked unindexed), or the zero
  MUST be explicitly qualified.
- **FR-019**: `caller_root_guard` (`src/sidecar/handlers.rs:344-346`) MUST NOT serve
  index-describing fields (`file_count`, `symbol_count`, `index_state`) to a mismatched
  `caller_root`. Liveness MUST remain answerable without identity.
- **FR-020**: `HealthResponse` (`src/sidecar/handlers.rs:88-93`) MUST carry an identity stamp, and
  the hook and the MCP `status` surface MUST print the same stamp.
- **FR-021**: A session descriptor with `project_root: None` MUST be rejected, not accepted by every
  project (`src/sidecar/port_file.rs:283-289`, `:161`, `:206`).
- **FR-022**: The legacy fixed-port fallback (`src/sidecar/port_file.rs:495-505`) MUST apply the same
  root validation as the descriptor path, and MUST NOT return a port without a liveness check.
- **FR-023**: The three root comparators (`src/sidecar/port_file.rs:337-348` — no canonicalization;
  `src/sidecar/handlers.rs:375-388` — `dunce` strips `\\?\`; `src/cli/hook.rs:1083-1091` — fed
  `std::fs::canonicalize`, which adds `\\?\`) MUST collapse to one shared comparator used by all
  three call sites.
- **FR-024**: The hook MUST resolve the project root deterministically and fail loudly, not
  `unwrap_or_default()` to an empty path (`src/cli/hook.rs:255`). The daemon session it selects
  (`src/cli/hook.rs:1041-1058`) MUST be validated against the index root that session currently
  holds, not only the registered `canonical_root`.
- **FR-025**: Descriptor resolution MUST prune or age out stale records on the **read** path
  (`cleanup_stale_descriptors_at`, `src/sidecar/port_file.rs:234-241`, is today only on the
  update/repair path), and MUST either migrate or explicitly stop honoring the legacy
  `<control_state>/sessions` location that the current reader (`:23`, `:62-64`) no longer inspects.
- **FR-026**: Every fix in this feature MUST ship with a regression test that fails on the pre-fix
  behavior and passes after. A check that passes either way is not evidence.
- **FR-027**: `SkipReason` honesty (`FR-020` of the sift slice) is **inherited, not restated**.
  021 gates on `T062`–`T065` landing before `ACH-03` and `ACH-04` are worked.

### Key Entities

- **Admission decision vs. file disposition**: `FileDisposition::MetadataOnly { reason }` carries the
  true `MetadataOnlyReason`; `AdmissionDecision { tier, reason: SkipReason }` is the compatibility
  projection that currently loses it. The two must stop disagreeing.
- **Admission oracle**: the source a tool consults to answer "is this path indexed, and at what
  tier". There are two today (`manifest_entries` and `files`) and impact prefers the stale one.
- **Reindex outcome**: what the watcher returns for one path. Currently one `Skipped` variant carries
  both "the gate refused this" and "you lost a publication race".
- **Path-shaped target**: an `edit_plan` selector that denotes a file, not a symbol. Must be
  distinguishable from `Type.Method`.
- **Sidecar identity**: the triple (declared project root, live indexed root, generation) that proves
  which index answered. Absent from `HealthResponse` today.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `edit_plan` on a nonexistent `.ts`/`.js`/`.rs`/`.py` path returns `file_not_found` or a
  new-file plan, and the response contains **zero** symbol or mutation recommendations. Verified by
  asserting the literal pre-fix output `Constant ts` is absent.
- **SC-002**: `edit_plan` on an existing metadata-only path returns `metadata_only` with the real
  reason, textually distinct from the `file_not_found` result.
- **SC-003**: `Type.Method` and `Foo::bar` resolve through the symbol cascade with byte-identical
  output before and after `ACH-01`.
- **SC-004**: On a deliberately Tier-2 fixture, `around_match` for a literal near EOF returns a
  bounded window containing that literal at its real line number; an absent literal returns the
  explicit no-match text; `chunk_index` and `around_symbol` never return the file head.
- **SC-005**: No response asserts a selection mode it did not honor — verified by a test that reads
  the mode annotation and the returned first line number together.
- **SC-006**: For one path in one generation, `get_file_context` and `analyze_file_impact` report the
  same tier, admission reason, byte size, project, and generation.
- **SC-007**: A publication race produces a typed stale-generation response that succeeds after one
  refresh; the string `Tier 1 — reason: policy` is unreachable.
- **SC-008**: No non-empty file reports `0.0 MB` anywhere in the impact or context surfaces —
  verified with a ~10 KB fixture, which today prints `0.0 MB`.
- **SC-009**: After one full index, exact search finds the unique identifier in each of four
  untracked fixtures (`.ts`, `.js`, `.rs`, `.py`) with no separate file mutation; where policy
  excludes a fixture, the index receipt and the search response name the same reason.
- **SC-010**: `analyze_file_impact(new_file=true)` either makes the next exact search succeed, or its
  description and schema state that it cannot admit an excluded file. No response is nominally
  successful while its payload says the file is unavailable.
- **SC-011**: A search whose only matches live in unindexed files returns a **qualified** zero naming
  the excluded count and reason class.
- **SC-012**: The two-curl reproduction inverts: `/health` with a mismatched `caller_root` no longer
  returns another project's `file_count`/`symbol_count`. Verified against the exact figures observed
  (`2265`/`76582` from `E:\project\aap-rooms-019`).
- **SC-013**: Every health/status surface prints an identity stamp, and the hook's stamp equals the
  MCP `status` stamp for the same session.
- **SC-014**: A descriptor census after the fix shows zero dead-port descriptors returned as resolved
  endpoints (baseline: 22 descriptors, 7 distinct ports, all dead).
- **SC-015**: `SF-DOG-001`…`SF-DOG-005` are closed by `T062`–`T082`, and the demoted-file set
  measured in **T102** is back at Tier 1 with `search_symbols`/`search_text`/`get_symbol` returning
  real bodies for each.
- **SC-016**: The full gate is green: `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`,
  `cargo build --release`, `cargo check --no-default-features --features embed`, and
  `cd npm && npm test`.

## Assumptions

- The nine findings are **not** independently deliverable in arbitrary order. `ACH-01` is fully
  independent and is the MVP. `ACH-03` and `ACH-04` depend on the inherited reason-code split
  (`T062`–`T065`). `ACH-02` and `ACH-05` are independent but sequenced for review size. See
  [tasks.md](tasks.md) §Dependencies.
- `src/protocol/tools.rs`, `src/protocol/edit_tools.rs`, `src/server/serve.rs`,
  `.github/workflows/ci.yml`, `Cargo.toml`, `tests/serve_port.rs`, and `tests/stel_symforge_edit.rs`
  are in PR #479's footprint as of 2026-07-28. `ACH-04` is gated on #479 landing; `ACH-02` is
  deliberately routed through `src/protocol/format.rs` so it needs no `tools.rs` edit.
- Every before/after measurement in this feature is taken from the MCP `status` route, which was
  proven consistent (four symforge sessions all reporting `891`/`25050`) — **never** from the
  PostToolUse hook, which is the surface `ACH-05` exists to fix.
- No new dependency is introduced. The regex crate, `dunce`, and the existing trust-envelope
  mechanism are sufficient.
- Findings whose cause is **undetermined** get an investigation task with a recorded output before any
  fix task. Three exist: which `MetadataOnlyReason` demoted the ledger's `.ts` file; which of the
  ~8 `Skipped` sites fired for `exact-origin-proxy.ts`; whether the two unregistered sidecars are
  orphaned or failed-to-register.

## Out of Scope

- Rewriting or re-owning `T062`–`T082`. They are referenced; only `T066`'s elimination list is
  amended.
- The remaining `SF-DOG` **observations awaiting isolated reproduction** (ledger lines 613–881),
  except where an investigation task in this feature happens to close one. Specifically deferred:
  the `batch_edit` double-semicolon report, the per-delegated-agent empty-index cost (four
  reproductions, but the daemon/session contract question is a separate design decision), the
  `project:` selector discoverability mismatch, `degraded[ObservationFailed]` manifest freshness, and
  `atomic_durability_unavailable` curation.
- Per-range secret redaction as an alternative to full-file demotion. That is a design question with
  a recorded owner decision (task **T104**); implementing it is a separate feature if chosen.
- Moving the repository's own detector-test canary strings out of
  `src/knowledge/mod.rs` / `src/live_index/store.rs` / `src/protocol/tools.rs` into external
  fixtures. Fixing the rule restores those files without editing them — which is also why the rule
  fix is the correct target and not the fixtures.
- Any change to `E:\project\testpilot` or to the read-only defect ledger.
