//! Bounded, in-process local Git ref tree/blob scout (Gate L, L-G02).
//!
//! Walks a local ref's tree through libgit2 (`git2`) — never a Git/LFS child
//! process (L-R05) — and produces a bounded catalog of blob entries keyed by
//! immutable object ID. A blob larger than the per-blob materialization budget
//! is catalog-only (L-R04): its object ID and size come from the ODB header,
//! and its bytes are never read into memory, so a giant blob cannot force
//! materialization. Identical bytes reachable at several paths share one object
//! ID while each path re-derives its own classification/language (L-R02/L-R14).
//! `materialize_ingest_blobs` reads each distinct ingest-decision object ID once
//! and never touches catalog-only blobs (L-G03 raw-bytes layer); routing those
//! bytes through the shared extraction/secret/bridge adapters is a later stage
//! (L-G04).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use git2::{ObjectType, Repository};

use crate::discovery::DiscoveryLimits;
use crate::domain::index::{FileClassification, LanguageId};

/// Default per-blob materialization ceiling. A blob larger than this is
/// catalogued by object ID and size only; its bytes are never read.
// ponytail: 8 MiB flat default; add SYMFORGE_MAX_REF_BLOB_MATERIALIZE_BYTES
// override when a deployment needs a different ceiling.
const DEFAULT_MAX_BLOB_MATERIALIZE_BYTES: u64 = 8 * 1024 * 1024;

/// Bounds applied while scouting a single local ref tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalRefScoutBudget {
    /// Maximum blob entries enumerated before the catalog is marked degraded.
    pub max_entries: u64,
    /// A blob strictly larger than this is catalog-only; its bytes stay unread.
    pub max_blob_materialize_bytes: u64,
}

impl Default for LocalRefScoutBudget {
    fn default() -> Self {
        Self {
            max_entries: DiscoveryLimits::default().max_files,
            max_blob_materialize_bytes: DEFAULT_MAX_BLOB_MATERIALIZE_BYTES,
        }
    }
}

/// Whether a scouted blob's bytes are within the materialization budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefBlobDecision {
    /// Within budget: bytes may be read on demand by object ID.
    Ingest,
    /// Over budget: catalog-only. The bytes were never read.
    CatalogOnly,
}

/// One blob reachable from a ref tree, with a source-local path mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefBlobEntry {
    /// Repository-relative forward-slash path this blob is mapped at.
    pub relative_path: String,
    /// Immutable Git object ID (hex) of the raw blob bytes.
    pub object_id: String,
    /// Blob size in bytes, read from the ODB header (never from content).
    pub size: u64,
    /// Language inferred from the path, if recognized.
    pub language: Option<LanguageId>,
    /// Deterministic semantic-lane classification for this path.
    pub classification: FileClassification,
    /// Materialization decision under the per-blob budget.
    pub decision: RefBlobDecision,
}

/// Whether ref enumeration completed within budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefScoutCoverage {
    Complete,
    Degraded,
}

/// Bounded catalog of the blobs reachable from one local ref tip.
#[derive(Clone, Debug)]
pub struct LocalRefCatalog {
    /// The resolved ref name as requested.
    pub ref_name: String,
    /// Commit object ID at the ref tip.
    pub tip_object_id: String,
    /// Root tree object ID of the tip commit.
    pub tree_object_id: String,
    /// Blob entries in canonical path order; one per path mapping.
    pub entries: Vec<RefBlobEntry>,
    /// Whether enumeration completed within budget.
    pub coverage: RefScoutCoverage,
    /// Distinct raw-blob object IDs (identical bytes counted once).
    pub distinct_blob_object_ids: usize,
    /// Total size across distinct blobs, from ODB headers.
    pub total_distinct_blob_bytes: u64,
}

/// Scout the tree reachable from a local ref tip, entirely in-process.
///
/// `ref_name` is resolved with `revparse_single` so both full ref names
/// (`refs/heads/main`) and short names (`main`, tags) work. Enumeration is
/// bounded by `budget`; exceeding the entry budget yields a degraded catalog
/// rather than an unbounded collection.
pub fn scout_local_ref(
    repository: &Repository,
    ref_name: &str,
    budget: &LocalRefScoutBudget,
) -> Result<LocalRefCatalog, String> {
    let object = repository
        .revparse_single(ref_name)
        .map_err(|_| format!("Error: local ref '{ref_name}' could not be resolved."))?;
    let commit = object
        .peel_to_commit()
        .map_err(|_| format!("Error: local ref '{ref_name}' does not resolve to a commit."))?;
    let root_tree = commit
        .tree()
        .map_err(|_| format!("Error: local ref '{ref_name}' tip commit has no tree."))?;
    let odb = repository
        .odb()
        .map_err(|_| "Error: repository object database is unavailable.".to_string())?;

    let tree_object_id = root_tree.id().to_string();
    let mut entries: Vec<RefBlobEntry> = Vec::new();
    let mut distinct: BTreeSet<git2::Oid> = BTreeSet::new();
    let mut total_distinct_blob_bytes = 0u64;
    let mut coverage = RefScoutCoverage::Complete;

    // Manual bounded DFS. Git tree entries are already name-sorted; a final
    // sort over accumulated paths gives one deterministic canonical order and
    // sidesteps git2 tree-walk callback version differences.
    let mut stack: Vec<(git2::Tree<'_>, String)> = vec![(root_tree, String::new())];
    'walk: while let Some((tree, prefix)) = stack.pop() {
        for entry in tree.iter() {
            let Ok(name) = std::str::from_utf8(entry.name_bytes()) else {
                // A non-UTF-8 tree entry name is not addressable by our
                // repository-relative path model; record degraded coverage.
                coverage = RefScoutCoverage::Degraded;
                continue;
            };
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            match entry.kind() {
                Some(ObjectType::Tree) => {
                    let subtree = repository.find_tree(entry.id()).map_err(|_| {
                        format!("Error: subtree '{path}' could not be read from '{ref_name}'.")
                    })?;
                    stack.push((subtree, path));
                }
                Some(ObjectType::Blob) => {
                    if entries.len() as u64 >= budget.max_entries {
                        coverage = RefScoutCoverage::Degraded;
                        break 'walk;
                    }
                    let oid = entry.id();
                    // Header read only: size and type without materializing bytes.
                    let (size, _kind) = odb.read_header(oid).map_err(|_| {
                        format!("Error: blob header for '{path}' is unreadable in '{ref_name}'.")
                    })?;
                    let size = size as u64;
                    if distinct.insert(oid) {
                        total_distinct_blob_bytes = total_distinct_blob_bytes.saturating_add(size);
                    }
                    let decision = if size > budget.max_blob_materialize_bytes {
                        RefBlobDecision::CatalogOnly
                    } else {
                        RefBlobDecision::Ingest
                    };
                    entries.push(RefBlobEntry {
                        relative_path: path.clone(),
                        object_id: oid.to_string(),
                        size,
                        language: LanguageId::from_path(&path),
                        classification: FileClassification::for_code_path(&path),
                        decision,
                    });
                }
                // Submodule (commit) links and any other kind carry no blob
                // bytes for this source; they are outside prose/code scope.
                _ => {}
            }
        }
    }

    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(LocalRefCatalog {
        ref_name: ref_name.to_string(),
        tip_object_id: commit.id().to_string(),
        tree_object_id,
        entries,
        coverage,
        distinct_blob_object_ids: distinct.len(),
        total_distinct_blob_bytes,
    })
}

/// Raw bytes for the ingest-decision blobs of a catalog, deduplicated by
/// immutable object ID. Catalog-only blobs are never read, and identical bytes
/// reachable at several paths are stored once (L-G03 raw-bytes layer).
#[derive(Clone, Debug, Default)]
pub struct RefBlobBytes {
    by_object_id: BTreeMap<String, Arc<[u8]>>,
}

impl RefBlobBytes {
    /// Raw bytes for one object ID, if it was an ingest-decision blob.
    pub fn get(&self, object_id: &str) -> Option<&Arc<[u8]>> {
        self.by_object_id.get(object_id)
    }

    /// Count of distinct object IDs whose bytes were materialized.
    pub fn distinct_len(&self) -> usize {
        self.by_object_id.len()
    }
}

/// Read the ingest-decision blob bytes of a catalog, once per distinct object
/// ID. Catalog-only blobs are skipped: their bytes are never materialized.
pub fn materialize_ingest_blobs(
    repository: &Repository,
    catalog: &LocalRefCatalog,
) -> Result<RefBlobBytes, String> {
    let mut by_object_id: BTreeMap<String, Arc<[u8]>> = BTreeMap::new();
    for entry in &catalog.entries {
        if entry.decision != RefBlobDecision::Ingest || by_object_id.contains_key(&entry.object_id)
        {
            continue;
        }
        let oid = git2::Oid::from_str(&entry.object_id)
            .map_err(|_| format!("Error: blob object id '{}' is invalid.", entry.object_id))?;
        let blob = repository
            .find_blob(oid)
            .map_err(|_| format!("Error: blob '{}' could not be read.", entry.object_id))?;
        by_object_id.insert(entry.object_id.clone(), Arc::from(blob.content()));
    }
    Ok(RefBlobBytes { by_object_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    fn init_repo(root: &Path) -> Repository {
        git2::Repository::init(root).expect("init repo")
    }

    fn commit_files(root: &Path, files: &[(&str, &[u8])], message: &str) -> git2::Oid {
        let repository = git2::Repository::open(root).expect("open repo");
        for (relative, bytes) in files {
            let full = root.join(relative);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(&full, bytes).expect("write file");
        }
        let mut index = repository.index().expect("index");
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .expect("stage");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repository.find_tree(tree_id).expect("tree");
        let signature =
            git2::Signature::now("SymForge Test", "symforge@example.invalid").expect("sig");
        let parent = repository
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .expect("commit")
    }

    #[test]
    fn enumerates_blobs_by_object_id_with_classification() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(
            root,
            &[
                ("src/lib.rs", b"pub fn a() {}\n"),
                ("docs/guide.md", b"# Guide\n"),
                ("vendor/dep/lib.rs", b"pub fn v() {}\n"),
            ],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let catalog = scout_local_ref(
            &repository,
            "refs/heads/master",
            &LocalRefScoutBudget::default(),
        )
        .or_else(|_| scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()))
        .expect("scout");

        assert_eq!(catalog.coverage, RefScoutCoverage::Complete);
        let paths: Vec<&str> = catalog
            .entries
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect();
        assert_eq!(paths, ["docs/guide.md", "src/lib.rs", "vendor/dep/lib.rs"]);
        assert!(
            catalog
                .entries
                .iter()
                .all(|entry| entry.object_id.len() >= 40)
        );
        assert!(
            catalog
                .entries
                .iter()
                .find(|entry| entry.relative_path == "vendor/dep/lib.rs")
                .expect("vendor entry")
                .classification
                .is_vendor
        );
        assert_eq!(
            catalog
                .entries
                .iter()
                .find(|entry| entry.relative_path == "src/lib.rs")
                .expect("rust entry")
                .language,
            Some(LanguageId::Rust)
        );
        assert!(
            catalog
                .entries
                .iter()
                .all(|entry| entry.decision == RefBlobDecision::Ingest)
        );
        assert_eq!(catalog.distinct_blob_object_ids, 3);
    }

    #[test]
    fn giant_blob_is_catalog_only_without_materialization() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        let big = vec![b'x'; 4096];
        commit_files(
            root,
            &[("small.md", b"# small\n"), ("big.bin", &big)],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let budget = LocalRefScoutBudget {
            max_entries: 1_000,
            max_blob_materialize_bytes: 1024,
        };
        let catalog = scout_local_ref(&repository, "HEAD", &budget).expect("scout");

        let big_entry = catalog
            .entries
            .iter()
            .find(|entry| entry.relative_path == "big.bin")
            .expect("big entry");
        assert_eq!(big_entry.decision, RefBlobDecision::CatalogOnly);
        assert_eq!(big_entry.size, 4096);
        let small_entry = catalog
            .entries
            .iter()
            .find(|entry| entry.relative_path == "small.md")
            .expect("small entry");
        assert_eq!(small_entry.decision, RefBlobDecision::Ingest);
    }

    #[test]
    fn identical_bytes_share_object_id_across_paths_with_distinct_classification() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        let shared = b"identical content\n";
        commit_files(
            root,
            &[("notes.md", shared), ("data.txt", shared)],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let catalog =
            scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()).expect("scout");

        let md = catalog
            .entries
            .iter()
            .find(|entry| entry.relative_path == "notes.md")
            .expect("md entry");
        let txt = catalog
            .entries
            .iter()
            .find(|entry| entry.relative_path == "data.txt")
            .expect("txt entry");
        assert_eq!(
            md.object_id, txt.object_id,
            "identical bytes share one blob"
        );
        assert_eq!(catalog.distinct_blob_object_ids, 1);
        assert_eq!(md.language, Some(LanguageId::Markdown));
        assert_eq!(txt.language, Some(LanguageId::Text));
    }

    #[test]
    fn entry_budget_degrades_coverage_without_unbounded_collection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(
            root,
            &[
                ("a.rs", b"//a\n"),
                ("b.rs", b"//b\n"),
                ("c.rs", b"//c\n"),
                ("d.rs", b"//d\n"),
            ],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let budget = LocalRefScoutBudget {
            max_entries: 2,
            max_blob_materialize_bytes: DEFAULT_MAX_BLOB_MATERIALIZE_BYTES,
        };
        let catalog = scout_local_ref(&repository, "HEAD", &budget).expect("scout");
        assert_eq!(catalog.coverage, RefScoutCoverage::Degraded);
        assert_eq!(catalog.entries.len(), 2);
    }

    #[test]
    fn missing_ref_is_typed_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("a.rs", b"//a\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let error = scout_local_ref(
            &repository,
            "refs/heads/does-not-exist",
            &LocalRefScoutBudget::default(),
        )
        .expect_err("missing ref must error");
        assert!(error.contains("could not be resolved"), "{error}");
    }

    #[test]
    fn materializes_shared_object_id_once_and_bytes_match_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        let shared = b"identical content\n";
        commit_files(
            root,
            &[("notes.md", shared), ("data.txt", shared)],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let catalog =
            scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()).expect("scout");
        let bytes = materialize_ingest_blobs(&repository, &catalog).expect("materialize");

        assert_eq!(bytes.distinct_len(), 1, "identical bytes materialized once");
        let object_id = &catalog.entries[0].object_id;
        assert_eq!(
            bytes.get(object_id).expect("shared blob bytes").as_ref(),
            shared
        );
    }

    #[test]
    fn catalog_only_blob_bytes_are_not_materialized() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        let big = vec![b'x'; 4096];
        commit_files(
            root,
            &[("small.md", b"# small\n"), ("big.bin", &big)],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let budget = LocalRefScoutBudget {
            max_entries: 1_000,
            max_blob_materialize_bytes: 1024,
        };
        let catalog = scout_local_ref(&repository, "HEAD", &budget).expect("scout");
        let bytes = materialize_ingest_blobs(&repository, &catalog).expect("materialize");

        let big_id = &catalog
            .entries
            .iter()
            .find(|entry| entry.relative_path == "big.bin")
            .expect("big entry")
            .object_id;
        let small_id = &catalog
            .entries
            .iter()
            .find(|entry| entry.relative_path == "small.md")
            .expect("small entry")
            .object_id;
        assert!(
            bytes.get(big_id).is_none(),
            "catalog-only blob bytes must never be materialized"
        );
        assert!(bytes.get(small_id).is_some(), "ingest blob is materialized");
        assert_eq!(bytes.distinct_len(), 1);
    }
}
