# Golden route fixtures (Phase 0)

**File:** [routes.golden.jsonl](./routes.golden.jsonl) — 36 rows for H2 trajectory replay.

**Seed:** `node scripts/seed-routes-golden.cjs`  
**Validate:** `node scripts/validate-routes-golden.cjs`

Evidence: [A-028-golden-routes.md](../research/A-028-golden-routes.md)

## P-FF rows (4)

Full-file review tasks → `expected_decision=bypass`, `eligible_h6=false`:

- `cfg-if/pff_whole_lib`
- `records/pff_whole_module`
- `is-plain/pff_whole_index`
- `compression/pff_whole_service`

## Battery corpora

Cloned repos for legacy-tool batteries: [tests/fixtures/phase0-corpus/README.md](../../tests/fixtures/phase0-corpus/README.md)
