# Feature Specification: Perl Parser Hardening (Post ts-parser-perl Merge)

**Feature Branch**: `016-perl-parser-hardening`

**Created**: 2026-07-06

**Status**: S0–S2 complete on branch — S3 ops/doc polish remaining

**Baseline**: `main` @ **`9572b31`** — grammar swap merged (#341 closed); this program completes evidence, corpus, recall targets, and bump protocol.

**Input**: Reproducible Perl parse-quality baseline, xref/extractor recall on an in-repo fixture corpus, grammar bump protocol, and investigation doc — after `ts-parser-perl` merge on main.

**Speckit**: specify ✓ · plan ✓ · tasks ✓ · analyze ✓ · implement → PROG + S0 next

**Related**: GitHub issue #341 (closed), `docs/dart-parser-investigation.md` (template), `docs/semantic-tier-roadmap.md` (Perl Tier-0 only).

## Program goal

Make SymForge's Perl support **trustworthy and measurable** for agents indexing real Perl codebases — not merely "tests pass on snippets." The ts-parser-perl swap (`9572b31`) is **Phase 0 shipped**; this program proves value, closes xref gaps corpus-driven, and establishes operational discipline for grammar bumps.

**North Star**: An operator or agent can answer "how good is Perl indexing here?" from committed artifacts without re-running external benchmarks or guessing from issue comments.

## Context

### Already on main (`9572b31`)

| Surface | State |
|---------|--------|
| `Cargo.toml` | `tree-sitter-perl = { package = "ts-parser-perl", version = "1.1" }` → resolves to **1.1.3** |
| `src/parsing/languages/perl.rs` | Extractor for `subroutine_declaration_statement`, `method_declaration_statement`, `class_statement`, `package_statement`; `find_name` via `name:` field |
| `src/parsing/xref.rs` | `PERL_XREF_QUERY` rewritten; **`compile_xref_query`** degrades all 21 langs to empty refs instead of panic |
| Tests | 6 Perl unit tests + 1 ignored sexp probe + `test_perl_grammar_loads_and_parses` |

### Known gaps (SymForge evidence, 2026-07-06)

- No in-repo fixture corpus; #341 benchmark (~95% vs ~40%) not reproducible locally
- `PERL_XREF_QUERY` covers method/plain/ambiguous calls + use/require only — qualified `Foo::bar()`, `SUPER::`, etc. untested
- `probe_perl_grammar_sexp` is `#[ignore]` — no CI/bump checklist
- No `docs/perl-parser-investigation.md` (Dart analogue exists)
- C++ `@ref.qualified_call` (D13) must not regress when extending xref (verified at merge; needs regression lock in corpus)

## Clarifications

### Session 2026-07-06

Planning-phase resolutions (encoded before analyze/implement):

- Q: Full 8k-file CPAN benchmark in CI? → **A: No** — optional `#[ignore]` smoke only; committed fixture corpus (≥20 files) is the authoritative gate.
- Q: Re-open #341? → **A: No** — track residual work in this spec; reference #341 in investigation doc only.
- Q: SCIP / semantic tier for Perl? → **A: Out of scope** — Tier-0 tree-sitter only per semantic-tier-roadmap.
- Q: Collapse OnceLock query getters to table? → **A: No** — xref.rs ponytail rejection stands (robustness-by-construction).
- Q: Worktree `symforge-perl`? → **A: Retired** — single repo branch `016-perl-parser-hardening` on `E:/project/symforge`.

## Sprint Map

| Sprint | Theme | User stories | Target |
|--------|-------|--------------|--------|
| **PROG** | Program bootstrap | — | Planning gate |
| **S0** | Retrofit audit | US0 | Gate: merge safe |
| **S1** | Evidence + corpus | US1 | Baseline metrics |
| **S2** | Coverage expansion | US2 | Recall targets |
| **S3** | Operational hardening | US3–US4 | Bump protocol + docs |

Detailed tasks: [tasks.md](./tasks.md). Calendar: [sprints.md](./sprints.md).

## User Scenarios & Testing *(mandatory)*

### User Story 0 — Retrofit audit (Priority: P0) — Sprint S0

An engineer verifying the merged swap needs proof that `9572b31` is safe on current main before expanding Perl coverage.

**Why this priority**: Expanding on a shaky merge wastes effort; C++ xref regression is the highest-risk neighbor.

**Independent Test**: Run S0 validation bundle from [quickstart.md](./quickstart.md) § S0; all checks pass; evidence rows stamped in `planning/code-evidence.md`.

**Acceptance Scenarios**:

1. **Given** `main` @ `9572b31`, **When** full cargo gate runs, **Then** fmt/check/clippy/lib/all-targets/release pass.
2. **Given** `probe_perl_grammar_sexp` with `--ignored`, **When** executed, **Then** sexp output matches [contracts/perl-node-shapes.md](./contracts/perl-node-shapes.md).
3. **Given** `test_cpp_qualified_call_retains_head`, **When** run after any S2 xref edit, **Then** test stays green (D13 neighbor lock).

---

### User Story 1 — Reproducible parse baseline (Priority: P1) — Sprint S1

An agent or operator indexing a Perl repo needs confidence that parse failures are rare and measured — not marketing claims from #341.

**Why this priority**: Coverage stat drives symbol discovery; unmeasured claims erode trust.

**Independent Test**: Run corpus bench (ignored or script); investigation doc reports clean-parse % on `tests/fixtures/perl/`.

**Acceptance Scenarios**:

1. **Given** fixture corpus ≥20 tagged `.pl` files, **When** bench runs, **Then** clean-parse % and ERROR-node count are recorded in `docs/perl-parser-investigation.md`.
2. **Given** each fixture tagged by construct class, **When** taxonomy applied, **Then** failures bucket as parse ERROR vs extractor miss vs xref miss.
3. **Given** investigation doc, **When** read, **Then** node-shape table traces to sexp probe output (not guessed from issue text).

---

### User Story 2 — Xref and extractor recall (Priority: P1) — Sprint S2

An agent calling `find_references` on Perl symbols needs call sites and imports recovered on modern Perl constructs (class/method, qualified calls).

**Why this priority**: Xref is the commitment signal for navigation; silent misses break agent workflows.

**Independent Test**: Corpus recall suite: for each P1 construct in taxonomy, ≥1 fixture asserts expected refs/symbols.

**Acceptance Scenarios**:

1. **Given** `$obj->method()` and `helper()` in fixture, **When** xref runs, **Then** Call refs for `method` and `helper` present (regression lock for existing tests).
2. **Given** `class Point { method render { $self->draw(); } }`, **When** indexed, **Then** Module `Point`, Function `render`, Call `draw` all present.
3. **Given** qualified call construct proven in corpus (e.g. `Foo::bar()`), **When** S2 complete, **Then** Call ref captures leaf name with qualified_name when applicable (mirror Java/C++ import rules).
4. **Given** construct classified **accepted loss** in taxonomy, **When** documented, **Then** investigation doc states refusal loudly — no silent gap.

**Recall targets (S2 exit)** — measured on P1 fixture subset:

| Construct class | Symbol extract | Xref recall |
|-----------------|----------------|-------------|
| sub / package | 100% fixtures | n/a |
| class / method | 100% fixtures | method calls inside body |
| plain + method calls | n/a | 100% fixtures |
| use / require | n/a | 100% fixtures |
| qualified `::` calls | ≥80% P1 fixtures | ≥80% P1 fixtures |

---

### User Story 3 — Grammar bump protocol (Priority: P2) — Sprint S3

A maintainer bumping `ts-parser-perl` needs a checklist that prevents query/extractor drift.

**Why this priority**: Package rename hides grammar swaps; node renames panic or silently drop refs without probe.

**Independent Test**: Follow [quickstart.md](./quickstart.md) § Grammar bump; all steps complete in order.

**Acceptance Scenarios**:

1. **Given** `Cargo.lock` ts-parser-perl version change, **When** maintainer follows quickstart, **Then** sexp probe re-run and diffed against contract.
2. **Given** probe diff non-empty, **When** contract updated, **Then** matching query/extractor edits land in same PR.
3. **Given** bump PR, **When** CI/manual hook runs, **Then** Perl test bundle from quickstart executes.

---

### User Story 4 — Living investigation doc (Priority: P3) — Sprint S3

Future agents and operators need a single doc explaining Perl indexing behavior, limits, and evidence — like Dart.

**Independent Test**: Doc exists, links fixtures, contracts, and measured numbers; HANDOFF stale reference removed.

**Acceptance Scenarios**:

1. **Given** `docs/perl-parser-investigation.md`, **When** read, **Then** structure mirrors dart investigation (summary, evidence, symforge context, failure classes, final answers).
2. **Given** S1–S2 complete, **When** doc updated, **Then** all SC metrics from this spec appear with measurement date.

### Edge Cases

- **Grammar/query mismatch**: `compile_xref_query` returns None; Perl files index symbols but emit zero xref refs — MUST log `tracing::warn!` once (contract in [contracts/compile-xref-degradation.md](./contracts/compile-xref-degradation.md)).
- **Partial parse (ERROR nodes)**: `FileOutcome::PartialParse` — symbols in clean subtrees still extracted; fixture taxonomy records partial vs clean.
- **Legacy ganezdragon node kinds**: Extractor fallbacks remain harmless; no requirement to test ganezdragon grammar.
- **Dynamic/indirect calls**: Classified accepted loss unless corpus proves otherwise — no speculative query rules.
- **Windows paths**: Fixture paths repo-relative; no `\` in fixture names.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Program MUST treat `9572b31` as Phase 0 complete; S0 validates before S1 coding.
- **FR-002**: MUST maintain committed fixture corpus under `tests/fixtures/perl/` with construct tags and expected outcomes JSON or inline tests.
- **FR-003**: MUST publish measured clean-parse % on fixture corpus in investigation doc (not issue comment alone).
- **FR-004**: MUST extend Perl coverage only for construct classes proven missing in S1 taxonomy (corpus-driven — no speculative xref rules).
- **FR-005**: MUST preserve C++ `@ref.qualified_call` behavior through S2 (regression test in every S2 validation wave).
- **FR-006**: MUST document grammar bump protocol in `quickstart.md` with sexp probe as mandatory step.
- **FR-007**: MUST keep `compile_xref_query` panic-free contract for all languages (extend tests only if contract gaps found).
- **FR-008**: MUST NOT introduce second index or external parser process for Perl (Constitution I).
- **FR-009**: Discovery tools (`search_*`, `explore`, `ask`) MUST NOT gain frecency from corpus bench runs (Constitution V).

### Key Entities

- **PerlFixture**: Tagged `.pl` snippet + metadata (construct class, source attribution, expected symbols/refs).
- **FailureBucket**: Enum — `ParseError`, `ExtractorMiss`, `XrefMiss`, `AcceptedLoss`.
- **NodeShapeContract**: Mapping from construct → tree-sitter sexp pattern (from probe, not docs).
- **CorpusMetrics**: `{ clean_parse_pct, error_count, fixture_count, measured_at, symforge_version }`.
- **RecallMetrics**: Per construct-class pass rates on P1 fixture subset.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Fixture corpus ≥20 files committed with construct tags before S2 starts.
- **SC-002**: Investigation doc reports clean-parse % ≥90% on fixture corpus (target; if lower, doc explains failure classes — no silent pass).
- **SC-003**: S2 P1 construct recall targets in US2 table met or explicitly downgraded with accepted-loss documentation.
- **SC-004**: Full cargo gate green at each sprint Release Gate ([quickstart.md](./quickstart.md)).
- **SC-005**: Grammar bump checklist exists and was dry-run validated on current 1.1.3 baseline.
- **SC-006**: Zero `[NEEDS CLARIFICATION]` markers; `/speckit-analyze` reports 0 CRITICAL.

## Explicit Exclusions

- SCIP / LSP / semantic tier for Perl
- OnceLock query-table collapse in xref.rs
- External 8k-file benchmark as CI gate
- MCP surface changes (Perl is parsing-layer only)
- Reverting to ganezdragon grammar

## Assumptions

- `ts-parser-perl` 1.1.x remains the pinned lineage; bumps are patch/minor within 1.x unless research proves breaking node renames.
- SymForge MCP on `E:/project/symforge` is available for code-backed planning evidence.
- Agents consuming Perl indexing are the primary "users" — success is measured by indexing quality, not end-user UI.
