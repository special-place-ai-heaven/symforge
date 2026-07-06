# Perl Parser Investigation — ts-parser-perl post-merge hardening

**Program**: 016 · **Baseline**: main @ `9572b31` · **Measured**: 2026-07-06

## Executive summary

SymForge switched Perl from ganezdragon `tree-sitter-perl` to **`ts-parser-perl` 1.1.3**
(Cargo alias `tree-sitter-perl`). The merge rewrote the symbol extractor, `PERL_XREF_QUERY`,
and hardened xref compilation via `compile_xref_query` (no panic on query/grammar mismatch).

This document records **in-repo evidence** on a 22-fixture synthetic corpus. External #341
benchmark claims (~95% vs ~40% on 8k files) are cited for context but **not** reproduced in CI.

**Fixture corpus result**: **100% clean parse** (22/22), symbols and xref expectations met
except `qualified_call` xref (optional — S2 target).

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
| qualified `Foo::bar()` | **Green** (leaf `bar` + qualified_name `Foo::bar`) | Done S2 |
| SUPER / CORE | Not in corpus | P2 if sexp proves nodes |
| dynamic calls | — | Accepted loss |

## Final answers

1. **Is ts-parser-perl the right grammar?** Yes for Tier-0; fixture corpus parses cleanly.
2. **Is xref complete?** No — qualified calls need S2 work.
3. **What happens on grammar bump?** Follow [quickstart § Grammar bump](../specs/016-perl-parser-hardening/quickstart.md).
4. **What if query breaks?** `compile_xref_query` returns None; symbols may still index; warn once.

## References

- GitHub #341 (closed)
- [016 spec](../specs/016-perl-parser-hardening/spec.md)
- Dart template: `docs/dart-parser-investigation.md`
