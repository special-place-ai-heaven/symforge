# Contract: compile_xref_query Degradation

**Feature**: 016 · **Scope**: All 21 languages · **Introduced**: `9572b31`

## Purpose

Grammar/query node-kind mismatches MUST NOT panic the indexer. This contract applies
to Perl and all other languages using `compile_xref_query`.

## API

```rust
fn compile_xref_query(
    cache: &'static OnceLock<Option<Query>>,
    lang: &Language,
    src: &str,
    label: &str,
) -> Option<&'static Query>
```

## Behavior

| Condition | Result | Side effect |
|-----------|--------|-------------|
| First call, `Query::new` Ok | `Some(&Query)` | Cached in OnceLock |
| First call, `Query::new` Err | `None` | `tracing::warn!` with label |
| Subsequent calls after Err | `None` | No re-log (cached None) |
| Subsequent calls after Ok | `Some(&Query)` | — |

## extract_references integration

When language query resolves to `None`:

- Return `(Vec::new(), HashMap::new())` immediately
- Do not attempt partial extraction

## Test lock

- `test_compile_xref_query_degrades_on_mismatch` — MUST pass
- Deliberately invalid query `(no_such_node_kind_xyz) @ref.call` → None, no panic

## Non-goals

- Do NOT collapse per-language OnceLock statics into a table (rejected 2026-06-18)
- Do NOT auto-repair queries at runtime

## Perl-specific note

A ts-parser-perl bump that renames nodes without updating `PERL_XREF_QUERY` degrades
Perl xref to empty — symbols may still extract. Investigation doc MUST mention this
failure mode under trust envelopes (orientation: refs absent ≠ no calls in source).
