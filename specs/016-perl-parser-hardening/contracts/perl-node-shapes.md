# Contract: Perl Node Shapes (ts-parser-perl 1.1.3)

**Feature**: 016 · **Status**: FROZEN 2026-07-06 (baseline) · **Source**: `probe_perl_grammar_sexp`

## Purpose

Anchor `PERL_XREF_QUERY` and `perl.rs` extractor to **verified** tree-sitter node kinds.
Any grammar bump MUST re-run probe and diff this contract before merge.

## Verified mappings

### Symbol extraction (`perl.rs`)

| Construct | Node kind | Name resolution |
|-----------|-----------|-----------------|
| Subroutine | `subroutine_declaration_statement` | `name` field → `bareword` |
| Method | `method_declaration_statement` | `name` field → `bareword` |
| Package | `package_statement` | `name` field → `package` |
| Class | `class_statement` | `name` field → `package` |
| Legacy sub (compat) | `function_definition`, `function_definition_without_sub` | child scan fallback |

### Xref extraction (`PERL_XREF_QUERY`)

| Construct | Query pattern | Capture |
|-----------|---------------|---------|
| Method call `$o->m()` | `(method_call_expression method: (method) @ref.method_call)` | method name |
| Plain call `foo()` | `(function_call_expression function: (function) @ref.call)` | function name |
| List-op call `print foo` | `(ambiguous_function_call_expression function: (function) @ref.call)` | function name |
| use | `(use_statement module: (package) @ref.import)` | package path |
| require | `(require_expression (bareword) @ref.import)` | bareword path |

## Sexp samples (canonical)

```
(subroutine_declaration_statement name: (bareword) body: (block ...))
(package_statement name: (package))
(class_statement name: (package) (block (method_declaration_statement name: (bareword) ...)))
(method_call_expression invocant: ... method: (method) arguments: ...)
(function_call_expression function: (function) arguments: ...)
(use_statement module: (package))
(require_expression (bareword))
```

## Bump protocol

1. Run `cargo test probe_perl_grammar_sexp --lib -- --ignored --nocapture`
2. Diff output against this file
3. If diff non-empty: update contract + query + extractor in same PR

## Out of scope (until sexp proves node exists)

- Qualified call AST shape → S2 contract extension
- `role`, `attribute`, Moose exports
