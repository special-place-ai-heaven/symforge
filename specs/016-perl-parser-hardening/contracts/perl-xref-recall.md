# Contract: Perl Xref Recall

**Feature**: 016 · **Status**: FROZEN (2026-07-06, S2 complete)

## Purpose

Define expected xref behavior for Perl fixtures. S2 implementation satisfies this
contract on the P1 fixture subset; probe-proven extras are covered by unit tests.

## Reference processing rules

Imports use existing `push_import_reference` semantics:

- `use Foo::Bar` → name `Bar`, qualified_name `Foo::Bar`
- `require Baz::Qux` → name `Qux`, qualified_name `Baz::Qux`
- `use parent qw(Foo)` / `use base qw(Bar)` → name from qw list (`Foo` / `Bar`), not pragma name

Calls:

- Method calls → `ReferenceKind::Call`, name = leaf identifier; `::` paths split to `qualified_name`
- Plain/ambiguous calls → `ReferenceKind::Call`, name = leaf; `Foo::bar` / `CORE::push` retain full path in `qualified_name`
- Coderef → `$coderef->()` captured via `coderef_call_expression` invocant as `Call`

## P1 construct requirements (S2 exit)

| Construct | Min fixtures | Required refs |
|-----------|--------------|---------------|
| method_call | 2 | `@ref.method_call` captures |
| plain_call | 2 | `@ref.call` captures |
| use_import | 2 | `@ref.import` with qualified_name |
| require_import | 2 | `@ref.import` with qualified_name |
| class body method call | 1 | Call inside class method body |
| qualified_call | 1 | leaf name + `qualified_name` on `(function)` |
| super_method_call | — | unit test: `$self->SUPER::method()` |
| core_function_call | — | unit test: `CORE::push()` |
| coderef_call | — | unit test: `$coderef->()` |
| use_parent_base | — | unit test: `use parent qw(Foo)` |

## Accepted loss

| Construct | Reason |
|-----------|--------|
| Fully dynamic `${...}()` | No stable callee name in AST |

## Degradation

If `perl_query()` returns `None` (query compile failure):

- `extract_references` returns empty vec for Perl
- MUST NOT panic
- `tracing::warn!` emitted once per process (see compile-xref-degradation contract)

## Regression locks (must stay green)

- `test_perl_method_invocation_and_import`
- `test_perl_class_method_call_recovered`
- `test_perl_qualified_function_call`
- `test_perl_super_method_call`
- `test_perl_core_function_call`
- `test_perl_coderef_call`
- `test_perl_use_parent_import`
- `test_cpp_qualified_call_retains_head` (non-Perl neighbor)

## Verification

```bash
cargo test --lib test_perl_ --features server -- --test-threads=1
cargo test --lib test_cpp_qualified_call_retains_head --features server -- --test-threads=1
cargo test --test perl_corpus --features server -- --test-threads=1
```
