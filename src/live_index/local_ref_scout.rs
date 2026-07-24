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
use super::worktree_topology::{CheckedOutWorktree, checked_out_worktrees};

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
    // Finding D5: the entry budget must bound EVERY visited tree entry — subtrees
    // included — not just ingested blobs. A tree of millions of nested empty dirs
    // otherwise does unbounded `find_tree` work while still reporting Complete.
    let mut visited = 0u64;

    // Manual bounded DFS. Git tree entries are already name-sorted; a final
    // sort over accumulated paths gives one deterministic canonical order and
    // sidesteps git2 tree-walk callback version differences.
    let mut stack: Vec<(git2::Tree<'_>, String)> = vec![(root_tree, String::new())];
    'walk: while let Some((tree, prefix)) = stack.pop() {
        for entry in tree.iter() {
            // Count the entry FIRST so trees and blobs alike are bounded; on
            // exceed, stop the walk and report degraded coverage.
            if visited >= budget.max_entries {
                coverage = RefScoutCoverage::Degraded;
                break 'walk;
            }
            visited += 1;
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
/// sensitive-path → shared PATH+SIZE tiers → binary sniff → `classify_stable_content`
/// → classification. These are the exact primitives filesystem ingestion uses, so
/// identical bytes yield identical lifecycle/secret decisions regardless of origin.
/// The parse itself is deferred to the caller so it can be memoized across
/// same-class paths.
fn classify_ref_blob(entry: &RefBlobEntry, bytes: &[u8]) -> RefBlobRoute {
    // L-R10 secret parity: a sensitive PATH is withheld as metadata-only by path
    // alone — the exact rule and reason the filesystem scout applies (see
    // `src/discovery/mod.rs`) — BEFORE any content scan. `classify_stable_content`
    // only scans CONTENT, so a committed `.env`/private key whose bytes do not
    // trip the content scanner must still be withheld here, or the ref-blob path
    // would index a secret the disk path refuses.
    if let Some(rule_id) = crate::knowledge::sensitive_path_rule(&entry.relative_path) {
        return RefBlobRoute::Withheld(MetadataOnlyReason::SensitivePath {
            rule_id: rule_id.to_string(),
        });
    }
    // L-R10/L-G04 admission parity (finding D1): the filesystem scout also routes
    // every entry through `classify_admission`'s PATH+SIZE tiers (dependency
    // lockfile, denylisted extension, oversize) and an 8 KB binary sniff. Run the
    // SAME shared helper + predicate here so a committed lockfile, an oversized
    // data blob, or a binary is withheld exactly as on disk. `entry.size` is the
    // ODB header size, so an oversized blob is withheld before its bytes are
    // parsed (catalog-only blobs never reach this function at all).
    if let Some(reason) = crate::discovery::path_admission_reason(
        std::path::Path::new(&entry.relative_path),
        entry.size,
    ) {
        return RefBlobRoute::Withheld(reason);
    }
    // Match the disk probe exactly: the filesystem scout sniffs the first ≤8 KB.
    let sniff_len = bytes.len().min(crate::domain::index::BINARY_SNIFF_BYTES);
    if crate::discovery::is_binary_content(&bytes[..sniff_len]) {
        return RefBlobRoute::Withheld(MetadataOnlyReason::Binary);
    }
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

/// Route ONE ingest-decision ref blob through the SHARED admission + parser
/// adapters. Production ingestion routes blobs through `route_catalog_files` (which
/// memoizes parses across same-class paths); BOTH paths share `classify_ref_blob`
/// for admission, so they cannot drift on the withhold decision (the D1 root
/// cause). This single-blob wrapper is exercised only by the test harness — hence
/// `pub(crate)` and the not-test dead-code allowance — and stays byte-for-byte
/// aligned with production because it delegates to the same `classify_ref_blob` and
/// `process_file_with_classification` primitives.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn route_ref_blob(entry: &RefBlobEntry, bytes: Vec<u8>) -> RefBlobIngest {
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
    let generation = handle.build_ref_source_generation(
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

/// Outcome of one local ref/worktree topology reconcile.
#[derive(Debug)]
pub struct ReconcileOutcome {
    /// Ref lanes published (fresh or tip-updated) this pass.
    pub published: Vec<SourceId>,
    /// Ref lanes removed this pass (branch deleted or newly checked out).
    pub removed: Vec<SourceId>,
    /// Linked worktrees found checked out — routed to their own
    /// `ProjectInstance` by a later daemon layer, never ingested as P1 lanes.
    pub checked_out: Vec<CheckedOutWorktree>,
    /// Per-branch publish failures collected this pass as `(ref_name, error)`.
    /// A single bare branch failing to ingest must NOT abort the reconcile: the
    /// failure is recorded here and the deletion pass still runs, so stale lanes
    /// for deleted/checked-out branches are always reconciled (finding C).
    pub failed: Vec<(String, String)>,
    /// True when this pass was skipped because a reconcile was already running for
    /// this handle (single-flight, finding F). All other vecs are empty.
    pub skipped: bool,
}

/// Purge every P1 ref lane owned by `repository_id` from the published set
/// (finding D4).
///
/// Called when reconcile fails CLOSED on unprovable topology: with the checked-out
/// set unprovable, NO existing P1 lane can be proven to still be a bare branch, so
/// a lane for a branch that is NOW checked out would otherwise survive the aborted
/// deletion pass and keep serving a P0 branch as a P1 lane. Dropping all of this
/// repo's ref lanes is the only safe state; the next successful reconcile
/// republishes the genuinely-bare ones. Each removal goes through
/// `remove_ref_source` (writer lock, registry bump, P0 lane left untouched).
fn purge_repo_ref_lanes(handle: &SharedIndexHandle, repository_id: &RepositoryId) {
    let lane_prefix = format!("symforge:git-ref:{}:", repository_id.as_str());
    let current = handle.published_source_set();
    let lanes: Vec<SourceId> = current
        .sources
        .keys()
        .filter(|id| *id != &current.current_source_id)
        .filter(|id| id.as_str().starts_with(lane_prefix.as_str()))
        .cloned()
        .collect();
    for source_id in &lanes {
        handle.remove_ref_source(source_id);
    }
}

/// Reconcile the local branch/worktree topology into P1 ref lanes (L-G05/L-R03).
///
/// A P1 ref lane is published for every *bare* local branch — a local branch
/// that is NOT checked out in any linked worktree AND is NOT the main repo's own
/// current HEAD. The current worktree is the P0 lane and a checked-out linked
/// worktree is a separate `ProjectInstance`'s own P0 lane; neither may ever
/// become a P1 lane (`data-model.md:1258-1263`). Existing ref lanes whose branch
/// no longer exists as a bare local branch (deleted, or newly checked out) are
/// removed. Every source-map mutation goes through `publish_ref_source` /
/// `remove_ref_source`, which hold the publication writer lock, bump
/// `registry_generation`, and preserve the P0 current lane untouched.
pub fn reconcile_local_ref_topology(
    handle: &SharedIndexHandle,
    repository: &Repository,
    repository_id: RepositoryId,
    budget: &LocalRefScoutBudget,
) -> Result<ReconcileOutcome, String> {
    // Single-flight (finding F): a reconcile pass reads the latest git topology,
    // so if one is already running this pass is redundant and — worse — its
    // deletion step, working from a now-stale `local_branch_refs` snapshot, could
    // cross-delete a lane the running pass just published. `try_lock` never blocks,
    // so a concurrent pass never stalls P0; it skips because the running pass
    // already reflects the newest refs.
    let Some(_reconcile_guard) = handle.try_lock_ref_reconcile() else {
        return Ok(ReconcileOutcome {
            published: Vec::new(),
            removed: Vec::new(),
            checked_out: Vec::new(),
            failed: Vec::new(),
            skipped: true,
        });
    };

    let checked_out = checked_out_worktrees(repository)?;

    // Fail CLOSED (finding E): a worktree that validated but whose HEAD could not
    // be resolved leaves us unable to prove which branch it holds. Since ANY
    // bare-looking branch could be the one that worktree has checked out, we abort
    // the whole pass rather than risk publishing a checked-out branch as a P1 lane
    // (L-G01). Aborting is safe: reconcile is best-effort P1 background work, the
    // P0 current lane is never touched. Finding D4: purge every P1 lane for this
    // repo BEFORE aborting — an unprovable checked-out set means no existing P1
    // lane can be proven still bare, so a lane for a now-checked-out branch must
    // not survive the skipped deletion pass.
    if checked_out.iter().any(|worktree| !worktree.head_resolved) {
        purge_repo_ref_lanes(handle, &repository_id);
        return Err(
            "Error: a checked-out worktree's HEAD could not be resolved; local-ref \
             reconcile fails closed to avoid publishing a checked-out branch as a P1 lane."
                .to_string(),
        );
    }

    // The checked-out set: every linked-worktree HEAD branch plus the main
    // repo's own current HEAD branch. Each is the P0 lane of some
    // ProjectInstance and must never be ingested as a P1 ref lane.
    let mut checked_out_refs: BTreeSet<String> = checked_out
        .iter()
        .filter_map(|w| w.head_ref.clone())
        .collect();
    // Fail CLOSED for the main worktree HEAD too (finding E / Cursor #2): a main
    // HEAD that is a branch whose name cannot be read must abort — otherwise the
    // current branch is silently omitted from `checked_out_refs` and becomes
    // eligible for a P1 lane (L-G01: "checked-out branches are never P1"). A
    // detached HEAD (no branch here) and an unborn branch (empty repo, no local
    // branches to publish) are both fine and add nothing.
    match repository.head() {
        Ok(head) if head.is_branch() => match head.name() {
            Ok(name) => {
                checked_out_refs.insert(name.to_string());
            }
            Err(_) => {
                purge_repo_ref_lanes(handle, &repository_id);
                return Err(
                    "Error: the main repository HEAD is a branch whose name could \
                     not be decoded; local-ref reconcile fails closed to avoid \
                     publishing a checked-out branch as a P1 lane."
                        .to_string(),
                );
            }
        },
        Ok(_) => {} // detached main HEAD: no branch checked out at the main worktree
        Err(err) if err.code() == git2::ErrorCode::UnbornBranch => {}
        Err(err) => {
            purge_repo_ref_lanes(handle, &repository_id);
            return Err(format!(
                "Error: the main repository HEAD could not be read ({err}); \
                 local-ref reconcile fails closed."
            ));
        }
    }

    // Finding D2 (L-G01): when THIS instance was opened AT a linked worktree,
    // `repository.head()` above is the WORKTREE's HEAD — the MAIN worktree's
    // checked-out branch (its own P0 lane) is invisible here and would be wrongly
    // republished as a P1 lane. Opening a linked worktree as a project is supported
    // (data-model.md:1260-1263), so union the main worktree's HEAD from the common
    // dir, with the SAME fail-closed arms as the main-HEAD block above.
    if repository.is_worktree() {
        let main = match Repository::open(repository.commondir()) {
            Ok(main) => main,
            Err(err) => {
                purge_repo_ref_lanes(handle, &repository_id);
                return Err(format!(
                    "Error: the main worktree repository could not be opened from the \
                     common dir ({err}); local-ref reconcile fails closed."
                ));
            }
        };
        match main.head() {
            Ok(head) if head.is_branch() => match head.name() {
                Ok(name) => {
                    checked_out_refs.insert(name.to_string());
                }
                Err(_) => {
                    purge_repo_ref_lanes(handle, &repository_id);
                    return Err(
                        "Error: the main worktree HEAD is a branch whose name could \
                         not be decoded; local-ref reconcile fails closed to avoid \
                         publishing a checked-out branch as a P1 lane."
                            .to_string(),
                    );
                }
            },
            Ok(_) => {} // detached main worktree HEAD: no branch checked out there
            Err(err) if err.code() == git2::ErrorCode::UnbornBranch => {}
            Err(err) => {
                purge_repo_ref_lanes(handle, &repository_id);
                return Err(format!(
                    "Error: the main worktree HEAD could not be read ({err}); \
                     local-ref reconcile fails closed."
                ));
            }
        }
    }

    // Local branch refs by full refname (`refs/heads/<branch>`).
    let mut local_branch_refs: Vec<String> = Vec::new();
    let branches = repository
        .branches(Some(git2::BranchType::Local))
        .map_err(|err| format!("Error: local branches are unavailable: {err}."))?;
    for branch in branches {
        let (branch, _kind) =
            branch.map_err(|err| format!("Error: a local branch entry is unreadable: {err}."))?;
        if let Ok(name) = branch.get().name() {
            local_branch_refs.push(name.to_string());
        }
    }
    local_branch_refs.sort();

    let lane_prefix = format!("symforge:git-ref:{}:", repository_id.as_str());

    // Publish a P1 lane for every bare (non-checked-out) local branch. A single
    // branch failing to ingest is COLLECTED, not fatal (finding C): the deletion
    // pass below must still reconcile every branch whose ref is gone/checked-out,
    // so one broken branch cannot strand stale lanes for the others.
    let mut published: Vec<SourceId> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    for ref_name in &local_branch_refs {
        if checked_out_refs.contains(ref_name) {
            continue;
        }
        // Finding S1: skip the full materialize+parse republish when this branch
        // tip has not moved since its lane was last published. `build_ref_source_
        // generation` already carries `content_generation` forward on an
        // identical-tip republish, so the scout+ingest is provably redundant. The
        // deletion pass keeps the lane (its branch is still in `local_branch_refs`),
        // so skipping publish is safe. A branch tip we cannot resolve falls through
        // to a normal (re)publish attempt.
        if let Ok(oid) = repository.refname_to_id(ref_name) {
            let tip = oid.to_string();
            let source_id = SourceId::new(format!("{lane_prefix}{ref_name}"));
            let unchanged = handle
                .published_source_set()
                .sources
                .get(&source_id)
                .and_then(|lane| lane.source_version.as_ref())
                .map(|version| version.commit.as_deref() == Some(tip.as_str()))
                .unwrap_or(false);
            if unchanged {
                continue;
            }
        }
        match ingest_and_publish_local_ref(
            handle,
            repository,
            ref_name,
            repository_id.clone(),
            budget,
        ) {
            Ok(source_id) => published.push(source_id),
            Err(error) => failed.push((ref_name.clone(), error)),
        }
    }

    // Reconcile deletions: drop any existing P1 ref lane for this repository
    // whose branch is no longer a bare local branch (deleted or newly
    // checked out). Runs unconditionally after the publish pass — a publish
    // failure above never skips it. Reads a fresh snapshot so the lanes just
    // published above are already visible.
    let current = handle.published_source_set();
    let mut removed: Vec<SourceId> = Vec::new();
    for source_id in current.sources.keys() {
        if source_id == &current.current_source_id {
            continue;
        }
        if !source_id.as_str().starts_with("symforge:git-ref:") {
            continue;
        }
        // Only reconcile lanes owned by THIS repository id; leave any other's.
        let Some(ref_name) = source_id.as_str().strip_prefix(&lane_prefix) else {
            continue;
        };
        let still_bare = local_branch_refs.iter().any(|branch| branch == ref_name)
            && !checked_out_refs.contains(ref_name);
        if !still_bare && handle.remove_ref_source(source_id) {
            removed.push(source_id.clone());
        }
    }

    Ok(ReconcileOutcome {
        published,
        removed,
        checked_out,
        failed,
        skipped: false,
    })
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
    fn tree_entries_count_against_the_scout_budget() {
        // Finding D5: the entry budget must count EVERY visited tree entry (subtrees
        // included), not just ingested blobs. Here four blobs live under four
        // subtrees — eight tree entries total. A blob-only budget of 5 would never
        // trip and would wrongly report Complete; counting subtrees degrades the
        // walk, bounding the unbounded `find_tree` work a deep-empty-dir tree causes.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(
            root,
            &[
                ("d0/f.rs", b"//0\n"),
                ("d1/f.rs", b"//1\n"),
                ("d2/f.rs", b"//2\n"),
                ("d3/f.rs", b"//3\n"),
            ],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let budget = LocalRefScoutBudget {
            max_entries: 5,
            max_blob_materialize_bytes: DEFAULT_MAX_BLOB_MATERIALIZE_BYTES,
        };
        let catalog = scout_local_ref(&repository, "HEAD", &budget).expect("scout");
        assert_eq!(
            catalog.coverage,
            RefScoutCoverage::Degraded,
            "subtree entries must count against the budget, degrading coverage"
        );
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

    /// Create a bare local `branch` whose tree holds a single `dir_name/file_name`
    /// knowledge doc, built entirely in the object database — the working tree is
    /// never touched, so the current (P0) lane loaded from disk stays isolated.
    fn commit_bare_branch_with_doc(
        repository: &Repository,
        branch: &str,
        dir_name: &str,
        file_name: &str,
        bytes: &[u8],
    ) {
        let blob = repository.blob(bytes).expect("write blob");
        let mut dir_builder = repository.treebuilder(None).expect("dir treebuilder");
        dir_builder
            .insert(file_name, blob, 0o100644)
            .expect("insert blob into dir tree");
        let dir_tree = dir_builder.write().expect("write dir tree");
        let mut root_builder = repository.treebuilder(None).expect("root treebuilder");
        root_builder
            .insert(dir_name, dir_tree, 0o040000)
            .expect("insert dir into root tree");
        let root_tree_id = root_builder.write().expect("write root tree");
        let root_tree = repository.find_tree(root_tree_id).expect("find root tree");
        let signature =
            git2::Signature::now("SymForge Test", "symforge@example.invalid").expect("signature");
        let commit_id = repository
            .commit(
                None,
                &signature,
                &signature,
                "ref-only doc",
                &root_tree,
                &[],
            )
            .expect("commit ref tree");
        let commit = repository.find_commit(commit_id).expect("find commit");
        repository
            .branch(branch, &commit, false)
            .expect("create bare branch");
    }

    #[test]
    fn source_isolation_never_crosses_ref_and_current_boundaries() {
        // L-R08: a ref (P1) lane's source identity, documents, bridge, and authority
        // never reference the current (P0) worktree lane's — and vice versa. Each lane
        // is built from its own LiveIndex + SourceIdentity, so no bridge card or
        // authority record crosses the source boundary.
        use crate::domain::index::SourceLocation;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        // Current worktree lane: a distinctive knowledge doc on HEAD.
        commit_files(
            root,
            &[(
                "docs/current-only.md",
                b"# Current Only\n\nThe quantum meridian anchors the current worktree lane.\n",
            )],
            "current",
        );
        let repository = git2::Repository::open(root).expect("open");
        // Bare ref lane: a DIFFERENT distinctive knowledge doc, only in the ODB.
        commit_bare_branch_with_doc(
            &repository,
            "ref-only",
            "docs",
            "ref-only.md",
            b"# Ref Only\n\nThe helical cascade governs the divergent ref lane.\n",
        );

        // P0 current lane, loaded from the working tree (real WorkingTree identity).
        let handle = LiveIndex::load(root).expect("load current lane");
        // P1 ref lane, published into the same handle.
        let ref_id = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "refs/heads/ref-only",
            RepositoryId::new("repo-isolation"),
            &LocalRefScoutBudget::default(),
        )
        .expect("publish ref lane");

        let set = handle.published_source_set();
        let current = set.current_generation();
        let ref_gen = set.sources.get(&ref_id).expect("ref lane generation");

        // --- Source identity isolation ---
        let current_source = current.source.as_ref().expect("current source identity");
        let ref_source = ref_gen.source.as_ref().expect("ref source identity");
        assert!(
            matches!(current_source.location, SourceLocation::WorkingTree { .. }),
            "current lane is a WorkingTree source, got {:?}",
            current_source.location
        );
        match &ref_source.location {
            SourceLocation::GitRef { name } => assert_eq!(name, "refs/heads/ref-only"),
            other => panic!("ref lane must be a GitRef source, got {other:?}"),
        }
        assert_ne!(
            current_source.source_id, ref_source.source_id,
            "the two lanes carry distinct source ids"
        );

        // --- Document isolation: neither lane's files leak into the other ---
        assert!(current.live.files.contains_key("docs/current-only.md"));
        assert!(
            !current.live.files.contains_key("docs/ref-only.md"),
            "the current lane never carries the ref lane's document"
        );
        assert!(ref_gen.live.files.contains_key("docs/ref-only.md"));
        assert!(
            !ref_gen.live.files.contains_key("docs/current-only.md"),
            "the ref lane never carries the current lane's document"
        );

        // --- Bridge isolation: every card carries its own lane's source + document ---
        assert!(
            ref_gen
                .bridge
                .cards
                .iter()
                .any(|card| card.anchor.path == "docs/ref-only.md"),
            "the ref bridge has a card for its own document"
        );
        for card in &ref_gen.bridge.cards {
            assert_eq!(
                &card.anchor.source,
                ref_source.as_ref(),
                "a ref bridge card carries the ref source identity"
            );
            assert_ne!(
                card.anchor.path, "docs/current-only.md",
                "the ref bridge never references the current lane's document"
            );
        }
        assert!(
            current
                .bridge
                .cards
                .iter()
                .any(|card| card.anchor.path == "docs/current-only.md"),
            "the current bridge has a card for its own document"
        );
        for card in &current.bridge.cards {
            assert_eq!(
                &card.anchor.source,
                current_source.as_ref(),
                "a current bridge card carries the current source identity"
            );
            assert_ne!(
                card.anchor.path, "docs/ref-only.md",
                "the current bridge never references the ref lane's document"
            );
        }

        // --- Authority isolation: the authority view is source-local too ---
        assert_eq!(
            ref_gen
                .authority
                .source
                .as_ref()
                .expect("ref authority source"),
            ref_source.as_ref(),
            "the ref authority carries the ref source identity"
        );
        for record in &ref_gen.authority.records {
            assert_eq!(&record.unit.source, ref_source.as_ref());
            assert_ne!(record.unit.path, "docs/current-only.md");
        }
        assert_eq!(
            current
                .authority
                .source
                .as_ref()
                .expect("current authority source"),
            current_source.as_ref(),
            "the current authority carries the current source identity"
        );
        for record in &current.authority.records {
            assert_eq!(&record.unit.source, current_source.as_ref());
            assert_ne!(record.unit.path, "docs/ref-only.md");
        }
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
    fn reconcile_publishes_bare_branch_and_excludes_checked_out_worktree_branch() {
        // L-G01: a checked-out linked-worktree branch stays a separate
        // ProjectInstance's P0 lane and is never ingested as a P1 ref lane, while a
        // bare local branch becomes a P1 ref lane (data-model.md:1258-1263).
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let main_ref = repository
            .head()
            .expect("head")
            .name()
            .expect("head name")
            .to_string();

        // A bare local branch (no worktree) alongside a checked-out linked worktree.
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("bare", &head_commit, false)
            .expect("bare branch");
        let wt_parent = tempfile::tempdir().expect("wt tempdir");
        let wt_path = wt_parent.path().join("checked");
        add_worktree_on_branch(&repository, &wt_path, "checked-wt", "checked");

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-topology");
        let outcome = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("reconcile");

        let bare_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/bare",
            repository_id.as_str()
        ));
        let checked_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/checked",
            repository_id.as_str()
        ));
        let main_id = SourceId::new(format!(
            "symforge:git-ref:{}:{main_ref}",
            repository_id.as_str()
        ));

        let set = handle.published_source_set();
        assert!(
            set.sources.contains_key(&bare_id),
            "the bare branch gets a P1 lane"
        );
        assert!(
            !set.sources.contains_key(&checked_id),
            "the checked-out worktree branch is NOT a P1 lane"
        );
        assert!(
            !set.sources.contains_key(&main_id),
            "the current worktree HEAD (P0) is never a P1 lane"
        );
        assert!(outcome.published.contains(&bare_id));
        assert!(!outcome.published.contains(&checked_id));
        assert!(
            outcome
                .checked_out
                .iter()
                .any(|w| w.head_ref.as_deref() == Some("refs/heads/checked")),
            "the checked-out worktree is surfaced for its own ProjectInstance routing"
        );
    }

    #[test]
    fn reconcile_removes_lane_for_deleted_branch_and_advances_registry() {
        // L-R03: a ref lane whose branch was deleted is removed deterministically,
        // and every source-map change advances registry_generation.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("doomed", &head_commit, false)
            .expect("branch");

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-delete");
        let doomed_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/doomed",
            repository_id.as_str()
        ));

        let first = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("first reconcile");
        assert!(
            first.published.contains(&doomed_id),
            "bare branch published"
        );
        let registry_after_publish = handle.published_source_set().registry_generation;
        assert!(
            handle
                .published_source_set()
                .sources
                .contains_key(&doomed_id),
            "the bare branch lane exists before deletion"
        );

        repository
            .find_branch("doomed", git2::BranchType::Local)
            .expect("find branch")
            .delete()
            .expect("delete branch");

        let second = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("second reconcile");

        assert!(
            second.removed.contains(&doomed_id),
            "the deleted branch's lane is removed"
        );
        let set = handle.published_source_set();
        assert!(
            !set.sources.contains_key(&doomed_id),
            "the deleted branch's lane is gone"
        );
        assert!(
            set.registry_generation > registry_after_publish,
            "removing a lane advances registry_generation"
        );
    }

    #[test]
    fn reconcile_ingests_offline_with_no_remote_configured() {
        // L-R05: local-ref ingestion needs no network. This fixture has NO git
        // remote configured, so a successful reconcile proves no fetch is required.
        //
        // No Git/LFS subprocess is ever spawned during this path: the whole
        // scout/ingest chain is libgit2 (git2, `vendored-libgit2`) FFI in-process —
        // never `std::process::Command` — so there is no child process that could
        // reach the network in the first place.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn offline() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("offline-ref", &head_commit, false)
            .expect("branch");

        assert!(
            repository.remotes().expect("remotes").is_empty(),
            "the fixture genuinely has no git remote"
        );

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-offline");
        let outcome = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("offline reconcile succeeds without a remote");

        let ref_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/offline-ref",
            repository_id.as_str()
        ));
        assert!(
            outcome.published.contains(&ref_id),
            "the offline ref lane is published without any fetch"
        );
        assert!(
            handle.published_source_set().sources[&ref_id]
                .live
                .files
                .contains_key("src/lib.rs"),
            "the offline ref lane is queryable"
        );
    }

    /// Stage explicit repository-relative paths (incl. dotfiles/dot-directories) and
    /// commit them to HEAD. `add_path` force-adds each named path and bypasses ignore
    /// rules, so a `.env`/`.ssh/id_ed25519` blob is deterministically committed
    /// without depending on `*`-pathspec dotfile semantics.
    fn commit_explicit_paths(root: &Path, files: &[(&str, &[u8])], message: &str) {
        let repository = git2::Repository::open(root).expect("open repo");
        let mut index = repository.index().expect("index");
        for (relative, bytes) in files {
            let full = root.join(relative);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(&full, bytes).expect("write file");
            index.add_path(Path::new(relative)).expect("stage path");
        }
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
            .expect("commit");
    }

    #[test]
    fn sensitive_path_blob_is_withheld_by_path_even_when_content_is_clean() {
        // L-R10 secret parity (finding A): a committed `.env`/private-key blob is
        // withheld as metadata-only by PATH alone — exactly as the filesystem scout
        // does (src/discovery/mod.rs) — even when its bytes never trip the CONTENT
        // secret scanner. The identical bytes at a benign path are admitted, proving
        // the withhold is path-driven, not content-driven.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        // `your_api_key_here` is a placeholder value the content scanner treats as
        // clean; the ssh blob likewise carries no secret pattern.
        let env_bytes: &[u8] = b"API_KEY=your_api_key_here\n";
        commit_explicit_paths(
            root,
            &[
                (".env", env_bytes),
                (".ssh/id_ed25519", b"benign key material placeholder\n"),
                ("notes.txt", env_bytes),
                ("src/lib.rs", b"pub fn kept() {}\n"),
            ],
            "initial",
        );
        let repository = git2::Repository::open(root).expect("open");
        let catalog =
            scout_local_ref(&repository, "HEAD", &LocalRefScoutBudget::default()).expect("scout");
        let blobs = materialize_ingest_blobs(&repository, &catalog).expect("materialize");
        let route = |relative: &str| {
            let entry = catalog
                .entries
                .iter()
                .find(|entry| entry.relative_path == relative)
                .expect("entry present");
            let bytes = blobs.get(&entry.object_id).expect("bytes").to_vec();
            route_ref_blob(entry, bytes)
        };

        match route(".env") {
            RefBlobIngest::Withheld(MetadataOnlyReason::SensitivePath { rule_id }) => {
                assert_eq!(rule_id, "path.environment-credentials");
            }
            other => panic!("`.env` must be withheld by sensitive path, got {other:?}"),
        }
        match route(".ssh/id_ed25519") {
            RefBlobIngest::Withheld(MetadataOnlyReason::SensitivePath { rule_id }) => {
                assert_eq!(rule_id, "path.private-key-material");
            }
            other => panic!("`id_ed25519` must be withheld by sensitive path, got {other:?}"),
        }
        // Identical env bytes at a NON-sensitive path are admitted: the content is
        // clean, so only the sensitive PATH withholds — filesystem-scout parity.
        assert!(
            matches!(route("notes.txt"), RefBlobIngest::Indexed { .. }),
            "identical env bytes at a benign path must be admitted"
        );

        let index = build_ref_source_index(&repository, &catalog).expect("build index");
        assert!(
            !index.files.contains_key(".env"),
            "the sensitive `.env` blob must never enter the ref index"
        );
        assert!(
            !index.files.contains_key(".ssh/id_ed25519"),
            "the private-key blob must never enter the ref index"
        );
        assert!(
            index.files.contains_key("src/lib.rs"),
            "clean code stays indexed"
        );
        assert!(
            index.files.contains_key("notes.txt"),
            "clean env-at-benign-path stays indexed"
        );
    }

    #[test]
    fn dependency_lockfile_ref_blob_is_withheld_matching_filesystem_admission() {
        // Finding D1 (L-R10/L-G04): a committed dependency lockfile is withheld
        // metadata-only from a ref exactly as `classify_admission` withholds it on
        // disk — its machine-generated content is never parsed into junk symbols.
        // The `Lockfile` reason is byte-identical to the filesystem scout's.
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = route_single(
            dir.path(),
            "deps/package-lock.json",
            b"{\n  \"name\": \"x\",\n  \"lockfileVersion\": 3\n}\n",
        );
        assert!(
            matches!(
                outcome,
                RefBlobIngest::Withheld(MetadataOnlyReason::Lockfile)
            ),
            "a committed lockfile must be withheld as Lockfile, got {outcome:?}"
        );
    }

    #[test]
    fn oversized_data_ref_blob_is_withheld_from_odb_header_matching_filesystem() {
        // Finding D1 (L-R10/L-G04): a >1 MiB data blob is withheld `OversizedData`
        // from the ODB header size, before its bytes are parsed — identical to the
        // filesystem size tier (1 MiB data / 4 MiB code).
        let dir = tempfile::tempdir().expect("tempdir");
        let big_csv = vec![b'a'; 2 * 1024 * 1024];
        let outcome = route_single(dir.path(), "data/export.csv", &big_csv);
        assert!(
            matches!(
                outcome,
                RefBlobIngest::Withheld(MetadataOnlyReason::OversizedData)
            ),
            "a 2 MiB .csv must be withheld as OversizedData, got {outcome:?}"
        );
    }

    #[test]
    fn binary_ref_blob_is_withheld_via_shared_sniff_matching_filesystem() {
        // Finding D1 (L-R10/L-G04): a small binary blob at a NON-denylisted
        // extension is withheld via the SAME 8 KB binary-sniff predicate the
        // filesystem scout applies, matching its `Binary` reason. (`.dat` is not on
        // the extension denylist, so it must reach the content sniff, not the
        // denylist tier.)
        let dir = tempfile::tempdir().expect("tempdir");
        let binary: &[u8] = b"\x00\x01\x02BINARY\x00PAYLOAD\x00\xff\xfe";
        let outcome = route_single(dir.path(), "assets/blob.dat", binary);
        assert!(
            matches!(outcome, RefBlobIngest::Withheld(MetadataOnlyReason::Binary)),
            "a small binary blob must be withheld as Binary, got {outcome:?}"
        );
    }

    #[test]
    fn reconcile_deletion_pass_runs_despite_a_branch_publish_failure() {
        // Finding C: a single bare branch failing to ingest must NOT abort the pass;
        // the deletion pass still removes a stale lane for a deleted branch.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("doomed", &head_commit, false)
            .expect("doomed branch");

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-resilient");
        let doomed_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/doomed",
            repository_id.as_str()
        ));

        let first = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("first reconcile");
        assert!(
            first.published.contains(&doomed_id),
            "doomed lane published"
        );

        // A branch ref pointing at a blob (not a commit): its publish fails at
        // `peel_to_commit`, exercising the collect-and-continue path.
        let blob_oid = repository.blob(b"not a commit\n").expect("blob");
        repository
            .reference("refs/heads/broken", blob_oid, true, "broken ref")
            .expect("broken ref");
        // Delete the doomed branch so the deletion pass has stale work to reconcile.
        repository
            .find_branch("doomed", git2::BranchType::Local)
            .expect("find doomed")
            .delete()
            .expect("delete doomed");

        let second = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("second reconcile still succeeds");

        assert!(
            second
                .failed
                .iter()
                .any(|(name, _)| name == "refs/heads/broken"),
            "the broken branch's publish failure is collected, not fatal: {:?}",
            second.failed
        );
        assert!(
            second.removed.contains(&doomed_id),
            "the deletion pass still ran despite the publish failure"
        );
        assert!(
            !handle
                .published_source_set()
                .sources
                .contains_key(&doomed_id),
            "the stale lane for the deleted branch is gone"
        );
    }

    #[test]
    fn reconcile_from_a_linked_worktree_excludes_the_main_worktree_branch() {
        // Finding D2 (L-G01): when the project is opened AT a linked worktree,
        // `repository.head()` is the WORKTREE's HEAD — the MAIN worktree's
        // checked-out branch is its own P0 lane and must be unioned from the common
        // dir, never republished as a P1 lane (data-model.md:1260-1263).
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let main = git2::Repository::open(root).expect("open main");
        let main_ref = main
            .head()
            .expect("head")
            .name()
            .expect("head name")
            .to_string();

        // Add a linked worktree on a NEW branch and open THAT as the project.
        let wt_parent = tempfile::tempdir().expect("wt tempdir");
        let wt_path = wt_parent.path().join("feature-wt");
        add_worktree_on_branch(&main, &wt_path, "feature-wt", "feature");
        let worktree_repo = git2::Repository::open(&wt_path).expect("open worktree");
        assert!(
            worktree_repo.is_worktree(),
            "the project is opened at a linked worktree"
        );

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-wt-open");
        let outcome = reconcile_local_ref_topology(
            &handle,
            &worktree_repo,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("reconcile from a linked worktree");

        let main_id = SourceId::new(format!(
            "symforge:git-ref:{}:{main_ref}",
            repository_id.as_str()
        ));
        let feature_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/feature",
            repository_id.as_str()
        ));
        let set = handle.published_source_set();
        assert!(
            !set.sources.contains_key(&main_id),
            "the MAIN worktree branch is never a P1 lane when reconciling from a linked worktree"
        );
        assert!(
            !outcome.published.contains(&main_id),
            "the main branch is not among the published lanes"
        );
        assert!(
            !set.sources.contains_key(&feature_id),
            "the linked worktree's own branch is also not a P1 lane"
        );
    }

    #[test]
    fn fail_closed_reconcile_purges_stale_ref_lanes() {
        // Finding D4: when topology becomes unprovable the pass fails closed BEFORE
        // the deletion pass, so a pre-existing P1 lane for a branch that may now be
        // checked out must be purged — else a P0 branch keeps being served as a P1
        // lane.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("bare", &head_commit, false)
            .expect("bare branch");

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-failclosed");
        let bare_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/bare",
            repository_id.as_str()
        ));

        // A clean first pass publishes the bare-branch P1 lane.
        reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("first reconcile");
        assert!(
            handle.published_source_set().sources.contains_key(&bare_id),
            "the bare lane is published on the clean pass"
        );

        // Add a linked worktree, then corrupt its HEAD so the topology is
        // unprovable (head_resolved == false).
        let wt_parent = tempfile::tempdir().expect("wt tempdir");
        let wt_path = wt_parent.path().join("checked");
        add_worktree_on_branch(&repository, &wt_path, "checked-wt", "checked");
        let admin_head = root
            .join(".git")
            .join("worktrees")
            .join("checked-wt")
            .join("HEAD");
        std::fs::write(&admin_head, b"\x00\x01 not a ref\n").expect("corrupt worktree HEAD");

        let result = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        );
        assert!(
            result.is_err(),
            "an unprovable topology fails the reconcile closed, got {result:?}"
        );
        assert!(
            !handle.published_source_set().sources.contains_key(&bare_id),
            "the pre-existing bare P1 lane is purged on the fail-closed abort"
        );
    }

    #[test]
    fn reconcile_does_not_publish_a_moved_worktree_branch() {
        // Finding D3 (reconcile level): a moved-but-not-pruned worktree is
        // classified checked-out from its admin HEAD, so its branch is excluded from
        // P1 — never published as a bare lane even though `validate()` fails.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");

        let wt_parent = tempfile::tempdir().expect("wt tempdir");
        let wt_path = wt_parent.path().join("moved-wt");
        add_worktree_on_branch(&repository, &wt_path, "moved-wt", "moved");
        std::fs::remove_dir_all(&wt_path).expect("remove worktree working dir");

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-moved");
        let outcome = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("reconcile with a moved worktree");

        let moved_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/moved",
            repository_id.as_str()
        ));
        assert!(
            !outcome.published.contains(&moved_id),
            "the moved worktree's branch is not published as a bare lane"
        );
        assert!(
            !handle
                .published_source_set()
                .sources
                .contains_key(&moved_id),
            "no P1 lane exists for the moved worktree's branch"
        );
        assert!(
            outcome
                .checked_out
                .iter()
                .any(|w| w.head_ref.as_deref() == Some("refs/heads/moved")),
            "the moved worktree is surfaced as checked-out via its admin HEAD"
        );
    }

    #[test]
    fn unchanged_branch_tip_skips_republish() {
        // Finding S1: republishing a bare branch whose tip has not moved is provably
        // redundant (build_ref_source_generation carries content_generation forward
        // on an identical tip). The second reconcile must skip the scout+ingest
        // entirely, leaving the lane's publication_generation untouched.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("bare", &head_commit, false)
            .expect("bare branch");

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-s1");
        let bare_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/bare",
            repository_id.as_str()
        ));

        let first = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("first reconcile");
        assert!(
            first.published.contains(&bare_id),
            "the first pass publishes the bare lane"
        );
        let gen_after_first = handle
            .published_source_set()
            .sources
            .get(&bare_id)
            .expect("lane present after first pass")
            .publication_generation;

        let second = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        )
        .expect("second reconcile");
        assert!(
            !second.published.contains(&bare_id),
            "an unchanged tip is NOT republished"
        );
        let gen_after_second = handle
            .published_source_set()
            .sources
            .get(&bare_id)
            .expect("lane still present after second pass")
            .publication_generation;
        assert_eq!(
            gen_after_first, gen_after_second,
            "publication_generation is untouched when the tip did not move"
        );
    }

    #[test]
    fn ref_tip_move_advances_lane_generations_without_touching_current() {
        // Finding D / L-R06: republishing a ref lane after its tip moves advances THAT
        // lane's publication_generation AND content_generation, while the P0 current
        // lane's generations stay unchanged (L-R12/L-R13). An identical-tip republish
        // advances only publication_generation, never content_generation.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn a() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));

        let current_before = handle.published_source_set().current_generation();
        let cur_pub = current_before.publication_generation;
        let cur_content = current_before.content_generation;

        let id = ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-gen"),
            &LocalRefScoutBudget::default(),
        )
        .expect("publish ref");
        let set1 = handle.published_source_set();
        let lane1 = set1.sources.get(&id).expect("ref lane");
        let pub1 = lane1.publication_generation;
        let content1 = lane1.content_generation;
        assert!(
            pub1 > 0,
            "the ref lane carries a meaningful publication generation, got {pub1}"
        );
        assert!(
            content1 > 0,
            "the ref lane carries a meaningful content generation, got {content1}"
        );

        // Move the tip: content changes -> both generations advance for this lane.
        commit_files(root, &[("src/b.rs", b"pub fn b() {}\n")], "second");
        ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-gen"),
            &LocalRefScoutBudget::default(),
        )
        .expect("republish moved ref");
        let set2 = handle.published_source_set();
        let lane2 = set2.sources.get(&id).expect("ref lane");
        assert!(
            lane2.publication_generation > pub1,
            "a tip move advances the lane publication generation"
        );
        assert!(
            lane2.content_generation > content1,
            "a tip move advances the lane content generation"
        );
        let pub2 = lane2.publication_generation;
        let content2 = lane2.content_generation;

        // Same-tip republish: publication advances, content does NOT.
        ingest_and_publish_local_ref(
            &handle,
            &repository,
            "HEAD",
            RepositoryId::new("repo-gen"),
            &LocalRefScoutBudget::default(),
        )
        .expect("republish same tip");
        let set3 = handle.published_source_set();
        let lane3 = set3.sources.get(&id).expect("ref lane");
        assert!(
            lane3.publication_generation > pub2,
            "an identical-tip republish still advances publication generation"
        );
        assert_eq!(
            lane3.content_generation, content2,
            "an identical-tip republish keeps content generation stable"
        );

        // The P0 current lane's generations are untouched by every P1 republish.
        let current_after = set3.current_generation();
        assert_eq!(
            current_after.publication_generation, cur_pub,
            "a P1 republish must not advance the current lane publication generation"
        );
        assert_eq!(
            current_after.content_generation, cur_content,
            "a P1 republish must not advance the current lane content generation"
        );
    }

    #[test]
    fn unresolved_worktree_head_fails_reconcile_closed_and_publishes_nothing() {
        // Finding E / L-G01: a worktree that validates but whose HEAD cannot be
        // resolved is `head_resolved = false`; reconcile then fails CLOSED rather than
        // risk publishing a checked-out branch as a P1 lane.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        let repository = git2::Repository::open(root).expect("open");

        // A bare branch that WOULD be published if the pass proceeded.
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("bare", &head_commit, false)
            .expect("bare branch");

        // A linked worktree on its own branch, then corrupt its HEAD so it still
        // validates but resolves to a nonexistent ref (unreadable HEAD).
        let wt_parent = tempfile::tempdir().expect("wt tempdir");
        let wt_path = wt_parent.path().join("wt");
        add_worktree_on_branch(&repository, &wt_path, "wt", "wt-branch");
        let admin_head = repository.path().join("worktrees").join("wt").join("HEAD");
        std::fs::write(&admin_head, b"ref: refs/heads/ghost-does-not-exist\n")
            .expect("corrupt worktree HEAD");

        // The classifier marks the worktree unresolved (fail closed), not detached.
        let checked_out = checked_out_worktrees(&repository).expect("classify");
        let wt = checked_out
            .iter()
            .find(|w| w.name == "wt")
            .expect("worktree present");
        assert!(
            !wt.head_resolved,
            "an unreadable worktree HEAD must be marked unresolved"
        );
        assert_eq!(wt.head_ref, None);

        // Reconcile fails closed: it returns an error and publishes no lane.
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-failclosed");
        let result = reconcile_local_ref_topology(
            &handle,
            &repository,
            repository_id.clone(),
            &LocalRefScoutBudget::default(),
        );
        assert!(
            result.is_err(),
            "an unresolved worktree HEAD must fail the reconcile closed, got {result:?}"
        );
        let bare_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/bare",
            repository_id.as_str()
        ));
        assert!(
            !handle.published_source_set().sources.contains_key(&bare_id),
            "no bare lane is published when the worktree topology is unprovable"
        );
    }

    #[test]
    fn empty_repo_unborn_head_reconciles_cleanly_without_failing_closed() {
        // Finding E / Cursor #2 (main-HEAD arm): an empty repository has an UNBORN
        // main HEAD (no commits, no local branches). The main-HEAD read must treat
        // that as fine — NOT fail closed — and publish nothing. (The main-HEAD
        // fail-CLOSED path — a branch HEAD whose name cannot be read — is symmetric
        // with the tested worktree case above.)
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root); // git init, no commit -> unborn HEAD
        let repository = git2::Repository::open(root).expect("open");
        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let before = handle.published_source_set().registry_generation;
        let outcome = reconcile_local_ref_topology(
            &handle,
            &repository,
            RepositoryId::new("repo-unborn"),
            &LocalRefScoutBudget::default(),
        )
        .expect("an unborn main HEAD must reconcile cleanly, not fail closed");
        assert!(
            outcome.published.is_empty(),
            "unborn repo publishes no lanes"
        );
        assert!(!outcome.skipped);
        assert_eq!(
            handle.published_source_set().registry_generation,
            before,
            "an unborn repo leaves the registry unchanged"
        );
    }

    #[test]
    fn concurrent_reconcile_is_single_flighted_and_skips() {
        // Finding F / S2a: single-flight is proven with REAL threads. While one
        // thread holds the reconcile guard, a contender running CONCURRENTLY on
        // another thread must SKIP (never block, never race the publish/deletion
        // steps); once the guard is free, the next pass runs and publishes. This is
        // deterministic — the contender is spawned while the guard is held — so
        // exactly one attempt runs and the other skips.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(root, &[("src/lib.rs", b"pub fn base() {}\n")], "initial");
        {
            let repository = git2::Repository::open(root).expect("open");
            let head_commit = repository
                .head()
                .expect("head")
                .peel_to_commit()
                .expect("peel");
            repository
                .branch("bare", &head_commit, false)
                .expect("bare branch");
        }

        let handle = SharedIndexHandle::new(LiveIndex::from_source_files(HashMap::new()));
        let repository_id = RepositoryId::new("repo-singleflight");
        let bare_id = SourceId::new(format!(
            "symforge:git-ref:{}:refs/heads/bare",
            repository_id.as_str()
        ));

        // A contender running on another thread WHILE this thread holds the guard
        // must skip — deterministic single-flight, no busy-wait race. Each thread
        // opens its own `Repository` (git2::Repository is not `Sync`).
        let contender = std::thread::scope(|scope| {
            let guard = handle
                .try_lock_ref_reconcile()
                .expect("acquire the reconcile guard");
            let outcome = scope
                .spawn(|| {
                    let repo = git2::Repository::open(root).expect("open in contender");
                    reconcile_local_ref_topology(
                        &handle,
                        &repo,
                        repository_id.clone(),
                        &LocalRefScoutBudget::default(),
                    )
                    .expect("the contender reconcile returns Ok(skipped)")
                })
                .join()
                .expect("join contender");
            drop(guard);
            outcome
        });
        assert!(
            contender.skipped,
            "a reconcile attempted concurrently while the guard is held must skip"
        );
        assert!(
            contender.published.is_empty() && contender.removed.is_empty(),
            "the skipped pass mutates nothing"
        );
        assert!(
            !handle.published_source_set().sources.contains_key(&bare_id),
            "the skipped pass published no lane"
        );

        // With the guard free, a pass on its own thread runs and publishes.
        let runner = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let repo = git2::Repository::open(root).expect("open in runner");
                    reconcile_local_ref_topology(
                        &handle,
                        &repo,
                        repository_id.clone(),
                        &LocalRefScoutBudget::default(),
                    )
                    .expect("the guard-free reconcile runs")
                })
                .join()
                .expect("join runner")
        });
        assert!(
            !runner.skipped,
            "the guard-free pass runs (skipped == false)"
        );
        assert!(
            runner.published.contains(&bare_id),
            "the runner publishes the bare lane"
        );
        assert!(
            handle.published_source_set().sources.contains_key(&bare_id),
            "the final lane set is correct"
        );
    }

    #[test]
    fn source_isolation_holds_through_scoped_query_composition() {
        // L-R08 (finding H): source isolation must hold through the actual search/
        // review COMPOSITION path, not just the built bundles. Searching one scope
        // never surfaces another lane's document.
        use crate::protocol::knowledge_review::review_scoped;
        use crate::protocol::knowledge_search::search_scoped;
        use crate::protocol::search_tools::{
            KnowledgeSourceScope, ReviewKnowledgeInput, ReviewKnowledgeMode, SearchKnowledgeInput,
        };

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        commit_files(
            root,
            &[(
                "docs/current-only.md",
                b"# Current Only\n\nThe quantum meridian anchors the current worktree lane.\n",
            )],
            "current",
        );
        let repository = git2::Repository::open(root).expect("open");
        commit_bare_branch_with_doc(
            &repository,
            "ref-only",
            "docs",
            "ref-only.md",
            b"# Ref Only\n\nThe helical cascade governs the divergent ref lane.\n",
        );

        let handle = LiveIndex::load(root).expect("load current lane");
        ingest_and_publish_local_ref(
            &handle,
            &repository,
            "refs/heads/ref-only",
            RepositoryId::new("repo-compose-iso"),
            &LocalRefScoutBudget::default(),
        )
        .expect("publish ref lane");
        let set = handle.published_source_set();

        let search = |scope, query: &str| {
            search_scoped(
                &set,
                &SearchKnowledgeInput {
                    query: query.to_string(),
                    path_prefix: None,
                    source_scope: Some(scope),
                    authority_scope: None,
                    project: None,
                    projects: None,
                    limit: None,
                    max_tokens: None,
                },
            )
        };

        // Each scope finds its OWN document...
        assert!(
            search(KnowledgeSourceScope::Current, "quantum meridian")
                .contains("docs/current-only.md"),
            "current scope must find its own document"
        );
        assert!(
            search(KnowledgeSourceScope::LocalRefs, "helical cascade").contains("docs/ref-only.md"),
            "local_refs scope must find its own document"
        );
        // ...and NEVER the other lane's, even when querying the other lane's term.
        let current_for_ref_term = search(KnowledgeSourceScope::Current, "helical cascade");
        assert!(
            !current_for_ref_term.contains("docs/ref-only.md"),
            "current scope leaked a ref-lane document: {current_for_ref_term}"
        );
        let refs_for_current_term = search(KnowledgeSourceScope::LocalRefs, "quantum meridian");
        assert!(
            !refs_for_current_term.contains("docs/current-only.md"),
            "local_refs scope leaked the current document: {refs_for_current_term}"
        );

        // The review composition path is likewise source-isolated.
        let review = |scope| {
            review_scoped(
                &set,
                &ReviewKnowledgeInput {
                    mode: ReviewKnowledgeMode::Summary,
                    path: None,
                    path_prefix: None,
                    source_scope: Some(scope),
                    project: None,
                    projects: None,
                    limit: None,
                    max_tokens: None,
                },
            )
        };
        let refs_review = review(KnowledgeSourceScope::LocalRefs).expect("local_refs review");
        assert!(
            !refs_review.rendered.contains("docs/current-only.md"),
            "local_refs review leaked the current document: {}",
            refs_review.rendered
        );
        let current_review = review(KnowledgeSourceScope::Current).expect("current review");
        assert!(
            !current_review.rendered.contains("docs/ref-only.md"),
            "current review leaked a ref-lane document: {}",
            current_review.rendered
        );
    }
}
