# Review request — HTTP sidecar readiness and publication fencing

## 0. Review target

Repository: `special-place-ai-heaven/symforge`

Branch/worktree: `fix/http-sidecar-readiness`

Base: `origin/main` at SymForge 10.0.3,
`71fb88429134462cc8bdf1022ee3037bcec5f65d`.

**Frozen implementation target:**
`71c14e309a9a45ad01d145b136ef556a6b86190e`.

**Expected binary diff hash:**
`920e3002030392732514f2d560839da05e214099`.

Review that commit, not the branch tip and not an uncommitted working tree:

```text
git fetch origin
git show --stat 71c14e309a9a45ad01d145b136ef556a6b86190e
git diff --binary 71fb88429134462cc8bdf1022ee3037bcec5f65d...71c14e309a9a45ad01d145b136ef556a6b86190e | git hash-object --stdin
git diff 71fb88429134462cc8bdf1022ee3037bcec5f65d...71c14e309a9a45ad01d145b136ef556a6b86190e
```

Abort the review if the computed diff hash is not the expected hash above.
Record both identities in the answer. Do not assume this packet's line numbers
remain current.

This is an adversarial correctness review. I want concrete defects, not a
summary or a style pass. A clean verdict is useful if it follows an actual
sweep.

### Three-reviewer protocol

This request is being sent independently to Grok 4.5, Composer 2.5, and Claude
Fable. Do not read or incorporate another reviewer's findings before submitting
your own. All three reviewers should answer Q1–Q8; the emphasis below is an
additional lens, not permission to skip the rest:

- **Grok 4.5:** emphasize state-space enumeration, publication-generation
  domains, and concrete concurrency/ABA interleavings.
- **Composer 2.5:** emphasize the complete production call-path/route sweep,
  public API compatibility, and tests that can genuinely fail.
- **Claude Fable:** emphasize end-to-end semantic truth across HTTP, hook
  fallback/telemetry, impact claims, and any defect the stated invariants failed
  to anticipate.

An earlier Claude Fable pass reviewed four moving, uncommitted snapshots and is
preserved as
`REVIEW-FINDINGS-claude-fable-http-sidecar-readiness-2026-08-10.md`. It is
historical evidence, **not** a verdict on the frozen target. Do not read it (or
either other review) before completing your independent pass.

Write findings to the reviewer-specific final file below without editing the
implementation or this request:

```text
Grok 4.5:      docs/reviews/REVIEW-FINDINGS-grok-4.5-http-sidecar-readiness-final-2026-08-10.md
Composer 2.5: docs/reviews/REVIEW-FINDINGS-composer-2.5-http-sidecar-readiness-final-2026-08-10.md
Claude Fable: docs/reviews/REVIEW-FINDINGS-claude-fable-http-sidecar-readiness-round2-2026-08-10.md
```

## 1. Original defect and intended contract

The merged cold-start repair stopped MCP reads from treating a rootless
`EmptyBootstrap` placeholder as Ready. The HTTP sidecar remained another route
to the same unsafe state: handlers could freshen/admit one file into the
placeholder and then answer global questions from a partial, source-unbound
index.

The implementation under review intends to establish all of these invariants:

1. Five sidecar content families refuse an unqueryable publication:
   `/outline`, `/symbol-context`, `/prompt-context`, `/repo-map`, and `/impact`.
   Their workflow aliases and daemon proxy paths must behave identically.
2. `/health` and `/stats` remain available diagnostics while the index is not
   queryable.
3. A queryable sidecar publication is `Ready` or source-bound `Empty`, has both
   a source identity and an indexed root, and has queryable freshness. `Current`
   is queryable. `Degraded` always refuses. `Verifying` is queryable only for a
   source-bound `Empty` publication whose snapshot verification state is
   `NotNeeded` (the deliberate empty-repository initialization shape), never a
   pending/running snapshot. A legitimate rooted empty repository must still
   serve repo-map and admit its first file.
4. Readiness refusal is HTTP 503 with no enrichment body. Hook clients fail
   open, may use one valid daemon fallback, and must not call a live-but-loading
   sidecar dead or stale.
5. Root mismatch is HTTP 409, remains a live-refusal/sidecar-error signal, and
   must not produce a restart hint. A root-resolved alternate daemon session
   may still be tried when doing so cannot loop back to the same session.
6. A request that is already unqueryable at entry performs no freshen, index,
   cache, pre-update-snapshot, or hook-stat mutation.
7. A project rebind cannot cause bytes or side-table state from project A to be
   committed into project B. Project generation, source identity, and indexed
   root form the cross-project fence.
8. A successful same-project freshen is allowed to publish. The fence must not
   pin full publication/content generation and reject its own update.
9. Impact rendering consumes the exact immutable publication receipt produced
   by its mutation. A later watcher publication must not change what the request
   claims it indexed, nor let it drain a later pre-update snapshot.
10. This is a 10.0.x patch. Existing public Rust types and public struct fields
    must remain source-compatible, especially `embed::ReindexResult` and
    `SidecarState::symbol_cache`.

## 2. Ground rules

- A comment is a claim under review, not evidence.
- Existence is not invocation. Trace production call paths.
- For every race finding, give a concrete A/B interleaving and identify the
  mutation or wrong response.
- Distinguish the full `PublishedGeneration.publication_generation` from the
  health/live `PublishedIndexState.generation`; bridge- or authority-only
  publications can advance only the former.
- Exact publication fencing supplements, and never replaces, the caller's
  project-generation fence.
- Do not call a test protective merely because it is green. Explain why it
  fails when the relevant guard/fence is removed.
- Cite `file:symbol` and current `file:line`. Label each finding **proven**,
  **likely**, or **speculative**.
- Do not spend the review on formatting, naming, or comment prose.

## 3. Known findings being remediated

These were found internally during the final pass. Verify that the reviewed
snapshot actually closes them, but do not stop after rediscovering them:

1. A new `PublicationRejected` variant was temporarily added to the semver-public,
   exhaustively matchable `ReindexResult`. The final design must keep rejection
   typed inside the crate and preserve the six-variant public enum.
2. `freshen_exact_path_for_targeted_retrieval` temporarily collapsed a rejected
   publication to boolean `false`, allowing exact-path reads to continue from
   stale state. Rejection must propagate as an explicit retry/refusal.
3. Single-file CAS temporarily passed the full publication generation into
   store seams that compare the health/live generation. A bridge-only publish
   then caused four deterministic CAS losses. The two generation domains must
   stay distinct.
4. Hook topology temporarily inferred “initial endpoint is daemon” from
   `session_id.is_some()` while the routing function treats empty/whitespace IDs
   as local. Normalize once and use one predicate.
5. Suppressing daemon rediscovery for every failure from a daemon-shaped
   descriptor can strand a hook on a closed session. The no-loop rule is needed
   for an index-not-ready response; unavailable/root-conflict cases require a
   failure-specific analysis.
6. New edit retry messages initially classified as `InvalidRequest` instead of
   retryable/internal failure.
7. An exact index-publication fence alone does not prove continued filesystem
   absence. Audit delete/recreate ABA windows and the direct watcher removal
   paths rather than assuming the receipt work closes disk-only races.
8. Degraded freshness was initially written after construction, leaving the
   immutable published bundle falsely `Current`; startup construction must
   publish the degraded reason atomically.
9. Observation/reconciliation degradation was initially sticky after healing,
   then snapshot-verification transitions briefly overwrote unrelated degraded
   reasons. Freshness must be recomputed from manifest, scout coverage, and
   snapshot state while preserving unrelated typed reasons.
10. Impact initially performed a readiness check after committing its receipt,
    cache, snapshot, and stats effects; a freshness-only transition could turn
    the useful first response into 503. Post-commit validation must preserve the
    response while still rejecting project/source/root rebinding.
11. Confirmed-absent removal initially rejected on unrelated same-project
    publications and the snapshot verifier had a separate disk-recreation ABA
    path. Project generation plus under-lock filesystem absence is the deletion
    authority; every production removal lane must use it.
12. A symbols-only impact cache initially made a later one-symbol edit report
    every matching symbol as changed. Exact snapshot/index content must outrank
    the compatibility cache when computing body changes.

## 4. Required questions

### Q1 — Complete route sweep

Enumerate every production HTTP route and workflow alias that can read, freshen,
admit, remove, or derive global absence/caller claims from the live index. For
each, state whether it is guarded, deliberately diagnostic, or still unsafe.
Include daemon proxy dispatch, not only the local Axum router.

### Q2 — Readiness and root fence

Can any source-unbound `Ready`, rootless `Empty`, `Loading`, or `Degraded`
publication pass? Can a rooted `Empty` repository still serve and admit its
first file? Can caller-root validation pass for A and the handler subsequently
serve or mutate B after a rebind?

### Q3 — Mutation and receipt linearizability

For edit impact and new-file impact, trace disk observation, pre-update snapshot,
parse/admission, publication, cache write, stats write, caller lookup, and final
response. Identify the linearization point. Look specifically for:

- a response derived from a later publication rather than the receipt;
- a rejected mutation rendered as success;
- same-generation concurrent impacts regressing cache state;
- one request consuming another request's snapshot;
- side effects committed before a later 503 that callers may retry.

### Q4 — Single-file publication semantics

Audit every success, terminal admission, hash-skip, missing-file, exclusion,
retry-exhaustion, and publication-failure branch. Confirm:

- the store CAS uses the correct generation domain;
- the exact winning publication is returned while the writer lock still owns it;
- project generation is checked independently;
- `Removed`, `Skipped`, and `Reindexed` are never reported when no corresponding
  publication occurred;
- disk-only delete/recreate cannot authorize removal of a recreated path.

### Q5 — Hook behavior matrix

Trace local descriptor, daemon-backed descriptor, missing descriptor, stale
descriptor, empty legacy session ID, and fallback-selected daemon for HTTP 200,
409, 503, 404/500, and transport failure. For each, check request count, fallback
eligibility, fail-open output, session attribution, outcome telemetry, and
presence/absence of stale-port restart hints.

### Q6 — Public API compatibility

Diff the public/embed facade and public struct fields against `origin/main`.
Look for enum-variant additions, public field type changes, visibility changes,
or signature changes that break an external exhaustive match or custom router.
Do not limit this check to files named `embed.rs` or `lib.rs`.

### Q7 — Tests that can fail

For each new high-value test, name the production condition it pins and the
smallest code reversion that makes it fail. In particular check:

- the 10-route real Axum loading matrix and no-mutation assertions;
- independent readiness conjuncts (status, source, root);
- source-bound Empty first-file admission;
- exact Reindexed and HashSkip receipts after later publication;
- same-hash snapshot ABA preservation;
- stale project A unable to remove or publish into rebound project B;
- impact entry points sharing one single-flight lock;
- failed impact preserving its pre-update baseline;
- hook 503/409 fallback, no-loop, attribution, and no-restart-hint cases.

Call out false-green fixtures, mocks that accept the wrong path, unbounded joins,
or tests that only exercise a direct helper while the real Axum/daemon path can
still differ.

### Q8 — Anything else materially wrong

Report any additional path that can return wrong-project data, global absence
from partial state, stale exact-path content, invented success, an infinite wait,
or a patch-release API break.

## 5. Deliberately out of scope

Do not treat these as new findings for this patch unless this diff makes them
worse:

- the separate typed bootstrap lifecycle work for daemon catalog-capacity
  refusal and failed local cold-start reload;
- snapshot verification liveness/retry redesign;
- Spec 027 answer identity disclosure;
- `feat/knowledge-llm-sift`;
- Terminal Commander watcher-stop semantics.

The sidecar must nevertheless refuse the currently published nonqueryable states
those follow-ups produce.

## 6. Verification state for the frozen implementation commit

The following passed on
`71c14e309a9a45ad01d145b136ef556a6b86190e` (or the identical staged tree
immediately before that commit):

```text
cargo check --all-targets
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib freshness -- --test-threads=1
cargo test --lib snapshot_verify_transitions_preserve_other_degraded_reasons -- --test-threads=1
cargo test --lib degraded_initial_scout_is_published_in_the_trust_bundle -- --test-threads=1
cargo test --lib sidecar_queryability_requires_status_source_and_root_independently -- --test-threads=1
cargo test --lib removal -- --test-threads=1
cargo test --lib missing_file_finalization_ignores_unrelated_same_project_publication -- --test-threads=1
cargo test --lib targeted_retrieval_refuses -- --test-threads=1
cargo test --test hook_subprocess_integration -- --test-threads=1
cargo test --test hook_enrichment_integration -- --test-threads=1
cargo test --test sidecar_contract -- --test-threads=1
cargo test --test sidecar_integration -- --test-threads=1
cargo test --test batch_rename_perf -- --test-threads=1
```

The last command is important: an early moving-target review measured a real H.7
failure on its first uncommitted snapshot. The frozen candidate passes the
existing 5-second best-of-N gate. Likewise, the early pass's first-file Empty
admission, formatting, post-impact 503, deletion convergence, and live HTTP 500
findings are fixed and covered on the frozen target.

Claude Fable's frozen-target round-2 review independently ran the full
release-grade gate below and reported every command green. The landing agent
accepted that frozen review as the external gate; no implementation changed
afterward, and PR CI will rerun the repository gates before merge:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
node scripts/verify-tools.cjs --bin target/release/symforge.exe
node scripts/verify-tools.cjs --fixture verify-tools-real --bin target/release/symforge.exe
```

On Unix, omit the `.exe` suffix. There is no `--surface` CLI flag: compact
cases are exercised inside the canonical fixture harness, which sets
`SYMFORGE_SURFACE=compact` for the cases that require it.

Green commands are evidence of build/test health, not proof of the concurrency
contracts above.

## 7. Expected answer format

Start with findings ordered P0/P1/P2/P3. For each finding provide:

1. confidence;
2. current `file:line` and symbol;
3. concrete reachable sequence/interleaving;
4. user-visible or state-corruption consequence;
5. smallest safe remediation;
6. a regression test that fails before the remediation.

Then answer Q1–Q8 explicitly, including “no additional finding” where appropriate.
End with one verdict: **block**, **land after listed fixes**, or **clear to land**.
