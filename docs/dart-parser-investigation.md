# Dart Parser Investigation — Replacement Options for `tree-sitter-dart-orchard`

Status: research complete, 2026-06-11
Audience: SymForge maintainers and external research agents
Scope: Dart only. SymForge architecture (tree-sitter Tier-0 for other languages) is not under review.

Method: three independent research passes (official Dart toolchain; alternative-parser
landscape; prior internal draft) reconciled against **direct empirical measurement**:
three grammars compiled under SymForge's exact `tree-sitter = "=0.26.9"` pin on Windows
MSVC and run over 2,815 files — a 10-file Dart 3.0→3.12 feature corpus, flutter/samples
(483 files), and flutter/packages (2,322 files) — with SymForge's exact `DART_XREF_QUERY`
and extractor node kinds compiled against each grammar.

---

## Executive summary

**The only untenable option is the status quo.** The pinned crate
`tree-sitter-dart-orchard 0.3.2` (published 2025-11-17) measurably fails ~2.6% of
real-world Flutter files and 100% of Dart 3.10 dot-shorthand code. Claims that it
"parses Dart 3 cleanly" are true only for the Dart 3.0 feature subset that SymForge's
smoke test exercises.

**Tree-sitter remains the right technology.** No non-tree-sitter, Rust-embeddable,
production-credible Dart parser exists. The Dart Analysis Server is semantically
superior but categorically the wrong shape for Tier-0 batch indexing (multi-GB RAM,
10 s+ cold start, no syntax tree over the wire, mandatory SDK).

**Two grammar paths strictly dominate the pin, with identical measured correctness
(100% clean on 2,805 real files):**

| Path | Correctness (measured) | Extractor churn | Supply chain |
|---|---|---|---|
| **A. nielsenko `tree-sitter-dart` 0.2.0** (recommended) | 100% clean; all Dart 3.0–3.12 features | Node-kind rewrite (~1 day) | crates.io; co-owned by ast-grep author; spec-native |
| **B. orchard git HEAD, vendored** (conservative) | 100% clean; all Dart 3.0–3.12 features | **Zero** — drop-in | Codeberg; unreleased; requires generate+vendor pipeline |

The long-term architecture is **hybrid, not swap** (consistent with
`docs/semantic-tier-roadmap.md`):

| Depth | Backend | Role |
|---|---|---|
| D0/D1 | tree-sitter grammar (A or B above) | Always-on symbols, outlines, heuristic xrefs, ast-grep |
| D3 | `scip-dart` batch indexer | Stable cross-file symbol IDs, semantic refs (opt-in) |
| D2 | `dart language-server --protocol=lsp` | Lazy interactive definition/references when D1 confidence is low |

---

## Empirical evidence (measured 2026-06-11)

Probe: parse every file, count `ERROR` and `MISSING` nodes, compile SymForge's
`DART_XREF_QUERY`, extract symbols using SymForge's exact node-kind list, and measure
single-thread throughput. All grammars under `tree-sitter = "=0.26.9"`, MSVC.

### Real-world corpora

| Corpus | orchard 0.3.2 (pinned) | orchard HEAD (2026-05-05) | nielsenko 0.2.0 |
|---|---|---|---|
| flutter/samples (483 files) | **13 failing (2.69%)**, 124 bad nodes | 0 failing | 0 failing |
| flutter/packages (2,322 files) | **61 failing (2.63%)**, 246 bad nodes | 0 failing | 0 failing |
| Throughput (single-thread) | 6.8 MB/s | 6.8 MB/s | 6.9 MB/s |
| ABI | 14 | 15 (CLI 0.25.10) | 15 |
| SymForge xref query compiles | yes | yes (drop-in) | **no** (`selector` absent) |
| Parser panics | none | none | **none in 8,445 parses** |

### Feature corpus (Dart 3.0 → 3.12)

| Feature | orchard 0.3.2 | orchard HEAD | nielsenko 0.2.0 |
|---|---|---|---|
| 3.0 class modifiers (sealed/base/final/interface/mixin) | clean | clean | clean |
| 3.0 records, patterns, switch expressions | clean¹ | clean¹ | clean¹ |
| 3.3 extension types | clean | clean | clean |
| 3.6 digit separators | clean | clean | clean |
| 3.7 wildcard variables | clean | clean | clean |
| 3.8 null-aware elements (`[?a]`, `{?k: v}`) | **5 ERRORs** | clean | clean |
| 3.10 dot shorthands (`.running`, `.parse(...)`, `== .x`) | **8 ERRORs + 1 MISSING** | clean (`dot_shorthand` node) | clean (`static_member_shorthand` node) |
| 3.12 private named parameters (`{required this._x}`) | not tested² | clean | clean |
| abstract/external fields, `@anno (R, T) m()`, empty records | **4 ERRORs** | clean | clean |

¹ One shared exotic edge: `final (int p, {String? q}) = (1, q: 'hi');` (named-field
record destructuring) produces 1 MISSING node on orchard and a zero-width identifier on
nielsenko. Zero occurrences in 2,805 real files; candidate for upstream issues.
² 0.3.2 already fails on earlier features; not separately diagnostic.

### What 0.3.2 actually fails on in the wild (failure classes from flutter/packages)

Everyday modern Dart, not exotica:

- `case Ok<Destination>():` / `case UIViewWKWebView():` — empty object patterns, the
  standard sealed-Result idiom (most common failure class)
- `Status s = .running;` — dot shorthands (stable since Nov 2025)
- `[1, ?a, 3]`, `{?k: v}` — null-aware elements (stable since May 2025)
- `extension type X._(JSObject _) implements JSObject` — JS-interop extension types
- `external ffi.Pointer<ffi.Uint8> verbs;`, `@Uint32()` on fields — FFI bindings
- `library;` — unnamed library directive
- `({int? a, int? b}) get opts =>` — record-typed getters

### Extractor findings (apply to ANY grammar choice)

The "inherited extraction quirks" are mostly **SymForge-side, not grammar-side**:

1. **Return-type misnaming.** `dart.rs` uses first-named-child
   (`identifier`/`type_identifier`), which grabs the return type: `Widget build()`
   indexes as "Widget", `Future<void> main()` as "Future". Both orchard (even 0.3.2)
   and nielsenko expose a **`name` field** on declaration nodes —
   `child_by_field_name("name")` fixes this in a few lines. (On nielsenko the naive
   walker happens to produce correct names because return types are wrapped in a
   `type` node; the field-based fix is still the right implementation.)
2. **Missing declaration kinds.** `extension_type_declaration` (Dart 3.3+),
   `mixin_declaration`, `extension_declaration`, `getter_signature`/`setter_signature`
   exist in both grammars but are unmapped — extension types currently produce **zero
   symbols**.

---

## SymForge context

| Layer | Implementation | Coupling |
|---|---|---|
| Grammar | `tree-sitter-dart-orchard = "0.3.2"` (`Cargo.toml`) | crate swap is trivial; **node names are the real coupling** |
| Language selection | `src/parsing/mod.rs` (`parse_source`) | one match arm |
| Symbol extraction | `src/parsing/languages/dart.rs` | 4 node kinds: `function_signature`, `class_definition`, `enum_declaration`, `method_signature` |
| Cross-references | `src/parsing/xref.rs` (`DART_XREF_QUERY`) | `identifier+selector(argument_part)`, `unconditional_assignable_selector`, `import_specification>configurable_uri>uri>string_literal`, `type_identifier` |
| ast-grep | `src/parsing/ast_grep.rs` | shares the same `LANGUAGE` handle |

Hard requirements (derived from code, not assumption): byte-exact spans; deterministic;
in-process rayon batch (1000-file perf gate); Windows MSVC first-class; no mandatory
external SDK; one parsing stack shared with ~24 languages + ast-grep-core; per-file
symbols + syntactic refs (SymForge does no cross-file semantic resolution for ANY
language today).

---

## Dart syntax-feature timeline (what any grammar must chase)

Current stable: **Dart 3.12.1 (2026-05-26)**. Sources: dart.dev language evolution,
SDK CHANGELOG.

| Version | Released | Syntax-affecting features |
|---|---|---|
| 3.0 | 2023-05 | records, patterns, class modifiers, switch expressions, if-case |
| 3.3 | 2024-02 | extension types |
| 3.6 | 2024-12 | digit separators |
| 3.7 | 2025-02 | wildcard variables |
| 3.8 | 2025-05 | null-aware elements |
| 3.9 / 3.11 | 2025-08 / 2026-02 | none |
| 3.10 | 2025-11 | **dot shorthands** |
| 3.12 | 2026-05 | **private named parameters**; primary constructors behind `--enable-experiment` |
| 3.13 | unreleased | **primary constructors** (the next structural change — watch item) |

Macros were **cancelled January 2025** (replaced by smaller augmentations), removing
the one looming grammar earthquake. Net rate: roughly one modest syntax feature per
1–2 releases — a treadmill, but a slow one that an actively maintained grammar absorbs.

---

## Candidate 1: Dart Analysis Server — REJECT as replacement, ADOPT later as D2

Long-lived Dart process (`dart language-server`, LSP default; the legacy Analysis
Server Protocol is being retired — new integrations must target LSP). The engine
behind VS Code/IntelliJ.

- Exposes `documentSymbol`, `definition`, `references`, `workspace/symbol`,
  semantic tokens, multi-root workspaces. **No raw syntax tree/CST over the wire** —
  it cannot feed SymForge's extractor or ast-grep.
- Measured costs from dart-lang/sdk trackers: **multi-GB RAM** (1.8 GB with lints
  #41793; 19 GB monorepo #52447; 32 GB runaway #40243; P90 +300% at ≥20 analysis
  contexts #53875), **cold start 12.2 s** on dartdoc (devoncarew), up to ~3 min on
  pathological multi-project setups (#62539).
- Requires a user-installed, version-skew-prone SDK plus `pub get` for resolution;
  degraded results on unresolvable projects; orphan-process and OOM failure modes.
- Per-document request/response — wrong shape for batch indexing thousands of files.

Strengths are real and unique: resolved cross-file references, types, hierarchies —
beyond anything SymForge has for any language. That is a **D2 enrichment tier**
(lazy start, idle timeout, memory ceiling, D1 fallback), never the Tier-0 parser.

## Candidate 2: `package:analyzer` — AUGMENT-only (the reference parser, wrong process)

Dart library; **no C ABI, no FFI** — cannot link into Rust. Realistic embedding is a
custom Dart sidecar compiled with `dart compile exe` (AOT) emitting JSON over stdio.

- **Fidelity is permanently perfect** — it *is* the reference grammar; new syntax
  (dot shorthands, private named params, primary constructors) parses the day the SDK
  ships. Syntax-only `parseString` needs no `pub get`; resolved mode does.
- Costs: Dart toolchain in CI; **no AOT cross-compilation** (build on each target OS);
  ~5 MB runtime floor, est. 15–40 MB per platform × ~6 platforms; sidecar lifecycle
  management; **3–4 breaking analyzer majors per year** (7.0→13.2 in ~22 months — the
  package explicitly warns its public/internal API boundary is unsettled).
- `dart analyze --format=json/machine` emits **diagnostics only** — no symbols. A
  custom sidecar is mandatory; no official AST-dump CLI exists in the SDK.
- An unresolved (`parseString`) sidecar yields the *same information tier* SymForge
  already gets from tree-sitter, at far higher operational cost. Only the resolved
  mode adds value — and that is the D2/D3 story, not Tier-0.

## Candidate 3: Existing Rust crates

| Crate | What it is | Verdict |
|---|---|---|
| `tree-sitter-dart-orchard` 0.3.2 | incumbent; see Candidate 5 | baseline — pinned version empirically inadequate |
| `tree-sitter-dart` 0.2.0 (nielsenko) | **the crate name changed lineage in 2026** — see below | **recommended** |
| `arborium-dart` 2.18.0 | repackaged tree-sitter grammar + patched runtime (bearcove) | no edge; extra deps |
| `oak-dart` 0.0.11 (2026-03-30) | the one non-tree-sitter Rust Dart parser: an **example crate** of the ygg-lang/oaks framework; 404 total downloads | architecturally interesting (Roslyn-style green/red trees); not production-credible |
| pest / nom / chumsky / lalrpop Dart | none exist with Dart 3 coverage | — |
| analyzer FFI / Dart-VM embed / WASM analyzer | none Rust-consumable | — |

### The headline ecosystem event

The crates.io name `tree-sitter-dart` (frozen at 0.0.4 since 2024) was taken over in
March 2026 and now publishes **nielsenko's grammar** — written from scratch against
the official `dartLangSpec.tex`, with a custom scanner solving the record/annotation
ambiguity that structurally limits the UserNobody14 lineage (tree-sitter#3243), and
explicitly designed for structural-search tools (ships `tags.scm`).

- 0.1.0 → 2026-03-11; 0.2.0 → 2026-04-26, requires tree-sitter `^0.26` (exact match
  for SymForge's `=0.26.9` pin).
- **Crate co-owned by HerringtonDarkholme (ast-grep's author) + nielsenko**; adopted
  by ast-grep itself (`ast-grep-language`). ~320k downloads, ~798 transitive
  dependents vs orchard's ~36k / 7.
- Cleaner extraction surface: `call_expression(function:, arguments:)`,
  `member_expression(object:, property:)`, `import_specification(uri:, alias:)`,
  `function_signature(name:, parameters:, return_type:)`, `mixin_declaration(name:)`.
- Peer evidence: a Rust indexer (`Claude-ast-index-search`) migrated orchard-0.3.2 →
  nielsenko-0.2.0 on 96,505 files and reported **+22% symbols**, fixing five
  extraction bugs. (Caveat verified: their "orchard pre-dates Dart 3" framing is
  overstated — the gain is the crate-lag + extraction fixes, consistent with our
  measurements.)
- Risks: pre-1.0 node-kind churn (pin exact versions); repo is 3 months old, repo bus
  factor 1 (crate co-ownership is the real safety net); two pre-flagged quirks
  (`typedef X = ... Function(...)` mis-parse; `library_export` lacks an
  `import_specification` wrapper).
- The original "newer crate lineage introduces parser panics" concern **did not
  reproduce**: zero panics in 8,445 parses of 0.2.0.

### Consumer-pin map (mid-2026 center of gravity)

| Consumer | Grammar |
|---|---|
| nvim-treesitter, Helix, Zed, difftastic, Semgrep | UserNobody14 (editor world; no crate — vendoring required) |
| **ast-grep**, the live `tree-sitter-dart` crate | **nielsenko** (Rust structural-analysis world) |
| SymForge (incumbent) | orchard 0.3.2 |

SymForge depends on ast-grep-core and shares the grammar with it — its ecosystem has
already converged on nielsenko.

## Candidate 4: ANTLR — NOT-VIABLE

- The runtime is alive again (`antlr4rust` 0.5.2, 2025-10-25), **but there is no
  Dart 3 ANTLR4 grammar to feed it**: the official `Dart.g` in dart-lang/sdk is
  **ANTLR v3** (spec validation only — incompatible with antlr4rust), and
  `grammars-v4/dart2` is Dart-2-era and stale.
- A mechanically converted Dart.g grammar was tried (yanok/tree-sitter-dart); its own
  README reports badly degraded trees — evidence the conversion path doesn't work.
- ANTLR also forfeits incremental parsing, tree-sitter ERROR-node recovery (which
  `parse_diagnostic` keys off), and the shared ast-grep grammar.

## Candidate 5: orchard git HEAD vs crates.io 0.3.2

- **HEAD (2026-05-05) eliminates every measured real-world failure of 0.3.2**:
  100% clean on 2,805 Flutter files; dot shorthands (expression + pattern), null-aware
  map keys, abstract/external fields, empty records, annotated record-returning
  methods all fixed (Mar–May 2026 commits). Fully extractor-compatible: SymForge's
  query and node kinds work unchanged; ABI 15 loads fine under tree-sitter 0.26.9.
- **A plain Cargo git pin does NOT build**: generated `parser.c` is gitignored at
  HEAD; only releases/crates ship artifacts. Consuming HEAD means
  `tree-sitter generate` (CLI ^0.25.8) + vendoring (precedent:
  `vendor/tree-sitter-scss`) — or asking upstream to cut a release (HEAD's manifest
  still says 0.3.2; the 7-month lag is an uncut release, not divergence).
- Project health: genuinely active (commits 2026-05-05/06, Copilot-assisted PRs,
  org governance doc, 7 releases Aug–Nov 2025) but low visibility — 0 stars,
  3 watchers, ~1 primary human maintainer (Antonin Delpeuch / wetneb), 7 reverse deps.

## Candidate 6: Formatter and compiler internals — REJECT

- `dart format`/`dart_style`: parses via the analyzer internally; exposes no AST.
- `front_end`/kernel `.dill`: **desugared** IR (loses original syntax shapes — fatal
  for byte-exact spans); dump tools are internal to the SDK repo; format explicitly
  unstable with no external-consumer contract.
- No official machine-readable AST dump tool ships in the SDK. Programmatic
  `package:analyzer` (Candidate 2) is the only AST access.

## Candidate 7: Alternative parser technologies — NOT-VIABLE

- **Lezer**: no Dart grammar exists (CodeMirror serves Dart via the legacy CM5
  `clike` stream mode); Lezer is JS-only regardless.
- **WASM Dart parsers consumable from Rust**: none.
- **oak-dart**: see Candidate 3 — the lone non-tree-sitter Rust experiment; framework
  example, negligible adoption, unverified claims.

## Additional candidate: `scip-dart` (Workiva) — future D3 substrate

Batch CLI on `package:analyzer` → protobuf `index.scip` (Rust-ingestible via the
`scip` crate). Real but niche: v1.6.2 (2025-05-28), pushes into Feb 2026, 16 stars.
Requires Dart SDK + `dart pub get` per project; not incremental; out-of-process only.
**Two flags before adoption:** (1) its `analyzer` dependency lags several majors —
version-skew risk against current SDKs; (2) **license metadata is `NOASSERTION`** —
must be cleared before shipping. Fit: D3 semantic cache, never Tier-0.

---

## Comparative ranking (11 criteria)

| Criterion | orchard 0.3.2 | orchard HEAD (vendored) | **nielsenko 0.2.0** | DAS (D2) | analyzer sidecar | scip-dart (D3) | ANTLR/Lezer/oak |
|---|---|---|---|---|---|---|---|
| 1. Correctness (measured) | 97.4% files clean | **100%** | **100%** | semantic gold | reference | semantic | n/a–unproven |
| 2. Dart 3.x syntax | fails 3.8/3.10 | 3.0–3.12 | 3.0–3.12 | always current | always current | analyzer-lagged | no |
| 3. Maintenance | crate stale 7 mo | active, unreleased | active, young | first-party | first-party | Workiva, slow | dead/example |
| 4. Future-proofing | poor | good | good (spec-native) | perfect | perfect | moderate | poor |
| 5. Bus factor | ~1 | ~1 (+org) | repo 1 / **crate 2 incl. ast-grep author** | Google | Google | corporate | n/a |
| 6. Rust integration | native | native + vendor pipeline | **native, crates.io** | LSP subprocess | AOT sidecar | subprocess+ingest | poor |
| 7. Incremental | yes | yes | yes | warm only | no | no | no |
| 8. Startup | ms | ms | ms | 12 s–3 min | ~100 ms+spawn | minutes | n/a |
| 9. Memory | MB | MB | MB | **GB** | 100s MB | GB during run | n/a |
| 10. Symbol/ref extraction | lossy | working | working + richer fields | semantic, no CST | full AST | SCIP refs | from scratch |
| 11. Sustainability | weak | uncertain | strongest of the grammars | strong | strong | moderate | weak |

---

## Falsifying the current choice

| Claim in the migration rationale | Verdict |
|---|---|
| "0.3.2 parses Dart 3 successfully" | **FALSIFIED** — fails 2.6% of real Flutter files; 0% of dot-shorthand and null-aware-element code |
| "actively maintained through 2026" | True of the repo (HEAD), **false of the crate** (nothing since 2025-11-17) |
| "preserves extractor compatibility" | True — and remains true at HEAD |
| "newer crate lineage introduces parser panics" | **NOT REPRODUCED** on nielsenko 0.2.0 (0 panics / 8,445 parses); incompatible node kinds confirmed (migration cost, not defect) |
| "crate version lags git HEAD" | Confirmed and material: all Dart 3.8/3.10 fixes are unreleased |
| "inherited extraction quirks" | Mostly SymForge's extractor (ignores `name` fields; unmapped declaration kinds) — fixable on any grammar |
| "low visibility and bus factor" | Confirmed: 0 stars, ~1 maintainer, 7 reverse deps |

**The approach (tree-sitter for Tier-0) survives falsification. The pin does not.**

---

## Final answers

### 1. Is `tree-sitter-dart-orchard` currently the best practical option?

As pinned (0.3.2): **no** — empirically inadequate for post-3.7 Dart. As a project
(HEAD): excellent and extractor-compatible, but consumable only via a
generate-and-vendor pipeline because upstream hasn't cut a release in 7 months.

### 2. Is there an objectively superior replacement?

**Yes — within tree-sitter.** nielsenko `tree-sitter-dart` 0.2.0 ties orchard HEAD on
measured correctness (both 100%) and beats every option on supply-chain position:
published crate, tree-sitter `^0.26` match, spec-native design, co-ownership by the
ast-grep author SymForge already depends on, and node fields that make the extractor
*simpler and more correct*. Cost: a bounded ~1-day node-kind rewrite
(`class_definition`→`class_declaration`, xref query → `call_expression`/
`member_expression`, root `program`→`source_file`), pinned exactly against pre-1.0
churn. Vendored orchard HEAD is the legitimate zero-churn alternative if minimizing
diff is paramount.

### 3. Would Dart Analysis Server be a better long-term foundation?

**No.** Foundation means Tier-0, and it fails Tier-0 on every operational axis (RAM,
cold start, batch shape, SDK dependency, no CST). Its semantics belong in an optional
D2 layer, with `scip-dart` as the batch D3 substrate — additive, lazy, gracefully
absent. Dart should not become the one language with a gigabyte-class co-processor
for symbol extraction.

### 4. Greenfield 2026 choice?

```
D0/D1: tree-sitter via nielsenko tree-sitter-dart (exact-pinned crate)
        └─ symbols (name-field-based), outlines, heuristic xrefs, ast-grep — always on

D3:    scip-dart batch indexer → SCIP ingest → semantic cache
        └─ AFTER clearing its NOASSERTION license + analyzer-version skew

D2:    dart language-server --protocol=lsp (lazy, idle-timeout, memory ceiling)
        └─ definition/references/hover when D1 confidence is low
```

Tree-sitter is the only technology meeting the Tier-0 constraints; nielsenko is the
grammar with the strongest 2026 trajectory; the official toolchain enters exactly
where it is irreplaceable (semantics) and nowhere else.

---

## Recommended next steps

1. **Immediate:** treat pinned 0.3.2 as a known correctness hole (2.6% of real
   Flutter files; all dot-shorthand code). Do not advertise Dart 3 support on it.
2. **Short-term (pick one):**
   - **Path A (recommended):** swap to `tree-sitter-dart = "=0.2.0"`; rewrite
     `dart.rs` walker and `DART_XREF_QUERY` against `class_declaration` /
     `call_expression` / `member_expression` / `import_specification(uri:)`; lean on
     upstream `tags.scm`; budget the two pre-flagged quirks (typedef-function,
     library_export).
   - **Path B (zero-churn):** vendor orchard HEAD (`tree-sitter generate` with CLI
     0.25.x → `vendor/tree-sitter-dart-orchard`, mirroring the tree-sitter-scss
     precedent); ask upstream to cut 0.3.3 and drop the vendor copy when it ships.
3. **Either path:** switch symbol naming to `child_by_field_name("name")` and map
   `extension_type_declaration`, `mixin_declaration`, `extension_declaration`,
   getter/setter signatures. Add regression fixtures: dot shorthands (expr + pattern +
   `== .x`), null-aware elements, `case Ok<T>():`, `library;`, extension types,
   private named parameters.
4. **Watch items:** Dart 3.13 primary constructors (the next structural syntax;
   verify grammar response within one release); nielsenko 0.3.x node-kind churn
   before any bump; orchard release cadence.
5. **Later (semantic tier, per `docs/semantic-tier-roadmap.md`):** spike `scip-dart`
   ingest on one resolvable repo (measure RAM, batch time, ref quality vs D1) after
   clearing its license metadata; design D2 LSP adapter with lazy start and D1
   fallback.

---

## Appendix A — Errata vs the prior internal draft (branch `feat/dart-orchard`)

The prior draft (this file's first version) reached a defensible hybrid architecture
but contained five claims falsified by measurement or primary sources:

1. *"orchard 0.3.2 parses Dart 3 cleanly"* — false beyond the 3.0 subset; see
   empirical tables (2.6% real-file failure rate; 3.8/3.10 features fail outright).
2. *"Pinning git HEAD: integration complexity Low — Cargo git pin"* — a git pin does
   not build; `parser.c` is gitignored upstream. Vendoring or an upstream release is
   required.
3. *"nielsenko `tree-sitter-dart` 0.2.0: unknown Dart 3.11 coverage; not clearly
   superior; no evidence of better coverage or stability"* — argument from absence.
   Measured: 100% clean on 2,805 files, all 3.0–3.12 features, zero panics; crate
   co-owned by the ast-grep author; adopted by ast-grep.
4. *"Grammar Orchard standardizes on ABI 14"* — the crate ships ABI 14; HEAD pins
   tree-sitter-cli ^0.25.8, which generates ABI 15 (verified by building HEAD).
5. *Dot shorthands labeled a "Dart 3.11" feature* — they are Dart 3.10 (Nov 2025);
   3.11 added no syntax.

Gems preserved from that draft: the D0–D3 hybrid framing and roadmap alignment;
`oak-dart` (verified: real, but an example crate of the oaks framework, 404
downloads); scip-dart operational details (analyzer version-skew risk); dart-lang/sdk
#62539 (~3 min pathological startup); the D2 adapter design sketch (lazy start, SDK
path config, memory ceiling, D1 fallback metadata).

## Appendix B — Load-bearing sources

- Empirical: probe harness over flutter/samples @ HEAD (483 files) and
  flutter/packages @ HEAD (2,322 files), grammars `tree-sitter-dart-orchard 0.3.2`
  (crates.io), `grammar-orchard/tree-sitter-dart-orchard` @ 2026-05-05 HEAD
  (generated with tree-sitter-cli 0.25.10), `tree-sitter-dart 0.2.0` (crates.io);
  tree-sitter runtime `=0.26.9`, Windows MSVC, 2026-06-11.
- Grammars: https://codeberg.org/grammar-orchard/tree-sitter-dart-orchard ·
  https://crates.io/crates/tree-sitter-dart-orchard ·
  https://crates.io/crates/tree-sitter-dart ·
  https://github.com/nielsenko/tree-sitter-dart ·
  https://github.com/UserNobody14/tree-sitter-dart ·
  https://github.com/tree-sitter/tree-sitter/issues/3243
- ast-grep adoption: https://github.com/ast-grep/ast-grep/pull/2534
- Peer migration: https://github.com/defendend/Claude-ast-index-search
- Dart timeline: https://dart.dev/resources/language/evolution ·
  https://dart.dev/blog/announcing-dart-3-10
- Analysis Server: https://github.com/dart-lang/sdk/blob/main/pkg/analysis_server/tool/lsp_spec/README.md ·
  dart-lang/sdk #52447, #41793, #40243, #53875, #62539 ·
  https://medium.com/@devoncarew/towards-faster-dart-analysis-aa70c45e3d04
- analyzer: https://pub.dev/packages/analyzer/changelog · dart-lang/sdk #44465, #45068
- AOT/licensing: https://dart.dev/tools/dart-compile · dart-lang/sdk #52088 ·
  https://github.com/dart-lang/sdk/blob/main/LICENSE
- Macros cancellation: https://medium.com/dartlang/an-update-on-dart-macros-data-serialization-06d3037d4f12
- scip-dart: https://github.com/Workiva/scip-dart
- ANTLR: https://crates.io/crates/antlr4rust ·
  https://github.com/dart-lang/sdk/blob/master/tools/spec_parser/Dart.g ·
  https://github.com/antlr/grammars-v4/tree/master/dart2
- oak-dart: https://crates.io/crates/oak-dart
- Lezer absence: https://github.com/codemirror/legacy-modes/tree/main/mode
