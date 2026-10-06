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

/// Raw store bytes, `Ok(None)` when the store is absent. Refuses a symlink, a
/// non-regular file, and anything over the size cap. On Unix the open is
/// `O_NOFOLLOW | O_NONBLOCK`, so a swap between the check and the open can
/// neither follow a link nor hang on a FIFO.
fn read_store_bytes(root: &Path) -> Result<Option<Vec<u8>>, String> {
    let path = store_abs(root);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
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
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
    };
    #[cfg(not(unix))]
    let file = std::fs::File::open(&path);
    let file = file.map_err(|e| format!("read dismissal store: {e}"))?;
    if !file.metadata().is_ok_and(|opened| opened.is_file()) {
        return Err("dismissal store is not a regular file".to_string());
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(file, DISMISSAL_STORE_MAX_BYTES + 1),
        &mut bytes,
    )
    .map_err(|e| format!("read dismissal store: {e}"))?;
    if bytes.len() as u64 > DISMISSAL_STORE_MAX_BYTES {
        return Err("dismissal store exceeds size cap after read".to_string());
    }
    Ok(Some(bytes))
}

/// Load dismissals with symlink refusal + size cap. Returns empty on missing file.
pub fn load_dismissals(root: &Path) -> Result<Vec<DismissalRecord>, String> {
    let Some(bytes) = read_store_bytes(root)? else {
        return Ok(Vec::new());
    };
    let parsed: DismissalStoreFile =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse dismissal store: {e}"))?;
    Ok(parsed.records)
}

/// Digest of the store bytes as they would be applied: empty when absent, a
/// fixed marker when refused (the classifier then fails closed), else the
/// content digest. Persisted snapshots carry it so a store edited since the
/// snapshot invalidates the verdicts recorded under the old one.
pub fn dismissal_store_digest(root: &Path) -> String {
    match read_store_bytes(root) {
        Ok(None) => String::new(),
        Ok(Some(bytes)) => digest_hex(&bytes),
        Err(_) => "refused".to_string(),
    }
}

/// Number of records in the store, `None` when it cannot be loaded. Health
/// reports only this count, never paths, rules, or contents.
pub fn dismissal_count(root: &Path) -> Option<usize> {
    load_dismissals(root).ok().map(|records| records.len())
}

/// Digest persisted when no classification pass recorded what it ran under.
/// It never equals a real store digest, so such a snapshot always re-scouts.
pub const UNKNOWN_DISMISSAL_DIGEST: &str = "unknown";

/// The dismissal state a root's current verdicts were classified under: the
/// store digest, and every path the store's records named at that moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeldDismissals {
    pub digest: String,
    pub paths: std::collections::BTreeSet<String>,
}

/// Read the store at `root` now. A store that cannot be loaded names no
/// paths; the classifier fails closed on it, so nothing is admitted under it.
pub fn observe_dismissals(root: &Path) -> HeldDismissals {
    HeldDismissals {
        digest: dismissal_store_digest(root),
        paths: load_dismissals(root)
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| record.path.replace('\\', "/"))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

// ponytail: one process-wide map keyed by canonical root instead of a field on
// the index handle. Cold load, reload, snapshot restore (CLI, daemon, serve,
// embed) and every single-file lane already share these few choke points; a
// handle field would need plumbing through each install route.
fn held_registry() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, HeldDismissals>>
{
    static HELD: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, HeldDismissals>>,
    > = std::sync::OnceLock::new();
    HELD.get_or_init(Default::default)
}

fn held_key(root: &Path) -> PathBuf {
    dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// The dismissal state the live verdicts for `root` were classified under,
/// `None` when no classification pass has recorded one in this process.
pub fn held_dismissals(root: &Path) -> Option<HeldDismissals> {
    held_registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&held_key(root))
        .cloned()
}

/// Record the dismissal state the live verdicts for `root` now reflect.
pub fn set_held_dismissals(root: &Path, held: HeldDismissals) {
    held_registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(held_key(root), held);
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

/// Rules a dismissal can never clear. `secret.private-key-envelope` matches
/// only the constant `-----BEGIN ... PRIVATE KEY-----` header, so a line digest
/// would bind the header and not the key material beneath it.
pub const UNDISMISSABLE_RULE_IDS: &[&str] = &["secret.private-key-envelope"];

/// False for a rule whose matched line does not carry the secret itself.
pub fn rule_is_dismissable(rule_id: &str) -> bool {
    !UNDISMISSABLE_RULE_IDS.contains(&rule_id)
}

/// The one dismissal-aware secret scan, shared by the publication routes and
/// the read gate: a sensitive PATH is never filtered, everything else goes
/// through [`filter_scan_with_dismissals`].
pub fn scan_with_dismissals(root: &Path, path: &str, bytes: &[u8]) -> crate::knowledge::SecretScan {
    let scan = crate::knowledge::scan_secret_bytes(path, bytes);
    if crate::knowledge::sensitive_path_rule(path).is_some() {
        return scan;
    }
    filter_scan_with_dismissals(root, path, bytes, scan)
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
    // The scan keeps only the first FINDING_LINE_RANGES_KEPT findings. A
    // finding it did not keep cannot be checked against the store, so a
    // truncated scan is never filtered: fail closed.
    if findings.len() != finding_count as usize {
        return SecretScan::Sensitive {
            rule_ids,
            finding_count,
            line_ranges,
            findings,
        };
    }
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
        if digest.is_empty()
            || !rule_is_dismissable(finding.rule_id)
            || !is_dismissed(&records, path, finding.rule_id, &digest)
        {
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

    #[test]
    fn root_classifier_admits_only_fully_dismissed_unchanged_lines() {
        use crate::knowledge::StableContentAdmission;
        let dir = tempfile::tempdir().unwrap();
        let path = "config/app.json";
        let key = ["pass", "word"].concat();
        let body = format!("{{\n  \"{key}\": \"S3cretValue9xAb\"\n}}\n");
        let targets = crate::domain::IndexTargets::for_path(path, None);
        let classify = |bytes: &[u8]| {
            crate::knowledge::classify_stable_content_for_root(dir.path(), path, targets, bytes)
        };
        assert!(matches!(
            classify(body.as_bytes()),
            StableContentAdmission::MetadataOnly(_)
        ));
        assert_eq!(dismissal_store_digest(dir.path()), "");

        let crate::knowledge::SecretScan::Sensitive { findings, .. } =
            crate::knowledge::scan_secret_bytes(path, body.as_bytes())
        else {
            panic!("fixture must be sensitive");
        };
        let records = findings
            .iter()
            .map(|finding| DismissalRecord {
                path: path.to_string(),
                line_digest: line_content_digest(
                    line_bytes_at(body.as_bytes(), finding.line_start).unwrap(),
                ),
                rule_id: finding.rule_id.to_string(),
                created_at: None,
                note: None,
            })
            .collect();
        std::fs::create_dir_all(dir.path().join(".symforge")).unwrap();
        std::fs::write(
            store_abs(dir.path()),
            serde_json::to_vec(&DismissalStoreFile { records }).unwrap(),
        )
        .unwrap();
        assert_eq!(classify(body.as_bytes()), StableContentAdmission::Admitted);
        assert_ne!(dismissal_store_digest(dir.path()), "");
        assert_eq!(dismissal_count(dir.path()), Some(findings.len()));

        // A changed secret line no longer matches its dismissal digest.
        let changed = body.replace("S3cretValue9xAb", "Zt7Value9xAbQr3");
        assert!(matches!(
            classify(changed.as_bytes()),
            StableContentAdmission::MetadataOnly(_)
        ));
    }

    /// Write a store that dismisses every finding the scan of `body` reports.
    fn dismiss_every_reported_finding(root: &Path, path: &str, body: &str) -> usize {
        let crate::knowledge::SecretScan::Sensitive { findings, .. } =
            crate::knowledge::scan_secret_bytes(path, body.as_bytes())
        else {
            panic!("fixture must be sensitive");
        };
        let records: Vec<DismissalRecord> = findings
            .iter()
            .map(|finding| DismissalRecord {
                path: path.to_string(),
                line_digest: line_content_digest(
                    line_bytes_at(body.as_bytes(), finding.line_start).unwrap(),
                ),
                rule_id: finding.rule_id.to_string(),
                created_at: None,
                note: None,
            })
            .collect();
        std::fs::create_dir_all(root.join(".symforge")).unwrap();
        std::fs::write(
            store_abs(root),
            serde_json::to_vec(&DismissalStoreFile {
                records: records.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        records.len()
    }

    #[test]
    fn truncated_scan_is_never_filtered() {
        use crate::knowledge::StableContentAdmission;
        let dir = tempfile::tempdir().unwrap();
        let path = "config/app.json";
        let key = ["pass", "word"].concat();
        let lines: Vec<String> = (0..12)
            .map(|n| format!("  \"{key}\": \"S3cretValue9xAb{n:02}\""))
            .collect();
        let body = format!("{{\n{}\n}}\n", lines.join(",\n"));
        let crate::knowledge::SecretScan::Sensitive {
            finding_count,
            findings,
            ..
        } = crate::knowledge::scan_secret_bytes(path, body.as_bytes())
        else {
            panic!("fixture must be sensitive");
        };
        assert_eq!(finding_count, 12, "fixture must carry twelve findings");
        assert_eq!(
            findings.len(),
            crate::knowledge::FINDING_LINE_RANGES_KEPT,
            "the scan keeps only the first eleven"
        );
        assert_eq!(dismiss_every_reported_finding(dir.path(), path, &body), 11);
        let targets = crate::domain::IndexTargets::for_path(path, None);
        assert!(matches!(
            crate::knowledge::classify_stable_content_for_root(
                dir.path(),
                path,
                targets,
                body.as_bytes()
            ),
            StableContentAdmission::MetadataOnly(_)
        ));
    }

    #[test]
    fn private_key_envelope_is_never_dismissed() {
        use crate::knowledge::StableContentAdmission;
        let dir = tempfile::tempdir().unwrap();
        let path = "docs/notes.txt";
        // Assembled at runtime so this source file carries no key header itself.
        let kind = "PRIVATE";
        let body = format!(
            "notes\n-----BEGIN RSA {kind} KEY-----\nMIIBOgIBAAJBAKj34GkxFhD90vcNLYLInFEX6Ppy1tPf9Cnzj4p4WGeKLs1P\n-----END RSA {kind} KEY-----\n"
        );
        let crate::knowledge::SecretScan::Sensitive { rule_ids, .. } =
            crate::knowledge::scan_secret_bytes(path, body.as_bytes())
        else {
            panic!("fixture must be sensitive");
        };
        assert!(rule_ids.contains(&"secret.private-key-envelope"));
        assert!(!rule_is_dismissable("secret.private-key-envelope"));
        dismiss_every_reported_finding(dir.path(), path, &body);
        let targets = crate::domain::IndexTargets::for_path(path, None);
        assert!(matches!(
            crate::knowledge::classify_stable_content_for_root(
                dir.path(),
                path,
                targets,
                body.as_bytes()
            ),
            StableContentAdmission::MetadataOnly(_)
        ));
    }
}
