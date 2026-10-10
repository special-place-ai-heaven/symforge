//! Bounded, source-policy-aware previews of exact staged postimages.

use crate::knowledge::{SecretScan, guard_query, scan_secret_bytes};

pub(crate) const MAX_PREVIEW_BYTES: usize = 1_048_576;
const TRUNCATED: &str = "[preview truncated; hashes cover the complete staged image]\n";

pub(crate) struct SafeDiff {
    pub rendered: String,
    pub truncated: bool,
    pub redacted: bool,
}

/// Render the changed line range of one exact staged image. Secret-bearing or
/// indeterminate content yields an explicit metadata-only redaction. The whole
/// file is scanned, so a hidden unchanged line cannot leak through context.
pub(crate) fn render_safe_change(
    path: &str,
    old: &[u8],
    new: &[u8],
    max_bytes: usize,
) -> Result<SafeDiff, ()> {
    guard_query(path).map_err(|_| ())?;
    let clean = matches!(scan_secret_bytes(path, old), SecretScan::Clean)
        && matches!(scan_secret_bytes(path, new), SecretScan::Clean);
    let old_text = std::str::from_utf8(old).ok();
    let new_text = std::str::from_utf8(new).ok();
    if !clean || old_text.is_none() || new_text.is_none() {
        return render_redacted(path, old.len(), new.len(), max_bytes);
    }
    let old_lines = old_text.unwrap().split_inclusive('\n').collect::<Vec<_>>();
    let new_lines = new_text.unwrap().split_inclusive('\n').collect::<Vec<_>>();
    let mut prefix = 0;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old_lines.len() - prefix
        && suffix < new_lines.len() - prefix
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let removed = &old_lines[prefix..old_lines.len() - suffix];
    let added = &new_lines[prefix..new_lines.len() - suffix];
    let mut rendered = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -{},{} +{},{} @@\n",
        prefix + 1,
        removed.len(),
        prefix + 1,
        added.len()
    );
    if rendered.len() > max_bytes.saturating_sub(TRUNCATED.len()) {
        return Err(());
    }
    let mut truncated = false;
    for (marker, lines) in [('-', removed), ('+', added)] {
        for line in lines {
            let escaped = line.escape_debug().to_string();
            if rendered.len() + escaped.len() + 3 > max_bytes - TRUNCATED.len() {
                truncated = true;
                break;
            }
            rendered.push(marker);
            rendered.push(' ');
            rendered.push_str(&escaped);
            rendered.push('\n');
        }
        if truncated {
            break;
        }
    }
    if truncated {
        rendered.push_str(TRUNCATED);
    }
    if guard_query(&rendered).is_err() {
        // The visible-field policy may be stricter than a file scan on diff
        // syntax. Fall back to explicit redaction rather than leaking content.
        return render_redacted(path, old.len(), new.len(), max_bytes);
    }
    Ok(SafeDiff {
        rendered,
        truncated,
        redacted: false,
    })
}

fn render_redacted(
    path: &str,
    old_len: usize,
    new_len: usize,
    max_bytes: usize,
) -> Result<SafeDiff, ()> {
    let rendered = format!(
        "--- a/{path}\n+++ b/{path}\n[content withheld by source safety policy; old_bytes={old_len}; new_bytes={new_len}]\n"
    );
    if rendered.len() > max_bytes {
        return Err(());
    }
    guard_query(&rendered).map_err(|_| ())?;
    Ok(SafeDiff {
        rendered,
        truncated: false,
        redacted: true,
    })
}

pub(crate) fn render_safe_batch<'a>(
    changes: impl IntoIterator<Item = (&'a str, &'a [u8], &'a [u8])>,
) -> Result<SafeDiff, ()> {
    let mut rendered = String::new();
    let mut redacted = false;
    let mut truncated = false;
    for (path, old, new) in changes {
        let remaining = MAX_PREVIEW_BYTES.saturating_sub(rendered.len());
        if remaining <= TRUNCATED.len() + 128 {
            truncated = true;
            break;
        }
        let part = render_safe_change(path, old, new, remaining)?;
        rendered.push_str(&part.rendered);
        redacted |= part.redacted;
        if part.truncated {
            truncated = true;
            break;
        }
    }
    if truncated && !rendered.ends_with(TRUNCATED) {
        if rendered.len() + TRUNCATED.len() > MAX_PREVIEW_BYTES {
            return Err(());
        }
        rendered.push_str(TRUNCATED);
    }
    Ok(SafeDiff {
        rendered,
        truncated,
        redacted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_change_renders_old_and_new_lines_without_mutation() {
        let preview = render_safe_change(
            "src/lib.rs",
            b"fn main() { old() }\n",
            b"fn main() { new() }\n",
            MAX_PREVIEW_BYTES,
        )
        .expect("safe preview");
        assert!(preview.rendered.contains("- fn main() { old() }"));
        assert!(preview.rendered.contains("+ fn main() { new() }"));
        assert!(!preview.redacted);
        assert!(!preview.truncated);
    }

    #[test]
    fn indeterminate_scan_withholds_content_explicitly() {
        let oversized = vec![b'x'; 5 * 1_048_576];
        let preview =
            render_safe_change("src/lib.rs", &oversized, b"safe\n", 256).expect("redacted preview");
        assert!(preview.redacted);
        assert!(preview.rendered.contains("content withheld"));
        assert!(!preview.rendered.contains("xxxxxxxx"));
    }
}
