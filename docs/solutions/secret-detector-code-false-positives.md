# Secret detector: code that names credentials without holding them

Owner ruling, 2026-11-19: an ordinary source file that only names or handles
credentials must not be withheld. This was measured on three large TypeScript
service and spec files in a downstream repository. The `secret.context-assignment`
rule withheld all three, but most of its findings were code shape, not credentials.

The behaviour lives in `src/knowledge/mod.rs`. The pins are `code_handling_rows`,
`code_handling_credentials_matrix_pins` and
`long_code_line_of_placeholders_scans_in_bounded_time`, plus G10b in both the D1
matrix and `src/protocol/tools.rs`. The secret policy version moved to 8.

## Which paths get the relaxed rules

All new relaxations apply only when `relaxed_code_path` holds. That means the path
is a code language (`is_code_path`) and is not config-like (`is_config_like_path`).
A path is config-like when any of these holds:

- it sits under a `config` directory;
- it is a `*.gradle` or `*.gradle.kts` build script;
- it is a `settings*.py` file.

Config, data and env files, and config-like code, keep the version 7 verdicts.

Config-like code still gets the older version 7 code-expression exemption.
Making it fully strict would flag `System.get_env(...)` in every Phoenix
`config/runtime.exs`.

## The shapes and why each is safe

**Fixture literals** (`literal_reads_as_fixture`). A quoted value counts as a
fixture only when all of these hold:

- every byte is ASCII and is a letter, a digit, or one of `- _ space . ! %`;
- it has at most 3 segments;
- every segment is either letters only or a group of at most 3 digits;
- every letters-only segment is at most 12 characters, unless it is a listed word.

On top of that, the value must also meet ONE of these two conditions:

- **A true fixture marker:** one segment is synthetic, fixture, fake, dummy, mock,
  stub, placeholder, example or sample.
- **A credential name with one short companion word:** one segment is password,
  passwd, secret, token, key or credential, plus at most one letters-only word of
  up to 8 characters. Only `-`, `_` or a space may separate them, and the value has
  no digits and no leading or trailing punctuation. `stored-password` passes this
  way.

These counted as fixtures under the first version of this change and must not.
Review found them, and each is now a matrix row (RV1 to RV10):

- a credential name used alone as a marker, or the word `test` as a marker;
- no bound on segment count or length;
- non-ASCII bytes acting as separators.

A fixture literal is judged like a placeholder. The rest of the right-hand side is
still walked, so `"fixture" + "<real>"` stays caught.

**Comparisons** (`fenced_literal_is_benign`). Inside the right-hand-side walk, a
quoted literal right after `==`, `===`, `!=` or `!==` whose content is letters only
is not a payload. This decides from the shape of the code, not from a word list.
A comparand that mixes letters and digits still matches, which pins hardcoded
password checks.

**Next-line object entries** (`line_opens_object_entry`). After a trailing comma,
the walk ends at a next line that opens `key:` or `"key":`. That rules out three
cases:

- `::`, which is a path;
- a key whose rest of line holds a bare `=` at depth 0, which is a typed
  declarator such as `other: string = …` (RV21);
- comment and attribute lines, which keep the older carve-out.

That entry is its own key and is judged by the rule on its own. This retires the
G10b struct-literal regression that the comma rule had accepted.

**Interpolation inside a template literal** (`right_hand_side_continuation`,
`enclosing_literal_end`). On a relaxed code path, a placeholder capture that sits
inside a literal opened earlier on the line resumes the walk past that literal's
closing delimiter. Quote state is computed at the capture END, with the real Rust
lifetime rule. The skipped tail must not hold a quote-fenced payload run of 8 or
more bytes (RV16).

When the state is ambiguous, the walk falls back to the version 7 behaviour:

- more than one delimiter kind is open;
- no closer exists on the line;
- the line is longer than `QUOTE_PARITY_LINE_BOUND` (4096 bytes) before the capture.

That fallback is what keeps apostrophes in prose and comments from turning into
new findings (RV13 to RV15). The 4096-byte bound is also what keeps a minified
single-line bundle linear. Before the bound, a 1 MB line took 140 s in a debug
build.

Off relaxed paths, the version 7 continuation applies unchanged (RV11, RV12).

**Angle placeholders in code comments**. On a relaxed code path, an unquoted
`<placeholder>` capture ends at the capture's end, not at the end of the line. It
may be followed by closing punctuation, for example a markdown backtick and `),`.

## Accepted costs

- A name plus one short word (`orbit-key`) is admitted in code, even if someone
  meant it as a weak real password.
- A letters-only comparand of `==` is admitted, even if it is a weak real password
  check.
- The quoted env-var name row S9 in `src/protocol/tools.rs` (`API_KEY_NAME`) stays
  flagged. It has three segments around a credential name, which the
  name-with-companion rule refuses.
- A 16-character base32 TOTP seed in a spec file is still flagged. That holds even
  when it is the widely published example vector, because letters mixed with
  digits cannot be told apart from a real seed by shape.
