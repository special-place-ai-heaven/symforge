# Adversarial review disposition ledger — 2026-07-29

Every finding from the four-lens adversarial review of the three detector fixes
that landed this session (B2 char-literal skip, H1 placeholder discipline, H2
line continuation). 22 findings survived their own refutation pass; each is
dispositioned below against what the code *actually* does, verified by running
the detector rather than by reading the report.

Verdict vocabulary: **FIXED** (defect reproduced and closed, pinned by a row) ·
**NOT REPRODUCED** (claim traced or measured, verdict did not hold) ·
**SUPERSEDED** (a duplicate of another entry's root cause) ·
**CEILING** (real, out of scope for this campaign, stated rather than closed) ·
**NO DEFECT** (the lens verified safety).

## Fixed

| id | fix | direction | disposition |
|---|---|---|---|
| FO-1 | B2 | fail-open | **FIXED.** `'` is a string delimiter in Python/JS/TS/Ruby/PHP, all code languages. The skip fired on a string's *closing* quote, and the next `'` inside the 3-byte bound was the *opening* quote of the following argument (`', '` is a two-byte gap), so it jumped the fence byte the payload test needs to land on. Measured CLEAN in `.py` and `.js`; the same line with the first argument double-quoted was SENSITIVE. Skip now requires the span to carry a bracket. Rows S20a–S20d. |
| B2-CLOSING-QUOTE-REFIRE | B2 | fail-open | **SUPERSEDED by FO-1** — same root cause, same fix, same rows. |
| F3 | B2 | fail-open | **SUPERSEDED by FO-1** — "bounded skip swallows the opening fence of a genuine single-quoted payload" is FO-1 stated from the fence's side. |
| BA-1 / BA-1b | B2 | fail-open | **FIXED via FO-1.** Re-measured after the bracket requirement: 5-byte and 7-byte single-quoted first arguments both SENSITIVE. |
| BA-2 | B2 | fail-open | **FIXED via FO-1.** Empty `''` pair followed by a credential: SENSITIVE. |
| BA-3 | B2 | false-positive | **SUPERSEDED by FO-1** — the same phantom opener, measured clean after the fix. |
| H2-SEMICOLONLESS-GENERIC | H2 | false-positive | **FIXED.** `>` closed a generic parameter list, so an unterminated declaration ran the walk into the next line's unrelated string. Measured SENSITIVE before, CLEAN after. `<`/`>` dropped from the continuation set; no S19 row needed either byte. Row G10c. |
| H1-DOUBLE-BRACE-DIALECTS | H1 | false-positive | **FIXED.** GitHub Actions writes one expression with nested braces; the interior-brace test rejected `${{secrets.X}}` on every workflow file — a regression this session's H1 change introduced. Openers are now tried longest-first. Rows G11a/G11b. |
| H1-ADJACENT-EXPANSIONS | H1 | false-positive | **FIXED.** `${A}${B}` with no literal anywhere is a placeholder-*only* expression and is exempt; the single-group test had refused it. `is_placeholder_only_expression` consumes a sequence. Row G11c, with S21d as the paired control that a literal *between* groups stays sensitive. |
| FO-2 | H1 | fail-open | **FIXED.** The placeholder branch `continue`d before the withdrawal walk ran, so a placeholder in the first operand of a concatenation exempted the whole expression. The capture is exempt; the expression is not. Fixed at the shared scanner boundary both exemptions pass through. Rows S21a/S21b. |
| FO-4 / F4 | B2 ↔ H2 | interaction | **FIXED.** A quote span could cover a line break and carry the walk past the depth-0 gate without it being consulted. A genuine char literal never spans a line; the skip stops at `\n`. Row S21c — which also covers the second half of the owner's rule, that a span must not jump another opening fence. |

## Not reproduced

| id | fix | disposition |
|---|---|---|
| H2-LEADING-DOT-CHAIN | H2 | **NOT REPRODUCED.** A rustfmt leading-dot method chain followed by an unrelated quoted string measured CLEAN. The claim assumed the walk reaches the next statement; a finished statement still terminates it. |
| H2-BLOCK-COMMENT-STAR | H2 | **NOT REPRODUCED.** A `/* … */` continuation line beginning with `*` after a finished statement measured CLEAN — the preceding `;` ends the walk before the comment is reached. |

## Stated ceilings — real, needing an owner ruling, not closed here

| id | disposition |
|---|---|
| F1 | **CEILING.** `let token = ${A}<literal>${B};` *unquoted on a code path* is still CLEAN: `is_placeholder` now rejects it, but the code-expression walk then consumes it. H1 closed the quoted and config-path forms; this one is the separate code-expression premise. Measured, not fixed. |
| F2 | **CEILING.** The root cause behind F1, stated wider: `assignment_is_code_expression`'s premise that "in code a credential is a string literal" is applied to any right-hand side. Revisiting it is a design change to Ruling 3's shape, not a defect fix. |
| FO-5 | **CEILING.** The continuation set is operators only, so continuation styles whose boundary token is a comma or an identifier fall outside it. Deliberate: `,` was excluded because following it walks one struct-literal field's exemption into the next field's value — a false positive this repository's own source produces (row G10b). Widening needs a ruling on that trade. |
| FO-3 | **CEILING, narrowed.** The `unwrap_or(1)` fallback is a one-byte advance, not a fail-closed refusal. After the bracket requirement the measured leak band (BA-1/BA-2) is closed; what remains is the general statement that a one-byte advance is not the same as refusing. No reproducer survives. |
| F6 | **FIXED — and I initially dispositioned this wrongly.** My first pass recorded it as already corrected; checking the file instead of trusting that showed the whole contract block (the withdrawal conditions, the `'` paragraph, the not-line-local INVARIANT) had been left attached to `line_break_continues_expression`, because inserting the two new helpers split the block from its function. Moved back onto `expression_carries_quoted_payload`. Two claims inside it were also falsified by this session's own changes and are rewritten: the `'` paragraph's "the bound sits far below MIN_PAYLOAD so the skip can never hide a fenced payload" (false — the skip only has to hide the one opening *fence byte*, which is exactly how FO-1 leaked), and the PRECONDITION's claim of a single caller (the placeholder branch is now a second one, via `right_hand_side_continuation`). |

## No defect

| id | disposition |
|---|---|
| V-1 | **NO DEFECT.** The multibyte-UTF-8 panic in the placeholder slice is unreachable: `&&` short-circuits, and both `starts_with`/`ends_with` guards run first with ASCII delimiters, so both slice indices are char boundaries and the length guard prevents underflow. Independently re-derived. |
| V-2 | **NO DEFECT.** Every index computation in `bounded_char_literal_len` is sound: no overflow, no underflow, no zero-length return (the `cursor > at + 1` guard forces ≥3), no overshoot past the window, no infinite loop. |

## Campaign status since this ledger was written (2026-07-31)

Two things landed on `fix/raw-read-admission-gate` after the 22 findings above
were dispositioned.

**D1 — the comma decision** (`e85dbf6`). C5 turned out to be a live false
negative in shipped code, not a fix-dependent one: a credential on the second
declarator of a multi-line binding list was unreachable because `,` was absent
from the continuation set. Three other designs were measured and rejected first
— a global comma terminator, a non-code-gated terminator, and resuming on a
swallowed structural comma — each leaking somewhere different. Full reasoning
and the rejected designs are in `COMMA-DECISION-PROPOSAL.md`; the pinning matrix
is in `MATRIX-D1.md`. The struct-literal false positive returns as an accepted,
pinned regression, and the whole-tree tripwire found one real instance in
`src/server/serve.rs`, fixed at the source per Ruling 1.

**FU-1 — the working-tree disclosure lanes.** The three lanes that shared
`GitRepo::file_from_workdir` now route through
`read_gate::admit_worktree_text`, which performs the read behind
`admit_disk_read`. `diff_symbols` in uncommitted mode reports a refused file as
withheld rather than rendering it as empty, which would have claimed every
symbol in it was removed. Pinned by a behavioural test on the shared seam plus a
structural tripwire asserting no protocol source calls the ungated read — the
latter fails the moment a fourth lane is written, which is exactly how the first
three came to share it.

### Corrections to earlier entries

- **F6** was mis-dispositioned once as already-fixed; corrected in place above.
- The adversarial pass on D1–D4 named `apikey` as a non-keyword sibling. It is a
  keyword — `api[_-]?key` has an optional separator — and that row measures
  SENSITIVE with **two** findings. The mechanism it illustrated is real;
  `bearer`, `credential` and `accesskey` demonstrate it.
- Matrix row A4 was a worthless control: it passes because its sibling keyword
  produces an independent match, so the walk contributes nothing to its verdict.
  Replaced by non-keyword-sibling rows.

### New ceiling logged

Bracketed arrays of quoted strings never match the rule at all.
`api_key = ["placeholder", "<cred>"]` measures CLEAN because after `=` the next
byte is `[` and the next is `"`, giving a one-byte capture under the eight-byte
floor. Pre-existing, unrelated to any comma decision, and a total blind spot
across TOML, YAML and JSON.

## Owner rulings on the stated ceilings — 2026-08-03

Authority: delegated to the session by the owner ("I leave all this to your
judgement"), after the ceilings were restated in plain language. PR #485 was
already squash-merged as `114b793` when these were ruled.

| ceiling | ruling |
|---|---|
| F1 / F2 | **ACCEPTED, with recorded direction.** The code-expression premise stays. If ever revisited, the principled narrowing is: a bare `${…}` placeholder group inside a code-language RHS is not valid syntax in any of the nine exempted languages, so its presence self-refutes "this is an expression" — deny the exemption only then, never for unquoted RHS generally (which would flag ordinary `let token = fetch_token();`). |
| FO-5 | **ACCEPTED.** The measured false positive widening would cost (one struct field's exemption walking into the next field's value, row G10b, this repository's own source) outweighs the rarer continuation styles it would gain. |
| FO-3 | **ACCEPTED.** No reproducer survives the FO-1 bracket requirement; hardening the `unwrap_or(1)` advance to fail-closed buys nothing measurable today. |
| Bracketed arrays | **FIXED — landed as `80cdb67` (PR #486)**. The regex's optional-quote slot could not consume `[`, so the capture started at the bracket, died at the first quote under the 8-byte floor, and the keyword could not re-match later in the line. The rule now consumes an optional inline-array opener and skips short leading elements, with the skip coupled to a mandatory opening quote (the FO-5 false-positive guard, pinned by AR4 and mutation-checked). Rows B1 + AR1–AR8; `SECRET_POLICY_VERSION` 2→3. The tripwire then caught six true positives in this repo's own test fixtures — the `["<prefix>-", "to", "ken"].concat()` idiom, invisible precisely because of this blind spot — fixed at source per Ruling 1. |

## What the green suite did not catch

The leak in FO-1 was live through a full green suite — 3078 lib tests plus 113
binaries, zero failures — and through the whole-tree tripwire. Every S16/S17 row
was Rust with double-quoted payloads, so nothing exercised a language where `'`
delimits strings. The oracle set now carries `.py`, `.js`, `.ts` and
`.github/workflows` rows for exactly that reason.
