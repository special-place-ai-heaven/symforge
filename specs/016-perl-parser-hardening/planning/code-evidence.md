# Code Evidence — 016 Perl Parser Hardening

**Rule**: Every `[P]` task adds or updates a row. Symbol + file first; lines second.

| ID | Sprint | Symbol / topic | Location | SymForge / verify | Date |
|----|--------|----------------|----------|-------------------|------|
| EV-PROG-001 | PROG | Perl parsing cluster | `src/parsing/xref.rs`, `perl.rs` | `explore` Perl ts-parser-perl xref | 2026-07-06 |
| EV-PROG-002 | PROG | Merge retrofit diff | `src/parsing/` | `diff_symbols` 30dd4c3→9572b31 | 2026-07-06 |
| EV-S0-001 | S0 | LanguageId::Perl dispatch | 6 files — see tasks P-S0-001 | `search_text` LanguageId::Perl | pending |
| EV-S0-002 | S0 | Merge symbol delta | perl.rs + xref.rs | `diff_symbols` | pending |
| EV-S0-003 | S0 | C++ qualified_call neighbor | `xref.rs` CPP_XREF_QUERY | `get_symbol` + test lock | pending |
| EV-S1-001 | S1 | process_file harness | `src/parsing/mod.rs` | `get_symbol_context` | pending |

## Sexp baseline

Archived at `docs/research/perl/sexp-baseline-2026-07-06.txt` (V-S0-002).

## Dogfooding log

| Date | Tool | Query | Notes |
|------|------|-------|-------|
| 2026-07-06 | explore | Perl ts-parser-perl xref compile_xref_query | 12 symbols, xref.rs primary |
| 2026-07-06 | diff_symbols | 30dd4c3...9572b31 src/parsing | +7 symbols, ~51 modified |
| 2026-07-06 | search_text | ts-parser-perl src/ | 6 matches, 2 files |
| 2026-07-06 | get_symbol | PERL_XREF_QUERY | 414-429 verified |
