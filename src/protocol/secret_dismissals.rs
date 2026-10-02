//! Content-digest-bound secret dismissal store (feature 034 US4).
//!
//! Avoids the five rejected allowlist modes:
//! 1. Bind includes content digest (not path+rule alone)
//! 2. Refusal text does not teach a self-service bypass
//! 3. Revocation is observable (delete/amend record)
//! 4. Loader: no symlink escape; explicit size cap
//! 5. Health / status MUST NOT echo unscanned dismissal file text

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hash::digest_hex;

/// Repo-relative path of the committed dismissal ledger.
pub const DISMISSAL_STORE_REL: &str = ".symforge/secret-dismissals.json";

/// Hard size cap for the dismissal store (bytes). Larger files are refused.
pub const DISMISSAL_STORE_MAX_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DismissalRecord {
    pub path: String,
    pub line_digest: String,
    pub rule_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct DismissalStoreFile {
    #[serde(default)]
    pub records: Vec<DismissalRecord>,
}

/// Digest of one logical line's bytes (no trailing newline).
pub fn line_content_digest(line_bytes: &[u8]) -> String {
    digest_hex(line_bytes)
}

/// Extract the 1-based line's bytes from `file_bytes` (without trailing `\n`/`\r`).
pub fn line_bytes_at(file_bytes: &[u8], line_1based: u32) -> Option<&[u8]> {
    if line_1based == 0 {
        return None;
    }
    let mut start = 0usize;
    let mut current = 1u32;
    for (idx, b) in file_bytes.iter().enumerate() {
        if *b == b'\n' {
            if current == line_1based {
                let end = idx;
                let slice = &file_bytes[start..end];
                return Some(trim_cr(slice));
            }
            start = idx + 1;
            current = current.saturating_add(1);
        }
    }
    if current == line_1based {
        return Some(trim_cr(&file_bytes[start..]));
    }
    None
}

fn trim_cr(slice: &[u8]) -> &[u8] {
    slice.strip_suffix(b"\r").unwrap_or(slice)
}

/// Load dismissals with symlink refusal + size cap. Returns empty on missing file.
pub fn load_dismissals(root: &Path) -> Result<Vec<DismissalRecord>, String> {
    let path = root.join(DISMISSAL_STORE_REL);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("dismissal store metadata: {e}")),
    };
    if meta.file_type().is_symlink() {
        return Err("dismissal store must not be a symlink".to_string());
    }
    if !meta.is_file() {
        return Err("dismissal store is not a regular file".to_string());
    }
    if meta.len() > DISMISSAL_STORE_MAX_BYTES {
        return Err(format!(
            "dismissal store exceeds size cap ({DISMISSAL_STORE_MAX_BYTES} bytes)"
        ));
    }
    let bytes = crate::protocol::read_gate::read_regular_file(&path)
        .map_err(|e| format!("read dismissal store: {e}"))?;
    if bytes.len() as u64 > DISMISSAL_STORE_MAX_BYTES {
        return Err("dismissal store exceeds size cap after read".to_string());
    }
    let parsed: DismissalStoreFile =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse dismissal store: {e}"))?;
    Ok(parsed.records)
}

/// True when a finding's path+rule+line content digest is recorded.
pub fn is_dismissed(
    records: &[DismissalRecord],
    path: &str,
    rule_id: &str,
    line_digest: &str,
) -> bool {
    let norm = path.replace('\\', "/");
    records.iter().any(|r| {
        r.path.replace('\\', "/") == norm && r.rule_id == rule_id && r.line_digest == line_digest
    })
}

/// Filter a sensitive scan: drop findings whose line digest is dismissed.
/// If all findings are dismissed → Clean. Never admits Indeterminate via dismiss.
pub fn filter_scan_with_dismissals(
    root: &Path,
    path: &str,
    bytes: &[u8],
    scan: crate::knowledge::SecretScan,
) -> crate::knowledge::SecretScan {
    use crate::knowledge::SecretScan;
    let SecretScan::Sensitive {
        mut rule_ids,
        mut finding_count,
        mut line_ranges,
        mut findings,
    } = scan
    else {
        return scan;
    };
    let records = match load_dismissals(root) {
        Ok(r) => r,
        Err(_) => {
            // Fail closed: keep original Sensitive (do not silently admit).
            return SecretScan::Sensitive {
                rule_ids,
                finding_count,
                line_ranges,
                findings,
            };
        }
    };
    if records.is_empty() {
        return SecretScan::Sensitive {
            rule_ids,
            finding_count,
            line_ranges,
            findings,
        };
    }
    let mut kept_findings = Vec::new();
    let mut kept_ranges = Vec::new();
    for (idx, finding) in findings.into_iter().enumerate() {
        let digest = line_bytes_at(bytes, finding.line_start)
            .map(line_content_digest)
            .unwrap_or_default();
        if digest.is_empty() || !is_dismissed(&records, path, finding.rule_id, &digest) {
            kept_ranges.push(
                line_ranges
                    .get(idx)
                    .copied()
                    .unwrap_or((finding.line_start, finding.line_end)),
            );
            kept_findings.push(finding);
        }
    }
    if kept_findings.is_empty() {
        return SecretScan::Clean;
    }
    finding_count = kept_findings.len() as u32;
    rule_ids = Vec::new();
    for f in &kept_findings {
        if !rule_ids.iter().any(|id| id == &f.rule_id) {
            rule_ids.push(f.rule_id);
        }
    }
    line_ranges = kept_ranges;
    findings = kept_findings;
    SecretScan::Sensitive {
        rule_ids,
        finding_count,
        line_ranges,
        findings,
    }
}

/// Append (or upsert) a dismissal record and return the serialized store bytes.
pub fn upsert_dismissal_bytes(
    existing: &[DismissalRecord],
    record: DismissalRecord,
) -> Result<Vec<u8>, String> {
    let mut records = existing.to_vec();
    records.retain(|r| {
        !(r.path.replace('\\', "/") == record.path.replace('\\', "/")
            && r.rule_id == record.rule_id
            && r.line_digest == record.line_digest)
    });
    records.push(record);
    let file = DismissalStoreFile { records };
    serde_json::to_vec_pretty(&file).map_err(|e| format!("serialize dismissal store: {e}"))
}

pub fn store_abs(root: &Path) -> PathBuf {
    root.join(DISMISSAL_STORE_REL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_digest_stable_and_line_extract() {
        let bytes = b"one\ntwo\r\nthree\n";
        assert_eq!(line_bytes_at(bytes, 1), Some(b"one".as_slice()));
        assert_eq!(line_bytes_at(bytes, 2), Some(b"two".as_slice()));
        assert_eq!(line_bytes_at(bytes, 3), Some(b"three".as_slice()));
        let d1 = line_content_digest(b"two");
        let d2 = line_content_digest(b"two");
        assert_eq!(d1, d2);
        assert_ne!(d1, line_content_digest(b"one"));
    }

    #[test]
    fn loader_rejects_symlink_and_oversize() {
        let dir = tempfile::tempdir().unwrap();
        let symforge = dir.path().join(".symforge");
        std::fs::create_dir_all(&symforge).unwrap();
        let target = dir.path().join("target.json");
        std::fs::write(&target, b"{\"records\":[]}").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, store_abs(dir.path())).unwrap();
            let err = load_dismissals(dir.path()).unwrap_err();
            assert!(err.contains("symlink"), "{err}");
            std::fs::remove_file(store_abs(dir.path())).unwrap();
        }
        // Oversize: write a file larger than cap
        let big = vec![b'x'; (DISMISSAL_STORE_MAX_BYTES as usize) + 8];
        std::fs::write(store_abs(dir.path()), &big).unwrap();
        let err = load_dismissals(dir.path()).unwrap_err();
        assert!(err.contains("size cap"), "{err}");
    }
}
