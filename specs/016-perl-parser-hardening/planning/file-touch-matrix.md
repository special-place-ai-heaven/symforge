# File Touch Matrix — 016 Perl Parser Hardening

| Path | S0 | S1 | S2 | S3 | Notes |
|------|----|----|----|----|-------|
| `src/parsing/languages/perl.rs` | verify | — | edit | — | S2 if extractor gaps |
| `src/parsing/xref.rs` | verify | — | edit | verify | High risk — D13 neighbor |
| `src/parsing/mod.rs` | read | read | read | — | dispatch verify |
| `src/parsing/languages/mod.rs` | read | — | — | — | |
| `Cargo.toml` | — | — | — | comment | pin policy |
| `Cargo.lock` | hotfix | — | — | bump | grammar only |
| `tests/fixtures/perl/**` | — | **add** | add | — | ≥20 files |
| `tests/perl_corpus.rs` | — | **add** | edit | — | |
| `tests/tree_sitter_grammars.rs` | verify | — | — | — | |
| `docs/perl-parser-investigation.md` | — | draft | edit | **final** | |
| `docs/research/perl/**` | sexp archive | metrics | metrics | — | |
| `docs/reviews/HANDOFF-*.md` | — | — | — | edit | stale ref |
| `specs/016-*/**` | — | — | — | update gates | living docs |

**Rule**: No S2 edits to `xref.rs` until S1 taxonomy sign-off file exists.
