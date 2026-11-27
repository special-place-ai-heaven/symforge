use tree_sitter::Node;

pub(super) fn literal_key(index: Node<'_>, src: &[u8]) -> Option<String> {
    if !matches!(index.kind(), "string" | "template_string") {
        return None;
    }
    let mut key = String::new();
    for part in index.named_children(&mut index.walk()) {
        let text = part.utf8_text(src).ok()?;
        match part.kind() {
            "string_fragment" => key.push_str(text),
            "escape_sequence" => key.extend(decode_escape(text)?),
            _ => return None,
        }
    }
    Some(key)
}

pub(super) fn identifier_name(text: &str) -> Option<String> {
    let mut parts = text.split('\\');
    let mut name = parts.next()?.to_owned();
    for part in parts {
        let body = part.strip_prefix('u')?;
        let len = if body.starts_with('{') {
            body.find('}')? + 1
        } else {
            4
        };
        let escape = body.get(..len)?;
        name.push(decode_escape(&format!("\\u{escape}"))??);
        name.push_str(&body[len..]);
    }
    Some(name)
}

fn decode_escape(sequence: &str) -> Option<Option<char>> {
    let mut chars = sequence.strip_prefix('\\')?.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    let hex = |digits: &str| {
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| {
                u32::from_str_radix(digits, 16)
                    .ok()
                    .and_then(char::from_u32)
            })
            .flatten()
    };
    let decoded = match first {
        '\n' | '\r' | '\u{2028}' | '\u{2029}' if matches!(rest, "" | "\n") => return Some(None),
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        'b' => '\u{8}',
        'f' => '\u{c}',
        'v' => '\u{b}',
        '0' if rest.is_empty() => '\0',
        'x' if rest.len() == 2 => hex(rest)?,
        'u' => match rest.strip_prefix('{') {
            Some(braced) => hex(braced.strip_suffix('}')?)?,
            None if rest.len() == 4 => hex(rest)?,
            None => return None,
        },
        other if other.is_ascii_digit() || !rest.is_empty() => return None,
        other => other,
    };
    Some(Some(decoded))
}

#[cfg(test)]
mod tests {
    use super::{decode_escape, identifier_name};

    #[test]
    fn decodes_identifier_escapes_or_refuses_them() {
        for (text, expected) in [
            ("h\\u0061ndle", Some("handle")),
            ("h\\u{61}ndle", Some("handle")),
            ("\\u0068\\u0061ndle", Some("handle")),
            ("handle", Some("handle")),
            ("h\\x61ndle", None),
            ("h\\u06", None),
            ("h\\u{61ndle", None),
        ] {
            assert_eq!(identifier_name(text).as_deref(), expected, "{text:?}");
        }
    }

    #[test]
    fn decodes_each_javascript_escape_spelling() {
        for (sequence, expected) in [
            ("\\u0061", Some(Some('a'))),
            ("\\x61", Some(Some('a'))),
            ("\\u{61}", Some(Some('a'))),
            ("\\u{1F600}", Some(Some('\u{1F600}'))),
            ("\\'", Some(Some('\''))),
            ("\\n", Some(Some('\n'))),
            ("\\0", Some(Some('\0'))),
            ("\\\r\n", Some(None)),
            ("\\\n", Some(None)),
            ("\\uD83D", None),
            ("\\x+6", None),
            ("\\u{}", None),
            ("\\12", None),
        ] {
            assert_eq!(decode_escape(sequence), expected, "{sequence:?}");
        }
    }
}
