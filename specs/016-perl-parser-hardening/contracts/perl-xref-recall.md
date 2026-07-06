# Contract: Perl Xref Recall

**Feature**: 016 · **Status**: DRAFT until S1 taxonomy sign-off

## Purpose

Define expected xref behavior for Perl fixtures. S2 implementation MUST satisfy this
contract on the P1 fixture subset or document `AcceptedLoss`.

## Reference processing rules

Imports use existing `push_import_reference` semantics:

- `use Foo::Bar` → name `Bar`, qualified_name `Foo::Bar`
- `require Baz::Qux` → name `Qux`, qualified_name `Baz::Qux`

Calls:

- Method calls → `ReferenceKind::Call`, name = method identifier text
- Plain/ambiguous calls → `ReferenceKind::Call`, name = function identifier text

## P1 construct requirements (S2 exit)

| Construct | Min fixtures | Required refs |
|-----------|--------------|---------------|
| method_call | 2 | `@ref.method_call` captures |
| plain_call | 2 | `@ref.call` captures |
| use_import | 2 | `@ref.import` with qualified_name |
| require_import | 2 | `@ref.import` with qualified_name |
| class body method call | 1 | Call inside `method_declaration_statement` body |
| qualified_call | 3 | `@ref.call` + qualified_name when AST supports (S2) |

## Degradation

If `perl_query()` returns `None` (query compile failure):

- `extract_references` returns empty vec for Perl
- MUST NOT panic
- `tracing::warn!` emitted once per process (see compile-xref-degradation contract)

## Regression locks (must stay green)

- `test_perl_method_invocation_and_import`
- `test_perl_class_method_call_recovered`
- `test_cpp_qualified_call_retains_head` (non-Perl neighbor)

## Verification

```bash
cargo test --lib test_perl_ --features server -- --test-threads=1
cargo test --lib test_cpp_qualified_call_retains_head --features server -- --test-threads=1
# After S1:
cargo test --test perl_corpus --features server -- --test-threads=1
```
