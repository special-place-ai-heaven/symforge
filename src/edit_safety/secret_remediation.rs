//! Feature-neutral staging for guarded secret-remediation writes.
//! Source and replacement bytes remain private to the caller.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::knowledge::secret_dismissals::{self, DISMISSAL_STORE_MAX_BYTES, DISMISSAL_STORE_REL};
use crate::knowledge::secret_remediation::{self, SelectedFile, SelectionRefusal};

use super::batch_commit::StagedImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StageError {
    InvalidRequest,
    WriteConflict,
    SourceUnavailable,
    ResourceLimit,
    Selection(SelectionRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncryptError {
    InvalidFormat,
    Unavailable,
    ResourceLimit,
    #[cfg(feature = "embed")]
    Cancelled,
    DeadlineExceeded,
    SourceUnavailable,
}

/// Encrypts the admitted bytes, never a path reopened by the child. The only
/// temporary artifact contains ciphertext and is removed with its directory.
pub(crate) fn encrypt_selected_bytes(
    path: &str,
    selected_bytes: &[u8],
    binary: &Path,
    recipient: &str,
    timeout: Duration,
    mut stopped: impl FnMut() -> Option<EncryptError>,
) -> Result<Vec<u8>, EncryptError> {
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    if selected_bytes.len() > MAX_BYTES {
        return Err(EncryptError::ResourceLimit);
    }
    if let Some(reason) = stopped() {
        return Err(reason);
    }
    let format = secret_remediation::sops_input_type(path).ok_or(EncryptError::InvalidFormat)?;
    let directory = tempfile::tempdir().map_err(|_| EncryptError::SourceUnavailable)?;
    let output = directory.path().join(
        Path::new(path)
            .file_name()
            .ok_or(EncryptError::InvalidFormat)?,
    );
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(EncryptError::DeadlineExceeded)?;
    let mut child = crate::process_util::hidden_command(binary)
        .env_clear()
        .args(["encrypt", "--age", recipient, "--output"])
        .arg(&output)
        .args([
            "--filename-override",
            path,
            "--input-type",
            format,
            "--output-type",
            format,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| EncryptError::Unavailable)?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        EncryptError::Unavailable
    })?;
    let input = selected_bytes.to_vec();
    let writer = std::thread::spawn(move || {
        use std::io::Write as _;
        stdin.write_all(&input)
    });
    let outcome = loop {
        if let Some(reason) = stopped() {
            break Err(reason);
        }
        if Instant::now() >= deadline {
            break Err(EncryptError::DeadlineExceeded);
        }
        if std::fs::metadata(&output).is_ok_and(|meta| meta.len() > MAX_BYTES as u64) {
            break Err(EncryptError::ResourceLimit);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break Err(EncryptError::Unavailable),
        }
    };
    if outcome.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let wrote_input = writer.join().is_ok_and(|write| write.is_ok());
    let status = outcome?;
    if !wrote_input || !status.success() {
        return Err(EncryptError::Unavailable);
    }
    let mut file = std::fs::File::open(&output).map_err(|_| EncryptError::SourceUnavailable)?;
    use std::io::Read as _;
    let mut bytes = Vec::new();
    file.by_ref()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| EncryptError::SourceUnavailable)?;
    if bytes.len() > MAX_BYTES {
        return Err(EncryptError::ResourceLimit);
    }
    Ok(bytes)
}

fn image(
    root: &Path,
    path: &str,
    original: Option<Vec<u8>>,
    replacement: Vec<u8>,
    owner_only: bool,
) -> StagedImage {
    StagedImage {
        relative: PathBuf::from(path),
        absolute: root.join(path),
        original,
        replacement,
        owner_only,
    }
}

pub(crate) fn stage_externalize(
    root: &Path,
    selected: Vec<SelectedFile>,
    mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>, StageError>,
) -> Result<Vec<StagedImage>, StageError> {
    let plans = secret_remediation::plan_externalize(selected);
    let original_env = read(".env")?;
    let mut env = original_env
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()
        .map_err(|_| StageError::InvalidRequest)?
        .unwrap_or("")
        .to_owned();
    if !env.is_empty() && !env.ends_with('\n') {
        env.push('\n');
    }
    for plan in &plans {
        for line in &plan.env_lines {
            let (key, value) = line.split_once('=').ok_or(StageError::InvalidRequest)?;
            if value.contains('\r') || value.contains('\n') {
                return Err(StageError::InvalidRequest);
            }
            if let Some(existing) = env
                .lines()
                .find(|entry| entry.starts_with(&format!("{key}=")))
            {
                if existing.split_once('=').map(|(_, current)| current) != Some(value) {
                    return Err(StageError::WriteConflict);
                }
            } else {
                env.push_str(line);
                env.push('\n');
            }
        }
    }
    let original_ignore = read(".gitignore")?;
    let mut ignore = original_ignore
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()
        .map_err(|_| StageError::InvalidRequest)?
        .unwrap_or("")
        .to_owned();
    if !ignore.lines().any(|line| line.trim() == ".env") {
        if !ignore.is_empty() && !ignore.ends_with('\n') {
            ignore.push('\n');
        }
        ignore.push_str(".env\n");
    }
    let mut images = vec![image(root, ".env", original_env, env.into_bytes(), true)];
    images.extend(
        plans
            .into_iter()
            .map(|plan| image(root, &plan.path, Some(plan.original), plan.rewritten, false)),
    );
    images.push(image(
        root,
        ".gitignore",
        original_ignore,
        ignore.into_bytes(),
        false,
    ));
    Ok(images)
}

pub(crate) fn stage_dismiss(
    root: &Path,
    selected: &[SelectedFile],
    mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>, StageError>,
) -> Result<Vec<StagedImage>, StageError> {
    let records = secret_remediation::plan_dismiss(selected).map_err(StageError::Selection)?;
    let original = read(DISMISSAL_STORE_REL)?;
    if original
        .as_ref()
        .is_some_and(|bytes| bytes.len() as u64 > DISMISSAL_STORE_MAX_BYTES)
    {
        return Err(StageError::ResourceLimit);
    }
    let mut merged = match original.as_deref() {
        Some(bytes) => secret_dismissals::parse_dismissals_bytes(bytes)
            .map_err(|_| StageError::SourceUnavailable)?,
        None => Vec::new(),
    };
    for record in records {
        merged.retain(|previous| {
            !(previous.path == record.path
                && previous.rule_id == record.rule_id
                && previous.line_digest == record.line_digest)
        });
        merged.push(record);
    }
    let replacement = serde_json::to_vec_pretty(&serde_json::json!({ "records": merged }))
        .map_err(|_| StageError::InvalidRequest)?;
    if replacement.len() as u64 > DISMISSAL_STORE_MAX_BYTES {
        return Err(StageError::ResourceLimit);
    }
    Ok(vec![image(
        root,
        DISMISSAL_STORE_REL,
        original,
        replacement,
        false,
    )])
}
