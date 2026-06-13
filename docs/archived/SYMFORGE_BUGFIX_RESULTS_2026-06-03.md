# SymForge MCP Bug-Fix Campaign — Results

Date: 2026-06-03 → executed 2026-06-04
Branch: `fix/symforge-mcp-audit-2026-06-03` (off `main` @ 8e7e5ee = v7.18.1 code). **NOT pushed — human-review gate.**
Plan: `docs/SYMFORGE_BUGFIX_IMPLEMENTATION_PLAN.md` · Verdicts: `..._VERDICTS.md`
Runner machine: `E:\project\symforge` (goal authored for `C:\AI_STUFF\PROGRAMMING\` — remapped).
Real repos for live-verify: `E:\project\testpilot` (TS/Angular), `E:\project\Agent_Army_Professionals` (Rust, AAP).

Status: FIXED = code+test+per-issue gate green. VERIFIED-LIVE pending Phase G (fresh-daemon real-repo re-run).

## Baseline
- HEAD `8e7e5ee` = v7.18.1 code (released green). SF-001 ground truth re-confirmed: `git merge-base --is-ancestor 251d7f0 v7.18.0` → true.
- Infra: E: drive hit 0 GB free mid-campaign (target/ ~52 GB); reclaimed by clearing incremental + my target-wsl cruft (kept the 32 GB dep cache warm). No worktrees used (each = its own ~50 GB target).

## Overlap with round-2 (Codex/Cursor review, shipped as 7.18.1)
- SF-008 refines `path_shadow::format_shadow_warning` ForeignPrefix arm (shipped in 7.18.0). SF-001 ops-guard builds on round-2 M1 (version in health). File overlap was orthogonal; agents re-read current source (code-is-gospel) — plan line numbers had all drifted.

## Per-issue status (11/11 implemented + committed; per-issue lib gate green)

| ID | Verdict | Status | Commit | Notes / decision |
|---|---|---|---|---|
| SF-007 | confirmed | FIXED | ee92e5d | proxy-first + daemon dispatch arm; round-trip test uses round-2 fail-closed auth |
| SF-008 | confirmed | FIXED | b12ec86 | ForeignPrefix arm cfg!(windows) → PowerShell-native; #[cfg(windows)] test ran on host |
| SF-002 | confirmed | FIXED | 1a18878 | enclosing_symbol_index guard; unresolved_same_name_member_calls; **reviewed** (false-neg paths verified) |
| SF-006 | confirmed (wrong cause) | FIXED | a71fc84 | stem-equality (>=3-char policy honored); 4 distinct fallback reasons; real substrings verified |
| SF-003 | confirmed | FIXED | 6025bc9 | shape A (no persisted change); SOUND neutralize-reparse-whole-file detector. **Review caught a real bypass** (identifier-glue `Sub[]scription`); fixed with token-preserving space replacement |
| SF-004 | confirmed | FIXED | 94a331a | third expected_framework_partial bucket; **Review caught 2 real bypasses** (defect sharing @if ERROR span; erroneous_end_tag → no diagnostic + _=>true fallback); fixed by adopting SF-003's neutralize-reparse pattern + deleting the fallback |
| SF-005 | confirmed | FIXED | 32a6159 | FindFile-before-FindSymbol reorder; first-token extraction; truncation → Inferred + chained hint |
| SF-010 | partial (half harness) | FIXED | 431169b | ToolHelp intent (tool(s)+verb scoped, hijack-guarded) + tools/catalog resource; drift guard; 32-tool surface unchanged. Lazy-exposure = harness, out of scope |
| SF-011 | confirmed | FIXED | 15d92a4 | dominant-language vote (config excluded by LanguageId, JS+TS folded); per-file scan gated by language; TS/JS branch; >25% second-lang note |
| SF-001 | already-fixed | FIXED | 2a7f14f | NO algorithm change (frozen symbols untouched); real-parser regression test; daemon staleness ops-guard (WARN not restart) |
| SF-009 | mechanism refuted | FIXED | 2a7f14f→ | surfacing only (git2 tracked-set, NOT ignore crate; FAIL-OPEN); "indexed untracked files: N"; admission defaults UNCHANGED; opt-in SYMFORGE_EXCLUDE_UNTRACKED default off |

## The review gate earned its keep (honesty)
Per-issue code-review caught **3 real defects** the per-issue tests missed — all the same class (a classifier wrongly EXCUSING genuinely-broken input as OK):
1. SF-003: the `[]`-strip glued `Sub`+`scription` into a valid identifier → broken TS marked `Status: ok`. Fixed (token-preserving).
2. SF-004 (C1): a real defect sharing the `@if` ERROR span let the diagnostic pin to the `@if` line → masked. Fixed (whole-file reparse).
3. SF-004 (C2): `erroneous_end_tag` yields no diagnostic → `_ => true` excused arbitrary broken HTML. Fixed (deleted fallback).
Lesson reinforced (matching SF-001's own meta-finding): a "negative control" that hand-builds fixtures instead of running the real parser proves nothing — both holes hid behind hollow tests.

## Open maintainer decisions (RESOLVED in-campaign; recorded for sign-off)
- SF-006: honored the existing **>=3-char** stem policy (a 1-2 char stem stays prefix-tier). Amends the report's 1-char `a` fixture.
- SF-003: **shape A** (render/health-layer classification, no persisted ParseStatus variant) — postcard isn't forward-compatible for additive variants; consistent with SF-004.
- SF-002: output = sibling Vec (surfaced, not counted); Rust `self.foo()` not specially guarded (language-agnostic guard covers it; recursion preserved).
- SF-001: daemon staleness → **WARN, not auto-restart**.
- SF-009: **fail-open** (no git → off); `SYMFORGE_EXCLUDE_UNTRACKED` default OFF.
- SF-011: mixed repo = single primary_lang + a ">25% also" note.

## Phase F — full integration gate (GREEN, criterion 2)
On the integrated branch: `cargo fmt --check`=0, `cargo clippy --all-targets -- -D warnings`=0, `cargo test --all-targets -- --test-threads=1`=0 (every integration binary, 0 failed), `cargo build --release`=0 (release binary `target/release/symforge.exe` = 7.18.1, the branch code).

## Phase G — live-verify (criterion 3): real branch binary, NO_DAEMON local mode, real repos
Method: `target/release/symforge.exe` driven via a minimal MCP-stdio client (`.claude/mcp_verify.py`) in `SYMFORGE_NO_DAEMON=1` with a temp `SYMFORGE_HOME` (guarantees branch code, no stale-daemon proxy); stale daemons killed first. testpilot indexed 296 files/23680 symbols; AAP 1386 files/38587 symbols.

| ID | Live result | Verdict |
|---|---|---|
| SF-011 | conventions → `Language: TypeScript`, `Exception-based: try/catch 82, throw new 41, RxJS catchError 1`, `99% camelCase / 100% PascalCase`, Jest/Mocha+decorators+DTOs. No "Result-based"/snake_case. | **VERIFIED-LIVE** |
| SF-005 | ask("Where is TestingController defined and what module imports it?") → `inferred`, `search_symbols(query="TestingController")` (not the sentence), `Suggested next step: Chain search_symbols -> find_references`; found testing.controller.ts:44 | **VERIFIED-LIVE** |
| SF-010 | ask("what tools can I use for impact analysis?") → catalog `impact-analysis: find_references, find_dependents, get_symbol_context, analyze_file_impact, what_changed, diff_symbols` | **VERIFIED-LIVE** |
| SF-002 | get_symbol_context startExploration (controller) → "No references found" (callers=0, not self-caller); Callees = `Body` only (the this.testingService.startExploration() self-call NOT listed) | **VERIFIED-LIVE** |
| SF-003 | validate_file_syntax workflow-builder.component.ts → `Status: ok` + `Note: parser limitation (...)`, 113 symbols | **VERIFIED-LIVE** |
| SF-006 | search_files(query="work_item", anchor=work_item.rs, path+cochange, debug) → `co-change ranking preparing - no coupling store exists...; bounded background preparation started` — the NEW precise reason (replaces old "none matched"). Stem-gate co-change application not live-exercisable (coupling store not built on a fresh index) → covered by calibration tests | **PARTIAL-LIVE** (reason precision verified live; gate via tests) |
| SF-004 | get_file_context app.html → STILL `partial` (diagnostic line 14). The SOUND detector excuses only files that fully reparse clean after neutralizing relational operators; the REAL app.html has richer Angular control-flow (multiple @if/@for blocks) beyond bare `@if (x > 0)`, so it stays dirty → conservatively NOT excused (the safe under-excuse direction the review forced) | **TEST-VERIFIED; real-template under-excuses (see open item)** |
| SF-007 | daemon-proxy tool, not exercisable in NO_DAEMON local mode | TEST-VERIFIED (daemon round-trip integration test: "Checkpoint complete" over real HTTP w/ fail-closed auth) |
| SF-008 | Windows-host-specific shadow text | TEST-VERIFIED (#[cfg(windows)] test executed on this host) |

## OPEN ITEM for human review (surfaced by live-verify)
**SF-004 real-world coverage gap.** The fix is SOUND (the 2 review-caught excuse-a-broken-file bypasses are closed) and the acceptance criterion is met for the bare-`@if (x > 0)` fixture, but the REAL testpilot `app.html` stays `partial` because its richer Angular control-flow (multiple/nested `@if`/`@for` blocks) is not fully neutralized by the relational-operator-only transform, so the whole-file reparse stays dirty (the safe under-excuse). To make the real app.html land under `expected_framework_partial`, broaden the neutralization to the full Angular control-flow block grammar (`@for (... ; track ...)`, `@switch`/`@case`/`@default`, `@empty`/`@placeholder`/`@loading`/`@error`, nested blocks) while keeping the neutralize-and-reparse-whole-file soundness contract. DEFERRED — flagged for the maintainer (scope + soundness-risk decision).

## Evidence log
- 2026-06-04: branch created; env mapped (real repos at E:\project); baseline green.
- All 11 issues implemented serially on the shared branch (no worktrees — disk), each gate-verified disk-light; Phase F full `--all-targets` green; release binary built.
- 3 soundness bypasses (SF-003 glue, SF-004 ×2 excuse-broken) caught by code-review and fixed before commit — the per-issue review gate's highest-value catches.
- Phase G live-verify against real testpilot + AAP on the fresh branch binary: 5 VERIFIED-LIVE, 1 partial-live (SF-006), 1 real-template-under-excuse finding (SF-004), 2 test-verified (SF-007/008).
