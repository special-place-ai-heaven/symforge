# Contract: Evidence-Backed Repository Mental Model

**Status**: Frozen (2026-07-17)<br>
**Surface**: Existing `get_repo_map`, `ask`, `get_file_context`,
`get_symbol_context`, and `search_knowledge`<br>
**Mutation**: Read-only<br>
**Frecency**: Bridge/map discovery must not bump document or code frecency

## Outcome

One bounded first-contact call gives an agent a truthful repository orientation:
current code topology/hotspots, organized current/intent knowledge entry points,
explicit code-to-knowledge evidence links, and uncertainty. It is a derived view,
not a generated summary and not a third search index.

## One-capture rule

At call start the server resolves project/source selection, snapshots the selected
`ProjectInstance` handles, and captures exactly one immutable `PublishedSourceSet`
Arc from each selected instance. It then resolves every selected source to its
`PublishedGeneration` Arc inside that already-captured set; it never reloads the set
once per source. Every field in the response—live code, outline, Git temporal/hotspot
signals, role cards, bridge links, authority voice, health, digest, and coverage—
comes from those captured bundles. Its deterministic per-source envelope includes
the captured source identity and `SourceVersion`, including closed working-tree
state, alongside publication/content generation and digest/coverage.

The implementation MUST NOT independently reload existing outline, index, temporal,
health, or side-channel state while formatting. Async temporal completion publishes
a new derived-only publication generation with the same content generation; it
cannot mutate a captured response.

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
bytes are independently bounded; truncation is explicit bridge/map coverage.

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
- source publication/content generation, manifest/policy digest, and manifest,
  bridge, authority, temporal, and freshness coverage.

`tree`/`full` may expand existing cards within their existing file/token budgets;
they never inline all repository prose. Hot paths reuse existing topology/churn/
centrality signals and state their availability. The bridge does not manufacture a
hot-path conclusion.

### `ask`

Orientation intent routes to the combined map. Focused factual follow-up routes to
`search_knowledge`; code/symbol/reference intent remains code intelligence.
“Current behavior,” “where are we going,” and “why was this changed” may select
current, intent, or history authority views respectively, but the response labels
the selected view.

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
  never a guessed backlink; `max_tokens` may reduce to provenance/counts but cannot
  emit a partial anchor;
- bundle mode retains its current body/dependency contract and does not silently
  inject knowledge; callers use default or `sections=["knowledge"]` for backlinks.

The context call's existing requested-code commitment may retain its current
frecency behavior, but internal `ask`/map routing uses a non-bumping helper and
rendering backlinks MUST NOT bump linked document or code-anchor frecency. Repeat-
cache identity includes project/source plus publication and content generation, so
a watcher publication cannot reuse stale backlinks.
`get_file_content` uses that same generation-aware identity and performs freshness/
generation capture before cache lookup; an implementation without such a key must
serve current bytes instead of applying repeat-read suppression.

### Knowledge search

`search_knowledge` hits may include bounded resolved code anchors from the captured
bridge. Code anchors are evidence/navigation only; they do not make the prose a
code-search hit.

## Freshness and updates

Document creation/change/rename/removal, code symbol/path change, policy change,
temporal completion, ref movement, and reconciliation all rebuild affected role,
bridge, reverse-link, and authority state before one atomic publication. Removal or
rename yields repaired exact links or typed missing/ambiguous state in that same
generation. A failed observation returns last-verified/degraded evidence only under
the published freshness contract.

## Security

Only secret-policy-clean resident content can create cards, candidates, links, or
backlinks. Detector-positive/indeterminate documents and unsafe paths create none.
Every displayed title/path/heading/excerpt/code label/uncertainty field passes the
same `SafeHit` output guard before formatting, budgeting, analytics, or CCR.

## Contract tests

1. Compact map returns code topology plus exact role cards within its budget.
2. Missing roles and temporal/bridge/authority truncation remain explicit.
3. Exact path and unique code-spanned symbol resolve bidirectionally.
4. Bare unique symbol text outside a code span does not create a link.
5. Ambiguous symbol and missing path remain typed uncertainty.
6. Declared ownership selector yields an exact backlink without edge explosion.
7. Same path/symbol in another ref never satisfies the current source link.
8. Code/document rename or removal repairs links atomically.
9. Concurrent map/context calls observe no mixed live/outline/temporal/bridge state.
   A multi-project/multi-ref call performs one source-set load per selected
   `ProjectInstance`, not one load per selected source.
10. Async temporal completion advances publication but not content generation.
11. Secret-positive documents produce zero cards/links/backlinks/output.
12. Bridge/card discovery does not contaminate code scopes or frecency.
13. Mixed-purpose document authority remains section-level.
14. Repeated equal generations produce byte-for-byte equal ordering/output.
15. `sections=["knowledge"]`, omitted sections, empty/all sections, default symbol
    context, and bundle behavior match the schema above under tight token budgets.
16. Context repeat cache invalidates on publication/content generation and only the
    directly requested code context can receive its existing commitment bump.
17. A repeated `get_file_content` after watcher publication serves the new generation,
    never a prior-generation cache receipt or content.
