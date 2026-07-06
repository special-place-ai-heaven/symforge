# Sprint S2 — Coverage Expansion

**Feature**: 016 · **US**: US2

## Objective

Close extractor/xref gaps **only** for construct classes marked P1 in S1 taxonomy sign-off.

## Input artifact

`docs/research/perl/taxonomy-signoff.md` — P1 construct list is the sole scope authority.

## Construct wave template

For each P1 miss:

1. Add fixture (if not in S1)
2. Add failing unit test
3. sexp-verify node shape → update EV-S2-00N
4. Edit `PERL_XREF_QUERY` and/or `perl.rs`
5. Run partial gate:
   ```powershell
   cargo test --features server --lib test_perl_ test_cpp_qualified -- --test-threads=1
   cargo test --features server --test perl_corpus -- --test-threads=1
   ```

## Planning Gate checklist

- [ ] P-S2-001 backlog from S1 metrics
- [ ] P-S2-002 sexp proof for each planned query change
- [ ] contracts/perl-xref-recall updated
- [ ] file-touch-matrix S2 reviewed

## Release Gate checklist

- [ ] recall-metrics.json populated
- [ ] US2 table in spec.md met OR accepted-loss in investigation doc
- [ ] contracts/perl-xref-recall FROZEN
- [ ] V-S2-003 acceptance matrix signed

## Hard stops

- NO qualified_call query without sexp sample in EV-S2 row
- NO session with >6 `[C]` tasks without STOP gate
- ANY cpp qualified test failure → revert xref edit immediately
