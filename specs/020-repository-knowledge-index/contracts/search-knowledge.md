# MCP Contract: `search_knowledge`

**Status**: Frozen (2026-07-17)<br>
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
| `max_tokens` | no | Bounded response budget; truncation preserves provenance. |

The tool intentionally omits regex, AST patterns, code language filters, ranking
weights, embedding knobs, and parser selection. A server advertises/accepts only
source-scope values implemented by its capability version; requesting an
unavailable P1 scope is an explicit unsupported-scope error, never a complete
no-evidence response.

`authority_scope=default` includes current, intent, needs-review, and unknown units
with exact labels; it excludes history-only/suppressed units. `current` excludes
intent/history but keeps needs-review/unknown labeled so unclassified repositories
do not become falsely silent. `intent` and `history` select those voices. `all`
returns every security-permitted unit without promoting its authority.

## Query interpretation

1. Preserve the complete trimmed query for exact phrase matching.
2. Derive bounded significant terms using deterministic stopword/token rules.
3. Search only files whose captured target is `Knowledge` or `CodeAndKnowledge` and
   filter units by authority scope before ranking. Catalog-only entries never carry
   an empty or synthetic target.
4. Snapshot selected project/worktree handles, then capture one immutable published
   source set from each selected `ProjectInstance` at query start.
5. Rank by exact phrase, heading/title, distinct term coverage, source precedence,
   then canonical path/line tie-break. Document authority is a separate filter/
   label, never conflated with current-worktree precedence. Diversity is added only when a
   failing corpus fixture proves same-file flooding.
6. Format only from the captured source sets. A query never reloads current state
   to “verify” a hit against a newer generation.

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

Search never embeds `CodeEvidenceSummary` arrays or full bridge records. Their stable
IDs resolve through `review_knowledge`, which returns the bounded source-local
dossier. Direct and CCR search results preserve only the compact display, IDs,
preview anchors, and provenance.
Every ID and preview vector is canonically ordered and independently bounded with an
explicit omitted count/coverage state.

Top-level response MUST include:

- a deterministic per-source list of source identity, captured source version
  (including `Clean`/`Dirty`/`NotApplicable`/`Unknown` working-tree state),
  publication/content generations, freshness, coverage, and manifest digest;
- active secret-policy version;
- overall coverage equal to the worst included source;
- source scope searched;
- overflow/truncation count;
- withheld-sensitive count;
- authority-filtered counts by history-only/suppressed/review-required/unknown;
- role/bridge/authority/temporal derived coverage and policy/rule versions;
- deterministic no-match reason when empty.

## No-match classes

| Class | Meaning |
|---|---|
| `no_evidence_complete` | Complete current coverage contains no match. |
| `no_evidence_degraded` | No match, but one or more sources/files unavailable. |
| `evidence_withheld` | Candidate evidence exists but security policy withheld it. |
| `evidence_noncurrent` | Matching evidence exists but the requested authority scope excluded it; returns safe counts/guidance, not excerpts. |
| `query_too_weak` | Deterministic tokenization produced no useful term. |

## Error classes

| Error | Behavior |
|---|---|
| invalid path/source/authority scope | Reject with actionable valid values. |
| index scouting/verifying | Return readiness state; never stale “complete” evidence. |
| degraded last-valid source | Return explicitly degraded/last-verified evidence or a readiness result; never label it current. |
| stored CCR generation unavailable | A later CCR retrieval whose captured source generation was evicted returns an explicit stale/retryable result. In-call generation change is impossible because the source-set Arc is pinned. |
| corrupt snapshot/no valid source | Return recovery guidance; never serve quarantined data. |
| output budget too small | Return provenance-only bounded response or validation error. |

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
and every knowledge no-match class remains a successful response rather than an
MCP/protocol error. A CCR result produced on the compact surface stores a footer that
names the `symforge` facade retrieval intent plus hash; that route redeems the same
CCR record without advertising `symforge_retrieve`. Compact routing is release-gated
on these decode/mapping tests.

## Compatibility

- Existing `search_text` remains code-scoped by default.
- Existing `search_symbols` remains code-scoped.
- Existing `get_file_content` remains the deep-read path after a knowledge hit and
  serves the current captured generation; its repeat-cache identity includes project,
  source, publication generation, and content generation.
- Tool discovery/guidance additions must not bump frecency.

## Contract tests

1. Schema exposes only the eight fields above and read-only annotations.
2. Empty/whitespace query rejects deterministically.
3. Prose hit has exact path/line/heading/hash/generation.
4. Config targeted to both lanes is returned.
5. Sensitive result is withheld without value leakage.
6. Degraded coverage is visible on hit and no-match responses.
7. Output truncation retains complete provenance and CCR handle.
8. Ranking is byte-for-byte deterministic over repeated equal generations.
9. Current worktree ranks ahead of a divergent ref but does not hide it.
10. The compact surface count remains three.
11. `source_scope=all` returns per-source publication/content generations,
    digest/coverage/freshness, and worst overall state for mixed-freshness sources.
12. Compact/facade no-match is a successful result, never a protocol error. A
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
    checked/conflict/change/unresolved evidence array. A derived-only republication
    may reorder records but every prior finding/provenance ID resolves to the same
    dossier.
18. Stable link IDs and bounded exact/declared-set/ambiguous/missing bridge previews
    survive formatting, truncation, cross-project envelopes, and CCR; full bridge
    records remain available through `review_knowledge` without corpus duplication.
19. Clean, dirty, immutable-ref, and inspection-unknown sources preserve their
    captured source version in every per-source envelope; manifest/content digests,
    not branch/timestamp/state labels, remain exact content identity.
