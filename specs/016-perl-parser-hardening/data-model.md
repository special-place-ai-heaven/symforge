# Data Model: Perl Parser Hardening

**Feature**: 016 · **Date**: 2026-07-06

## Entities

### PerlFixture

A single committed test artifact representing real-world Perl patterns.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Stable slug e.g. `class-method-call` |
| `path` | string | yes | Repo-relative `tests/fixtures/perl/{id}.pl` |
| `construct_classes` | string[] | yes | Tags from taxonomy below |
| `source_note` | string | no | Provenance (e.g. "Mojolicious-style", "minimal") |
| `expect_symbols` | SymbolExpect[] | no | Expected extractor output |
| `expect_refs` | RefExpect[] | no | Expected xref output |
| `parse_expect` | enum | yes | `Clean` \| `PartialParse` \| `Error` |

### SymbolExpect

| Field | Type | Description |
|-------|------|-------------|
| `kind` | `Function` \| `Module` \| … | SymbolKind |
| `name` | string | Symbol name |

### RefExpect

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Leaf ref name |
| `kind` | `Call` \| `Import` \| … | ReferenceKind |
| `qualified_name` | string? | Full path when applicable |

### FailureBucket

| Value | Meaning |
|-------|---------|
| `ParseError` | tree-sitter ERROR/MISSING nodes prevent traversal |
| `ExtractorMiss` | Parse clean but symbol not extracted |
| `XrefMiss` | Symbols ok but expected ref absent |
| `AcceptedLoss` | Known limitation — documented, not a defect |

### ConstructClass (taxonomy tags)

```
sub
package
class
method
method_call
plain_call
ambiguous_call
use_import
require_import
qualified_call
super_call
role
attribute
dynamic_call
```

### NodeShapeContract

Immutable mapping maintained in [contracts/perl-node-shapes.md](./contracts/perl-node-shapes.md);
updated only when sexp probe output changes on grammar bump.

### CorpusMetrics

```json
{
  "fixture_count": 0,
  "clean_parse_count": 0,
  "clean_parse_pct": 0.0,
  "partial_parse_count": 0,
  "error_count": 0,
  "measured_at": "ISO-8601",
  "symforge_version": "8.10.7",
  "grammar_version": "ts-parser-perl 1.1.3"
}
```

Stored at `docs/research/perl/corpus-metrics.json` after S1 `[V]`.

### RecallMetrics

Per construct class:

```json
{
  "construct_class": "qualified_call",
  "symbol_recall_pct": null,
  "xref_recall_pct": null,
  "fixture_count": 0,
  "accepted_loss": false
}
```

Stored at `docs/research/perl/recall-metrics.json` after S2 `[V]`.

## Relationships

```text
PerlFixture ──tags──▶ ConstructClass
PerlFixture ──eval──▶ FailureBucket (when expectation not met)
CorpusMetrics ──aggregates──▶ PerlFixture.parse_expect
RecallMetrics ──aggregates──▶ PerlFixture expect_* per ConstructClass
NodeShapeContract ──validates──▶ PERL_XREF_QUERY + perl.rs walk_node kinds
```

## State transitions (S1 taxonomy workflow)

```text
unclassified fixture
  → parse run → ParseError | clean tree
  → symbol extract → ExtractorMiss | ok
  → xref extract → XrefMiss | ok | AcceptedLoss (explicit)
```

## File layout

```text
tests/fixtures/perl/
├── README.md              # tagging rules
├── sub-greet.pl
├── package-module.pl
├── class-point-method.pl
├── ...
docs/research/perl/
├── corpus-metrics.json
├── recall-metrics.json
└── taxonomy-signoff.md    # S1 gate artifact
```
