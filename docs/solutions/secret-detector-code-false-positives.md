# Secret detector: code that names credentials without holding them

Owner ruling, 2026-11-19: an ordinary source file that only names or handles
credentials must not be withheld. This was measured on three large TypeScript
service and spec files in a downstream repository. The `secret.context-assignment`
rule withheld all three, but most of its findings were code shape, not credentials.

The behaviour lives in `src/knowledge/mod.rs`. The pins are:

- `code_handling_rows`, through `code_handling_credentials_matrix_pins`;
- `long_code_line_of_placeholders_scans_in_bounded_time`;
- `long_line_of_angle_values_scans_in_bounded_time`;
- G10b, in both the D1 matrix and `src/protocol/tools.rs`.

The secret policy version moved to 8.

## Path classes

`classify_scan_path` runs once per scan and yields four flags:

- `code`: the path is a code language.
- `rust`: the path is Rust.
- `relaxed`: the path is code and not config-like.
- `test`: the path is a test path.

A path is **config-like** (`is_config_like_path`) when any of these holds:

- it sits under a `config`, `settings` or `conf` directory;
- its basename contains `settings` (this covers `local_settings.py`) or contains
  `.config.` (`webpack.config.js`);
- its basename starts with `ormconfig`;
- it is `mix.exs`;
- it is a `*.gradle` or `*.gradle.kts` build script.

These are pinned by rows C5a to C5g and RV18 to RV20.

A path is a **test path** (`is_test_path`) when its basename contains `.spec.` or
`.test.`, or when it sits under a `test`, `tests`, `__tests__` or `fixtures`
directory.

All new relaxations need `relaxed`. Config, data and env files, and config-like
code, keep the version 7 verdicts. Config-like code still gets the older version 7
code-expression exemption, because making it fully strict would flag
`System.get_env(...)` in every Phoenix `config/runtime.exs`.

## The shapes and why each is safe

### Fixture literals (`literal_reads_as_fixture`)

A quoted value counts as a fixture only when all of these hold:

- every byte is ASCII: a letter, a digit, or one of `- _ space . ! %`;
- it has at most 3 segments;
- every segment is either letters only or a group of at most 3 digits;
- a digit group appears only when at least two words do (RV38);
- every word that is neither a marker nor a credential name is one case and at
  most 12 letters (RV36, RV37).

It then needs ONE of these:

- **A true marker word:** synthetic, fixture, fake, dummy, mock, stub,
  placeholder, example or sample.
- **A credential name with one short companion (test paths only):** one of
  password, passwd, secret, token, key or credential, plus at most one letters-only
  word of up to 8 characters. The companion must be one case. Only `-`, `_` or a
  space may separate the two, and there are no digits and no leading or trailing
  punctuation. For example, `stored-password` passes on a spec file but not in
  `src/` (RV31 to RV34).

A fixture literal is judged like a placeholder, so the rest of the right-hand side
is still walked. The walk may only start from a resume point that
`proven_literal_end` actually proves: the closing delimiter of the literal the
capture sits in, found on the same line, with no quote-fenced payload before it.
When the literal is ambiguous or has no closer, the value is not exempt. Step 2
never uses the version 7 fallback (RV29).

### Comparisons (`fenced_literal_is_benign`, `comparand_reads_as_word`)

Inside the right-hand-side walk, a quoted literal right after `==`, `===`, `!=` or
`!==` is not a payload when its content is a word: all lowercase, or one leading
capital followed by lowercase, or a listed credential name. A mixed-case or
letters-and-digits comparand still matches (RV30, CH1c).

### Next-line object entries (`line_opens_object_entry`)

After a trailing comma, the walk ends at a next line that opens `key:` or
`"key":`. Three cases are not treated as entries:

- `::`, which is a path (RV27);
- a key whose rest of line holds a bare `=` at depth 0, which is a typed declarator
  (RV21);
- comment and attribute lines, which keep the older carve-out.

That entry is its own key and is judged by the rule on its own. This retires the
G10b struct-literal regression.

### Interpolation inside a template literal (`right_hand_side_continuation`, `enclosing_literal_end`)

On a relaxed code path, a placeholder capture that sits inside a literal opened
earlier on the line resumes the walk past that literal's closing delimiter. Quote
state is read at the capture end, using the real Rust lifetime rule. The skipped
tail must not hold a quote-fenced payload of 8 or more bytes (RV16).

For a placeholder, an ambiguous state falls back to the version 7 walk (RV22,
RV39). The state counts as ambiguous when:

- more than one delimiter kind is open;
- the literal has no closer on the line;
- the line runs past `QUOTE_PARITY_LINE_BOUND` (4096 bytes) before the capture.

The fallback keeps apostrophes in prose and comments from becoming new findings
(RV13 to RV15). Off relaxed paths, the version 7 continuation applies unchanged
(RV11, RV12).

### Angle placeholders (`is_angle_placeholder`)

On a relaxed code path, an unquoted `<placeholder>` ends at the capture's end and
may be followed by closing punctuation. On other paths it still runs to the end of
the line.

The line end is searched within `ANGLE_PLACEHOLDER_LINE_BOUND` (4096 bytes). A
value with no line end inside that bound is not a placeholder.

## Performance bounds

Both line searches are bounded. On a debug build:

- A 1 MB single code line of placeholders took 140 s before the parity bound.
- A 1 MB line of angle values in an env file took 41 s before the angle bound; that
  quadratic behaviour predates this change.

Both now scan linearly.

## Accepted costs

- On a test path, a credential name plus one short one-case word is admitted, even
  if it was meant as a weak real password.
- A word-shaped comparand of `==` is admitted, even if it is a weak real password
  check.
- A marker word plus up to two one-case words of up to 12 letters is admitted,
  including all-lowercase random letters.
- In `x: Map<A>= v`, the `>=` is still read as a comparison, so that line counts as
  an object entry.
- The quoted env-var name row S9 in `src/protocol/tools.rs` (`API_KEY_NAME`) stays
  flagged.
- A 16-character base32 TOTP seed in a spec file is still flagged, even when it is
  the widely published example vector.
- Long multi-word fixture strings over the 3-segment cap are still flagged.
