# CPAN-scale Perl parsing benchmark — SymForge

**Measured**: 2026-07-06 · **Binary**: shipped `symforge.exe` 8.11.1 (npm `symforge-windows-x64`) · **Grammar**: `ts-parser-perl 1.1.3`

Extends the 22-fixture synthetic corpus (100% clean, `corpus-metrics.json`) to
real, large, mostly-pure-Perl CPAN distributions. This is a **local research
benchmark**, not CI.

## Setup

| Repo | URL | Commit |
|------|-----|--------|
| Mojolicious | github.com/mojolicious/mojo | `6007271e9152aa0998129991030c3ed16eb407d0` |
| Perl-Critic | github.com/Perl-Critic/Perl-Critic | `c437d557ec903c99c5114bac738a484dfedb1f9f` |

Both shallow-cloned (`--depth 1`).

## Method — and what the tooling can/can't measure

The shipped CLI has **no standalone `scan`/`index` command**; indexing runs
through the MCP `index_folder` tool. Per-file parse outcomes come from the
`health` **quarantine registry**, which enumerates every partial/failed file
with its tree-sitter diagnostic and byte span.

Two honesty caveats about the proxy:

1. **Denominator = Tier-1 Perl files.** SymForge admits `.t` test files only at
   **Tier-2 (metadata-only)** — they are *not* symbol-parsed — so they are
   excluded from the parse-rate denominator. The Perl file count used here is
   the `Perl:` figure from `get_repo_map`, which counts the `.pm`/`.pl` files
   that were actually parsed. (Mojo: 274 Perl files on disk, but 110 are `.t`
   held at Tier-2; 164 Perl files reach the parser. Perl-Critic: 255 on disk,
   50 `.t` at Tier-2; 205 reach the parser.)
2. **No reference/xref total.** `health` reports index-wide **symbol** counts but
   **no aggregate reference count**. Per-file "Used by N refs" is available via
   `get_file_context`, but there is no total-refs command, so **total references
   were not measured** (reported as `null`).

Wall-clock is the daemon's own `Loaded in` figure (parse + index of Tier-1
files), not end-to-end clone/IO time.

## Results

| Repo | Perl files parsed | Clean | Partial | Failed | Clean % (raw) | Symbols | Load |
|------|------------------:|------:|--------:|-------:|--------------:|--------:|-----:|
| Mojolicious | 164 | 158 | 6 | 0 | 96.3% | 6650 | 78 ms |
| Perl-Critic | 205 | 205 | 0 | 0 | 100.0% | 1761 | 45 ms |
| **Total** | **369** | **363** | **6** | **0** | **98.4%** | **8411** | — |

Of Mojo's 6 partials, **3 are intentionally-malformed test fixtures** that
SymForge correctly flags (true positives). Excluding those, the real-code clean
rate is **161/164 = 98.2%** for Mojo and **99.2% across both repos**. There were
**zero Perl parse failures** — the only two "failed" files are non-Perl data
fixtures (`.json`/`.yml`) that happened to live in the tree.

## Notable failures (one-line diagnosis each)

Genuine `ts-parser-perl 1.1.3` grammar gaps (all three still recovered most
symbols via best-effort partial parse):

- **`lib/Mojo/JSON.pm`** L224 — `return true() if /\Gtrue/gc;` — user-defined
  bareword sub call (`true()`) as a statement-modifier expression trips the
  grammar. 18/18 symbols still recovered.
- **`lib/Mojo/Path.pm`** L16 — chained `if/elsif/else` whose bodies are block-form
  `{ splice ... }` with no trailing `;`; grammar reports "syntax missing ;".
  15 symbols recovered.
- **`lib/Mojo/Exception.pm`** L25 — `CHECK: for (my $i = 0; ...)` — a **statement
  label on a C-style for loop**; grammar errors on the label. 10 symbols
  recovered.

Correctly-flagged intentional fixtures (not parser weaknesses):

- `t/mojolicious/lib/MojoliciousTest/SyntaxError.pm` — unclosed `sub foo {`.
- `t/mojo/lib/Mojo/LoaderException.pm` — bare unclosed `foo {` block.
- `t/mojo/lib/Mojo/LoaderTestException/A.pm` — bare unclosed `foo {` block.

Non-Perl noise caught in the sweep (excluded from Perl rates): 2 CSS partials
(`bootstrap.css`, `mojo.css`) and the 2 `.json`/`.yml` "failed" data files above.

## Caveats

- Parse-rate is **file-level**, not construct-level: a "clean" file means no
  tree-sitter ERROR/MISSING node, not that every xref was attached.
- `.t` files are unmeasured here (Tier-2). A separate run forcing `.t` into
  Tier-1 would be needed to characterize test-file parsing at CPAN scale.
- Reference/xref recall was not quantified (no aggregate-count tool); see the
  fixture-corpus `recall-metrics.json` for construct-level xref recall on the
  synthetic corpus.
- Two repos only; not a statistically representative CPAN sample. The three
  grammar gaps found are candidate S2 backlog items if reproduced on the
  synthetic corpus.
