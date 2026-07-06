# Perl fixture corpus — Program 016

Synthetic snippets for parse-quality and recall measurement. Each file is tagged in
`manifest.json` with `construct_classes` and optional symbol/ref expectations.

## Tagging rules

- One primary construct per file when possible; secondary tags allowed.
- `parse_expect`: `clean` | `partial` | `error` (default `clean`).
- Expectations are minimum required — corpus may grow in S2.

## Construct classes

`sub`, `package`, `class`, `method`, `method_call`, `plain_call`, `ambiguous_call`,
`use_import`, `require_import`, `qualified_call`, `super_call`, `coderef_call`,
`parent_import`

## Running

```powershell
cargo test --features server --test perl_corpus -- --test-threads=1
cargo test --features server --test perl_corpus bench_ -- --ignored --nocapture
```

Metrics land in `docs/research/perl/corpus-metrics.json` when bench is run with
`PERL_CORPUS_WRITE_METRICS=1`.
