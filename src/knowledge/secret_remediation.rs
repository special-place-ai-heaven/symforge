//! Secret remediation planning shared by the MCP adapter and embedded hosts.
//! Source bytes and replacement bytes stay private to the caller's process.

use std::collections::BTreeSet;
use std::path::Path;

use crate::hash::digest_hex;
use crate::knowledge::secret_dismissals::{
    self, DismissalRecord, line_bytes_at, line_content_digest,
};
use crate::knowledge::{SECRET_POLICY_VERSION, SecretMatchSpan, SecretSpansScan};

/// Stable finding identity used by both withheld metadata and remediation.
pub fn mint_finding_id(
    path: &str,
    rule_id: &str,
    line_start: u32,
    line_end: u32,
    shape: &str,
) -> String {
    let material =
        format!("v{SECRET_POLICY_VERSION}\0{path}\0{rule_id}\0{line_start}\0{line_end}\0{shape}");
    let hex = digest_hex(material.as_bytes());
    format!("wf_{}", &hex[..24])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SelectionRefusal {
    EmptyFindings,
    DuplicateFinding,
    TooManyFindings,
    ScanIndeterminate,
    MissingFinding,
    UndismissableFinding,
    MissingLine,
    UnsupportedEncryptFormat,
    ResourceLimit,
    Cancelled,
    DeadlineExceeded,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct SelectedFinding {
    pub id: String,
    pub span: SecretMatchSpan,
}

/// Deliberately omits source bytes from `Debug` and all serializable DTOs.
#[allow(dead_code)]
pub struct SelectedFile {
    pub path: String,
    pub original: Vec<u8>,
    pub findings: Vec<SelectedFinding>,
}

impl std::fmt::Debug for SelectedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectedFile")
            .field("path", &self.path)
            .field("source_bytes", &self.original.len())
            .field("findings", &self.findings.len())
            .finish()
    }
}

/// Select exact content findings from a bounded list of admitted source bytes.
/// The reader may skip unavailable paths; missing requested IDs still refuse.
#[allow(dead_code)]
pub fn select_requested(
    paths: &[String],
    finding_ids: &[String],
    mut read: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<Vec<SelectedFile>, SelectionRefusal> {
    select_requested_checked(paths, finding_ids, |path| Ok(read(path)))
}

/// The checked reader can stop a repo scan on host resource or time limits.
pub fn select_requested_checked(
    paths: &[String],
    finding_ids: &[String],
    mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>, SelectionRefusal>,
) -> Result<Vec<SelectedFile>, SelectionRefusal> {
    if finding_ids.is_empty() {
        return Err(SelectionRefusal::EmptyFindings);
    }
    if finding_ids.len() > 128 {
        return Err(SelectionRefusal::TooManyFindings);
    }
    let mut wanted = finding_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if wanted.len() != finding_ids.len() {
        return Err(SelectionRefusal::DuplicateFinding);
    }
    let mut selected = Vec::new();
    for path in paths {
        if wanted.is_empty() {
            break;
        }
        let Some(original) = read(path)? else {
            continue;
        };
        let spans = match crate::knowledge::scan_secret_spans(path, &original) {
            SecretSpansScan::Spans(spans) => spans,
            SecretSpansScan::Indeterminate { .. } => {
                return Err(SelectionRefusal::ScanIndeterminate);
            }
        };
        let mut findings = Vec::new();
        for span in spans {
            let id = mint_finding_id(
                path,
                span.rule_id,
                span.line_start,
                span.line_end,
                &span.shape,
            );
            if wanted.remove(id.as_str()) {
                findings.push(SelectedFinding { id, span });
            }
        }
        if !findings.is_empty() {
            selected.push(SelectedFile {
                path: path.clone(),
                original,
                findings,
            });
        }
    }
    if !wanted.is_empty() {
        return Err(SelectionRefusal::MissingFinding);
    }
    Ok(selected)
}

/// A private plan. The environment entries contain captured credential bytes.
/// Neither this type nor its fields are serializable or exposed through Embed.
pub struct ExternalizeFile {
    pub path: String,
    pub original: Vec<u8>,
    pub rewritten: Vec<u8>,
    pub env_lines: Vec<String>,
    pub masked_diff: String,
    pub finding_ids: Vec<String>,
}

impl std::fmt::Debug for ExternalizeFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalizeFile")
            .field("path", &self.path)
            .field("source_bytes", &self.original.len())
            .field("replacement_bytes", &self.rewritten.len())
            .field("finding_count", &self.finding_ids.len())
            .finish()
    }
}

pub fn plan_externalize(selected: Vec<SelectedFile>) -> Vec<ExternalizeFile> {
    selected
        .into_iter()
        .map(|file| {
            let mut rewritten = file.original.clone();
            let mut env_lines = Vec::new();
            let mut masked_diff = String::new();
            let mut finding_ids = Vec::new();
            let mut findings = file.findings;
            findings.sort_by_key(|finding| std::cmp::Reverse(finding.span.value_start));
            let idiom = externalize_idiom(&file.path);
            for (number, finding) in findings.into_iter().enumerate() {
                let span = finding.span;
                let variable = env_var_name(&file.path, span.rule_id, span.line_start, number);
                let value = &file.original[span.value_start..span.value_end];
                env_lines.push(format!("{variable}={}", String::from_utf8_lossy(value)));
                let replacement = idiom_replacement(idiom, &variable);
                rewritten.splice(span.value_start..span.value_end, replacement.bytes());
                finding_ids.push(finding.id);
                let mask_kind = ["sec", "ret"].concat();
                masked_diff.push_str(&format!(
                    "--- {path}\n+++ {path}\n@@ line {line} @@\n-«{mask_kind}:{index}»\n+{replacement}\n",
                    path = file.path,
                    line = span.line_start,
                    index = number + 1,
                ));
            }
            ExternalizeFile {
                path: file.path,
                original: file.original,
                rewritten,
                env_lines,
                masked_diff,
                finding_ids,
            }
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Idiom {
    EnvSubst,
    ProcessEnv,
    OsEnviron,
    StdEnvVar,
}

fn externalize_idiom(path: &str) -> Idiom {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => Idiom::ProcessEnv,
        "py" => Idiom::OsEnviron,
        "rs" => Idiom::StdEnvVar,
        _ => Idiom::EnvSubst,
    }
}

fn idiom_replacement(idiom: Idiom, variable: &str) -> String {
    match idiom {
        Idiom::EnvSubst => format!("${{{variable}}}"),
        Idiom::ProcessEnv => format!("process.env.{variable}"),
        Idiom::OsEnviron => format!("os.environ[\"{variable}\"]"),
        Idiom::StdEnvVar => {
            format!("std::env::var(\"{variable}\").expect(\"{variable} must be set\")")
        }
    }
}

fn env_var_name(path: &str, rule_id: &str, line: u32, number: usize) -> String {
    let material = format!("{path}\0{rule_id}\0{line}\0{number}\0{SECRET_POLICY_VERSION}");
    let hex = digest_hex(material.as_bytes());
    format!("SYMFORGE_SECRET_{}", hex[..8].to_ascii_uppercase())
}

pub fn encrypt_format_supported(path: &str) -> bool {
    sops_input_type(path).is_some()
}

/// SOPS' documented stdin input stores. A TOML filename is deliberately
/// refused because SOPS cannot emit encrypted TOML in place.
pub fn sops_input_type(path: &str) -> Option<&'static str> {
    let file_name = Path::new(path).file_name()?.to_str()?;
    if file_name == ".env" || file_name.ends_with(".env") {
        return Some("dotenv");
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "ini" => Some("ini"),
        "env" => Some("dotenv"),
        _ => None,
    }
}

pub fn ensure_encrypt_formats(selected: &[SelectedFile]) -> Result<(), SelectionRefusal> {
    if selected
        .iter()
        .all(|file| encrypt_format_supported(&file.path))
    {
        Ok(())
    } else {
        Err(SelectionRefusal::UnsupportedEncryptFormat)
    }
}

/// Reject ciphertext that still contains a selected captured value as UTF-8.
/// This examines bytes only and emits a boolean, never captured content.
pub fn contains_selected_plaintext(encrypted: &[u8], file: &SelectedFile) -> bool {
    file.findings.iter().any(|finding| {
        let value = &file.original[finding.span.value_start..finding.span.value_end];
        value.len() >= 8 && encrypted.windows(value.len()).any(|window| window == value)
    })
}

pub fn plan_dismiss(selected: &[SelectedFile]) -> Result<Vec<DismissalRecord>, SelectionRefusal> {
    let mut records = Vec::new();
    for file in selected {
        for finding in &file.findings {
            let span = &finding.span;
            if !secret_dismissals::rule_is_dismissable(span.rule_id) {
                return Err(SelectionRefusal::UndismissableFinding);
            }
            let line = line_bytes_at(&file.original, span.line_start)
                .ok_or(SelectionRefusal::MissingLine)?;
            records.push(DismissalRecord {
                path: file.path.replace('\\', "/"),
                line_digest: line_content_digest(line),
                rule_id: span.rule_id.to_owned(),
                created_at: Some(unix_stamp()),
                note: Some("dismissed via secret_remediate".to_owned()),
            });
        }
    }
    Ok(records)
}

fn unix_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("unix:{seconds}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_id_matches_policy_and_selection_redacts_debug() {
        let mut source = Vec::new();
        source.extend_from_slice(b"token ");
        source.extend_from_slice(b"= ghp_");
        source.extend_from_slice(b"abcdef");
        source.extend_from_slice(b"ghijkl");
        source.extend_from_slice(b"mnopqr");
        source.extend_from_slice(b"stuvwx");
        source.extend_from_slice(b"123456");
        source.extend_from_slice(b"7890");
        let SecretSpansScan::Spans(spans) =
            crate::knowledge::scan_secret_spans("config.txt", &source)
        else {
            panic!("synthetic fixture must scan")
        };
        let span = spans.first().expect("synthetic finding");
        let id = mint_finding_id(
            "config.txt",
            span.rule_id,
            span.line_start,
            span.line_end,
            &span.shape,
        );
        let selected = select_requested(&["config.txt".into()], &[id], |_| Some(source.clone()))
            .expect("select finding");
        let debug = format!("{:?}", selected);
        assert!(!debug.contains("ghp_"));
        assert_eq!(selected.len(), 1);
    }
}
