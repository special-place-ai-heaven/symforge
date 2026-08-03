# Adversarial review — secret-detector exemption logic (specs/023)

**Pass:** reason-only (brief-only, no repo access, no cargo). Reviewer re-implemented the §2/§3 logic verbatim in Python and executed every input through it, current code vs. candidate fix.
**Delivered:** 2026-07-31, 18m19s runtime.

---

Method note: per my constraints I read ONLY the brief. To make traces exact I re-implemented the §2/§3 logic verbatim (regex, `is_placeholder`, `is_placeholder_only_expression`, `right_hand_side_continuation`, `expression_carries_quoted_payload`, `bounded_char_literal_len`, `line_break_continues`) in Python and executed every input below through it, current code vs. candidate fix. "traced through the code" below means: hand-traced through the inlined Rust AND reproduced in that port. One limitation: `assignment_is_code_expression` (exemption B) is not inlined; per §2 ("both now funnel through the same withdrawal test") I assume it invokes the walk from `right_hand_side_continuation(bytes, secret.end())`. Where a finding depends on that assumption I say so. All findings that matter go through exemption A, which is fully inlined.

================================================================
FALSE NEGATIVES (ranked by commonness of shape)
================================================================

ID:          FN-1 / Q1-ANSWER (candidate fix leaks JS multi-declarator)
DIRECTION:   false-negative (leak) — introduced by the candidate fix
CLAIM:       A real hardcoded credential sitting after a depth-0 comma in a credential-bound RHS is newly reported CLEAN by `b',' if depth == 0 => return false`.
INPUT SHAPE: const token = "placeholder", backup = "a-long-literal-value";
PATH/EXT:    .js (code language; irrelevant — exemption A is ungated)
TRACE:       Rule matches `token = "placeholder"`: keyword `token`, `[ \t]*[:=]` on ` = `, `["']?` consumes the opening `"`, capture = `placeholder` (11 bytes, stops at closing `"`, capture_end = 26). `is_placeholder("placeholder")` → exact-list hit → exemption A. `right_hand_side_continuation` steps the closing quote → walk from byte 27 = `, backup = "a-long-literal-value";`. CURRENT code: `,` default-skipped at depth 0; walk reaches the `"` at window idx 10, fenced run = 20 bytes ≥ 8, `window[run] == '"'` → return true → exemption WITHDRAWN → SENSITIVE. WITH FIX: window idx 0 is `,` at depth 0 → return false → consumed → `continue` → no other rule match (`backup` is not a keyword) → CLEAN. Simulated: current=SENSITIVE, with-fix=CLEAN.
CONFIDENCE:  traced through the code
NOTE:        `const a = "x", b = "y"` multi-declarator is bog-standard JS/TS. Same trace works for `let`/`var`, and for Python tuple assignment `token = "changeme" , "a-long-literal-value"` (simulated: SENSITIVE → CLEAN). The attempted-shapes list the brief asked for if Q1 failed is moot — Q1 succeeded on the first shape and three more besides.

ID:          FN-2 / Q2-ANSWER (the fix kills the walk's own motivating case)
DIRECTION:   false-negative (leak) — introduced by the candidate fix
CLAIM:       The depth bookkeeping does NOT survive the walk starting inside an already-open bracket: an argument-separating comma inside the enclosing `(` sits at depth 0 in the walk's frame, so the fix terminates the walk exactly where §7 needed it to look.
INPUT SHAPE: token = compute_header('label', 'a-long-literal-value')   (§7's canonical row)
PATH/EXT:    .py / .js (code languages)
TRACE:       Capture = `compute_header(` (15 bytes; stops at the first `'`; capture_end = 23). `right_hand_side_continuation` steps the quote at 23 (see FN-5: it is an OPENING quote) → walk from 24. Worked depth table (window-relative idx, absolute byte, depth BEFORE processing):
               idx0-4  abs24-28  `label`           depth=0 (the `(` at abs 22 < from was never counted)
               idx5    abs29     `'` closing 'label'  depth=0; fenced run after it = `,` then space → 1 < 8; `bounded_char_literal_len`: interior `, ` carries no bracket → None → idx += 1
               idx6    abs30     `,`               depth=0  ← WITH FIX: return false → CONSUMED → CLEAN
               idx7    abs31     ` `               depth=0
               idx8    abs32     `'` opening cred  depth=0; CURRENT code reaches here: fenced run = 20 bytes, `window[run] == '\''` → return true → SENSITIVE
             Simulated: walk(current)=True, walk(comma-fix)=False. Identical result for a cleaner variant whose walk starts OUTSIDE any literal: `token = combine(label , 'a-long-literal-value')` — capture = `combine(label`, capture_end = 21, byte there is ` ` so no continuation step; comma at abs 22 is at depth 0 (the `(` at abs 15 predates `from`); current walk returns true at the `'` at abs 24, fix returns false at the comma. Depth never left 0 in either example because the only bracket byte (`)`) comes AFTER the credential — and had the walk reached it, depth would have gone to -1 → "consumed", which is the design's own admission that the walk starts inside an uncounted group.
CONFIDENCE:  traced through the code (via §2's stated funneling of exemption B through the same walk)

ID:          FN-3 / Q1 (the fix leaks the §4 shape's evil twin ON YAML ITSELF)
DIRECTION:   false-negative (leak) — introduced by the candidate fix
CLAIM:       On the exact file class §4 is about, the fix converts "false positive when the sibling value is benign" into "false negative when the sibling value is real" — the two are byte-identical to the walk.
INPUT SHAPE: {token: ${TOKEN_NAME} , "a-long-literal-value"}   (flow mapping, keywordless second element; space before comma — see MECH-1 for why the space matters)
PATH/EXT:    .yaml (non-code)
TRACE:       Capture = `${TOKEN_NAME}` (stops at the space). `is_placeholder_only_expression` consumes `${...}` → exemption A. capture_end sits on the space; continuation doesn't advance. Walk: idx0 ` `, idx1 `,` at depth 0 → WITH FIX: return false → CLEAN. CURRENT: walk continues to the `"`, fenced 20-byte run → true → SENSITIVE. Simulated: SENSITIVE → CLEAN. A sibling WITH a keyword (`api_key: "..."`) is unaffected — it self-matches the rule — so on YAML the fix's ONLY effect is to flip keywordless-sibling payloads from flagged to leaked, plus the benign case to CLEAN. The detector cannot tell benign from real (both are 20-byte fenced runs); any fix keyed on the comma treats them identically. QED the comma is the wrong lever.
CONFIDENCE:  traced through the code

ID:          FN-4 / C5-FALSIFIED (comma continuation across a newline — CURRENT code, no fix needed)
DIRECTION:   false-negative (leak) — exists in the code under review today
CLAIM:       C5's claim fails on the most common continuation style in existence: a trailing comma. `,` is not in CONTINUATION, so a depth-0 newline after a comma consumes the walk with a credential on the next line.
INPUT SHAPE: (a) const token = "placeholder",\n  backup = "a-long-literal-value";   (.js — pure exemption A)
             (b) token = compute_header('label',\n  'a-long-literal-value')          (.py — black's default arg-per-line; via exemption B's walk)
PATH/EXT:    .js / .py (code languages)
TRACE:       (a) capture `placeholder`, continuation steps closing `"`, walk from the `,`: idx0 `,` skipped (current code), idx1 `\n` at depth 0 → `line_break_continues_expression`: trailing non-ws = `,` ∉ `+-*|&^%=.?:\`; leading non-ws = `b` ∉ set → return false → CONSUMED → CLEAN. Simulated: CLEAN. (b) capture `compute_header(`, continuation steps the opening `'` of 'label', walk: closing `'` at idx5 (char-literal skip → None, interior `,\n` has no bracket), `,` at idx6, `\n` at idx7: trailing `,` ∉ set, leading `'` ∉ set → return false → CLEAN. Simulated: CLEAN. Note (b) only bites because the walk starts inside the uncounted `(` (FN-2's bookkeeping again): had depth been ≥1, the newline rule is skipped. Also missed, same mechanism: leading-`(` continuations and Python implicit string concatenation across lines. Shell `\`, `&&`/`||`, `+`, ternary `?:` are covered; the comma is not.
CONFIDENCE:  traced through the code ((b) additionally assumes the §2 funneling statement)

ID:          FN-5 / C1-FALSIFIED + C2-FALSIFIED (continuation steps an OPENING quote; walk starts inside a literal)
DIRECTION:   false-negative (leak) — exists in the code under review today
CLAIM:       C1's "always correct" is wrong whenever `["']?` consumed NOTHING: an unquoted capture that ends immediately before a quote leaves an OPENING quote at capture_end, and `right_hand_side_continuation` steps over it — the walk begins inside the literal with inverted parity. C2's precondition therefore does not hold for exemption A.
INPUT SHAPE: TOKEN=placeholder'a-long-literal-value'   (shell/dotenv adjacent-string concatenation)
PATH/EXT:    .sh / .env / unknown ext (non-code; exemption A ungated so it runs anyway)
TRACE:       Capture = `placeholder` (stops at `'`; capture_end = 17; `["']?` consumed nothing because the byte after `=` is `p`). continuation sees `'` at 17 → steps to 18, i.e. INSIDE the credential string — its opening fence is consumed before the walk starts. Walk: 20 payload bytes pass through the default arm; the closing `'` at window idx 20 fires the fenced test on what FOLLOWS it (`\n` → run 0 < 8); `bounded_char_literal_len` → None (newline); idx 21 `\n` at depth 0, trailing `'` ∉ CONTINUATION → return false → CONSUMED → CLEAN. Simulated: CLEAN. The credential was never fenced-tested because the test only fires when the walk LANDS ON a quote (§7's lesson 1, recurring). Second inversion source: a capture that stops at WHITESPACE inside a quoted string — `token = "your-key x" "a-long-literal-value"` (capture `your-key`, `your-` prefix → placeholder; capture_end sits on a space inside the string; continuation cannot advance) — parity inverts; that instance happened to fail CLOSED via the unterminated-literal rule (simulated SENSITIVE), so I report it as parity-inversion evidence, not a leak. C1's sub-attacks: backticks — the regex's `["']?` never consumes a backtick but the value class DOES include it, so `` `${...}` `` captures arrive with backticks inside the capture (trimmed later by `is_placeholder`); capture_end then sits past the closing backtick and continuation is a no-op — correct, but by accident of the class, not by C1's stated reasoning. A capture whose last byte is a quote is impossible for `"`/`'` (class excludes them) and benign for backtick. Multi-byte UTF-8 after capture_end: harmless (byte compare against ASCII quotes; all slicing is on `&[u8]`).
CONFIDENCE:  traced through the code

================================================================
CLAIMS THAT HOLD
================================================================

ID:          C3-HOLDS
DIRECTION:   none (with one theoretical caveat)
CLAIM:       `is_placeholder_only_expression` cannot be made to accept a capture containing bytes outside interpolation groups.
TRACE:       Battery executed: `${{a}}literal`, `literal${{a}}`, `${{a}}b}}`, `${${x}}`, `${{${x}}}`, `${{a}${{b}}}}`, `${{a}b}}`, `${}`, `${{}}`, bare `$`/`{`/`}`/empty → all False. `${{secrets.TOKEN}}`, `{{x}}`, `${V}`, `${{a}}${{b}}`, `${a}${b}${c}`, `${{a}}{{b}}${c}` → True (intended). Nesting is rejected because interior must be brace-free; `find(close)` takes the EARLIEST close so no group can stretch over a literal; longest-first opener ordering is actually redundant (the `${` path rejects `${{x}}` on the brace check anyway) but harmless. `to_ascii_lowercase` and the `trim_matches` of `"'`<>[]` that run before it cannot smuggle anything: trim only strips wrapper bytes, lowercase only case-folds. CAVEAT (theoretical FN): `${a-long-literal-value}` → True. Any brace-free 8+-byte token inside `${}` is accepted as a placeholder regardless of whether the file's language has interpolation semantics — the check validates SHAPE, not REFERENT. I could not turn this into a plausible real-source leak; a hardcoded credential dressed as `${...}` in a language without interpolation is not a shape that occurs.
CONFIDENCE:  traced through the code (holds); caveat is plausible but not traced to a real input

ID:          C4-HOLDS
DIRECTION:   termination: none. DoS: theoretical only.
CLAIM:       Always terminates; not a practical DoS vector.
TRACE:       Every `'consume` iteration either consumes ≥ 4 bytes (`${x}` minimal group) or returns false; `rest` strictly shrinks → termination. Worst case is O(n²/4) byte-scanning via `find` over shrinking `rest`. Executed: 50,000 adjacent `${a}` groups (200 KB single capture) = 281 ms in an interpreted Python port; the Rust original is far faster. A capture is one unbroken `[^\s"'#]` token, so a meaningful stall needs a single token of interpolation groups in the ≥100 KB range — no real source file contains that. Adversarial-repo ceiling exists in principle; not worth defending here.
CONFIDENCE:  termination: traced through the code. Cost bound: plausible but not traced (measured in my port, not against the Rust build — running cargo was outside my constraints).

ID:          C6-HOLDS
DIRECTION:   panic: none found.
CLAIM:       No index arithmetic here can panic.
TRACE:       Audited every site. `window[..newline]` / `window[newline + 1..]`: newline < window.len() (loop bound) so newline+1 ≤ len — an empty tail slice is legal, not a panic. `interior[..end]` / `interior[end + close.len()..]`: `str::find` returns a char boundary and `close` is pure ASCII → both boundaries valid. `bounded_char_literal_len`: every access is `window.get(cursor)?`; `cursor += 2` overshooting `last_close` just exits the loop → None. The `"` skip loop: `window.get(cursor)`, None → return true. `run - (index + 1)`: run ≥ index+1. `cursor + 1 - at`: guarded by `cursor > at + 1`. `index = cursor + 1` ≤ window.len(), re-checked by the while condition. `&bytes[from..end]`: from ≤ len (capture_end ≤ len, +1 after a successful `.get`), end = min(from+512, len); `&[u8]` slicing has no char-boundary requirement. depth: i32 bounded by the 512-byte window (≤ 512 increments) — no overflow. The regex's `captures.get` is handled. Multi-byte UTF-8 in captures: the regex crate yields boundary-aligned spans; all comparisons are single-byte against ASCII.
CONFIDENCE:  traced through the code

================================================================
MECHANISM CORRECTION (affects how the §4 fix should be evaluated)
================================================================

ID:          MECH-1 (§4's stated trace does not match the inlined regex for the stated input)
DIRECTION:   none directly (the §4 verdict — SENSITIVE on .yaml — still reproduces, so the defect stands; but the WHY differs)
CLAIM:       For the exact §4 input `{token: ${TOKEN_NAME}, banner: "a-long-literal-value"}`, exemption A never engages: `,` is a MEMBER of the value class `[^\s"'#]`, so the capture is `${TOKEN_NAME},` (15 bytes, stops at the SPACE before `banner`), and `is_placeholder_only_expression("${token_name},")` returns false on the trailing comma. The file is SENSITIVE because no exemption applies — not because the walk crosses the comma.
INPUT SHAPE: as above; contrast `{token: ${TOKEN_NAME} , banner: "a-long-literal-value"}` (space before comma)
PATH/EXT:    .yaml
TRACE:       Executed: unspaced → capture `${TOKEN_NAME},`, is_placeholder = False, verdict SENSITIVE (plain finding, no walk). Spaced → capture `${TOKEN_NAME}`, placeholder = True, walk crosses the comma, fenced 20-byte run → SENSITIVE; with the candidate fix → CLEAN. So the walk-crossing mechanism the brief describes requires whitespace before the comma — which flow-style emitters commonly DO produce, so the §4 FP class is real either way. Consequence for the fix decision: (1) the comma fix does nothing for the unspaced variant (already not-placeholder; stays SENSITIVE — the §4 row as literally written would NOT be fixed by the comma arm); (2) the oracle should pin BOTH variants.
CONFIDENCE:  traced through the code

================================================================
Q3 — THE BETTER FIX
================================================================

Recommendation: candidate (a) — restore a language gate — but scope it precisely: gate the WITHDRAWAL WALK, not the placeholder suppression. On non-code paths, a placeholder capture is exempt, full stop; run `expression_carries_quoted_payload` only when `is_code_language(path)`.

Why the alternatives lose:
- The comma fix (and candidate (c), `,`/`;`): dead on arrival per FN-1/FN-2/FN-3. The walk's depth is relative to `from`, so NOTHING evaluated at walk-depth 0 can distinguish an argument separator (§7's case, must look) from an element separator (§4's case, must not look). Worse, §4's benign shape and FN-3's leak shape are byte-identical up to payload semantics the detector cannot judge — any payload-blind terminator rule must treat them the same, so it either keeps the FP or creates the FN. The fix must use information available AT or BEFORE `from`, not after.
- Candidate (b), line-local walk for non-code paths: does not fix §4 — the banner sibling is on the SAME line. Verified against the §4 trace: the entire flow mapping is one line.
- Candidate (a) as literally stated (gate all of exemption A) would re-flag every `${VAR}` interpolation in every config file — the FP flood that presumably motivated ungating. Hence the refinement: suppress placeholders everywhere, withdraw only on code paths.

Cost accounting per §1's asymmetry: on non-code paths the walk's marginal recall is only keywordless same-line sibling credentials in flow collections (FN-3's shape) — rare, and pre-commit behavior flagged ALL config placeholders anyway. Its marginal FP cost is the entire class of config files mixing `${...}` with benign string literals — common. Code paths keep the walk exactly as-is, so FN-1/FN-2's shapes stay SENSITIVE. Residual, accepted: on CODE paths, `{token: "${TOKEN_NAME}", banner: "a-long-literal-value"}` in a .js config object stays a false positive (walk crosses the object-literal comma); tolerable per §1, and unfixable without true depth-at-`from` (a backward bracket scan over literals), which is not justified complexity here.

================================================================
LIMITATIONS
================================================================
- Exemption B (`assignment_is_code_expression`) is not inlined; FN-2 and FN-4(b) route through it per §2's "both funnel through the same withdrawal test." If B does not call the walk, those two transfer to "B's internal logic unknown," and FN-1/FN-3/FN-4(a)/FN-5 (all pure exemption A) stand unchanged.
- Verdicts were reproduced in a faithful Python port of the inlined code, not via the repo's oracle suite (my constraints forbade opening other files and running cargo). The port is line-for-line from §3; the one intentional simplification is ASCII-only whitespace classification, which does not affect any input above.
- I wanted to check the repo for: B's body, and whether the value class really is `[^\s"'#]` in the compiled rule (MECH-1 hinges on it — if the shipped regex excludes `,` from the class, MECH-1 evaporates and §4's original trace is exactly right, leaving FN-3 as the YAML-relevant finding). Recorded as a limitation per instructions.
