# cbm_impact fixture — Program 015 S1a

Minimal Rust crate for `detect_impact` acceptance (A-US1-*).

## Layout

- `core()` in `lib.rs` — hub symbol
- `a.rs`, `b.rs`, `c.rs` — each call `core()`
- `main.rs` — entry

## Git bootstrap (required before V-S1A-001)

```bash
cd tests/fixtures/cbm_impact
git init
git add .
git commit -m "initial"
# edit src/a.rs, then:
git commit -am "change a"
```

## Manifest

See `expected_impact.json` — maps **changed file** from commit 2 → expected blast symbols.

## Spec

`specs/015-cbm-capability-ports/planning/sprint-1-quick-wins-spec.md` § Fixture spec
