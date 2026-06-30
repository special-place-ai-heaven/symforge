# Research: CBM Capability Ports

**Feature**: 015 · **Date**: 2026-06-29

## R0 — Program framing

**Decision**: Port CBM capabilities as LiveIndex-derived projections, not SQLite graph clone.

**Rationale**: Constitution I and 007 FR-015 forbid second authoritative index. CBM's
SQLite is query authority; SymForge's moat is byte-exact LiveIndex + edits.

**Alternatives considered**:
- Embed CBM binary as sidecar — rejected (two truths, Windows process overhead).
- Full SQLite graph mirror in `.symforge/` — rejected (Soul Map).

## R1 — CBM reference architecture (verified)

**Source tree**: `E:/project/codebase-memory-mcp/src/`

| Module | Role |
|--------|------|
| `mcp/mcp.c` | 14 tools, pagination contracts |
| `pipeline/pipeline.c` | RAM graph buffer → SQLite dump |
| `store/store.c` | BFS, FTS5, vector search, Leiden |
| `internal/cbm/lsp/*.c` | Hybrid LSP per language |
| `semantic/semantic.c` | 11-signal index-time semantic |
| `pipeline/artifact.c` | zstd team artifact |

**CBM moat**: graph-native query, Hybrid LSP, bundled semantic, team artifacts.
**SymForge moat**: edits, STEL, recovery, resources/prompts.

## R2 — Graph projection design

**Decision**: `GraphProjection` built from `ReferenceRecord` + `ResolvedCall` at index
load and after incremental updates; stored in-memory only.

**Rationale**: Matches Principle I; rebuild cost amortized on snapshot load (same as
trigram index today in `persist.rs`).

**Alternatives**:
- Persist adjacency in snapshot v5 — deferred; rebuild from references is O(edges)
  and simpler for v1.

## R3 — detect_impact vs existing tools

**Decision**: New `detect_impact` tool + STEL impact intent upgrade; keep
`what_changed` and `analyze_file_impact` unchanged.

**Rationale**: CBM merges git sources + symbol blast + risk in one call; chaining
existing tools costs agent round-trips.

**CBM reference**: `mcp.c` `handle_detect_changes` — merges diff + status porcelain
(#520 untracked fix).

## R4 — Team artifact

**Decision**: zstd compress `index.bin`; two tiers (fast watcher / best checkpoint);
`.gitattributes merge=ours` on first export.

**CBM reference**: `pipeline/artifact.c` — VACUUM INTO + zstd -3/-9.

**SymForge delta**: Postcard not SQLite; strip nonessential rebuildable fields in
"best" tier if size critical (document in contract).

## R5 — Hybrid LSP port strategy

**Decision**: Rust-first in `parsing/resolver/`; reverse-engineer CBM `rust_lsp.c`
algorithm structure (use/import/type eval/method dispatch); no FFI to CBM.

**Rationale**: symforge dogfood is Rust; CBM proves in-process resolver works without
LSP subprocess.

**Milestone order**: Rust → TypeScript → Python → Go (matches SymForge language priority).

## R6 — Semantic without embeddings (v1)

**Decision**: Port CBM algorithmic signals (TF-IDF, MinHash on signatures, module
proximity) before Nomic int8 vectors.

**Rationale**: AGENTS.md "start simple"; CBM uses 11-signal edges without query-time
LLM; embeddings optional in S4+ extension.

## R7 — Cypher subset scope

**Decision**: v1 supports MATCH (single pattern), WHERE (comparisons, NOT EXISTS
single-hop), RETURN, LIMIT, count aggregate.

**CBM reference**: `src/cypher/cypher.c` — fail-closed on unsupported.

**Ponytail ceiling**: No variable-length paths `[*1..3]` in v1; add in 8.11.x patch if
needed.

## R8 — Hook augment

**Decision**: Extend existing `src/cli/hook.rs` sidecar path; match CBM
`hook_augment.c` behavior (Grep/Glob only, exit 0 always).

**SymForge already has**: sidecar HTTP hook infra; CBM has broader 11-agent installer —
defer installer expansion to S6 docs only.

## R9 — Spike falsifiers (S0 gate)

| Spike | Falsifier |
|-------|-----------|
| Graph BFS | p95 >200ms depth-5 on symforge repo |
| Artifact | Import corrupts byte-exact content hash |
| Rust resolver | <60% on benchmark set after 2 weeks |

## R10 — Dependencies on 012

Cross-project graph queries defer until `WorkingSet` Phase 3 routing lands; S1–S2 tools
are single-project scoped with `project_root` in envelope.

## R11 — zstd dependency

**Decision**: Check `Cargo.toml` for existing zstd; if absent, add `zstd` crate (pure
Rust safe) — one dependency justified by team artifact (CBM uses zstd 1.5.7).

**Ponytail**: If dependency rejected, use gzip in v1 with documented ratio tradeoff.

> **Correction (S0 spike, 2026-06-30)**: `zstd` 0.13 is **C-backed** (`zstd-sys`,
> built with `cc`), not "pure Rust" as written above. This adds **no new** CI risk —
> a C toolchain is already required (`libsqlite3-sys` bundled SQLite, `libgit2-sys`
> vendored libgit2, tree-sitter C grammars). The genuine pure-Rust fallback is
> `flate2` (miniz_oxide backend), not `zstd`. See § Spike Results SP-0B.

---

## Spike Results (S0 gate — 2026-06-30)

**Decision: GO.** All three falsifiers cleared their S0 bars. A spike agent
produced the code/numbers; **three independent adversarial agents then verified**
(one re-ran the ignored tests and audited for measurement gaming; two skeptic
reviewers pressure-tested the metrics and methodology). Reproduced numbers
matched — **no gaming found**. The S0 bars prove *feasibility*, NOT the per-sprint
production targets; caveats below are mandatory.

**Method**: spike code behind `#[ignore]` tests over the symforge index
(~600 files / 19,817 symbol nodes), **debug** build. Verification re-ran
`cargo test --test cbm_spike_* -- --ignored --test-threads=1` (exit 0; full
suite 103 test binaries green, 0 failed).

| Spike | S0 bar | Result | Verdict |
|-------|--------|--------|---------|
| SP-0A graph BFS | p95 < 200ms depth-5 | p95 ≈ 46–48ms (4× margin) | **GO** |
| SP-0B artifact | every `content_hash` byte-exact | 607/607, 3.61× ratio | **GO** |
| SP-0C resolver | ≥ 60% | **73% strict** (11/15); clears every framing | **GO** (feasibility) |

### SP-0A — graph BFS — GO at symforge scale
p95 ≈ 46–48ms for inbound BFS depth-5 over 19,817 nodes / 127,540 name-based Call
edges (debug; 1000 samples seeded from the highest in-degree symbols = worst-case).
Verified: depth genuinely reaches 5; visited-set dedup bounds any query to O(V+E);
no early-cap or short-circuit. Conservative on three axes — name-based edge
over-approximation (real S2 resolver narrows), debug build, un-interned `String`
node keys — all of which the real graph improves on.
**Caveats**: (a) per-query cost is O(V+E), linear in graph size → the 4× margin is
**not** established for repos materially larger than symforge; (b) module-level
callers with no enclosing symbol are dropped → under-counts file-scope edges in
Python/JS; (c) does not measure S2's confidence-weighted edge filtering or
generation-fence overhead.

### SP-0B — zstd artifact round-trip — GO
607/607 per-file `content_hash` byte-exact through postcard→zstd→decompress→postcard
(real `build_snapshot` path; `content_hash` is a genuine per-file SHA digest,
`src/parsing/mod.rs:51`), ratio 3.61×. Corrupt/truncated frames return `Err` with
no partial state (pure function — partial state is structurally impossible).
**Caveats**: (a) verifies `content_hash` survival + full-snapshot decodability, not
field-by-field fidelity of every snapshot field — C-S1A-005 should assert
`postcard::to_stdvec(&after) == raw` (deterministic re-encode ⇒ whole-snapshot
byte-exact); (b) `zstd` is C-backed (R11 correction above); (c) the gzip fallback is
not wired.

### SP-0C — Rust resolver — GO (S0 feasibility only; NOT the S3 target)
Same-file + in-file `use` resolver on the 16-case `cbm_resolver_rust` fixture.
**Publish the strict number: true-callee recall 11/15 = 73%**; precision over
claimed resolutions 11/13 = 85%; absolute floor 11/16 = 69%. The "verdict accuracy"
14/16 = 87.5% headline credits 3 "correct declines" (2 are out-of-scope calls with
real-but-unreachable targets), and in-scope recall 11/11 = 100% is a curation
artifact — **neither is a real-repo predictor**. The S0 ≥60% floor clears under
every framing; the **S3 80% real-repo target is NOT demonstrated**.
**Keystone risk**: bare method-call names resolve against all same-file definitions
with no receiver type (`src/parsing/resolver/rust.rs:101-104`) → **false-positive
call edges** (worse than missing edges for a graph). Both fixture misses are this
one class (`Bag::len` over-resolving `HashSet::len` + slice `len`). The fixture
under-samples it (one collision); real Rust is saturated with name collisions
(`len`/`new`/`get`/`push`/…) and adds method chains, trait dispatch,
generics/turbofish, glob imports, multi-impl scope — all absent.
**S3 requirement**: add receiver-type gating before any bare-method resolution and
re-benchmark on a real-repo sample.

### Spike-code disposition (open — decide at S1a kickoff)
Per spike-spec §Rollback, spike code should be `#[cfg(test)]`/ignored-test-only.
Currently `graph.rs` / `parsing/resolver/` / `persist.rs` `spike_*` helpers are
`pub` and exercised only by ignored tests (dormant), and `zstd` is a non-optional
dependency. Options: **(1)** keep as the S1a/S2 foundation — C-S1A-002 wires
`graph.rs`, C-S1A-005 replaces the artifact helpers — accepting ~1 sprint of
transitional dormancy; **(2)** `#[cfg(test)]`-gate the helpers and move `zstd` to a
dev-dependency until C-S1A-005 promotes it to a real runtime dep. Per the
no-dormant-code policy, (2) is the conservative default unless S1a starts
immediately. **Decision pending operator direction.**

### Gate decision
**GO to Sprint 1a `[C]`.** (S1a planning gate already signed 2026-06-30; coding was
blocked only on this S0 GO.)
