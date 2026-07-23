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
//! and never touches catalog-only blobs (L-G03 raw-bytes layer). `route_ref_blob`
//! sends those bytes through the SHARED target-routing/secret/parser adapters —
//! the same functions filesystem ingestion uses, no second parser or index
//! (L-G04). `build_ref_source_index` assembles those routed files into a
//! queryable, root-less `LiveIndex` for one ref source (L-G05). `SharedIndexHandle`
//! wraps it in a full published bundle and reconciles it into the instance's
//! multi-source `PublishedSourceSet` under the publication writer lock;
//! `ingest_and_publish_local_ref` is the end-to-end entry point (L-G07). The
//! remaining stage is the query-composition surface (L-G06): advertising and
//! composing the `worktrees`/`local_refs`/`all` source scopes.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use git2::{ObjectType, Repository};

use crate::discovery::DiscoveryLimits;
use crate::domain::index::{
    FileClassification, FileProcessingResult, IndexTargets, LanguageId, MetadataOnlyReason,
    RepositoryId, SourceId,
};
use crate::knowledge::{StableContentAdmission, classify_stable_content};
use crate::parsing::process_file_with_classification;

use super::store::{IndexedFile, LiveIndex, SharedIndexHandle};

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

/// Outcome of routing one ref blob's bytes through the shared adapters.
#[derive(Debug)]
pub enum RefBlobIngest {
    /// Content policy withheld the bytes (secret/LFS/encoding). Metadata only:
    /// no parsed file, no card, no search/bridge contribution.
    Withheld(MetadataOnlyReason),
    /// Parsed into an indexed file for the given ingest lanes.
    Indexed {
        targets: IndexTargets,
        file: Box<IndexedFile>,
    },
}

/// The path-dependent routing decision for one ref blob, from the SHARED
/// filesystem adapters (target routing + content policy + classification). The
/// parse is deferred so identical bytes at several same-classification paths can
/// be parsed once (L-R02); a `Withheld` decision (secret/LFS/encoding) yields no
/// parse at all.
enum RefBlobRoute {
    Withheld(MetadataOnlyReason),
    Parse {
        targets: IndexTargets,
        language: LanguageId,
        classification: FileClassification,
    },
}

/// Run the shared, path-dependent admission for one ref blob (L-R10 parity):
/// `IndexTargets::for_path` -> `classify_stable_content` -> classification. These
/// are the exact primitives filesystem ingestion uses, so identical bytes yield
/// identical lifecycle/secret decisions regardless of origin. The parse itself is
/// deferred to the caller so it can be memoized across same-class paths.
fn classify_ref_blob(entry: &RefBlobEntry, bytes: &[u8]) -> RefBlobRoute {
    let targets = IndexTargets::for_path(&entry.relative_path, entry.language.as_ref());
    match classify_stable_content(&entry.relative_path, targets, bytes) {
        StableContentAdmission::MetadataOnly(reason) => RefBlobRoute::Withheld(reason),
        StableContentAdmission::Admitted => {
            let classification =
                FileClassification::for_indexed_path(&entry.relative_path, targets);
            let language = entry.language.clone().unwrap_or(LanguageId::Text);
            RefBlobRoute::Parse {
                targets,
                language,
                classification,
            }
        }
    }
}

/// Route one ingest-decision ref blob through the SHARED target-routing,
/// content-policy (secret/LFS/encoding), and parser adapters — the exact same
/// functions the filesystem ingestion path uses (L-G04). It creates no second
/// prose parser or search index: `IndexTargets::for_path`,
/// `classify_stable_content`, and `process_file_with_classification` are the
/// shared primitives, so identical bytes yield identical lifecycle/extraction/
/// secret results regardless of whether they arrived from disk or a ref blob.
pub fn route_ref_blob(entry: &RefBlobEntry, bytes: Vec<u8>) -> RefBlobIngest {
    match classify_ref_blob(entry, &bytes) {
        RefBlobRoute::Withheld(reason) => RefBlobIngest::Withheld(reason),
        RefBlobRoute::Parse {
            targets,
            language,
            classification,
        } => {
            let result = process_file_with_classification(
                &entry.relative_path,
                &bytes,
                language,
                classification,
            );
            RefBlobIngest::Indexed {
                targets,
                file: Box::new(IndexedFile::from_parse_result(result, bytes)),
            }
        }
    }
}

/// Build a queryable in-memory `LiveIndex` for one local ref source.
///
/// Every ingest-decision blob is materialized once (dedup by object ID) and
/// routed through the shared parser/secret adapters; catalog-only and
/// content-withheld blobs contribute no indexed file. The resulting index has
/// no filesystem root — it is assembled purely from the routed files (L-G05
/// source-index foundation). It is not yet published into a `PublishedSourceSet`
/// (L-G07); that reconciliation runs under the owning instance's publication
/// lock.
pub fn build_ref_source_index(
    repository: &Repository,
    catalog: &LocalRefCatalog,
) -> Result<LiveIndex, String> {
    let blobs = materialize_ingest_blobs(repository, catalog)?;
    Ok(LiveIndex::from_source_files(
        route_catalog_files(catalog, &blobs).files,
    ))
}

/// Per-path indexed files plus the number of distinct parses actually run.
/// `parses_performed` is the L-R02 witness: it equals the count of distinct
/// (object id, classification, language, grammar-flavor) keys among admitted
/// ingest entries.
struct RefRouteOutcome {
    files: HashMap<String, Arc<IndexedFile>>,
    /// L-R02 witness: distinct parses actually run. Read only by tests; production
    /// consumes `files` alone.
    #[cfg_attr(not(test), allow(dead_code))]
    parses_performed: usize,
}

/// Route every ingest-decision blob into a per-path `IndexedFile`, parsing each
/// distinct (object id, classification, language) exactly once (L-R02 / L-G03).
/// Identical bytes reachable at several same-classification paths reuse one
/// parse, re-mapped to each path; the same object under a different
/// classification re-derives its own parse (L-R14). Path-dependent admission
/// (secret/LFS/encoding) still runs per path via the shared adapters.
fn route_catalog_files(catalog: &LocalRefCatalog, blobs: &RefBlobBytes) -> RefRouteOutcome {
    let mut files: HashMap<String, Arc<IndexedFile>> = HashMap::new();
    // Key: object id + classification + language + the two path-selected grammar
    // flavors (`.tsx` vs `.ts`; the `.h` C/C++ disambiguation branch). Same bytes
    // + same key => one parse, re-mapped to each path.
    let mut parse_cache: HashMap<
        (String, FileClassification, LanguageId, bool, bool),
        FileProcessingResult,
    > = HashMap::new();
    let mut parses_performed = 0usize;
    for entry in &catalog.entries {
        if entry.decision != RefBlobDecision::Ingest {
            continue;
        }
        let Some(bytes) = blobs.get(&entry.object_id) else {
            continue;
        };
        let RefBlobRoute::Parse {
            language,
            classification,
            ..
        } = classify_ref_blob(entry, bytes)
        else {
            continue; // withheld: metadata only, no parse, no index contribution
        };
        // `process_file_with_classification` also selects the grammar from the
        // path — `.tsx` needs the TSX grammar and `.h` runs C/C++ header
        // disambiguation — so two same-language paths can need different parses.
        // ponytail: mirrors exactly the two path-grammar branches in
        // `process_file_with_classification`; add a key field if a third appears.
        let is_tsx = LanguageId::is_tsx_path(&entry.relative_path);
        let is_c_header = LanguageId::is_c_header_path(&entry.relative_path);
        let key = (
            entry.object_id.clone(),
            classification,
            language.clone(),
            is_tsx,
            is_c_header,
        );
        let mut result = match parse_cache.get(&key) {
            Some(cached) => cached.clone(),
            None => {
                parses_performed += 1;
                let parsed = process_file_with_classification(
                    &entry.relative_path,
                    bytes,
                    language,
                    classification,
                );
                parse_cache.insert(key, parsed.clone());
                parsed
            }
        };
        // The parse is path-independent except for the label; re-map it to this
        // path so each source mapping points at its own repository-relative path.
        result.relative_path = entry.relative_path.clone();
        files.insert(
            entry.relative_path.clone(),
            Arc::new(IndexedFile::from_parse_result(result, bytes.to_vec())),
        );
    }
    RefRouteOutcome {
        files,
        parses_performed,
    }
}

/// Scout, ingest, and publish one local ref as a P1 source lane (Gate L L-G07).
///
/// End-to-end entry point: resolve and scout the ref, build its root-less
/// `LiveIndex` from the shared-adapter-routed blobs, wrap it in a full
/// published bundle, and reconcile it into `handle`'s `PublishedSourceSet`
/// under the single publication writer lock. The current worktree lane is left
/// untouched. Returns the published ref source id.
pub fn ingest_and_publish_local_ref(
    handle: &SharedIndexHandle,
    repository: &Repository,
    ref_name: &str,
    repository_id: RepositoryId,
    budget: &LocalRefScoutBudget,
) -> Result<SourceId, String> {
    let catalog = scout_local_ref(repository, ref_name, budget)?;
    let index = build_ref_source_index(repository, &catalog)?;
    // A degraded scout (entry-budget/undecodable) must not publish a false
    // Complete ref scope (L-R07): carry the scout coverage into the manifest.
    let coverage = match catalog.coverage {
        RefScoutCoverage::Complete => crate::domain::CoverageStatus::Complete,
        RefScoutCoverage::Degraded => crate::domain::CoverageStatus::Degraded,
    };
    let generation = SharedIndexHandle::build_ref_source_generation(
        index,
        repository_id,
        ref_name,
        &catalog.tip_object_id,
        coverage,
    );
    let source_id = generation
        .source
        .as_ref()
        .expect("ref-source generation carries a source identity")
        .source_id
        .clone();
    handle.publish_ref_source(generation);
    Ok(source_id)
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

    fn route_single(root: &Path, relative: &str, bytes: &[u8]) -> RefBlobIngest {
        init_repo(root);
        commit_files(root, &[(relative, bytes)], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let catalog =
            scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()).expect("scout");
        let blobs = materialize_ingest_blobs(&repository, &catalog).expect("materialize");
        let entry = catalog
            .entries
            .iter()
            .find(|entry| entry.relative_path == relative)
            .expect("entry");
        let bytes = blobs.get(&entry.object_id).expect("bytes").to_vec();
        route_ref_blob(entry, bytes)
    }

    #[test]
    fn routes_clean_code_blob_through_shared_parser() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = route_single(dir.path(), "src/lib.rs", b"pub fn routed() -> u32 { 7 }\n");
        match outcome {
            RefBlobIngest::Indexed { targets, file } => {
                assert_eq!(targets, IndexTargets::Code);
                assert!(
                    file.symbols.iter().any(|symbol| symbol.name == "routed"),
                    "shared parser must extract the ref blob's symbol"
                );
                assert!(file.classification.is_code());
            }
            other => panic!("expected Indexed, got {other:?}"),
        }
    }

    #[test]
    fn routes_markdown_blob_to_knowledge_lane() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = route_single(dir.path(), "docs/guide.md", b"# Guide\n\nBody text.\n");
        match outcome {
            RefBlobIngest::Indexed { targets, .. } => {
                assert_eq!(targets, IndexTargets::Knowledge);
            }
            other => panic!("expected Indexed, got {other:?}"),
        }
    }

    #[test]
    fn withholds_secret_positive_blob_as_metadata_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let secret =
            b"-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAK\n-----END RSA PRIVATE KEY-----\n";
        let outcome = route_single(dir.path(), "config/key.txt", secret);
        assert!(
            matches!(outcome, RefBlobIngest::Withheld(_)),
            "secret-positive ref blob must be withheld, got {outcome:?}"
        );
    }

    #[test]
    fn builds_queryable_ref_source_index_excluding_withheld_blobs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        let secret =
            b"-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAK\n-----END RSA PRIVATE KEY-----\n";
        commit_files(
            root,
            &[
                ("src/lib.rs", b"pub fn indexed_symbol() -> u32 { 1 }\n"),
                ("docs/guide.md", b"# Guide\n\nProse.\n"),
                ("config/key.txt", secret),
            ],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let catalog =
            scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()).expect("scout");
        let index = build_ref_source_index(&repository, &catalog).expect("build index");

        assert!(!index.is_empty, "ref-source index must not be empty");
        assert!(
            index.files.contains_key("src/lib.rs"),
            "routed code blob must be indexed"
        );
        assert!(
            index.files.contains_key("docs/guide.md"),
            "routed knowledge blob must be indexed"
        );
        assert!(
            !index.files.contains_key("config/key.txt"),
            "secret-withheld blob must never enter the index"
        );
        assert!(
            index.files["src/lib.rs"]
                .symbols
                .iter()
                .any(|symbol| symbol.name == "indexed_symbol"),
            "ref-source index must carry parsed symbols"
        );
    }

    #[test]
    fn ingest_and_publish_makes_a_queryable_ref_lane_without_touching_current() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(
            root,
            &[("src/lib.rs", b"pub fn ref_lane_symbol() -> bool { true }\n")],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let before = handle.published_source_set();
        let current_id = before.current_source_id.clone();

        let source_id = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-e2e"),
            &LocalRefScoutBudget::default(),
        )
        .expect("ingest and publish");

        let set = handle.published_source_set();
        assert_eq!(set.registry_generation, before.registry_generation + 1);
        assert_eq!(
            set.current_source_id, current_id,
            "current lane identity is stable"
        );
        let lane = set.sources.get(&source_id).expect("published ref lane");
        assert!(
            lane.live.files.contains_key("src/lib.rs"),
            "the ref lane is queryable"
        );
        assert!(
            lane.live.files["src/lib.rs"]
                .symbols
                .iter()
                .any(|symbol| symbol.name == "ref_lane_symbol")
        );
        match &lane.source.as_ref().expect("lane source").location {
            crate::domain::index::SourceLocation::GitRef { name } => assert_eq!(name, "HEAD"),
            other => panic!("expected GitRef location, got {other:?}"),
        }
        assert_eq!(
            lane.source_version
                .as_ref()
                .expect("lane version")
                .working_tree,
            crate::domain::index::WorkingTreeState::NotApplicable
        );
    }

    #[test]
    fn identical_blob_is_parsed_once_across_same_classification_paths() {
        // L-R02 / L-G03: identical bytes at several same-classification paths are
        // parsed exactly once and mapped to every path; the same object under a
        // different classification re-derives its own parse (L-R14).
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        let body: &[u8] = b"pub fn shared() -> u32 { 7 }\n";
        commit_files(
            root,
            &[("one.rs", body), ("two.rs", body), ("notes.md", body)],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let catalog =
            scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()).expect("scout");
        let blobs = materialize_ingest_blobs(&repository, &catalog).expect("materialize");
        assert_eq!(
            blobs.distinct_len(),
            1,
            "identical bytes share one object id"
        );

        let outcome = route_catalog_files(&catalog, &blobs);
        assert_eq!(
            outcome.files.len(),
            3,
            "every path is mapped to its own file"
        );
        assert_eq!(
            outcome.parses_performed, 2,
            "same-class duplicate parsed once; the md path re-derives its own parse"
        );
        assert_eq!(
            outcome.files["two.rs"].relative_path, "two.rs",
            "each mapping keeps its own path label"
        );
    }

    #[test]
    fn identical_blob_reparsed_per_path_selected_grammar_flavor() {
        // L-R02 / L-R10 / L-R14: `.ts` and `.tsx` share one LanguageId but the
        // grammar is path-selected inside `process_file_with_classification`, so
        // identical bytes at the two paths must NOT share a parse — each re-parses
        // under its own grammar flavor.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        let source: &[u8] = b"export const x = 1;\n";
        commit_files(root, &[("a.ts", source), ("b.tsx", source)], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let catalog =
            scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()).expect("scout");
        let blobs = materialize_ingest_blobs(&repository, &catalog).expect("materialize");
        assert_eq!(
            blobs.distinct_len(),
            1,
            "identical bytes share one object id"
        );

        let outcome = route_catalog_files(&catalog, &blobs);
        assert_eq!(outcome.files.len(), 2, "both paths are mapped");
        assert_eq!(
            outcome.parses_performed, 2,
            ".ts and .tsx select different grammars and must not share a parse"
        );
    }

    #[test]
    fn degraded_scout_publishes_degraded_ref_manifest_coverage() {
        // L-R07: a budget-degraded scout must not publish a false Complete ref
        // scope — the manifest coverage carries the scout's Degraded status.
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
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let budget = LocalRefScoutBudget {
            max_entries: 2,
            max_blob_materialize_bytes: DEFAULT_MAX_BLOB_MATERIALIZE_BYTES,
        };
        let source_id = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-degraded"),
            &budget,
        )
        .expect("ingest and publish");
        let set = handle.published_source_set();
        let lane = set.sources.get(&source_id).expect("published ref lane");
        assert_eq!(
            lane.manifest.as_ref().expect("ref lane manifest").coverage,
            crate::domain::CoverageStatus::Degraded,
            "a degraded scout must publish Degraded ref coverage"
        );
    }

    #[test]
    fn search_scoped_composes_local_ref_lane_and_reports_typed_empty_worktrees() {
        use crate::protocol::knowledge_search::search_scoped;
        use crate::protocol::search_tools::{KnowledgeSourceScope, SearchKnowledgeInput};

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(
            root,
            &[(
                "docs/topic.md",
                b"# Topic\n\nThe orbital lattice resonates across the source.\n",
            )],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-scope"),
            &LocalRefScoutBudget::default(),
        )
        .expect("publish ref");
        let set = handle.published_source_set();

        let input = |scope| SearchKnowledgeInput {
            query: "orbital lattice".to_string(),
            path_prefix: None,
            source_scope: Some(scope),
            authority_scope: None,
            project: None,
            projects: None,
            limit: None,
            max_tokens: None,
        };

        let local_refs = search_scoped(&set, &input(KnowledgeSourceScope::LocalRefs));
        assert!(
            local_refs.contains("Source scope searched: local_refs"),
            "{local_refs}"
        );
        assert!(local_refs.contains("ref:HEAD"), "{local_refs}");

        let worktrees = search_scoped(&set, &input(KnowledgeSourceScope::Worktrees));
        assert!(worktrees.contains("no_sources_in_scope"), "{worktrees}");

        let all = search_scoped(&set, &input(KnowledgeSourceScope::All));
        assert!(all.contains("Source scope searched: all"), "{all}");
        assert!(all.contains("ref:HEAD"), "{all}");
    }

    #[test]
    fn review_scoped_composes_local_ref_lane_and_reports_typed_empty_worktrees() {
        use crate::protocol::knowledge_review::review_scoped;
        use crate::protocol::search_tools::{
            KnowledgeSourceScope, ReviewKnowledgeInput, ReviewKnowledgeMode,
        };

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(
            root,
            &[(
                "docs/topic.md",
                b"# Topic\n\nThe orbital lattice resonates across the source.\n",
            )],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-review"),
            &LocalRefScoutBudget::default(),
        )
        .expect("publish ref");
        let set = handle.published_source_set();

        let input = |scope| ReviewKnowledgeInput {
            mode: ReviewKnowledgeMode::Summary,
            path: None,
            path_prefix: None,
            source_scope: Some(scope),
            project: None,
            projects: None,
            limit: None,
            max_tokens: None,
        };

        let local_refs = review_scoped(&set, &input(KnowledgeSourceScope::LocalRefs))
            .expect("local_refs review");
        assert!(local_refs.rendered.contains("top_result_hash="));
        assert!(
            local_refs.rendered.contains("source_scope=local_refs"),
            "{}",
            local_refs.rendered
        );

        let worktrees = review_scoped(&set, &input(KnowledgeSourceScope::Worktrees));
        assert!(
            worktrees
                .as_ref()
                .is_err_and(|error| error.contains("no_sources_in_scope")),
            "{worktrees:?}"
        );

        let all = review_scoped(&set, &input(KnowledgeSourceScope::All)).expect("all review");
        assert!(
            all.rendered.contains("source_scope=all"),
            "{}",
            all.rendered
        );
    }

    #[test]
    fn all_scope_lists_current_lane_before_ref_lanes() {
        // L-R01: current worktree outranks a divergent ref (ordered first).
        use crate::protocol::knowledge_search::select_scoped_sources;
        use crate::protocol::search_tools::KnowledgeSourceScope;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn a() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let ref_id = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-order"),
            &LocalRefScoutBudget::default(),
        )
        .expect("publish ref");
        let set = handle.published_source_set();

        let all = select_scoped_sources(&set, KnowledgeSourceScope::All);
        assert_eq!(all.len(), 2, "current + one ref lane");
        assert_eq!(
            all[1].source.as_ref().expect("ref lane source").source_id,
            ref_id,
            "the ref lane is ordered after the current lane"
        );
    }

    #[test]
    fn ref_movement_replaces_the_lane_and_bumps_registry() {
        // L-R03: moving a ref updates its mapping deterministically without
        // duplicating the lane, and every source-map change advances the registry.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn a() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let id = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-move"),
            &LocalRefScoutBudget::default(),
        )
        .expect("publish ref");
        let set1 = handle.published_source_set();
        let tip1 = set1.sources[&id]
            .source_version
            .as_ref()
            .expect("ref version")
            .commit
            .clone();
        let registry1 = set1.registry_generation;

        commit_files(root, &[("src/b.rs", b"pub fn b() {}\n")], "second");
        let id2 = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-move"),
            &LocalRefScoutBudget::default(),
        )
        .expect("republish moved ref");
        assert_eq!(id, id2, "the same ref keeps its source id");

        let set2 = handle.published_source_set();
        let tip2 = set2.sources[&id]
            .source_version
            .as_ref()
            .expect("ref version")
            .commit
            .clone();
        assert_ne!(tip1, tip2, "ref movement updated the recorded tip");
        assert!(
            set2.registry_generation > registry1,
            "a source-map change advances registry_generation"
        );
        assert_eq!(
            set2.sources.len(),
            2,
            "the moved ref replaced its lane rather than duplicating it"
        );
    }

    #[test]
    fn failed_ref_ingestion_leaves_the_current_lane_untouched() {
        // L-V04: a local-ref lane failure leaves the current worktree P0 lane
        // present and queryable, and advances nothing in the published set.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn a() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let before = handle.published_source_set();
        let before_registry = before.registry_generation;
        let before_len = before.sources.len();
        let current_id = before.current_source_id.clone();

        let result = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "refs/heads/does-not-exist",
            RepositoryId::new("repo-fail"),
            &LocalRefScoutBudget::default(),
        );
        assert!(result.is_err(), "a missing ref must fail to ingest");

        let after = handle.published_source_set();
        assert_eq!(
            after.registry_generation, before_registry,
            "a failed ref ingestion must not advance registry_generation"
        );
        assert_eq!(
            after.sources.len(),
            before_len,
            "a failed ref ingestion must not add a lane"
        );
        assert!(
            after.sources.contains_key(&current_id),
            "the current worktree lane stays present and queryable"
        );
    }
}
