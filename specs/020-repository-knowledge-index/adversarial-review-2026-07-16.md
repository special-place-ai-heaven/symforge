# Adversarial Specification Review — Repository Knowledge Index

## Intent

Review a local-first, exact-evidence repository knowledge lane that shares one
metadata-first lifecycle with code indexing, cannot be choked by giant artifacts,
remains fresh/recoverable, and supports labeled worktree/local-ref sources without
adding embeddings, a vector database, or a second search store in v1.

Review execution:

- reviewer CLI: `claude` 2.1.211 (opposite-model requirement);
- three parallel read-only lenses: Architect, Skeptic, Minimalist;
- target: the complete feature-020 SpecKit plus relevant current Rust seams;
- requested `brain/principles.md` was absent, so reviewers use the repository's
  `AGENTS.md` mission, product, storage, recovery, and working-style principles as
  the explicit review-principles fallback; the missing optional resource is recorded
  rather than silently invented;
- first pass completed 2026-07-16; no reviewer modified repository files;
- full re-review required after accepted HIGH corrections.

## Verdict

**First pass: FAIL / not implementation-ready.**

The design direction withstood scrutiny, but three overlapping HIGH defects made
the original contract impossible or unsafe to implement:

1. case-collision/scout failures lacked representable per-file terminal states and
   could poison the whole repository;
2. in-flight permits were specified to cover all staged resident bytes, which can
   deadlock any corpus larger than the in-flight budget;
3. the single-source publication/data model could not represent the required
   worktree/local-ref source set or a truthful multi-source response envelope.

All three were accepted and revised. Implementation remains blocked until the
re-review section records a passing verdict.

## Findings

### Accepted HIGH findings

| ID | Finding | Lead correction |
|---|---|---|
| H-01 | Case-fold collision and non-I/O scout failures were generation-poisoning and not serializable/representable as `FileDisposition`. | Keep exact path identities distinct under a deterministic `(case-folded, exact-bytes)` order; isolate only a platform-unsafe entry; use owned serializable access/reason enums and total decision-to-disposition mapping. |
| H-02 | In-flight permit lifetime could deadlock staged cold load. | Permit covers read/verify/parse/hand-off and releases on transfer to staged-index ownership; admitted-content ceiling independently governs resident bytes. |
| H-03 | `PublishedGeneration`/`LiveIndex`/contract were singular while P1 requires multiple divergent sources. | Publish one immutable `PublishedSourceSet` containing per-source immutable bundles; all-source queries pin one set and report generation/digest/coverage per source plus worst overall coverage. |

### Accepted MEDIUM findings

| ID | Finding | Lead correction |
|---|---|---|
| M-01 | Check-then-swap fencing still allowed watcher/reconciliation lost updates. | Serialize commits under one writer boundary; long off-lock builds rebase/retry or abort when their base changed. |
| M-02 | Degraded coverage could remain permanently degraded behind equal-digest no-op. | Degraded state triggers bounded reconciliation backoff; only equal Complete observations are no-ops. |
| M-03 | Manifest dispositions and legacy `LiveIndex.skipped_files` formed two authorities. | Manifest is sole authority; remove or derive the legacy view and retire direct legacy mutations. |
| M-04 | Targets could be assigned before a probe and coexist with catalog-only plans. | Finalize targets inside the ingest decision; only `Indexed` terminal state carries targets. |
| M-05 | Hidden instruction/documentation paths were silently excluded. | Include repository-owned hidden knowledge trees; hard-exclude declared VCS/runtime internals and expose ignore-pruned coverage. |
| M-06 | Compact no-match could become an MCP error and recreate broad-read fallback. | Gate compact routing on successful no-match/decode tests; `ask` and facade no-match remain successful typed results. |
| M-07 | Sensitive-path accounting conflicted with path/content withholding. | Keep typed sensitive catalog counts and safe location metadata; never ingest bytes; withhold detector-positive hits whole rather than rewriting exact excerpts. |
| M-08 | Working-tree LFS pointers would become searchable knowledge noise. | Recognize with a bounded probe, store declared metadata, mark catalog-only, and never materialize or index pointer text. |
| M-09 | The secret detector/output boundary was underspecified and snapshots would retain detector-positive Tier-1 bytes. | Use a small versioned policy over existing bounded byte regexes; sensitive paths stop before reads, positive/indeterminate stable bytes are discarded before publication for both targets, and only constructed safe output may enter CCR. |

### Accepted LOW findings

- Add existing `project`/`projects` selectors to the knowledge tool instead of
  promising cross-project dispatch with an unscopable schema.
- Pin one immutable source set for the entire query; generation-change retry is
  meaningful only for later CCR retrieval of an evicted generation.
- Define catalog-entry-ceiling failure explicitly; never publish a truncated
  manifest as complete.
- Compute/cache the manifest digest once per debounced publication and measure
  event-storm cost before adding an incremental digest structure.
- Use a second bounded streaming hash pass to narrow same-stamp torn-write races;
  retain reconciliation as repair because hostile concurrent writers cannot be
  proven absent portably.
- V1 must project existing Markdown Section spans; no persisted duplicate
  `KnowledgeUnit` store.
- Keep code doc-comments out of knowledge v1.
- Remove redundant per-entry source identity from a single-source manifest.

### Partially accepted or rejected findings

| Proposal | Judgment |
|---|---|
| Delete local-ref ingestion from this feature. | **Rejected.** The user explicitly requires knowledge beyond the current branch. It remains P1, independently bounded, capability-gated, and unable to block current-worktree P0. |
| Advertise only current source forever. | **Rejected as end state; accepted during staged implementation.** Capability/schema values expand only after the worktree/local-ref phase passes its own gate; the release contract includes the completed P1 scopes. |
| Defer compact routing entirely. | **Partially accepted.** `ask` ships first; compact routing is allowed only after the known decode/no-match mapping is proven correct. |
| Reduce ranking to phrase + term + path only. | **Partially accepted.** Keep heading and current-source precedence because they directly serve structural knowledge and source trust; document authority remains a separate filter/label. Diversity requires a failing corpus fixture. |
| Add an incremental/Merkle manifest digest immediately. | **Rejected for v1.** Cache one digest per debounced generation and measure. A new structure requires failed cost gates. |

## What Went Well

- Reviewers independently source-verified the lifecycle defects; the feature is
  fixing observed failures rather than hypothetical architecture.
- Metadata-first admission, independent budgets, total disposition accounting,
  and zero reads for metadata-terminal artifacts were unanimously supported.
- Lifecycle-before-knowledge sequencing was unanimously supported.
- Reusing `FileClass::Text`, `SearchScope::Text`, trigram search, exact line
  rendering, and Markdown Section spans was unanimously preferred over another
  database/indexing pipeline.
- One immutable publication boundary, previous-generation retention, red-oracle
  gates, and the explicit complexity budget were judged strong.

## Lead Judgment

The product thesis is sound and valuable. The first draft was not yet “beyond
reproach” because its most important invariants were stronger in prose than in its
types and concurrency model. The accepted changes make the failure modes explicit
and implementable without expanding the search architecture.

The scope is intentionally asymmetric:

- P0 must make current-worktree catalog/code/knowledge safe, exact, fresh, and
  recoverable.
- P1 worktree/local-ref knowledge remains in the same feature because it is an
  explicit product requirement, but it is isolated behind its own source registry,
  budgets, capability gate, and failure boundary.

No production code may begin until opposite-model Architect, Skeptic, and Minimalist
all re-review the revised high-severity seams and the checklist records a passing
lead verdict. Any accepted HIGH finding requires correction and a fresh three-lens
run; partial lens reuse does not satisfy this gate.

## Re-review

Pending.
