# Independent review findings — PolicyWithheld + curation reorder

**Reviewer:** Kimi (OMP session), working against `E:\project\symforge-policy` directly
**Date:** 2026-08-06
**Branch reviewed:** `fix/policy-withheld-skip-reason`, commit `d87c748`
**Base:** `origin/main` @ `93cdd00`
**Method:** Read every production path cited in the request plus the ones it did not
cite (read gate, sidecar, file-tree view, embed surface). Ran the skip-reason tests
(receipt below). Did **not** run the full serial suite.

**Tree-drift disclosure.** The working tree gained **uncommitted** modifications to
`src/domain/index.rs`, `src/live_index/query.rs`, `src/live_index/store.rs`,
`src/protocol/tools.rs` *during* this review (comment fixes, a `debug_assert`, and a
new mapping-level test — apparently the author reacting to other reviewers in real
time). Verdicts below target the **committed `d87c748` state**; where the drift
matters I say so explicitly. I did not read the other reviewers' findings files
before forming these verdicts.

---

## Item A — `PolicyWithheld` skip-reason split (commit `d87c748`)

### A1. Does `PolicyWithheld` leak anything? — **CONFIRMED** (the property holds)

The protected bit — **path rule vs content detector** — is still hidden:
`store.rs:3442-3443` maps both `SensitivePath { .. }` and `SensitiveContent { .. }`
to the same `SkipReason::PolicyWithheld`, whose `Display` is the static string
`"withheld by admission policy"` (`domain/index.rs:1516`). No rule id, no finding
count, no size, no oid.

Every surface a caller can reach shows only that static string or a uniform refusal:

| Surface | What it emits for a sensitive file | Evidence |
|---|---|---|
| `search_files` / resolve suffix | `[metadata-only: withheld by admission policy]` | `query.rs:1243-1255` → `format.rs:2928, 3006` |
| file-tree view | same Display string | `tools.rs:4478-4479` → `format::file_tree_view_with_skipped` |
| symbol/reference degradation | `Reason: withheld by admission policy` | `tools.rs:2490-2500, 2586-2602` |
| `not_indexed_skipped_file` | same Display string | `format.rs:3698` |
| sidecar `impact_skipped_text` | same Display string | `sidecar/handlers.rs:889-902` |
| read gate (`get_file_content`) | uniform `content_withheld_by_admission` | `read_gate.rs:105-122`, `format.rs:3636-3648` |
| `health` / `status` | tier **counts** only, no per-reason breakdown | `health_view.rs:557-560` |

The typed fields (`rule_id`, `rule_ids`, `finding_count`) are consumed only to
*make* the refuse decision (`read_gate.rs:70-122`) and never rendered; the only
`rule_id`/`finding_count` strings in `src/protocol/` belong to the knowledge-policy
domain (the repo's own policy TOML), not to per-file admission verdicts.

**On the narrowing question — the part the request flags as most important.**
Yes, `PolicyWithheld` is a narrower set than the old seven-way collapse, so a caller
now learns "a policy demotion fired" rather than "one of seven causes". But this bit
was **already served to any caller before this commit**: the read gate answers it
per path on request, distinguishing `content_withheld_by_admission` (policy) from
`content_withheld_unscanned` (scan-budget/encoding) — `read_gate.rs:105-122`,
`format.rs:3650-3663`. And for any *supported* extension the old label was already
invertible by anyone who can read this open-source code: "unsupported language" on a
`.rs`/`.ts` file could only be the hidden bucket. The genuinely content-derived bit
(which of the two sensitive variants fired) remains protected, and the inference
"benign-looking path + withheld ⇒ content detector" predates the change (it works
against the read-path refusal too). The new label adds **no new oracle**; it stops
asserting a falsehood on a surface the old oracle already covered.

### A2. Is splitting the four non-sensitive reasons safe? — **CONFIRMED**

- `UnsupportedPathEncoding` / `PathMetadataTooLarge` are minted at
  `discovery/mod.rs:957-965` from the **path alone, before any byte is read**.
  Not content-derived.
- `PlatformPathCollision` — **no minting site exists in the current tree** (dead
  variant; only the enum definition at `domain/index.rs:888` and the mapping at
  `store.rs:3453`). Mapping it is harmless; noting it so the claim "four reasons
  split" is understood as "three live + one dormant".
- `LfsPointer` *is* byte-derived (`knowledge/mod.rs:745-749`), but the Display
  string is the static `"git-lfs pointer"` — it discloses only that the first
  <1 KiB match the **public** LFS pointer grammar, a format class. The typed
  `declared_oid` / `declared_size` ride along in `MetadataOnlyReason` (serde,
  local manifest only) and are never rendered to a caller surface.

### A3. Missed mapping sites? — **CONFIRMED none missed; one related residual found**

Production sites, all exhaustive (compiler-checked, no `_ =>` on these domains):

- forward map `compatibility_admission_decision` — `store.rs:3401-3466`
- reverse map `disposition_from_admission` — `store.rs:3469-3522`
- discovery reverse map — `discovery/mod.rs:1151-1182`
- test helpers — `query.rs:3849-3865`, `tools.rs:13148-13164`

The two `_ =>` catch-alls that exist are on disjoint domains and cannot conflate
these variants: `discovery/mod.rs:1153` (HardSkip arm only) and
`discovery/mod.rs:2169` (`path_admission_reason` yields only
Lockfile/OversizedData/GeneratedOrVendor).

**Residual the commit did not catch (same dishonesty class):**
`store.rs:3460-3464` still maps `FileDisposition::Unreadable { .. } |
UnstableDuringRead | AbortedCircuitBreaker` to `SkipReason::UnsupportedLanguage` —
an I/O failure or circuit-breaker abort is still reported to callers as
"unsupported language". Outside the commit's stated seven-variant scope, but it is
the same false-diagnosis pattern and will produce the same kind of misleading
external report. Recommend a follow-up variant (e.g. an honest `Unreadable`
reason), not a blocker for this one.

### A4. Snapshot / serialization compatibility — **CONFIRMED no break**

- `SkipReason` derives only `Debug, Clone, Copy, PartialEq, Eq`
  (`domain/index.rs:1439-1440`); `AdmissionDecision` likewise (`1519-1523`);
  `SkippedFile` only `Debug, Clone` (`1544-1545`). None is serializable, so adding
  four variants cannot touch persisted bytes.
- Snapshots/manifests persist `MetadataOnlyReason`
  (`domain/index.rs:871-892`, `Serialize/Deserialize`), which this commit does not
  modify — old snapshots load against identical variant definitions.
- Embedders: `symforge::embed` re-exports `stel_core::types::AdmissionDecision`
  (`embed.rs:158-160`), which is a **different enum** (`Serve | Degrade | Bypass`,
  `stel_core/types.rs:51`). AAP cannot observe `SkipReason` through the embed API.

*Not verified:* a live load of a real pre-`d87c748` snapshot against this binary —
inferred safe from type layout, not measured.

### A5. Are the two new tests load-bearing? — **PARTIALLY WRONG** (at `d87c748`)

Both committed tests assert on `SkipReason::X.to_string()` **strings only**
(`domain/index.rs:2290-2332`). A regression that re-mapped
`SensitiveContent → UnsupportedLanguage` in the forward map would leave both tests
green — they pin the *wording* of the new variants, not the *projection* that
produces them. So: load-bearing for the Display contract, vacuous for the actual
fix.

**Drift note:** the uncommitted working-tree changes add
`security_demotions_project_to_policy_withheld_not_a_language_verdict`
(`store.rs` ~7766), which drives a real `LiveIndex::load` over a canary `.env` and
a token-bearing `src/config.rs` and asserts the production projection yields
`PolicyWithheld` and never `UnsupportedLanguage`. That is exactly the missing
mapping-level pin, and it is genuine (it would fail on a forward-map regression).
It is **not part of the commit under review**.

### Declined TestPilot asks — **CONFIRMED correctly declined**

- Bounded Tier-2 reads: Tier-2 includes `SensitiveContent`; the gate refuses both
  sensitive variants uniformly (`read_gate.rs:118-120`). A bounded read would
  disclose exactly what the gate withholds.
- Machine-readable exclusion codes / thresholds: reopens the channel
  `format.rs:3636-3648` documents as deliberately closed.
- `force_admit`: a bypass of a fail-closed security gate. No safe variant exists.

---

## Item B — the ~3.8 s cold-start reorder — **safe-with-modification**

### B1. Does `curation_plan_current` have side effects? — **CONFIRMED pure**

`review_current` (`knowledge_review.rs:145-301`) and `curation_plan_current`
(`371-431`) take only `&PublishedGeneration` / `&ReviewKnowledgeInput`. The
production bodies contain no `std::fs`, no writes, no mutation — the only `std::fs`
hits in the file are `#[cfg(test)]` fixtures (1381+). `guard_hit`
(`knowledge/mod.rs:847-852`) and `source_envelope_is_safe`
(`knowledge_review.rs:1242-1250`) are pure validations. The 3.8 s is CPU:
per-record `review_facts` + dossier rendering + hashing + a secret-policy scan of
the rendered text — and on the fresh-project path the entire result is **discarded**
except `plan.source.location`.

### B2. Is `PublishedGeneration.source` equivalent to the plan's? — **CONFIRMED**

`plan.source` is `generation.source` cloned, erroring only if absent
(`knowledge_review.rs:421-427`). `apply_capability` uses it for exactly one
`matches!(source_location, SourceLocation::WorkingTree { .. })`
(`knowledge_curation.rs:1534-1536`). Same value, same check. Bonus: the
source-absent error currently fires at the *end* of `curation_plan_current` —
after the 3.8 s is already spent; resolving from `generation.source` directly
errors fast in the same class.

### B3. Does the current order encode an invariant? — **No; and one worry is REFUTED**

The request's caution "note `apply_capability` may CREATE the state dir" does not
survive reading: `apply_capability` (`knowledge_curation.rs:1527-1563`) creates
**nothing** — it is five pure capability checks plus two `fs::metadata` reads,
returning a `PathBuf`. Directory creation lives in `prepare_mutation` (`:421`) and
`probe_apply_directories` (`:669`), both gated *behind* the `replay_dir.is_dir()`
early return. On the fresh path no state directory is created, validated, locked,
or touched beyond the `is_dir()` metadata read itself. There is no
"plan must be validated before the state dir is touched" invariant to break,
because nothing is touched on this path.

What the reorder **does** lose on the fresh path: if `curation_plan_current` would
*error* (absent source, unsafe envelope, guard hit), today's code logs
`"knowledge curation startup recovery remained fail-closed: {error}"`
(`protocol/mod.rs:335-344`, `daemon.rs:2995-3001` — both log-and-continue), while
reordered code returns `Ok(())` silently. That is an **observability** difference,
not a correctness one: nothing downstream consumes the plan on this path, and the
same error resurfaces on the first later `review_knowledge` call. If that warning
is valued, the reordered fast path can run the probe and *still* log on
plan-error — cheap insurance, not a requirement.

### B4. Cheaper correct variant — **CONFIRMED viable**

```rust
let generation = index.published_source_set().current_generation();

// Fast path: resolve the replay dir WITHOUT the plan.
if let Some(dir) = state_placement.and_then(StatePlacement::directory) {
    if !dir.as_path().join(CURATION_STATE_DIR).join(REPLAY_DIR).is_dir() {
        return Ok(());                       // fresh project: skip the 3.8 s
    }
}
// (placement absent → fall through to the slow path, which errors as today)

// Slow path — unchanged semantics:
let plan = curation_plan_current(&generation)?;
let state_dir = apply_capability(repo_root, state_placement, persistence_health,
                                 &plan.source.location).map_err(unavailable)?;
let replay_dir = state_dir.join(CURATION_STATE_DIR).join(REPLAY_DIR);
if !replay_dir.is_dir() { return Ok(()); }   // keep this re-check (TOCTOU)
// ... probe, lock, re-plan, recover — exactly as today
```

Keep the in-slow-path re-check: a replay dir appearing between the fast probe and
the lock must still be recovered. Today's race window (dir appears between the
existing check and lock acquisition) is shifted earlier, not grown.

**Additional observation the request did not make:** on the *recovery* path the
plan is computed **twice** — once outside the lock (used only for
`source.location`) and again inside it, where the result is discarded
(`let _plan = curation_plan_current(&generation)?;`,
`knowledge_curation.rs:330-333`). The inside-lock run is the real fail-closed
re-validation; the outside run is redundant except for the enum match. Resolving
the location from `generation.source` and deleting the outside-lock computation
saves ~3.8 s on the recovery path too, with the inside-lock plan still guarding
recovery. That is the stronger form of the same fix.

---

## Anything missed

1. **Residual false-language mapping** (A3 above): `Unreadable |
   UnstableDuringRead | AbortedCircuitBreaker → UnsupportedLanguage`,
   `store.rs:3460-3464`. Same lie, different door.
2. **Stale comments at `d87c748`:** `query.rs:1258-1264` and `tools.rs:2789-2793`
   still describe the pre-fix seven-way collapse into `UnsupportedLanguage` in
   present tense (both already fixed in the uncommitted drift);
   `tools.rs:31096-31100` (SF-DOG-004) still says the file "is excluded only
   because `compatibility_admission_decision` mislabels every security demotion"
   and anticipates a correction that has now landed — still stale even in the
   drift.
3. **`PlatformPathCollision` is a dead variant** (A2 above) — no minting site.
4. **Double plan computation on the recovery path** (B4 above).

---

## What I ran vs inferred

- **Ran:** `cargo test --lib -- --test-threads=1 policy_withheld non_sensitive_skip_reasons`
  in the worktree → **3 passed, 0 failed, exit 0** (the two committed Display tests
  plus the uncommitted mapping test; compiled against the drifted working tree —
  the Display strings are identical in `d87c748`, so the pass is valid evidence for
  the commit). Full serial suite **not run**.
- **Verified by reading:** every file:line cited above against the worktree at
  `d87c748` (+ drift where noted).
- **Inferred, not proven:** snapshot-load compatibility (type-layout argument, A4);
  purity of every helper below `review_facts` (bodies inspected are pure; I did not
  line-by-line trace `render_dossier`/`effective_action` callees — none take
  `&mut` or do I/O by signature).

## Summary

| Item | Question | Verdict |
|---|---|---|
| A1 | `PolicyWithheld` leak? | **CONFIRMED** — path-vs-content bit preserved; policy-vs-technical bit was already served by the read gate; no new oracle |
| A2 | Non-sensitive splits safe? | **CONFIRMED** (`PlatformPathCollision` dead variant noted) |
| A3 | Missed mappings? | **CONFIRMED** none; residual `Unreadable → UnsupportedLanguage` mislabel found |
| A4 | Snapshot/embed compat? | **CONFIRMED** — `SkipReason` never serialized; embed exports a different type |
| A5 | Tests load-bearing? | **PARTIALLY WRONG** — Display-only at `d87c748`; uncommitted drift adds the real projection test |
| B | Reorder safe? | **safe-with-modification** — fast replay-dir probe without the plan; keep slow path + re-check; bonus: drop the duplicate outside-lock plan |

**Item A: land it.** The change removes a real false diagnosis and preserves the
one bit the threat model protects. **Item B: ship the fast probe, not the naive
reorder** — and consider deleting the duplicate plan computation on the recovery
path while in there.
