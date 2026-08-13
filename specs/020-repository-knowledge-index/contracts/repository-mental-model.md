# Contract: Evidence-Backed Repository Mental Model

**Status**: V11 refreeze candidate (2026-08-11; non-conflicting V10 evidence retained)<br>
**Surface**: Existing `get_repo_map`, `ask`, `get_file_context`,
`get_symbol_context`, and `search_knowledge`<br>
**Mutation**: Read-only<br>
**Frecency**: Bridge/map discovery must not bump document or code frecency

## Outcome

After strict lifecycle acquisition succeeds, one bounded first-contact call gives an
agent a truthful repository orientation: current code topology/hotspots, organized
current/intent knowledge entry points, explicit code-to-knowledge evidence links, and
uncertainty. It is a derived view, not a generated summary and not a third search
index. If any required selected source is non-current, the call returns exact
`SourceRefusal` instead of a partial or last-verified orientation.

## V11 lifecycle amendment

The V10 role, bridge, ranking, security, and formatting evidence remains applicable
after strict acquisition. Every map, `ask`, file/symbol context, and knowledge-search
generation read acquires lifecycle `Current` for every required selected source through
the project registry, or, per F020-V11-A20, what a `Refreshing` RELOAD retains. A
permit-issuing refresh, a `Blocked` or a `Stopping` retention is internal, non-current
material; it cannot supply a map, backlink, absence, or last-verified response.

Single-source failure returns `SourceUnavailable`; multi-source failure returns
`SelectionUnavailable` whose evidence is an exact bijection over the sealed selection
receipt. Invalid or unauthorized selection retains its indistinguishable
`InvalidSelection` form. Any authority-view choice is a `KnowledgeVoiceFilter` inside
the captured `Current` generation, never a generation-consistency selector.

## V11 claim envelope and authority lanes

Every public orientation success is an operation-specific `Claim<T>` carrying one
opaque `OperationReceipt`, the full `ClaimProvenance`, and its producing
runtime/publication identity; every `SourceRefusal` carries the same operation
receipt. Generation-backed topology, cards, links, policy, temporal, and absence
claims use `Generation` authority only when derived wholly from the captured strict
`Current` leases. A pure live path read, complete worktree scan, or immutable Git read
uses `DiskObservation`, `WorktreeScopeObservation`, or `GitObservation` instead and
may remain responsive while generation state is non-current. A disk receipt may prove
path-local bytes/metadata/missing at its observation time; a worktree-scope receipt may
prove completeness only for its sealed declared scope and interval; a Git receipt may
prove membership/non-membership only in its exact object/tree. None claims generation
membership, lifecycle `Current`, or generation/repository-wide completeness or
absence. Mixed evidence is an allowed `Comparison`/`Derivation`; selected totals and
generation-wide negative claims use `SelectedAggregate` with the lease's exact source
bijection.

The ranking Adapter captures one immutable `RankingSnapshot` after authority
acquisition. Every observable rank, order, score, or ranking explanation carries its
`EvaluationProvenance`, which cannot establish source truth or readiness. Text,
structured content, cache entries, persisted results, CCR handles, and retrieval
round trips preserve the identical operation, claim, and evaluation envelope.
Post-lease response budgeting may shorten values but never authority.

## One-capture rule

At call start the server resolves and authorizes project/source selection, freezes the
exact selected source IDs in a `SourceSelectionReceipt`, and captures one immutable
`ProjectQueryLease` per selected project. The registry admits that lease only when
every required selected source is lifecycle `Current`; the lease owns each selected
verified-generation Arc and the receipt after the runtime snapshot is dropped. The
server never reloads live state again for that source. Every field in the response—live code,
outline, Git temporal/hotspot signals, role cards, bridge links, authority voice,
digest, and coverage—comes from those captured generations or from explicitly typed
observation claims allowed by the operation. Its deterministic per-source envelope includes the
captured source identity and `SourceVersion`, including closed working-tree state,
alongside publication/content generation and digest/coverage.

Health/status remains a separate immutable runtime-publication observation that may
respond while generation state is non-current. If an orientation response includes a
health summary, it labels that runtime evidence separately from generation truth and
cannot use it to attest source bytes, completeness, absence, or a disk/Git fact. A
pure observation lane does not make the generation-backed orientation partially
successful; the latter still refuses unless all required generation leases are
`Current`.

The implementation MUST NOT independently reload existing outline, index, temporal,
health, or side-channel state while formatting. Temporal evidence required by the
advertised scope completes inside an isolated candidate before promotion. A stale or
late completion is discarded; it cannot mutate Current or a captured response.

## Knowledge roles

The fixed v1 roles are:

- architecture;
- ownership/governance;
- decisions/invariants;
- schemas/contracts;
- operations;
- testing/security;
- plans/handoffs;
- other.

A card carries an exact safe path, source/content generation, content hash, and
section/file line range. Role assignment cites one of:

- an exact declared metadata/span;
- a versioned exact heading rule;
- a versioned path convention.

Path conventions may organize a card but cannot invent an owner, lifecycle, active
status, implementation conclusion, or semantic summary. Missing roles are reported
as `unknown/no declared evidence`, not filled by inference. A document may have
multiple roles; mixed authority remains unit-level.

## Bridge evidence and resolution

V1 bridge candidates are closed-world:

1. internal Markdown/repository links;
2. exact repository-relative path tokens;
3. exact symbol names inside code spans that resolve uniquely;
4. supported structured values under a versioned rule;
5. declared ownership selectors.

Resolution is one of:

- `resolved_exact` with one exact file/symbol anchor;
- `resolved_declared_set` for a compact ownership selector;
- `ambiguous` with exact candidate count and bounded samples;
- `missing`.

External links are not code-bridge candidates. Bare similarity, embeddings,
co-occurrence, or an LLM guess cannot create a link. Resolution happens only
against code from the same source identity and content generation. A ref/worktree
document never silently links to another source's code.

Reverse links use compact exact anchor IDs. Ownership patterns remain selectors and
are evaluated against the queried captured code anchor instead of materializing an
edge to every file. Card/link/selector/sample counts and canonical derived metadata
bytes are independently capacity-accounted while building a candidate. If any
required bridge/map/authority/suppression artifact exceeds its bound or truncates,
the breach is attempt-only: discard the candidate, retain bounded attempt accounting,
and refuse strict acquisition. A promoted generation never represents required
truncation as incomplete coverage. A feature omitted from protocol advertisement
before lifecycle startup is absent from `RequiredArtifactSet`; any bounded/truncated
optional work for it remains non-authoritative outside the candidate and produces no
capability, generation artifact, or public claim.

## Surface behavior

### `get_repo_map`

Compact/default output remains bounded. It adds:

- existing code structure and hotspot evidence with temporal coverage;
- at most the policy-capped highest-priority card per present role, then a bounded
  overflow count;
- separate current and intent sections;
- exact bridge anchors for returned cards;
- lifecycle/hygiene counts (`needs_review`, `history_only`, `suppressed`);
- missing roles, ambiguous/missing anchors, withheld/unreadable counts;
- source publication/content identity, manifest/policy digest, completeness
  certificate, and manifest/bridge/authority/temporal proof evidence.

These display caps are applied only after strict lease acquisition against complete
required generation artifacts. Omitted presentation entries carry canonical totals,
full claim/evaluation provenance, and a redeemable CCR handle when applicable; output
budgeting does not change the completeness certificate.

Missing-role, missing-anchor, zero-backlink, and other absence counts are legal only
for the sealed all-`Current` selection captured by the query lease. A non-current
selected source refuses the call rather than being omitted from those counts.

`tree`/`full` may expand existing cards within their existing file/token budgets;
they never inline all repository prose. Hot paths reuse existing topology/churn/
centrality signals and state their availability. The bridge does not manufacture a
hot-path conclusion.

### `ask`

Orientation intent routes to the combined map. Focused factual follow-up routes to
`search_knowledge`; code/symbol/reference intent remains code intelligence.
“Current behavior,” “where are we going,” and “why was this changed” may select
current, intent, or history authority views respectively, but the response labels
the selected view. These are `KnowledgeVoiceFilter` choices inside the same strict
`Current` generation; none selects lifecycle consistency or retained state.

### File and symbol context

`get_file_context` and `get_symbol_context` MUST expose a bounded
`Knowledge evidence` section containing exact reverse backlinks, lifecycle/voice,
source generation, bridge resolution, and ambiguous/missing counts.

V1 schema/format rules:

- add `"knowledge"` to each tool's existing `sections` allowlist;
- `sections=["knowledge"]` returns only the normal trust/identity header plus this
  section; an empty sections list (existing “all” behavior) includes it;
- when `sections` is omitted, `get_file_context` includes it with the existing
  default all-sections response, while default `get_symbol_context` appends it after
  its existing code context only when evidence/counts exist;
- the v1 display cap is five canonically ordered exact/declared links per requested
  code anchor, followed by total/overflow/ambiguous/missing counts; there is no new
  caller-controlled bridge-limit knob;
- unresolved/ambiguous links show typed state and bounded candidate count/samples,
  never a guessed backlink; after strict lease acquisition, `max_tokens` may reduce
  the response to its complete operation/claim/evaluation provenance plus counts/CCR,
  but cannot emit a partial anchor;
- bundle mode retains its current body/dependency contract and does not silently
  inject knowledge; callers use default or `sections=["knowledge"]` for backlinks.

The context call's existing requested-code commitment may retain its current
frecency behavior, but internal `ask`/map routing uses a non-bumping helper and
rendering backlinks MUST NOT bump linked document or code-anchor frecency. Repeat-
cache identity includes project/source plus publication and content generation, so
a watcher publication cannot reuse stale backlinks.
`get_file_content` uses that same generation-aware identity and performs freshness/
generation capture before cache lookup. It serves generation-owned bytes from the
same strict lease; an implementation without such a cache key must bypass repeat-read
suppression, not reopen disk bytes or reuse a prior-generation receipt.

### Knowledge search

`search_knowledge` hits may include bounded resolved code anchors from the captured
bridge. Code anchors are evidence/navigation only; they do not make the prose a
code-search hit.

## Freshness and updates

Document creation/change/rename/removal, code symbol/path change, policy change,
temporal completion, ref movement, and reconciliation all rebuild affected role,
bridge, reverse-link, and authority state before one atomic publication. Removal or
rename yields repaired exact links or typed missing/ambiguous state in that same
generation. A source-affecting observation first publishes non-current lifecycle state;
public generation reads return exact `SourceRefusal` until the complete replacement
promotes. A failed source-affecting observation leaves any retained verified generation unchanged and
internal; there is no public last-verified/degraded mental-model response.

## Security

Only secret-policy-clean resident content can create cards, candidates, links, or
backlinks. Detector-positive/indeterminate documents and unsafe paths create none.
Every displayed title/path/heading/excerpt/code label/uncertainty field passes the
same `SafeHit` output guard before formatting, budgeting, analytics, or CCR.

## Contract tests

1. Compact map returns code topology plus exact role cards within its budget.
2. Missing roles remain explicit. Required temporal/bridge/authority/suppression
   truncation discards the candidate and refuses strict generation reads. Only post-
   lease presentation may truncate a public value; bounded optional work for an
   explicitly unadvertised feature remains outside `RequiredArtifactSet` and produces
   no candidate artifact or public claim.
3. Exact path and unique code-spanned symbol resolve bidirectionally.
4. Bare unique symbol text outside a code span does not create a link.
5. Ambiguous symbol and missing path remain typed uncertainty.
6. Declared ownership selector yields an exact backlink without edge explosion.
7. Same path/symbol in another ref never satisfies the current source link.
8. Code/document rename or removal repairs links atomically.
9. Concurrent map/context calls observe no mixed live/outline/temporal/bridge state.
   A multi-project/multi-ref call performs one source-set load per selected
   `ProjectInstance`, not one load per selected source.
10. Async temporal completion finishes inside a complete successor candidate and may
    advance publication without changing content identity; it never mutates `Current`
    derived state in place.
11. Secret-positive documents produce zero cards/links/backlinks/output.
12. Bridge/card discovery does not contaminate code scopes or frecency.
13. Mixed-purpose document authority remains section-level.
14. Repeated equal generations produce byte-for-byte equal ordering/output.
15. `sections=["knowledge"]`, omitted sections, empty/all sections, default symbol
    context, and bundle behavior match the schema above under tight token budgets.
16. Context repeat cache invalidates on publication/content generation and only the
    directly requested code context can receive its existing commitment bump.
17. A repeated `get_file_content` during watcher-triggered non-current work refuses;
    after successor promotion it serves the new generation, never a prior-generation
    cache receipt or content.
18. Loading, Blocked, Stopping, `Gapped`, verification-overdue, and any Refreshing
    that issued a permit return exact `SourceRefusal` before map/card/backlink/absence
    formatting; only a complete generation is ever exposed, per F020-V11-A20.
19. Current/intent/history authority choices filter voice within one captured
    lifecycle-`Current` generation and cannot select generation consistency.
20. Every map/ask/context/search text and structured result, cache/persisted entry,
    CCR handle, and retrieval round trip preserves one `OperationReceipt` and full
    `ClaimProvenance`; observable ranking/order/scores also preserve the captured
    `EvaluationProvenance`.
21. Pure `DiskObservation`, complete `WorktreeScopeObservation`, `GitObservation`, and
    health/runtime evidence remain distinctly typed and never borrow lifecycle
    `Current`, generation completeness, or repository-wide absence authority;
    path-local `PathMissing`, sealed-scope completeness, and exact-tree `NotInTree`
    remain legal only inside their own receipts.
