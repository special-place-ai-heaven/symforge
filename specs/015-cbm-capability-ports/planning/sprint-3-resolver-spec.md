# Sprint 3 Planning Spec — Hybrid Resolution

**Status**: draft  
**Release**: 8.12.x  
**User stories**: US8, US9  
**Depends on**: S2 graph, S0 SP-0C GO

## Objective

Type-aware **ResolvedCall** edges replacing name-only xref for Rust + TypeScript.
This is the largest accuracy jump and highest implementation risk.

## In scope

- `parsing/resolver/` module tree
- Rust cross-file resolution (≥80% benchmark)
- TS/JS cross-file resolution (≥75% benchmark)
- GraphProjection uses resolved edges when confidence ≥ threshold
- Snapshot v5 **if** PD-01 resolves to persist (plan migration in [P])

## Out of scope

- Python, Go, Java, Kotlin, C#, PHP, C++ (S3.1 backlog)
- External LSP subprocess
- Full macro expansion / rustc

## CBM deep-read (mandatory — estimate 2 planning days)

| File | Est. lines | Sections |
|------|------------|----------|
| `rust_lsp.c` | ~3300 | §1–§12 per file header |
| `ts_lsp.c` | ~4000 | import graph, JSX |
| `cbm.c` | dispatch 607–650 | per-language hook |
| `pass_parallel.c` | registry merge | Phase 3B serial merge |
| `pass_lsp_cross.c` | cross-file | pruning rules |

**Deliverable [P]**: `planning/resolver-port-notes.md` — function-level map CBM → Rust modules.

## Resolver pipeline ([P] P-S3-010)

```text
process_file_with_classification
  1. tree-sitter symbols + xref (existing)
  2. resolver::resolve_file(lang, file_result) → Vec<ResolvedCall>
  3. store on IndexedFile

index_folder completion (all files)
  4. resolver::merge_registry(all_files)  // serial
  5. resolver::resolve_cross_file(&mut index)
  6. rebuild/patch GraphProjection
```

## Confidence model ([P] P-S3-015)

| Strategy | Min confidence |
|----------|----------------|
| SameFileDirect | 0.95 |
| ImportBinding | 0.85 |
| CrossFileRegistry | 0.75 |
| TraitDispatch | 0.70 |
| Unresolved | 0.0 |

**Disclosure**: trace_path output includes `(confidence=0.82, strategy=ImportBinding)`.

## Snapshot v5 planning ([P] P-S3-020)

If persisting ResolvedCall:

| Field | Serialize? |
|-------|------------|
| ResolvedCall vec | yes |
| GraphProjection | no (rebuild) |

Migration test: load v4 → empty resolved; load v5 → round-trip.

**Decision PD-01** must be closed at Planning Gate.

## Fixtures

| Fixture | Min cases |
|---------|-----------|
| `cbm_resolver_rust/` | 20 call sites |
| `cbm_resolver_ts/` | 15 call sites |

Each: `expected_resolutions.json` schema:

```json
{"call_site": {"file","line","col"}, "expected_qname": "...", "min_confidence": 0.75}
```

## Benchmark methodology ([P] P-S3-025)

- **Pass**: matched expected_qname OR equivalent symbol id
- **Partial**: correct module, wrong overload — count as 0.5 (document only)
- **Fail**: Unresolved or wrong symbol

Report in `[V]`: table in sprint sign-off.

## Risk focus

R-02, R-03, R-12 — all HIGH

## Planning Gate

- [ ] resolver-port-notes.md complete
- [ ] PD-01 decided (D-015-008 filled)
- [ ] CBM rust_lsp.c §1–§6 read log
- [ ] Benchmark manifest approved
- [ ] No compact surface change

**Sign-off**: _________________ Date: _______

## Rollback

- Feature flag `SYMFORGE_RESOLVER=0` → xref-only graph (document in P-S3-030)

## Linked tasks

tasks.md S3 [P], [C], [V]
