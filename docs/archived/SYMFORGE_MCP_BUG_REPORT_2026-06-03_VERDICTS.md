# SymForge Bug Report — Verification Verdicts

Companion to `SYMFORGE_MCP_BUG_REPORT_2026-06-03.md`. Each SF issue was independently
root-caused (rust-pro investigator), adversarially verified (code-reviewer), and — for the
repo-specific claims — live-reproduced against the real `Agent_Army_Professionals`, `testpilot`,
and `symforge` repos. This file records the VERIFIED verdict and every correction to the original
report. The actionable build instructions live in `SYMFORGE_BUGFIX_IMPLEMENTATION_PLAN.md`.

Method per issue:
- **Investigation**: read the live source, cite file:line, propose fix + test.
- **Verification**: adversarial re-check of the investigation against the code.
- **Reproduction**: shell/parser ground truth against the real repos (where externally checkable).

---

## Verdict summary

| ID | Report severity | VERIFIED verdict | Report accuracy |
|---|---|---|---|
| SF-001 | Critical | **ALREADY FIXED** — not reproducible >=7.10.0 | WRONG against current source (stale-binary artifact) |
| SF-002 | High | CONFIRMED defect | Accurate; cleaner fix exists (enclosing_symbol_index) |
| SF-003 | High | CONFIRMED defect | Accurate; trigger narrower (only `[]` suffix) |
| SF-004 | Medium | CONFIRMED limitation | Accurate |
| SF-005 | Medium | CONFIRMED defect | Accurate |
| SF-006 | Medium | CONFIRMED defect | Accurate symptom; WRONG cause (not path-normalization) |
| SF-007 | Medium | CONFIRMED defect | Accurate |
| SF-008 | Medium | CONFIRMED defect | Accurate |
| SF-009 | Medium | **MECHANISM REFUTED** | WRONG cause; scratch files add 0 symbols |
| SF-010 | Low | PARTIAL | Half is harness behavior, not SymForge |
| SF-011 | Low | CONFIRMED defect | Accurate |

Scoreboard: 7 fully accurate, 2 accurate-symptom/wrong-cause (SF-006, SF-009-partial),
1 already-fixed (SF-001), 1 conflated-with-harness (SF-010). The audit was high quality; the
two consequential misses (SF-001, SF-009) are both explained by the same root operational cause —
a stale/shadowed daemon binary and a concurrent agent editing source during the audit.

---

## SF-001 — find_dependents false positives :: ALREADY FIXED

- **Reproduction (shell, AAP):** CONFIRMED the *ground truth* the report asserts — the 4 actor files
  have ZERO textual reference to `work_item`, and they share the generic bare name `new` (and `state`/`get`)
  with `WorkItemStore`'s public methods; `aap-agents` has a real `aap-db` path dependency. So a name-keyed
  graph WOULD collide. BUT the reproducer could not run `find_dependents` itself.
- **Investigation + verification (live source):** the name-keyed collision is already GATED. Every Call
  edge needs a `::`-boundary qualified-name suffix match (`disambiguation.rs:16-49`) or a same-name import;
  `can_match_type_dependents` is false for Rust. A zero-reference file as a dependent is structurally
  impossible. The fix shipped in commit `251d7f0` (ancestor of `v7.18.0`). Independent re-run via the real
  `LiveIndex::load` pipeline produced 0 edges.
- **DIRECTLY RE-VERIFIED:** `git merge-base --is-ancestor 251d7f0 v7.18.0` -> true.
- **Why the audit saw it:** stale daemon binary — `daemon_reused_session` mode + the PATH-shadowed
  `C:\Program Files\nodejs\symforge` ("version unknown", the SF-008 shadow) served pre-fix behavior.
- **Correction to report:** the Critical correctness verdict is invalid against current source. Do NOT
  change the matching algorithm. Residual work is operational (binary-staleness guard) + a real-parser
  regression test (existing 4 tests hand-build refs).

## SF-002 — TS same-name method conflation :: CONFIRMED

- **Reproduction (testpilot):** CONFIRMED — 3 distinct `startExploration` defs (controller:44,
  service:415, frontend ApiService:378); controller body delegates to `this.testingService.startExploration()`.
  Same-name cross-object ambiguity is real. (Bonus: the surface is WIDER than the report — the frontend
  ApiService method is a 3rd collider.)
- **Root cause:** the receiver guard in `matches_exact_symbol_reference` is gated Rust+Function-only;
  TS methods bypass it and match on bare name. Callee half has no name-vs-self check.
- **Correction to report:** the report implies a receiver/byte heuristic; the index already has
  `enclosing_symbol_index` (DIRECTLY RE-VERIFIED present across 16 files) — keying on it is precise and
  does NOT suppress legitimate intra-class `this.foo()` callers, which the byte heuristic WOULD.
  Verifier also found the report's regression-test recipe broken (the `make_ref` helper couples
  line_range to byte_start/100, so the ref won't land in the method's line range).

## SF-003 — TS import-type `[]` parse error :: CONFIRMED

- **Reproduction (dogfooded the real pinned grammar):** CONFIRMED, with a NARROWER trigger than reported.
  Scalar `import('rxjs').Subscription` parses CLEAN everywhere; only the `[]`/tuple suffix breaks. The
  reported diagnostic (line/col) reproduced byte-exact. `tree-sitter-typescript 0.23.2` is the LATEST
  crate — no grammar-bump remedy.
- **Root cause:** binary `has_error -> PartialParse` classification with no "grammar limitation" concept;
  the only expected-partial bucket is vendor-SCSS path-based.
- **Correction to report:** the report's implied "valid syntax mis-flagged" is right, but the obvious
  detector is UNSOUND — verifier proved a genuinely broken `import('rxjs').Subscription[] = [ ; foo bar`
  yields the same error-node prefix and would be wrongly marked OK. The fix MUST validate the whole
  construct. Also: `.tsx` is already covered by the same grammar (no separate handling). And the new
  `ParseStatus` variant touches the PERSISTED checkpoint format — serde back-compat required.

## SF-004 — Angular template control-flow :: CONFIRMED limitation

- **Reproduction (dogfooded tree-sitter-html 0.23.2):** CONFIRMED byte-exact (`0) {` at line 14, col 38).
  The `>` relational operator is the trigger (`@if (cond) {` without `>` parses clean). The grammar has
  zero Angular rules; SymForge's own source already comments it text-scans Angular.
- **Root cause:** same classification gap as SF-003 — only the vendor-SCSS expected-partial bucket exists.
- **Correction to report:** none material. Verifier confirmed `PublishedIndexState` has NO serde derive,
  so the investigation's serde-default worry was moot. Heuristic caveat: a malformed `.html` that also
  contains a valid `@if` would be mis-bucketed — don't over-promise. And the `get_file_context`
  "Parse status: partial" line (the actual repro surface) must also be fixed, not just the health bucket.

## SF-005 — `ask` compound query :: CONFIRMED

- **Root cause:** the "where is " branch captures the whole remainder and only strips trailing suffixes,
  so "...defined and what module imports it?" becomes the symbol name. Reported as `Exact` confidence
  with no next-step hint (confident false negative).
- **Correction to report:** verifier — must reorder `where is file ` before bare `where is ` (else a
  new regression on file queries), and drop the proposed stop-word list in favor of first-token extraction.

## SF-006 — Co-change ranking fallback :: CONFIRMED defect, WRONG cause in report

- **Reproduction (git, AAP):** CONFIRMED the co-change partnership is REAL (work_item.rs has 3 commits,
  ALL 3 also touched work_item_store.rs). The reproducer's leading hypothesis was path-normalization.
- **Investigation REFUTED that:** the `neighbors.get(path)` lookup SUCCEEDS. The real cause is the
  anchor-confidence gate: stem query `work_item` scores only PREFIX tier (50) against `work_item.rs`
  (basename incl. extension), below the basename floor (100), so fusion never applies. Fusion DOES work
  for the with-extension query shape today.
- **Correction to report:** cause is the stem-vs-basename gate, NOT a path mismatch. The fallback message
  is misleading on two counts. Four distinct fallback reasons must be disambiguated (incl. chore-anchor).
  Verifier flagged a real tension: the report's 1-char acceptance fixture (`a` vs `a.test.ts`) conflicts
  with SymForge's existing `len >= 3` prefix policy — a maintainer decision.

## SF-007 — checkpoint_now in daemon-proxy mode :: CONFIRMED

- **Root cause:** only index-bearing tool not wired into the proxy path; hard-fails before proxying and
  has no daemon dispatch arm. Forwarding to the daemon (which owns the authoritative index) is correct
  and feasible.
- **Correction to report:** none. Verifier confirmed `CheckpointNowInput` already derives `Serialize`
  (no manual JSON needed) and that the local-fallback reloads the index (does not checkpoint empty).

## SF-008 — PATH remediation not Windows-native :: CONFIRMED

- **Reproduction (this Windows box):** CONFIRMED — `Get-Command symforge -All` shows the shadow;
  `format_shadow_warning` branches only on `ShadowKind`, never OS; any `C:\` shadow -> ForeignPrefix ->
  POSIX `~/.profile`/`export PATH` string (and `$PATH` is a broken var in PowerShell).
- **Correction to report:** none. Verifier — key off `cfg!(windows)`; the npm-side POSIX emitters are
  gated to non-native-Windows paths (out of scope); lead with the simple PATH-order fix, not setx/nvm.

## SF-009 — Untracked scratch files in index :: MECHANISM REFUTED

- **Reproduction (AAP + empirical `discover_all_files`):** the scratch files exist and are not gitignored,
  BUT they are dotfiles -> filtered by `ignore` default `hidden:true` before admission; and any `.txt`
  maps to `LanguageId::None` -> Tier-2 metadata-only -> 0 symbols, 0 Tier-1 count. `discover_all_files`
  on a representative tempdir returned only `[notes.txt(Tier-2), scratch_probe.json(Tier-1), src_main.rs]`.
- **Correction to report:** the +1450 symbols CANNOT come from text scratch files (arithmetically
  impossible) — it was the concurrent agent's real source edits. The report's own proposed regression test
  (assert `.probe.txt` not indexed) PASSES today with zero code change (vacuous). Only real residue:
  surface "indexed untracked files: N" for non-dotfile untracked recognized-extension files. Do NOT change
  admission defaults.

## SF-010 — Tool discoverability :: PARTIAL

- **Root cause:** "lazy exposure" (two-pass discovery) is HARNESS behavior — SymForge eagerly returns all
  32 tools via the rmcp `#[tool_handler]` macro. The SymForge-owned half: `ask` has no tool-meta intent,
  so `ask("what tools for impact analysis?")` becomes a code search.
- **Correction to report:** the discoverability complaint conflates harness behavior with a SymForge gap.
  Fix the `ask` ToolHelp + tool-catalog half; document the lazy-exposure half as out of scope. Verifier:
  keep ToolHelp out of the Understand|Explore upgrade guard, and scope detection to `tool(s)` + a
  recommendation verb to avoid hijacking code queries.

## SF-011 — conventions Rust-biased :: CONFIRMED

- **Root cause:** `detect_conventions` runs one Rust-flavored pass, never reads `IndexedFile.language`.
- **Correction to report:** verifier — the dominant-language filter must exclude config files by
  `LanguageId`/`is_config`, NOT by `FileClass` (every path is `FileClass::Code`); gate the per-file SCAN by
  language (not just the summary branch); `test_file_count` is already language-agnostic (only inline-module
  counts are Rust-only); fold JS+TS into one language bucket.

---

## Operational meta-finding

Two of the report's misses (SF-001 already-fixed, SF-009 wrong-cause) trace to the SAME operational
conditions the audit itself documented: it ran against a PATH-shadowed daemon of "version unknown"
(`daemon_reused_session`) while a concurrent agent edited source. This argues strongly for the SF-001
ops-guard work (assert served-binary freshness in `health`) as the highest-leverage durable fix — it
would have surfaced both the stale-binary cause of SF-001 and the real source-of-truth of the SF-009
count delta. Prioritize it even though SF-001's algorithm needs no change.
