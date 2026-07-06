# Research: Perl Parser Hardening

**Feature**: 016 · **Date**: 2026-07-06 · **Baseline**: `9572b31`

## Decision summary

| ID | Decision | Rationale |
|----|----------|-----------|
| D-016-001 | Fixture corpus is authoritative gate | #341 8k benchmark not CI-viable |
| D-016-002 | sexp probe anchors node contract | Issue #341 listed guessed names; merge used verified sexp |
| D-016-003 | Corpus-driven S2 only | YAGNI on qualified calls until taxonomy proves gap |
| D-016-004 | Keep explicit OnceLock getters | xref.rs ponytail rejection 2026-06-18 |
| D-016-005 | Dart investigation doc as template | Proven pattern in repo |

## ts-parser-perl node shapes (verified 2026-07-06)

From `probe_perl_grammar_sexp` on ts-parser-perl **1.1.3**:

| Sample source | Root construct | Name field |
|---------------|----------------|------------|
| `sub greet { 1 }` | `subroutine_declaration_statement` | `name: (bareword)` |
| `package MyApp::Module;` | `package_statement` | `name: (package)` |
| `class Bar { method m { 1 } }` | `class_statement` + `method_declaration_statement` | class: `(package)`; method: `name: (bareword)` |
| `$obj->method(1, 2);` | `method_call_expression` | `method: (method)` |
| `foo(1, 2);` | `function_call_expression` | `function: (function)` |
| `print foo;` | `ambiguous_function_call_expression` | `function: (function)` |
| `use Foo::Bar;` | `use_statement` | `module: (package)` |
| `require Baz::Qux;` | `require_expression` | `(bareword)` |

Full contract: [contracts/perl-node-shapes.md](./contracts/perl-node-shapes.md).

## Merge retrofit scope (`30dd4c3` → `9572b31`)

SymForge `diff_symbols` (2026-07-06):

- **perl.rs**: +1 test, ~3 symbols modified
- **xref.rs**: +4 tests + `compile_xref_query`, ~51 symbols modified (all langs Option cache)
- **C++ qualified_call**: preserved ( `test_cpp_qualified_call_retains_head` green)

## #341 benchmark vs program scope

| Claim (#341) | Program handling |
|--------------|------------------|
| ~95% clean parse (ts-parser-perl) | Re-measure on fixture corpus (SC-002); optional ignored external bench |
| ~40% (ganezdragon) | Historical only; no dual-grammar CI |
| Naive swap panics xref | **Fixed** via `compile_xref_query` |
| 8,342 file corpus | Out of scope for CI; cite in investigation doc |

## Likely S2 gap classes (hypothesis — must prove in S1)

| Class | Extractor today | Xref today | S1 action |
|-------|-----------------|------------|-----------|
| sub / package | ✓ | partial | baseline fixtures |
| class / method | ✓ | method calls in body | baseline fixtures |
| plain + method calls | n/a | ✓ | lock tests |
| use / require | n/a | ✓ | lock tests |
| qualified `Foo::bar()` | n/a | **unknown** | taxonomy + fixtures |
| `SUPER::`, `CORE::` | n/a | **unknown** | taxonomy |
| indirect / dynamic | n/a | **loss** | document accepted loss |

## References

- GitHub #341 (closed 2026-07-06)
- `docs/dart-parser-investigation.md` — doc template
- `docs/semantic-tier-roadmap.md` — Perl Tier-0
- Commit `9572b31` on main

## Open questions (resolved at planning)

All resolved in [spec.md § Clarifications](./spec.md#clarifications). None remain for implement.
