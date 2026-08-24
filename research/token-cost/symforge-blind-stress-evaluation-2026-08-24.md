# SymForge blind stress evaluation — 2026-08-24

Status: blind provisional report. The Grok 4.6 report has not been viewed.
These findings were frozen independently for a later overlap/false-positive
comparison.

## Executive verdict

SymForge is useful code intelligence now, not vaporware. Its strongest tools
materially improve large-file navigation, exact symbol retrieval, reference
inspection, edit planning, and recovery. Ordinary Rust, Python, JavaScript and
TypeScript, Go, C#, and non-macro-heavy C++ answers were usually exact and
actionable. Watcher refresh, guarded edits, idempotent replay, checkpoint
verification, malformed-config diagnostics, and CCR retrieval all worked.

The token-saving premise is real but conditional. It is strongest in a
large-codebase, narrow-symbol workflow. It can be weak or negative on small
files, already-perfect native searches, verbose health/knowledge responses, or
a short session that pays the full 39-tool schema cost. The compact three-tool
surface solves most schema tax. Installed 11.0.5 compact local processes never
became ready in the tested topology. A clean current-checkout 11.0.6 launch
served a search in under one second, but a project switch followed by shutdown
made the next compact launch remain Loading for more than 20 seconds despite a
complete watcher reconcile. Current source therefore has a state-dependent
readiness/recovery defect, not a complete fix.

The knowledge lane has excellent exact retrieval and provenance, but authority
behavior is not intuitive enough to trust as a default mental model. Historical
text can appear in default results while being absent from explicit history
scope. Provenance frequently dominates the useful answer. Curation is strict
and safe, but local availability and the approval path were inconsistent.

My honest product judgment:

- Core code navigation: strong and useful now.
- Structural mutation: conservative and mostly strong; rename is too timid and
  error metadata needs one reliable runtime contract.
- Speed: interactive for code queries, slow for knowledge.
- Realized token economy: meaningful on targeted work, not universally or
  automatically positive.
- Knowledge: promising beta, not a dependable default authority layer.
- Product polish: held back by compact readiness transitions, package drift, automatic
  worktree mutation, parser/test-filter edges, and noisy trust envelopes.

## Versions and topology

- Available MCP connector: version 11.0.5, local_process, full 39-tool surface
  unless stated otherwise.
- Current checkout during source review: version 11.0.6.
- Project guidance still describes a shipped v8.14.0 surface.
- The connector was local-process rather than daemon-backed. Cross-project and
  local-ref positive paths could be checked only for honest refusal.
- A clean 11.0.6 release build completed successfully in 18m 23s. Focused
  current-checkout controls are recorded below.

## Corpus and method

The external report was withheld. I built a deterministic seven-language oracle
outside this checkout and cloned seven public repositories:

| Corpus | Language | Tracked files | Indexed symbols |
|---|---:|---:|---:|
| ripgrep | Rust | 237 | 5,826 |
| Flask | Python | 236 | 2,296 |
| Express | JavaScript | 213 | 2,827 |
| p-limit | JS/TypeScript | 16 | 276 |
| chi | Go | 99 | 821 |
| spdlog | C++ | 181 | 2,603 |
| MediatR | C# | 192 | 1,715 |

The oracle contained known definitions, calls, interface implementations,
tests, vendor noise, valid/malformed config, Markdown architecture claims, an
explicitly obsolete history document, plain-text runbook facts, and two Git
revisions/local branches. Unique markers and numeric values made correctness
independent of model judgment.

All 39 installed tools were called. Resources and prompts were listed and
sampled. Mutations were previewed; one symbol-local edit was applied, replayed
with the same idempotency key, challenged with same-key/different-body input,
verified, and reverted. External disk edits tested watcher convergence. Native
rg controls and repeated calls supplied speed/output baselines.

## Scorecard

| Dimension | Score | Judgment |
|---|---:|---|
| Core code correctness | 8.0/10 | Strong on six ordinary language families; macro-heavy C++ and test filtering are material exceptions. |
| Safety/recovery | 7.0/10 | Checkpoint, watcher, rollback, preview, syntax validation, and replay were strong; post-switch snapshot readiness is a serious recovery regression. |
| Code-query speed | 8.0/10 | Roughly 0.2–0.3 seconds median through MCP for warm reads/searches. |
| Index/knowledge speed | 6.5/10 | Public repos were quick; this doc-heavy checkout took 47.4 seconds initially. Knowledge search was about 1.6 seconds median. |
| Realized token saving | 6.5/10 | Large targeted-work wins; schema and verbose envelopes erase short-task wins. |
| Token-saving potential | 8.5/10 | Compact schema is about 94% smaller if readiness is fixed. |
| Knowledge correctness/UX | 6.0/10 | Exact retrieval/provenance are strong; lifecycle, freshness, verbosity, and ref availability are not smooth. |

## Correctness: what worked

- Exact definitions and calls were recovered in all seven oracle languages.
- Rust trait and TypeScript interface implementations were correct. A C#
  interface with no implementation correctly returned none.
- Symbol context, callers/callees, file context, match inspection, and targeted
  retrieval were generally internally consistent.
- Literal, regex, whole-word, reference-enriched, vendor, and ordinary tests/
  path filters worked.
- The watcher reflected an external edit within 1.6 seconds and its restoration
  within another 1.6 seconds.
- Valid JSON passed; malformed JSON returned a precise line/column diagnostic.
- checkpoint_now with verification wrote and verified a snapshot.
- An applied edit replayed with the same idempotency key. Different arguments
  under the same key were refused.
- Batch validation prevented partial writes when one symbol was missing.
- Exact Markdown and plain-text facts returned hashes and exact anchors.
- CCR retrieval returned the stored full result.

## Material correctness gaps

1. **Macro-heavy C++:** spdlog had 124/180 partial files, including 98
   unexpected repo-owned partials; only 56 parsed fully. The declaration
   class SPDLOG_API async_logger was absent from symbol search. Across the
   other six public repos, 953/959 files parsed fully, so this is concentrated.

2. **Rust test suppression:** include_tests=false still returns functions from
   src/protocol/format/tests.rs and include_tests=true is identical. Source
   classification handles tests directory segments and test_, _test, .test,
   _spec, and .spec stems, but not a stem exactly test or tests. testutil.rs
   also survives.

3. **Natural-language punctuation:** “who calls normalize in src/lib.rs?”
   routes with path src/lib.rs? and fails. Source accepts a slash-containing
   scope tail as a path before terminal punctuation is removed.

4. **Mixed-language conventions:** the seven-language oracle was described as
   JavaScript conventions. A research-area path scope returned Rust project
   conventions. This can mislead polyglot edits.

5. **Change terminology:** two changed Markdown documents became nine
   changed_symbols in detect_impact, while diff_symbols/what_changed reported
   zero code symbol boundaries. The facts are explainable but the term is not
   semantically consistent.

6. **Host-visible failure semantics:** a missing-symbol batch returned “ROLLED
   BACK; no files modified” with isError:false. Raw JSON-RPC from both installed
   11.0.5 and current 11.0.6 contained exact symforge/result_status metadata:
   not_found plus the failing operation index. The Codex MCP wrapper used for
   the main dogfood calls did not expose that metadata. This is not a missing
   SymForge classifier; it is an integration/semantics problem. A host that
   keys only on isError sees a failed mutation as success, while a host that
   preserves _meta can handle it correctly.

7. **Rename recall:** batch_rename found the definition but classified an exact
   same-file call as uncertain and would leave it unchanged. This is safe but
   incomplete despite an already-known reference edge.

## Speed

### Indexing

| Corpus/run | Time |
|---|---:|
| Checkout, first explicit installed-runtime index | 47.449 s |
| Checkout, later fresh load shown by health | 4.747 s |
| Seven public repos, cold total | 6.001 s |
| Seven public repos, warm total | 3.613 s |
| Oracle cold / warm | 0.503 s / 0.359 s |

The public repositories totalled 1,139 indexed files and 16,364 symbols: about
190 files/s cold and 315 files/s warm. Individual cold times were 0.280–1.636
seconds. The 47-second doc-heavy checkout case is not normal small-repo
performance, but it matters for the new knowledge use case.

### Warm queries (15 repeats on 1,129 files)

| Tool | Median | p95 |
|---|---:|---:|
| health_compact | 244 ms | 263 ms |
| search_symbols | 216 ms | 259 ms |
| search_text | 225 ms | 258 ms |
| get_symbol | 236 ms | 266 ms |
| get_file_context | 246 ms | 285 ms |
| search_knowledge | 1,558 ms | 1,730 ms |

Native rg --files was about 40 ms median. A repo-wide frequent-identifier rg was
about 615 ms versus 225 ms through SymForge. A constrained Rust LiveIndex rg
was about 218 ms—equal to SymForge but with less context. SymForge wins broad
high-noise search and loses simple path listing; it is not uniformly faster.

## Token economics

### Schema cost

| Surface | Tools | JSON chars | Approx. tokens (chars/4) |
|---|---:|---:|---:|
| Full | 39 | 85,608 | 21,402 |
| Compact | 3 | 4,768 | 1,192 |

Compact removes 80,840 characters, about 20,200 tokens or 94.4% of the full
schema. This is the largest reliable token-saving result. Current 11.0.6 showed
that compact can realize it from a healthy state, but the reproducible
post-switch Loading state and a missing compact end-to-end task benchmark remain
the missing proof.

### Retrieval results

- tools.rs: raw estimate about 366k tokens; outline about 29k before tighter
  budgeting, roughly 92% smaller.
- store.rs: raw about 92k; outline about 7.4k. One LiveIndex body was about 905
  tokens.
- Budgeted file context reduced large-file output to about 1,200 tokens and
  disclosed omissions.
- Broad idempotency_key search returned 1,527 characters of grouped context
  instead of 134 rg lines.
- A perfectly narrowed native definition search returned five exact lines at
  the same latency; SymForge's trust envelope was larger. Savings disappear
  when the native query is already ideal.

Health later claimed about 155k tokens saved, 73.7k served, and 67% reduction
against its competent-manual model. This is instrumentation, not isolated proof:
it accumulates across projects, included one huge raw todo read, mixes
historical/global hook totals, and models a 50-line/80-byte alternative rather
than observing a control agent. Schema and provenance overhead are not cleanly
represented per task.

The defensible claim is conditional: surgical symbol work on large files often
saves 70–95% against whole-file reads and substantially against broad grep.
Small files, narrow grep, short sessions, and full schemas can be neutral or
negative. Net savings should include schemas, retries, envelopes, and CCR.

## Knowledge feature

### Strengths

- Markdown and plain text were indexed and rare facts retrieved exactly.
- Provenance is excellent: source/publication generations, hashes, anchors,
  lifecycle, voice, bridge state, and coverage limits.
- Malformed JSON remained retrievable while syntax validation rejected it.
- review_knowledge produced dossiers, findings, hashes, and remediation.
- Symbol context attached exact knowledge backlinks.
- Curation correctly rejected stale hashes, mismatched action IDs, and
  mutations not authorized by a fresh review.

### Weaknesses

1. An obsolete document headed Historical Notes remained lifecycle/domain
   unknown in relevant output. It appeared beside the accepted current value,
   but authority_scope=history returned none. Source explains the strict split:
   a heading can establish historical authority domain, while lifecycle needs
   status metadata, archive path, or policy. Internally defensible, externally
   surprising.

2. Freshness stayed degraded[WatcherUnavailable] after re-index, active watcher,
   successful events, zero overflows, and reconcile repairs. Source deliberately
   latches an observer gap until proof-grade reconciliation. The tested
   project-switch path did not clear it.

3. source_scope=local_refs returned no_sources_in_scope despite a local branch.
   This local connector published one source; daemon positive control was not
   available.

4. search_knowledge was roughly seven times slower than code search.
   review_knowledge stored 28,956 characters for ten dossiers and still had
   overflow. Provenance often outweighed the fact.

5. Repo maps, including narrow subtree tree views, append a large repository-wide
   knowledge model that can dominate the answer.

6. Curation capability changed from unavailable (atomic durability) to available
   later in the oracle, then unavailable after switching/re-indexing. Guards
   were safe, but local availability is not predictable to a user.

## Compact-surface readiness and recovery defect

Five isolated installed-runtime clients used compact surface, no daemon, and
both CWD and SYMFORGE_WORKSPACE_ROOT set to the oracle. Each status showed the
correct root, 17 files, and 57 symbols, but index_ready:false for more than 20
seconds. Every facade intent refused as still loading. Compact exposes no
index_folder, so there was no in-surface recovery.

A full-surface process with the same root/no-daemon setting became Ready and
returned health in under one second. This is topology-specific, not a claim
about daemon-backed compact. It still contradicts compact guidance that setting
the root and reconnecting is sufficient.

The fresh current-checkout 11.0.6 release binary initially passed the same
root/no-daemon compact control. It became Ready and served a two-step
search_files plus search_text chain in 0.94 seconds, returning the expected
normalize definitions.

That success was state-dependent. In three repeated cycles, a full-surface
11.0.6 process switched oracle to p-limit and back to oracle, reported
degraded[WatcherUnavailable], and shut down cleanly. The immediately following
compact process restored 17 files from snapshot with snapshot verification
Pending, but index_ready stayed false beyond 20 seconds. Debug logs showed the
watcher hash-skipping/reconciling every file and reporting “now fresh,” while
background snapshot verification never reached its completion log. The compact
surface has no index_folder recovery. A full-surface explicit index_folder, or
in some runs one additional restart, recovered readiness.

The strongest implementation hypothesis is a publication-fence/state-transition
race between snapshot background verification and watcher reconciliation. The
observable bug does not depend on that hypothesis: populated index, complete
reconcile, and permanent Loading disagree.

The same current binary independently reproduced two additional source-head
bugs: ask retained the question mark in src/lib.rs?, and include_tests=false
returned the test function from src/protocol/format/tests.rs.

## Automatic filesystem side effect

Indexing each public Git repository appended /.symforge/ to its tracked root
.gitignore. All seven clean clones became dirty; what_changed/impact then
reported SymForge's edit as user work.

Source makes this an intentional guarded hygiene mutation. It is still a poor
default for an operation described as indexing: it edits tracked source without
explicit consent, pollutes change analysis, and creates commit noise in every
repository merely inspected. Prefer observe-only indexing, an explicit init
repair, or a non-tracked local exclusion where appropriate.

## Contract drift and other impediments

- Initial health was unbound/empty despite repository CWD; explicit indexing
  was required.
- Installed 11.0.5 versus checkout 11.0.6 made dogfood easy to misattribute;
  11.0.6 improved clean startup but retained state-dependent compact failure.
- Project guidance documents v8.14.0; runtime/source are v11.x.
- Prompt listing includes knowledge-hygiene, omitted from project guidance.
- Debug prompt says detect_impact defaults origin/main; live schema says main.
- Resource metadata used project name project in an oracle-bound process.
- Small inline Rust tests were identical with include_tests false/true because
  source only collapses modules over 100 functions unless outline is explicit.
- The structural example without a return type misses normal Rust functions
  with return types; a pattern containing -> $RET worked.
- Multi-project/local-ref success was blocked by local topology.
- Subagents were not fanned out because project-local lessons document repeated
  Windows child-process leakage. One controlled build process was tracked.

## Recommended priorities

1. **P0/P1: fix compact readiness transitions.** Add an artifact-level golden
   sequence: healthy snapshot, project switch away/back, clean shutdown,
   immediate compact restart, status polling, and every facade intent. A
   complete watcher reconcile must retire or complete Pending snapshot
   verification; every early-return path must publish a terminal state. Retain
   an in-surface recovery path and test packaged npm/native binaries.

2. **P1: net token accounting.** Include schema, request/response, retry, CCR,
   and provenance tokens per task. Separate estimated_saved from measured_net.

3. **P1: stop tracked-file mutation during indexing.** Put .gitignore repair
   behind explicit initialization/approval.

4. **P1: macro-heavy C/C++.** Add spdlog-like decorated class/enum/method
   fixtures and per-language expected missing-symbol diagnostics.

5. **P1/P2: intuitive knowledge authority.** Map HistoricalRecord into history
   retrieval, infer a review warning from obsolete/superseded wording, or
   prominently require status: historical. Separate contradictory values.

6. **P2: progressive trust envelopes.** Default to a one-line provenance digest
   and retrieve hashes/dossiers on demand. Do not append a whole-repo knowledge
   model to a narrow subtree map unless requested.

7. **P2: deterministic edges.** Strip path punctuation; classify test.rs,
   tests.rs, and testutil; separate document sections from code symbols in
   change tools.

8. **P2: reference-driven rename.** Treat exact parsed call edges as confident,
   keep comments/strings uncertain, and prove no unresolved references remain.

9. **P2: one host-visible result-status contract.** Typed _meta exists, but
   wrappers may discard it and not_found keeps isError false. Either require
   integrations to preserve result status, duplicate status in bounded text,
   or treat unsuccessful mutations as MCP errors. Smoke test the complete host
   path, not only source-unit results.

10. **P2: generated release documentation.** Generate tool/resource/prompt
    inventory from the binary and surface checkout/package mismatch in health.

## Reproduction artifacts

- External root: E:\symforge-eval-20260824
- Probe: E:\symforge-eval-20260824\compact-probe.ps1
- Oracle: E:\symforge-eval-20260824\oracle

Artifacts are retained so another model or developer can reproduce/challenge
the findings.

## Blind-comparison checkpoint

This verdict and issue list were written before viewing Grok 4.6. The next phase
should classify each external claim as independent overlap, valid Grok-only
miss, valid SymForge-only miss, version/topology difference, unsupported claim,
false positive, or already fixed in 11.0.6.

## Post-freeze internal corroboration

After freezing the independent findings, I read the repository's July 13 paired
feature benchmark. It reported 24.8% fewer end-to-end tokens with SymForge
enabled and indexed across two completed runs (36.2% and 17.1%). The benchmark
included schemas, prompts, turns, retries, verification, and final output, so it
is stronger evidence than the health estimator and supports the claim that net
savings can be real.

Its own caveat is important and agrees with this report: only two pairs were
valid, the spread was large, and enabled agents continued to use native
commands. It proves a positive result for that host/task, not that explicit
SymForge retrieval caused all 24.8% or that every task saves tokens. This raises
confidence in the conditional token verdict but does not change the 6.5/10
realized-economy score.
