//! Repository-knowledge admission and exact-text helpers.
//!
//! Knowledge reuses the live index's byte store and Markdown section records;
//! this module owns only policy and projection seams, never a second corpus.

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
pub const SECRET_POLICY_VERSION: u32 = 2;
const SECRET_SCAN_MAX_BYTES: usize = crate::domain::index::METADATA_ONLY_CODE_BYTES as usize;
/// The one reserved rule id every [`DetectorFailure`] collapses onto. Public so
/// the disclosure gate can tell an indeterminate verdict — which a reindex
/// cannot change — from a real content match.
pub const INDETERMINATE_RULE_ID: &str = "secret.detector.indeterminate";
const CONTEXT_ASSIGNMENT_RULE_ID: &str = "secret.context-assignment";
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetectorFailure {
    PolicyCompilation,
    ResourceLimit,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretScan {
    Clean,
    Sensitive {
        rule_ids: Vec<&'static str>,
        finding_count: u32,
    },
    Indeterminate {
        reason: DetectorFailure,
    },
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
            r#"(?i)(?:api[_-]?key|secret|token|password|passwd|pwd|client[_-]?secret)[ \t]*[:=][ \t]*["']?([^\s"'#]{8,})"#,
            1,
            true,
        ),
        (
            "secret.uri-credentials",
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
        || is_single_placeholder_group(&normalized, "${", "}")
        || is_single_placeholder_group(&normalized, "{{", "}}")
}

/// A capture that is WHOLLY one delimited placeholder group: `open`, at least
/// one interior byte, `close`, and NO interior brace of either kind.
///
/// Whole-capture, exactly as [`capture_is_single_interpolation`] is — never
/// "starts with the opener and ends with the closer", which any capture
/// BRACKETED by two placeholders satisfies while carrying a hardcoded literal
/// between them (H1). The interior-brace test is what distinguishes the one
/// group from the two: a lone `${VAR:-fallback}` or `{{name}}` keeps its
/// exemption, `${A}<literal>${B}` loses it. This branch runs before the
/// code-language gate, so the bracketing leak reached EVERY path class,
/// config included.
fn is_single_placeholder_group(value: &str, open: &str, close: &str) -> bool {
    value.len() > open.len() + close.len()
        && value.starts_with(open)
        && value.ends_with(close)
        && !value[open.len()..value.len() - close.len()]
            .bytes()
            .any(|byte| matches!(byte, b'{' | b'}'))
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
    value_start: usize,
    value: &[u8],
) -> bool {
    if !crate::domain::LanguageId::from_path(path)
        .is_some_and(|language| language.is_code_language())
    {
        return false;
    }
    let opens_literal = value_start
        .checked_sub(1)
        .and_then(|index| bytes.get(index).copied())
        .is_some_and(|byte| matches!(byte, b'"' | b'\''));
    if opens_literal {
        return false;
    }
    if match_is_inside_string_literal(bytes, value_start) {
        return capture_is_single_interpolation(value);
    }
    !expression_carries_quoted_payload(bytes, value_start)
}

/// True when `value_start` sits inside a string, char or template literal that
/// OPENED earlier on the same source line.
///
/// Per-delimiter parity from the line start, `\` consuming the next byte.
/// Counting each delimiter class SEPARATELY is what keeps a double-quoted
/// literal detectable when it also contains an apostrophe.
///
/// CEILING: a literal opened on a PRIOR line is invisible here, and every other
/// parity mistake (a lone lifetime, an apostrophe in a comment, a raw-string
/// hash count) reports "inside", which fails closed.
fn match_is_inside_string_literal(bytes: &[u8], value_start: usize) -> bool {
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
            b'\'' => single = !single,
            b'`' => backtick = !backtick,
            _ => {}
        }
        index += 1;
    }
    double || single || backtick
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
/// `'` is fence-tested like the other two and then skipped ONLY as a BOUNDED
/// char literal: a closing `'` within [`CHAR_LITERAL_MAX_CONTENT`] content
/// bytes (escapes counted) proves a genuine char literal, whose content —
/// possibly a bracket — must stay invisible to `depth`. A `)` seen there
/// underflows into the enclosing-group arm and CONSUMES the walk ahead of a
/// real fenced payload, which fails OPEN (row S17a — the row that forced the
/// bound). Without a nearby closing partner — a lifetime sigil, an apostrophe
/// in a comment — it advances ONE byte, never a whole-literal skip: two such
/// apostrophes would bracket, and hide, a quoted payload (row S16). The bound
/// sits far below [`CONTEXT_ASSIGNMENT_MIN_PAYLOAD`], so the skip itself can
/// never hide a fenced payload.
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
/// Does the expression CONTINUE past a depth-0 line break at `newline`?
///
/// True when the last non-whitespace byte before the break, or the first after
/// it, is a binary/chain continuation operator — the two shapes formatters
/// produce (rustfmt leads the next line with the operator, black and prettier
/// trail the previous one, and `\` is Python's explicit continuation). A line
/// ending in `;`, `)` or an identifier byte is a finished statement and still
/// terminates the walk, which is what keeps the ordinary next statement out.
///
/// `,` is deliberately ABSENT: a trailing comma means the element ENDED, and
/// following it walks a struct-literal field's exemption into the NEXT field's
/// quoted value — a false positive this repository's own source produces. `/`
/// is absent for the same reason in the other direction: a leading or trailing
/// slash is far more often a comment than an operator.
fn line_break_continues_expression(window: &[u8], newline: usize) -> bool {
    const CONTINUATION: &[u8] = b"+-*|&^%=<>.?:\\";
    let trailing = window[..newline]
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| window[index]);
    let leading = window[newline + 1..]
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .copied();
    trailing.is_some_and(|byte| CONTINUATION.contains(&byte))
        || leading.is_some_and(|byte| CONTINUATION.contains(&byte))
}

/// Length of the bounded char literal opening at `at` (which holds `'`):
/// opening quote, 1..=[`CHAR_LITERAL_MAX_CONTENT`] content bytes — a `\`
/// escape counted with its escaped byte — then the closing quote. `None` when
/// no closing quote lands inside the bound: a lifetime sigil, a contraction,
/// an empty `''` pair, or a literal too long to be trusted.
fn bounded_char_literal_len(window: &[u8], at: usize) -> Option<usize> {
    let last_close = at + 1 + CHAR_LITERAL_MAX_CONTENT;
    let mut cursor = at + 1;
    while cursor <= last_close {
        match window.get(cursor)? {
            b'\\' => cursor += 2,
            b'\'' if cursor > at + 1 => return Some(cursor + 1 - at),
            _ => cursor += 1,
        }
    }
    None
}

/// PRECONDITION — `from` is provably OUTSIDE any string literal, guaranteed by
/// step 2 of [`assignment_is_code_expression`].
fn expression_carries_quoted_payload(bytes: &[u8], from: usize) -> bool {
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
                    // The match sat inside an enclosing group: consumed.
                    return false;
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
/// The result retains only safe rule IDs and a count; matched bytes and ranges
/// never escape this function.
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
    for rule in rules {
        if !rule
            .keywords
            .iter()
            .any(|keyword| contains_ascii_case_insensitive(bytes, keyword))
        {
            continue;
        }
        for captures in rule.pattern.captures_iter(bytes) {
            let Some(secret) = captures.get(rule.secret_capture) else {
                return SecretScan::Indeterminate {
                    reason: DetectorFailure::Internal,
                };
            };
            if rule.placeholders_allowed && is_placeholder(secret.as_bytes()) {
                continue;
            }
            if rule.id == CONTEXT_ASSIGNMENT_RULE_ID
                && assignment_is_code_expression(path, bytes, secret.start(), secret.as_bytes())
            {
                continue;
            }
            finding_count = finding_count.saturating_add(1);
            if !rule_ids.contains(&rule.id) {
                rule_ids.push(rule.id);
            }
        }
    }

    if finding_count == 0 {
        SecretScan::Clean
    } else {
        SecretScan::Sensitive {
            rule_ids,
            finding_count,
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
    let normalized = relative_path.replace('\\', "/").to_ascii_lowercase();
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
