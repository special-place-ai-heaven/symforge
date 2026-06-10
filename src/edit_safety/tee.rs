use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::paths;

/// Maximum number of edit snapshots retained in `.symforge/tee/`. Newer
/// snapshots beyond this count are pruned at write time. Override with
/// `SYMFORGE_TEE_MAX_COUNT`; set to `0` to disable count-based pruning.
pub const TEE_MAX_FILES: usize = 200;

/// Maximum age of a retained snapshot. Older snapshots are pruned at write
/// time. Override with `SYMFORGE_TEE_MAX_AGE_DAYS`; set to `0` to disable
/// age-based pruning.
pub const TEE_MAX_AGE_DAYS: u64 = 7;

pub const TEE_MAX_FILE_BYTES: usize = 1024 * 1024;

/// Env override for the retained snapshot count (`0` disables count pruning).
pub const TEE_MAX_COUNT_ENV: &str = "SYMFORGE_TEE_MAX_COUNT";
/// Env override for the retained snapshot age in days (`0` disables age pruning).
pub const TEE_MAX_AGE_DAYS_ENV: &str = "SYMFORGE_TEE_MAX_AGE_DAYS";

static TEE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Retention limits applied to the tee directory after each snapshot write.
/// `0` on either field disables that dimension of pruning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeeRetention {
    /// Keep at most this many snapshots; `0` disables count-based pruning.
    pub max_count: usize,
    /// Delete snapshots older than this; `None` disables age-based pruning.
    pub max_age: Option<Duration>,
}

impl TeeRetention {
    /// Resolve retention limits from the environment, falling back to the
    /// compiled defaults. Mirrors the `*_from_value` / `*_from_env` pattern
    /// used by `persist::checkpoint_interval_from_env`.
    pub fn from_env() -> Self {
        Self {
            max_count: max_count_from_value(std::env::var(TEE_MAX_COUNT_ENV).ok().as_deref()),
            max_age: max_age_from_value(std::env::var(TEE_MAX_AGE_DAYS_ENV).ok().as_deref()),
        }
    }
}

impl Default for TeeRetention {
    fn default() -> Self {
        Self {
            max_count: TEE_MAX_FILES,
            max_age: Some(Duration::from_secs(TEE_MAX_AGE_DAYS * 86_400)),
        }
    }
}

/// Parse the count override. Unparseable values fall back to the default.
/// `0` is honored as "disable count pruning".
fn max_count_from_value(raw: Option<&str>) -> usize {
    match raw.map(str::trim) {
        Some(value) if !value.is_empty() => value.parse::<usize>().unwrap_or(TEE_MAX_FILES),
        _ => TEE_MAX_FILES,
    }
}

/// Parse the age override (in days). Unparseable values fall back to the
/// default. `0` disables age pruning (returns `None`).
fn max_age_from_value(raw: Option<&str>) -> Option<Duration> {
    let days = match raw.map(str::trim) {
        Some(value) if !value.is_empty() => value.parse::<u64>().unwrap_or(TEE_MAX_AGE_DAYS),
        _ => TEE_MAX_AGE_DAYS,
    };
    if days == 0 {
        None
    } else {
        Some(Duration::from_secs(days * 86_400))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeeRecord {
    pub original_path: PathBuf,
    pub tee_path: PathBuf,
    pub repo_root: PathBuf,
}

impl TeeRecord {
    pub fn recovery_hint(&self) -> String {
        format!(
            "Tee snapshot: `{}` preserves `{}` before this write.",
            display_relative(&self.repo_root, &self.tee_path),
            display_relative(&self.repo_root, &self.original_path),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeeSnapshot {
    Created(TeeRecord),
    SkippedMissing {
        original_path: PathBuf,
    },
    SkippedTooLarge {
        size: usize,
        max_size: usize,
    },
    Warning {
        original_path: PathBuf,
        message: String,
    },
}

impl TeeSnapshot {
    pub fn response_hint(&self) -> Option<String> {
        match self {
            Self::Created(record) => Some(record.recovery_hint()),
            Self::SkippedMissing { .. } => None,
            Self::SkippedTooLarge { size, max_size } => Some(format!(
                "Tee snapshot skipped: original file is {size} bytes, above {max_size} byte cap."
            )),
            Self::Warning { message, .. } => Some(format!(
                "Tee snapshot warning: {message}; edit still proceeded."
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tee {
    repo_root: PathBuf,
    retention: TeeRetention,
    max_file_bytes: u64,
}

impl Tee {
    pub fn for_repo(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            retention: TeeRetention::from_env(),
            max_file_bytes: TEE_MAX_FILE_BYTES as u64,
        }
    }

    /// Construct a `Tee` with explicit retention limits, bypassing env
    /// resolution. Primarily for tests that exercise pruning behavior.
    pub fn with_retention(repo_root: impl AsRef<Path>, retention: TeeRetention) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            retention,
            max_file_bytes: TEE_MAX_FILE_BYTES as u64,
        }
    }

    pub fn for_target(target: impl AsRef<Path>) -> Self {
        Self::for_repo(discover_repo_root(target.as_ref()))
    }

    pub fn snapshot(&self, original_path: impl AsRef<Path>) -> io::Result<TeeSnapshot> {
        let original_path = original_path.as_ref();
        let metadata = match fs::metadata(original_path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(TeeSnapshot::SkippedMissing {
                    original_path: original_path.to_path_buf(),
                });
            }
            Err(err) => {
                return Ok(TeeSnapshot::Warning {
                    original_path: original_path.to_path_buf(),
                    message: format!("could not inspect {}: {err}", original_path.display()),
                });
            }
        };

        if !metadata.is_file() {
            return Ok(TeeSnapshot::Warning {
                original_path: original_path.to_path_buf(),
                message: format!("{} is not a regular file", original_path.display()),
            });
        }

        if metadata.len() > self.max_file_bytes {
            return Ok(TeeSnapshot::SkippedTooLarge {
                size: metadata.len() as usize,
                max_size: self.max_file_bytes as usize,
            });
        }

        let tee_dir = paths::ensure_symforge_dir(&self.repo_root)
            .map(|dir| dir.join("tee"))
            .and_then(|dir| {
                fs::create_dir_all(&dir)?;
                Ok(dir)
            });
        let tee_dir = match tee_dir {
            Ok(dir) => dir,
            Err(err) => {
                return Ok(TeeSnapshot::Warning {
                    original_path: original_path.to_path_buf(),
                    message: format!("could not create tee directory: {err}"),
                });
            }
        };

        let tee_path = tee_dir.join(snapshot_file_name(original_path));
        if let Err(err) = fs::copy(original_path, &tee_path) {
            return Ok(TeeSnapshot::Warning {
                original_path: original_path.to_path_buf(),
                message: format!(
                    "could not snapshot {} to {}: {err}",
                    original_path.display(),
                    tee_path.display()
                ),
            });
        }

        if let Err(err) = enforce_retention(&tee_dir, self.retention) {
            tracing::warn!(
                "tee snapshot retention failed for {}: {err}",
                tee_dir.display()
            );
        }

        Ok(TeeSnapshot::Created(TeeRecord {
            original_path: original_path.to_path_buf(),
            tee_path,
            repo_root: self.repo_root.clone(),
        }))
    }
}

fn discover_repo_root(target: &Path) -> PathBuf {
    let start = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    };

    for ancestor in start.ancestors() {
        if ancestor.join(".git").exists() || ancestor.join(paths::SYMFORGE_DIR_NAME).exists() {
            return ancestor.to_path_buf();
        }
    }

    start.to_path_buf()
}

fn snapshot_file_name(original_path: &Path) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = TEE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = original_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    format!("{millis}-{counter:06}-{}", sanitize_file_name(file_name))
}

fn sanitize_file_name(file_name: &str) -> String {
    file_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// One snapshot file considered for retention.
struct TeeEntry {
    modified: SystemTime,
    name: std::ffi::OsString,
    path: PathBuf,
}

/// Pure retention policy: given the current snapshot set and a reference
/// `now`, decide which files to delete. Age pruning runs first (drop anything
/// older than `max_age`), then count pruning trims the oldest survivors down
/// to `max_count`. Both dimensions are independently disableable
/// (`max_count == 0` / `max_age == None`).
///
/// Returns the paths to delete. Kept side-effect-free so it can be unit-tested
/// with synthetic timestamps without manipulating filesystem mtimes.
fn plan_retention(
    mut entries: Vec<TeeEntry>,
    retention: TeeRetention,
    now: SystemTime,
) -> Vec<PathBuf> {
    // Oldest first so count pruning can take the leading prefix.
    entries.sort_by(|a, b| {
        a.modified
            .cmp(&b.modified)
            .then_with(|| a.name.cmp(&b.name))
    });

    let age_cutoff = retention
        .max_age
        .and_then(|max_age| now.checked_sub(max_age));

    let mut to_delete = Vec::new();
    let mut survivors = Vec::with_capacity(entries.len());

    // Pass 1: age pruning.
    for entry in entries {
        if let Some(cutoff) = age_cutoff
            && entry.modified < cutoff
        {
            to_delete.push(entry.path);
        } else {
            survivors.push(entry);
        }
    }

    // Pass 2: count pruning over the age survivors.
    if retention.max_count > 0 {
        let excess = survivors.len().saturating_sub(retention.max_count);
        for entry in survivors.into_iter().take(excess) {
            to_delete.push(entry.path);
        }
    }

    to_delete
}

/// Prune the tee directory to the supplied retention limits. Enforced at
/// write time so no background job is needed. Reads the directory, then
/// defers the keep/delete decision to the pure [`plan_retention`] so the
/// policy is unit-testable without touching the filesystem clock.
///
/// Errors are surfaced to the caller, which logs and continues — pruning
/// must never block an edit.
fn enforce_retention(tee_dir: &Path, retention: TeeRetention) -> io::Result<()> {
    if retention.max_count == 0 && retention.max_age.is_none() {
        return Ok(());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(tee_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        entries.push(TeeEntry {
            modified,
            name: entry.file_name(),
            path: entry.path(),
        });
    }

    for path in plan_retention(entries, retention, SystemTime::now()) {
        fs::remove_file(path)?;
    }

    Ok(())
}

fn display_relative(repo_root: &Path, path: &Path) -> String {
    let display_path = path.strip_prefix(repo_root).unwrap_or(path);
    display_path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, modified: SystemTime) -> TeeEntry {
        TeeEntry {
            modified,
            name: name.into(),
            path: PathBuf::from(name),
        }
    }

    fn ago(now: SystemTime, days: u64) -> SystemTime {
        now.checked_sub(Duration::from_secs(days * 86_400)).unwrap()
    }

    fn deleted_names(paths: &[PathBuf]) -> Vec<String> {
        let mut names: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    // ─── Env-value parsing ──────────────────────────────────────────────

    #[test]
    fn max_count_defaults_and_parses() {
        assert_eq!(max_count_from_value(None), TEE_MAX_FILES);
        assert_eq!(max_count_from_value(Some("")), TEE_MAX_FILES);
        assert_eq!(max_count_from_value(Some("   ")), TEE_MAX_FILES);
        assert_eq!(max_count_from_value(Some("50")), 50);
        assert_eq!(max_count_from_value(Some(" 50 ")), 50);
        // 0 disables count pruning (honored, not defaulted).
        assert_eq!(max_count_from_value(Some("0")), 0);
        // Garbage falls back to default.
        assert_eq!(max_count_from_value(Some("nope")), TEE_MAX_FILES);
    }

    #[test]
    fn max_age_defaults_and_parses() {
        let default = Some(Duration::from_secs(TEE_MAX_AGE_DAYS * 86_400));
        assert_eq!(max_age_from_value(None), default);
        assert_eq!(max_age_from_value(Some("")), default);
        assert_eq!(
            max_age_from_value(Some("3")),
            Some(Duration::from_secs(3 * 86_400))
        );
        // 0 disables age pruning.
        assert_eq!(max_age_from_value(Some("0")), None);
        // Garbage falls back to default.
        assert_eq!(max_age_from_value(Some("nope")), default);
    }

    // ─── Pure retention policy ──────────────────────────────────────────

    #[test]
    fn plan_retention_prunes_by_age() {
        let now = SystemTime::now();
        let retention = TeeRetention {
            max_count: 0, // count disabled — isolate age behavior
            max_age: Some(Duration::from_secs(7 * 86_400)),
        };
        let entries = vec![
            entry("old-10d", ago(now, 10)),
            entry("old-8d", ago(now, 8)),
            entry("fresh-1d", ago(now, 1)),
            entry("fresh-now", now),
        ];

        let deleted = plan_retention(entries, retention, now);

        assert_eq!(deleted_names(&deleted), vec!["old-10d", "old-8d"]);
    }

    #[test]
    fn plan_retention_prunes_by_count() {
        let now = SystemTime::now();
        let retention = TeeRetention {
            max_count: 2,
            max_age: None, // age disabled — isolate count behavior
        };
        // Distinct mtimes so ordering is deterministic; all recent.
        let entries = vec![
            entry("a-oldest", ago(now, 4)),
            entry("b", ago(now, 3)),
            entry("c", ago(now, 2)),
            entry("d-newest", ago(now, 1)),
        ];

        let deleted = plan_retention(entries, retention, now);

        // Keep newest 2 (c, d); delete oldest 2 (a, b).
        assert_eq!(deleted_names(&deleted), vec!["a-oldest", "b"]);
    }

    #[test]
    fn plan_retention_applies_age_then_count() {
        let now = SystemTime::now();
        let retention = TeeRetention {
            max_count: 1,
            max_age: Some(Duration::from_secs(7 * 86_400)),
        };
        let entries = vec![
            entry("old-9d", ago(now, 9)),    // age-pruned
            entry("recent-3d", ago(now, 3)), // survives age, count-pruned
            entry("recent-1d", ago(now, 1)), // survives both
        ];

        let deleted = plan_retention(entries, retention, now);

        // old-9d dropped by age; of the 2 survivors keep newest 1 (recent-1d),
        // so recent-3d is also deleted.
        assert_eq!(deleted_names(&deleted), vec!["old-9d", "recent-3d"]);
    }

    #[test]
    fn plan_retention_disabled_when_both_dimensions_off() {
        let now = SystemTime::now();
        let retention = TeeRetention {
            max_count: 0,
            max_age: None,
        };
        let entries = vec![
            entry("ancient", ago(now, 365)),
            entry("a", now),
            entry("b", now),
        ];

        let deleted = plan_retention(entries, retention, now);

        assert!(
            deleted.is_empty(),
            "no pruning when both dimensions disabled, got {deleted:?}"
        );
    }

    #[test]
    fn plan_retention_count_zero_keeps_all_recent() {
        let now = SystemTime::now();
        let retention = TeeRetention {
            max_count: 0,
            max_age: Some(Duration::from_secs(7 * 86_400)),
        };
        let entries = vec![
            entry("a", now),
            entry("b", ago(now, 2)),
            entry("c", ago(now, 6)),
        ];

        let deleted = plan_retention(entries, retention, now);

        assert!(
            deleted.is_empty(),
            "count=0 must not prune recent files, got {deleted:?}"
        );
    }
}
