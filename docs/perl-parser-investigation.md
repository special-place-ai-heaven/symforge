# Perl Parser Investigation — ts-parser-perl post-merge hardening

**Program**: 016 · **Baseline**: main @ `9572b31` · **Measured**: 2026-07-06

## Executive summary

SymForge switched Perl from ganezdragon `tree-sitter-perl` to **`ts-parser-perl` 1.1.3**
(Cargo alias `tree-sitter-perl`). The merge rewrote the symbol extractor, `PERL_XREF_QUERY`,
and hardened xref compilation via `compile_xref_query` (no panic on query/grammar mismatch).

This document records **in-repo evidence** on a 22-fixture synthetic corpus. External #341
benchmark claims (~95% vs ~40% on 8k files) are cited for context but **not** reproduced in CI.

**Fixture corpus result**: **100% clean parse** (22/22), symbols and xref expectations met.
Post-S2 probe added SUPER/CORE method-call split, coderef calls, and `use parent`/`use base` qw imports.

## Empirical evidence

| Metric | Value | Source |
|--------|-------|--------|
| Fixture count | 22 | `tests/fixtures/perl/manifest.json` |
| Clean parse % | 100.0% | `docs/research/perl/corpus-metrics.json` |
| Grammar | ts-parser-perl 1.1.3 | Cargo.lock |
| SymForge version | 8.10.7 | corpus-metrics.json |

Run bench:

```powershell
$env:PERL_CORPUS_WRITE_METRICS='1'
cargo test --features server --test perl_corpus bench_corpus_parse_metrics -- --ignored --nocapture
```

## SymForge context — three surfaces

| Surface | File | Role |
|---------|------|------|
| Dependency | `Cargo.toml` | `tree-sitter-perl = { package = "ts-parser-perl", version = "1.1" }` |
| Extractor | `src/parsing/languages/perl.rs` | subs, packages, classes, methods |
| Xref | `src/parsing/xref.rs` | `PERL_XREF_QUERY` + `compile_xref_query` |

Perl dispatch (8 files): `parsing/mod.rs`, `languages/mod.rs`, `perl.rs`, `xref.rs`,
`ast_grep.rs`, `inline_tests.rs`, `protocol/search_tools.rs`, `protocol/conventions.rs`.

Node shapes: [specs/016-perl-parser-hardening/contracts/perl-node-shapes.md](../specs/016-perl-parser-hardening/contracts/perl-node-shapes.md)  
Sexp archive: [docs/research/perl/sexp-baseline-2026-07-06.txt](./research/perl/sexp-baseline-2026-07-06.txt)

## Failure classes (S1 taxonomy)

| Bucket | Example | Program action |
|--------|---------|----------------|
| ParseError | tree-sitter ERROR nodes | Track in corpus `parse_expect` |
| ExtractorMiss | symbol not in index | S2 if P1 in taxonomy |
| XrefMiss | call/import not in refs | S2 if P1 (e.g. qualified_call) |
| AcceptedLoss | dynamic/indirect calls | Document only |

## S2 backlog (from corpus)

| Construct | Status | Priority |
|-----------|--------|----------|
| sub / package / class / method | Green | — |
| method / plain / ambiguous calls | Green | — |
| use / require imports | Green | — |
| qualified `Foo::bar()` | **Green** (leaf + qualified_name) | Done S2 |
| `$self->SUPER::method()` | **Green** (method :: split) | Done S2+ |
| `CORE::push()` | **Green** (function :: split) | Done S2 |
| `$coderef->()` | **Green** (coderef_call_expression) | Done S2+ |
| `use parent qw(Foo)` | **Green** (qw list module) | Done S2+ |
| fully dynamic `${...}()` | — | Accepted loss (after probe) |

## Final answers

1. **Is ts-parser-perl the right grammar?** Yes for Tier-0; fixture corpus parses cleanly.
2. **Is xref complete?** Tier-0 constructs covered; only truly dynamic invocation remains documented loss.
3. **What happens on grammar bump?** Follow [quickstart § Grammar bump](../specs/016-perl-parser-hardening/quickstart.md).
4. **What if query breaks?** `compile_xref_query` returns None; symbols may still index; warn once.

## Limits (accepted loss)

- **Dynamic invocation** — `${$coderef}()`, indirect calls through computed names: grammar may parse but xref cannot attach a stable callee. Not indexed; not silent.

## References

- GitHub #341 (closed)
- [016 spec](../specs/016-perl-parser-hardening/spec.md)
- Dart template: `docs/dart-parser-investigation.md`
