# Independent review request — two admission/security changes

**For:** a reviewer working directly against this repository (Cursor).
**From:** the Claude session of 2026-08-05.
**Branch under review:** `fix/policy-withheld-skip-reason`, commit `d87c748`.
**Base:** `origin/main` @ `93cdd00`.

Read this file fully before looking at code. It is self-contained.

There are **two independent items**. Item A is a change I made and want checked.
Item B is a change I *refused* to make and want a second opinion on. They share a
theme — both are about a fail-closed security boundary — but they can be reviewed
separately.

**Please try to break these, not confirm them.** Prior context: this campaign
produced twelve claims that did not survive verification, five of them mine,
including one change shipped and then reverted on measurement. Agreement without
independent evidence is worth nothing here.

---

## Item A — did I preserve the non-disclosure property? (commit `d87c748`)

### What was wrong

`src/live_index/store.rs` mapped **seven** distinct `MetadataOnlyReason`
variants onto one `SkipReason::UnsupportedLanguage`, which `Display`s as
`"unsupported language"`:

```rust
MetadataOnlyReason::SensitivePath { .. }
| MetadataOnlyReason::SensitiveContent { .. }
| MetadataOnlyReason::LfsPointer { .. }
| MetadataOnlyReason::PlatformPathCollision
| MetadataOnlyReason::UnsupportedPathEncoding
| MetadataOnlyReason::PathMetadataTooLarge
| MetadataOnlyReason::UnsupportedTextEncoding => SkipReason::UnsupportedLanguage,
```

So a **secret-detector verdict on perfectly valid TypeScript or Rust was reported
as a language problem**. An external report (TestPilot) hit this on two valid
`.ts` files and concluded TypeScript support was broken and that a file-size
policy was to blame. Neither was true — their files were 0.33 MB and 0.60 MB
against limits of 1 MiB (data) / 4 MiB (code) / 100 MiB (hard ceiling).

SymForge reproduces it on its **own** source: `src/live_index/store.rs`, 0.30 MB
of Rust, is withheld the same way.

### What I changed

```
SensitivePath | SensitiveContent   -> PolicyWithheld  (new)  "withheld by admission policy"
LfsPointer                         -> LfsPointer             "git-lfs pointer"
PlatformPathCollision
  | UnsupportedPathEncoding
  | PathMetadataTooLarge           -> UnsupportedPath         "unsupported path"
UnsupportedTextEncoding            -> UnsupportedTextEncoding "undecodable text encoding"
```

Plus: `SizeCeiling` no longer folds into the text-encoding bucket (it has a
correct home in `OversizedData`, which the *test helpers* already used while the
production path did not), and `None` — meaning "reason not recorded" — stays
neutral instead of being promoted into a substantive diagnosis.

### The property I claim to have preserved

`src/protocol/format.rs` (~line 3638) documents the read-path refusal as
deliberately uniform:

> Whether the exclusion came from the path rule or from a content detector is
> **the one content-derived bit a refusal could still leak**, and the recovery
> action is identical either way, so the message does not distinguish them.

My claim: `PolicyWithheld` keeps that property on the **index-side** surface —
`SensitivePath` and `SensitiveContent` still collapse together, so a caller
cannot tell whether a path rule or a content detector fired — while no longer
asserting a false statement about the file's language.

### What to attack

1. **Does `PolicyWithheld` leak anything?** Is there any path — `health`,
   `status`, catalog entries, manifest digests, MCP tool output, serialized
   snapshots — where the *presence* of `PolicyWithheld` versus another reason
   discloses something about file CONTENT that the old collapse hid? In
   particular: the old mapping put sensitive files in the same bucket as
   `UnsupportedTextEncoding` and `LfsPointer`. Splitting those out means
   `PolicyWithheld` is now a **narrower** set. Does narrowing it turn "this file
   is withheld" into "a detector or path rule fired on this file", and is that a
   meaningful disclosure? I judged it is not — the file being Tier-2 was already
   visible, and "policy applied" is what a caller must know to stop retrying —
   but this is the single most important question in this review.

2. **Is splitting the four non-sensitive reasons safe?** I judged
   `LfsPointer`, `PlatformPathCollision`, `UnsupportedPathEncoding`,
   `PathMetadataTooLarge` to be non-content-derived, so naming them leaks
   nothing. Is any of them in fact content-derived? `LfsPointer` detection reads
   file bytes (`detect_lfs_pointer` requires valid UTF-8 under 1 KiB) — does
   naming it disclose anything about content?

3. **Did I miss a mapping site?** I found and updated four
   (`store.rs` forward map, `store.rs` reverse map, `discovery/mod.rs` reverse
   map, plus two test helpers in `query.rs` / `tools.rs`). The compiler forced
   the last two. Are there non-exhaustive matches, `_ =>` catch-alls, or
   serialization paths that still conflate these?

4. **Snapshot/serialization compatibility.** `SkipReason` gained four variants.
   Does anything persist it, and would an older snapshot or a downstream
   embedder (AAP consumes `symforge::embed::*`) break? I did **not** verify this
   and it is a real risk.

5. **Are the two new tests actually load-bearing**, or do they pass vacuously?
   `policy_withheld_never_claims_a_language_problem` and
   `non_sensitive_skip_reasons_are_distinct_and_honest` in `src/domain/index.rs`.

### Deliberately NOT done

The TestPilot report asked for three things, all declined:

- expose the threshold and a machine-readable exclusion code — reopens the side
  channel above;
- permit bounded `get_file_content` reads for Tier-2 files — Tier-2 **includes**
  `SensitiveContent`, so this would disclose exactly what the gate withholds;
- a `force_admit` / lazy-parse override — a bypass of a security gate.

If you think any of these is actually safe, say so and show why.

---

## Item B — a change I refused to make: the ~3.8 s cold-start reorder

### The measurement

Instrumented on `origin/main` (PRs #521, #528), release build, genuinely-cold
index (fresh `git worktree add --detach`, no `.symforge/`):

```
serve: runtime built in                              3.8705102s
  serve: protocol/curation recover_on_project_load     3.8243504s   99.2%
    curation/published source set                          5.6µs
    curation/plan current                              3.820374s    99.9%
    curation/apply capability                            101.8µs
    curation/no replay dir — early return
  serve: protocol/routers                               18.8897ms
```

### The structure

`KnowledgeCurationCoordinator::recover_on_project_load`
(`src/protocol/knowledge_curation.rs` ~285):

```rust
let generation = index.published_source_set().current_generation();   //   5.6µs
let plan = curation_plan_current(&generation)?;                       // 3.82s
let state_dir = apply_capability(.., &plan.source.location)?;         // 102µs
let curation_dir = state_dir.join(CURATION_STATE_DIR);
let replay_dir = curation_dir.join(REPLAY_DIR);
if !replay_dir.is_dir() { return Ok(()); }                            // fires on a fresh project
```

`curation_plan_current` (`src/protocol/knowledge_review.rs` ~371) runs
`review_current` in Remediation mode, then selects **all** authority records
(`(0..generation.authority.records.len())`) and computes `review_facts` +
`effective_action` for every one.

On this path its only consumer is `apply_capability`, which uses
`plan.source.location` for a **single** check:

```rust
if !matches!(source_location, SourceLocation::WorkingTree { .. })
```

So: a full remediation review runs for 3.8 s to feed one enum match, and is then
discarded because there is no replay directory to apply anything to. The
microsecond existence check sits *after* the expensive computation.

### The proposed fix

Hoist the `replay_dir` existence check above `curation_plan_current`, and resolve
the source location from `PublishedGeneration.source` (already set cheaply in
`store.rs` from `manifest.source`) instead of via the plan.

### Why I did not ship it

`recover_on_project_load` is a **fail-closed integrity path**. Its own error
strings say so:

- `"knowledge curation startup recovery remained fail-closed"`
- `"curation_startup_publication_failed; live queries remain on the last complete generation."`

Reordering a fail-closed check to make startup faster is exactly where an
unaccounted side effect turns a performance win into a correctness regression.

### What to determine

1. **Does `curation_plan_current` have side effects** that other startup
   behaviour depends on? It looks pure (it builds a plan from a generation), but
   `review_current` is a large call and I did not trace it exhaustively.
2. **Is `PublishedGeneration.source` equivalent to `CurationReviewPlan.source`**
   for `apply_capability`'s single `matches!` check?
3. **Is the reorder actually safe**, or does the current order encode an
   invariant — e.g. that the plan is validated before any state directory is
   touched or created? Note `apply_capability` may CREATE the state dir.
4. If it is not safe as proposed, **is there a cheaper correct variant** — e.g. a
   fast probe for the replay directory that does not need `plan.source.location`
   at all?

---

## Deliverable

A markdown file, `specs/021-admission-coverage-honesty/REVIEW-FINDINGS-cursor.md`:

- **Item A verdict** per numbered question: CONFIRMED / PARTIALLY WRONG /
  REFUTED, each with file:line evidence or a command and its output.
- **Item B verdict**: is the reorder safe, unsafe, or safe-with-modification?
  If unsafe, name the invariant it breaks.
- Anything I missed in either.
- If your honest answer to Item A is "this leaks something", say so plainly —
  that is the most valuable outcome available here, and the change should then
  not land.

## Ground rules

- Read the code before concluding. Every wrong claim in this campaign came from
  reasoning about behaviour instead of reading the implementation.
- Do not modify the repository except to write the findings file.
- The full serial suite is ~16-25 min (`cargo test --all-targets --
  --test-threads=1`); say whether you ran it rather than assuming.
- Distinguish what you verified, what you inferred, and what you are guessing.
