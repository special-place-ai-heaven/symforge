# MCP Contract: `search_knowledge`

**Status**: V11 refreeze candidate (2026-08-11; non-conflicting V10 evidence retained)<br>
**Surface**: Full<br>
**Mutation**: Read-only<br>
**Frecency**: Must not bump

MCP annotations: `readOnlyHint=true`, `destructiveHint=false`,
`idempotentHint=true`, `openWorldHint=false`.

## Purpose

Retrieve exact, bounded repository knowledge evidence without exposing prose as
code symbols or requiring broad file reads.

## Input

```json
{
  "query": "why is shutdown not a persistence boundary",
  "path_prefix": "docs/",
  "source_scope": "current",
  "authority_scope": "default",
  "project": "symforge",
  "limit": 10,
  "max_tokens": 2500
}
```

Fields:

| Field | Required | Contract |
|---|---:|---|
| `query` | yes | Non-empty natural-language or identifier query. |
| `path_prefix` | no | Normalized repository-relative prefix; no traversal. |
| `source_scope` | no | `current` (default), `worktrees`, `local_refs`, or `all`. |
| `authority_scope` | no | `default`, `current`, `intent`, `history`, or `all`; see authority behavior below. |
| `project` | no | One open project id/alias; mutually exclusive with `projects`. |
| `projects` | no | Explicit open-project ids/aliases or `["*"]`; mutually exclusive with `project`. |
| `limit` | no | Default 10; bounded by server policy. |
| `max_tokens` | no | Post-lease response budget; truncation preserves the complete claim/evaluation envelope and CCR recovery. |

The tool intentionally omits regex, AST patterns, code language filters, ranking
weights, embedding knobs, and parser selection. A server advertises/accepts only
source-scope values implemented by its capability version; requesting an
unavailable P1 scope is an explicit unsupported-scope error, never a complete
no-evidence response.

Every `authority_scope` value filters knowledge voice only; none selects or proves a
lifecycle generation. `default` includes current, intent, needs-review, and unknown
units with exact labels while excluding history-only/suppressed units. `current`
excludes intent/history but keeps needs-review/unknown labeled so unclassified
repositories do not become falsely silent. `intent` and `history` select only their
named voices. `all` returns every security-permitted unit without promoting its
authority.

## V11 lifecycle acquisition

The V10 query, ranking, security, and evidence-shape rules remain applicable after one
new mandatory gate. `authority_scope` is a `KnowledgeVoiceFilter` evaluated **inside**
an already-acquired lifecycle `Current` generation; it is never a generation-
consistency selector. In particular, the wire value `current` means current-
implementation voice, not lifecycle `Current`.

Before searching, the project registry freezes the authorized selected-source set in
a sealed selection receipt and acquires one strict `Current` lease for every selected
source. `Loading`, `Refreshing`, `Blocked`, `Stopping`, `Gapped`, or verification-
overdue state returns the one exact `SourceRefusal`: `SourceUnavailable` for one
resolved source or `SelectionUnavailable` with bijective per-source evidence for a
selected set. Invalid or unauthorized selection retains its indistinguishable
`InvalidSelection` shape. A retained verified generation remains internal recovery
material and cannot supply hits, coverage, or a no-match result.

A successful empty result is therefore an absence claim over the exact sealed
selection and is legal only when every selected required source was acquired as
`Current`. The V10 degraded/last-valid response rows below are retained only as
historical evidence and are explicitly superseded by this gate.

## V11 claim envelope and authority lane

Every successful result is one operation-specific `Claim<SearchKnowledgeResult>` with
an opaque `OperationReceipt`, the full `ClaimProvenance`, and the producing
runtime/publication identity; every `SourceRefusal` carries the same operation
receipt. The operation receipt binds the normalized query, selectors, voice filter,
limits, and every value-affecting algorithm/policy version. Hits use `Generation`
authority only because their bytes and required knowledge artifacts come wholly from
the captured strict `Current` leases. A complete empty result and selection-wide
counts use the private `SelectedAggregate` constructor with the exact leased-source
bijection.

`DiskObservation`, complete `WorktreeScopeObservation`, and `GitObservation` belong
to explicit pure observation tools and may remain responsive while a generation is
non-current. A disk receipt may establish path-local bytes/metadata/missing at its
observation time; a worktree-scope receipt may establish completeness only for its
sealed declared scope and interval; a Git receipt may establish membership/non-
membership only in its exact object/tree. None establishes lifecycle `Current`,
generation membership, or generation/repository-wide completeness/absence, and
`search_knowledge` never substitutes one for a missing generation lease or selected-
scope no-match. An operation that later relates those authorities must use a typed
`Comparison`/`Derivation`. Health/status is runtime-publication evidence, not a search
claim or a generation-read bypass.

The ranking Adapter captures one immutable `RankingSnapshot` after strict source
acquisition. Because result order and ranking explanations are observable, every
success carries its `EvaluationProvenance`; ranking never establishes readiness or
source truth. Human-readable text, structured content, cache keys/values, persisted
results, CCR handles, and retrieval round trips preserve the identical operation,
claim, and evaluation envelope.

## Query interpretation

1. Preserve the complete trimmed query for exact phrase matching.
2. Derive bounded significant terms using deterministic stopword/token rules.
3. Search only files whose captured target is `Knowledge` or `CodeAndKnowledge` and
   filter units by authority scope before ranking. Catalog-only entries never carry
   an empty or synthetic target.
4. Snapshot the authorized project/worktree selection, freeze its exact source IDs in
   a selection receipt, and acquire one immutable strict-`Current` generation lease
   for every selected source from each `ProjectInstance` at query start.
5. Rank by exact phrase, heading/title, distinct term coverage, source precedence,
   then canonical path/line tie-break. Document authority is a separate filter/
   label, never conflated with current-worktree precedence. Diversity is added only when a
   failing corpus fixture proves same-file flooding. Capture one immutable
   `RankingSnapshot` and its `EvaluationProvenance` before ranking; no formatter,
   cache, or CCR path may reopen mutable ranking state.
6. Format only from the captured leases and their selection receipt. A query never
   reloads live state to “verify” a hit against a newer generation.

## Successful response

Human-readable default shape:

```text
Trust: exact source evidence | publication 42 | content 42 | current | coverage complete
Secret policy: version 1
Scope: current worktree + docs/

1. docs/architecture/recovery.md:37
   Recovery > Persistence boundaries
   "Shutdown is not a safe persistence boundary."
   source=current@working-tree hash=<bounded-id>
```

Every hit MUST include:

- source/worktree/ref label;
- repository-relative `path:line`;
- exact excerpt, or no hit plus an incremented withheld count;
- heading breadcrumb/unit range when available;
- content hash/object identity;
- published generation;
- lifecycle, authority domain, deterministic checked-code display, retrieval voice,
  stable finding/provenance IDs (including safe rule/policy IDs), and evidence
  coverage;
- stable link IDs plus bounded exact/declared-set/ambiguous/missing bridge-anchor
  previews when present.

The human-readable header and structured result also expose the same opaque
`OperationReceipt`, complete `ClaimProvenance`, producing publication identity, and
required `EvaluationProvenance`; neither representation is authoritative without the
other envelope fields.

Search never embeds `CodeEvidenceSummary` arrays or full bridge records. Their stable
IDs resolve through `review_knowledge`, which returns the bounded source-local
dossier. Direct and CCR search results preserve the compact display, IDs, preview
anchors, and the complete claim/evaluation envelope. Every display ID and preview
vector is canonically ordered and response-bounded after lease acquisition, with an
explicit omitted count/coverage state and redeemable CCR identity when applicable.

Top-level response MUST include:

- a deterministic per-source list of source identity, captured source version
  (including `Clean`/`Dirty`/`NotApplicable`/`Unknown` working-tree state),
  publication/content generations, lifecycle `Current` proof, coverage, and manifest
  digest;
- active secret-policy version;
- overall coverage equal to the worst included source;
- source scope searched;
- overflow/truncation count;
- withheld-sensitive count;
- authority-filtered counts by history-only/suppressed/review-required/unknown;
- role/bridge/authority/temporal derived coverage and policy/rule versions;
- deterministic no-match reason when empty.

All required knowledge, suppression, authority, bridge, and temporal artifacts were
complete before the source could become `Current`. If candidate construction exceeds
capacity or truncates any required artifact, the breach remains attempt-only: discard
the candidate, retain bounded attempt accounting, and make this tool return strict
`SourceRefusal`. Only post-lease response budgeting, or an optional artifact omitted
from protocol advertisement before lifecycle startup, may bound or truncate only work
outside `RequiredArtifactSet`. Such optional work produces no candidate artifact,
capability, authority, or public claim; neither case changes generation completeness
or permits a selected-scope absence claim.

## No-match classes

| Class | Meaning |
|---|---|
| `no_evidence_complete` | Every source in the sealed selection is `Current` with complete required coverage and contains no match. |
| `no_evidence_degraded` | V10 historical value; V11 never emits it. An unavailable selected source returns `SourceRefusal` before search, not a no-match claim. |
| `evidence_withheld` | Candidate evidence exists but security policy withheld it. |
| `evidence_noncurrent` | Matching evidence exists but the `KnowledgeVoiceFilter` excluded it; “noncurrent” names voice filtering only, not lifecycle state. Returns safe counts/guidance, not excerpts. |
| `query_too_weak` | Deterministic tokenization produced no useful term. |

## Error classes

| Error | Behavior |
|---|---|
| invalid path/source/authority scope | Reject with actionable valid values. |
| index scouting/verifying | Return exact `SourceRefusal`; never stale “complete” evidence. |
| degraded last-valid source | V10 behavior superseded: return exact `SourceRefusal`; the retained generation is internal and never formatted. |
| stored CCR generation unavailable | A later CCR retrieval whose captured source generation was evicted returns an explicit stale/retryable result. In-call generation change is impossible because the strict generation lease is pinned. |
| corrupt snapshot/no valid source | Return recovery guidance; never serve quarantined data. |
| output budget too small | After strict lease acquisition, return the complete operation/claim/evaluation envelope plus bounded counts/CCR, or a validation error. |

## Security

- Sensitive catalog entries are counted but are not content candidates.
- The raw query is scanned before tokenization; a positive/indeterminate scan is
  rejected without echo and never enters analytics, cache state, diagnostics, or
  CCR.
- A final deterministic output guard withholds a detector-positive hit whole; it
  never rewrites a value inside an exact excerpt.
- Path, source label, heading, excerpt, diagnostics, and ranking explanations are
  independently guarded. Unsafe path text becomes an opaque catalog ID.
- Stable detector-positive files are demoted to metadata-only before publication;
  transient bytes and hashes are discarded from both code and knowledge targets.
- Only a constructed `SafeHit` may be formatted or budgeted. CCR stores already-
  safe formatted output tagged by secret-policy version; mismatch refuses/rechecks.
- Diagnostics identify only safe path, line, rule ID, and count.

## Compact/facade routing

The compact surface remains exactly `symforge`, `symforge_edit`, and `status`.
Knowledge intent routed through `symforge`/`ask` internally returns this contract's
result shape. The facade must not route symbol/reference questions to knowledge,
and every V11-emitted knowledge no-match class remains a successful response rather
than an MCP/protocol error after strict acquisition. `SourceRefusal` is a typed
readiness/selection result, never a no-match class. A CCR result produced on the compact surface stores a footer that
names the `symforge` facade retrieval intent plus hash; that route redeems the same
CCR record without advertising `symforge_retrieve`. Compact routing is release-gated
on these decode/mapping tests.

## Compatibility

- Existing `search_text` remains code-scoped by default.
- Existing `search_symbols` remains code-scoped.
- Existing `get_file_content` remains the deep-read path after a knowledge hit and
  serves bytes owned by the same captured `Current` generation; its repeat-cache
  identity includes project, source, publication generation, and content generation.
- Tool discovery/guidance additions must not bump frecency.

## Contract tests

1. Schema exposes only the eight fields above and read-only annotations.
2. Empty/whitespace query rejects deterministically.
3. Prose hit has exact path/line/heading/hash/generation.
4. Config targeted to both lanes is returned.
5. Sensitive result is withheld without value leakage.
6. Any selected non-Current, `Gapped`, or verification-overdue source returns the
   exact `SourceRefusal` before hits or no-match; retained generations are not visible.
7. Output truncation retains complete provenance and CCR handle.
8. Ranking is byte-for-byte deterministic over repeated equal generations.
9. Current worktree ranks ahead of a divergent ref but does not hide it.
10. The compact surface count remains three.
11. `source_scope=all` returns per-source publication/content generations and
    digest/coverage for the sealed all-`Current` selection. Mixed readiness returns
    bijective `SelectionUnavailable` evidence and no partial success.
12. Compact/facade no-match after all-selected-`Current` acquisition is a successful
    result, never a protocol error. A
    truncated compact result's facade retrieval intent redeems its CCR hash.
13. Sensitive query rejects without echo; path/heading/context/source fields are
    each guarded independently.
14. Detector-positive and indeterminate files have no hit/content hash; direct,
    CCR, analytics, diagnostics, and snapshot runtime-canary checks are negative.
15. CCR secret-policy mismatch refuses or revalidates before retrieval.
16. Default/current/intent/history/all scopes produce deterministic distinct sets;
    filtered history/suppressed matches yield `evidence_noncurrent`, not false
    complete no-evidence.
17. One mixed unit returns a deterministic compact display plus stable finding/rule
    IDs; following those IDs through `review_knowledge` returns every bounded
    checked/conflict/change/unresolved evidence array. A complete successor candidate
    may reorder records, but stable content-derived finding/provenance IDs resolve to
    the same dossier when their underlying authority is unchanged; Current is never
    mutated by a derived-only publication.
18. Stable link IDs and bounded exact/declared-set/ambiguous/missing bridge previews
    survive formatting, truncation, cross-project envelopes, and CCR; full bridge
    records remain available through `review_knowledge` without corpus duplication.
19. Clean, dirty, immutable-ref, and inspection-unknown sources preserve their
    captured source version in every per-source envelope; manifest/content digests,
    not branch/timestamp/state labels, remain exact content identity.
20. Every `authority_scope` wire value changes only the `KnowledgeVoiceFilter` within
    the same captured `Current` generation; no value enables degraded, retained, or
    last-verified generation consistency.
21. Truncated or capacity-exhausted required knowledge/suppression/authority/bridge/
    temporal work discards the candidate and causes strict refusal. Only post-lease
    presentation may truncate a public value; bounded optional work for an explicitly
    unadvertised feature stays outside `RequiredArtifactSet` and produces no candidate
    artifact or public claim.
22. Text, structured content, caches, persistence, CCR, and retrieval preserve the
    same `OperationReceipt`, full `ClaimProvenance`, producing identity, and required
    `EvaluationProvenance`; an observable ranking cannot be returned without it.
23. Pure disk, complete-worktree-scan, Git, and health/runtime evidence remain
    distinctly typed and cannot satisfy `search_knowledge` generation acquisition,
    completeness, or no-match authority.
