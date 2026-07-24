# Gate M + AAP — Adversarial Review Brief (Cursor + Kimi)

Adversarial review of SymForge Feature 020's **Gate M** (health/surface/corpus/embed) and
the **AAP tool-contract fixes**, branch `feat/repository-knowledge-index` (HEAD at review
time near `0ff1ec3`), repo `E:\project\symforge` (Rust MCP server). Gate L (22/22) was
already reviewed across three cross-model rounds — this pass is the Gate M + AAP surface.

**Read `specs/020-repository-knowledge-index/GATE-L-REVIEW.md` first and apply its MANDATORY
8-point methodology verbatim** (full-body reads; trace shared-helper arg usage; parity diffs;
concrete named inputs; explicit interleavings; consumer-trace before severity; attack the
tests; fail-open audit). Same two jobs: **find defects AND suggest improvements**, each with a
**concrete proposed fix**. **READ-ONLY** — propose in your report, do not edit code.

## The review surface (commits)

- `883d997` — AAP blockers: `edit_plan` literal-path precedence; `get_file_content` concurrency
  (already-satisfied + regression coverage); `analyze_file_impact` non-parser false-absence.
- `1f52606` — M-001 health knowledge fields + M-002 assertions.
- `9ee4df0` — M-001 review fixes (authorization from `MemoryOnly.failures`, gitignore-hygiene,
  live_postcondition, target rendering, unbound closed-set) + M-003/005/006/007.

## Verify-these-are-correct (the highest-value targets)

1. **AAP-001 `edit_plan` literal-path short-circuit** (`src/protocol/edit_plan.rs`, `plan_edit`
   bare-target `else`): does an existing literal path (`path == target` or ends_with `/{target}`)
   correctly beat the symbol cascade for EVERY dotted/symbol-free case, WITHOUT breaking a
   genuine bare-symbol target? Attack: a symbol whose name equals a real filename; a target
   that's a suffix of multiple files.
2. **AAP-002 `analyze_file_impact` non-parser gate** (`src/sidecar/handlers.rs::impact_skipped_text`):
   the `exists:true`/"no code parser" framing keys on `LanguageId::from_extension(ext).is_some()`.
   Attack: a file whose extension has a parser but is content-detected differently; a parser-less
   extension that IS a text file; confirm oversized PARSER files still get the honest refusal
   (`impact_admission` parity) and non-parser files never report false absence.
3. **M-001 authorization derivation** (`src/protocol/format.rs::placement_authorization`): it
   infers `normal` vs `explicit_protected` from `StatePlacement::MemoryOnly.failures` (ProjectLocal
   failure ⇒ normal; only-UserLocal ⇒ explicit_protected). Attack: is that inference sound for
   EVERY way `resolve_state_placement_with` (`src/discovery/mod.rs`) can populate `failures`?
   Any failure ordering/combination that misclassifies a protected root as normal (a security-
   relevant fail-open on the authorization label)?
4. **M-001 health honesty**: every field reads existing published-generation data. Attack: any
   field that renders a stale/misleading value (e.g. `query_readiness=ready` while degraded), or
   conflates a proxy (bridge "version" = content_generation; in-flight = configured ceiling not
   usage) in a way that reads as a real signal. Are the compact vs full outputs consistent?
5. **M-003 surface count** (`tests/surface_default.rs`): is `exactly 39` pinned to the right
   source (production `list_tools`, not a hardcoded 39)? Would adding/removing a tool correctly
   break it? Compact == 3 correct?
6. **M-002 assertions**: do they actually exercise the invariants or pass vacuously (e.g. the
   membership label test, the budget-degraded test)?

## Known-OPEN — do NOT report as bugs

- M-004 (corpus ≥50% token reduction) and M-012 (embed gate) are being measured separately.
- M-001 accepted proxies (recorded in `tasks/todo.md` M-014): `retry` via freshness reason-codes;
  bridge "version" via content_generation; in-flight = configured ceiling (live usage not retained
  past cold load). These are deliberate "surface existing data" scope choices, not defects.
- Gate L internals (already reviewed across 3 rounds).

## Verification commands (repo-pinned; clean up after)

```
C:\Users\rakovnik\.cargo\bin\cargo.EXE fmt --all -- --check
C:\Users\rakovnik\.cargo\bin\cargo.EXE clippy -j1 --all-targets --features server -- -D warnings
C:\Users\rakovnik\.cargo\bin\cargo.EXE test -j1 --lib --features server -- --test-threads=1
```

State at review: lib suite 3012/0/4; full `--all-targets` suite 113 binaries / 0 failed; clippy
`-D warnings` clean; fmt clean.

## Output

Two-part report (defects BLOCKER→LOW with proposed fixes; suggestions with value marks), per
the GATE-L-REVIEW.md output contract. "Clean" is credible only after concrete inputs, a parity
diff, an interleaving, and a test-attack.
