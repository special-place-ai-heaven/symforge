# cbm_resolver_rust fixture

Benchmark for SP-0C / SC-004 (Program 015 Sprint 0 falsifier). See
`specs/015-cbm-capability-ports/planning/sprint-0-spike-spec.md`.

Each `*.rs` file is a self-contained Rust snippet (never compiled). `manifest.json`
is the ground truth. The spike resolver (`src/parsing/resolver/rust.rs`,
same-file + `use`-import only, no type inference / cross-file / FFI) is run over
each fixture and scored against it.

## Implemented manifest schema

The S0 spike implements a concrete subset of the drafted sketch (no
`min_confidence` — v1 emits no confidence; keyed by `name`+`line` instead of
`caller_symbol`+`call_text`, which the test verifies so line drift fails loudly).

```json
{
  "version": 1,
  "fixtures": [
    {
      "file": "local_calls.rs",
      "cases": [
        {
          "name": "helper",
          "line": 6,
          "category": "same_file",
          "expected_strategy": "same_file",
          "expected_callee": "helper"
        }
      ]
    }
  ]
}
```

- `name` + `line` = 1-based call-site key (unique within a file).
- `category` documents the case kind (`same_file`, `use_path`, `method`,
  `crate_path`, `negative`).
- `expected_strategy` ∈ `same_file | import | unresolved`.
- `expected_callee` = the verdict v1 *should* produce; `null` for `unresolved`
  (out-of-scope: stdlib method, cross-file `crate::`, undefined name).

## Two reported numbers

1. **Verdict accuracy** (the falsifier metric / GO bar): a case is correct iff
   the resolver's `(strategy, callee)` matches `(expected_strategy,
   expected_callee)`. Correctly *declining* an out-of-scope call counts as
   correct; over-resolving it (false positive) counts as wrong. **GO ≥ 60%**
   (S0); S3 target 80%.
2. **In-scope callee recall**: of `same_file`+`use_path` cases, how many got the
   exact callee. Shows v1 is right where it claims a resolution.

Macros (`println!`, `vec!`) are excluded: they are `MacroUse`, not `Call`
references, so the resolver never sees them — scoring them would be free credit.
