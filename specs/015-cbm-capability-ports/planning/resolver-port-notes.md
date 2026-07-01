# Resolver Port Notes — Program 015

**Status**: S0 skim · **Full**: S3 P-S3-001..003  
**CBM**: `internal/cbm/lsp/rust_lsp.c`

## Section map (P-S0-004 skim)

| § | Topic | SymForge | Sprint |
|---|-------|----------|--------|
| 1–2 | Registry | `parsing/resolver/registry.rs` | S3 |
| 3–4 | Same-file | `parsing/resolver/rust.rs` | S0/S3 |
| 5–6 | Imports | `rust.rs` | S3 |
| 7–12 | Traits/stdlib/confidence | defer partial | S3+ |

S0 spike: same-file + `use` in-file only. See fixture README under `tests/fixtures/cbm_resolver_rust/`.
