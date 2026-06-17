//! Backup-then-apply writer for harness configs: dry-run planning, timestamped
//! backups, atomic writes, restore, and idempotency.
//!
//! Safety invariants (spec GATE-1 / GATE-2):
//! - **Dry-run writes nothing.** [`plan`] performs no filesystem mutation.
//! - **Every write is preceded by a restorable backup.** [`apply`] writes a
//!   timestamped `<config>.<ts>.bak` of the prior bytes *before* the atomic
//!   write, and [`restore`] reproduces the prior file byte-for-byte.
//! - **Idempotent.** A target already in `PresentCurrent` is skipped; re-apply
//!   with the same inputs changes nothing.
//! - **Non-aborting.** A malformed or permission-denied target is reported as
//!   an error in the plan/result and never corrupts the file or aborts the run.

use std::path::{Path, PathBuf};

use crate::cli::harness::{
    AttachEntry, HarnessFormat, HarnessId, HarnessRegistry, HarnessState, apply_attach_entry,
};
use crate::cli::init::read_config_text;

/// What an apply would do to a single target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedAction {
    /// Add a new SymForge attach entry (config absent of one).
    Add,
    /// Refresh an existing (stale or duplicated) SymForge entry.
    Refresh,
    /// Nothing to do — already current, or client not installed.
    Skip(String),
    /// Target could not be planned safely (malformed / unreadable); never written.
    Error(String),
}

/// A planned change for one harness target.
#[derive(Debug, Clone)]
pub struct PlannedChange {
    pub id: HarnessId,
    pub config_path: PathBuf,
    pub format: HarnessFormat,
    pub action: PlannedAction,
}

impl PlannedChange {
    /// True when [`apply`] would actually write this target.
    pub fn writes(&self) -> bool {
        matches!(self.action, PlannedAction::Add | PlannedAction::Refresh)
    }
}

/// A dry-run plan over all targets. Holds the desired entry so [`apply`] can
/// reuse it without re-deriving.
#[derive(Debug, Clone)]
pub struct ApplyPlan {
    pub entry: AttachEntry,
    pub changes: Vec<PlannedChange>,
}

/// A backup created during [`apply`], mapped to its source for [`restore`].
#[derive(Debug, Clone)]
pub struct BackupRecord {
    pub source: PathBuf,
    pub backup: PathBuf,
}

/// The outcome of applying one target.
#[derive(Debug, Clone)]
pub enum ApplyOutcome {
    /// Written successfully; carries the backup created beforehand (if the file
    /// pre-existed) so it can be restored.
    Wrote {
        id: HarnessId,
        config_path: PathBuf,
        backup: Option<BackupRecord>,
    },
    /// Skipped (already current or not installed).
    Skipped { id: HarnessId, reason: String },
    /// Failed without corrupting the target.
    Failed { id: HarnessId, reason: String },
}

/// Build a dry-run plan. **No filesystem mutation.**
pub fn plan(registry: &HarnessRegistry, entry: &AttachEntry) -> ApplyPlan {
    let statuses = registry.scan(entry);
    let changes = statuses
        .into_iter()
        .map(|status| {
            let action = match status.state {
                HarnessState::NotInstalled => {
                    PlannedAction::Skip("client not installed".to_string())
                }
                HarnessState::PresentCurrent => PlannedAction::Skip("already current".to_string()),
                HarnessState::Absent => PlannedAction::Add,
                HarnessState::PresentStale => PlannedAction::Refresh,
                HarnessState::Malformed(why) => {
                    PlannedAction::Error(format!("config does not parse: {why}"))
                }
            };
            PlannedChange {
                id: status.id,
                config_path: status.config_path,
                format: status.format,
                action,
            }
        })
        .collect();

    ApplyPlan {
        entry: entry.clone(),
        changes,
    }
}

/// Execute a plan: for each writable target, back up the prior bytes (if any)
/// then atomically write the new content. Malformed / permission-denied targets
/// are reported and never abort the run. Returns per-target outcomes.
pub fn apply(plan: &ApplyPlan) -> Vec<ApplyOutcome> {
    plan.changes
        .iter()
        .map(|change| match &change.action {
            PlannedAction::Skip(reason) => ApplyOutcome::Skipped {
                id: change.id,
                reason: reason.clone(),
            },
            PlannedAction::Error(reason) => ApplyOutcome::Failed {
                id: change.id,
                reason: reason.clone(),
            },
            PlannedAction::Add | PlannedAction::Refresh => apply_one(change, &plan.entry),
        })
        .collect()
}

/// Apply a single writable change: backup-then-atomic-write. Any error is
/// captured as `Failed` (the run continues); the live file is never left
/// half-written because the new content is materialized via an atomic rename.
fn apply_one(change: &PlannedChange, entry: &AttachEntry) -> ApplyOutcome {
    let path = &change.config_path;

    // Read prior text (BOM-safe). A missing file is fine (fresh create); any
    // other read error (e.g. permission denied) is reported, not fatal.
    let existing_text = if path.exists() {
        match read_config_text(path) {
            Ok(text) => Some(text),
            Err(e) => {
                return ApplyOutcome::Failed {
                    id: change.id,
                    reason: format!("cannot read {}: {e}", path.display()),
                };
            }
        }
    } else {
        None
    };

    // Compute the new content first. If the transform fails (should not for a
    // planned Add/Refresh, but be defensive), nothing is written.
    let new_content = match apply_attach_entry(change.format, existing_text.as_deref(), entry) {
        Ok(content) => content,
        Err(e) => {
            return ApplyOutcome::Failed {
                id: change.id,
                reason: format!("building entry for {}: {e}", path.display()),
            };
        }
    };

    // Back up the *raw prior bytes* (not the BOM-stripped text) so restore is
    // byte-exact, before any write.
    let backup = if path.exists() {
        match write_backup(path) {
            Ok(record) => Some(record),
            Err(e) => {
                return ApplyOutcome::Failed {
                    id: change.id,
                    reason: format!("backing up {}: {e}", path.display()),
                };
            }
        }
    } else {
        // Ensure the parent directory exists for a fresh create.
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return ApplyOutcome::Failed {
                id: change.id,
                reason: format!("creating {}: {e}", parent.display()),
            };
        }
        None
    };

    if let Err(e) = atomic_write(path, new_content.as_bytes()) {
        return ApplyOutcome::Failed {
            id: change.id,
            reason: format!("writing {}: {e}", path.display()),
        };
    }

    ApplyOutcome::Wrote {
        id: change.id,
        config_path: path.clone(),
        backup,
    }
}

/// Write a timestamped backup of `path`'s current raw bytes beside it.
/// Returns the [`BackupRecord`] for restore.
pub fn write_backup(path: &Path) -> std::io::Result<BackupRecord> {
    let prior = std::fs::read(path)?;
    let backup = backup_path(path);
    std::fs::write(&backup, &prior)?;
    Ok(BackupRecord {
        source: path.to_path_buf(),
        backup,
    })
}

/// Compute the timestamped backup path: `<config>.<UTC-compact>.bak`.
fn backup_path(path: &Path) -> PathBuf {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".to_string());
    let backup_name = format!("{name}.{ts}.bak");
    match path.parent() {
        Some(parent) => parent.join(backup_name),
        None => PathBuf::from(backup_name),
    }
}

/// Restore a backup over its source, reproducing the prior bytes exactly.
pub fn restore(record: &BackupRecord) -> std::io::Result<()> {
    let bytes = std::fs::read(&record.backup)?;
    atomic_write(&record.source, &bytes)
}

/// Atomically write `content` to `path` (temp file in the same dir + rename).
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "config path has no parent directory",
        )
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    // rename(2) on Unix / MoveFileExW(MOVEFILE_REPLACE_EXISTING) on Windows.
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::harness::{HarnessTarget, SYMFORGE_SERVER_NAME};

    fn entry() -> AttachEntry {
        AttachEntry::new("http://127.0.0.1:8787/mcp", Some("sf_key".to_string()))
    }

    fn json_target(id: HarnessId, path: PathBuf) -> HarnessTarget {
        HarnessTarget {
            id,
            config_path: path,
            format: HarnessFormat::Json,
        }
    }

    #[test]
    fn plan_classifies_absent_as_add() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join(".claude.json");
        std::fs::write(&cfg, "{}").unwrap();
        let reg = HarnessRegistry::from_targets(vec![json_target(HarnessId::ClaudeCode, cfg)]);
        let p = plan(&reg, &entry());
        assert_eq!(p.changes[0].action, PlannedAction::Add);
    }

    #[test]
    fn apply_creates_backup_and_restores_byte_exact() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.json");
        let original = "{\n  \"mcpServers\": {}\n}\n";
        std::fs::write(&cfg, original).unwrap();

        let reg =
            HarnessRegistry::from_targets(vec![json_target(HarnessId::ClaudeCode, cfg.clone())]);
        let p = plan(&reg, &entry());
        let outcomes = apply(&p);

        let backup = match &outcomes[0] {
            ApplyOutcome::Wrote { backup, .. } => backup.clone().expect("backup recorded"),
            other => panic!("expected Wrote, got {other:?}"),
        };

        // File now has the symforge entry.
        let after = std::fs::read_to_string(&cfg).unwrap();
        assert!(after.contains(SYMFORGE_SERVER_NAME));

        // Restore reproduces the prior bytes exactly.
        restore(&backup).unwrap();
        let restored = std::fs::read(&cfg).unwrap();
        assert_eq!(restored, original.as_bytes());
    }

    #[test]
    fn second_apply_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.json");
        std::fs::write(&cfg, "{}").unwrap();
        let reg =
            HarnessRegistry::from_targets(vec![json_target(HarnessId::ClaudeCode, cfg.clone())]);

        apply(&plan(&reg, &entry()));
        let after_first = std::fs::read(&cfg).unwrap();

        // Re-plan against the now-current config: should be a Skip.
        let p2 = plan(&reg, &entry());
        assert!(matches!(p2.changes[0].action, PlannedAction::Skip(_)));
        let outcomes2 = apply(&p2);
        assert!(matches!(outcomes2[0], ApplyOutcome::Skipped { .. }));

        let after_second = std::fs::read(&cfg).unwrap();
        assert_eq!(
            after_first, after_second,
            "second apply must not change bytes"
        );
    }

    #[test]
    fn malformed_is_reported_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.json");
        let bad = "{ not valid json";
        std::fs::write(&cfg, bad).unwrap();
        let reg =
            HarnessRegistry::from_targets(vec![json_target(HarnessId::ClaudeCode, cfg.clone())]);

        let p = plan(&reg, &entry());
        assert!(matches!(p.changes[0].action, PlannedAction::Error(_)));
        let outcomes = apply(&p);
        assert!(matches!(outcomes[0], ApplyOutcome::Failed { .. }));

        // File is untouched.
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), bad);
    }
}
