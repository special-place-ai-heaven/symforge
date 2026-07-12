# Feature Specification: SFBENCH Surface Correctness & Safety

**Feature Branch**: `019-sfbench-surface-correctness`

**Created**: 2026-07-12

**Status**: Draft

**Input**: The SFBENCH-1.0 full-surface benchmark of `symforge 8.14.0`
(`docs/dogfood/2026-07-12-symforge-8.14.0-full-surface-benchmark-report.md`)
alleged four P0 and several P1 defects. Every finding was **independently
re-verified against current source** by a 7-agent skeptical review
(2026-07-12); the benchmark's severities were **not** taken at face value. This
spec carries only the findings that survived verification, at their *verified*
severity, and explicitly records what the report got wrong so settled work is
not re-opened.

> [!IMPORTANT]
> Spec 018 (dogfood surface hardening) is already merged. 018 made
> `detect_impact` / `what_changed` **default to source-focused** so *data files*
> don't dominate the blast radius. US2 here is a **different, deeper** layer:
> even with only source symbols, `detect_impact` over-approximates because it
> seeds *every* symbol of a changed file and links call edges by *bare name*
> across the whole repo. 018 removed data-file noise; 019 fixes symbol-identity
> coarseness. Do not revert or duplicate 018's `code_only` defaulting.

## Verification summary (what is in scope and why)

> [!IMPORTANT]
> **This spec was adversarially reviewed against source (2026-07-12) and the
> review found real defects IN THIS SPEC — it is the amended (post-review)
> version.** The severities and stories below reflect the amendments. See
> `research.md` § "Adversarial review corrections" for the full before/after.
> The load-bearing change: **P0-03 was wrongly refuted** — the *stdio cold-start*
> path IS a real permanent-staleness bug (US5b, P1), separate from the counter
> over-count (US5a, P2). And a new state defect (daemon `reset_calibration`
> no-op, US5c) surfaced. US1/US2 fix rules were corrected to not regress green
> tests; US2 oracle was strengthened.

| Report ID | Report sev | Verified verdict | Verified sev | Story |
|---|---|---|---|---|
| P0-01 `batch_rename` unsafe multi-file write | P0 | **CONFIRMED** | **P0** | US1 |
| P0-02 `detect_impact` confidently wrong | P0 | **CONFIRMED** | **P0** | US2 |
| P1-04 selector refusal + Python relative-import drop | P1 | **CONFIRMED** | **P1** | US3 |
| P1-06a `symforge_edit` same-key replay rejected by stale `if_match` | P1 | **CONFIRMED** | **P2** | US4 |
| P0-03b watcher cold-start permanent staleness | P0 | **CONFIRMED (review)** — refutation was wrong for stdio cold-start | **P1** | US5b |
| P0-03a watcher "reconcile repairs" over-counts no-ops | P0 | **CONFIRMED** | **P2** | US5a |
| P1-07 daemon `status(reset_calibration=true)` silent no-op | P1 | **CONFIRMED (review)** — was wrongly omitted | **P2** | US5c |
| P1-05 `validate_file_syntax` rejects valid BOM JSON; inert `estimate` | P1 | PLAUSIBLE (root cause refuted; BOM real) | **P3** | US6 |

**Out of scope — verified NOT a product bug:**

- **P0-04 (meta surface advertises one unrunnable tool)** — `meta` is a
  *measurement-only* A-019 L0 A/B probe that already tied then **lost** the
  selection to `compact-3` and is never advertised to users. The guard is
  by-design; the intended probe-relay path works. No fix. (If desired later,
  the only reasonable action is deleting the retired variant — tracked as a
  cleanup note in `research.md`, not a story here.)

**Corrections folded into the stories (report was partly wrong):**

- P0-02 conflated `detect_impact` (the real explosion: seed + bare-name edges)
  with `analyze_file_impact` (a separate, *partially guarded* tool). US2 fixes
  the former and only tightens the latter's residual name-only lookup.
- P1-05's stated root cause ("JSON uses permissive tree-sitter") is **false** —
  JSON already uses `serde_json`. Trailing-comma tolerance is **deliberate
  JSONC/tsconfig support** with dedicated tests and MUST NOT be "fixed". Only
  the BOM-rejection and the inert `estimate` flag are real. US6 is scoped to
  exactly those.
- P1-06's insertion-whitespace sub-claim is **refuted** (layout is already
  shared; single-`\n` before doc-commented items is intentional). US4 carries
  only the replay-ordering half.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 — `batch_rename` never writes code it cannot prove is the target (Priority: P0 — SAFETY, MVP)

An agent renames a method on a real polyglot repo. Today, `batch_rename` with a
**common name** (e.g. Python `run`) scoped to one file resolves the target only
to locate the *definition*, then collects references by **bare name across the
whole index** and writes **every** match — including unrelated same-named
definitions in other packages/files — because `dry_run` defaults to `false` and
`code_only` filters only by *language class*, not by symbol identity. The result
is falsely labeled a "constrained" rename. This is a source-write escape.

**Why this priority**: A tool that silently rewrites unrelated source on a
common name is the single most dangerous behavior on the surface — it corrupts
code the user never named. It is a fail-closed safety obligation (Constitution:
Determinism & Recovery; Trust Envelopes) and outranks every economics or
correctness item.

**Independent Test**: On a fixture with two unrelated definitions sharing a name
(`Target::run` in file A, an unrelated `Widget::run` / Flask handler `run` in
file B), call `batch_rename(name="run", path="A", ...)` at apply. Confirm **zero
bytes change in file B** and that file B's references are reported as *uncertain
/ not written*, not silently rewritten. Fully testable on its own.

> [!IMPORTANT]
> **Writability is keyed on index-wide name AMBIGUITY, not on "has resolved
> identity"** (adversarial-review correction B3). Tier-0 bare-name references
> *never* carry resolved identity, so keying on identity would demote every
> rename and regress the green tests `batch_rename_updates_definition_and_callers`
> and `batch_rename_bumps_definition_and_call_site` (which require the bare
> `old_name()` call site to be rewritten and labeled "constrained"). The rule:
> **a name with exactly ONE definition in the index → all bare-name refs stay
> writable and "constrained" (unchanged behavior); a name with 2+ definitions →
> name-only refs are demoted to uncertain / non-writable and the trust label
> reflects that.**

**Acceptance Scenarios**:

1. **Given** two unrelated same-named symbols in different files (name is
   index-ambiguous), **When** `batch_rename` targets one by path, **Then** only
   references bound to the target are written; unrelated same-name sites are left
   untouched and surfaced as uncertain.
2. **Given** an index-ambiguous name where exact binding of a given reference is
   unavailable (dynamic language, name-only match), **When** apply runs, **Then**
   that reference is **not written** and is reported as uncertain — never
   silently rewritten. If NO reference can be safely bound, the tool fails closed
   (aborts before staging any write).
3. **Given** a genuinely unambiguous, single-definition symbol (name has exactly
   one definition in the index), **When** `batch_rename` runs, **Then** its
   existing behavior, full reference set, writes, and "constrained" label are
   **unchanged** (the fix removes unsafe writes on ambiguous names, not safe ones
   on unique names — the two green regression tests must still pass).
4. **Given** any rename result, **When** the trust envelope is emitted, **Then**
   the match label reflects the *actual* binding confidence — "constrained" only
   when the name is unambiguous or the ref is identity-bound; never "constrained"
   over name-only matches of an ambiguous name.

---

### User Story 2 — `detect_impact` reports the symbols that actually changed and the calls that actually reach them (Priority: P0 — CORRECTNESS)

An agent asks "what does this change affect?" after a one-line edit to a leaf
function. Today `detect_impact` seeds **every symbol in every changed file** as
"changed" (a 1-line edit in a 20-symbol file reports 20 changed symbols), and
the impact graph links each call to **every same-bare-name definition in the
repo**, so unrelated functions and duplicate `main` nodes flood the blast
radius. The output is confidently wrong and far larger than a competent
`git diff` + reference walk.

**Why this priority**: `detect_impact` is a core "what breaks" tool; a wrong,
inflated answer trains agents to distrust the surface and costs tokens. The
project's own contract already recorded a 291K-symbol / 54MB "explosion" from
this path and only band-aided it with truncation — the root cause is still live.

**Independent Test**: On the controlled fixture graph
(`sfbench_entry → sfbench_mid → sfbench_leaf`), edit only `sfbench_leaf`'s body
and call `detect_impact`. Assert exactly **one** changed symbol (`sfbench_leaf`)
and a hop-1 blast of exactly `{sfbench_mid}`.

> [!IMPORTANT]
> **The leaf-edit-on-the-unique-name fixture is NOT sufficient on its own**
> (adversarial-review corrections M2/M3). Because `sfbench_leaf`/`sfbench_mid`
> are globally unique names, that test passes whether or not the *edge* fix (b)
> ships — it only exercises the *seed* fix (a). The gating test MUST ALSO include
> the two-same-name case (Scenario 3) and, for the entry-point clause (FR-004),
> a fixture that actually contains a reachable duplicate `main` (the current
> SFBENCH fixture has **no** `main` function, so it cannot trigger the
> entry-point collapse). Add a `main`-bearing, multi-owner element to the US2
> fixture, or explicitly mark the entry-point clause untested-on-fixture — do not
> self-certify FR-004 from a fixture that structurally cannot exercise it.

**Acceptance Scenarios**:

1. **Given** a body-only edit to one function in a multi-symbol file, **When**
   `detect_impact` runs, **Then** only that function is a changed (hop-0) symbol
   — sibling symbols in the same file are not reported as changed.
2. **Given** a comment-only / whitespace shift, **When** `detect_impact` runs,
   **Then** it yields **zero** changed symbols.
3. **Given** two unrelated definitions named `run`, **When** the blast radius is
   computed from a change to one, **Then** the other never enters the blast set,
   and every blast node carries disambiguating identity (path/kind), not a bare
   name — no indistinguishable duplicate nodes.
4. **Given** an added file and a deleted file, **When** `detect_impact` runs,
   **Then** added/removed symbols are classified correctly and the blast set is
   seeded only from genuinely added/modified/removed symbols.
5. **Given** the separate `analyze_file_impact` path, **When** a same-file caller
   exists, **Then** it is included and typed as a call (residual name-only
   lookup tightened), preserving 018's existing parent-type narrowing.

---

### User Story 3 — Local project selectors work, and Python relative imports are resolved (Priority: P1 — CORRECTNESS)

An agent working in a bound project passes a **matching** `project` selector to
`search_symbols` / `search_text` / `find_references`. Today those three tools
refuse **any** selector (they call the wrong refusal guard), while sibling tools
accept a matching-local selector — an inconsistent, surprising failure.
Separately, Python `from .protocol import x` (relative import) is dropped by the
xref query, so `find_dependents` misses the real importer and a same-stem
fallback can invent a false consumer.

**Why this priority**: Selector inconsistency and false/missing dependents
undermine navigation trust across the most-used read tools, and the import gap
feeds P0-02's identity problem. Lower frequency and blast radius than the P0
safety/correctness items, so P1.

**Independent Test (selector)**: In a single bound project, call each of the
three tools with a `project` selector equal to the bound project's name/key/root
and confirm the call **succeeds** (same result as no selector), while a *foreign*
selector is still refused with a typed invalid-request. **Independent Test
(import)**: index a Python package where `pkg/a.py` does `from .b import f`;
confirm `find_dependents` on `pkg/b.py` includes `pkg/a.py` and does **not**
include an unrelated same-stem module.

**Acceptance Scenarios**:

1. **Given** a selector matching the bound project (name, key, or root), **When**
   `search_symbols` / `search_text` / `find_references` run, **Then** the call
   proceeds and returns the same result as the no-selector call.
2. **Given** a foreign or non-matching selector, **When** those tools run,
   **Then** they refuse with a typed invalid-request (foreign, not blanket).
3. **Given** both singular `project` and plural `projects` (incl. `"*"`), **When**
   any selector-bearing tool runs, **Then** matching-local proceeds and
   foreign/over-broad is refused consistently across both parameters and both
   directions (no regression of the P2-10 `projects=["*"]` under-refusal).
4. **Given** a Python relative import `from .mod import x` (and `..mod`), **When**
   `find_dependents` runs on the imported module, **Then** the importing file is
   reported, resolved by counting the relative prefix against the importer's
   package — and a same-stem unrelated module is not.

---

### User Story 4 — `symforge_edit` idempotent replay returns the stored result instead of failing on a now-stale `if_match` (Priority: P2 — CORRECTNESS)

An agent re-sends an identical `symforge_edit` apply (same idempotency key, same
request, including the original `if_match`) — the intended idempotent retry.
Today the pre-apply concurrency gate validates `if_match` **before** the replay
lookup, and after the first successful apply the body has changed, so the stale
`if_match` no longer matches and the replay is rejected (observed 3/3). A replay
carrying only the idempotency key (no `if_match`) already works.

**Why this priority**: Correct idempotent-retry semantics matter for reliable
apply-with-retry, but the caller has a working path (key-only replay) and no
data is corrupted, so P2 rather than P0/P1.

**Independent Test**: Apply an edit with `{idempotency_key, if_match}`; re-send
the identical request. Assert the second call returns the **stored** result and
writes **zero** bytes. Then send the same key with a *changed* request and assert
it conflicts; and a new key with a stale `if_match` still fails concurrency.

**Acceptance Scenarios**:

1. **Given** a completed apply, **When** the identical `{key, request, if_match}`
   is replayed, **Then** the stored result is returned and no bytes are written.
2. **Given** the same key but a changed request/hash, **When** replayed, **Then**
   it is rejected as an idempotency conflict.
3. **Given** a new key whose `if_match` does not match current state, **When**
   applied, **Then** it fails the concurrency guard (fix must not weaken genuine
   optimistic-concurrency protection).

---

### User Story 5b — Watcher does not go permanently stale after cold-start indexing (Priority: P1 — CORRECTNESS)

> [!CAUTION]
> **This story exists because the adversarial review (2026-07-12) refuted my
> own earlier refutation.** I originally claimed "every reload path restarts the
> watcher, so permanent staleness does not exist" and downgraded P0-03 to a
> counter-only P2. That is **false for the stdio cold-start path**, verified in
> source below. This is a real correctness bug on the *common* cold-start case.

On a cold start with no persisted snapshot, `src/main.rs` fires
`bg_index.reload(&bg_root)` **fire-and-forget** on a blocking thread (not
awaited) — which bumps the project generation — and then, without synchronizing,
spawns the file watcher, which captures `expected_gen` **once**. The cheap
watcher spawn almost always wins the race against the ~220 ms full-tree reload,
so it captures the *pre-reload* generation. Every later watcher/reconcile
operation then sees `current_project_generation() != expected_gen` →
`GenerationMismatch` → the tracked file is **removed and never re-indexed**.
Because the reconcile loop is pinned to the same stale generation, it cannot
self-heal. The daemon `reload_with` and stdio `index_folder` paths *do* restart
the watcher (which is why they are fine); the cold-start bootstrap does not.

**Why this priority**: A watcher that silently stops indexing edits on a
freshly-started cold repo is a correctness failure of the core promise (the
index tracks the working tree). It is common (cold start is the default
first-run path), but it is recoverable by restart and does not corrupt data, so
P1 rather than P0.

**Independent Test**: Cold-start a snapshot-less repo (force the fire-and-forget
reload to bump the generation before the watcher captures it — e.g. deterministic
ordering hook or a seam that lets the test observe the captured vs current
generation). Modify a tracked source file. Assert the change is **indexed**
(reconcile repairs it) rather than `GenerationMismatch`-removed, and that
`current_project_generation() == expected_gen` holds for the running watcher.

**Acceptance Scenarios**:

1. **Given** a cold start whose background reload bumps the generation after the
   watcher spawns, **When** a tracked file is modified, **Then** the edit is
   indexed (not permanently rejected as a generation mismatch).
2. **Given** the fix, **When** the daemon `reload_with` / `index_folder` restart
   paths run, **Then** their existing correct behavior is unchanged.
3. **Given** a genuine in-flight stale mutation (a real concurrency race), **When**
   it is applied, **Then** it is still correctly fenced/rejected — the fix
   re-syncs the cold-start generation, it does not remove the generation guard.

**Fix direction** (choose one, decide in `plan.md`/`research.md`): await the
cold-start reload before spawning the watcher (reorder in `main.rs`), OR have
`run_watcher_with_stop` re-read `current_project_generation()` per committed
reconcile/watcher batch instead of capturing once, OR subscribe the watcher to
generation changes.

---

### User Story 5a — Health "reconcile repairs" counts real repairs, not generation-mismatch no-ops (Priority: P2 — OBSERVABILITY)

An operator reads `health` after an index reload and sees a "reconcile repairs"
count that includes files where reconciliation actually **did nothing** (a
generation mismatch was correctly rejected, repairing zero bytes). The number
overstates repair activity during any restart/reset window.

> [!NOTE]
> This is the *narrow, real* half of the benchmark's P0-03 that survived
> verification (the counter over-counts). It is **causally linked to US5b**: the
> flood of `GenerationMismatch` no-ops that inflate this counter is largely
> *produced by* the cold-start staleness race — fixing US5b drains most of them.
> Fix both; do not treat this counter fix as the whole of P0-03.

**Why this priority**: Honest health signals matter (Trust Envelopes /
Observability), but this is a reporting overcount, not lost correctness — P2.

**Independent Test**: Force a reconcile pass over files whose freshen result is
`GenerationMismatch` (no actual reindex/remove) and assert the reported repair
count is **zero** for those, while genuine `StaleReindexed`/`StaleRemoved` files
still count. Rejected attempts are surfaced separately, not as repairs.

**Acceptance Scenarios**:

1. **Given** a reconcile pass over generation-mismatched files, **When** `health`
   reports repairs, **Then** those no-op files contribute **zero** to the repair
   count.
2. **Given** genuinely stale files that are reindexed/removed, **When** `health`
   reports, **Then** they are counted as repairs as before.
3. **Given** rejected (mismatch) attempts, **When** `health` reports, **Then**
   they are surfaced as a distinct, honestly-labeled figure (or omitted), never
   folded into "repairs".

---

### User Story 5c — Daemon `status(reset_calibration=true)` actually resets durable calibration (Priority: P2 — CORRECTNESS/TRUST)

> [!CAUTION]
> **Added by the adversarial review** — this was a `status` FAIL the first draft
> wrongly dropped into the deferred "economics" bucket. It is a trust-envelope
> state defect, exactly the class 019 carries.

In the default daemon-proxy topology, `status(reset_calibration=true)` returns a
success-shaped response (`OutcomeClass::Found`) but **resets nothing**:
`status_stel_tool` proxies to the storeless daemon worker and returns early after
overlaying proxy-owned status lines, before ever reaching the actual reset logic
in `render_stel_status_body`. The proxy owns the durable calibration store but
never calls its own `reset_calibration()`; the worker it proxies to has no store.
So durable calibration is left byte-identical while the caller is told it
succeeded.

**Why this priority**: A documented operator action that silently no-ops while
reporting success is a real correctness/honesty defect, but it affects an
infrequent maintenance operation and corrupts no data — P2.

**Independent Test**: In daemon mode with durable calibration samples present,
call `status(reset_calibration=true)`; assert the durable samples/constants are
cleared to `deferred` afterward, and that an equivalent local (non-daemon) reset
behaves identically.

**Acceptance Scenarios**:

1. **Given** a daemon with durable calibration state, **When**
   `status(reset_calibration=true)` runs, **Then** the proxy-owned durable store
   is reset to `deferred` and the response honestly reports the reset.
2. **Given** the no-store worker path, **When** reset is requested, **Then** the
   response does not falsely claim success on state it cannot touch (honest
   receipt).
3. **Given** the local (non-daemon) path, **When** reset runs, **Then** its
   existing correct clearing behavior is unchanged.

---

### User Story 6 — `validate_file_syntax` accepts valid BOM-prefixed JSON and honors (or drops) `estimate` (Priority: P3 — CORRECTNESS)

A caller validates a JSON file saved with a UTF-8 BOM (valid per RFC 8259).
Today `validate_file_syntax` rejects it at line 1 col 1 because `normalize_jsonc`
does not strip a leading BOM before `serde_json`. Separately, the `estimate`
input flag is defined but never read by the handler.

> [!NOTE]
> The benchmark also claimed trailing-comma JSON is wrongly accepted. That is
> **deliberate JSONC/tsconfig support** (dedicated tests
> `test_jsonc_trailing_commas_now_parse`, `test_tsconfig_jsonc_*`). It is **not**
> a defect and MUST NOT be "fixed" — doing so regresses shipped behavior. This
> story is scoped to BOM handling and the inert `estimate` flag only.

**Why this priority**: BOM-prefixed JSON is uncommon and the tool already returns
an honest failure (not a crash), so low impact — P3. Bundled because both are
tiny, same-tool, low-risk fixes.

**Independent Test**: Validate a byte-identical valid JSON file with and without a
leading UTF-8 BOM; assert both report `ok` with identical byte/span accounting.
Validate a genuinely malformed JSON and assert it still fails at the oracle
location. Confirm `estimate=true` either returns a tagged estimate within
tolerance or the flag is removed from the schema.

**Acceptance Scenarios**:

1. **Given** valid JSON with a leading UTF-8 BOM, **When** validated, **Then** it
   reports `ok`, matching the same file without a BOM.
2. **Given** genuinely malformed JSON (not JSONC), **When** validated, **Then** it
   still reports failure at the correct location (no regression).
3. **Given** shipped JSONC (trailing commas / comments), **When** validated,
   **Then** it still reports `ok` (018/existing behavior preserved).
4. **Given** `estimate=true`, **When** validated, **Then** the response is a
   tagged estimate within declared tolerance, or `estimate` is removed from the
   input contract (no silent inert flag).

---

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `batch_rename` writability MUST be keyed on **index-wide name
  ambiguity**: a name with exactly one definition keeps its current writable +
  "constrained" behavior; a name with 2+ definitions demotes name-only /
  dynamic / textual matches to uncertain / non-writable. If NO reference of an
  ambiguous name can be safely bound, it MUST fail closed (no staged write). It
  MUST NOT regress `batch_rename_updates_definition_and_callers` /
  `batch_rename_bumps_definition_and_call_site`. (US1)
- **FR-002**: `batch_rename` trust labels MUST reflect actual binding confidence;
  a "constrained" label MUST NOT be applied to name-only matches of an
  index-ambiguous name. (US1)
- **FR-003**: `detect_impact` MUST seed the changed-symbol set only from
  genuinely added/modified/removed symbols (body-level delta), not from every
  symbol of a changed file. This requires a **base-ref symbol source** (reuse
  `diff_symbols`' git-blob reparse) since the live index holds only current
  bodies — it is net-new machinery, not a seed tweak. (US2)
- **FR-004**: `detect_impact` blast nodes MUST carry disambiguating identity
  (path/kind) and MUST NOT link a call to unrelated same-bare-name definitions,
  **while preserving the existing cross-module edge** asserted by
  `compute_impact_reaches_across_qualified_module_call` — i.e. keep a bare-name
  fallback when the callee definition carries no resolvable owner. Entry-point
  tagging MUST NOT collapse distinct `main` symbols (tested only against a
  fixture that actually contains a reachable duplicate `main`). (US2)
- **FR-005**: `analyze_file_impact` MUST include same-file callers and resolve
  callers with typed (not bare-name) identity, preserving 018's parent-type
  narrowing. (US2)
- **FR-006**: `search_symbols`, `search_text`, `find_references` MUST accept a
  selector matching the bound project and refuse only foreign/over-broad
  selectors, consistently for both `project` and `projects` (no regression of
  the P2-10 `projects=["*"]` under-refusal). (US3)
- **FR-007**: Python relative imports (`from .x import`, `from ..x import`) MUST
  be captured and resolved against the importer's package for
  `find_dependents`. (US3)
- **FR-008**: An identical idempotent `symforge_edit` replay MUST return the
  stored result without writing bytes, without being rejected by a now-stale
  `if_match`; genuine concurrency/conflict protection MUST remain. (US4)
- **FR-009a**: The watcher MUST NOT go permanently stale after cold-start
  indexing — its generation MUST be re-synced with (or captured after) the
  fire-and-forget cold-start reload, so tracked edits are indexed, not
  `GenerationMismatch`-removed. The generation fence for genuine concurrency
  races MUST remain. (US5b)
- **FR-009b**: Health repair counts MUST count only successful reindex/remove and
  MUST NOT count generation-mismatch no-ops as repairs. (US5a)
- **FR-009c**: Daemon `status(reset_calibration=true)` MUST reset the
  proxy-owned durable calibration store (not silently no-op while reporting
  success). (US5c)
- **FR-010**: `validate_file_syntax` MUST accept valid UTF-8-BOM JSON and MUST
  either honor or remove the `estimate` flag, while preserving JSONC tolerance
  and correct rejection of genuinely malformed input. (US6)

### Key Entities

- **Resolved symbol identity** — the (project, path, language, kind, qualified
  owner, name, duplicate-ordinal) tuple that distinguishes one definition from a
  same-named other. Central to US1, US2, US3; today the graph `SymbolId` carries
  only `{path, name, kind}`.
- **Changed-symbol delta** — the body-level added/modified/removed set that must
  replace "all symbols of a changed file" as the impact seed. (US2)
- **Binding confidence** — identity-constrained vs name-derived-uncertain;
  gates both what `batch_rename` writes and what trust label it emits. (US1)

## Success Criteria *(mandatory)*

- **SC-001**: `batch_rename` on a name with **2+ unrelated owners** writes zero
  unrelated bytes and labels the unbound sites uncertain, in every supported
  language fixture (Rust, Python, TS, one of Go/Java/C++), with the **Python arm
  mandatory-gating** (that is where the escape was measured). A **unique**-name
  rename still writes and labels "constrained" as before (green regression tests
  pass). The struct-rename honeytrap shape (renaming a unique struct and
  asserting an untouched-by-construction method) is explicitly forbidden as the
  oracle. (US1)
- **SC-002**: `detect_impact` satisfies BOTH: (1) the leaf edit returns exactly
  one changed symbol and one hop-1 caller and a comment-only shift returns zero
  (seed fix); AND (2) on a **two-unrelated-same-name** case, changing one leaves
  the other out of the blast and every blast node carries path/kind (edge fix).
  A green result on (1) alone does NOT satisfy SC-002. (US2)
- **SC-003**: A matching-local selector succeeds and a foreign selector is
  refused across all three tools and both selector parameters; the Python
  relative-import dependent is found and a same-stem module is excluded. (US3)
- **SC-004**: Identical idempotent `symforge_edit` replay returns the stored
  result with zero writes; changed-request conflict and new-key concurrency still
  fire. (US4)
- **SC-005a**: After cold start, a tracked edit is indexed (not
  `GenerationMismatch`-removed). (US5b)
- **SC-005b**: Reconcile generation-mismatch no-ops contribute zero to the health
  repair count. (US5a)
- **SC-005c**: Daemon `status(reset_calibration=true)` clears durable calibration
  to `deferred`. (US5c)
- **SC-006**: Valid BOM JSON validates `ok`; JSONC still `ok`; malformed still
  fails. (US6)
- **SC-007**: The full gate (`fmt --check`, `check`, `clippy -D warnings`, full
  test suite `--test-threads=1`, release build, npm tests) is green, plus the
  embed no-default-features check, with a fail-first regression test for every
  story, AND every named must-not-regress test still green
  (`batch_rename_updates_definition_and_callers`,
  `batch_rename_bumps_definition_and_call_site`,
  `compute_impact_reaches_across_qualified_module_call`, the risk-tier tests,
  the JSONC tests). (all)

## Scope boundaries

- **In scope**: US1, US2, US3, US4, US5b, US5a, US5c, US6 (verified real defects,
  including the two added by adversarial review), at verified severity.
- **Out of scope**: P0-04 meta (verified non-bug); the report's refuted claims
  (JSON tree-sitter root cause, insertion whitespace — the "permanent watcher
  staleness" claim is now IN scope as US5b after the review corrected my
  refutation); the full "typed identity end-to-end" refactor beyond what US1–US3
  need; economics/schema-tax work (surface-tax, compact-default, receipt
  compaction) and the pure contract-tightening `what_changed` status/rename gap
  (paths returned are correct) — real report themes but not correctness/safety,
  deferred to a separate spec and recorded in `research.md` so they are not read
  as verified-clean.
