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

/// Environment variable overriding the maximum number of files a single
/// discovery pass will accept before refusing to index the tree.
const MAX_INDEX_FILES_ENV: &str = "SYMFORGE_MAX_INDEX_FILES";
/// Environment variable overriding the maximum cumulative byte size a single
/// discovery pass will accept before refusing to index the tree.
const MAX_INDEX_BYTES_ENV: &str = "SYMFORGE_MAX_INDEX_BYTES";

/// Default file-count ceiling. Generous enough for very large real monorepos
/// (this repo is ~230 files; 50k+ file monorepos are common), while still well
/// below the point where building the in-memory index maps/strings would
/// exhaust memory or trip `String join would overflow memory bounds`.
const DEFAULT_MAX_INDEX_FILES: u64 = 200_000;
/// Default cumulative-bytes ceiling: 16 GiB of accepted file content. A tree
/// whose discoverable files exceed this is almost certainly a generated-file
/// bomb, a mounted volume, or an accidental scratch root, not a project.
const DEFAULT_MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Resource ceilings applied DURING the filesystem walk, before any in-memory
/// index build commits to the discovered set. Bounding the streaming walk (not
/// the post-collection `Vec`) is what keeps a huge but non-sensitive tree from
/// OOM-ing or panicking the reload: we stop and return a graceful error the
/// moment either ceiling is crossed, instead of collecting megabytes of paths
/// and then letting `LiveIndex::load` blow the memory bound.
#[derive(Debug, Clone, Copy)]
pub struct DiscoveryLimits {
    /// Maximum number of files accepted before refusing the tree.
    pub max_files: u64,
    /// Maximum cumulative bytes of accepted files before refusing the tree.
    pub max_bytes: u64,
}

impl DiscoveryLimits {
    /// Resolve limits from the environment, falling back to the generous
    /// defaults. A non-parseable or empty override is ignored (the default is
    /// used) so a typo can never silently *lower* the ceiling to zero and brick
    /// indexing — only an explicit, well-formed value takes effect.
    pub fn from_env() -> Self {
        let max_files = parse_positive_env(MAX_INDEX_FILES_ENV).unwrap_or(DEFAULT_MAX_INDEX_FILES);
        let max_bytes = parse_positive_env(MAX_INDEX_BYTES_ENV).unwrap_or(DEFAULT_MAX_INDEX_BYTES);
        Self {
            max_files,
            max_bytes,
        }
    }
}

impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            max_files: DEFAULT_MAX_INDEX_FILES,
            max_bytes: DEFAULT_MAX_INDEX_BYTES,
        }
    }
}

/// Parse a strictly-positive `u64` from the named env var, or `None` if the var
/// is unset, empty, non-numeric, or zero. Zero is rejected so an override can
/// never disable indexing entirely; callers fall back to the default instead.
fn parse_positive_env(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|&value| value > 0)
}

/// Environment variable Cargo honors to relocate the build directory. When set
/// to a path whose final component is a direct child of the repo root, that
/// child is a build dir and must be skipped regardless of `.gitignore`.
const CARGO_TARGET_DIR_ENV: &str = "CARGO_TARGET_DIR";

/// Returns `true` when `name` is a Cargo build-directory name: exactly `target`
/// or `target-<suffix>` where `<suffix>` is one or more ASCII alphanumerics or
/// underscores (e.g. `target`, `target-wsl`, `target-x86_64`). This matches the
/// regex `^target(-[A-Za-z0-9_]+)?$` without a per-call regex compile.
///
/// Used to hard-skip build dirs at the REPO ROOT independently of each user's
/// `.gitignore`. `/target` is conventionally gitignored, but a `CARGO_TARGET_DIR`
/// variant like `target-wsl` (common on dual Windows/WSL machines) usually is
/// not, so it would otherwise be indexed as source.
fn is_cargo_build_dir_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("target") else {
        return false;
    };
    match rest.strip_prefix('-') {
        // Bare `target`.
        None if rest.is_empty() => true,
        // `target` followed by something other than `-<suffix>` (e.g. `targets`,
        // `target_dir`) is NOT a build dir.
        None => false,
        // `target-<suffix>`: suffix must be non-empty and [A-Za-z0-9_]+.
        Some(suffix) => {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }
    }
}

/// The repo-root child directory name designated by `CARGO_TARGET_DIR`, if that
/// env var is set AND resolves to a direct child of `root`. Returns `None` when
/// the var is unset, empty, or points somewhere that is not a single-segment
/// child of the root (an absolute path outside the tree never produces
/// discoverable entries, so it needs no skip entry here).
///
/// Comparison is done on canonicalized paths so a relative or symlinked
/// `CARGO_TARGET_DIR` still matches the root child the walk actually traverses.
fn cargo_target_dir_root_child(root: &Path) -> Option<String> {
    let raw = std::env::var_os(CARGO_TARGET_DIR_ENV)?;
    if raw.is_empty() {
        return None;
    }
    let target = PathBuf::from(&raw);
    // Resolve against the root for relative values (Cargo interprets a relative
    // CARGO_TARGET_DIR against the working directory; for discovery we only care
    // about the case where it lands directly under the indexed root).
    let target_abs = if target.is_absolute() {
        target
    } else {
        root.join(&target)
    };
    let canon_target = std::fs::canonicalize(&target_abs).unwrap_or(target_abs);
    let parent = canon_target.parent()?;
    if parent != root {
        return None;
    }
    canon_target
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// Returns `true` when `relative_path` (forward-slash normalized, relative to the
/// repo root) lives under a repo-root-level Cargo build directory and must be
/// skipped. Only the FIRST path component is inspected, so a legitimately-named
/// nested source dir such as `src/target/mod.rs` is never over-skipped — only a
/// `target*` (or `CARGO_TARGET_DIR`) directory that is a direct child of the root.
fn is_under_repo_root_build_dir(relative_path: &str, target_dir_child: Option<&str>) -> bool {
    let Some(first) = relative_path.split('/').next() else {
        return false;
    };
    // A path with no separator is a root-level FILE, not a build dir; only treat
    // it as build output when it is the first segment of a deeper path.
    if first == relative_path {
        return false;
    }
    if is_cargo_build_dir_name(first) {
        return true;
    }
    matches!(target_dir_child, Some(child) if child == first)
}

/// SF-025: returns `true` when any component of `relative_path` (forward-slash
/// normalized, relative to the repo root) is a "hidden" dotfile/dotdir — a
/// segment beginning with `.` other than the `.`/`..` traversal segments.
///
/// The bulk discovery walk (`discover_all_files`) uses `ignore::WalkBuilder`
/// with its default `.hidden(true)`, which skips any entry whose name — or an
/// ancestor directory's name — starts with `.` (e.g. `.github/workflows/ci.yml`,
/// `.travis.yml`). The single-file (re)index choke point in the watcher did NOT
/// apply that rule, so a freshen-on-read of a tracked hidden file would parse
/// and INSERT it even though a fresh bulk load never discovered it. That made
/// index membership query-history-dependent: a file was invisible to
/// `search_files` until someone happened to `get_file_context` it, and identical
/// health calls disagreed across processes. This predicate lets the single-file
/// path apply the SAME hidden-path exclusion as the walk, keeping admission
/// symmetric and the index deterministic from scan policy alone.
pub fn path_has_hidden_component(relative_path: &str) -> bool {
    relative_path
        .split('/')
        .any(|component| component.starts_with('.') && component != "." && component != "..")
}

/// Build the graceful, explicit over-cap error. Surfaced to the caller (and
/// thus the MCP client) instead of an OOM/panic, and it names the override knob
/// so an operator with a genuinely huge repo can raise the ceiling.
fn tree_too_large_error(files: u64, bytes: u64, limits: &DiscoveryLimits) -> anyhow::Error {
    anyhow::anyhow!(
        "tree too large to index ({files} files / {bytes} bytes exceeds limit of \
         {max_files} files / {max_bytes} bytes); set {MAX_INDEX_FILES_ENV} or \
         {MAX_INDEX_BYTES_ENV} to override",
        max_files = limits.max_files,
        max_bytes = limits.max_bytes,
    )
}

/// Discover all source files under `root` that have a recognized language extension.
///
/// - Respects `.gitignore` files via the `ignore` crate.
/// - Normalizes path separators to `/` in `relative_path`.
/// - Returns files sorted case-insensitively by `relative_path`.
/// - Refuses trees that exceed [`DiscoveryLimits`] with a graceful error rather
///   than collecting an unbounded set and OOM-ing the in-memory index build.
pub fn discover_files(root: &Path) -> Result<Vec<DiscoveredFile>> {
    use ignore::WalkBuilder;

    // Canonicalize root so that strip_prefix succeeds even when the walker
    // resolves symlinks to their canonical targets.
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    // Bound the walk by accepted file count. This pass only tracks files with a
    // recognized language, so byte ceilings are enforced by `discover_all_files`
    // (the full-load entry point that has file sizes from the walk metadata).
    let limits = DiscoveryLimits::from_env();
    // Repo-root build-dir child designated by CARGO_TARGET_DIR, resolved once.
    let target_dir_child = cargo_target_dir_root_child(&root);
    let mut files: Vec<DiscoveredFile> = Vec::new();
    for entry_result in WalkBuilder::new(&root).build() {
        let Ok(entry) = entry_result else { continue };
        let path =
            std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());

        // Use the already-known file_type from the walker instead of
        // path.is_file() which would issue a redundant stat() syscall.
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(language) = LanguageId::from_extension(ext) else {
            continue;
        };

        // Compute relative path from root
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        // Normalize backslashes to forward slashes
        let relative_path = relative.to_string_lossy().replace('\\', "/");

        // Repo-independent skip for Cargo build dirs at the REPO ROOT level
        // (`target`, `target-wsl`, `CARGO_TARGET_DIR`, …). `/target` is usually
        // gitignored, but variant build dirs often are not, so do not rely on
        // each user's `.gitignore`. Nested source dirs like `src/target/` are
        // unaffected because only the first path component is inspected.
        if is_under_repo_root_build_dir(&relative_path, target_dir_child.as_deref()) {
            continue;
        }

        // Refuse BEFORE growing the set past the ceiling, so a huge tree returns
        // a graceful error rather than collecting an unbounded path vector.
        if files.len() as u64 >= limits.max_files {
            return Err(tree_too_large_error(files.len() as u64 + 1, 0, &limits));
        }

        files.push(DiscoveredFile {
            classification: FileClassification::for_code_path(&relative_path),
            relative_path,
            absolute_path: path,
            language,
        });
    }

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
/// - Refuses trees that exceed [`DiscoveryLimits`] (file count AND cumulative
///   bytes) with a graceful error rather than collecting an unbounded set and
///   OOM-ing / panicking the in-memory index build in `LiveIndex::load`.
///
/// This is the discovery entry point used by the full `LiveIndex::load`, so the
/// byte ceiling is enforced here (file sizes are already known from the walk
/// metadata, so no extra `stat` is needed to track cumulative bytes).
pub fn discover_all_files(root: &Path) -> Result<Vec<DiscoveredEntry>> {
    use ignore::WalkBuilder;

    // Canonicalize root so that strip_prefix succeeds even when the walker
    // resolves symlinks to their canonical targets.
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    // Bound the streaming walk by accepted file count AND cumulative bytes,
    // refusing the moment either ceiling is crossed — before the unbounded
    // path/byte set is handed to the in-memory index build.
    let limits = DiscoveryLimits::from_env();
    // Repo-root build-dir child designated by CARGO_TARGET_DIR, resolved once.
    let target_dir_child = cargo_target_dir_root_child(&root);
    let mut total_bytes: u64 = 0;
    let mut entries: Vec<DiscoveredEntry> = Vec::new();
    // SF-012(B): the repo-root build-dir heuristic (`is_under_repo_root_build_dir`)
    // matches `target-<alnum>` by design (e.g. `target-wsl`, a `CARGO_TARGET_DIR`
    // variant), but false-positives on legitimately tracked source dirs whose name
    // happens to match — tokio's `target-specs/` (tracked `.md`/`.json`, not
    // gitignored). Build output is NEVER git-tracked, so git-tracked status is a
    // decisive counter-signal: if a path the heuristic would skip is tracked, keep
    // it. The tracked set is computed LAZILY (only on the first build-dir hit) so
    // the common case — no root-level `target-*` dir — pays nothing. `None` means
    // "no git / unreadable index" (fail open: heuristic decides alone, as before).
    let mut tracked_for_build_dirs: Option<Option<std::collections::HashSet<String>>> = None;
    for entry_result in WalkBuilder::new(&root).build() {
        let Ok(entry) = entry_result else { continue };
        let path =
            std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        // Get file size from the walk metadata (DirEntry has it on most platforms).
        // Fall back to a stat call only when metadata is unavailable.
        let file_size = entry
            .metadata()
            .ok()
            .map(|m| m.len())
            .unwrap_or_else(|| std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));

        // Compute relative path from root
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        let relative_path = relative.to_string_lossy().replace('\\', "/");

        // Repo-independent skip for Cargo build dirs at the REPO ROOT level
        // (`target`, `target-wsl`, `CARGO_TARGET_DIR`, …), independent of each
        // user's `.gitignore`. Skipping before the size/byte accounting keeps
        // build output from counting against the discovery ceilings. Nested
        // source dirs like `src/target/` are unaffected (first component only).
        if is_under_repo_root_build_dir(&relative_path, target_dir_child.as_deref()) {
            // SF-012(B): rescue genuine source. Only build output reaches the size
            // ceilings, so the heuristic's intent is to drop build artifacts — but
            // a tracked `target-*` source dir (tokio `target-specs/`) is not build
            // output. Consult the git-tracked set (computed once, lazily); a
            // tracked path overrides the heuristic and is admitted normally. When
            // git is unavailable the set is `None` and the heuristic decides alone.
            let tracked = tracked_for_build_dirs
                .get_or_insert_with(|| tracked_path_set_for_build_dir_rescue(&root));
            let rescued = tracked
                .as_ref()
                .is_some_and(|set| set.contains(relative_path.as_str()));
            if !rescued {
                continue;
            }
        }

        // Attempt language detection; None for unknown/denylisted extensions.
        let language = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(LanguageId::from_extension);

        let classification = FileClassification::for_code_path(&relative_path);

        // Refuse BEFORE pushing past either ceiling. `saturating_add` keeps the
        // byte counter from wrapping on a pathological tree; once it crosses the
        // limit we return the graceful error instead of continuing to allocate.
        let projected_files = entries.len() as u64 + 1;
        let projected_bytes = total_bytes.saturating_add(file_size);
        if projected_files > limits.max_files || projected_bytes > limits.max_bytes {
            return Err(tree_too_large_error(
                projected_files,
                projected_bytes,
                &limits,
            ));
        }
        total_bytes = projected_bytes;

        entries.push(DiscoveredEntry {
            relative_path,
            absolute_path: path,
            file_size,
            language,
            classification,
        });
    }

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

/// Environment override for the project root used by cold-start discovery.
///
/// Some launchers cannot give the server a useful working directory: Claude
/// Desktop on Windows launches MCP servers with CWD = `C:\WINDOWS\System32`
/// (forbidden), so the wrapper historically `cd`'d to `%USERPROFILE%` — also
/// forbidden — leaving `find_project_root` with no discoverable root and binding
/// an empty index (TR-03). `symforge init` discovers the operator's workspace at
/// install time and writes it into the registered MCP `env` under this key, so
/// cold start indexes the real workspace instead of the home directory.
pub const WORKSPACE_ROOT_ENV: &str = "SYMFORGE_WORKSPACE_ROOT";

/// Walk upward from the current working directory, looking for a `.git` directory.
/// Returns `None` if no git root is found and the cwd is a forbidden directory.
///
/// A non-empty `SYMFORGE_WORKSPACE_ROOT` env var takes priority over CWD-based
/// discovery (TR-03): it is the workspace `symforge init` resolved at install
/// time, threaded through to a launcher whose CWD is otherwise useless. It is
/// still validated through the SAME `is_forbidden_root` guard as CWD discovery,
/// so the override can never widen the trust boundary — a missing, non-directory,
/// or sensitive/broad path is ignored and discovery falls back to CWD.
pub fn find_project_root() -> Option<PathBuf> {
    if let Some(root) = workspace_root_env_override() {
        return Some(root);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Try to find a git root first (scoped by repo boundary), BUT run the same
    // sensitive/forbidden guard on the discovered `.git` root that the cwd
    // fallback below uses. A `.git` planted at a sensitive ancestor (e.g.
    // `git init` in `C:\Users\<name>` or a malicious `/etc/.git`) must NOT be
    // selected and indexed unguarded: if the `.git`-bearing ancestor is
    // forbidden we skip it and keep walking up, exactly as the rest of the
    // guard does, so a deeper legitimate `.git` is still found and a genuine
    // project `.git` continues to be selected.
    let mut current = cwd.clone();
    loop {
        if current.join(".git").exists() && !is_forbidden_root(&current) {
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

/// Resolve and validate the `SYMFORGE_WORKSPACE_ROOT` cold-start override.
///
/// Returns `Some(root)` only when the env var is set to a non-empty path that
/// exists, is a directory, and passes the SAME `is_forbidden_root` guard used by
/// CWD-based discovery — so the override can never index a sensitive or overly
/// broad tree. Any failure logs and returns `None`, letting `find_project_root`
/// fall back to its normal CWD walk (the override is a hint, never a bypass).
///
/// Public so the per-connection retarget gate
/// (`SymForgeServer::bind_workspace_from_client_roots`, feature 012 D4-A) can ask
/// "did the bound root come from the env override?" — when it did, `env > roots`
/// precedence requires the env decision to win and client-roots retarget is
/// skipped; when it did not, the bound root came from the CWD walk and declared
/// client roots are allowed to retarget the session (`roots > CWD`).
pub fn workspace_root_env_override() -> Option<PathBuf> {
    let raw = std::env::var(WORKSPACE_ROOT_ENV).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    validate_workspace_candidate(Path::new(trimmed), WORKSPACE_ROOT_ENV)
}

/// Validate a workspace-root candidate through the SAME guard chain used by
/// `SYMFORGE_WORKSPACE_ROOT` and CWD discovery: it must be an existing directory
/// that passes [`is_forbidden_root`]. Returns `Some(path)` only when both hold;
/// any failure logs (tagged with `source` for diagnosis) and returns `None`.
///
/// This is the single shared gate so that no workspace-resolution path — env
/// override, MCP client roots, or CWD walk — can ever widen the trust boundary.
fn validate_workspace_candidate(candidate: &Path, source: &str) -> Option<PathBuf> {
    if !candidate.is_dir() {
        tracing::warn!(
            path = %candidate.display(),
            "ignoring {source}: not an existing directory"
        );
        return None;
    }
    if is_forbidden_root(candidate) {
        tracing::warn!(
            path = %candidate.display(),
            "ignoring {source}: directory is too broad (home dir, drive root, or system path)"
        );
        return None;
    }
    Some(candidate.to_path_buf())
}

/// Decode `%XX` percent-escapes in a URI path segment back to raw bytes, then
/// interpret the result as UTF-8. Returns `None` only when an escape is
/// malformed; un-escaped input passes through unchanged. Kept dependency-free
/// so it compiles in the engine-only `embed` build (where `url`/`reqwest` are
/// absent).
pub(crate) fn percent_decode_path(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = bytes.get(i + 1).copied()?;
            let lo = bytes.get(i + 2).copied()?;
            let decode = |b: u8| -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            };
            out.push(decode(hi)? << 4 | decode(lo)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Convert an MCP `roots/list` URI into a filesystem path.
///
/// MCP roots arrive as `file://` URIs (per spec) but lenient clients may send a
/// bare path. Returns `None` for empty input or a non-`file` scheme (e.g. an
/// `http://` root we cannot index locally). Parsing is dependency-free so it
/// compiles in the engine-only `embed` build (no `url`/`reqwest`): the `file://`
/// authority and a leading slash before a Windows drive letter
/// (`file:///C:/proj`) are stripped, and `%XX` escapes are decoded. A raw
/// (non-URI) path is accepted verbatim for lenient clients.
pub fn parse_root_uri(uri: &str) -> Option<PathBuf> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Non-`file` scheme (http/https/...) is not a local path. Detect a generic
    // `scheme://` prefix; only `file` proceeds.
    if let Some(scheme_end) = trimmed.find("://") {
        let scheme = &trimmed[..scheme_end];
        if !scheme.eq_ignore_ascii_case("file") {
            // A scheme that is not `file` cannot be a local workspace root.
            // Reject rather than guess.
            if scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            {
                return None;
            }
        }
    }

    // Strip a `file://<authority>` prefix if present. The authority is empty for
    // the common `file:///path` form; a non-empty authority (`file://host/path`)
    // is treated as a UNC-style or remote host we cannot index, so it is dropped
    // to the path component only when the authority is `localhost`/empty.
    let rest = if let Some(after) = trimmed.strip_prefix("file://") {
        // `after` is `<authority><path>`; the path begins at the first `/`.
        match after.find('/') {
            Some(idx) => {
                let authority = &after[..idx];
                if authority.is_empty() || authority.eq_ignore_ascii_case("localhost") {
                    &after[idx..]
                } else {
                    // Remote/UNC host — not a local workspace root.
                    return None;
                }
            }
            // `file://something` with no path — nothing usable.
            None => return None,
        }
    } else {
        trimmed
    };

    let decoded = percent_decode_path(rest)?;

    // Windows drive form: `/C:/proj` -> `C:/proj`. A leading slash before a
    // `<letter>:` drive is the URI artifact, not part of the path.
    let cleaned = {
        let bytes = decoded.as_bytes();
        if bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
        {
            decoded[1..].to_string()
        } else {
            decoded
        }
    };

    Some(PathBuf::from(cleaned))
}

/// Resolve the workspace root from the three resolution sources in strict
/// precedence order, independent of process global state so it is unit-testable:
///
/// 1. `env_root` — the validated [`WORKSPACE_ROOT_ENV`] override (explicit operator intent).
/// 2. `root_uris` — MCP client-declared roots, in client order (the open workspace folder).
/// 3. `cwd_root` — the launch-CWD walk result from [`find_project_root`] (last resort).
///
/// Each MCP root URI is parsed via [`parse_root_uri`] and validated through the
/// SAME [`validate_workspace_candidate`] guard as the env override, so a client
/// cannot push a forbidden root (home dir, drive root, system path) past the
/// trust boundary. The first source that yields a usable directory wins; a
/// forbidden or unparseable client root is skipped, not fatal.
///
/// `env_root` and `cwd_root` are passed pre-resolved (the caller owns reading
/// `WORKSPACE_ROOT_ENV` and walking the CWD) so this function performs no I/O
/// beyond validating the candidate directories, keeping the precedence logic
/// pure and testable with a temp-dir fixture.
pub fn resolve_workspace_root(
    env_root: Option<PathBuf>,
    root_uris: &[String],
    cwd_root: Option<PathBuf>,
) -> Option<PathBuf> {
    // 1. Explicit env override wins outright (already validated by its caller).
    if let Some(root) = env_root {
        return Some(root);
    }

    // 2. MCP client roots, in order; first valid directory wins.
    for uri in root_uris {
        let Some(candidate) = parse_root_uri(uri) else {
            continue;
        };
        if let Some(root) = validate_workspace_candidate(&candidate, "MCP client root") {
            return Some(root);
        }
    }

    // 3. Launch-CWD walk (already validated by find_project_root).
    cwd_root
}

/// Returns `true` if `path` is a directory that should never be auto-indexed
/// because it would be too large or contain unrelated files.
fn is_forbidden_root(path: &Path) -> bool {
    // Canonicalize for reliable comparison (resolves symlinks, normalizes separators).
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // 0. Unified trust-boundary guard. `paths::is_sensitive_path` is the SINGLE
    //    canonical guard shared with the attacker-facing index tools
    //    (`tools::index_folder`, `daemon::index_folder_for_session`,
    //    `daemon::open_project_session`). Delegating here makes the trusted
    //    launcher AT LEAST as strict as the tool surface, so the two can never
    //    drift apart again — the drift that caused the original daemon bypass.
    //    The launcher-specific rules below (running-user `$HOME`, WSL probe)
    //    remain as additional, narrower checks on top of this shared floor.
    if crate::paths::is_sensitive_path(&path) {
        return true;
    }

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
/// 2. Dependency lockfile (exact basename) → Tier 2
/// 3. Extension denylist → Tier 2
/// 4. Metadata-only size threshold (>1MB) → Tier 2
/// 5. Binary sniff (null bytes in first 8KB) → Tier 2
/// 6. All else → Tier 1
pub fn classify_admission(
    path: &std::path::Path,
    file_size: u64,
    content_sample: Option<&[u8]>,
) -> AdmissionDecision {
    use crate::domain::index::{is_denylisted_extension, is_dependency_lockfile};

    if file_size > HARD_SKIP_BYTES {
        return AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling);
    }
    // Dependency lockfiles are machine-generated manifests: their resolved
    // dependency trees parse into thousands of meaningless key/value symbols that
    // pollute symbol counts and `conventions` complexity stats. Demote to Tier-2
    // metadata-only (path stays searchable; no symbol extraction). Checked before
    // the size threshold so a >1MB lockfile still reports `lockfile`, not `>1MB`.
    if is_dependency_lockfile(path) {
        return AdmissionDecision::skip(
            AdmissionTier::MetadataOnly,
            SkipReason::DependencyLockfile,
        );
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str())
        && is_denylisted_extension(ext)
    {
        return AdmissionDecision::skip(
            AdmissionTier::MetadataOnly,
            SkipReason::DenylistedExtension,
        );
    }
    // Language-aware threshold (dogfood #1/#7, 2026-07-06): code languages get
    // METADATA_ONLY_CODE_BYTES (4MB) before demotion — >1MB first-party source
    // is load-bearing in real repos and tree-sitter parses it in milliseconds.
    // Data/markup formats keep the 1MB threshold (symbol-pollution guard).
    let size_threshold = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(crate::domain::LanguageId::from_extension)
        .filter(crate::domain::LanguageId::is_code_language)
        .map_or(METADATA_ONLY_BYTES, |_| {
            crate::domain::index::METADATA_ONLY_CODE_BYTES
        });
    if file_size > size_threshold {
        return AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::SizeThreshold);
    }
    if let Some(content) = content_sample
        && is_binary_content(content)
    {
        return AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::BinaryContent);
    }
    AdmissionDecision::normal()
}

/// SF-004 / SF-012: reconcile a `classify_admission` result for a file whose
/// extension maps to no supported tree-sitter grammar.
///
/// `classify_admission` only inspects size / denylist / binary content — it has
/// no concept of language recognition, so a small, non-binary, non-denylisted
/// file with an unknown extension (`.tcl`, `.sh`, `.m`, `.eex`, extensionless
/// `LICENSE`/`Makefile`, …) comes back `AdmissionTier::Normal`. But the parser
/// cannot extract symbols from it, so storing a `Normal` decision is
/// self-contradictory: such records were silently dropped by `tier_counts`
/// (the `Normal => {}` arm) and minted a false "File not found" in
/// `get_file_context`.
///
/// This helper is the single place that maps that "Normal but unparseable"
/// state onto an honest `Tier-2 metadata-only / UnsupportedLanguage` decision.
/// A non-`Normal` decision (real size/denylist/binary skip) is returned
/// unchanged, so this never overrides a more specific reason. Callers invoke it
/// ONLY on the no-recognized-language branch, so a `Normal` input here always
/// means "unparseable language", never "Tier-1 source".
pub fn unsupported_language_decision(decision: AdmissionDecision) -> AdmissionDecision {
    if decision.tier == AdmissionTier::Normal {
        AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::UnsupportedLanguage)
    } else {
        decision
    }
}

/// Env var gating the SF-009 opt-in "exclude untracked" admission policy.
/// Default OFF — when unset (or set to anything other than a truthy value) the
/// index admits files exactly as before, so admission defaults are unchanged.
pub const EXCLUDE_UNTRACKED_ENV: &str = "SYMFORGE_EXCLUDE_UNTRACKED";

/// Returns `true` when the opt-in `SYMFORGE_EXCLUDE_UNTRACKED` policy is enabled.
///
/// Accepts the usual truthy spellings (`1`, `true`, `yes`, `on`,
/// case-insensitive). Anything else — including unset — is treated as OFF, so
/// the default is a strict no-op. This gate is the ONLY thing that can demote a
/// recognized-extension file to Tier-2 on the basis of git-tracking; with it
/// off, the admission gate behaves identically to before SF-009.
pub fn exclude_untracked_enabled() -> bool {
    std::env::var(EXCLUDE_UNTRACKED_ENV)
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Compute the set of git-tracked relative paths (forward-slash normalized) for
/// the repository containing `root`, for the SF-009 exclude-untracked policy.
///
/// **Fails open to `None`** when the policy is disabled, when no git repository
/// is discoverable, or when the git index cannot be read. A `None` result means
/// "do not demote anything" — never "treat every file as untracked". An empty
/// tracked set (readable but empty index) also yields `None` for the same
/// reason, so a freshly `git init`-ed tree does not demote every source file.
///
/// Uses the git index (`git ls-files` semantics) via [`crate::git::GitRepo`],
/// NOT the `ignore` crate — the `ignore` crate models gitignore rules but has no
/// concept of which files are tracked.
pub fn tracked_path_set_for_exclusion(root: &Path) -> Option<std::collections::HashSet<String>> {
    if !exclude_untracked_enabled() {
        return None;
    }
    let git_repo = crate::git::GitRepo::open(root).ok()?;
    let tracked = git_repo.tracked_paths().ok()?;
    if tracked.is_empty() {
        return None;
    }
    Some(tracked.into_iter().collect())
}

/// SF-012(B): git-tracked path set used to RESCUE source files the repo-root
/// build-dir heuristic (`is_under_repo_root_build_dir`) would otherwise skip.
///
/// Unlike [`tracked_path_set_for_exclusion`] this is NOT env-gated: build output
/// is never git-tracked, so a tracked path matching `target-<alnum>` (e.g.
/// tokio's `target-specs/`) is real source, not a build artifact, and must be
/// admitted. **Fails open to `None`** (heuristic decides alone) when no git repo
/// is discoverable, the index cannot be read, or the tracked set is empty — so a
/// non-git tree keeps the conservative build-dir skip exactly as before.
fn tracked_path_set_for_build_dir_rescue(root: &Path) -> Option<std::collections::HashSet<String>> {
    let git_repo = crate::git::GitRepo::open(root).ok()?;
    let tracked = git_repo.tracked_paths().ok()?;
    if tracked.is_empty() {
        return None;
    }
    Some(tracked.into_iter().collect())
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

    // ── repo-root Cargo build-dir skip (X6) ──
    //
    // Build dirs at the repo root (`target`, `target-wsl`, `CARGO_TARGET_DIR`)
    // must be skipped independently of `.gitignore`, while normal source and a
    // legitimately-named non-build dir stay indexed. The skip inspects only the
    // first path component, so a nested `src/target/` source dir is preserved.
    mod cargo_build_dir_skip {
        use super::*;

        #[test]
        fn build_dir_name_matcher_classifies_correctly() {
            // Matches: bare `target` and `target-<alnum/underscore suffix>`.
            assert!(is_cargo_build_dir_name("target"));
            assert!(is_cargo_build_dir_name("target-wsl"));
            assert!(is_cargo_build_dir_name("target-debug"));
            assert!(is_cargo_build_dir_name("target-x86_64"));
            assert!(is_cargo_build_dir_name("target-CI_2"));
            // Non-matches: lookalikes that are legitimate source dir names.
            assert!(!is_cargo_build_dir_name("targets"));
            assert!(!is_cargo_build_dir_name("target_dir"));
            assert!(!is_cargo_build_dir_name("target-"));
            assert!(!is_cargo_build_dir_name("target-foo/bar"));
            assert!(!is_cargo_build_dir_name("my-target"));
            assert!(!is_cargo_build_dir_name("src"));
        }

        #[test]
        fn under_repo_root_build_dir_only_matches_root_child() {
            // Root-level build dirs are skipped.
            assert!(is_under_repo_root_build_dir("target/debug/foo.rs", None));
            assert!(is_under_repo_root_build_dir(
                "target-wsl/release/x.rs",
                None
            ));
            // A nested source dir literally named `target` is NOT skipped.
            assert!(!is_under_repo_root_build_dir("src/target/mod.rs", None));
            // A root-level FILE (no separator) is not a build dir.
            assert!(!is_under_repo_root_build_dir("target", None));
            // Normal source is untouched.
            assert!(!is_under_repo_root_build_dir("src/lib.rs", None));
            // CARGO_TARGET_DIR child is skipped when supplied.
            assert!(is_under_repo_root_build_dir(
                "build-out/app.rs",
                Some("build-out")
            ));
            assert!(!is_under_repo_root_build_dir(
                "src/build-out/app.rs",
                Some("build-out")
            ));
        }

        #[test]
        fn discover_files_skips_target_wsl_keeps_source() {
            let tmp = TempDir::new().unwrap();
            // A build-dir variant that is NOT gitignored here.
            create_file(
                tmp.path(),
                "target-wsl/debug/build_artifact.rs",
                "fn a() {}",
            );
            // Bare `target` build dir.
            create_file(tmp.path(), "target/debug/other.rs", "fn b() {}");
            // Normal source MUST be indexed.
            create_file(tmp.path(), "src/lib.rs", "pub fn lib() {}");
            // A legitimately-named non-build dir must NOT be over-skipped.
            create_file(tmp.path(), "targets/config.rs", "fn cfg() {}");
            // A nested source dir literally named `target` must be preserved.
            create_file(tmp.path(), "src/target/mod.rs", "fn nested() {}");

            let files = discover_files(tmp.path()).unwrap();
            let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();

            assert!(
                !paths.contains(&"target-wsl/debug/build_artifact.rs"),
                "target-wsl/ build output must be skipped: {paths:?}"
            );
            assert!(
                !paths.contains(&"target/debug/other.rs"),
                "target/ build output must be skipped: {paths:?}"
            );
            assert!(
                paths.contains(&"src/lib.rs"),
                "normal source must be indexed: {paths:?}"
            );
            assert!(
                paths.contains(&"targets/config.rs"),
                "non-build dir `targets/` must not be over-skipped: {paths:?}"
            );
            assert!(
                paths.contains(&"src/target/mod.rs"),
                "nested source dir `src/target/` must be preserved: {paths:?}"
            );
        }

        #[test]
        fn discover_all_files_skips_target_wsl_keeps_source() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "target-wsl/debug/artifact.rs", "fn a() {}");
            create_file(tmp.path(), "src/main.rs", "fn main() {}");
            create_file(tmp.path(), "targets/x.rs", "fn x() {}");

            let entries = discover_all_files(tmp.path()).unwrap();
            let paths: Vec<&str> = entries.iter().map(|e| e.relative_path.as_str()).collect();

            assert!(
                !paths.contains(&"target-wsl/debug/artifact.rs"),
                "target-wsl/ build output must be skipped in full discovery: {paths:?}"
            );
            assert!(
                paths.contains(&"src/main.rs"),
                "normal source must be discovered: {paths:?}"
            );
            assert!(
                paths.contains(&"targets/x.rs"),
                "non-build dir `targets/` must not be over-skipped: {paths:?}"
            );
        }
    }

    // ── SF-004 / SF-012(A): unsupported-language admission demotion ──
    //
    // A small, non-binary file with an extension that maps to no supported
    // grammar must be admitted Tier-2 (metadata-only / unsupported-language),
    // NOT stored with a contradictory Tier-1/Normal decision that vanishes from
    // tier accounting and mints a false "File not found".
    mod unsupported_language {
        use super::*;

        #[test]
        fn unsupported_language_decision_demotes_normal_to_metadata_only() {
            // classify_admission returns Normal for a small non-binary file (it
            // never inspects language); the helper must demote it honestly.
            let normal = AdmissionDecision::normal();
            let demoted = unsupported_language_decision(normal);
            assert_eq!(demoted.tier, AdmissionTier::MetadataOnly);
            assert_eq!(demoted.reason, Some(SkipReason::UnsupportedLanguage));
        }

        #[test]
        fn unsupported_language_decision_preserves_specific_skip_reasons() {
            // A real size/denylist/binary skip must pass through unchanged — the
            // helper only rewrites the contradictory Normal-but-unparseable state.
            for original in [
                AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling),
                AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::SizeThreshold),
                AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::BinaryContent),
                AdmissionDecision::skip(
                    AdmissionTier::MetadataOnly,
                    SkipReason::DependencyLockfile,
                ),
            ] {
                assert_eq!(
                    unsupported_language_decision(original),
                    original,
                    "non-Normal decision must be returned unchanged"
                );
            }
        }

        #[test]
        fn unsupported_language_reason_renders_honestly() {
            assert_eq!(
                SkipReason::UnsupportedLanguage.to_string(),
                "unsupported language"
            );
        }
    }

    // ── SF-025: hidden-path scan-policy predicate ──
    //
    // The bulk walk skips hidden dotfiles/dotdirs; the single-file (re)index path
    // must mirror that so index membership is deterministic from scan policy.
    mod hidden_path {
        use super::*;

        #[test]
        fn detects_hidden_directory_component() {
            assert!(path_has_hidden_component(".github/workflows/ci.yml"));
            assert!(path_has_hidden_component(
                "deps/hiredis/.github/release.yml"
            ));
            assert!(path_has_hidden_component(".travis.yml"));
            assert!(path_has_hidden_component("a/b/.hidden"));
        }

        #[test]
        fn allows_visible_paths_and_traversal_segments() {
            assert!(!path_has_hidden_component("src/main.rs"));
            assert!(!path_has_hidden_component("README.md"));
            // `.`/`..` traversal segments are not "hidden" file names.
            assert!(!path_has_hidden_component("./src/main.rs"));
            assert!(!path_has_hidden_component("../sibling/main.rs"));
            // A dot inside a name (not a leading-dot component) is visible.
            assert!(!path_has_hidden_component("src/a.b.c/main.rs"));
        }

        #[test]
        fn discover_all_files_skips_hidden_supported_extension_files() {
            let tmp = TempDir::new().unwrap();
            // A hidden dir with a SUPPORTED extension inside — the exact SF-025
            // shape (e.g. `.github/workflows/ci.yml`).
            create_file(tmp.path(), ".github/workflows/ci.yml", "name: ci\n");
            create_file(tmp.path(), ".travis.yml", "language: rust\n");
            // Visible source must still be discovered.
            create_file(tmp.path(), "src/main.rs", "fn main() {}");

            let entries = discover_all_files(tmp.path()).unwrap();
            let paths: Vec<&str> = entries.iter().map(|e| e.relative_path.as_str()).collect();

            assert!(
                !paths.contains(&".github/workflows/ci.yml"),
                "hidden-dir file must not be discovered by the bulk walk: {paths:?}"
            );
            assert!(
                !paths.contains(&".travis.yml"),
                "hidden dotfile must not be discovered by the bulk walk: {paths:?}"
            );
            assert!(
                paths.contains(&"src/main.rs"),
                "visible source must be discovered: {paths:?}"
            );
        }
    }

    // ── SF-012(B): build-dir heuristic rescues tracked source dirs ──
    mod build_dir_tracked_rescue {
        use super::*;
        use std::process::Command;

        #[test]
        fn discover_all_files_rescues_tracked_target_specs_dir() {
            let tmp = TempDir::new().unwrap();
            let root = tmp.path();
            let run = |args: &[&str]| {
                Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .expect("git command");
            };
            run(&["init"]);
            run(&["config", "user.email", "test@test.com"]);
            run(&["config", "user.name", "Test"]);

            // tokio's real shape: a tracked `target-specs/` source dir whose name
            // matches the build-dir heuristic, plus a genuine `target/` build dir.
            create_file(root, "target-specs/i686.json", "{}\n");
            create_file(root, "target-specs/README.md", "# specs\n");
            create_file(root, "src/main.rs", "fn main() {}");
            // Stage+commit ONLY the source and the target-specs dir — the build
            // dir below is left untracked, exactly like real build output.
            run(&["add", "target-specs", "src"]);
            run(&["commit", "-m", "initial"]);
            // A genuine (untracked) build dir matching the heuristic.
            create_file(root, "target/debug/artifact.rs", "fn a() {}");

            let entries = discover_all_files(root).unwrap();
            let paths: Vec<&str> = entries.iter().map(|e| e.relative_path.as_str()).collect();

            assert!(
                paths.contains(&"target-specs/i686.json"),
                "tracked target-specs/ source must be rescued: {paths:?}"
            );
            assert!(
                paths.contains(&"target-specs/README.md"),
                "tracked target-specs/ source must be rescued: {paths:?}"
            );
            assert!(
                paths.contains(&"src/main.rs"),
                "normal source must be discovered: {paths:?}"
            );
            assert!(
                !paths.contains(&"target/debug/artifact.rs"),
                "untracked build output must still be skipped: {paths:?}"
            );
        }

        #[test]
        fn discover_all_files_without_git_keeps_conservative_build_dir_skip() {
            // No git repo: the rescue helper fails open to None, so the heuristic
            // decides alone and a `target-*` dir is skipped exactly as before.
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "target-wsl/debug/artifact.rs", "fn a() {}");
            create_file(tmp.path(), "src/main.rs", "fn main() {}");

            let entries = discover_all_files(tmp.path()).unwrap();
            let paths: Vec<&str> = entries.iter().map(|e| e.relative_path.as_str()).collect();

            assert!(
                !paths.contains(&"target-wsl/debug/artifact.rs"),
                "non-git tree must still skip target-* build dirs: {paths:?}"
            );
            assert!(
                paths.contains(&"src/main.rs"),
                "normal source must be discovered: {paths:?}"
            );
        }
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
    fn test_oversized_code_file_under_4mb_is_normal() {
        // Dogfood #1/#7 (2026-07-06): >1MB first-party code is load-bearing
        // (a 1.2MB Rust module held the only construction site of a queried
        // type; symforge's own tools.rs crossed 1MB). Code languages get the
        // 4MB METADATA_ONLY_CODE_BYTES threshold.
        for name in ["orchestrator.rs", "big.py", "huge.ts", "large.pm"] {
            let decision = classify_admission(std::path::Path::new(name), 1_200_000, None);
            assert_eq!(
                decision,
                AdmissionDecision::normal(),
                "1.2MB code file {name} must stay Tier-1"
            );
        }
    }

    #[test]
    fn test_code_file_above_4mb_is_metadata_only() {
        let decision =
            classify_admission(std::path::Path::new("generated.rs"), 5 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
    }

    #[test]
    fn test_data_formats_keep_1mb_threshold() {
        // The symbol-pollution guard: machine-generated data files demote at
        // 1MB even though their language is "supported".
        for name in ["big.yaml", "big.toml", "big.md", "big.html"] {
            let decision = classify_admission(std::path::Path::new(name), 1_200_000, None);
            assert_eq!(
                decision.tier,
                AdmissionTier::MetadataOnly,
                "1.2MB data file {name} must stay Tier-2"
            );
            assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
        }
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

    // ── bounded discovery (resource ceilings) ──
    //
    // These guard against OOM/panic on a huge but NON-sensitive tree: discovery
    // must refuse with a graceful, explicit error before committing the full set
    // to the in-memory index build. Env-mutating cases are serialized by a mutex
    // and restore the prior value on drop so they don't race other env readers.
    mod bounded_discovery {
        use super::*;
        use std::ffi::OsString;
        use std::sync::Mutex;

        // Serializes the env-mutating limit tests against each other. Discovery
        // env vars are process-global, so two tests setting them concurrently
        // would interfere.
        static ENV_LOCK: Mutex<()> = Mutex::new(());

        struct LimitEnvGuard {
            files_prev: Option<OsString>,
            bytes_prev: Option<OsString>,
        }

        #[allow(unsafe_code)] // test-only env guard; mutation is serialized by ENV_LOCK.
        impl LimitEnvGuard {
            /// Set both limit env vars (any `None` clears that var) and capture
            /// the prior values for restoration on drop.
            fn set(max_files: Option<&str>, max_bytes: Option<&str>) -> Self {
                let files_prev = std::env::var_os(MAX_INDEX_FILES_ENV);
                let bytes_prev = std::env::var_os(MAX_INDEX_BYTES_ENV);
                // SAFETY: env mutation is serialized by ENV_LOCK held by the caller;
                // no concurrent env readers in this single-threaded test section.
                unsafe {
                    match max_files {
                        Some(v) => std::env::set_var(MAX_INDEX_FILES_ENV, v),
                        None => std::env::remove_var(MAX_INDEX_FILES_ENV),
                    }
                    match max_bytes {
                        Some(v) => std::env::set_var(MAX_INDEX_BYTES_ENV, v),
                        None => std::env::remove_var(MAX_INDEX_BYTES_ENV),
                    }
                }
                Self {
                    files_prev,
                    bytes_prev,
                }
            }
        }

        #[allow(unsafe_code)] // test-only env guard; restores serialized env mutation.
        impl Drop for LimitEnvGuard {
            fn drop(&mut self) {
                // SAFETY: env mutation is serialized by ENV_LOCK; restore prior state.
                unsafe {
                    match &self.files_prev {
                        Some(v) => std::env::set_var(MAX_INDEX_FILES_ENV, v),
                        None => std::env::remove_var(MAX_INDEX_FILES_ENV),
                    }
                    match &self.bytes_prev {
                        Some(v) => std::env::set_var(MAX_INDEX_BYTES_ENV, v),
                        None => std::env::remove_var(MAX_INDEX_BYTES_ENV),
                    }
                }
            }
        }

        #[test]
        fn default_limits_are_generous() {
            let limits = DiscoveryLimits::default();
            assert_eq!(limits.max_files, DEFAULT_MAX_INDEX_FILES);
            assert_eq!(limits.max_bytes, DEFAULT_MAX_INDEX_BYTES);
            // 200k files is comfortably above a very large real monorepo.
            assert!(limits.max_files >= 200_000);
        }

        #[test]
        fn parse_positive_env_rejects_zero_empty_and_garbage() {
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(Some("0"), Some("not-a-number"));
            // Zero and non-numeric overrides are ignored, so the defaults stand —
            // a typo can never silently disable indexing.
            assert_eq!(parse_positive_env(MAX_INDEX_FILES_ENV), None);
            assert_eq!(parse_positive_env(MAX_INDEX_BYTES_ENV), None);
            let limits = DiscoveryLimits::from_env();
            assert_eq!(limits.max_files, DEFAULT_MAX_INDEX_FILES);
            assert_eq!(limits.max_bytes, DEFAULT_MAX_INDEX_BYTES);
        }

        #[test]
        fn from_env_honors_valid_override() {
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(Some("5"), Some("4096"));
            let limits = DiscoveryLimits::from_env();
            assert_eq!(limits.max_files, 5);
            assert_eq!(limits.max_bytes, 4096);
        }

        #[test]
        fn normal_repo_indexes_under_default_cap() {
            // No env override: the generous default cap must not interfere with a
            // small, ordinary project.
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(None, None);
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "main.rs", "fn main() {}");
            create_file(tmp.path(), "lib.rs", "pub fn f() {}");
            create_file(tmp.path(), "README.md", "# hi");

            let files = discover_files(tmp.path()).expect("normal repo indexes fine");
            assert_eq!(files.len(), 3);

            let entries = discover_all_files(tmp.path()).expect("normal repo full-discovery fine");
            assert!(entries.len() >= 3);
        }

        #[test]
        fn over_file_cap_yields_graceful_error_not_panic() {
            let _lock = ENV_LOCK.lock().unwrap();
            // Cap at 2 files; create 5 source files to exceed it.
            let _guard = LimitEnvGuard::set(Some("2"), None);
            let tmp = TempDir::new().unwrap();
            for i in 0..5 {
                create_file(tmp.path(), &format!("f{i}.rs"), "fn x() {}");
            }

            let err = discover_files(tmp.path()).expect_err("over file cap must error");
            let msg = err.to_string();
            assert!(
                msg.contains("tree too large to index"),
                "error must be the graceful over-cap message: {msg}"
            );
            assert!(
                msg.contains(MAX_INDEX_FILES_ENV),
                "error must name the override knob: {msg}"
            );

            let err2 = discover_all_files(tmp.path()).expect_err("full discovery over cap errors");
            assert!(err2.to_string().contains("tree too large to index"));
        }

        #[test]
        fn over_byte_cap_yields_graceful_error() {
            let _lock = ENV_LOCK.lock().unwrap();
            // Very high file cap, tiny byte cap (8 bytes). A single non-empty file
            // exceeds the byte ceiling, exercising the cumulative-bytes path that
            // only `discover_all_files` enforces.
            let _guard = LimitEnvGuard::set(Some("1000000"), Some("8"));
            let tmp = TempDir::new().unwrap();
            create_file(
                tmp.path(),
                "big.rs",
                "fn this_is_more_than_eight_bytes() {}",
            );

            let err = discover_all_files(tmp.path()).expect_err("over byte cap must error");
            let msg = err.to_string();
            assert!(
                msg.contains("tree too large to index"),
                "error must be the graceful over-cap message: {msg}"
            );
            assert!(
                msg.contains(MAX_INDEX_BYTES_ENV),
                "error must name the byte override knob: {msg}"
            );
        }

        #[test]
        fn raised_cap_lets_a_previously_over_cap_tree_index() {
            let _lock = ENV_LOCK.lock().unwrap();
            let tmp = TempDir::new().unwrap();
            for i in 0..4 {
                create_file(tmp.path(), &format!("f{i}.rs"), "fn x() {}");
            }
            // Low cap: refused.
            {
                let _guard = LimitEnvGuard::set(Some("2"), None);
                assert!(discover_files(tmp.path()).is_err());
            }
            // Raised cap: accepted — the limit is genuinely configurable.
            {
                let _guard = LimitEnvGuard::set(Some("100"), None);
                let files = discover_files(tmp.path()).expect("raised cap indexes the tree");
                assert_eq!(files.len(), 4);
            }
        }
    }

    // ── `.git` fast-path sensitive-root guard (find_project_root) ──
    //
    // A `.git` planted under a forbidden/sensitive ancestor must NOT be selected
    // as the project root; a genuine project `.git` still is. We exercise the
    // guard helper `is_forbidden_root` that the fast-path now consults, on
    // synthetic sensitive shapes, plus a positive case for an ordinary repo.
    mod git_fast_path_guard {
        use super::*;

        #[test]
        fn ordinary_project_with_git_is_not_forbidden() {
            // A normal temp project dir is not sensitive/forbidden, so a `.git`
            // there would be selected by the fast-path.
            let tmp = TempDir::new().unwrap();
            fs::create_dir_all(tmp.path().join(".git")).unwrap();
            assert!(
                !is_forbidden_root(tmp.path()),
                "an ordinary project dir must remain selectable as a git root"
            );
        }

        #[cfg(not(target_os = "windows"))]
        #[test]
        fn sensitive_unix_root_with_git_is_forbidden() {
            // `/etc` is sensitive; a planted `/etc/.git` must NOT be selected.
            // We assert the guard the fast-path consults rejects the root itself,
            // independent of whether the path exists on the test host.
            assert!(
                is_forbidden_root(Path::new("/etc")),
                "/etc must be forbidden even if a `.git` is planted there"
            );
            assert!(crate::paths::is_sensitive_path(Path::new("/etc")));
        }

        #[cfg(target_os = "windows")]
        #[test]
        fn sensitive_windows_root_with_git_is_forbidden() {
            // A bare drive root and the Windows user container are sensitive; a
            // planted `.git` there must NOT be selected by the fast-path.
            assert!(is_forbidden_root(Path::new("C:\\Windows")));
            assert!(crate::paths::is_sensitive_path(Path::new("C:\\Windows")));
        }
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

    /// TR-03 / FR-013: the `SYMFORGE_WORKSPACE_ROOT` cold-start override is
    /// honored for a real directory and rejected (via the shared trust-boundary
    /// guard) for a sensitive/broad one — it can never widen what is auto-indexed.
    mod workspace_root_override {
        use super::*;
        use std::ffi::OsString;
        use std::sync::Mutex;

        // Serializes env mutation; `SYMFORGE_WORKSPACE_ROOT` is process-global.
        static ENV_LOCK: Mutex<()> = Mutex::new(());

        struct RootEnvGuard {
            prev: Option<OsString>,
        }

        #[allow(unsafe_code)] // test-only env guard; mutation serialized by ENV_LOCK.
        impl RootEnvGuard {
            fn set(value: Option<&str>) -> Self {
                let prev = std::env::var_os(WORKSPACE_ROOT_ENV);
                // SAFETY: serialized by ENV_LOCK held by the caller.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(WORKSPACE_ROOT_ENV, v),
                        None => std::env::remove_var(WORKSPACE_ROOT_ENV),
                    }
                }
                Self { prev }
            }
        }

        #[allow(unsafe_code)] // test-only env guard; restores serialized state.
        impl Drop for RootEnvGuard {
            fn drop(&mut self) {
                // SAFETY: serialized by ENV_LOCK.
                unsafe {
                    match &self.prev {
                        Some(v) => std::env::set_var(WORKSPACE_ROOT_ENV, v),
                        None => std::env::remove_var(WORKSPACE_ROOT_ENV),
                    }
                }
            }
        }

        #[test]
        fn honors_a_real_workspace_directory() {
            let _lock = ENV_LOCK.lock().unwrap();
            let workspace = TempDir::new().unwrap();
            let _guard = RootEnvGuard::set(Some(&workspace.path().display().to_string()));

            let resolved = workspace_root_env_override().expect("real dir must resolve");
            let resolved = resolved.canonicalize().unwrap_or(resolved);
            let expected = workspace.path().canonicalize().unwrap();
            assert_eq!(resolved, expected);
        }

        #[test]
        fn ignores_empty_and_missing_paths() {
            let _lock = ENV_LOCK.lock().unwrap();
            let _empty = RootEnvGuard::set(Some("   "));
            assert!(workspace_root_env_override().is_none());

            let _missing = RootEnvGuard::set(Some("/no/such/symforge/workspace/xyzzy"));
            assert!(workspace_root_env_override().is_none());

            let _unset = RootEnvGuard::set(None);
            assert!(workspace_root_env_override().is_none());
        }

        #[test]
        fn rejects_a_forbidden_home_dir_override() {
            let _lock = ENV_LOCK.lock().unwrap();
            let Some(home) = home_dir() else {
                return; // no home dir in this environment; nothing to assert
            };
            let _guard = RootEnvGuard::set(Some(&home.display().to_string()));
            assert!(
                workspace_root_env_override().is_none(),
                "the forbidden home dir must be rejected by the shared trust-boundary guard"
            );
        }
    }

    // ── MCP-roots workspace resolution: pure precedence + URI parsing ──
    //
    // These exercise `resolve_workspace_root`/`parse_root_uri` with explicit
    // arguments (no process global state), so they need neither the env lock
    // nor a real launch CWD. The keystone case — no usable CWD and no env, a
    // client root resolving the workspace — is asserted directly.
    mod roots_workspace_resolution {
        use super::*;

        /// Build a `file://` URI for a real path in the host-native form so the
        /// assertion holds on both Windows (`file:///C:/...`) and Unix
        /// (`file:///home/...`). Percent-encoding is not applied; a dedicated
        /// test covers decode.
        fn file_uri(path: &std::path::Path) -> String {
            let s = path.display().to_string().replace('\\', "/");
            if s.starts_with('/') {
                format!("file://{s}")
            } else {
                // Windows drive path: `C:/proj` -> `file:///C:/proj`.
                format!("file:///{s}")
            }
        }

        #[test]
        fn client_root_wins_over_forbidden_cwd_and_no_env() {
            // The keystone: launch CWD is the forbidden home dir (so the CWD
            // walk yields None via find_project_root), no env override, and the
            // MCP client declares its open workspace folder. The client root
            // MUST resolve the workspace.
            let workspace = TempDir::new().unwrap();
            let uri = file_uri(workspace.path());

            // `cwd_root` is None exactly as `find_project_root` returns for a
            // forbidden home/system CWD — the bug condition.
            let resolved = resolve_workspace_root(None, std::slice::from_ref(&uri), None)
                .expect("a valid client root must resolve the workspace with no env and no CWD");
            let resolved = resolved.canonicalize().unwrap_or(resolved);
            let expected = workspace.path().canonicalize().unwrap();
            assert_eq!(
                resolved, expected,
                "client root must drive workspace resolution when CWD is unusable"
            );
        }

        #[test]
        fn env_override_beats_client_roots() {
            // Precedence rule 1: an explicit (already-validated) env root wins
            // over any client root, even a valid one.
            let env_ws = TempDir::new().unwrap();
            let client_ws = TempDir::new().unwrap();
            let client_uri = file_uri(client_ws.path());

            let resolved = resolve_workspace_root(
                Some(env_ws.path().to_path_buf()),
                std::slice::from_ref(&client_uri),
                None,
            )
            .expect("env override must resolve");
            assert_eq!(
                resolved,
                env_ws.path().to_path_buf(),
                "SYMFORGE_WORKSPACE_ROOT must take precedence over client roots"
            );
        }

        #[test]
        fn cwd_used_only_when_env_and_roots_absent() {
            // Precedence rule 3: with no env and no usable client roots, fall
            // back to the (already-validated) CWD walk result.
            let cwd_ws = TempDir::new().unwrap();
            let resolved = resolve_workspace_root(None, &[], Some(cwd_ws.path().to_path_buf()))
                .expect("CWD fallback must resolve");
            assert_eq!(resolved, cwd_ws.path().to_path_buf());
        }

        #[test]
        fn forbidden_client_root_is_skipped_not_fatal() {
            // A forbidden client root (home dir) must be skipped; a later valid
            // root in the same list still wins. Trust boundary holds: a client
            // cannot push a forbidden root past the guard.
            let Some(home) = home_dir() else {
                return; // no home dir in this environment
            };
            let valid = TempDir::new().unwrap();
            let roots = vec![file_uri(&home), file_uri(valid.path())];

            let resolved = resolve_workspace_root(None, &roots, None)
                .expect("a valid later root must resolve after a forbidden one is skipped");
            let resolved = resolved.canonicalize().unwrap_or(resolved);
            let expected = valid.path().canonicalize().unwrap();
            assert_eq!(resolved, expected);
        }

        #[test]
        fn all_forbidden_roots_yield_none_when_no_other_source() {
            let Some(home) = home_dir() else {
                return;
            };
            let roots = vec![file_uri(&home)];
            assert!(
                resolve_workspace_root(None, &roots, None).is_none(),
                "no env, all-forbidden roots, no CWD -> no workspace (must not widen trust)"
            );
        }

        #[test]
        fn parse_root_uri_handles_file_scheme_and_raw_path() {
            let ws = TempDir::new().unwrap();
            let native = ws.path().to_path_buf();

            // file:// form round-trips to the same directory.
            let from_uri = parse_root_uri(&file_uri(ws.path())).expect("file:// URI must parse");
            assert_eq!(
                from_uri.canonicalize().unwrap(),
                native.canonicalize().unwrap()
            );

            // Raw (non-URI) path passes through verbatim for lenient clients.
            let raw = native.display().to_string();
            assert_eq!(parse_root_uri(&raw), Some(native.clone()));

            // Empty / whitespace -> None.
            assert_eq!(parse_root_uri("   "), None);
        }

        #[test]
        fn parse_root_uri_rejects_non_file_scheme() {
            assert_eq!(parse_root_uri("http://example.com/repo"), None);
            assert_eq!(parse_root_uri("https://example.com/repo"), None);
        }

        #[test]
        fn parse_root_uri_percent_decodes() {
            // `file:///tmp/a%20b` -> `/tmp/a b`. Use a Unix-style path literal so
            // the decode is asserted independent of the host filesystem.
            let decoded = parse_root_uri("file:///tmp/a%20b/c%2Bd").expect("must parse");
            // On Windows the leading-slash-before-drive rule does not apply here
            // (no drive letter), so the path keeps its leading slash.
            assert_eq!(
                decoded.to_string_lossy().replace('\\', "/"),
                "/tmp/a b/c+d",
                "percent escapes must decode to literal bytes"
            );
        }
    }
}
