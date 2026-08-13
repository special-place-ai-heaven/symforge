//! Feature 020 V11 Slice 1 physical-root primitives (T023).
//!
//! As in the authority oracles, every refusal case is paired with the accepting
//! case so a blanket refusal cannot masquerade as containment.

use std::path::Path;

use symforge::live_index::index_lifecycle::physical_root::{
    PhysicalRootLease, ReplacementStep, RootRefusal, replace_beneath, stage_replacement,
};

/// TEST-PHYSICAL-ROOT (T023). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// "Stable" means the identity follows the lease, not the path: two leases on
/// the same directory are different roots, one lease keeps its identity for
/// life, and revocation does not recycle an identity onto a successor.
#[test]
fn canonical_physical_root_identity_is_stable() {
    let root = tempfile::tempdir().expect("temp root");

    let lease = PhysicalRootLease::take(root.path());
    let identity = lease.identity();

    // Stable for the life of the lease, and unchanged by revocation: a permit
    // holding this identity must not silently start matching a successor.
    assert_eq!(lease.identity(), identity);
    assert_eq!(lease.root(), root.path());
    lease.revoke();
    assert_eq!(
        lease.identity(),
        identity,
        "revocation recycled the root identity"
    );

    // A second lease on the SAME directory is a different root. Path equality
    // is not root identity; that is what stops a rebind from being mistaken for
    // the binding it replaced.
    let successor = PhysicalRootLease::take(root.path());
    assert_ne!(
        successor.identity(),
        identity,
        "a successor lease reused its predecessor's identity"
    );
    assert_eq!(successor.root(), lease.root());
    assert!(successor.is_live());
    assert!(!lease.is_live());

    // Distinct directories are distinct roots, so the identity is not derived
    // from the path either.
    let elsewhere = tempfile::tempdir().expect("other root");
    assert_ne!(
        PhysicalRootLease::take(elsewhere.path()).identity(),
        successor.identity()
    );
}

#[test]
fn resolution_stays_beneath_the_leased_root() {
    let root = tempfile::tempdir().expect("temp root");
    let lease = PhysicalRootLease::take(root.path());

    // Positive: an ordinary relative path resolves to a parent and leaf.
    let resolved = lease
        .resolve_beneath(Path::new("nested/leaf.txt"))
        .expect("an ordinary relative path resolves");
    assert_eq!(resolved.parent(), root.path().join("nested"));
    assert_eq!(resolved.leaf(), std::ffi::OsStr::new("leaf.txt"));

    // Negative: parent traversal and absolute paths escape and are refused.
    for escape in ["../outside.txt", "nested/../../outside.txt"] {
        let refusal = lease
            .resolve_beneath(Path::new(escape))
            .expect_err("parent traversal must not resolve");
        assert!(
            matches!(refusal, RootRefusal::EscapesRoot { .. }),
            "unexpected refusal for {escape}: {refusal:?}"
        );
    }

    let absolute = root.path().join("absolute.txt");
    let refusal = lease
        .resolve_beneath(&absolute)
        .expect_err("an absolute path must not resolve");
    assert!(matches!(refusal, RootRefusal::EscapesRoot { .. }));
}

#[test]
fn a_revoked_lease_resolves_nothing() {
    let root = tempfile::tempdir().expect("temp root");
    let lease = PhysicalRootLease::take(root.path());

    // Positive: it resolves while installed.
    lease
        .resolve_beneath(Path::new("leaf.txt"))
        .expect("an installed lease resolves");

    lease.revoke();

    // Negative: and nothing at all once revoked.
    assert_eq!(
        lease
            .resolve_beneath(Path::new("leaf.txt"))
            .expect_err("a revoked lease must refuse"),
        RootRefusal::LeaseRevoked
    );
}

/// The ordering is observed ON DISK, not read off the receipt.
///
/// The previous oracle asserted that `WriteReceipt` listed `TempCreated` before
/// `Replaced`, which only ever proved that the receipt records what the receipt
/// records: a build that renamed first while pushing the labels in order would
/// have passed it, and the mutation sweep would still have reported the guard
/// caught. Reviewer grok-4-5 found that hole. Staging the write makes the claim
/// checkable — with the stage held, the temporary must exist AND the target must
/// still hold its original bytes.
#[test]
fn the_target_still_holds_its_preimage_while_the_replacement_is_staged() {
    let root = tempfile::tempdir().expect("temp root");
    let lease = PhysicalRootLease::take(root.path());
    let target = Path::new("state.txt");
    std::fs::write(root.path().join(target), b"before").expect("seed target");

    let staged = symforge::live_index::index_lifecycle::physical_root::stage_replacement(
        &lease, target, b"after",
    )
    .expect("staging succeeds");

    // Observed on the filesystem, mid-flight: the replacement exists...
    assert!(
        staged.temp_path().exists(),
        "the staged content is not on disk"
    );
    assert_eq!(
        std::fs::read(staged.temp_path()).expect("read the staged content"),
        b"after".to_vec()
    );
    // ...and the target is untouched.
    assert_eq!(
        std::fs::read(root.path().join(target)).expect("read the target"),
        b"before".to_vec(),
        "the target was modified before its replacement was committed"
    );

    // Committing swaps it, and the temporary is gone afterwards.
    let temp = staged.temp_path().to_path_buf();
    let receipt = staged.commit().expect("commit succeeds");
    assert_eq!(
        std::fs::read(root.path().join(target)).expect("read back"),
        b"after".to_vec()
    );
    assert!(!temp.exists(), "the temporary survived the commit");
    assert_eq!(receipt.target(), root.path().join(target));
}

/// An abandoned stage leaves nothing behind.
#[test]
fn dropping_a_stage_without_committing_removes_its_temporary() {
    let root = tempfile::tempdir().expect("temp root");
    let lease = PhysicalRootLease::take(root.path());
    let target = Path::new("state.txt");
    std::fs::write(root.path().join(target), b"before").expect("seed target");

    let staged = symforge::live_index::index_lifecycle::physical_root::stage_replacement(
        &lease, target, b"after",
    )
    .expect("staging succeeds");
    let temp = staged.temp_path().to_path_buf();
    assert!(temp.exists());

    drop(staged);

    assert!(
        !temp.exists(),
        "an abandoned stage left its temporary behind"
    );
    assert_eq!(
        std::fs::read(root.path().join(target)).expect("read back"),
        b"before".to_vec(),
        "an abandoned stage modified the target"
    );
}

#[test]
fn replacement_creates_its_temporary_before_replacing() {
    let root = tempfile::tempdir().expect("temp root");
    let lease = PhysicalRootLease::take(root.path());
    let target = Path::new("state.txt");
    std::fs::write(root.path().join(target), b"before").expect("seed target");

    let receipt = replace_beneath(&lease, target, b"after").expect("replacement succeeds");

    assert_eq!(
        receipt.steps(),
        &[ReplacementStep::TempCreated, ReplacementStep::Replaced],
        "the temporary must exist before the target is replaced"
    );
    assert_eq!(
        std::fs::read(root.path().join(target)).expect("read back"),
        b"after".to_vec()
    );

    // No temporary survives the replacement.
    let leftovers: Vec<_> = std::fs::read_dir(root.path())
        .expect("list root")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("symforge-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary left behind: {leftovers:?}");
}

#[test]
fn replacement_through_a_revoked_lease_touches_nothing() {
    let root = tempfile::tempdir().expect("temp root");
    let lease = PhysicalRootLease::take(root.path());
    let target = Path::new("state.txt");
    std::fs::write(root.path().join(target), b"before").expect("seed target");

    // Positive: it replaces while installed.
    replace_beneath(&lease, target, b"after").expect("replacement succeeds while installed");

    lease.revoke();

    // Negative: and leaves the file untouched once revoked.
    assert_eq!(
        replace_beneath(&lease, target, b"later").expect_err("a revoked lease must refuse"),
        RootRefusal::LeaseRevoked
    );
    assert_eq!(
        std::fs::read(root.path().join(target)).expect("read back"),
        b"after".to_vec(),
        "a refused replacement must not have written"
    );
}

/// Symlink creation is unprivileged on Unix; CI runs there.
///
/// The never-follow policy is this userspace `symlink_metadata().is_symlink()`
/// walk, NOT `cap-std` — cap-std follows links that stay inside the capability
/// and refuses only those that escape it. On Windows `is_symlink` is true for
/// name-surrogate reparse tags, which covers symlinks and `mklink /J`
/// junctions, but not cloud-placeholder or WOF tags. So the claim this pins is
/// narrower than "reparse points are refused", and the Windows half now has its
/// own test below rather than a comment asserting equivalence.
#[cfg(unix)]
#[test]
fn a_link_component_is_refused_rather_than_followed() {
    let root = tempfile::tempdir().expect("temp root");
    let outside = tempfile::tempdir().expect("outside root");
    std::fs::write(outside.path().join("secret.txt"), b"outside").expect("seed outside");

    let lease = PhysicalRootLease::take(root.path());

    // Positive: a real directory beneath the root resolves.
    std::fs::create_dir(root.path().join("real")).expect("create real dir");
    lease
        .resolve_beneath(Path::new("real/leaf.txt"))
        .expect("a real directory component resolves");

    // Negative: a symlinked directory component is refused, not followed.
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).expect("create symlink");
    let refusal = lease
        .resolve_beneath(Path::new("escape/secret.txt"))
        .expect_err("a symlinked component must not be followed");
    assert!(
        matches!(refusal, RootRefusal::LinkComponent { .. }),
        "unexpected refusal: {refusal:?}"
    );

    // And the destructive path refuses through the same gate.
    let refusal = replace_beneath(&lease, Path::new("escape/secret.txt"), b"overwritten")
        .expect_err("a replacement through a link must refuse");
    assert!(matches!(refusal, RootRefusal::LinkComponent { .. }));
    assert_eq!(
        std::fs::read(outside.path().join("secret.txt")).expect("read outside"),
        b"outside".to_vec(),
        "the refused replacement must not have escaped the root"
    );
}

/// A stage taken before a revocation must not commit after it.
///
/// Found by adversarial review. `transition::apply` revokes the outgoing lease
/// "so no surviving permit can resolve a path under the replaced root", and the
/// two-step write introduced by this slice opened a window in the middle of that
/// ordering: `commit` held its own directory handle and never asked whether the
/// lease was still live, so it renamed happily after the revocation and returned
/// a receipt naming the retired lease.
#[test]
fn a_staged_replacement_refuses_to_commit_under_a_revoked_lease() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("target.txt"), b"original").expect("preimage");
    let lease = PhysicalRootLease::take(root.path());

    let staged = stage_replacement(&lease, Path::new("target.txt"), b"replacement")
        .expect("a live lease stages");
    let temp = staged.temp_path().to_path_buf();
    assert!(
        temp.exists(),
        "the stage must exist for the window to be real"
    );

    lease.revoke();

    assert_eq!(
        staged
            .commit()
            .expect_err("a revoked lease must not commit"),
        RootRefusal::LeaseRevoked
    );
    // The target still holds its preimage, and the abandoned stage cleaned up.
    assert_eq!(
        std::fs::read(root.path().join("target.txt")).expect("target survives"),
        b"original"
    );
    assert!(!temp.exists(), "a refused commit left its temporary behind");

    // Paired positive: on a live lease the identical sequence commits, so the
    // refusal is about revocation rather than about staging in general.
    let live = PhysicalRootLease::take(root.path());
    let staged = stage_replacement(&live, Path::new("target.txt"), b"replacement")
        .expect("a live lease stages");
    let receipt = staged.commit().expect("a live lease commits");
    assert_eq!(receipt.lease(), live.identity());
    assert_eq!(
        std::fs::read(root.path().join("target.txt")).expect("target replaced"),
        b"replacement"
    );
}

/// The Windows half of the same refusal, on a directory junction.
///
/// `mklink /J` is unprivileged, unlike `mklink /D`, so this runs without
/// Developer Mode or elevation. It is the one Windows reparse kind the policy
/// claims to catch: a name-surrogate tag that `symlink_metadata().is_symlink()`
/// reports true for. Until this existed, the Windows claim rested on a comment.
#[cfg(windows)]
#[test]
fn a_junction_component_is_refused_rather_than_followed() {
    let root = tempfile::tempdir().expect("temp root");
    let outside = tempfile::tempdir().expect("outside root");
    std::fs::write(outside.path().join("escaped.txt"), b"outside").expect("outside file");
    std::fs::create_dir(root.path().join("real")).expect("real dir");

    // Routed through `hidden_command` rather than built raw: a repo guard
    // requires every spawn to carry CREATE_NO_WINDOW so no console flashes, and
    // it is right to — a test that pops a window on every CI run is a test
    // nobody keeps.
    let junction = root.path().join("link");
    let status = symforge::process_util::hidden_command("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(outside.path())
        .output()
        .expect("mklink runs");
    assert!(
        status.status.success(),
        "mklink /J failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let lease = PhysicalRootLease::take(root.path());

    // Negative: a write through the junction is refused before any I/O, and the
    // file outside the root is untouched.
    let refusal = replace_beneath(&lease, Path::new("link/escaped.txt"), b"written")
        .expect_err("a junction component must be refused");
    assert!(
        matches!(refusal, RootRefusal::LinkComponent { .. }),
        "expected a link-component refusal, got {refusal:?}"
    );
    assert_eq!(
        std::fs::read(outside.path().join("escaped.txt")).expect("outside file survives"),
        b"outside",
        "a refused write reached through the junction"
    );

    // Positive: an ordinary directory beneath the same root still writes, so the
    // refusal is about the junction rather than about subdirectories.
    let receipt = replace_beneath(&lease, Path::new("real/ok.txt"), b"written")
        .expect("a real subdirectory writes");
    assert_eq!(receipt.lease(), lease.identity());
    assert_eq!(
        std::fs::read(root.path().join("real/ok.txt")).expect("written"),
        b"written"
    );
}
