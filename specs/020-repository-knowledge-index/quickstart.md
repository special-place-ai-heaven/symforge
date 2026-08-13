# Quickstart: Repository Knowledge Index Gates

**Feature**: 020
**Branch**: `design/project-activation-prevention`

## Preconditions

- Run from the controlled SymForge checkout on
  `design/project-activation-prevention`. The old `E:\project\symforge` /
  `feat/repository-knowledge-index` location is a v10 historical receipt, not a v11
  execution prerequisite.
- Use a disposable external `CARGO_TARGET_DIR` with sufficient free space.
- Do not place multi-gigabyte real models in fixtures; use sparse files via
  `File::set_len`.
- Preserve RED and GREEN command receipts for every gate.

## V11 lifecycle refreeze gate

The round-two lifecycle-prevention design review is **CLEAR**. Before Slice 0, and
before any production-reachable PreventiveV1 implementation:

1. Hash-pin and validate `REFREEZE-MANIFEST-v11.md`, classifying every Feature 020
   artifact plus bound `CONTEXT.md` and mapping every replacement clause to its
   contract/task/regression IDs.
2. Pin the detached refreeze attestation and obtain the trusted signed append-only
   `RefreezeApprovalRecordV11` for the exact commit/tree outside the mutable tree.
3. Check in and attest `contracts/public-api-v11.json`, its closed supported
   target/cfg/feature domain, and the complete v10 `keep | replace | remove` map.
4. Prove a coordinated in-tree manifest/design/context/API/attestation rewrite is
   rejected while the prior external approval record remains unchanged.

All v11 generation-query acceptance uses one rule, F020-V11-A20: only a COMPLETE
verified generation may grant a strict query lease — `Current`, and what
`Refreshing` retains while no mutation permit is outstanding. `Blocked` and
`Stopping` retentions are internal recovery material and explicitly non-current.
There is no public degraded wrapper or automatic last-known-good answer. A cold
placeholder and every candidate remain nonqueryable; an incomplete selected source
returns the closed `SourceRefusal` algebra. Promotion requires one complete,
source-bound, root-bound, observation-complete `VerifiedGeneration`, including every
certificate required by the advertised strict scope. This restriction applies to
generation-backed consumers, not to runtime health or pure root-bound
`DiskObservation`, complete `WorktreeScopeObservation`, or `GitObservation`; those
remain independently authoritative within their closed scope. Disk may prove path-
local `PathMissing`, a worktree-scope receipt may prove completeness for its sealed
declared scope/interval, and Git may prove `NotInTree` for one exact tree. None may
claim generation membership, generation completeness, or unqualified repository-wide
absence. Any mixed response must acquire one identity-compatible operation
`ClaimContext` naming every input.

Every write-capable repository-content path—including curation and root-ignore
hygiene—must obtain a non-cloneable `SourceMutationPermit`. Grant publishes
non-Current before side effects. The permit pins the physical root and confines every
path component, temp write, and replacement through no-follow/reparse-safe,
handle-relative I/O. Success, failure, rollback, and a valid no-side-effect proof can
return to Current only through a fresh complete candidate at the latest observer cut.
Cold `index_folder`, project-aware init, and restart recovery remain read-only and
cannot grant this permit until one complete Current generation promotes. Exact pre-
image retry/cleanup/probe/source bytes then require a fresh permit; exact post-image
completion finalization in `ProjectStateDir` is persistence-only.

Existing v10 commands and receipts below remain useful regression evidence. Any v10
oracle that expected a degraded publication, a last-valid wrapper, partial candidate
availability, or public retained-generation reads is **superseded for v11** and must
be rerun against the strict-current oracle stated here.

Run the in-tree consistency oracle on the committed refreeze target:

```powershell
python -m unittest discover -s execution -p 'test_*.py'
python -I execution/refreeze_v11.py `
  --git-executable <absolute-trusted-git> `
  verify-internal --target-ref HEAD
```

Then verify the externally stored approval inputs. The approval record, signature,
and signer policy MUST resolve outside this repository; never copy trust material into
the working tree or command output:

```powershell
python -I execution/refreeze_v11.py `
  --git-executable <absolute-trusted-git> `
  verify-approval `
  --target-ref HEAD `
  --approval-record <outside-repo>/RefreezeApprovalRecordV11.json `
  --approval-signature <outside-repo>/RefreezeApprovalRecordV11.json.sig `
  --allowed-signers <outside-repo>/allowed_signers `
  --ssh-keygen-executable <absolute-trusted-ssh-keygen> `
  --expected-release-identity <trusted-principal> `
  --expected-repository special-place-ai-heaven/symforge
```

The first approval has `sequence: 1` and a null predecessor. For a later approval,
also pass `--approval-history-dir <outside-repo>/history`; that directory contains
each predecessor as `<sha256>.json` plus `<sha256>.json.sig`. The verifier walks and
signature-checks the complete same-store sequence back to 1. A non-initial record
without that external history fails closed.

`verify-internal` proves only in-tree consistency. Slice 0 remains blocked until the
second command validates the immutable external trust anchor for the exact commit/tree.

## Focused gate order

Run the narrow test introduced by the current task first. Then run the relevant
module/integration family. These v10 families remain useful regression baselines but
do not replace the v11 refreeze, lifecycle, activation, or observed-refresh gates:

```powershell
cargo test --features server --lib discovery::tests -- --test-threads=1
cargo test --features server --lib watcher::tests -- --test-threads=1
cargo test --features server --lib live_index::store::tests -- --test-threads=1
cargo test --features server --lib live_index::persist::tests -- --test-threads=1
cargo test --features server --lib live_index::search::tests -- --test-threads=1
cargo test --features server --test admission_acceptance -- --test-threads=1
cargo test --features server --test live_index_publish_atomicity -- --test-threads=1
```

If Rust's test filter layout differs, use the smallest exact test binary/name that
exercises the intended red oracle; record the actual command.

## Bootstrap, protected-root, and state-placement acceptance

Use an injected filesystem/path-policy fixture for protected roots. Never traverse
or write the machine's real home, OS, or System32 directory during tests.

1. Start the server with launch CWD, workspace env, and client-root candidates that
   canonically model `%USERPROFILE%`, `C:\Windows\System32`, `/`, `/home/<user>`,
   `/System`, and their symlink/extended-prefix aliases. Assert responsive Unbound
   health, zero source traversal/project watcher/candidate-root or per-project state
   writes, and corrective `index_folder` guidance. A process-global transport record
   is allowed only under a safe user-local base.
2. In that same process, call `index_folder` for a normal writable temp project.
   While cold observation is incomplete, assert protocol and health remain responsive
   but the placeholder is nonqueryable, generation reads return `SourceRefusal`, and
   pending startup performs no repository-content write or mutation-permit grant.
   Assert lifecycle `Current`, normal managed-observer operation, project-local
   placement, and usable queries only after one complete generation promotes. Only
   after that point may ignore hygiene acquire its own fresh permit and temporarily
   make the source non-Current.
3. Call `index_folder` for the modeled System32 root without the override. Assert the
   applicable closed `SourceRefusal` and no source/state fallback.
4. Repeat with `allow_protected_root=true`. Assert the exact requested source becomes
   queryable for that session, placement is user-local per-root, protected mutation
   capabilities are unavailable with reasons, and the filesystem spy records zero
   inspect/create/write attempts against `<protected-root>/.symforge`.
5. From a second session, prove alias selection, `projects=["*"]`, reconnect metadata,
   and another session's membership cannot address the protected project. Issue a
   fresh direct exact override from the second session and assert it joins the same
   `ProjectInstance`/watcher. Restart the daemon and prove both sessions must grant
   fresh direct authority again before persisted protected state becomes live.
6. Exercise `index_folder` idempotency: same key/request must re-establish the current
   session's live binding before returning stored success; override/path changes
   under one key conflict; if reattach/rebuild cannot restore the live postcondition,
   return typed `live_postcondition_unavailable` while preserving the old receipt.
7. Make the user-local base unavailable and repeat step 4. Assert lifecycle `Current` with
   memory-only placement and working live queries. `checkpoint_now` must return a
   successful operation envelope with typed `persistence_unavailable` and
   `applied=false`; it is not an MCP/protocol error and changes no generation.
8. Make a normal project readable but its root state directory unwritable. Assert
   user-local fallback without changing the indexed source. Then make both state
   locations unavailable and assert memory-only live indexing.
9. Verify canonical aliases produce the same placement ID while distinct repositories
   and linked worktrees produce different IDs.
10. Replace the repository at the same canonical path, preserving the path-derived
    placement ID. Assert snapshot/temporal state remains non-Current until repository,
    source/version, manifest, resident-content, and applicable history fingerprints
    verify; mismatched state is neither loaded nor overwritten.
11. Reindex the same live `ProjectInstance` after either fallback. Assert its resolved
    placement does not silently change. Close it, construct a new `ProjectInstance`
    for the writable project, and assert placement is re-resolved so normal durable
    capabilities can recover without server restart.
12. While a normal project is active, attempt a rejected/failed retarget and assert
   the prior source, watcher, and published generation remain unchanged.
13. Model project and control state directories nested beneath an explicitly indexed
    home source. Assert both absolute subtrees remain absent from scout, watcher,
    reconciliation, code, knowledge, and snapshot verification results.
14. Make the safe global control base unavailable. Assert SymForge does not create a
    CWD-relative `.symforge`, remains responsive with process-local/non-durable
    coordination, and can still bind a later valid project.
15. After a valid bind/publication, fail the next state write. Assert source, watcher,
    live generation, and queries remain usable while durability and affected
    mutation capabilities degrade independently with reason-bearing status.
16. Run a typed-owner spy over the complete consumer inventory. Snapshot/temp/
    quarantine/reset/checkpoint/per-project replay/mutation intent/coupling/frecency/
    STEL/analytics/API-key/edit-safety TEE/cleanup receive only `ProjectStateDir`;
    edit-safety trust store, sidecar port/PID/session descriptors and status readers, daemon discovery/control,
    hook adoption/hints, operator profile, onboarding, runtime-startup coordination,
    cross-project replay/locks, version registry, and updater receive only
    `ControlStateDir`; excluded team-artifact bytes and metadata receive only
    `ProjectStateDir`, while source/Git/watcher/policy/ignore and `.gitattributes`
    operations receive only the canonical source root. No consumer may reconstruct source-local
    state or use launch CWD, and every reader must resolve the same owner as its writer.
    Start two project/daemon instances and prove their control descriptors use
    distinct `ProjectId`/instance namespaces and remain independently discoverable.
17. Seed distinct legacy per-project operator-profile/onboarding files, but no global
    record. Assert the legacy files remain byte-for-byte untouched and are neither
    merged nor treated as fallback; onboarding runs once to create the intentional
    process-global control-state record, and subsequent projects reuse that record.
18. Exercise explicit team-artifact export. Protected, read-only, user-local, and
    memory-only bindings write neither artifact nor `.gitattributes`. Normal writable
    project-local fixtures produce each exact receipt state. If export must change
    `.gitattributes`, assert that repository-content write uses
    `SourceMutationPermit`, publishes non-Current first, performs confined
    handle-relative I/O, and returns through a fresh complete candidate; the excluded
    team-artifact state write alone does not revoke Current:

    - tracked artifact -> `already_tracked`;
    - untracked and visible -> `untracked_visible`;
    - ignored artifact -> `ignored_force_add_required`;
    - Git visibility probe unavailable -> `git_visibility_unavailable`.

Run the shared root `.gitignore` byte matrix only after explicit normal
`index_folder` or project-aware init has completed read-only cold observation and
promoted one complete Current generation:

| Fixture | Expected result |
|---|---|
| absent | remains absent |
| empty | exact `/.symforge/`, no invented final newline |
| BOM-only | BOM preserved, then exact rule, no invented final newline |
| LF/CRLF with final newline | existing bytes/style preserved; appended rule ends in that style |
| LF/CRLF without final newline | one styled separator before rule; result still has no final newline |
| effective rooted equivalent | byte-for-byte no-op |
| prior ignore followed by effective negation | append canonical rule so final ordered result ignores root state |
| global exclude or `.git/info/exclude` only | append to the root file; external rules do not satisfy hygiene |
| concurrent hash change | typed hygiene refusal; retained generation is unchanged but non-current until a fresh no-side-effect candidate promotes |
| symlink/reparse-point root file | typed hygiene refusal; target is never followed |
| automatic/protected/ref path | observation only; zero mutation |

Pending startup remains read-only and cannot grant a permit. The later write-capable
phase obtains a fresh `SourceMutationPermit` before any side effect and
publishes non-Current. It revalidates through the permit's pinned root, then performs
the guarded atomic append with component-confined handle-relative I/O. It never
rewrites pre-existing bytes and uses the first existing newline sequence (LF fallback
when none exists). Permission/race failure cannot corrupt the retained generation,
but generation-backed reads refuse until a fresh verification/no-op candidate
promotes; an effective/absent/no-write outcome after permit grant follows the same
candidate path. Every state placement excludes `.symforge/` regardless of ignore
state.

## Scout acceptance fixture

Fixture contents:

```text
src/lib.rs                 small code target
README.md                  knowledge target
models/model.gguf          sparse > hard-skip ceiling
notes/architecture.odd     safe UTF-8 unknown extension
.github/CONTRIBUTING.md    hidden knowledge target
assets/model.bin           bounded Git LFS pointer
notes/legacy.txt           unsupported UTF-16/legacy encoding
package-lock.json          metadata-only lockfile
```

Pass criteria:

- lifecycle `Current` is reached only after complete promotion;
- model receives zero probe/full reads and zero admitted bytes;
- every path has one disposition; every indexed disposition is exactly `Code`,
  `Knowledge`, or `CodeAndKnowledge`, while catalog-only entries carry no target;
- a platform-supported non-UTF-8 path fixture has an opaque unique catalog ID and
  no content target (skip with an explicit platform reason where unconstructible);
- Rust is code-searchable;
- README/unknown/hidden prose are knowledge-searchable;
- lockfile/model/LFS pointer/unsupported encoding are absent from both content scopes;
- LFS declared metadata is cataloged without materialization.
- disposition parse status is only `Parsed`, `PartialParse`, or `Failed`, with no
  diagnostic string in the canonical digest; Knowledge-only files use extractor
  status rather than a synthetic code-parse result. Reword only operational parser
  diagnostics and prove the manifest digest remains byte-identical. A candidate with
  `PartialParse` or `Failed` in an advertised strict scope does not promote; the
  disposition remains bounded attempt evidence, not a committed Current manifest.

Then add enough tiny metadata-only entries to exceed the catalog-metadata budget
while remaining below catalog-entry, admitted-content, and in-flight limits. Assert
those four budgets account independently, the failed candidate publishes no partial
manifest, the previous verified generation remains byte-for-byte retained but
internal/non-current (or an initial process remains nonqueryable), and query
acquisition returns typed capacity `SourceRefusal` evidence rather than a degraded
wrapper or false Complete manifest. Repeat with the entry ceiling as the only
exhausted limit; entry and metadata exhaustion must produce distinct closed refusal
causes with no manifest-level budget `ScoutIssue`.
Then set the total in-flight budget below one admitted file's declared size. Assert
zero allocation/read and terminal `HardSkip(PerFileCeiling)` accounting.

## Watcher/reconciliation acceptance

1. Start from a complete manifest.
2. Create unknown prose while suppressing/dropping its watcher event.
3. Trigger reconciliation and verify the knowledge hit.
4. Delete a catalog-only path while suppressing its event.
5. Reconcile and verify removal.
6. Retarget project while a verifier/update holding the old opaque
   `SourcePublicationToken`/`GenerationAuthority` is paused; resume and prove
   rejection even if the new source has equal numeric diagnostic epochs.
7. Repeat equal reconciliation and prove no generation change.
8. Inject a transient walk failure; prove the candidate is discarded, strict reads
   return `SourceRefusal`, the supervised bounded retry remains owned, and a complete
   successor eventually promotes `Current` after recovery.
9. Race reconciliation with a watcher update; prove neither change is lost.
10. Temporarily deny/read-race one file until it becomes `Unreadable` or
    `UnstableDuringRead`. The candidate cannot promote, any retained generation stays
    internal/non-current, and bounded owned re-observation converges after the file
    becomes stable.
11. Trip the parse-failure threshold in one source/lane/stage. Cancel upstream work
    and discard the whole candidate; no already-computed tail becomes a partially
    committed `AbortedCircuitBreaker` generation.
12. Exhaust the automatic-attempt series into typed `Blocked`, then emit a source,
    capacity/configuration, or explicit-operator retry trigger. Assert lifecycle
    ownership persists, repair re-triggers, and no task silently stops forever.

The prior v10 receipts for `Degraded -> Complete`, settled degraded coverage, and
late circuit-breaker folding remain positive-control history. Their degraded
publication outcomes are superseded and are not v11 acceptance criteria.

## Snapshot/publication acceptance

1. Index a complete mix of promotable `Indexed`, `MetadataOnly`, and `HardSkip`
   dispositions and checkpoint. Do not include `Unreadable`, unstable, aborted, or
   partial/failed-parse attempt dispositions in this successful parity fixture.
2. Restart from the snapshot candidate.
3. Verify non-Current state until scope/content validation completes.
4. Assert logical manifest/query parity with source build.
5. Corrupt a disposable snapshot copy and verify quarantine/rebuild guidance.
6. Run concurrent reload/read and sibling-source publication stress. Assert every
   query captures one `ArcSwap<ProjectRuntimePublication>` root and never a hybrid
   bundle. Each sibling commit mints a new project-runtime publication identity while
   every unchanged source retains its exact `GenerationAuthority`; equal/advancing
   numeric generations remain diagnostic only.
7. Assert an in-flight budget smaller than the admitted corpus completes without
   deadlock and transfers accounting at staged-index hand-off.
8. Fail a new observation; assert the prior verified generation is retained
   byte-for-byte as internal recovery material, the source publishes non-Current
   work/failure state, and strict acquisition returns `SourceRefusal`. No degraded
   generation wrapper is published.
9. Pause background verification holding the exact opaque
   `SourcePublicationToken` and `GenerationAuthority`, publish a watcher edit for the
   same source, then resume. The verifier rebases/retries or aborts and never replaces
   the newer publication. Repeat with equal numeric binding/observer/publication/
   content/project epochs to prove those values are diagnostic, not fences.
10. Make one required path unreadable during source build and again during snapshot
    verification. Each candidate is discarded; the path appears only in bounded
    attempt accounting, any retained generation is internal/non-current, and strict
    acquisition refuses. Restore access and prove bounded re-observation promotes a
    fresh complete candidate whose manifest and query corpus match a clean source
    build.

Repeat with a different repository placed at the same canonical path. A matching
placement ID is insufficient: no prior manifest/content/temporal state may become
`Current` or be overwritten until the strong source/header fingerprints verify.

## Claim-authority acceptance

1. Put the source in `Refreshing` **with a mutation permit outstanding** and prove a
   generation-backed search/read refuses, per F020-V11-A20; prove the same source
   serves its complete retained generation once that permit retires. Health returns
   its immutable runtime snapshot throughout, and a pure root-bound syntax/untracked
   read can return `DiskObservation` from the actually opened object, including
   path-local `PathMissing` from a retained final-parent handle.
2. Under the same non-Current state, prove a complete root-bound scan may return
   `WorktreeScopeObservation` and an immutable Git-object/ref read may return
   `GitObservation`. The former may attest completeness only for its sealed declared
   scope/interval; the latter may attest `NotInTree` only for its exact tree. Neither
   may state generation membership, generation completeness, unqualified repository-
   wide absence, or reuse retained-generation identity.
3. Exercise uncommitted diff and `detect_impact` through one operation-specific
   `ClaimContext`. The resulting Comparison/Derivation names every atomic authority
   and selection input. Pause between input acquisitions and rebind root A to B;
   construction must refuse rather than combine Generation(A) with Disk/Git(B).
4. Rebind only after a complete compatible context capture. The response may finish
   wholly from the captured old-root authorities and must not perform a trailing
   live-state check that changes its attribution.

## Text-format, byte, and lifecycle parity acceptance

Build the following safe fixture without normalizing bytes:

| Path | Bytes/content | Expected knowledge unit |
|---|---|---|
| `docs/guide.md` | ATX/Setext/frontmatter/fence/table/link corpus | Markdown sections |
| `docs/guide.mdx` | Markdown plus JSX outside a fenced block | Markdown/MDX sections |
| `docs/guide.rst` | reStructuredText headings | exact generic line evidence |
| `docs/guide.adoc` | AsciiDoc headings | exact generic line evidence |
| `docs/guide.org` | Org headings | exact generic line evidence |
| `README` / `CHANGELOG` / `NOTICE` | extensionless UTF-8 | exact generic line evidence |
| `config/app.toml` / `.yaml` / `.json` | safe structured config | knowledge and code/config targets |
| `notes/unknown.odd` | safe UTF-8 unknown extension | exact generic line evidence |
| `notes/empty.txt` | zero bytes | admitted text with no false hit |
| `notes/lf.txt` | LF plus multibyte UTF-8, final newline | exact byte/line pointer |
| `notes/crlf.txt` | CRLF plus multibyte UTF-8, no final newline | exact byte/line pointer |
| `notes/bom.txt` | UTF-8 BOM | BOM retained in source/hash, not rendered, and counted by offsets |
| `notes/invalid.txt` | invalid UTF-8 | catalog-only `UnsupportedTextEncoding` |

For every searchable fixture, prove the evidence slice, one-based `line`, one-based
half-open `line_range`, unit byte range, and content hash refer to the original bytes;
no LF/CRLF/BOM/multibyte/no-final-newline rewrite is permitted. Hit/envelope source
identity, source version, and generation must be projected from the captured bundle
at format time, so no independently stored mismatched copy can be rendered. Prose
remains absent from code symbol/reference/text scopes, while the declared safe
configs are intentionally available in both.

Run the identical fixture through cold load, watcher create/change, missed-event
reconciliation, and background snapshot verification. Compare canonical manifest
disposition, targets, knowledge units, secret-policy outcome, bridge candidates,
lifecycle/evidence/voice, and published hashes byte-for-byte. Gate L repeats the
same oracle through linked-worktree and local-ref blob ingestion, allowing only
declared source/temporal-provenance differences.

## `search_knowledge` contract acceptance

Open two disposable projects in one daemon session. Give both `docs/shared.md`, but
use distinct safe phrases and code anchors. Add a third open project to another
session only.

1. `project=A`, `projects=[A,B]`, and `projects=["*"]` return exactly the selected
   session-visible projects in canonical envelope order. The wildcard excludes the
   other session's project. `project` plus `projects`, an unknown alias, duplicate/
   malformed selectors, and unauthorized protected membership return typed errors.
2. Search project A's phrase through B and vice versa. Assert zero hit, bridge,
   authority, repeat-cache, or CCR leakage. Each selected `ProjectInstance` is
   captured once before search; every per-source envelope reports its captured
   source version (including closed working-tree state) and every hit reports its
   exact `GenerationAuthority` and content identity rather than a call-global or
   numeric-generation approximation. The envelope carries the one captured
   `ProjectRuntimePublication` identity.
3. Pause a search after capture, publish a watcher update, then resume. The paused
   response must contain only the old captured generation; the next call must contain
   only the new generation. Repeat across two selected projects while only one moves.
4. Before Gate L, advertise only `source_scope=current`; requests for `worktrees`,
   `local_refs`, or `all` return typed unsupported-scope errors. After Gate L, rerun
   the same matrix against all four supported scopes. The wire value `current` is a
   knowledge-voice/source-scope selection inside one lifecycle `Current` generation;
   it is not a consistency selector and cannot expose retained state.
5. Truncate/direct and CCR-retrieve the same result. Exact excerpt, path/line/unit,
   compact authority display, stable finding/rule/link IDs, bounded bridge previews,
   source identity/version/generation, coverage, and whole-hit withholding remain
   identical; full evidence arrays remain available through `review_knowledge` and
   are not copied into CCR. An evicted captured generation returns explicit stale/
   retryable CCR state. Repeat on the compact surface: the footer names the
   `symforge` facade retrieval intent and its hash round-trips without a fourth tool.
6. Deep-read a hit with `get_file_content`, publish a watcher edit, and repeat the
   same request. The second read serves the new generation rather than repeat-cache
   suppression or prior content.

Exercise every successful empty-result shape:

| Fixture | Required result |
|---|---|
| complete covered scope, absent phrase | `no_evidence_complete` |
| final output guard withholds the only otherwise-safe hit | `evidence_withheld` with safe count, no excerpt |
| match excluded by requested authority | `evidence_noncurrent` with safe guidance/count |
| stopwords/punctuation only | `query_too_weak` |

The v10 `no_evidence_degraded` receipt is preserved as historical evidence but is
superseded: absence is never returned for a selected non-Current source. The
`evidence_noncurrent` spelling above is knowledge-authority filtering, not lifecycle
consistency.

Exercise every validation/readiness failure without converting it to complete no-
evidence: empty/whitespace query; path traversal or invalid path prefix; invalid or
unsupported source/authority scope; `project` plus `projects`; unknown/unauthorized
project; Loading/Refreshing/Blocked/Stopping source; corrupt/no-valid snapshot;
evicted CCR generation; and output budget too small even for provenance. Assert each
non-Current selected source returns deterministic, actionable, non-echoing
`SourceRefusal` evidence. Multi-source refusal preserves an exact selected-source
bijection and never omits either a healthy or failing member. Knowledge facade
routing must preserve all four v11 successful no-match shapes and must not capture
symbol/reference/code-text intent.

## Knowledge corpus

Run `search_knowledge` against this repository for:

1. “shutdown is not a persistence boundary”
2. “repair_index is intentionally retired”
3. “compact surface has three tools”
4. “byte exact storage line endings”
5. “why embeddings are optional”
6. “GGUF or safetensors indexing limits”
7. “worktree routing and stale generations”
8. “FTS5 planned or deferred”

For each query record:

- returned source and line;
- source version, exact `GenerationAuthority`, content identity, and strict-scope
  certificate state;
- returned token count;
- whether a direct read was needed;
- broad discovery + direct-read comparison tokens/time;
- false/missing/conflicting variants.

Pass criteria: the correct source pointer appears in one bounded call for each query
whose evidence exists in complete scope, the corpus median returned-token count is
at least 50% below the recorded broad-discovery-plus-direct-read baseline, and no
response makes a false freshness claim.

## Mental-model and bridge acceptance

Build a source-local fixture with one declared/heading/path-derived card for every
knowledge role, one missing role, exact repository links, a code-spanned unique
symbol, a bare prose symbol, two same-name/kind symbols at different spans, a
missing symbol/path, an ownership selector, and contributor history without
declared ownership.

Verify:

1. Compact `get_repo_map` returns bounded code topology plus current/intent cards,
   exact anchors, missing roles, overflow, ambiguity, and all coverage versions from
   one publication.
2. Only the exact path and code-spanned unique symbol resolve. Bare prose creates no
   edge; same-name spans remain ambiguous; contributor history is labeled
   contributor and never owner.
3. Forward/reverse links update atomically after rename/remove. A temporal result
   computed for the old generation is rejected. Its replacement builds a complete
   successor candidate containing content, manifest, links, temporal evidence, and
   every required artifact, then performs one atomic Current promotion; no
   derived-only publication is permitted.
4. Exhaust role/bridge/authority budgets and prove required derived truncation
   discards the candidate and survives only as bounded attempt accounting with no
   query/cache/CCR/snapshot identity. Strict generation queries return
   `SourceRefusal`; only response rendering after a complete lease may truncate.
5. `get_file_context` and `get_symbol_context` behavior for omitted sections, empty
   all-sections, `sections=["knowledge"]`, default symbol context, bundle mode, and
   tight `max_tokens` matches the contract. Rebuild links and prove the repeat cache
   cannot serve the prior generation.
6. Map, ask, search, bridge, and review leave frecency unchanged; only the directly
   requested code context may retain its existing commitment behavior.

## Authority and read-only review acceptance

Use unit-level fixtures for old-correct prose, new structured wrong prose, a mixed
document, future intent, accepted ADR/governance divergence, explicit supersession,
history/changelog, unknown role, generated docs, exact and near duplicates, broken/
cyclic successors, and protected north-star/security/legal material.

Verify:

1. Lifecycle, authority domain, aggregate code evidence, and voice remain
   independent. Age/timestamps produce review signals only; exact linked-code change
   after the document commit produces relevant-change evidence, never conflict.
2. Deterministic structured mismatch returns bounded typed scalar diff and exact
   anchors; unsupported semantics stay unresolved/advisory.
3. Default/current/intent/history/all retrieval returns distinct deterministic sets.
   History contains voice `HistoryOnly` and `Suppressed` even when a proven-divergent
   unit's lifecycle remains Active. One conflicting section never suppresses unaffected units.
4. Malformed/unsupported/stale ledger entries lose suppression and expose findings
   while raw safe code/knowledge remains queryable; unsafe curation is blocked.
5. Build temporal variants for complete-to-root, shallow, bounded-window, rename-
   follow-limited, divergent, dirty/new working tree, clock-skewed filesystem times,
   and unavailable history. Every date/commit reports provenance/coverage; commit
   topology outranks clocks and no time signal alone yields stale/archive/delete.
6. Pause temporal/authority derivation across a watcher update. Stale completion is
   rejected and queues one coalesced latest-state candidate whose marker captures the
   then-live commit/tip; accept only when analyzed target, marker, and candidate
   target agree. Temporal, bridge, authority, and every advertised derived artifact
   complete inside that candidate before one atomic Current promotion; no derived-only
   mutation of an existing Current generation is allowed. Repeat with an
   identical-byte commit/ref-tip change and prove the source-version fence rejects old
   history, recomputes, and converges on one coherent generation.
   During continuous edits, each source has at most one running worker and one pending
   latest marker. A review captured before publication completes from its old source.
7. Multi-project review returns one isolated plan and `review_hash` per source plus a
   deterministic top-level result hash. Repeat with different `limit`, `max_tokens`,
   direct formatting, truncation, and CCR storage; hashes remain identical because
   they cover each complete untruncated plan, not the rendered subset.
8. Summary/document/remediation modes return stable IDs, exact unit/code anchors,
   full aggregate evidence arrays and bridge records referenced by search IDs,
   temporal provenance, backlinks, protected-role/unique-content blockers, and the
   smallest rule-allowed proposal. Age alone proposes review.
   Trigger a change that reorders authority records and build one complete successor
   candidate containing the unchanged content plus the full required derived set;
   atomically promote it. Prior finding/provenance IDs still resolve to the same
   dossiers, and every per-source review result carries its captured source version.
9. Map/context/review calls pinned across watcher or temporal publication never mix
   generations. Role/review budget exhaustion preserves bounded attempt diagnostics
   but blocks complete promotion; strict reads refuse rather than exposing degraded
   coverage or inlining the prose corpus.
   Put a hash-valid superseded unit beyond the authority-record capacity. The
   candidate must either include its complete required suppression/authority proof or
   be discarded with attempt-only capacity/truncation evidence; it cannot promote a
   queryable skipped-suppression/truncated record. After raising capacity, the fresh
   complete candidate keeps the unit outside default/current with voice `Suppressed`
   and retrievable through history/all.
10. Secret-positive synthetic input/content creates no card, link, finding, proposal,
    plan-hash input, log, diagnostic, analytics, CCR entry, or echoed value.

## Curation and crash-recovery acceptance

Use one normal writable current worktree with durable replay/atomic replacement, and
separate explicit-protected, read-only, user-local-without-durable-replay, memory-
only, linked-worktree, and ref fixtures.

1. Preview with explicit actions and fresh review/manifest/policy/target guards.
   Assert no ledger write, temp file, durable intent, or idempotency reservation.
2. Apply one approved plan. After durable intent/capability preparation and before
   source-content I/O, assert one non-cloneable `SourceMutationPermit` is granted and
   the source publishes non-Current. It changes only
   `.symforge-knowledge.toml` using component-confined, handle-relative temp/write/
   sync/replace operations through the permit's pinned `PhysicalRootLease`;
   move/delete, unknown/mixed invalid action, stale guard, wrong source, and
   disallowed capability cases write nothing. Target documents and `.gitattributes`
   remain byte-identical.
3. Two curators serialize and revalidate under one mutation lock. Identical replay
   after success returns stored success before now-stale freshness guards; the same
   key with a different canonical request conflicts. Repeat after an ordinary commit
   and branch switch: resolvable anchor-tip continuity prevents a false foreign-source
   result. A same-path unrelated replacement still conflicts.
4. Run one crash/restart case at each durable stage:

   - after the validated request-hash reservation, before the pending-write intent is
     durably synced;
   - after the pending-write intent sync, before the source temp file is written;
   - after temp-file `write_all`, before temp-file `sync_all`;
   - after temp-file `sync_all`, before atomic replace;
   - after atomic replace, before parent-directory durability;
   - after parent-directory durability, before the success record is durably synced;
   - after success-record sync, before response.

   Each restart must yield exactly one old-or-new complete ledger and one recoverable
   request. Post-replace recovery verifies the exact post-state and terminalizes
   without applying twice; indeterminate/corrupt intent blocks or quarantines instead
   of guessing. Pending startup remains read-only and first promotes a complete
   Current generation. Retrying/cleaning/probing an exact pre-image or writing source
   bytes requires a fresh `SourceMutationPermit`; if Current already contains the
   exact post-image, completion finalization touches only `ProjectStateDir`, is
   persistence-only, and does not grant a permit or revoke Current. Stored terminal
   completion replays byte-identically.
5. On successful apply, pause a reader before the watcher publication. The reader
   finishes from its old captured generation; the receipt reports applied/pending
   generation. New generation-backed acquisitions refuse while non-Current. A later
   reader observes ledger, authority, voice, bridge, and review state atomically only
   after the fresh complete candidate promotes.
6. Disable durable replay or atomic replacement/durability. Preview remains read-
   only, but apply reports a reason-bearing unavailable capability; it never falls
   back to an unsafe best-effort write.
   Only after every non-probe apply requirement is available, exercise the exact Unix
   parent-sync or Windows write-through replacement probe in both the ledger parent
   and `ProjectStateDir` journal parent. Failed/unsupported probes reserve no
   idempotency key and touch no durable record. Instrument every disallowed source
   fixture and assert zero probe file operations anywhere beneath its source root.
7. Secret-positive input is rejected before routing, echo, logging, intent/
   idempotency storage, evidence construction, temp write, or receipt.
8. Crash with a matching `pending_write`, replace the repository at the same path,
   then restart and replay the old key. Recovery quarantines the foreign intent,
   returns typed foreign-source conflict for both recovery and replay, writes no
   ledger bytes, and never reports the old result as applied.
9. Repeat recovery after an ordinary intervening commit. The recorded Git anchor tip
   remains resolvable, so post-image recovery terminalizes normally; non-Git recovery
   likewise requires unchanged root-object identity plus unbroken catalog lineage.
10. Exercise permit commit, guarded refusal before side effect, valid
    `NoSideEffectProof`, failure, rollback, drop, and panic. None directly restores
    Current. A side effect requires a fresh full/delta candidate; a valid no-side-
    effect terminal uses the cheaper verification/no-op candidate, but still validates
    the latest observer cut and mutation epoch and installs a fresh safety package.

## Worktree/ref acceptance

Use disposable linked worktrees/local refs with:

- one identical knowledge blob on two refs;
- one identical blob stored under Markdown and plain-text extensions;
- one divergent version in the current worktree;
- one giant Git blob metadata entry;
- the text-format/byte fixture above committed as immutable blobs;
- one moved ref and one explicit-protected worktree visible to only one authorized
  session.

Verify current precedence, divergent labels, identical-blob dedupe, per-source
identity/version/generation/digest with an exact sealed selection/generation
bijection for generation-backed aggregation, bounded
ref-mapping memory, no materialized giant content, no Git subprocess/remote fetch/LFS smudge, and
deterministic repeated output. The same blob must produce the same disposition,
units, secret outcome, bridge, lifecycle/evidence/voice, and hashes as filesystem
ingestion, except for explicit source and temporal-provenance fields.
The cross-extension blob shares only raw bytes; classification-specific extraction
and secret-policy inputs are re-derived.

Run `current`, `worktrees`, `local_refs`, and `all` with one project, two explicit
projects, and session-scoped `projects=["*"]`. Assert one captured source set per
selected `ProjectInstance`, canonical source ordering, current-worktree precedence,
and zero source/project identity leakage. Every generation-backed selected source
must be lifecycle `Current`; otherwise return one closed `SourceRefusal` whose
receipts and evidence form an exact selected-source bijection. Pure immutable Git
observation remains independently available as `GitObservation`, without generation
or repository-wide coverage/absence semantics; exact-tree `NotInTree` remains legal.
No empty/degraded generation success lane or silent partial generation aggregate
remains.
Ref movement atomically invalidates old mappings. Local-ref budget failure must not
delay or degrade current-worktree readiness. A second session's wildcard cannot
inherit the protected worktree; only its own direct exact override may admit it.
Race continuous P1 add/update/remove publication against a long P0 build. P0 reaches
readiness without unbounded retry; every completed source update remains present;
P1-only registry commits preserve every latest unaffected sibling and leave the
current worktree's opaque source publication/`GenerationAuthority` unchanged. Each
commit still mints a new never-reused whole `ProjectRuntimePublication` identity in
the sole ArcSwap root. Numeric diagnostic generations/epochs may advance but are
never used as publication authority.

## `ObservedRefreshGateV1` and activation acceptance

Public PreventiveV1 enablement is one indivisible v11 cut. Run the checked-in,
versioned `ObservedRefreshGateV1` campaign with:

- baseline commit `1521abb0`, named and digested SymForge plus maximum-admitted
  calibration corpora, fixed cache/quiescence/concurrency state, and declared
  OS/host class;
- add, modify, delete, rename, terminal-classification, and edit-burst workloads for
  daemon, stdio, serve, and the managed embed observer/poller;
- delivered-event, latched gap/`need_rescan`, and intentionally suppressed-
  notification campaigns;
- an exact completion receipt containing the new byte identity, canonical manifest
  effect, and every certificate required by the advertised strict scope;
- delta-vs-clean-full equivalence for sealed artifact digests, manifest, and the
  representative query corpus before latency is counted;
- p95 visibility at or below 2 seconds, maximum at or below 5 seconds, and p95 no
  worse than 1.25x the pinned v10 externally observable baseline;
- retained-plus-candidate peak admitted accounting bounded by pre-granted delta
  residency plus declared scratch/headroom, with sustained bursts converging and no
  shipped full-repository rebuild/refusal cycle for every edit.

Then prove one process-wide, non-user-selectable activation sequence:
`LegacyOpen -> LegacyClosing -> PreventiveV1Open`. Close legacy query, cache, CCR,
retrieval, and response-finalization registration; drain admitted work; invalidate v10
cache/CCR authority; install the successor schema/mode epoch; and only then open every
preventive daemon/stdio/serve/embed/snapshot/observer/mutation/local-ref/derived lane.
The activation oracle must prove legacy and PreventiveV1 publication roots are never
simultaneously authoritative and that PreventiveV1 has no in-place fallback.

## Secret-safety acceptance

Use runtime-assembled synthetic canaries; never commit or print the tested value.

1. A known sensitive path is cataloged without a content read.
2. A safe template path still runs the content detector.
3. Clean prose returns byte-exact evidence.
4. Positive or indeterminate content becomes metadata-only for both targets and
   drops transient bytes/hash before publication.
5. Query, path, heading, context, source label, diagnostic, and ranking fields are
   independently guarded without echo.
6. CCR receives only an already-safe formatted result tagged with policy version.
7. Snapshot policy mismatch forces re-scout before lifecycle `Current`.
8. Boolean containment checks prove snapshot/CCR/analytics/log/diagnostic bytes do
   not contain the runtime canary; failure messages never interpolate it.

## Release gate

```powershell
cargo fmt --check
cargo check --features server
cargo clippy --all-targets --features server -- -D warnings
cargo test --features server --all-targets -- --test-threads=1
cargo test --no-default-features --features embed --lib
```

Then:

- verify the hash-pinned V11 refreeze manifest, detached attestation, external
  `RefreezeApprovalRecordV11`, amendment mapping, and closed public API allowlist for
  the exact activation commit/tree;
- verify the default full surface contains exactly 39 tools and compact remains
  exactly `symforge`, `symforge_edit`, and `status`;
- inspect terminal-disposition/metadata/admitted/in-flight accounting equality and
  independent binding/membership/placement/durability/lifecycle/observer/capacity
  health on the real repository without inferring queryability from side fields;
- prove every query captures the sole whole `ProjectRuntimePublication` root, sibling
  updates mint a new project-root identity without changing unaffected source
  `GenerationAuthority`, and numeric generations are diagnostic only;
- rerun the full no-match/error/selector, review-hash, lifecycle-parity, curation-
  crash, and memory-only typed-checkpoint matrices;
- run `ObservedRefreshGateV1`, retained-plus-candidate capacity tests, and the
  `LegacyOpen -> LegacyClosing -> PreventiveV1Open` activation race/oracle;
- prove every generation-backed non-Current selected source returns `SourceRefusal`,
  every promoted generation is complete, retained generations remain internal, pure
  observation/health claims keep only their own typed authority, every mixed claim
  comes from a compatible `ClaimContext`, and no degraded wrapper, partial candidate,
  cold placeholder, or legacy fallback is queryable;
- run the corpus comparison, record exact pointers/token reduction, and require a
  median returned-token count at least 50% below the recorded baseline;
- run adversarial implementation review;
- update `tasks/todo.md` with exact receipts;
- verify no delegated worker processes remain.
