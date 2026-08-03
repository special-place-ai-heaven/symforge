# Implementation Plan: Admission Coverage Honesty

**Feature**: `specs/021-admission-coverage-honesty/` | **Date**: 2026-07-28 | **Spec**: [spec.md](spec.md)

**Input**: [spec.md](spec.md), [research.md](research.md), and the read-only defect ledger
`E:\project\testpilot\.scratch\symforge-dogfood-issues-2026-07-27.md`.

**Inherited prerequisite**: `specs/020-repository-knowledge-index/sift/tasks.md` Phase 9,
`T062`–`T082` (`SIFT-WS5`). Referenced by ID, never restated. One amendment only — see
[research.md](research.md) §1 and task **T101**.

## Summary

Close all nine `SF-DOG` findings plus the index-identity defect by making four claims true that the
surface currently makes falsely:

1. **"I looked there."** A search or plan that did not inspect a file says so, and names why.
2. **"This is the reason."** An exclusion reason is the actual disqualifying condition, once.
3. **"This is your file."** A path-shaped target resolves to that path or to nothing — never to an
   unrelated symbol that happens to share its extension.
4. **"This is your index."** A response describing an index carries the identity of the index that
   answered it.

Five native workstreams plus one inherited:

| WS | Story | Owns |
|---|---|---|
| `ACH-01` | US1 | `SF-DOG-007` — fail-closed path-shaped targets in `edit_plan` |
| `ACH-02` | US2 | `SF-DOG-006` — Tier-2 selector dispatch or explicit refusal |
| `ACH-03` | US3 | `SF-DOG-009` — one admission oracle, typed stale-generation, honest size/generation |
| `ACH-04` | US4 | `SF-DOG-008` — untracked/excluded truth and a `new_file` contract that is real |
| `ACH-05` | US5 | index identity — `/health` stops answering for another project |
| `SIFT-WS5` | US6 | `SF-DOG-001`…`005` — **inherited**, `T062`–`T082` |

## Technical Context

**Language/Version**: Rust 2024 edition, single crate `symforge` (8.16.6).

**Primary Dependencies**: `regex` 1.11 (secret detector), `rmcp` (MCP protocol), `tree-sitter`
(parsing), `dunce` (path canonicalization), `axum` (sidecar HTTP), `tempfile` + `tokio` (tests).
**No new dependency.** The root-cause fix tightens an existing regex; the identity fix collapses
three existing comparators into one.

**Storage**: In-process `LiveIndex` + `.symforge/` snapshots + `.symforge/sessions/` descriptors.
`ACH-05` changes how descriptors are **read and validated**, not their format.

**Testing**: `cargo test --all-targets -- --test-threads=1` for the gate; focused
`cargo test --test edit_plan_literal_path_precedence`, `cargo test --lib knowledge::`,
`cargo test --lib store`, `cargo test --lib health_view`, `cargo test --lib sidecar::` during
development. New integration test files under `tests/`.

**Target Platform**: Windows 11 primary (this workstation, where the `\\?\` prefix asymmetry is
live); Linux/macOS via CI.

**Project Type**: MCP server (single Rust crate, library + binary + sidecar HTTP surface).

**Performance Goals**: No new asymptotic cost. The tightened regex is the same single-pass scan.
`ACH-01`'s guard **removes** a full `all_files()` cascade walk on the path-shaped-miss path.
`ACH-03`'s oracle reorder replaces a linear `manifest_entries` scan with a `files` hash lookup on the
hit path. `ACH-05` adds one liveness probe per endpoint resolution (already the cheaper alternative to
returning a dead port and timing out).

**Constraints**:

- **Frozen security invariant, verbatim**: *a bounded lexical fallback must NEVER read files excluded
  for `SensitivePath` or `SensitiveContent`. Feature 020's security contract governs.* This is
  `FR-023` of the sift slice and is not reopened. Correcting the over-broad rule removes false
  positives; it does not relax the invariant for true positives.
- **PR #479 footprint** (as of 2026-07-28): `.github/workflows/ci.yml`, `Cargo.toml`,
  `src/protocol/edit_tools.rs`, `src/protocol/tools.rs`, `src/server/serve.rs`, `tests/serve_port.rs`,
  `tests/stel_symforge_edit.rs`. `ACH-04` is **gated** on #479 landing. `ACH-02` is deliberately
  routed through `src/protocol/format.rs` so it needs no `tools.rs` edit.
- **Measurement channel**: every before/after number comes from the MCP `status` route (proven
  consistent at `891`/`25050` across four sessions), never from the PostToolUse hook — the surface
  `ACH-05` exists to fix.
- **Disk**: `df -h /e | tail -1` before and after any `cargo` invocation; abort below 6 GB.
  `cargo clean` after any heavy gate (repo `CLAUDE.md` Windows disk rule).
- `strip_qualification` (`src/live_index/disambiguation.rs:438-442`) must not change.
- Do not edit the read-only ledger, and do not touch `E:\project\symforge-pr479`.

**Scale/Scope**: 891 indexed files, 25050 symbols. **19 of 167 `src/**/*.rs` files are currently
withheld** by the root cause; the non-Rust blast radius is unmeasured and is task **T102**.
Eight source files change natively (`src/protocol/edit_plan.rs`, `src/protocol/format.rs`,
`src/watcher/mod.rs`, `src/sidecar/handlers.rs`, `src/live_index/health_view.rs`,
`src/sidecar/port_file.rs`, `src/cli/hook.rs`, and — after #479 — `src/protocol/tools.rs`), plus
whatever `T062`–`T066` touch in the prerequisite (`src/knowledge/mod.rs`, `src/domain/index.rs`,
`src/live_index/store.rs`).

## Constitution Check

*GATE: passed before Phase 0; re-evaluated after design (bottom of this file).*

| Principle | Assessment |
|---|---|
| **I. Local-First In-Process Index** | PASS — no new store. `ACH-03` changes which of two **existing** in-index records is authoritative. `ACH-05` changes descriptor validation, not persistence. |
| **II. MCP-Native Surface** | PASS — every change is the behavior of an existing tool (`edit_plan`, `get_file_content`, `analyze_file_impact`, `search_text`, `status`) or of the sidecar HTTP endpoints the hook already calls. One tool **description/schema** may change (`analyze_file_impact`'s `new_file`, FR-017) — that is a truthfulness fix, not a new surface. No chat injection. No new tool. |
| **III. Trust Envelopes** | PASS, and this feature **is** principle III. Every finding is an envelope that lied: an unqualified negative (`SF-DOG-001`/`008`), a false reason (`004`), a false mode assertion (`006`), a false absence (`009`), or a foreign index's counts (identity). `FR-021`/`FR-011` reuse the existing trust-envelope mechanism rather than inventing a second one. |
| **IV. Determinism & Recovery** | PASS — `classify_stable_content` is already a pure function of file bytes, so the corrected rule is equally deterministic. `ACH-01`'s guard is a pure predicate. `ACH-03` **adds** recoverability: a stale-generation result with a retry instruction replaces a terminal-looking refusal. |
| **V. Frecency Invariant** | PASS — no read path in this feature writes frecency. `ACH-01` removes a walk; `ACH-02` adds only rendering; `ACH-05` adds only validation. |
| **VI. Embed Isolation (G-045)** | ATTENTION — `ACH-05` touches `src/sidecar/*` and `src/cli/hook.rs`, which are server-side. The shared root comparator (FR-023) must live where it does **not** pull a server dependency into `embed`. Gated by `cargo check --no-default-features --features embed` (task **T171**). |
| **VII. Transport Parity** | ATTENTION — `ACH-03` and `ACH-04` change `src/sidecar/handlers.rs`, the `serve` side. The equivalent stdio path must receive the same honest reason, size, and generation. Task **T141** asserts parity explicitly rather than assuming it. |
| **VIII. Verification Before Done** | Enforced by the task list: every finding carries RED → GREEN → VERIFY, each VERIFY names a runnable command **and** an assertion that fails without the fix (FR-026). Undetermined causes get an investigation task with a recorded output **before** any fix task. |

**Two ATTENTION items (VI, VII), no violations.** Both are gated by explicit tasks rather than
assertion. Complexity Tracking omitted — nothing to justify.

## Project Structure

### Documentation (this feature)

```text
specs/021-admission-coverage-honesty/
├── spec.md        # Ownership, stories, FRs, success criteria
├── plan.md        # This file
├── research.md    # Phase 0: the measured root cause, the code chains, open questions + owners
└── tasks.md       # T100-T174
```

No `data-model.md`: this feature adds no new persistent structure. The two new types
(`ReindexResult::StaleGeneration`, the `HealthResponse` identity stamp) are described inline in
`FR-009`/`FR-020` and in their GREEN tasks; a separate document would only duplicate them.

No `contracts/`: 021 implements against Feature 020's **existing frozen** contracts. Adding a
contract file would falsely imply a new surface — the surface is unchanged and only its truthfulness
is being brought into compliance.

### Source Code (repository root)

```text
src/
├── protocol/
│   ├── edit_plan.rs          # ACH-01 (the whole fix)  :90, :103-121, :132-136
│   ├── format.rs             # ACH-02 (the whole fix)  :3176-3225, :3305, :3497-3526, :3658-3686
│   └── tools.rs              # ACH-04 — BLOCKED on PR #479  :2184, :2191-2198, :2336-2365, :2410-2421, :3316-3323, :3351
├── watcher/mod.rs            # ACH-03 :352-359/:539/:566-573/:576-579 · ACH-04 :277-284
├── live_index/
│   ├── health_view.rs        # ACH-03 :275-309 (oracle order)
│   ├── query.rs              # ACH-01 read-only source (:1243-1256); likely NO change
│   └── disambiguation.rs     # DO NOT CHANGE (:438-442)
├── sidecar/
│   ├── handlers.rs           # ACH-03 :886-939/:958/:1113 · shared size :905/:920/:934 · ACH-05 :88-93/:344-346/:375-388
│   └── port_file.rs          # ACH-05 :23/:62-64/:234-241/:283-289/:337-348/:495-505
└── cli/hook.rs               # ACH-05 :255/:1041-1058/:1083-1091

tests/
├── edit_plan_literal_path_precedence.rs   # ACH-01 — extend the existing SF-AAP-001 regression
├── file_content_tier2_selection.rs        # ACH-02 — new
├── impact_admission_consistency.rs        # ACH-03 — new
├── untracked_admission_truth.rs           # ACH-04 — new (after #479)
└── sidecar_identity_guard.rs              # ACH-05 — new

Inherited (SIFT-WS5, T062-T082, NOT this plan's diff):
  src/knowledge/mod.rs · src/domain/index.rs · src/live_index/store.rs · src/discovery/mod.rs
```

**Structure Decision**: existing single-crate layout, unchanged. No module is added. Four new
integration test files, one existing test file extended. `src/live_index/query.rs` is a **read-only
lookup source** for `ACH-01` — `metadata_only_skipped_paths()` already returns `(path, reason)` on the
same `impl` that `plan_edit` borrows `all_files()` from, so no new API is needed. Add an
`is_metadata_only(path)` helper there **only** if the linear scan proves hot.

## Implementation order and rationale

**Sequencing is causal, not cosmetic.** What unblocks what, and why:

| Order | WS | Why here |
|---|---|---|
| **1** | Setup + investigations (`T100`–`T106`) | Two causes are **undetermined** (`Q1`, `Q2`) and one hygiene question is open (`Q3`); two design questions need an owner (`D1`, `D2`). A plan that pretends to know an unknown cause is worse than one that names it. Also: this phase measures the demotion blast radius, which becomes the SC-015 baseline, and pins the measurement channel to MCP `status` before any number is quoted. |
| **2** | Shared size rendering (`T107`–`T109`) | `0.0 MB` is one bug appearing in **four** findings (`SF-DOG-002`, `004`, `008`, `009`). One shared task, landed early, so every later VERIFY can assert against a correct size instead of re-litigating it. Two-line fix, zero dependencies. |
| **3** | `ACH-01` — `SF-DOG-007` (`T110`–`T118`) | **The MVP, and deliberately first.** It is the only finding that causes a **wrong write**: an agent following the plan edits an unrelated file. Its fail-closed fix depends on nothing — the predicate exists at `edit_plan.rs:90` and the Tier-2 lookup exists at `query.rs:1243-1256` — so it does **not** wait on the admission root cause and **must be schedulable immediately**. It is also outside PR #479's footprint. |
| **4** | Prerequisite gate — `SIFT-WS5` (`T119`–`T121`) | **`SkipReason::UnsupportedLanguage` is a ≥11-way catch-all** (`store.rs:3360-3366`, `:3376-3380`, `:3673`). While every cause reports as one string, no test can assert *which* cause fired — so `ACH-03` cannot distinguish a real policy exclusion from `reason: None`, and `ACH-04` cannot make the receipt and the search response name the same reason. `T062`–`T065` split the reason codes; `T066` fixes the root cause they make observable. **This is the honest-reason-codes → root-cause → downstream-truth chain, and it is why `SF-DOG-004` (rated LOW) is scheduled ahead of two HIGHs.** |
| **5** | `ACH-02` — `SF-DOG-006` (`T122`–`T131`) | Independent of everything, but placed after the gate for a **test-validity** reason: once `T066` restores 19 accidentally-demoted files to Tier 1, a fixture chosen from that set would stop exercising the Tier-2 path and the test would pass vacuously. Choosing the fixture *after* the gate guarantees it is Tier 2 by deliberate policy. |
| **6** | `ACH-03` — `SF-DOG-009` (`T132`–`T142`) | Needs the honest reason codes (step 4) to tell a real exclusion from `reason: None`. Must precede `ACH-04` because `ACH-04`'s honest `new_file` refusal has to distinguish "the gate refused this" from "you lost a publication race" — which only exists once `FR-009`'s typed variant lands. Also establishes the single authoritative oracle that `ACH-04`'s admission-aware diagnostic then reads. |
| **7** | `ACH-04` — `SF-DOG-008` (`T143`–`T154`) | **Hard-blocked on PR #479 merging** — its fix sites are almost entirely in `src/protocol/tools.rs`. Depends on step 4 (the mislabel *is* the reason collapse), step 6 (the oracle and the race variant), and its own investigation `Q1`. Last of the code-truth workstreams by necessity, not by preference. |
| **8** | `ACH-05` — index identity (`T155`–`T167`) | Technically independent — a disjoint file set (`sidecar/*`, `cli/hook.rs`) that nothing else here touches. Placed last for review size, **not** because it is low value: it corrupted this very investigation's inputs. The risk of deferring it is managed in step 1 by binding every measurement to the MCP `status` route, which was proven clean. |
| **9** | Polish + full gate (`T168`–`T174`) | Includes `D7`: re-run the `SF-DOG-001` reproductions after the demotion is fixed, because some "search silently misses" reports may collapse into that root cause rather than being separate defects. |

**`ACH-01` is stated independent on purpose.** It is the only wrong-write defect in the ledger, and
nothing in this feature or in `SIFT-WS5` gates it. If only one workstream ships, it is this one.

## Design decisions

Evidence, code chains, and open questions in [research.md](research.md).

### D1 — Full-file demotion vs. per-range suppression *(owner ruling required, task `T104`)*

Today one detector finding discards a whole file's symbols and its owned byte buffer before parsing
(`src/knowledge/mod.rs:318-328`). Even with a corrected rule, a file containing one **genuine** secret
still loses all its code intelligence. `FR-023`/`T070` freezes "security dispositions must never be
lexically read" — the open question is whether that also forbids indexing the file's *other* symbols.

**Recommendation to the owner**: keep full-file demotion for this feature. The measured population of
genuine secrets in demoted files is **zero out of 29 findings**; per-range suppression is therefore
solving a hypothetical at the cost of a much larger security-sensitive change. Record the decision;
if per-range is wanted, it is a separate feature. `T104` must produce a recorded answer either way —
`ACH-02`'s and `SIFT-WS5B`'s fallback scope both depend on it.

### D2 — `around_symbol` on a Tier-2 file *(owner ruling required, task `T105`)*

Without an index there is no symbol range. Two options: an explicit structured refusal naming the
file as metadata-only, or degrade to a text search for the symbol name (i.e. treat it as
`around_match`).

**Recommendation to the owner**: **explicit refusal.** It is fail-closed, it is what the ledger's
"structured refusal instead of unrelated content" asks for, and the substitute silently changes mode
semantics — which is the exact class of defect `SF-DOG-006` is. A caller who wants the text search can
ask for `around_match` explicitly. Record the ruling in `T105`; `T126` tests whichever is chosen.

### D3 — The path-shaped predicate must not be "contains a dot"

`edit_plan.rs:90` currently reads `target.contains('/') || target.contains('.')`. As a **suffix-match
gate** that is harmless. As a **cascade veto** it would regress legitimate selectors: `Type.Method`
(Go receiver) and `Foo::bar` contain a dot / colons and **must** still reach `find_candidates_cascade`.

**Decision**: the veto keys on a path separator **or** a known-file-extension tail — not on any dot.
Tested in **both** directions (`T110`/`T112`), because getting this wrong trades a wrong-write bug for
a Go/C++ resolution regression.

### D4 — Fix `render_file_content_bytes` without changing its signature

`ACH-02`'s primary site is `src/protocol/format.rs:3176-3225`. The lazy and safe move is to **add
selector branches** there rather than change the signature, because the full caller set was not
enumerated (`D4` in [research.md](research.md) §7 — `file_content_view` at `format.rs:3110` and the
resources render path are suspected additional callers). `T122` enumerates callers **before** any
signature change is considered.

The reusable cores are extracted from `render_numbered_around_match_excerpt`
(`format.rs:3497-3526`) and `render_numbered_chunk_excerpt` (`:3305`) by splitting the
`&IndexedFile` coupling into `(path: &str, lines: &[&str], …)` functions, keeping the `IndexedFile`
wrappers for the existing callers. The existing refusals (`not_found_file_match` at `:3508`, the
occurrence-not-found message at `:3518-3522`) are **already exactly** what the ledger's acceptance
checks demand — they only need to become reachable from Tier 2. The dispatch order mirrors
`file_content_from_indexed_file_with_context` (`:3061-3100`: chunk → `around_symbol` →
`around_match` → bytes) so the two branches cannot drift again.

**Consequence worth stating**: `src/protocol/tools.rs:8607` and the misleading `mode_annotation` at
`:8618-8621` need **no edit**. That is what keeps `ACH-02` clear of PR #479. If it turns out a
`tools.rs` edit is genuinely required, `T131` flags and sequences it after #479 — it does not land on
a blocked file.

### D5 — Make the live `files` record the admission authority

`capture_admission_tier_lookup_view` (`src/live_index/health_view.rs:275-309`) checks
`manifest_entries` **first** (`:280-296`) and falls back to `files` (`:297-306`). So impact answers
from a possibly-stale terminal manifest disposition while `get_file_context` answers from the parsed
`files` record — one index, two oracles, and the stale one wins in impact.

**Decision**: the live `files` record is authoritative when present. Where both are present and
disagree, return both and **flag the disagreement** rather than silently preferring either — a
disagreement is diagnostic information, and hiding it is how this defect stayed invisible.
`get_file_context` and `analyze_file_impact` must then be unable to report different tiers for one
path in one generation (`FR-011`, `SC-006`).

### D6 — A lost race is a distinct outcome, not an admission verdict

`ReindexResult::Skipped` is returned from ≥8 sites, four of which are optimistic-concurrency losses
(`src/watcher/mod.rs:352-359`, `:539`, `:566-573`, `:576-579`). Both impact branches
(`src/sidecar/handlers.rs:958`, `:1113`) route that one variant into `impact_skipped_text`, which is
written for admission refusals and **default-fills** what it lacks (`reason: None` → `"policy"`).

**Decision**: add `ReindexResult::StaleGeneration { expected, observed }` and handle it separately with
an explicit retry instruction. This is what turns "the next identical call succeeded" from a mystery
into a documented, recoverable outcome — and it is what `ACH-04` needs in order to tell an honest
`new_file=true` refusal apart from a race.

### D7 — One root comparator, not three

Three functions answer "are these the same project root" with three semantics:
`same_root_identity` (`src/sidecar/port_file.rs:337-348`, no canonicalization), `roots_match`
(`src/sidecar/handlers.rs:375-388`, `dunce` **strips** `\\?\`), and `normalize_path_for_match`
(`src/cli/hook.rs:1083-1091`, fed `std::fs::canonicalize`, which **adds** `\\?\`). The daemon emits
`//?/E:/project/symforge`; on-disk descriptors store `E:\\project\\symforge`.

**Decision**: collapse to one shared comparator used by all three call sites (`FR-023`). It must live
somewhere that does not pull a server dependency into the `embed` build — Constitution VI, gated by
`T171`. This did not cause a mismatch in the observed session only because `same_root_identity`
compares CWD-vs-descriptor (both plain); it becomes live the moment a descriptor is written from a
canonicalized root.

### D8 — Gate the payload, not the endpoint

`caller_root_guard` exempts `/health` and `/stats` for a legitimate reason recorded in its own comment
(`src/sidecar/handlers.rs:342-343`): "liveness probes and the hook's fail-open target must never 409".
Removing the exemption would break the hook's fail-open path.

**Decision**: keep liveness answerable without identity, but gate the **index-describing fields**
(`file_count`, `symbol_count`, `index_state`) on the `caller_root` check, or return the mismatch in
band. Combined with the `HealthResponse` identity stamp (`FR-020`), a caller can then detect a
substitution even on a fail-open response. This is the smallest change that closes the reproduction
without regressing the reason the exemption exists.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| The cascade veto (`ACH-01`) regresses `Type.Method` / `Foo::bar` selector resolution | `D3`: key on `/` or a known-extension tail, never any dot. Bidirectional tests `T110`/`T112`; `SC-003` requires byte-identical output for the legitimate selectors. |
| `ACH-02`'s fixture is chosen from the accidentally-demoted set and goes vacuous when `T066` lands | Fixture selected **after** the prerequisite gate, and required to be Tier 2 by deliberate policy (lockfile / oversized data). Stated in `T123`. |
| Tightening the secret rule (prerequisite `T066`) creates a **false negative** — a real secret now missed | The corrected rule adds a left word boundary and constrains the value class; it does not remove a rule. `T066` must keep the existing detector unit tests green, and its canary fixtures (`password={canary}`, `token={canary}`) must still be recognized as **placeholders**, not as clean-because-unmatched. Distinguish "correctly identified as a placeholder" from "no longer matched at all". |
| `ACH-04` blocked indefinitely if PR #479 stalls | `T144` is an explicit gate. Everything else in the feature is independent of `tools.rs`, so `ACH-04` is the only thing that waits. Do not work around the footprint. |
| Transport parity drift: `ACH-03`/`ACH-04` fix the `serve` path in `handlers.rs` and leave stdio lying | `T141` asserts parity explicitly for reason, size, and generation rather than assuming shared formatters cover it (Constitution VII ATTENTION). |
| `ACH-05`'s shared comparator pulls a server dependency into `embed` | `T171` (`cargo check --no-default-features --features embed`) is a hard gate (Constitution VI ATTENTION). Place the comparator in a dependency-free module. |
| Windows disk pressure from repeated full gates | Focused per-workstream test commands during development; the full gate once per phase close-out; `df -h /e` before/after every `cargo`, abort below 6 GB; `cargo clean` after heavy runs. |
| Measuring success on a hook that reports another project's index | Every number in this feature comes from MCP `status` (`T100`), never the hook, until `ACH-05` lands. |
| An investigation task "concludes" without evidence and a fix is built on a guess | `T103`, `T132`, `T143`, `T155` each require a **recorded output** in their receipt. A fix task whose investigation has no recorded finding does not start. |

## Out of scope for this plan

Re-owning `T062`–`T082`; per-range secret redaction (unless `T104` rules for it, in which case it is a
new feature); relocating the repository's detector canaries into external fixtures; the remaining
ledger **observations awaiting isolated reproduction** (lines 613–881) — notably the `batch_edit`
double-semicolon report, the per-delegated-agent empty-index cost, the `project:` selector
discoverability mismatch, `degraded[ObservationFailed]` manifest freshness, and
`atomic_durability_unavailable` curation; any change to `E:\project\testpilot` or the ledger; and the
git-worktree identity case, which is mechanically plausible via
`src/sidecar/port_file.rs:501` but **unreproduced** because that worktree belongs to another agent.

## Constitution re-check (post-design)

Re-evaluated against the eight decisions above. No decision introduces a second index (I) — `D5`
selects between two existing records. No decision adds a non-MCP surface (II) — `FR-017` may correct a
tool description, which is the opposite of adding surface. `D5`, `D6`, and `D8` each **strengthen**
III by replacing a default-filled or foreign-sourced claim with an evidenced one. `D3`, `D4`, and `D7`
are pure predicates and comparators, so IV holds and `D6` adds a recovery path. No read path gains a
frecency write (V). `D7` carries an explicit `embed` gate (VI) and `D5`/`D6` carry an explicit parity
gate (VII); both are ATTENTION items with owning tasks rather than assumptions. VIII is enforced
per-finding by RED → GREEN → VERIFY with investigation-first tasks for the three undetermined causes.

**PASS — no violations; two gated ATTENTION items (VI, VII); no complexity to track.**
