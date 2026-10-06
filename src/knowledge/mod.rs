//! Repository-knowledge admission and exact-text helpers.
//!
//! Knowledge reuses the live index's byte store and Markdown section records;
//! this module owns only policy and projection seams, never a second corpus.

pub mod secret_dismissals;

/// UTF-8 byte-order mark accepted by the v1 searchable-text contract.
pub const UTF8_BOM: &[u8; 3] = b"\xEF\xBB\xBF";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedText<'a> {
    pub text: &'a str,
    /// Number of leading bytes omitted from `text`. Callers that publish source
    /// offsets must add this to offsets derived from the decoded slice.
    pub leading_bytes: u32,
}

/// Decode the only searchable text encodings accepted in v1: UTF-8 and UTF-8
/// with a leading BOM. No lossy conversion is ever attempted.
pub fn decode_searchable_text(bytes: &[u8]) -> Result<DecodedText<'_>, std::str::Utf8Error> {
    let (text_bytes, leading_bytes) = bytes
        .strip_prefix(UTF8_BOM)
        .map_or((bytes, 0), |without_bom| {
            (without_bom, UTF8_BOM.len() as u32)
        });
    std::str::from_utf8(text_bytes).map(|text| DecodedText {
        text,
        leading_bytes,
    })
}

/// Bumped to 2: the detector's verdicts moved (bounded balanced right-hand-side
/// consumption, embedded-literal tightening) and whole-buffer encoding
/// validation now runs on code paths, so every manifest persisted under v1 is
/// stale and must be re-scouted rather than trusted.
///
/// Bumped to 3: the context-assignment rule now enters inline arrays and skips
/// short leading elements (`key = ["dev", "<cred>"]`), so files a v2 manifest
/// recorded as clean can carry findings under v3.
///
/// Bumped to 4: the context-assignment and uri-credentials verdicts moved in the
/// other direction (a quote that CLOSES an earlier literal, a comma before a
/// comment or attribute line, a Rust lifetime, `<`-prefixed and userinfo
/// placeholders). A v3 manifest withholds files v4 admits, so it must be
/// re-scouted rather than trusted.
///
/// Bumped to 5: `secret_access_key` (the INI credential-profile key) now
/// reaches the context-assignment rule, so a v4 manifest admits files v5
/// withholds.
///
/// Bumped to 6: quoted credential keys (`"password": "…"`) now reach the
/// context-assignment rule, with label-like and placeholder values exempted,
/// so a v5 manifest admits files v6 withholds.
///
/// Bumped to 7: the natural-language label exemption no longer admits
/// Diceware-style multi-word or long Title-Case mashed passphrases under
/// credential keys, so a v6 manifest admits files v7 withholds.
pub const SECRET_POLICY_VERSION: u32 = 7;
pub const SECRET_SCAN_MAX_BYTES: usize = crate::domain::index::METADATA_ONLY_CODE_BYTES as usize;
/// The one reserved rule id every [`DetectorFailure`] collapses onto. Public so
/// the disclosure gate can tell an indeterminate verdict — which a reindex
/// cannot change — from a real content match.
pub const INDETERMINATE_RULE_ID: &str = "secret.detector.indeterminate";
const CONTEXT_ASSIGNMENT_RULE_ID: &str = "secret.context-assignment";
const URI_CREDENTIALS_RULE_ID: &str = "secret.uri-credentials";
/// Mirrors the `{8,}` payload floor inside that rule's pattern.
const CONTEXT_ASSIGNMENT_MIN_PAYLOAD: usize = 8;
/// Bytes of right-hand-side expression the exemption test will read. Five to six
/// full-width formatter lines (rustfmt 100, black 88, prettier 80), so every
/// wrapped argument list a formatter produces fits. Both error directions land on
/// SENSITIVE — under-scan exhausts the window, over-scan runs into neighbouring
/// code — so no value of this constant can create a false negative.
const CONTEXT_ASSIGNMENT_SCAN_BOUND: usize = 512;

/// Longest char-literal CONTENT the withdrawal walk will skip whole: covers
/// `'x'`, an escape pair like `'\n'` or `'\''`, and a BMP multibyte char, and
/// sits far below [`CONTEXT_ASSIGNMENT_MIN_PAYLOAD`] so the skip itself can
/// never jump a fenced payload. Anything longer falls back to the one-byte
/// advance, which fails closed.
const CHAR_LITERAL_MAX_CONTENT: usize = 3;

/// Whether a buffer exceeds the deterministic scan budget, i.e. the detector
/// will refuse to inspect it. Exposed so the disclosure gate can distinguish
/// "scanned and matched" from "never scanned": [`classify_stable_content`]
/// collapses every [`DetectorFailure`] into one `SensitiveContent` rule id.
pub fn exceeds_scan_limit(len: usize) -> bool {
    len > SECRET_SCAN_MAX_BYTES
}

/// How many finding line ranges a scan keeps: the ten a refusal shows, plus one
/// so it knows to say there are more.
pub const FINDING_LINE_RANGES_KEPT: usize = 11;

/// The first [`FINDING_LINE_RANGES_KEPT`] runs of set bits, as inclusive
/// 1-based line ranges. Bit `n` is line `n + 1`.
fn first_line_ranges(line_bits: &[u64]) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for (word_index, &word) in line_bits.iter().enumerate() {
        let mut rest = word;
        while rest != 0 {
            let bit = word_index * 64 + rest.trailing_zeros() as usize;
            rest &= rest - 1;
            let line = u32::try_from(bit + 1).unwrap_or(u32::MAX);
            match ranges.last_mut() {
                Some((_, end)) if end.checked_add(1) == Some(line) => *end = line,
                _ => {
                    if ranges.len() == FINDING_LINE_RANGES_KEPT {
                        return ranges;
                    }
                    ranges.push((line, line));
                }
            }
        }
    }
    ranges
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetectorFailure {
    PolicyCompilation,
    ResourceLimit,
    Internal,
}

/// Safe per-finding descriptor for withheld `_meta` (034). Never carries secret bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretFindingDescriptor {
    pub rule_id: &'static str,
    /// Inclusive 1-based line of the capture start (end equals start today).
    pub line_start: u32,
    pub line_end: u32,
    /// Shape string — length + charset class + rule id; never a substring of the value.
    pub shape: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretScan {
    Clean,
    Sensitive {
        rule_ids: Vec<&'static str>,
        finding_count: u32,
        /// Inclusive 1-based line ranges holding a finding's capture,
        /// ascending, merged across rules, and only the first
        /// [`FINDING_LINE_RANGES_KEPT`]: one more than a refusal shows, so it
        /// can say more exist. Lines, never bytes: enough to locate a false
        /// positive, nothing of the value. Rendered by the read gate at refusal
        /// time and never persisted, so the snapshot schema does not change.
        line_ranges: Vec<(u32, u32)>,
        /// Per-finding descriptors (capped like `line_ranges`) for actionable
        /// refusal metadata. Shape strings only — never secret bytes.
        findings: Vec<SecretFindingDescriptor>,
    },
    Indeterminate {
        reason: DetectorFailure,
    },
}

/// Describe a captured secret's form without embedding any of its bytes.
///
/// Output is stable for equal forms and is asserted by 034 oracles to omit the
/// synthetic secret string from serialized results.
pub fn describe_secret_shape(value: &[u8], rule_id: &str) -> String {
    let len = value.len();
    let has_digit = value.iter().any(u8::is_ascii_digit);
    let has_upper = value.iter().any(u8::is_ascii_uppercase);
    let has_lower = value.iter().any(u8::is_ascii_lowercase);
    let has_symbol = value.iter().any(|b| !b.is_ascii_alphanumeric());
    let charset = match (has_upper, has_lower, has_digit, has_symbol) {
        (_, _, true, true) => "mixed alnum+symbol",
        (true, true, true, false) => "mixed alnum",
        (_, _, true, false) => "alnum with digits",
        (_, _, false, true) => "alpha+symbol",
        (true, true, false, false) => "alpha mixed case",
        (false, true, false, false) => "lowercase alpha",
        (true, false, false, false) => "uppercase alpha",
        _ => "opaque bytes",
    };
    format!("{len}-char {charset} ({rule_id})")
}

/// Internal span for remediation rewrites. Held only in-process; never serialized
/// to agents. `value_start`/`value_end` index the secret capture in `bytes`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretMatchSpan {
    pub rule_id: &'static str,
    pub line_start: u32,
    pub line_end: u32,
    pub shape: String,
    pub value_start: usize,
    pub value_end: usize,
}

/// Like [`scan_secret_bytes`], but also returns byte spans for each kept finding
/// so remediation can rewrite without a second detector. Spans never leave the
/// remediation apply path as wire data.
/// Result of [`scan_secret_spans`]. Capture-miss / compile / limit failures are
/// [`Indeterminate`](SecretSpansScan::Indeterminate) — matching [`scan_secret_bytes`] —
/// never an empty span list that would look Clean.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretSpansScan {
    Spans(Vec<SecretMatchSpan>),
    Indeterminate { reason: DetectorFailure },
}

pub fn scan_secret_spans(path: &str, bytes: &[u8]) -> SecretSpansScan {
    if exceeds_scan_limit(bytes.len()) {
        return SecretSpansScan::Indeterminate {
            reason: DetectorFailure::ResourceLimit,
        };
    }
    let rules = match SECRET_RULES.get_or_init(compile_secret_rules) {
        Ok(rules) => rules,
        Err(reason) => {
            return SecretSpansScan::Indeterminate { reason: *reason };
        }
    };
    let mut spans = Vec::new();
    for rule in rules {
        if !rule
            .keywords
            .iter()
            .any(|keyword| contains_ascii_case_insensitive(bytes, keyword))
        {
            continue;
        }
        let (mut cursor, mut line) = (0_usize, 0_usize);
        for captures in rule.pattern.captures_iter(bytes) {
            let Some(secret) = (if rule.secret_capture == 0 {
                captures.get(0)
            } else {
                (1..=rule.secret_capture).find_map(|index| captures.get(index))
            }) else {
                return SecretSpansScan::Indeterminate {
                    reason: DetectorFailure::Internal,
                };
            };
            let match_start = context_assignment_match_start(
                rule.id,
                bytes,
                captures
                    .get(0)
                    .map_or(secret.start(), |whole| whole.start()),
            );
            if rule.placeholders_allowed
                && (is_placeholder(secret.as_bytes())
                    || is_angle_placeholder(bytes, secret.start()))
            {
                if right_hand_side_continuation(bytes, secret.start(), secret.end())
                    .is_some_and(|from| !expression_carries_quoted_payload(bytes, from, false))
                {
                    continue;
                }
            } else if (rule.id == CONTEXT_ASSIGNMENT_RULE_ID
                && (assignment_is_code_expression(
                    path,
                    bytes,
                    match_start,
                    secret.start(),
                    secret.as_bytes(),
                ) || !assignment_value_looks_like_credential(
                    bytes,
                    match_start,
                    secret.start(),
                    secret.as_bytes(),
                )))
                || (rule.id == URI_CREDENTIALS_RULE_ID
                    && uri_credentials_are_placeholder(bytes, secret.start(), secret.end()))
            {
                continue;
            }
            line += bytes[cursor..secret.start()]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count();
            cursor = secret.start();
            if spans.len() < FINDING_LINE_RANGES_KEPT {
                let line_1based = u32::try_from(line.saturating_add(1)).unwrap_or(u32::MAX);
                spans.push(SecretMatchSpan {
                    rule_id: rule.id,
                    line_start: line_1based,
                    line_end: line_1based,
                    shape: describe_secret_shape(secret.as_bytes(), rule.id),
                    value_start: secret.start(),
                    value_end: secret.end(),
                });
            }
        }
    }
    SecretSpansScan::Spans(spans)
}

struct SecretRule {
    id: &'static str,
    keywords: &'static [&'static [u8]],
    pattern: regex::bytes::Regex,
    secret_capture: usize,
    placeholders_allowed: bool,
}

static SECRET_RULES: std::sync::OnceLock<Result<Vec<SecretRule>, DetectorFailure>> =
    std::sync::OnceLock::new();

fn compile_secret_rules() -> Result<Vec<SecretRule>, DetectorFailure> {
    #[allow(clippy::type_complexity)]
    let definitions: &[(&str, &[&[u8]], &str, usize, bool)] = &[
        (
            "secret.private-key-envelope",
            &[b"PRIVATE KEY"],
            r"-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----",
            0,
            false,
        ),
        (
            "secret.authorization-header",
            &[b"authorization"],
            r"(?i)authorization[ \t]*[:=][ \t]*(?:bearer|basic)[ \t]+([A-Za-z0-9._~+/=-]{8,})",
            1,
            false,
        ),
        (
            "secret.provider-token",
            &[b"gh"],
            r"gh[pousr]_[A-Za-z0-9]{36,}",
            0,
            false,
        ),
        (
            CONTEXT_ASSIGNMENT_RULE_ID,
            &[b"key", b"secret", b"token", b"password", b"passwd", b"pwd"],
            // Optional quotes around the key so JSON / JS object literals
            // (`"password": "…"`) match; unquoted keys keep working. After the
            // separator: an optional inline-array opener, then EITHER
            // one-or-more short quoted elements (each under the capture floor,
            // so they could never match on their own) followed by a mandatory
            // quoted value, OR a plain quoted value (spaces allowed — UI
            // labels), OR an unquoted run. The mandatory quote after a skip is
            // load-bearing: without it the capture lands on the next unquoted
            // run past the comma — in a YAML flow mapping that is the NEXT KEY
            // (`{token: "ab", environment: production}` would capture
            // `environment:`), the exact false-positive class the FO-5 ruling
            // refuses. Zero skips keeps the historical shape. Capture groups 1
            // (array element), 2 (quoted value), 3 (unquoted) are mutually
            // exclusive; the scanner takes the first that participated.
            r#"(?i)["']?(?:api[_-]?key|access[_-]?key|private[_-]?key|secret(?:[_-]?access[_-]?key)?|token|password|passwd|pwd|client[_-]?secret)["']?[ \t]*[:=][ \t]*(?:\[[ \t]*)?(?:(?:["'][^"'\n]{0,7}["'][ \t]*,[ \t]*)+["']([^"'\n]{8,})["']|["']([^"'\n]{8,})["']|([^\s"'#]{8,}))"#,
            3,
            true,
        ),
        (
            URI_CREDENTIALS_RULE_ID,
            &[b"://"],
            r"://[^/\s:@]+:([^@\s/]{4,})@",
            1,
            false,
        ),
    ];

    definitions
        .iter()
        .map(
            |(id, keywords, pattern, secret_capture, placeholders_allowed)| {
                regex::bytes::Regex::new(pattern)
                    .map(|pattern| SecretRule {
                        id,
                        keywords,
                        pattern,
                        secret_capture: *secret_capture,
                        placeholders_allowed: *placeholders_allowed,
                    })
                    .map_err(|_| DetectorFailure::PolicyCompilation)
            },
        )
        .collect()
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

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
        "example"
            | "sample"
            | "placeholder"
            | "changeme"
            | "change-me"
            | "change_me"
            | "redacted"
            | "replace-me"
            | "replace_me"
            | "dummy"
            | "fake"
            | "test-value"
    ) || normalized.starts_with("your_")
        || normalized.starts_with("your-")
        || is_placeholder_only_expression(&normalized)
}

/// A capture built ONLY of interpolation placeholder groups — one or more, back
/// to back, with nothing substantive between them.
///
/// Whole-capture, exactly as [`capture_is_single_interpolation`] is, and never
/// "starts with the opener and ends with the closer": any capture BRACKETED by
/// two placeholders satisfies that while carrying a hardcoded literal between
/// them (H1), and this branch runs before the code-language gate, so that leak
/// reached EVERY path class, config included.
///
/// Consuming a SEQUENCE rather than demanding a single group is what keeps
/// `${A}${B}` — adjacent expansions with no literal anywhere — exempt, while
/// `${A}<literal>${B}` is not: the literal is what fails to be consumed. Each
/// group's interior must be brace-free, so a group can never swallow its
/// neighbour.
///
/// Openers are tried LONGEST FIRST so GitHub Actions' `${{ … }}`, one logical
/// expression with nested braces, is read as one group rather than as `${`
/// leaving a stray brace behind.
fn is_placeholder_only_expression(value: &str) -> bool {
    const GROUPS: [(&str, &str); 3] = [("${{", "}}"), ("{{", "}}"), ("${", "}")];
    let mut rest = value;
    let mut matched_any = false;
    'consume: while !rest.is_empty() {
        for (open, close) in GROUPS {
            let Some(interior) = rest.strip_prefix(open) else {
                continue;
            };
            let Some(end) = interior.find(close) else {
                continue;
            };
            if end == 0
                || interior[..end]
                    .bytes()
                    .any(|byte| matches!(byte, b'{' | b'}'))
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

/// Byte offset of the credential KEY for context-assignment exemptions.
///
/// The pattern allows an optional quote before the keyword so JSON keys match.
/// That quote is part of the overall regex match, but every downstream test
/// (`assignment_is_code_expression`, value-equals-key) expects the keyword's
/// first identifier byte — the same origin unquoted keys have always had.
fn context_assignment_match_start(rule_id: &str, bytes: &[u8], match_start: usize) -> usize {
    if rule_id != CONTEXT_ASSIGNMENT_RULE_ID {
        return match_start;
    }
    match bytes.get(match_start) {
        Some(b'"' | b'\'') => match_start + 1,
        _ => match_start,
    }
}

/// Keyword bytes between the match start and the assignment separator, with
/// surrounding quotes and ASCII whitespace stripped. Used only to compare a
/// capture against its own key (`"password": "password"`).
fn matched_assignment_key(bytes: &[u8], match_start: usize, value_start: usize) -> &[u8] {
    let head = bytes.get(match_start..value_start).unwrap_or(b"");
    let sep = head
        .iter()
        .position(|byte| matches!(byte, b':' | b'='))
        .unwrap_or(head.len());
    let key = head[..sep].trim_ascii();
    let key = match key.first().copied() {
        Some(b'"' | b'\'') => &key[1..],
        _ => key,
    };
    let key = match key.last().copied() {
        Some(b'"' | b'\'') if !key.is_empty() => &key[..key.len() - 1],
        _ => key,
    };
    key.trim_ascii()
}

/// Max whitespace-separated tokens still treated as a UI label.
/// Diceware / correct-horse passphrases are typically ≥4 words; short i18n
/// phrases (`Mot de passe`, `Enter your password`) stay at ≤3. Prefer
/// withhold on ambiguity rather than admit spaced passphrase material.
const NATURAL_LANGUAGE_LABEL_MAX_WORDS: usize = 3;
/// Max chars for a Title-Case single-token label (`Password`). Longer mashed
/// Title-Case (`Correcthorsebatterystaple`) is withheld as a credential.
const NATURAL_LANGUAGE_LABEL_MAX_SINGLE_TOKEN_CHARS: usize = 16;

/// True when a context-assignment capture is a UI / i18n label, not a secret:
/// short natural-language words or phrases with no digits (and no
/// credential-like symbols). Title Case single words (`Password`) and short
/// spaced phrases (`Mot de passe`, `Enter your password`) are labels;
/// Diceware-style multi-word phrases, long mashed Title-Case tokens, mixed
/// alphanumeric, or symbol soup are not.
fn is_natural_language_label(value: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(value) else {
        return false;
    };
    if text.is_empty() || text.chars().any(|character| character.is_ascii_digit()) {
        return false;
    }
    if !text.chars().all(|character| {
        character.is_alphabetic()
            || character.is_whitespace()
            || matches!(character, '-' | '\'' | '.' | ',' | ':' | '!' | '?')
    }) {
        return false;
    }
    if !text.chars().any(|character| character.is_alphabetic()) {
        return false;
    }
    if text.chars().any(|character| character.is_whitespace()) {
        let word_count = text.split_whitespace().count();
        return (1..=NATURAL_LANGUAGE_LABEL_MAX_WORDS).contains(&word_count);
    }
    // Single token: short Title Case pure letters (`Password`) reads as a UI
    // label. Long mashed Title Case (`Correcthorsebatterystaple`),
    // all-lowercase (`mypassword`), and mixed-case alphabet soup stay on the
    // default withhold path — value==key already covers `password`/`PASSWORD`.
    if text.chars().count() > NATURAL_LANGUAGE_LABEL_MAX_SINGLE_TOKEN_CHARS {
        return false;
    }
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_uppercase()
        && text.chars().all(|character| character.is_alphabetic())
        && chars.all(|character| character.is_lowercase())
}

/// Context-assignment stage: after a match, withhold only when the capture
/// still looks like a credential. Placeholders are handled by the shared
/// placeholder path; this covers label-like values and value-equals-key.
fn assignment_value_looks_like_credential(
    bytes: &[u8],
    match_start: usize,
    value_start: usize,
    value: &[u8],
) -> bool {
    if value.is_empty() {
        return false;
    }
    let key = matched_assignment_key(bytes, match_start, value_start);
    if !key.is_empty() && value.eq_ignore_ascii_case(key) {
        return false;
    }
    if is_natural_language_label(value) {
        return false;
    }
    true
}

/// Where to resume inspecting a right-hand side after a capture ends.
///
/// The rule's optional `["']?` consumes an OPENING quote, so the matching
/// closing quote sits at `capture_end`. Stepping over it is what guarantees the
/// walk begins outside that literal — starting ON it makes the walk read a
/// closing quote as an opening one, and for a double-quoted literal the
/// whole-literal skip then jumps to the NEXT literal's opening quote and blinds
/// the payload fence behind it.
///
/// A placeholder capture can also stop SHORT of its closing quote: the value
/// class ends at whitespace, so `"<<FILL_IN: model key>>"` captures only
/// `<<FILL_IN:`. Resuming there would start the walk inside the literal, so the
/// walk resumes past that literal's closing quote instead — and `None` (never
/// exempt) when the line ends before the quote closes.
fn right_hand_side_continuation(
    bytes: &[u8],
    capture_start: usize,
    capture_end: usize,
) -> Option<usize> {
    let opener = capture_start
        .checked_sub(1)
        .and_then(|index| bytes.get(index).copied())
        .filter(|byte| matches!(byte, b'"' | b'\'' | b'`'));
    match (opener, bytes.get(capture_end)) {
        (_, Some(b'"' | b'\'' | b'`')) => Some(capture_end + 1),
        (Some(quote), _) => bytes[capture_end..]
            .iter()
            .take_while(|byte| **byte != b'\n')
            .position(|byte| *byte == quote)
            .map(|offset| capture_end + offset + 1),
        (None, _) => Some(capture_end),
    }
}

/// `<<FILL_IN: model key>>`, `<your key>`: template syntax, not a credential —
/// but only when the WHOLE value is one angle group. The value runs from the
/// capture's `<` to the closing quote of the literal it sits in or, unquoted,
/// to the end of the line, and after its FIRST `>` it may hold nothing but more
/// `>`. So `<v1><credential>` and a heredoc opener (`<<-EOT`, whose body is the
/// credential) are not placeholders, and a literal that never closes on this
/// line is not one either.
///
/// Brackets alone prove nothing: `<credential>` is still the credential. The
/// group's interior must read as prose (whitespace), carry no digit, or be a
/// placeholder word (`<your-api-key>`).
fn is_angle_placeholder(bytes: &[u8], start: usize) -> bool {
    if bytes.get(start) != Some(&b'<') {
        return false;
    }
    let line_end = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start + offset);
    let opener = start
        .checked_sub(1)
        .and_then(|index| bytes.get(index).copied())
        .filter(|byte| matches!(byte, b'"' | b'\'' | b'`'));
    let end = match opener {
        Some(quote) => match bytes[start..line_end]
            .iter()
            .position(|byte| *byte == quote)
        {
            Some(offset) => start + offset,
            None => return false,
        },
        None => line_end,
    };
    let value = bytes[start..end].trim_ascii_end();
    let Some(first_close) = value.iter().position(|byte| *byte == b'>') else {
        return false;
    };
    if !value[first_close..].iter().all(|byte| *byte == b'>') {
        return false;
    }
    let interior = &value[..first_close];
    let interior = &interior[interior.iter().take_while(|byte| **byte == b'<').count()..];
    interior.iter().any(u8::is_ascii_whitespace)
        || !interior.iter().any(u8::is_ascii_digit)
        || is_placeholder(interior)
}

/// uri-credentials exemption: the URI documents a SHAPE rather than carrying a
/// credential. BOTH halves must be placeholders: the userinfo password (`pass`,
/// `password`, `pw`, `secret`, all `x`, or a generic placeholder), whole-capture
/// only, AND the host (`host`, `localhost`, `example.*`, `<...>`, `${...}`). A
/// placeholder-looking password on a real host (`admin:changeme@prod-db`) is a
/// real, if weak, credential and stays flagged.
fn uri_credentials_are_placeholder(
    bytes: &[u8],
    userinfo_start: usize,
    userinfo_end: usize,
) -> bool {
    let userinfo = &bytes[userinfo_start..userinfo_end];
    let placeholder_userinfo = is_placeholder(userinfo)
        || matches!(
            userinfo.to_ascii_lowercase().as_slice(),
            b"pass" | b"password" | b"pw" | b"secret"
        )
        || userinfo.iter().all(|byte| byte.eq_ignore_ascii_case(&b'x'));
    // `userinfo_end` holds the `@`; the host runs to the first delimiter.
    let host = bytes[userinfo_end + 1..]
        .iter()
        .take_while(|byte| {
            !matches!(
                byte,
                b'/' | b':' | b'?' | b'#' | b'"' | b'\'' | b'`' | b')' | b']' | b',' | b';'
            ) && !byte.is_ascii_whitespace()
        })
        .count();
    let host = std::str::from_utf8(&bytes[userinfo_end + 1..userinfo_end + 1 + host])
        .unwrap_or("")
        .to_ascii_lowercase();
    let placeholder_host = matches!(host.as_str(), "host" | "localhost")
        || host.starts_with("example.")
        || (host.len() > 2 && host.starts_with('<') && host.ends_with('>'))
        || is_placeholder_only_expression(&host);
    placeholder_userinfo && placeholder_host
}

/// Stage 2 for [`CONTEXT_ASSIGNMENT_RULE_ID`], on CODE-language paths only.
///
/// `KEY=VALUE` is config syntax. Source code produces the identical shape from
/// ordinary expressions — a typed struct field holding an `Arc<AtomicBool>`, an
/// associated constructor call, a member-chain clone — none of which can BE a
/// credential, because in code a credential is a string LITERAL. (Spelled in
/// prose rather than shown: an inline example would itself be a keyword adjacent
/// to `=` ahead of a payload run, which this rule rightly flags.)
///
/// Config, data, markup and every unrecognized extension stay STRICT:
/// [`crate::domain::LanguageId::is_code_language`] is false for Json, Toml,
/// Yaml, Markdown, Text, Env, Html, Css and Scss, and `from_path` yields `None`
/// for anything unknown — including the synthetic labels the visible-field
/// guards scan.
///
/// Three steps, IN THIS ORDER. The order is load-bearing, not stylistic:
///  1. the value OPENS a literal — never exempt;
///  2. the value sits INSIDE a literal opened earlier on this line — exempt only
///     if the WHOLE capture is one interpolation placeholder, so a credential
///     embedded in a URL or connection string stays sensitive;
///  3. otherwise — walk the right-hand-side expression.
///
/// Step 2 returns in BOTH directions. That is what guarantees step 3's walk
/// begins outside any literal: entering it mid-literal reads the literal's
/// CLOSING quote as an opening one and inverts quote parity for the whole
/// window.
fn assignment_is_code_expression(
    path: &str,
    bytes: &[u8],
    match_start: usize,
    value_start: usize,
    value: &[u8],
) -> bool {
    let Some(language) =
        crate::domain::LanguageId::from_path(path).filter(|language| language.is_code_language())
    else {
        return false;
    };
    if separator_is_path_not_assignment(bytes, match_start, value_start) {
        return true;
    }
    let rust = language == crate::domain::LanguageId::Rust;
    let quote_before_value = value_start
        .checked_sub(1)
        .and_then(|index| bytes.get(index).copied())
        .is_some_and(|byte| matches!(byte, b'"' | b'\''));
    if quote_before_value {
        // Step 1 applies only to an OPENING quote. When the KEYWORD sits inside
        // a literal and the value does not, that quote CLOSED the keyword's
        // literal (a `split` call whose argument ends in keyword and separator,
        // matrix row PA1): the capture is the code after it. Walk from just past the quote, which is outside every literal,
        // tolerating the closers of groups opened before it. Any other shape —
        // including a parity doubt that leaves either side "inside" — is an
        // opener and never exempt.
        // The keyword's literal must also be COMPACT — no whitespace between its
        // opening quote and this one (`"token="`, `"&access_token="`). Parity
        // alone is fooled by a stray quote earlier on the line (`'"'`), which
        // would make a real assignment's OPENING quote look like a closer.
        let quote = bytes[value_start - 1];
        let compact_keyword_literal = bytes[..value_start - 1]
            .iter()
            .rposition(|byte| *byte == quote || byte.is_ascii_whitespace())
            .is_some_and(|open| bytes[open] == quote && open < match_start);
        let closes_keyword_literal = compact_keyword_literal
            && match_is_inside_string_literal(bytes, match_start, rust)
            && !match_is_inside_string_literal(bytes, value_start, rust);
        return closes_keyword_literal
            && !expression_carries_quoted_payload(bytes, value_start, true);
    }
    if match_is_inside_string_literal(bytes, value_start, rust) {
        return capture_is_single_interpolation(value);
    }
    !expression_carries_quoted_payload(bytes, value_start, false)
}

/// True when `value_start` sits inside a string, char or template literal that
/// OPENED earlier on the same source line.
///
/// Per-delimiter parity from the line start, `\` consuming the next byte.
/// Counting each delimiter class SEPARATELY is what keeps a double-quoted
/// literal detectable when it also contains an apostrophe.
///
/// In Rust (`rust`), an apostrophe followed by an identifier byte whose closing
/// quote is not two bytes on is a lifetime or label (`&'static str`), not a
/// char literal, and does not toggle parity. Rust only: in Python, JavaScript
/// and friends that apostrophe OPENS a string, and skipping it would move a
/// credential outside the literal it sits in.
///
/// CEILING: a literal opened on a PRIOR line is invisible here, and every other
/// parity mistake (an apostrophe in a comment, an odd backtick in a doc
/// comment, a raw-string hash count) reports "inside", which fails closed.
fn match_is_inside_string_literal(bytes: &[u8], value_start: usize, rust: bool) -> bool {
    let line_start = bytes[..value_start]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let (mut double, mut single, mut backtick) = (false, false, false);
    let mut index = line_start;
    while index < value_start {
        match bytes[index] {
            b'\\' => {
                index += 2;
                continue;
            }
            b'"' => double = !double,
            b'\''
                if rust
                    && bytes
                        .get(index + 1)
                        .is_some_and(|next| next.is_ascii_alphabetic() || *next == b'_')
                    && bytes.get(index + 2) != Some(&b'\'') => {}
            b'\'' => single = !single,
            b'`' => backtick = !backtick,
            _ => {}
        }
        index += 1;
    }
    double || single || backtick
}

/// A doubled colon after the keyword (`CancellationToken::new`, a doc link to
/// `CancellationToken::is_cancelled`) is a PATH separator in every code
/// language here, never an assignment. Exempt only when the rest of the line
/// leaves no bracket open, fences no payload, and does not continue onto the
/// next line, so a path CALL or constant carrying a literal — on this line or
/// on a continuation line — is still judged by the ordinary steps.
fn separator_is_path_not_assignment(bytes: &[u8], match_start: usize, value_start: usize) -> bool {
    let Some(separator) = bytes[match_start..value_start]
        .iter()
        .position(|byte| matches!(byte, b':' | b'='))
        .map(|offset| match_start + offset)
    else {
        return false;
    };
    if bytes[separator] != b':' || bytes.get(separator + 1) != Some(&b':') {
        return false;
    }
    let line_end = bytes[separator..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| separator + offset);
    // The line proves nothing when the expression continues past it
    // (a C++ constant whose literal sits on the next line, a builder chain):
    // fall through to the ordinary steps, whose walk follows continuations.
    if line_end < bytes.len() && line_break_continues_expression(bytes, line_end) {
        return false;
    }
    let rest = &bytes[separator + 2..line_end];
    let mut depth = 0_u32;
    for byte in rest {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let fences_payload = rest.iter().enumerate().any(|(index, quote)| {
        matches!(quote, b'"' | b'\'' | b'`') && {
            let run = rest[index + 1..]
                .iter()
                .take_while(|byte| {
                    !matches!(byte, b'"' | b'\'' | b'`' | b'#') && !byte.is_ascii_whitespace()
                })
                .count();
            run >= CONTEXT_ASSIGNMENT_MIN_PAYLOAD && rest.get(index + 1 + run) == Some(quote)
        }
    });
    depth == 0 && !fences_payload
}

/// A capture that is WHOLLY one interpolation placeholder: `{`, at least one
/// interior byte, `}`, and no interior brace.
///
/// Tested on the CAPTURE, whole — NEVER on the enclosing string, and never with
/// `contains`. The rule's value class is greedy, so any further payload byte
/// adjacent to the closing brace is swallowed INTO the capture and this test
/// fails; a payload byte separated by whitespace starts its own independent
/// match, judged on its own merits. A placeholder therefore cannot license a
/// hardcoded literal anywhere else in the same string.
fn capture_is_single_interpolation(value: &[u8]) -> bool {
    value.len() >= 3
        && value.first() == Some(&b'{')
        && value.last() == Some(&b'}')
        && !value[1..value.len() - 1]
            .iter()
            .any(|byte| matches!(byte, b'{' | b'}'))
}

/// Does the expression CONTINUE past a depth-0 line break at `newline`?
///
/// True when the last non-whitespace byte before the break, or the first after
/// it, is a binary/chain continuation operator — the two shapes formatters
/// produce (rustfmt leads the next line with the operator, black and prettier
/// trail the previous one, and `\` is Python's explicit continuation). A line
/// ending in `;`, `)` or an identifier byte is a finished statement and still
/// terminates the walk, which is what keeps the ordinary next statement out.
///
/// Two bytes are deliberately ABSENT, each for a measured false positive.
/// `/` — a leading or trailing slash is far more often a comment than an
/// operator. `<`/`>` — a generic parameter list closes with `>`, so an
/// unterminated declaration (`semi: false` TypeScript, Kotlin, Scala) would
/// run the walk into the next line's unrelated string.
///
/// `,` is PRESENT (spec 023, decision D1): a trailing comma separates
/// declarators and tuple elements as often as it ends one, and excluding it
/// let a line break hide a credential in the next declarator — a measured
/// false negative (C5). The reverse cost is accepted and pinned: a
/// struct-literal field's exemption now walks into the NEXT field's quoted
/// value (oracle row G10b, accepted regression). Every terminator heuristic
/// keyed on this byte produced a credential leak, so the comma is a
/// continuation, never a terminator.
///
/// ONE carve-out (owner ruling 2026-09-29): a trailing comma followed by a line
/// that opens with `//` or `#[` ends the expression. That is a struct field
/// followed by a doc comment or attribute, and walking on read the NEXT field's
/// doc-comment example (`(e.g. "model-id")`) as this field's payload. Accepted
/// cost, pinned by matrix row PB4: a JavaScript declarator list interrupted by
/// a `//` comment line hides the declarators after it.
fn line_break_continues_expression(window: &[u8], newline: usize) -> bool {
    const CONTINUATION: &[u8] = b"+-*|&^%=.?:\\,";
    let trailing = window[..newline]
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| window[index]);
    let next_line = window[newline + 1..].trim_ascii_start();
    if trailing == Some(b',') && (next_line.starts_with(b"//") || next_line.starts_with(b"#[")) {
        return false;
    }
    let leading = next_line.first().copied();
    trailing.is_some_and(|byte| CONTINUATION.contains(&byte))
        || leading.is_some_and(|byte| CONTINUATION.contains(&byte))
}

/// Length of the bounded char literal opening at `at` (which holds `'`), but
/// ONLY when skipping it is necessary: opening quote, 1..=
/// [`CHAR_LITERAL_MAX_CONTENT`] content bytes — a `\` escape counted with its
/// escaped byte — a closing quote, AND at least one bracket among the content.
///
/// The bracket requirement is load-bearing, not an optimization. This skip
/// exists for exactly one purpose: keep a genuine char literal's bracket out
/// of the walk's `depth`. Skipping a bracket-free span buys nothing and costs
/// everything, because `'` is a STRING delimiter in Python, JavaScript,
/// TypeScript, Ruby and PHP — all code languages here. There the byte at `at`
/// is usually a string's CLOSING quote, and the next `'` within the bound is
/// the OPENING quote of the following argument (`', '` is a 2-byte gap). An
/// unconditional skip jumps over that opening quote, and since the payload
/// fence is only ever tested when the walk LANDS on a quote, the argument
/// behind it — a real credential — is never fence-tested at all. Requiring a
/// bracket keeps the skip to the case it was built for: a bracket-free gap
/// falls through to the one-byte advance, so the next quote is always seen.
///
/// `None` when no closing quote lands inside the bound (a lifetime sigil, a
/// contraction, an empty `''` pair, a literal too long to trust) or when the
/// bounded content carries no bracket.
fn bounded_char_literal_len(window: &[u8], at: usize) -> Option<usize> {
    let last_close = at + 1 + CHAR_LITERAL_MAX_CONTENT;
    let mut cursor = at + 1;
    let mut carries_bracket = false;
    while cursor <= last_close {
        match window.get(cursor)? {
            b'\\' => cursor += 2,
            // A genuine char literal never spans a line. Stopping here keeps
            // the skip from swallowing a line break whole, which would carry
            // the walk past the depth-0 gate without it ever being consulted.
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

/// Withdrawal test for the code-expression exemption.
///
/// Walks the right-hand-side expression from `from` over AT MOST
/// [`CONTEXT_ASSIGNMENT_SCAN_BOUND`] bytes, tracking `()`/`[]`/`{}` depth and
/// skipping `"` and `` ` `` literals whole. Returns `true` — exemption
/// WITHDRAWN — when any of:
///   * a delimiter-fenced run of at least [`CONTEXT_ASSIGNMENT_MIN_PAYLOAD`]
///     payload bytes appears anywhere in the window;
///   * a `"` or `` ` `` literal opens in the window and never closes inside it;
///   * the window is exhausted before the expression terminates at depth 0
///     (UNBALANCED or OVER-BOUND — fail closed).
///
/// `'` is fence-tested like the other two, then skipped only by
/// [`bounded_char_literal_len`] — bounded, bracket-bearing, never across a
/// line. Three rows fix those three conditions and none is negotiable: two
/// unpaired apostrophes must not bracket and hide a payload (S16), a genuine
/// char literal's bracket must stay invisible to `depth` or it underflows into
/// the enclosing-group arm and consumes the walk ahead of a real payload
/// (S17a), and a span must not cross a newline onto the next line's opening
/// fence (S21c).
///
/// The safety argument is about the FENCE BYTE, not the payload run. An earlier
/// version of this comment reasoned that the bound sits far below
/// [`CONTEXT_ASSIGNMENT_MIN_PAYLOAD`] so the skip "can never hide a fenced
/// payload" — false, and the cause of a real leak. The fence test only ever
/// fires when the walk LANDS on a quote, so a skip never needs to cover the
/// payload; stepping over its single opening quote is enough to blind the test
/// completely. Any change here is measured against what the walk still SEES.
///
/// INVARIANT — this walk is NOT line-local, and must never be made line-local
/// again. Inside an open bracket a `\n` never terminates; at depth 0 it
/// terminates only when [`line_break_continues_expression`] finds no
/// continuation operator on either side of the break. That is the entire
/// point: rustfmt and black put long arguments on a continuation line — with
/// an open bracket OR with a bare leading/trailing operator — and credentials
/// are long, so a line-local test's false negatives are systematically aligned
/// with the values this rule exists to catch.
///
/// PRECONDITION — `from` is provably OUTSIDE any string literal. Two callers
/// establish it, and both must keep doing so: step 2 of
/// [`assignment_is_code_expression`], which RETURNS rather than falling through
/// when the match sits inside a literal; and the placeholder branch of
/// [`scan_secret_bytes`], which resumes at
/// [`right_hand_side_continuation`] — one byte past the quote the capture
/// closed. Entering mid-literal reads that literal's CLOSING quote as an
/// opening one and inverts quote parity for the whole window. The third caller,
/// step 1's closing-quote branch, starts one byte past the quote that closed
/// the keyword's literal.
///
/// `after_closed_literal` is set by that third caller only. Its window starts
/// INSIDE the argument list the keyword's literal was passed to, so a closer
/// arriving before any opener belongs to that list and is not evidence the
/// expression ended: the walk steps over it instead of reporting "consumed".
fn expression_carries_quoted_payload(
    bytes: &[u8],
    from: usize,
    after_closed_literal: bool,
) -> bool {
    let end = from
        .saturating_add(CONTEXT_ASSIGNMENT_SCAN_BOUND)
        .min(bytes.len());
    let window = &bytes[from..end];
    let truncated = end < bytes.len();
    let is_payload =
        |byte: u8| !matches!(byte, b'"' | b'\'' | b'`' | b'#') && !byte.is_ascii_whitespace();

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
                    return true;
                }
                if byte == b'\'' {
                    // An apostrophe is NOT reliably a literal opener: a lifetime
                    // sigil and an English contraction inside a comment both
                    // carry no closing partner, and an unbounded skip to the
                    // next identical byte jumps OVER whatever sits between two
                    // such apostrophes — including a quoted payload — which
                    // fails OPEN (B1). But a closing quote within the char-
                    // literal bound proves a GENUINE char literal, and its
                    // content must not touch `depth`: a `)` in there underflows
                    // into the enclosing-group arm and consumes the walk ahead
                    // of a real fenced payload (S17a). Bounded skip, else one
                    // byte.
                    index += bounded_char_literal_len(window, index).unwrap_or(1);
                } else {
                    // Not a payload fence: skip the whole literal, so brackets
                    // inside it cannot move `depth`.
                    let mut cursor = index + 1;
                    loop {
                        match window.get(cursor) {
                            None => return true,
                            Some(b'\\') => cursor += 2,
                            Some(other) if *other == byte => break,
                            Some(_) => cursor += 1,
                        }
                    }
                    index = cursor + 1;
                }
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                index += 1;
            }
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth < 0 {
                    if !after_closed_literal {
                        // The match sat inside an enclosing group: consumed.
                        return false;
                    }
                    depth = 0;
                }
                index += 1;
            }
            b'\n' if depth == 0 => {
                // Depth 0 alone does NOT end the expression: a formatter splits
                // a long right-hand side across lines WITHOUT opening a
                // bracket — a method chain, a `??`/`+` fallback, a `\`
                // continuation — and credentials are long, so terminating here
                // unconditionally aligns the false negatives with the values
                // this rule exists to catch (H2). Continue only on a
                // continuation operator; an ordinary next statement still ends
                // the walk.
                if !line_break_continues_expression(window, index) {
                    return false;
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    // Window exhausted. Consumed only if we ran off the true end of the buffer
    // at depth 0; otherwise unbalanced or over-bound — fail closed.
    truncated || depth != 0
}

/// Scan stable admitted bytes under the deterministic, compile-once v1 policy.
/// The result retains only safe rule IDs, a count, and the 1-based line of each
/// finding; matched bytes and byte ranges never escape this function.
pub fn scan_secret_bytes(path: &str, bytes: &[u8]) -> SecretScan {
    if exceeds_scan_limit(bytes.len()) {
        return SecretScan::Indeterminate {
            reason: DetectorFailure::ResourceLimit,
        };
    }
    let rules = match SECRET_RULES.get_or_init(compile_secret_rules) {
        Ok(rules) => rules,
        Err(reason) => return SecretScan::Indeterminate { reason: *reason },
    };

    let mut rule_ids = Vec::new();
    let mut finding_count = 0_u32;
    // One bit per line, merged across rules. Bounded by the scan budget: at most
    // one bit per byte, so 512 KiB for a file of nothing but newlines.
    let mut line_bits: Vec<u64> = Vec::new();
    let mut findings: Vec<SecretFindingDescriptor> = Vec::new();
    for rule in rules {
        if !rule
            .keywords
            .iter()
            .any(|keyword| contains_ascii_case_insensitive(bytes, keyword))
        {
            continue;
        }
        // A rule's captures arrive in ascending order, so one cursor per rule
        // counts every newline once: linear in the file, not in findings times
        // file.
        let (mut cursor, mut line) = (0_usize, 0_usize);
        for captures in rule.pattern.captures_iter(bytes) {
            // Context-assignment publishes up to three mutually exclusive
            // capture groups (array element / quoted / unquoted); take the
            // first that participated. Other rules keep a single index
            // (including 0 for whole-match rules).
            let Some(secret) = (if rule.secret_capture == 0 {
                captures.get(0)
            } else {
                (1..=rule.secret_capture).find_map(|index| captures.get(index))
            }) else {
                return SecretScan::Indeterminate {
                    reason: DetectorFailure::Internal,
                };
            };
            let match_start = context_assignment_match_start(
                rule.id,
                bytes,
                captures
                    .get(0)
                    .map_or(secret.start(), |whole| whole.start()),
            );
            if rule.placeholders_allowed
                && (is_placeholder(secret.as_bytes())
                    || is_angle_placeholder(bytes, secret.start()))
            {
                // The CAPTURE is a placeholder — but that exempts the capture,
                // not the expression it sits in. A placeholder in one operand
                // must never license a hardcoded literal in another, so the
                // rest of the right-hand side is still inspected, from just
                // outside the literal this capture closed. Same withdrawal test
                // the code-expression exemption answers to, applied at the one
                // boundary both exemptions pass through.
                if right_hand_side_continuation(bytes, secret.start(), secret.end())
                    .is_some_and(|from| !expression_carries_quoted_payload(bytes, from, false))
                {
                    continue;
                }
            } else if (rule.id == CONTEXT_ASSIGNMENT_RULE_ID
                && (assignment_is_code_expression(
                    path,
                    bytes,
                    match_start,
                    secret.start(),
                    secret.as_bytes(),
                ) || !assignment_value_looks_like_credential(
                    bytes,
                    match_start,
                    secret.start(),
                    secret.as_bytes(),
                )))
                || (rule.id == URI_CREDENTIALS_RULE_ID
                    && uri_credentials_are_placeholder(bytes, secret.start(), secret.end()))
            {
                continue;
            }
            finding_count = finding_count.saturating_add(1);
            if !rule_ids.contains(&rule.id) {
                rule_ids.push(rule.id);
            }
            line += bytes[cursor..secret.start()]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count();
            cursor = secret.start();
            let word = line / 64;
            if word >= line_bits.len() {
                line_bits.resize(word + 1, 0);
            }
            line_bits[word] |= 1 << (line % 64);
            if findings.len() < FINDING_LINE_RANGES_KEPT {
                let line_1based = u32::try_from(line.saturating_add(1)).unwrap_or(u32::MAX);
                findings.push(SecretFindingDescriptor {
                    rule_id: rule.id,
                    line_start: line_1based,
                    line_end: line_1based,
                    shape: describe_secret_shape(secret.as_bytes(), rule.id),
                });
            }
        }
    }

    if finding_count == 0 {
        SecretScan::Clean
    } else {
        SecretScan::Sensitive {
            rule_ids,
            finding_count,
            line_ranges: first_line_ranges(&line_bits),
            findings,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LfsPointerMetadata {
    pub declared_oid: Option<String>,
    pub declared_size: Option<u64>,
}

/// Recognize the bounded canonical three-line Git LFS pointer. Extra lines make
/// the file ordinary content, preventing a pointer prefix from hiding payload.
pub fn detect_lfs_pointer(bytes: &[u8]) -> Option<LfsPointerMetadata> {
    if bytes.len() > 1024 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let lines = text
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect::<Vec<_>>();
    if lines.len() != 3 || lines[0] != "version https://git-lfs.github.com/spec/v1" {
        return None;
    }
    let oid = lines[1].strip_prefix("oid sha256:")?;
    if oid.len() != 64 || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let declared_size = lines[2].strip_prefix("size ")?.parse::<u64>().ok()?;
    Some(LfsPointerMetadata {
        declared_oid: Some(format!("sha256:{oid}")),
        declared_size: Some(declared_size),
    })
}

fn is_safe_template_basename(basename: &str) -> bool {
    ["example", "sample", "template", "dist"]
        .iter()
        .any(|marker| {
            basename.ends_with(&format!(".{marker}")) || basename.starts_with(&format!("{marker}."))
        })
}

/// Return the fixed v1 rule ID for a definite repository credential container.
/// Prose names containing words such as "secret" are intentionally not enough.
pub fn sensitive_path_rule(relative_path: &str) -> Option<&'static str> {
    // Empty and `.` segments open the same file, so they are dropped before
    // matching; otherwise `.aws//credentials` escapes the multi-component rules.
    // Folded as case-insensitive filesystems fold names, not just ASCII.
    let normalized = crate::paths::fold_case(
        &relative_path
            .split(['/', '\\'])
            .filter(|segment| !segment.is_empty() && *segment != ".")
            .collect::<Vec<_>>()
            .join("/"),
    );
    let basename = normalized.rsplit('/').next().unwrap_or(normalized.as_str());

    if !is_safe_template_basename(basename)
        && (basename == ".env" || basename.starts_with(".env.") || basename.ends_with(".env"))
    {
        return Some("path.environment-credentials");
    }
    if basename == ".git-credentials" || matches!(basename, ".netrc" | "_netrc") {
        return Some("path.network-credential-store");
    }
    if basename.ends_with(".key")
        || matches!(basename, "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519")
        || (basename.ends_with(".pem") && basename.contains("private"))
    {
        return Some("path.private-key-material");
    }
    if normalized.ends_with("/.aws/credentials")
        || normalized == ".aws/credentials"
        || normalized.ends_with("/.kube/config")
        || normalized == ".kube/config"
        || normalized.ends_with("application_default_credentials.json")
    {
        return Some("path.cloud-credential-store");
    }
    if basename.ends_with(".tfstate") || basename.ends_with(".tfstate.backup") {
        return Some("path.infrastructure-state");
    }
    None
}

/// [`sensitive_path_rule`] judged on the file's absolute path as well as its
/// root-relative one. A root inside a credential directory leaves only the
/// bare name (`credentials` under a root ending in `.aws`), which no
/// multi-component rule matches; the absolute path still carries it.
pub fn sensitive_path_rule_at(
    relative_path: &str,
    absolute_path: &std::path::Path,
) -> Option<&'static str> {
    sensitive_path_rule(relative_path)
        .or_else(|| sensitive_path_rule(&absolute_path.to_string_lossy()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StableContentAdmission {
    Admitted,
    MetadataOnly(crate::domain::MetadataOnlyReason),
}

pub fn classify_stable_content(
    path: &str,
    targets: crate::domain::IndexTargets,
    bytes: &[u8],
) -> StableContentAdmission {
    classify_stable_content_with(path, targets, bytes, scan_secret_bytes)
}

/// [`classify_stable_content`] with the dismissal store at `root` applied: a
/// content-rule verdict whose EVERY finding is dismissed (path + rule + line
/// content digest) is admitted. A changed line, a new finding elsewhere, an
/// indeterminate scan, a sensitive path, or an unloadable store stays withheld.
/// Every working-tree publication route uses this, so a dismissed file gets
/// symbols and search hits, not only raw reads.
pub fn classify_stable_content_for_root(
    root: &std::path::Path,
    path: &str,
    targets: crate::domain::IndexTargets,
    bytes: &[u8],
) -> StableContentAdmission {
    classify_stable_content_with(path, targets, bytes, |path, bytes| {
        secret_dismissals::scan_with_dismissals(root, path, bytes)
    })
}

/// `_targets` is retained so every publication route keeps declaring what it is
/// publishing, but no admission decision reads it any more: encoding validation
/// used to be gated on `includes_knowledge()`, which silently exempted every
/// code language (Ruling 4).
pub fn classify_stable_content_with<F>(
    path: &str,
    _targets: crate::domain::IndexTargets,
    bytes: &[u8],
    scan: F,
) -> StableContentAdmission
where
    F: FnOnce(&str, &[u8]) -> SecretScan,
{
    if let Some(pointer) = detect_lfs_pointer(bytes) {
        return StableContentAdmission::MetadataOnly(
            crate::domain::MetadataOnlyReason::LfsPointer {
                declared_oid: pointer.declared_oid,
                declared_size: pointer.declared_size,
            },
        );
    }
    // Every Tier-1 publication route funnels through here — cold load
    // (`live_index::store`), watcher reindex (`watcher`), local-ref blob lane
    // (`live_index::local_ref_scout`) and post-edit reindex
    // (`protocol::edit::reindex_after_write`) — so this is where the WHOLE byte
    // buffer is encoding-validated. It is deliberately NOT gated on
    // `targets.includes_knowledge()`: that is FALSE for every code language, and
    // the binary sniff clips at `BINARY_SNIFF_BYTES`, so a code file whose first
    // invalid byte lands past the clip was published and served from memory
    // without any encoding check at all. Read-time lanes gain nothing here:
    // content already in the index was validated once, before `IndexedFile`
    // existed.
    if decode_searchable_text(bytes).is_err() {
        return StableContentAdmission::MetadataOnly(
            crate::domain::MetadataOnlyReason::UnsupportedTextEncoding,
        );
    }
    match scan(path, bytes) {
        SecretScan::Clean => StableContentAdmission::Admitted,
        SecretScan::Sensitive {
            rule_ids,
            finding_count,
            ..
        } => StableContentAdmission::MetadataOnly(
            crate::domain::MetadataOnlyReason::SensitiveContent {
                rule_ids: rule_ids.into_iter().map(ToOwned::to_owned).collect(),
                finding_count,
            },
        ),
        SecretScan::Indeterminate { .. } => StableContentAdmission::MetadataOnly(
            crate::domain::MetadataOnlyReason::SensitiveContent {
                rule_ids: vec![INDETERMINATE_RULE_ID.to_string()],
                finding_count: 0,
            },
        ),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardFailure {
    pub policy_version: u32,
    pub rule_ids: Vec<&'static str>,
    pub finding_count: u32,
}

pub struct SafeHit<'a, T> {
    inner: &'a T,
    pub policy_version: u32,
}

impl<'a, T> SafeHit<'a, T> {
    pub fn into_inner(self) -> &'a T {
        self.inner
    }
}

fn guard_visible_fields(fields: &[&str]) -> Result<(), GuardFailure> {
    let mut rule_ids = Vec::new();
    let mut finding_count = 0_u32;
    for field in fields {
        match scan_secret_bytes("external-field", field.as_bytes()) {
            SecretScan::Clean => {}
            SecretScan::Sensitive {
                rule_ids: field_rule_ids,
                finding_count: field_count,
                ..
            } => {
                finding_count = finding_count.saturating_add(field_count);
                for rule_id in field_rule_ids {
                    if !rule_ids.contains(&rule_id) {
                        rule_ids.push(rule_id);
                    }
                }
            }
            SecretScan::Indeterminate { .. } => {
                if !rule_ids.contains(&INDETERMINATE_RULE_ID) {
                    rule_ids.push(INDETERMINATE_RULE_ID);
                }
            }
        }
    }
    if rule_ids.is_empty() {
        Ok(())
    } else {
        Err(GuardFailure {
            policy_version: SECRET_POLICY_VERSION,
            rule_ids,
            finding_count,
        })
    }
}

/// Reject a raw query without echoing it into the error value.
pub fn guard_query(query: &str) -> Result<(), GuardFailure> {
    guard_visible_fields(&[query])
}

/// Construct the only hit type eligible for direct formatting or CCR storage.
pub fn guard_hit<'a, T>(hit: &'a T, fields: &[&str]) -> Result<SafeHit<'a, T>, GuardFailure> {
    guard_visible_fields(fields)?;
    Ok(SafeHit {
        inner: hit,
        policy_version: SECRET_POLICY_VERSION,
    })
}

/// Project canonical Markdown section symbols into the public knowledge-unit
/// contract. This is an on-demand metadata view: source bytes and section spans
/// remain owned only by the existing indexed file and `SymbolRecord` lanes.
pub fn project_markdown_sections(
    source: &crate::domain::SourceIdentity,
    path: &str,
    content_hash: &str,
    symbols: &[crate::domain::SymbolRecord],
) -> Vec<crate::domain::KnowledgeUnit> {
    let mut units: Vec<crate::domain::KnowledgeUnit> = Vec::new();
    let mut parents: Vec<(u32, u32, String)> = Vec::new();

    for symbol in symbols
        .iter()
        .filter(|symbol| symbol.kind == crate::domain::SymbolKind::Section)
    {
        while parents
            .last()
            .is_some_and(|(depth, _, _)| *depth >= symbol.depth)
        {
            parents.pop();
        }

        let parent = parents.last().map(|(_, index, _)| *index);
        let segment = parents
            .last()
            .and_then(|(_, _, parent_name)| {
                symbol
                    .name
                    .strip_prefix(parent_name)
                    .and_then(|suffix| suffix.strip_prefix('.'))
            })
            .unwrap_or(symbol.name.as_str())
            .to_string();
        let mut heading_path = parent
            .and_then(|index| units.get(index as usize))
            .map(|unit| unit.heading_path.clone())
            .unwrap_or_default();
        heading_path.push(segment);

        let index = units.len() as u32;
        units.push(crate::domain::KnowledgeUnit {
            source: source.clone(),
            path: path.to_string(),
            content_hash: content_hash.to_string(),
            kind: crate::domain::KnowledgeUnitKind::MarkdownSection,
            heading_path,
            byte_range: symbol.byte_range.0..symbol.byte_range.1,
            line_range: symbol.line_range.0..symbol.line_range.1,
            parent,
        });
        parents.push((symbol.depth, index, symbol.name.clone()));
    }

    units
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{IndexTargets, MetadataOnlyReason};

    fn runtime_canary() -> String {
        ["runtime", "-", "canary", "-", "segment"].concat()
    }

    #[test]
    fn text_byte_matrix_handles_zero_lf_crlf_bom_multibyte_invalid_utf8_and_no_final_newline() {
        for bytes in [
            b"".as_slice(),
            b"one\ntwo\n".as_slice(),
            b"one\r\ntwo\r\n".as_slice(),
            b"one\ntwo".as_slice(),
            "one α\n二".as_bytes(),
        ] {
            let decoded = decode_searchable_text(bytes).expect("valid UTF-8 must decode exactly");
            assert_eq!(decoded.text.as_bytes(), bytes);
            assert_eq!(decoded.leading_bytes, 0);
        }

        let mut bom = UTF8_BOM.to_vec();
        bom.extend_from_slice("α\r\nlast".as_bytes());
        let decoded = decode_searchable_text(&bom).expect("UTF-8 BOM must be accepted");
        assert_eq!(decoded.text, "α\r\nlast");
        assert_eq!(decoded.leading_bytes, 3);
        assert!(decode_searchable_text(&[0xff, 0xfe, b'x']).is_err());
    }

    #[test]
    fn detector_failure_fails_closed_and_discards_transient_bytes() {
        let bytes = b"clean candidate".to_vec();
        let decision = classify_stable_content_with(
            "notes/guide.txt",
            IndexTargets::Knowledge,
            &bytes,
            |_path, _bytes| SecretScan::Indeterminate {
                reason: DetectorFailure::Internal,
            },
        );
        drop(bytes);

        assert!(matches!(
            decision,
            StableContentAdmission::MetadataOnly(MetadataOnlyReason::SensitiveContent {
                finding_count: 0,
                ..
            })
        ));
    }

    #[derive(Debug)]
    struct CandidateHit {
        path: String,
        heading: String,
        excerpt: String,
        diagnostic: String,
        source_label: String,
        ranking: String,
    }

    #[test]
    fn detector_positive_hit_is_withheld_whole_in_direct_and_ccr_paths() {
        let canary = runtime_canary();
        let hit = CandidateHit {
            path: "notes/guide.md".to_string(),
            heading: "Guide".to_string(),
            excerpt: format!("password={canary}"),
            diagnostic: "none".to_string(),
            source_label: "current".to_string(),
            ranking: "exact".to_string(),
        };
        let fields = [
            hit.path.as_str(),
            hit.heading.as_str(),
            hit.excerpt.as_str(),
            hit.diagnostic.as_str(),
            hit.source_label.as_str(),
            hit.ranking.as_str(),
        ];
        let guarded = guard_hit(&hit, &fields);
        let direct_visible = guarded.as_ref().ok().is_some();
        let ccr_visible = guarded.ok().map(SafeHit::into_inner).is_some();

        assert!(!direct_visible);
        assert!(!ccr_visible);
    }

    #[test]
    fn query_and_every_visible_hit_field_are_guarded_without_echo() {
        let canary = runtime_canary();
        let unsafe_field = format!("token={canary}");
        assert!(guard_query(&unsafe_field).is_err());

        for label in [
            "path",
            "heading",
            "excerpt",
            "diagnostic",
            "source_label",
            "ranking",
        ] {
            let result = guard_hit(&label, &[unsafe_field.as_str()]);
            assert!(result.is_err(), "guard missed visible field class {label}");
        }
    }

    #[test]
    fn lfs_pointer_parser_retains_only_declared_metadata() {
        let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n";
        let metadata = detect_lfs_pointer(pointer).expect("valid pointer must be recognized");
        assert!(metadata.declared_oid.is_some());
        assert_eq!(metadata.declared_size, Some(42));
        assert!(detect_lfs_pointer(b"ordinary prose").is_none());
    }

    #[test]
    fn sensitive_path_policy_distinguishes_credentials_from_safe_templates() {
        assert!(sensitive_path_rule(".env").is_some());
        assert!(sensitive_path_rule("deploy/.env.production").is_some());
        assert!(sensitive_path_rule(".ssh/id_ed25519").is_some());
        assert!(sensitive_path_rule("state/prod.tfstate").is_some());
        assert!(sensitive_path_rule(".env.example").is_none());
        assert!(sensitive_path_rule("docs/secret-design.md").is_none());
    }

    /// Repeated separators and `.` segments open the same file, so they must not
    /// hide a multi-component credential path from the lexical rule.
    #[test]
    fn sensitive_path_rule_ignores_empty_and_dot_segments() {
        for spelling in [
            ".aws//credentials",
            ".aws/./credentials",
            "./.aws/credentials",
            "infra/.kube/./config",
            "infra\\.kube\\config",
            ".aws/credentials/",
        ] {
            assert!(
                sensitive_path_rule(spelling).is_some(),
                "{spelling:?} must match the credential rule"
            );
        }
        assert!(sensitive_path_rule("src/./lib.rs").is_none());
    }

    /// Case-insensitive filesystems fold more than ASCII: ext4/f2fs casefold,
    /// NTFS and APFS all fold U+017F (long s) with `s` and U+212A (Kelvin sign)
    /// with `k`, so these spellings open the credential file itself.
    #[test]
    fn sensitive_path_rule_folds_unicode_case() {
        for spelling in [
            ".aws/credential\u{17F}",
            ".\u{212A}ube/config",
            "keys/id_r\u{17F}a",
            "deploy/.ENV",
            ".\u{212A}UBE/CONFIG",
        ] {
            assert!(
                sensitive_path_rule(spelling).is_some(),
                "{spelling:?} must match its credential rule"
            );
        }
    }

    /// A root inside a credential directory leaves only the bare file name as
    /// the relative path; the absolute path still carries the directory.
    #[test]
    fn sensitive_path_rule_at_judges_the_absolute_path_too() {
        let root = std::path::Path::new("/home/user/.aws");
        assert!(sensitive_path_rule("credentials").is_none());
        assert_eq!(
            sensitive_path_rule_at("credentials", &root.join("credentials")),
            Some("path.cloud-credential-store")
        );
        let kube = std::path::Path::new(r"C:\Users\user\.kube");
        assert!(sensitive_path_rule_at("config", &kube.join("config")).is_some());
        assert!(
            sensitive_path_rule_at(
                "src/lib.rs",
                std::path::Path::new("/home/user/repo/src/lib.rs")
            )
            .is_none()
        );
    }

    // ── D1 comma-continuation pinning matrix (spec 023 proposal v2 §6) ─────
    //
    // Row bodies are assembled at runtime so no keyword-plus-assignment shape
    // enters this source file: the tripwire
    // (`repository_source_is_clean_under_its_own_detector`) scans this file
    // with the same detector. `v` is benign filler, never a real credential.
    // Every placeholder capture exceeds the 8-byte floor so no row can read
    // CLEAN for the vacuous no-match reason.

    fn mx_token() -> String {
        ["to", "ken"].concat()
    }
    fn mx_password() -> String {
        ["pass", "word"].concat()
    }
    fn mx_apikey() -> String {
        ["api", "key"].concat()
    }
    fn mx_deploy_token() -> String {
        ["DEPLOY_TO", "KEN"].concat()
    }
    fn mx_fallback_token() -> String {
        ["FALLBACK_TO", "KEN"].concat()
    }
    fn mx_value() -> String {
        ["a-long-", "literal-", "value"].concat()
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum MxVerdict {
        Clean,
        Sensitive(u32),
    }

    /// (row id, path, body, expected verdict under V1 = D1 shipped).
    fn matrix_d1_rows() -> Vec<(&'static str, &'static str, String, MxVerdict)> {
        let token = mx_token();
        let password = mx_password();
        let apikey = mx_apikey();
        let deploy_token = mx_deploy_token();
        let fallback_token = mx_fallback_token();
        let v = mx_value();
        vec![
            // C5 fix rows: a line break after a trailing comma must not end
            // the walk when the next declarator carries a literal.
            (
                "C5a js multi-declarator across lines",
                "src/probe.js",
                [
                    "const ",
                    &token,
                    " = \"placeholder\",\n      backup = \"",
                    &v,
                    "\";",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "C5b js multi-declarator one line control",
                "src/probe.js",
                [
                    "const ",
                    &token,
                    " = \"placeholder\", backup = \"",
                    &v,
                    "\";",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "C5d rust multi-binding across lines",
                "src/probe.rs",
                [
                    "let ",
                    &token,
                    " = \"placeholder\",\n    backup = \"",
                    &v,
                    "\";",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // G10b: the accepted D1 regression — the walk now follows the
            // struct-literal field comma into the next field's value.
            (
                "G10b rust struct literal sibling field",
                "src/probe.rs",
                [
                    "let config = Config {\n    ",
                    &apikey,
                    ": resolve_from_environment(),\n    banner_text: \"",
                    &v,
                    "\",\n};",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // D1-scan: a long post-D1 walk passes over a later independent
            // match; the scanner must still find it (count pins resume).
            (
                "D1-scan consumed walk passes over later match",
                "src/probe.js",
                [
                    "const ",
                    &token,
                    " = \"placeholder\",\n      a1 = \"x1\",\n      a2 = \"x2\",\n      a3 = \"x3\",\n      a4 = \"x4\";\nconst ",
                    &password,
                    " = \"",
                    &v,
                    "\";",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "P1 python tuple placeholder then literal",
                "src/probe.py",
                [&password, " = \"placeholder\", \"", &v, "\""].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "P4 python interpolation then literal",
                "src/probe.py",
                [
                    &token,
                    " = \"${{DEPLOY_HOST}}\", \"",
                    &v,
                    "\"",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "E2 python unquoted placeholder comma literal",
                "src/probe.py",
                [&password, " = placeholder, '", &v, "'"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // K rows: the walk is the only thing catching a non-keyword
            // sibling; K1's sibling IS a keyword and finds two.
            (
                "K1 yaml apikey sibling two findings",
                "config.yaml",
                [
                    "auth: {",
                    &password,
                    ": \"${CI_FALLBACK_TOKEN}\", ",
                    &apikey,
                    ": \"",
                    &v,
                    "\"}",
                ]
                .concat(),
                MxVerdict::Sensitive(2),
            ),
            (
                "K2 yaml bearer sibling walk only",
                "config.yaml",
                [
                    "auth: {",
                    &password,
                    ": \"${CI_FALLBACK_TOKEN}\", bearer: \"",
                    &v,
                    "\"}",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "K3 yaml credential sibling walk only",
                "config.yaml",
                [
                    "auth: {",
                    &password,
                    ": \"${CI_FALLBACK_TOKEN}\", credential: \"",
                    &v,
                    "\"}",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                // Two findings under v6: the placeholder password's RHS walk
                // still catches the sibling literal, and bare `accesskey` is
                // now a context-assignment keyword in its own right.
                "K4 yaml accesskey sibling walk only",
                "config.yaml",
                [
                    "auth: {",
                    &password,
                    ": \"${CI_FALLBACK_TOKEN}\", accesskey: \"",
                    &v,
                    "\"}",
                ]
                .concat(),
                MxVerdict::Sensitive(2),
            ),
            (
                "F5 env comma-joined list one key",
                "deploy.env",
                [
                    &fallback_token,
                    "=\"${CI_TOKEN}\", \"",
                    &v,
                    "\"",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F3 env one-comma evasion variant",
                "deploy.env",
                [
                    &deploy_token,
                    "=${CI_FALLBACK_TOKEN}, \"",
                    &v,
                    "\"",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F3ctl env no-comma control",
                "deploy.env",
                [
                    &deploy_token,
                    "=${CI_FALLBACK_TOKEN} \"",
                    &v,
                    "\"",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // Config sibling false positives — accepted ceiling (proposal §5).
            (
                "Y1u yaml flow unquoted capture swallows comma",
                "config.yaml",
                ["{", &token, ": ${TOKEN_NAME}, banner: \"", &v, "\"}"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "Y1q yaml flow quoted sibling",
                "config.yaml",
                [
                    "{",
                    &token,
                    ": \"${TOKEN_NAME}\", banner: \"",
                    &v,
                    "\"}",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "A3 yaml note sibling walk coverage",
                "config.yaml",
                [
                    "auth: {",
                    &token,
                    ": \"${TOKEN_NAME}\", note: \"",
                    &v,
                    "\"}",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // B1: bracketed arrays — the stated blind spot, closed by the
            // inline-array capture (owner ruling 2026-08-03). The first
            // substantive element sits in credential-bound position, exactly
            // like the unbracketed comma-list form of this row.
            (
                "B1 toml bracketed array",
                "config.toml",
                [&apikey, " = [\"placeholder\", \"", &v, "\"]"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // Parity addendum (beyond spec §6, sizes the C1/C2 backstop and
            // what the D4 mate check would close): unquoted capture before a
            // fence; apostrophe inside a double-quoted placeholder literal.
            (
                "X3 js unquoted capture before fence",
                "src/probe.js",
                ["const ", &token, " = placeholder\"", &v, "\", \"x"].concat(),
                MxVerdict::Clean,
            ),
            (
                "X1 js apostrophe in placeholder literal",
                "src/probe.js",
                [
                    "const ",
                    &token,
                    " = \"your-org's-token\", \"",
                    &v,
                    "\", \"x",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
        ]
    }

    #[test]
    fn comma_continuation_d1_matrix_pins() {
        let mut failures = Vec::new();
        for (id, path, body, expected) in matrix_d1_rows() {
            let actual = match scan_secret_bytes(path, body.as_bytes()) {
                SecretScan::Clean => MxVerdict::Clean,
                SecretScan::Sensitive { finding_count, .. } => MxVerdict::Sensitive(finding_count),
                SecretScan::Indeterminate { reason } => {
                    failures.push(format!("{id}: unexpected Indeterminate: {reason:?}"));
                    continue;
                }
            };
            if actual != expected {
                failures.push(format!("{id}: expected {expected:?}, got {actual:?}"));
            }
        }
        assert!(
            failures.is_empty(),
            "matrix rows off expectation: {failures:?}"
        );
    }

    /// (row id, path, body, expected verdict) for the inline-array capture
    /// (owner ruling 2026-08-03, closes the bracketed-array ceiling).
    ///
    /// Every fixture is assembled at runtime from non-secret fragments; `v` is
    /// benign filler. Each capture-bearing row's counterpart control proves the
    /// row could fail (no vacuous CLEANs under the 8-byte floor).
    fn bracketed_array_rows() -> Vec<(&'static str, &'static str, String, MxVerdict)> {
        let token = mx_token();
        let apikey = mx_apikey();
        let deploy_token = mx_deploy_token();
        let v = mx_value();
        vec![
            // AR1: opener alone — a single substantive element is captured.
            (
                "AR1 toml single-element array",
                "config.toml",
                [&apikey, " = [\"", &v, "\"]"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // AR2: a short leading element must not hide the credential — the
            // skip group carries the capture past it.
            (
                "AR2 toml short-first element",
                "config.toml",
                [&apikey, " = [\"dev\", \"", &v, "\"]"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // AR3 control for AR1/AR2: nothing reaches the 8-byte floor, so the
            // array machinery alone must not manufacture a finding.
            (
                "AR3 toml all-short elements",
                "config.toml",
                [&apikey, " = [\"dev\", \"shrt\"]"].concat(),
                MxVerdict::Clean,
            ),
            // AR4: the false-positive guard the FO-5 ruling demands. After a
            // skipped short element the capture MUST open with a quote —
            // otherwise it would land on the next flow-mapping KEY.
            (
                "AR4 yaml flow-map key not captured",
                "probe.yaml",
                ["{", &token, ": \"ab\", environment: production}"].concat(),
                MxVerdict::Clean,
            ),
            // AR5: a placeholder-only element stays exempt (H1 discipline
            // composes with the array capture; ${DEPLOY_TOKEN} clears the floor
            // so this row cannot pass for the vacuous no-match reason).
            (
                "AR5 toml placeholder-only element",
                "config.toml",
                [&apikey, " = [\"${", &deploy_token, "}\"]"].concat(),
                MxVerdict::Clean,
            ),
            // AR6 control for AR5: a substantive element after the placeholder
            // withdraws the exemption — the H1 payload walk crosses the comma.
            (
                "AR6 toml placeholder then payload",
                "config.toml",
                [&apikey, " = [\"${", &deploy_token, "}\", \"", &v, "\"]"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // AR7: on a code path the captured element is a QUOTED literal, so
            // the code-expression exemption (unquoted RHS only) does not apply
            // — same semantics as `let token = "literal"`. Measured 2026-08-03.
            (
                "AR7 js array on code path",
                "src/probe.js",
                ["const ", &token, " = [\"dev\", \"", &v, "\"];"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // AR8: YAML flow sequences are the same shape as TOML arrays.
            (
                "AR8 yaml flow sequence",
                "probe.yaml",
                [&token, ": [\"dev\", \"", &v, "\"]"].concat(),
                MxVerdict::Sensitive(1),
            ),
        ]
    }

    #[test]
    fn bracketed_array_capture_pins() {
        let mut failures = Vec::new();
        for (id, path, body, expected) in bracketed_array_rows() {
            let actual = match scan_secret_bytes(path, body.as_bytes()) {
                SecretScan::Clean => MxVerdict::Clean,
                SecretScan::Sensitive { finding_count, .. } => MxVerdict::Sensitive(finding_count),
                SecretScan::Indeterminate { reason } => {
                    failures.push(format!("{id}: unexpected Indeterminate: {reason:?}"));
                    continue;
                }
            };
            if actual != expected {
                failures.push(format!("{id}: expected {expected:?}, got {actual:?}"));
            }
        }
        assert!(
            failures.is_empty(),
            "array rows off expectation: {failures:?}"
        );
    }

    /// A real-looking alphanumeric value, assembled at runtime so no literal of
    /// credential shape enters this file.
    fn mx_real() -> String {
        ["Zq8r", "Lm3v", "Tx7w", "Pk2n", "Hy5s"].concat()
    }

    fn scan_verdict(path: &str, body: &str) -> Result<MxVerdict, String> {
        match scan_secret_bytes(path, body.as_bytes()) {
            SecretScan::Clean => Ok(MxVerdict::Clean),
            SecretScan::Sensitive { finding_count, .. } => Ok(MxVerdict::Sensitive(finding_count)),
            SecretScan::Indeterminate { reason } => Err(format!("Indeterminate: {reason:?}")),
        }
    }

    /// (row id, path, body, expected) for the 2026-09-29 precision ruling.
    /// Every CLEAN row is a false positive measured in a real repository; each
    /// has a SENSITIVE control in the same shape, here or in
    /// [`true_positive_corpus_stays_caught`], so no row passes by the detector
    /// simply not firing.
    fn precision_rows() -> Vec<(&'static str, &'static str, String, MxVerdict)> {
        let token = mx_token();
        let token_camel = ["To", "ken"].concat();
        let apikey_camel = ["Api", "Key"].concat();
        let secret_kw = ["sec", "ret"].concat();
        let real40 = [mx_real(), mx_real()].concat();
        let password = mx_password();
        let apikey = mx_apikey();
        let v = mx_value();
        let real = mx_real();
        vec![
            // (a) A quote that CLOSES the keyword's literal is not an opener.
            (
                "PA1 rust split closes keyword literal",
                "src/probe.rs",
                [
                    "let first_", &token, " = first.split(\"", &token,
                    "=\").nth(1).expect(\"", &token, " in first URL\");\n",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
            (
                "PA2 control: literal after the closing quote",
                "src/probe.rs",
                [
                    "let url = base.split(\"", &token, "=\").nth(1).unwrap_or(\"", &real,
                    "\");\n",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // Adversarial control: a stray quote earlier on the line makes a
            // real assignment's OPENING quote look like a closer by parity.
            (
                "PA3 control: char-literal quote then real assignment",
                "src/probe.rs",
                [
                    "let q = '\"'; let ", &token, " = \"", &real, "\"; // \"\n",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // (b) Comma then a doc-comment or attribute line ends the field.
            (
                "PB1 rust struct field then doc comment",
                "src/probe.rs",
                [
                    "pub struct Config {\n    pub gemini_", &apikey,
                    ": Option<String>,\n    /// Gemini model ID (e.g. \"gemini-2.5-flash\").\n    pub gemini_model: Option<String>,\n}\n",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
            (
                "PB2 rust struct field then attribute",
                "src/probe.rs",
                [
                    "pub struct Config {\n    pub openai_", &apikey,
                    ": Option<String>,\n    #[serde(rename = \"", &v,
                    "\")]\n    pub model: Option<String>,\n}\n",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
            // Accepted cost of (b), pinned so it cannot move silently.
            (
                "PB4 js declarator after a comment line (accepted)",
                "src/probe.js",
                [
                    "const ", &token, " = compute(),\n  // fallback\n  backup = \"", &v,
                    "\";\n",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
            // (c) `<`-prefixed placeholders; the rest of the RHS still walked.
            (
                "PC1 env example fill-in placeholder",
                "server/.env.example",
                [&apikey.to_uppercase(), "=<<FILL_IN: gemini key>>\nPORT=8080\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PC2 toml quoted fill-in placeholder",
                "config.toml",
                [&apikey, " = \"<<FILL_IN: gemini key>>\"\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PC3 control: fill-in then literal element",
                "config.toml",
                [&apikey, " = [\"<<FILL_IN: gemini key>>\", \"", &v, "\"]\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // (d) A Rust lifetime is not a char literal.
            (
                "PD1 rust lifetime before keyword parameter",
                "src/probe.rs",
                [
                    "pub fn spawn_worker_on<F>(&self, name: &'static str, ", &token,
                    ": CancellationToken, work: F) -> bool {\n",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
            (
                "PD2 control: python apostrophe still opens a string",
                "src/probe.py",
                ["url = 'https://host/?", &token, "=", &v, "'\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // (e) Placeholder userinfo in a URI.
            (
                "PE1 uri pass placeholder in comment",
                "src/probe.rs",
                ["// userinfo in scheme://user:", "pass", "@host/db\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PE2 uri redacted placeholder",
                "src/probe.rs",
                ["// postgres://u:", "<redacted>", "@host/db\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PE3 uri password xxxx secret placeholders",
                "notes.txt",
                [
                    "a://admin:",
                    &password,
                    "@localhost\nb://admin:",
                    "xxxx",
                    "@example.com\nc://admin:",
                    "secret",
                    "@${DB_HOST}\n",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
            // (f) No cheap fix: a keyword assignment inside a doc-comment code
            // span reads as inside a literal. Known ceiling, pinned.
            (
                "PF1 rust doc comment code span (known ceiling)",
                "src/probe.rs",
                [
                    "/// So `--", &password, "=postgres://u:pw@host` is fully opaque.\n",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // A doubled colon is a path separator, not an assignment.
            (
                "PG1 rust doc link to a token path",
                "src/probe.rs",
                [
                    "    /// and must poll [`Cancellation", &token.to_uppercase()[..1], &token[1..],
                    "::is_cancelled`] wherever it would\n",
                ]
                .concat(),
                MxVerdict::Clean,
            ),
            (
                "PG2 control: path call carrying a literal",
                "src/probe.rs",
                ["let t = ", &token, "::from_secret_value(\"", &real, "\");\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "PG3 control: path call whose literal is on the next line",
                "src/probe.rs",
                [
                    "let t = ", &token, "::from_secret_value(\n    \"", &real, "\",\n);\n",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            // Review F1: the doubled-colon exemption must follow continuation
            // lines.
            (
                "F1a cpp constant with its literal on the next line",
                "src/probe.cpp",
                [
                    "const char* ",
                    &apikey_camel,
                    "::kDefault =\n    \"",
                    &real40,
                    "\";\n",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F1b rust builder chain on a continuation line",
                "src/probe.rs",
                [
                    "let client = ",
                    &token_camel,
                    "::builder()\n    .secret(\"",
                    &real40,
                    "\")\n    .build();\n",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F1c ruby constant with its literal on the next line",
                "lib/probe.rb",
                [&token_camel, "::DEFAULT_VALUE =\n  \"", &real40, "\"\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // Review F6: only a WHOLE angle group is a placeholder.
            (
                "F6a angle group followed by a value",
                "src/probe.rs",
                [
                    "let u = \"https://h/?",
                    &token,
                    "=<v1>",
                    &real40,
                    "\";\n",
                ]
                .concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F6b heredoc opener with the value on the next line",
                "infra/main.tf",
                [&secret_kw, " = <<-EOT_LONGNAME\n", &real40, "\nEOT_LONGNAME\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // Review round 2: brackets around a real value do not make it a
            // placeholder. Only an interior with whitespace, no digit, or a
            // placeholder word is template syntax.
            (
                "F6c env angle group around a real value",
                "deploy.env",
                [&apikey.to_uppercase(), "=<", &real40, ">\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F6d yaml quoted angle group around a real value",
                "config.yaml",
                [&password, ": \"<", &real40, ">\"\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F6e env angle placeholder word",
                "deploy.env",
                [&apikey.to_uppercase(), "=<your-api-", "key>\n"].concat(),
                MxVerdict::Clean,
            ),
            // Review F7: a placeholder password on a real host stays flagged.
            (
                "F7a changeme on a production host",
                "config.yaml",
                ["url: postgres://admin:", "changeme", "@prod-db.internal:5432/app\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "F7b password on a production host",
                "config.yaml",
                ["broker: amqp://app:", &password, "@mq-prod\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // The INI credential key: a code lookup stays clean, its literal
            // twin is caught.
            (
                "AWS1 python secret access key from environment",
                "src/probe.py",
                ["aws_", &secret_kw, "_access", "_key = os.environ.get(KEY_NAME)\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "AWS2 python secret access key literal",
                "src/probe.py",
                ["aws_", &secret_kw, "_access", "_key = \"", &real40, "\"\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            // Quoted credential keys (JSON / YAML / TOML / JS): withhold real
            // values, admit UI labels and placeholders.
            (
                "PJ1 json quoted password with digits",
                "config.json",
                ["{\"", &password, "\": \"hunter2hunter2\"}\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "PJ2 yaml password quoted secret",
                "config.yaml",
                [&password, ": \"S3cr3t!x\"\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "PJ3 toml quoted token",
                "config.toml",
                ["\"", &token, "\" = \"", &real, "\"\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "PJ4 i18n password label Password",
                "i18n.json",
                ["{\"", &password, "\":\"Password\"}\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PJ5 french password label",
                "i18n.json",
                ["{\"", &password, "\": \"Mot de passe\"}\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PJ6 env-style placeholder in json",
                "config.json",
                ["{\"", &password, "\": \"${DB_PASSWORD}\"}\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PJ7 angle placeholder in json",
                "config.json",
                ["{\"", &password, "\": \"<your-password>\"}\n"].concat(),
                MxVerdict::Clean,
            ),
            (
                "PJ8 empty quoted password value",
                "config.json",
                ["{\"", &password, "\": \"\"}\n"].concat(),
                MxVerdict::Clean,
            ),
            // Digit-free Diceware / mashed passphrases under credential keys
            // must stay Sensitive (narrow label exemption; not UI labels).
            (
                "PJ9 diceware correct-horse passphrase",
                "config.json",
                ["{\"", &password, "\": \"correct horse battery staple\"}\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "PJ10 diceware fruit-word passphrase",
                "config.json",
                ["{\"", &password, "\": \"apple banana cherry dragonfruit\"}\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
            (
                "PJ11 title-case mashed passphrase",
                "config.json",
                ["{\"", &password, "\": \"Correcthorsebatterystaple\"}\n"].concat(),
                MxVerdict::Sensitive(1),
            ),
        ]
    }

    #[test]
    fn precision_ruling_matrix_pins() {
        let mut failures = Vec::new();
        for (id, path, body, expected) in precision_rows() {
            match scan_verdict(path, &body) {
                Ok(actual) if actual == expected => {}
                Ok(actual) => failures.push(format!("{id}: expected {expected:?}, got {actual:?}")),
                Err(error) => failures.push(format!("{id}: {error}")),
            }
        }
        assert!(
            failures.is_empty(),
            "precision rows off expectation: {failures:?}"
        );
    }

    /// Security floor for the precision ruling: every row MUST stay caught.
    /// Real-shaped values, assembled at runtime, in each file class, plus each
    /// false-positive shape above with its placeholder replaced by a real value.
    #[test]
    fn true_positive_corpus_stays_caught() {
        let token = mx_token();
        let apikey = mx_apikey();
        let real = mx_real();
        let sk = ["sk-", &real, &real].concat();
        let ghp = ["gh", "p_", &real, &real[..16]].concat();
        let pem = [
            "-----BEGIN RSA ",
            "PRIVATE KEY-----\nMIIEowIBAAKCAQEA",
            &real,
            "\n-----END RSA ",
            "PRIVATE KEY-----\n",
        ]
        .concat();
        let bearer = ["Author", "ization: Bearer ", &real, &real[..12]].concat();
        // Forty characters of the AWS secret-key alphabet, `/` and `+` included.
        let aws_secret = [&real, "/", &real[..9], "+", &real[..9]].concat();
        let rows: Vec<(&str, &str, String, &str)> = vec![
            (
                "github token rust",
                "src/probe.rs",
                ["let t = \"", &ghp, "\";\n"].concat(),
                "secret.provider-token",
            ),
            (
                "github token yaml",
                "ci.yaml",
                ["gh: ", &ghp, "\n"].concat(),
                "secret.provider-token",
            ),
            (
                "pem private key",
                "certs/server.txt",
                pem,
                "secret.private-key-envelope",
            ),
            (
                "authorization bearer",
                "requests.http",
                ["GET /\n", &bearer, "\n"].concat(),
                "secret.authorization-header",
            ),
            (
                "sk key rust",
                "src/probe.rs",
                ["let ", &apikey, " = \"", &sk, "\";\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "sk key python",
                "src/probe.py",
                [&apikey, " = \"", &sk, "\"\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "sk key toml",
                "config.toml",
                [&apikey, " = \"", &sk, "\"\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "sk key yaml",
                "config.yaml",
                [&apikey, ": \"", &sk, "\"\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "sk key env",
                "deploy.env",
                [&apikey.to_uppercase(), "=", &sk, "\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "uri real password",
                "src/probe.rs",
                ["let url = \"postgres://user:", &real, "@db/prod\";\n"].concat(),
                URI_CREDENTIALS_RULE_ID,
            ),
            // The false-positive shapes, placeholder replaced by a real value.
            (
                "fp-shape split then literal",
                "src/probe.rs",
                [
                    "let url = base.split(\"",
                    &token,
                    "=\").nth(1).unwrap_or(\"",
                    &real,
                    "\");\n",
                ]
                .concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "fp-shape struct literal field then doc comment",
                "src/probe.rs",
                [
                    "let c = Config {\n    ",
                    &apikey,
                    ": \"",
                    &real,
                    "\".into(),\n    /// doc\n    other: 1,\n};\n",
                ]
                .concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "fp-shape test struct literal",
                "tests/api.rs",
                ["    admin_", &token, ": \"", &real, "\".into(),\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "fp-shape env example real value",
                "server/.env.example",
                [&apikey.to_uppercase(), "=", &sk, "\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "fp-shape lifetime then literal",
                "src/probe.rs",
                [
                    "fn f(name: &'static str) { let ",
                    &token,
                    " = \"",
                    &real,
                    "\"; }\n",
                ]
                .concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "fp-shape lifetime then keyword in literal",
                "src/probe.rs",
                [
                    "const URL: &'static str = \"https://h/?",
                    &token,
                    "=",
                    &real,
                    "\";\n",
                ]
                .concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "fp-shape uri placeholder slot real value",
                "src/probe.rs",
                ["// scheme://user:", &real, "@authority\n"].concat(),
                URI_CREDENTIALS_RULE_ID,
            ),
            (
                "fp-shape fill-in slot real value",
                "config.toml",
                [&apikey, " = \"", &sk, "\"\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            // INI credential profile: the keyword is followed by more of the
            // key's name before the separator, and the value is unquoted.
            (
                "aws secret access key ini",
                "deploy/aws.ini",
                [
                    "[default]\naws_",
                    "sec",
                    "ret_access",
                    "_key = ",
                    &aws_secret,
                    "\n",
                ]
                .concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
            (
                "aws secret access key ini upper",
                "deploy/profile.cfg",
                ["AWS_", "SEC", "RET_ACCESS", "_KEY=", &aws_secret, "\n"].concat(),
                CONTEXT_ASSIGNMENT_RULE_ID,
            ),
        ];
        let mut failures = Vec::new();
        for (id, path, body, rule) in rows {
            match scan_secret_bytes(path, body.as_bytes()) {
                SecretScan::Sensitive {
                    rule_ids,
                    line_ranges,
                    ..
                } if rule_ids.contains(&rule) => {
                    if line_ranges.is_empty() {
                        failures.push(format!("{id}: caught but no finding line recorded"));
                    }
                }
                other => failures.push(format!(
                    "{id}: expected {rule}, got {:?}",
                    scan_kind(&other)
                )),
            }
        }
        assert!(failures.is_empty(), "true positives missed: {failures:?}");
    }

    /// The verdict's shape without its contents, so a failure never echoes
    /// matched bytes.
    fn scan_kind(scan: &SecretScan) -> String {
        match scan {
            SecretScan::Clean => "Clean".to_string(),
            SecretScan::Sensitive { rule_ids, .. } => format!("Sensitive{rule_ids:?}"),
            SecretScan::Indeterminate { reason } => format!("Indeterminate({reason:?})"),
        }
    }

    #[test]
    fn findings_carry_one_based_lines() {
        let body = ["fn a() {}\nlet ", &mx_token(), " = \"", &mx_real(), "\";\n"].concat();
        let SecretScan::Sensitive { line_ranges, .. } =
            scan_secret_bytes("src/probe.rs", body.as_bytes())
        else {
            panic!("fixture must be caught");
        };
        assert_eq!(line_ranges, vec![(2, 2)]);
    }

    /// Two rules on alternating lines are ONE range: ranges merge across rules,
    /// not per rule, so a rule's own gaps never split what the file shows.
    #[test]
    fn finding_ranges_merge_across_rules() {
        let uri = ["scheme://u:", &mx_real(), "@db.prod\n"].concat();
        let short = [&["p", "wd"].concat(), "=", "a1b2c3d4", "\n"].concat();
        let body = format!("{uri}{short}").repeat(3);
        let SecretScan::Sensitive {
            finding_count,
            line_ranges,
            ..
        } = scan_secret_bytes("deploy.env", body.as_bytes())
        else {
            panic!("fixture must be caught");
        };
        assert_eq!(finding_count, 6);
        assert_eq!(line_ranges, vec![(1, 6)]);
    }

    /// Only the first ranges are kept, and one more than a refusal shows.
    #[test]
    fn finding_ranges_stop_after_the_kept_count() {
        let short = [&["p", "wd"].concat(), "=", "a1b2c3d4", "\n\n"].concat();
        let SecretScan::Sensitive {
            finding_count,
            line_ranges,
            ..
        } = scan_secret_bytes("deploy.env", short.repeat(15).as_bytes())
        else {
            panic!("fixture must be caught");
        };
        assert_eq!(finding_count, 15);
        let expected = (0..FINDING_LINE_RANGES_KEPT as u32)
            .map(|index| (2 * index + 1, 2 * index + 1))
            .collect::<Vec<_>>();
        assert_eq!(line_ranges, expected);
    }

    /// A file of nothing but short findings at the scan budget: every
    /// classification pays for the line accounting, so it must be linear in the
    /// file, not in findings times file. About 2 s in a local debug build; the
    /// per-finding recount did not finish in 120 s.
    #[test]
    fn dense_findings_at_the_scan_budget_scan_in_linear_time() {
        let finding = [&["p", "wd"].concat(), "=", "a1b2c3d4", "\n"].concat();
        let count = (SECRET_SCAN_MAX_BYTES - 64) / finding.len();
        let body = finding.repeat(count);
        let started = std::time::Instant::now();
        let SecretScan::Sensitive {
            finding_count,
            line_ranges,
            ..
        } = scan_secret_bytes("deploy.env", body.as_bytes())
        else {
            panic!("fixture must be caught");
        };
        let elapsed = started.elapsed();
        assert_eq!(finding_count as usize, count);
        assert_eq!(line_ranges, vec![(1, u32::try_from(count).unwrap())]);
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "{count} findings took {elapsed:?}"
        );
    }

    #[test]
    fn markdown_sections_project_to_knowledge_units_without_a_duplicate_store() {
        let source = crate::domain::SourceIdentity {
            repository_id: crate::domain::RepositoryId::new("repository-fixture"),
            source_id: crate::domain::SourceId::new("source-fixture"),
            location: crate::domain::SourceLocation::WorkingTree {
                worktree_id: "worktree-fixture".to_string(),
            },
        };
        let result = crate::parsing::process_file_with_classification(
            "README.md",
            b"# Root\nintro\n## Child.with.dot\nbody\n",
            crate::domain::LanguageId::Markdown,
            crate::domain::FileClassification::for_indexed_path(
                "README.md",
                IndexTargets::Knowledge,
            ),
        );

        let units = project_markdown_sections(
            &source,
            &result.relative_path,
            &result.content_hash,
            &result.symbols,
        );
        assert_eq!(units.len(), result.symbols.len());
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].heading_path, ["Root"]);
        assert_eq!(units[0].parent, None);
        assert_eq!(units[1].heading_path, ["Root", "Child.with.dot"]);
        assert_eq!(units[1].parent, Some(0));
        assert!(units.iter().all(|unit| {
            unit.kind == crate::domain::KnowledgeUnitKind::MarkdownSection
                && unit.path == result.relative_path
                && unit.content_hash == result.content_hash
        }));
    }
}
