use std::io;
use std::path::{Path, PathBuf};

pub const SYMFORGE_DIR_NAME: &str = ".symforge";
pub const SYMFORGE_FRECENCY_DB_PATH: &str = ".symforge/frecency.db";
pub const SYMFORGE_COUPLING_DB_PATH: &str = ".symforge/coupling.db";
pub const SYMFORGE_ANALYTICS_DB_PATH: &str = ".symforge/analytics.db";

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
}
