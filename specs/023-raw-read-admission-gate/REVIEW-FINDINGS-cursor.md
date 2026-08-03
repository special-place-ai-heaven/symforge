# Independent review findings — Cursor
Date: 2026-07-31
Brief: specs/023-raw-read-admission-gate/REVIEW-BRIEF.md
Commit under review: 20b51c8 (E:/project/symforge-rawread)
Mode: adversarial attack on depth-0-comma candidate fix; measured where possible

---

## Executive summary

**Do not ship the bare depth-0 comma terminator.** It introduces confirmed
false negatives on code paths (Python tuple RHS / JS multi-declarator /
unquoted identifier-then-literal), while **failing to fix** the brief's exact
unquoted YAML flow shape. The brief's defect trace is also wrong about capture
semantics: the value class `[^\s"'#]{8,}` **does not stop at comma**, so the
unquoted shape's capture is `${TOKEN_NAME},` with `is_placeholder=false`, and
SENSITIVE is decided without the withdrawal walk.

Recommended fix: short-circuit exemption A on non-code paths (no withdrawal
walk), and separately normalize a trailing structural `,` before the
placeholder test so unquoted flow maps can become placeholder-clean without
fail-opening code.

---

## Q1 — Primary: break the depth-0 comma candidate fix

### Verdict: **BROKEN — confirmed false negatives (measured)**

I temporarily applied:

```rust
b',' if depth == 0 => return false,
```

inside `expression_carries_quoted_payload`, measured via `scan_secret_bytes`,
then reverted the worktree (left clean, no commit).

### Confirmed false-negative shapes (were SENSITIVE before fix → CLEAN after)

```
ID:          Q1-P1-python-tuple-placeholder
DIRECTION:   false-negative (leak)
CLAIM:       Depth-0 comma termination newly admits a credential that is the
             second element of a password/token tuple RHS after a placeholder.
INPUT SHAPE: password = "placeholder", "a-long-literal-value"
PATH/EXT:    .py (code language)
TRACE:       CAPTURE=`placeholder` (placeholder=true). Exemption A. Continuation
             steps over closing `"`. Walk sees `, "a-long-literal-value"`.
             Before fix: fence hits → withdraw → SENSITIVE.
             After fix: depth-0 `,` → return false → exemption kept → CLEAN.
CONFIDENCE:  traced through the code; measured both sides
```

```
ID:          Q1-P4-python-tuple-env-placeholder
DIRECTION:   false-negative (leak)
CLAIM:       Same as Q1-P1 with an interpolation placeholder as the first
             tuple element.
INPUT SHAPE: token = "${DEPLOY_HOST}", "a-long-literal-value"
PATH/EXT:    .py (code)
TRACE:       CAPTURE=`${DEPLOY_HOST}` (placeholder-only expression). Same walk
             as Q1-P1. Measured: SENSITIVE → CLEAN under the fix.
CONFIDENCE:  traced through the code; measured both sides
```

Sibling measured FNs under the same mechanism (all code paths):

| Row | Shape | Before | After fix |
|---|---|---|---|
| P2 | `password = "changeme", "a-long-literal-value"` | SENSITIVE | CLEAN |
| P3 | `password = "test-value", "a-long-literal-value"` | SENSITIVE | CLEAN |
| P6 | `password = "placeholder" , "a-long-literal-value"` | SENSITIVE | CLEAN |
| J1 | `const token = "placeholder", other = "a-long-literal-value";` | SENSITIVE | CLEAN |
| C4 | `let token = "placeholder", x = "a-long-literal-value";` | SENSITIVE | CLEAN |
| E2 | `password = placeholder, 'a-long-literal-value'` | SENSITIVE | CLEAN |
| C3 | `token = placeholder, "a-long-literal-value";` | SENSITIVE | CLEAN |

**E2 / C3 are worse than “exemption A only”:** capture is `placeholder,`
(`is_placeholder=false` because of the trailing comma). Exemption B then runs
the same walk from `value_start`, hits the depth-0 comma under the fix, and
grants a code-expression exemption ahead of the fenced credential. So the
candidate fix fail-opens **both** callers of the walk, not only the new
placeholder branch.

J2 (`const token = "placeholder", password = "a-long-literal-value"`) stays
SENSITIVE under the fix because the second keyword produces an independent
match — that does not redeem P1/J1/E2.

### Shapes tried that did **not** yield a new FN (or never matched)

- `token = ("placeholder", "a-long-literal-value")` — no rule match (`(` is a
  1-byte capture before `"`); CLEAN for an unrelated ceiling.
- `token = ("${X}" + "a-long-literal-value")` — same no-match ceiling.
- `token = your_token_here, "a-long-literal-value"` — capture is
  `your_token_here,` (comma swallowed into capture; `your_` prefix →
  placeholder). Walk starts **after** the comma, so the depth-0 comma arm never
  fires; stays SENSITIVE under the fix.
- `token = '${DEPLOY_HOST}' + 'a-long-literal-value'` (P5) — no comma; stays
  SENSITIVE under the fix (good).
- `token = "placeholder"; const banner = "a-long-literal-value";` — semicolon
  is not in the candidate arm; stays SENSITIVE under the fix.

### Oracle suite blindness

`cargo test --lib -- protocol::tools::tests::oracle_` → **27/27 ok with the
fix applied**. Existing rows do not pin tuple / multi-decl / unquoted
`placeholder,` shapes. A green oracle is not evidence the fix is safe — same
class of miss as §7 of the brief.

---

## Q2 — Depth bookkeeping when the walk starts inside an already-open bracket

### Verdict: **bookkeeping does NOT model already-open brackets; the motivating call shape still works for a different reason**

```
ID:          Q2-depth-precondition
DIRECTION:   none (correctness of candidate / depth model)
CLAIM:       `depth` starts at 0 at `from` regardless of brackets opened before
             `from`. Property-separator commas inside `{...}` / struct literals
             are therefore treated as depth 0.
INPUT SHAPE: (a) `token = compute_header("label", "a-long-literal-value")`
             (b) `const cfg = {token: "${DEPLOY}", banner: "a-long-literal-value"};`
             (c) `let cfg = Cfg { token: "placeholder", banner: "a-long-literal-value" };`
PATH/EXT:    .py / .js / .rs (code)
TRACE:       (a) Exemption B. Walk starts at CAPTURE=`compute_header(` — the
             opening `(` is **inside** the window, so depth becomes 1 before the
             arg-list comma. Candidate fix does **not** stop there. Measured
             under fix: still SENSITIVE (R1/R2/Q2d).
             (b)(c) Match sits inside a `{` that opened **before** `from`.
             Walk never increments depth for that brace. The property comma is
             depth 0. Candidate fix stops and returns CLEAN (Q2b/Q2c measured).
CONFIDENCE:  traced through the code; measured under the fix
```

So:

- The **motivating** `compute_header(...)` case is preserved under the candidate
  fix, because the opener is visible after `from` (usually as the last byte of
  the capture `compute_header(`).
- That is **not** proof that “start inside already-open bracket” is safe. Object /
  struct / YAML flow property commas are exactly the already-open case, and the
  candidate fix’s YAML “win” is the same depth-0 mis-model that creates the
  code-path FNs in Q1.

I could not construct an exemption-A shape where a *placeholder capture* is the
first argument of `wrap('${DEPLOY}', '...')` such that the walk starts past an
unseen `(`: the rule’s value capture begins at the function name, so the capture
becomes `wrap(` (too short / not a placeholder) and there is no match. The
already-open problem shows up on **braces opened before the match**, not on
call openers that are part of the capture.

---

## Q3 — Better fix recommendation

### Reject: bare `b',' if depth == 0 => return false` on the shared walk

Fail-open on code (Q1), does not fix the brief’s exact unquoted YAML shape
(below), and the oracle suite will not catch the regression.

### Prefer: non-code short-circuit on exemption A (+ trailing-comma placeholder normalize)

**Primary recommendation — restore a language gate on the withdrawal walk for
exemption A only:**

- On non-code paths (JSON/TOML/YAML/Env/Markdown/…): if the capture is a
  placeholder, `continue` **without** calling `expression_carries_quoted_payload`.
- On code paths: keep today’s “capture exempt, expression not” walk unchanged
  (no depth-0 comma arm).

Argument:

- The §4 regression is caused by running the code-oriented withdrawal walk on
  config after exemption A lost its language gate. Putting the gate back on the
  *walk*, not on placeholder recognition itself, preserves S18/S21-style
  “placeholder must not license an adjacent literal” on code (P5 still
  SENSITIVE) while stopping config sibling-key FPs for **quoted** flow values.
- It does not open Q1-P1 / E2 on `.py`/`.js`.
- Narrower than “terminate all depth-0 commas everywhere,” which conflates
  Python tuple commas with YAML entry commas.

**Required companion for the brief’s exact unquoted shape:**

```
ID:          DEFECT-Y1u-capture-includes-comma
DIRECTION:   false-positive (and brief-trace error)
CLAIM:       The brief’s unquoted YAML flow example is SENSITIVE because the
             capture includes the trailing comma and fails the placeholder test,
             not because the walk crosses into `banner`.
INPUT SHAPE: {token: ${TOKEN_NAME}, banner: "a-long-literal-value"}
PATH/EXT:    .yaml (non-code)
TRACE:       Measured CAPTURE=`${TOKEN_NAME},` start/end via the compiled
             `secret.context-assignment` rule; `is_placeholder=false`.
             Exemption A does not run. Exemption B is language-gated off.
             finding_count += 1 with **no walk**. Depth-0 comma fix leaves this
             row SENSITIVE (measured Y1/Y1u).
CONFIDENCE:  traced through the code; measured
```

Fix the companion with a **targeted** normalize, e.g. for placeholder testing
only, also accept “placeholder-only expression + trailing `,`” (or trim trailing
commas before `is_placeholder` / `is_placeholder_only_expression`), **and** pair
it with the non-code short-circuit so a newly recognized placeholder on YAML
does not immediately re-enter the walk and re-create the sibling FP.

Quoted sibling shape (the walk-mediated FP the brief described):

```
ID:          DEFECT-Y1q-quoted-flow-sibling
DIRECTION:   false-positive
CLAIM:       Quoted placeholder then sibling fenced value in a flow map is
             SENSITIVE today via exemption-A withdrawal; depth-0 comma would
             clear it, but so would a non-code short-circuit without code FNs.
INPUT SHAPE: {token: "${TOKEN_NAME}", banner: "a-long-literal-value"}
PATH/EXT:    .yaml (non-code)
TRACE:       CAPTURE=`${TOKEN_NAME}` (placeholder=true). Continuation past
             closing `"`. Walk at depth 0 crosses `,` into `banner`’s fence →
             withdraw. Measured SENSITIVE today; CLEAN under bare comma fix
             (Y1q) — but that is the fail-open design.
CONFIDENCE:  traced through the code; measured
```

### Candidates considered and ranked

| Candidate | Fixes Y1q FP? | Fixes Y1u FP? | Opens Q1 code FNs? | Notes |
|---|---|---|---|---|
| Bare depth-0 `,` on shared walk | Yes | **No** | **Yes** | Reject |
| Depth-0 `,` only when path is non-code | Yes | No (alone) | No | Acceptable if walk grows a path/lang flag; still need Y1u normalize |
| Non-code short-circuit on exemption A | Yes | No (alone) | No | **Preferred**; simplest intent match |
| Depth-0 `,` and `;` everywhere | Yes | No | Yes (+ more) | Worse than comma-only |
| Line-local walk on non-code | No (same line) | No | No | Does not address single-line flow maps |

---

## Secondary claims C1–C6

### C1 — `right_hand_side_continuation` steps over exactly one closing quote

```
ID:          C1-continuation
DIRECTION:   none
CLAIM:       Holds for the regex as written: `["']?` consumes at most one
             opening `"`/`'`, so at most one closing quote sits at `capture_end`.
INPUT SHAPE: n/a (structural)
PATH/EXT:    all
TRACE:       Value class excludes `"`/`'`, so a capture cannot end on a quote
             byte; the closer is outside the capture. Unquoted captures leave
             `capture_end` on a non-quote (continuation is a no-op) — correct.
             **Caveat (not a C1 falsification):** the optional opener does **not**
             include `` ` ``. Backtick / template literals are a separate ceiling
             (capture can swallow backticks because `` ` `` ∈ `[^\s"'#]`), not a
             double-step bug in `right_hand_side_continuation`.
CONFIDENCE:  traced through the code
```

**Hold**, with the backtick caveat noted as out-of-scope ceiling rather than a
continuation off-by-one.

### C2 — walk PRECONDITION `from` outside any string literal

```
ID:          C2-precondition
DIRECTION:   none (for exemption A’s continuation helper)
CLAIM:       Holds for exemption A when the optional `["']?` opener fired: stepping
             one closer puts `from` outside that literal. Holds vacuously for
             unquoted placeholder captures (never entered a literal).
INPUT SHAPE: attacked via mismatched quotes / adjacent string concat
PATH/EXT:    code
TRACE:       `token = "placeholder""a-long-literal-value"` — continuation lands
             on the second literal’s opener; fence still fires (SENSITIVE). No
             inverted-parity CLEAN leak found for the A-path helper.
             Exemption B still relies on `match_is_inside_string_literal` + early
             return; that path was not newly broken by A.
CONFIDENCE:  traced through the code; spot-checked shapes (not exhaustive fuzz)
```

**Hold** for the A-path establishment via `right_hand_side_continuation`.

### C3 — `is_placeholder_only_expression` cannot accept a capture containing a hardcoded literal

```
ID:          C3-placeholder-only
DIRECTION:   none
CLAIM:       Holds under the consume-groups loop: longest-opener-first, brace-free
             interior, no leftover bytes. Adjacent `${A}${B}` ok; `${A}lit${B}`
             fails on `lit`. `find(close)` cannot swallow a neighbour because
             interiors reject `{`/`}` and openers are tried only at `rest`’s start.
INPUT SHAPE: nested / adjacent / `${{` vs `{{` / trim_matches interaction
PATH/EXT:    all (A has no language gate)
TRACE:       `trim_matches` of `"'` `` ` `` `<>[]` runs before the group walk —
             can turn `[${FOO_BAR}]` into a placeholder, but that still contains
             no hardcoded literal payload. `to_ascii_lowercase` does not create
             group openers. No capture constructed that is placeholder-true while
             still containing an 8+ literal run outside groups.
CONFIDENCE:  traced through the code
```

**Hold.**

### C4 — termination / DoS of `is_placeholder_only_expression`

```
ID:          C4-termination
DIRECTION:   termination
CLAIM:       Always terminates. Each successful group advances `rest`; each
             failing outer iteration returns false; capture length is bounded by
             the file scan limit / value run, and work is O(n * small constant).
INPUT SHAPE: many tiny `${a}` groups; pathological missing closers
PATH/EXT:    all
TRACE:       No recursion; no retry that re-scans the same prefix without
             progress. Missing closer → that opener fails → eventually `return false`.
CONFIDENCE:  traced through the code
```

**Hold** (not a practical DoS vector relative to the existing 512-byte walk and
scan-size cap).

### C5 — `line_break_continues_expression` CONTINUATION set

```
ID:          C5-continuation-ops
DIRECTION:   false-negative (accepted ceiling, if any)
CLAIM:       Mostly holds for rustfmt/black/prettier idioms named in-tree. Comma
             exclusion is deliberate (struct-field FP). I did not find a *common*
             depth-0 credential-carrying continuation that needs `,` `/` `<` `>`
             at line break.
INPUT SHAPE: trailing-comma parameter lists continue at depth>0 (newline arm
             only special-cases depth==0; deeper newlines fall through and keep
             scanning — measured intent of the walk).
PATH/EXT:    code formatters
TRACE:       Speculative residual: languages using `\` already covered; C# `\`
             not used; shell line-cont `\` covered. Implicit string concat across
             lines usually sits inside `()` (depth>0).
CONFIDENCE:  traced for depth>0 newline behaviour; speculative on “no common
             missed idiom” beyond the repo’s stated formatters
```

**Hold** for common formatters in scope; residual risk is speculative only.

### C6 — panic safety

```
ID:          C6-panic
DIRECTION:   panic
CLAIM:       Holds for the cited slices/indexes under Rust slice rules.
INPUT SHAPE: newline at end of window; `cursor += 2` escape near bound; depth
PATH/EXT:    all
TRACE:       `window[newline+1..]` is empty-safe when `newline` is last index.
             `bounded_char_literal_len`: `get(cursor)?` yields `None` (no panic)
             if escape jumps off the end; loop also ends when `cursor > last_close`.
             `interior[..end]` is on a `str` with ASCII `}` delimiter → char
             boundary. `depth: i32` cannot overflow inside a 512-byte window.
CONFIDENCE:  traced through the code
```

**Hold.**

---

## Ranked findings (false negatives first)

1. **Q1-P1 / Q1-P4 / P2 / P3 / P6** — depth-0 comma fix admits tuple-RHS credentials after placeholders (code). **Critical.**
2. **E2 / C3** — same fix admits credentials after `placeholder,` via exemption B (capture not even a placeholder). **Critical.**
3. **J1 / C4** — multi-declarator / second binding after placeholder first value; file-level admit if the second name is not itself a keyword match. **High.**
4. **DEFECT-Y1u** — brief’s exact YAML shape is a capture/placeholder FP, **not** a walk-crossing bug; candidate fix does not repair it. **High (correctness of proposed fix).**
5. **DEFECT-Y1q** — quoted flow sibling FP via walk; real, but must not be fixed by fail-open comma termination. **Medium (FP).**
6. **Q2-depth-precondition** — already-open `{` depth is wrong; fix that relies on it is structurally unsound. **Medium (design).**
7. **Oracle blind spot** — full `oracle_*` suite green under the broken fix. **Medium (process).**

---

## Measurement log

Worktree: `E:/project/symforge-rawread` @ `20b51c8`.

Commands:

- Temporary unit probe `knowledge::tests::_probe_adversarial_review_shapes` (removed;
  `git checkout -- src/knowledge/mod.rs`).
- Temporary depth-0 comma arm (removed; same checkout).
- `cargo test --lib -- knowledge::tests::_probe_adversarial_review_shapes --exact --nocapture`
- `cargo test --lib -- protocol::tools::tests::oracle_` → 27 passed with fix applied.
- Capture dump used the compiled `CONTEXT_ASSIGNMENT_RULE_ID` regex on the same
  bytes as production.

Worktree restored clean after measurement (`git status` empty). No commit made.

---

## Answers keyed to the brief

| Ask | Answer |
|---|---|
| **Q1** | Yes — multiple measured FNs; strongest is `password = "placeholder", "a-long-literal-value"` on `.py`. |
| **Q2** | Motivating `compute_header` stays SENSITIVE (opener visible). Already-open `{` is **not** tracked; property commas look depth-0. |
| **Q3** | Non-code short-circuit on exemption A’s walk + trailing-comma placeholder normalize for Y1u. Reject bare shared-walk comma termination. |
| **C1** | Hold (traced), backtick opener is a separate ceiling. |
| **C2** | Hold for A-path continuation (traced / spot-checked). |
| **C3** | Hold (traced). |
| **C4** | Hold (traced). |
| **C5** | Hold for stated formatters; residual speculative. |
| **C6** | Hold (traced). |
