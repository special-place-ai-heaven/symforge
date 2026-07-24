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

/// A worktree's admin-HEAD classification, read from the git metadata directory
/// (`<commondir>/worktrees/<name>/HEAD`) when the LIVE worktree object is
/// unavailable (finding D3). Git's own branch-checkout protection reads this file,
/// so a branch recorded here is still "checked out" and must never become a P1 lane.
enum AdminHead {
    /// `ref: refs/heads/<branch>` — a branch is checked out here; protect it.
    Branch(String),
    /// The admin dir is gone: the worktree is genuinely pruned, nothing to protect.
    Pruned,
    /// The admin dir exists but HEAD is missing/unreadable/not a branch ref — we
    /// cannot prove which branch (if any) it holds; reconcile must fail closed.
    Unresolved,
}

/// Classify a worktree's admin HEAD for the moved/stale-but-not-pruned fallback
/// (finding D3). A moved-but-not-pruned worktree fails `validate()`, yet git still
/// refuses to check its branch out elsewhere because THIS admin HEAD names it — so
/// the branch is still protected and must be treated as checked out.
fn read_worktree_admin_head(repository: &Repository, name: &str) -> AdminHead {
    let admin_dir = repository.commondir().join("worktrees").join(name);
    if !admin_dir.is_dir() {
        return AdminHead::Pruned;
    }
    let Ok(contents) = std::fs::read_to_string(admin_dir.join("HEAD")) else {
        return AdminHead::Unresolved;
    };
    match contents.trim().strip_prefix("ref:") {
        Some(refname) if !refname.trim().is_empty() => {
            AdminHead::Branch(refname.trim().to_string())
        }
        _ => AdminHead::Unresolved,
    }
}

/// Classify a repository's linked worktrees in deterministic (name-sorted) order.
///
/// For each registered worktree we capture its path and read its HEAD. The LIVE
/// worktree object is preferred; when it is missing (`find_worktree` failed) or
/// stale (`validate` failed — e.g. MOVED but not pruned) we fall back to the admin
/// HEAD file, git's own branch-checkout protection source (finding D3). A branch
/// found there is still checked out and is classified as such; only a genuinely
/// PRUNED worktree (admin dir gone) is skipped. A worktree we cannot even NAME, or
/// whose HEAD is unreadable at every level, is recorded `head_resolved = false` so
/// reconcile fails CLOSED rather than fail-OPEN publishing a checked-out branch as
/// a P1 lane (L-G01).
pub fn checked_out_worktrees(repository: &Repository) -> Result<Vec<CheckedOutWorktree>, String> {
    let names = repository
        .worktrees()
        .map_err(|err| format!("Error: repository worktrees are unavailable: {err}."))?;

    let mut out: Vec<CheckedOutWorktree> = Vec::new();
    for entry in names.iter() {
        // git2 yields `Result<Option<&str>, Error>`; a worktree we cannot even
        // NAME (non-UTF-8 or errored listing) cannot be identified or have its
        // admin HEAD read — record it UNRESOLVED so reconcile fails CLOSED rather
        // than silently ignore a possibly-checked-out branch (finding D3).
        let Ok(Some(name)) = entry else {
            out.push(CheckedOutWorktree {
                name: "<unnameable-worktree>".to_string(),
                path: PathBuf::new(),
                head_ref: None,
                head_resolved: false,
            });
            continue;
        };

        // Prefer the LIVE worktree object; fall back to the admin HEAD when it is
        // missing or stale (moved-but-not-pruned).
        let live = repository
            .find_worktree(name)
            .ok()
            .filter(|worktree| worktree.validate().is_ok());
        let (path, head_ref, head_resolved) = match live {
            Some(worktree) => {
                let path = worktree.path().to_path_buf();
                // Fail CLOSED: distinguish a genuinely detached HEAD (resolved, no
                // branch) from a HEAD we could not read at all.
                let (head_ref, head_resolved) = match Repository::open_from_worktree(&worktree) {
                    Ok(repo) => match repo.head() {
                        Ok(head) if head.is_branch() => match head.name() {
                            Ok(name) => (Some(name.to_string()), true),
                            Err(_) => (None, false),
                        },
                        Ok(_) => (None, true), // detached HEAD: no branch to protect
                        Err(_) => (None, false), // validated worktree, HEAD unreadable
                    },
                    Err(_) => (None, false), // validated worktree could not be opened
                };
                (path, head_ref, head_resolved)
            }
            None => match read_worktree_admin_head(repository, name) {
                // Genuinely pruned (admin dir gone): no live P0 lane; nothing to skip.
                AdminHead::Pruned => continue,
                // A branch is still recorded → protect it. ponytail: the live
                // worktree path is gone; an empty path is honest — a moved/
                // unresolvable worktree cannot be P0-routed anyway. Read
                // `<name>/gitdir` if downstream routing ever needs the real path.
                AdminHead::Branch(refname) => (PathBuf::new(), Some(refname), true),
                // Admin HEAD present but unreadable/not a branch → cannot prove the
                // held branch → fail closed.
                AdminHead::Unresolved => (PathBuf::new(), None, false),
            },
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

    #[test]
    fn moved_but_unpruned_worktree_is_classified_from_admin_head() {
        // Finding D3: a worktree MOVED but not pruned fails `validate()`, yet its
        // admin HEAD (which git's branch-checkout protection reads) still records
        // the branch. It must be classified checked-out from the admin HEAD, not
        // fail-OPEN skipped — else its branch would wrongly be published as a bare
        // P1 lane.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_initial(root);
        let repository = git2::Repository::open(root).expect("open");

        let wt_parent = tempfile::tempdir().expect("wt tempdir");
        let wt_path = wt_parent.path().join("moved-wt");
        add_worktree_on_branch(&repository, &wt_path, "moved-wt", "moved");
        // Move (here: remove) the working directory so `validate()` fails while the
        // admin HEAD at <commondir>/worktrees/moved-wt/HEAD stays intact.
        std::fs::remove_dir_all(&wt_path).expect("remove worktree working dir");

        let checked_out = checked_out_worktrees(&repository).expect("classify");
        let moved = checked_out
            .iter()
            .find(|w| w.name == "moved-wt")
            .expect("the moved worktree is still classified, not skipped");
        assert_eq!(
            moved.head_ref.as_deref(),
            Some("refs/heads/moved"),
            "the branch is read from the admin HEAD fallback"
        );
        assert!(
            moved.head_resolved,
            "an admin-HEAD branch is a resolved (protected) checkout"
        );
    }
}
