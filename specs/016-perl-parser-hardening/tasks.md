# Tasks: Perl Parser Hardening — 55% Planning · 35% Coding · 10% Validation

**Program**: 016 · **Model**: [execution-model.md](./execution-model.md)

**Hard rule**: No `[C]` until linked `[P]` done + sprint Planning Gate sign-off.

**Code-backed rule**: Every `[P]` task ends with an `EV-*` row in [planning/code-evidence.md](./planning/code-evidence.md).

**Totals**: **~77 tasks** — ~42 `[P]` · ~27 `[C]` · ~8 `[V]`

**Planning complete**: PROG + analyze (2026-07-06). See [planning/program-planning-gate.md](./planning/program-planning-gate.md), [analyze.md](./analyze.md).

**Baseline**: `main` @ `9572b31` — Phase 0 merge shipped; S0 validates.

---

## Phase PROG — Program bootstrap

**Spec**: [planning/program-planning-gate.md](./planning/program-planning-gate.md)

### [P] Program-wide

- [ ] P-PROG-001 Read execution-model.md + planning/README.md; confirm 55/35/10 gates.
- [ ] P-PROG-002 [P] Review [risk-register.md](./planning/risk-register.md); confirm owners.
- [ ] P-PROG-003 Read #341 issue body + [research.md](./research.md); note benchmark vs fixture scope in decision-log D-016-001.
- [ ] P-PROG-004 SymForge MCP `explore` Perl parsing cluster → bootstrap **EV-PROG-001** in code-evidence.md.
- [ ] P-PROG-005 SymForge `diff_symbols` `30dd4c3...9572b31` path `src/parsing/` → **EV-PROG-002**.
- [ ] P-PROG-006 Branch `016-perl-parser-hardening`; `.specify/feature.json` → 016.
- [ ] P-PROG-007 [P] Review constitution.md I–VIII; sign [checklists/requirements.md](./checklists/requirements.md).
- [ ] P-PROG-008 [P] Read `docs/dart-parser-investigation.md` outline via SymForge `get_file_context`; note template sections for US4.
- [ ] P-PROG-009 Freeze [contracts/perl-node-shapes.md](./contracts/perl-node-shapes.md) baseline from sexp probe output.

**STOP** — PROG Planning Gate before S0 `[V]`.

---

## Sprint S0 — Retrofit audit (US0)

**Spec**: [planning/sprint-0-retrofit-audit-spec.md](./planning/sprint-0-retrofit-audit-spec.md)

### Wave 1 — [P] audit design

- [ ] P-S0-001 SymForge dispatch map: all `LanguageId::Perl` sites → **EV-S0-001** (6 files).
- [ ] P-S0-002 Document merge symbol diff in sprint-0 spec Baseline table → **EV-S0-002**.
- [ ] P-S0-003 Map C++ `qualified_call` hunks as D13 neighbor lock → **EV-S0-003**.
- [ ] P-S0-004 Freeze [contracts/compile-xref-degradation.md](./contracts/compile-xref-degradation.md) review checklist.
- [ ] P-S0-005 Acceptance matrix S0 rows in [planning/acceptance-matrix.md](./planning/acceptance-matrix.md).

**STOP** — S0 Planning Gate (checklist in sprint-0 spec).

### Wave 2 — [V] only (no [C] unless failure)

- [ ] V-S0-001 Run [quickstart.md](./quickstart.md) § S0 Gate command block; record results in sprint-0 spec.
- [ ] V-S0-002 Archive sexp probe output → `docs/research/perl/sexp-baseline-2026-07-06.txt`.
- [ ] V-S0-003 Confirm `test_cpp_qualified_call_retains_head` green — sign acceptance matrix.

### Wave 3 — [C] hotfix (ONLY if V-S0 fails)

- [ ] C-S0-001 Fix gate failure on `src/parsing/*` or `Cargo.lock` — minimal diff; re-run V-S0.

---

## Sprint S1 — Evidence + corpus (US1)

**Spec**: [planning/sprint-1-evidence-corpus-spec.md](./planning/sprint-1-evidence-corpus-spec.md)

### Wave 1 — [P] taxonomy + fixture design

- [ ] P-S1-001 Define construct taxonomy in [data-model.md](./data-model.md) § ConstructClass — sign-off template.
- [ ] P-S1-002 [P] Source ≥25 candidate snippets (CPAN-style, Mojolicious, DBIx, minimal); rank top 20+ for commit.
- [ ] P-S1-003 [P] Fixture README schema `tests/fixtures/perl/README.md` design.
- [ ] P-S1-004 Failure bucket decision tree for taxonomy reviewers.
- [ ] P-S1-005 Investigation doc outline (mirror dart sections) — section headers only in P-S1-005 artifact.
- [ ] P-S1-006 Classify qualified_call / SUPER / dynamic in taxonomy — mark P1 vs accepted-loss.

**STOP** — mini-gate: taxonomy complete?

### Wave 2 — [P] contracts + metrics

- [ ] P-S1-007 Design `tests/perl_corpus.rs` structure (table-driven vs per-file tests).
- [ ] P-S1-008 Design `corpus-metrics.json` schema validation approach.
- [ ] P-S1-009 [P] SymForge `get_symbol` on `process_file` path for fixture harness → **EV-S1-001**.

**STOP** — S1 Planning Gate.

### Wave 3 — [C] fixtures + harness

- [ ] C-S1-001 Create `tests/fixtures/perl/README.md` with tagging rules.
- [ ] C-S1-002 [P] Add core fixtures: sub, package, class/method (≥5 files).
- [ ] C-S1-003 [P] Add call/import fixtures: method, plain, ambiguous, use, require (≥8 files).
- [ ] C-S1-004 [P] Add edge/partial fixtures per taxonomy (≥7 files) — total ≥20.
- [ ] C-S1-005 Implement `tests/perl_corpus.rs` parse + symbol expectations.
- [ ] C-S1-006 Implement ignored `bench_corpus_parse_metrics` writing metrics JSON.
- [ ] C-S1-007 Draft `docs/perl-parser-investigation.md` — summary + evidence sections.

### Wave 4 — [V]

- [ ] V-S1-001 Run S1 Gate ([quickstart.md](./quickstart.md) § S1); SC-001 SC-002 recorded.
- [ ] V-S1-002 Taxonomy sign-off `docs/research/perl/taxonomy-signoff.md` completed.
- [ ] V-S1-003 `/speckit-converge` optional after S1 if gaps found.

---

## Sprint S2 — Coverage expansion (US2)

**Spec**: [planning/sprint-2-coverage-expansion-spec.md](./planning/sprint-2-coverage-expansion-spec.md)

**Prerequisite**: S1 taxonomy sign-off + corpus metrics.

### Wave 1 — [P] per construct class (from taxonomy misses)

- [ ] P-S2-001 List xref/extractor misses from S1 bench → prioritized backlog in sprint-2 spec.
- [ ] P-S2-002 [P] For each P1 miss: sexp probe snippet + intended query capture → **EV-S2-00N**.
- [ ] P-S2-003 Update [contracts/perl-xref-recall.md](./contracts/perl-xref-recall.md) with proven shapes only.
- [ ] P-S2-004 File-touch matrix S2 delta in [planning/file-touch-matrix.md](./planning/file-touch-matrix.md).

**STOP** — S2 Planning Gate.

### Wave 2 — [C] construct waves (max 6 per session)

#### Wave 2a — qualified calls (if taxonomy P1)

- [ ] C-S2-001 Add fixtures for `Foo::bar()` / `$pkg->method()` per P-S2-002.
- [ ] C-S2-002 Extend `PERL_XREF_QUERY` + tests in `xref.rs`.
- [ ] C-S2-003 Verify `push_import_reference` / qualified_name behavior for calls if needed.

**STOP** — run S2 partial gate + C++ regression.

#### Wave 2b — remaining P1 classes (repeat pattern)

- [ ] C-S2-004 [P] Extractor gaps from taxonomy (role/attribute) — only if sexp proves nodes.
- [ ] C-S2-005 [P] Xref gaps: SUPER/CORE — only if taxonomy P1.
- [ ] C-S2-006 Document accepted-loss constructs in investigation doc § Limits.

### Wave 3 — [V]

- [ ] V-S2-001 Run S2 Gate; populate `docs/research/perl/recall-metrics.json`.
- [ ] V-S2-002 Freeze [contracts/perl-xref-recall.md](./contracts/perl-xref-recall.md).
- [ ] V-S2-003 US2 recall table sign-off in acceptance matrix.

---

## Sprint S3 — Operational hardening (US3–US4)

**Spec**: [planning/sprint-3-operational-spec.md](./planning/sprint-3-operational-spec.md)

### Wave 1 — [P]

- [ ] P-S3-001 Dry-run grammar bump checklist ([quickstart.md](./quickstart.md) § Grammar bump).
- [ ] P-S3-002 [P] Decide CI vs manual-only for probe ([risk-register.md](./planning/risk-register.md) R-016-003).
- [ ] P-S3-003 HANDOFF doc edit plan for stale symforge-perl reference.

**STOP** — S3 Planning Gate.

### Wave 2 — [C]

- [ ] C-S3-001 Finalize `docs/perl-parser-investigation.md` with all SC-* metrics.
- [ ] C-S3-002 Add optional `.github` or `CONTRIBUTING` pointer to grammar bump checklist (minimal).
- [ ] C-S3-003 Fix HANDOFF stale worktree line in `docs/reviews/HANDOFF-symforge-trust-campaign.md`.
- [ ] C-S3-004 [P] Pin note in Cargo.toml comment for ts-parser-perl bump policy.

### Wave 3 — [V]

- [ ] V-S3-001 Full Release Gate ([quickstart.md](./quickstart.md) § Release Gate).
- [ ] V-S3-002 `/speckit-converge` — zero new tasks.
- [ ] V-S3-003 PR to main with release-please body guard per CLAUDE.md.

---

## Dependencies

```text
PROG → S0 → S1 → S2 → S3
P-S1-* blocks all C-S2-*
P-S2-* blocks all C-S2-*
S1 taxonomy sign-off blocks S2 Planning Gate
```

## Parallel opportunities

- C-S1-002, C-S1-003, C-S1-004 (different fixture files) — parallel
- P-S1-002 snippet sourcing || P-S1-005 doc outline
- S3 doc work can draft during S2 but metrics final waits V-S2-001

## Implementation strategy

1. **MVP**: PROG + S0 + S1 → measurable baseline (US1 alone delivers value)
2. **Recall**: S2 on taxonomy misses only
3. **Ops**: S3 closes program

Stop at any sprint Release Gate to merge incremental value.
