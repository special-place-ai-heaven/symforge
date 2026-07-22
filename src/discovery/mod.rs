use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::{
    AccessErrorKind, AccessStage, CatalogPath, CoverageStatus, FileClassification, FileStamp,
    FreshnessReason, HardSkipReason, IndexTargets, LanguageId, ManifestResourceUsage,
    MetadataOnlyReason, ProjectId, RootBinding, RootCandidateSource, RootClass, RootRefusalReason,
    RootRequestMode, RootResolution, ScoutDecision, ScoutIssue, ScoutIssueKind, ScoutedEntry,
    SourceAccessMode, StateFailure, StateLocationKind, StatePlacement, UnboundReason,
    UserLocalPlacementReason,
};

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
    /// Lossless repository-relative native path used by the metadata-first scout.
    pub relative_os_path: PathBuf,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// File size in bytes from the walk metadata (no extra stat syscall).
    pub file_size: u64,
    /// Language inferred from the extension, if recognized.
    pub language: Option<LanguageId>,
    /// Semantic-lane classification (test/vendor/generated/config flags).
    pub classification: FileClassification,
}

/// Immutable metadata-first observation consumed by later content execution.
#[derive(Debug, Clone)]
pub struct ScoutPlan {
    pub coverage: CoverageStatus,
    pub entries: Vec<ScoutedEntry>,
    pub issues: Vec<ScoutIssue>,
    pub usage: ManifestResourceUsage,
}

/// Recompute canonical ordering, coverage, and resource accounting after a
/// generation-fenced incremental manifest mutation.
pub(crate) fn refresh_scout_plan(plan: &mut ScoutPlan) -> Result<()> {
    plan.entries.sort_by_cached_key(|entry| {
        scout_order_key(
            entry.path.normalized_utf8.as_deref(),
            Some(entry.path.public_id.as_str()),
        )
    });
    plan.issues.sort_by_cached_key(|issue| {
        scout_order_key(issue.safe_path.as_deref(), issue.path_id.as_deref())
    });

    let limits = DiscoveryLimits::from_env();
    let mut catalog_metadata_bytes = 2u64;
    for issue in &plan.issues {
        catalog_metadata_bytes = account_catalog_metadata_record(
            catalog_metadata_bytes,
            &("issue", issue),
            limits.max_catalog_metadata_bytes,
        )?;
    }
    let mut admitted_content_bytes = 0u64;
    for entry in &plan.entries {
        catalog_metadata_bytes = account_catalog_metadata_record(
            catalog_metadata_bytes,
            &(
                "entry",
                &entry.path,
                entry.stamp.size,
                &entry.language,
                entry.classification,
                &entry.decision,
            ),
            limits.max_catalog_metadata_bytes,
        )?;
        if matches!(entry.decision, ScoutDecision::Ingest { .. }) {
            admitted_content_bytes = admitted_content_bytes.saturating_add(entry.stamp.size);
        }
    }

    plan.coverage = if plan.issues.is_empty()
        && plan
            .entries
            .iter()
            .all(|entry| !matches!(entry.decision, ScoutDecision::Unavailable { .. }))
    {
        CoverageStatus::Complete
    } else {
        CoverageStatus::Degraded
    };
    plan.usage = ManifestResourceUsage {
        catalog_entries: plan.entries.len() as u64,
        catalog_metadata_bytes,
        admitted_content_bytes,
    };
    Ok(())
}

/// A candidate scout refused before a manifest exists because a catalog budget
/// was exhausted. This is operational freshness state, never a manifest issue.
#[derive(Debug)]
pub struct ScoutCapacityError {
    reason: FreshnessReason,
    safe_message: String,
}

impl ScoutCapacityError {
    fn new(reason: FreshnessReason, safe_message: String) -> Self {
        Self {
            reason,
            safe_message,
        }
    }

    pub fn reason(&self) -> FreshnessReason {
        self.reason
    }
}

impl std::fmt::Display for ScoutCapacityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.safe_message)
    }
}

impl std::error::Error for ScoutCapacityError {}

/// Canonical repository-relative subtrees that are outside source scope.
///
/// The name-based `.git` / `.symforge` exclusions are universal. This policy
/// carries resolved state directories whose names are not universal (for
/// example, a user-local state root configured beneath the repository). Keeping
/// the policy repository-relative lets watcher paths be rejected before any
/// metadata or content I/O.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceExclusions {
    relative_subtrees: Vec<PathBuf>,
}

impl SourceExclusions {
    pub(crate) fn for_state_placement(root: &Path, placement: &StatePlacement) -> Self {
        let directory = match placement {
            StatePlacement::ProjectLocal { directory }
            | StatePlacement::UserLocal { directory, .. } => directory.as_path(),
            StatePlacement::MemoryOnly { .. } => return Self::default(),
        };

        let canonical_root = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let canonical_directory =
            dunce::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
        let relative = canonical_directory
            .strip_prefix(&canonical_root)
            .or_else(|_| directory.strip_prefix(root));
        let Ok(relative) = relative else {
            return Self::default();
        };

        let mut relative_subtrees = vec![relative.to_path_buf()];
        relative_subtrees.sort();
        relative_subtrees.dedup();
        Self { relative_subtrees }
    }

    pub(crate) fn excludes_relative(&self, relative_path: &Path) -> bool {
        self.relative_subtrees
            .iter()
            .any(|subtree| relative_path.starts_with(subtree))
    }
}

/// Environment variable overriding the maximum number of files a single
/// discovery pass will accept before refusing to index the tree.
const MAX_INDEX_FILES_ENV: &str = "SYMFORGE_MAX_INDEX_FILES";
/// Environment variable overriding the maximum cumulative byte size a single
/// discovery pass will admit as file payload before refusing to index the tree.
const MAX_INDEX_BYTES_ENV: &str = "SYMFORGE_MAX_INDEX_BYTES";
/// Environment variable overriding the maximum canonical public catalog metadata
/// bytes a single scout candidate may retain.
const MAX_CATALOG_METADATA_BYTES_ENV: &str = "SYMFORGE_MAX_CATALOG_METADATA_BYTES";

/// Default file-count ceiling. Generous enough for very large real monorepos
/// (this repo is ~230 files; 50k+ file monorepos are common), while still well
/// below the point where building the in-memory index maps/strings would
/// exhaust memory or trip `String join would overflow memory bounds`.
const DEFAULT_MAX_INDEX_FILES: u64 = 200_000;
/// Default cumulative-bytes ceiling: 16 GiB of accepted file content. A tree
/// whose discoverable files exceed this is almost certainly a generated-file
/// bomb, a mounted volume, or an accidental scratch root, not a project.
const DEFAULT_MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// Default canonical catalog-metadata ceiling: 512 MiB. This remains independent
/// from both file count and admitted payload bytes.
const DEFAULT_MAX_CATALOG_METADATA_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum exact UTF-8 spelling retained in a persisted catalog descriptor.
/// Longer native paths remain addressable transiently but are published only by
/// their bounded opaque ID.
const MAX_CATALOG_SAFE_PATH_BYTES: usize = 64 * 1024;

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
    /// Maximum canonical encoded bytes of public catalog metadata.
    pub max_catalog_metadata_bytes: u64,
}

impl DiscoveryLimits {
    /// Resolve limits from the environment, falling back to the generous
    /// defaults. A non-parseable or empty override is ignored (the default is
    /// used) so a typo can never silently *lower* the ceiling to zero and brick
    /// indexing — only an explicit, well-formed value takes effect.
    pub fn from_env() -> Self {
        let max_files = parse_positive_env(MAX_INDEX_FILES_ENV).unwrap_or(DEFAULT_MAX_INDEX_FILES);
        let max_bytes = parse_positive_env(MAX_INDEX_BYTES_ENV).unwrap_or(DEFAULT_MAX_INDEX_BYTES);
        let max_catalog_metadata_bytes = parse_positive_env(MAX_CATALOG_METADATA_BYTES_ENV)
            .unwrap_or(DEFAULT_MAX_CATALOG_METADATA_BYTES);
        Self {
            max_files,
            max_bytes,
            max_catalog_metadata_bytes,
        }
    }
}

impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            max_files: DEFAULT_MAX_INDEX_FILES,
            max_bytes: DEFAULT_MAX_INDEX_BYTES,
            max_catalog_metadata_bytes: DEFAULT_MAX_CATALOG_METADATA_BYTES,
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

/// VCS/runtime internals are outside source scope even when ignore rules
/// explicitly re-include them. Other repository-owned hidden paths remain
/// discoverable and flow through normal ignore/admission policy.
pub(crate) fn path_is_hard_scope_excluded(relative_path: &Path) -> bool {
    relative_path.components().any(|component| {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        name.to_str().is_some_and(|name| {
            name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case(".symforge")
        })
    })
}

fn repository_walk(root: &Path, exclusions: &SourceExclusions) -> ignore::Walk {
    let filter_root = root.to_path_buf();
    let exclusions = exclusions.clone();
    let mut builder = ignore::WalkBuilder::new(root);
    builder.hidden(false).filter_entry(move |entry| {
        entry
            .path()
            .strip_prefix(&filter_root)
            .is_ok_and(|relative| {
                !path_is_hard_scope_excluded(relative) && !exclusions.excludes_relative(relative)
            })
    });
    builder.build()
}

/// Build the graceful, explicit over-cap error. Surfaced to the caller (and
/// thus the MCP client) instead of an OOM/panic, and it names the override knob
/// so an operator with a genuinely huge repo can raise the ceiling.
fn tree_too_large_message(files: u64, bytes: u64, limits: &DiscoveryLimits) -> String {
    format!(
        "tree too large to index ({files} files / {bytes} bytes exceeds limit of \
         {max_files} files / {max_bytes} bytes); set {MAX_INDEX_FILES_ENV} or \
         {MAX_INDEX_BYTES_ENV} to override",
        max_files = limits.max_files,
        max_bytes = limits.max_bytes,
    )
}

fn catalog_entry_capacity_error(files: u64, bytes: u64, limits: &DiscoveryLimits) -> anyhow::Error {
    ScoutCapacityError::new(
        FreshnessReason::CatalogEntryCapacityExceeded,
        tree_too_large_message(files, bytes, limits),
    )
    .into()
}

fn admitted_content_capacity_error(
    files: u64,
    bytes: u64,
    limits: &DiscoveryLimits,
) -> anyhow::Error {
    anyhow::anyhow!(tree_too_large_message(files, bytes, limits))
}

fn catalog_metadata_capacity_error(bytes: u64, limit: u64) -> anyhow::Error {
    ScoutCapacityError::new(
        FreshnessReason::CatalogMetadataCapacityExceeded,
        format!(
            "catalog metadata capacity exceeded ({bytes} bytes exceeds limit of {limit} bytes); \
             set {MAX_CATALOG_METADATA_BYTES_ENV} to override"
        ),
    )
    .into()
}

/// Discover all source files under `root` that have a recognized language extension.
///
/// - Respects `.gitignore` files via the `ignore` crate.
/// - Normalizes path separators to `/` in `relative_path`.
/// - Returns files sorted case-insensitively by `relative_path`.
/// - Refuses trees that exceed [`DiscoveryLimits`] with a graceful error rather
///   than collecting an unbounded set and OOM-ing the in-memory index build.
pub fn discover_files(root: &Path) -> Result<Vec<DiscoveredFile>> {
    discover_files_with_exclusions(root, &SourceExclusions::default())
}

pub fn discover_files_with_exclusions(
    root: &Path,
    exclusions: &SourceExclusions,
) -> Result<Vec<DiscoveredFile>> {
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
    for entry_result in repository_walk(&root, exclusions) {
        let Ok(entry) = entry_result else { continue };
        let path =
            std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());

        // Use the already-known file_type from the walker instead of
        // path.is_file() which would issue a redundant stat() syscall.
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        // Compute relative path from root
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        // Normalize backslashes to forward slashes
        let relative_path = relative.to_string_lossy().replace('\\', "/");
        let Some(language) = LanguageId::from_path(&relative_path) else {
            continue;
        };
        let targets = IndexTargets::for_path(&relative_path, Some(&language));

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
            return Err(catalog_entry_capacity_error(
                files.len() as u64 + 1,
                0,
                &limits,
            ));
        }

        files.push(DiscoveredFile {
            classification: FileClassification::for_indexed_path(&relative_path, targets),
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
///   admitted bytes) with a graceful error rather than collecting an unbounded
///   set and OOM-ing / panicking the in-memory index build in `LiveIndex::load`.
///
/// This is the discovery entry point used by the full `LiveIndex::load`, so the
/// byte ceiling is enforced here (file sizes are already known from the walk
/// metadata, so no extra `stat` is needed to track cumulative admitted bytes).
pub fn discover_all_files(root: &Path) -> Result<Vec<DiscoveredEntry>> {
    discover_all_files_with_exclusions(root, &SourceExclusions::default())
}

pub fn discover_all_files_with_exclusions(
    root: &Path,
    exclusions: &SourceExclusions,
) -> Result<Vec<DiscoveredEntry>> {
    discover_all_files_with_exclusions_and_issues(root, exclusions).map(|(entries, _)| entries)
}

fn discover_all_files_with_exclusions_and_issues(
    root: &Path,
    exclusions: &SourceExclusions,
) -> Result<(Vec<DiscoveredEntry>, Vec<ScoutIssue>)> {
    // Canonicalize root so that strip_prefix succeeds even when the walker
    // resolves symlinks to their canonical targets.
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    // Bound the streaming walk by accepted file count AND cumulative admitted
    // bytes, refusing the moment either ceiling is crossed — before the
    // unbounded path/byte set is handed to the in-memory index build.
    let limits = DiscoveryLimits::from_env();
    // Repo-root build-dir child designated by CARGO_TARGET_DIR, resolved once.
    let target_dir_child = cargo_target_dir_root_child(&root);
    let mut total_bytes: u64 = 0;
    let mut entries: Vec<DiscoveredEntry> = Vec::new();
    let mut issues = Vec::new();
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
    for entry_result in repository_walk(&root, exclusions) {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                let kind = error
                    .io_error()
                    .map(std::io::Error::kind)
                    .unwrap_or(std::io::ErrorKind::Other);
                issues.push(walk_issue_for_error(&root, ignore_error_path(&error), kind));
                continue;
            }
        };
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
        let language = LanguageId::from_path(&relative_path);
        let targets = IndexTargets::for_path(&relative_path, language.as_ref());
        let classification = FileClassification::for_indexed_path(&relative_path, targets);

        // Apply metadata-terminal admission before accounting. Files that are
        // already MetadataOnly or HardSkip are never read, so their declared
        // size must not consume the ingest budget.
        let admitted_bytes = match classify_admission(&path, file_size, None).tier {
            AdmissionTier::Normal => file_size,
            AdmissionTier::MetadataOnly | AdmissionTier::HardSkip => 0,
        };

        // Refuse BEFORE pushing past either ceiling. `saturating_add` keeps the
        // admitted-byte counter from wrapping on a pathological tree; once it
        // crosses the limit we return the graceful error instead of allocating.
        let projected_files = entries.len() as u64 + 1;
        let projected_bytes = total_bytes.saturating_add(admitted_bytes);
        if projected_files > limits.max_files {
            return Err(catalog_entry_capacity_error(
                projected_files,
                projected_bytes,
                &limits,
            ));
        }
        if projected_bytes > limits.max_bytes {
            return Err(admitted_content_capacity_error(
                projected_files,
                projected_bytes,
                &limits,
            ));
        }
        total_bytes = projected_bytes;

        entries.push(DiscoveredEntry {
            relative_path,
            relative_os_path: relative.to_path_buf(),
            absolute_path: path,
            file_size,
            language,
            classification,
        });
    }

    // Cache sort keys once instead of lowercasing paths on every comparator call.
    entries.sort_by_cached_key(|entry| entry.relative_path.to_lowercase());

    Ok((entries, issues))
}

/// Build an immutable metadata-first plan without retaining file payload bytes.
pub fn scout_repository(root: &Path) -> Result<ScoutPlan> {
    scout_repository_with_metadata(root, |path| std::fs::metadata(path))
}

pub fn scout_repository_with_exclusions(
    root: &Path,
    exclusions: &SourceExclusions,
) -> Result<ScoutPlan> {
    scout_repository_with_io_and_exclusions(
        root,
        exclusions,
        |path| std::fs::metadata(path),
        read_binary_probe,
    )
}

fn scout_repository_with_metadata<F>(root: &Path, metadata_reader: F) -> Result<ScoutPlan>
where
    F: Fn(&Path) -> std::io::Result<std::fs::Metadata>,
{
    scout_repository_with_io(root, metadata_reader, read_binary_probe)
}

fn scout_repository_with_io<F, P>(
    root: &Path,
    metadata_reader: F,
    probe_reader: P,
) -> Result<ScoutPlan>
where
    F: Fn(&Path) -> std::io::Result<std::fs::Metadata>,
    P: FnMut(&Path, usize) -> std::io::Result<Vec<u8>>,
{
    scout_repository_with_io_and_exclusions(
        root,
        &SourceExclusions::default(),
        metadata_reader,
        probe_reader,
    )
}

/// Scout one watcher/reconciliation path through the same metadata-first
/// admission pipeline as a cold repository walk.
pub(crate) fn scout_single_path_with_io<F, P>(
    relative_path: &str,
    absolute_path: &Path,
    metadata_reader: F,
    probe_reader: P,
) -> Result<ScoutedEntry>
where
    F: Fn(&Path) -> std::io::Result<std::fs::Metadata>,
    P: FnMut(&Path, usize) -> std::io::Result<Vec<u8>>,
{
    let language = LanguageId::from_path(relative_path);
    let targets = IndexTargets::for_path(relative_path, language.as_ref());
    let discovered = DiscoveredEntry {
        relative_path: relative_path.to_string(),
        relative_os_path: PathBuf::from(relative_path),
        absolute_path: absolute_path.to_path_buf(),
        file_size: 0,
        language,
        classification: FileClassification::for_indexed_path(relative_path, targets),
    };
    let plan = scout_entries_with_io(vec![discovered], metadata_reader, probe_reader, Vec::new())?;
    plan.entries
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("single-path scout produced no entry for {relative_path}"))
}

pub(crate) fn scout_single_path(relative_path: &str, absolute_path: &Path) -> Result<ScoutedEntry> {
    scout_single_path_with_io(
        relative_path,
        absolute_path,
        |path| std::fs::metadata(path),
        read_binary_probe,
    )
}

fn scout_repository_with_io_and_exclusions<F, P>(
    root: &Path,
    exclusions: &SourceExclusions,
    metadata_reader: F,
    probe_reader: P,
) -> Result<ScoutPlan>
where
    F: Fn(&Path) -> std::io::Result<std::fs::Metadata>,
    P: FnMut(&Path, usize) -> std::io::Result<Vec<u8>>,
{
    let (discovered, walk_issues) =
        discover_all_files_with_exclusions_and_issues(root, exclusions)?;
    scout_entries_with_io(discovered, metadata_reader, probe_reader, walk_issues)
}

fn scout_entries_with_io<F, P>(
    discovered: Vec<DiscoveredEntry>,
    metadata_reader: F,
    mut probe_reader: P,
    mut issues: Vec<ScoutIssue>,
) -> Result<ScoutPlan>
where
    F: Fn(&Path) -> std::io::Result<std::fs::Metadata>,
    P: FnMut(&Path, usize) -> std::io::Result<Vec<u8>>,
{
    // The compatibility walk supplies path candidates only. Its metadata fallback
    // is not authoritative here: this observation either yields real metadata or a
    // typed issue, never a fabricated size.
    let mut admitted_content_bytes = 0u64;
    let limits = DiscoveryLimits::from_env();
    // Exact compact-JSON array framing for the canonical logical metadata records.
    let mut catalog_metadata_bytes = 2u64;
    let mut entries = Vec::with_capacity(discovered.len());
    let mut degraded = !issues.is_empty();

    for issue in &issues {
        catalog_metadata_bytes = account_catalog_metadata_record(
            catalog_metadata_bytes,
            &("issue", issue),
            limits.max_catalog_metadata_bytes,
        )?;
    }

    for mut entry in discovered {
        let (catalog_path, path_reason) = catalog_path_projection(&entry.relative_os_path);
        let public_id = catalog_path.public_id.clone();
        let metadata = match metadata_reader(&entry.absolute_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                degraded = true;
                let kind = access_error_kind(error.kind());
                let issue = ScoutIssue {
                    path_id: Some(public_id),
                    safe_path: catalog_path.normalized_utf8.clone(),
                    kind: ScoutIssueKind::DirectoryEntryUnreadable { kind: kind.clone() },
                    safe_message: "metadata unavailable".to_string(),
                };
                catalog_metadata_bytes = account_catalog_metadata_record(
                    catalog_metadata_bytes,
                    &("issue", &issue),
                    limits.max_catalog_metadata_bytes,
                )?;
                issues.push(issue);
                let scouted_entry = ScoutedEntry {
                    path: catalog_path,
                    absolute_path: Some(entry.absolute_path),
                    stamp: FileStamp {
                        size: entry.file_size,
                        created_hint: None,
                        modified_hint: None,
                        platform_id: None,
                    },
                    language: entry.language,
                    classification: entry.classification,
                    decision: ScoutDecision::Unavailable {
                        stage: AccessStage::Metadata,
                        kind,
                    },
                };
                catalog_metadata_bytes = account_catalog_metadata_record(
                    catalog_metadata_bytes,
                    &(
                        "entry",
                        &scouted_entry.path,
                        scouted_entry.stamp.size,
                        &scouted_entry.language,
                        scouted_entry.classification,
                        &scouted_entry.decision,
                    ),
                    limits.max_catalog_metadata_bytes,
                )?;
                entries.push(scouted_entry);
                continue;
            }
        };

        entry.file_size = metadata.len();
        let decision = if let Some(reason) = path_reason {
            ScoutDecision::MetadataOnly { reason }
        } else if let Some(rule_id) = crate::knowledge::sensitive_path_rule(&entry.relative_path) {
            ScoutDecision::MetadataOnly {
                reason: MetadataOnlyReason::SensitivePath {
                    rule_id: rule_id.to_string(),
                },
            }
        } else {
            match scout_decision_for_discovered(&entry, None) {
                ScoutDecision::Ingest { .. } => match probe_reader(
                    &entry.absolute_path,
                    crate::domain::index::BINARY_SNIFF_BYTES,
                ) {
                    Ok(sample) => scout_decision_for_discovered(&entry, Some(&sample)),
                    Err(error) => {
                        degraded = true;
                        ScoutDecision::Unavailable {
                            stage: AccessStage::Probe,
                            kind: access_error_kind(error.kind()),
                        }
                    }
                },
                terminal => terminal,
            }
        };
        if matches!(decision, ScoutDecision::Ingest { .. }) {
            admitted_content_bytes = admitted_content_bytes.saturating_add(entry.file_size);
        }
        let scouted_entry = ScoutedEntry {
            path: catalog_path,
            absolute_path: Some(entry.absolute_path),
            stamp: FileStamp {
                size: entry.file_size,
                created_hint: metadata.created().ok(),
                modified_hint: metadata.modified().ok(),
                platform_id: None,
            },
            language: entry.language,
            classification: entry.classification,
            decision,
        };
        catalog_metadata_bytes = account_catalog_metadata_record(
            catalog_metadata_bytes,
            &(
                "entry",
                &scouted_entry.path,
                scouted_entry.stamp.size,
                &scouted_entry.language,
                scouted_entry.classification,
                &scouted_entry.decision,
            ),
            limits.max_catalog_metadata_bytes,
        )?;
        entries.push(scouted_entry);
    }

    entries.sort_by_cached_key(|entry| {
        scout_order_key(
            entry.path.normalized_utf8.as_deref(),
            Some(entry.path.public_id.as_str()),
        )
    });
    let collision_issues = path_identity_collision_issues(&entries);
    if !collision_issues.is_empty() {
        degraded = true;
        for issue in collision_issues {
            catalog_metadata_bytes = account_catalog_metadata_record(
                catalog_metadata_bytes,
                &("issue", &issue),
                limits.max_catalog_metadata_bytes,
            )?;
            issues.push(issue);
        }
    }
    issues.sort_by_cached_key(|issue| {
        scout_order_key(issue.safe_path.as_deref(), issue.path_id.as_deref())
    });

    Ok(ScoutPlan {
        coverage: if degraded {
            CoverageStatus::Degraded
        } else {
            CoverageStatus::Complete
        },
        usage: ManifestResourceUsage {
            catalog_entries: entries.len() as u64,
            catalog_metadata_bytes,
            admitted_content_bytes,
        },
        entries,
        issues,
    })
}

fn account_catalog_metadata_record<T: serde::Serialize + ?Sized>(
    current_bytes: u64,
    record: &T,
    limit: u64,
) -> Result<u64> {
    let encoded_bytes = serde_json::to_vec(record)?.len() as u64;
    let separator_bytes = u64::from(current_bytes > 2);
    let projected_bytes = current_bytes
        .saturating_add(separator_bytes)
        .saturating_add(encoded_bytes);
    if projected_bytes > limit {
        Err(catalog_metadata_capacity_error(projected_bytes, limit))
    } else {
        Ok(projected_bytes)
    }
}

fn catalog_path_for_relative(path: &Path) -> CatalogPath {
    catalog_path_projection(path).0
}

fn catalog_path_projection(path: &Path) -> (CatalogPath, Option<MetadataOnlyReason>) {
    let (normalized_utf8, reason) = match path.to_str() {
        None => (None, Some(MetadataOnlyReason::UnsupportedPathEncoding)),
        Some(value) if value.len() > MAX_CATALOG_SAFE_PATH_BYTES => {
            (None, Some(MetadataOnlyReason::PathMetadataTooLarge))
        }
        Some(value) if !is_safe_catalog_path(path, value) => {
            (None, Some(MetadataOnlyReason::UnsupportedPathEncoding))
        }
        Some(value) => (Some(normalize_catalog_path(value)), None),
    };
    let mut identity = Vec::new();

    if let Some(safe_path) = normalized_utf8.as_deref() {
        identity.extend_from_slice(b"symforge-catalog-path-v1:utf8\0");
        identity.extend_from_slice(safe_path.as_bytes());
    } else {
        identity.extend_from_slice(b"symforge-catalog-path-v1:native\0");
        identity.extend_from_slice(&native_path_identity_bytes(path));
    }

    (
        CatalogPath {
            public_id: crate::hash::digest_hex(&identity),
            normalized_utf8,
        },
        reason,
    )
}

fn is_safe_catalog_path(path: &Path, value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_control)
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

#[cfg(windows)]
fn normalize_catalog_path(value: &str) -> String {
    value.replace('\\', "/")
}

#[cfg(not(windows))]
fn normalize_catalog_path(value: &str) -> String {
    value.to_string()
}

#[cfg(unix)]
fn native_path_identity_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn native_path_identity_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(not(any(unix, windows)))]
fn native_path_identity_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

fn scout_order_key(
    safe_path: Option<&str>,
    public_id: Option<&str>,
) -> (u8, String, Vec<u8>, String) {
    match safe_path {
        Some(path) => (
            0,
            path.to_lowercase(),
            path.as_bytes().to_vec(),
            public_id.unwrap_or_default().to_string(),
        ),
        None => (
            1,
            String::new(),
            Vec::new(),
            public_id.unwrap_or_default().to_string(),
        ),
    }
}

fn path_identity_collision_issues(entries: &[ScoutedEntry]) -> Vec<ScoutIssue> {
    let mut issues = Vec::new();
    let mut start = 0usize;

    while start < entries.len() {
        let Some(path) = entries[start].path.normalized_utf8.as_deref() else {
            start += 1;
            continue;
        };
        let folded = path.to_lowercase();
        let mut end = start + 1;
        while end < entries.len()
            && entries[end]
                .path
                .normalized_utf8
                .as_deref()
                .is_some_and(|candidate| candidate.to_lowercase() == folded)
        {
            end += 1;
        }

        let has_distinct_spellings = entries[start..end]
            .windows(2)
            .any(|pair| pair[0].path.normalized_utf8 != pair[1].path.normalized_utf8);
        if has_distinct_spellings {
            for entry in &entries[start..end] {
                issues.push(ScoutIssue {
                    path_id: Some(entry.path.public_id.clone()),
                    safe_path: entry.path.normalized_utf8.clone(),
                    kind: ScoutIssueKind::PathIdentityCollision,
                    safe_message: "case-fold path identity collision".to_string(),
                });
            }
        }
        start = end;
    }

    issues
}

fn walk_issue_for_error(
    root: &Path,
    failed_path: Option<&Path>,
    kind: std::io::ErrorKind,
) -> ScoutIssue {
    let catalog_path = failed_path
        .and_then(|path| path.strip_prefix(root).ok())
        .map(catalog_path_for_relative);
    ScoutIssue {
        path_id: catalog_path.as_ref().map(|path| path.public_id.clone()),
        safe_path: catalog_path.and_then(|path| path.normalized_utf8),
        kind: ScoutIssueKind::DirectoryEntryUnreadable {
            kind: access_error_kind(kind),
        },
        safe_message: "directory entry unavailable".to_string(),
    }
}

fn ignore_error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::Partial(errors) => errors.iter().find_map(ignore_error_path),
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => {
            ignore_error_path(err)
        }
        ignore::Error::WithPath { path, .. } => Some(path.as_path()),
        ignore::Error::Loop { child, .. } => Some(child.as_path()),
        ignore::Error::Io(_)
        | ignore::Error::Glob { .. }
        | ignore::Error::UnrecognizedFileType(_)
        | ignore::Error::InvalidDefinition => None,
    }
}

pub(crate) fn access_error_kind(kind: std::io::ErrorKind) -> AccessErrorKind {
    match kind {
        std::io::ErrorKind::NotFound => AccessErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => AccessErrorKind::PermissionDenied,
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::InvalidInput => {
            AccessErrorKind::InvalidData
        }
        std::io::ErrorKind::OutOfMemory => AccessErrorKind::ResourceExhausted,
        _ => AccessErrorKind::Other,
    }
}

fn read_binary_probe(path: &Path, max_bytes: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;

    let mut bytes = Vec::with_capacity(max_bytes);
    std::fs::File::open(path)?
        .take(max_bytes as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn scout_decision_for_discovered(
    entry: &DiscoveredEntry,
    content_sample: Option<&[u8]>,
) -> ScoutDecision {
    let admission = classify_admission(&entry.absolute_path, entry.file_size, content_sample);
    match admission.tier {
        AdmissionTier::Normal => ScoutDecision::Ingest {
            targets: IndexTargets::for_path(&entry.relative_path, entry.language.as_ref()),
        },
        AdmissionTier::HardSkip => ScoutDecision::HardSkip {
            reason: match admission.reason {
                Some(SkipReason::SizeCeiling) => HardSkipReason::PerFileCeiling,
                _ => HardSkipReason::ArtifactType,
            },
        },
        AdmissionTier::MetadataOnly => ScoutDecision::MetadataOnly {
            reason: match admission.reason {
                Some(SkipReason::DependencyLockfile) => MetadataOnlyReason::Lockfile,
                Some(SkipReason::BinaryContent) => MetadataOnlyReason::Binary,
                Some(SkipReason::SizeThreshold) => MetadataOnlyReason::OversizedData,
                Some(SkipReason::DenylistedExtension)
                | Some(SkipReason::Untracked)
                | Some(SkipReason::GeneratedOutput) => MetadataOnlyReason::GeneratedOrVendor,
                Some(SkipReason::UnsupportedLanguage) | Some(SkipReason::SizeCeiling) | None => {
                    MetadataOnlyReason::UnsupportedTextEncoding
                }
            },
        },
    }
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

/// Walk up from `start` looking for a `.git`-bearing ancestor STRICTLY ABOVE
/// `start` (the directory itself is not considered). Returns the first such
/// non-forbidden ancestor, or `None` when there is no wider git root.
///
/// Used by `index_folder` to warn (without retargeting) when a caller indexes a
/// subfolder of a git repo, which shifts the path namespace off the repo root.
pub(crate) fn git_root_above(start: &Path) -> Option<PathBuf> {
    let mut current = start.parent()?.to_path_buf();
    loop {
        if current.join(".git").exists() && !is_forbidden_root(&current) {
            return Some(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return None,
        }
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

#[cfg(windows)]
fn is_device_or_special_namespace(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    let mut native: Vec<u16> = path.as_os_str().encode_wide().collect();
    for unit in &mut native {
        if (*unit >= b'A' as u16) && (*unit <= b'Z' as u16) {
            *unit += (b'a' - b'A') as u16;
        }
    }
    let starts_with = |prefix: &str| {
        let prefix: Vec<u16> = prefix.encode_utf16().collect();
        native.starts_with(&prefix)
    };

    starts_with(r"\\.\")
        || starts_with(r"\\?\globalroot\")
        || starts_with(r"\\?\device\")
        || starts_with(r"\device\")
        || starts_with(r"\??\")
}

#[cfg(unix)]
fn is_device_or_special_namespace(path: &Path) -> bool {
    let mut components = path.components();
    if !matches!(components.next(), Some(std::path::Component::RootDir)) {
        return false;
    }
    matches!(
        components.next(),
        Some(std::path::Component::Normal(component))
            if component == "dev" || component == "proc" || component == "sys"
    )
}

#[cfg(not(any(windows, unix)))]
fn is_device_or_special_namespace(_path: &Path) -> bool {
    false
}

#[cfg(unix)]
fn is_special_filesystem_entry(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;

    std::fs::metadata(path).is_ok_and(|metadata| {
        let kind = metadata.file_type();
        kind.is_block_device() || kind.is_char_device() || kind.is_fifo() || kind.is_socket()
    })
}

#[cfg(not(unix))]
fn is_special_filesystem_entry(_path: &Path) -> bool {
    false
}

/// Resolve one declared source root without selecting or touching project state.
pub fn resolve_root_candidate(
    candidate: &Path,
    source: RootCandidateSource,
    mode: RootRequestMode,
) -> RootResolution {
    resolve_root_candidate_with(
        candidate,
        source,
        mode,
        |path| path.is_dir(),
        |path| path.canonicalize(),
    )
}

fn resolve_root_candidate_with<D, C>(
    candidate: &Path,
    source: RootCandidateSource,
    mode: RootRequestMode,
    is_directory: D,
    canonicalize: C,
) -> RootResolution
where
    D: Fn(&Path) -> bool,
    C: Fn(&Path) -> std::io::Result<PathBuf>,
{
    let safe_path_id = || {
        format!(
            "path-{}",
            crate::hash::digest_hex(&native_path_identity_bytes(candidate))
        )
    };
    let unbound = |reason| RootResolution::Unbound {
        rejected_source: Some(source),
        reason: UnboundReason::Refused(reason),
        safe_path_id: Some(safe_path_id()),
    };

    if is_device_or_special_namespace(candidate) || is_special_filesystem_entry(candidate) {
        return unbound(RootRefusalReason::DeviceOrSpecialNamespace);
    }

    let raw_is_protected =
        crate::paths::is_sensitive_path(candidate) || is_forbidden_root(candidate);
    if raw_is_protected
        && !matches!(
            mode,
            RootRequestMode::ExplicitIndexFolder {
                allow_protected_root: true
            }
        )
    {
        return unbound(RootRefusalReason::ProtectedRootRequiresExplicitOverride);
    }
    if !is_directory(candidate) {
        return unbound(RootRefusalReason::MissingOrNotDirectory);
    }
    let canonical_root = match canonicalize(candidate) {
        Ok(root) => root,
        Err(_) => return unbound(RootRefusalReason::CanonicalizationFailed),
    };
    if is_device_or_special_namespace(&canonical_root)
        || is_special_filesystem_entry(&canonical_root)
    {
        return unbound(RootRefusalReason::DeviceOrSpecialNamespace);
    }
    let canonical_is_protected =
        crate::paths::is_sensitive_path(&canonical_root) || is_forbidden_root(&canonical_root);
    let class = if raw_is_protected || canonical_is_protected {
        RootClass::Protected
    } else {
        RootClass::Normal
    };

    let access_mode = match (class, mode) {
        (
            RootClass::Protected,
            RootRequestMode::ExplicitIndexFolder {
                allow_protected_root: true,
            },
        ) => SourceAccessMode::ExplicitProtected,
        (RootClass::Protected, _) => {
            return unbound(RootRefusalReason::ProtectedRootRequiresExplicitOverride);
        }
        (RootClass::Normal, _) => SourceAccessMode::NormalProject,
        (RootClass::NeverIndexable, _) => unreachable!("terminal roots return before binding"),
    };
    RootResolution::Bound(RootBinding {
        source,
        root_id: project_id_for_canonical_root(&canonical_root),
        canonical_root,
        access_mode,
    })
}

/// Derive the daemon/catalog identity for an already-canonical source root.
///
/// Keep this in the root-resolution layer so every consumer uses the same
/// platform-equivalence rule and cannot disagree about the user-local state
/// directory for one binding.
pub(crate) fn project_id_for_canonical_root(canonical_root: &Path) -> ProjectId {
    #[cfg(windows)]
    let platform_domain: &[u8] = b"windows";
    #[cfg(unix)]
    let platform_domain: &[u8] = b"unix";
    #[cfg(not(any(windows, unix)))]
    let platform_domain: &[u8] = b"other";
    let root_identity = canonical_root_identity_bytes(canonical_root);
    let mut digest_input = Vec::with_capacity(48 + root_identity.len());
    digest_input.extend_from_slice(b"symforge.project-state-root\0v1\0");
    digest_input.extend_from_slice(platform_domain);
    digest_input.push(0);
    digest_input.extend_from_slice(&(root_identity.len() as u64).to_le_bytes());
    digest_input.extend_from_slice(&root_identity);

    ProjectId(format!(
        "project-v1-{}",
        crate::hash::digest_hex(&digest_input)
    ))
}

#[cfg(windows)]
fn canonical_root_identity_bytes(canonical_root: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    // `Path::canonicalize` returns a verbatim `\\?\` path on Windows while
    // `dunce::canonicalize` deliberately returns the equivalent ordinary form.
    // Project identity must describe the directory, not which canonicalizer a
    // caller used, or binding and persistence can disagree about one root.
    let canonical_root = dunce::simplified(canonical_root);
    let mut identity = Vec::new();
    for decoded in std::char::decode_utf16(canonical_root.as_os_str().encode_wide()) {
        match decoded {
            Ok(character) => {
                let character = if character == '/' { '\\' } else { character };
                for folded in character.to_lowercase() {
                    identity.push(0);
                    identity.extend_from_slice(&u32::from(folded).to_le_bytes());
                }
            }
            Err(unpaired) => {
                identity.push(1);
                identity.extend_from_slice(&unpaired.unpaired_surrogate().to_le_bytes());
            }
        }
    }
    identity
}

#[cfg(not(windows))]
fn canonical_root_identity_bytes(canonical_root: &Path) -> Vec<u8> {
    let root_identity = native_path_identity_bytes(canonical_root);
    root_identity
}

pub(crate) fn resolve_state_placement_with<F>(
    binding: &RootBinding,
    user_local_directory: Option<PathBuf>,
    mut prepare: F,
) -> StatePlacement
where
    F: FnMut(&Path) -> std::result::Result<(), AccessErrorKind>,
{
    let mut failures = Vec::new();
    let user_reason = if binding.access_mode == SourceAccessMode::ExplicitProtected {
        UserLocalPlacementReason::ExplicitProtected
    } else {
        let project_directory = binding.canonical_root.join(".symforge");
        match prepare_state_directory_with(&project_directory, &mut prepare) {
            Ok(()) => {
                return StatePlacement::ProjectLocal {
                    directory: crate::domain::ProjectStateDir::new(project_directory),
                };
            }
            Err(safe_reason) => {
                failures.push(StateFailure {
                    location: StateLocationKind::ProjectLocal,
                    safe_reason,
                });
                UserLocalPlacementReason::ProjectLocalUnavailable { safe_reason }
            }
        }
    };

    if let Some(directory) = user_local_directory {
        match prepare_state_directory_with(&directory, &mut prepare) {
            Ok(()) => {
                return StatePlacement::UserLocal {
                    directory: crate::domain::ProjectStateDir::new(directory),
                    root_id: binding.root_id.clone(),
                    reason: user_reason,
                };
            }
            Err(safe_reason) => failures.push(StateFailure {
                location: StateLocationKind::UserLocal,
                safe_reason,
            }),
        }
    } else {
        failures.push(StateFailure {
            location: StateLocationKind::UserLocal,
            safe_reason: AccessErrorKind::NotFound,
        });
    }
    StatePlacement::MemoryOnly { failures }
}

fn prepare_state_directory_with<F>(
    directory: &Path,
    prepare: &mut F,
) -> std::result::Result<(), AccessErrorKind>
where
    F: FnMut(&Path) -> std::result::Result<(), AccessErrorKind>,
{
    match std::fs::symlink_metadata(directory) {
        Ok(metadata) => {
            if crate::paths::state_directory_metadata_is_unsafe(&metadata) {
                return Err(AccessErrorKind::InvalidData);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(access_error_kind(error.kind())),
    }

    prepare(directory)?;

    let metadata =
        std::fs::symlink_metadata(directory).map_err(|error| access_error_kind(error.kind()))?;
    if crate::paths::state_directory_metadata_is_unsafe(&metadata) {
        Err(AccessErrorKind::InvalidData)
    } else {
        Ok(())
    }
}

pub fn resolve_state_placement(binding: &RootBinding) -> StatePlacement {
    let user_local_directory = crate::paths::resolve_user_local_project_state_base()
        .ok()
        .map(|base| base.join("projects").join(&binding.root_id.0));
    resolve_state_placement_with(binding, user_local_directory, |directory| {
        std::fs::create_dir_all(directory).map_err(|error| access_error_kind(error.kind()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| access_error_kind(error.kind()))?;
        }
        Ok(())
    })
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
/// 1. `env_root` — the [`WORKSPACE_ROOT_ENV`] override (explicit operator intent).
/// 2. `root_uris` — MCP client-declared roots, in client order (the open workspace folder).
/// 3. `cwd_root` — the launch-CWD walk result from [`find_project_root`] (last resort).
///
/// Every candidate is validated through the SAME
/// [`validate_workspace_candidate`] guard, so no automatic source can push a
/// forbidden root (home dir, drive root, system path) past the trust boundary.
/// The first source that yields a usable directory wins; a forbidden or
/// unparseable candidate is skipped, not fatal.
///
/// `env_root` and `cwd_root` are passed pre-resolved (the caller owns reading
/// `WORKSPACE_ROOT_ENV` and walking the CWD). This function still validates
/// them at the final binding boundary, keeping the precedence logic testable
/// with a temp-dir fixture without trusting caller provenance.
pub fn resolve_workspace_root(
    env_root: Option<PathBuf>,
    root_uris: &[String],
    cwd_root: Option<PathBuf>,
) -> Option<PathBuf> {
    // 1. Explicit env override wins when it passes the shared binding guard.
    if let Some(candidate) = env_root {
        if let Some(root) = validate_workspace_candidate(&candidate, "workspace environment root") {
            return Some(root);
        }
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

    // 3. Launch-CWD walk, revalidated at the final binding boundary.
    cwd_root.and_then(|candidate| validate_workspace_candidate(&candidate, "launch CWD root"))
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

    // Heuristic 2: Invalid UTF-8. An INCOMPLETE multibyte sequence at the end
    // of a TRUNCATED window is a sampling artifact (the cut landed mid-char),
    // not binary evidence — `error_len() == None` means "unexpected end of
    // data", and more bytes exist beyond the window to complete the sequence.
    // (Dogfood 2026-07-11: the 8KB cut split a `─` in src/protocol/tools.rs
    // at byte 8190 and demoted 1.1 MB of pure-UTF-8 Rust to Tier 2 "binary".)
    if let Err(error) = std::str::from_utf8(window) {
        let boundary_cut = error.error_len().is_none() && check_len < content.len();
        if !boundary_cut {
            return true;
        }
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
/// 4. Metadata-only size threshold (1MB data / 4MB code) → Tier 2
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

/// Env var opting back INTO full indexing of untracked generated-output
/// directories (F5). Default OFF — when unset, files under an untracked
/// directory matching [`is_generated_output_dir_name`] are demoted to Tier-2
/// metadata-only. Set to a truthy value (`1`, `true`, `yes`, `on`) to restore
/// the previous behavior (full Tier-1 admission).
pub const INDEX_GENERATED_OUTPUT_ENV: &str = "SYMFORGE_INDEX_GENERATED_OUTPUT";

/// Returns `true` when the operator opted back into indexing untracked
/// generated-output dirs. Same truthy spellings as [`exclude_untracked_enabled`].
fn index_generated_output_enabled() -> bool {
    std::env::var(INDEX_GENERATED_OUTPUT_ENV)
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Conservative name heuristic for machine-generated output directories.
///
/// Deliberately small: only names that are overwhelmingly build/cache output in
/// practice (`dist`, `build`, `out`, `output`, `cache`, `.cache`, `generated`,
/// `*-out`, `*-output`). False positives are bounded by the tracked-file guard
/// in [`untracked_generated_output_demotions`]: a dir with ANY tracked file
/// beneath it is never demoted.
pub fn is_generated_output_dir_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "dist" | "build" | "out" | "output" | "cache" | ".cache" | "generated"
    ) || n.ends_with("-out")
        || n.ends_with("-output")
}

/// F5: compute the set of discovered file paths to demote to Tier-2 because
/// they live under an UNTRACKED generated-output directory.
///
/// Field-report motivation: a single untracked (not gitignored) JSON cache dump
/// (`graphify-out/cache`, 963 files) contributed 86% of a 327k-symbol index.
/// Untracked dirs whose name matches [`is_generated_output_dir_name`] are
/// machine output the operator never chose to version; demote their files to
/// Tier-2 metadata-only (path stays searchable, no symbol extraction).
///
/// Conservative guarantees:
/// - A directory with ANY git-tracked file beneath it is NEVER demoted
///   (tracked = the operator chose to version it; this also preserves the
///   SF-012(B) tracked-rescue contract, e.g. a committed `frontend/dist`).
/// - Non-git trees / unreadable git index → empty set (fail open: admit,
///   exactly the current behavior).
/// - `SYMFORGE_INDEX_GENERATED_OUTPUT=1` → empty set (explicit opt-in).
pub fn untracked_generated_output_demotions(
    root: &Path,
    entries: &[DiscoveredEntry],
) -> std::collections::HashSet<String> {
    if index_generated_output_enabled() {
        return std::collections::HashSet::new();
    }
    let Some(tracked) = tracked_path_set_for_build_dir_rescue(root) else {
        return std::collections::HashSet::new();
    };
    untracked_generated_output_demotions_inner(entries, &tracked)
}

/// Shallowest DIRECTORY component (never the file name) of `relative_path`
/// matching [`is_generated_output_dir_name`]; returns the prefix up to and
/// including that component. `None` for root-level files and paths with no
/// generated-looking directory. The ONE path-shape rule shared by the bulk
/// demotion walk and the watcher single-file parity check.
fn shallowest_generated_output_prefix(relative_path: &str) -> Option<&str> {
    let (dirs, _file) = relative_path.rsplit_once('/')?;
    let mut end = 0usize;
    for comp in dirs.split('/') {
        end += comp.len();
        if is_generated_output_dir_name(comp) {
            return Some(&relative_path[..end]);
        }
        end += 1; // the '/' separator
    }
    None
}

/// F5 watcher parity: does ONE relative path fall under the untracked
/// generated-output demotion policy? Same contract as the bulk
/// [`untracked_generated_output_demotions`] walk, evaluated per event:
///
/// - no generated-looking directory component → `false` (checked FIRST, pure
///   string work, so ordinary watcher events never touch git);
/// - `SYMFORGE_INDEX_GENERATED_OUTPUT` opt-in → `false`;
/// - no git repo / unreadable index / empty tracked set → `false` (fail open,
///   exactly like the bulk walk);
/// - the file itself is tracked → `false` (operator versioned it);
/// - ANY tracked file beneath the candidate prefix → `false` (prefix-wide
///   tracked rescue);
/// - otherwise → `true` (demote to Tier-2 `GeneratedOutput`).
pub(crate) fn is_untracked_generated_output_path(root: &Path, relative_path: &str) -> bool {
    let Some(candidate) = shallowest_generated_output_prefix(relative_path) else {
        return false;
    };
    if index_generated_output_enabled() {
        return false;
    }
    let Some(tracked) = tracked_path_set_for_build_dir_rescue(root) else {
        return false;
    };
    if tracked.contains(relative_path) {
        return false;
    }
    let prefix = format!("{candidate}/");
    !tracked.iter().any(|t| t.starts_with(&prefix))
}

/// Env-free core of [`untracked_generated_output_demotions`], unit-testable
/// without process-global state.
fn untracked_generated_output_demotions_inner(
    entries: &[DiscoveredEntry],
    tracked: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    use std::collections::{HashMap, HashSet};

    // Candidate-dir decision cache: prefix -> has any tracked file beneath it.
    // ponytail: O(candidates * tracked) linear scans; prefix trie if this shows
    // up in load profiles.
    let mut dir_has_tracked: HashMap<String, bool> = HashMap::new();
    let mut demoted: HashSet<String> = HashSet::new();

    for entry in entries {
        let rel = entry.relative_path.as_str();
        if tracked.contains(rel) {
            continue; // tracked file: never demoted by this policy
        }
        let Some(candidate) = shallowest_generated_output_prefix(rel) else {
            continue;
        };
        let has_tracked = *dir_has_tracked
            .entry(candidate.to_string())
            .or_insert_with(|| {
                let prefix = format!("{candidate}/");
                tracked.iter().any(|t| t.starts_with(&prefix))
            });
        if !has_tracked {
            demoted.insert(entry.relative_path.clone());
        }
    }
    demoted
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
        let paths: Vec<&str> = files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec![".gitignore", "main.rs"],
            "generic text admission must retain .gitignore while its rule excludes ignored.rs"
        );
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

    // ── Feature 020: repository-owned hidden knowledge vs hard scope ──
    mod hidden_path {
        use super::*;

        #[test]
        fn detects_only_vcs_and_runtime_state_components() {
            assert!(path_is_hard_scope_excluded(Path::new(
                ".symforge/index.bin"
            )));
            assert!(path_is_hard_scope_excluded(Path::new(
                "nested/.symforge/tee/edit.rs"
            )));
            assert!(path_is_hard_scope_excluded(Path::new(".git/objects/pack")));
            assert!(path_is_hard_scope_excluded(Path::new(
                "nested/.SYmFoRgE/state.rs"
            )));
        }

        #[test]
        fn allows_repository_owned_hidden_knowledge_and_traversal_segments() {
            assert!(!path_is_hard_scope_excluded(Path::new(
                ".github/workflows/ci.yml"
            )));
            assert!(!path_is_hard_scope_excluded(Path::new(".codex/AGENTS.md")));
            assert!(!path_is_hard_scope_excluded(Path::new(".travis.yml")));
            assert!(!path_is_hard_scope_excluded(Path::new("src/main.rs")));
            assert!(!path_is_hard_scope_excluded(Path::new(
                "../sibling/main.rs"
            )));
            assert!(!path_is_hard_scope_excluded(Path::new("src/a.b.c/main.rs")));
        }

        #[test]
        fn discover_all_files_includes_hidden_knowledge_but_excludes_runtime_internals() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), ".github/workflows/ci.yml", "name: ci\n");
            create_file(tmp.path(), ".travis.yml", "language: rust\n");
            create_file(tmp.path(), ".symforge/tee/snapshot.rs", "fn state() {}\n");
            create_file(tmp.path(), ".git/objects/fake.rs", "fn git_state() {}\n");
            create_file(tmp.path(), "src/main.rs", "fn main() {}");

            let entries = discover_all_files(tmp.path()).unwrap();
            let paths: Vec<&str> = entries.iter().map(|e| e.relative_path.as_str()).collect();

            assert!(
                paths.contains(&".github/workflows/ci.yml"),
                "repository-owned hidden knowledge must be discovered: {paths:?}"
            );
            assert!(
                paths.contains(&".travis.yml"),
                "repository-owned hidden files must be discovered: {paths:?}"
            );
            assert!(paths.contains(&"src/main.rs"));
            assert!(
                !paths.contains(&".symforge/tee/snapshot.rs")
                    && !paths.contains(&".git/objects/fake.rs"),
                "VCS/runtime internals must be pruned independently: {paths:?}"
            );
        }

        #[test]
        fn symforge_is_hard_excluded_under_every_state_placement() {
            let tmp = TempDir::new().unwrap();
            create_file(
                tmp.path(),
                ".gitignore",
                "!/.symforge/\n!/.symforge/**\n!/.git/\n!/.git/**\n",
            );
            create_file(tmp.path(), ".github/workflows/ci.yml", "name: ci\n");
            create_file(tmp.path(), ".codex/AGENTS.md", "# instructions\n");
            create_file(tmp.path(), "src/main.rs", "fn main() {}\n");

            // Stale/on-source artifacts representing each placement outcome must
            // remain outside source scope independently of ignore configuration.
            create_file(
                tmp.path(),
                ".symforge/project-local/index.rs",
                "fn project_state() {}\n",
            );
            create_file(
                tmp.path(),
                ".symforge/user-local/projects/project-v1/state.rs",
                "fn user_state() {}\n",
            );
            create_file(
                tmp.path(),
                "nested/.symforge/memory-only/stale.rs",
                "fn stale_state() {}\n",
            );
            create_file(tmp.path(), ".git/objects/fake.rs", "fn vcs_internal() {}\n");

            let plan = scout_repository(tmp.path()).expect("scout repository");
            let paths: Vec<&str> = plan
                .entries
                .iter()
                .filter_map(|entry| entry.path.normalized_utf8.as_deref())
                .collect();

            assert!(
                paths.contains(&".github/workflows/ci.yml") && paths.contains(&".codex/AGENTS.md"),
                "repository-owned hidden knowledge is the control proving this is a hard runtime-state exclusion, not a blanket hidden-path skip: {paths:?}"
            );
            assert!(
                paths.iter().all(|path| {
                    !path.split('/').any(|component| {
                        component.eq_ignore_ascii_case(".symforge")
                            || component.eq_ignore_ascii_case(".git")
                    })
                }),
                "VCS/runtime internals must stay outside the manifest for every placement: {paths:?}"
            );
            assert!(paths.contains(&"src/main.rs"));
        }
    }

    // ── SF-012(B): build-dir heuristic rescues tracked source dirs ──
    mod build_dir_tracked_rescue {
        use super::*;

        #[test]
        fn discover_all_files_rescues_tracked_target_specs_dir() {
            let tmp = TempDir::new().unwrap();
            let root = tmp.path();
            let run = |args: &[&str]| {
                crate::process_util::hidden_command("git")
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

    mod generated_output_demotion {
        use super::*;

        // Serializes the opt-in test (which mutates the process-global
        // SYMFORGE_INDEX_GENERATED_OUTPUT) against the sibling tests that read it via
        // `untracked_generated_output_demotions`. Without this, parallel cargo test
        // runs let the opt-in window leak into a reader and flip its expected tier.
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        fn init_git(root: &Path) -> impl Fn(&[&str]) + '_ {
            let run = move |args: &[&str]| {
                crate::process_util::hidden_command("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .expect("git command");
            };
            run(&["init"]);
            run(&["config", "user.email", "test@test.com"]);
            run(&["config", "user.name", "Test"]);
            run
        }

        #[test]
        fn generated_output_dir_name_matcher_is_conservative() {
            for name in [
                "dist",
                "build",
                "out",
                "output",
                "cache",
                ".cache",
                "generated",
                "graphify-out",
                "codegen-output",
                "Dist",
            ] {
                assert!(is_generated_output_dir_name(name), "{name} should match");
            }
            for name in ["src", "docs", "outline", "scout", "distros", "builder"] {
                assert!(!is_generated_output_dir_name(name), "{name} must NOT match");
            }
        }

        #[test]
        fn untracked_cache_dir_is_demoted_tracked_dist_is_not() {
            let _lock = ENV_LOCK.lock().unwrap();
            let tmp = TempDir::new().unwrap();
            let root = tmp.path();
            let run = init_git(root);

            // Tracked source + a TRACKED build-output dir (operator chose to
            // version it — SF-012(B) contract: never demote tracked files).
            create_file(root, "src/main.rs", "fn main() {}");
            create_file(root, "frontend/dist/bundle.js", "var x = 1;");
            run(&["add", "src", "frontend"]);
            run(&["commit", "-m", "initial"]);

            // The field-report shape: an UNTRACKED, non-gitignored JSON cache dump.
            create_file(root, "graphify-out/cache/a.json", "{\"k\": 1}");
            create_file(root, "graphify-out/cache/b.json", "{\"k\": 2}");
            // Untracked file in a NORMAL dir: not demoted by this policy.
            create_file(root, "src/new_module.rs", "fn newer() {}");

            let entries = discover_all_files(root).unwrap();
            let demoted = untracked_generated_output_demotions(root, &entries);

            assert!(
                demoted.contains("graphify-out/cache/a.json")
                    && demoted.contains("graphify-out/cache/b.json"),
                "untracked generated-output files must be demoted: {demoted:?}"
            );
            assert!(
                !demoted.contains("frontend/dist/bundle.js"),
                "tracked build output must NOT be demoted: {demoted:?}"
            );
            assert!(
                !demoted.contains("src/main.rs") && !demoted.contains("src/new_module.rs"),
                "normal source (tracked or not) must NOT be demoted: {demoted:?}"
            );
        }

        #[test]
        fn generated_dir_with_any_tracked_file_is_fully_admitted() {
            let _lock = ENV_LOCK.lock().unwrap();
            let tmp = TempDir::new().unwrap();
            let root = tmp.path();
            let run = init_git(root);

            // One tracked file inside `out/` protects the WHOLE dir, including
            // untracked siblings — when uncertain, admit.
            create_file(root, "out/kept.json", "{}");
            run(&["add", "out"]);
            run(&["commit", "-m", "initial"]);
            create_file(root, "out/generated.json", "{\"g\": true}");

            let entries = discover_all_files(root).unwrap();
            let demoted = untracked_generated_output_demotions(root, &entries);
            assert!(
                demoted.is_empty(),
                "a generated-output dir containing a tracked file must not be demoted: {demoted:?}"
            );
        }

        #[test]
        fn non_git_tree_demotes_nothing() {
            let _lock = ENV_LOCK.lock().unwrap();
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "dist/bundle.js", "var x = 1;");
            create_file(tmp.path(), "src/main.rs", "fn main() {}");

            let entries = discover_all_files(tmp.path()).unwrap();
            let demoted = untracked_generated_output_demotions(tmp.path(), &entries);
            assert!(
                demoted.is_empty(),
                "no git evidence → fail open, admit everything: {demoted:?}"
            );
        }

        #[test]
        fn opt_in_env_restores_full_indexing() {
            let _lock = ENV_LOCK.lock().unwrap();
            let tmp = TempDir::new().unwrap();
            let root = tmp.path();
            let run = init_git(root);
            create_file(root, "src/main.rs", "fn main() {}");
            run(&["add", "src"]);
            run(&["commit", "-m", "initial"]);
            create_file(root, "graphify-out/cache/a.json", "{}");

            let entries = discover_all_files(root).unwrap();

            // Env-free core demotes (proves the policy is live) …
            let tracked: std::collections::HashSet<String> =
                std::iter::once("src/main.rs".to_string()).collect();
            let demoted = untracked_generated_output_demotions_inner(&entries, &tracked);
            assert!(demoted.contains("graphify-out/cache/a.json"), "{demoted:?}");

            // … and the opt-in env gate empties the set. The module `ENV_LOCK`
            // (held above) serializes this writer against the sibling readers of
            // SYMFORGE_INDEX_GENERATED_OUTPUT; this guard restores the prior
            // value on drop, before the lock is released.
            struct EnvGuard(Option<std::ffi::OsString>);
            #[allow(unsafe_code)] // test-only env guard; sole writer of this var.
            impl Drop for EnvGuard {
                fn drop(&mut self) {
                    // SAFETY: single test mutating this var; restored on drop.
                    unsafe {
                        match &self.0 {
                            Some(v) => std::env::set_var(INDEX_GENERATED_OUTPUT_ENV, v),
                            None => std::env::remove_var(INDEX_GENERATED_OUTPUT_ENV),
                        }
                    }
                }
            }
            let _guard = EnvGuard(std::env::var_os(INDEX_GENERATED_OUTPUT_ENV));
            #[allow(unsafe_code)]
            // SAFETY: single test mutating this var; guard restores prior state.
            unsafe {
                std::env::set_var(INDEX_GENERATED_OUTPUT_ENV, "1")
            };

            let demoted = untracked_generated_output_demotions(root, &entries);
            assert!(
                demoted.is_empty(),
                "opt-in must restore full indexing: {demoted:?}"
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

    /// Dogfood 2026-07-11: the 8KB sniff window cut `src/protocol/tools.rs`
    /// (pure-UTF-8 Rust, 1.1 MB, box-drawing `─` chars) mid-multibyte
    /// sequence at byte 8190, and the resulting "unexpected end of data"
    /// decode error demoted the project's biggest source file to Tier 2 as
    /// "binary". An INCOMPLETE sequence at the truncation boundary is a
    /// sampling artifact, not binary evidence.
    #[test]
    fn test_binary_sniff_forgives_multibyte_cut_at_window_boundary() {
        let sniff = crate::domain::index::BINARY_SNIFF_BYTES;
        // Valid ASCII up to two bytes before the window edge, then a 3-byte
        // `─` (U+2500) straddling the cut, then more valid text beyond it.
        let mut content = vec![b'a'; sniff - 2];
        content.extend_from_slice("─".as_bytes());
        content.extend_from_slice(b" trailing source text beyond the sniff window");
        assert!(
            !is_binary_content(&content),
            "a multibyte char cut by the sniff window must not read as binary"
        );
    }

    /// Genuinely invalid bytes INSIDE the window (not a boundary cut) must
    /// still classify as binary — the boundary forgiveness is narrow.
    #[test]
    fn test_binary_sniff_still_detects_interior_invalid_utf8() {
        let sniff = crate::domain::index::BINARY_SNIFF_BYTES;
        let mut content = vec![b'a'; 100];
        content.push(0x80); // orphan continuation byte, interior
        content.extend(vec![b'b'; sniff]); // window extends well past it
        assert!(
            is_binary_content(&content),
            "interior invalid UTF-8 must stay classified as binary"
        );
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
            catalog_metadata_bytes_prev: Option<OsString>,
        }

        #[allow(unsafe_code)] // test-only env guard; mutation is serialized by ENV_LOCK.
        impl LimitEnvGuard {
            /// Set both limit env vars (any `None` clears that var) and capture
            /// the prior values for restoration on drop.
            fn set(
                max_files: Option<&str>,
                max_bytes: Option<&str>,
                max_catalog_metadata_bytes: Option<&str>,
            ) -> Self {
                let files_prev = std::env::var_os(MAX_INDEX_FILES_ENV);
                let bytes_prev = std::env::var_os(MAX_INDEX_BYTES_ENV);
                let catalog_metadata_bytes_prev = std::env::var_os(MAX_CATALOG_METADATA_BYTES_ENV);
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
                    match max_catalog_metadata_bytes {
                        Some(v) => std::env::set_var(MAX_CATALOG_METADATA_BYTES_ENV, v),
                        None => std::env::remove_var(MAX_CATALOG_METADATA_BYTES_ENV),
                    }
                }
                Self {
                    files_prev,
                    bytes_prev,
                    catalog_metadata_bytes_prev,
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
                    match &self.catalog_metadata_bytes_prev {
                        Some(v) => std::env::set_var(MAX_CATALOG_METADATA_BYTES_ENV, v),
                        None => std::env::remove_var(MAX_CATALOG_METADATA_BYTES_ENV),
                    }
                }
            }
        }

        #[test]
        fn default_limits_are_generous() {
            let limits = DiscoveryLimits::default();
            assert_eq!(limits.max_files, DEFAULT_MAX_INDEX_FILES);
            assert_eq!(limits.max_bytes, DEFAULT_MAX_INDEX_BYTES);
            assert_eq!(
                limits.max_catalog_metadata_bytes,
                DEFAULT_MAX_CATALOG_METADATA_BYTES
            );
            // 200k files is comfortably above a very large real monorepo.
            assert!(limits.max_files >= 200_000);
        }

        #[test]
        fn parse_positive_env_rejects_zero_empty_and_garbage() {
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(Some("0"), Some("not-a-number"), Some("0"));
            // Zero and non-numeric overrides are ignored, so the defaults stand —
            // a typo can never silently disable indexing.
            assert_eq!(parse_positive_env(MAX_INDEX_FILES_ENV), None);
            assert_eq!(parse_positive_env(MAX_INDEX_BYTES_ENV), None);
            assert_eq!(parse_positive_env(MAX_CATALOG_METADATA_BYTES_ENV), None);
            let limits = DiscoveryLimits::from_env();
            assert_eq!(limits.max_files, DEFAULT_MAX_INDEX_FILES);
            assert_eq!(limits.max_bytes, DEFAULT_MAX_INDEX_BYTES);
            assert_eq!(
                limits.max_catalog_metadata_bytes,
                DEFAULT_MAX_CATALOG_METADATA_BYTES
            );
        }

        #[test]
        fn from_env_honors_valid_override() {
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(Some("5"), Some("4096"), Some("8192"));
            let limits = DiscoveryLimits::from_env();
            assert_eq!(limits.max_files, 5);
            assert_eq!(limits.max_bytes, 4096);
            assert_eq!(limits.max_catalog_metadata_bytes, 8192);
        }

        #[test]
        fn normal_repo_indexes_under_default_cap() {
            // No env override: the generous default cap must not interfere with a
            // small, ordinary project.
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(None, None, None);
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
            let _guard = LimitEnvGuard::set(Some("2"), None, None);
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
            let _guard = LimitEnvGuard::set(Some("1000000"), Some("8"), None);
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
        fn scout_sparse_hard_skip_does_not_consume_ingest_budget() {
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(Some("1000000"), Some("1024"), None);
            let tmp = TempDir::new().unwrap();

            let sparse = tmp.path().join("huge.log");
            std::fs::File::create(&sparse)
                .unwrap()
                .set_len(HARD_SKIP_BYTES + 1)
                .unwrap();
            create_file(tmp.path(), "main.rs", "fn main() {}");

            let entries = discover_all_files(tmp.path())
                .expect("metadata-terminal hard skip must not consume admitted-byte budget");
            assert_eq!(entries.len(), 2);
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.relative_path == "huge.log")
            );
            assert!(entries.iter().any(|entry| entry.relative_path == "main.rs"));
        }

        #[test]
        fn catalog_entry_ceiling_never_publishes_false_complete_manifest() {
            let _lock = ENV_LOCK.lock().unwrap();
            let _guard = LimitEnvGuard::set(Some("1"), Some("1073741824"), None);
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "main.rs", "fn main() {}");
            create_file(tmp.path(), "model.gguf", "catalog only");

            let error = scout_repository(tmp.path())
                .expect_err("an over-cap candidate must not return a partial scout plan");
            assert!(error.to_string().contains("tree too large to index"));
            assert!(error.to_string().contains(MAX_INDEX_FILES_ENV));
        }

        #[test]
        fn catalog_metadata_budget_is_independent_and_never_publishes_partial_manifest() {
            let _lock = ENV_LOCK.lock().unwrap();
            let tmp = TempDir::new().unwrap();
            let artifact = tmp.path().join("model.gguf");
            std::fs::File::create(&artifact)
                .unwrap()
                .set_len(HARD_SKIP_BYTES + 1)
                .unwrap();

            let exact_usage = {
                let _guard = LimitEnvGuard::set(Some("10"), Some("1"), Some("1048576"));
                let plan = scout_repository(tmp.path())
                    .expect("metadata-terminal payload size must not consume catalog metadata");
                assert_eq!(plan.usage.catalog_entries, 1);
                assert_eq!(plan.usage.admitted_content_bytes, 0);
                assert!(plan.usage.catalog_metadata_bytes > 0);
                plan.usage.catalog_metadata_bytes
            };

            {
                let below_exact = (exact_usage - 1).to_string();
                let _guard = LimitEnvGuard::set(Some("10"), Some("1"), Some(&below_exact));
                let error = scout_repository(tmp.path())
                    .expect_err("metadata over-cap must return no partial scout plan");
                assert!(
                    error
                        .to_string()
                        .contains("catalog metadata capacity exceeded")
                );
                assert!(error.to_string().contains(MAX_CATALOG_METADATA_BYTES_ENV));
            }

            let exact = exact_usage.to_string();
            let _guard = LimitEnvGuard::set(Some("10"), Some("1"), Some(&exact));
            let plan = scout_repository(tmp.path())
                .expect("the exact canonical metadata byte ceiling must be accepted");
            assert_eq!(plan.usage.catalog_metadata_bytes, exact_usage);
            assert_eq!(plan.entries.len(), 1);
        }

        #[test]
        fn cold_start_budget_exhaustion_yields_distinct_typed_capacity_reasons() {
            let _lock = ENV_LOCK.lock().unwrap();

            let entry_limited = TempDir::new().unwrap();
            create_file(entry_limited.path(), "main.rs", "fn main() {}");
            create_file(entry_limited.path(), "README.md", "knowledge");
            let entry_error = {
                let _guard = LimitEnvGuard::set(Some("1"), Some("1073741824"), None);
                scout_repository(entry_limited.path())
                    .expect_err("entry exhaustion must publish no partial cold-start plan")
            };
            let entry_reason = entry_error
                .downcast_ref::<ScoutCapacityError>()
                .expect("entry exhaustion must remain a typed capacity refusal")
                .reason();

            let metadata_limited = TempDir::new().unwrap();
            create_file(metadata_limited.path(), "README.md", "knowledge");
            let exact_metadata_bytes = {
                let _guard = LimitEnvGuard::set(Some("10"), Some("1073741824"), None);
                scout_repository(metadata_limited.path())
                    .expect("calibration scout must fit")
                    .usage
                    .catalog_metadata_bytes
            };
            let metadata_error = {
                let below_exact = (exact_metadata_bytes - 1).to_string();
                let _guard = LimitEnvGuard::set(Some("10"), Some("1073741824"), Some(&below_exact));
                scout_repository(metadata_limited.path())
                    .expect_err("metadata exhaustion must publish no partial cold-start plan")
            };
            let metadata_reason = metadata_error
                .downcast_ref::<ScoutCapacityError>()
                .expect("metadata exhaustion must remain a typed capacity refusal")
                .reason();

            assert_eq!(
                entry_reason,
                crate::domain::FreshnessReason::CatalogEntryCapacityExceeded
            );
            assert_eq!(
                metadata_reason,
                crate::domain::FreshnessReason::CatalogMetadataCapacityExceeded
            );
            assert_ne!(entry_reason, metadata_reason);
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
                let _guard = LimitEnvGuard::set(Some("2"), None, None);
                assert!(discover_files(tmp.path()).is_err());
            }
            // Raised cap: accepted — the limit is genuinely configurable.
            {
                let _guard = LimitEnvGuard::set(Some("100"), None, None);
                let files = discover_files(tmp.path()).expect("raised cap indexes the tree");
                assert_eq!(files.len(), 4);
            }
        }
    }

    mod metadata_first_scout {
        use super::*;
        use crate::domain::{AccessErrorKind, CoverageStatus, ScoutIssueKind};
        use std::ffi::OsString;
        use std::io;

        #[cfg(unix)]
        fn opaque_relative_path(marker: u16) -> PathBuf {
            use std::os::unix::ffi::OsStringExt;

            let invalid = if marker & 1 == 0 { 0xff } else { 0xfe };
            PathBuf::from(OsString::from_vec(vec![
                b'o', b'p', b'a', b'q', b'u', b'e', invalid, b'.', b'r', b's',
            ]))
        }

        #[cfg(windows)]
        fn opaque_relative_path(marker: u16) -> PathBuf {
            use std::os::windows::ffi::OsStringExt;

            PathBuf::from(OsString::from_wide(&[
                b'o' as u16,
                b'p' as u16,
                b'a' as u16,
                b'q' as u16,
                b'u' as u16,
                b'e' as u16,
                marker,
                b'.' as u16,
                b'r' as u16,
                b's' as u16,
            ]))
        }

        #[test]
        fn scout_metadata_failure_retains_unavailable_entry_with_walk_stamp() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "main.rs", "fn main() {}");
            let expected_size = std::fs::metadata(tmp.path().join("main.rs")).unwrap().len();

            let plan = scout_repository_with_metadata(tmp.path(), |_path| {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected metadata failure",
                ))
            })
            .expect("per-entry metadata failure must degrade the plan, not abort it");

            assert_eq!(plan.entries.len(), 1);
            assert_eq!(
                plan.entries[0].path.normalized_utf8.as_deref(),
                Some("main.rs")
            );
            assert_eq!(plan.entries[0].stamp.size, expected_size);
            assert!(matches!(
                plan.entries[0].decision,
                ScoutDecision::Unavailable {
                    stage: crate::domain::AccessStage::Metadata,
                    kind: AccessErrorKind::PermissionDenied
                }
            ));
            assert_eq!(plan.coverage, CoverageStatus::Degraded);
            assert_eq!(plan.issues.len(), 1);
            assert_eq!(plan.issues[0].safe_path.as_deref(), Some("main.rs"));
            assert!(matches!(
                plan.issues[0].kind,
                ScoutIssueKind::DirectoryEntryUnreadable {
                    kind: AccessErrorKind::PermissionDenied
                }
            ));
        }

        #[test]
        fn walk_failure_retains_bounded_path_issue() {
            let tmp = TempDir::new().unwrap();
            let failed_path = tmp.path().join("blocked").join("child.rs");
            let issue = walk_issue_for_error(
                tmp.path(),
                Some(&failed_path),
                io::ErrorKind::PermissionDenied,
            );

            let plan = scout_entries_with_io(
                Vec::new(),
                |_path| -> io::Result<std::fs::Metadata> { unreachable!("no discovered entries") },
                |_path, _limit| -> io::Result<Vec<u8>> { unreachable!("no discovered entries") },
                vec![issue],
            )
            .expect("walk issue must remain a bounded degraded scout result");

            assert_eq!(plan.coverage, CoverageStatus::Degraded);
            assert!(plan.entries.is_empty());
            assert_eq!(plan.issues.len(), 1);
            assert_eq!(
                plan.issues[0].safe_path.as_deref(),
                Some("blocked/child.rs")
            );
            assert!(plan.issues[0].path_id.is_some());
            assert!(matches!(
                plan.issues[0].kind,
                ScoutIssueKind::DirectoryEntryUnreadable {
                    kind: AccessErrorKind::PermissionDenied
                }
            ));
        }

        #[test]
        fn scout_manifest_is_total_and_deterministically_sorted() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "zeta.rs", "pub fn zeta() {}");
            create_file(tmp.path(), "README.md", "# Read me");
            create_file(tmp.path(), "model.gguf", "catalog only");

            let first = scout_repository(tmp.path()).expect("first scout must succeed");
            let second = scout_repository(tmp.path()).expect("second scout must succeed");

            assert_eq!(first.coverage, CoverageStatus::Complete);
            assert!(first.issues.is_empty());
            assert_eq!(first.usage.catalog_entries, 3);
            assert_eq!(first.entries, second.entries);

            let paths = first
                .entries
                .iter()
                .map(|entry| entry.path.normalized_utf8.as_deref().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(paths, ["model.gguf", "README.md", "zeta.rs"]);

            assert!(matches!(
                first.entries[0].decision,
                ScoutDecision::MetadataOnly { .. }
            ));
            assert!(matches!(
                first.entries[1].decision,
                ScoutDecision::Ingest {
                    targets: IndexTargets::Knowledge
                }
            ));
            assert!(matches!(
                first.entries[2].decision,
                ScoutDecision::Ingest {
                    targets: IndexTargets::Code
                }
            ));
        }

        #[test]
        fn scout_uses_authoritative_target_routing_and_text_classification() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "guide.rst", "Guide\n=====\n");
            create_file(tmp.path(), "settings.toml", "enabled = true\n");
            create_file(tmp.path(), "page.html", "<main>hello</main>\n");

            let plan = scout_repository(tmp.path()).expect("scout must succeed");
            let entry = |path: &str| {
                plan.entries
                    .iter()
                    .find(|entry| entry.path.normalized_utf8.as_deref() == Some(path))
                    .unwrap_or_else(|| panic!("missing routed entry {path}"))
            };

            assert!(matches!(
                entry("guide.rst").decision,
                ScoutDecision::Ingest {
                    targets: IndexTargets::Knowledge
                }
            ));
            assert!(entry("guide.rst").classification.is_text());

            assert!(matches!(
                entry("settings.toml").decision,
                ScoutDecision::Ingest {
                    targets: IndexTargets::CodeAndKnowledge
                }
            ));
            assert!(entry("settings.toml").classification.is_code());
            assert!(entry("settings.toml").classification.is_config);

            assert!(matches!(
                entry("page.html").decision,
                ScoutDecision::Ingest {
                    targets: IndexTargets::Code
                }
            ));
        }

        #[test]
        fn sensitive_path_is_terminal_before_any_content_probe() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), ".env", "placeholder=true\n");

            let mut probed = Vec::new();
            let plan = scout_repository_with_io(
                tmp.path(),
                |path| std::fs::metadata(path),
                |path, _max_bytes| {
                    probed.push(path.to_path_buf());
                    Ok(Vec::new())
                },
            )
            .expect("sensitive paths must remain catalog-visible");

            let entry = plan
                .entries
                .iter()
                .find(|entry| entry.path.normalized_utf8.as_deref() == Some(".env"))
                .expect("sensitive path must remain in the scout catalog");
            assert!(matches!(
                &entry.decision,
                ScoutDecision::MetadataOnly {
                    reason: MetadataOnlyReason::SensitivePath { rule_id }
                } if rule_id == "path.environment-credentials"
            ));
            assert!(probed.is_empty(), "path policy must run before content I/O");
        }

        #[test]
        fn scout_binary_probe_never_exceeds_binary_sniff_bytes() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "notes.txt", "plain text");
            create_file(tmp.path(), "model.gguf", "catalog only");
            std::fs::File::create(tmp.path().join("huge.log"))
                .unwrap()
                .set_len(HARD_SKIP_BYTES + 1)
                .unwrap();

            let mut probes = Vec::new();
            let plan = scout_repository_with_io(
                tmp.path(),
                |path| std::fs::metadata(path),
                |path, max_bytes| {
                    probes.push((
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        max_bytes,
                    ));
                    Ok(Vec::new())
                },
            )
            .expect("bounded probing must produce a scout plan");

            assert_eq!(plan.entries.len(), 3);
            assert_eq!(
                probes,
                vec![(
                    "notes.txt".to_string(),
                    crate::domain::index::BINARY_SNIFF_BYTES
                )],
                "only undecided candidates may be probed, through the exact hard bound"
            );
        }

        #[test]
        fn scout_case_fold_pair_is_total_and_failure_is_per_entry() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "lower.rs", "pub fn lower() {}");
            create_file(tmp.path(), "upper.rs", "pub fn upper() {}");

            let lower_path = tmp.path().join("lower.rs");
            let candidates = vec![
                DiscoveredEntry {
                    relative_path: "a.rs".to_string(),
                    relative_os_path: PathBuf::from("a.rs"),
                    absolute_path: lower_path.clone(),
                    file_size: 0,
                    language: Some(LanguageId::Rust),
                    classification: FileClassification::for_code_path("a.rs"),
                },
                DiscoveredEntry {
                    relative_path: "A.rs".to_string(),
                    relative_os_path: PathBuf::from("A.rs"),
                    absolute_path: tmp.path().join("upper.rs"),
                    file_size: 0,
                    language: Some(LanguageId::Rust),
                    classification: FileClassification::for_code_path("A.rs"),
                },
            ];

            let complete = scout_entries_with_io(
                candidates.clone(),
                |path| std::fs::metadata(path),
                |_path, _max_bytes| Ok(Vec::new()),
                Vec::new(),
            )
            .expect("case-fold pair must remain scoutable");
            let ordered_paths = complete
                .entries
                .iter()
                .map(|entry| entry.path.normalized_utf8.as_deref().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(ordered_paths, ["A.rs", "a.rs"]);
            assert_ne!(
                complete.entries[0].path.public_id,
                complete.entries[1].path.public_id
            );
            assert_eq!(complete.coverage, CoverageStatus::Degraded);
            assert_eq!(complete.issues.len(), 2);
            assert!(
                complete
                    .issues
                    .iter()
                    .all(|issue| issue.kind == ScoutIssueKind::PathIdentityCollision)
            );
            assert_eq!(
                complete
                    .issues
                    .iter()
                    .filter_map(|issue| issue.safe_path.as_deref())
                    .collect::<Vec<_>>(),
                ["A.rs", "a.rs"]
            );

            let degraded = scout_entries_with_io(
                candidates,
                |path| {
                    if path == lower_path {
                        Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "injected per-entry failure",
                        ))
                    } else {
                        std::fs::metadata(path)
                    }
                },
                |_path, _max_bytes| Ok(Vec::new()),
                Vec::new(),
            )
            .expect("one failed case-fold peer must not abort the other");

            assert_eq!(degraded.coverage, CoverageStatus::Degraded);
            assert_eq!(degraded.entries.len(), 2);
            let unavailable = degraded
                .entries
                .iter()
                .find(|entry| entry.path.normalized_utf8.as_deref() == Some("a.rs"))
                .expect("metadata failure must retain the failed peer");
            assert!(matches!(
                unavailable.decision,
                ScoutDecision::Unavailable {
                    stage: crate::domain::AccessStage::Metadata,
                    kind: crate::domain::AccessErrorKind::PermissionDenied,
                }
            ));
            assert_ne!(
                degraded.entries[0].path.public_id,
                degraded.entries[1].path.public_id
            );
            assert_eq!(
                degraded
                    .issues
                    .iter()
                    .filter(|issue| matches!(
                        issue.kind,
                        ScoutIssueKind::DirectoryEntryUnreadable { .. }
                    ))
                    .count(),
                1
            );
            assert_eq!(
                degraded
                    .issues
                    .iter()
                    .filter(|issue| issue.kind == ScoutIssueKind::PathIdentityCollision)
                    .count(),
                2
            );
        }

        #[test]
        fn non_utf8_path_is_opaque_catalog_only_without_lossy_collision() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "first.rs", "pub fn first() {}");
            create_file(tmp.path(), "second.rs", "pub fn second() {}");

            let first_opaque = opaque_relative_path(0xd800);
            let second_opaque = opaque_relative_path(0xd801);
            assert_eq!(
                first_opaque.to_string_lossy(),
                second_opaque.to_string_lossy(),
                "fixture must prove two native identities collide under lossy conversion"
            );

            let candidates = vec![
                DiscoveredEntry {
                    relative_path: first_opaque.to_string_lossy().into_owned(),
                    relative_os_path: first_opaque,
                    absolute_path: tmp.path().join("first.rs"),
                    file_size: 0,
                    language: Some(LanguageId::Rust),
                    classification: FileClassification::for_code_path("first.rs"),
                },
                DiscoveredEntry {
                    relative_path: second_opaque.to_string_lossy().into_owned(),
                    relative_os_path: second_opaque,
                    absolute_path: tmp.path().join("second.rs"),
                    file_size: 0,
                    language: Some(LanguageId::Rust),
                    classification: FileClassification::for_code_path("second.rs"),
                },
            ];

            let mut probe_count = 0usize;
            let plan = scout_entries_with_io(
                candidates,
                |path| std::fs::metadata(path),
                |_path, _max_bytes| {
                    probe_count += 1;
                    Ok(Vec::new())
                },
                Vec::new(),
            )
            .expect("opaque paths must remain catalogable");

            assert_eq!(plan.entries.len(), 2);
            assert_eq!(probe_count, 0);
            assert!(
                plan.entries
                    .iter()
                    .all(|entry| entry.path.normalized_utf8.is_none())
            );
            assert_ne!(
                plan.entries[0].path.public_id,
                plan.entries[1].path.public_id
            );
            assert!(plan.entries.iter().all(|entry| matches!(
                entry.decision,
                ScoutDecision::MetadataOnly {
                    reason: MetadataOnlyReason::UnsupportedPathEncoding
                }
            )));
        }

        #[test]
        fn unsafe_or_oversized_path_is_opaque_without_retaining_spelling() {
            let tmp = TempDir::new().unwrap();
            create_file(tmp.path(), "payload.md", "# safe payload");
            let absolute_path = tmp.path().join("payload.md");
            let oversized = "x".repeat(MAX_CATALOG_SAFE_PATH_BYTES + 1);
            let candidates = vec![
                DiscoveredEntry {
                    relative_path: "unsafe".to_string(),
                    relative_os_path: PathBuf::from("line\nbreak.md"),
                    absolute_path: absolute_path.clone(),
                    file_size: 0,
                    language: None,
                    classification: FileClassification::for_code_path("payload.md"),
                },
                DiscoveredEntry {
                    relative_path: "oversized".to_string(),
                    relative_os_path: PathBuf::from(&oversized),
                    absolute_path,
                    file_size: 0,
                    language: None,
                    classification: FileClassification::for_code_path("payload.md"),
                },
            ];

            let mut probe_count = 0usize;
            let plan = scout_entries_with_io(
                candidates,
                |path| std::fs::metadata(path),
                |_path, _max_bytes| {
                    probe_count += 1;
                    Ok(Vec::new())
                },
                Vec::new(),
            )
            .expect("unsafe path metadata must remain catalogable by opaque ID");

            assert_eq!(probe_count, 0);
            assert_eq!(plan.entries.len(), 2);
            assert!(
                plan.entries
                    .iter()
                    .all(|entry| entry.path.normalized_utf8.is_none())
            );
            assert_ne!(
                plan.entries[0].path.public_id,
                plan.entries[1].path.public_id
            );
            assert!(plan.entries.iter().any(|entry| matches!(
                entry.decision,
                ScoutDecision::MetadataOnly {
                    reason: MetadataOnlyReason::UnsupportedPathEncoding
                }
            )));
            assert!(plan.entries.iter().any(|entry| matches!(
                entry.decision,
                ScoutDecision::MetadataOnly {
                    reason: MetadataOnlyReason::PathMetadataTooLarge
                }
            )));
            assert!(plan.entries.iter().all(|entry| {
                entry.path.normalized_utf8.as_deref() != Some(oversized.as_str())
            }));
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
        fn automatic_protected_roots_stay_unbound_before_source_or_project_state_io() {
            let tmp = TempDir::new().unwrap();
            let protected = tmp.path().join("System32");
            std::fs::create_dir(&protected).unwrap();
            let protected_uri = file_uri(&protected);

            assert!(
                resolve_workspace_root(Some(protected.clone()), &[], None).is_none(),
                "workspace environment candidates must pass the shared root gate"
            );
            assert!(
                resolve_workspace_root(None, &[protected_uri], None).is_none(),
                "MCP client candidates must pass the shared root gate"
            );
            assert!(
                resolve_workspace_root(None, &[], Some(protected.clone())).is_none(),
                "launch-CWD candidates must pass the shared root gate"
            );
            assert!(
                !protected.join(".symforge").exists(),
                "rejected automatic roots must not receive per-project state"
            );
        }

        #[test]
        fn device_or_uncanonicalizable_root_remains_nonindexable_with_override() {
            let override_mode = RootRequestMode::ExplicitIndexFolder {
                allow_protected_root: true,
            };
            let device_namespace = if cfg!(windows) {
                PathBuf::from(r"\\.\NUL")
            } else {
                PathBuf::from("/dev/null")
            };

            let device = resolve_root_candidate(
                &device_namespace,
                RootCandidateSource::ExplicitIndexFolder,
                override_mode,
            );
            assert!(
                matches!(
                    device,
                    RootResolution::Unbound {
                        reason: UnboundReason::Refused(RootRefusalReason::DeviceOrSpecialNamespace),
                        ..
                    }
                ),
                "explicit protected-root authority must never authorize a device namespace: {device:?}"
            );

            let ordinary = TempDir::new().unwrap();
            let uncanonicalizable = resolve_root_candidate_with(
                ordinary.path(),
                RootCandidateSource::ExplicitIndexFolder,
                override_mode,
                |_| true,
                |_| Err(std::io::Error::other("injected canonicalization failure")),
            );
            assert!(
                matches!(
                    uncanonicalizable,
                    RootResolution::Unbound {
                        reason: UnboundReason::Refused(RootRefusalReason::CanonicalizationFailed),
                        ..
                    }
                ),
                "override must not turn an unknown canonical identity into a binding: {uncanonicalizable:?}"
            );
        }

        #[cfg(windows)]
        #[test]
        fn windows_verbatim_and_simplified_roots_share_project_id() {
            let root = TempDir::new().unwrap();
            let verbatim = root.path().canonicalize().unwrap();
            let simplified = dunce::canonicalize(root.path()).unwrap();

            assert_eq!(
                project_id_for_canonical_root(&verbatim),
                project_id_for_canonical_root(&simplified),
                "equivalent Windows canonical path spellings must select one user-local state identity"
            );
        }

        #[test]
        fn explicit_protected_root_uses_user_local_then_memory_only() {
            let tmp = TempDir::new().unwrap();
            let protected = tmp.path().join("System32");
            std::fs::create_dir(&protected).unwrap();
            let RootResolution::Bound(binding) = resolve_root_candidate(
                &protected,
                RootCandidateSource::ExplicitIndexFolder,
                RootRequestMode::ExplicitIndexFolder {
                    allow_protected_root: true,
                },
            ) else {
                panic!("the modeled protected root should bind with direct authority");
            };
            let user_state = tmp.path().join("private-user-state");

            let placement =
                resolve_state_placement_with(&binding, Some(user_state.clone()), |candidate| {
                    assert_eq!(candidate, user_state);
                    std::fs::create_dir_all(candidate)
                        .map_err(|error| access_error_kind(error.kind()))
                });
            match placement {
                crate::domain::StatePlacement::UserLocal {
                    directory,
                    root_id,
                    reason: crate::domain::UserLocalPlacementReason::ExplicitProtected,
                } => {
                    assert_eq!(directory.as_path(), user_state);
                    assert_eq!(root_id, binding.root_id);
                }
                other => panic!("expected user-local placement, got {other:?}"),
            }
            assert!(!protected.join(".symforge").exists());

            let placement = resolve_state_placement_with(&binding, Some(user_state), |_| {
                Err(AccessErrorKind::PermissionDenied)
            });
            match placement {
                crate::domain::StatePlacement::MemoryOnly { failures } => {
                    assert_eq!(failures.len(), 1);
                    assert_eq!(
                        failures[0].location,
                        crate::domain::StateLocationKind::UserLocal
                    );
                    assert_eq!(failures[0].safe_reason, AccessErrorKind::PermissionDenied);
                }
                other => panic!("expected memory-only placement, got {other:?}"),
            }
            assert!(!protected.join(".symforge").exists());
        }

        #[test]
        fn readable_unwritable_project_relocates_state_without_retargeting_source() {
            let project = TempDir::new().unwrap();
            std::fs::write(project.path().join("lib.rs"), "pub fn readable() {}\n").unwrap();
            let RootResolution::Bound(binding) = resolve_root_candidate(
                project.path(),
                RootCandidateSource::ExplicitIndexFolder,
                RootRequestMode::ExplicitIndexFolder {
                    allow_protected_root: false,
                },
            ) else {
                panic!("ordinary readable project should bind");
            };
            let bound_source = binding.canonical_root.clone();
            let bound_id = binding.root_id.clone();
            let project_state = bound_source.join(".symforge");
            let user_base = TempDir::new().unwrap();
            let user_state = user_base.path().join("projects").join(&bound_id.0);
            let mut attempts = Vec::new();

            let placement =
                resolve_state_placement_with(&binding, Some(user_state.clone()), |candidate| {
                    attempts.push(candidate.to_path_buf());
                    if candidate == project_state {
                        Err(AccessErrorKind::PermissionDenied)
                    } else {
                        std::fs::create_dir_all(candidate)
                            .map_err(|error| access_error_kind(error.kind()))
                    }
                });

            assert_eq!(attempts, vec![project_state.clone(), user_state.clone()]);
            assert_eq!(binding.canonical_root, bound_source);
            assert_eq!(binding.root_id, bound_id);
            assert!(std::fs::read_to_string(bound_source.join("lib.rs")).is_ok());
            assert!(!project_state.exists());
            match placement {
                StatePlacement::UserLocal {
                    directory,
                    root_id,
                    reason:
                        UserLocalPlacementReason::ProjectLocalUnavailable {
                            safe_reason: AccessErrorKind::PermissionDenied,
                        },
                } => {
                    assert_eq!(directory.as_path(), user_state);
                    assert_eq!(root_id, binding.root_id);
                }
                other => panic!("expected user-local fallback, got {other:?}"),
            }
        }

        #[test]
        fn project_symforge_symlink_or_reparse_point_uses_global_without_following() {
            let project = TempDir::new().unwrap();
            std::fs::write(project.path().join("lib.rs"), "pub fn readable() {}\n").unwrap();
            let RootResolution::Bound(binding) = resolve_root_candidate(
                project.path(),
                RootCandidateSource::ExplicitIndexFolder,
                RootRequestMode::ExplicitIndexFolder {
                    allow_protected_root: false,
                },
            ) else {
                panic!("ordinary readable project should bind");
            };

            let outside = TempDir::new().unwrap();
            let sentinel = outside.path().join("sentinel");
            std::fs::write(&sentinel, b"outside state remains untouched\n").unwrap();
            let project_state = binding.canonical_root.join(".symforge");
            #[cfg(unix)]
            let link_result = std::os::unix::fs::symlink(outside.path(), &project_state);
            #[cfg(windows)]
            let link_result = std::os::windows::fs::symlink_dir(outside.path(), &project_state);
            let linked = if let Err(error) = link_result {
                #[cfg(windows)]
                {
                    assert_eq!(error.raw_os_error(), Some(1314));
                    std::fs::write(&project_state, b"unsafe state entry\n").unwrap();
                    false
                }
                #[cfg(not(windows))]
                panic!("test host must support directory symlinks: {error}");
            } else {
                true
            };

            let user_base = TempDir::new().unwrap();
            let user_state = user_base.path().join("projects").join(&binding.root_id.0);
            let placement =
                resolve_state_placement_with(&binding, Some(user_state.clone()), |candidate| {
                    std::fs::create_dir_all(candidate)
                        .map_err(|error| access_error_kind(error.kind()))?;
                    std::fs::write(candidate.join("prepared"), b"prepared\n")
                        .map_err(|error| access_error_kind(error.kind()))
                });

            assert_eq!(
                std::fs::read(&sentinel).unwrap(),
                b"outside state remains untouched\n"
            );
            assert!(!outside.path().join("prepared").exists());
            if !linked {
                assert!(crate::paths::state_directory_entry_type_is_unsafe(
                    true, false, true
                ));
                assert_eq!(
                    std::fs::read(&project_state).unwrap(),
                    b"unsafe state entry\n"
                );
            }
            assert_eq!(
                std::fs::read(user_state.join("prepared")).unwrap(),
                b"prepared\n"
            );
            match placement {
                StatePlacement::UserLocal {
                    directory,
                    root_id,
                    reason:
                        UserLocalPlacementReason::ProjectLocalUnavailable {
                            safe_reason: AccessErrorKind::InvalidData,
                        },
                } => {
                    assert_eq!(directory.as_path(), user_state);
                    assert_eq!(root_id, binding.root_id);
                }
                other => panic!("expected user-local fallback, got {other:?}"),
            }
        }

        #[test]
        fn root_state_key_coalesces_aliases_and_isolates_repos_and_worktrees() {
            fn bind(root: &Path) -> RootBinding {
                let RootResolution::Bound(binding) = resolve_root_candidate(
                    root,
                    RootCandidateSource::ExplicitIndexFolder,
                    RootRequestMode::ExplicitIndexFolder {
                        allow_protected_root: false,
                    },
                ) else {
                    panic!("ordinary root should bind: {}", root.display());
                };
                binding
            }

            let parent_a = TempDir::new().unwrap();
            let parent_b = TempDir::new().unwrap();
            let repo_a = parent_a.path().join("same-name");
            let repo_b = parent_b.path().join("same-name");
            let linked_worktree = parent_a.path().join("linked-worktree");
            for root in [&repo_a, &repo_b, &linked_worktree] {
                std::fs::create_dir(root).unwrap();
            }
            std::fs::write(
                linked_worktree.join(".git"),
                format!("gitdir: {}/.git/worktrees/linked\n", repo_a.display()),
            )
            .unwrap();

            let primary = bind(&repo_a);
            let alias = bind(&repo_a.join("."));
            let same_basename_other_repo = bind(&repo_b);
            let worktree = bind(&linked_worktree);

            assert_eq!(primary.canonical_root, alias.canonical_root);
            assert_eq!(primary.root_id, alias.root_id);
            assert_ne!(primary.root_id, same_basename_other_repo.root_id);
            assert_ne!(primary.root_id, worktree.root_id);
            assert!(primary.root_id.0.starts_with("project-v1-"));
            assert_eq!(primary.root_id.0.len(), "project-v1-".len() + 64);
            assert!(!primary.root_id.0.contains("same-name"));
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
