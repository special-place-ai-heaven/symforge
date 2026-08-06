# Independent review findings — PolicyWithheld + curation reorder

**Reviewer:** Cursor agent  
**Date:** 2026-08-06  
**Branch reviewed:** `fix/policy-withheld-skip-reason`, commit `d87c748`  
**Base:** `origin/main` @ `93cdd00`  
**Method:** Read implementation paths cited in the review request; ran the two new unit tests (not the full serial suite).

---

## Item A — `PolicyWithheld` skip-reason split (commit `d87c748`)

### A1. Does `PolicyWithheld` leak anything?

**Verdict: CONFIRMED** (for the property the change claims to preserve), with one **documented tradeoff** below.

**Path-rule vs content-detector collapse — preserved**

Both sensitive variants still map to one display label:

```3442:3443:src/live_index/store.rs
                MetadataOnlyReason::SensitivePath { .. }
                | MetadataOnlyReason::SensitiveContent { .. } => SkipReason::PolicyWithheld,
```

The read-path refusal remains uniform and names no rule class:

```3636:3648:src/protocol/format.rs
/// Refusal for a file the admission pipeline excluded from content disclosure.
///
/// Deliberately UNIFORM across every security exclusion: it names no rule id, no
/// rule class, no finding count, no size, and no byte of the file. Whether the
/// exclusion came from the path rule or from a content detector is the one
/// content-derived bit a refusal could still leak, and the recovery action is
/// identical either way, so the message does not distinguish them.
pub fn content_withheld_by_admission(path: &str) -> String {
```

Security-sensitive decisions on the read gate still use typed `MetadataOnlyReason`, not the `SkipReason` projection:

```1258:1275:src/live_index/query.rs
    /// Typed per-path admission disposition, read straight off the manifest.
    ///
    /// Deliberately NOT `capture_admission_tier_lookup_view`: that projects through
    /// `compatibility_admission_decision`, which collapses seven `MetadataOnlyReason`
    /// variants — including both security variants — into
    /// `SkipReason::UnsupportedLanguage`. A security decision must never be taken on
    /// that projection.
    pub fn capture_file_disposition(
```

I found no path where `MetadataOnlyReason::SensitivePath { rule_id }` or `SensitiveContent { rule_ids, finding_count }` fields are serialized into MCP tool output, health text, or catalog digests exposed to callers. Manifest persistence keeps the typed reason; the new label is a display projection only.

**Tradeoff (not a blocker): policy vs non-policy is now distinguishable**

Before `d87c748`, sensitive files, LFS pointers, path-metadata failures, and undecodable encodings all displayed as `"unsupported language"`. After the change, only `SensitivePath | SensitiveContent` display as `"withheld by admission policy"`; the other four variants have honest, distinct labels (`UnsupportedTextEncoding`, `LfsPointer`, `UnsupportedPath`).

So a caller who sees `[metadata-only: withheld by admission policy]` on a Tier-2 path can infer **a policy demotion occurred** (path rule *or* content detector), as opposed to an encoding/LFS/path-shape issue. That is narrower than the old seven-way collapse, but it does **not** distinguish path rule from content detector — the bit the read-path contract protects.

I judge this an acceptable, intentional trade: stopping the false language claim is worth revealing the policy-vs-technical bucket. It should not block landing unless the product goal is to hide even *that* a policy fired (which would require collapsing policy back with encoding/LFS/path, reintroducing misinformation).

**Surfaces checked (verified by read, not exhaustive runtime probe):**

| Surface | Uses `SkipReason` projection? | Leaks path-vs-content? |
|---------|------------------------------|------------------------|
| `get_file_content` / read gate | No — typed disposition + uniform refusal | No |
| `not_indexed_skipped_file` | Yes — `reason.to_string()` | No (both → `PolicyWithheld`) |
| `search_files` resolve `[metadata-only: …]` | Yes — via `metadata_only_skipped_paths()` | No |
| `capture_admission_tier_lookup_view` / health tier lookup | Yes | No |
| `health` / `status` tier counts | Counts only, no per-reason breakdown | No |
| Manifest / snapshot persistence | `MetadataOnlyReason` (typed), not `SkipReason` | No in external API |

---

### A2. Is splitting the four non-sensitive reasons safe?

**Verdict: CONFIRMED**

| Variant | Content-derived? | Evidence |
|---------|------------------|----------|
| `PlatformPathCollision` | No — path identity | Set before payload read in scout |
| `UnsupportedPathEncoding` | No — path encoding | Same |
| `PathMetadataTooLarge` | No — path metadata bound | Same |
| `UnsupportedTextEncoding` | Encoding fact, not semantic content | Distinct from language support; comment at `domain/index.rs:1485-1488` |
| `LfsPointer` | **Format class only, not payload** | `detect_lfs_pointer` (`knowledge/mod.rs:652-672`) requires valid UTF-8 ≤1 KiB and exactly three LFS header lines. Display string is `"git-lfs pointer"` — it does not expose `declared_oid` / `declared_size` from `MetadataOnlyReason::LfsPointer { .. }`. Knowing a file is an LFS pointer is equivalent to knowing its first ~200 bytes match the public LFS spec, not arbitrary file semantics. |

---

### A3. Missed mapping sites?

**Verdict: CONFIRMED — no missed production mapping sites found**

Production forward map: `compatibility_admission_decision` in `store.rs:3401-3467`.

Reverse maps (display-only → manifest write path, defensive):

- `disposition_from_admission` — `store.rs:3469-3509`
- discovery compat helper — `discovery/mod.rs:1158-1180`

Test helpers (compiler-forced exhaustiveness):

- `live_index/query.rs:3848-3863`
- `protocol/tools.rs:13147-13162`

`SkipReason` has no `Serialize`/`Deserialize` (`domain/index.rs:1439-1440`), so Rust exhaustiveness on `match decision.reason` in the forward map and `Display` impl covers all variants; no `_ =>` catch-alls swallow the new variants in production paths.

**Note:** `disposition_from_admission` collapses `PolicyWithheld` back into `MetadataOnlyReason::UnsupportedTextEncoding` (`store.rs:3494-3497`). That is intentional — comment says these are display-only reasons never produced by the admission pipeline — and is not a reporting leak because nothing reads that round-trip for external output.

---

### A4. Snapshot / serialization compatibility

**Verdict: CONFIRMED — no break identified**

- `SkipReason` is **not** persisted (`#[derive(Debug, Clone, Copy, PartialEq, Eq)]` only, `domain/index.rs:1439`).
- Snapshots persist `MetadataOnlyReason` (`domain/index.rs:871-892`, `serde::Serialize/Deserialize`), which is **unchanged** by this commit.
- `symforge::embed` public API does not re-export `SkipReason` (grep over `src/embed.rs` — no hits).
- Adding enum variants to a non-serialized display type cannot break older snapshot bytes.

**Not verified:** a live round-trip load of a pre-`d87c748` snapshot against this binary. Risk is inferred from type layout, not measured.

---

### A5. Are the two new tests load-bearing?

**Verdict: PARTIALLY WRONG — necessary but not sufficient**

Both tests pass (verified):

```
cargo test --lib policy_withheld -- --test-threads=1          → ok
cargo test --lib non_sensitive_skip_reasons -- --test-threads=1 → ok
```

What they actually pin:

- `policy_withheld_never_claims_a_language_problem` — `Display` string for `PolicyWithheld` contains no `language`, `size`, `path`, `content`, `secret`, `detector`, or `rule` substrings (`domain/index.rs:2290-2309`).
- `non_sensitive_skip_reasons_are_distinct_and_honest` — four display strings are pairwise distinct and contain expected keywords (`domain/index.rs:2315-2332`).

What they do **not** pin:

- End-to-end projection from a loaded manifest with real `SensitivePath`/`SensitiveContent` dispositions through `compatibility_admission_decision` → MCP output.
- That `store.rs` sensitive-fixture tests (`~7685+`) already cover typed disposition + no canary in serialized manifest; they assert `MetadataOnlyReason`, not `SkipReason::PolicyWithheld`.

**Recommendation:** Add one integration assertion that `compatibility_admission_decision` on a `SensitiveContent` catalog entry yields `Some(PolicyWithheld)`, not `UnsupportedLanguage`. Low cost, closes the gap the Display-only tests leave.

---

### Item A — declined TestPilot asks

**Verdict: CONFIRMED — correctly declined**

- Threshold / machine-readable exclusion codes — would reopen the side channel `format.rs:3638-3642` deliberately closes.
- Bounded `get_file_content` for Tier-2 — Tier-2 includes `SensitiveContent`; read gate refuses both variants uniformly (`read_gate.rs:118-120`).
- `force_admit` — bypasses fail-closed gate; no safe variant found.

---

## Item B — cold-start curation reorder (~3.8 s)

### Verdict: **safe-with-modification**

The proposed reorder is safe **only** for the no-replay-directory fast path. It must not skip `curation_plan_current` when a replay directory exists and recovery will run.

### B1. Side effects of `curation_plan_current` / `review_current`?

**Verdict: CONFIRMED — appears pure on the startup path (inferred, not traced to every callee)**

```145:301:src/protocol/knowledge_review.rs
pub(crate) fn review_current(
    generation: &PublishedGeneration,
    input: &ReviewKnowledgeInput,
) -> Result<ReviewKnowledgeOutput, String> {
    // ... reads generation, computes facts, renders strings ...
    Ok(ReviewKnowledgeOutput { ... })
}
```

- Takes `&PublishedGeneration`; no `&mut`, no file I/O in the function body shown.
- `guard_hit` (`knowledge/mod.rs:847-852`) validates visible fields; does not write state.
- `review_facts` builds in-memory maps from generation data (`knowledge_review.rs:436+`).

**Not exhaustively traced:** every helper below `review_facts`. Nothing in the startup call chain suggests mutation, but this is inference, not line-by-line proof.

### B2. Is `PublishedGeneration.source` equivalent to `CurationReviewPlan.source` for `apply_capability`?

**Verdict: CONFIRMED**

```421:431:src/protocol/knowledge_review.rs
    let source = generation
        .source
        .as_ref()
        .map(|source| source.as_ref().clone())
        .ok_or_else(|| "Curation unavailable: source identity is absent.".to_string())?;
    Ok(CurationReviewPlan {
        // ...
        source,
```

`CurationReviewPlan.source` is `SourceIdentity` (`knowledge_review.rs:66-72`). `apply_capability` uses only:

```1542:1544:src/protocol/knowledge_curation.rs
    if !matches!(source_location, SourceLocation::WorkingTree { .. }) {
        return Err(CapabilityUnavailableReason::NonProjectLocalPlacement);
    }
```

`generation.source.as_ref().map(|s| &s.location)` is the same value passed through the plan.

### B3. Does current order encode an invariant?

**Verdict: PARTIALLY — fail-closed wording overstates startup impact**

Current order when replay dir **missing** (fresh project):

1. `curation_plan_current` — ~3.8 s, succeeds on SymForge itself
2. `apply_capability` — ~102 µs
3. `!replay_dir.is_dir()` → `Ok(())`

When `curation_plan_current` **errors**, startup is **not** blocked:

```335:344:src/protocol/mod.rs
        if let Some(root) = repo_root.as_deref()
            && let Err(error) = curation_coordinator.recover_on_project_load(...)
        {
            tracing::warn!("knowledge curation startup recovery remained fail-closed: {error}");
        }
```

Same pattern in `daemon.rs:2995-3001`. Errors become warnings; the server still serves.

**Invariant that matters:** when `replay_dir` **exists**, recovery must still run `curation_plan_current` before touching replay records (plan validation, action set, review hash). The proposed hoist must not apply on that path.

**Minor behavior change on reorder:** if `curation_plan_current` would error but there is no replay dir, current code logs a warning after wasting 3.8 s; a correct reorder returns `Ok(())` silently. Functionally equivalent for tool serving; observability differs.

### B4. Cheaper correct variant

**Verdict: CONFIRMED — viable**

```rust
// Pseudocode — safe shape
let generation = index.published_source_set().current_generation();

// Cheap replay-dir probe — no plan needed
let state_dir = state_placement
    .and_then(|p| p.directory())
    .map(|d| d.as_path().to_path_buf())
    .ok_or(...)?;  // same error class as apply_capability when placement missing
let replay_dir = state_dir.join(CURATION_STATE_DIR).join(REPLAY_DIR);
if !replay_dir.is_dir() {
    return Ok(());
}

// Expensive path only when recovery may actually run
let plan = curation_plan_current(&generation)?;
let state_dir = apply_capability(
    repo_root,
    state_placement,
    persistence_health,
    &plan.source.location,  // == generation.source.location
)?;
// ... existing recovery ...
```

`apply_capability` does **not** create the state directory on this path — it validates placement, WorkingTree source, persistence health, and writability, then returns `state_placement.directory()` (`knowledge_curation.rs:1527-1563`). The replay-dir existence check is a plain `is_dir()` and needs only the placement path.

**Do not** use `generation.source` alone to skip `apply_capability` when `replay_dir` exists; the capability checks (readonly root, non-WorkingTree source, etc.) remain required before recovery.

---

## Additional findings

1. **False-language bug was real.** The old seven-way collapse into `UnsupportedLanguage` is gone from `compatibility_admission_decision`; sensitive fixtures in `store.rs:7700-7718` retain typed dispositions while the projection now yields an honest label.

2. **`tier2_sweep_never_names_a_security_demoted_file`** (`tools.rs:31095-31151`) still references the pre-fix mislabel in a comment (SF-DOG-004). The test guards sweep behavior via typed disposition, not `SkipReason`; it should still pass, but the comment is stale after `d87c748`.

3. **Full serial suite not run.** Only the two new unit tests were executed (~18 s compile + run). Total gate (`cargo test --all-targets -- --test-threads=1`) was not run per time budget; no regressions inferred beyond the tests above.

---

## Summary

| Item | Question | Verdict |
|------|----------|---------|
| A1 | `PolicyWithheld` leak? | **CONFIRMED** — path-vs-content preserved; policy-vs-technical now distinguishable (accepted tradeoff) |
| A2 | Non-sensitive splits safe? | **CONFIRMED** |
| A3 | Missed mappings? | **CONFIRMED** — none found |
| A4 | Snapshot compat? | **CONFIRMED** — `SkipReason` not persisted |
| A5 | Tests load-bearing? | **PARTIALLY WRONG** — Display-only; add one projection integration test |
| B | Reorder safe? | **safe-with-modification** — hoist replay-dir check using placement path; keep full plan when replay dir exists |

**Landing recommendation for Item A:** **Land.** The change fixes a real false-positive language diagnosis without breaking the critical path-vs-content non-disclosure property. The policy-vs-technical distinction is new but defensible.

**Landing recommendation for Item B:** **Do not ship the naive reorder without the guard above.** The safe variant should save ~3.8 s on cold fresh projects with no `.symforge/curation/replay/` directory and no functional regression on the recovery path.
