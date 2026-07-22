# MCP Contract: Knowledge Authority and Hygiene

**Status**: Frozen (2026-07-17)<br>
**Read surface**: Full `review_knowledge`; existing map/search/context views<br>
**Mutation surface**: Full `curate_knowledge` (policy ledger only)<br>
**Prompt**: `symforge-knowledge-hygiene`<br>
**Frecency**: Audit/review/proposals do not bump; committed curation follows normal
mutation commitment policy for the policy file only

MCP annotations:

- `review_knowledge`: `readOnlyHint=true`, `destructiveHint=false`,
  `idempotentHint=true`, `openWorldHint=false`;
- `curate_knowledge`: `readOnlyHint=false`, `destructiveHint=true`,
  `idempotentHint=true`, `openWorldHint=false` (preview remains the default).

## Purpose

Prevent superseded or code-diverged implementation documentation from speaking as
current truth while preserving proposals, plans, ADRs, governance, ideation,
north-star direction, and explicit history in the correct authority view.

SymForge proves only closed-world, versioned facts. Unsupported semantic review is
performed by the calling agent/user from a bounded evidence pack and remains
advisory until an explicit policy action is approved.

## Independent state axes

Every supported file/section unit carries:

1. `lifecycle`: active/proposed/accepted/implemented/deferred/rejected/withdrawn/
   deprecated/superseded/archived/historical/unknown;
2. `authority_domain`: current implementation, normative intent, decision,
   operations, governance, historical record, or unknown;
3. `code_evidence`: an aggregate summary that can retain checked consistency,
   broken anchors, deterministic conflicts, implementation gaps, relevant-code-change
   signals, suspected conflicts, review signals, and unresolved semantics together;
   one precedence-derived display label never erases the underlying sets;
4. derived `voice`: current, intent, needs-review, unknown, history-only, suppressed.

Lifecycle always cites hash-valid policy or exact declared evidence. Code does not
assign lifecycle; in particular, code consistency cannot turn a proposal into
`implemented`. Voice is a pure versioned derivation, never arbitrary mutable input.
One unit's finding cannot suppress unaffected units in the same file.

## Deterministic proof matrix

| Evidence | Allowed conclusion | Forbidden conclusion |
|---|---|---|
| Exact internal path missing in the same source | `broken_anchor`; deterministic conflict only for a declared current-reference claim | Whole document stale; future proposal invalid |
| Exact code-spanned symbol resolved uniquely before and now missing | `broken_anchor` with before/current anchors | Semantic claim false without claim-domain evidence |
| Versioned structured extractor proves signature/schema/CLI/MCP/config mismatch | `deterministic_conflict` for the exact unit/field | Unchecked surrounding prose false |
| Explicit hash-valid lifecycle/supersession policy | Declared lifecycle and corresponding voice | Apply status to changed bytes or another source |
| Exact file/unit duplicate with retained target | High-confidence merge/deletion candidate, subject to blockers | Automatic deletion |
| Linked code anchor changed after the document commit | `relevant_code_changed_since_document` | Conflict, archive, or deletion |
| Filesystem/Git age or later commits | `review_due` clue | Stale, superseded, or wrong |
| Lexical similarity or LLM judgment | `suspected_conflict` advisory evidence | Deterministic finding or policy mutation |
| Intent/ADR/governance differs from code | `implementation_gap` | Stale intent/decision proof |

Rule IDs and versions are stable. An unrecognized claim is `unresolved`, never
silently consistent. Formatting-only code changes may invalidate verification but
cannot prove contradiction. Feature/platform/config ambiguity is explicit.

## Temporal evidence

The review may report:

- filesystem birth/creation and modification hints when supported;
- Git first-seen and last-touch commit/time;
- working-tree dirty/new state;
- exact linked code commits/anchor changes after the document commit.

Every value includes provenance and history coverage: complete-to-root, shallow,
bounded-window, rename-follow-limited, divergent, working-tree-only, or unavailable.
Commit topology outranks wall-clock comparison. Copies, rebases, checkout, clock
skew, and filesystem metadata prevent any timestamp from becoming proof by itself.

File mtime/birth time, human review timestamps, and remembered generations can
produce only `review_due`; an exact linked-code change after the document commit may
produce `relevant_code_changed_since_document`. Neither proves semantic conflict.

## Retrieval voice

`search_knowledge.authority_scope` accepts:

- `default`: current, intent, needs-review, and unknown evidence, each labeled;
- `current`: current and needs-review/unknown evidence, with labels; excludes intent
  and history/suppressed;
- `intent`: proposals/plans/north-star/decisions/governance as direction, never as
  implemented behavior;
- `history`: units whose derived voice is `HistoryOnly` or `Suppressed`, regardless
  of lifecycle label;
- `all`: every permitted unit, preserving its original voice.

Proven-divergent current-reference and explicit superseded/archived units are
suppressed from current/default voice. They remain retrievable only through
`history`/`all` or review. `needs_review` and `unknown` stay visible in current
answers but are labeled; otherwise sparse repositories would hide all
unclassified documentation. `ask` may select a scope from intent, but always states
it. No scope bypasses secret policy.

## Policy ledger

The canonical input is root `.symforge-knowledge.toml`:

```toml
version = 1

[[entry]]
entry_id = "<stable-opaque-id>"
path = "docs/example.md"
content_hash = "<bounded-content-id>"
unit_start_byte = 120
unit_end_byte = 260
unit_hash = "<bounded-unit-id>"
lifecycle = "superseded"
superseded_by = "docs/replacement.md"
justification_code = "explicit-successor"
```

Exact schema names may follow existing serde conventions, but semantics are fixed:

- canonical safe path + whole-file hash are required;
- unit byte range/hash are optional; omission explicitly targets the whole document;
- byte offsets are zero-based and half-open (`start <= byte < end`) into the exact
  whole-file bytes identified by `content_hash`; they are never character, line, or
  newline-normalized offsets;
- entries are canonically ordered and IDs are content-derived;
- a changed file/unit invalidates suppression and creates a stale-policy finding;
- supersession targets resolve in the same source; missing/cyclic chains are findings;
- native frontmatter/MADR/RFC/archive-path status is evidence, not a second writable
  authority; conflict is visible;
- the ledger cannot target itself and is never a knowledge-result candidate;
- refs/worktrees read their own ledger version from their own source.

The file is repo configuration, not `.symforge/` runtime state, snapshot truth, or a
secondary content index.

## `review_knowledge` (read-only)

Modes:

- `summary`: counts by lifecycle/domain/evidence/voice, coverage, oldest/review-due,
  broken/conflicting, duplicates, and protected categories;
- `document`: one exact safe path with unit-level dossier;
- `remediation`: ranked bounded proposals across a path/source scope.

Inputs are mode, optional path/path prefix/source scope/project(s), limit, and
`max_tokens`. The tool captures immutable source bundles once and returns per-source
identity/version/generation/digest/coverage plus a deterministic per-source
`review_hash` over that source's complete untruncated plan and one deterministic
top-level result hash. Source version includes closed clean/dirty/not-applicable/
unknown working-tree state; exact manifest/content digests remain byte identity.
Output truncation/CCR preserves action/finding/link IDs and evidence locations.

Each finding contains:

- stable finding/action/rule IDs;
- exact document unit and safe source/content generation/hash;
- lifecycle/domain/evidence/voice with provenance;
- the full bounded `CodeEvidenceSummary` arrays and bridge records referenced by
  compact search finding/link IDs;
- temporal values and coverage;
- exact code anchors and a bounded typed structured diff where proof exists. The
  diff contains rule ID, knowledge/code anchors, and secret-safe scalar values only;
  it never embeds an arbitrary raw document or source fragment;
- inbound current knowledge links and source-local ownership evidence;
- protected-role and unique-content checks;
- proposed smallest action, confidence basis, and unmet preconditions.

No finding may copy secret-positive content or turn an LLM conclusion into a rule.

## Remediation actions

Allowed proposals:

- `keep`;
- `update`;
- `relabel_intent`;
- `merge`;
- `mark_superseded`;
- `archive`;
- `deletion_candidate`;
- `needs_review`.

Age alone yields only `needs_review`. `deletion_candidate` is evidence-only. Its
strongest tier requires exact duplicate/reproducible content or explicit
supersession with exact retained unit coverage, no unique units, no unresolved live
backlinks, and no protected intent/ADR/governance/legal/security/north-star role.
Every failed check is returned. Feature 020 never moves or deletes a file.

## `curate_knowledge` (ledger-only mutation)

Preview is default. Input contains an explicit non-empty list of action IDs and
their exact policy mutations, plus:

- `if_source_review_hash` from exactly one current-working-tree source result;
- `if_manifest_digest`;
- `if_policy_digest`;
- per-target path/file/unit hashes inside each mutation;
- one current project selector;
- `idempotency_key` on apply;
- `apply=true` only after approval.

Preview revalidates and returns the exact canonical ledger diff with no write or
idempotency reservation. Apply is available only when the selected project's
reason-bearing curation capability is `Available`: one normal current working tree,
writable source, durable per-project replay/intent state, and tested file-plus-parent
atomic durability. Review and preview remain available when apply is unavailable;
apply returns the typed reason without reserving a key or touching the ledger.

Apply:

1. scans request strings with no-echo failure, validates only syntactic/schema
    safety, and resolves exactly one current working-tree project;
2. canonicalizes the request and current curation continuity binding, then probes
    existing idempotency state; source sameness requires matching `RepositoryId`/
    `SourceId` plus the continuity predicate below, never equality of moving ref/tip/
    history, manifest, or policy state. A failed continuity proof returns typed
    `foreign_source_conflict` before any stored result, while same source/key/hash
    returns the stored result and same key/different hash fails before freshness
    guards;
3. for a first execution only, acquires the same per-project policy mutation lock
    used by every curator and re-probes idempotency under the lock. It first evaluates
    normal-current-worktree, writable-source, and durable replay/intent requirements;
    an unavailable requirement returns its typed reason with no durability-probe I/O.
    It then runs the required probes in both durable-record directories and, only on
    success, durably reserves a request-hash record with repository/source continuity
    proof plus separately guarded manifest/policy digests before mutation;
4. while holding that lock, rejects path traversal, symlinks, unsafe paths, file move/
    delete actions, stale review/manifest/policy/target hashes, unknown action IDs, and
    every action that no longer reproduces from the captured source;
5. reads and revalidates the on-disk ledger immediately before mutation, computes
   canonical pre- and post-images, then durably advances the replay record to
   `pending_write` with the canonical request/mutations, curation continuity binding,
   exact post-image bytes, and pre/post digests; journal `write_all`, file `sync_all`,
   atomic replace, and parent-directory durability complete before the ledger is
   touched;
6. writes the post-image to a same-directory create-new temp file with `write_all`,
   verifies its digest, calls file `sync_all`, atomically replaces the ledger, and
   completes the platform's required parent-directory durability operation;
7. durably records the exact success result, syncs that record and its parent as
   required, removes only safely attributable temp state, and releases the lock;
8. triggers ordinary watcher/reconciliation publication and returns applied or
   explicitly pending-generation evidence without overriding published state.

Same source binding/key/hash replays the stored result; same key/different hash fails
deterministically. Any precondition failure writes no ledger bytes; after durable
reservation it records only the typed terminal failure. A policy write failure leaves
the previous complete ledger. Apply never edits, moves, or deletes a target document.
Concurrent curators cannot validate against the same old policy and both write; the
second observes the first under the shared lock and deterministically replays or
fails a freshness guard.

Startup and next-use recovery run under that same lock. Before inspecting ledger
bytes, recovery verifies that the live `RepositoryId`/`SourceId` match the durable
binding. Git continuity additionally requires the recorded object format and anchor
tip to remain resolvable as a commit in the live object database. Non-Git continuity
uses the data model's bounded open-handle `PlatformFileId` encoding of the canonical
root plus prior-to-current catalog-digest transitions appended by accepted publication
and applied/recovered curation to the durable `ProjectStateDir` replay store; a missing
required link fails closed. Current tip/ref/history movement alone is drift. Manifest/policy digests are
first-execution freshness guards, not source-sameness fields. A failed continuity
proof returns `foreign_source_conflict`, quarantines attributable pending intent,
writes nothing, and cannot return the old stored success. A matching reserved record
that never reached `pending_write` reruns validation. For `pending_write`, a ledger matching the
stored post-image re-establishes required file/parent durability before finalizing
success; one matching the pre-image safely repeats the stored post-image write; any
other or unreadable state becomes a typed
`indeterminate_conflict` and is never overwritten. Recovery is required after a
crash at reservation, intent sync, temp write/sync, atomic replace, parent durability,
or success-record sync. A platform that cannot provide the tested atomic-replace and
durability contract exposes curation apply as unavailable rather than weakening it.

### Tested atomic-durability contract

The ledger temp file is always create-new in the ledger's directory; cross-volume
replacement is forbidden. Probe evaluation is last: only a first apply use whose
normal current-worktree, writable-source, and durable replay/intent requirements are
already `Available` may run it. Explicit-protected, read-only, ref, implicit-worktree,
and memory-only bindings return their typed reason with zero probe file operations
anywhere beneath the source root. Apply is `Available` only when the shipped platform
path passes a first-use same-directory capability probe in both the ledger parent and
the `ProjectStateDir` replay/intent-journal parent, deduplicated if identical:

| Platform | Required successful sequence |
|---|---|
| Unix | temp `write_all`; temp `sync_all`; same-directory atomic rename/replace; open and `sync_all` the parent directory |
| Windows | temp `write_all`; temp `sync_all` (`FlushFileBuffers`); same-directory `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`, or a documented equivalent with the same tested contract |

Each probe uses private disposable same-directory files, verifies replace/readback and
cleanup, and never touches durable records. Crash-injection tests cover each boundary in
the exact production primitive. Unsupported filesystems, failed probes, or an
unavailable parent-durability operation yield
`Unavailable(AtomicDurabilityUnavailable)` before idempotency reservation or ledger
mutation. Preview and review do not run a mutating probe.

## Prompt workflow

`symforge-knowledge-hygiene` instructs the calling agent to:

1. capture `review_knowledge` summary/remediation;
2. inspect only unresolved high-value units and exact code anchors;
3. distinguish implementation truth from intent/governance;
4. show the user evidence and proposed selected actions;
5. stop for approval;
6. preview `curate_knowledge`;
7. apply only the explicitly approved action IDs with fresh guards.

The prompt cannot embed approval, choose all candidates by default, or invoke file
deletion. Fable/other-model advice remains read-only unless the user separately
approves a policy mutation.

## Security and boundedness

Only post-secret-scan units may create authority records, bridge evidence,
findings, action IDs, policy evidence, diagnostics, analytics, or CCR output. Query
and every displayed field pass the output guard. The policy writer rejects evidence
whose safe IDs cannot be reproduced from captured state.

Catalog metadata, derived cards, bridge links, authority records/findings, temporal
walks, review output, and policy entries/bytes have independent limits. Exhaustion
sets explicit derived/review coverage and cannot publish a partial policy write or
claim complete remediation coverage.
Suppression-bearing hash-valid policy entries and proven-divergence findings have
reserved derivation priority and are never silently dropped. If a configured limit
still cannot represent one, every affected unit fails closed to voice `Suppressed`,
stays out of default/current, remains retrievable through history/all, and the response
reports canonically ordered skipped-suppression IDs and truncated coverage.

Malformed, unsupported-version, or hash-stale policy fails open only for raw safe
knowledge availability and fails closed for suppression/curation authority: code and
raw knowledge remain queryable with a policy finding, no stale policy entry keeps a
unit silent, and apply is unavailable until the ledger can be guarded safely.

## Contract tests

1. Five-year-old unchanged correct docs become at most `review_due`, never archived.
2. Newly written structured wrong current-reference unit is deterministic conflict.
3. Future proposal differing from code remains intent with implementation gap.
4. Accepted ADR conflict reports implementation divergence, not stale ADR.
5. One broken section does not suppress unaffected sections.
6. Missing, renamed, ambiguous, formatting-only, feature/platform cases differ.
7. Superseded content is absent from current and present in history.
8. Broken/cyclic supersession and native/ledger conflicts are visible.
9. Stale file/unit policy hashes stop suppression and require review.
10. Filesystem/Git time provenance and shallow/rename/dirty coverage are exact.
11. Exact duplicate may be deletion candidate; near duplicate/age alone may not.
12. Protected roles and live backlinks block deletion eligibility.
13. Current/intent/history/all results are deterministic and source-local.
14. Watcher updates bridge/authority/voice atomically with code/docs/policy changes.
15. Temporal completion republishes one coherent derived-only generation.
16. Review does not bump frecency.
17. Preview writes nothing; apply requires all guards and explicit action IDs.
18. Stale plan/file/policy, idempotency conflict, wrong worktree/ref, or any failed
    action causes zero ledger mutation.
19. `curate_knowledge` move/delete input is rejected by schema/validation.
20. Secret-positive content produces zero units, links, findings, actions, or output.
21. A code change after the document commit yields at most
    `relevant_code_changed_since_document`, never deterministic conflict.
22. Malformed/stale policy preserves raw code/knowledge and removes suppression while
    blocking unsafe curation.
23. Multi-source review returns per-source hashes; apply accepts one current source.
24. Concurrent curators serialize/revalidate; identical replay after apply or an
    intervening ordinary commit succeeds before now-stale freshness guards, while an
    unrelated same-path replacement fails continuity.
25. Explicit-protected/read-only bindings can review but cannot curate.
26. Unit targets use zero-based half-open byte offsets under the guarded whole-file
    hash, including CRLF, BOM, multibyte UTF-8, and no-final-newline fixtures.
27. Crash injection after reservation, pending-intent sync, temp write/sync, atomic
    replace, parent durability, and result sync recovers to the exact pre- or
    post-image; third-state bytes produce `indeterminate_conflict` with no overwrite.
28. Missing durable replay or atomic file/parent durability disables apply with a
    typed reason while review and preview remain read-only and available; probes run
    last in both durable-record directories, and disallowed sources receive zero probe
    I/O anywhere beneath their root.
