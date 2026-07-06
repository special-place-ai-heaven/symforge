# Quickstart: Perl Parser Hardening Gates

**Feature**: 016 · **Branch**: `016-perl-parser-hardening`

## Prerequisites

```powershell
cd E:\project\symforge
git checkout 016-perl-parser-hardening
```

SymForge MCP indexed on repo (for planning tasks). Rust toolchain per CLAUDE.md.

---

## PROG Gate — Program bootstrap

- [ ] Read [execution-model.md](./execution-model.md)
- [ ] Read [spec.md](./spec.md) user stories
- [ ] Complete PROG `[P]` tasks in [tasks.md](./tasks.md)
- [ ] [planning/program-planning-gate.md](./planning/program-planning-gate.md) signed

---

## S0 Gate — Retrofit audit (merge validation)

**Purpose**: Prove `9572b31` safe before S1 fixtures.

```powershell
cargo fmt --check
cargo check --features server
cargo clippy --all-targets --features server -- -D warnings
cargo test --features server --lib perl compile_xref cpp_qualified -- --test-threads=1
cargo test --features server --test tree_sitter_grammars test_perl -- --test-threads=1
cargo test --features server --lib probe_perl_grammar_sexp -- --ignored --nocapture
```

**Pass criteria**:

- All commands exit 0
- Sexp output matches [contracts/perl-node-shapes.md](./contracts/perl-node-shapes.md)
- Archive sexp to `docs/research/perl/sexp-baseline-2026-07-06.txt`

Optional full gate (release sprint):

```powershell
cargo test --features server --all-targets -- --test-threads=1
cargo build --release --features server
```

---

## S1 Gate — Evidence + corpus

```powershell
# After fixtures committed:
cargo test --features server --test perl_corpus -- --test-threads=1
# Ignored bench (metrics):
cargo test --features server --test perl_corpus bench_ -- --ignored --nocapture
```

**Pass criteria**:

- ≥20 fixtures in `tests/fixtures/perl/`
- `docs/research/perl/corpus-metrics.json` populated
- `docs/perl-parser-investigation.md` draft with SC-002 numbers
- [planning/sprint-1-evidence-corpus-spec.md](./planning/sprint-1-evidence-corpus-spec.md) Planning Gate ✓

---

## S2 Gate — Coverage expansion

After each construct-class wave:

```powershell
cargo test --features server --lib test_perl_ test_cpp_qualified -- --test-threads=1
cargo test --features server --test perl_corpus -- --test-threads=1
```

**Pass criteria**:

- US2 recall table in spec met or accepted-loss documented
- `docs/research/perl/recall-metrics.json` populated
- [contracts/perl-xref-recall.md](./contracts/perl-xref-recall.md) status → FROZEN

---

## S3 Gate — Operational + doc

```powershell
# Dry-run grammar bump checklist (no actual bump required for first pass):
# 1. probe  2. diff contract  3. test bundle S0
```

**Pass criteria**:

- [quickstart.md](./quickstart.md) § Grammar bump complete
- HANDOFF doc stale `symforge-perl` reference removed
- Investigation doc final with all SC-* metrics

---

## Grammar bump checklist (maintainers)

When `ts-parser-perl` version changes in `Cargo.lock`:

1. `cargo update -p ts-parser-perl` (or intentional pin bump in Cargo.toml)
2. `cargo test probe_perl_grammar_sexp --lib --features server -- --ignored --nocapture`
3. Diff output vs [contracts/perl-node-shapes.md](./contracts/perl-node-shapes.md)
4. If changed: update contract + `PERL_XREF_QUERY` + `perl.rs` + fixtures
5. Run **S0 Gate** command block
6. Run **S2 Gate** if xref touched
7. Update `docs/research/perl/corpus-metrics.json` measured_at

---

## SymForge MCP planning commands

```text
explore(query="Perl parsing xref", path_prefix="src/parsing/")
diff_symbols(base="30dd4c3", target="9572b31", path_prefix="src/parsing/")
get_symbol(name="PERL_XREF_QUERY", path="src/parsing/xref.rs")
get_file_context(path="src/parsing/languages/perl.rs", sections=["outline"])
analyze_file_impact(path="src/parsing/xref.rs")   # post-edit only
```

---

## Release Gate (program complete)

```powershell
cargo fmt --check
cargo check --features server
cargo clippy --all-targets --features server -- -D warnings
cargo test --features server --all-targets -- --test-threads=1
cargo build --release --features server
```

Then `/speckit-converge` — zero new tasks appended.
