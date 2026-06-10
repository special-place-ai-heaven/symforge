//! Acceptance tests for the full admission tier system end-to-end.
//!
//! Covers all three tiers across realistic file layouts:
//!   Tier 1 (Normal / indexed)   — source files that produce symbols
//!   Tier 2 (MetadataOnly)       — denylisted extensions, >1 MB, binary content
//!   Tier 3 (HardSkip)           — >100 MB (tested via classify_admission directly)

use std::fs;
use std::path::Path;
use symforge::live_index::LiveIndex;
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn write_file(dir: &Path, name: &str, content: &[u8]) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

// ---------------------------------------------------------------------------
// Test 1: full pipeline — Tier 1 indexed, Tier 2 skipped, counts correct
// ---------------------------------------------------------------------------

/// Exercises all three admission tiers across realistic file layouts.
///
/// Tier 1: normal Rust/Markdown/TOML source files  → indexed, symbols extracted
/// Tier 2: denylisted extensions, >1 MB, binary    → MetadataOnly skip
/// Tier 3: not exercised at runtime (files would be >100 MB); see Test 2.
#[test]
fn test_admission_tier_acceptance() {
    let dir = tempdir().unwrap();

    // ── Tier 1: normal source files ──────────────────────────────────────
    write_file(dir.path(), "src/main.rs", b"fn main() {}\n");
    write_file(dir.path(), "src/lib.rs", b"pub fn helper() -> i32 { 42 }\n");
    write_file(dir.path(), "src/utils/mod.rs", b"pub struct Config;\n");
    write_file(dir.path(), "README.md", b"# Project\n");
    write_file(dir.path(), "config.toml", b"[settings]\nkey = \"value\"\n");

    // ── Tier 2: denylisted extensions ────────────────────────────────────
    write_file(dir.path(), "models/v1.safetensors", b"fake model");
    write_file(dir.path(), "models/v2.ckpt", b"fake checkpoint");
    write_file(dir.path(), "assets/logo.png", b"fake png");
    write_file(dir.path(), "assets/font.woff2", b"fake font");
    write_file(dir.path(), "backups/data.sqlite3", b"fake db");
    write_file(dir.path(), "release.zip", b"fake archive");

    // ── Tier 2: size threshold (>1 MB) ───────────────────────────────────
    // Write 1.5 MB of repeated ASCII — text content so no binary sniff
    let big_content = b"x".repeat(1_500_000);
    write_file(dir.path(), "data/big_config.json", &big_content);

    // ── Tier 2: binary content (not denylisted, contains NUL bytes) ───────
    let binary_content: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48];
    write_file(dir.path(), "data/custom.dat", &binary_content);

    // ── Load index ────────────────────────────────────────────────────────
    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    // ── Verify tier counts ────────────────────────────────────────────────
    let (tier1, tier2, tier3) = index.tier_counts();

    // Tier 1: src/main.rs, src/lib.rs, src/utils/mod.rs, README.md, config.toml = 5
    assert_eq!(
        tier1,
        5,
        "expected 5 Tier-1 (indexed) files, got {tier1}; skipped={:?}",
        index
            .skipped_files()
            .iter()
            .map(|sf| (&sf.path, sf.reason()))
            .collect::<Vec<_>>()
    );

    // Tier 2: 6 denylisted + 1 big + 1 binary = 8
    assert_eq!(
        tier2, 8,
        "expected 8 Tier-2 (MetadataOnly) files, got {tier2}"
    );

    // Tier 3: none created at runtime
    assert_eq!(tier3, 0, "expected 0 Tier-3 (HardSkip) files, got {tier3}");

    // ── Verify symbols come from Tier-1 files only ────────────────────────
    assert!(
        index.symbol_count() > 0,
        "Rust files should produce symbols (main, helper, Config)"
    );

    // None of the skipped files should appear as indexed files
    let skipped_paths: Vec<String> = index
        .skipped_files()
        .iter()
        .map(|sf| sf.path.replace('\\', "/"))
        .collect();

    // Normalise path separators for comparison (index uses forward slashes)
    for path in [
        "models/v1.safetensors",
        "models/v2.ckpt",
        "assets/logo.png",
        "assets/font.woff2",
        "backups/data.sqlite3",
        "release.zip",
        "data/big_config.json",
        "data/custom.dat",
    ] {
        assert!(
            skipped_paths.iter().any(|p: &String| p.ends_with(path)),
            "expected {path} in skipped_files but it was missing; skipped={skipped_paths:?}"
        );
        assert!(
            index.get_file(path).is_none(),
            "skipped file {path} must not appear as an indexed (Tier-1) file"
        );
    }

    // ── Verify skip reasons ───────────────────────────────────────────────
    use symforge::domain::index::SkipReason;

    // A denylisted extension
    let ckpt = index
        .skipped_files()
        .iter()
        .find(|sf| sf.path.replace('\\', "/").ends_with("models/v2.ckpt"))
        .expect("models/v2.ckpt should be in skipped_files");
    assert_eq!(
        ckpt.reason(),
        Some(SkipReason::DenylistedExtension),
        "models/v2.ckpt should be skipped with DenylistedExtension"
    );

    // The oversized file
    let big = index
        .skipped_files()
        .iter()
        .find(|sf| sf.path.replace('\\', "/").ends_with("data/big_config.json"))
        .expect("data/big_config.json should be in skipped_files");
    assert_eq!(
        big.reason(),
        Some(SkipReason::SizeThreshold),
        "data/big_config.json (1.5 MB) should be skipped with SizeThreshold"
    );

    // The binary file
    let bin = index
        .skipped_files()
        .iter()
        .find(|sf| sf.path.replace('\\', "/").ends_with("data/custom.dat"))
        .expect("data/custom.dat should be in skipped_files");
    assert_eq!(
        bin.reason(),
        Some(SkipReason::BinaryContent),
        "data/custom.dat (contains NUL) should be skipped with BinaryContent"
    );
}

// ---------------------------------------------------------------------------
// Test 1b: admission-tier assignment is stable across a reindex
//
// The live index does NOT persist admission tier: `IndexSnapshot` only carries
// Tier-1 indexed files, and `snapshot_to_live_index` resets `skipped_files` to
// empty (a snapshot restore re-derives tiers on the next reconcile). Tier
// assignment is therefore recomputed by the admission gate on every fresh
// `LiveIndex::load` AND on every `build_reload_data` (the reload / `index_folder`
// path now runs the same admission pipeline). This test asserts that the
// recomputation is deterministic — two back-to-back loads of the same directory
// tree must produce byte-identical tier assignments and skip reasons for every
// file, so `health` output stays honest after a cold start or a manual reindex.
// ---------------------------------------------------------------------------

/// Flat, comparable snapshot of the admission-gate output for one load.
/// `(tier1_indexed_paths, tier2_by_path, tier3_by_path)`.
type TierSnapshot = (
    Vec<String>,
    std::collections::BTreeMap<String, symforge::domain::index::SkipReason>,
    std::collections::BTreeMap<String, symforge::domain::index::SkipReason>,
);

fn snapshot_tiers(index: &symforge::live_index::LiveIndex) -> TierSnapshot {
    use std::collections::BTreeMap;
    use symforge::domain::index::{AdmissionTier, SkipReason};

    let mut tier1: Vec<String> = index
        .all_files()
        .map(|(p, _)| p.replace('\\', "/"))
        .collect();
    tier1.sort();

    let mut tier2: BTreeMap<String, SkipReason> = BTreeMap::new();
    let mut tier3: BTreeMap<String, SkipReason> = BTreeMap::new();

    for sf in index.skipped_files() {
        let path = sf.path.replace('\\', "/");
        let reason = sf
            .reason()
            .expect("a skipped file must always carry a SkipReason");
        match sf.tier() {
            AdmissionTier::MetadataOnly => {
                tier2.insert(path, reason);
            }
            AdmissionTier::HardSkip => {
                tier3.insert(path, reason);
            }
            AdmissionTier::Normal => {
                panic!("Tier-1 file {path} leaked into skipped_files");
            }
        }
    }

    (tier1, tier2, tier3)
}

#[test]
fn test_admission_tiers_stable_across_reindex() {
    use symforge::domain::index::SkipReason;

    let dir = tempdir().unwrap();

    // ── Tier 1: normal source files ──────────────────────────────────────
    write_file(dir.path(), "src/main.rs", b"fn main() {}\n");
    write_file(dir.path(), "src/lib.rs", b"pub fn helper() -> i32 { 42 }\n");
    write_file(dir.path(), "README.md", b"# Project\n");

    // ── Tier 2: one file per SkipReason ──────────────────────────────────
    write_file(dir.path(), "assets/logo.png", b"fake png");
    write_file(dir.path(), "data/big.json", &b"x".repeat(1_500_000));
    write_file(
        dir.path(),
        "data/custom.dat",
        &[0x89, 0x50, 0x4E, 0x47, 0x00, 0x00, 0x00, 0x0D],
    );

    // ── First load ────────────────────────────────────────────────────────
    let snap1 = {
        let shared = LiveIndex::load(dir.path()).unwrap();
        snapshot_tiers(&shared.read())
    };

    // ── Reindex: load again from the same tree ───────────────────────────
    let snap2 = {
        let shared = LiveIndex::load(dir.path()).unwrap();
        snapshot_tiers(&shared.read())
    };

    assert_eq!(
        snap1, snap2,
        "admission tier assignments must be stable across a reindex \
         (recompute stability is the only guarantee — tier is not persisted)"
    );

    // ── Sanity: the snapshot describes what we actually created ──────────
    let (tier1, tier2, tier3) = &snap1;
    assert_eq!(
        tier1.len(),
        3,
        "expected exactly 3 Tier-1 files, got {tier1:?}"
    );
    assert!(tier1.iter().any(|p| p.ends_with("src/main.rs")));
    assert!(tier1.iter().any(|p| p.ends_with("src/lib.rs")));
    assert!(tier1.iter().any(|p| p.ends_with("README.md")));

    assert_eq!(tier2.len(), 3, "expected exactly 3 Tier-2 files");
    assert_eq!(tier3.len(), 0, "no Tier-3 files were created in this test");

    let by_suffix = |suffix: &str| -> SkipReason {
        *tier2
            .iter()
            .find(|(p, _)| p.ends_with(suffix))
            .unwrap_or_else(|| panic!("Tier-2 missing {suffix}: {tier2:?}"))
            .1
    };
    assert_eq!(
        by_suffix("assets/logo.png"),
        SkipReason::DenylistedExtension
    );
    assert_eq!(by_suffix("data/big.json"), SkipReason::SizeThreshold);
    assert_eq!(by_suffix("data/custom.dat"), SkipReason::BinaryContent);
}

// ---------------------------------------------------------------------------
// Test 2: classify_admission — Tier 3 (HardSkip / SizeCeiling) direct test
//
// We cannot create 150 MB files in tests, so we call classify_admission
// directly to verify the >100 MB ceiling.
// ---------------------------------------------------------------------------

/// Tests classify_admission directly for Tier 3 since we can't create 150 MB
/// files in tests.
#[test]
fn test_admission_tier3_classify_direct() {
    use symforge::discovery::classify_admission;
    use symforge::domain::index::{AdmissionTier, SkipReason};

    // Plain text file, but size exceeds 100 MB ceiling → HardSkip
    let decision = classify_admission(Path::new("huge.log"), 150 * 1024 * 1024, None);
    assert_eq!(
        decision.tier,
        AdmissionTier::HardSkip,
        "150 MB file should be HardSkip"
    );
    assert_eq!(
        decision.reason,
        Some(SkipReason::SizeCeiling),
        "150 MB file reason should be SizeCeiling"
    );

    // Denylisted extension AND over ceiling — ceiling wins (checked first)
    let decision = classify_admission(Path::new("big.ckpt"), 4_200_000_000, None);
    assert_eq!(
        decision.tier,
        AdmissionTier::HardSkip,
        "4.2 GB .ckpt should be HardSkip (size ceiling checked before denylist)"
    );
    assert_eq!(
        decision.reason,
        Some(SkipReason::SizeCeiling),
        "4.2 GB .ckpt reason should be SizeCeiling"
    );
}

// ---------------------------------------------------------------------------
// Test 3: dependency lockfiles → Tier 2 (DependencyLockfile), manifests stay
//         Tier 1.
//
// A lockfile (`package-lock.json`, `Cargo.lock`, ...) is machine-generated and
// parses into thousands of meaningless key symbols. It must land Tier-2
// (metadata only, path still searchable) with the `DependencyLockfile` reason,
// while its sibling MANIFEST (`package.json`, `Cargo.toml`) — which a developer
// authors and which carries real structure — stays Tier-1 indexed.
// ---------------------------------------------------------------------------

#[test]
fn test_admission_dependency_lockfiles_are_tier2() {
    use symforge::domain::index::{AdmissionTier, SkipReason};

    let dir = tempdir().unwrap();

    // ── Tier 1: manifests a developer authors ────────────────────────────
    write_file(
        dir.path(),
        "package.json",
        b"{\n  \"name\": \"app\",\n  \"version\": \"1.0.0\"\n}\n",
    );

    // ── Tier 2: lockfiles (machine-generated) ────────────────────────────
    // package-lock.json content is valid JSON but its admission must be decided
    // by basename, not by extension or parseability.
    write_file(
        dir.path(),
        "package-lock.json",
        b"{\n  \"name\": \"app\",\n  \"lockfileVersion\": 3,\n  \"packages\": {}\n}\n",
    );
    write_file(
        dir.path(),
        "Cargo.lock",
        b"# This file is automatically @generated by Cargo.\nversion = 3\n",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    // package.json stays Tier-1: indexed, NOT in skipped_files.
    assert!(
        index.get_file("package.json").is_some(),
        "package.json (manifest) must stay Tier-1 indexed"
    );

    // Both lockfiles must be Tier-2 with the DependencyLockfile reason and must
    // NOT appear as indexed Tier-1 files.
    for (name, label) in [
        ("package-lock.json", "npm lockfile"),
        ("Cargo.lock", "cargo lockfile"),
    ] {
        assert!(
            index.get_file(name).is_none(),
            "{label} {name} must NOT be a Tier-1 indexed file"
        );
        let sf = index
            .skipped_files()
            .iter()
            .find(|sf| sf.path.replace('\\', "/").ends_with(name))
            .unwrap_or_else(|| panic!("{label} {name} should be in skipped_files"));
        assert_eq!(
            sf.tier(),
            AdmissionTier::MetadataOnly,
            "{label} {name} should be Tier-2 (MetadataOnly)"
        );
        assert_eq!(
            sf.reason(),
            Some(SkipReason::DependencyLockfile),
            "{label} {name} should carry the DependencyLockfile reason"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 4: classify_admission — lockfile precedence (basename beats size
//         threshold; a >1 MB lockfile still reports `DependencyLockfile`).
// ---------------------------------------------------------------------------

#[test]
fn test_admission_lockfile_classify_direct() {
    use symforge::discovery::classify_admission;
    use symforge::domain::index::{AdmissionTier, SkipReason};

    // Small lockfile → Tier-2 DependencyLockfile.
    let decision = classify_admission(Path::new("repo/yarn.lock"), 4 * 1024, None);
    assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
    assert_eq!(decision.reason, Some(SkipReason::DependencyLockfile));

    // Oversized lockfile (>1 MB) → still DependencyLockfile, NOT SizeThreshold
    // (basename is checked before the size threshold).
    let decision = classify_admission(
        Path::new("backend/package-lock.json"),
        2 * 1024 * 1024,
        None,
    );
    assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
    assert_eq!(
        decision.reason,
        Some(SkipReason::DependencyLockfile),
        "a >1MB lockfile should report DependencyLockfile, not SizeThreshold"
    );

    // The sibling manifest is NOT a lockfile → Tier-1 Normal.
    let decision = classify_admission(Path::new("backend/package.json"), 4 * 1024, None);
    assert_eq!(
        decision,
        symforge::domain::index::AdmissionDecision::normal(),
        "package.json manifest must stay Tier-1 Normal"
    );

    // go.sum (no conventional extension match) is still caught by basename.
    let decision = classify_admission(Path::new("go.sum"), 4 * 1024, None);
    assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
    assert_eq!(decision.reason, Some(SkipReason::DependencyLockfile));
}
