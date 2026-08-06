# Independent review findings — PolicyWithheld + curation reorder

**Reviewer:** Cursor Grok (independent pass; parallel multi-LLM review)  
**Date:** 2026-08-06  
**Request:** `REVIEW-REQUEST-POLICY-WITHHELD-2026-08-05.md`  
**Branch:** `fix/policy-withheld-skip-reason`  
**Code under review:** `d87c748` (repo HEAD also has `b200f6f` review-request docs only)  
**Base:** `origin/main` @ `93cdd00`  
**Full serial suite:** not run. Ran only:

```
cargo test --lib -- --exact \
  domain::index::tests::policy_withheld_never_claims_a_language_problem \
  domain::index::tests::non_sensitive_skip_reasons_are_distinct_and_honest
→ 2 passed
```

Claims tagged **verified** (read code / ran command), **inferred**, or **guess**.

---

## Item A — did `d87c748` preserve the non-disclosure property?

### A1. Does `PolicyWithheld` leak anything?

**Verdict: PARTIALLY WRONG** on the author's judgment that narrowing is not a meaningful disclosure.

#### Claim A1a — path-rule vs content-detector stay collapsed

**CONFIRMED (verified).**

```3442:3443:src/live_index/store.rs
                MetadataOnlyReason::SensitivePath { .. }
                | MetadataOnlyReason::SensitiveContent { .. } => SkipReason::PolicyWithheld,
```

Read-path refusals still go through typed disposition and the uniform message; they never render `SkipReason`:

```118:120:src/protocol/read_gate.rs
            MetadataOnlyReason::SensitivePath { .. }
            | MetadataOnlyReason::SensitiveContent { .. } => {
                return Err(format::content_withheld_by_admission(relative_path));
```

```3638:3642:src/protocol/format.rs
/// Deliberately UNIFORM across every security exclusion: it names no rule id, no
/// rule class, no finding count, no size, and no byte of the file. Whether the
/// exclusion came from the path rule or from a content detector is the one
/// content-derived bit a refusal could still leak
```

`rule_id` / `rule_ids` / `finding_count` stay on persisted `MetadataOnlyReason` only. Public MCP/health reporting uses `SkipReason::Display`. Health admission section is tier counts only. **Verified.**

#### Claim A1b — narrowing the bucket is not a meaningful disclosure

**PARTIALLY WRONG (verified).**

Before `d87c748`, seven `MetadataOnlyReason`s (including both security variants, LFS, path failures, encoding) all displayed as `"unsupported language"`. After:

| Disposition | Display |
|---|---|
| `SensitivePath` \| `SensitiveContent` | `withheld by admission policy` |
| `LfsPointer` | `git-lfs pointer` |
| path-shape failures | `unsupported path` |
| `UnsupportedTextEncoding` | `undecodable text encoding` |
| real unsupported grammar | `unsupported language` |

So `PolicyWithheld` now means: **a path rule or a content detector fired** — the class the old collapse hid among encoding/LFS/path/language excuses.

For paths that cannot match `sensitive_path_rule` (verified: `.rs` paths are outside the path-rule set at `knowledge/mod.rs:685-714`), the collapse inside `PolicyWithheld` is empty — the label is a near-oracle for `SensitiveContent`. Example from the request: `src/live_index/store.rs`.

Surfaces where this class bit is visible as the reason string alone:

- `search_files` resolve / hits — `format.rs:2927-2928`, `tools.rs:6064`
- `get_repo_map` tree tags — `format.rs:1670-1677`
- daemon cross-project search — `daemon.rs:4977-4979`
- embed `SearchFilesHit.metadata_reason: Option<String>`

Partial mitigation already present (verified): `not_indexed_skipped_file` already appends *"Its contents are withheld by the admission policy"* when `disk_read_would_refuse` is true (`format.rs:3721-3724`), and that predicate is true for both security variants (`read_gate.rs:66-74`). On `get_file_context`, the policy sentence pre-existed; only the reason tag was lying. The **new** oracle is on search / repo-map / metadata-reason strings that previously said `"unsupported language"`.

**Plain answer:** yes — this leaks something the old collapse hid: the coarser bit "security/admission policy applied," which for ordinary-looking source paths is nearly `SensitiveContent`. It does **not** leak path-vs-content.

**Landing call (judgment):**

- If *"which ordinary files tripped the secret detector"* is sensitive (search/repo-map as oracle) → **do not land as-is**. Keep security demotions in a bucket that still includes at least one common non-security technical reason (encoding), or stop emitting per-file reason strings for that bucket.
- If the contract is only path-vs-content + no rule ids (what `format.rs` documents) → honesty fix is correct; land it, but stop calling the narrowing "not a disclosure."

I am **not** rubber-stamping "no leak."

---

### A2. Is splitting the four non-sensitive reasons safe?

**Verdict: PARTIALLY WRONG** on the blanket "not content-derived" claim for LFS; **CONFIRMED** for the three path-shape reasons.

| Variant | Verdict | Evidence |
|---|---|---|
| `UnsupportedPathEncoding` | safe to name | Minted from path UTF-8 / safety checks in `discovery/mod.rs:956-964` |
| `PathMetadataTooLarge` | safe to name | Same site — path length vs catalog bound |
| `PlatformPathCollision` | safe *if ever minted* | Enum exists (`domain/index.rs:888`); **no production mint site found** (grep only hits enum + new forward map). Dead arm today. |
| `LfsPointer` | format-class, content-derived | `detect_lfs_pointer` (`knowledge/mod.rs:652-672`) reads file bytes. Naming discloses the body matches the public LFS pointer grammar. Does **not** expose `declared_oid` / `declared_size` on the SkipReason surface. Not a secret side-channel; still content-derived, contrary to the commit message's absolute claim. |
| `UnsupportedTextEncoding` | encoding fact | Distinct from language support; naming is correct |

---

### A3. Missed mapping sites?

**Verdict: CONFIRMED** — no missed production map that still collapses security into `UnsupportedLanguage`.

Updated in `d87c748` (verified via `git show`):

1. Forward: `store.rs` `compatibility_admission_decision`
2. Reverse: `store.rs` `disposition_from_admission`
3. Reverse: `discovery/mod.rs` `scout_decision_for_discovered`
4. Test helpers: `query.rs`, `tools.rs` (compiler-forced)

No production `_ =>` swallows the new variants into a wrong display bucket. HardSkip-only `_ => ArtifactType` at `discovery/mod.rs:1152-1153` is scoped correctly.

**Missed (documentation, not mapping):**

- `query.rs:1261-1263` still claims seven variants collapse into `UnsupportedLanguage`
- `tools.rs:2789-2791` same stale claim

**Round-trip trap (verified, currently unreachable for security files):** reverse maps send `PolicyWithheld | LfsPointer | UnsupportedPath` → `MetadataOnlyReason::UnsupportedTextEncoding`. Scout writes typed security reasons directly, so live demotions are not re-labeled. If something later fed `PolicyWithheld` into `disposition_from_admission`, rematerialized display would be `"undecodable text encoding"` — a different lie.

---

### A4. Snapshot / serialization compatibility

**Verdict: CONFIRMED** — no snapshot wire break from adding `SkipReason` variants. **Not verified:** live load of a pre-change snapshot against this binary.

- `SkipReason` has **no** serde derives (`domain/index.rs:1439`) — **verified**
- Snapshots persist `MetadataOnlyReason`, unchanged by this commit — **verified**
- `symforge::embed` does not export `SkipReason`; embedders can see `metadata_reason` **strings** change at runtime — **verified**

**Inferred:** out-of-tree consumers that treated `"unsupported language"` as "security demotion" will break. That was never a contract.

---

### A5. Are the two new tests load-bearing?

**Verdict: PARTIALLY WRONG** — they pin Display wording, not the production mapping.

Both pass (command above). They do **not** assert that `compatibility_admission_decision` on a real `SensitiveContent` / `SensitivePath` catalog entry yields `PolicyWithheld`. A store.rs regression remapping security back to `UnsupportedLanguage` leaves both tests green.

Existing store fixtures around `store.rs:7700+` assert typed `MetadataOnlyReason`, not the SkipReason projection.

**Recommendation:** assert `compatibility_admission_decision(sensitive_entry).reason == Some(PolicyWithheld)`.

---

### Declined TestPilot asks

**Verdict: CONFIRMED — correctly declined.**

- Machine-readable exclusion codes / thresholds — reopen `format.rs:3638-3642`
- Bounded `get_file_content` for Tier-2 — includes `SensitiveContent`; gate refuses both (`read_gate.rs:118-120`)
- `force_admit` — bypass of fail-closed gate

---

## Item B — cold-start reorder of `recover_on_project_load`

### Verdict: **safe-with-modification**

The measured cold path (no replay dir) can drop the 3.8 s plan. A blind hoist that moves plan validation past probe on the recovery path risks a capability lie.

### B1. Does `curation_plan_current` have side effects?

**Verdict: CONFIRMED — no side effects on the startup path** (direct call chain verified; not every transitive helper line-audited).

```371:433:src/protocol/knowledge_review.rs
pub(crate) fn curation_plan_current(
    generation: &PublishedGeneration,
) -> Result<CurationReviewPlan, String> {
    let review = review_current(/* Remediation, limit: Some(1) */)?;
    // in-memory actions from generation.authority.records
    let source = generation.source.as_ref().map(|s| s.as_ref().clone())...;
    Ok(CurationReviewPlan { /* ... */ source, actions })
}
```

`review_current` takes `&PublishedGeneration`, builds strings/hashes, returns. No durable writes on that path. Cost is pure CPU (full remediation facts + dossier render for the canonical hash even with `limit: Some(1)`).

On the cold path the plan's only consumer is `apply_capability(..., &plan.source.location)` — then discarded before the early return (`knowledge_curation.rs:302-320`).

### B2. Is `PublishedGeneration.source` equivalent for the `matches!` check?

**Verdict: CONFIRMED (verified).**

Plan source is a clone of `generation.source` (`knowledge_review.rs:421-425`). Generation source comes from `manifest.source` (`store.rs:1296-1298`). `apply_capability` only checks WorkingTree (`knowledge_curation.rs:1542-1544`). `capability_status` already uses `generation.source.location` directly (`knowledge_curation.rs:249-253`).

**Not equivalent for full recovery:** the plan also validates manifest presence, unit hashes, and safety guards.

### B3. Does current order encode an invariant?

**Verdict: PARTIALLY — yes on the recovery path; no on the cold path.**

Cold path today: plan → `apply_capability` (no dir creation; returns placement path at `knowledge_curation.rs:1548-1563`) → early return. Startup treats recovery `Err` as non-fatal warn (`protocol/mod.rs:335-344`).

Recovery path today:

```
plan₁ → apply_capability → probe_apply_directories (CREATE curation dir + fill probe_cache)
      → lock → plan₂ → recover_pending_records
```

**Invariant at risk** if plan₁ is dropped and plan validation moves after probe: `probe_apply_directories` inserts `Ok(())` into `probe_cache` (`knowledge_curation.rs:693-694`); `capability_status` reports Available from that cache (`knowledge_curation.rs:260-265`). Plan failure after probe can make capability lie. Real correctness regression.

Also: when replay exists, plan runs twice (lines 302 and 329) against the same generation — pure waste.

### B4. Cheaper correct variant

**Verdict: CONFIRMED — hoist via `state_placement`, not via the plan.**

`health_line` already probes replay from placement alone (`knowledge_curation.rs:189-191`). Safe shape:

1. After `current_generation()`, if placement has no `curation/replay/` dir → `return Ok(())` immediately.
2. When replay exists: readiness/plan gate **before** `probe_apply_directories`; `apply_capability` with `generation.source.location`; then probe / lock / recover; single in-lock plan (drop redundant plan₁).
3. Do not skip `apply_capability` on the recovery path.

Cold-path observability delta: plan failure with nothing to replay currently warns after 3.8 s; after hoist returns `Ok(())` silently. Serve impact: none.

---

## Anything missed

1. **A1 is the real question.** Path-vs-content holds. Policy-vs-technical is a new public bit on search/repo-map. Decide the threat model before merge.
2. **`PlatformPathCollision` looks unminted** — split is harmless but untested in production.
3. **Stale comments** at `query.rs:1261-1263` and `tools.rs:2789-2791`.
4. **Display-only tests do not pin the store projection.**
5. **Item B refusal was directionally right** for a blind reorder; `state_placement` early-return reclaimsthe 3.8 s safely.

---

## Summary table

| # | Question | Verdict |
|---|---|---|
| A1 | Leak? | **PARTIALLY WRONG** — path-vs-content preserved; narrowing *is* a meaningful policy-class disclosure (SensitiveContent oracle on innocuous paths via search/repo-map) |
| A2 | Non-sensitive splits safe? | **PARTIALLY WRONG** for absolute "LFS not content-derived"; path-shape splits **CONFIRMED** |
| A3 | Missed mappings? | **CONFIRMED** none in production; stale comments remain |
| A4 | Snapshot compat? | **CONFIRMED** `SkipReason` not persisted; live old-snapshot load **not verified** |
| A5 | Tests load-bearing? | **PARTIALLY WRONG** — Display-only; add projection assertion |
| B | Reorder safe? | **safe-with-modification** — hoist via `state_placement`; keep plan validation before probe when replay exists |

**Item A land?** Only after an explicit threat-model call on the policy-class oracle. If path-vs-content + no rule ids is the whole contract, land with that acknowledgment. If detector-hit oracles matter, keep security demotions in a broader neutral bucket.

**Item B land?** Not as proposed. Land the `state_placement` early-return for missing replay dir; do not reorder plan past probe on the recovery path.
