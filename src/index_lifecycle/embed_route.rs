//! MCP `working_directory` edit routing (`crate::worktree::resolve_target_path`)
//! over host-admitted sources only. A requested directory is matched lexically
//! against admitted roots; nothing outside them is opened, and the worktree
//! relationship is read from each admitted root's own `.git` entry.

use std::path::{Component, Path};

use crate::embed::parity::edit::{
    AdmittedEditTarget, BatchRenamePlan, EditError, EditErrorKind, EditGuard, EditRoute,
    EditTarget, ResolvedEditTarget,
};

use super::embed_edit_body::RouteContext;

use super::activation::ProjectSourceAuthority;
use super::embed_query::EmbeddedQuerySnapshot;
use super::embedded::EmbeddedSourceHandle;

/// Bytes read from a `.git` gitfile or `HEAD`; both are one short line.
const GIT_ENTRY_MAX_BYTES: usize = 4096;

/// Lexical path identity: verbatim prefix stripped, `.`/`..` folded, and ASCII
/// case folded on Windows, where the filesystem is case-insensitive.
fn lexical(path: &Path) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in dunce::simplified(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    if cfg!(windows) {
        for part in &mut parts {
            part.make_ascii_lowercase();
        }
    }
    parts
}

fn same_path(left: &Path, right: &Path) -> bool {
    lexical(left) == lexical(right)
}

/// The repository's common git directory, read beneath the admitted root:
/// `<root>/.git` for a main worktree, or `<common>` from a linked worktree's
/// `gitdir: <common>/worktrees/<name>` file (git-worktree(1) layout).
fn git_common_dir(authority: &ProjectSourceAuthority, root: &Path) -> Option<Vec<String>> {
    if matches!(
        authority.read_regular_beneath_anchor(Path::new(".git/HEAD"), GIT_ENTRY_MAX_BYTES),
        Ok(Some(_))
    ) {
        return Some(lexical(&root.join(".git")));
    }
    let bytes = authority
        .read_regular_beneath_anchor(Path::new(".git"), GIT_ENTRY_MAX_BYTES)
        .ok()??;
    let gitdir = std::str::from_utf8(&bytes)
        .ok()?
        .trim()
        .strip_prefix("gitdir:")?
        .trim();
    let gitdir = root.join(gitdir);
    let worktrees = gitdir.parent()?;
    if worktrees.file_name()? != "worktrees" {
        return None;
    }
    Some(lexical(worktrees.parent()?))
}

/// Whether two admitted sources are worktrees of one repository, read from
/// each root's own `.git` entry.
pub(super) fn same_repository(left: &EmbeddedQuerySnapshot, right: &EmbeddedQuerySnapshot) -> bool {
    let left_common = git_common_dir(&left.authority, &left.root);
    left_common.is_some() && left_common == git_common_dir(&right.authority, &right.root)
}

impl EmbeddedSourceHandle {
    /// Resolve MCP's `working_directory` for an edit of `path` on this source.
    ///
    /// The bound root itself passes through. Any other directory must equal
    /// the root of one `admitted` source, which must be a worktree of the same
    /// repository and hold an admitted file at `path`; otherwise the edit is
    /// refused before anything is written. An unadmitted directory is never
    /// opened, so it refuses as `WorkingDirectoryNotAdmitted` whether or not it
    /// exists.
    pub fn route_edit<'a>(
        &self,
        path: &str,
        working_directory: &Path,
        admitted: &[AdmittedEditTarget<'a>],
    ) -> Result<EditRoute<'a>, EditError> {
        let snapshot = self.capture_query_snapshot(b"route-edit")?;
        crate::discovery::resolve_repo_path(&snapshot.root, path)
            .map_err(|_| EditError::Edit(EditErrorKind::InvalidPath))?
            .ok_or(EditError::Edit(EditErrorKind::FileNotAdmitted))?;
        if snapshot.generation.live.get_file(path).is_none() {
            return Err(EditError::Edit(EditErrorKind::FileNotAdmitted));
        }
        let bound_root = dunce::simplified(&snapshot.root).to_path_buf();
        let indexed_path = bound_root.join(path);
        let context = |rerouted: bool, rebased: bool, target_root: &Path| RouteContext {
            rerouted,
            rebased,
            bound_root: bound_root.clone(),
            target_root: target_root.to_path_buf(),
            working_directory: working_directory.to_path_buf(),
            bound_live: std::sync::Arc::clone(&snapshot.generation.live),
            // MCP leaves its index untouched for a write it routed elsewhere,
            // so the footer reads the bound copy as it stands.
            bound_impact: rerouted.then(|| {
                super::embed_edit_body::impact_footer(&snapshot, &snapshot.generation.live, path)
            }),
            bound_trust: super::embed_edit_body::trust_suffix(&snapshot),
        };
        if same_path(working_directory, &bound_root) {
            return Ok(EditRoute {
                target: None,
                resolved: ResolvedEditTarget {
                    working_directory: working_directory.to_path_buf(),
                    rerouted: false,
                    target_path: indexed_path.clone(),
                    indexed_path,
                },
                path: path.to_owned(),
                bound_root: snapshot.root.clone(),
                context: context(false, false, &bound_root),
            });
        }
        let target = admitted
            .iter()
            .find(|candidate| {
                candidate
                    .handle
                    .capture_health_context()
                    .is_some_and(|(_, root, _)| same_path(working_directory, &root))
            })
            .copied()
            .ok_or(EditError::Edit(EditErrorKind::WorkingDirectoryNotAdmitted))?;
        let target_snapshot = target.handle.capture_query_snapshot(b"route-edit-target")?;
        if target.authority.root != target_snapshot.root {
            return Err(EditError::Edit(EditErrorKind::WriteAuthorityRefused));
        }
        if !same_repository(&snapshot, &target_snapshot) {
            return Err(EditError::Edit(EditErrorKind::WorkingDirectoryNotAWorktree));
        }
        let target_file = match crate::discovery::resolve_repo_path(&target_snapshot.root, path) {
            Ok(Some(_)) => target_snapshot
                .generation
                .live
                .get_file(path)
                .ok_or(EditError::Edit(EditErrorKind::TargetFileMissing))?,
            Ok(None) => return Err(EditError::Edit(EditErrorKind::TargetFileMissing)),
            Err(_) => return Err(EditError::Edit(EditErrorKind::InvalidPath)),
        };
        // MCP's rebase: the target's bytes differ from the indexed copy's.
        let rebased = snapshot
            .generation
            .live
            .get_file(path)
            .is_none_or(|bound_file| bound_file.content != target_file.content);
        let target_root = dunce::simplified(&target_snapshot.root).to_path_buf();
        Ok(EditRoute {
            target: Some(target),
            resolved: ResolvedEditTarget {
                working_directory: working_directory.to_path_buf(),
                rerouted: true,
                target_path: target_root.join(path),
                indexed_path,
            },
            path: path.to_owned(),
            bound_root: snapshot.root.clone(),
            context: context(true, rebased, &target_root),
        })
    }

    /// Rebase a guard minted on this source onto the route's target, as MCP
    /// rebases a rerouted edit onto the worktree's current bytes: the symbol is
    /// re-selected in the target's own publication by name and kind, using the
    /// guard's line only to break an ambiguity. A pass-through route returns
    /// the guard unchanged after checking it is still current here.
    pub fn rebase_guard(
        &self,
        route: &EditRoute<'_>,
        guard: &EditGuard,
    ) -> Result<EditGuard, EditError> {
        self.check_routed_guard(route, guard)?;
        match route.target {
            None => Ok(guard.clone()),
            Some(target) => rebase_selector(guard, |selector| target.handle.edit_plan(selector))
                .map(|plan| EditGuard {
                    selector: guard.selector.clone(),
                    ..plan.guard
                }),
        }
    }

    /// [`Self::rebase_guard`] for a batch rename: the target re-plans the
    /// rename over its own references.
    pub fn rebase_rename_plan(
        &self,
        route: &EditRoute<'_>,
        plan: &BatchRenamePlan,
    ) -> Result<BatchRenamePlan, EditError> {
        self.check_routed_guard(route, &plan.guard)?;
        match route.target {
            None => Ok(plan.clone()),
            Some(target) => rebase_selector(&plan.guard, |selector| {
                target.handle.plan_batch_rename(selector, plan.code_only)
            }),
        }
    }

    fn check_routed_guard(
        &self,
        route: &EditRoute<'_>,
        guard: &EditGuard,
    ) -> Result<(), EditError> {
        let snapshot = self.capture_query_snapshot(b"rebase-edit-guard")?;
        if route.bound_root != snapshot.root || route.path != guard.path {
            return Err(EditError::Edit(EditErrorKind::InvalidPath));
        }
        super::embed_mutation::validate_guard(&snapshot, guard).map_err(EditError::Edit)?;
        Ok(())
    }
}

/// Select the guard's symbol by name and kind; fall back to its one-based line
/// only when the name alone is ambiguous in the target.
fn rebase_selector<T>(
    guard: &EditGuard,
    plan: impl Fn(&EditTarget) -> Result<T, EditError>,
) -> Result<T, EditError> {
    let mut selector = EditTarget {
        path: guard.path.clone(),
        name: guard.name.clone(),
        kind: Some(guard.kind.clone()),
        symbol_line: None,
    };
    match plan(&selector) {
        Err(EditError::Edit(EditErrorKind::AmbiguousSymbol { .. })) => {
            selector.symbol_line = Some(guard.symbol_line);
            plan(&selector)
        }
        other => other,
    }
}
