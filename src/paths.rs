use std::io;
use std::path::{Path, PathBuf};

pub const SYMFORGE_DIR_NAME: &str = ".symforge";
pub const SYMFORGE_FRECENCY_DB_PATH: &str = ".symforge/frecency.db";
pub const SYMFORGE_COUPLING_DB_PATH: &str = ".symforge/coupling.db";
pub const SYMFORGE_ANALYTICS_DB_PATH: &str = ".symforge/analytics.db";
pub const SYMFORGE_IDEMPOTENCY_DIR_PATH: &str = ".symforge/idempotency";
pub const SYMFORGE_IDEMPOTENCY_RECORDS_DIR_PATH: &str = ".symforge/idempotency/records";
pub const SYMFORGE_IDEMPOTENCY_QUARANTINE_DIR_PATH: &str = ".symforge/idempotency/quarantine";
pub const SYMFORGE_INDEX_SNAPSHOT_QUARANTINE_DIR_PATH: &str =
    ".symforge/quarantine/index-snapshots";

/// OS isolation tag for per-process runtime files (sidecar/daemon port/pid/session).
///
/// This is a pure compile-time constant baked into the binary from its build
/// target (`std::env::consts::OS`): `"windows"`, `"linux"`, `"macos"`, etc. It is
/// NOT a runtime probe, so any two binaries built for the same OS — notably the
/// sidecar/daemon writer and the `symforge hook` reader, which are the SAME crate
/// — always compute the IDENTICAL tag and therefore always agree on filenames.
///
/// Rationale: a Windows symforge and a WSL/Linux symforge can share one physical
/// project-local `.symforge/` directory (a project on a Windows drive opened from
/// both `C:\proj` and `/mnt/c/proj`). Each writes a port that is only valid in its
/// own loopback namespace. Tagging the runtime filenames by OS guarantees neither
/// side ever reads the other's port file. WSL2 reports `"linux"`, which is correct:
/// two Linux processes sharing a dir share the same namespace semantics, so no
/// WSL-vs-native discriminator is needed (adding a `/proc` sniff would make the tag
/// a runtime probe that the Windows side could not reproduce — defeating agreement).
#[must_use]
pub fn os_runtime_tag() -> &'static str {
    std::env::consts::OS
}

/// Build an OS-tagged runtime filename: `sidecar_runtime_file_name("sidecar", "port")`
/// yields e.g. `"sidecar.linux.port"`. The extension is preserved so docs/tools that
/// key on `.port`/`.pid`/`.session` continue to match, and the file stays a sibling
/// in the same `.symforge/` directory.
#[must_use]
pub fn os_tagged_runtime_file_name(stem: &str, ext: &str) -> String {
    format!("{stem}.{tag}.{ext}", tag = os_runtime_tag())
}

/// Resolve the canonical symforge data directory under `base`.
pub fn resolve_symforge_dir(base: &Path) -> PathBuf {
    base.join(SYMFORGE_DIR_NAME)
}

/// Ensure the canonical symforge data directory exists under `base`.
pub fn ensure_symforge_dir(base: &Path) -> io::Result<PathBuf> {
    let dir = resolve_symforge_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("ensuring symforge data dir at {}: {}", dir.display(), e),
        )
    })?;
    Ok(dir)
}

/// Resolve the canonical idempotency replay directory under `base`.
pub fn resolve_idempotency_dir(base: &Path) -> PathBuf {
    base.join(SYMFORGE_IDEMPOTENCY_DIR_PATH)
}

/// Ensure the canonical idempotency replay directory exists under `base`.
pub fn ensure_idempotency_dir(base: &Path) -> io::Result<PathBuf> {
    let dir = resolve_idempotency_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("ensuring idempotency dir at {}: {}", dir.display(), e),
        )
    })?;
    Ok(dir)
}

/// Resolve the canonical index-snapshot quarantine directory under `base`.
pub fn resolve_index_snapshot_quarantine_dir(base: &Path) -> PathBuf {
    base.join(SYMFORGE_INDEX_SNAPSHOT_QUARANTINE_DIR_PATH)
}

/// Ensure the canonical index-snapshot quarantine directory exists under `base`.
pub fn ensure_index_snapshot_quarantine_dir(base: &Path) -> io::Result<PathBuf> {
    let dir = resolve_index_snapshot_quarantine_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "ensuring index snapshot quarantine dir at {}: {}",
                dir.display(),
                e
            ),
        )
    })?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_resolve_symforge_dir_prefers_existing_canonical_dir() {
        let tmp = TempDir::new().unwrap();
        let symforge_dir = tmp.path().join(SYMFORGE_DIR_NAME);
        std::fs::create_dir_all(&symforge_dir).unwrap();

        let resolved = resolve_symforge_dir(tmp.path());

        assert_eq!(resolved, symforge_dir);
    }

    #[test]
    fn test_ensure_symforge_dir_creates_canonical_dir_when_missing() {
        let tmp = TempDir::new().unwrap();

        let dir = ensure_symforge_dir(tmp.path()).unwrap();

        assert_eq!(dir, tmp.path().join(SYMFORGE_DIR_NAME));
        assert!(dir.exists(), "canonical directory should be created");
    }

    #[test]
    fn test_analytics_db_path_stays_under_canonical_symforge_dir() {
        let tmp = TempDir::new().unwrap();

        assert_eq!(
            tmp.path().join(SYMFORGE_ANALYTICS_DB_PATH),
            tmp.path().join(SYMFORGE_DIR_NAME).join("analytics.db")
        );
    }

    #[test]
    fn test_idempotency_paths_stay_under_canonical_symforge_dir() {
        let tmp = TempDir::new().unwrap();

        assert_eq!(
            resolve_idempotency_dir(tmp.path()),
            tmp.path().join(SYMFORGE_DIR_NAME).join("idempotency")
        );
        assert_eq!(
            tmp.path().join(SYMFORGE_IDEMPOTENCY_RECORDS_DIR_PATH),
            resolve_idempotency_dir(tmp.path()).join("records")
        );
        assert_eq!(
            tmp.path().join(SYMFORGE_IDEMPOTENCY_QUARANTINE_DIR_PATH),
            resolve_idempotency_dir(tmp.path()).join("quarantine")
        );
    }

    #[test]
    fn test_index_snapshot_quarantine_path_stays_under_canonical_symforge_dir() {
        let tmp = TempDir::new().unwrap();

        assert_eq!(
            resolve_index_snapshot_quarantine_dir(tmp.path()),
            tmp.path()
                .join(SYMFORGE_DIR_NAME)
                .join("quarantine")
                .join("index-snapshots")
        );
    }
}
