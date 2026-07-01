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

**Resolved 2026-06-30 (S1a landed):** option (1). `graph.rs` was un-gated and
extended with `compute_impact` (C-S1A-002); the `persist.rs` spike helpers were
promoted/renamed into the real, always-on artifact export/import (C-S1A-005);
`zstd` is a normal (non-optional) dependency again. `cbm-spike` now gates only
the Rust resolver (no consumer until S3) — no S0 dormancy remains.

### Gate decision
**GO to Sprint 1a `[C]`.** (S1a planning gate already signed 2026-06-30; coding was
blocked only on this S0 GO.)

---

## S1a Implementation Results (2026-06-30)

**Shipped**: `detect_impact` (US1) + team zstd artifact (US2), C-S1A-001..007,
via two sequential implementer agents (impact chain, then artifact +
registration, sequenced to avoid both touching `tools.rs` at once) followed by
three parallel adversarial reviewers (security, contract-correctness,
independent build). The reviewers earned their keep: they surfaced **three real
defects** the implementers' own self-reports missed, all subsequently fixed and
independently re-verified by me (not just re-reported):

1. **`base_branch` had no `"main"` default** despite the frozen contract; the
   primary STEL path (`route_impact`) never supplied it, so the main upgraded
   entry point could silently return an empty blast radius. **Fixed**: the
   `detect_impact` handler now substitutes `"main"` when the caller supplies
   neither `base_branch` nor `since` (kept out of `merge_git_changed_paths`
   itself, which stays a documented uncommitted-only primitive for other
   callers). New test: `detect_impact_defaults_base_branch_to_main_when_unset`.
2. **The daemon's real bootstrap never consumed the exported artifact** —
   `ProjectInstance::load` → `LiveIndex::load` always did a full discovery
   scan, so the artifact (the whole justification for prioritizing US2 per the
   operator benchmark evidence) was inert under the default desktop topology.
   **Fixed**: new `bootstrap_project_index` tries `persist::load_snapshot`
   first (falls back to full scan on miss/corrupt), mirroring the existing
   `main.rs` stdio path; reconciles via `background_verify` when a tokio
   runtime is present. New test:
   `daemon::tests::project_instance_load_consumes_exported_team_artifact`.
3. **Silent integrity-check bypass**: a missing/unparseable `artifact.json`
   sidecar (reachable — the artifact and its sidecar are written by two
   non-atomic operations) made `import_artifact` skip `content_hash`
   verification entirely and trust the payload with no warning. **Fixed**:
   treated as an integrity failure — quarantines (`reason: "missing-sidecar"`)
   and falls back to a full build. New test:
   `test_load_snapshot_quarantines_artifact_with_missing_sidecar`.

Plus one cleanup: `tests/cbm_spike_graph_bfs.rs` had not received the same
promote-and-un-gate treatment as its artifact sibling after `graph.rs`
graduated to production — promoted to `tests/graph_bfs_calibration.rs`
(permanent `#[ignore]`d real-repo-scale perf check), matching the precedent
already set by `tests/team_artifact_calibration.rs` (promoted from
`cbm_spike_artifact.rs`) and this repo's existing `calibrate_current_repo_smoke`
/ `test_load_perf_1000_files` ignored checks.

**Known, disclosed, out-of-scope gap** (pre-existing, not introduced by S1a):
a snapshot/artifact-restored `LiveIndex` has `gitignore: None` and
`coupling_store: None` (set by the pre-existing `snapshot_to_live_index`,
unchanged by this sprint) — identical to the behavior of the already-shipped
`main.rs` local-snapshot path. `background_verify` + the watcher reconcile
file-freshness drift but do not repopulate those two fields. Neither adversarial
reviewer flagged this; recording it here as a candidate follow-up rather than
silently carrying it forward.

**Verification**: `cargo fmt`/`check`/`clippy --all-targets -- -D warnings`
green (independently re-run by me, not just self-reported). Full
`cargo test --all-targets -- --test-threads=1` could not complete as one
unbroken run in this sandbox — three separate attempts (two full-suite, one
scoped to test+release) were killed by what looks like an environment wall-time
ceiling on long single invocations, never a test failure (zero `FAILED`/panics
logged across any attempt). Coverage obtained instead, all green: the entire
2573-test lib unit suite plus dozens of integration binaries (two independent
partial runs, together spanning effectively the whole corpus), every
specifically changed/added test file run to completion
(`detect_impact`, `team_artifact`, `daemon_aliases`, `conformance`,
`impact_intent`: 29/29), and targeted lib-module reruns
(`persist::` 38/38, `daemon::` 60/60, `graph::` 6/6, plus the three
fix-confirmation tests individually). `cargo build --release` was attempted
three times and could not complete locally in this sandbox (each attempt died
mid dependency-compilation with zero errors — an LTO/`codegen-units=1` release
build of this crate, with `aws-lc-sys`/vendored libgit2/bundled sqlite/~20
tree-sitter grammars, appears to exceed the environment's ceiling for one
invocation); this repo's CI runs `cargo build --release` on the PR, which is
the authoritative environment for that specific check.
