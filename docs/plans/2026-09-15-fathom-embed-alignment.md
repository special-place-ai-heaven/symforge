# Fathom Embed Alignment Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship a complete, verifiable embedded knowledge-engine contract for Fathom: safe process-wide ownership, prompt cooperative shutdown, generation-consistent census/progress access, and full admitted repository-knowledge retrieval with relationships and coverage/provenance. CR-0 through CR-3 and Task 6 are one required, additive release: no existing public atom changes or disappears, and every added atom is recorded in the frozen public-API contract.

**Architecture:** Base the release branch on Fathom's verified `v11.2.0` dependency (`b549aa7`), not the divergent current branch. CR-0 makes the process runtime own the factory; CR-1 propagates the existing shutdown state through the binding worker, scout, reload, and publish boundary; CR-2 and CR-3 expose immutable published census data and monotonic per-reload progress without leaking internal types. `search_text` remains code search; the embed facade additionally exposes typed knowledge retrieval over the published repository-knowledge source set, preserving hit provenance, relationship evidence, authority/coverage, and withholding truthfulness.

**Tech Stack:** Rust 2024, std synchronization primitives, existing SymForge integration tests.

---

### Task 1: Align the Fathom release baseline

**Files:**
- Modify: `Cargo.toml` (only if the verified release version changes)
- Modify: `docs/research/fathom-embed-change-brief.md` (only to record resolved source provenance)
- Test: existing `cargo test --no-default-features --features embed --lib` gate

**Step 1:** Create the working release branch from `v11.2.0` (`b549aa7`) and prove its `Cargo.toml` version plus the cited worker-backed `src/index_lifecycle/embedded.rs` shape.

**Step 2:** Compare it with the current branch and carry only the Fathom brief, revised plan, task ledger, and lesson forward. Do not mix unrelated current-branch architecture into this release.

**Step 3:** Run the embed gate on the aligned source and record its result.

### Task 2: Prevent duplicate embedded opens process-wide (CR-0)

**Files:**
- Modify: `src/index_lifecycle/process_runtime.rs`
- Modify: `src/index_lifecycle/public_api.rs`
- Test: `tests/activation_cut_v11.rs` or the existing focused embed lifecycle test file

**Step 1:** Write a failing regression that acquires two `ProcessRuntimeApi` values, opens the same canonical root through the first, and requires the second to return `SelectionUnavailable`.

**Step 2:** Write a second failing regression where the first attempt panics after registration; prove the same root can subsequently open and that no registration leaks.

**Step 3:** Move the existing `EmbeddedSourceFactory` ownership to the process runtime, so each acquired API clones that shared factory. Preserve the current identity-checked drop guard and no-new-dependency behavior.

**Step 4:** Run both focused tests, then `cargo test --no-default-features --features embed --lib`.

### Task 3: Make close cooperative during embedded indexing (CR-1)

**Files:**
- Modify: verified worker-backed `src/index_lifecycle/embedded.rs`
- Modify: verified reload/scout owners, expected `src/live_index/store.rs` and `src/discovery/mod.rs`
- Test: existing embed lifecycle integration test file

**Step 1:** Add a process-wide, root-scoped test gate that can hold reload or scout work in embed-only tests.

**Step 2:** Write failing tests for closing during first load and refresh: close returns before gate release, no cancelled generation publishes, and `Stopping` is never overwritten by `Blocked` or `Current`.

**Step 3:** Thread the already-owned shutdown atomic through cancellable scout/reload paths; check before and after blocking admission and between derived-index stages. Return a typed internal cancellation result that publishes nothing.

**Step 4:** Make worker phase writes conditional on shutdown while holding their state mutex; use condition waits that predicate on stop, avoiding the lost-notify poll delay.

**Step 5:** Enforce and measure a one-second close bound against the agreed large-repository fixture. In the same gate, prove that opening an unrelated root completes within that bound while another root closes. Run the focused tests plus an embed gate that actually executes `tests/embed_bound_index.rs`, not only library tests.

### Task 4: Publish a generation-consistent index census (CR-2)

**Files:**
- Modify: `src/index_lifecycle/embedded.rs`
- Modify: `src/index_lifecycle/public_api.rs`
- Modify: `src/embed.rs`
- Modify: `src/lifecycle_identity.rs`
- Modify: frozen public-API contract/delta artifacts identified by the existing contract test
- Test: nearest embed lifecycle and public-API delta tests

**Step 1:** Write failing tests for a `Current` source census containing sorted zero-symbol parsed files, path-level counts matching the one captured publication, and typed readiness refusal before `Current`.

**Step 2:** Add the public `IndexCensus` and `CensusFile` contract atoms and an `index_census` handle method. Read one publication generation for both live data and outline; use the normal runtime acquisition/revocation path, not a raw data-plane bypass.

**Step 3:** Amend the frozen public contract, export/wrap assertions, operation identity table, and checked-in delta evidence in the same change.

**Step 4:** Run focused embed and public-API contract gates.

### Task 5: Publish monotonic live indexing progress (CR-3)

**Files:**
- Modify: `src/index_lifecycle/embedded.rs`
- Modify: `src/index_lifecycle/public_api.rs`
- Modify: `src/embed.rs`
- Modify: `src/lifecycle_identity.rs`
- Modify: frozen public-API contract/delta artifacts identified by the existing contract test
- Test: nearest embed lifecycle and public-API delta tests

**Step 1:** Define and document exact meanings for discovered, parsed, and found-symbol counts; add failing tests that hold a reload after N parses and prove counters rise only during that reload.

**Step 2:** Add an infallible `index_progress` handle method backed by binding-owned atomics. Reset before each real reload, increment only at the defined lifecycle points, retain last values after cancellation, and leave idle tick scouts untouched.

**Step 3:** Amend all frozen public-contract surfaces and run focused contract/embed gates.

### Task 6: Make code and embedded knowledge retrieval honest and complete (F-1, F-2, selector, path-prefix)

**Files:**
- Modify: actual `search_text` response construction after source tracing
- Modify: `src/protocol/knowledge_search.rs` only through a reusable, server-independent typed retrieval seam
- Modify: `src/index_lifecycle/embedded.rs`, `src/index_lifecycle/public_api.rs`, `src/embed.rs`, `src/lifecycle_identity.rs`
- Modify: selector resolver after source tracing
- Modify: path-prefix matcher after source tracing
- Modify: frozen public-API contract/delta artifacts identified by the existing contract test
- Test: nearest existing protocol/integration tests

**Step 1:** Write failing protocol and embed tests: a code-search zero hit that excluded Markdown/knowledge content reports the exact excluded count and points to `search_knowledge`; an embedded knowledge query returns admitted Markdown/text/config knowledge with provenance, relationship evidence, authority/coverage, and withholding evidence; a partial result reports policy-withheld in-scope files and reasons; an ambiguous name refuses with candidate IDs; `src/` excludes `srcx/`.

**Step 2:** Reuse the published source-set and existing knowledge retrieval/ranking logic through a typed engine seam. Do not index Markdown bodies twice, do not stringify server tool output for embed callers, and do not alter `search_knowledge` retrieval semantics.

**Step 3:** Add the knowledge request/result atoms and `EmbeddedSourceHandle::search_knowledge`, amend every frozen public-contract surface, then run focused embed and server gates.

### Task 7: Release handoff

**Files:**
- Modify: release notes/changelog location identified by repository convention
- Modify: `tasks/todo.md`

**Step 1:** Document delivered CR identifiers, verification commands/results, and the exact release revision.

**Step 2:** Before CI, inventory equivalent active/queued runs and retain only the newest required gate.

**Step 3:** Tag/release only after all required gates pass; Fathom then updates its pinned revision and lockfile.
