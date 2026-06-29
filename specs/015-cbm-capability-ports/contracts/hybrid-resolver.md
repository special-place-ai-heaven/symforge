# Contract: Hybrid Resolver

**Feature**: 015 · **Sprint**: S3 · **US**: US8, US9

## Pipeline placement

```
process_file (tree-sitter)
  → extract references (xref.rs)
  → resolve_calls (resolver/{lang}.rs)  [NEW]
  → merge registry (resolver/registry.rs) [cross-file pass]
  → LiveIndex update
  → GraphProjection patch
```

## ResolvedCall requirements

- Every syntactic call site MUST produce exactly one ResolvedCall record.
- `callee_symbol_id: None` allowed with `strategy: Unresolved` and reason string.
- Confidence ≥0.9 for same-file direct binding; ≥0.7 cross-file import binding.

## Language milestones

| Lang | Module | Min confidence (benchmark) |
|------|--------|---------------------------|
| Rust | `rust.rs` | 80% |
| TS/JS | `typescript.rs` | 75% |

## Cross-file registry

- Serial merge after parallel per-file extract (CBM Phase 3B pattern).
- Keyed by module path + qualified name.

## Embed

- Resolver runs under `embed` feature (no network).
- Not in frozen embed contract test.

## Non-goals

- No external LSP subprocess.
- No macro expansion full rustc (ponytail: common derive/macro rules only).
