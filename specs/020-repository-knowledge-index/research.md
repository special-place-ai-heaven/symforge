# Research: Repository Knowledge Index

## Executive decision

Stay inside the existing Rust `LiveIndex`. The repo already has exact in-memory
content search and document-section provenance. Add a metadata-first manifest and
activate the unused text scope; do not add embeddings, SQLite FTS, Tantivy, or a
second content store or knowledge-specific sidecar in v1.

## Current-code findings

### Existing reusable capability

- `FileClass::Text` and `SearchScope::Text` already exist.
- `TrigramIndex` already indexes resident file content for exact candidate lookup.
- `search_text_with_options` already supplies bounded line context and enclosing
  symbol/section data.
- Markdown extraction already emits section byte/line spans.
- `ReloadData` builds off-lock and `apply_reload_data` publishes replacement state.
- `git2`, worktree routing, cross-project search, snapshots, and CCR already ship.

### Lifecycle defects found in source

1. `discover_all_files` charges every file size before admission and silently
   skips walker errors; double metadata failure becomes size zero.
2. `admit_and_parse_entries` fully reads unknown files to sniff them, drops read
   failures, releases in-flight permits before retained bytes leave outcomes, and
   drops results after circuit-breaker `break`.
3. Watcher `read_and_index` reads/hashes before hidden/size/lock/generated admission
   and publishes re-admission plus skip clearing as separate mutations.
4. Watcher event filtering ignores new/removed unknown prose; reconciliation scans
   only current Tier-1 paths.
5. Snapshots persist Tier-1 only; verification uses recognized-only discovery,
   unrestricted reads, unfenced mutations, and intermediate publication.
6. `FileClassification::for_code_path` assigns Code to every file and protocol
   search defaults hardcode Code.
7. `swap_and_publish` stores live/state/outline independently and increments
   project generation later, permitting mixed observations across accessors.

### Root binding and runtime-state defect

- `src/discovery/mod.rs::{find_project_root, resolve_workspace_root,
  is_forbidden_root}` and `src/paths.rs::is_sensitive_path` already recognize many
  unsafe automatic roots.
- `src/paths.rs::{select_runtime_data_base, ensure_runtime_symforge_dir}` already
  contains a runtime-path selection seam, but project binding and project-local
  `.symforge` creation are still treated as one successful operation by important
  startup/session/index paths.
- Protocol and daemon `index_folder` paths currently reject sensitive roots
  unconditionally, so they cannot express the user's explicit read/index authority.
- A failed `.symforge` creation can leave an LLM with a failed SymForge bootstrap
  even though the MCP could have stayed unbound, accepted a later accessible project,
  or served a deliberately requested protected root entirely in memory.
- `select_runtime_data_base` currently has a last-resort CWD-relative `.symforge`
  path when safe global-home resolution fails. That recreates the original failure
  in System32/home launches even for process-global transport/control state.

Decision: model two serial decisions. `RootResolution` authorizes the exact source;
automatic/init requests stay unbound on protected candidates, while only explicit
`index_folder(..., allow_protected_root=true)` may bind one. `StatePlacement` then
chooses project-local, private user-local per-root, or memory-only derived state.
Explicit protected mode skips `<root>/.symforge` without even probing it. Permission
failure relocates or disables persistence; it never retargets the source and never
prevents live indexing. Protected authorization is per session: another session,
reconnect, or restart receives no membership from a live slot, persisted state, or
receipt and must issue its own direct override request. Health reports the chosen
placement, session membership authority, and capability loss.

Process-global transport/replay coordination is a separate existing lane: use only
a safe private user-local base, then degrade to process-local coordination. It never
falls back to launch CWD, a rejected source candidate, or relative `.symforge` and
does not create a per-project entry while unbound. A durable `index_folder` receipt
is historical evidence, not a live postcondition: replay verifies or reconstructs
the live source/session membership or returns successful typed
`live_postcondition_unavailable` with `applied=false`.

State ownership is closed rather than inferred from a generic path:

| Owner | Consumers |
|---|---|
| canonical source root | source/Git reads, relative paths, watcher, guarded repository policy/ignore/team-artifact writes |
| `ProjectStateDir` | snapshot/temp/quarantine/reset/checkpoint, per-project replay/mutation intent, edit-safety TEE, frecency/coupling/STEL, analytics, API-key state, derived cleanup |
| `ControlStateDir` | sidecar port/PID/session and status readers, daemon discovery/control and runtime-startup coordination, hook adoption/hints, operator profile, onboarding, version registry/updater, cross-project `index_folder` replay/locks |
| process memory | live index/watcher/session memberships and explicitly non-durable fallbacks |

Every reader and writer receives its typed owner; none rebuilds state from source,
launch CWD, or a relative path. If either selected state directory is nested in an
explicitly indexed source, its canonical subtree is excluded from scout, watcher,
reconciliation, and verification. Persistence-only operations expose capability
directly: in memory-only mode `checkpoint_now` returns successful typed
`applied=false`/unavailable rather than an MCP error or stale receipt.

The existing `persist::export_artifact` deliberately writes the optional team
artifact under `.symforge/` and updates root `.gitattributes`. Standard ignore
hygiene therefore makes a new export ignored unless it is already tracked or the
user deliberately force-adds it. Decision: preserve this compatibility path as an
explicit normal-writable-project export and report exactly `already_tracked`,
`untracked_visible`, `ignored_force_add_required`, or
`git_visibility_unavailable`; do not infer shareability, do not
silently redirect a team artifact into user-local state or weaken the `.symforge/`
ignore rule. Protected/read-only/non-project-local modes refuse before either write.
Relocating or retiring the artifact needs a separate migration decision.

The user-local key is a versioned digest of the lossless canonical-root identity
under platform path-equivalence rules. This avoids leaking protected paths in the
directory name, coalesces aliases, and prevents state collision between repositories
or linked worktrees. It is placement identity only. Snapshot load separately verifies
project, repository, stable source location, source version, manifest, admitted-
content, and available Git-history fingerprints, so a replaced directory cannot
inherit state merely because its path key is unchanged. If a private writable
`ProjectStateDir` cannot be established, no snapshot/quarantine/project-state file is
attempted and the in-process index remains the query authority for that process
lifetime. Existing sidecar/session/control state still uses the independent
`ControlStateDir` decision; it is never placed in `ProjectStateDir` and may degrade
independently to process-local control.

## Document parsing research

The useful pattern is semantic elements before optional chunking, not blind token
windows. Unstructured models documents as typed elements such as titles,
narrative text, list items, and tables, and its title-aware chunking preserves
section boundaries: [document elements](https://docs.unstructured.io/open-source/concepts/document-elements),
[chunking](https://docs.unstructured.io/open-source/core-functionality/chunking).

Rust Markdown options with source positions:

- [`markdown-rs`](https://github.com/wooorm/markdown-rs) supports CommonMark,
  GFM, MDX, frontmatter, math, and mdast nodes with line/column/offset positions.
- [`comrak`](https://github.com/kivikakk/comrak) provides a mature CommonMark/GFM
  AST with source positions.
- [`pulldown-cmark::OffsetIter`](https://docs.rs/pulldown-cmark/latest/pulldown_cmark/struct.OffsetIter.html)
  yields parser events paired with byte ranges.

Decision: reuse/harden the existing Markdown extractor first. Add one parser
dependency only if corpus tests prove the scanner cannot meet required structural
positions economically. Generic text needs no Markdown parser.

### Text-centric coverage matrix

| Repository knowledge family | Examples | V1 treatment |
|---|---|---|
| Narrative/docs | Markdown/MDX, reStructuredText, AsciiDoc, Org, plain text, extensionless README/CHANGELOG/LICENSE/NOTICE | Knowledge; Markdown gets sections, others exact lines |
| Agent/project instructions | AGENTS/CLAUDE files, CONTRIBUTING, SECURITY, governance, ADR/RFC/design/plan/handoff/runbook/postmortem files | Knowledge, including declared hidden instruction trees |
| Schema/contracts | OpenAPI/AsyncAPI, JSON Schema, GraphQL, protobuf/IDL, SQL schema/migrations | Code + knowledge where existing parser/classifier supports it |
| Configuration/policy | TOML, YAML, JSON/JSON5, XML, INI/properties, dotenv templates, Docker/Compose, CI workflows, `.gitignore`, `.gitattributes` | Code + knowledge when safe text; secret policy runs first |
| Executable examples | HTTP request files, Gherkin feature files, shell/PowerShell snippets, build files | Code or both according to existing language support |
| Generated/noisy text | Lockfiles, logs, minified/bundled output, notebook output, large CSV/datasets | Catalog-only by deterministic policy unless a failing corpus proves value |
| Unsupported containers | PDF/office/RTF, archives, databases, model weights, binary notebooks/media | Catalog-only; no conversion, expansion, or parser invocation |

UTF-8 and UTF-8 BOM text are the v1 content contract. Other encodings and invalid
UTF-8 remain cataloged with a typed unsupported-encoding reason; no lossy decode or
re-encoding is allowed. Format names guide targeting but never bypass byte/secret/
artifact admission.

## Search backend research

[`SQLite FTS5`](https://www.sqlite.org/fts5.html) offers BM25, phrase/prefix/NEAR,
weighted columns, snippets, and transactional updates. The repo already depends on
bundled `rusqlite`. [`Tantivy`](https://github.com/quickwit-oss/tantivy) provides a
Rust-native Lucene-like engine and in-memory directories.

Both are unnecessary for v1 because exact content candidate lookup, line matching,
context rendering, and section provenance already exist. A second search store
would introduce synchronization and publication semantics that the feature is
specifically trying to make singular. Measure relevance first; add BM25 only if
exact phrase/significant-term retrieval misses important corpus questions.

Embeddings/vector search are deferred because they add model/runtime selection,
chunk identity, vector persistence, stale-vector recovery, memory/disk cost, and
nondeterministic ranking before lexical/structural relevance has been measured.

## Knowledge authority and documentation-drift research

Established documentation systems separate kind/lifecycle from implementation
truth. [PEP 1](https://peps.python.org/pep-0001/) keeps rejected, withdrawn,
deferred, and superseded proposals as history while directing current behavior to
separate documentation. [MADR](https://adr.github.io/madr/) uses explicit proposed,
accepted, rejected, deprecated, and superseded decision states. The
[Rust RFC process](https://rust-lang.github.io/rfcs/) explicitly warns that an
accepted RFC does not prove implementation or the final shipped shape.

This supports independent axes rather than one `stale` flag:

- lifecycle: declared proposal/accepted/deferred/superseded/archive state;
- authority domain: current implementation versus intent/decision/governance/
  operations/history;
- code evidence: exact checked consistency, broken anchor, deterministic conflict,
  changed-since-verification, review due, unresolved, or not applicable;
- retrieval voice: current, intent, needs-review/unknown, history-only, suppressed.

Here “authority” is not a universal code-over-doc hierarchy. Code evidence has
precedence only for a deterministically checked claim about current implementation
behavior. Intent, ADR, governance, security policy, operations, and north-star
evidence retain their own domains; implementation disagreement is a gap, not proof
that those documents are stale.

Cloudflare's documentation review guidance records an explicit reviewed date but
[rejects inactivity as proof of irrelevance](https://developers.cloudflare.com/style-guide/how-we-docs/reviews/).
Its [frontmatter guidance](https://developers.cloudflare.com/style-guide/frontmatter/custom-properties/)
also distinguishes historically accurate but no-longer-recommended pages. Decision:
filesystem creation/birth and modification times, Git first-seen/last-touch, and
code changes after a document are useful review signals with provenance/coverage;
none may prove contradiction, archive a unit, or make it deletion-eligible alone.
Commit topology is preferred to timestamp ordering when available, while shallow
history, renames, copies, rebases, dirty worktrees, and clock skew remain explicit
uncertainty.

[DOCER](https://arxiv.org/abs/2307.04291) demonstrates useful two-revision detection
of deleted/renamed code references and documents false positives where removed
identifiers remain valid concepts or history. Decision: replace its regex/GitHub-
specific extraction with SymForge's exact path/symbol index and use the narrow
conclusion `BrokenAnchor`; do not generalize it into semantic truth.

Deterministic v1 proof is closed-world:

- exact internal path no longer resolves;
- code-spanned exact symbol uniquely resolved before and is now missing/changed;
- a versioned structured extractor proves a signature, schema field, CLI/MCP
  option, or config-key mismatch;
- explicit lifecycle/supersession metadata or exact content/unit duplication.

Related code churn, lexical similarity, age, or an LLM opinion yields only
`RelevantCodeChangedSinceDocument`, `ReviewDue`, or `SuspectedConflict`. A mismatch against
declared intent, an ADR, governance, a security invariant, or a north star is an
implementation gap—the code may be defective—not stale-doc proof. Findings are
unit-level so one broken section cannot condemn unaffected evidence.

The closest standalone project found was
[docfresh](https://github.com/os-tack/docfresh), which maps docs to source files and
records verification revisions. It is a separate young binary, treats coarse
source changes as staleness, and offers optional embedding-based suggestions.
Decision: borrow explicit mappings/fingerprints, not the dependency or conclusion.
SymForge already has stronger exact anchors, source generations, watchers,
snapshots, worktrees, and code topology.

For result shape, borrow only the useful concepts from
[SARIF 2.1](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html):
stable rule/finding IDs, exact locations, suppressions with
justification, and proposed fixes. Do not import the full schema. Branch-specific
[CODEOWNERS](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners)
supplies declared ownership evidence but never cross-source ownership inference.

Approved lifecycle decisions need to survive sessions without rewriting every
document format. Use one versioned, reviewable `.symforge-knowledge.toml` ledger
bound to exact file and optional unit hashes. Native frontmatter/MADR/RFC status is
evidence; a conflict is reported. A changed file invalidates its old policy entry.
Logical archive/supersession is sufficient to remove stale voice without breaking
links or navigation. Physical move/delete stays proposal-only in feature 020.

The coding LLM remains valuable for unsupported semantic review, explanations, and
wording proposals, but only after receiving a bounded evidence pack of exact doc
and code spans/source generations. Its judgment cannot directly change voice or
policy. A read-only `review_knowledge` tool and separate preview-first, hash-guarded
`curate_knowledge` ledger mutation preserve MCP annotations and approval boundaries.
Apply needs durable per-project idempotency, a synced pending intent containing the
exact pre/post ledger digests, and a tested platform contract for guarded temp
`write_all` + file `sync_all` + atomic replace + durable parent-directory commit,
followed by a durable completion record. Recovery accepts only the exact pre-image or
post-image; any third state conflicts. Apply is exposed only when durable replay and
the complete file-plus-parent durability contract are available; no best-effort
weakening is allowed. Preview and review remain available.

## Admission-before-read research

The robust staged contract is:

```text
Discovered -> metadata admission -> bounded probe -> content admission -> full read
```

Rust’s [`ignore::WalkBuilder`](https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html)
supports ignore-aware traversal and pre-descent filtering. SymForge must retain
explicit diagnostics for walk failures and canonicalize final ordering itself.

Generated/vendor classification should prefer repository-declared evidence where
available. GitHub Linguist documents `.gitattributes` overrides including
`linguist-generated`, `linguist-vendored`, `linguist-documentation`, and language
overrides: [Linguist overrides](https://github.com/github-linguist/linguist/blob/main/docs/overrides.md).

Decision: metadata terminal decisions are hard safety gates. Generated/vendor are
soft targeting/noise signals and cannot make a tracked path disappear. Full
`.gitattributes` override parity may land after the core manifest only if required
by a failing fixture; the manifest records which deterministic rule won.

## Deterministic manifest research

Bazel Remote Execution canonicalizes directory nodes in lexicographic order and
uses digest plus size for content identity. This demonstrates the relevant
principle: logical content identity excludes observation-time metadata and
requires canonical ordering. Primary contract:
[remote_execution.proto](https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto).

Decision:

- exact normalized safe UTF-8 path plus opaque stable ID; non-UTF-8/unsafe names
  never pass through lossy conversion or content targets;
- deterministic `(case-folded path, exact original bytes)` ordering; collisions
  remain distinct and are isolated per entry only when a host cannot address one;
- sorted vector serialization;
- schema/policy versions in digest;
- scan times/mtimes/platform IDs outside logical digest;
- optional content/object digest, never a fabricated hash for unread artifacts.

## Stable-read identity research

Size and mtime are change hints, not identity. OCI descriptors separately require
media type, digest, and size and verify content against them:
[OCI descriptor](https://github.com/opencontainers/image-spec/blob/main/descriptor.md).

Decision: admitted filesystem bytes are hashed during a bounded read, then a
second independently opened bounded pass streams and compares length/hash while
pre/post handle/path metadata remains stable. A bounded retry may recover normal
editor writes; persistent instability becomes an explicit terminal state. This
cost is accepted for source correctness; no portable scheme can prove stability
against a hostile writer, so watcher/reconciliation still repair later changes.

## Giant artifacts and Git LFS

Git LFS represents large content as a small UTF-8 pointer containing a version,
OID, and size. The official pointer contract is
[git-lfs/docs/spec.md](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md).

Decision:

- giant artifact: catalog path/type/size/reason, optional declared digest;
- never full-read/hash/decompress/deserialize solely for cataloging;
- recognize a bounded LFS pointer probe and retain declared metadata;
- classify recognized pointers as catalog-only (`LfsPointer`) so pointer syntax
  cannot pollute knowledge search;
- Git-object traversal uses `git2` directly and never invokes checkout/smudge or
  network fetch.

## Watcher reconciliation research

Filesystem notifications are lossy hints. Rust `notify` exposes
[`Event::need_rescan`](https://docs.rs/notify/latest/notify/struct.Event.html#method.need_rescan)
because any watched file may have changed after event loss. Watchman performs a
complete recrawl after overflow and exposes fresh-instance semantics:
[recrawl](https://facebook.github.io/watchman/docs/troubleshooting#recrawl),
[query](https://facebook.github.io/watchman/docs/cmd/query).

Decision: per-event fast path uses the same scout; complete manifest diff is the
authority after overflow/restart/topology/policy uncertainty and periodically.

## Worktree/ref strategy

- Current and linked checked-out worktrees are independent filesystem sources.
- Local refs are immutable Git tree/blob sources until the ref moves.
- Inspect blob size before loading content.
- Deduplicate raw bytes by object ID; parse/extraction reuse also requires the same
  classification/route/extractor version, while source-derived state is rebuilt.
- Current working tree ranks first; divergent variants remain visible.
- No remote fetch, reflog/stash, remote-only refs, or submodule traversal in v1.
- Each source owns one immutable manifest/generation. A single atomically published
  source-set registry maps source IDs to captured bundles; all-source queries report
  generation/digest/coverage per source and worst overall coverage.
- Local refs are a P1 lane with independent entry/blob/memory limits and cannot
  block P0 current-worktree readiness.
- Bridge, policy, lifecycle, and code-drift evidence resolve inside one source only.
  A current-worktree curation may not write policy into another worktree/ref.

## Sensitive-content strategy

Existing `src/paths.rs` protects system/profile roots, not repository credential
files. Add a separate normalized repository-relative path policy. Definite
credential containers become `MetadataOnly(SensitivePath)` before content access;
template suffixes are not automatically trusted and continue to content scanning.
Safe path/location metadata remains countable. If a path string itself detects
positive, external output uses an opaque catalog ID.

The fixed v1 path categories are non-template environment files, conventional
private-key filenames/extensions, exact Git/network credential stores, credential
material beneath conventional SSH/cloud/GPG/Kubernetes paths, and infrastructure
state files known to serialize sensitive values. A path is never denied merely
because its prose name contains words such as “secret”, “token”, or “password”.

Use the existing Rust `regex` dependency and `regex::bytes` offsets; do not embed a
full secret-scanner product. Rust regex has bounded worst-case behavior on
untrusted input, while size admission still bounds total work:
[regex security/performance](https://docs.rs/regex/latest/regex/#untrusted-input).
High-precision v1 rules combine keyword prefilters, context-anchored byte regexes,
captured-value length, and optional entropy on that capture only. This borrows the
useful deterministic rule shape from
[Gitleaks configuration](https://github.com/gitleaks/gitleaks#configuration)
without adding baselines that can authorize emission.

Rule families are private-key envelopes, well-defined provider formats,
authorization credential headers, context-anchored sensitive-name assignments,
and embedded URI credentials. Placeholder recognition may suppress only the weak
generic-assignment rule; it cannot override structural/provider rules. There is no
runtime query knob, allowlist, or project baseline that converts a finding into
emitted evidence.

Scan stable bytes before publication. Positive or indeterminate results discard
the transient bytes/hash and become `MetadataOnly(SensitiveContent)` for both code
and knowledge. Persist only safe rule IDs/counts. Then apply defense in depth to
the raw query and every externally visible path/source/heading/excerpt/diagnostic
field. A positive result withholds the whole hit; SymForge never edits a value
inside an excerpt advertised as exact.

The only valid pipeline is `extract -> detect -> SafeHit -> format -> budget ->
CCR`. CCR stores already-safe output tagged with policy version, and a mismatch
refuses/revalidates. Snapshot policy mismatch forces re-scout before Ready.
Provider verification, as used by dedicated scanners such as
[TruffleHog](https://github.com/trufflesecurity/trufflehog), is deliberately
rejected because it is network-dependent and time-varying.

The testable closed-world guarantee is: for policy version V, no byte range
identified by V may reach published content, output, analytics, CCR, diagnostics,
logs, or snapshots. Unknown formats remain detector defects; claiming detection of
all possible secrets would be dishonest unless SymForge emitted no excerpts.

## Alternatives rejected

| Alternative | Reason rejected for v1 |
|---|---|
| Separate knowledge MCP/product | Adds tool/server coordination and duplicate lifecycle state. |
| SQLite FTS5 now | Existing trigram/line search covers the first contract; adds publication sync. |
| Tantivy now | New dependency/store and rebuild/recovery path without measured need. |
| Embeddings/vector DB | High complexity/nondeterminism before lexical relevance evidence. |
| Fixed token chunks | Breaks natural structure and provenance; unnecessary for exact search. |
| Parse every format deeply | Generic text plus existing config parsers delivers value with less risk. |
| Trust watcher events | Cannot recover missed create/delete/unsupported paths. |
| Hash every artifact | Multi-GB artifacts would reintroduce the resource failure being fixed. |
| Treat mtime/size as identity | Same-size/preserved-time rewrites can be stale. |
| Publish partial components | Readers can observe mixed generations. |
| Embed Nosey Parker/Kingfisher/Gitleaks/TruffleHog | Full scanner/rule-store or network-verification surface is unnecessary for bounded deterministic v1. |
| One stale/authority score | Conflates lifecycle, intent, checked code evidence, age, and retrieval voice. |
| Archive from age/churn | Old can remain correct; later code changes only invalidate verification. |
| Let an LLM suppress/delete | Model judgment is useful advice but not deterministic repository state or approval. |
| Physical cleanup in feature 020 | Logical policy removes current voice safely; move/delete adds link, rollback, and crash-recovery scope. |

## Open measurements, not design blockers

- cold-start scout/hash/parse duration and peak RSS on small/medium/large repos;
- relevance of significant-term ranking on the real corpus;
- whether CommonMark AST replacement materially improves retrieval;
- memory cost of local-ref blob mappings;
- value of doc-comment knowledge bridge.
- precision/recall of versioned structured drift rules and lifecycle conventions;
- whether unit-policy ranges need an explicit stable section ID after real edits.

These measurements may tune bounded constants or justify a later search backend;
they do not weaken v1 invariants.
