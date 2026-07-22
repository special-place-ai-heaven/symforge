use crate::domain::{AccessErrorKind, GitignoreHygiene};
use ignore::gitignore::GitignoreBuilder;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const CANONICAL_RULE: &[u8] = b"/.symforge/";
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
static GITIGNORE_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitignoreHygieneAuthority {
    ObserveOnly,
    ExplicitNormalBinding,
    ProjectAwareInit,
    ExplicitProtected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitignoreHygieneReport {
    pub status: GitignoreHygiene,
    pub changed: bool,
}

impl GitignoreHygieneReport {
    fn new(status: GitignoreHygiene, changed: bool) -> Self {
        Self { status, changed }
    }

    pub fn receipt(&self) -> String {
        let status = match &self.status {
            GitignoreHygiene::Effective => "effective",
            GitignoreHygiene::MissingRule => "missing_rule",
            GitignoreHygiene::NoRootGitignore => "no_root_gitignore",
            GitignoreHygiene::Unverifiable { .. } => "unverifiable",
            GitignoreHygiene::NotApplicableExplicitProtected => "not_applicable_explicit_protected",
        };
        format!("gitignore_hygiene={status} changed={}", self.changed)
    }
}

pub fn find_repository_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|candidate| candidate.join(".git").exists())
        .and_then(|candidate| candidate.canonicalize().ok())
}

/// Reconcile the root `.gitignore` for the repository/worktree containing
/// `project_root`. A non-repository directory is always a no-op: hygiene must
/// never promote an arbitrary ancestor or create a new `.gitignore`.
pub fn reconcile_project_gitignore(
    project_root: &Path,
    authority: GitignoreHygieneAuthority,
) -> GitignoreHygieneReport {
    if authority == GitignoreHygieneAuthority::ExplicitProtected {
        return reconcile_root_gitignore(project_root, authority);
    }
    let Some(repository_root) = find_repository_root(project_root) else {
        return GitignoreHygieneReport::new(GitignoreHygiene::NoRootGitignore, false);
    };
    reconcile_root_gitignore(&repository_root, authority)
}

pub fn reconcile_root_gitignore(
    repository_root: &Path,
    authority: GitignoreHygieneAuthority,
) -> GitignoreHygieneReport {
    reconcile_root_gitignore_with_before_commit(repository_root, authority, |_| {})
}

fn reconcile_root_gitignore_with_before_commit<F>(
    repository_root: &Path,
    authority: GitignoreHygieneAuthority,
    before_commit: F,
) -> GitignoreHygieneReport
where
    F: FnOnce(&Path),
{
    if authority == GitignoreHygieneAuthority::ExplicitProtected {
        return GitignoreHygieneReport::new(
            GitignoreHygiene::NotApplicableExplicitProtected,
            false,
        );
    }

    let _write_guard = match GITIGNORE_WRITE_LOCK.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return GitignoreHygieneReport::new(
                GitignoreHygiene::Unverifiable {
                    safe_reason: AccessErrorKind::Other,
                },
                false,
            );
        }
    };
    let path = repository_root.join(".gitignore");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return GitignoreHygieneReport::new(GitignoreHygiene::NoRootGitignore, false);
        }
        Err(error) => return unverifiable(error_kind(&error)),
    };
    if is_unsafe_gitignore_entry(&metadata) {
        return unverifiable(AccessErrorKind::InvalidData);
    }
    let original = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => return unverifiable(error_kind(&error)),
    };
    match root_rule_is_effective(repository_root, &path, &original) {
        Ok(true) => {
            return GitignoreHygieneReport::new(GitignoreHygiene::Effective, false);
        }
        Ok(false) => {}
        Err(reason) => return unverifiable(reason),
    }
    if authority == GitignoreHygieneAuthority::ObserveOnly {
        return GitignoreHygieneReport::new(GitignoreHygiene::MissingRule, false);
    }

    let updated = append_canonical_rule(&original);
    before_commit(&path);
    let current_metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => return unverifiable(error_kind(&error)),
    };
    if is_unsafe_gitignore_entry(&current_metadata) {
        return unverifiable(AccessErrorKind::InvalidData);
    }
    match fs::read(&path) {
        Ok(current) if current == original => {}
        Ok(_) => return unverifiable(AccessErrorKind::Other),
        Err(error) => return unverifiable(error_kind(&error)),
    }
    match atomic_replace(&path, &updated, &metadata) {
        Ok(()) => GitignoreHygieneReport::new(GitignoreHygiene::Effective, true),
        Err(error) => unverifiable(error_kind(&error)),
    }
}

fn root_rule_is_effective(
    repository_root: &Path,
    gitignore_path: &Path,
    bytes: &[u8],
) -> Result<bool, AccessErrorKind> {
    let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let text = std::str::from_utf8(body).map_err(|_| AccessErrorKind::InvalidData)?;
    let mut builder = GitignoreBuilder::new(repository_root);
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        builder
            .add_line(Some(gitignore_path.to_path_buf()), line)
            .map_err(|_| AccessErrorKind::InvalidData)?;
    }
    let matcher = builder.build().map_err(|_| AccessErrorKind::InvalidData)?;
    Ok(matcher
        .matched_path_or_any_parents(".symforge", true)
        .is_ignore())
}

fn append_canonical_rule(original: &[u8]) -> Vec<u8> {
    let body = original.strip_prefix(UTF8_BOM).unwrap_or(original);
    let mut updated = Vec::with_capacity(original.len() + CANONICAL_RULE.len() + 4);
    updated.extend_from_slice(original);
    if body.is_empty() {
        updated.extend_from_slice(CANONICAL_RULE);
        return updated;
    }

    let newline = first_newline(body);
    let ended_with_newline = body.ends_with(b"\n");
    if !ended_with_newline {
        updated.extend_from_slice(newline);
    }
    updated.extend_from_slice(CANONICAL_RULE);
    if ended_with_newline {
        updated.extend_from_slice(newline);
    }
    updated
}

fn first_newline(bytes: &[u8]) -> &'static [u8] {
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            return if index > 0 && bytes[index - 1] == b'\r' {
                b"\r\n"
            } else {
                b"\n"
            };
        }
    }
    b"\n"
}

fn atomic_replace(path: &Path, content: &[u8], metadata: &fs::Metadata) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "gitignore has no parent")
    })?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary
        .as_file()
        .set_permissions(metadata.permissions())?;
    temporary.write_all(content)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn is_unsafe_gitignore_entry(metadata: &fs::Metadata) -> bool {
    gitignore_entry_type_is_unsafe(
        metadata.file_type().is_file(),
        metadata.file_type().is_symlink(),
        crate::paths::metadata_is_reparse_point(metadata),
    )
}

fn gitignore_entry_type_is_unsafe(is_file: bool, is_symlink: bool, is_reparse: bool) -> bool {
    !is_file || is_symlink || is_reparse
}

fn error_kind(error: &std::io::Error) -> AccessErrorKind {
    match error.kind() {
        std::io::ErrorKind::NotFound => AccessErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => AccessErrorKind::PermissionDenied,
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::InvalidInput => {
            AccessErrorKind::InvalidData
        }
        std::io::ErrorKind::OutOfMemory | std::io::ErrorKind::StorageFull => {
            AccessErrorKind::ResourceExhausted
        }
        _ => AccessErrorKind::Other,
    }
}

fn unverifiable(safe_reason: AccessErrorKind) -> GitignoreHygieneReport {
    GitignoreHygieneReport::new(GitignoreHygiene::Unverifiable { safe_reason }, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccessErrorKind, GitignoreHygiene};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    const RULE: &[u8] = b"/.symforge/";

    fn repository() -> TempDir {
        let repository = TempDir::new().unwrap();
        fs::create_dir(repository.path().join(".git")).unwrap();
        repository
    }

    fn assert_rewrite(authority: GitignoreHygieneAuthority, original: &[u8], expected: &[u8]) {
        let repository = repository();
        let gitignore = repository.path().join(".gitignore");
        fs::write(&gitignore, original).unwrap();

        let first = reconcile_root_gitignore(repository.path(), authority);
        assert_eq!(first.status, GitignoreHygiene::Effective);
        assert!(first.changed);
        assert_eq!(fs::read(&gitignore).unwrap(), expected);

        let second = reconcile_root_gitignore(repository.path(), authority);
        assert_eq!(second.status, GitignoreHygiene::Effective);
        assert!(!second.changed);
        assert_eq!(fs::read(&gitignore).unwrap(), expected);
    }

    #[test]
    fn existing_gitignore_append_preserves_bytes_and_line_endings_matrix() {
        let cases: &[(&[u8], &[u8])] = &[
            (b"", RULE),
            (b"\xEF\xBB\xBF", b"\xEF\xBB\xBF/.symforge/"),
            (b"target/\n", b"target/\n/.symforge/\n"),
            (b"target/", b"target/\n/.symforge/"),
            (b"target/\r\n", b"target/\r\n/.symforge/\r\n"),
            (b"target/\r\nother/", b"target/\r\nother/\r\n/.symforge/"),
            (
                b"first/\r\nsecond/\n",
                b"first/\r\nsecond/\n/.symforge/\r\n",
            ),
        ];
        for authority in [
            GitignoreHygieneAuthority::ExplicitNormalBinding,
            GitignoreHygieneAuthority::ProjectAwareInit,
        ] {
            for (original, expected) in cases {
                assert_rewrite(authority, original, expected);
            }
        }
    }

    #[test]
    fn absent_equivalent_ordered_negation_and_external_exclude_matrix() {
        let absent = repository();
        let absent_report = reconcile_root_gitignore(
            absent.path(),
            GitignoreHygieneAuthority::ExplicitNormalBinding,
        );
        assert_eq!(absent_report.status, GitignoreHygiene::NoRootGitignore);
        assert!(!absent_report.changed);
        assert!(!absent.path().join(".gitignore").exists());

        for effective in [b"/.symforge/\n".as_slice(), b"/.symforge\n".as_slice()] {
            let repository = repository();
            let gitignore = repository.path().join(".gitignore");
            fs::write(&gitignore, effective).unwrap();
            let report = reconcile_root_gitignore(
                repository.path(),
                GitignoreHygieneAuthority::ExplicitNormalBinding,
            );
            assert_eq!(report.status, GitignoreHygiene::Effective);
            assert!(!report.changed);
            assert_eq!(fs::read(gitignore).unwrap(), effective);
        }

        assert_rewrite(
            GitignoreHygieneAuthority::ExplicitNormalBinding,
            b"/.symforge/\n!/.symforge/\n",
            b"/.symforge/\n!/.symforge/\n/.symforge/\n",
        );

        let effective_order = repository();
        let effective_order_path = effective_order.path().join(".gitignore");
        let effective_order_bytes = b"!/.symforge/\n/.symforge/\n";
        fs::write(&effective_order_path, effective_order_bytes).unwrap();
        let report = reconcile_root_gitignore(
            effective_order.path(),
            GitignoreHygieneAuthority::ExplicitNormalBinding,
        );
        assert_eq!(report.status, GitignoreHygiene::Effective);
        assert!(!report.changed);
        assert_eq!(
            fs::read(effective_order_path).unwrap(),
            effective_order_bytes
        );

        let external_only = repository();
        fs::create_dir_all(external_only.path().join(".git/info")).unwrap();
        fs::write(
            external_only.path().join(".git/info/exclude"),
            b"/.symforge/\n",
        )
        .unwrap();
        fs::write(
            external_only.path().join("global-excludes"),
            b"/.symforge/\n",
        )
        .unwrap();
        let root_ignore = external_only.path().join(".gitignore");
        fs::write(&root_ignore, b"target/\n").unwrap();
        let report = reconcile_root_gitignore(
            external_only.path(),
            GitignoreHygieneAuthority::ProjectAwareInit,
        );
        assert_eq!(report.status, GitignoreHygiene::Effective);
        assert!(report.changed);
        assert_eq!(fs::read(root_ignore).unwrap(), b"target/\n/.symforge/\n");
    }

    #[test]
    fn automatic_and_explicit_protected_authority_are_read_only() {
        let repository = repository();
        let gitignore = repository.path().join(".gitignore");
        let original = b"target/\n";
        fs::write(&gitignore, original).unwrap();

        let observed =
            reconcile_root_gitignore(repository.path(), GitignoreHygieneAuthority::ObserveOnly);
        assert_eq!(observed.status, GitignoreHygiene::MissingRule);
        assert!(!observed.changed);
        assert_eq!(fs::read(&gitignore).unwrap(), original);

        let protected = reconcile_root_gitignore(
            repository.path(),
            GitignoreHygieneAuthority::ExplicitProtected,
        );
        assert_eq!(
            protected.status,
            GitignoreHygiene::NotApplicableExplicitProtected
        );
        assert!(!protected.changed);
        assert_eq!(fs::read(gitignore).unwrap(), original);
    }

    #[test]
    fn concurrent_hash_change_is_reported_without_clobbering() {
        let repository = repository();
        let gitignore = repository.path().join(".gitignore");
        fs::write(&gitignore, b"target/\n").unwrap();

        let report = reconcile_root_gitignore_with_before_commit(
            repository.path(),
            GitignoreHygieneAuthority::ExplicitNormalBinding,
            |path| fs::write(path, b"concurrent/\n").unwrap(),
        );
        assert_eq!(
            report.status,
            GitignoreHygiene::Unverifiable {
                safe_reason: AccessErrorKind::Other,
            }
        );
        assert!(!report.changed);
        assert_eq!(fs::read(gitignore).unwrap(), b"concurrent/\n");
    }

    #[cfg(unix)]
    fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(target, link)
    }

    #[test]
    fn symlink_or_reparse_gitignore_is_refused_without_following() {
        let repository = repository();
        let target_dir = TempDir::new().unwrap();
        let target = target_dir.path().join("outside-ignore");
        fs::write(&target, b"outside/\n").unwrap();
        let link = repository.path().join(".gitignore");
        if let Err(error) = symlink_file(&target, &link) {
            #[cfg(windows)]
            {
                assert_eq!(error.raw_os_error(), Some(1314));
                assert!(gitignore_entry_type_is_unsafe(true, false, true));
                assert_eq!(fs::read(target).unwrap(), b"outside/\n");
                return;
            }
            #[cfg(not(windows))]
            panic!("test host must support file symlinks: {error}");
        }

        let report = reconcile_root_gitignore(
            repository.path(),
            GitignoreHygieneAuthority::ExplicitNormalBinding,
        );
        assert_eq!(
            report.status,
            GitignoreHygiene::Unverifiable {
                safe_reason: AccessErrorKind::InvalidData,
            }
        );
        assert!(!report.changed);
        assert_eq!(fs::read(target).unwrap(), b"outside/\n");
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    }
}
