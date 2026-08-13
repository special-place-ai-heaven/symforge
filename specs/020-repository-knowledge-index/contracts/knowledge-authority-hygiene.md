# MCP Contract: Knowledge Authority and Hygiene

**Status**: V11 refreeze candidate (2026-08-11; non-conflicting V10 evidence retained)<br>
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

## V11 lifecycle acquisition and voice filtering

The V10 hygiene, proof, policy, security, and remediation evidence remains applicable
after strict lifecycle acquisition. `search_knowledge`, `review_knowledge`, map/context
views, and curation preview acquire lifecycle `Current` for every required selected
source through one sealed project query lease, or what a `Refreshing` RELOAD retains,
per F020-V11-A20. `Loading`, `Blocked`, `Stopping`, a permit-issuing `Refreshing`,
`Gapped`, and verification-overdue return the exact `SourceRefusal`, one source or
`SelectionUnavailable` with bijective evidence for a selected set. Invalid or
unauthorized selection retains its indistinguishable `InvalidSelection` form.

A retained verified generation remains byte-identical internal recovery material and
cannot supply findings, counts, remediation coverage, an empty result, or degraded/
last-verified evidence. An empty review or search claim is legal only after every
selected required source is acquired as `Current`.

Every `authority_scope` wire value is parsed as a `KnowledgeVoiceFilter` **inside**
that acquired `Current` generation. The wire value `current` means current-
implementation voice, not lifecycle `Current`, and no voice value selects generation
consistency. Persistence health remains orthogonal to read truth; curation apply may
still require durable replay and atomic-write capability in addition to lifecycle
`Current`.

## V11 claim and observation envelope

Every public review/search/preview success is one operation-specific `Claim<T>` with
an opaque `OperationReceipt`, the full `ClaimProvenance`, and the producing
runtime/publication identity. Every `SourceRefusal` carries the same operation
receipt. Generation findings are legal only when all of their bytes, policy, bridge,
authority, temporal, and suppression evidence comes from the strict `Current` leases.
A live path read, complete worktree scan, or immutable Git read is instead a
`DiskObservation`, `WorktreeScopeObservation`, or `GitObservation`; a pure observation
may run while generation state is non-current. A disk receipt may establish path-local
bytes/metadata/missing at its observation time; a worktree-scope receipt may establish
completeness only for its sealed declared scope and interval; a Git receipt may
establish membership/non-membership only in its exact object/tree. None claims
generation membership, lifecycle `Current`, or generation/repository-wide
completeness or absence. A relation between authorities is a typed
`Comparison`/`Derivation`, and selection-wide totals or empty results use
`SelectedAggregate` with the exact leased-source bijection.

A curation apply/recovery result is a mutation receipt rather than a generation-read
success. It still carries its `OperationReceipt`, exact write/terminal receipt, and
producing runtime publication; any source-truth field it includes carries the full
allowed `ClaimProvenance` and cannot call the source `Current` before the fenced
successor promotes.

The ranking Adapter captures one immutable `RankingSnapshot` after authority
acquisition. Any observable ordering, score, confidence ordering, or ranking
explanation carries its `EvaluationProvenance`; ranking evidence never establishes
source truth or readiness. Human-readable text, structured content, cache entries,
durable persistence, CCR handles, and retrieval round trips preserve the identical
operation, claim, and evaluation envelope. A formatter may shorten post-lease output,
but cannot drop, replace, or synthesize those fields.

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

Temporal evidence sealed into a complete generation has `Generation` authority. A
fresh filesystem value, complete root scan, or Git-object/ref value observed outside
that generation retains its own `DiskObservation`, `WorktreeScopeObservation`, or
`GitObservation`; using it with generation structure requires an explicitly allowed
`Comparison`/`Derivation`. Health/runtime telemetry is neither temporal source truth
nor a substitute authority and cannot upgrade an observation to `Current`.

File mtime/birth time, human review timestamps, and remembered generations can
produce only `review_due`; an exact linked-code change after the document commit may
produce `relevant_code_changed_since_document`. Neither proves semantic conflict.

## Retrieval voice

`search_knowledge.authority_scope` accepts the following `KnowledgeVoiceFilter`
values after strict `Current` acquisition:

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
it. No scope bypasses secret policy or selects a retained/non-current generation.

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
`max_tokens`. The tool first acquires the exact sealed all-`Current` selected-source
lease, captures its immutable generation bundles once, and returns per-source
identity/version/generation/digest/coverage plus a deterministic per-source
`review_hash` over that source's complete untruncated plan and one deterministic
top-level result hash. Source version includes closed clean/dirty/not-applicable/
unknown working-tree state; exact manifest/content digests remain byte identity.
The result is a `Claim<ReviewKnowledgeResult>`; observable remediation ordering
requires `EvaluationProvenance`. Output truncation happens only after the strict lease
and complete result hashes exist, and text/structured/CCR/retrieval forms preserve the
full claim envelope plus action/finding/link IDs and evidence locations.

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
as read-only capabilities they still require strict `Current` acquisition. Apply
returns the typed reason without reserving a key or touching the ledger. This
availability split applies only after strict lifecycle `Current` read acquisition;
non-current state returns `SourceRefusal` before review, preview, or mutation planning.

Every curation repository-content write is additionally gated by the shared,
non-cloneable `SourceMutationPermit`. The permit owns the exact binding,
`PhysicalRootLease`, and mutation epoch; granting it invalidates old-epoch candidates
and atomically publishes `Refreshing` before the first source-root side effect. Durable
replay/intent records remain in `ProjectStateDir`, but they cannot authorize source
I/O or substitute for the permit. `start_side_effect` produces the sole in-flight
authority and a validated final-parent handle through component-by-component
beneath-root, no-follow/reparse-safe traversal. A platform without an equivalent
root-confined primitive reports typed unavailability and performs no destructive I/O.

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
    It may then run the `ProjectStateDir` replay/intent durability probe, but performs
    no source-root probe and does not reserve the idempotency key yet;
4. while holding that lock, rejects path traversal, symlinks, unsafe paths, file move/
    delete actions, stale review/manifest/policy/target hashes, unknown action IDs, and
    every action that no longer reproduces from the captured source;
5. acquires one `SourceMutationPermit` for the captured binding/root/policy path.
   Permit grant advances the mutation epoch and publishes non-current `Refreshing`;
   it then calls `start_side_effect`, whose exact publication/permit/root checks and
   handle-relative component traversal return an in-flight authority and validated
   final-parent handle before any repository-content write;
6. through that handle, runs any first-use ledger-parent durability probe, then reads
   and revalidates the on-disk ledger immediately before mutation and computes
   canonical pre- and post-images. A stale or unsafe state may terminally resolve the
   permit with `NoSideEffectProof` only when neither a source-root probe nor any other
   source side effect began;
7. only after both required probes and the source revalidation succeed, durably
   reserves the request-hash record with repository/source continuity proof plus the
   guarded manifest/policy digests, then advances it to `pending_write` with the
   canonical request/mutations, curation continuity binding, exact post-image bytes,
   and pre/post digests. Replay-journal `write_all`, file `sync_all`, atomic replace,
   and parent-directory durability complete before the source ledger is touched;
8. through the same validated parent handle, writes the post-image to a same-directory
   create-new temp file with `write_all`, verifies its digest, calls file `sync_all`,
   atomically replaces the ledger, and completes the platform's required parent-
   directory durability operation;
9. durably records the exact success result, syncs that record and its parent as
   required, and removes only safely attributable source-root temp state through the
   same validated parent handle;
10. commits the permit with the exact write receipt. That terminal lifecycle handoff
    schedules the fenced complete successor candidate before the lock is released and
    returns applied/pending-generation evidence without overriding lifecycle state.
    Watcher delivery is an observation input, not the authority that first makes the
    source non-current.

Same source binding/key/hash replays the stored result; same key/different hash fails
deterministically. Any precondition failure writes no ledger bytes; after durable
reservation it records only the typed terminal failure. A policy write failure leaves
the previous complete ledger. A failure before permit grant leaves the prior runtime
publication untouched. After permit grant, success, drop, panic, or failure after any
source side effect terminally hands lifecycle a fenced refresh and remains non-current
until verified candidate promotion. Even a valid `NoSideEffectProof` returns through a
fenced verification/no-op candidate at the latest observer cut and mutation epoch and
installs fresh safety authority; it never restores `Current` directly. Apply never
edits, moves, or deletes a target document.
Concurrent curators cannot validate against the same old policy and both write; the
second observes the first under the shared lock and deterministically replays or
fails a freshness guard.

Startup and next-use recovery run under that same lock. While the source is `Loading`,
`Refreshing`, `Blocked`, or otherwise non-current, recovery may read only the durable
pending/replay metadata in `ProjectStateDir`. It does not inspect, probe, sync, clean
up, or write source-root ledger/temp paths and cannot acquire a
`SourceMutationPermit`. The ordinary candidate pipeline must first observe the source,
seal all required artifacts, and promote a complete lifecycle-`Current` generation.
If it cannot do so, pending recovery remains read-only/deferred and strict operations
return `SourceRefusal`; recovery never mutates a non-current source to make it
queryable.

After `Current` promotion, recovery verifies that the live `RepositoryId`/`SourceId`
match the durable binding. Git continuity additionally requires the recorded object
format and anchor tip to remain resolvable as a commit in the live object database.
Non-Git continuity uses the data model's bounded open-handle `PlatformFileId` encoding
of the canonical root plus prior-to-current catalog-digest transitions appended by
accepted publication and applied/recovered curation to the durable `ProjectStateDir`
replay store; a missing required link fails closed. Current tip/ref/history movement
alone is drift. Manifest/policy digests are first-execution freshness guards, not
source-sameness fields. A failed continuity proof returns `foreign_source_conflict`,
quarantines attributable pending intent in `ProjectStateDir`, writes no source-ledger
bytes, and cannot return the old stored success. A matching reserved record that never
reached `pending_write` reruns validation only against the acquired `Current` source.

For `pending_write`, the acquired generation's exact policy bytes/digest determine the
branch. A post-image match finalizes the durable success record and attributable
`ProjectStateDir` intent only; this is persistence-only, acquires no mutation permit,
performs no source-root I/O, and does not change lifecycle `Current`. A pre-image match
must acquire a fresh `SourceMutationPermit` from that exact `Current` binding before
any probe, cleanup, retry, or source write, then uses the same beneath-confined parent-
handle primitive. Any attributable source-root temp cleanup after a post-image match is
a separate Current-admitted permit operation, never part of persistence finalization.
Any third-state digest in the acquired generation becomes typed
`indeterminate_conflict` and is never overwritten; inability to read/verify the source
would have prevented `Current` and leaves recovery deferred. A persisted intent or
prior write receipt cannot substitute for the fresh permit. The recovered write
receipt, any failure after a source side effect, and a valid proof that none began all
return through the fenced successor/no-op candidate protocol and never restore
`Current` directly. Recovery covers crashes at reservation, intent sync, temp write/
sync, atomic replace, parent durability, or success-record sync. A platform that
cannot provide the tested atomic-replace and durability contract exposes curation
apply as unavailable rather than weakening it.

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
cleanup, and never touches durable records. The `ProjectStateDir` probe runs through
its typed state owner. The ledger-parent probe is repository-content I/O: it runs only
after a fresh permit has been acquired from `Current` and its grant has published non-
current, through the permit's validated final-parent handle, and before idempotency
reservation or ledger mutation. Crash-injection tests cover each boundary in the exact
production primitive. Unsupported filesystems,
failed probes, or an unavailable parent-durability operation yield
`Unavailable(AtomicDurabilityUnavailable)`. A failed source-root probe remains
non-current until a fenced complete candidate promotes because the probe itself may
have had side effects. Preview and review do not run a mutating probe.

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
of response budgets after strict lease acquisition sets explicit output coverage and
cannot publish a partial policy write or claim complete remediation coverage.
Suppression-bearing hash-valid policy entries and proven-divergence findings have
reserved derivation priority and are never silently dropped. If candidate capacity
cannot represent any required suppression, authority, bridge, temporal, or policy
artifact, the breach is attempt-only: discard the whole candidate, record bounded
attempt accounting, and refuse strict reads until a complete successor promotes. It
is forbidden to publish `Current` with skipped-suppression IDs or truncated required
coverage. Only post-lease response formatting may omit presentation items with exact
counts/CCR recovery. A feature omitted from protocol advertisement before lifecycle
startup is absent from `RequiredArtifactSet`; any bounded/truncated optional evidence
work for it remains non-authoritative outside the candidate and produces no
capability, generation artifact, or public claim. Neither case can establish
generation/repository-wide absence or weaken the generation certificate.

Malformed, unsupported-version, or hash-stale policy fails open only for raw safe
knowledge availability and fails closed for suppression/curation authority: code and
raw knowledge remain queryable with a policy finding, no stale policy entry keeps a
unit silent, and apply is unavailable until the ledger can be guarded safely. This
policy fail-open never bypasses strict lifecycle acquisition: a non-current source
still returns `SourceRefusal` and exposes no retained generation.

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
14. A watcher observation first makes the source non-current; bridge/authority/voice
    become queryable again only with the complete atomic successor generation.
15. Required temporal evidence completes inside the isolated candidate and becomes
    queryable only with the complete successor generation; no derived-only Current
    mutation exists.
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
23. Multi-source review returns per-source hashes only for the sealed all-`Current`
    selection; mixed readiness returns bijective `SelectionUnavailable` evidence.
    Apply accepts one lifecycle-`Current` source.
24. Concurrent curators serialize/revalidate; identical replay after apply or an
    intervening ordinary commit succeeds before now-stale freshness guards, while an
    unrelated same-path replacement fails continuity.
25. Explicit-protected/read-only bindings can review but cannot curate.
26. Unit targets use zero-based half-open byte offsets under the guarded whole-file
    hash, including CRLF, BOM, multibyte UTF-8, and no-final-newline fixtures.
27. Crash injection after reservation, pending-intent sync, temp write/sync, atomic
    replace, parent durability, and result sync first permits an ordinary complete
    `Current` promotion. A post-image match then finalizes only `ProjectStateDir`
    persistence; a pre-image retry acquires a fresh permit from that exact `Current`;
    third-state bytes produce `indeterminate_conflict` with no overwrite.
28. Missing durable replay or atomic file/parent durability disables apply with a
    typed reason while review and preview remain read-only capabilities. Each probe
    runs last under its typed owner; the ledger-parent probe runs only through an
    in-flight permit acquired from `Current` and after its `Refreshing` publication;
    disallowed sources receive zero probe I/O beneath their root. Review/preview still
    require strict `Current`;
    a `ProjectStateDir` persistence failure before permit grant does not revoke it,
    while a source-root probe side effect stays non-current pending its successor.
29. Default/current/intent/history/all values alter only `KnowledgeVoiceFilter` within
    one captured `Current` generation; none enables a degraded, retained, or last-
    verified read.
30. Exhausting required suppression/authority/bridge/temporal capacity discards the
    candidate and returns strict `SourceRefusal`; no published generation reports
    skipped required suppression. Only post-lease presentation may truncate a public
    value; bounded optional work for an explicitly unadvertised feature remains
    outside `RequiredArtifactSet` and produces no candidate artifact or public claim.
31. Apply and each source-writing recovery/cleanup branch acquire one non-cloneable
    `SourceMutationPermit` only from an exact `Current` binding, publish `Refreshing`
    before their first repository side effect, and perform every source-root probe/
    temp/write/replace/cleanup through beneath-confined final-parent handles. Startup
    non-current recovery never requests a permit; post-image finalization is
    `ProjectStateDir`-only. Commit, failure after a side effect, and a valid no-side-
    effect proof regain `Current` only by a fenced complete successor candidate.
32. Review/search/preview successes and refusals preserve one operation receipt and
    full claim provenance through text, structured content, caches, persistence, CCR,
    and retrieval; observable ranking/order/scores also preserve one immutable
    `EvaluationProvenance`. Pure disk/worktree-scan/Git claims and health never borrow
    generation `Current` authority.
