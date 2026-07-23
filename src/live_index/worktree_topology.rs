//! Linked-worktree classifier for Gate L (L-G01 foundation).
//!
//! Enumerates a repository's *linked* worktrees entirely in-process through
//! libgit2 (`git2`) — never a Git/LFS child process — and reports which branch
//! (if any) each has checked out. This is the topology input the ref-reconcile
//! driver uses to keep the contract invariant (`data-model.md:1258-1263`):
//! a checked-out linked worktree remains a SEPARATE `ProjectInstance` (its own
//! P0 lane) and must never be ingested as a P1 local-ref lane.

use std::path::PathBuf;

use git2::Repository;

/// One linked worktree of a repository and the branch it has checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedOutWorktree {
    /// The worktree name as registered in `.git/worktrees/<name>`.
    pub name: String,
    /// The worktree's working-directory path.
    pub path: PathBuf,
    /// The branch ref the worktree HEAD points at (`refs/heads/<branch>`), or
    /// `None` when the worktree HEAD is detached OR could not be resolved (see
    /// `head_resolved`).
    pub head_ref: Option<String>,
    /// Whether the worktree HEAD was actually read. `true` covers both a resolved
    /// branch (`head_ref = Some`) and a genuinely detached HEAD (`head_ref =
    /// None`). `false` means the worktree validated but its HEAD could NOT be
    /// resolved — we cannot prove which branch (if any) it holds, so reconcile
    /// must fail CLOSED rather than risk publishing a checked-out branch as a P1
    /// lane (L-G01: "checked-out worktrees are never P1").
    pub head_resolved: bool,
}

/// Classify a repository's linked worktrees in deterministic (name-sorted) order.
///
/// For each registered worktree we capture its path and open it via
/// `open_from_worktree` to read HEAD: `head_ref` is the branch refname when the
/// worktree HEAD is a branch, else `None` (detached). A worktree that fails
/// `validate()` (stale/pruned) is skipped rather than failing the whole call.
pub fn checked_out_worktrees(repository: &Repository) -> Result<Vec<CheckedOutWorktree>, String> {
    let names = repository
        .worktrees()
        .map_err(|err| format!("Error: repository worktrees are unavailable: {err}."))?;

    let mut out: Vec<CheckedOutWorktree> = Vec::new();
    for entry in names.iter() {
        // git2 yields `Result<Option<&str>, Error>`; a non-UTF-8 or errored
        // entry is skipped rather than failing the whole scan.
        let Ok(Some(name)) = entry else {
            continue;
        };
        let Ok(worktree) = repository.find_worktree(name) else {
            // Listed but unresolvable: skip, do not fail the whole scan.
            continue;
        };
        // A stale/pruned worktree carries no live P0 lane; skip it.
        if worktree.validate().is_err() {
            continue;
        }
        let path = worktree.path().to_path_buf();
        // Fail CLOSED: distinguish a genuinely detached HEAD (resolved, no branch)
        // from a HEAD we could not read at all. A validated worktree whose HEAD is
        // unreadable, or a branch HEAD whose name we cannot decode, is
        // `head_resolved = false` — reconcile treats that as an unprovable topology
        // and refuses to publish, so we never mistake a checked-out branch for a
        // bare one (L-G01).
        let (head_ref, head_resolved) = match Repository::open_from_worktree(&worktree) {
            Ok(repo) => match repo.head() {
                Ok(head) if head.is_branch() => match head.name() {
                    Ok(name) => (Some(name.to_string()), true),
                    Err(_) => (None, false),
                },
                Ok(_) => (None, true),   // detached HEAD: no branch to protect
                Err(_) => (None, false), // validated worktree, HEAD unreadable
            },
            Err(_) => (None, false), // validated worktree could not be opened
        };
        out.push(CheckedOutWorktree {
            name: name.to_string(),
            path,
            head_ref,
            head_resolved,
        });
    }

    out.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    fn init_repo(root: &Path) -> Repository {
        git2::Repository::init(root).expect("init repo")
    }

    fn commit_initial(root: &Path) {
        let repository = git2::Repository::open(root).expect("open repo");
        std::fs::write(root.join("src.rs"), b"pub fn a() {}\n").expect("write file");
        let mut index = repository.index().expect("index");
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .expect("stage");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repository.find_tree(tree_id).expect("tree");
        let signature =
            git2::Signature::now("SymForge Test", "symforge@example.invalid").expect("sig");
        repository
            .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .expect("commit");
    }

    /// Create a fresh branch and check it out in a linked worktree at `path`.
    fn add_worktree_on_branch(repository: &Repository, path: &Path, name: &str, branch: &str) {
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch(branch, &head_commit, false)
            .expect("branch");
        let reference = repository
            .find_reference(&format!("refs/heads/{branch}"))
            .expect("reference");
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        repository
            .worktree(name, path, Some(&opts))
            .expect("add worktree");
    }

    #[test]
    fn classifies_a_linked_worktree_on_a_branch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_initial(root);
        let repository = git2::Repository::open(root).expect("open");

        // The worktree lives in a sibling tempdir kept alive for the test.
        let wt_parent = tempfile::tempdir().expect("wt tempdir");
        let wt_path = wt_parent.path().join("feature-wt");
        add_worktree_on_branch(&repository, &wt_path, "feature-wt", "feature");

        let checked_out = checked_out_worktrees(&repository).expect("classify");
        assert_eq!(checked_out.len(), 1, "one linked worktree is classified");
        let wt = &checked_out[0];
        assert_eq!(wt.name, "feature-wt");
        assert_eq!(
            wt.head_ref.as_deref(),
            Some("refs/heads/feature"),
            "the worktree HEAD branch is captured"
        );
        assert!(wt.head_resolved, "a readable worktree HEAD is resolved");
        assert!(
            wt.path.ends_with("feature-wt"),
            "the worktree working directory is captured, got {:?}",
            wt.path
        );
    }

    #[test]
    fn classifies_empty_when_no_linked_worktrees() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_initial(root);
        let repository = git2::Repository::open(root).expect("open");

        let checked_out = checked_out_worktrees(&repository).expect("classify");
        assert!(
            checked_out.is_empty(),
            "a repo with no linked worktrees classifies empty"
        );
    }
}
