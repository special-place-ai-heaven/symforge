# Dart Parser Investigation — Replacement Options for `tree-sitter-dart-orchard`

Status: research complete, 2026-06-11  
Audience: SymForge maintainers and external research agents  
Scope: Dart only. SymForge architecture (tree-sitter Tier-0 for other languages) is not under review.

## Executive summary

**Conclusion:** `tree-sitter-dart-orchard` remains the best practical choice for SymForge's Tier-0 syntactic index. No drop-in replacement parser beats it on speed, Rust integration, and architectural uniformity.

The only path that clearly surpasses orchard on **semantic correctness** is the official Dart toolchain (Analysis Server, `analyzer`, `scip-dart`). Everything else is another tree-sitter fork, an immature Rust reimplementation, or a spec-only grammar with no semantic layer.

The correct long-term architecture is **hybrid**, not swap:

| Depth | Backend | Role |
|-------|---------|------|
| D0/D1 | `tree-sitter-dart-orchard` | Always-on symbols, outlines, heuristic xrefs |
| D3 | `scip-dart` batch indexer | Stable cross-file symbol IDs, semantic refs |
| D2 | `dart language-server --lsp` | Interactive definition/references when D1 confidence is low |

This aligns with `docs/semantic-tier-roadmap.md`.

---

## SymForge context

### Current Dart integration

| Layer | Implementation | Precision |
|-------|----------------|-----------|
| Grammar | `tree-sitter-dart-orchard = "0.3.2"` in `Cargo.toml` | — |
| Language selection | `src/parsing/mod.rs` | — |
| Symbol extraction | `src/parsing/languages/dart.rs` | Syntactic (tree walk) |
| Cross-references | `src/parsing/xref.rs` (`DART_XREF_QUERY`) | Heuristic, name-based |
| ast-grep | `src/parsing/ast_grep.rs` | Syntactic |

SymForge uses Dart at **Depth 0 only** today. Cross-references are produced by tree-sitter queries — identifier matches, not resolved symbols. Overloaded or shadowed names over-match by design. This is the precision ceiling of Tier-0/Tier-1.

### Why orchard was chosen

- crates.io `tree-sitter-dart` 0.0.4 is frozen and lacks Dart 3 support.
- Newer crate lineages introduced incompatible node kinds and parser panics.
- `tree-sitter-dart-orchard` preserves extractor compatibility, parses Dart 3 cleanly, and is actively maintained through 2026.

### Known limitations (in-repo)

Tests in `src/parsing/languages/dart.rs` document that the orchard grammar may model concrete methods as `function_signature` and name them after the return type (`int` instead of `add`). This is a grammar/extractor quirk, not a SymForge bug. Dart 3 syntax (sealed classes, records, switch expressions) is validated and passes.

---

## Evaluation criteria

Candidates were ranked against:

1. Correctness
2. Support for modern Dart 3 syntax
3. Maintenance activity
4. Future-proofing
5. Bus factor
6. Ease of integration with Rust
7. Incremental updates
8. Startup latency
9. Memory usage
10. Ability to extract symbols and references
11. Long-term sustainability

---

## Candidate 1: Dart Analysis Server

### Architecture

Long-lived Dart process started via `dart language-server --lsp` (or legacy JSON-RPC over stdio). Built on the official `analyzer` package. Maintains analysis contexts, pub resolution, `.dart_tool`, SDK paths, and incremental file state.

Exposes LSP methods including:

- `textDocument/documentSymbol`
- `textDocument/definition`
- `textDocument/references`
- `textDocument/hover`
- `workspace/symbol`

Legacy Analysis Server Protocol is documented at `pkg/analysis_server/doc/api.html` in the Dart SDK. LSP support is documented at `pkg/analysis_server/tool/lsp_spec/README.md`.

### Maintenance status

**Excellent.** Part of `dart-lang/sdk`, shipped with every Dart/Flutter SDK, actively developed through 2026.

### Strengths

- Authoritative semantics: resolved definitions, imports, types, inheritance, overrides
- Incremental updates after warm-up
- Headless operation is supported and production-proven (VS Code, IntelliJ, Android Studio)
- Answers questions tree-sitter cannot: inferred types, export chains, mixins, extension methods

### Weaknesses

- **Not a drop-in tree-sitter replacement.** Does not expose a stable CST for SymForge's existing extractor/query pipeline. Provides resolved elements via protocol, not `function_signature` nodes.
- **Cold start is expensive.** SDK issues report 2–12s+ LSP init on Flutter projects with large `.dart_tool/build` trees; multi-project setups have seen ~3 minute startup from redundant analysis-options I/O (dart-lang/sdk#62539).
- **Memory:** Historically 5–20GB on large monorepos (many leaks fixed, but 2–5GB+ on big Flutter repos remains realistic; dart-lang/sdk#52447).
- **Requires project context:** `pub get`, valid `pubspec.yaml`, SDK on PATH, analysis options. Broken or incomplete projects degrade or fail.
- **Batch indexing is a poor fit:** Designed for incremental IDE analysis, not cold parallel indexing of thousands of files.
- **Multi-repo:** One server ≈ one analysis root/workspace. Many unrelated Dart repos means multiple heavy processes or complex workspace orchestration.
- **Rust integration:** Subprocess + JSON-RPC/LSP client only. No in-process embed.

### Performance (reported, not SymForge-measured)

| Metric | Order of magnitude |
|--------|-------------------|
| Cold start (medium Flutter app) | 2–12s+ |
| Cold start (complex monorepo) | Up to ~3 min (SDK #62539) |
| Warm per-query latency | ~10–100ms typical for LSP |
| Memory (large repo) | ~2–5GB typical; historical outliers 19GB+ |
| Batch full-repo index | Minutes; not competitive with tree-sitter ms/file |

### Integration complexity

**High** — process lifecycle, SDK discovery, pub workspace detection, LSP client, Windows orphan-process cleanup, graceful D1 fallback.

### Migration difficulty

**High** if replacing tree-sitter entirely (rewrite symbol/xref pipeline). **Moderate** as D2 adapter per existing roadmap.

### Key questions answered

| Question | Answer |
|----------|--------|
| Can it replace tree-sitter entirely? | Technically for symbols+refs yes; architecturally no for SymForge's polyglot fast-index model |
| Can it provide a syntax tree? | Not as a consumable CST API; internally yes, externally via LSP outlines only |
| Can it expose semantic symbols? | Yes |
| Can it operate headlessly? | Yes (`dart language-server --lsp`) |
| RAM/CPU costs? | GB RAM, second-scale cold start, moderate warm CPU |
| Failure modes? | Missing SDK, broken pub, huge build dirs, memory pressure, version skew |
| Multiple repositories simultaneously? | Awkward; one heavy process per workspace typically |

---

## Candidate 2: `analyzer` package

### Architecture

Dart library (`pub.dev/packages/analyzer`) used by Analysis Server, `dart analyze`, `dart format`, and `scip-dart`. Parses Dart, resolves elements, produces diagnostics.

Official documentation states: *"Integrators that want to add Dart support to their editor should use the Dart Analysis Server."* Direct embedding is supported for Dart tools, not recommended for IDE-like integrations.

### Maintenance status

**Excellent** — same team as Analysis Server.

### Strengths

- Same semantic engine as the IDE
- `scip-dart` proves batch symbol/reference extraction is feasible
- Documented embedder API (analysis options, feature sets, element model)

### Weaknesses

- **Dart-only.** A Rust application cannot embed it efficiently; only subprocess or a sidecar Dart helper.
- Running `analyzer` per-file without full analysis context loses cross-file resolution.
- No existing JSON protocol designed for one-shot Rust batch queries beyond what Analysis Server or scip-dart already wrap.

### Would this produce superior results to tree-sitter?

**Yes for semantics. No for SymForge Tier-0 goals** (speed, uniformity, offline fault tolerance, no SDK dependency).

### Integration complexity

Medium–high via Dart sidecar; impractical as pure Rust.

### Migration difficulty

High as Tier-0 replacement; moderate as batch semantic indexer (see scip-dart).

---

## Candidate 3: Existing Rust crates

| Crate | What it is | Maturity | Verdict |
|-------|------------|----------|---------|
| **`tree-sitter-dart-orchard` 0.3.2** | Best-maintained TS Dart 3 grammar | Production-ready for SymForge | **Current choice** |
| **`tree-sitter-dart` 0.2.0** (nielsenko) | Another fork lineage | 5 GitHub stars; unknown Dart 3.11 coverage | Not clearly superior |
| **`arborium-dart` 2.18.0** | Tree-sitter binding wrapper | Same underlying fork problem | No advantage |
| **`oak-dart` 0.0.11** | Native Rust incremental parser (Oak/Roslyn-style) | ~400 total downloads, 0.0.x | Promising architecture, **not production-ready** |
| **`flutter_rust_bridge`, `dart-sys`** | Dart↔Rust FFI | N/A for parsing | Wrong tool |
| **Rust analyzer bindings** | — | Do not exist | — |

**Conclusion:** No mature Rust-native Dart semantic analyzer exists. The only credible Rust-in-process parsers are tree-sitter forks. `oak-dart` is the one non-tree-sitter experiment worth watching, but it is far behind orchard on adoption and battle-testing.

---

## Candidate 4: ANTLR grammars

### Architecture

Official `Dart.g4` lives in the Dart SDK under `tools/spec_parser/dart_spec_parser/` — a **spec-validation tool**, not a production parser for tooling at scale. Community grammars (e.g. grammars-v4) are often stale.

Rust generation is possible via community projects (`antlr4rust`, `antlr-rust-runtime`) but is not an official or maintained integration path.

### Maintenance status

Spec grammar updated with Dart releases (seen through 3.13 dev tags). Rust target tooling is community-maintained.

### Strengths

- Spec-aligned grammar
- Can parse modern Dart when the spec grammar is current

### Weaknesses

- Non-incremental; full reparse per change
- Slower than tree-sitter for indexing workloads
- **Zero semantic analysis** — syntax only
- No SymForge integration path; extractors would be built from scratch

### Performance

No production benchmarks relevant to SymForge. ANTLR is generally slower and heavier than tree-sitter for high-volume file indexing.

### Integration complexity

**High** — generate Rust bindings, build extractors, maintain grammar sync with Dart releases, no semantic layer.

### Migration difficulty

**Very high** for no semantic gain over orchard.

### Verdict

Worse than `tree-sitter-dart-orchard` on every SymForge criterion except "tracks the written language spec."

---

## Candidate 5: `tree-sitter-dart-orchard` git HEAD vs crates.io 0.3.2

### What 0.3.2 provides (Nov 2025)

Dart 3 sealed classes, records, switch expressions — validated by SymForge tests in `src/parsing/languages/dart.rs`.

### What git HEAD adds (May 2026, unreleased on crates.io)

Recent commits on Codeberg include Dart 3.11 features:

- Dot-shorthands
- Empty records
- Abstract/external fields
- Null-aware map keys
- Annotation parsing fixes (`@annotation (R, T) method() {...}`)

Grammar Orchard org was updated 2026-05-06. crates.io 0.3.2 lags git HEAD by approximately six months.

### Stability

- Active CI on Codeberg
- 3 org members with governance model for onboarding maintainers
- Low visibility vs GitHub, but not abandoned
- PyPI downloads ~28k/month suggest real downstream use

### tree-sitter 0.26.x compatibility

Grammar Orchard standardizes on ABI 14 (compatible with tree-sitter 0.20–0.26). SymForge pins `tree-sitter = "=0.26.9"` — compatible.

### Recommendation

Pinning git HEAD (or bumping when 0.3.3+ ships) is the **lowest-risk improvement** to the current approach. It addresses Dart 3.11+ without architectural change. Inherited extractor quirks (method naming) remain and need separate fixes.

### Integration complexity

**Low** — Cargo git pin or version bump; add Dart 3.11 fixtures to `tests/tree_sitter_grammars.rs`.

### Migration difficulty

**Low.**

---

## Candidate 6: Dart formatter and compiler internals

| Component | Exposes AST/semantics externally? | Usable from Rust? |
|-----------|-----------------------------------|-------------------|
| **`dart format` / `dart_style`** | Parses via `analyzer` internally; API returns formatted text only | Subprocess only; no AST export |
| **`front_end` / CFE / Kernel** | Produces Kernel IR (`.dill`); accessible via Dart `package:kernel` | Dart-only; needs full compilation pipeline |
| **VM embedding** | Runs compiled kernel, not source intelligence | C API for execution, not indexing |

`dart format` uses `parseString` from the analyzer internally (`dart_style/lib/src/dart_formatter.dart`) but does not expose the AST to callers.

Kernel/CFE is the compiler pipeline's intermediate representation. It is not designed as a code-intelligence API for external tools.

### Verdict

Implementation details of the Dart toolchain, not embeddable intelligence APIs for a Rust MCP server. `scip-dart` is the intended external batch interface for semantic indexing.

---

## Candidate 7: Alternative parsers (Lezer, PEG, pest, nom, custom)

| Approach | Status |
|----------|--------|
| **Lezer** | No maintained Dart grammar |
| **pest / nom / PEG** | No production Dart 3 parser; from-scratch multi-year effort |
| **Custom generated parsers** | No mature ecosystem for Dart 3 |
| **`oak-dart`** | Closest greenfield Rust option; pre-1.0, negligible adoption |

None outperform orchard on correctness + maintenance + feature coverage **today**.

`oak-dart` (Oak framework, Roslyn-inspired green/red trees) claims sub-millisecond incremental parsing and Dart 3 feature support, but with ~400 total crate downloads and version 0.0.11 it is not a credible production replacement yet.

---

## Additional candidate: `scip-dart` (Workiva)

Worth treating as a first-class option for semantic depth, though not listed as a separate numbered candidate in the original brief.

### Architecture

Batch CLI using `analyzer ^5.13.0` → protobuf `index.scip` → ingestible in Rust via the `scip` crate.

```sh
dart pub global activate scip_dart
cd ./path/to/project/root
dart pub get
dart pub global run scip_dart ./
```

### Maintenance status

Workiva-backed; last GitHub push Feb 2026; 16 stars, 9 open issues. `analyzer` dependency (^5.13.0) is several major versions behind current SDK — version skew risk.

### Strengths

- Stable cross-file symbol IDs (SCIP format)
- Batch-oriented — fits SymForge D3 roadmap
- No interactive LSP session required for indexing
- Directly addresses semantic "find references" / "go to definition"

### Weaknesses

- Requires `dart pub get`, valid pub project
- Not incremental; full reindex on change
- Smaller community than rust-analyzer / scip-typescript
- Still a Dart subprocess, not Rust-embedded
- Known edge cases: nested subpackages, pubspec indexing quirks

### Fit

**D3 semantic cache substrate**, not Tier-0 replacement. Listed as "Unknown" for SCIP coverage in `docs/semantic-tier-roadmap.md` — this investigation confirms a production-quality emitter exists, with caveats.

---

## Comparative ranking

| Criterion | tree-sitter-dart-orchard | Dart Analysis Server | scip-dart | oak-dart | ANTLR / other |
|-----------|--------------------------|----------------------|-----------|----------|---------------|
| Correctness (syntax) | Good | Excellent | N/A (semantic) | Unknown | Good (syntax only) |
| Dart 3.x support | Good (HEAD: very good) | Excellent | Good | Claimed | Spec-good |
| Maintenance | Moderate (niche host) | Excellent | Moderate | Early | Spec-only |
| Future-proofing | Moderate | Excellent | Moderate | Low | Low |
| Bus factor | Low–moderate | Excellent | Moderate | Very low | N/A |
| Rust integration | **Excellent** (native) | Subprocess | Subprocess | Native | Poor |
| Incremental updates | **Excellent** | Excellent (when warm) | None | Claimed | None |
| Startup latency | **~ms/file** | Seconds–minutes | Minutes (batch) | ~ms (claimed) | Slow |
| Memory | **~MB** | GB | GB (during run) | ~MB (claimed) | Varies |
| Symbol/ref extraction | Heuristic | **Semantic** | **Semantic (batch)** | Syntactic only | Syntactic only |
| Long-term sustainability | Uncertain | **Strong** | Moderate | Unknown | Weak |

---

## Falsifying the current choice

| Alternative | Falsification result |
|-------------|---------------------|
| Official Analysis Server | **Superior semantics**, but fails SymForge Tier-0 requirements (speed, SDK independence, uniform architecture, batch cold index) |
| Another TS fork (`tree-sitter-dart` 0.2.0) | No evidence of better Dart 3.11 coverage or stability |
| Rust-native parser (`oak-dart`) | Too immature (~400 downloads, 0.0.11) |
| ANTLR / PEG / custom | No semantic layer; high build cost; slower |
| Direct `analyzer` embed | Not Rust-embeddable |
| `dart format` AST | AST not exported |

**Partial falsification:** git HEAD orchard **is** strictly better than crates.io 0.3.2 for Dart 3.11+. The *approach* (tree-sitter for Tier-0) survives; the *pin* should move forward.

---

## Final answers

### 1. Is `tree-sitter-dart-orchard` currently the best practical option?

**Yes, for SymForge's Tier-0 syntactic index** — fast, in-process, uniform with other languages, fault-tolerant, offline, no SDK required.

**Caveat:** Pin git HEAD (or release 0.3.3+ when published) for Dart 3.11 dot-shorthands and related syntax. Consider fixing the method-naming extractor quirk separately.

### 2. Is there an objectively superior replacement?

**No single drop-in replacement** that beats orchard on all SymForge criteria.

- **Superior for semantics:** Dart Analysis Server / `analyzer` / `scip-dart`
- **Superior for Tier-0 speed/uniformity:** orchard (nothing else is close in Rust)

The superior solution is **hybrid**, not swap.

### 3. Would Dart Analysis Server be a better long-term foundation?

**Yes — for semantic depth (D2), not as the universal parse foundation.**

Semantic gains are real:

- Resolved refs across files, exports, parts, extensions
- Correct overload/instance distinction
- Types, hierarchies, inferred receivers
- Diagnostics as confidence signals

Costs are also real:

- SDK + pub dependency
- GB RAM, second-scale cold start
- Process management per workspace
- Poor fit for "index everything fast on first open"

Dart Analysis Server belongs in **D2**. `scip-dart` belongs in **D3**.

### 4. Greenfield 2026 choice for Dart inside SymForge

```
D0/D1: tree-sitter-dart-orchard (git HEAD pin)
         └─ symbols, outlines, heuristic xrefs, ast-grep — always on

D3:     scip-dart batch indexer → SCIP ingest → semantic cache
         └─ stable symbol IDs, cross-file refs, batch after index_folder

D2:     dart language-server --lsp (lazy, idle-timeout)
         └─ interactive definition/references/hover when D1 confidence is low
```

**Why not Analysis Server-only?** SymForge's mission is fast index across ~25 languages to save tokens. Making Dart SDK-heavy analysis mandatory for every `.dart` file would violate that for a language often minority in polyglot repos — while leaving other languages on tree-sitter.

**Why not abandon orchard for oak-dart?** Oak is architecturally interesting (Roslyn-style, native Rust) but has negligible adoption vs orchard's real-world validation in SymForge, ast-grep, and PyPI (~28k/month). Switching would be a bet on an unproven parser with no semantic layer.

---

## Recommended next steps

1. **Near-term (low risk):** Pin `tree-sitter-dart-orchard` to git HEAD or bump when 0.3.3 ships; add Dart 3.11 fixtures to `tests/tree_sitter_grammars.rs`.
2. **Medium-term (high value):** Prototype `scip-dart` ingest for D3 — measure batch time and index quality vs D1 xrefs on a Flutter corpus.
3. **Medium-term (targeted):** D2 LSP adapter for `dart language-server` with lazy start, explicit SDK path config, memory ceiling, D1 fallback metadata.
4. **Do not:** Replace tree-sitter with Analysis Server as the sole Dart backend.

---

## References

- SymForge: `docs/semantic-tier-roadmap.md`, `src/parsing/languages/dart.rs`, `src/parsing/xref.rs`
- Grammar Orchard: https://codeberg.org/grammar-orchard/tree-sitter-dart-orchard
- Dart Analysis Server LSP spec: `pkg/analysis_server/tool/lsp_spec/README.md` (Dart SDK)
- Dart analyzer performance: https://dart.dev/tools/analyzer-performance
- scip-dart: https://github.com/Workiva/scip-dart
- SCIP protocol: https://scip-code.org
- oak-dart: https://crates.io/crates/oak-dart
- SDK issues: dart-lang/sdk#52447 (memory), #62539 (startup), #54513 (LSP init)
