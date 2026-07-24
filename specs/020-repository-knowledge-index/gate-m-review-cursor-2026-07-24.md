# Cursor Review Prompt — SymForge Feature 020, Gate M (M-001/M-002 health surfacing)

You are performing an **adversarial code review** of an in-progress change on the
branch `feat/repository-knowledge-index` in `E:\project\symforge` (a Rust MCP server).
Your job is to **find defects and contract violations and report them** — do **not**
rewrite or "fix" broadly. Produce findings; the maintainer decides what to act on.

## Ground rules

1. **The frozen SpecKit contracts are the authority.** If code disagrees with a frozen
   contract, the *code* is wrong — never propose weakening a contract to fit code. Read
   before judging:
   - `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`
     — the **"Health contract"** section (lines ~232-251) and contract tests 18/19 are
     the primary authority for this change.
   - `specs/020-repository-knowledge-index/data-model.md`
     — `ManifestResourceUsage` (~650-666, esp. "In-flight/peak/derived-state usage is
     operational health state"), `FreshnessStatus`/`FreshnessReason` (~595-612),
     `CoverageStatus` (~587-593), digest exclusions (~709-717, "watcher receipt and
     retry counters").
   - `specs/020-repository-knowledge-index/tasks.md` — **Gate M** (M-001, M-002, M-003).
2. **Code is gospel, docs are testimony.** Verify claims against source, not comments.
   Cite every finding as `path:line`.
3. **Report by severity**: BLOCKER / HIGH / MEDIUM / LOW, most-severe first. Each finding:
   one-line claim, concrete failure scenario (inputs → wrong output), `file:line`, and the
   contract/task ID it violates. **Empty is a valid answer.**
4. **Hard constraint (M-003):** the change must NOT alter the tool surface. Full surface
   stays exactly **39** tools, compact stays **3**. Everything here *extends the existing
   `health`/`health_compact` report strings* — flag any new `#[tool]`, `SYMFORGE_TOOL_NAMES`
   edit, or alias change as a BLOCKER.
5. **Secret-safety (project rule):** flag any path where a health line could emit a raw
   filesystem path, token, or other secret. Health may use the safe project ID and typed
   reason codes only.

## What was built this session (the review surface)

M-001/M-002: surface already-published Feature 020 evidence + source-binding/runtime state
into the `health` and `health_compact` reports. **No new tool; no new upstream computation
— every field reads data that already existed after Gates A-L.**

New/changed code:

- **`src/live_index/store.rs:194`** — new `pub fn configured_inflight_byte_budget() -> u64`
  (returns `InflightByteBudget::from_env().total`; the configured in-flight admission ceiling).
- **`src/protocol/format.rs`** (new block ~1975-2410):
  - `pub struct SourceBindingHealthView<'a>` (`:1980`) — the reachable binding view.
  - `pub fn format_repository_knowledge_health(...)` (`:2341`) — full section.
  - `pub fn format_repository_knowledge_health_compact(...)` (`:2377`) — 2-line compact.
  - private helpers: `placement_authorization`/`authorization_label` (`:2026`,`:2071`),
    `persistence_label` (`:2119`), `replay_label` (`:2141`), `render_source_binding_line`
    (`:2090`), `manifest_coverage_digest` (`:2131`), `render_manifest_line` (`:2156`),
    `render_disposition_line` (`:2181`), `render_source_entry_line` (`:2215`),
    `render_source_set_section` (`:2237`), `render_bridge_line` (`:2274`),
    `render_temporal_line` (`:2285`), `render_authority_hygiene_line` (`:2324`),
    `derived_coverage_label`, `coverage_label`, `freshness_label`, `policy_status_label`,
    `capability_unavailable_code`, `short_digest`.
- **`src/protocol/tools.rs`** — `health_for_runtime` (`:6738`) and `health_compact_for_runtime`
  (`:6913`) each capture `session_is_daemon = session_id.is_some()` before the id is moved,
  then append the section from `self.index.published_source_set()` +
  `capture_state_placement()` + `*persistence_health.read()` + `runtime_status.session_id`.
- Tests: `src/protocol/format/tests.rs` (`m002_terminal_disposition_…`,
  `m002_budget_degraded_…`, helper `m002_tiny_manifest`); `src/protocol/tools.rs` tests
  (`m001_health_surfaces_repository_knowledge_section_full_and_compact`,
  `m002_health_reflects_unbound_to_bound_transition`,
  `m002_health_shows_per_session_protected_membership`,
  `m002_post_bind_durability_degradation_is_independent_of_query_readiness`).

Data flow: `PublishedSourceSet` (`store.rs:885`) → `current_generation()` (`:892`) →
`PublishedGeneration` (`store.rs:854`) fields `manifest` / `bridge` / `authority` /
`code_signals` / `freshness`. Binding state is on the per-session `SymForgeServer`
(`src/protocol/mod.rs:159`): `repo_root`, `state_placement`, `persistence_health`.

## Specific things to scrutinize (highest value — 3 judgment calls)

These are the places I made a call under the "surface existing data, do not build new
plumbing" constraint. Confirm or reject each.

1. **Authorization derived from placement, not from the authoritative `SourceAccessMode`.**
   `placement_authorization` (`format.rs:2026`) maps `StatePlacement::ProjectLocal` →
   `normal`, `UserLocal{ExplicitProtected}` → `explicit_protected`,
   `UserLocal{ProjectLocalUnavailable}` → `normal`, and **`MemoryOnly` → `indeterminate`**.
   The authoritative `SourceAccessMode` (`domain/index.rs:579`) lives on the daemon
   `ProjectInstance` (`daemon.rs:253`) and is **not** plumbed to the health render site
   (`SymForgeServer` has no access-mode field).
   - Q: The source-binding "Health contract" requires `authorization: normal | explicit_protected`
     reported independently. Is placement-derivation an acceptable source of truth, or does
     the contract demand the real `SourceAccessMode` be threaded
     `ProjectInstance → SessionRuntime → health_for_daemon_session`?
   - Q: Is the `MemoryOnly` → `indeterminate` case a real correctness hole? Can a *normal*
     source legitimately land in `MemoryOnly` (both tiers fail) AND can an *explicit-protected*
     one (user-local fails) — so the two are genuinely indistinguishable from placement alone?
     If so, is `indeterminate` honest, or does the contract forbid it?

2. **"retry" has no dedicated field.** The M-001 task line lists `retry`. There is no
   retry-counter symbol in the published generation; data-model.md:713 explicitly excludes
   "watcher receipt and retry counters" from the manifest digest. I surface retry *indirectly*
   via `FreshnessStatus::Degraded{reason_codes}` (e.g. `ReconciliationPending`) rendered in
   `freshness=degraded[…]`, plus the pre-existing Watcher line's `reconcile repairs` counter.
   - Q: Is that the intended reading of "retry", or is there upstream retry state
     (scout / reconcile / admission) that exists and should have been surfaced?

3. **Bridge "version" proxied by `content_generation`.** `render_bridge_line` (`format.rs:2274`)
   emits `version=content<N>` using the enclosing `PublishedGeneration.content_generation`,
   because `KnowledgeBridge` (`knowledge_bridge.rs:193`) has no version field of its own.
   - Q: Is `content_generation` the right "bridge version", or is there a distinct
     bridge/rule version I should be reading?

## Broader adversarial pass (find anything else)

Beyond the three above, hunt for:

- **Contract-field completeness vs `source-binding-and-state.md` Health contract** (~232-251):
  it requires binding, authorization, state placement, persistence (healthy/degraded/disabled +
  reason codes), durable-replay availability, current-session membership authority + live
  replay postcondition, query readiness, watcher/freshness, snapshot load/identity, and
  reason-bearing capability statuses — each reported **independently**. Which required fields
  are missing, conflated, or mislabeled in the rendered strings? (I claim `Ready`-vs-durability
  independence is honored — verify `persistence_label`/`replay_label`/`query_readiness` can't
  co-vary incorrectly.)
- **`persistence_label` trichotomy** (`format.rs:2119`): I map `AtomicDurabilityUnavailable`
  and `DurableMutationReplayUnavailable` → `degraded`, everything else Unavailable → `disabled`.
  Is that split faithful to "healthy/degraded/disabled" semantics (was-durable-now-lost vs
  never-durable)? Any reason a mapping is backwards?
- **`replay_label`** (`format.rs:2141`): `available` iff placement is Project/User-local AND
  persistence `Available`. Does this over- or under-claim durable replay vs the contract's
  "durable replay availability"?
- **Truncation / unbounded output**: `render_source_set_section` caps at
  `MAX_HEALTH_SOURCES = 8`. Any other list that could grow unbounded in health
  (dispositions, authority records, freshness reason codes)?
- **`short_digest`** (`format.rs:1998`): ~~byte-slices to `min(len,12)`; non-ASCII source id
  would panic mid-codepoint~~ **RESOLVED this session** — now char-boundary-safe via
  `char_indices().nth(12)` (ASCII feeders unchanged, byte-identical). Re-confirm the fix is
  correct and no other health helper byte-slices an arbitrary `&str`.
- **Borrow/lifetime & lock discipline** in the two `tools.rs` sites: `rk_placement` is held
  as an owned local and borrowed into the view; `*self.persistence_health.read()` is copied.
  Confirm no lock is held across the `format_*` call and no `.read()`/`.write()` guard leaks.
- **Test honesty**: do the M-002 tests actually prove the invariant, or do they pass trivially?
  In particular `m002_health_shows_per_session_protected_membership` overrides
  `state_placement` post-construction to model explicit-protected — is that a faithful model of
  the L-R11 path, or does it sidestep the real membership check?

## Verification already run (for context, re-run if you doubt it)

`cargo fmt --all --check` clean; `cargo clippy -j1 --all-targets --features server -- -D warnings`
0 warnings; new tests 6/6 pass; `protocol::` slice 971/971; daemon membership 2/2.
Report findings that these gates would NOT catch (logic, contract, panic, secret-leak).
