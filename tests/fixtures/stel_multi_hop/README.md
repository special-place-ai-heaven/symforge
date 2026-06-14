# STEL multi-hop golden replay fixtures

Checked-in minimal corpora for CI-proof replay of the three Phase 2 multi-hop golden rows.
These are intentionally small and committed (unlike gitignored `phase0-corpus/` clones).

Used by `tests/stel_golden_replay.rs::multi_hop_golden_rows_replay_on_compact_symforge`.

| Row id | Corpus path | Marker |
|--------|-------------|--------|
| `cfg-if/multi_search_symbol` | `cfg-if-rust/` | `src/lib.rs` |
| `records/multi_context_refs` | `records-python/` | `records.py` |
| `is-plain/multi_files_content` | `is-plain-obj-ts/` | `test.js` |
