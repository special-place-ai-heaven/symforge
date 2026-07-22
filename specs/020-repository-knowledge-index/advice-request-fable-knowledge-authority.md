# Fable Read-Only Advice Request: Knowledge Authority and Hygiene

## Assignment

Advise on the hardest architecture and product decisions in SymForge feature 020.
This is a read-only design consultation. Do not edit code, specifications, tasks,
configuration, Git state, or any repository file. Do not run mutating commands.

Read `AGENTS.md` first and obey its secret-safety rule. Never inspect, quote, print,
or reproduce a secret value. Findings may cite safe `file:line` locations, type/
symbol names, synthetic rule IDs, and non-sensitive metadata only.

## Product outcome

SymForge is evolving two deliberately separate lanes:

1. code intelligence: exact symbols, references, topology, schemas, hot paths;
2. repository knowledge: exact prose/config/spec/plan/decision evidence.

A derived bridge must turn them into an immediate, evidence-backed repository
mental model. It must also stop obsolete implementation documentation from
masquerading as current truth. Code is authoritative for what the selected source
generation implements now; it is not allowed to invalidate a north star, proposal,
ADR, governance rule, or future intent merely because implementation differs.

The desired user workflow is:

```text
index -> orient -> reconcile evidence -> review remediation -> approve -> curate
```

No embeddings, vector database, second content store, remote service, or internal
LLM judge is planned for v1.

## Canonical artifacts

Read these completely:

1. `specs/020-repository-knowledge-index/spec.md`
2. `specs/020-repository-knowledge-index/plan.md`
3. `specs/020-repository-knowledge-index/research.md`
4. `specs/020-repository-knowledge-index/data-model.md`
5. `specs/020-repository-knowledge-index/tasks.md`
6. `specs/020-repository-knowledge-index/contracts/search-knowledge.md`
7. `specs/020-repository-knowledge-index/contracts/repository-mental-model.md`
8. `specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md`
9. `specs/020-repository-knowledge-index/contracts/source-binding-and-state.md`

If a listed contract is absent or contradicts another canonical artifact, treat that
as a finding rather than inventing the missing behavior.

Use current source to falsify assumptions, especially:

- `src/live_index/store.rs`, `query.rs`, `search.rs`, and `git_temporal.rs`;
- `src/live_index/graph.rs` and parsing/config extractors;
- `src/protocol/tools.rs`, `search_tools.rs`, `format.rs`, `ccr.rs`, and edit paths;
- `src/watcher/mod.rs`, `src/live_index/persist.rs`, `src/git.rs`, and worktree code.
- `src/paths.rs`, `src/discovery/mod.rs`, `src/daemon.rs`, and `src/cli/init.rs`
  plus `src/idempotency.rs`, `src/sidecar/`, and snapshot/DB path consumers for
  source-root authorization versus runtime-state placement.

## Headscratchers to resolve

### 1. Smallest honest state model

Can lifecycle, authority domain, evidence state, and retrieval voice be represented
with fewer axes without recreating illegal conflations such as “old = wrong,”
“proposal = implemented,” or “one broken section = stale file”? Recommend exact
Rust enums and constructor invariants.

### 2. Deterministic proof boundary

Define a closed table of what may establish:

- exact consistency for checked claims;
- broken anchor;
- deterministic code divergence;
- changed since verification / review due;
- suspected semantic conflict;
- implementation gap for intent/governance.

Challenge missing paths/symbols, renamed symbols, overload ambiguity, feature flags,
platform variants, generated surfaces, schemas, CLI/MCP/config keys, changelogs, and
mixed-purpose documents. No heuristic or LLM opinion may silently become proof.

### 3. Temporal evidence

Recommend how to combine filesystem birth/creation and modification hints, Git
first-seen/last-touch commits, working-tree dirtiness, code commits since document
review, shallow history, rename gaps, rebases, copied files, and clock skew. Specify
what is advisory, what is topologically provable, and the coverage envelope.

### 4. Retrieval authority

Specify the safest default for current/intent/history/all retrieval. Decide whether
review-required and unknown evidence stays visible or is excluded/down-ranked.
Prove that explicitly superseded or deterministically divergent current-reference
units have no current voice while preserved intent remains discoverable and labeled.

### 5. Bridge and publication atomicity

The bridge should accept only explicit repository links/paths, exact code-spanned
unique symbols, supported structured values, and declared ownership selectors.
Design compact forward/reverse state, ambiguity/missing states, bounded coverage,
incremental invalidation, and one-captured-`PublishedGeneration` behavior across
`get_repo_map`, `ask`, `get_file_context`, `get_symbol_context`, review, and search.

### 6. Lifecycle policy

Assess a versioned repo-owned `.symforge-knowledge.toml` ledger whose decisions bind
to exact path + content hash. A changed file must invalidate old suppression. Native
frontmatter/MADR/RFC status is evidence but not a second mutable authority. Resolve
conflicts, supersession cycles, renames, and branch/worktree-local policy.

### 7. Remediation and deletion eligibility

Define evidence and preconditions for keep, update, relabel intent, merge, mark
superseded, archive, deletion candidate, and needs review. Be especially adversarial
about what is reasonable to delete. Age alone must never exceed needs-review.
Protected intent/ADR/governance/legal/security/north-star material must not become a
deletion candidate merely because code differs.

### 8. MCP surface and approval boundary

Compare:

- separate read-only `review_knowledge` and mutating `curate_knowledge`; versus
- one `knowledge_hygiene(mode=audit|plan|apply|undo)` tool.

Account for MCP annotations, compact-3 stability, schema complexity, explicit user
approval, idempotency, stale-plan/hash rejection, and agent misuse. Recommend the
smallest safe surface plus prompt/resource exposure.

### 9. Physical recycling

Should v1 stop at logical archive/supersession, leaving physical move/delete as a
proposal, or ship a P1 journaled local-quarantine/undo transaction? If physical
recycling is defensible, specify clean/tracked/hash/backlink/path/worktree preflight,
all-or-rollback crash recovery, and what must make it refuse.

### 10. Security and boundedness

Prove secret-positive content cannot create snippets, links, findings, proposals,
policy evidence, logs, analytics, CCR entries, or cleanup receipts. Add explicit
catalog-metadata, derived-card, bridge-edge, authority-finding, audit-output, and
transaction budgets without permitting false completeness.

### 11. Protected roots and writable state

Audit the corrected separation between source authorization and runtime-state
placement. Automatic home/OS/root launches must stay responsive but unbound and
accept a later accessible `index_folder` request. A direct
`index_folder(..., allow_protected_root=true)` may index the exact protected source
but must never inspect/create `<source>/.symforge`; it uses private user-local
per-root state or live memory-only operation. Challenge raw/canonical alias rules,
device namespaces, failed-retarget preservation, state-key collisions, nested state
self-indexing, snapshot identity, durable/process-local idempotency, watcher health,
and which capabilities must be disabled. Also verify explicit normal `index_folder`
and project-aware init share one guarded append to an existing root `.gitignore`,
never create it, and do not let hygiene failure disable the live index.

## Required output

Return Markdown only:

```text
# Fable Knowledge Authority Advice

## Executive recommendation
- The smallest architecture you recommend

## Decision table
| Question | Recommendation | Why | Rejected alternative |

## Critical findings
1. [HIGH|MEDIUM|LOW] Title
   - Evidence: exact file:line and/or source symbol
   - Failure sequence
   - Smallest correction

## Proposed state model
- Exact enums, invariants, and derivation rules

## Deterministic proof matrix
- Evidence -> allowed conclusion -> forbidden conclusion

## Cleanup safety contract
- Preview/apply/undo and deletion criteria

## Missing adversarial tests
- Exact red-test names and failure oracles

## Scope cuts
- What should be deferred without weakening the core outcome
```

Do not praise intent in place of analysis. Trace actual state transitions and source
ownership. Do not propose implementation edits during this consultation.
