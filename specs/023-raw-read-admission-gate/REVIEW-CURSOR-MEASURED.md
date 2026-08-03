# Adversarial review — secret-detector exemption logic (spec 023, commit 20b51c8)

**Pass:** measured (repo access, cargo oracle + probe in `E:/project/symforge-rawread`, then in clone `E:/project/symforge-023-review` after two worktree wipes).
**Delivered:** 2026-07-31, 1h03m runtime.

## Verdict summary

Q1: BROKEN — the candidate comma fix introduces 10 measured false negatives and does NOT cure the brief's headline YAML defect. Reject it. C1 and C2: FALSIFIED, measured — a pre-existing, fix-independent credential leak via quote-parity inversion in `right_hand_side_continuation`. C3–C6: HOLD (traced; spot-measured). Three additional pre-existing false negatives measured (paren-led RHS regex ceiling, Go := ceiling, wrap(<8)-style callee ceiling).

## Brief vs. file drift

No logic drift between the brief's §3 inline copy and src/knowledge/mod.rs @20b51c8: `is_placeholder`, `is_placeholder_only_expression`, `right_hand_side_continuation`, `line_break_continues_expression`, `bounded_char_literal_len`, `expression_carries_quoted_payload`, and the `scan_secret_bytes` exemption loop are semantically identical (the file carries expanded doc comments and rustfmt line-breaking only). The comma arm exists only as uncommitted probe material, not in HEAD. Assignment text said the probe was '+117 lines, 17 rows (E1–E3)'; both surviving captures contain only E1/E2 (no E3 exists); Main's artifact://6 = +117/17 rows; my session-start capture artifact://10 = +176/31 rows (see delta section).

---

## Findings

### FN-1 (Q1 answer — fix REJECTED)

- **DIRECTION:** false-negative (leak)
- **CLAIM:** The candidate fix `b',' if depth == 0 => return false,` flips 10 of 31 adversarial rows from SENSITIVE to CLEAN — every one a hardcoded 20-byte literal sitting after a depth-0 comma in a credential-bound right-hand side.
- **INPUT SHAPE:** (a) JS object literal: `const cfg = {token: "${DEPLOY}", banner: "a-long-literal-value"};` (b) Rust struct literal: `let cfg = Cfg { token: "placeholder", banner: "a-long-literal-value" };` (c) Python tuple: `password = "placeholder", "a-long-literal-value"` (also with spaces around the comma); (d) JS multi-declarator: `const token = "placeholder", other = "a-long-literal-value";` (e) unquoted capture on a code path: `password = placeholder, 'a-long-literal-value'` (the comma is swallowed INTO the capture, so the leak flows through exemption B, not A).
- **PATH/EXT:** .js/.rs/.py/.go — all code languages (exemption A and B both reachable)
- **TRACE:** Q2b shape: regex captures `${DEPLOY}` (placeholder-only → exemption A candidate); `right_hand_side_continuation` steps over the closing quote; the walk starts at `, banner: ...`. The enclosing `{` of the object literal sits BEFORE capture_end, so it was never counted: depth == 0 at the entry comma although source-nesting is 1. With the arm: comma at depth 0 → return false → exemption granted → CLEAN. Without the arm: comma skipped as `_`, then the walk lands on the opening `"` of banner's value, fenced run of 20 ≥ 8 → return true → SENSITIVE.
- **CONFIDENCE:** measured
- **EVIDENCE:** Probe rows (command: `cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes`, eprintln rewired to `_probe_results.txt` because this environment only surfaces output of FAILED tests). Fix OFF → fix ON flips, verbatim: `ROW P1 ... demoted=true` → `demoted=false`; same for P2, P3, P4, P6, J1, E2, C3, C4, Q2b, Q2c. Gate-level confirmation in the clone (`cargo test --lib -- protocol::tools::tests::oracle_scratch_adversarial_review_rows`): `GATE G-Q1a js object literal sibling: demoted=false`, `GATE G-Q1b rust struct literal sibling: demoted=false`, `GATE G-Q1c py tuple placeholder then literal: demoted=false` — all three ADMITTED through classify_stable_content with the fix ON. The full 27-row oracle suite stays green with the fix ON (28 passed incl. scratch), i.e., no existing pinned row covers these shapes — same blind-spot class as §7.

### FN-2 (C1+C2 FALSIFIED — pre-existing, fix-independent)

- **DIRECTION:** false-negative (leak)
- **CLAIM:** `right_hand_side_continuation` steps over exactly one quote whatever it is, but the byte at capture_end is only the MATE of the regex-consumed opener when it is the same quote type. When a double-quoted placeholder literal contains an apostrophe (capture ends at the opposite quote type), the step-over leaves the walk at the literal's true closing `"`, which the walk reads as an OPENER; the skip-whole-literal then jumps past the next `"` — a credential's opening fence — blinding the fence test. A balancing quote later lets the walk terminate 'consumed'.
- **INPUT SHAPE:** `const token = "your-org's-token", "a-long-literal-value", "x` (JS; an 8+-byte your_-prefixed placeholder containing a possessive apostrophe, then a credential, then any balancing quote). Also, simpler but less idiomatic: unquoted capture directly before an opening fence — `const token = placeholder"a-long-literal-value", "x` — the step-over consumes the credential's opening quote itself.
- **PATH/EXT:** .js (measured); any code language where exemption A runs; fix-independent (present on pristine HEAD)
- **TRACE:** X1: capture = `your-org` (measured: `CAPTURE="your-org" start=15 end=23 placeholder=true`). continuation: `bytes.get(end) == '\''` → +1 → walk starts at `s-token", "a-long...`. The walk lands on the first literal's CLOSING `"`, treats it as an opener: fence run after it is `,` (1 < 8), so whole-literal skip scans to the NEXT `"` — the credential's opening fence — and jumps past it. The 20-byte credential is walked as bare bytes and never fence-tested. Credential's closing `"` fence-tests run `,` (1 < 8) and skips to the balancing `"` of `"x`; window then exhausts at depth 0, truncated=false → return false → CONSUMED → CLEAN. Control X4 (identical but no balancing quote): the last skip finds no partner → None → return true → SENSITIVE (fail-closed backstop), confirming the mechanism. Single-quoted placeholder literals are NOT affected (the `'` arm advances one byte via `bounded_char_literal_len(None).unwrap_or(1)`, so the walk still lands on every subsequent quote) — measured: X2 mirror is SENSITIVE.
- **CONFIDENCE:** measured
- **EVIDENCE:** Probe (fix OFF, pristine logic): `ROW X1 js apostrophe-in-placeholder parity inversion: demoted=false scan=Clean`; `ROW X3 js unquoted capture before opening fence: demoted=false scan=Clean`; `ROW X2 py mirrored quote-type parity inversion: demoted=true scan=Sensitive...`; `ROW X4 ... WITHOUT balancing quote (control): demoted=true`. Fix ON identical for all four (leak is orthogonal to the comma arm). Gate level: `GATE G-X1 parity inversion apostrophe: demoted=false` (ADMITTED), `GATE G-X4 ...: demoted=true`. C1 claim therefore fails on two attack classes it named itself (opposite-type quote at capture_end; capture never quote-wrapped). Backtick captures and multi-byte UTF-8 at capture_end traced safe (backticks are inside the value class so the capture includes both, capture_end lands past them; UTF-8 continuation bytes are ≥0x80 so capture_end is always a char boundary and the step only fires on ASCII quotes).

### FN-3 (additional — regex ceiling, pre-existing)

- **DIRECTION:** false-negative (leak)
- **CLAIM:** A right-hand side that begins with `(` within 7 bytes of the first quote/space produces NO regex match at all (value class `[^\s"'#]{8,}` fails), so black/rustfmt-style parenthesized multi-line assignments of credentials are invisible to the rule.
- **INPUT SHAPE:** `token = (\n    "a-long-literal-value",\n    "b-long-literal-value",\n)` or `token = ("placeholder"\n         "a-long-literal-value")`
- **PATH/EXT:** .py (measured); any language/path — the failure is in the rule pattern, before any exemption
- **TRACE:** After `=`, `[ \t]*["']?` consumes nothing, value class matches `(` then stops at the quote/newline → 1 byte < 8 → whole match fails; no other keyword position exists → zero captures → Clean. No exemption logic ever runs.
- **CONFIDENCE:** measured
- **EVIDENCE:** scratch test `knowledge::tests::_scratch_paren_rhs_capture_dump`: `V1 implicit concat in parens (G-C5 shape) NO-REGEX-MATCH / ROW V1: demoted=false scan=Clean`; `V2 black-style parenthesized multiline RHS NO-REGEX-MATCH / ROW V2: demoted=false scan=Clean`. Same ceiling measured for short callee names: probe row Q2a `token = wrap('${DEPLOY}', '<cred>')` → `wrap(` is 5 bytes → no capture line, Clean. (`compute_header(` is 15 bytes, which is why the classic R1/Q2d shapes DO match.)

### FN-4 (additional — regex ceiling, pre-existing)

- **DIRECTION:** false-negative (leak)
- **CLAIM:** Go short variable declarations evade the `keyword[:=]` anchor: in `password := "..."` the `:` is consumed by `[:=]` and the value class then sees `=` (1 byte < 8) → no match.
- **INPUT SHAPE:** `token, password := "placeholder", "a-long-literal-value"` and `token, err := "placeholder", fmt.Errorf("a-long-literal-value")`
- **PATH/EXT:** .go — code language, but the failure precedes the language gate
- **TRACE:** regex `password` + `[ \t]*` + `[:=]` matches the `:` of `:=`; value class then has only `=` before the space → <8 → fail; no other anchor → zero captures → Clean.
- **CONFIDENCE:** measured
- **EVIDENCE:** Probe rows: `ROW G1 go multiple return first placeholder: demoted=false` and `ROW G2 go short decl credential second: demoted=false` with NO CAPTURE lines (identical fix ON/OFF).

### FP-1 (brief §4 defect — mechanism correction, fix does NOT cure it)

- **DIRECTION:** false-positive
- **CLAIM:** The brief's Y1 trace is wrong on the mechanism, and the candidate fix does not cure Y1. Measured capture for `{token: ${TOKEN_NAME}, banner: "<v>"}` is `${TOKEN_NAME},` — the comma is INSIDE the capture (`,` is in `[^\s"'#]`), not 'the capture is ${TOKEN_NAME}, which stops at the comma'. The trailing comma makes `is_placeholder_only_expression` return false (`rest=','` is unconsumable), so exemption A never applies and the walk never runs; the finding stands on both configurations.
- **INPUT SHAPE:** `{token: ${TOKEN_NAME}, banner: "a-long-literal-value"}` (unquoted, flow style)
- **PATH/EXT:** .yaml — non-code; exemption A has no language gate, but it is never reached here
- **TRACE:** capture `${TOKEN_NAME},` → placeholder=false → not exemption A; path is YAML → `assignment_is_code_expression` false → `finding_count += 1` → SENSITIVE with and without the comma arm. The quoted variant `{token: "${TOKEN_NAME}", banner: "<v>"}` IS the shape the fix cures: capture `${TOKEN_NAME}` placeholder-only → walk starts at `, banner` → arm fires at depth 0 → consumed → CLEAN.
- **CONFIDENCE:** measured
- **EVIDENCE:** Fix OFF probe: `Y1u yaml flow unquoted — brief's exact shape CAPTURE="${TOKEN_NAME}," start=8 end=22 placeholder=false` / `ROW Y1u: demoted=true`. Fix ON: identical (demoted=true). Gate: `GATE G-Y1 yaml unquoted flow (brief headline): demoted=true` with fix ON vs `GATE G-Q1d yaml quoted flow sibling (FP the fix cures): demoted=false`. Y2 (no space after comma) capture=`${TOKEN_NAME},banner:` placeholder=false → SENSITIVE both configs. So the fix cures only the quoted variant while opening FN-1 — a trade the brief's §1 asymmetry forbids.

---

## Q2 answer

Depth bookkeeping holds only when the enclosing opener is INSIDE the walk window. `compute_header` case (the walk's raison d'être): capture = `compute_header(` (measured, 15 bytes ≥ 8), placeholder=false → exemption B walk starts at value_start; the walk itself sees `(` → depth=1; the comma between arguments is therefore at depth 1 and the arm does NOT fire; the fenced credential in argument 2 returns true → SENSITIVE. Measured preserved with the fix ON, both quote styles, detector and gate: probe rows R1/R2/Q2d demoted=true both configs; gate rows `GATE G-Q2a depth1 compute_header double: demoted=true`, `GATE G-Q2b depth1 compute_header single: demoted=true`. Arithmetic: without fix — `(` → 1, `,` skipped at depth 1, fence → true. With fix — identical, because depth==1 ≠ 0 at the comma. BUT when the opener precedes capture_end (object/struct literals, Q2b/Q2c in FN-1) the walk starts logically nested with depth==0 and the arm misfires — the exact arithmetic the brief worried about, measured leaking.

## Q3 answer

Reject the global comma arm. Recommended two-part fix, derived from the measured mechanisms: (i) strip ONE trailing entry separator (`,` or `;`) from the capture before placeholder recognition AND before computing the walk start (equivalently: evaluate the placeholder on the trimmed capture and begin the walk AT the separator). This cures Y1's actual mechanism: capture `${TOKEN_NAME},` trims to `${TOKEN_NAME}` → placeholder-only → walk starts at `,`. On code paths this is safe because the walk resumes at the separator and crosses it exactly as today (E2/C3 stay SENSITIVE — the credential fence is still ahead). (ii) Terminate the walk at a depth-0 `,` or `;` ONLY on non-code paths (`LanguageId::is_code_language() == false`), where a depth-0 comma can only begin a new flow entry, never a tuple/argument/operand — so the terminator cannot hide a credential the way FN-1 shows it does on code paths. With (i)+(ii): Y1 → walk starts at `,` on a non-code path → terminator fires immediately → CLEAN (cured); quoted variant → same; code paths unchanged → FN-1 shapes stay SENSITIVE. Alternatives rejected: 'restore a language gate on exemption A' makes YAML/GitHub-Actions `${...}` placeholders unconditional findings (config-wide false positives — the pre-commit behavior this commit was fixing); 'line-local walk for non-code paths' does not cure Y1, which is a single-line shape. Residual risk of (ii): a credential after a depth-0 comma with no keyword adjacency on a non-code path (e.g. a bare scalar flow entry) — far narrower than FN-1 and confined to config files.

---

## C-claim verdicts

- **C1 — FALSIFIED (measured).** See FN-2. 'Steps over exactly one closing quote' is only safe when the byte at capture_end is the mate of the regex's opener. Same-type: guaranteed by the value class (a capture can never contain the opener's quote type, so the first such quote IS the mate). Opposite-type (apostrophe inside a double-quoted placeholder): the step leaves the walk inside the still-open literal → parity inversion → measured leak X1. Never-quote-wrapped capture directly before a quote: the step consumes an OPENING fence → measured leak X3. Backtick-wrapped captures: safe by accident (backtick is in the value class, so both backticks are captured and capture_end lands past them) — traced. Multi-byte UTF-8 at capture_end: safe (continuation bytes ≥0x80, capture ends only at ASCII excluded bytes; quotes are ASCII) — traced. Capture ending in a quote: impossible (value class) — traced.
- **C2 — FALSIFIED (measured).** The precondition 'from is provably outside any string literal' does NOT hold for exemption A's new caller: X1 measured shows the walk beginning on a closing `"` treated as an opener (inverted parity), and the skip-whole-literal consuming a credential's opening fence. The fail-closed backstop (unterminated literal → return true) usually catches this — control X4 measured SENSITIVE — but one balancing quote defeats it. Gate-level: G-X1 admitted.
- **C3 — HOLDS (traced + measured).** A capture with substantive content between/around groups cannot pass: groups consume strictly, interior must be brace-free and non-empty, leftmost find() is correct, longest-first opener order reads `${{` as one group, and trim/lowercase run before parsing without introducing content. Measured: `GATE G-C3a literal between two interpolation groups: demoted=true` (`token: ${A}a-long-literal-value${B}` → not placeholder → SENSITIVE). Noted design ceiling (not a break): `GATE G-C3b single group with long interior: demoted=false` — a 20-byte literal inside one `${...}` group is exempt BY DESIGN on any path; a credential spelled as an interpolation token is admitted. Also note prefix rules bypass content checks: `your_token_here,` is placeholder=true even with the trailing comma (measured E1), because `starts_with("your_")` ignores the tail.
- **C4 — HOLDS (traced + measured).** Termination: every iteration either returns false or strictly shrinks rest (end≥1 enforced, so rest advances by ≥1+close.len()). Cost: each group's find() is bounded by the distance to its own closer (interior is brace-free, so a closer for `${` is never past the next `}`); captures are whitespace-free (value class), so length ≤ longest line and ≤ the scan-size limit. Measured: one 8000-byte capture of 2000 back-to-back `${a}` groups classified in 1.624ms (`GATE G-C4 2000 tiny groups (8000-byte capture): demoted=false elapsed=1.624ms`). Not a DoS vector.
- **C5 — HOLDS (traced + measured).** No common continuation style that can carry a credential onto the next line is missed by `+-*|&^%=.?:\`: bracket-open continuations are covered by depth>0 (the newline arm only fires at depth 0); operator continuations are in the set. Measured positive control: V3 backslash continuation `token = "placeholder" \\\n    "<v>"` → `CAPTURE="placeholder"` … demoted=true (SENSITIVE). Python implicit string concatenation across lines requires parens or a backslash; with parens the rule never matches at all (FN-3 ceiling — a regex defect, not a continuation-set defect); on one line it never reaches a newline. Excluding `,` `/` `<` `>` remains justified.
- **C6 — HOLDS (traced + measured).** `window[..newline]` / `window[newline+1..]`: newline < len by the loop bound, newline+1 ≤ len — safe. `interior[..end]` and `interior[end+close.len()..]`: closers are ASCII, so both are char boundaries; end+close.len() ≤ len because find() matched the full closer — safe even with multi-byte interiors (measured: `GATE G-C6 utf8 multibyte interior before closer: demoted=false`, `token: ${tökén}`, no panic). `bounded_char_literal_len` cursor+=2 overshoot: `window.get(cursor)?` → None → return None — safe. Whole-literal skip cursor+=2: same get()-guarded, None → return true — safe. depth: i32 overflow needs 2^31 openers inside a 512-byte window — impossible. `from.saturating_add` and from ≤ bytes.len() (secret.end()+1 ≤ len) — empty-window case returns false — safe.

---

## Additional notes

1. Environment quirk (affects reproduction): in this harness, cargo test surfaces captured output ONLY for FAILED tests — passing tests print nothing even with `--nocapture`/`--show-output` (verified with a marker test). All per-row verdicts were therefore captured by rewiring the scratch probe's `eprintln!` to `std::fs::write`, and by failing-assertion channels. The author's eprintln-only probe would have printed nothing here.
2. Whole-oracle regression check for the fix: with the comma arm applied, `cargo test --lib -- protocol::tools::tests::oracle_` → `test result: ok. 28 passed; 0 failed` (27 pinned + scratch) — the pinned suite does not cover any depth-0-comma-after-placeholder shape, so a green suite certifies nothing about the fix. Same blind-spot class as §7.
3. Probe rows C1/C2 (`token = ("placeholder", ...)` and `token = ("${X}" + ...)`) are Clean on BOTH configurations because the `(` led RHS produces no regex match (FN-3), not because of any exemption — the row labels overstate what they exercise.
4. Fix flip inventory (31 probe rows, fix OFF → ON): flips to CLEAN = P1, P2, P3, P4, P6, J1, E2, C3, C4, Y1q (the only intended cure), Q2b, Q2c — 12 rows, 11 of them false negatives; flips none = all others; J2 stays SENSITIVE but finding_count drops 2→1 (placeholder capture exempted, second keyword-adjacent literal still found). X1–X4 identical in both configurations.

## Artifact delta explanation

Main's artifact://6 (authoritative author-original, captured before my session): +117 lines = the probe test ONLY, 17 rows (Y1, P1–P4, J1–J2, R1–R2, Y2, Y3, E1, E2, C1–C4), NO comma arm. My artifact://10 (git diff captured read-only at my session start): +176 lines = the same 17 rows PLUS a 4-line comma arm (`// PROBE ONLY — candidate fix under adversarial review. REVERT.` + `b','` arm) PLUS 11 more rows ×5 lines (Y1q, Y1u, P5, P6, G1, G2, Q2a, Q2b, Q2c, Q2d, FN-semi) = +59 lines. 117+59=176 ✓. Who added the delta: NOT me. Every command I ran before the first wipe was read-only (git status/log/diff, grep, sed reads, cargo test/list); my first mutating act in the worktree was the post-wipe re-application of artifact://10 itself. The +176 state therefore existed in the working tree before my first command completed; the delta was added by the author (or the same unknown actor) between Main's +117 capture and my session start. Two wipes of the worktree occurred during my run (HEAD unmoved, untracked files deleted — restore/clean style, no reflog trail); per Main's order I did all remaining work in the clone E:/project/symforge-023-review and left it in place.

## BriefReasoning disclosure

None received. BriefReasoning offered a from-scratch Python port of the §3 logic in one IRC message; I never requested it, it was never sent, and no shapes, verdicts, hints, or other material from BriefReasoning were used. All shapes and conclusions here are my own, derived from the brief and measured against the repository.

## Command log

IN E:/project/symforge-rawread (git all read-only; cargo as listed):

```
git status --porcelain && git log --oneline -1 && git diff --stat
git diff src/knowledge/mod.rs
grep -n (several read-only greps/sed reads of src/knowledge/mod.rs, src/protocol/tools.rs)
cargo test --lib -- protocol::tools::tests::oracle_ 2>&1 | tail -45
cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes -- --nocapture 2>&1 | grep -E "CAPTURE|ROW|test result|running"
cargo test --lib -- --list 2>&1 | grep -i "probe\|adversarial"
grep -n "mod tests" src/knowledge/mod.rs && sed -n '1015,1025p' src/knowledge/mod.rs && cargo test --lib -- --list 2>&1 | grep "knowledge::tests" | head -5
grep -n "_probe_adversarial_review_shapes\|PROBE ONLY" src/knowledge/mod.rs; git diff --stat; wc -l src/knowledge/mod.rs
git status --porcelain; git log --oneline -1; ls -la _probe_captures.py 2>&1; git stash list
cat .git; git reflog -5; git fsck --lost-found | head; ls -la
git diff --stat && git status --porcelain
cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes -- --nocapture 2>&1 | grep -E "CAPTURE|ROW|test result"
cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes -- --nocapture 2>&1
cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes -- --nocapture > probe_out.txt 2>&1; wc -l probe_out.txt
cat probe_out.txt
touch src/knowledge/mod.rs && cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes -- --nocapture > probe_out.txt 2>&1; grep -c "ROW\|CAPTURE" probe_out.txt
./target/debug/deps/symforge-1f46c1ee6d235e88.exe --nocapture --exact knowledge::tests::_probe_adversarial_review_shapes 2>&1 | head -50  (failed: command not found)
target/debug/deps/symforge-1f46c1ee6d235e88.exe --nocapture --exact ... > direct_out.txt 2>&1  (failed: command not found)
cat .cargo/config.toml; ls target/debug/deps/ | head -5; printenv CARGO_TARGET_DIR
ls target/debug/deps/symforge-*.exe
cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes -- --show-output > probe_out.txt 2>&1; grep -c ROW probe_out.txt
sed -n '880,900p' src/knowledge/mod.rs; grep -rn "set_output_capture\|gag\|ctor\b" src/lib.rs Cargo.toml | head
cargo test --lib -- knowledge::tests::_scratch_stderr_check -- --nocapture > probe_out.txt 2>&1; cat probe_out.txt
cargo test --lib -- knowledge::tests::_scratch_stderr_check -- --nocapture > probe_out.txt 2>&1; echo exit=$?; cat probe_out.txt; cat _scratch_probe_results.txt  (failing assert proved output channels)
rm -f probe_out.txt _scratch_probe_results.txt _probe_results.txt && cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes 2>&1 | tail -3 && cat _probe_results.txt  (transient rustc failure; next run succeeded)
cargo test --lib -- knowledge::tests::_probe > err.txt 2>&1; echo exit=$?; grep -n error err.txt | head
tail -5 err.txt; ls _probe_results.txt && cat _probe_results.txt  (FIX-ON probe verdicts captured)
rm -f _probe_results.txt && cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes > err.txt 2>&1; echo exit=$?; tail -2 err.txt; cp _probe_results.txt _probe_results_fixOFF.txt && cat _probe_results.txt  (FIX-OFF probe verdicts captured)
cargo test --lib -- protocol::tools::tests::oracle_ > oracle_fixOFF.txt 2>&1; echo exit=$?; tail -2 oracle_fixOFF.txt  (27 passed, fix OFF)
cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes > err.txt 2>&1; grep "^X\|ROW X" _probe_results.txt  (X1–X4 fix OFF)
cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes > err.txt 2>&1; grep "ROW X" _probe_results.txt  (X1–X4 fix ON, comma arm re-added)
ls -la _probe* probe* err.txt oracle* scratch* direct* 2>&1; git status --porcelain  (second wipe discovered)
git log --oneline -1; git status --porcelain; echo CLEAN-CHECK-DONE
```

IN THE CLONE (per Main's order):

```
git clone E:/project/symforge E:/project/symforge-023-review && git -C E:/project/symforge-023-review checkout --detach 20b51c8 && git -C E:/project/symforge-023-review log --oneline -1
export CARGO_TARGET_DIR=E:/project/symforge-rawread/target; cargo test --lib -- protocol::tools::tests::oracle_scratch_adversarial_review_rows > _scratch_run.txt 2>&1; cat _oracle_scratch_results.txt
cargo test --lib -- protocol::tools::tests::oracle_ > _oracle_fixON.txt 2>&1  (28 passed, fix ON)
cargo test --lib -- protocol::tools::tests::oracle_scratch_adversarial_review_rows > _scratch_run2.txt 2>&1; cat _oracle_scratch_results.txt  (with C3/C4/C5/C6 rows)
cargo test --lib -- knowledge::tests::_scratch_paren_rhs_capture_dump > _s.txt 2>&1; cat _scratch_paren_results.txt
```

NO mutating git commands (no commit/restore/checkout-of-files/clean/stash/branch) were run in either worktree; the only checkout was `--detach` in the fresh clone, as ordered.

## Worktree state proof

E:/project/symforge-rawread: wiped twice during the run by an unknown actor (restore/clean style; HEAD never moved). My last direct observation: `git status --porcelain` → (empty), `git log --oneline -1` → `20b51c8 fix(023): scope the placeholder exemption to the capture, not the expression`. Per Main's instruction, final clean-state verification of rawread was done by Main, not me. The author's +117 probe (artifact://6) and `_probe_captures.py` are gone from disk; the probe content survives as artifact://6 (Main's capture) and artifact://10 (my +176 capture). Clone E:/project/symforge-023-review left in place at detached 20b51c8 containing my scratch (comma arm in src/knowledge/mod.rs, `oracle_scratch_adversarial_review_rows` in src/protocol/tools.rs, `_scratch_paren_rhs_capture_dump` in src/knowledge/mod.rs) for reproduction; Main will remove it.
