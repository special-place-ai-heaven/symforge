# Independent review brief — secret-detector exemption logic

**Repository:** symforge (Rust, MCP code-intelligence server)
**File under review:** `src/knowledge/mod.rs`
**Commit:** `20b51c8` on branch `fix/raw-read-admission-gate`
**Date prepared:** 2026-07-31

You are one of **two independent reviewers** looking at the same code. Do not
try to guess what the other found or split the scope — the value here is two
uncoordinated passes over the same claims. Everything you need is inlined
below; you do not need the repository to do this review.

---

## 1. Read this first: the failure asymmetry

This code decides whether a source file is **published into a served index** or
**withheld**. The two error directions are not symmetric:

| Direction | Meaning | Severity |
|---|---|---|
| **False negative** (real credential reported CLEAN) | The file is published; its contents become readable through the server | **Severe — this is a credential leak** |
| **False positive** (clean code reported SENSITIVE) | The file is withheld; users lose search/read on a benign file | Real but tolerable — precision loss, not disclosure |

So: **hunting false negatives is the priority.** But false positives matter
enough to report, because a detector that refuses everything is useless and the
project has a whole-tree test that fails if ordinary source starts tripping it.

A defect that trades one false negative for a *wider* false negative is the
worst outcome, and has already happened once here (see §7).

---

## 2. How the detector works

`scan_secret_bytes(path, bytes)` runs five regex rules over a file's bytes and
returns `Clean`, `Sensitive`, or `Indeterminate`. Only one rule is in scope for
this review:

```rust
// rule id: "secret.context-assignment"
// keywords that must appear somewhere in the file for the rule to run at all:
//   key, secret, token, password, passwd, pwd
r#"(?i)(?:api[_-]?key|secret|token|password|passwd|pwd|client[_-]?secret)[ \t]*[:=][ \t]*["']?([^\s"'#]{8,})"#
// capture group 1 is "the value"; placeholders_allowed = true for this rule
```

**Critical property of the capture** (reviewers commonly get this wrong): the
value class is `[^\s"'#]{8,}`. It **cannot cross a quote, whitespace, or `#`**.
The leading `["']?` consumes an *opening* quote if present, so for
`token = "abcdefghij"` the capture is `abcdefghij` and `capture_end` points at
the **closing** quote. For `token = compute_header('x', 'y')` the capture is
`compute_header(` — it stops at the first `'`.

Relevant constants:

```rust
const CONTEXT_ASSIGNMENT_MIN_PAYLOAD: usize = 8;   // fenced-run floor
const CONTEXT_ASSIGNMENT_SCAN_BOUND: usize = 512;  // walk window
const CHAR_LITERAL_MAX_CONTENT: usize = 3;
```

For each match, two **exemptions** can suppress the finding. Both now funnel
through the same withdrawal test. This is the code under review:

```rust
for captures in rule.pattern.captures_iter(bytes) {
    let Some(secret) = captures.get(rule.secret_capture) else { /* Indeterminate */ };

    if rule.placeholders_allowed && is_placeholder(secret.as_bytes()) {
        // EXEMPTION A — the capture is a placeholder.
        // The capture is exempt; the EXPRESSION is not. Keep inspecting.
        if !expression_carries_quoted_payload(
            bytes,
            right_hand_side_continuation(bytes, secret.end()),
        ) {
            continue;                 // suppressed -> file may stay CLEAN
        }
    } else if rule.id == CONTEXT_ASSIGNMENT_RULE_ID
        && assignment_is_code_expression(path, bytes, secret.start(), secret.as_bytes())
    {
        continue;                     // EXEMPTION B — suppressed
    }
    finding_count += 1;               // SENSITIVE
}
```

**Language gate:** `assignment_is_code_expression` (exemption B) returns `false`
immediately for non-code paths. `is_code_language()` is **false** for JSON,
TOML, YAML, Markdown, Text, Env, HTML, CSS, SCSS, and for any unknown
extension. It is **true** for Rust, Python, JavaScript, TypeScript, Java, Go,
C#, Ruby, PHP, and similar.

**Exemption A has no language gate.** It runs on every path class. That is new
in this commit and is the single largest behavioural change under review.

---

## 3. The code, verbatim

### 3a. Placeholder recognition

```rust
fn is_placeholder(value: &[u8]) -> bool {
    let Ok(value) = std::str::from_utf8(value) else {
        return false;
    };
    let normalized = value
        .trim_matches(|character: char| {
            matches!(character, '"' | '\'' | '`' | '<' | '>' | '[' | ']')
        })
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "example" | "sample" | "placeholder" | "changeme" | "change-me"
            | "change_me" | "redacted" | "replace-me" | "replace_me"
            | "dummy" | "fake" | "test-value"
    ) || normalized.starts_with("your_")
        || normalized.starts_with("your-")
        || is_placeholder_only_expression(&normalized)
}

/// True when the capture consists ONLY of interpolation placeholder groups,
/// one or more, back to back, with nothing substantive between them.
/// Openers are tried longest-first so GitHub Actions `${{ ... }}` reads as one
/// group rather than as `${` leaving a stray brace.
fn is_placeholder_only_expression(value: &str) -> bool {
    const GROUPS: [(&str, &str); 3] = [("${{", "}}"), ("{{", "}}"), ("${", "}")];
    let mut rest = value;
    let mut matched_any = false;
    'consume: while !rest.is_empty() {
        for (open, close) in GROUPS {
            let Some(interior) = rest.strip_prefix(open) else { continue };
            let Some(end) = interior.find(close) else { continue };
            if end == 0
                || interior[..end].bytes().any(|b| matches!(b, b'{' | b'}'))
            {
                continue;
            }
            rest = &interior[end + close.len()..];
            matched_any = true;
            continue 'consume;
        }
        return false;
    }
    matched_any
}

/// Where to resume inspecting a right-hand side after a capture ends.
/// The rule's optional `["']?` consumed an OPENING quote, so the matching
/// closing quote sits at `capture_end`; step over it so the walk begins
/// outside that literal.
fn right_hand_side_continuation(bytes: &[u8], capture_end: usize) -> usize {
    match bytes.get(capture_end) {
        Some(b'"' | b'\'' | b'`') => capture_end + 1,
        _ => capture_end,
    }
}
```

### 3b. The withdrawal walk

Returns `true` = exemption **WITHDRAWN** (file stays SENSITIVE).
Returns `false` = expression **CONSUMED** (exemption granted, file may be CLEAN).

```rust
fn expression_carries_quoted_payload(bytes: &[u8], from: usize) -> bool {
    let end = from.saturating_add(CONTEXT_ASSIGNMENT_SCAN_BOUND).min(bytes.len());
    let window = &bytes[from..end];
    let truncated = end < bytes.len();
    let is_payload = |byte: u8| {
        !matches!(byte, b'"' | b'\'' | b'`' | b'#') && !byte.is_ascii_whitespace()
    };

    let mut depth: i32 = 0;
    let mut index = 0;
    while index < window.len() {
        let byte = window[index];
        match byte {
            b'"' | b'\'' | b'`' => {
                // Fenced-payload test, evaluated the moment the fence opens.
                let mut run = index + 1;
                while run < window.len() && is_payload(window[run]) {
                    run += 1;
                }
                if run - (index + 1) >= CONTEXT_ASSIGNMENT_MIN_PAYLOAD
                    && window.get(run) == Some(&byte)
                {
                    return true;                      // payload found
                }
                if byte == b'\'' {
                    index += bounded_char_literal_len(window, index).unwrap_or(1);
                } else {
                    // Skip the whole literal so brackets inside cannot move depth.
                    let mut cursor = index + 1;
                    loop {
                        match window.get(cursor) {
                            None => return true,      // unterminated -> fail closed
                            Some(b'\\') => cursor += 2,
                            Some(other) if *other == byte => break,
                            Some(_) => cursor += 1,
                        }
                    }
                    index = cursor + 1;
                }
            }
            b'(' | b'[' | b'{' => { depth += 1; index += 1; }
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth < 0 {
                    return false;   // match sat inside an enclosing group: consumed
                }
                index += 1;
            }
            b'\n' if depth == 0 => {
                if !line_break_continues_expression(window, index) {
                    return false;   // statement ended
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    truncated || depth != 0        // unbalanced or over-bound -> fail closed
}

fn line_break_continues_expression(window: &[u8], newline: usize) -> bool {
    const CONTINUATION: &[u8] = b"+-*|&^%=.?:\\";
    let trailing = window[..newline]
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|i| window[i]);
    let leading = window[newline + 1..]
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .copied();
    trailing.is_some_and(|b| CONTINUATION.contains(&b))
        || leading.is_some_and(|b| CONTINUATION.contains(&b))
}

/// Skip a char literal ONLY when it is bounded, bracket-bearing, and does not
/// cross a line. The bracket requirement exists because `'` is a STRING
/// delimiter in Python/JS/TS/Ruby/PHP — see §7.
fn bounded_char_literal_len(window: &[u8], at: usize) -> Option<usize> {
    let last_close = at + 1 + CHAR_LITERAL_MAX_CONTENT;
    let mut cursor = at + 1;
    let mut carries_bracket = false;
    while cursor <= last_close {
        match window.get(cursor)? {
            b'\\' => cursor += 2,
            b'\n' => return None,
            b'\'' if cursor > at + 1 => {
                return carries_bracket.then_some(cursor + 1 - at);
            }
            byte => {
                carries_bracket |= matches!(byte, b'(' | b')' | b'[' | b']' | b'{' | b'}');
                cursor += 1;
            }
        }
    }
    None
}
```

---

## 4. PRIMARY ASK — a confirmed defect and a candidate fix

### The defect (measured, reproducible)

On a **`.yaml`** path, this input is now reported **SENSITIVE** and should be
**CLEAN**:

```yaml
{token: ${TOKEN_NAME}, banner: "a-long-literal-value"}
```

Trace: `token:` matches the rule; the capture is `${TOKEN_NAME}`, which stops at
the `,`. `is_placeholder_only_expression` returns true, so exemption A applies to
the capture. The new walk then resumes at `capture_end` (the `,` — not a quote,
so `right_hand_side_continuation` does not advance) and walks **straight through
the depth-0 comma into the next key's value**, finds the fenced 20-byte run
`a-long-literal-value`, and withdraws the exemption.

This is a **false positive** — the safe direction — but it is a regression
introduced by this commit, and it only shows up on non-code paths because
exemption A lost its language gate.

Five sibling shapes were measured **CLEAN** and are *not* affected: pretty-printed
JSON, single-line JSON, YAML block style, `.env`, and TOML. (Quoted JSON keys
never match the rule at all, because the regex needs the keyword immediately
followed by optional spaces and then `:` or `=`, and `"token":` has a `"` in
between. That is a pre-existing ceiling, not part of this review.)

### The candidate fix I want you to attack

Terminate the walk at a **depth-0 comma**, mirroring the existing depth-0
newline rule, on the theory that a comma at depth zero means *the element
ended*:

```rust
b',' if depth == 0 => return false,     // element ended: consumed
```

**Why I am not confident, and what I want from you:**

This makes the walk stop *earlier*, which grants *more* exemptions — the
**fail-open** direction, the one this walk may never take. Before I apply it I
want an independent attempt to break it.

- **Q1 (primary).** Construct a source shape in **any** language where a real
  hardcoded credential sits after a **depth-0 comma** in a credential-bound
  right-hand side, such that this fix would newly report it CLEAN. If you find
  one, this fix is wrong and I need a different approach.
- **Q2.** Does the fix preserve the case that motivated the walk crossing
  commas in the first place — `token = compute_header('label', '<credential>')`,
  where the comma sits at depth 1 inside the parentheses? Verify the depth
  bookkeeping actually holds when the walk *starts inside* an already-open
  bracket (the walk begins after the capture, so an enclosing `(` opened before
  `capture_end` was never counted, and `depth` starts at 0 regardless).
- **Q3.** Is there a better fix? Candidates I considered but did not evaluate:
  restore a language gate on exemption A; make the walk line-local for non-code
  paths; or treat any depth-0 structural separator (`,` `;`) as a terminator.
  Argue for one.

---

## 5. SECONDARY — claims to attack

Each of these is a claim I believe but have not adversarially tested. Try to
falsify them. State clearly whether your conclusion is *traced through the code*
or *speculative*.

- **C1.** `right_hand_side_continuation` steps over exactly **one** closing
  quote. Claim: this is always correct, because the regex's `["']?` consumed at
  most one opening quote. Attack: backtick/template-literal captures; captures
  that were never quote-wrapped; a capture whose last byte is itself a quote;
  multi-byte UTF-8 immediately after `capture_end`.
- **C2.** The walk's stated PRECONDITION is that `from` is provably **outside**
  any string literal. Exemption A is a *new second caller* establishing this via
  `right_hand_side_continuation`. Claim: it holds. Attack: find an input where
  the walk begins *inside* a literal, which inverts quote parity for the whole
  512-byte window and can flip the verdict either way.
- **C3.** `is_placeholder_only_expression` cannot be made to accept a capture
  containing a hardcoded literal. Attack: nesting, adjacent openers, an opener
  with no closer, `${{` vs `{{` ordering, `find(close)` locating a *later*
  closer than intended, and the interaction with `to_ascii_lowercase()` and the
  `trim_matches` of `"'` backtick `<>[]` that runs **before** it.
- **C4.** Termination and cost. Claim: `is_placeholder_only_expression` always
  terminates and is not a DoS vector. Attack: adversarial captures (many tiny
  groups; pathological `find` behaviour). Note the capture is bounded by the
  value class but a single capture can still be long.
- **C5.** `line_break_continues_expression`'s `CONTINUATION` set is
  `+-*|&^%=.?:\` — `,` `/` `<` `>` are deliberately excluded, each for a
  measured false positive. Claim: no *common* continuation style that carries a
  credential onto the next line is missed. Attack: name a real formatter or
  language idiom that this misses.
- **C6.** Panic safety. Claim: no index arithmetic here can panic. Attack:
  `window[..newline]` / `window[newline + 1..]`; `interior[..end]` on a
  multi-byte UTF-8 boundary; `bounded_char_literal_len`'s `cursor += 2` escape
  overshooting `last_close`; `depth` overflow. A panic in the detector is a
  denial of service on indexing.

---

## 6. What is already verified — please do not spend time here

- The whole test suite passes: 3080 unit tests plus 111 integration binaries,
  zero failures.
- Each fix in this commit has a mutation check: the fix was neutralized, and
  **only** its own test failed, with the total test count unchanged (so the
  mutation perturbed behaviour, not the measurement).
- A whole-tree tripwire scans all of `src/` and `tests/` with this detector and
  requires zero findings — so gross false positives on Rust source are already
  excluded. **It does not scan config files**, which is exactly why the §4
  defect survived.
- Two claims from an earlier review did **not** reproduce and are settled: a
  leading-dot method chain, and a `/* */` block-comment continuation line
  beginning with `*`, both measured CLEAN.

---

## 7. Calibration — the bug that already got through

Read this before you start; it tells you what class of defect survives here.

Earlier in this work I added the `bounded_char_literal_len` skip to fix a case
where a `')'` **char literal** underflowed the walk's `depth` and consumed the
walk ahead of a real credential. I reasoned about it purely as a Rust question:
char literal versus lifetime sigil.

But `'` is a **string delimiter** in Python, JavaScript, TypeScript, Ruby and
PHP — all code languages here. In those languages the byte the skip fires on is
usually a string's *closing* quote, and the next `'` within the 3-byte bound is
the *opening* quote of the following argument, because `', '` is a two-byte gap.
The skip jumped clean over that opening quote — and the fenced-payload test only
ever fires when the walk **lands on** a quote. So a credential in the second
argument was never tested at all.

Measured: `token = compute_header('label', '<credential>')` was CLEAN in both
Python and JavaScript, while the identical line with the **first** argument
double-quoted was SENSITIVE.

My safety comment had argued the bound sits far below the 8-byte payload
minimum, so the skip "can never hide a fenced payload." **That was false, and it
constrained the wrong quantity.** A skip never has to hide the payload *run* —
hiding its single opening *fence byte* is enough to blind the test completely.

Two lessons worth carrying into your pass:

1. **Ask what the check needs to SEE, not how much a shortcut can hide.**
2. **Ask which other languages or formats give this byte a different meaning.**
   A full green test suite did not catch the above, because every test row was
   Rust with double-quoted payloads.

---

## 8. How to report back

For each finding:

```
ID:          short handle
DIRECTION:   false-negative (leak) | false-positive | panic | termination | none
CLAIM:       one sentence
INPUT SHAPE: the triggering input, described concretely
             (use a benign filler like "a-long-literal-value" — see §9)
PATH/EXT:    which file extension it applies to, and whether that is a code language
TRACE:       step through the actual code: what the CAPTURE is, which exemption
             applies, and what the walk returns at each decision point
CONFIDENCE:  traced through the code | plausible but not traced
```

If you conclude a claim in §5 **holds**, say so explicitly and show the
reasoning — a confirmed-safe verdict is a useful result, not a null one. If you
cannot construct a counterexample for **Q1**, say that plainly rather than
inventing a weak one; "I tried these five shapes and none worked" is exactly
what I need to hear.

Rank findings by direction first (false negatives above all), then by how common
the triggering shape is in real source.

---

## 9. Ground rules

- **Never write a realistic credential into your output.** Use obviously-benign
  filler such as `a-long-literal-value` or `DEPLOY_TOKEN_NAME`. Describe secret
  shapes abstractly ("a 20-byte payload run between two single quotes").
- Prefer tracing the actual code above over pattern-matching to detectors you
  have seen elsewhere. The capture semantics in §2 are the most common source of
  wrong analysis — re-derive what the capture actually contains before reasoning
  about any input.
- This is defensive security work on the author's own repository.

### If you have the repository open (Cursor)

```bash
# from the worktree root
cargo test --lib -- protocol::tools::tests::oracle_    # the row-based oracle suite
```

Existing rows live in `src/protocol/tools.rs` under `mod tests` and use the
helper `oracle_rows_off_expectation(&rows, expect_demoted)`. To test a shape,
add a row and assert its expected verdict. Please **do not commit**; report the
row and its measured verdict instead.
