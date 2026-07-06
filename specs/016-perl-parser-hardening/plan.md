# Implementation Plan: Perl Parser Hardening

**Branch**: `016-perl-parser-hardening` | **Date**: 2026-07-06 | **Spec**: [spec.md](./spec.md)

**Execution model**: [execution-model.md](./execution-model.md) — **55% planning · 35% coding · 10% validation**

**Task list**: [tasks.md](./tasks.md) — `[P]` → `[C]` → `[V]` per sprint; no `[C]` until Planning Gate.

**Planning artifacts**: [planning/README.md](./planning/README.md) · **Code evidence**: [planning/code-evidence.md](./planning/code-evidence.md)

## Summary

Complete the Perl indexing story after the ts-parser-perl merge (`9572b31`): validate the
retrofit, build an in-repo fixture corpus with measured parse-quality metrics, expand
extractor/xref coverage corpus-driven, and ship a grammar bump protocol. SymForge MCP
is mandatory for code-backed planning; Dart investigation doc is the documentation template.

## Technical Context

**Language/Version**: Rust 2024, single crate `symforge` v8.10.7+.

**Primary Dependencies**: Existing only — `ts-parser-perl` (Cargo alias `tree-sitter-perl`),
tree-sitter query API, existing `parsing/` module graph.

**Storage**: No new persistence. Fixtures under `tests/fixtures/perl/`; research artifacts
under `docs/research/perl/` and `docs/perl-parser-investigation.md`.

**Testing**:
- Unit: `src/parsing/languages/perl.rs`, `src/parsing/xref.rs` (existing + new)
- Integration: `tests/tree_sitter_grammars.rs`, new `tests/perl_corpus.rs` (optional ignored bench)
- Gate: `cargo fmt --check`, `check`, `clippy --all-targets -D warnings`,
  `test --all-targets --test-threads=1`, `build --release`

**Target Platform**: Windows/Linux/macOS; Perl indexing path identical stdio/daemon/embed read path.

**Performance Goals**: Corpus bench completes in <60s on dev machine (ignored test); no query-path regression.

**Constraints**: Constitution I–VIII; no new dependencies unless bench script requires none;
xref OnceLock explicit getters preserved.

**Scale/Scope**: ~4 files core touch (`perl.rs`, `xref.rs`, `Cargo.toml`, `Cargo.lock`);
+ fixtures + docs; ~60 tasks across 4 sprints.

## Constitution Check

| Principle | Verdict | Note |
|-----------|---------|------|
| I. Local-First Index | **PASS** | Perl parsing in-process; no external perl interpreter |
| II. MCP-Native | **PASS** | No MCP surface changes; indexing quality only |
| III. Trust Envelopes | **PASS** | Accepted-loss constructs documented loudly in investigation doc |
| IV. Determinism & Recovery | **PASS** | `compile_xref_query` improves degradation vs panic |
| V. Frecency | **PASS** | Corpus bench/tests must not bump frecency; bench uses lib API not MCP discovery tools |
| VI. Embed | **PASS** | Same parsing path for embed consumers |
| VII. Transport Parity | **PASS** | N/A — no transport change |
| VIII. Verification | **PASS** | Full gate per sprint in quickstart.md |

No unjustified violations.

## Project Structure

### Documentation

```text
specs/016-perl-parser-hardening/
├── spec.md
├── plan.md                    # this file
├── execution-model.md
├── sprints.md
├── research.md
├── data-model.md
├── quickstart.md
├── tasks.md
├── analyze.md
├── checklists/requirements.md
├── contracts/
│   ├── perl-node-shapes.md
│   ├── perl-xref-recall.md
│   └── compile-xref-degradation.md
└── planning/
    ├── README.md
    ├── BRANCH.md
    ├── program-planning-gate.md
    ├── code-evidence.md
    ├── acceptance-matrix.md
    ├── risk-register.md
    ├── file-touch-matrix.md
    ├── decision-log.md
    ├── sprint-0-retrofit-audit-spec.md
    ├── sprint-1-evidence-corpus-spec.md
    ├── sprint-2-coverage-expansion-spec.md
    └── sprint-3-operational-spec.md
```

### Source (touch matrix)

```text
Cargo.toml                              # version pin notes only (S3)
Cargo.lock                              # grammar bump only
src/parsing/languages/perl.rs           # S2 extractor extensions
src/parsing/xref.rs                     # S2 PERL_XREF_QUERY; S0/S3 probe
src/parsing/mod.rs                      # read-only verify dispatch
src/parsing/languages/mod.rs            # read-only verify dispatch
tests/fixtures/perl/                    # S1 new
tests/perl_corpus.rs                    # S1 new (ignored bench)
tests/tree_sitter_grammars.rs           # S0 verify
docs/perl-parser-investigation.md       # S1–S3
docs/research/perl/                     # S1 metrics JSON
docs/reviews/HANDOFF-symforge-trust-campaign.md  # S3 stale ref fix
```

**Structure Decision**: Single-crate; no new modules unless S2 taxonomy forces a tiny
`tests/perl_corpus.rs` helper — prefer inline test modules over new `src/` modules.

## Phase 0 Research Output

See [research.md](./research.md) — node shapes verified via sexp probe; #341 claims mapped
to in-repo measurable substitutes; Dart doc structure adopted.

## Phase 1 Design Output

- [data-model.md](./data-model.md) — fixture + metrics entities
- [contracts/](./contracts/) — node shapes, xref recall, compile degradation
- [quickstart.md](./quickstart.md) — S0–S3 gates + grammar bump

## Complexity Tracking

> No constitution violations requiring justification.

| Item | Decision | Rationale |
|------|----------|-----------|
| Ignored corpus bench | Accepted | Full CPAN bench too heavy for CI; fixtures are gate |
| No new crate deps | Accepted | tree-sitter + existing test harness sufficient |

## Next Steps

1. Complete PROG `[P]` tasks in [tasks.md](./tasks.md)
2. S0 Planning Gate → S0 `[V]` only (merge already shipped — no S0 `[C]` unless gate fails)
3. S1 evidence layer before any S2 xref edits
