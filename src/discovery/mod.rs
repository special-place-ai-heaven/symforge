use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::{FileClassification, LanguageId};

/// A file found during directory traversal that has a recognized language extension.
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    /// Relative path from the root, using forward slashes (e.g., "src/lib.rs").
    pub relative_path: String,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Language inferred from the file extension.
    pub language: LanguageId,
    /// Deterministic semantic-lane classification captured at discovery time.
    pub classification: FileClassification,
}

/// A file found during a full-filesystem walk (all files, not just known-language ones).
///
/// Used by the admission gate to classify every file — including those with unknown or
/// denylisted extensions — before deciding whether to parse them.
#[derive(Debug, Clone)]
pub struct DiscoveredEntry {
    /// Relative path from the root, using forward slashes.
    pub relative_path: String,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// File size in bytes from the walk metadata (no extra stat syscall).
    pub file_size: u64,
    /// Language inferred from the extension, if recognized.
    pub language: Option<LanguageId>,
    /// Semantic-lane classification (test/vendor/generated/config flags).
    pub classification: FileClassification,
}

/// Discover all source files under `root` that have a recognized language extension.
///
/// - Respects `.gitignore` files via the `ignore` crate.
/// - Normalizes path separators to `/` in `relative_path`.
/// - Returns files sorted case-insensitively by `relative_path`.
pub fn discover_files(root: &Path) -> Result<Vec<DiscoveredFile>> {
    use ignore::WalkBuilder;

    // Canonicalize root so that strip_prefix succeeds even when the walker
    // resolves symlinks to their canonical targets.
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    let mut files: Vec<DiscoveredFile> = WalkBuilder::new(&root)
        .build()
        .filter_map(|entry_result| {
            let entry = entry_result.ok()?;
            let path =
                std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());

            // Use the already-known file_type from the walker instead of
            // path.is_file() which would issue a redundant stat() syscall.
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                return None;
            }

            let ext = path.extension()?.to_str()?;
            let language = LanguageId::from_extension(ext)?;

            // Compute relative path from root
            let relative = path.strip_prefix(&root).ok()?;
            // Normalize backslashes to forward slashes
            let relative_path = relative.to_string_lossy().replace('\\', "/");

            Some(DiscoveredFile {
                classification: FileClassification::for_code_path(&relative_path),
                relative_path,
                absolute_path: path,
                language,
            })
        })
        .collect();

    // Cache sort keys once instead of lowercasing paths on every comparator call.
    files.sort_by_cached_key(|file| file.relative_path.to_lowercase());

    Ok(files)
}

/// Discover ALL files under `root` regardless of extension, for admission-gate classification.
///
/// Unlike `discover_files`, this function:
/// - Yields every file (not just known-language ones), so denylisted/binary files are visible.
/// - Captures file size from walk metadata (avoids a separate stat() call).
/// - Sets `language = None` for files with unrecognized extensions.
/// - Returns files sorted case-insensitively by `relative_path`.
pub fn discover_all_files(root: &Path) -> Result<Vec<DiscoveredEntry>> {
    use ignore::WalkBuilder;

    // Canonicalize root so that strip_prefix succeeds even when the walker
    // resolves symlinks to their canonical targets.
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    let mut entries: Vec<DiscoveredEntry> = WalkBuilder::new(&root)
        .build()
        .filter_map(|entry_result| {
            let entry = entry_result.ok()?;
            let path =
                std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());

            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                return None;
            }

            // Get file size from the walk metadata (DirEntry has it on most platforms).
            // Fall back to a stat call only when metadata is unavailable.
            let file_size = entry
                .metadata()
                .ok()
                .map(|m| m.len())
                .unwrap_or_else(|| std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));

            // Compute relative path from root
            let relative = path.strip_prefix(&root).ok()?;
            let relative_path = relative.to_string_lossy().replace('\\', "/");

            // Attempt language detection; None for unknown/denylisted extensions.
            let language = path
                .extension()
                .and_then(|e| e.to_str())
                .and_then(LanguageId::from_extension);

            let classification = FileClassification::for_code_path(&relative_path);

            Some(DiscoveredEntry {
                relative_path,
                absolute_path: path,
                file_size,
                language,
                classification,
            })
        })
        .collect();

    // Cache sort keys once instead of lowercasing paths on every comparator call.
    entries.sort_by_cached_key(|entry| entry.relative_path.to_lowercase());

    Ok(entries)
}

/// Load all `.gitignore` patterns from a repository root and nested directories.
///
/// Uses `ignore::gitignore::GitignoreBuilder` to build a composite gitignore matcher.
/// Walks nested `.gitignore` files up to `max_depth` levels (default 6).
/// Returns `None` if no `.gitignore` files are found or if loading fails.
pub fn load_gitignore(root: &Path) -> Option<ignore::gitignore::Gitignore> {
    use ignore::gitignore::GitignoreBuilder;
    use std::collections::VecDeque;

    let root_gitignore = root.join(".gitignore");
    if !root_gitignore.exists() {
        return None;
    }

    let mut builder = GitignoreBuilder::new(root);

    // BFS to find nested .gitignore files (max depth 6)
    let max_depth: usize = 6;
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    while let Some((dir, depth)) = queue.pop_front() {
        let gitignore_path = dir.join(".gitignore");
        if gitignore_path.is_file()
            && let Some(err) = builder.add(&gitignore_path)
        {
            tracing::debug!("failed to load {:?}: {}", gitignore_path, err);
        }

        if depth < max_depth
            && let Ok(entries) = std::fs::read_dir(&dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip common directories that won't have relevant .gitignore files
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with('.') && name_str != ".github" {
                        continue;
                    }
                    queue.push_back((path, depth + 1));
                }
            }
        }
    }

    match builder.build() {
        Ok(gi) => {
            // Only return Some if there are actual patterns
            if gi.is_empty() { None } else { Some(gi) }
        }
        Err(e) => {
            tracing::debug!("failed to build gitignore matcher: {}", e);
            None
        }
    }
}

/// Walk upward from the current working directory, looking for a `.git` directory.
/// Returns `None` if no git root is found and the cwd is a forbidden directory.
pub fn find_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Try to find a git root first (always safe — scoped by repo boundary).
    let mut current = cwd.clone();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    // No git root found — use cwd if it's not a forbidden directory.
    if is_forbidden_root(&cwd) {
        tracing::warn!(
            path = %cwd.display(),
            "refusing to auto-index: directory is too broad (home dir, drive root, or system path)"
        );
        None
    } else {
        Some(cwd)
    }
}

/// Returns `true` if `path` is a directory that should never be auto-indexed
/// because it would be too large or contain unrelated files.
fn is_forbidden_root(path: &Path) -> bool {
    // Canonicalize for reliable comparison (resolves symlinks, normalizes separators).
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // 1. Drive roots: C:\, D:\, /, etc.
    if path.parent().is_none() {
        return true;
    }

    // 2. Windows drive roots that have a parent but are still just "C:\"
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        if path_str.len() <= 7 && path_str.ends_with('\\') {
            return true;
        }
    }

    // 3. User home directories.
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if path == home {
            return true;
        }
    }

    // 4a. System directory names — always forbidden anywhere.
    //     These are unambiguous: a directory literally named `system32`
    //     or `node_modules` is virtually never a project root.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();
        const SYSTEM_NAMES: &[&str] = &[
            "windows",
            "system32",
            "program files",
            "program files (x86)",
            "programdata",
            "node_modules",
            ".npm",
            ".cargo",
        ];
        if SYSTEM_NAMES.contains(&lower.as_str()) {
            return true;
        }
    }

    // 4b. Top-level container names — forbidden only when sitting directly
    //     under a filesystem root or drive root. A legitimate project named
    //     `tmp` or `var` deeper in the tree is allowed.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();
        const CONTAINER_NAMES: &[&str] = &["users", "home", "tmp", "temp", "var", "appdata"];
        if CONTAINER_NAMES.contains(&lower.as_str())
            && path
                .parent()
                .map(|p| {
                    // Parent is a drive root or filesystem root → forbid.
                    p.parent().is_none() || {
                        #[cfg(target_os = "windows")]
                        {
                            let pstr = p.to_string_lossy();
                            pstr.len() <= 7 && pstr.ends_with('\\')
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            false
                        }
                    }
                })
                .unwrap_or(false)
        {
            return true;
        }
    }

    // 4c. WSL DrvFs Windows-profile / drive-root guard (Linux only).
    //     Under WSL, Windows drives mount at /mnt/<drive> (default automount root)
    //     and the Windows user profile surfaces at /mnt/<drive>/Users/<name>. None
    //     of the rules above catch this: $HOME is the Linux home (/home/<user>), so
    //     the home-based guards never match, and the leaf-name guards never inspect
    //     the intermediate `Users` component. Auto-indexing any of these roots walks
    //     a huge tree over the slow DrvFs/9p mount and hangs the daemon.
    //
    //     We forbid the broad container roots only — NOT deep project dirs — so a
    //     non-git project kept at /mnt/c/Users/<name>/dev/proj stays auto-indexable:
    //       /mnt/<drive>                 (bare Windows drive root)
    //       /mnt/<drive>/Users           (the profile container)
    //       /mnt/<drive>/Users/<name>    (a bare profile root)
    //     A genuine git repo anywhere under these is still indexable because the
    //     `.git` fast-path in `find_project_root` returns before this gate runs.
    //
    //     Gated on an actual WSL probe so a real Linux host that merely mounts a
    //     volume at /mnt/<letter>/Users is not falsely forbidden. The `Users`
    //     segment is matched case-insensitively because DrvFs is case-insensitive
    //     but path canonicalization is case-preserving — `cd /mnt/c/users/...`
    //     reaches the identical Windows tree and must be caught too.
    #[cfg(not(target_os = "windows"))]
    {
        if is_running_under_wsl() && is_wsl_windows_container_path(&path) {
            return true;
        }
    }

    // 5. Parent-of-home: e.g. C:\Users or /home
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if let Some(parent) = home.parent() {
            let parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            if path == parent {
                return true;
            }
        }
    }

    false
}

/// Cross-platform home directory lookup.
fn home_dir() -> Option<PathBuf> {
    // std::env::home_dir is deprecated but dirs::home_dir may not be available.
    // Use environment variables directly for reliability.
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// Returns `true` when running inside the Windows Subsystem for Linux.
///
/// Detected by sniffing `/proc/version` for the `microsoft` / `WSL` marker the
/// WSL kernel writes there. The result is computed once and cached, so the file
/// is read at most one time per process. Always `false` on non-Linux targets.
#[cfg(not(target_os = "windows"))]
fn is_running_under_wsl() -> bool {
    use std::sync::OnceLock;
    static IS_WSL: OnceLock<bool> = OnceLock::new();
    *IS_WSL.get_or_init(|| {
        std::fs::read_to_string("/proc/version")
            .map(|v| {
                let v = v.to_ascii_lowercase();
                v.contains("microsoft") || v.contains("wsl")
            })
            .unwrap_or(false)
    })
}

/// Pure path-shape test for the broad WSL DrvFs container roots that must never
/// be auto-indexed: the bare Windows drive mount and the Windows user-profile
/// container/root surfaced under WSL's default `/mnt/` automount.
///
/// Returns `true` for exactly:
///
/// - `/mnt/<drive>` (bare drive root)
/// - `/mnt/<drive>/Users` (profile container)
/// - `/mnt/<drive>/Users/<name>` (bare profile root)
///
/// where `<drive>` is a single ASCII letter and `Users` matches case-insensitively
/// (DrvFs is case-insensitive but canonicalization is case-preserving). Anything
/// deeper (`/mnt/<drive>/Users/<name>/...`) and any non-`Users` mount path
/// (`/mnt/<drive>/code/proj`) returns `false` and stays indexable.
///
/// Path-shape only — the caller is responsible for confirming the host is WSL.
/// Kept separate from the WSL probe so it is host-independent and unit-testable.
#[cfg(not(target_os = "windows"))]
fn is_wsl_windows_container_path(path: &Path) -> bool {
    // Lexically normalize: drop `.`, pop on `..`. A path that escapes above the
    // root via `..` is treated as non-matching rather than silently collapsing,
    // so the gate never misfires on `..`-bearing input a future caller passes.
    let mut comps: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(os) => {
                if let Some(s) = os.to_str() {
                    comps.push(s);
                }
            }
            // `..` that pops past the root means the path escapes above `/mnt`;
            // treat as non-matching rather than silently collapsing.
            std::path::Component::ParentDir if comps.pop().is_none() => return false,
            std::path::Component::ParentDir => {}
            // RootDir / CurDir / Prefix carry no addressable segment.
            _ => {}
        }
    }

    if comps.first() != Some(&"mnt") {
        return false;
    }

    let is_drive_letter =
        |s: &str| s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic());

    let is_users = |s: &str| s.eq_ignore_ascii_case("Users");

    match comps.as_slice() {
        // /mnt/<drive> — bare Windows drive root.
        [_mnt, drive] => is_drive_letter(drive),
        // /mnt/<drive>/Users — the profile container.
        [_mnt, drive, users] => is_drive_letter(drive) && is_users(users),
        // /mnt/<drive>/Users/<name> — a bare profile root (exactly 4 segments).
        [_mnt, drive, users, _name] => is_drive_letter(drive) && is_users(users),
        // Bare /mnt, or deeper than a bare profile root
        // (/mnt/<drive>/Users/<name>/...), stays indexable.
        _ => false,
    }
}

/// Check if content appears to be binary.
/// Examines up to BINARY_SNIFF_BYTES of the content using three heuristics:
/// 1. NUL byte present -> binary
/// 2. UTF-8 decode failure -> binary
/// 3. >30% suspicious control bytes (excluding \t, \n, \r) -> binary
pub fn is_binary_content(content: &[u8]) -> bool {
    if content.is_empty() {
        return false;
    }
    let check_len = content.len().min(crate::domain::index::BINARY_SNIFF_BYTES);
    let window = &content[..check_len];

    // Heuristic 1: NUL byte
    if window.contains(&0) {
        return true;
    }

    // Heuristic 2: Invalid UTF-8
    if std::str::from_utf8(window).is_err() {
        return true;
    }

    // Heuristic 3: High control byte ratio
    // Control bytes: 0x01-0x08, 0x0E-0x1F, 0x7F
    // Excludes common text controls: \t (0x09), \n (0x0A), \r (0x0D)
    let suspicious_controls = window
        .iter()
        .filter(|&&b| matches!(b, 0x01..=0x08 | 0x0E..=0x1F | 0x7F))
        .count();
    let ratio = suspicious_controls as f64 / window.len() as f64;
    if ratio > 0.30 {
        return true;
    }

    false
}

use crate::domain::index::{
    AdmissionDecision, AdmissionTier, HARD_SKIP_BYTES, METADATA_ONLY_BYTES, SkipReason,
};

/// Classify a file's admission tier. Returns AdmissionDecision with both tier and reason.
///
/// Precedence (first match wins):
/// 1. Hard-skip size ceiling (>100MB) → Tier 3
/// 2. Extension denylist → Tier 2
/// 3. Metadata-only size threshold (>1MB) → Tier 2
/// 4. Binary sniff (null bytes in first 8KB) → Tier 2
/// 5. All else → Tier 1
pub fn classify_admission(
    path: &std::path::Path,
    file_size: u64,
    content_sample: Option<&[u8]>,
) -> AdmissionDecision {
    use crate::domain::index::is_denylisted_extension;

    if file_size > HARD_SKIP_BYTES {
        return AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling);
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str())
        && is_denylisted_extension(ext)
    {
        return AdmissionDecision::skip(
            AdmissionTier::MetadataOnly,
            SkipReason::DenylistedExtension,
        );
    }
    if file_size > METADATA_ONLY_BYTES {
        return AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::SizeThreshold);
    }
    if let Some(content) = content_sample
        && is_binary_content(content)
    {
        return AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::BinaryContent);
    }
    AdmissionDecision::normal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_discover_files_finds_rs_py_js() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "main.rs", "fn main() {}");
        create_file(tmp.path(), "script.py", "def foo(): pass");
        create_file(tmp.path(), "app.js", "function bar() {}");

        let files = discover_files(tmp.path()).unwrap();
        let extensions: Vec<&str> = files
            .iter()
            .map(|f| f.relative_path.rsplit('.').next().unwrap())
            .collect();

        assert!(extensions.contains(&"rs"), "should find .rs");
        assert!(extensions.contains(&"py"), "should find .py");
        assert!(extensions.contains(&"js"), "should find .js");
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_discover_files_includes_config_files() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "config.json", "{}");
        create_file(tmp.path(), "README.md", "# readme");
        create_file(tmp.path(), "Cargo.toml", "[package]");
        create_file(tmp.path(), "main.rs", "fn main() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 4, "should discover .rs + .json + .md + .toml");
        let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"config.json"), "should find .json");
        assert!(paths.contains(&"README.md"), "should find .md");
        assert!(paths.contains(&"Cargo.toml"), "should find .toml");
        assert!(paths.contains(&"main.rs"), "should find .rs");
    }

    #[test]
    fn test_discover_files_respects_gitignore() {
        let tmp = TempDir::new().unwrap();
        // Must create .git dir for gitignore to be respected
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join(".gitignore"), "ignored.rs\n").unwrap();

        create_file(tmp.path(), "main.rs", "fn main() {}");
        create_file(tmp.path(), "ignored.rs", "fn ignored() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "ignored.rs should be excluded");
        assert_eq!(files[0].relative_path, "main.rs");
    }

    #[test]
    fn test_discover_files_normalizes_backslashes() {
        let tmp = TempDir::new().unwrap();
        // Create a file in a subdirectory — the path separator will be OS-native
        create_file(tmp.path(), "src/lib.rs", "pub fn lib() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        // Must use forward slashes regardless of OS
        assert!(
            !files[0].relative_path.contains('\\'),
            "should have no backslashes: {:?}",
            files[0].relative_path
        );
        assert!(files[0].relative_path.contains('/') || files[0].relative_path == "src/lib.rs");
    }

    #[test]
    fn test_discover_files_deterministic_sorted_order() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "Zoo.rs", "fn zoo() {}");
        create_file(tmp.path(), "apple.rs", "fn apple() {}");
        create_file(tmp.path(), "Mango.rs", "fn mango() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 3);
        // Case-insensitive alphabetical order
        let names: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        // "apple" < "Mango" < "Zoo" case-insensitively
        let lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let mut sorted = lower.clone();
        sorted.sort();
        assert_eq!(
            lower, sorted,
            "files should be in case-insensitive sorted order"
        );
    }

    #[test]
    fn test_discover_files_assigns_classification_tags_from_path() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "tests/unit_spec.rs", "fn spec_case() {}");
        create_file(tmp.path(), "vendor/pkg/lib.rs", "fn vendored() {}");
        create_file(
            tmp.path(),
            "src/generated/client.generated.rs",
            "fn generated() {}",
        );

        let files = discover_files(tmp.path()).unwrap();
        let by_path: std::collections::HashMap<&str, &DiscoveredFile> = files
            .iter()
            .map(|file| (file.relative_path.as_str(), file))
            .collect();

        assert!(
            by_path["tests/unit_spec.rs"].classification.is_test,
            "tests path should set is_test"
        );
        assert!(
            by_path["vendor/pkg/lib.rs"].classification.is_vendor,
            "vendor path should set is_vendor"
        );
        assert!(
            by_path["src/generated/client.generated.rs"]
                .classification
                .is_generated,
            "generated path should set is_generated"
        );
    }

    #[test]
    fn test_is_forbidden_root_blocks_home_dir() {
        let home = home_dir();
        if let Some(h) = home {
            assert!(is_forbidden_root(&h), "home directory should be forbidden");
        }
    }

    #[test]
    fn test_is_forbidden_root_blocks_drive_root() {
        #[cfg(target_os = "windows")]
        assert!(is_forbidden_root(Path::new("C:\\")));
        #[cfg(not(target_os = "windows"))]
        assert!(is_forbidden_root(Path::new("/")));
    }

    #[test]
    fn test_is_forbidden_root_blocks_system_dirs() {
        assert!(is_forbidden_root(Path::new("/tmp")));
        assert!(is_forbidden_root(Path::new("/home")));
    }

    #[test]
    fn test_is_forbidden_root_allows_project_dirs() {
        let tmp = TempDir::new().unwrap();
        assert!(
            !is_forbidden_root(tmp.path()),
            "temp project dir should be allowed"
        );
    }

    #[test]
    fn test_is_forbidden_root_allows_project_named_tmp() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("tmp");
        std::fs::create_dir_all(&project).unwrap();
        assert!(
            !is_forbidden_root(&project),
            "project at C:\\projects\\tmp must not be rejected by basename"
        );
    }

    #[test]
    fn test_is_forbidden_root_allows_project_named_var() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("workspace").join("var");
        std::fs::create_dir_all(&project).unwrap();
        assert!(
            !is_forbidden_root(&project),
            "project at workspace/var must not be rejected by basename"
        );
    }

    #[test]
    fn test_is_forbidden_root_still_blocks_top_level_tmp_on_unix() {
        // Skip on Windows where /tmp doesn't apply
        #[cfg(unix)]
        {
            // /tmp itself is a real path; canonicalize will succeed.
            let path = std::path::Path::new("/tmp");
            if path.exists() {
                assert!(
                    is_forbidden_root(path),
                    "/tmp must still be blocked as system path"
                );
            }
        }
    }

    #[test]
    fn test_is_forbidden_root_still_blocks_windows_system_paths() {
        #[cfg(target_os = "windows")]
        {
            let path = std::path::Path::new(r"C:\Windows\System32");
            if path.exists() {
                assert!(
                    is_forbidden_root(path),
                    "C:\\Windows\\System32 must remain blocked"
                );
            }
        }
    }

    #[test]
    fn test_binary_sniff_detects_null_bytes() {
        let content = b"hello\x00world";
        assert!(is_binary_content(content));
    }

    #[test]
    fn test_binary_sniff_allows_pure_utf8() {
        let content = b"fn main() { println!(\"hello\"); }";
        assert!(!is_binary_content(content));
    }

    #[test]
    fn test_binary_sniff_empty_file() {
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn test_binary_sniff_detects_invalid_utf8() {
        let content: &[u8] = &[0x80, 0x81, 0x82, 0x83, 0x84];
        assert!(is_binary_content(content));
    }

    #[test]
    fn test_binary_sniff_detects_high_control_ratio() {
        let mut content = Vec::new();
        content.extend(std::iter::repeat_n(0x01, 80)); // SOH — control char
        content.extend(std::iter::repeat_n(b'A', 20)); // printable
        // 80% control bytes > 30% threshold -> binary
        assert!(is_binary_content(&content));
    }

    #[test]
    fn test_binary_sniff_allows_low_control_ratio() {
        let content = b"line1\tvalue1\nline2\tvalue2\nline3\tvalue3\n";
        assert!(!is_binary_content(content));
    }

    #[test]
    fn test_binary_sniff_allows_common_whitespace_controls() {
        let content = b"col1\tcol2\tcol3\r\nval1\tval2\tval3\r\n";
        assert!(!is_binary_content(content));
    }

    // ── classify_admission tests ──

    use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason};

    #[test]
    fn test_huge_text_file_is_hard_skip() {
        let decision =
            classify_admission(std::path::Path::new("huge.txt"), 150 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::HardSkip);
        assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));
    }

    #[test]
    fn test_small_ckpt_is_metadata_only() {
        let decision = classify_admission(std::path::Path::new("model.ckpt"), 50 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::DenylistedExtension));
    }

    #[test]
    fn test_huge_ckpt_is_hard_skip() {
        let decision = classify_admission(std::path::Path::new("big.ckpt"), 4_200_000_000, None);
        assert_eq!(decision.tier, AdmissionTier::HardSkip);
        assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));
    }

    #[test]
    fn test_large_json_is_metadata_only() {
        let decision = classify_admission(std::path::Path::new("big.json"), 2 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
    }

    #[test]
    fn test_small_txt_is_normal() {
        let decision = classify_admission(std::path::Path::new("readme.txt"), 50 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_medium_rust_source_is_normal() {
        let decision = classify_admission(std::path::Path::new("big_module.rs"), 500 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_binary_content_is_metadata_only() {
        let content = b"ELF\x00\x00\x00binary";
        let decision =
            classify_admission(std::path::Path::new("unknown_file"), 1024, Some(content));
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::BinaryContent));
    }

    #[test]
    fn test_svg_not_denylisted() {
        let decision = classify_admission(std::path::Path::new("icon.svg"), 50 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_large_svg_is_metadata_only_by_size() {
        let decision = classify_admission(std::path::Path::new("huge.svg"), 2 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
    }

    // ── discover_all_files + admission gate integration tests ──

    #[test]
    fn test_discovery_skips_denylisted_extension() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "main.rs", "fn main() {}");
        // Write a fake .safetensors file (extension is on the denylist)
        fs::write(tmp.path().join("model.safetensors"), b"fake model bytes").unwrap();

        let entries = discover_all_files(tmp.path()).unwrap();

        // Classify each entry and collect skipped ones
        let mut rs_found = false;
        let mut safetensors_skipped = false;
        let mut safetensors_reason = None;

        for entry in &entries {
            let size = entry.file_size;
            let decision = classify_admission(&entry.absolute_path, size, None);
            if entry.relative_path == "main.rs" {
                assert_eq!(
                    decision.tier,
                    AdmissionTier::Normal,
                    ".rs file should be Normal"
                );
                rs_found = true;
            }
            if entry.relative_path == "model.safetensors" {
                assert_eq!(
                    decision.tier,
                    AdmissionTier::MetadataOnly,
                    ".safetensors should be MetadataOnly"
                );
                safetensors_skipped = true;
                safetensors_reason = decision.reason;
            }
        }

        assert!(rs_found, ".rs file must appear in discovered entries");
        assert!(
            safetensors_skipped,
            ".safetensors must appear in discovered entries and be skipped"
        );
        assert_eq!(
            safetensors_reason,
            Some(SkipReason::DenylistedExtension),
            ".safetensors skip reason must be DenylistedExtension"
        );
    }

    #[test]
    fn test_discovery_deferred_binary_sniff_reclassifies() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "lib.rs", "pub fn hello() {}");

        // Write a .dat file with NUL-heavy content: not on denylist, under 1MB,
        // but binary sniff (NUL bytes) should reclassify to MetadataOnly.
        let mut binary_content = vec![0u8; 512]; // NUL bytes — triggers binary sniff
        binary_content.extend_from_slice(b"some trailing text");
        fs::write(tmp.path().join("custom.dat"), &binary_content).unwrap();

        let entries = discover_all_files(tmp.path()).unwrap();

        let mut rs_normal = false;
        let mut dat_skipped = false;
        let mut dat_reason = None;

        for entry in &entries {
            let size = entry.file_size;

            // Phase 1: pre-content check
            let pre = classify_admission(&entry.absolute_path, size, None);

            if entry.relative_path == "lib.rs" {
                assert_eq!(pre.tier, AdmissionTier::Normal);
                rs_normal = true;
            }

            if entry.relative_path == "custom.dat" {
                // Pre-content: should be Normal (not denylisted, under 1MB)
                assert_eq!(
                    pre.tier,
                    AdmissionTier::Normal,
                    "custom.dat should be Normal before binary sniff"
                );

                // Phase 2: with content — binary sniff should reclassify
                let content = fs::read(&entry.absolute_path).unwrap();
                let post = classify_admission(&entry.absolute_path, size, Some(&content));
                assert_eq!(
                    post.tier,
                    AdmissionTier::MetadataOnly,
                    "custom.dat should be MetadataOnly after binary sniff"
                );
                dat_skipped = true;
                dat_reason = post.reason;
            }
        }

        assert!(rs_normal, "lib.rs must be Normal");
        assert!(
            dat_skipped,
            "custom.dat must be discovered and reclassified"
        );
        assert_eq!(
            dat_reason,
            Some(SkipReason::BinaryContent),
            "custom.dat skip reason must be BinaryContent"
        );
    }

    // WSL DrvFs Windows-profile / drive-root guard (rule 4c).
    //
    // These exercise the pure path-shape helper `is_wsl_windows_container_path`,
    // which is independent of the WSL probe and therefore deterministic on any
    // non-Windows host (CI, macOS, native Linux). The helper only exists on
    // non-Windows targets, so the whole group is gated to match.
    #[cfg(not(target_os = "windows"))]
    mod wsl_drvfs {
        use super::*;

        // --- forbidden: the broad container roots that caused the hang ---

        #[test]
        fn blocks_bare_drive_root() {
            assert!(is_wsl_windows_container_path(Path::new("/mnt/c")));
            assert!(is_wsl_windows_container_path(Path::new("/mnt/d")));
        }

        #[test]
        fn blocks_users_container() {
            assert!(is_wsl_windows_container_path(Path::new("/mnt/c/Users")));
        }

        #[test]
        fn blocks_bare_profile_root() {
            // The exact reported hang path.
            assert!(is_wsl_windows_container_path(Path::new(
                "/mnt/c/Users/poslj"
            )));
        }

        #[test]
        fn blocks_other_drive_profile() {
            assert!(is_wsl_windows_container_path(Path::new(
                "/mnt/d/Users/alice"
            )));
        }

        #[test]
        fn blocks_case_insensitive_users_segment() {
            // DrvFs is case-insensitive but canonicalize is case-preserving, so
            // `cd /mnt/c/users/...` reaches the identical Windows tree. All
            // casings of the profile container/root must be caught.
            assert!(is_wsl_windows_container_path(Path::new("/mnt/c/users")));
            assert!(is_wsl_windows_container_path(Path::new(
                "/mnt/c/USERS/poslj"
            )));
            assert!(is_wsl_windows_container_path(Path::new(
                "/mnt/c/UsErS/poslj"
            )));
        }

        // --- allowed: deep projects and lookalikes must stay indexable ---

        #[test]
        fn allows_deep_project_under_profile() {
            // A non-git project kept under the profile must NOT be forbidden;
            // the .git fast-path handles real repos, and deep dirs are scoped.
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/c/Users/poslj/dev/my-lib"
            )));
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/c/Users/poslj/Documents/project"
            )));
        }

        #[test]
        fn allows_non_users_mount_project() {
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/c/code/proj"
            )));
        }

        #[test]
        fn allows_users_named_deeper() {
            // A dir literally named Users but NOT at the /mnt/<drive>/Users
            // position must stay allowed (guards against over-broad matching).
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/c/code/Users"
            )));
        }

        #[test]
        fn allows_non_mnt_paths() {
            // Genuine Linux paths with a Users dir are not under /mnt.
            assert!(!is_wsl_windows_container_path(Path::new("/srv/Users/bob")));
            assert!(!is_wsl_windows_container_path(Path::new("/home/robert")));
        }

        #[test]
        fn allows_multichar_second_segment() {
            // comps[1] must be a single ASCII letter; multi-char (a real mount
            // name, not a drive) is allowed.
            assert!(!is_wsl_windows_container_path(Path::new("/mnt/cc/Users/x")));
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/wsl/Users/x"
            )));
        }

        #[test]
        fn allows_lookalike_prefixes() {
            // Substring/prefix lookalikes must not collide with `Users`.
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/c/Users-data/proj"
            )));
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/c/UserStuff/proj"
            )));
        }

        #[test]
        fn allows_bare_mnt() {
            assert!(!is_wsl_windows_container_path(Path::new("/mnt")));
        }

        #[test]
        fn parent_dir_escape_does_not_misfire() {
            // `..` is popped lexically rather than dropped, so a path whose real
            // target is a non-Users dir is not falsely forbidden.
            assert!(!is_wsl_windows_container_path(Path::new(
                "/mnt/c/Users/../code/proj"
            )));
            // `..` popping past the root yields no match (not a panic / false true).
            assert!(!is_wsl_windows_container_path(Path::new("/mnt/c/../..")));
        }
    }
}
